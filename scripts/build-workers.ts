import { mkdir, copyFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const windows = process.platform === 'win32';
const version = Bun.spawnSync(['rustc', '-vV']);
if (version.exitCode) throw new Error('Install Rust before building the native worker.');
const host = version.stdout.toString().match(/^host: (.+)$/m)?.[1];
const triple = windows ? 'x86_64-pc-windows-msvc' : host!;
const all = process.argv[2] === 'all';
if (all && !windows) throw new Error('The CPU + Vulkan installer must be built on Windows x64. Use the cpu argument for local benchmarks.');
await mkdir(join(root, 'src-tauri', 'binaries'), { recursive: true });
for (const mode of all ? ['cpu', 'gpu'] : ['cpu']) {
  const args = ['cargo', 'build', '--locked', '--release', '-p', 'linguist-worker', '--target', triple];
  if (mode === 'gpu') args.push('--features', 'gpu');
  const child = Bun.spawn(args, { cwd: root, stdout: 'inherit', stderr: 'inherit', env: {
    ...process.env,
    // Avoid building a binary that only runs on the build machine's CPU.
    GGML_NATIVE: 'OFF', GGML_AVX512: 'OFF',
    ...(windows ? { RUSTFLAGS: [process.env.RUSTFLAGS, '-C target-feature=+crt-static'].filter(Boolean).join(' ') } : {}),
  } });
  if (await child.exited) throw new Error(`${mode} worker build failed.`);
  const extension = windows ? '.exe' : '';
  await copyFile(join(root, 'target', triple, 'release', `linguist-worker${extension}`), join(root, 'src-tauri', 'binaries', `linguist-worker-${mode}-${triple}${extension}`));
}
