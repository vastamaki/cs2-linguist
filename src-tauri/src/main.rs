#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod engine;
mod models;
mod overlay;

use linguist_core::{Backend, EngineStatus, Settings};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex},
};
use tauri::{Emitter, Manager};

struct AppState {
    settings: Mutex<Settings>,
    runtime: Mutex<engine::Runtime>,
    download: Mutex<Option<models::DownloadProgress>>,
    cancel_download: AtomicBool,
    data: PathBuf,
}

#[derive(Serialize)]
struct Snapshot {
    settings: Settings,
    status: EngineStatus,
    models: Vec<models::InstalledModel>,
    download: Option<models::DownloadProgress>,
    overlay_locked: bool,
    supported: bool,
    languages: Vec<&'static str>,
}

#[tauri::command]
fn snapshot(app: tauri::AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let snapshot = Snapshot {
        settings: state.settings.lock().unwrap().clone(),
        status: state.runtime.lock().unwrap().status.clone(),
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
        // Geometry and visibility are owned by the overlay; a settings form may be stale.
        settings.position = current.position.clone();
        settings.overlay_visible = current.overlay_visible;
        let changed = current.engine_changed(&settings);
        persist(&app, &settings)?;
        *current = settings.clone();
        changed
    };
    let _ = app.emit("settings", &settings);
    let running = state.runtime.lock().unwrap().status.running;
    if changed && running {
        engine::start(&app)?;
    }
    Ok(())
}

#[tauri::command]
fn start(app: tauri::AppHandle) -> Result<(), String> {
    engine::start(&app)
}
#[tauri::command]
fn stop(app: tauri::AppHandle) {
    engine::stop(&app);
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
    let visibility = item("visibility", "Show / Hide overlay")?;
    let position = item("position", "Move / Lock overlay")?;
    let reset = item("reset", "Reset overlay position")?;
    let settings = item("settings", "Settings…")?;
    let quit = item("quit", "Quit Linguist")?;
    let menu = Menu::with_items(
        app,
        &[
            &start,
            &backend,
            &visibility,
            &position,
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
                    let running = app
                        .state::<AppState>()
                        .runtime
                        .lock()
                        .unwrap()
                        .status
                        .running;
                    if running {
                        engine::stop(app);
                        Ok(())
                    } else {
                        engine::start(app)
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
                "visibility" => {
                    let visible = app
                        .state::<AppState>()
                        .settings
                        .lock()
                        .unwrap()
                        .overlay_visible;
                    overlay::set_overlay_visible(app.clone(), !visible)
                }
                "position" => {
                    let locked = app
                        .get_webview_window("overlay")
                        .map(|w| !w.is_resizable().unwrap_or(false))
                        .unwrap_or(true);
                    overlay::set_overlay_locked(app.clone(), !locked)
                }
                "reset" => overlay::reset_overlay(app.clone()),
                "settings" => {
                    show_settings(app);
                    Ok(())
                }
                "quit" => {
                    engine::stop(app);
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
        .setup(|app| {
            let data = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(data.join("models"))?;
            let settings_path = data.join("settings.json");
            let first_run = !settings_path.exists();
            let settings = std::fs::read(&settings_path)
                .ok()
                .and_then(|s| serde_json::from_slice::<Settings>(&s).ok())
                .filter(|s| s.validate().is_ok())
                .unwrap_or_default();
            app.manage(AppState {
                settings: Mutex::new(settings),
                runtime: Mutex::new(engine::Runtime::default()),
                download: Mutex::new(None),
                cancel_download: AtomicBool::new(false),
                data,
            });
            tray(app.handle())?;
            overlay::initialize(app.handle())?;
            let monitor_app = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                let handle = monitor_app.clone();
                if monitor_app
                    .run_on_main_thread(move || {
                        let _ = overlay::ensure_visible(&handle);
                    })
                    .is_err()
                {
                    break;
                }
            });
            if first_run || cfg!(debug_assertions) {
                show_settings(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            if window.label() == "overlay" {
                match event {
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                        overlay::save_geometry(window.app_handle())
                    }
                    tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        let _ = overlay::ensure_visible(window.app_handle());
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
            overlay::set_overlay_locked,
            overlay::set_overlay_visible,
            overlay::reset_overlay
        ])
        .build(tauri::generate_context!())
        .expect("Could not initialize Linguist")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                engine::stop(app);
            }
        });
}
