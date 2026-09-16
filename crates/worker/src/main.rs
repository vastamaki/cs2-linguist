#[cfg(windows)]
mod capture;
mod pipeline;

use linguist_core::audio::{Pending, Utterance};
use linguist_core::{models, Backend, WorkerCommand, WorkerEvent};
use std::{
    io::{self, BufRead, Write},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Instant,
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub type Queue = Arc<(Mutex<Pending<Utterance>>, Condvar)>;

pub fn emit(event: WorkerEvent) {
    let mut out = io::stdout().lock();
    if serde_json::to_writer(&mut out, &event).is_err()
        || writeln!(out).is_err()
        || out.flush().is_err()
    {
        std::process::exit(0); // Parent exited; never keep capturing after losing its control pipe.
    }
}
pub fn status(phase: &str, message: impl Into<String>) {
    emit(WorkerEvent::Status {
        phase: phase.into(),
        message: message.into(),
    });
}

fn main() {
    if let Err(message) = run() {
        emit(WorkerEvent::Error { message });
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    whisper_rs::install_logging_hooks();
    // stdout is exclusively JSON; whisper.cpp's diagnostic output goes to stderr.
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let command: WorkerCommand = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    let WorkerCommand::Start {
        settings,
        model_path,
        vad_path,
    } = command
    else {
        return Ok(());
    };
    settings.validate()?;
    if (settings.backend == Backend::Gpu) != cfg!(feature = "gpu") {
        return Err("Worker backend does not match the selected mode.".into());
    }
    std::thread::spawn(|| {
        // Stop or EOF also interrupts an ongoing inference immediately.
        for line in io::stdin().lock().lines() {
            if line.is_err()
                || matches!(
                    line.ok()
                        .and_then(|s| serde_json::from_str::<WorkerCommand>(&s).ok()),
                    Some(WorkerCommand::Stop)
                )
            {
                break;
            }
        }
        std::process::exit(0);
    });
    status("loading", "Verifying and loading local models…");
    models::verify_file(Path::new(&model_path), models::model(&settings.model)?)
        .map_err(|e| e.to_string())?;
    models::verify_file(Path::new(&vad_path), models::model("vad")?).map_err(|e| e.to_string())?;

    #[cfg(feature = "gpu")]
    {
        verify_gpu()?;
        unsafe {
            whisper_rs::whisper_rs_sys::whisper_log_set(Some(gpu_log), std::ptr::null_mut());
        }
    }
    let mut context_params = WhisperContextParameters::default();
    context_params.use_gpu(cfg!(feature = "gpu"));
    let context = WhisperContext::new_with_params(&model_path, context_params)
        .map_err(|e| format!("Cannot load Whisper: {e}"))?;
    let mut state = context.create_state().map_err(|e| e.to_string())?;
    #[cfg(feature = "gpu")]
    if GPU_FAILED.load(Ordering::SeqCst) {
        return Err("Whisper could not initialize the Vulkan backend.".into());
    }
    let queue: Queue = Arc::new((Mutex::new(Pending::new(2)), Condvar::new()));
    let stream = Arc::new(AtomicU64::new(1));

    // A WAV path is a developer benchmark input; normal operation captures CS2 only.
    let wav = std::env::args().skip(1).collect::<Vec<_>>();
    let replay = wav.first().map(String::as_str) == Some("--wav");
    let source_queue = queue.clone();
    let source_stream = stream.clone();
    std::thread::spawn(move || {
        let result = if wav.first().map(String::as_str) == Some("--wav") {
            wav.get(1)
                .ok_or_else(|| "Missing WAV path.".into())
                .and_then(|path| pipeline::replay(path, &vad_path, source_queue, source_stream))
        } else {
            #[cfg(windows)]
            {
                capture::watch_cs2(&vad_path, source_queue, source_stream)
            }
            #[cfg(not(windows))]
            {
                Err(
                    "Live CS2 capture requires Windows 11. Use --wav for an offline benchmark."
                        .into(),
                )
            }
        };
        if let Err(message) = result {
            emit(WorkerEvent::Error { message });
            std::process::exit(1);
        }
    });

    let mut id = 0;
    loop {
        let item = {
            let (items, ready) = &*queue;
            let mut items = items.lock().unwrap();
            loop {
                if let Some(item) = items.pop() {
                    break item;
                }
                items = ready.wait(items).unwrap();
            }
        };
        if item.stream != stream.load(Ordering::SeqCst) {
            continue;
        }
        if item.stale() {
            status(
                "behind",
                "Falling behind — skipped old audio. Try base or GPU mode.",
            );
            continue;
        }
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(settings.threads as i32);
        params.set_translate(true);
        params.set_language(if settings.language == "auto" {
            None
        } else {
            Some(&settings.language)
        });
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        params.set_no_timestamps(true);
        params.set_temperature_inc(0.0);
        let began = Instant::now();
        state
            .full(params, &item.samples)
            .map_err(|e| format!("Whisper inference failed: {e}"))?;
        if item.stream != stream.load(Ordering::SeqCst) {
            continue;
        }
        if item.stale() {
            status(
                "behind",
                "Falling behind — result expired. Try base or GPU mode.",
            );
            continue;
        }
        let mut text = String::new();
        for segment in state.as_iter() {
            if segment.no_speech_probability() < 0.6 {
                text.push_str(&segment.to_str_lossy().map_err(|e| e.to_string())?);
            }
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        id += 1;
        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state())
            .unwrap_or("unknown")
            .to_string();
        emit(WorkerEvent::Caption {
            id,
            text,
            language,
            latency_ms: item.speech_end.elapsed().as_millis() as u64,
            inference_ms: began.elapsed().as_millis() as u64,
        });
        status(
            "listening",
            if replay {
                "Replaying benchmark WAV"
            } else {
                "Listening to CS2"
            },
        );
    }
}

#[cfg(feature = "gpu")]
fn verify_gpu() -> Result<(), String> {
    // Do not label a silent whisper.cpp CPU fallback as GPU execution.
    // Actual backend availability is exposed by the compiled ggml registry.
    unsafe {
        whisper_rs::whisper_rs_sys::ggml_backend_load_all();
        let count = whisper_rs::whisper_rs_sys::ggml_backend_dev_count();
        for index in 0..count {
            let device = whisper_rs::whisper_rs_sys::ggml_backend_dev_get(index);
            let kind = whisper_rs::whisper_rs_sys::ggml_backend_dev_type(device);
            if kind == whisper_rs::whisper_rs_sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU
                || kind == whisper_rs::whisper_rs_sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU
            {
                return Ok(());
            }
        }
    }
    Err("No compatible Vulkan GPU was found. Update your graphics driver or use CPU mode.".into())
}

#[cfg(feature = "gpu")]
static GPU_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "gpu")]
unsafe extern "C" fn gpu_log(
    _level: whisper_rs::whisper_rs_sys::ggml_log_level,
    text: *const std::ffi::c_char,
    _data: *mut std::ffi::c_void,
) {
    if text.is_null() {
        return;
    }
    let message = std::ffi::CStr::from_ptr(text).to_string_lossy();
    // whisper.cpp can silently substitute CPU when backend initialization fails.
    if message.contains("whisper_backend_init_gpu: no GPU found")
        || message.contains("whisper_backend_init_gpu: failed to initialize")
    {
        GPU_FAILED.store(true, Ordering::SeqCst);
    }
}
