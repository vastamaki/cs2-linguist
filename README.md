# Linguist

Local English subtitles for CS2 voice chat. Built with Bun, Vite, Tauri 2,
Rust, WASAPI application loopback, Silero VAD, and whisper.cpp.

**Target: Windows 11 x64, CS2 in borderless-windowed mode.** The current source
implements the capture pipeline, local models, CPU/Vulkan workers, tray controls,
and transparent movable overlay. Windows gameplay/GPU/installer acceptance is
still required; no Windows installer has been produced on the macOS development host.

## Use

1. Start Linguist and download **Whisper small** (about 465 MiB) or **base**
   (about 141 MiB). The download also includes the 865 KiB Silero speech detector.
2. Leave the source language on automatic, or choose Russian for short callouts.
3. Choose CPU or GPU, apply changes, and click **Start translation**. The app waits
   for `cs2.exe` and reconnects when the game restarts.
4. Use **Move overlay** to drag/resize the caption window, then **Lock overlay**.
   The locked overlay passes mouse input to the game without taking focus.
5. Closing Settings leaves Linguist in the tray. Use **Quit Linguist** to exit.

The tray also controls Start/Pause, CPU/GPU mode, visibility, position, and Settings.
The Settings status shows the **actual** backend, including the reason for a GPU
fallback. CPU is the initial default. Each backend/model/language/thread change
restarts the worker and discards pending captions.

Model import accepts the exact upstream GGML files listed in
[`crates/core/src/models.rs`](crates/core/src/models.rs), verified by SHA-256.
For an offline setup, import both the speech model and the speech detector using
their separate buttons. `.en`, `turbo`, quantized, and arbitrary other models are
not accepted by this version.

## Develop on Windows

Install these build prerequisites:

- [Bun](https://bun.sh/), and stable [Rust](https://rustup.rs/) with the MSVC target.
- Visual Studio Build Tools with **Desktop development with C++** and Windows SDK.
- CMake and LLVM/libclang. Set `LIBCLANG_PATH` to the directory containing
  `libclang.dll` if it is not discovered automatically.
- Microsoft Edge WebView2 (normally already installed on Windows 11).
- For GPU builds: [Vulkan SDK](https://vulkan.lunarg.com/) with `VULKAN_SDK` set.

Run in a Developer PowerShell or terminal with the MSVC toolchain available:

```powershell
bun install --frozen-lockfile
bun run desktop
```

This builds the CPU worker and starts the native app with Vite. To also test GPU
mode during development:

```powershell
bun run scripts/build-workers.ts all
bun run tauri dev
```

The worker build script produces separate CPU and Vulkan executables from the
same source and uses the static MSVC C runtime. The CPU executable does not link
Vulkan. End users need a compatible graphics driver for GPU mode, not the Vulkan
SDK, Bun, Rust, Python, or an API key.

## Build the Windows installer

```powershell
bun run package:windows
```

This builds both workers, builds the frontend, and bundles them into an NSIS
installer under `target/x86_64-pc-windows-msvc/release/bundle/nsis/`.
The installer includes neither speech models nor a GPU driver. Tauri's standard
WebView2 setup may require a network connection if WebView2 is absent.

The [Windows workflow](.github/workflows/windows.yml) runs checks and builds the
installer artifact on `windows-2022`. It has not been dispatched from this local
workspace. Distribution signing and automatic updates are not configured.

## Preview on macOS / in a browser

```sh
bun install --frozen-lockfile
bun run dev
```

Open `http://127.0.0.1:1420`. Browser preview has sample captions and working
appearance controls; capture, model setup, and native window actions are visibly
disabled. It makes no model-download or translation requests.

`bun run tauri dev` can preview the native settings/tray on macOS, but live capture
is intentionally unavailable. A CPU worker can run WAV benchmarks on macOS with
CMake and Xcode Command Line Tools installed.

## Checks and reproducible benchmarks

```sh
bun run build
bun test
cargo test -p linguist-core --locked
cargo fmt --all --check
cargo clippy -p linguist-core -p linguist --locked -- -D warnings
```

For actual inference, prepare a **16 kHz mono signed 16-bit PCM WAV**, obtain the
supported base/small and VAD model files, and run:

```sh
cargo build --release -p linguist-worker --locked
bun scripts/benchmark.ts target/release/linguist-worker base /path/ggml-base.bin /path/ggml-silero-v6.2.0.bin /path/speech.wav cpu ru
```

Use `target/release/linguist-worker.exe` on Windows. Use `auto` for automatic
language detection. Build with `--features gpu` and pass `gpu` for a Vulkan run.
The benchmark replays the WAV at real-time speed, prints caption/status JSON,
and reports median/95th-percentile speech-end-to-caption latency. It leaves stdin
open until queued results have drained. Its explicit console output includes the
fixture transcript; the normal app never writes transcripts or audio to disk.

See [`docs/VALIDATION.md`](docs/VALIDATION.md) for observed results and the Windows
acceptance checklist, including frame-time measurement.

## Implementation

```text
CS2 process tree
  → WASAPI shared-mode capture (Windows converts to 16 kHz mono float)
  → Silero VAD (32 ms decisions, 128 ms batches with 512 ms lookback)
  → utterance segmentation (200 ms pre-roll, ~400 ms silence, ≤5 s chunks)
  → bounded queue (two utterances; oldest replaced on overflow)
  → multilingual Whisper translate-to-English
  → Tauri caption event → transparent, click-through overlay
```

The worker keeps Whisper loaded while waiting for CS2. Capture/VAD and inference
run on separate threads. Results older than ten seconds are discarded both before
and after inference. Parent session IDs invalidate stopped/replaced workers;
capture stream IDs invalidate audio when the game or device disconnects. Caption
events are deduplicated by ID, not text, so repeated callouts remain meaningful.
Captions expire after eight seconds and at most three are retained.

Tauri controls workers through private stdin/stdout pipes. Closing the control
pipe stops the worker, including during inference. There is no HTTP server,
cloud inference, game injection, microphone capture, or whole-system fallback.
The model downloader is the only runtime network client. It uses pinned HTTPS
URLs, size limits and SHA-256 verification, and atomically activates verified files.

Settings and models are under Tauri's local app-data directory, typically
`%LOCALAPPDATA%\dev.linguist.cs2\` on Windows. Audio and caption history are in
memory only. Rendered captions always use DOM `textContent`.

`vendor/wasapi` is a minimal source patch of wasapi 0.24.0 correcting its handling
of silent audio buffers; see [`PATCH.md`](vendor/wasapi/PATCH.md). Dependency and
model versions are pinned by the committed lockfiles and model catalog.

## Current limits

- Process loopback includes gunfire, music, and character dialogue. It cannot
  isolate teammates or recover separately overlapping speakers.
- VAD reduces silence/noise inference; it cannot guarantee hallucination-free
  subtitles. Slang, clipped callouts, and language auto-detection need game testing.
- Continuous speech is split at five seconds; a word at that boundary may be
  clipped. Timestamp-aware overlap is deliberately deferred until recordings
  demonstrate the need.
- GPU inference shares graphics resources with CS2. Measure frame times; use CPU
  or the smaller model if gameplay suffers. No performance guarantee is made.
- Only the default compatible Vulkan GPU is selected. No device picker in v1.
- Models are intended for speech-to-English, not arbitrary target languages.
- Windows 10, other games, Linux, exclusive fullscreen, speech synthesis, speaker
  attribution, and transcript storage are outside this version.
