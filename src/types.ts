export type Backend = 'cpu' | 'gpu';
export interface ChatSettings {
  enabled: boolean; model: 'm2m100-418m' | 'm2m100-1.2b'; language: string;
  log_path: string; threads: number; show_original: boolean; position: Settings['position'];
}
export const defaultChatSettings: ChatSettings = { enabled: false, model: 'm2m100-418m', language: 'auto', log_path: '', threads: 2, show_original: true, position: null };
export interface Settings {
  voice_enabled: boolean; chat: ChatSettings;
  backend: Backend; model: 'base' | 'small' | 'medium' | 'large-v3'; language: string; threads: number;
  font_size: number; opacity: number;
  position: { x: number; y: number; width: number; height: number } | null;
}
export interface EngineStatus {
  phase: string; message: string; backend: Backend; running: boolean; generation: number; warning: string | null;
}
export interface Caption { id: number; generation: number; text: string; language: string; latency_ms: number; inference_ms: number }
export interface ChatCaption extends Caption { channel: string; player: string; original: string; translated: boolean }
export interface InstalledModel { id: string; bytes: number; installed: boolean }
export interface DownloadProgress { id: string; received: number; total: number; phase: string; message: string }
export interface Snapshot {
  settings: Settings; status: EngineStatus; chat_status: EngineStatus; chat_overlay_locked: boolean; chat_languages: string[]; models: InstalledModel[];
  download: DownloadProgress | null; overlay_locked: boolean; supported: boolean; languages: string[];
}
