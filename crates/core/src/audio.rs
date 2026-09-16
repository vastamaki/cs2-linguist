use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const SAMPLE_RATE: usize = 16_000;
pub const FRAME: usize = 512;
pub const MAX_AGE: Duration = Duration::from_secs(10);
const PRE_ROLL: usize = SAMPLE_RATE / 5;
const SILENCE: usize = SAMPLE_RATE * 4 / 10;
const MAX_SEGMENT: usize = SAMPLE_RATE * 5;

pub struct Utterance {
    pub samples: Vec<f32>,
    pub speech_end: Instant,
    pub stream: u64,
}

impl Utterance {
    pub fn stale(&self) -> bool {
        self.speech_end.elapsed() > MAX_AGE
    }
}

#[derive(Default)]
pub struct Segmenter {
    pre_roll: VecDeque<f32>,
    audio: Vec<f32>,
    silence: usize,
    voiced: usize,
    last_voice: Option<Instant>,
}

impl Segmenter {
    /// Called with 32 ms frames and a VAD decision. No transcript-based deduplication:
    /// repeated callouts are meaningful, and each captured frame is consumed once.
    pub fn push(
        &mut self,
        frame: &[f32],
        speech: bool,
        captured: Instant,
        stream: u64,
    ) -> Option<Utterance> {
        if self.audio.is_empty() && !speech {
            self.pre_roll.extend(frame);
            while self.pre_roll.len() > PRE_ROLL {
                self.pre_roll.pop_front();
            }
            return None;
        }
        if self.audio.is_empty() {
            self.audio.extend(self.pre_roll.drain(..));
        }
        self.audio.extend_from_slice(frame);
        if speech {
            self.silence = 0;
            self.voiced += frame.len();
            self.last_voice = Some(captured);
        } else {
            self.silence += frame.len();
        }
        if self.silence >= SILENCE || self.audio.len() >= MAX_SEGMENT {
            // ponytail: hard five-second splits can cut a word during continuous speech;
            // add overlap with timestamp-based reconciliation if game recordings justify it.
            let samples = std::mem::take(&mut self.audio);
            let valid = self.voiced >= SAMPLE_RATE / 10;
            let speech_end = self.last_voice.take().unwrap_or(captured);
            self.silence = 0;
            self.voiced = 0;
            return valid.then_some(Utterance {
                samples,
                speech_end,
                stream,
            });
        }
        None
    }
}

/// Single consumer, tiny bounded queue. Caller holds its mutex only while pushing/popping.
pub struct Pending<T> {
    items: VecDeque<T>,
    capacity: usize,
}
impl<T> Pending<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            items: VecDeque::new(),
            capacity,
        }
    }
    pub fn push(&mut self, item: T) -> bool {
        let dropped = self.items.len() == self.capacity;
        if dropped {
            self.items.pop_front();
        }
        self.items.push_back(item);
        dropped
    }
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop_front()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_callouts_preroll_and_continuous_speech() {
        let mut s = Segmenter::default();
        let now = Instant::now();
        for _ in 0..100 {
            assert!(s.push(&[0.0; FRAME], false, now, 1).is_none());
        }
        for _ in 0..5 {
            assert!(s.push(&[0.3; FRAME], true, now, 1).is_none());
        }
        let mut result = None;
        for _ in 0..13 {
            result = s.push(&[0.0; FRAME], false, now, 1).or(result);
        }
        let result = result.unwrap();
        assert_eq!(result.samples.len(), PRE_ROLL + 18 * FRAME);
        assert_eq!(result.stream, 1);
        let mut count = 0;
        for _ in 0..320 {
            if let Some(chunk) = s.push(&[0.3; FRAME], true, now, 2) {
                assert!(chunk.samples.len() <= MAX_SEGMENT + FRAME);
                count += 1;
            }
        }
        assert_eq!(count, 2);
    }
    #[test]
    fn backlog_keeps_newest_and_old_audio_expires() {
        let mut p = Pending::new(2);
        assert!(!p.push(1));
        assert!(!p.push(2));
        assert!(p.push(3));
        assert_eq!(p.pop(), Some(2));
        assert_eq!(p.pop(), Some(3));
        let u = Utterance {
            samples: vec![],
            speech_end: Instant::now() - Duration::from_secs(11),
            stream: 1,
        };
        assert!(u.stale());
    }
}
