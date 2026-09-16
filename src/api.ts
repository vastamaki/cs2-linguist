import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { defaultChatSettings, type Snapshot } from './types';

export const desktop = isTauri();
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (desktop) return invoke<T>(command, args);
  if (command === 'snapshot') return {
    settings: { voice_enabled: true, overlays_enabled: true, chat: { ...defaultChatSettings }, backend: 'cpu', model: 'small', language: 'auto', threads: 4, font_size: 24, opacity: 0.45, position: null },
    status: { phase: 'paused', message: 'Ready when you are', backend: 'cpu', running: false, generation: 0, warning: null },
    chat_status: { phase: 'paused', message: 'Chat paused', backend: 'cpu', running: false, generation: 0, warning: null },
    chat_overlay_locked: true, chat_languages: ['auto', 'ru', 'en', 'fi', 'de', 'fr', 'es', 'uk', 'pl', 'tr', 'zh', 'ja', 'pt', 'ar', 'sv'],
    models: [
      { id: 'base', bytes: 147951465, installed: false }, { id: 'small', bytes: 487601967, installed: false },
      { id: 'medium', bytes: 1533763059, installed: false }, { id: 'large-v3', bytes: 3095033483, installed: false },
      { id: 'm2m100-418m', bytes: 496016000, installed: false }, { id: 'm2m100-1.2b', bytes: 1255003436, installed: false },
      { id: 'vad', bytes: 885098, installed: false },
    ],
    history: { revision: 1, voice: [
      { id: 1, generation: 0, text: 'Two players coming through mid.', language: 'ru', latency_ms: 1200, inference_ms: 600 },
      { id: 2, generation: 0, text: 'Watch your left. I’m covering B.', language: 'ru', latency_ms: 900, inference_ms: 500 },
    ], chat: [
      { id: 1, generation: 0, channel: 'CT', player: 'Player 1', original: 'Идём на B.', text: "Let's go to B.", language: 'ru', translated: true, latency_ms: 800, inference_ms: 600 },
      { id: 2, generation: 0, channel: 'T', player: 'Player 2', original: 'Good luck, have fun!', text: 'Good luck, have fun!', language: 'en', translated: false, latency_ms: 0, inference_ms: 0 },
    ] },
    download: null, overlay_locked: true, supported: false,
    languages: ['auto', 'ru', 'en', 'fi', 'de', 'fr', 'es', 'uk', 'pl', 'tr', 'zh', 'ja', 'pt', 'ar', 'sv'],
  } satisfies Snapshot as T;
  throw new Error('Open the desktop app to use this control. This is a UI preview.');
}
export async function on<T>(name: string, callback: (data: T) => void): Promise<() => void> {
  if (!desktop) return () => {};
  return listen<T>(name, event => callback(event.payload));
}
