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
  translationModel: document.getElementById('translationModel'),
  translationQuantization: document.getElementById('translationQuantization'),
  savePipelineModelsBtn: document.getElementById('savePipelineModelsBtn'),
  pipelineModelsFeedback: document.getElementById('pipelineModelsFeedback'),
  feedback: document.getElementById('feedback'),
  form: document.getElementById('settingsForm'),
  usageText: document.getElementById('usageText'),
  usageDot: document.querySelector('#usageStatus .dot'),
  resourceText: document.getElementById('resourceText'),
  resourceDetailText: document.getElementById('resourceDetailText'),
  resourceDot: document.querySelector('#resourceStatus .dot'),
  unloadModelsBtn: document.getElementById('unloadModelsBtn'),
  downloadBanner: document.getElementById('downloadBanner'),
  downloadTitle: document.getElementById('downloadTitle'),
  downloadPercent: document.getElementById('downloadPercent'),
  downloadBarFill: document.getElementById('downloadBarFill'),
  serverDot: document.getElementById('serverDot'),
  serverStatusText: document.getElementById('serverStatusText'),
  serverToggleBtn: document.getElementById('serverToggleBtn'),
  serverError: document.getElementById('serverError'),
  installChromeBtn: document.getElementById('installChromeBtn'),
  installFirefoxBtn: document.getElementById('installFirefoxBtn'),
  extensionInstallFeedback: document.getElementById('extensionInstallFeedback'),
  extensionInstallSteps: document.getElementById('extensionInstallSteps'),
  extensionStepChrome: document.getElementById('extensionStepChrome'),
  extensionStepFirefox: document.getElementById('extensionStepFirefox'),
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
    input.placeholder = i === 0 ? 'Caminho da fonte principal (.ttf/.otf)' : 'Caminho da fonte secundária (.ttf/.otf)';
    input.value = path;
    input.addEventListener('input', () => {
      currentFontPaths[i] = input.value;
    });
    row.appendChild(input);

    const up = document.createElement('button');
    up.type = 'button';
    up.textContent = '↑';
    up.title = 'Mover para cima (prioridade maior)';
    up.disabled = i === 0;
    up.addEventListener('click', () => {
      [currentFontPaths[i - 1], currentFontPaths[i]] = [currentFontPaths[i], currentFontPaths[i - 1]];
      renderFontStack();
    });
    row.appendChild(up);

    const down = document.createElement('button');
    down.type = 'button';
    down.textContent = '↓';
    down.title = 'Mover para baixo (prioridade menor)';
    down.disabled = i === currentFontPaths.length - 1;
    down.addEventListener('click', () => {
      [currentFontPaths[i], currentFontPaths[i + 1]] = [currentFontPaths[i + 1], currentFontPaths[i]];
      renderFontStack();
    });
    row.appendChild(down);

    const remove = document.createElement('button');
    remove.type = 'button';
    remove.textContent = '×';
    remove.title = 'Remover';
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
    el.feedback.textContent = `Não foi possível abrir o seletor de pastas: ${err}`;
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

// The local server (http_server.rs) never starts on its own -- a native
// app on this machine could already own the configured port, and
// silently rebinding it on every launch would fail with no visible
// feedback. The user starts/stops it explicitly here instead.
async function refreshServerStatus() {
  try {
    const status = await invoke('get_server_status');
    if (status.running) {
      el.serverDot.style.background = 'var(--ok)';
      el.serverStatusText.textContent = `Servidor ativo em http://127.0.0.1:${status.port}`;
      el.serverToggleBtn.textContent = 'Parar';
    } else {
      el.serverDot.style.background = '#9a9a9a';
      el.serverStatusText.textContent = 'Servidor parado';
      el.serverToggleBtn.textContent = 'Iniciar';
    }
  } catch {
    el.serverDot.style.background = 'var(--bad)';
    el.serverStatusText.textContent = 'Status do servidor indisponível';
  }
}

el.serverToggleBtn.addEventListener('click', async () => {
  el.serverError.hidden = true;
  el.serverToggleBtn.disabled = true;
  try {
    const status = await invoke('get_server_status');
    if (status.running) {
      await invoke('stop_server');
    } else {
      await invoke('start_server');
    }
  } catch (err) {
    el.serverError.hidden = false;
    el.serverError.textContent = String(err);
  }
  el.serverToggleBtn.disabled = false;
  await refreshServerStatus();
});

async function installExtension(target) {
  el.extensionInstallFeedback.classList.remove('error');
  el.extensionInstallFeedback.textContent = 'Abrindo…';
  el.extensionInstallSteps.hidden = true;
  el.extensionStepChrome.hidden = true;
  el.extensionStepFirefox.hidden = true;
  try {
    await invoke('install_extension', { target });
    el.extensionInstallFeedback.textContent = 'Pasta aberta — siga os passos abaixo.';
    el.extensionInstallSteps.hidden = false;
    (target === 'chrome' ? el.extensionStepChrome : el.extensionStepFirefox).hidden = false;
  } catch (err) {
    el.extensionInstallFeedback.classList.add('error');
    el.extensionInstallFeedback.textContent = String(err);
  }
}

el.installChromeBtn.addEventListener('click', () => installExtension('chrome'));
el.installFirefoxBtn.addEventListener('click', () => installExtension('firefox'));

async function load() {
  const settings = await invoke('get_settings');
  applySettings(settings);
}

// Koharu's full local translation catalog (~27 models as of the pinned
// rev), fetched once and kept here so the quantization dropdown can be
// re-populated whenever the model dropdown changes, without a round trip.
let translationModels = [];

function renderQuantizationOptions(modelId) {
  const model = translationModels.find((m) => m.model === modelId);
  el.translationQuantization.innerHTML = '';
  for (const q of model?.quantizations ?? []) {
    const opt = document.createElement('option');
    opt.value = q.id;
    opt.textContent = q.downloaded ? q.name : `${q.name} (será baixado)`;
    el.translationQuantization.appendChild(opt);
  }
}

el.translationModel.addEventListener('change', () => {
  renderQuantizationOptions(el.translationModel.value);
});

async function loadPipelineModels() {
  try {
    const models = await invoke('get_pipeline_models');
    el.ocrModel.value = models.ocr;
    el.inpaintingModel.value = models.inpainting;
  } catch (err) {
    el.pipelineModelsFeedback.classList.add('error');
    el.pipelineModelsFeedback.textContent = `Não foi possível ler a seleção de modelos do Koharu: ${err}`;
  }

  try {
    const [models, selection] = await Promise.all([
      invoke('get_translation_models'),
      invoke('get_translation_selection'),
    ]);
    translationModels = models;
    el.translationModel.innerHTML = '';
    for (const m of models) {
      const opt = document.createElement('option');
      opt.value = m.model;
      opt.textContent = m.name;
      el.translationModel.appendChild(opt);
    }
    if (selection.model && models.some((m) => m.model === selection.model)) {
      el.translationModel.value = selection.model;
    }
    renderQuantizationOptions(el.translationModel.value);
    if (selection.quantization) {
      el.translationQuantization.value = selection.quantization;
    }
  } catch (err) {
    el.translationModel.innerHTML = '<option value="">Não foi possível carregar</option>';
    el.pipelineModelsFeedback.classList.add('error');
    el.pipelineModelsFeedback.textContent = `Não foi possível ler o catálogo de tradução do Koharu: ${err}`;
  }
}

el.savePipelineModelsBtn.addEventListener('click', async () => {
  el.pipelineModelsFeedback.classList.remove('error');
  el.pipelineModelsFeedback.textContent = 'Salvando…';
  try {
    await invoke('set_pipeline_models', { ocr: el.ocrModel.value, inpainting: el.inpaintingModel.value });
    if (el.translationModel.value) {
      await invoke('set_translation_model', {
        model: el.translationModel.value,
        quantization: el.translationQuantization.value || null,
      });
    }
    el.pipelineModelsFeedback.textContent = 'Salvo com sucesso! O próximo trabalho de tradução utilizará esta configuração.';
  } catch (err) {
    el.pipelineModelsFeedback.classList.add('error');
    el.pipelineModelsFeedback.textContent = `Não foi possível salvar: ${err}`;
  }
  setTimeout(() => {
    el.pipelineModelsFeedback.textContent = '';
  }, 3000);
});

async function refreshUsage() {
  try {
    const { count, cap } = await invoke('get_google_usage');
    el.usageText.textContent = cap > 0 ? `Uso diário do Google: ${count}/${cap} requisições` : `Uso diário do Google: ${count} requisições (sem limite)`;
    el.usageDot.style.background = cap > 0 && count >= cap ? '#dc2626' : cap > 0 && count >= cap * 0.8 ? '#f59e0b' : '#16a34a';
  } catch {
    el.usageText.textContent = 'Uso diário do Google: indisponível';
  }
}

async function refreshResourceUsage() {
  try {
    const u = await invoke('get_resource_usage');
    const cpuCap = u.maxCpuThreads > 0 ? ` (cap ${u.maxCpuThreads}t)` : '';
    const procMem = u.processMemoryMb != null ? `${u.processMemoryMb} MB` : `${u.usedMemoryMb} MB`;
    const gpuText = u.gpuName
      ? ` · GPU (${u.gpuName}): ${u.vramUsedMb || 0}/${u.vramTotalMb || 0} MB`
      : u.vramUsedMb != null
        ? ` · VRAM: ${u.vramUsedMb}/${u.vramTotalMb || 0} MB`
        : '';

    el.resourceText.textContent = `CPU ${Math.round(u.cpuPercent)}%${cpuCap} · RAM do App: ${procMem}${gpuText}`;

    const sysUsed = u.systemUsedMemoryMb ?? u.usedMemoryMb;
    const sysTotal = u.systemTotalMemoryMb ?? u.totalMemoryMb;
    const modelStatus = u.modelsLoaded ? 'Modelos em RAM: Ativos' : 'Modelos: Desalocados (Em espera)';
    const jobsInfo = u.activeJobs > 0 ? ` · ${u.activeJobs} tarefa(s) em processamento` : '';
    if (el.resourceDetailText) {
      el.resourceDetailText.textContent = `RAM do Sistema: ${sysUsed}/${sysTotal} MB · ${modelStatus}${jobsInfo}`;
    }

    el.resourceDot.style.background = u.modelsLoaded ? '#3b82f6' : '#16a34a';
    if (el.unloadModelsBtn) {
      el.unloadModelsBtn.style.display = u.modelsLoaded ? 'inline-block' : 'none';
    }

    // Active download progress
    if (el.downloadBanner) {
      if (u.download && u.download.active) {
        el.downloadBanner.style.display = 'block';
        el.downloadTitle.textContent = `Baixando modelo: ${u.download.name}`;
        const pct = Math.round(u.download.percent || 0);
        el.downloadPercent.textContent = `${pct}%`;
        el.downloadBarFill.style.width = `${pct}%`;
      } else {
        el.downloadBanner.style.display = 'none';
      }
    }
  } catch {
    el.resourceText.textContent = 'CPU / RAM: indisponível';
    if (el.unloadModelsBtn) el.unloadModelsBtn.style.display = 'none';
    if (el.downloadBanner) el.downloadBanner.style.display = 'none';
  }
}

if (el.unloadModelsBtn) {
  el.unloadModelsBtn.addEventListener('click', async () => {
    el.unloadModelsBtn.disabled = true;
    el.unloadModelsBtn.textContent = 'Liberando…';
    try {
      await invoke('unload_models');
      await refreshResourceUsage();
    } catch (err) {
      console.error('Failed to unload models', err);
    } finally {
      el.unloadModelsBtn.disabled = false;
      el.unloadModelsBtn.textContent = 'Liberar Memória';
    }
  });
}

el.form.addEventListener('submit', async (e) => {
  e.preventDefault();
  el.feedback.classList.remove('error');
  el.feedback.textContent = 'Salvando…';
  try {
    await invoke('save_settings', { settings: readSettings() });
    el.feedback.textContent = 'Salvo com sucesso! As novas configurações já estão valendo.';
  } catch (err) {
    el.feedback.classList.add('error');
    el.feedback.textContent = `Não foi possível salvar: ${err}`;
  }
  setTimeout(() => {
    el.feedback.textContent = '';
  }, 3000);
  refreshUsage();
});

load();
loadPipelineModels();
refreshServerStatus();
setInterval(refreshServerStatus, 3000);
refreshUsage();
setInterval(refreshUsage, 5000);
refreshResourceUsage();
setInterval(refreshResourceUsage, 2000);
