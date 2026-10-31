import browser from 'webextension-polyfill';

export interface Settings {
  /** Base URL of the companion desktop app's local server (desktop/ in this repo). */
  serverUrl: string;
  targetLang: string;
  /** UI language for extension popup and in-page overlays ('pt' | 'en' | 'es'). */
  uiLang: 'pt' | 'en' | 'es';
  /**
   * Origins (e.g. "https://mangadex.org") where auto-translate is allowed to
   * run on its own as the page loads/changes. Everywhere else, nothing runs
   * automatically -- translation only happens when the user explicitly asks
   * for it (popup button or keyboard shortcut), scoped to the active tab.
   * Empty by default: opt-in per site, never blanket-on across every tab.
   */
  enabledOrigins: string[];
  /**
   * Whether the popup's first-run connection setup (server URL + target
   * language) has been completed at least once. Gates which view
   * popup/main.ts shows by default -- false shows the setup view every
   * time the popup opens; true shows the main (translate) view, with
   * those same two fields moved behind the gear-icon settings view
   * instead. Flips to true the first time the setup view's Save button
   * is used.
   */
  hasCompletedSetup: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  serverUrl: 'http://127.0.0.1:8000',
  targetLang: 'pt',
  uiLang: (typeof navigator !== 'undefined' && navigator.language?.toLowerCase().startsWith('pt')) ? 'pt' : 'en',
  enabledOrigins: [],
  hasCompletedSetup: false,
};

const STORAGE_KEY = 'mwt:settings';

export async function getSettings(): Promise<Settings> {
  const stored = await browser.storage.local.get(STORAGE_KEY);
  const value = stored[STORAGE_KEY] as Partial<Settings> | undefined;
  return { ...DEFAULT_SETTINGS, ...value };
}

export async function saveSettings(settings: Settings): Promise<void> {
  await browser.storage.local.set({ [STORAGE_KEY]: settings });
}

export function onSettingsChanged(cb: (settings: Settings) => void): () => void {
  const listener = (changes: Record<string, browser.Storage.StorageChange>) => {
    if (STORAGE_KEY in changes) {
      cb({ ...DEFAULT_SETTINGS, ...(changes[STORAGE_KEY].newValue as Partial<Settings>) });
    }
  };
  browser.storage.onChanged.addListener(listener);
  return () => browser.storage.onChanged.removeListener(listener);
}

export async function setOriginEnabled(origin: string, enabled: boolean): Promise<Settings> {
  const settings = await getSettings();
  const set = new Set(settings.enabledOrigins);
  if (enabled) set.add(origin);
  else set.delete(origin);
  const next = { ...settings, enabledOrigins: [...set] };
  await saveSettings(next);
  return next;
}
