import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { defaultChatSettings, type Snapshot } from './types';

export const desktop = isTauri();
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (desktop) return invoke<T>(command, args);
  if (command === 'snapshot') return {
    settings: { voice_enabled: true, chat: { ...defaultChatSettings }, backend: 'cpu', model: 'small', language: 'auto', threads: 4, font_size: 24, opacity: 0.45, position: null },
    status: { phase: 'paused', message: 'Ready when you are', backend: 'cpu', running: false, generation: 0, warning: null },
    chat_status: { phase: 'paused', message: 'Chat paused', backend: 'cpu', running: false, generation: 0, warning: null },
    chat_overlay_locked: true, chat_languages: ['auto', 'ru', 'en', 'fi', 'de', 'fr', 'es', 'uk', 'pl', 'tr', 'zh', 'ja', 'pt', 'ar', 'sv'],
    models: [
      { id: 'base', bytes: 147951465, installed: false }, { id: 'small', bytes: 487601967, installed: false },
      { id: 'medium', bytes: 1533763059, installed: false }, { id: 'large-v3', bytes: 3095033483, installed: false },
      { id: 'm2m100-418m', bytes: 496016000, installed: false }, { id: 'm2m100-1.2b', bytes: 1255003436, installed: false },
      { id: 'vad', bytes: 885098, installed: false },
    ],
    download: null, overlay_locked: true, supported: false,
    languages: ['auto', 'ru', 'en', 'fi', 'de', 'fr', 'es', 'uk', 'pl', 'tr', 'zh', 'ja', 'pt', 'ar', 'sv'],
  } satisfies Snapshot as T;
  throw new Error('Open the desktop app to use this control. This is a UI preview.');
}
export async function on<T>(name: string, callback: (data: T) => void): Promise<() => void> {
  if (!desktop) return () => {};
  return listen<T>(name, event => callback(event.payload));
}
