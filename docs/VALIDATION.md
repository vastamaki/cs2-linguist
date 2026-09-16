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

## Text chat feature (macOS, 2026-09-16)

- Native CTranslate2/SentencePiece CPU worker compiled in release mode; both CPU
  voice and text workers built and copied by `scripts/build-workers.ts cpu`.
  Core/Tauri and release voice/text worker Clippy checks passed.
- Four Bun tests (23 assertions) and nine Rust core tests passed, including chat
  history limits, expiry, stale sessions, settings migration, independent restart
  decisions, UTF-8 split across file writes, repeated messages, log truncation,
  deletion/recreation, and filtering non-chat/oversized messages. The text worker's
  fixed-language/English/very-short-message detection check passed separately.
- Both pinned M2M100 int8 models were downloaded into temporary storage. All four
  files per model passed size/SHA-256 verification before real inference. No model
  weights are committed. Native download/import UI cancellation and Windows
  installation are not marked verified by these checks.
- The real worker's stdin/stdout integration check (`scripts/check-chat.ts`) passed
  with **both models**: old history skipped, partial UTF-8 waits for newline,
  identical repeated messages get distinct IDs, English passes through, missing
  files reconnect, and resetting a log during inference suppresses old results.
  Stop exits successfully. Log fixtures contain only synthetic test messages.
- Browser inspection verified conditional voice/chat controls, the in-app
  `-condebug` instructions and disk-log notice, both model choices/sizes, separate
  written-language selection and automatic preview updates. The original-text
  toggle was subsequently removed for the rolling-chat layout below.
  This does not validate the native overlay's focus/click-through behavior.

Individual CLI smoke calls used two CPU threads, four-beam decoding, and a loaded
model on Apple Silicon. These are **examples, not accuracy or speed guarantees**:

| Input / fixed source | 418M output (decode) | 1.2B output (decode) |
| --- | --- | --- |
| Два игрока идут через центр. Я прикрываю тебя. / Russian | “Two players go through the center.I’m hiding you.” (958 ms) | “Two players go through the center. I cover you.” (1634 ms) |
| Zwei Spieler kommen durch die Mitte. / German | “Two players come through the middle.” (532 ms) | Same text (1266 ms) |
| Kaksi pelaajaa tulee keskeltä. / Finnish | “Two players are in the middle.” (585 ms) | “There are two players in the middle.” (1332 ms) |

Automatic detection preserved the English fixture unchanged. For “Привет, давайте
пойдём вместе на точку Б.” it incorrectly guessed Bulgarian with both models;
both nevertheless produced “Let’s go to point B.” (772 / 1659 ms). The detector is
shared by both choices: a larger translation model does **not** fix language
identification. This is why the UI recommends a fixed Russian source when
appropriate. Original messages can be checked in CS2 itself. Finnish movement semantics were also
imperfect. No CS2 slang accuracy claim follows from these few clean sentences.

Two back-to-back Russian log messages accumulated extra queue delay. In one
integration run, read-to-caption times were 587 / 1084 ms for 418M and 1713 /
3121 ms for 1.2B. These omit up to 150 ms log-polling delay and exclude model load;
they do not measure Windows CPU use, memory, game frame times, or live CS2 logs.

## Rolling history and in-app view (macOS, 2026-09-16)

- Core tests verify independent ten-message limits for chat and voice, FIFO
  eviction on the eleventh message, repeated messages, and retention across new
  worker generations. Completed history belongs to the app, not to a worker or
  webview, and is never serialized to settings or disk.
- The visibility check covers running/paused and locked/unlocked combinations:
  in-app-only mode hides both overlays in every case. Existing settings migrate
  with overlays enabled; old `show_original` values are ignored. Changing display
  mode does not restart either engine.
- Frontend checks cover literal `[channel] player: text` formatting, English
  passthrough rows, and rejection of older history snapshots after newer events.
  The chat worker checks English passthrough with a fixed Russian source.
- Browser inspection verifies the Display selector, conditional overlay controls,
  Settings/Translations navigation, equal-width scrollable panes, unchanged English
  example messages and removal of original-text duplicates. Chat history has no
  expiry timer. The voice overlay retains its previous eight-second expiry.
- Windows acceptance must additionally verify switching display mode while
  translating or moving an overlay, keeping completed messages across Pause/Start
  and model changes, collecting messages while Settings is hidden, preserving
  history when moving between pages, and clearing it only after quitting the app.

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
7. **Text chat:** add `-condebug` to existing Steam launch options, restart CS2,
   and choose its actual `game/csgo/console.log`. Verify all-chat, CT/T team chat,
   dead/spectator messages, Unicode player names, Cyrillic, timestamps/prefixes,
   and the log format with the player's CS2 UI language. Confirm every supported
   channel produces exactly one caption per newly logged message and old history
   is skipped on Start. Test log rotation/truncation, missing-file recreation,
   permission errors, repeated identical lines, and unrelated console spam.
   Download/import both text models; reject missing or modified files, cancel
   midway, retry, and restart offline. Compare Auto/Russian and use the original in-game chat
   when assessing slang and typos. Verify text CPU operation on a
   PC without Vulkan, including simultaneous GPU voice translation.
   Enable voice only, chat only, then both. Switch the chat model while voice is
   active and vice versa; only the affected worker should restart. Pause/Start
   during inference must discard pending old results, while completed history stays. Move/resize/lock each overlay,
   check focus, DPI and monitor restoration, and persist both positions.
   With `-condebug`, **CS2 writes a raw console log**; this is expected. Linguist
   should only read it and never create a translated transcript. Test literal
   HTML-like player names and chat text. Send any test chat manually in-game.
8. **Performance:** replay the same CS2 demo or controlled sequence three times
   each with translation off, CPU, and GPU, using identical game settings.
   Record average and 1% low FPS/p95 frame time, app working set, CPU/GPU use,
   caption speech-end latency, and any Falling behind events. Use Task Manager
   for memory/utilization and a frame-time tool such as PresentMon for gameplay.
   The WAV benchmark provides reproducible inference/latency measurements but
   does not measure live WASAPI acquisition delay or CS2 FPS.
   Repeat with each text model, voice alone, chat alone, and both together.
   Record chat read-to-caption delay, queue drops, and model load time separately.
   Repeat for medium and large-v3 on GPU; verify model loading with CS2 already
   running, memory pressure, and explicit CPU fallback if GPU initialization fails.

Do not publish performance claims or mark Windows acceptance complete from the
macOS fixture results. Overlap/noise quality, GPU fallback, and borderless overlay
behavior are the release gates still requiring Windows hardware.
