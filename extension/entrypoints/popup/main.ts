import browser from 'webextension-polyfill';
import { getSettings, saveSettings, setOriginEnabled, type Settings } from '../../lib/settings';

const openSettingsBtn = document.getElementById('openSettings') as HTMLButtonElement;
const backToMainBtn = document.getElementById('backToMain') as HTMLButtonElement;
const setupViewEl = document.getElementById('setupView') as HTMLElement;
const mainViewEl = document.getElementById('mainView') as HTMLElement;
const settingsViewEl = document.getElementById('settingsView') as HTMLElement;

const connectionDotEl = document.getElementById('connectionDot') as HTMLSpanElement;
const connectionTextEl = document.getElementById('connectionText') as HTMLSpanElement;

const setupServerUrlEl = document.getElementById('setupServerUrl') as HTMLInputElement;
const setupTargetLangEl = document.getElementById('setupTargetLang') as HTMLSelectElement;
const setupSaveBtn = document.getElementById('setupSave') as HTMLButtonElement;
const setupStatusEl = document.getElementById('setupStatus') as HTMLDivElement;

const currentSiteEl = document.getElementById('currentSite') as HTMLDivElement;
const siteEnabledEl = document.getElementById('siteEnabled') as HTMLInputElement;
const translateNowBtn = document.getElementById('translateNow') as HTMLButtonElement;
const manualStatusEl = document.getElementById('manualStatus') as HTMLDivElement;
const resourceUsageEl = document.getElementById('resourceUsage') as HTMLDivElement;

const serverUrlEl = document.getElementById('serverUrl') as HTMLInputElement;
const targetLangEl = document.getElementById('targetLang') as HTMLSelectElement;
const saveBtn = document.getElementById('save') as HTMLButtonElement;
const statusEl = document.getElementById('status') as HTMLDivElement;

let activeTab: browser.Tabs.Tab | undefined;
let activeOrigin: string | null = null;
let settingsCache: Settings;

async function getActiveTab(): Promise<browser.Tabs.Tab | undefined> {
  const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
  return tab;
}

function originOf(url: string | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

type View = 'setup' | 'main' | 'settings';

function showView(view: View) {
  setupViewEl.hidden = view !== 'setup';
  mainViewEl.hidden = view !== 'main';
  settingsViewEl.hidden = view !== 'settings';
  // The first-run setup view already has these two fields front and
  // center -- no need for a gear icon back into itself.
  openSettingsBtn.hidden = view === 'setup';
}

async function load() {
  activeTab = await getActiveTab();
  activeOrigin = originOf(activeTab?.url);

  if (!activeOrigin) {
    currentSiteEl.textContent = 'Not a regular web page';
    siteEnabledEl.disabled = true;
    translateNowBtn.disabled = true;
  } else {
    currentSiteEl.textContent = activeOrigin;
  }

  settingsCache = await getSettings();
  setupServerUrlEl.value = settingsCache.serverUrl;
  setupTargetLangEl.value = settingsCache.targetLang;
  serverUrlEl.value = settingsCache.serverUrl;
  targetLangEl.value = settingsCache.targetLang;
  siteEnabledEl.checked = !!activeOrigin && settingsCache.enabledOrigins.includes(activeOrigin);

  showView(settingsCache.hasCompletedSetup ? 'main' : 'setup');
}

openSettingsBtn.addEventListener('click', () => showView('settings'));
backToMainBtn.addEventListener('click', () => showView('main'));

siteEnabledEl.addEventListener('change', async () => {
  if (!activeOrigin) return;
  await setOriginEnabled(activeOrigin, siteEnabledEl.checked);
});

translateNowBtn.addEventListener('click', async () => {
  if (!activeTab?.id) return;
  manualStatusEl.textContent = 'Scanning this page...';
  try {
    await browser.tabs.sendMessage(activeTab.id, { type: 'TRANSLATE_PAGE_NOW' });
    manualStatusEl.textContent = 'Started -- watch the page for progress.';
  } catch (err) {
    manualStatusEl.textContent = `Could not reach this page: ${err instanceof Error ? err.message : String(err)}`;
  }
  setTimeout(() => (manualStatusEl.textContent = ''), 3000);
});

interface DownloadStatus {
  active: boolean;
  name: string;
  completedBytes: number;
  totalBytes: number;
  percent: number;
}

interface ResourceUsage {
  cpuPercent: number;
  processCpuPercent?: number;
  processMemoryMb?: number;
  systemUsedMemoryMb?: number;
  systemTotalMemoryMb?: number;
  usedMemoryMb: number;
  totalMemoryMb: number;
  gpuName?: string;
  vramUsedMb?: number;
  vramTotalMb?: number;
  modelsLoaded?: boolean;
  activeJobs?: number;
  maxCpuThreads: number;
  maxMemoryMb: number;
  download?: DownloadStatus | null;
}

const popupUnloadBtn = document.getElementById('popupUnloadBtn') as HTMLButtonElement | null;
const popupUnloadFeedback = document.getElementById('popupUnloadFeedback') as HTMLDivElement | null;
const popupDownloadBanner = document.getElementById('popupDownloadBanner') as HTMLDivElement | null;
const popupDownloadTitle = document.getElementById('popupDownloadTitle') as HTMLSpanElement | null;
const popupDownloadPercent = document.getElementById('popupDownloadPercent') as HTMLSpanElement | null;
const popupDownloadBar = document.getElementById('popupDownloadBar') as HTMLDivElement | null;

popupUnloadBtn?.addEventListener('click', async () => {
  const base = (settingsCache?.serverUrl.trim() || 'http://127.0.0.1:8000').replace(/\/+$/, '');
  popupUnloadBtn.disabled = true;
  popupUnloadBtn.textContent = 'Liberando memória…';
  try {
    const res = await fetch(`${base}/api/pipeline/unload`, { method: 'POST' });
    if (res.ok) {
      if (popupUnloadFeedback) popupUnloadFeedback.textContent = 'Memória liberada com sucesso!';
      setTimeout(() => {
        if (popupUnloadFeedback) popupUnloadFeedback.textContent = '';
      }, 3000);
      await refreshResourceUsage();
    }
  } catch (err) {
    if (popupUnloadFeedback) popupUnloadFeedback.textContent = `Erro ao liberar: ${err}`;
  } finally {
    popupUnloadBtn.disabled = false;
    popupUnloadBtn.textContent = 'Liberar Memória dos Modelos';
  }
});

function setConnected(connected: boolean) {
  connectionDotEl.style.background = connected ? '#4ade80' : '#f87171';
  connectionTextEl.textContent = connected ? 'Connected to desktop app' : 'Not connected -- is the server running?';
}

// Polls the desktop app's own /api/resources (http_server.rs) -- same
// live reading the desktop settings window shows, just reachable from
// the extension without the desktop window needing to be open. Doubles
// as the connection check: a successful response means the extension
// can actually reach the app right now, a failed one means it can't
// (server not started, wrong port, app not running, etc).
async function refreshResourceUsage() {
  const base = (settingsCache?.serverUrl.trim() || 'http://127.0.0.1:8000').replace(/\/+$/, '');
  try {
    const res = await fetch(`${base}/api/resources`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const u = (await res.json()) as ResourceUsage;
    setConnected(true);
    const cpuCap = u.maxCpuThreads > 0 ? ` (cap ${u.maxCpuThreads}t)` : '';
    const procMem = u.processMemoryMb != null ? `${u.processMemoryMb} MB` : `${u.usedMemoryMb} MB`;
    const sysUsed = u.systemUsedMemoryMb ?? u.usedMemoryMb;
    const sysTotal = u.systemTotalMemoryMb ?? u.totalMemoryMb;
    const gpuText = u.gpuName
      ? `\nGPU (${u.gpuName}): ${u.vramUsedMb || 0}/${u.vramTotalMb || 0} MB`
      : u.vramUsedMb != null
        ? `\nVRAM: ${u.vramUsedMb}/${u.vramTotalMb || 0} MB`
        : '';
    const modelStatus = u.modelsLoaded ? 'Modelos em RAM: Ativos' : 'Modelos: Desalocados (Idle)';
    const jobsInfo = u.activeJobs && u.activeJobs > 0 ? ` (${u.activeJobs} ativo)` : '';

    resourceUsageEl.textContent = `CPU ${Math.round(u.cpuPercent)}%${cpuCap}\nApp RAM: ${procMem} · Sistema: ${sysUsed}/${sysTotal} MB${gpuText}\nStatus: ${modelStatus}${jobsInfo}`;

    if (popupUnloadBtn) {
      popupUnloadBtn.style.display = u.modelsLoaded ? 'block' : 'none';
    }

    if (popupDownloadBanner) {
      if (u.download && u.download.active) {
        popupDownloadBanner.style.display = 'block';
        if (popupDownloadTitle) popupDownloadTitle.textContent = u.download.name;
        const pct = Math.round(u.download.percent || 0);
        if (popupDownloadPercent) popupDownloadPercent.textContent = `${pct}%`;
        if (popupDownloadBar) popupDownloadBar.style.width = `${pct}%`;
      } else {
        popupDownloadBanner.style.display = 'none';
      }
    }
  } catch {
    setConnected(false);
    resourceUsageEl.textContent = 'Desktop app not reachable.';
    if (popupUnloadBtn) popupUnloadBtn.style.display = 'none';
    if (popupDownloadBanner) popupDownloadBanner.style.display = 'none';
  }
}

setupSaveBtn.addEventListener('click', async () => {
  const next: Settings = {
    ...settingsCache,
    serverUrl: setupServerUrlEl.value.trim() || 'http://127.0.0.1:8000',
    targetLang: setupTargetLangEl.value,
    hasCompletedSetup: true,
  };
  await saveSettings(next);
  settingsCache = next;
  serverUrlEl.value = next.serverUrl;
  targetLangEl.value = next.targetLang;
  setupStatusEl.textContent = '';
  showView('main');
  refreshResourceUsage();
});

saveBtn.addEventListener('click', async () => {
  const next: Settings = {
    ...settingsCache,
    serverUrl: serverUrlEl.value.trim() || 'http://127.0.0.1:8000',
    targetLang: targetLangEl.value,
  };
  await saveSettings(next);
  settingsCache = next;
  statusEl.textContent = 'Saved.';
  setTimeout(() => (statusEl.textContent = ''), 1500);
  refreshResourceUsage();
});

void load().then(() => {
  refreshResourceUsage();
  setInterval(refreshResourceUsage, 2000);
});
