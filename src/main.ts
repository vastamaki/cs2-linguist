import './style.css';
import { chatControls, chatPreview, setupChat, fillChat, readChat, updateChatModels, updateChatControls, chatOverlay } from './chat';
import { call, desktop, on } from './api';
import { appendCaption, languageName, latestHistory, renderHistory, overlayStats, type VisibleCaption } from './captions';
import { SettingsAutosave } from './settings';
import type { Caption, DownloadProgress, EngineStatus, InstalledModel, Settings, Snapshot, TranslationHistory } from './types';

const root = document.querySelector<HTMLDivElement>('#app')!;
const isOverlay = new URLSearchParams(location.search).has('overlay');
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const speechIcon = '<svg viewBox="0 0 32 32" fill="none" aria-hidden="true"><path d="M6 7h20v14H15l-7 5v-5H6V7Z" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/><path d="M11 12h10M11 16h7" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>';
const playIcon = '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 4 10 6-10 6Z" fill="currentColor"/></svg>';
const overlayHeader = `<header class="caption-header" aria-label="Translation details">
  <div class="caption-heading"><span id="overlay-engine">Paused</span><span><span id="overlay-backend">CPU</span> · <span id="overlay-model">small</span></span></div>
  <dl class="caption-stats">
    <div title="Language of the latest caption. Detection is off when a fixed spoken language is selected."><dt>Detected</dt><dd id="overlay-detected">—</dd></div>
    <div title="The spoken language currently selected in settings."><dt>Spoken</dt><dd id="overlay-spoken">Auto</dd></div>
    <div title="Last caption: time from speech ending to translation ready."><dt>Delay</dt><dd id="overlay-delay">—</dd></div>
    <div title="Time spent translating the last audio segment."><dt>Decode</dt><dd id="overlay-decode">—</dd></div>
  </dl>
</header>`;
let snapshot: Snapshot;
let status: EngineStatus;
let chatStatus: EngineStatus;
let captions: VisibleCaption[] = [];
let history: TranslationHistory = { revision: 0, voice: [], chat: [] };
let locked = true;
const autosave = new SettingsAutosave(async settings => {
  if (desktop) await call('save_settings', { settings });
  else snapshot.settings = settings;
});

function error(message: unknown) {
  const text = message instanceof Error ? message.message : String(message);
  if (text === 'Cancelled') return;
  const target = $('error');
  if (target) { target.textContent = text; target.hidden = false; }
}
async function attempt(task: () => Promise<unknown>) {
  if ($('error')) $('error').hidden = true;
  try { await task(); } catch (e) { error(e); }
}
function applyAppearance(settings: Settings) {
  document.documentElement.style.setProperty('--caption-size', `${settings.font_size}px`);
  document.documentElement.style.setProperty('--caption-bg', `rgba(12, 17, 13, ${settings.opacity})`);
}
function renderCaptions() {
  const target = $('captions');
  if (!target) return;
  target.replaceChildren(...captions.map(c => {
    const entry = document.createElement('div');
    entry.className = 'caption'; entry.textContent = c.text;
    return entry;
  }));
  renderOverlayStats();
}
function renderOverlayStats() {
  if (!status) return;
  const preview = !isOverlay;
  const settings = preview ? formSettings() : snapshot.settings;
  const stats = overlayStats(settings, preview ? { ...status, running: true, backend: settings.backend } : status,
    preview ? { id: 0, generation: status.generation, text: '', language: 'ru', latency_ms: 1200, inference_ms: 600, expires: Infinity } : captions.at(-1), Date.now());
  for (const [key, value] of Object.entries(stats)) $(`overlay-${key}`).textContent = value;
  const phases: Record<string, string> = { paused: 'Paused', loading: 'Loading model', waiting: 'Waiting for CS2', listening: 'Listening', behind: 'Falling behind', reconnecting: 'Reconnecting', error: 'Engine error' };
  $('overlay-engine').textContent = preview ? 'Example · English subtitles' : phases[status.phase] ?? status.message;
  $('overlay-engine').classList.toggle('active', !preview && status.phase === 'listening');
}
function updateStatus(value: EngineStatus) {
  if (status && value.generation < status.generation) return;
  if (!status || value.generation !== status.generation || !value.running || ['loading', 'waiting', 'reconnecting', 'error'].includes(value.phase)) { captions = []; renderCaptions(); }
  status = value;
  renderOverlayStats();
  if (isOverlay) {
    const visible = ['behind', 'error', 'reconnecting'].includes(value.phase);
    $('overlay-status').hidden = !visible;
    $('overlay-status').textContent = visible ? value.message : '';
    return;
  }
  renderSessionStatus();
  $('warning').textContent = value.warning ?? '';
  $('warning').hidden = !value.warning;
}
function renderSessionStatus() {
  if (isOverlay || !status || !chatStatus) return;
  const running = status.running || chatStatus.running;
  const parts = [];
  if ($<HTMLInputElement>('voice-enabled').checked || status.running) parts.push(`Voice: ${status.message}`);
  if ($<HTMLInputElement>('chat-enabled').checked || chatStatus.running) parts.push(`Chat: ${chatStatus.message}`);
  $('status-text').textContent = parts.join(' · ') || 'Enable voice or chat translation to start.';
  $('status-dot').classList.toggle('active', running);
  $('engine-phase').textContent = running ? 'TRANSLATION ACTIVE' : 'TRANSLATION PAUSED';
  $('start-label').textContent = running ? 'Pause translation' : 'Start translation';
  const modes = [];
  if ($<HTMLInputElement>('voice-enabled').checked || status.running) modes.push(`Voice ${status.backend === 'gpu' ? 'GPU' : 'CPU'}`);
  if ($<HTMLInputElement>('chat-enabled').checked || chatStatus.running) modes.push('Chat CPU');
  $('engine-mode').textContent = modes.join(' · ');
  $('chat-state').textContent = chatStatus.message;
  $('live-chat-status').textContent = $<HTMLInputElement>('chat-enabled').checked ? chatStatus.message : 'Chat translation is disabled';
  $('live-voice-status').textContent = $<HTMLInputElement>('voice-enabled').checked ? status.message : 'Voice translation is disabled';
  $('live-chat-details').textContent = `CPU · ${languageName($<HTMLSelectElement>('chat-language').value)} → English`;
  $('live-voice-details').textContent = `${status.backend.toUpperCase()} · ${languageName($<HTMLSelectElement>('language').value)} → English`;
  for (const [kind, engine] of [['chat', chatStatus], ['voice', status]] as const) {
    const last = history[kind].at(-1);
    const fresh = engine.running && last?.generation === engine.generation ? last : undefined;
    $(`live-${kind}-timing`).textContent = `Last language: ${languageName(fresh?.language ?? '')} · Delay: ${fresh ? `${(fresh.latency_ms / 1000).toFixed(1)} s` : '—'}`;
  }
}
function updateHistory(value: TranslationHistory) {
  history = latestHistory(history, value);
  if (isOverlay) return;
  renderHistory($('live-chat'), history.chat, 'New chat messages appear here.');
  renderHistory($('live-voice'), history.voice, 'New voice translations appear here.');
  $('chat-count').textContent = `${history.chat.length} / 10`;
  $('voice-count').textContent = `${history.voice.length} / 10`;
  renderSessionStatus();
}
function showPage(page: 'settings' | 'translations') {
  const translations = page === 'translations';
  $('settings-form').hidden = translations;
  $('translations-page').hidden = !translations;
  $('app-shell').classList.toggle('translations-open', translations);
  for (const name of ['settings', 'translations']) {
    const button = $(`${name}-page-button`);
    if (name === page) button.setAttribute('aria-current', 'page'); else button.removeAttribute('aria-current');
  }
  window.history.replaceState(null, '', `#${page}`);
  window.scrollTo(0, 0);
  if (translations) {
    updateHistory(history);
    for (const id of ['live-chat', 'live-voice']) $(id).scrollTop = $(id).scrollHeight;
  }
}
function updateChatStatus(value: EngineStatus) {
  if (chatStatus && value.generation < chatStatus.generation) return;
  chatStatus = value; renderSessionStatus();
}
function updateModels(models: InstalledModel[]) {
  snapshot.models = models;
  updateChatModels(models);
  const selected = $<HTMLSelectElement>('model').value;
  const model = models.find(m => m.id === selected)!;
  const vad = models.find(m => m.id === 'vad')!;
  $('model-ready').textContent = model.installed && vad.installed ? 'READY OFFLINE' : 'SETUP REQUIRED';
  $('model-ready').classList.toggle('ready', model.installed && vad.installed);
  updateConditionalSettings();
  const size = model.bytes >= 1e9 ? `${(model.bytes / 1e9).toFixed(2)} GB` : `${Math.round(model.bytes / 1e6)} MB`;
  $('model-size').textContent = `${size} + speech detector`;
  $('download-label').textContent = model.installed && vad.installed ? 'Verify / repair models' : 'Download models';
  $('vad-state').textContent = vad.installed ? 'Speech detector ready' : 'Speech detector included in download';
}
function updateProgress(progress: DownloadProgress | null) {
  if (!progress || isOverlay) return;
  const busy = ['downloading', 'verifying', 'importing'].includes(progress.phase);
  $<HTMLButtonElement>('download').disabled = busy || !desktop;
  $<HTMLButtonElement>('import').disabled = busy || !desktop;
  $<HTMLButtonElement>('import-vad').disabled = busy || !desktop;
  for (const id of ['download-chat', 'import-chat']) $<HTMLButtonElement>(id).disabled = busy || !desktop;
  $('cancel').hidden = !busy;
  $('progress-section').hidden = false;
  $('progress-text').textContent = progress.message;
  const bar = $<HTMLProgressElement>('progress');
  bar.max = progress.total || 1; bar.value = progress.phase === 'complete' ? bar.max : progress.received;
  if (progress.phase === 'error') error(progress.message);
}
function formSettings(): Settings {
  const gpu = $<HTMLInputElement>('gpu').checked;
  const threads = Number($<HTMLInputElement>('threads').value);
  return { ...snapshot.settings,
    voice_enabled: $<HTMLInputElement>('voice-enabled').checked, chat: readChat(snapshot.settings.chat),
    overlays_enabled: $<HTMLSelectElement>('display-mode').value === 'overlay',
    backend: gpu ? 'gpu' : 'cpu',
    model: $<HTMLSelectElement>('model').value as Settings['model'],
    language: $<HTMLSelectElement>('language').value,
    threads: Number.isInteger(threads) && threads >= 1 && threads <= 32 ? threads : snapshot.settings.threads,
    font_size: Number($<HTMLInputElement>('font-size').value),
    opacity: Number($<HTMLInputElement>('opacity').value) / 100,
  };
}
async function save() {
  if (!$<HTMLFormElement>('settings-form').reportValidity()) return;
  $('save-state').textContent = desktop ? 'Saving settings…' : 'Updating preview…';
  try {
    await autosave.save(formSettings());
    if (!autosave.pending) $('save-state').textContent = desktop ? 'Settings saved automatically.' : 'Preview updated · settings are not saved.';
  } catch (e) {
    $('save-state').textContent = 'Could not apply settings. Change the setting to retry.';
    throw e;
  }
}
function fillSettings(s: Settings) {
  $<HTMLSelectElement>('display-mode').value = s.overlays_enabled ? 'overlay' : 'app';
  $<HTMLInputElement>('voice-enabled').checked = s.voice_enabled; fillChat(s.chat);
  $<HTMLInputElement>(s.backend).checked = true;
  $<HTMLSelectElement>('model').value = s.model;
  $<HTMLSelectElement>('language').value = s.language;
  $<HTMLInputElement>('threads').value = String(s.threads);
  $<HTMLInputElement>('font-size').value = String(s.font_size);
  $<HTMLInputElement>('opacity').value = String(Math.round(s.opacity * 100));
  updateConditionalSettings();
  updateAppearanceLabels();
}
function updateConditionalSettings() {
  const voice = $<HTMLInputElement>('voice-enabled').checked;
  const overlays = $<HTMLSelectElement>('display-mode').value === 'overlay';
  $('voice-options').hidden = !voice; $<HTMLFieldSetElement>('voice-options').disabled = !voice;
  $('voice-processing').hidden = !voice;
  $('voice-preview').hidden = !voice || !overlays;
  $('voice-overlay-actions').hidden = !voice || !overlays;
  $('overlay-style-options').hidden = !overlays;
  $('overlay-help').hidden = !overlays;
  $('display-help').textContent = overlays ? 'Use the overlays while playing, or open the Translations page here.' : 'Both overlays are hidden. Read chat and voice side by side on the Translations page.';
  updateChatControls(); renderSessionStatus();
  const gpu = $<HTMLInputElement>('gpu').checked;
  $('cpu-options').hidden = gpu;
  $<HTMLInputElement>('threads').disabled = gpu || !voice;
  $('gpu-help').hidden = !gpu;
  const model = $<HTMLSelectElement>('model').value as Settings['model'];
  const descriptions: Record<Settings['model'], string> = {
    base: 'Lowest memory use and fastest processing, with lower recognition accuracy.',
    small: 'A balance of speed and accuracy. Try medium if words are missed.',
    medium: 'Higher accuracy, with more memory use and delay. A good next step from small.',
    'large-v3': 'The largest model offered for difficult speech. Uses the most memory and may delay captions.',
  };
  const larger = model === 'medium' || model === 'large-v3';
  $('model-description').textContent = descriptions[model] + (larger
    ? gpu ? ' Shares GPU memory with CS2; choose a smaller model if captions fall behind.' : ' GPU recommended; CPU processing may be too slow for live captions.'
    : '');
  $('language-help').textContent = $<HTMLSelectElement>('language').value === 'auto'
    ? 'Auto guesses the language for each phrase. For mostly Russian chat, choose Russian to avoid wrong guesses on short callouts.'
    : 'Fixed language avoids guessing on short callouts. Use Auto when players speak different languages.';
  renderOverlayStats();
}
function updateAppearanceLabels() {
  const s = formSettings();
  $('font-value').textContent = `${s.font_size}px`;
  $('opacity-value').textContent = `${Math.round(s.opacity * 100)}%`;
  applyAppearance(s);
}

function settingsPage() {
  root.innerHTML = `
  <main id="app-shell" class="shell">
    <header class="topbar"><a class="brand" href="#" aria-label="Linguist home"><span class="brand-icon">${speechIcon}</span><span>linguist<span class="brand-period">.</span></span></a><span class="local-badge"><span></span> LOCAL BY DESIGN</span></header>
    <div class="intro"><div><div class="eyebrow">COUNTER-STRIKE 2 / LOCAL TRANSLATION</div><h1>Stay in the conversation.</h1><p>Translate voice and text chat into English. Keep your eyes on the game.</p></div><span class="version">v0.1</span></div>
    <div id="platform-note" class="notice" hidden>UI preview — live game capture requires the Windows 11 desktop app.</div>
    <div id="error" class="notice error" role="alert" hidden></div><div id="warning" class="notice" role="status" hidden></div>
    <section class="status-strip" aria-label="Translation status"><div class="status-copy"><span id="status-dot" class="status-dot"></span><div><span id="engine-phase" class="eyebrow">ENGINE PAUSED</span><strong id="status-text">Ready when you are</strong></div></div><button id="start" class="button primary">${playIcon}<span id="start-label">Start translation</span></button></section>
    <nav class="page-navigation" aria-label="Pages"><button id="settings-page-button" type="button" aria-current="page">Settings</button><button id="translations-page-button" type="button">Translations</button></nav>
    <section id="translations-page" hidden aria-label="Translations">
      <div class="history-note"><span>Last 10 messages per window · kept until replaced or the app closes</span><span id="history-preview-label" class="preview-tag" hidden>EXAMPLE MESSAGES</span></div>
      <div class="translation-split">
        <section class="translation-pane"><header><div class="pane-title"><h2>Text chat</h2><span id="chat-count">0 / 10</span></div><p id="live-chat-details" class="help"></p><p id="live-chat-timing" class="help"></p><p id="live-chat-status" class="help" role="status"></p></header><div id="live-chat" class="history-list" role="log" aria-label="Chat translations" aria-live="polite" tabindex="0"></div></section>
        <section class="translation-pane"><header><div class="pane-title"><h2>Voice</h2><span id="voice-count">0 / 10</span></div><p id="live-voice-details" class="help"></p><p id="live-voice-timing" class="help"></p><p id="live-voice-status" class="help" role="status"></p></header><div id="live-voice" class="history-list" role="log" aria-label="Voice translations" aria-live="polite" tabindex="0"></div></section>
      </div>
    </section>
    <form id="settings-form" class="workspace"><div class="controls">
      <section class="panel"><div class="section-heading"><span class="section-number">01</span><h2>Voice</h2><label class="check-label"><input type="checkbox" id="voice-enabled" checked> Translate voice</label></div><fieldset id="voice-options" class="plain-fieldset">
        <label for="language">Spoken language</label><select id="language" aria-describedby="language-help"></select><p id="language-help" class="help"></p>
        <div class="field-header"><label for="model">Speech model</label><span id="model-ready" class="tag">SETUP REQUIRED</span></div>
        <select id="model" aria-describedby="model-description"><option value="base">Whisper base · lightweight</option><option value="small">Whisper small · balanced</option><option value="medium">Whisper medium · higher accuracy</option><option value="large-v3">Whisper large-v3 · highest capacity</option></select><p id="model-description" class="help"></p>
        <div class="download-box"><div class="download-meta"><span class="download-symbol">↓</span><div><strong id="model-size"></strong><small id="vad-state"></small></div></div><button id="download" type="button" class="button small"><span id="download-label">Download models</span></button>
          <div id="progress-section" hidden><progress id="progress" max="1" value="0"></progress><div class="progress-row"><span id="progress-text" role="status"></span><button id="cancel" type="button" class="text-button">Cancel</button></div></div>
          <div class="import-row"><button id="import" type="button" class="text-button">Import model file</button><span>·</span><button id="import-vad" type="button" class="text-button">Import speech detector</button></div>
        </div>
      </fieldset></section>
      <section id="voice-processing" class="panel"><div class="section-heading"><span class="section-number">02</span><h2>Processing</h2></div><fieldset class="segmented"><legend class="sr-only">Processing backend</legend><label><input type="radio" name="backend" id="cpu" value="cpu" checked><span>CPU <small>Compatible</small></span></label><label><input type="radio" name="backend" id="gpu" value="gpu"><span>GPU <small>Accelerated</small></span></label></fieldset><div id="cpu-options" class="thread-row"><div><label for="threads">CPU threads</label><p class="help">Leave some room for the game.</p></div><input id="threads" type="number" required min="1" max="32" value="4"></div><p id="gpu-help" class="help gpu-help" hidden>GPU mode uses Vulkan. If unavailable, Linguist uses CPU and tells you why.</p></section>
      ${chatControls}
      <p id="save-state" class="help" role="status">Settings save automatically.</p>
    </div>
    <aside class="preview-column"><section id="voice-preview" class="preview-panel"><div class="preview-header"><span class="eyebrow">VOICE OVERLAY</span><span class="preview-tag">PREVIEW</span></div><div class="game-preview"><div class="map-grid"></div><div class="crosshair"></div><span class="game-coordinate">MID / 01</span><div class="sample-captions caption-panel">${overlayHeader}<div class="caption-content"><div class="caption sample-old">Two players coming through mid.</div><div class="caption">Watch your left. I’m covering B.</div></div></div><span class="preview-footnote">Example captions and timing</span></div><div class="preview-caption">The conversation, without the distraction.</div></section>
      ${chatPreview}
      <section class="panel appearance"><div class="section-heading"><span class="section-number">03</span><h2>Display</h2></div>
        <div class="field-header"><label for="display-mode">Translation display</label></div><select id="display-mode"><option value="overlay">Overlays + in-app page</option><option value="app">In app only · no overlays</option></select><p id="display-help" class="help"></p><button id="open-translations" type="button" class="text-button open-translations">Open Translations page →</button>
        <div class="field-header"><label for="font-size">Subtitle size</label><output id="font-value" for="font-size">24px</output></div><input id="font-size" type="range" min="14" max="48" value="24">
        <div id="overlay-style-options"><div class="field-header"><label for="opacity">Background opacity</label><output id="opacity-value" for="opacity">45%</output></div><input id="opacity" type="range" min="0" max="90" value="45"></div>
        <div id="voice-overlay-actions" class="overlay-actions"><button id="move-overlay" class="button small" type="button">↔ Move voice overlay</button><button id="reset-overlay" class="text-button" type="button">Reset position</button></div><p id="overlay-help" class="help">Start translation to show captions. Pause to hide the overlays. Unlock to drag or resize, then lock it in place.</p>
      </section>
      <div class="privacy-note"><span class="privacy-mark">⌁</span><p><strong>Your translations stay local.</strong><br>Download models once. Translate offline.<br>Use CS2 in borderless-windowed mode.</p></div>
    </aside></form>
    <footer><span><span class="footer-dot"></span> CS2 VOICE + CHAT</span><span id="engine-mode">CPU</span><span id="latency">No audio stored</span><span class="footer-right">BUILT FOR THE NEXT ROUND</span></footer>
  </main>`;

  $('platform-note').hidden = snapshot.supported;
  $('history-preview-label').hidden = desktop;
  $('settings-page-button').onclick = () => showPage('settings');
  for (const id of ['translations-page-button', 'open-translations']) $(id).onclick = () => showPage('translations');
  window.addEventListener('hashchange', () => showPage(location.hash === '#translations' ? 'translations' : 'settings'));
  for (const code of snapshot.languages) {
    const option = document.createElement('option'); option.value = code;
    option.textContent = code === 'auto' ? 'Detect automatically' : languageName(code);
    $('language').append(option);
  }
  setupChat(snapshot, save, attempt);
  $('save-state').before($('progress-section'));
  fillSettings(snapshot.settings); updateModels(snapshot.models); updateProgress(snapshot.download);
  $<HTMLButtonElement>('start').disabled = !snapshot.supported;
  if (!desktop) {
    $('save-state').textContent = 'Preview only · settings are not saved.';
    for (const id of ['download', 'import', 'import-vad', 'move-overlay', 'reset-overlay']) $<HTMLButtonElement>(id).disabled = true;
  }
  $('settings-form').addEventListener('submit', event => { event.preventDefault(); void attempt(save); });
  $('settings-form').addEventListener('change', () => { renderSessionStatus(); void attempt(save); });
  $('start').onclick = () => void attempt(async () => {
    const button = $<HTMLButtonElement>('start');
    button.disabled = true;
    try {
      if (status.running || chatStatus.running) await call('stop');
      else {
        if (!$<HTMLFormElement>('settings-form').reportValidity()) return;
        await save();
        await call('start');
      }
    } finally { button.disabled = !snapshot.supported; }
  });
  $('model').onchange = () => updateModels(snapshot.models);
  for (const id of ['cpu', 'gpu', 'language', 'voice-enabled', 'chat-enabled', 'display-mode']) $(id).onchange = updateConditionalSettings;
  $('download').onclick = () => void attempt(() => call('download_model', { id: $<HTMLSelectElement>('model').value }));
  $('import').onclick = () => void attempt(() => call('import_model', { id: $<HTMLSelectElement>('model').value }));
  $('import-vad').onclick = () => void attempt(() => call('import_model', { id: 'vad' }));
  $('cancel').onclick = () => void attempt(() => call('cancel_download'));
  for (const id of ['font-size', 'opacity']) $(id).oninput = updateAppearanceLabels;
  $('move-overlay').onclick = () => void attempt(() => call('set_overlay_locked', { locked: !locked }));
  $('reset-overlay').onclick = () => void attempt(() => call('reset_overlay'));
}

function overlayPage() {
  document.body.classList.add('overlay-body');
  root.innerHTML = `<div class="overlay-shell"><div id="move-bar" hidden><span id="drag-handle">⠿ Drag to move · resize from edges</span><button id="lock-overlay" type="button">Lock overlay</button></div><section class="caption-panel">${overlayHeader}<div id="overlay-status" class="overlay-status" role="status" hidden></div><div class="caption-content"><div id="captions" aria-live="polite"></div><div id="move-example" class="caption" hidden>Your translated callouts will appear here.</div></div><div id="error" class="notice error" hidden></div></section></div>`;
  $('lock-overlay').onclick = () => void attempt(() => call('set_overlay_locked', { locked: true }));
  $('drag-handle').onpointerdown = event => {
    if (event.button !== 0 || locked || !desktop) return;
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().startDragging()).catch(error);
  };
  setInterval(() => {
    const remaining = captions.filter(c => c.expires > Date.now());
    if (remaining.length !== captions.length) { captions = remaining; renderCaptions(); }
  }, 200);
}

function updateLock(value: boolean) {
  locked = value;
  if (isOverlay) {
    $('move-bar').hidden = value; $('move-example').hidden = value;
    document.body.classList.toggle('unlocked', !value);
  } else { $('move-overlay').textContent = value ? '↔ Move voice overlay' : '✓ Lock voice overlay'; }
}

async function initialize() {
  snapshot = await call<Snapshot>('snapshot'); chatStatus = snapshot.chat_status; history = snapshot.history;
  if (isOverlay) overlayPage(); else settingsPage();
  applyAppearance(snapshot.settings); updateStatus(snapshot.status); updateLock(snapshot.overlay_locked);
  await Promise.all([
    on<EngineStatus>('engine-status', updateStatus),
    on<EngineStatus>('chat-status', updateChatStatus),
    on<TranslationHistory>('translation-history', updateHistory),
    on<null>('show-translations', () => { if (!isOverlay) showPage('translations'); }),
    on<Caption>('caption', caption => {
      if (isOverlay) { captions = appendCaption(captions, caption, status, Date.now()); renderCaptions(); }
      else if (status.running && caption.generation === status.generation) $('latency').textContent = `Last caption · ${(caption.latency_ms / 1000).toFixed(1)}s delay`;
    }),
    on<Settings>('settings', settings => {
      snapshot.settings = settings;
      // An acknowledgement for an earlier save must not reset a newer edit.
      if (isOverlay) { applyAppearance(settings); renderOverlayStats(); }
      else if (!autosave.pending) { fillSettings(settings); updateModels(snapshot.models); }
    }),
    on<InstalledModel[]>('models-changed', models => { if (!isOverlay) updateModels(models); }),
    on<DownloadProgress>('model-progress', updateProgress),
    on<boolean>('overlay-locked', updateLock),
    on<string>('app-error', error),
  ]);
  // Refresh after listeners are attached so window creation cannot miss a status transition.
  const current = await call<Snapshot>('snapshot'); snapshot.settings = current.settings;
  if (isOverlay) applyAppearance(current.settings);
  updateStatus(current.status); updateChatStatus(current.chat_status); updateLock(current.overlay_locked);
  updateHistory(current.history);
  if (!isOverlay) showPage(location.hash === '#translations' || !current.settings.overlays_enabled ? 'translations' : 'settings');
}
void (new URLSearchParams(location.search).has('chat-overlay') ? chatOverlay(root) : initialize()).catch(error);
