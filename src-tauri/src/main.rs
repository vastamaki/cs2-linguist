#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod engine;
mod models;
mod overlay;
mod startup;

use engine::Engine;
use linguist_core::{history::History, Backend, EngineStatus, Settings};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex},
};
use tauri::{Emitter, Manager};

struct AppState {
    settings: Mutex<Settings>,
    history: Mutex<History>,
    runtime: Mutex<engine::Runtime>,
    chat_runtime: Mutex<engine::Runtime>,
    download: Mutex<Option<models::DownloadProgress>>,
    cancel_download: AtomicBool,
    data: PathBuf,
}

#[derive(Serialize)]
struct Snapshot {
    settings: Settings,
    history: History,
    status: EngineStatus,
    chat_status: EngineStatus,
    chat_overlay_locked: bool,
    chat_languages: Vec<&'static str>,
    models: Vec<models::InstalledModel>,
    download: Option<models::DownloadProgress>,
    overlay_locked: bool,
    supported: bool,
    languages: Vec<&'static str>,
}

#[tauri::command]
fn snapshot(app: tauri::AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    // Read independently; worker publication holds its runtime lock before history.
    let settings = state.settings.lock().unwrap().clone();
    let status = state.runtime.lock().unwrap().status.clone();
    let chat_status = state.chat_runtime.lock().unwrap().status.clone();
    let history = state.history.lock().unwrap().clone();
    let snapshot = Snapshot {
        settings,
        history,
        status,
        chat_status,
        chat_overlay_locked: !app
            .get_webview_window("chat-overlay")
            .map(|w| w.is_resizable().unwrap_or(false))
            .unwrap_or(false),
        chat_languages: linguist_core::chat::LANGUAGES.split_whitespace().collect(),
        models: models::installed(&state.data),
        download: state.download.lock().unwrap().clone(),
        overlay_locked: !app
            .get_webview_window("overlay")
            .map(|w| w.is_resizable().unwrap_or(false))
            .unwrap_or(false),
        supported: cfg!(windows),
        languages: linguist_core::LANGUAGES.split_whitespace().collect(),
    };
    snapshot
}

fn persist(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    let temp = state.data.join("settings.json.partial");
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, state.data.join("settings.json")).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, mut settings: Settings) -> Result<(), String> {
    settings.validate()?;
    let state = app.state::<AppState>();
    let changed = {
        let mut current = state.settings.lock().unwrap();
        // Geometry is owned by the overlay; a settings form may be stale.
        settings.position = current.position.clone();
        settings.chat.position = current.chat.position.clone();
        let changed = (
            current.engine_changed(&settings),
            current.chat.engine_changed(&settings.chat),
            current.overlays_enabled != settings.overlays_enabled,
        );
        persist(&app, &settings)?;
        *current = settings.clone();
        changed
    };
    let _ = app.emit("settings", &settings);
    if changed.2 {
        overlay::set_overlay_locked(app.clone(), true, None)?;
        overlay::set_overlay_locked(app.clone(), true, Some("chat-overlay".into()))?;
    }
    let running = state.runtime.lock().unwrap().status.running
        || state.chat_runtime.lock().unwrap().status.running;
    if running {
        let mut errors = Vec::new();
        for (kind, changed, enabled, label) in [
            (Engine::Voice, changed.0, settings.voice_enabled, "overlay"),
            (
                Engine::Chat,
                changed.1,
                settings.chat.enabled,
                "chat-overlay",
            ),
        ] {
            if !changed {
                continue;
            }
            engine::stop(&app, kind);
            if enabled {
                if let Err(e) = engine::start(&app, kind) {
                    errors.push(e);
                }
            }
            if let Err(e) = overlay::set_overlay_locked(app.clone(), true, Some(label.into())) {
                errors.push(e);
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
    }
    Ok(())
}

#[tauri::command]
fn start(app: tauri::AppHandle) -> Result<(), String> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if !settings.voice_enabled && !settings.chat.enabled {
        return Err("Enable voice or chat translation first.".into());
    }
    let result = (|| {
        for (kind, enabled, label) in [
            (Engine::Voice, settings.voice_enabled, "overlay"),
            (Engine::Chat, settings.chat.enabled, "chat-overlay"),
        ] {
            if enabled {
                engine::start(&app, kind)?;
            }
            overlay::set_overlay_locked(app.clone(), true, Some(label.into()))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = stop(app);
    } else if !settings.overlays_enabled {
        show_settings(&app);
        let _ = app.emit("show-translations", ());
    }
    result
}
#[tauri::command]
fn stop(app: tauri::AppHandle) -> Result<(), String> {
    engine::stop(&app, Engine::Voice);
    engine::stop(&app, Engine::Chat);
    let voice = overlay::set_overlay_locked(app.clone(), true, None);
    let chat = overlay::set_overlay_locked(app, true, Some("chat-overlay".into()));
    voice.and(chat)
}

fn show_settings(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem, Submenu},
        tray::TrayIconBuilder,
    };
    let item = |id, label| MenuItem::with_id(app, id, label, true, None::<&str>);
    let start = item("start", "Start / Pause")?;
    let cpu = item("cpu", "CPU")?;
    let gpu = item("gpu", "GPU (Vulkan)")?;
    let backend = Submenu::with_items(app, "Processing mode", true, &[&cpu, &gpu])?;
    let position = item("position", "Move / Lock overlay")?;
    let reset = item("reset", "Reset overlay position")?;
    let chat_position = item("chat-position", "Move / Lock chat overlay")?;
    let settings = item("settings", "Settings…")?;
    let quit = item("quit", "Quit Linguist")?;
    let menu = Menu::with_items(
        app,
        &[
            &start,
            &backend,
            &position,
            &chat_position,
            &reset,
            &settings,
            &quit,
        ],
    )?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Linguist — local CS2 captions")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "start" => {
                    let state = app.state::<AppState>();
                    let running = state.runtime.lock().unwrap().status.running
                        || state.chat_runtime.lock().unwrap().status.running;
                    if running {
                        crate::stop(app.clone())
                    } else {
                        crate::start(app.clone())
                    }
                }
                "cpu" | "gpu" => {
                    let mut settings = app.state::<AppState>().settings.lock().unwrap().clone();
                    settings.backend = if event.id.as_ref() == "cpu" {
                        Backend::Cpu
                    } else {
                        Backend::Gpu
                    };
                    save_settings(app.clone(), settings)
                }
                "position" | "chat-position" => {
                    let label = if event.id.as_ref() == "chat-position" {
                        "chat-overlay"
                    } else {
                        "overlay"
                    };
                    let locked = app
                        .get_webview_window(label)
                        .map(|w| !w.is_resizable().unwrap_or(false))
                        .unwrap_or(true);
                    overlay::set_overlay_locked(app.clone(), !locked, Some(label.into()))
                }
                "reset" => overlay::reset_overlay(app.clone(), None),
                "settings" => {
                    show_settings(app);
                    Ok(())
                }
                "quit" => {
                    engine::stop(app, Engine::Voice);
                    engine::stop(app, Engine::Chat);
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(message) = result {
                let _ = app.emit("app-error", message);
                show_settings(app);
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    let startup_log = startup::begin();
    let setup_log = startup_log.clone();
    // Children inherit this mode: a missing Vulkan DLL must fail back to CPU,
    // not block startup behind a Windows loader/crash dialog.
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Diagnostics::Debug::{
            SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX,
        };
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_settings(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            startup::record(&setup_log, "Loading settings");
            let data = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(data.join("models"))?;
            let settings_path = data.join("settings.json");
            let settings = std::fs::read(&settings_path)
                .ok()
                .and_then(|s| serde_json::from_slice::<Settings>(&s).ok())
                .filter(|s| s.validate().is_ok())
                .unwrap_or_default();
            app.manage(AppState {
                settings: Mutex::new(settings),
                history: Mutex::new(History::default()),
                runtime: Mutex::new(engine::Runtime::default()),
                chat_runtime: Mutex::new(engine::Runtime::default()),
                download: Mutex::new(None),
                cancel_download: AtomicBool::new(false),
                data,
            });
            // Pages immediately invoke snapshot. Register state before creating
            // any WebViews; Windows can dispatch IPC while creating the next one.
            for config in &app.config().app.windows {
                startup::record(&setup_log, &format!("Creating window: {}", config.label));
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;
            }
            startup::record(&setup_log, "Creating tray");
            tray(app.handle())?;
            startup::record(&setup_log, "Initializing voice overlay");
            overlay::initialize(app.handle(), "overlay")?;
            startup::record(&setup_log, "Initializing chat overlay");
            overlay::initialize(app.handle(), "chat-overlay")?;
            let monitor_app = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                let handle = monitor_app.clone();
                if monitor_app
                    .run_on_main_thread(move || {
                        let _ = overlay::ensure_visible(&handle, "overlay");
                        let _ = overlay::ensure_visible(&handle, "chat-overlay");
                    })
                    .is_err()
                {
                    break;
                }
            });
            show_settings(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            if matches!(window.label(), "overlay" | "chat-overlay") {
                match event {
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                        overlay::save_geometry(window.app_handle(), window.label())
                    }
                    tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        let _ = overlay::ensure_visible(window.app_handle(), window.label());
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            save_settings,
            start,
            stop,
            models::download_model,
            models::cancel_download,
            models::import_model,
            models::pick_chat_log,
            overlay::set_overlay_locked,
            overlay::reset_overlay
        ])
        .build(tauri::generate_context!())
        .expect("Could not initialize Linguist")
        .run(move |app, event| {
            if matches!(event, tauri::RunEvent::Ready) {
                startup::record(&startup_log, "Startup complete");
            }
            if matches!(event, tauri::RunEvent::Exit) {
                engine::stop(app, Engine::Voice);
                engine::stop(app, Engine::Chat);
            }
        });
}
