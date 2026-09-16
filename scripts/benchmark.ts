import { resolve } from 'node:path';

// Usage: bun scripts/benchmark.ts <worker> <base|small> <model.bin> <vad.bin> <audio.wav> [cpu|gpu] [language]
const [worker, model, modelPath, vadPath, wav, backend = 'cpu', language = 'auto'] = process.argv.slice(2);
if (!worker || !['base', 'small'].includes(model) || !modelPath || !vadPath || !wav) {
  console.error('Usage: bun scripts/benchmark.ts <worker> <base|small> <model.bin> <vad.bin> <16k-mono.wav> [cpu|gpu] [language]');
  process.exit(1);
}
const child = Bun.spawn([resolve(worker), '--wav', resolve(wav)], { stdin: 'pipe', stdout: 'pipe', stderr: 'inherit' });
child.stdin.write(JSON.stringify({ type: 'start', settings: { model, backend, language, threads: 4 }, model_path: resolve(modelPath), vad_path: resolve(vadPath) }) + '\n');
child.stdin.flush();
const latencies: number[] = [];
const inference: number[] = [];
let drain: ReturnType<typeof setTimeout> | undefined;
const reader = child.stdout.getReader();
const decoder = new TextDecoder();
let buffered = '';
while (true) {
  const { value, done } = await reader.read();
  if (done) break;
  buffered += decoder.decode(value, { stream: true });
  let newline: number;
  while ((newline = buffered.indexOf('\n')) >= 0) {
    const line = buffered.slice(0, newline); buffered = buffered.slice(newline + 1);
    const event = JSON.parse(line);
    console.log(JSON.stringify(event));
    if (event.type === 'caption') { latencies.push(event.latency_ms); inference.push(event.inference_ms); }
    if (event.type === 'error') { child.kill(); process.exitCode = 1; }
    if (event.phase === 'draining') drain = setTimeout(() => child.stdin.end(), 12_000);
  }
}
clearTimeout(drain);
const code = await child.exited;
const percentile = (values: number[], p: number) => [...values].sort((a, b) => a - b)[Math.max(0, Math.ceil(values.length * p) - 1)] ?? null;
console.log(JSON.stringify({ captions: latencies.length, latency_p50_ms: percentile(latencies, .5), latency_p95_ms: percentile(latencies, .95), inference_p50_ms: percentile(inference, .5) }));
if (code) process.exitCode = code;
