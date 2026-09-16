// Opt-in integration check with real model weights and a temporary, synthetic CS2 log.
import { strict as assert } from 'node:assert';
import { appendFile, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const [worker, model, folder] = process.argv.slice(2);
if (!worker || !model || !folder) throw new Error('Usage: bun scripts/check-chat.ts <worker> <model-id> <model-folder>');
const temp = await mkdtemp(join(tmpdir(), 'linguist-chat-'));
const log = join(temp, 'console.log');
await writeFile(log, '[ALL] Old: old history must not appear\n');
const child = Bun.spawn([resolve(worker)], { stdin: 'pipe', stdout: 'pipe', stderr: 'inherit' });
type Event = { type: string; phase?: string; message?: string; caption?: { id: number; player: string; original: string; text: string; translated: boolean; latency_ms: number } };
const events: Event[] = [];
let failure: Error | undefined;
const output = (async () => {
  let pending = '';
  const decoder = new TextDecoder();
  const reader = child.stdout.getReader();
  for (;;) {
    const { value: chunk, done } = await reader.read();
    if (done) break;
    pending += decoder.decode(chunk, { stream: true });
    let end: number;
    while ((end = pending.indexOf('\n')) >= 0) {
      const event = JSON.parse(pending.slice(0, end)) as Event;
      pending = pending.slice(end + 1); events.push(event);
      if (event.type === 'error') throw new Error(event.message);
    }
  }
})().catch(error => { failure = error; });
async function waitFor(predicate: () => boolean, description: string) {
  const deadline = Date.now() + 60000;
  while (!predicate()) {
    if (failure) throw failure;
    if (Date.now() > deadline) throw new Error(`Timed out: ${description}`);
    if (child.exitCode !== null) throw new Error(`Worker exited: ${child.exitCode}`);
    await Bun.sleep(50);
  }
}
const captions = () => events.filter(e => e.type === 'chat').map(e => e.caption!);
try {
  child.stdin.write(JSON.stringify({ type: 'start', settings: { enabled: true, model, language: 'auto', log_path: log, threads: 2 }, model_path: resolve(folder) }) + '\n');
  await child.stdin.flush();
  await waitFor(() => events.some(e => e.phase === 'listening'), 'model ready');
  assert.equal(captions().length, 0);
  const line = Buffer.from('[CT] Игрок: Привет, давайте пойдём вместе на точку Б.\n');
  const split = line.indexOf(Buffer.from('Привет')) + 1;
  await appendFile(log, line.subarray(0, split));
  await Bun.sleep(350);
  assert.equal(captions().length, 0, 'incomplete UTF-8 line must wait');
  await appendFile(log, Buffer.concat([line.subarray(split), line]));
  await waitFor(() => captions().length === 2, 'repeated Russian messages');
  assert.equal(captions()[0].player, 'Игрок');
  assert.equal(captions()[0].original, captions()[1].original);
  assert.notEqual(captions()[0].id, captions()[1].id);
  assert.ok(captions().every(c => c.translated && c.text !== c.original));
  await writeFile(log, '[ALL] New: Please come with me to the bomb site.\n');
  await waitFor(() => captions().length === 3, 'log truncation');
  assert.ok(events.some(e => e.type === 'chat_reset'));
  assert.equal(captions()[2].text, 'Please come with me to the bomb site.');
  assert.equal(captions()[2].translated, false);
  await rm(log);
  await waitFor(() => events.some(e => e.phase === 'waiting'), 'missing log');
  await writeFile(log, '[TEAM] Back: Please come with me to the bomb site.\n');
  await waitFor(() => captions().length === 4, 'recreated log');
  const resets = events.filter(e => e.type === 'chat_reset').length;
  await appendFile(log, line);
  await Bun.sleep(200);
  await writeFile(log, '[ALL] Reset: Please come with me to the bomb site.\n');
  await waitFor(() => events.filter(e => e.type === 'chat_reset').length > resets && captions().at(-1)?.player === 'Reset', 'reset during inference');
  const resetIndex = events.map(e => e.type).lastIndexOf('chat_reset');
  assert.ok(events.slice(resetIndex).filter(e => e.type === 'chat').every(e => e.caption!.player === 'Reset'), 'old log results cannot follow reset');
  child.stdin.write('{"type":"stop"}\n');
  await child.stdin.flush();
  await waitFor(() => child.exitCode !== null, 'stop');
  assert.equal(await child.exited, 0);
  await output;
  if (failure) throw failure;
  console.log(JSON.stringify({ model, messages: captions().length, delay_ms: captions().map(c => c.latency_ms), checks: 'history skipped; partial UTF-8; repeats; rotation; English passthrough; missing/recreated log; reset during inference; stop' }));
} finally {
  if (child.exitCode === null) child.kill();
  await child.exited;
  await rm(temp, { recursive: true, force: true });
}
