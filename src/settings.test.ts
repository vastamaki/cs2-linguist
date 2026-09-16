import { defaultChatSettings } from './types';
import { expect, test } from 'bun:test';
import { SettingsAutosave } from './settings';
import type { Settings } from './types';

test('rapid edits wait for earlier saves and a failed save does not block the next edit', async () => {
  const settings: Settings = { voice_enabled: true, chat: { ...defaultChatSettings }, backend: 'cpu', model: 'small', language: 'auto', threads: 4, font_size: 24, opacity: 0.45, position: null };
  const first = Promise.withResolvers<void>();
  const entered = Promise.withResolvers<void>();
  const calls: Settings[] = [];
  let stored: Settings | undefined;
  const saver = new SettingsAutosave(async value => {
    calls.push(value);
    if (calls.length === 1) { entered.resolve(); await first.promise; }
    if (value.language === 'ru') throw new Error('Disk full');
    stored = value;
  });
  const a = saver.save(settings);
  const b = saver.save({ ...settings, font_size: 30 });
  await entered.promise;
  expect(calls).toHaveLength(1);
  expect(saver.pending).toBe(2);
  first.resolve();
  await Promise.all([a, b]);
  expect(stored?.font_size).toBe(30);
  expect(saver.pending).toBe(0);

  const failed = saver.save({ ...settings, language: 'ru' });
  const rejected = expect(failed).rejects.toThrow('Disk full');
  const recovered = saver.save({ ...settings, backend: 'gpu' });
  await rejected;
  await recovered;
  expect(stored?.backend).toBe('gpu');
  expect(saver.pending).toBe(0);
});
