use crate::OverlayPosition;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};

pub const LANGUAGES: &str = "auto af am ar ast az ba be bg bn br bs ca ceb cs cy da de el en es et fa ff fi fr fy ga gd gl gu ha he hi hr ht hu hy id ig ilo is it ja jv ka kk km kn ko lb lg ln lo lt lv mg mk ml mn mr ms my ne nl no ns oc or pa pl ps pt ro ru sd si sk sl so sq sr ss su sv sw ta th tl tn tr uk ur uz vi wo xh yi yo zh zu";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChatSettings {
    pub enabled: bool,
    pub model: String,
    pub language: String,
    pub log_path: String,
    pub threads: u32,
    pub show_original: bool,
    pub position: Option<OverlayPosition>,
}
impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "m2m100-418m".into(),
            language: "auto".into(),
            log_path: String::new(),
            threads: 2,
            show_original: true,
            position: None,
        }
    }
}
impl ChatSettings {
    pub fn validate(&self) -> Result<(), String> {
        crate::models::chat_model(&self.model)?;
        if !LANGUAGES.split_whitespace().any(|l| l == self.language) {
            return Err("Unsupported chat language.".into());
        }
        if !(1..=8).contains(&self.threads) {
            return Err("Chat CPU threads must be between 1 and 8.".into());
        }
        if self.log_path.contains('\0') || self.log_path.len() > 32768 {
            return Err("Invalid chat log path.".into());
        }
        Ok(())
    }
    pub fn engine_changed(&self, other: &Self) -> bool {
        self.enabled != other.enabled
            || self.model != other.model
            || self.language != other.language
            || self.log_path != other.log_path
            || self.threads != other.threads
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatLine {
    pub channel: String,
    pub player: String,
    pub original: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCaption {
    pub id: u64,
    pub generation: u64,
    #[serde(flatten)]
    pub line: ChatLine,
    pub text: String,
    pub language: String,
    pub translated: bool,
    pub latency_ms: u64,
    pub inference_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCommand {
    Start {
        settings: ChatSettings,
        model_path: String,
    },
    Stop,
}

pub fn parse_line(line: &[u8]) -> Option<ChatLine> {
    let clean: String = std::str::from_utf8(line)
        .ok()?
        .chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect();
    let (index, channel) = ["[CT]", "[T]", "[ALL]", "[TEAM]"]
        .into_iter()
        .filter_map(|tag| clean.find(tag).map(|i| (i, tag)))
        .min_by_key(|v| v.0)?;
    let (player, text) = clean[index + channel.len()..].trim().split_once(": ")?;
    let player = player.trim();
    let text = text.trim();
    if player.is_empty()
        || text.is_empty()
        || player.chars().count() > 128
        || text.chars().count() > 2048
    {
        return None;
    }
    Some(ChatLine {
        channel: channel[1..channel.len() - 1].into(),
        player: player.into(),
        original: text.into(),
    })
}

// The file is reopened each poll so rotation cannot leave us reading an old handle.
// A trailing byte anchor also detects truncation followed by rapid regrowth.
#[derive(Default)]
pub struct LogTail {
    offset: u64,
    anchor: Vec<u8>,
    pending: Vec<u8>,
    oversized: bool,
    attached: bool,
    start_at_end: bool,
    created: Option<SystemTime>,
}
impl LogTail {
    pub fn new() -> Self {
        Self {
            start_at_end: true,
            ..Self::default()
        }
    }
    pub fn poll(&mut self, path: &Path) -> io::Result<(bool, Vec<ChatLine>)> {
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    self.start_at_end = false;
                    self.attached = false;
                    self.offset = 0;
                    self.pending.clear();
                    self.anchor.clear();
                }
                return Err(e);
            }
        };
        let meta = file.metadata()?;
        let created = meta.created().ok();
        let mut reset = !self.attached && !self.start_at_end;
        if self.attached {
            reset = meta.len() < self.offset || created != self.created;
            if !reset && !self.anchor.is_empty() {
                file.seek(SeekFrom::Start(self.offset - self.anchor.len() as u64))?;
                let mut check = vec![0; self.anchor.len()];
                reset = file.read_exact(&mut check).is_err() || check != self.anchor;
            }
        }
        if !self.attached || reset {
            self.offset = if self.start_at_end { meta.len() } else { 0 };
            self.pending.clear();
            self.oversized = false;
            // Ignore the remainder of an existing incomplete line on initial attach.
            if self.offset > 0 {
                file.seek(SeekFrom::Start(self.offset - 1))?;
                let mut last = [0];
                file.read_exact(&mut last)?;
                self.oversized = last[0] != b'\n';
            }
        }
        self.attached = true;
        self.start_at_end = false;
        self.created = created;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        (&mut file).take(256 * 1024).read_to_end(&mut bytes)?;
        self.offset += bytes.len() as u64;
        let mut messages = Vec::new();
        for b in bytes {
            if b == b'\n' {
                if !self.oversized {
                    if let Some(line) = parse_line(&self.pending) {
                        messages.push(line);
                    }
                }
                self.pending.clear();
                self.oversized = false;
            } else if !self.oversized {
                if self.pending.len() < 16384 {
                    self.pending.push(b);
                } else {
                    self.pending.clear();
                    self.oversized = true;
                }
            }
        }
        self.anchor.resize(self.offset.min(64) as usize, 0);
        file.seek(SeekFrom::Start(self.offset - self.anchor.len() as u64))?;
        file.read_exact(&mut self.anchor)?;
        Ok((reset, messages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn chat_settings_migrate_and_restart_independently() {
        let original: crate::Settings = serde_json::from_str(r#"{"model":"small"}"#).unwrap();
        assert!(original.voice_enabled);
        assert!(!original.chat.enabled);
        let mut changed = original.clone();
        changed.chat.enabled = true;
        changed.chat.model = "m2m100-1.2b".into();
        assert!(changed.validate().is_ok());
        assert!(!original.engine_changed(&changed));
        assert!(original.chat.engine_changed(&changed.chat));
        let mut appearance = changed.clone();
        appearance.chat.show_original = false;
        assert!(!changed.chat.engine_changed(&appearance.chat));
        appearance.chat.model = "small".into();
        assert!(appearance.validate().is_err());
        appearance = original.clone();
        appearance.model = "m2m100-418m".into();
        assert!(appearance.validate().is_err());
    }
    #[test]
    fn tail_handles_unicode_partial_lines_repeats_and_log_restarts() {
        let path =
            std::env::temp_dir().join(format!("linguist-chat-test-{}.log", std::process::id()));
        std::fs::write(&path, b"[ALL] old: old history\n").unwrap();
        let mut tail = LogTail::new();
        assert!(tail.poll(&path).unwrap().1.is_empty());
        let line = "[ALL] Игрок: Привет, идём на B\n".as_bytes();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(&line[..17]).unwrap();
        assert!(tail.poll(&path).unwrap().1.is_empty());
        file.write_all(&line[17..]).unwrap();
        file.write_all(line).unwrap();
        let messages = tail.poll(&path).unwrap().1;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], messages[1]);
        assert_eq!(messages[0].player, "Игрок");
        assert!(tail.poll(&path).unwrap().1.is_empty());
        drop(file);
        std::fs::write(&path, b"[CT] new: go mid\n").unwrap();
        let (reset, messages) = tail.poll(&path).unwrap();
        assert!(reset);
        assert_eq!(messages[0].original, "go mid");
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            tail.poll(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        std::fs::write(&path, b"[T] new: new game\n").unwrap();
        assert!(tail.poll(&path).unwrap().0);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn only_chat_lines_and_bounded_messages_are_accepted() {
        assert!(parse_line(b"network error").is_none());
        assert!(parse_line(b"[ALL] : empty player").is_none());
        assert!(parse_line(format!("[ALL] player: {}", "a".repeat(2049)).as_bytes()).is_none());
        assert_eq!(
            parse_line(b"[Console] *DEAD* [CT] player: <script>hello</script>\r")
                .unwrap()
                .original,
            "<script>hello</script>"
        );
    }
}
