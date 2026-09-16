use ct2rs::{
    sys::Translator, tokenizers::sentencepiece::Tokenizer, ComputeType, Config, Device,
    Tokenizer as _, TranslationOptions,
};
use linguist_core::{
    audio::Pending,
    chat::{ChatCaption, ChatCommand, ChatLine, ChatSettings, LogTail, LANGUAGES},
    models, WorkerEvent as ChatEvent,
};
use std::{
    io::{self, BufRead, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

fn emit(event: ChatEvent) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = serde_json::to_writer(&mut out, &event);
    let _ = writeln!(out);
    let _ = out.flush();
}
fn status(phase: &str, message: &str) {
    emit(ChatEvent::Status {
        phase: phase.into(),
        message: message.into(),
    });
}

fn source_language(text: &str, setting: &str) -> String {
    // English chat must pass through even when another source language is selected.
    // With a fixed source, compare only that language and English. In particular,
    // Latin-script callouts must not be sent to the model as Russian.
    let fixed = isolang::Language::from_639_1(setting)
        .and_then(|language| whatlang::Lang::from_code(language.to_639_3()));
    let detected = if let Some(language) = fixed {
        whatlang::Detector::with_allowlist(vec![language, whatlang::Lang::Eng]).detect(text)
    } else {
        whatlang::detect(text)
    };
    if detected
        .as_ref()
        .is_some_and(|info| info.lang() == whatlang::Lang::Eng)
    {
        return "en".into();
    }
    if setting != "auto" {
        return setting.into();
    }
    if text.chars().filter(|c| c.is_alphabetic()).count() < 4 {
        return "unknown".into();
    }
    detected
        .and_then(|info| isolang::Language::from_639_3(info.lang().code()))
        .and_then(|lang| lang.to_639_1())
        .filter(|code| LANGUAGES.split_whitespace().any(|l| l == *code))
        .unwrap_or("unknown")
        .into()
}

struct TextEngine {
    translator: Translator,
    tokenizer: Tokenizer,
}
impl TextEngine {
    fn load(settings: &ChatSettings, path: &Path) -> Result<Self, String> {
        settings.validate()?;
        for file in models::chat_model(&settings.model)?.files {
            let name = Path::new(file.filename).file_name().unwrap();
            models::verify_file(&path.join(name), file)
                .map_err(|e| format!("Chat model verification failed: {e}"))?;
        }
        let config = Config {
            device: Device::CPU,
            compute_type: ComputeType::INT8,
            num_threads_per_replica: settings.threads as usize,
            max_queued_batches: 1,
            ..Default::default()
        };
        let spm = path.join("sentencepiece.bpe.model");
        Ok(Self {
            translator: Translator::new(path, &config).map_err(|e| e.to_string())?,
            tokenizer: Tokenizer::from_file(&spm, &spm).map_err(|e| e.to_string())?,
        })
    }
    fn translate(&self, text: &str, setting: &str) -> Result<(String, String, bool), String> {
        let language = source_language(text, setting);
        // Preserve short/ambiguous and already-English messages instead of inventing translations.
        if language == "unknown" || language == "en" || !text.chars().any(char::is_alphabetic) {
            return Ok((text.into(), language, false));
        }
        let mut tokens = self.tokenizer.encode(text).map_err(|e| e.to_string())?;
        tokens.insert(0, format!("__{language}__"));
        let options: TranslationOptions<String, String> = TranslationOptions {
            beam_size: 4,
            max_input_length: 0,
            max_decoding_length: 256,
            ..Default::default()
        };
        if tokens.len() > 512 {
            return Ok((text.into(), language, false));
        }
        let results = self
            .translator
            .translate_batch_with_target_prefix(&[tokens], &[vec!["__en__"]], &options, None)
            .map_err(|e| e.to_string())?;
        let mut tokens = results
            .into_iter()
            .next()
            .and_then(|r| r.hypotheses.into_iter().next())
            .ok_or("Text model returned no translation.")?;
        if tokens.first().is_some_and(|t| t == "__en__") {
            tokens.remove(0);
        }
        let translated = self.tokenizer.decode(tokens).map_err(|e| e.to_string())?;
        if translated.trim().is_empty() {
            return Ok((text.into(), language, false));
        }
        Ok((translated.trim().into(), language, true))
    }
}
struct Work {
    line: ChatLine,
    read: Instant,
    stream: u64,
}
fn main() {
    if let Err(message) = run() {
        emit(ChatEvent::Error { message });
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    // Standalone offline smoke test; no game, log, network, or IPC server required.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--translate") {
        if args.len() != 6 {
            return Err(
                "Usage: --translate <model-id> <model-folder> <language|auto> <text>".into(),
            );
        }
        let settings = ChatSettings {
            model: args[2].clone(),
            language: args[4].clone(),
            ..Default::default()
        };
        let engine = TextEngine::load(&settings, Path::new(&args[3]))?;
        let start = Instant::now();
        let (text, language, translated) = engine.translate(&args[5], &settings.language)?;
        println!(
            "{}",
            serde_json::json!({"text": text, "language": language, "translated": translated, "inference_ms": start.elapsed().as_millis()})
        );
        return Ok(());
    }
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let ChatCommand::Start {
        settings,
        model_path,
    } = serde_json::from_str(&line).map_err(|e| format!("Invalid chat worker command: {e}"))?
    else {
        return Ok(());
    };
    settings.validate()?;
    if settings.log_path.trim().is_empty() {
        return Err("Select CS2's console.log in Chat settings first.".into());
    }
    std::thread::spawn(|| {
        for line in io::stdin().lock().lines() {
            if line
                .ok()
                .and_then(|s| serde_json::from_str::<ChatCommand>(&s).ok())
                .is_some_and(|c| matches!(c, ChatCommand::Stop))
            {
                break;
            }
        }
        std::process::exit(0);
    });
    status("loading", "Verifying and loading local chat model…");
    // Attach now, before model loading, so messages received during loading can be queued.
    let queue = Arc::new((Mutex::new(Pending::<Work>::new(8)), Condvar::new()));
    let stream = Arc::new(AtomicU64::new(1));
    let capture_queue = queue.clone();
    let capture_stream = stream.clone();
    let loaded = Arc::new(AtomicBool::new(false));
    let capture_loaded = loaded.clone();
    let log = settings.log_path.clone();
    std::thread::spawn(move || {
        let mut tail = LogTail::new();
        let mut missing = false;
        loop {
            match tail.poll(Path::new(&log)) {
                Ok((reset, messages)) => {
                    if reset {
                        let mut items = capture_queue.0.lock().unwrap();
                        capture_stream.fetch_add(1, Ordering::SeqCst);
                        items.clear();
                        emit(ChatEvent::ChatReset);
                    }
                    if missing {
                        if capture_loaded.load(Ordering::SeqCst) {
                            status("listening", "Watching CS2 chat");
                        }
                        missing = false;
                    }
                    let mut dropped = false;
                    for line in messages {
                        dropped |= capture_queue.0.lock().unwrap().push(Work {
                            line,
                            read: Instant::now(),
                            stream: capture_stream.load(Ordering::SeqCst),
                        });
                        capture_queue.1.notify_one();
                    }
                    if dropped {
                        status("behind", "Chat is falling behind — skipped queued messages. Try the smaller text model.");
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if !missing {
                        let mut items = capture_queue.0.lock().unwrap();
                        capture_stream.fetch_add(1, Ordering::SeqCst);
                        items.clear();
                        emit(ChatEvent::ChatReset);
                        status(
                            "waiting",
                            "Waiting for console.log — launch CS2 with -condebug.",
                        );
                        missing = true;
                    }
                }
                Err(error) => {
                    emit(ChatEvent::Error {
                        message: format!("Cannot read CS2 chat log: {error}"),
                    });
                    std::process::exit(1);
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
    let engine = TextEngine::load(&settings, Path::new(&model_path))?;
    loaded.store(true, Ordering::SeqCst);
    if Path::new(&settings.log_path).is_file() {
        status("listening", "Chat model ready — watching console.log");
    } else {
        status(
            "waiting",
            "Waiting for console.log — launch CS2 with -condebug.",
        );
    }
    let mut id = 0;
    loop {
        let work = {
            let (items, ready) = &*queue;
            let mut items = items.lock().unwrap();
            loop {
                if let Some(work) = items.pop() {
                    break work;
                }
                items = ready.wait(items).unwrap();
            }
        };
        if work.stream != stream.load(Ordering::SeqCst) {
            continue;
        }
        if work.read.elapsed() > Duration::from_secs(30) {
            status("behind", "Chat is falling behind — skipped an old message.");
            continue;
        }
        let start = Instant::now();
        let (text, language, translated) =
            engine.translate(&work.line.original, &settings.language)?;
        // Serialize publication with log resets: no old result can follow ChatReset.
        let _items = queue.0.lock().unwrap();
        if work.stream != stream.load(Ordering::SeqCst) {
            continue;
        }
        if work.read.elapsed() > Duration::from_secs(30) {
            status(
                "behind",
                "Chat translation expired. Try the smaller text model.",
            );
            continue;
        }
        id += 1;
        emit(ChatEvent::Chat {
            caption: ChatCaption {
                id,
                generation: 0,
                line: work.line,
                text,
                language,
                translated,
                latency_ms: work.read.elapsed().as_millis() as u64,
                inference_ms: start.elapsed().as_millis() as u64,
            },
        });
        status("listening", "Watching CS2 chat");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_detection_and_fixed_language_are_independent() {
        assert_eq!(
            source_language("Please come with me to the bomb site.", "auto"),
            "en"
        );
        assert_eq!(source_language("gg", "auto"), "unknown");
        assert_eq!(source_language("да", "ru"), "ru");
        assert_eq!(source_language("go B", "ru"), "en");
        assert_eq!(source_language("gg", "ru"), "en");
        assert_eq!(
            source_language("Zwei Spieler kommen durch die Mitte.", "de"),
            "de"
        );
        assert_eq!(
            source_language("Please come with me to the bomb site.", "ru"),
            "en"
        );
    }
}
