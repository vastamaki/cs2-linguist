import type { Caption, ChatCaption, EngineStatus, Settings, TranslationHistory } from './types';

export function latestHistory(current: TranslationHistory, incoming: TranslationHistory): TranslationHistory {
  // A command response can arrive after a newer event from a different worker.
  return incoming.revision >= current.revision ? incoming : current;
}
export function chatText(caption: ChatCaption): string {
  return `[${caption.channel}] ${caption.player}: ${caption.text}`;
}
export function renderHistory(target: HTMLElement, entries: Caption[], empty: string) {
  // Keep a user's scroll position while they read earlier messages.
  const follow = target.scrollHeight - target.scrollTop - target.clientHeight < 32;
  const previousTop = target.scrollTop;
  const keys = new Set(entries.map(c => `${c.generation}:${c.id}`));
  let removedHeight = 0;
  for (const row of target.children) {
    if (!(row instanceof HTMLElement) || !row.dataset.key || keys.has(row.dataset.key)) break;
    removedHeight += row.offsetHeight;
  }
  target.replaceChildren(...entries.map(caption => {
    const row = document.createElement('div'); row.className = 'history-message';
    row.dataset.key = `${caption.generation}:${caption.id}`;
    row.textContent = 'player' in caption ? chatText(caption as ChatCaption) : caption.text;
    return row;
  }));
  if (!entries.length) {
    const hint = document.createElement('p'); hint.className = 'history-empty'; hint.textContent = empty; target.append(hint);
  }
  target.scrollTop = follow ? target.scrollHeight : Math.max(0, previousTop - removedHeight);
}

const languageNames = new Intl.DisplayNames(['en'], { type: 'language' });
export function languageName(code: string): string {
  if (code === 'auto') return 'Auto';
  if (!code || code === 'unknown') return '—';
  try { return languageNames.of(code) ?? code; } catch { return code; }
}

export interface VisibleCaption extends Caption { expires: number }
export function appendCaption(current: VisibleCaption[], caption: Caption, status: EngineStatus, now: number): VisibleCaption[] {
  if (!status.running || caption.generation !== status.generation) return current;
  if (current.some(c => c.generation === caption.generation && c.id === caption.id)) return current;
  return [...current.filter(c => c.expires > now), { ...caption, expires: now + 8000 }].slice(-3);
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
