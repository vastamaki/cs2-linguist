import { expect, test } from 'bun:test';
import { appendCaption } from './captions';
import type { Caption, EngineStatus } from './types';
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
