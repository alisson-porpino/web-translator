import browser from 'webextension-polyfill';
import { getSettings, saveSettings, setOriginEnabled, type Settings } from '../../lib/settings';
import { getI18n, type SupportedLang } from '../../lib/i18n';

const openSettingsBtn = document.getElementById('openSettings') as HTMLButtonElement;
const backToMainBtn = document.getElementById('backToMain') as HTMLButtonElement;
const setupViewEl = document.getElementById('setupView') as HTMLElement;
const mainViewEl = document.getElementById('mainView') as HTMLElement;
const settingsViewEl = document.getElementById('settingsView') as HTMLElement;

const connectionDotEl = document.getElementById('connectionDot') as HTMLSpanElement;
const connectionTextEl = document.getElementById('connectionText') as HTMLSpanElement;

const setupServerUrlEl = document.getElementById('setupServerUrl') as HTMLInputElement;
const setupTargetLangEl = document.getElementById('setupTargetLang') as HTMLSelectElement;
const setupUiLangEl = document.getElementById('setupUiLang') as HTMLSelectElement;
const setupSaveBtn = document.getElementById('setupSave') as HTMLButtonElement;
const setupStatusEl = document.getElementById('setupStatus') as HTMLDivElement;

const currentSiteEl = document.getElementById('currentSite') as HTMLDivElement;
const siteEnabledEl = document.getElementById('siteEnabled') as HTMLInputElement;
const translateNowBtn = document.getElementById('translateNow') as HTMLButtonElement;
const manualStatusEl = document.getElementById('manualStatus') as HTMLDivElement;
const resourceUsageEl = document.getElementById('resourceUsage') as HTMLDivElement;

const serverUrlEl = document.getElementById('serverUrl') as HTMLInputElement;
const targetLangEl = document.getElementById('targetLang') as HTMLSelectElement;
const uiLangEl = document.getElementById('uiLang') as HTMLSelectElement;
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

function t() {
  return getI18n(settingsCache?.uiLang);
}

function applyI18n(lang: SupportedLang) {
  const dict = getI18n(lang);
  document.documentElement.lang = lang === 'pt' ? 'pt-BR' : lang;

  openSettingsBtn.title = dict.settingsTitle;
  openSettingsBtn.setAttribute('aria-label', dict.settingsTitle);

  const setupHint = document.querySelector('#setupView p.hint');
  if (setupHint) setupHint.textContent = dict.setupHint;
  const setupUrlLbl = document.querySelector('label[for="setupServerUrl"]');
  if (setupUrlLbl) setupUrlLbl.textContent = dict.desktopServerUrl;
  const setupTargetLbl = document.getElementById('setupTargetLangLabel');
  if (setupTargetLbl) setupTargetLbl.textContent = dict.targetLangLabel;
  const setupUiLbl = document.getElementById('setupUiLangLabel');
  if (setupUiLbl) setupUiLbl.textContent = dict.uiLangLabel;
  setupSaveBtn.textContent = dict.getStarted;

  const thisTabH2 = document.querySelector('#mainView h2:first-of-type');
  if (thisTabH2) thisTabH2.textContent = dict.thisTab;
  const toggleRowSpan = document.querySelector('.toggle-row span');
  if (toggleRowSpan) toggleRowSpan.textContent = dict.autoTranslateSite;
  const autoHint = document.querySelector('#mainView p.hint');
  if (autoHint) autoHint.textContent = dict.autoTranslateHint;
  translateNowBtn.textContent = dict.translatePageNow;

  const resH2 = document.querySelector('#mainView h2:nth-of-type(2)');
  if (resH2) resH2.textContent = dict.desktopResources;
  if (popupUnloadBtn) popupUnloadBtn.textContent = dict.freeMemoryBtn;

  const setUrlLbl = document.querySelector('label[for="serverUrl"]');
  if (setUrlLbl) setUrlLbl.textContent = dict.desktopServerUrl;
  const setHint = document.querySelector('#settingsView p.hint');
  if (setHint) setHint.textContent = dict.settingsHint;
  const targetLbl = document.getElementById('targetLangLabel');
  if (targetLbl) targetLbl.textContent = dict.targetLangLabel;
  const uiLbl = document.getElementById('uiLangLabel');
  if (uiLbl) uiLbl.textContent = dict.uiLangLabel;
  saveBtn.textContent = dict.save;
  backToMainBtn.textContent = dict.back;
}

async function load() {
  activeTab = await getActiveTab();
  activeOrigin = originOf(activeTab?.url);

  settingsCache = await getSettings();
  applyI18n(settingsCache.uiLang || 'pt');

  if (!activeOrigin) {
    currentSiteEl.textContent = t().notRegularPage;
    siteEnabledEl.disabled = true;
    translateNowBtn.disabled = true;
  } else {
    currentSiteEl.textContent = activeOrigin;
  }

  setupServerUrlEl.value = settingsCache.serverUrl;
  setupTargetLangEl.value = settingsCache.targetLang;
  if (setupUiLangEl) setupUiLangEl.value = settingsCache.uiLang || 'pt';
  serverUrlEl.value = settingsCache.serverUrl;
  targetLangEl.value = settingsCache.targetLang;
  if (uiLangEl) uiLangEl.value = settingsCache.uiLang || 'pt';
  siteEnabledEl.checked = !!activeOrigin && settingsCache.enabledOrigins.includes(activeOrigin);

  showView(settingsCache.hasCompletedSetup ? 'main' : 'setup');
}

setupUiLangEl?.addEventListener('change', () => {
  const lang = setupUiLangEl.value as SupportedLang;
  if (uiLangEl) uiLangEl.value = lang;
  settingsCache.uiLang = lang;
  applyI18n(lang);
});

uiLangEl?.addEventListener('change', () => {
  const lang = uiLangEl.value as SupportedLang;
  if (setupUiLangEl) setupUiLangEl.value = lang;
  settingsCache.uiLang = lang;
  applyI18n(lang);
});

openSettingsBtn.addEventListener('click', () => showView('settings'));
backToMainBtn.addEventListener('click', () => showView('main'));

siteEnabledEl.addEventListener('change', async () => {
  if (!activeOrigin) return;
  await setOriginEnabled(activeOrigin, siteEnabledEl.checked);
});

translateNowBtn.addEventListener('click', async () => {
  if (!activeTab?.id) return;
  manualStatusEl.textContent = t().scanningPage;
  try {
    await browser.tabs.sendMessage(activeTab.id, { type: 'TRANSLATE_PAGE_NOW' });
    manualStatusEl.textContent = t().scanningStarted;
  } catch (err) {
    manualStatusEl.textContent = t().couldNotReachPage(err instanceof Error ? err.message : String(err));
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
  popupUnloadBtn.textContent = t().freeingMemory;
  try {
    const res = await fetch(`${base}/api/pipeline/unload`, { method: 'POST' });
    if (res.ok) {
      if (popupUnloadFeedback) popupUnloadFeedback.textContent = t().memoryFreed;
      setTimeout(() => {
        if (popupUnloadFeedback) popupUnloadFeedback.textContent = '';
      }, 3000);
      await refreshResourceUsage();
    }
  } catch (err) {
    if (popupUnloadFeedback) popupUnloadFeedback.textContent = t().freeMemoryError(String(err));
  } finally {
    popupUnloadBtn.disabled = false;
    popupUnloadBtn.textContent = t().freeMemoryBtn;
  }
});

function setConnected(connected: boolean) {
  connectionDotEl.style.background = connected ? '#4ade80' : '#f87171';
  connectionTextEl.textContent = connected ? t().connected : t().notConnected;
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
    const modelStatus = u.modelsLoaded ? t().modelsActive : t().modelsIdle;
    const jobsInfo = u.activeJobs && u.activeJobs > 0 ? t().activeJobs(u.activeJobs) : '';

    resourceUsageEl.textContent = `CPU ${Math.round(u.cpuPercent)}%${cpuCap}\n${t().appRam}: ${procMem} · ${t().sysRam}: ${sysUsed}/${sysTotal} MB${gpuText}\nStatus: ${modelStatus}${jobsInfo}`;

    if (popupUnloadBtn) {
      popupUnloadBtn.style.display = 'block';
      if (u.modelsLoaded) {
        popupUnloadBtn.disabled = false;
        popupUnloadBtn.textContent = t().freeMemoryBtn;
        popupUnloadBtn.style.opacity = '1';
        popupUnloadBtn.style.cursor = 'pointer';
      } else {
        popupUnloadBtn.disabled = true;
        popupUnloadBtn.textContent = t().memoryFreed;
        popupUnloadBtn.style.opacity = '0.6';
        popupUnloadBtn.style.cursor = 'default';
      }
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
    resourceUsageEl.textContent = t().appUnreachable;
    if (popupUnloadBtn) {
      popupUnloadBtn.disabled = true;
      popupUnloadBtn.style.opacity = '0.5';
    }
    if (popupDownloadBanner) popupDownloadBanner.style.display = 'none';
  }
}

setupSaveBtn.addEventListener('click', async () => {
  const chosenUiLang = (setupUiLangEl?.value || 'pt') as SupportedLang;
  const next: Settings = {
    ...settingsCache,
    serverUrl: setupServerUrlEl.value.trim() || 'http://127.0.0.1:8000',
    targetLang: setupTargetLangEl.value,
    uiLang: chosenUiLang,
    hasCompletedSetup: true,
  };
  await saveSettings(next);
  settingsCache = next;
  serverUrlEl.value = next.serverUrl;
  targetLangEl.value = next.targetLang;
  if (uiLangEl) uiLangEl.value = chosenUiLang;
  setupStatusEl.textContent = '';
  applyI18n(chosenUiLang);
  showView('main');
  refreshResourceUsage();
});

saveBtn.addEventListener('click', async () => {
  const chosenUiLang = (uiLangEl?.value || 'pt') as SupportedLang;
  const next: Settings = {
    ...settingsCache,
    serverUrl: serverUrlEl.value.trim() || 'http://127.0.0.1:8000',
    targetLang: targetLangEl.value,
    uiLang: chosenUiLang,
  };
  await saveSettings(next);
  settingsCache = next;
  applyI18n(chosenUiLang);
  statusEl.textContent = t().saved;
  setTimeout(() => (statusEl.textContent = ''), 1500);
  refreshResourceUsage();
});

void load().then(() => {
  refreshResourceUsage();
  setInterval(refreshResourceUsage, 2000);
});
