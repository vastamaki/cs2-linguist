pub mod audio;
pub mod chat;
pub mod models;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cpu,
    Gpu,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU (Vulkan)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlayPosition {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl OverlayPosition {
    pub fn visible_on(&self, monitor: &Self) -> bool {
        let overlap_x = (self.x as i64 + self.width as i64)
            .min(monitor.x as i64 + monitor.width as i64)
            - (self.x as i64).max(monitor.x as i64);
        let overlap_y = (self.y as i64 + self.height as i64)
            .min(monitor.y as i64 + monitor.height as i64)
            - (self.y as i64).max(monitor.y as i64);
        overlap_x >= self.width.min(120) as i64 && overlap_y >= self.height.min(40) as i64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub voice_enabled: bool,
    pub chat: chat::ChatSettings,
    pub backend: Backend,
    pub model: String,
    pub language: String,
    pub threads: u32,
    pub font_size: u32,
    pub opacity: f64,
    pub position: Option<OverlayPosition>,
}

impl Default for Settings {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        Self {
            voice_enabled: true,
            chat: chat::ChatSettings::default(),
            backend: Backend::Cpu,
            model: "small".into(),
            language: "auto".into(),
            threads: (cores / 2).clamp(1, 4) as u32,
            font_size: 24,
            opacity: 0.45,
            position: None,
        }
    }
}

// The original 99 languages supported by every offered multilingual model.
// Cantonese is omitted because it is only supported by large-v3.
pub const LANGUAGES: &str = "auto en zh de es ru ko fr ja pt tr pl ca nl ar sv it id hi fi vi he uk el ms cs ro da hu ta no th ur hr bg lt la mi ml cy sk te fa lv bn sr az sl kn et mk br eu is hy ne mn bs kk sq sw gl mr pa si km sn yo so af oc ka be tg sd gu am yi lo uz fo ht ps tk nn mt sa lb my bo tl mg as tt haw ln ha ba jw su";

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        self.chat.validate()?;
        if !models::MODELS
            .iter()
            .any(|m| m.id == self.model && m.id != "vad")
        {
            return Err("Choose a supported multilingual Whisper model.".into());
        }
        if !LANGUAGES.split_whitespace().any(|s| s == self.language) {
            return Err("Unsupported source language.".into());
        }
        if !(1..=32).contains(&self.threads) {
            return Err("CPU threads must be between 1 and 32.".into());
        }
        if !(14..=48).contains(&self.font_size) {
            return Err("Subtitle size must be between 14 and 48.".into());
        }
        if !self.opacity.is_finite() || !(0.0..=0.9).contains(&self.opacity) {
            return Err("Background opacity must be between 0 and 90%.".into());
        }
        Ok(())
    }
    pub fn engine_changed(&self, other: &Self) -> bool {
        self.voice_enabled != other.voice_enabled
            || self.backend != other.backend
            || self.model != other.model
            || self.language != other.language
            || self.threads != other.threads
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub phase: String,
    pub message: String,
    pub backend: Backend,
    pub running: bool,
    pub generation: u64,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caption {
    pub id: u64,
    pub generation: u64,
    pub text: String,
    pub language: String,
    pub latency_ms: u64,
    pub inference_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerCommand {
    Start {
        settings: Box<Settings>,
        model_path: String,
        vad_path: String,
    },
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerEvent {
    Chat {
        caption: chat::ChatCaption,
    },
    ChatReset,
    Status {
        phase: String,
        message: String,
    },
    Caption {
        id: u64,
        text: String,
        language: String,
        latency_ms: u64,
        inference_ms: u64,
    },
    Error {
        message: String,
    },
}

/// Both the desktop and UI reject output from stopped/replaced workers.
pub fn accepts_result(active_generation: u64, result_generation: u64, running: bool) -> bool {
    running && active_generation == result_generation
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_spanning_monitors_does_not_jump_but_disconnected_positions_recover() {
        let screen = OverlayPosition {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let mut overlay = OverlayPosition {
            x: -300,
            y: 400,
            width: 720,
            height: 230,
        };
        assert!(overlay.visible_on(&screen));
        overlay.x = 2000;
        assert!(!overlay.visible_on(&screen));
    }
    #[test]
    fn stopped_and_replaced_workers_cannot_publish() {
        assert!(accepts_result(3, 3, true));
        assert!(!accepts_result(4, 3, true));
        assert!(!accepts_result(3, 3, false));
    }
    #[test]
    fn reject_unsupported_models_and_invalid_controls() {
        let mut s = Settings::default();
        assert!(s.validate().is_ok());
        for model in ["base", "small", "medium", "large-v3"] {
            s.model = model.into();
            assert!(s.validate().is_ok(), "{model}");
        }
        for model in ["turbo", "base.en", "medium.en", "vad", "unknown"] {
            s.model = model.into();
            assert!(s.validate().is_err(), "{model}");
        }
        s.model = "base".into();
        s.opacity = f64::NAN;
        assert!(s.validate().is_err());
    }
}
