use crate::{persist, AppState};
use linguist_core::OverlayPosition;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

pub fn initialize(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = app.get_webview_window("overlay").unwrap();
    window.set_ignore_cursor_events(true)?;
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if let Some(position) = settings.position {
        window.set_size(PhysicalSize::new(
            position.width.clamp(300, 3000),
            position.height.clamp(140, 1400),
        ))?;
        window.set_position(PhysicalPosition::new(position.x, position.y))?;
    } else {
        let _ = reset_overlay(app.clone());
    }
    let _ = ensure_visible(app);
    Ok(())
}

pub fn ensure_visible(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay unavailable")?;
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
        reset_overlay(app.clone())?;
    }
    Ok(())
}

#[tauri::command]
pub fn reset_overlay(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay unavailable")?;
    if let Some(monitor) = window.primary_monitor().map_err(|e| e.to_string())? {
        let scale = monitor.scale_factor();
        let width = (720.0 * scale).min(monitor.size().width as f64 * 0.85) as u32;
        let height = (230.0 * scale).min(monitor.size().height as f64 * 0.5) as u32;
        window
            .set_size(PhysicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        let x = monitor.position().x + (monitor.size().width as i32 - width as i32) / 2;
        let y = monitor.position().y + monitor.size().height as i32
            - height as i32
            - (80.0 * scale) as i32;
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        save_geometry(&app);
    }
    Ok(())
}

pub fn save_geometry(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    let (Ok(p), Ok(s)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut settings = state.settings.lock().unwrap();
    settings.position = Some(OverlayPosition {
        x: p.x,
        y: p.y,
        width: s.width,
        height: s.height,
    });
    let _ = persist(app, &settings);
}

#[tauri::command]
pub fn set_overlay_locked(app: tauri::AppHandle, locked: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay unavailable")?;
    window
        .set_ignore_cursor_events(locked)
        .map_err(|e| e.to_string())?;
    window.set_focusable(!locked).map_err(|e| e.to_string())?;
    window.set_resizable(!locked).map_err(|e| e.to_string())?;
    let running = app
        .state::<AppState>()
        .runtime
        .lock()
        .unwrap()
        .status
        .running;
    // While paused, show only the positioning preview; locking closes it again.
    if running || !locked {
        ensure_visible(&app)?;
        window.show()
    } else {
        window.hide()
    }
    .map_err(|e| e.to_string())?;
    let _ = app.emit("overlay-locked", locked);
    Ok(())
}
