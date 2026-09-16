use crate::AppState;
use linguist_core::models::{chat_model, files, model, verify_file, Model, CHAT_MODELS, MODELS};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio::io::AsyncWriteExt;

#[derive(Clone, Serialize)]
pub struct InstalledModel {
    pub id: String,
    pub bytes: u64,
    pub installed: bool,
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub id: String,
    pub received: u64,
    pub total: u64,
    pub phase: String,
    pub message: String,
}

pub fn installed(data: &Path) -> Vec<InstalledModel> {
    MODELS
        .iter()
        .map(|m| (m.id, vec![m]))
        .chain(CHAT_MODELS.iter().map(|m| (m.id, m.files.iter().collect())))
        .map(|(id, files)| InstalledModel {
            id: id.into(),
            bytes: files.iter().map(|m| m.bytes).sum(),
            installed: files.iter().all(|m| {
                std::fs::metadata(data.join("models").join(m.filename))
                    .map(|s| s.len() == m.bytes)
                    .unwrap_or(false)
            }),
        })
        .collect()
}

fn progress(app: &tauri::AppHandle, value: DownloadProgress) {
    *app.state::<AppState>().download.lock().unwrap() = Some(value.clone());
    let _ = app.emit("model-progress", value);
}

fn begin(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    files(id)?;
    let state = app.state::<AppState>();
    let mut current = state.download.lock().unwrap();
    if current
        .as_ref()
        .is_some_and(|p| matches!(p.phase.as_str(), "downloading" | "verifying" | "importing"))
    {
        return Err("A model operation is already running.".into());
    }
    state.cancel_download.store(false, Ordering::SeqCst);
    *current = Some(DownloadProgress {
        id: id.into(),
        received: 0,
        total: files(id)?.iter().map(|m| m.bytes).sum(),
        phase: "downloading".into(),
        message: "Preparing model…".into(),
    });
    let initial = current.clone();
    drop(current);
    if let Some(initial) = initial {
        let _ = app.emit("model-progress", initial);
    }
    Ok(())
}

fn cancelled(app: &tauri::AppHandle) -> Result<(), String> {
    if app
        .state::<AppState>()
        .cancel_download
        .load(Ordering::SeqCst)
    {
        Err("Cancelled".into())
    } else {
        Ok(())
    }
}

fn finish(app: &tauri::AppHandle, id: &str, result: &Result<(), String>) {
    let error = result.as_ref().err();
    progress(
        app,
        DownloadProgress {
            id: id.into(),
            received: 0,
            total: 0,
            phase: if error.is_none() {
                "complete"
            } else if error.is_some_and(|s| s == "Cancelled") {
                "cancelled"
            } else {
                "error"
            }
            .into(),
            message: error
                .cloned()
                .unwrap_or_else(|| "Model verified and ready".into()),
        },
    );
    let _ = app.emit("models-changed", installed(&app.state::<AppState>().data));
}

#[tauri::command]
pub fn cancel_download(app: tauri::AppHandle) {
    app.state::<AppState>()
        .cancel_download
        .store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn download_model(app: tauri::AppHandle, id: String) -> Result<(), String> {
    begin(&app, &id)?;
    let result = async {
        for file in files(&id)? {
            download_one(&app, file).await?;
        }
        if id != "vad" && chat_model(&id).is_err() {
            download_one(&app, model("vad")?).await?;
        }
        Ok(())
    }
    .await;
    finish(&app, &id, &result);
    result
}

async fn download_one(app: &tauri::AppHandle, model: &Model) -> Result<(), String> {
    let path = app
        .state::<AppState>()
        .data
        .join("models")
        .join(model.filename);
    let existing = path.clone();
    let spec = *model;
    if tokio::task::spawn_blocking(move || verify_file(&existing, &spec).is_ok())
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .map_err(|e| e.to_string())?;
    let partial = path.with_extension("partial");
    let result = async {
        cancelled(app)?;
        let client = reqwest::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        let mut response = client
            .get(model.url)
            .send()
            .await
            .map_err(|e| format!("Download failed: {e}"))?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if response
            .content_length()
            .is_some_and(|len| len != model.bytes)
        {
            return Err("Unexpected model download size.".into());
        }
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut received = 0;
        let mut last_progress = Instant::now() - Duration::from_secs(1);
        loop {
            cancelled(app)?;
            // Poll cancellation even while the server has stalled.
            let next = response.chunk();
            tokio::pin!(next);
            let chunk = loop {
                tokio::select! {
                    result = &mut next => break result.map_err(|e| e.to_string())?,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => cancelled(app)?,
                }
            };
            let Some(chunk) = chunk else {
                break;
            };
            received += chunk.len() as u64;
            if received > model.bytes {
                return Err("Model download exceeded its expected size.".into());
            }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            hash.update(&chunk);
            if last_progress.elapsed() >= Duration::from_millis(100) {
                progress(
                    app,
                    DownloadProgress {
                        id: model.id.into(),
                        received,
                        total: model.bytes,
                        phase: "downloading".into(),
                        message: format!("Downloading {}", model.filename),
                    },
                );
                last_progress = Instant::now();
            }
        }
        cancelled(app)?;
        if received != model.bytes || format!("{:x}", hash.finalize()) != model.sha256 {
            return Err("Model checksum mismatch. Please retry the download.".into());
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        tokio::fs::rename(&partial, &path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result
}

#[tauri::command]
pub async fn import_model(app: tauri::AppHandle, id: String) -> Result<(), String> {
    begin(&app, &id)?;
    let task_app = app.clone();
    let task_id = id.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let app = task_app;
        let id = task_id;
        let group = chat_model(&id).is_ok();
        progress(
            &app,
            DownloadProgress {
                id: id.clone(),
                received: 0,
                total: 0,
                phase: "importing".into(),
                message: if group {
                    "Select the extracted text model folder…"
                } else {
                    "Select the original GGML model file…"
                }
                .into(),
            },
        );
        let source = if group {
            app.dialog().file().blocking_pick_folder()
        } else {
            app.dialog()
                .file()
                .add_filter("Whisper GGML model", &["bin"])
                .blocking_pick_file()
        }
        .ok_or("Cancelled")?
        .into_path()
        .map_err(|e| e.to_string())?;
        for file in files(&id)? {
            let source = if group {
                source.join(Path::new(file.filename).file_name().unwrap())
            } else {
                source.clone()
            };
            import_one(&app, file, &source)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    finish(&app, &id, &result);
    result
}

fn import_one(app: &tauri::AppHandle, model: &Model, source: &Path) -> Result<(), String> {
    use std::io::{Read, Write};
    cancelled(app)?;
    let target = app
        .state::<AppState>()
        .data
        .join("models")
        .join(model.filename);
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    let partial = target.with_extension("partial");
    let result = (|| {
        let mut source = std::fs::File::open(source).map_err(|e| e.to_string())?;
        if source.metadata().map_err(|e| e.to_string())?.len() != model.bytes {
            return Err(format!(
                "Select the supported {} file ({} bytes).",
                model.filename, model.bytes
            ));
        }
        let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 65536];
        let mut received = 0;
        let mut hash = Sha256::new();
        let mut last_progress = Instant::now();
        loop {
            cancelled(app)?;
            let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > model.bytes {
                return Err("Model is larger than expected.".into());
            }
            hash.update(&buffer[..n]);
            file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            if last_progress.elapsed() > Duration::from_millis(100) {
                progress(
                    app,
                    DownloadProgress {
                        id: model.id.into(),
                        received,
                        total: model.bytes,
                        phase: "importing".into(),
                        message: format!("Importing and verifying {}", model.filename),
                    },
                );
                last_progress = Instant::now();
            }
        }
        if received != model.bytes || format!("{:x}", hash.finalize()) != model.sha256 {
            return Err("Checksum mismatch. Choose the supported unmodified model files.".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        cancelled(app)?;
        std::fs::rename(&partial, &target).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

#[tauri::command]
pub async fn pick_chat_log(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Select CS2 console.log (launch CS2 with -condebug first)")
            .add_filter("CS2 console log", &["log"])
            .blocking_pick_file()
            .map(|file| {
                file.into_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
