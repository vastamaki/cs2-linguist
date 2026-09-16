import { defaultChatSettings } from './types';
import { expect, test } from 'bun:test';
import { appendCaption, chatText, latestHistory, overlayStats } from './captions';
import type { Caption, ChatCaption, EngineStatus, Settings, TranslationHistory } from './types';
const status: EngineStatus = { phase: 'listening', message: '', backend: 'cpu', running: true, generation: 2, warning: null };
const caption: Caption = { id: 1, generation: 2, text: '<script>go B</script>', language: 'ru', latency_ms: 800, inference_ms: 300 };
test('reject stale sessions and duplicate events, preserve repeated callouts, cap and expire captions', () => {
  let list = appendCaption([], caption, status, 0);
  expect(appendCaption(list, caption, status, 1)).toHaveLength(1);
  expect(appendCaption(list, { ...caption, generation: 1 }, status, 1)).toBe(list);
  expect(appendCaption([], caption, { ...status, running: false }, 1)).toHaveLength(0);
  for (let id = 2; id <= 4; id++) list = appendCaption(list, { ...caption, id }, status, id);
  expect(list.map(c => c.id)).toEqual([2, 3, 4]);
  list = appendCaption(list, { ...caption, id: 5 }, status, 9000);
  expect(list.map(c => c.id)).toEqual([5]);
});

test('overlay stats use current captions, actual backend, and distinguish fixed from detected language', () => {
  const settings: Settings = { voice_enabled: true, overlays_enabled: true, chat: { ...defaultChatSettings }, backend: 'gpu', model: 'medium', language: 'auto', threads: 4, font_size: 24, opacity: .45, position: null };
  const visible = { ...caption, expires: 8000 };
  expect(overlayStats(settings, status, visible, 1)).toEqual({
    detected: 'Russian', spoken: 'Auto', delay: '0.8 s', decode: '0.3 s', backend: 'CPU', model: 'medium',
  });
  expect(overlayStats({ ...settings, language: 'ru' }, status, visible, 1)).toMatchObject({ detected: 'Off', spoken: 'Russian' });
  for (const [engine, entry, now] of [
    [{ ...status, running: false }, visible, 1],
    [{ ...status, generation: 3 }, visible, 1],
    [status, visible, 8000],
    [status, undefined, 1],
  ] as const) {
    expect(overlayStats(settings, engine, entry, now)).toMatchObject({ detected: '—', delay: '—', decode: '—' });
  }
});

test('chat formats one row without originals and older snapshots cannot roll history back', () => {
  const chat: ChatCaption = { ...caption, channel: 'CT', player: '<img src=x>', original: 'Идём на B', translated: true };
  expect(chatText(chat)).toBe('[CT] <img src=x>: <script>go B</script>');
  expect(chatText({ ...chat, player: 'Player 2', channel: 'T', text: 'Good luck!', original: 'Good luck!', translated: false, language: 'en' })).toBe('[T] Player 2: Good luck!');
  const current: TranslationHistory = { revision: 2, voice: [caption], chat: [chat] };
  expect(latestHistory(current, { revision: 1, voice: [], chat: [] })).toBe(current);
  const newer: TranslationHistory = { ...current, revision: 3, voice: [...current.voice, { ...caption, id: 2 }] };
  expect(latestHistory(current, newer)).toBe(newer);
  expect(newer.chat).toEqual([chat]);
});
