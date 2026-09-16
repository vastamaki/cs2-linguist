# Validation record

## Verified locally (macOS, 2026-09-16)

- TypeScript check, Vite production build, and Bun caption lifecycle tests passed.
- Autosave queue tests passed for rapid edits, ordered persistence, and recovery
  after a failed write. Browser inspection confirmed automatic preview updates,
  backend-dependent controls, and removal of Apply and overlay visibility controls.
- Overlay header checks passed for detected versus fixed language, actual backend
  after fallback, and clearing expired/stopped/previous-session readings. Production
  frontend build and all three Bun tests passed. Browser inspection verified the
  thin border, shared settings preview, live preview changes, and empty readings
  on the paused overlay page. Native resizing/DPI behavior still needs Windows QA.
- Rust core tests passed: silence/pre-roll/continuous segmentation, bounded queues,
  stale audio, settings validation, stopped/replaced worker results, corrupt and
  truncated model checksums.
- `cargo check` and `cargo build` succeeded for the native Tauri app and CPU worker.
- A Windows-targeted clang-cl/CMake object-only check reproduced the mixed CRT
  configuration: legacy flags selected the DLL runtime despite `-MT`; setting
  `CMP0091=NEW` and `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded` compiled both C and
  C++ with the static runtime. This checks compiler flags, not Windows linking.
- Native macOS debug app bundle was produced as a UI test artifact, not a supported
  game-capture release. Automated native interaction was blocked by desktop-tool
  app access approval; tray/overlay interactions are not marked verified.
- Windows worker Rust type-check passed with the `gpu` feature. On this Mac,
  `DOCS_RS=1` skipped C++ linking and bindgen used Windows-target headers with an
  opaque `FILE` declaration. This validates Rust/Windows API use, **not** the
  MSVC/Vulkan binary or Windows runtime behavior. The actual Windows build must
  use real SDK headers and must not set `DOCS_RS`.
- Browser UI inspection verified model selection, Russian source-language
  selection, subtitle sizing, clear preview labeling, and disabled native actions.
- Recognition update: frontend build, eight Bun/Rust tests, CPU worker release
  build, and core/app/worker Clippy checks passed. Medium/large-v3 byte lengths
  and SHA-256 values were checked against upstream Git LFS pointers at the pinned
  revision. Browser inspection verified both choices, sizes, autosave, and hints
  that change with CPU/GPU and Auto/fixed language. Their multi-GB weights were
  not downloaded or executed on this Mac; GPU/gameplay testing remains required.

Real model benchmarks used pinned **multilingual base**, Silero v6.2.0, CPU mode,
four inference threads, five-candidate beam search, Whisper's built-in confidence
filter, and real-time WAV replay after the recognition update:

| Fixture | Captions | Speech-end delay p50 / p95 | Inference p50 |
| --- | ---: | ---: | ---: |
| Upstream whisper.cpp `samples/jfk.wav`, fixed English | 4 | 821 / 869 ms | 354 ms |
| macOS Milena synthetic Russian callouts, fixed Russian | 1 | 958 / 958 ms | 403 ms |
| Same Russian fixture, automatic language detection | 1 | 1279 / 1279 ms | 728 ms |
| Three seconds of digital silence, automatic language | 0 | — | — |

Russian input: “Два игрока идут через центр. Посмотри налево. Я прикрываю тебя.”

Observed output: “Two players go to the center, look at the left, I cover you.”

These are small clean-audio smoke tests on Apple Silicon, not accuracy estimates
or Windows/CS2 performance measurements. Model weights and fixture audio were
downloaded/generated into a temporary directory and are not part of this repo.
The pre-change greedy worker produced the same Russian translation with Auto
at 1299 ms total / 753 ms inference in a fresh run. These single runs do not
establish a speed or accuracy improvement from beam search; use game recordings
to assess the decoding and larger models together.

## Required Windows acceptance

Build with `bun run package:windows`, then install the generated NSIS package on
a Windows 11 x64 machine. Record CPU, GPU/driver, RAM, display scale, model,
threads, game graphics settings, and the installed app version.

1. **Clean installation:** start on a PC with no Bun/Rust/Python/Vulkan SDK.
   Confirm CPU mode works without a Vulkan driver. Download base, small, medium,
   and large-v3 (the last two require more disk space and memory);
   verify cancellation, retry, corrupted imports, local import, and offline restart.
   A CPU worker must not import `vulkan-1.dll` (`dumpbin /dependents`).
2. **Source isolation:** start before CS2; expect Waiting for CS2. Play browser or
   Discord audio separately; it must not generate captions. Launch CS2 and confirm
   its audio produces captions. Restart CS2 and change headsets/default output.
   Confirm reconnection without old captions.
3. **Speech:** test Russian, another supported language, English, automatic
   detection, short repeated callouts, silence, gunfire only, overlapping speakers,
   and continuous speech longer than five seconds. Save only opt-in test fixtures.
   Assess meaning with a fluent speaker; do not use exact-string assertions for
   translations.
   Compare small/medium/large-v3 on the same recording with fixed Russian and
   Auto. Include quiet but intelligible speech to check for missing captions.
4. **Worker lifecycle:** switch CPU/GPU/model/language while speaking; pause during
   inference, immediately restart, and quit. No old-session caption may appear.
   Settings must save without an Apply button, survive reopening the app, and
   preserve the latest value after rapid edits. Slider drags preview immediately
   and persist on release. Selecting an unavailable model pauses translation and
   reports the required setup instead of continuing with the previous model.
   Unavailable GPU, missing GPU DLL, and GPU initialization failure must lead to
   a clearly identified CPU fallback. Missing/corrupt models must show an error.
5. **Overlay:** use borderless CS2, click and aim through captions, verify focus
   stays in the game, unlock/drag/resize/relock, start/pause translation, close Settings,
   reopen from the tray, restart the app, and remove a monitor. Test 100%, 150%,
   and 200% scaling and negative monitor coordinates. Start must show a locked
   overlay; Pause must hide it, including while moving it. Moving while paused
   shows a preview that disappears on Lock. App launch must leave it hidden.
   Verify the thin frame and header at small overlay sizes: detected language
   (Auto only), selected language, last-caption delay, decode time, model, and
   actual CPU/GPU backend. Readings must clear after expiry, stop, restart, and
   game/device reconnection; CPU fallback must say CPU even when GPU is selected.
6. **Privacy:** after models are installed, disconnect networking and verify
   translation. Inspect app-data files: settings/models only, no audio/transcripts.
   Caption text containing `<script>` must display literally.
7. **Performance:** replay the same CS2 demo or controlled sequence three times
   each with translation off, CPU, and GPU, using identical game settings.
   Record average and 1% low FPS/p95 frame time, app working set, CPU/GPU use,
   caption speech-end latency, and any Falling behind events. Use Task Manager
   for memory/utilization and a frame-time tool such as PresentMon for gameplay.
   The WAV benchmark provides reproducible inference/latency measurements but
   does not measure live WASAPI acquisition delay or CS2 FPS.
   Repeat for medium and large-v3 on GPU; verify model loading with CS2 already
   running, memory pressure, and explicit CPU fallback if GPU initialization fails.

Do not publish performance claims or mark Windows acceptance complete from the
macOS fixture results. Overlap/noise quality, GPU fallback, and borderless overlay
behavior are the release gates still requiring Windows hardware.
