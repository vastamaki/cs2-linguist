import { defaultChatSettings } from './types';
import { expect, test } from 'bun:test';
import { appendCaption, overlayStats } from './captions';
import type { Caption, ChatCaption, EngineStatus, Settings } from './types';
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
  const settings: Settings = { voice_enabled: true, chat: { ...defaultChatSettings }, backend: 'gpu', model: 'medium', language: 'auto', threads: 4, font_size: 24, opacity: .45, position: null };
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

test('chat preserves player and original text, bounds history, and rejects stopped or replaced sessions', () => {
  const chat: ChatCaption = { ...caption, channel: 'CT', player: '<img src=x>', original: 'Идём на B', translated: true };
  let list = appendCaption([], chat, status, 0, 6, 20000);
  for (let id = 2; id <= 7; id++) list = appendCaption(list, { ...chat, id }, status, id, 6, 20000);
  expect(list.map(c => c.id)).toEqual([2, 3, 4, 5, 6, 7]);
  expect(list[0]).toMatchObject({ player: chat.player, original: chat.original });
  expect(appendCaption([], chat, { ...status, generation: 3 }, 10, 6, 20000)).toHaveLength(0);
  expect(appendCaption([], chat, { ...status, running: false }, 10, 6, 20000)).toHaveLength(0);
  expect(appendCaption(list, { ...chat, id: 8 }, status, 20008, 6, 20000).map(c => c.id)).toEqual([8]);
});
