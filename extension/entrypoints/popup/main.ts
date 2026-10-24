import browser from 'webextension-polyfill';
import { getSettings, saveSettings, setOriginEnabled } from '../../lib/settings';

const currentSiteEl = document.getElementById('currentSite') as HTMLDivElement;
const siteEnabledEl = document.getElementById('siteEnabled') as HTMLInputElement;
const translateNowBtn = document.getElementById('translateNow') as HTMLButtonElement;
const manualStatusEl = document.getElementById('manualStatus') as HTMLDivElement;

const serverUrlEl = document.getElementById('serverUrl') as HTMLInputElement;
const targetLangEl = document.getElementById('targetLang') as HTMLSelectElement;
const saveBtn = document.getElementById('save') as HTMLButtonElement;
const statusEl = document.getElementById('status') as HTMLDivElement;
const resourceUsageEl = document.getElementById('resourceUsage') as HTMLDivElement;

let activeTab: browser.Tabs.Tab | undefined;
let activeOrigin: string | null = null;

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

  const settings = await getSettings();
  serverUrlEl.value = settings.serverUrl;
  targetLangEl.value = settings.targetLang;
  siteEnabledEl.checked = !!activeOrigin && settings.enabledOrigins.includes(activeOrigin);
}

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

interface ResourceUsage {
  cpuPercent: number;
  usedMemoryMb: number;
  totalMemoryMb: number;
  maxCpuThreads: number;
  maxMemoryMb: number;
  maxVramMb: number;
}

// Polls the desktop app's own /api/resources (http_server.rs) -- same
// live reading the desktop settings window shows, just reachable from
// the extension without the desktop window needing to be open.
async function refreshResourceUsage() {
  const base = (serverUrlEl.value.trim() || 'http://127.0.0.1:8000').replace(/\/+$/, '');
  try {
    const res = await fetch(`${base}/api/resources`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const u = (await res.json()) as ResourceUsage;
    const cpuCap = u.maxCpuThreads > 0 ? ` (cap ${u.maxCpuThreads} threads)` : '';
    const ramCap = u.maxMemoryMb > 0 ? ` / cap ${u.maxMemoryMb}MB` : '';
    const vram = u.maxVramMb > 0 ? ` · VRAM budget ${u.maxVramMb}MB` : '';
    resourceUsageEl.textContent =
      `CPU ${Math.round(u.cpuPercent)}%${cpuCap}\nRAM ${u.usedMemoryMb}/${u.totalMemoryMb}MB${ramCap}${vram}`;
  } catch {
    resourceUsageEl.textContent = 'Desktop app not reachable.';
  }
}

saveBtn.addEventListener('click', async () => {
  const settings = await getSettings();
  await saveSettings({
    ...settings,
    serverUrl: serverUrlEl.value.trim() || 'http://127.0.0.1:8000',
    targetLang: targetLangEl.value,
  });
  statusEl.textContent = 'Saved.';
  setTimeout(() => (statusEl.textContent = ''), 1500);
});

void load().then(() => {
  refreshResourceUsage();
  setInterval(refreshResourceUsage, 2000);
});
