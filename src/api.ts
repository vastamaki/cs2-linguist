import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Snapshot } from './types';

export const desktop = isTauri();
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (desktop) return invoke<T>(command, args);
  if (command === 'snapshot') return {
    settings: { backend: 'cpu', model: 'small', language: 'auto', threads: 4, font_size: 24, opacity: 0.45, position: null },
    status: { phase: 'paused', message: 'Ready when you are', backend: 'cpu', running: false, generation: 0, warning: null },
    models: [{ id: 'small', bytes: 487601967, installed: false }, { id: 'base', bytes: 147951465, installed: false }, { id: 'vad', bytes: 885098, installed: false }],
    download: null, overlay_locked: true, supported: false,
    languages: ['auto', 'ru', 'en', 'fi', 'de', 'fr', 'es', 'uk', 'pl', 'tr', 'zh', 'ja', 'pt', 'ar', 'sv'],
  } satisfies Snapshot as T;
  throw new Error('Open the desktop app to use this control. This is a UI preview.');
}
export async function on<T>(name: string, callback: (data: T) => void): Promise<() => void> {
  if (!desktop) return () => {};
  return listen<T>(name, event => callback(event.payload));
}
