import { call, desktop, on } from './api';
import { appendCaption, languageName } from './captions';
import type { ChatCaption, ChatSettings, EngineStatus, InstalledModel, Settings, Snapshot } from './types';
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
export const chatPreview = `<section id="chat-preview" class="preview-panel" hidden><div class="preview-header"><span class="eyebrow">TEXT CHAT OVERLAY</span><span class="preview-tag">PREVIEW</span></div><div class="chat-preview-scene"><div class="caption-panel"><header class="caption-header"><div class="caption-heading"><span>TEXT CHAT → ENGLISH</span><span id="chat-preview-model">CPU · 418M</span></div><dl class="caption-stats"><div><dt>Written</dt><dd id="chat-preview-language">Auto</dd></div><div><dt>Last language</dt><dd>Russian</dd></div><div><dt>Delay</dt><dd>1.2 s</dd></div></dl></header><div class="caption-content"><article class="chat-message"><div class="chat-message-meta">[CT] Player · Russian</div><div class="chat-message-text">Let's go to B together.</div><div id="chat-preview-original" class="chat-original">Давайте пойдём вместе на B.</div></article></div></div><p class="help">Example message and timing</p></div></section>`;
export const chatControls = `<section class="panel chat-settings"><div class="section-heading"><span class="section-number">+</span><h2>Text chat</h2><label class="check-label"><input type="checkbox" id="chat-enabled"> Translate chat</label></div>
<fieldset id="chat-options" class="plain-fieldset" hidden disabled>
  <p class="help">Read CS2 chat locally. Text models are separate from Whisper and run on CPU.</p>
  <div class="chat-setup"><strong>One-time CS2 setup</strong><ol><li>Steam → CS2 → Properties → Launch Options: add <code>-condebug</code> alongside your existing options.</li><li>Restart CS2, then select <code>game/csgo/console.log</code> from its installation folder.</li></ol><p>CS2 writes raw chat to this file on disk. Translations stay in memory; nothing is sent back to the game.</p></div>
  <label for="chat-log">CS2 console.log</label><div class="log-picker"><input id="chat-log" type="text" readonly placeholder="Choose console.log after starting CS2"><button id="choose-chat-log" class="button small" type="button">Choose log</button></div>
  <p id="chat-state" class="help" role="status">Chat paused</p>
  <div class="field-header"><label for="chat-model">Text translation model</label><span id="chat-ready" class="tag">SETUP REQUIRED</span></div>
  <select id="chat-model"><option value="m2m100-418m">M2M100 418M · balanced</option><option value="m2m100-1.2b">M2M100 1.2B · larger model</option></select><p id="chat-model-help" class="help"></p>
  <div class="download-box"><strong id="chat-model-size"></strong><button id="download-chat" class="button small" type="button">Download text model</button><button id="import-chat" class="text-button" type="button">Import model folder</button></div>
  <div class="field-header"><label for="chat-language">Written language</label></div><select id="chat-language"></select><p class="help">Auto detects each message separately. Select Russian for mostly Russian chat; short messages and slang can confuse detection.</p>
  <div class="thread-row"><div><label for="chat-threads">Chat CPU threads</label><p class="help">Independent of voice processing.</p></div><input id="chat-threads" type="number" required min="1" max="8" value="2"></div>
  <label class="check-label original-option"><input id="chat-original" type="checkbox" checked> Show original text below translations</label>
  <div class="overlay-actions"><button id="move-chat" class="button small" type="button">↔ Move chat overlay</button><button id="reset-chat" class="text-button" type="button">Reset position</button></div>
</fieldset></section>`;

export function fillChat(settings: ChatSettings) {
  $<HTMLInputElement>('chat-enabled').checked = settings.enabled;
  $<HTMLSelectElement>('chat-model').value = settings.model;
  $<HTMLSelectElement>('chat-language').value = settings.language;
  $<HTMLInputElement>('chat-log').value = settings.log_path;
  $<HTMLInputElement>('chat-threads').value = String(settings.threads);
  $<HTMLInputElement>('chat-original').checked = settings.show_original;
  updateChatControls();
}
export function readChat(previous: ChatSettings): ChatSettings {
  const threads = Number($<HTMLInputElement>('chat-threads').value);
  return { ...previous, enabled: $<HTMLInputElement>('chat-enabled').checked, model: $<HTMLSelectElement>('chat-model').value as ChatSettings['model'], language: $<HTMLSelectElement>('chat-language').value, log_path: $<HTMLInputElement>('chat-log').value, threads: Number.isInteger(threads) && threads >= 1 && threads <= 8 ? threads : previous.threads, show_original: $<HTMLInputElement>('chat-original').checked };
}
export function updateChatControls() {
  const enabled = $<HTMLInputElement>('chat-enabled').checked;
  $('chat-options').hidden = !enabled; $<HTMLFieldSetElement>('chat-options').disabled = !enabled;
  $('chat-preview').hidden = !enabled;
  $('chat-preview-model').textContent = `CPU · ${$<HTMLSelectElement>('chat-model').value === 'm2m100-418m' ? '418M' : '1.2B'}`;
  $('chat-preview-language').textContent = languageName($<HTMLSelectElement>('chat-language').value);
  $('chat-preview-original').hidden = !$<HTMLInputElement>('chat-original').checked;
}
export function updateChatModels(models: InstalledModel[]) {
  const model = models.find(m => m.id === $<HTMLSelectElement>('chat-model').value)!;
  $('chat-model-size').textContent = model.bytes >= 1e9 ? `${(model.bytes / 1e9).toFixed(2)} GB` : `${Math.round(model.bytes / 1e6)} MB`;
  $('chat-ready').textContent = model.installed ? 'READY OFFLINE' : 'SETUP REQUIRED';
  $('chat-ready').classList.toggle('ready', model.installed);
  $('download-chat').textContent = model.installed ? 'Verify / repair text model' : 'Download text model';
  $('chat-model-help').textContent = model.id === 'm2m100-418m' ? '100 languages. Lower memory use and delay; start here.' : '100 languages. More capacity for translation, with higher memory use and longer delays. Quality still varies with chat slang.';
}
export function setupChat(snapshot: Snapshot, save: () => Promise<void>, attempt: (action: () => Promise<unknown>) => Promise<void>) {
  for (const code of snapshot.chat_languages) { const option = document.createElement('option'); option.value = code; option.textContent = code === 'auto' ? 'Detect automatically' : languageName(code); $('chat-language').append(option); }
  fillChat(snapshot.settings.chat); updateChatModels(snapshot.models);
  $('chat-enabled').onchange = updateChatControls;
  $('chat-model').onchange = () => { updateChatModels(snapshot.models); updateChatControls(); };
  for (const id of ['chat-language', 'chat-original']) $(id).onchange = updateChatControls;
  $('choose-chat-log').onclick = () => void attempt(async () => { const path = await call<string | null>('pick_chat_log'); if (path) { $<HTMLInputElement>('chat-log').value = path; await save(); } });
  $('download-chat').onclick = () => void attempt(() => call('download_model', { id: $<HTMLSelectElement>('chat-model').value }));
  $('import-chat').onclick = () => void attempt(() => call('import_model', { id: $<HTMLSelectElement>('chat-model').value }));
  let locked = snapshot.chat_overlay_locked;
  const lock = (value: boolean) => { locked = value; $('move-chat').textContent = value ? '↔ Move chat overlay' : '✓ Lock chat overlay'; };
  lock(locked); void on<boolean>('chat-overlay-locked', lock);
  $('move-chat').onclick = () => void attempt(() => call('set_overlay_locked', { locked: !locked, target: 'chat-overlay' }));
  $('reset-chat').onclick = () => void attempt(() => call('reset_overlay', { target: 'chat-overlay' }));
  if (!desktop) for (const id of ['choose-chat-log', 'download-chat', 'import-chat', 'move-chat', 'reset-chat']) $<HTMLButtonElement>(id).disabled = true;
}

export async function chatOverlay(root: HTMLElement) {
  document.body.classList.add('overlay-body', 'chat-overlay-body');
  root.innerHTML = `<div class="overlay-shell"><div id="move-bar" hidden><span id="drag-handle">⠿ Drag chat · resize from edges</span><button id="lock-overlay" type="button">Lock chat</button></div><section class="caption-panel"><header class="caption-header"><div class="caption-heading"><span>TEXT CHAT → ENGLISH</span><span id="chat-engine"></span></div><dl class="caption-stats"><div><dt>Written</dt><dd id="chat-source"></dd></div><div><dt>Last language</dt><dd id="chat-detected">—</dd></div><div title="Time from reading the log message to translation ready"><dt>Delay</dt><dd id="chat-delay">—</dd></div></dl><p id="chat-state" class="help" role="status"></p></header><div id="chat-messages" class="caption-content" aria-live="polite"></div><div id="move-example" class="caption" hidden>Translated chat appears here.</div><div id="error" class="notice error" hidden></div></section></div>`;
  let snapshot = await call<Snapshot>('snapshot'); let status = snapshot.chat_status;
  let entries: (ChatCaption & { expires: number })[] = [];
  const appearance = () => { document.documentElement.style.setProperty('--caption-size', `${snapshot.settings.font_size}px`); document.documentElement.style.setProperty('--caption-bg', `rgba(12,17,13,${snapshot.settings.opacity})`); };
  const render = () => {
    $('chat-engine').textContent = `CPU · ${snapshot.settings.chat.model === 'm2m100-418m' ? '418M' : '1.2B'}`;
    $('chat-source').textContent = languageName(snapshot.settings.chat.language);
    $('chat-state').textContent = status.message;
    const last = entries.at(-1);
    $('chat-detected').textContent = last ? languageName(last.language) : '—';
    $('chat-delay').textContent = last ? `${(last.latency_ms / 1000).toFixed(1)} s` : '—';
    $('chat-messages').replaceChildren(...entries.map(caption => {
      const item = document.createElement('article'); item.className = 'chat-message';
      const meta = document.createElement('div'); meta.className = 'chat-message-meta';
      meta.textContent = `[${caption.channel}] ${caption.player} · ${languageName(caption.language)}${!caption.translated && caption.language !== 'en' ? ' · original' : ''}`;
      const text = document.createElement('div'); text.className = 'chat-message-text'; text.textContent = caption.text; item.append(meta, text);
      if (snapshot.settings.chat.show_original && caption.translated) { const original = document.createElement('div'); original.className = 'chat-original'; original.textContent = caption.original; item.append(original); }
      return item;
    }));
    $('chat-messages').scrollTop = $('chat-messages').scrollHeight;
  };
  const updateStatus = (value: EngineStatus) => { if (value.generation < status.generation) return; if (value.generation !== status.generation || !value.running || ['loading', 'waiting', 'error'].includes(value.phase)) entries = []; status = value; render(); };
  let locked = true;
  const lock = (value: boolean) => { locked = value; $('move-bar').hidden = value; $('move-example').hidden = value; document.body.classList.toggle('unlocked', !value); };
  $('lock-overlay').onclick = () => { void call('set_overlay_locked', { locked: true, target: 'chat-overlay' }).catch(showError); };
  $('drag-handle').onpointerdown = event => { if (event.button === 0 && !locked && desktop) void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().startDragging()).catch(showError); };
  function showError(error: unknown) { $('error').textContent = String(error); $('error').hidden = false; }
  await Promise.all([
    on<EngineStatus>('chat-status', updateStatus),
    on<ChatCaption>('chat-caption', caption => { entries = appendCaption(entries, caption, status, Date.now(), 6, 20000); render(); }),
    on<number>('chat-reset', generation => { if (generation === status.generation) { entries = []; render(); } }),
    on<Settings>('settings', settings => { snapshot.settings = settings; appearance(); render(); }),
    on<boolean>('chat-overlay-locked', lock),
    on<string>('app-error', showError),
  ]);
  snapshot = await call<Snapshot>('snapshot'); appearance(); updateStatus(snapshot.chat_status); lock(snapshot.chat_overlay_locked);
  setInterval(() => { const remaining = entries.filter(c => c.expires > Date.now()); if (remaining.length !== entries.length) { entries = remaining; render(); } }, 250);
}
