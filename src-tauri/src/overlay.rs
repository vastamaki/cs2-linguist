use crate::{persist, AppState};
use linguist_core::OverlayPosition;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

pub fn initialize(app: &tauri::AppHandle, label: &str) -> tauri::Result<()> {
    let window = app.get_webview_window(label).unwrap();
    window.set_ignore_cursor_events(true)?;
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let position = if label == "chat-overlay" {
        settings.chat.position
    } else {
        settings.position
    };
    if let Some(position) = position {
        window.set_size(PhysicalSize::new(
            position.width.clamp(300, 3000),
            position.height.clamp(140, 1400),
        ))?;
        window.set_position(PhysicalPosition::new(position.x, position.y))?;
    } else {
        let _ = reset_overlay(app.clone(), Some(label.into()));
    }
    let _ = ensure_visible(app, label);
    Ok(())
}

pub fn ensure_visible(app: &tauri::AppHandle, label: &str) -> Result<(), String> {
    let window = app.get_webview_window(label).ok_or("Overlay unavailable")?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let visible = monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        OverlayPosition {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
        .visible_on(&OverlayPosition {
            x: p.x,
            y: p.y,
            width: s.width,
            height: s.height,
        })
    });
    if !visible {
        reset_overlay(app.clone(), Some(label.into()))?;
    }
    Ok(())
}

#[tauri::command]
pub fn reset_overlay(app: tauri::AppHandle, target: Option<String>) -> Result<(), String> {
    let label = label(target.as_deref())?;
    let window = app.get_webview_window(label).ok_or("Overlay unavailable")?;
    if let Some(monitor) = window.primary_monitor().map_err(|e| e.to_string())? {
        let scale = monitor.scale_factor();
        let width = ((if label == "chat-overlay" {
            480.0
        } else {
            720.0
        }) * scale)
            .min(monitor.size().width as f64 * 0.85) as u32;
        let height = ((if label == "chat-overlay" {
            320.0
        } else {
            230.0
        }) * scale)
            .min(monitor.size().height as f64 * 0.5) as u32;
        window
            .set_size(PhysicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        let x = monitor.position().x
            + if label == "chat-overlay" {
                (24.0 * scale) as i32
            } else {
                (monitor.size().width as i32 - width as i32) / 2
            };
        let y = monitor.position().y + monitor.size().height as i32
            - height as i32
            - (80.0 * scale) as i32;
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        save_geometry(&app, label);
    }
    Ok(())
}

pub fn save_geometry(app: &tauri::AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let (Ok(p), Ok(s)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut settings = state.settings.lock().unwrap();
    let position = Some(OverlayPosition {
        x: p.x,
        y: p.y,
        width: s.width,
        height: s.height,
    });
    if label == "chat-overlay" {
        settings.chat.position = position;
    } else {
        settings.position = position;
    }
    let _ = persist(app, &settings);
}

#[tauri::command]
pub fn set_overlay_locked(
    app: tauri::AppHandle,
    locked: bool,
    target: Option<String>,
) -> Result<(), String> {
    let label = label(target.as_deref())?;
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let locked = locked || !settings.overlays_enabled;
    let window = app.get_webview_window(label).ok_or("Overlay unavailable")?;
    window
        .set_ignore_cursor_events(locked)
        .map_err(|e| e.to_string())?;
    window.set_focusable(!locked).map_err(|e| e.to_string())?;
    window.set_resizable(!locked).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let kind = if label == "chat-overlay" {
        crate::engine::Engine::Chat
    } else {
        crate::engine::Engine::Voice
    };
    let running = kind.runtime(&state).lock().unwrap().status.running;
    // While paused, show only the positioning preview; locking closes it again.
    if settings.show_overlay(running, locked) {
        ensure_visible(&app, label)?;
        window.show()
    } else {
        window.hide()
    }
    .map_err(|e| e.to_string())?;
    let _ = app.emit(
        if label == "chat-overlay" {
            "chat-overlay-locked"
        } else {
            "overlay-locked"
        },
        locked,
    );
    Ok(())
}

fn label(target: Option<&str>) -> Result<&str, String> {
    match target.unwrap_or("overlay") {
        "overlay" => Ok("overlay"),
        "chat-overlay" => Ok("chat-overlay"),
        _ => Err("Unknown overlay.".into()),
    }
}
