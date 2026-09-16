export type Backend = 'cpu' | 'gpu';
export interface Settings {
  backend: Backend; model: 'base' | 'small' | 'medium' | 'large-v3'; language: string; threads: number;
  font_size: number; opacity: number;
  position: { x: number; y: number; width: number; height: number } | null;
}
export interface EngineStatus {
  phase: string; message: string; backend: Backend; running: boolean; generation: number; warning: string | null;
}
export interface Caption { id: number; generation: number; text: string; language: string; latency_ms: number; inference_ms: number }
export interface InstalledModel { id: string; bytes: number; installed: boolean }
export interface DownloadProgress { id: string; received: number; total: number; phase: string; message: string }
export interface Snapshot {
  settings: Settings; status: EngineStatus; models: InstalledModel[];
  download: DownloadProgress | null; overlay_locked: boolean; supported: boolean; languages: string[];
}
