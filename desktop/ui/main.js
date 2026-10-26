const invoke = window.__TAURI__.core.invoke;

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
  modelsDir: document.getElementById('modelsDir'),
  browseModelsDirBtn: document.getElementById('browseModelsDirBtn'),
  ocrModel: document.getElementById('ocrModel'),
  inpaintingModel: document.getElementById('inpaintingModel'),
  savePipelineModelsBtn: document.getElementById('savePipelineModelsBtn'),
  pipelineModelsFeedback: document.getElementById('pipelineModelsFeedback'),
  serverAddr: document.getElementById('serverAddr'),
  feedback: document.getElementById('feedback'),
  form: document.getElementById('settingsForm'),
  usageText: document.getElementById('usageText'),
  usageDot: document.querySelector('#usageStatus .dot'),
  resourceText: document.getElementById('resourceText'),
  resourceDot: document.querySelector('#resourceStatus .dot'),
};

let currentSettings = null;
// Ordered list of font paths (Settings.fontPaths) -- rendered as a row
// per entry below. Tracked separately from currentSettings since the
// rows are rebuilt dynamically rather than being fixed markup. Only
// read by the Google engine path today (engines/google.rs) -- the local
// Koharu engine does its own typesetting.
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

el.browseModelsDirBtn.addEventListener('click', async () => {
  try {
    const picked = await window.__TAURI__.dialog.open({ directory: true, multiple: false });
    if (typeof picked === 'string') {
      el.modelsDir.value = picked;
    }
  } catch (err) {
    el.feedback.classList.add('error');
    el.feedback.textContent = `Could not open folder picker: ${err}`;
  }
});

el.addFontBtn.addEventListener('click', () => {
  currentFontPaths.push('');
  renderFontStack();
  const inputs = el.fontStackList.querySelectorAll('input');
  inputs[inputs.length - 1]?.focus();
});

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
  el.modelsDir.value = s.modelsDir || '';
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
    modelsDir: el.modelsDir.value.trim() || null,
  };
}

async function load() {
  const settings = await invoke('get_settings');
  applySettings(settings);
}

async function loadPipelineModels() {
  try {
    const models = await invoke('get_pipeline_models');
    el.ocrModel.value = models.ocr;
    el.inpaintingModel.value = models.inpainting;
  } catch (err) {
    el.pipelineModelsFeedback.classList.add('error');
    el.pipelineModelsFeedback.textContent = `Could not read Koharu's current model selection: ${err}`;
  }
}

el.savePipelineModelsBtn.addEventListener('click', async () => {
  el.pipelineModelsFeedback.classList.remove('error');
  el.pipelineModelsFeedback.textContent = 'Saving…';
  try {
    await invoke('set_pipeline_models', { ocr: el.ocrModel.value, inpainting: el.inpaintingModel.value });
    el.pipelineModelsFeedback.textContent = 'Saved. The next translation job will use it.';
  } catch (err) {
    el.pipelineModelsFeedback.classList.add('error');
    el.pipelineModelsFeedback.textContent = `Could not save: ${err}`;
  }
  setTimeout(() => {
    el.pipelineModelsFeedback.textContent = '';
  }, 3000);
});

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
    el.resourceText.textContent = `CPU ${Math.round(u.cpuPercent)}%${cpuCap} · RAM ${u.usedMemoryMb}/${u.totalMemoryMb}MB${ramCap}`;
    el.resourceDot.style.background = u.cpuPercent >= 90 ? '#dc2626' : u.cpuPercent >= 70 ? '#f59e0b' : '#16a34a';
  } catch {
    el.resourceText.textContent = 'CPU / RAM: unavailable';
  }
}

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

load();
loadPipelineModels();
refreshUsage();
setInterval(refreshUsage, 5000);
refreshResourceUsage();
setInterval(refreshResourceUsage, 2000);
