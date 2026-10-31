import browser from 'webextension-polyfill';
import { getSettings, onSettingsChanged } from '../../lib/settings';
import { getI18n } from '../../lib/i18n';
import type { TranslateImageRequest, TranslateImageResponse } from '../../lib/messages';

const LOG = '[MWT:bg]';
const POLL_INTERVAL_MS = 800;
// The desktop app serializes local-engine jobs (one pipeline -- vision LLM
// translation + LaMa inpainting -- at a time; see state.rs's
// local_engine_slots), and a single image can take ~3 minutes end to end.
// A page-wide scan queues several images behind each other, so the Nth
// image's real wait is roughly N * 3 minutes before it even starts
// processing, not just its own processing time. 300_000 (5 min, itself
// already bumped up once from an initial 60s tuned for the much faster
// Google engine) was still too short the moment more than one or two
// images queued up -- exactly the TRANSLATION_TIMEOUT reports this went
// through. Since the user now has an explicit, immediate way to bail out
// (the in-page Stop button -- see content/index.ts), this no longer needs
// to double as "the UI's way of giving up on a slow page": raised it to a
// ceiling that should only ever fire if the backend has genuinely hung,
// not because a long queue made a job wait its turn.
const POLL_TIMEOUT_MS = 1_800_000; // 30 min

interface CreateJobResponse {
  jobId: string;
}

interface JobStatusResponse {
  status: 'queued' | 'processing' | 'succeeded' | 'failed';
  stage?: string;
  progressMessage?: string;
  progressPercent?: number;
  translatedImageUrl?: string;
  errorCode?: string;
  errorMessage?: string;
  engineUsed?: string;
}

async function createJob(
  serverUrl: string,
  body: Record<string, unknown>,
): Promise<CreateJobResponse> {
  let lastErr: unknown;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const res = await fetch(`${serverUrl}/api/jobs`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error(`createJob failed (${res.status}): ${text}`);
      }
      return await res.json();
    } catch (err) {
      lastErr = err;
      if (attempt < 2) {
        await new Promise((r) => setTimeout(r, 600 * (attempt + 1)));
      }
    }
  }
  throw lastErr;
}

async function pollJob(
  serverUrl: string,
  jobId: string,
  shouldStop: () => boolean,
  tabId?: number,
  imageUrl?: string,
  uiLang?: string,
): Promise<JobStatusResponse> {
  const deadline = Date.now() + POLL_TIMEOUT_MS;
  const dict = getI18n(uiLang);
  let lastProgressMsg = '';
  while (Date.now() < deadline) {
    if (shouldStop()) {
      return { status: 'failed', errorCode: 'CANCELLED', errorMessage: dict.stoppedByUser };
    }
    let res: Response;
    try {
      res = await fetch(`${serverUrl}/api/jobs/${jobId}`);
    } catch {
      // Falha temporária de rede/túnel: aguarda o próximo ciclo sem derrubar o job
      await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
      continue;
    }
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(`pollJob failed (${res.status}): ${text}`);
    }
    const data: JobStatusResponse = await res.json();
    if (data.progressMessage && data.progressMessage !== lastProgressMsg) {
      lastProgressMsg = data.progressMessage;
      if (tabId !== undefined && imageUrl) {
        browser.tabs
          .sendMessage(tabId, {
            type: 'JOB_PROGRESS',
            imageUrl,
            stage: data.stage,
            progressMessage: data.progressMessage,
            progressPercent: data.progressPercent,
          })
          .catch(() => {});
      }
    }
    if (data.status === 'succeeded' || data.status === 'failed') return data;
    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }
  return { status: 'failed', errorCode: 'TRANSLATION_TIMEOUT', errorMessage: dict.timedOut };
}

// Tracks which tab asked for each in-flight job, so a "Stop" click (always
// scoped to the tab it came from -- see StopTranslationsRequest) can find
// and cancel just that tab's jobs, never another open tab's. stoppedJobs is
// checked by pollJob above; the actual server-side cancellation (so the
// desktop app stops burning CPU/GPU on a queued-but-unwanted job) happens
// via a DELETE call in stopTabTranslations.
const jobTabs = new Map<string, number>();
const stoppedJobs = new Set<string>();

async function stopTabTranslations(tabId: number): Promise<void> {
  const settings = await getSettings();
  const jobIds = [...jobTabs.entries()].filter(([, t]) => t === tabId).map(([id]) => id);
  for (const jobId of jobIds) {
    stoppedJobs.add(jobId);
    fetch(`${settings.serverUrl}/api/jobs/${jobId}`, { method: 'DELETE' }).catch(() => {});
  }
  console.log(LOG, `stopped ${jobIds.length} job(s) for tab`, tabId);
}

/**
 * Browsers now auto-deny a public HTTPS page loading an <img src="http://
 * 127.0.0.1:...">, treating it as the page itself reaching into the local
 * network ("Local Network Access"/"Private Network Access" -- no prompt,
 * just a silent failure: the overlay element gets created but never
 * displays anything). That check is evaluated against the *document*
 * loading the resource, not whoever inserted the tag, so it bites even
 * though the content script put it there. The background script's own
 * fetch() isn't a page-document resource load at all, so it isn't subject
 * to the same check -- it already works today for job creation/polling
 * against this exact origin. Fetching the finished PNG here too and
 * handing the content script a data: URL means the page never makes a
 * network request to 127.0.0.1 itself; there's nothing left to deny.
 */
async function toDataUrl(url: string): Promise<string> {
  let res: Response | null = null;
  let lastErr: unknown;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      res = await fetch(url);
      if (res.ok) break;
    } catch (e) {
      lastErr = e;
      await new Promise((r) => setTimeout(r, 600));
    }
  }
  if (!res || !res.ok) throw new Error(`fetching translated image failed (${res?.status ?? lastErr})`);
  const contentType = res.headers.get('content-type') || 'image/png';
  // Not FileReader.readAsDataURL: this runs in the background context, which
  // for the Chrome MV3 build is a service worker with no DOM -- no
  // FileReader there. btoa() is available in both the Firefox MV2 background
  // page and the Chrome MV3 service worker.
  const bytes = new Uint8Array(await res.arrayBuffer());
  let binary = '';
  const chunkSize = 0x8000; // avoid a call-stack blowup from spreading a huge array into String.fromCharCode
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }
  return `data:${contentType};base64,${btoa(binary)}`;
}

async function handleTranslateImage(req: TranslateImageRequest, tabId: number | undefined): Promise<TranslateImageResponse> {
  const settings = await getSettings();
  console.log(LOG, 'got TRANSLATE_IMAGE', req, 'serverUrl =', settings.serverUrl);
  let jobId: string | undefined;
  try {
    ({ jobId } = await createJob(settings.serverUrl, {
      ...(req.imageBytesBase64
        ? { imageBase64: req.imageBytesBase64, mimeType: req.mimeType }
        : { imageUrl: req.imageUrl }),
      pageUrl: req.pageUrl,
      targetLang: settings.targetLang,
    }));
    if (tabId !== undefined) jobTabs.set(jobId, tabId);
    const result = await pollJob(
      settings.serverUrl,
      jobId,
      () => stoppedJobs.has(jobId!),
      tabId,
      req.imageUrl,
      settings.uiLang,
    );
    console.log(LOG, 'job finished', jobId, result);
    if (result.errorCode === 'CANCELLED') {
      return { ok: false, errorCode: 'CANCELLED', errorMessage: result.errorMessage, cancelled: true };
    }
    if (result.status === 'succeeded' && result.translatedImageUrl) {
      if (result.translatedImageUrl.startsWith('data:')) {
        return { ok: true, translatedImageUrl: result.translatedImageUrl, engineUsed: result.engineUsed };
      }
      let fullUrl = result.translatedImageUrl;
      if (fullUrl.startsWith('/')) {
        fullUrl = `${settings.serverUrl.replace(/\/$/, '')}${fullUrl}`;
      } else {
        try {
          const parsed = new URL(fullUrl);
          const serverBase = new URL(settings.serverUrl);
          if (
            (parsed.hostname === '127.0.0.1' || parsed.hostname === 'localhost') &&
            serverBase.hostname !== '127.0.0.1' &&
            serverBase.hostname !== 'localhost'
          ) {
            fullUrl = `${settings.serverUrl.replace(/\/$/, '')}${parsed.pathname}`;
          }
        } catch {
          // fallback to original
        }
      }
      const dataUrl = await toDataUrl(fullUrl);
      return { ok: true, translatedImageUrl: dataUrl, engineUsed: result.engineUsed };
    }
    return {
      ok: false,
      errorCode: result.errorCode ?? 'TRANSLATION_FAILED',
      errorMessage: result.errorMessage ?? getI18n(settings.uiLang).translationFailed,
    };
  } catch (err) {
    console.error(LOG, 'request to desktop app failed', err);
    return {
      ok: false,
      errorCode: 'SERVER_CONNECTION_ERROR',
      errorMessage: err instanceof Error ? err.message : String(err),
    };
  } finally {
    if (jobId !== undefined) {
      jobTabs.delete(jobId);
      stoppedJobs.delete(jobId);
    }
  }
}

export default defineBackground(() => {
  browser.runtime.onMessage.addListener((message: unknown, sender: browser.Runtime.MessageSender) => {
    const msg = message as { type?: string };
    if (msg.type === 'TRANSLATE_IMAGE') {
      return handleTranslateImage(message as TranslateImageRequest, sender.tab?.id);
    }
    if (msg.type === 'STOP_TRANSLATIONS') {
      if (sender.tab?.id !== undefined) void stopTabTranslations(sender.tab.id);
      return undefined;
    }
    if (msg.type === 'GET_RESOURCE_USAGE') {
      // Relayed through here (rather than a direct fetch from
      // content/index.ts, which runs injected into arbitrary pages and
      // could be blocked by a strict site CSP) the same way job creation
      // already is -- see createJob above.
      return getSettings().then(async (settings) => {
        const res = await fetch(`${settings.serverUrl}/api/resources`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      });
    }
    return undefined;
  });

  browser.commands.onCommand.addListener(async (command) => {
    if (command !== 'translate-page') return;
    const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
    if (tab?.id) {
      await browser.tabs.sendMessage(tab.id, { type: 'TRANSLATE_PAGE_NOW' });
    }
  });

  // removeAll() first: avoids a "duplicate id" error if the background
  // context is re-created without a full browser restart (e.g. reloading a
  // temporary add-on during development).
  getSettings().then((s) => {
    browser.contextMenus.removeAll().then(() => {
      browser.contextMenus.create({
        id: 'mwt-translate-image',
        title: getI18n(s.uiLang).contextMenuTitle,
        contexts: ['image'],
      });
    });
  });

  onSettingsChanged((next) => {
    browser.contextMenus.update('mwt-translate-image', {
      title: getI18n(next.uiLang).contextMenuTitle,
    }).catch(() => {});
  });

  browser.contextMenus.onClicked.addListener(async (info, tab) => {
    if (info.menuItemId !== 'mwt-translate-image' || !tab?.id || !info.srcUrl) return;
    await browser.tabs.sendMessage(tab.id, { type: 'TRANSLATE_ONE_IMAGE', srcUrl: info.srcUrl });
  });
});
