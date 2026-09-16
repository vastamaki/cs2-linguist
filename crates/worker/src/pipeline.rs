use crate::{status, Queue};
use linguist_core::audio::{Segmenter, FRAME, SAMPLE_RATE};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams};

pub struct SpeechPipeline {
    vad: WhisperVadContext,
    history: Vec<f32>,
    pending: VecDeque<(Vec<f32>, Instant)>,
    segmenter: Segmenter,
    queue: Queue,
    stream: u64,
}

impl SpeechPipeline {
    pub fn new(path: &str, queue: Queue, stream: u64) -> Result<Self, String> {
        let mut params = WhisperVadContextParams::default();
        params.set_n_threads(1);
        params.set_use_gpu(false);
        Ok(Self {
            vad: WhisperVadContext::new(path, params)
                .map_err(|e| format!("Cannot load speech detector: {e}"))?,
            history: Vec::new(),
            pending: VecDeque::new(),
            segmenter: Segmenter::default(),
            queue,
            stream,
        })
    }

    pub fn frame(&mut self, frame: &[f32], captured: Instant) -> Result<(), String> {
        self.pending.push_back((frame.to_vec(), captured));
        if self.pending.len() < 4 {
            return Ok(());
        }
        // whisper.cpp resets VAD recurrent state per call. Reprocess 512 ms of context
        // with each 128 ms batch, then consume only the four new frame decisions.
        let mut window = self.history.clone();
        for (frame, _) in &self.pending {
            window.extend_from_slice(frame);
        }
        self.vad
            .detect_speech(&window)
            .map_err(|e| format!("Speech detection failed: {e}"))?;
        let probabilities = self.vad.probabilities();
        if probabilities.len() < self.pending.len() {
            return Err("Speech detector returned too few frames.".into());
        }
        let offset = probabilities.len() - self.pending.len();
        let decisions: Vec<bool> = probabilities[offset..].iter().map(|p| *p >= 0.5).collect();
        self.history = window[window.len().saturating_sub(16 * FRAME)..].to_vec();
        for speech in decisions {
            let (frame, captured) = self.pending.pop_front().unwrap();
            if let Some(utterance) = self.segmenter.push(&frame, speech, captured, self.stream) {
                let (items, ready) = &*self.queue;
                let dropped = items.lock().unwrap().push(utterance);
                ready.notify_one();
                if dropped {
                    status(
                        "behind",
                        "Falling behind — skipped queued audio. Try base or GPU mode.",
                    );
                }
            }
        }
        Ok(())
    }
}

pub fn replay(path: &str, vad: &str, queue: Queue, stream: Arc<AtomicU64>) -> Result<(), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != SAMPLE_RATE as u32
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("Benchmark WAV must be 16 kHz, mono, signed 16-bit PCM.".into());
    }
    let mut pipeline = SpeechPipeline::new(vad, queue, stream.load(Ordering::SeqCst))?;
    status("listening", "Replaying benchmark WAV at real-time speed");
    let mut samples = reader.samples::<i16>();
    let mut deadline = Instant::now();
    loop {
        let frame: Vec<f32> = samples
            .by_ref()
            .take(FRAME)
            .map(|s| s.map(|s| s as f32 / 32768.0))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        if frame.is_empty() {
            break;
        }
        let mut padded = [0.0; FRAME];
        padded[..frame.len()].copy_from_slice(&frame);
        pipeline.frame(&padded, Instant::now())?;
        deadline += Duration::from_millis(32);
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    }
    for _ in 0..20 {
        pipeline.frame(&[0.0; FRAME], Instant::now())?;
        std::thread::sleep(Duration::from_millis(32));
    }
    status(
        "draining",
        "Benchmark input finished; waiting for remaining captions",
    );
    Ok(())
}
