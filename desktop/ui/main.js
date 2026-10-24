const invoke = window.__TAURI__.core.invoke;
const { listen } = window.__TAURI__.event;

const CATEGORY_LABELS = {
  detection: 'Detection',
  ocr: 'OCR',
  inpainting: 'Inpainting (cleanup)',
  translation: 'Translation',
};

// Which Settings field (see settings.rs) holds the chosen option id for a
// category -- "detection" has none: there's only ever the one bundled
// option, nothing to remember a choice for.
const CATEGORY_SETTINGS_FIELD = {
  ocr: 'ocrModel',
  inpainting: 'inpaintingModel',
  translation: 'translationModel',
};

const el = {
  enginePreference: document.getElementById('enginePreference'),
  googleApiKey: document.getElementById('googleApiKey'),
  googleDailyCap: document.getElementById('googleDailyCap'),
  geminiModel: document.getElementById('geminiModel'),
  fontStackList: document.getElementById('fontStackList'),
  addFontBtn: document.getElementById('addFontBtn'),
  port: document.getElementById('port'),
  detectionScoreThreshold: document.getElementById('detectionScoreThreshold'),
  maxCpuThreads: document.getElementById('maxCpuThreads'),
  maxMemoryMb: document.getElementById('maxMemoryMb'),
  maxVramMb: document.getElementById('maxVramMb'),
  llmTemperature: document.getElementById('llmTemperature'),
  llmTopK: document.getElementById('llmTopK'),
  llmTopP: document.getElementById('llmTopP'),
  llmMinP: document.getElementById('llmMinP'),
  llmMaxTokens: document.getElementById('llmMaxTokens'),
  llmRepeatPenalty: document.getElementById('llmRepeatPenalty'),
  llmFrequencyPenalty: document.getElementById('llmFrequencyPenalty'),
  llmPresencePenalty: document.getElementById('llmPresencePenalty'),
  serverAddr: document.getElementById('serverAddr'),
  feedback: document.getElementById('feedback'),
  form: document.getElementById('settingsForm'),
  usageText: document.getElementById('usageText'),
  usageDot: document.querySelector('#usageStatus .dot'),
  resourceText: document.getElementById('resourceText'),
  resourceDot: document.querySelector('#resourceStatus .dot'),
  modelCatalog: document.getElementById('modelCatalog'),
  downloadProgressWrap: document.getElementById('downloadProgressWrap'),
  downloadProgressBar: document.getElementById('downloadProgressBar'),
  downloadProgressText: document.getElementById('downloadProgressText'),
  manageOverlay: document.getElementById('manageOverlay'),
  manageTitle: document.getElementById('manageTitle'),
  manageList: document.getElementById('manageList'),
  manageCloseBtn: document.getElementById('manageCloseBtn'),
};

// The model-choice selects live inside settings (ocrModel/inpaintingModel/
// translationModel), separate from the rest of the form's simple
// input-to-field mapping, since they're rebuilt dynamically from the
// catalog rather than fixed markup -- tracked here instead.
let currentSettings = null;
let currentCatalog = [];
// Ordered list of font paths (Settings.fontPaths) -- rendered as a row
// per entry below. Tracked separately from currentSettings the same way
// the catalog selects are, since the rows are rebuilt dynamically rather
// than being fixed markup.
let currentFontPaths = [];

function renderFontStack() {
  el.fontStackList.innerHTML = '';
  currentFontPaths.forEach((path, i) => {
    const row = document.createElement('div');
    row.className = 'font-stack-row';

    const input = document.createElement('input');
    input.type = 'text';
    input.placeholder = i === 0 ? 'Main lettering font .ttf/.otf path' : 'Fallback font .ttf/.otf path';
    input.value = path;
    input.addEventListener('input', () => {
      currentFontPaths[i] = input.value;
    });
    row.appendChild(input);

    const up = document.createElement('button');
    up.type = 'button';
    up.textContent = '↑';
    up.title = 'Move up (tried earlier)';
    up.disabled = i === 0;
    up.addEventListener('click', () => {
      [currentFontPaths[i - 1], currentFontPaths[i]] = [currentFontPaths[i], currentFontPaths[i - 1]];
      renderFontStack();
    });
    row.appendChild(up);

    const down = document.createElement('button');
    down.type = 'button';
    down.textContent = '↓';
    down.title = 'Move down (tried later)';
    down.disabled = i === currentFontPaths.length - 1;
    down.addEventListener('click', () => {
      [currentFontPaths[i], currentFontPaths[i + 1]] = [currentFontPaths[i + 1], currentFontPaths[i]];
      renderFontStack();
    });
    row.appendChild(down);

    const remove = document.createElement('button');
    remove.type = 'button';
    remove.textContent = '×';
    remove.title = 'Remove';
    remove.addEventListener('click', () => {
      currentFontPaths.splice(i, 1);
      renderFontStack();
    });
    row.appendChild(remove);

    el.fontStackList.appendChild(row);
  });
}

el.addFontBtn.addEventListener('click', () => {
  currentFontPaths.push('');
  renderFontStack();
  const inputs = el.fontStackList.querySelectorAll('input');
  inputs[inputs.length - 1]?.focus();
});

// The llm* generation settings are all Option<T> on the Rust side (see
// settings.rs) -- an empty input means "unset" (null, not 0), different
// from every other numeric field on this form which always has a real
// value. These two helpers keep that distinction at the edges instead of
// scattering `=== '' ? null : Number(...)` everywhere.
function applyOptionalNumber(input, value) {
  input.value = value === null || value === undefined ? '' : value;
}

function readOptionalNumber(input) {
  const raw = input.value.trim();
  if (raw === '') return null;
  const n = Number(raw);
  return Number.isNaN(n) ? null : n;
}

function applySettings(s) {
  currentSettings = s;
  el.enginePreference.value = s.enginePreference;
  el.googleApiKey.value = s.googleApiKey;
  el.googleDailyCap.value = s.googleDailyRequestCap;
  el.geminiModel.value = s.geminiModel;
  currentFontPaths = Array.isArray(s.fontPaths) ? [...s.fontPaths] : [];
  renderFontStack();
  el.port.value = s.port;
  el.detectionScoreThreshold.value = s.detectionScoreThreshold;
  el.maxCpuThreads.value = s.maxCpuThreads;
  el.maxMemoryMb.value = s.maxMemoryMb;
  el.maxVramMb.value = s.maxVramMb;
  applyOptionalNumber(el.llmTemperature, s.llmTemperature);
  applyOptionalNumber(el.llmTopK, s.llmTopK);
  applyOptionalNumber(el.llmTopP, s.llmTopP);
  applyOptionalNumber(el.llmMinP, s.llmMinP);
  applyOptionalNumber(el.llmMaxTokens, s.llmMaxTokens);
  applyOptionalNumber(el.llmRepeatPenalty, s.llmRepeatPenalty);
  applyOptionalNumber(el.llmFrequencyPenalty, s.llmFrequencyPenalty);
  applyOptionalNumber(el.llmPresencePenalty, s.llmPresencePenalty);
  el.serverAddr.textContent = `http://127.0.0.1:${s.port}`;
}

function readSettings() {
  return {
    ...currentSettings,
    port: Number(el.port.value) || 8000,
    enginePreference: el.enginePreference.value,
    googleApiKey: el.googleApiKey.value,
    googleDailyRequestCap: Math.max(0, Number(el.googleDailyCap.value) || 0),
    geminiModel: el.geminiModel.value,
    fontPaths: currentFontPaths.map((p) => p.trim()).filter((p) => p !== ''),
    detectionScoreThreshold: Math.min(1, Math.max(0, Number(el.detectionScoreThreshold.value) || 0)),
    maxCpuThreads: Math.max(0, Number(el.maxCpuThreads.value) || 0),
    maxMemoryMb: Math.max(0, Number(el.maxMemoryMb.value) || 0),
    maxVramMb: Math.max(0, Number(el.maxVramMb.value) || 0),
    llmTemperature: readOptionalNumber(el.llmTemperature),
    llmTopK: readOptionalNumber(el.llmTopK),
    llmTopP: readOptionalNumber(el.llmTopP),
    llmMinP: readOptionalNumber(el.llmMinP),
    llmMaxTokens: readOptionalNumber(el.llmMaxTokens),
    llmRepeatPenalty: readOptionalNumber(el.llmRepeatPenalty),
    llmFrequencyPenalty: readOptionalNumber(el.llmFrequencyPenalty),
    llmPresencePenalty: readOptionalNumber(el.llmPresencePenalty),
  };
}

async function load() {
  const settings = await invoke('get_settings');
  applySettings(settings);
}

async function refreshUsage() {
  try {
    const { count, cap } = await invoke('get_google_usage');
    el.usageText.textContent = cap > 0 ? `Google usage today: ${count}/${cap} requests` : `Google usage today: ${count} requests (no cap set)`;
    el.usageDot.style.background = cap > 0 && count >= cap ? '#dc2626' : cap > 0 && count >= cap * 0.8 ? '#f59e0b' : '#16a34a';
  } catch {
    el.usageText.textContent = 'Google usage today: unavailable';
  }
}

async function refreshResourceUsage() {
  try {
    const u = await invoke('get_resource_usage');
    const cpuCap = u.maxCpuThreads > 0 ? ` (cap ${u.maxCpuThreads} threads)` : '';
    const ramCap = u.maxMemoryMb > 0 ? ` / cap ${u.maxMemoryMb}MB` : '';
    const vram = u.maxVramMb > 0 ? ` · VRAM budget ${u.maxVramMb}MB` : '';
    el.resourceText.textContent =
      `CPU ${Math.round(u.cpuPercent)}%${cpuCap} · RAM ${u.usedMemoryMb}/${u.totalMemoryMb}MB${ramCap}${vram}`;
    el.resourceDot.style.background = u.cpuPercent >= 90 ? '#dc2626' : u.cpuPercent >= 70 ? '#f59e0b' : '#16a34a';
  } catch {
    el.resourceText.textContent = 'CPU / RAM: unavailable';
  }
}

function formatBytes(bytes) {
  if (!bytes) return '0 MB';
  const mb = bytes / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

// --- Model catalog (pick-per-stage + manage/download/delete) -------------

async function refreshCatalog() {
  try {
    currentCatalog = await invoke('get_model_catalog');
    renderCatalog();
  } catch (err) {
    el.modelCatalog.innerHTML = `<p class="hint">Could not load the model catalog: ${err}</p>`;
  }
}

function categoryOf(id) {
  return currentCatalog.find((c) => c.category === id);
}

function optionOf(categoryId, optionId) {
  return categoryOf(categoryId)?.options.find((o) => o.id === optionId);
}

function renderCatalog() {
  el.modelCatalog.innerHTML = '';
  for (const cat of currentCatalog) {
    const settingsField = CATEGORY_SETTINGS_FIELD[cat.category];
    const selectedId = settingsField ? currentSettings?.[settingsField] : cat.options[0]?.id;

    const row = document.createElement('div');
    row.className = 'catalog-row';

    const head = document.createElement('div');
    head.className = 'catalog-row__head';
    const stage = document.createElement('span');
    stage.className = 'catalog-row__stage';
    stage.textContent = CATEGORY_LABELS[cat.category] ?? cat.category;
    head.appendChild(stage);
    if (cat.options.length > 1 || cat.category !== 'detection') {
      const manageBtn = document.createElement('button');
      manageBtn.type = 'button';
      manageBtn.className = 'catalog-row__manage';
      manageBtn.textContent = '⚙ Manage';
      manageBtn.addEventListener('click', () => openManage(cat.category));
      head.appendChild(manageBtn);
    }
    row.appendChild(head);

    const picker = document.createElement('div');
    picker.className = 'catalog-row__picker';
    const select = document.createElement('select');
    select.disabled = !settingsField; // detection: nothing to choose between yet
    for (const opt of cat.options) {
      const optionEl = document.createElement('option');
      optionEl.value = opt.id;
      optionEl.textContent = `${opt.downloaded ? '✓ ' : ''}${opt.label}`;
      if (opt.id === selectedId) optionEl.selected = true;
      select.appendChild(optionEl);
    }
    let previousValue = select.value;
    select.addEventListener('change', async () => {
      const chosen = optionOf(cat.category, select.value);
      if (!chosen || chosen.downloaded) {
        previousValue = select.value;
        if (settingsField) currentSettings[settingsField] = select.value;
        return;
      }
      const ok = window.confirm(
        `"${chosen.label}" isn't downloaded yet (~${formatBytes(chosen.approxBytes)}). Download it now?`,
      );
      if (!ok) {
        select.value = previousValue;
        return;
      }
      const success = await downloadOption(chosen.id, chosen.label);
      if (success) {
        previousValue = select.value;
        if (settingsField) currentSettings[settingsField] = select.value;
        await refreshCatalog();
      } else {
        select.value = previousValue;
      }
    });
    picker.appendChild(select);
    row.appendChild(picker);

    el.modelCatalog.appendChild(row);
  }
}

let progressListenerBound = false;
function bindProgressListenerOnce() {
  if (progressListenerBound) return;
  progressListenerBound = true;
  listen('model-download-progress', (event) => {
    const p = event.payload;
    const pct = p.totalBytes > 0 ? Math.min(100, Math.round((p.downloadedBytes / p.totalBytes) * 100)) : 0;
    el.downloadProgressBar.style.width = `${pct}%`;
    if (p.error) {
      el.downloadProgressText.textContent = `${p.file}: failed — ${p.error}`;
    } else if (p.done) {
      el.downloadProgressText.textContent = `${p.file} done (${p.fileIndex + 1}/${p.fileCount})`;
    } else {
      el.downloadProgressText.textContent =
        `File ${p.fileIndex + 1} of ${p.fileCount}: ${p.file} — ${formatBytes(p.downloadedBytes)} / ${formatBytes(p.totalBytes)}`;
    }
  });
}

/** Downloads one catalog option, showing the shared progress bar. Returns
 * whether it succeeded. */
async function downloadOption(optionId, label) {
  bindProgressListenerOnce();
  el.downloadProgressWrap.hidden = false;
  el.downloadProgressBar.style.width = '0%';
  el.downloadProgressText.textContent = `Starting download of ${label}…`;
  try {
    await invoke('download_model_option', { optionId });
    el.downloadProgressWrap.hidden = true;
    return true;
  } catch (err) {
    el.downloadProgressText.textContent = `Download of ${label} failed: ${err}`;
    return false;
  }
}

async function deleteOption(optionId) {
  try {
    await invoke('delete_model_option', { optionId });
    return true;
  } catch (err) {
    window.alert(`Could not delete: ${err}`);
    return false;
  }
}

function openManage(categoryId) {
  const cat = categoryOf(categoryId);
  if (!cat) return;
  el.manageTitle.textContent = `Manage: ${CATEGORY_LABELS[categoryId] ?? categoryId}`;
  renderManageList(categoryId);
  el.manageOverlay.hidden = false;
}

function closeManage() {
  el.manageOverlay.hidden = true;
}

function renderManageList(categoryId) {
  const cat = categoryOf(categoryId);
  el.manageList.innerHTML = '';
  for (const opt of cat.options) {
    const item = document.createElement('div');
    item.className = 'manage-item';

    const info = document.createElement('div');
    info.className = 'manage-item__info';
    const label = document.createElement('div');
    label.className = 'manage-item__label';
    label.textContent = `${opt.downloaded ? '✓ ' : ''}${opt.label}`;
    const size = document.createElement('div');
    size.className = 'manage-item__size';
    size.textContent = opt.downloaded ? 'Downloaded' : `~${formatBytes(opt.approxBytes)}`;
    info.appendChild(label);
    info.appendChild(size);
    item.appendChild(info);

    const action = document.createElement('button');
    action.type = 'button';
    action.className = 'manage-item__action';
    if (opt.downloaded) {
      // The currently-selected option for this category can't be deleted
      // out from under the engine that's using it.
      const settingsField = CATEGORY_SETTINGS_FIELD[categoryId];
      const isSelected = settingsField && currentSettings?.[settingsField] === opt.id;
      action.textContent = 'Delete';
      action.disabled = !!isSelected || opt.builtin;
      action.title = opt.builtin
        ? 'Bundled with the app -- nothing to delete.'
        : isSelected
          ? 'Currently selected -- pick a different option first.'
          : '';
      action.addEventListener('click', async () => {
        action.disabled = true;
        action.textContent = 'Deleting…';
        const ok = await deleteOption(opt.id);
        await refreshCatalog();
        renderManageList(categoryId);
        if (!ok) {
          action.disabled = false;
          action.textContent = 'Delete';
        }
      });
    } else {
      action.textContent = 'Download';
      action.addEventListener('click', async () => {
        action.disabled = true;
        action.textContent = 'Downloading…';
        await downloadOption(opt.id, opt.label);
        await refreshCatalog();
        renderManageList(categoryId);
      });
    }
    item.appendChild(action);

    el.manageList.appendChild(item);
  }
}

el.manageCloseBtn.addEventListener('click', closeManage);
el.manageOverlay.addEventListener('click', (e) => {
  if (e.target === el.manageOverlay) closeManage();
});

el.form.addEventListener('submit', async (e) => {
  e.preventDefault();
  el.feedback.classList.remove('error');
  el.feedback.textContent = 'Saving…';
  try {
    await invoke('save_settings', { settings: readSettings() });
    el.feedback.textContent = 'Saved. New jobs will use these settings right away.';
  } catch (err) {
    el.feedback.classList.add('error');
    el.feedback.textContent = `Could not save: ${err}`;
  }
  setTimeout(() => {
    el.feedback.textContent = '';
  }, 3000);
  refreshUsage();
});

load().then(() => refreshCatalog());
refreshUsage();
setInterval(refreshUsage, 5000);
refreshResourceUsage();
setInterval(refreshResourceUsage, 2000);
