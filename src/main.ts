import './style.css';
import { call, desktop, on } from './api';
import { appendCaption, type VisibleCaption } from './captions';
import type { Caption, DownloadProgress, EngineStatus, InstalledModel, Settings, Snapshot } from './types';

const root = document.querySelector<HTMLDivElement>('#app')!;
const isOverlay = new URLSearchParams(location.search).has('overlay');
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const speechIcon = '<svg viewBox="0 0 32 32" fill="none" aria-hidden="true"><path d="M6 7h20v14H15l-7 5v-5H6V7Z" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/><path d="M11 12h10M11 16h7" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>';
const playIcon = '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 4 10 6-10 6Z" fill="currentColor"/></svg>';
let snapshot: Snapshot;
let status: EngineStatus;
let captions: VisibleCaption[] = [];
let locked = true;

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
}
function updateStatus(value: EngineStatus) {
  if (status && value.generation < status.generation) return;
  if (!status || value.generation !== status.generation || !value.running) { captions = []; renderCaptions(); }
  status = value;
  if (isOverlay) {
    const visible = ['behind', 'error', 'reconnecting'].includes(value.phase);
    $('overlay-status').hidden = !visible;
    $('overlay-status').textContent = visible ? value.message : '';
    return;
  }
  $('status-text').textContent = value.message;
  $('status-dot').classList.toggle('active', value.running);
  $('engine-mode').textContent = value.backend === 'gpu' ? 'GPU · Vulkan' : 'CPU';
  $('engine-phase').textContent = value.running ? 'ENGINE ACTIVE' : 'ENGINE PAUSED';
  $('start-label').textContent = value.running ? 'Pause translation' : 'Start translation';
  $('warning').textContent = value.warning ?? '';
  $('warning').hidden = !value.warning;
}
function updateModels(models: InstalledModel[]) {
  snapshot.models = models;
  const selected = $<HTMLSelectElement>('model').value;
  const model = models.find(m => m.id === selected)!;
  const vad = models.find(m => m.id === 'vad')!;
  $('model-ready').textContent = model.installed && vad.installed ? 'READY OFFLINE' : 'SETUP REQUIRED';
  $('model-ready').classList.toggle('ready', model.installed && vad.installed);
  $('model-description').textContent = selected === 'small' ? 'Better recognition for multilingual voice chat.' : 'A lighter model with less CPU and memory use.';
  $('model-size').textContent = `${Math.round(model.bytes / 1024 / 1024)} MB + speech detector`;
  $('download-label').textContent = model.installed && vad.installed ? 'Verify / repair models' : 'Download models';
  $('vad-state').textContent = vad.installed ? 'Speech detector ready' : 'Speech detector included in download';
}
function updateProgress(progress: DownloadProgress | null) {
  if (!progress || isOverlay) return;
  const busy = ['downloading', 'verifying', 'importing'].includes(progress.phase);
  $<HTMLButtonElement>('download').disabled = busy || !desktop;
  $<HTMLButtonElement>('import').disabled = busy || !desktop;
  $<HTMLButtonElement>('import-vad').disabled = busy || !desktop;
  $('cancel').hidden = !busy;
  $('progress-section').hidden = false;
  $('progress-text').textContent = progress.message;
  const bar = $<HTMLProgressElement>('progress');
  bar.max = progress.total || 1; bar.value = progress.phase === 'complete' ? bar.max : progress.received;
  if (progress.phase === 'error') error(progress.message);
}
function formSettings(): Settings {
  return { ...snapshot.settings,
    backend: $<HTMLInputElement>('gpu').checked ? 'gpu' : 'cpu',
    model: $<HTMLSelectElement>('model').value as Settings['model'],
    language: $<HTMLSelectElement>('language').value,
    threads: Number($<HTMLInputElement>('threads').value),
    font_size: Number($<HTMLInputElement>('font-size').value),
    opacity: Number($<HTMLInputElement>('opacity').value) / 100,
  };
}
async function save() {
  const settings = formSettings();
  await call('save_settings', { settings });
  snapshot.settings = settings;
  $('save-label').textContent = 'Changes saved';
}
function fillSettings(s: Settings) {
  $<HTMLInputElement>(s.backend).checked = true;
  $<HTMLSelectElement>('model').value = s.model;
  $<HTMLSelectElement>('language').value = s.language;
  $<HTMLInputElement>('threads').value = String(s.threads);
  $<HTMLInputElement>('font-size').value = String(s.font_size);
  $<HTMLInputElement>('opacity').value = String(Math.round(s.opacity * 100));
  $<HTMLInputElement>('show-overlay').checked = s.overlay_visible;
  updateAppearanceLabels();
}
function updateAppearanceLabels() {
  const s = formSettings();
  $('font-value').textContent = `${s.font_size}px`;
  $('opacity-value').textContent = `${Math.round(s.opacity * 100)}%`;
  applyAppearance(s);
}

function settingsPage() {
  root.innerHTML = `
  <main class="shell">
    <header class="topbar"><a class="brand" href="#" aria-label="Linguist home"><span class="brand-icon">${speechIcon}</span><span>linguist<span class="brand-period">.</span></span></a><span class="local-badge"><span></span> LOCAL BY DESIGN</span></header>
    <div class="intro"><div><div class="eyebrow">COUNTER-STRIKE 2 / VOICE TRANSLATION</div><h1>Stay in the conversation.</h1><p>Translate voice chat into English. Keep your eyes on the game.</p></div><span class="version">v0.1</span></div>
    <div id="platform-note" class="notice" hidden>UI preview — live game capture requires the Windows 11 desktop app.</div>
    <div id="error" class="notice error" role="alert" hidden></div><div id="warning" class="notice" role="status" hidden></div>
    <section class="status-strip" aria-label="Translation status"><div class="status-copy"><span id="status-dot" class="status-dot"></span><div><span id="engine-phase" class="eyebrow">ENGINE PAUSED</span><strong id="status-text">Ready when you are</strong></div></div><button id="start" class="button primary">${playIcon}<span id="start-label">Start translation</span></button></section>
    <div class="workspace"><form id="settings-form" class="controls">
      <section class="panel"><div class="section-heading"><span class="section-number">01</span><h2>Translation</h2><span class="destination">→ ENGLISH</span></div>
        <label for="language">Spoken language</label><select id="language"></select><p class="help">Auto works across languages. Choose Russian for more consistent short callouts.</p>
        <div class="field-header"><label for="model">Speech model</label><span id="model-ready" class="tag">SETUP REQUIRED</span></div>
        <select id="model"><option value="small">Whisper small · recommended</option><option value="base">Whisper base · lightweight</option></select><p id="model-description" class="help"></p>
        <div class="download-box"><div class="download-meta"><span class="download-symbol">↓</span><div><strong id="model-size"></strong><small id="vad-state"></small></div></div><button id="download" type="button" class="button small"><span id="download-label">Download models</span></button>
          <div id="progress-section" hidden><progress id="progress" max="1" value="0"></progress><div class="progress-row"><span id="progress-text" role="status"></span><button id="cancel" type="button" class="text-button">Cancel</button></div></div>
          <div class="import-row"><button id="import" type="button" class="text-button">Import model file</button><span>·</span><button id="import-vad" type="button" class="text-button">Import speech detector</button></div>
        </div>
      </section>
      <section class="panel"><div class="section-heading"><span class="section-number">02</span><h2>Processing</h2></div><fieldset class="segmented"><legend class="sr-only">Processing backend</legend><label><input type="radio" name="backend" id="cpu" value="cpu" checked><span>CPU <small>Compatible</small></span></label><label><input type="radio" name="backend" id="gpu" value="gpu"><span>GPU <small>Accelerated</small></span></label></fieldset><div class="thread-row"><div><label for="threads">CPU threads</label><p class="help">Leave some room for the game.</p></div><input id="threads" type="number" min="1" max="32" value="4"></div><p class="help gpu-help">GPU mode uses Vulkan. If unavailable, Linguist uses CPU and tells you why.</p></section>
      <button id="save" class="button save" type="submit"><span id="save-label">Apply changes</span><span>↗</span></button>
    </form>
    <aside class="preview-column"><section class="preview-panel"><div class="preview-header"><span class="eyebrow">YOUR IN-GAME OVERLAY</span><span class="preview-tag">PREVIEW</span></div><div class="game-preview"><div class="map-grid"></div><div class="crosshair"></div><span class="game-coordinate">MID / 01</span><div class="sample-captions"><div class="caption sample-old">Two players coming through mid.</div><div class="caption">Watch your left. I’m covering B.</div></div><span class="preview-footnote">Example captions</span></div><div class="preview-caption">The conversation, without the distraction.</div></section>
      <section class="panel appearance"><div class="section-heading"><span class="section-number">03</span><h2>Overlay</h2><label class="switch"><input id="show-overlay" type="checkbox" checked aria-label="Show overlay"><span></span></label></div>
        <div class="field-header"><label for="font-size">Subtitle size</label><output id="font-value" for="font-size">24px</output></div><input id="font-size" type="range" min="14" max="48" value="24">
        <div class="field-header"><label for="opacity">Background opacity</label><output id="opacity-value" for="opacity">45%</output></div><input id="opacity" type="range" min="0" max="90" value="45">
        <div class="overlay-actions"><button id="move-overlay" class="button small" type="button">↔ Move overlay</button><button id="reset-overlay" class="text-button" type="button">Reset position</button></div><p class="help">Click-through during play. Unlock to drag or resize, then lock it in place.</p>
      </section>
      <div class="privacy-note"><span class="privacy-mark">⌁</span><p><strong>Your audio stays with you.</strong><br>One model download. Fully offline translation.<br>Use CS2 in borderless-windowed mode.</p></div>
    </aside></div>
    <footer><span><span class="footer-dot"></span> CS2 AUDIO ONLY</span><span id="engine-mode">CPU</span><span id="latency">No audio stored</span><span class="footer-right">BUILT FOR THE NEXT ROUND</span></footer>
  </main>`;

  $('platform-note').hidden = snapshot.supported;
  const languageNames = new Intl.DisplayNames(['en'], { type: 'language' });
  for (const code of snapshot.languages) {
    const option = document.createElement('option'); option.value = code;
    try { option.textContent = code === 'auto' ? 'Detect automatically' : languageNames.of(code) ?? code; } catch { option.textContent = code; }
    $('language').append(option);
  }
  fillSettings(snapshot.settings); updateModels(snapshot.models); updateProgress(snapshot.download);
  $<HTMLButtonElement>('start').disabled = !snapshot.supported;
  if (!desktop) for (const id of ['download', 'import', 'import-vad', 'save', 'show-overlay', 'move-overlay', 'reset-overlay']) ($<HTMLButtonElement>(id)).disabled = true;
  $('settings-form').addEventListener('submit', event => { event.preventDefault(); void attempt(save); });
  $('settings-form').addEventListener('input', () => { $('save-label').textContent = 'Apply changes'; });
  $('start').onclick = () => void attempt(async () => { if (status.running) await call('stop'); else { await save(); await call('start'); } });
  $('model').onchange = () => updateModels(snapshot.models);
  $('download').onclick = () => void attempt(() => call('download_model', { id: $<HTMLSelectElement>('model').value }));
  $('import').onclick = () => void attempt(() => call('import_model', { id: $<HTMLSelectElement>('model').value }));
  $('import-vad').onclick = () => void attempt(() => call('import_model', { id: 'vad' }));
  $('cancel').onclick = () => void attempt(() => call('cancel_download'));
  for (const id of ['font-size', 'opacity']) $(id).oninput = () => { updateAppearanceLabels(); $('save-label').textContent = 'Apply changes'; };
  $('show-overlay').onchange = () => void attempt(() => call('set_overlay_visible', { visible: $<HTMLInputElement>('show-overlay').checked }));
  $('move-overlay').onclick = () => void attempt(() => call('set_overlay_locked', { locked: !locked }));
  $('reset-overlay').onclick = () => void attempt(() => call('reset_overlay'));
}

function overlayPage() {
  document.body.classList.add('overlay-body');
  root.innerHTML = `<div class="overlay-shell"><div id="move-bar" hidden><span id="drag-handle">⠿ Drag to move · resize from edges</span><button id="lock-overlay" type="button">Lock overlay</button></div><div id="overlay-status" class="overlay-status" role="status" hidden></div><div id="captions" aria-live="polite"></div><div id="move-example" class="caption" hidden>Your translated callouts will appear here.</div><div id="error" class="notice error" hidden></div></div>`;
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
  } else { $('move-overlay').textContent = value ? '↔ Move overlay' : '✓ Lock overlay'; }
}

async function initialize() {
  snapshot = await call<Snapshot>('snapshot');
  if (isOverlay) overlayPage(); else settingsPage();
  applyAppearance(snapshot.settings); updateStatus(snapshot.status); updateLock(snapshot.overlay_locked);
  await Promise.all([
    on<EngineStatus>('engine-status', updateStatus),
    on<Caption>('caption', caption => {
      if (isOverlay) { captions = appendCaption(captions, caption, status, Date.now()); renderCaptions(); }
      else if (status.running && caption.generation === status.generation) $('latency').textContent = `Last caption · ${(caption.latency_ms / 1000).toFixed(1)}s delay`;
    }),
    on<Settings>('settings', settings => { snapshot.settings = settings; applyAppearance(settings); if (!isOverlay) { fillSettings(settings); updateModels(snapshot.models); } }),
    on<InstalledModel[]>('models-changed', models => { if (!isOverlay) updateModels(models); }),
    on<DownloadProgress>('model-progress', updateProgress),
    on<boolean>('overlay-locked', updateLock),
    on<string>('app-error', error),
  ]);
  // Refresh after listeners are attached so window creation cannot miss a status transition.
  const current = await call<Snapshot>('snapshot'); updateStatus(current.status); updateLock(current.overlay_locked);
}
void initialize().catch(error);
