use std::{io::Write, path::PathBuf};

// Available before Tauri/WebView2 initialization, including failures in setup.
pub fn begin() -> PathBuf {
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("dev.linguist.cs2");
    let _ = std::fs::create_dir_all(&directory);
    let path = directory.join("startup.log");
    let header = format!(
        "Linguist {} · {} · process {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        std::process::id()
    );
    let _ = std::fs::write(&path, header);
    let error_path = path.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(&error_path, &format!("Fatal error: {info}"));
        #[cfg(windows)]
        unsafe {
            use windows::{
                core::{w, PCWSTR},
                Win32::UI::WindowsAndMessaging::{
                    MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND,
                },
            };
            let message: Vec<u16> = format!(
                "Linguist encountered an error:\n\n{info}\n\nDiagnostic log: {}",
                error_path.display()
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            // Does not depend on Tauri or a working WebView2 runtime.
            MessageBoxW(
                None,
                PCWSTR(message.as_ptr()),
                w!("Linguist — application error"),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
            );
        }
        previous(info);
    }));
    path
}

pub fn record(path: &std::path::Path, message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}
