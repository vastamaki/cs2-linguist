# Linguist

Local English overlays for CS2 voice and text chat. Built with Bun, Vite, Tauri 2,
Rust, WASAPI application loopback, Silero VAD, whisper.cpp, and CTranslate2.

**Target: Windows 11 x64, CS2 in borderless-windowed mode.** The current source
implements the capture pipeline, local models, CPU/Vulkan workers, tray controls,
and two independent transparent movable overlays plus a split in-app view. Windows gameplay/GPU/installer acceptance is
still required; no Windows installer has been produced on the macOS development host.

## Use

1. Start Linguist and download a model: **base** (148 MB, lightweight), **small**
   (488 MB, balanced default), **medium** (1.53 GB, higher accuracy), or
   **large-v3** (3.10 GB, highest capacity). Sizes are downloads, not GPU memory
   requirements. Each download also includes the 885 KB Silero speech detector.
2. Choose Russian when the voice chat is mostly Russian, especially for short
   callouts. Auto guesses separately for each phrase; use it for mixed languages.
3. Choose CPU or GPU and click **Start translation**. Settings save automatically;
   sliders preview immediately and save when released. Start shows the enabled overlays;
   **Pause translation** stops both workers and hides both overlays. The app waits for `cs2.exe`
   and reconnects when the game restarts.
4. Use **Move voice overlay** to drag/resize the caption window, then **Lock overlay**.
   The locked overlay passes mouse input to the game without taking focus.
   You can position the overlay while paused; locking closes that preview.
5. Closing Settings leaves Linguist in the tray. Use **Quit Linguist** to exit.

The tray also controls Start/Pause, CPU/GPU mode, position, and Settings.
The Settings status shows the **actual** backend, including the reason for a GPU
fallback. CPU is the initial default. Each backend/model/language/thread change
restarts the affected worker and discards its pending captions. Voice and chat
can be enabled separately; changing a chat model leaves voice translation running.

The overlay has a thin border and a compact header showing engine state, actual
CPU/GPU backend, model, selected spoken language, and the latest caption's detected
language, delay, and decoding time. Detection reads **Off** when a fixed source
language is selected. Timing/language readings clear when captions expire or the
engine restarts; delay measures speech end to translation ready. Settings shows
the same design with clearly labeled example captions and timing.

Model import accepts the exact upstream GGML files listed in
[`crates/core/src/models.rs`](crates/core/src/models.rs), verified by SHA-256.
For an offline setup, import both the speech model and the speech detector using
their separate buttons. `.en`, `turbo`, quantized, and arbitrary other models are
not accepted as speech models by this version.

For missed or incorrect words, try **GPU + medium + a fixed spoken language**
first. If your GPU has enough memory alongside CS2, try large-v3 next.
[Whisper recommends medium or large for better translation](https://github.com/openai/whisper#available-models-and-languages).
Larger models increase memory use and caption delay; if you see **Falling behind**
or game performance suffers, move down a model size. No model can reliably recover
voices buried in gunfire or multiple people talking over each other.

Decoding uses five-candidate beam search to compare possible phrases. Whisper's
built-in speech/confidence check filters silence; the app does not discard its
accepted segments using an additional speech-probability-only threshold.

## Translate text chat

1. Enable **Translate chat** in Settings. Voice translation is optional.
2. In Steam → CS2 → Properties → Launch Options, add **`-condebug`** alongside
   existing launch options, then restart CS2.
3. Click **Choose log** and select `game/csgo/console.log` in CS2's installation
   folder (Steam → CS2 → Manage → Browse local files).
4. Download one of the two **text translation** models:

   | Model | Download | Tradeoff |
   | --- | ---: | --- |
   | M2M100 418M int8 | 496 MB | Default; lower memory use and delay |
   | M2M100 1.2B int8 | 1.26 GB | More capacity; higher memory use and delay |

   Both translate from the M2M100 set of 100 source languages into English.
   These are text models, separate from Whisper; no OCR is involved. Text
   translation currently runs on CPU, independently of the voice CPU/GPU setting.
   Start with two chat CPU threads to leave resources for the game.
5. Choose **Written language** independently of **Spoken language**. Auto guesses
   each message; choose Russian for mostly Russian chat. Short messages and slang
   can be misidentified, including Russian being guessed as Bulgarian. Auto's
   language coverage is smaller than the model's; select other languages manually.
6. Click **Start translation**. Use **Move chat overlay** to place it, then lock it.
   Each message is one row: `[CT] Player 1: message`. No original-text duplicate
   is shown, and English messages are preserved. The header shows selected/last
   language, model, and read-to-caption delay. The latest **10 messages** remain
   until replaced: the eleventh removes the first. They do not expire on a timer.

**`-condebug` makes CS2 write console output, including raw player chat, to disk.**
Linguist reads the chosen file without modifying it; it does not save translations
or send messages to CS2. Remove the launch option when you no longer want CS2 to
log. Existing chat history is skipped on Start; only new complete lines are read.
Missing files are watched, and truncation/recreation clears pending translations.
Team/all-chat formats `[CT]`, `[T]`, `[ALL]`, and `[TEAM]` are recognized. Changes
in CS2's log format or localization need validation on the game's current build.
Messages detected as English pass through even when a fixed written language is
selected. Automatic language detection can still mistake short or mixed messages.

### Use the app without overlays

In **Settings → Display → Translation display**, select **In app only · no
overlays**. Both overlays hide immediately, including positioning previews;
translation continues. Open **Translations** to see text chat and voice in two
independently scrolling, equal-width panes. Start in this mode opens that page.
You can also open it while overlays are enabled.

Each pane retains its latest 10 messages in memory. Changing pages, pausing,
restarting a worker, changing models, or switching display modes preserves this
history. Closing Settings keeps the app and history in the tray; **Quit Linguist**
clears it. No transcript is written to disk or restored after quitting. Results
that were still pending when a worker stopped or a log reset remain discarded.
The display preference is saved automatically and takes effect without reloading
models. Switch back to **Overlays + in-app page** to use the overlays again.

**Import model folder** accepts the four exact files (`model.bin`, `config.json`,
`shared_vocabulary.json`, `sentencepiece.bpe.model`) from the selected pinned
conversion in [`crates/core/src/models.rs`](crates/core/src/models.rs). Downloads
and imports verify each file's SHA-256; all four are required and checked again
before the worker loads. Models remain on disk for offline reuse.

The original [Meta M2M100 models](https://huggingface.co/facebook/m2m100_418M)
are MIT licensed. We use community
[CTranslate2 int8 conversions](https://huggingface.co/Torurzr/screentranslator-mt/tree/3e496f278e70067ba5490c35bc1203d995e4df74)
at a pinned revision, with [CTranslate2](https://github.com/OpenNMT/CTranslate2)
and [SentencePiece](https://github.com/google/sentencepiece) for native inference.
A larger model is not a guarantee of better slang or game terminology translation.

## Develop on Windows

Install these build prerequisites:

- [Bun](https://bun.sh/), and stable [Rust](https://rustup.rs/) with the MSVC target.
- Visual Studio Build Tools with **Desktop development with C++** and Windows SDK.
- CMake, Ninja, and LLVM/libclang. Set `LIBCLANG_PATH` to the directory containing
  `libclang.dll` if it is not discovered automatically.
- Microsoft Edge WebView2 (normally already installed on Windows 11).
- For GPU builds: [Vulkan SDK](https://vulkan.lunarg.com/) with `VULKAN_SDK` set.

Run in **Developer PowerShell for VS 2022** with the x64 MSVC toolchain available.
Use a short Cargo output path: Whisper's nested Vulkan shader-generator build can
exceed Windows path limits even when the main C++ build succeeds.

```powershell
$env:CARGO_TARGET_DIR = 'C:/t'
$env:CMAKE_GENERATOR = 'Ninja'
$env:CC = 'cl'
$env:CXX = 'cl'
bun install --frozen-lockfile
bun run desktop
```

This builds the CPU voice and text workers and starts the native app with Vite. To also test GPU
mode during development:

```powershell
bun run scripts/build-workers.ts all
bun run tauri dev
```

The worker build script produces separate CPU and Vulkan voice executables from
the same source, plus a CPU text worker. All use the static MSVC C runtime.
Neither CPU executable links Vulkan. End users need a compatible graphics driver for GPU mode, not the Vulkan
SDK, Bun, Rust, Python, or an API key.

Both Rust and Whisper/ggml must use that same runtime. The script sets Rust's
`+crt-static` together with CMake's `CMP0091=NEW` and
`CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded` ([CMake runtime selection](https://cmake.org/cmake/help/latest/variable/CMAKE_MSVC_RUNTIME_LIBRARY.html)); Rust's flag alone does not override
Ninja's legacy CMake `/MD` flags. When updating an existing local build from the
old configuration, clear the cached Whisper objects once before rebuilding:

```powershell
cargo clean -p whisper-rs-sys --release --target x86_64-pc-windows-msvc
```

## Build the Windows installer

From the same configured Developer PowerShell:

```powershell
bun run package:windows
```

This builds all three workers, builds the frontend, and bundles them into an NSIS
installer under `$env:CARGO_TARGET_DIR/x86_64-pc-windows-msvc/release/bundle/nsis/`
(`C:/t/x86_64-pc-windows-msvc/release/bundle/nsis/` with the setup above).
The installer includes neither model weights nor a GPU driver. Tauri's standard
WebView2 setup may require a network connection if WebView2 is absent.

The [Windows workflow](.github/workflows/windows.yml) runs checks and builds the
installer artifact on `windows-2022`. It activates x64 MSVC, uses Ninja and the
short output path `D:/t`, verifies that workers have no MSVC runtime DLL dependency
and that CPU workers do not link Vulkan, and uploads `linguist-windows-x64`
on success. If a native build fails, `windows-cmake-diagnostics` contains the available CMake configure
logs, including the nested shader-generator compiler checks. This build fix still
needs a successful Windows run. Distribution signing and automatic updates are
not configured.

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
one of the supported speech model files and the VAD model, and run:

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

For real text inference on this Mac or a Windows build, use the downloaded model
folder and the native worker. No Python or running game is needed:

```sh
bun scripts/build-workers.ts cpu
bun scripts/check-chat.ts src-tauri/binaries/linguist-chat-worker-aarch64-apple-darwin m2m100-418m /path/to/model-folder
```

On Windows, substitute `src-tauri/binaries/linguist-chat-worker-x86_64-pc-windows-msvc.exe`.
Use `m2m100-1.2b` with its matching folder to check the larger model. The integration
check creates and removes a temporary synthetic console log and verifies partial
UTF-8, repeated messages, log recreation, reset during inference, and Stop. It
reports delays without printing captions. To inspect a translation explicitly:

```sh
/path/to/linguist-chat-worker --translate m2m100-418m /path/to/model-folder ru 'Два игрока идут через центр.'
```

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
Voice overlay captions expire after eight seconds and at most three are visible.
The app retains the last ten completed voice translations for its in-app page.

Text chat takes a separate path:

```text
CS2 -condebug → console.log (read-only, 150 ms polling)
  → bounded UTF-8 line parser → chat messages only
  → bounded queue (eight messages; stale after thirty seconds)
  → language detection / fixed language → M2M100 int8 → English
  → shared app history (10 messages) → chat overlay and in-app chat pane
```

The text worker retains the model in memory. Reading the log and inference run
on separate threads. Log resets invalidate in-flight work; Stop/restart invalidate
parent worker generations. Repeated messages from the same player remain separate.
Only the requested file is read, with at most 256 KiB per poll and 16 KiB per line.

Tauri controls workers through private stdin/stdout pipes. Closing the control
pipe stops the worker, including during inference. There is no HTTP server,
cloud inference, game injection, microphone capture, or whole-system fallback.
The model downloader is the only runtime network client. It uses pinned HTTPS
URLs, size limits and SHA-256 verification, and atomically activates verified files.

Settings and models are under Tauri's local app-data directory, typically
`%LOCALAPPDATA%\dev.linguist.cs2\` on Windows. Audio and caption history are in
memory only; CS2 itself writes the console log when `-condebug` is enabled.
Rendered captions and player names always use DOM `textContent`.

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
- Output is English for both voice and text. Text translation is CPU-only; voice
  translation additionally supports Vulkan. Chat slang, typos, mixed-language
  messages, and source-language guesses can produce incorrect translations.
- Windows 10, other games, Linux, exclusive fullscreen, speech synthesis, speaker
  attribution, and transcript storage are outside this version.
