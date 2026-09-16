use crate::AppState;
use linguist_core::chat::ChatCommand;
use linguist_core::{
    accepts_result, models, Backend, Caption, EngineStatus, Settings, WorkerCommand, WorkerEvent,
};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Voice,
    Chat,
}
impl Engine {
    pub fn runtime(self, state: &AppState) -> &std::sync::Mutex<Runtime> {
        match self {
            Self::Voice => &state.runtime,
            Self::Chat => &state.chat_runtime,
        }
    }
    fn status_event(self) -> &'static str {
        if self == Self::Voice {
            "engine-status"
        } else {
            "chat-status"
        }
    }
}

pub struct Runtime {
    pub status: EngineStatus,
    child: Option<Child>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            child: None,
            status: EngineStatus {
                phase: "paused".into(),
                message: "Ready when you are".into(),
                backend: Backend::Cpu,
                running: false,
                generation: 0,
                warning: None,
            },
        }
    }
}

fn announce(app: &tauri::AppHandle, runtime: &Runtime, kind: Engine) {
    let _ = app.emit(kind.status_event(), &runtime.status);
}

fn terminate(runtime: &mut Runtime) {
    runtime.status.generation += 1;
    if let Some(mut child) = runtime.child.take() {
        if let Some(input) = &mut child.stdin {
            let _ = writeln!(input, "{{\"type\":\"stop\"}}");
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn stop(app: &tauri::AppHandle, kind: Engine) {
    let state = app.state::<AppState>();
    let mut runtime = kind.runtime(&state).lock().unwrap();
    terminate(&mut runtime);
    runtime.status.running = false;
    runtime.status.phase = "paused".into();
    runtime.status.message = "Paused".into();
    announce(app, &runtime, kind);
}

pub fn start(app: &tauri::AppHandle, kind: Engine) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Live game capture is available on Windows 11. This platform supports UI preview and WAV worker benchmarks.".into());
    }
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    settings.validate()?;
    let ids = if kind == Engine::Chat {
        vec![settings.chat.model.as_str()]
    } else {
        vec![settings.model.as_str(), "vad"]
    };
    for id in ids {
        for model in models::files(id)? {
            if !state.data.join("models").join(model.filename).exists() {
                return Err(format!("Download or import the selected {id} model first."));
            }
        }
    }
    if kind == Engine::Chat && settings.chat.log_path.trim().is_empty() {
        return Err(
            "Select CS2's console.log in Chat settings first (launch with -condebug).".into(),
        );
    }
    let mut runtime = kind.runtime(&state).lock().unwrap();
    terminate(&mut runtime);
    runtime.status.warning = None;
    runtime.status.running = true;
    launch_or_fallback(app, &mut runtime, settings, kind)
}

fn launch_or_fallback(
    app: &tauri::AppHandle,
    runtime: &mut Runtime,
    mut settings: Settings,
    kind: Engine,
) -> Result<(), String> {
    let result = launch(app, runtime, &settings, kind);
    if let Err(error) = result {
        if kind == Engine::Voice && settings.backend == Backend::Gpu {
            runtime.status.warning = Some(format!("GPU unavailable: {error}. Using CPU."));
            settings.backend = Backend::Cpu;
            if let Err(error) = launch(app, runtime, &settings, kind) {
                return failed(app, runtime, error, kind);
            }
        } else {
            return failed(app, runtime, error, kind);
        }
    }
    Ok(())
}

fn failed(
    app: &tauri::AppHandle,
    runtime: &mut Runtime,
    error: String,
    kind: Engine,
) -> Result<(), String> {
    runtime.status.running = false;
    runtime.status.phase = "error".into();
    runtime.status.message = error.clone();
    announce(app, runtime, kind);
    Err(error)
}

fn launch(
    app: &tauri::AppHandle,
    runtime: &mut Runtime,
    settings: &Settings,
    kind: Engine,
) -> Result<(), String> {
    let mode = if kind == Engine::Chat {
        "chat"
    } else if settings.backend == Backend::Cpu {
        "cpu"
    } else {
        "gpu"
    };
    let stem = if kind == Engine::Chat {
        "linguist-chat-worker".into()
    } else {
        format!("linguist-worker-{mode}")
    };
    let filename = format!("{stem}{}", std::env::consts::EXE_SUFFIX);
    let installed = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name(&filename);
    let worker = if cfg!(debug_assertions) {
        let triple = if cfg!(windows) {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("{stem}-{triple}{}", std::env::consts::EXE_SUFFIX))
    } else {
        installed
    };
    let mut command = Command::new(worker);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start {mode} worker: {e}"))?;
    let data = &app.state::<AppState>().data;
    let message = if kind == Engine::Chat {
        serde_json::to_value(ChatCommand::Start {
            settings: settings.chat.clone(),
            model_path: data
                .join("models")
                .join(&settings.chat.model)
                .to_string_lossy()
                .into_owned(),
        })
    } else {
        serde_json::to_value(WorkerCommand::Start {
            settings: Box::new(settings.clone()),
            model_path: data
                .join("models")
                .join(models::model(&settings.model)?.filename)
                .to_string_lossy()
                .into_owned(),
            vad_path: data
                .join("models")
                .join(models::model("vad")?.filename)
                .to_string_lossy()
                .into_owned(),
        })
    }
    .map_err(|e| e.to_string())?;
    let input = child.stdin.as_mut().unwrap();
    if let Err(error) = serde_json::to_writer(&mut *input, &message)
        .map_err(|e| e.to_string())
        .and_then(|_| writeln!(input).map_err(|e| e.to_string()))
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let stdout = child.stdout.take().unwrap();
    runtime.child = Some(child);
    runtime.status.backend = if kind == Engine::Chat {
        Backend::Cpu
    } else {
        settings.backend
    };
    runtime.status.phase = "loading".into();
    runtime.status.message = if kind == Engine::Chat {
        "Starting local chat engine…"
    } else {
        "Starting local speech engine…"
    }
    .into();
    let generation = runtime.status.generation;
    announce(app, runtime, kind);
    let app = app.clone();
    let mut settings = settings.clone();
    std::thread::spawn(move || {
        let mut error_message = None;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            let Ok(event) = serde_json::from_str::<WorkerEvent>(&line) else {
                continue;
            };
            let state = app.state::<AppState>();
            let mut runtime = kind.runtime(&state).lock().unwrap();
            if !accepts_result(
                runtime.status.generation,
                generation,
                runtime.status.running,
            ) {
                return;
            }
            match event {
                WorkerEvent::Chat { mut caption } if kind == Engine::Chat => {
                    caption.generation = generation;
                    let _ = app.emit("chat-caption", caption);
                }
                WorkerEvent::ChatReset if kind == Engine::Chat => {
                    let _ = app.emit("chat-reset", generation);
                }
                WorkerEvent::Chat { .. } | WorkerEvent::ChatReset => {}

                WorkerEvent::Status { phase, message } => {
                    runtime.status.phase = phase;
                    runtime.status.message = message;
                    announce(&app, &runtime, kind);
                }
                WorkerEvent::Caption {
                    id,
                    text,
                    language,
                    latency_ms,
                    inference_ms,
                } => {
                    let _ = app.emit(
                        "caption",
                        Caption {
                            id,
                            generation,
                            text,
                            language,
                            latency_ms,
                            inference_ms,
                        },
                    );
                }
                WorkerEvent::Error { message } => {
                    error_message = Some(message);
                }
            }
        }
        let state = app.state::<AppState>();
        let mut runtime = kind.runtime(&state).lock().unwrap();
        if !accepts_result(
            runtime.status.generation,
            generation,
            runtime.status.running,
        ) {
            return;
        }
        let exit = runtime.child.take().and_then(|mut child| child.wait().ok());
        let error = error_message.unwrap_or_else(|| format!("{mode} worker exited unexpectedly ({exit:?}). Check your model files and graphics driver."));
        if kind == Engine::Voice && settings.backend == Backend::Gpu {
            runtime.status.generation += 1;
            runtime.status.warning = Some(format!("GPU unavailable: {error} Using CPU."));
            settings.backend = Backend::Cpu;
            let _ = launch_or_fallback(&app, &mut runtime, settings, kind);
        } else {
            let _ = failed(&app, &mut runtime, error, kind);
        }
    });
    Ok(())
}
