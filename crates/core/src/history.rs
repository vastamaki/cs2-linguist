use crate::{chat::ChatCaption, Caption};
use serde::Serialize;

// Owned by the app, independently of worker sessions and webview navigation.
// Never persisted: closing the app clears both histories.
#[derive(Clone, Default, Serialize)]
pub struct History {
    pub revision: u64,
    pub voice: Vec<Caption>,
    pub chat: Vec<ChatCaption>,
}

fn append<T>(entries: &mut Vec<T>, entry: T) {
    entries.push(entry);
    if entries.len() > 10 {
        entries.remove(0);
    }
}

impl History {
    pub fn record_voice(&mut self, caption: Caption) {
        append(&mut self.voice, caption);
        self.revision += 1;
    }
    pub fn record_chat(&mut self, caption: ChatCaption) {
        append(&mut self.chat, caption);
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{chat::ChatLine, Settings};

    #[test]
    fn histories_roll_independently_and_survive_new_sessions() {
        let mut history = History::default();
        for id in 1..=11 {
            history.record_chat(ChatCaption {
                id,
                generation: 1,
                line: ChatLine {
                    channel: "CT".into(),
                    player: "Player".into(),
                    original: "go B".into(),
                },
                text: "go B".into(),
                language: "en".into(),
                translated: false,
                latency_ms: 0,
                inference_ms: 0,
            });
        }
        assert_eq!(history.chat.len(), 10);
        assert_eq!(history.chat[0].id, 2);
        assert_eq!(history.chat[9].text, "go B");
        for id in 1..=11 {
            history.record_voice(Caption {
                id,
                generation: 2,
                text: "Watch mid".into(),
                language: "en".into(),
                latency_ms: 0,
                inference_ms: 0,
            });
        }
        assert_eq!(history.voice.len(), 10);
        assert_eq!(history.voice[0].id, 2);
        assert_eq!(history.chat.len(), 10);
        let mut next = history.chat[9].clone();
        next.generation = 3;
        next.id = 1;
        history.record_chat(next);
        assert_eq!(history.chat[0].id, 3);
        assert_eq!(history.revision, 23);
    }

    #[test]
    fn app_only_mode_hides_even_unlocked_overlays_without_restarting_engines() {
        let original: Settings =
            serde_json::from_str(r#"{"chat":{"show_original":true}}"#).unwrap();
        assert!(original.overlays_enabled);
        let mut app_only = original.clone();
        app_only.overlays_enabled = false;
        assert!(!original.engine_changed(&app_only));
        assert!(!original.chat.engine_changed(&app_only.chat));
        for running in [false, true] {
            for locked in [false, true] {
                assert!(!app_only.show_overlay(running, locked));
                assert_eq!(original.show_overlay(running, locked), running || !locked);
            }
        }
    }
}
