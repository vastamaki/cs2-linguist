import type { Caption, EngineStatus, Settings } from './types';

const languageNames = new Intl.DisplayNames(['en'], { type: 'language' });
export function languageName(code: string): string {
  if (code === 'auto') return 'Auto';
  if (!code || code === 'unknown') return '—';
  try { return languageNames.of(code) ?? code; } catch { return code; }
}

export interface VisibleCaption extends Caption { expires: number }
export function appendCaption<T extends Caption>(current: (T & { expires: number })[], caption: T, status: EngineStatus, now: number, limit = 3, lifetime = 8000): (T & { expires: number })[] {
  if (!status.running || caption.generation !== status.generation) return current;
  if (current.some(c => c.generation === caption.generation && c.id === caption.id)) return current;
  return [...current.filter(c => c.expires > now), { ...caption, expires: now + lifetime }].slice(-limit);
}

export function overlayStats(settings: Settings, status: EngineStatus, caption: VisibleCaption | undefined, now: number) {
  const fresh = status.running && caption?.generation === status.generation && caption.expires > now ? caption : undefined;
  return {
    // With a fixed source, Whisper reports the forced language, not a detection.
    detected: settings.language === 'auto' ? languageName(fresh?.language ?? '') : 'Off',
    spoken: languageName(settings.language),
    delay: fresh ? `${(fresh.latency_ms / 1000).toFixed(1)} s` : '—',
    decode: fresh ? `${(fresh.inference_ms / 1000).toFixed(1)} s` : '—',
    backend: status.backend === 'gpu' ? 'GPU' : 'CPU',
    model: settings.model,
  };
}
