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
// Cargo resolves CARGO_TARGET_DIR and .cargo/config.toml for us.
const metadata = Bun.spawnSync(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], { cwd: root, stderr: 'inherit' });
if (metadata.exitCode) throw new Error('Could not resolve the Cargo build directory.');
const targetDirectory: string = JSON.parse(metadata.stdout.toString()).target_directory;
await mkdir(join(root, 'src-tauri', 'binaries'), { recursive: true });
for (const mode of all ? ['cpu', 'gpu', 'chat'] : ['cpu', 'chat']) {
  const packageName = mode === 'chat' ? 'linguist-chat-worker' : 'linguist-worker';
  const outputName = mode === 'chat' ? 'linguist-chat-worker' : `linguist-worker-${mode}`;
  const args = ['cargo', 'build', '--locked', '--release', '-p', packageName, '--target', triple];
  if (mode === 'gpu') args.push('--features', 'gpu');
  const child = Bun.spawn(args, { cwd: root, stdout: 'inherit', stderr: 'inherit', env: {
    ...process.env,
    // Avoid building a binary that only runs on the build machine's CPU.
    GGML_NATIVE: 'OFF', GGML_AVX512: 'OFF',
    ...(windows ? {
      RUSTFLAGS: [process.env.RUSTFLAGS, '-C target-feature=+crt-static'].filter(Boolean).join(' '),
      // Match Rust's static CRT in Whisper/ggml. With Ninja, CMake's legacy
      // Release flags otherwise append /MD after cc-rs's /MT.
      CMAKE_POLICY_DEFAULT_CMP0091: 'NEW',
      CMAKE_MSVC_RUNTIME_LIBRARY: 'MultiThreaded',
    } : {}),
  } });
  if (await child.exited) throw new Error(`${mode} worker build failed.`);
  const extension = windows ? '.exe' : '';
  await copyFile(join(targetDirectory, triple, 'release', `${packageName}${extension}`), join(root, 'src-tauri', 'binaries', `${outputName}-${triple}${extension}`));
}
