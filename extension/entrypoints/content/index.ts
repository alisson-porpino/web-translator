import browser from 'webextension-polyfill';
import './style.css';
import { getSettings, onSettingsChanged, type Settings } from '../../lib/settings';
import type { TranslateImageResponse } from '../../lib/messages';

// --- Detection heuristics -------------------------------------------------
// Separate "likely a manga/manhwa/webtoon panel" from icons, avatars, ads.
// Two shapes are accepted: tall scan pages/webtoon strips, and smaller
// roughly-square speech-bubble-sized crops.
function isLikelyScanPanel(w: number, h: number): boolean {
  const ratio = h / w;
  // Manga/manhwa/webtoon pages are tall strips or (rarer) wide double-page
  // spreads -- real content is essentially never near-square. Large square
  // images are almost always something else: emoji packs, reaction icons,
  // logos, avatars. Excluding ~0.85-1.15 kills that whole false-positive
  // class without needing any site-specific rule.
  const nearSquare = ratio >= 0.85 && ratio <= 1.15;
  return w >= 500 && h >= 500 && ratio >= 0.3 && !nearSquare;
}
function isLikelyBubbleCrop(w: number, h: number): boolean {
  // Unlike isLikelyScanPanel this intentionally allows near-square shapes
  // (a single speech-bubble crop's bounding box often is square-ish), but
  // that only makes sense at bubble scale. Without an upper bound this also
  // matched large square site chrome -- logos, cover-art thumbnails in a
  // "related manga" rail -- anything exactly as square as a real bubble
  // crop but far bigger than one would ever be.
  return w >= 180 && h >= 80 && w <= 420 && h <= 420 && w * h >= 40_000;
}
function isCandidateImage(img: HTMLImageElement): boolean {
  const src = img.currentSrc || img.src;
  // SVGs are vector icons/logos/payment badges, never scanned manga pages --
  // without this, large inline SVGs (e.g. MangaDex's footer payment icons)
  // can report a naturalWidth/Height that passes the size heuristic.
  if (src.endsWith('.svg') || src.startsWith('data:image/svg')) return false;
  const w = img.naturalWidth || img.width;
  const h = img.naturalHeight || img.height;
  return isLikelyScanPanel(w, h) || isLikelyBubbleCrop(w, h);
}

const LOG = '[MWT]';
// Element-identity dedup: fast, but useless against SPAs (MangaDex included)
// that unmount/remount <img> nodes for the same panel on re-render -- a new
// DOM node looks "new" to a WeakSet even though it's the same picture. Real
// protection is processedKeys below, keyed by something that survives a
// remount: the URL for http(s) sources, a content hash for blob:/data:
// sources (whose URL itself is regenerated per mount and so isn't stable).
const processed = new WeakSet<HTMLImageElement>();
const processedKeys = new Set<string>();
let settings: Settings;

// --- Progress / stop panel --------------------------------------------
// The local engine can take minutes per image (see background.js's
// POLL_TIMEOUT_MS comment) -- a page-wide scan with no visible progress or
// way to back out looked hung. This small fixed panel is the extension's
// own answer to "how far along is this, and how do I stop it", separate
// from the per-image dashed/red border, which only ever showed the state
// of one image at a time.
let progressPanel: HTMLDivElement | null = null;
let progressText: HTMLSpanElement | null = null;
let progressStopBtn: HTMLButtonElement | null = null;
let totalQueued = 0;
let totalFinished = 0;
let hidePanelTimer: ReturnType<typeof setTimeout> | null = null;

function ensureProgressPanel(): void {
  if (progressPanel) return;
  progressPanel = document.createElement('div');
  progressPanel.className = 'mwt-progress-panel';
  progressText = document.createElement('span');
  progressText.className = 'mwt-progress-panel__text';
  progressStopBtn = document.createElement('button');
  progressStopBtn.className = 'mwt-progress-panel__stop';
  progressStopBtn.type = 'button';
  progressStopBtn.textContent = 'Stop';
  progressStopBtn.addEventListener('click', stopAllTranslations);
  progressPanel.appendChild(progressText);
  progressPanel.appendChild(progressStopBtn);
  document.documentElement.appendChild(progressPanel);
}

function updateProgressPanel(): void {
  // The review panel shows its own per-row + header progress once a
  // manual review batch is confirmed -- this simpler text+stop panel
  // would just be a redundant second status line in that case.
  if (reviewPanelActive) return;
  if (totalQueued === 0) return;
  if (hidePanelTimer) {
    clearTimeout(hidePanelTimer);
    hidePanelTimer = null;
  }
  ensureProgressPanel();
  progressPanel!.hidden = false;

  if (totalFinished >= totalQueued) {
    progressText!.textContent = `Translated ${totalFinished}/${totalQueued}.`;
    progressStopBtn!.disabled = true;
    // Auto-dismiss, then reset so the next "translate this page" starts a
    // fresh count instead of carrying this batch's totals forward.
    hidePanelTimer = setTimeout(() => {
      if (progressPanel) progressPanel.hidden = true;
      totalQueued = 0;
      totalFinished = 0;
    }, 4000);
  } else {
    const running = runningTranslations;
    progressText!.textContent =
      `Translating ${totalFinished}/${totalQueued}${running > 0 ? ` (${running} running)` : ''}…`;
    progressStopBtn!.disabled = false;
  }
}

// --- Review panel (manual scan only) -----------------------------------
// "Translate this page now" collects candidates here instead of enqueueing
// them immediately -- shows a panel with a thumbnail + checkbox per
// candidate, lets the user exclude false positives from the detection
// heuristic *before* any backend job is created for them, and lets
// clicking a row scroll the page to that image so it can be checked in
// context. Auto-scan (enabledOrigins) and the right-click single-image
// action are untouched -- both already are an explicit, deliberate choice
// (opting a whole site in, or picking one image), so there's nothing to
// review there.
type ReviewRowState = 'review' | 'queued' | 'translating' | 'done' | 'error' | 'cancelled';
interface ReviewRow {
  img: HTMLImageElement;
  include: boolean;
  state: ReviewRowState;
  rowEl: HTMLDivElement;
  checkboxEl: HTMLInputElement;
  statusEl: HTMLSpanElement;
}
let reviewRows: ReviewRow[] = [];
// Rows actively translating/queued after confirm -- pumpTranslationQueue
// looks images up here to update their row's live status without needing
// any change to translateImage's own signature (it reads the same
// mwt-border--* wrapper classes translateImage already sets).
const activeReviewRows = new Map<HTMLImageElement, ReviewRow>();
let reviewPanelActive = false;
let reviewPanel: HTMLDivElement | null = null;
let reviewHeaderText: HTMLSpanElement | null = null;
let reviewConfirmBtn: HTMLButtonElement | null = null;
let reviewSelectAllBtn: HTMLButtonElement | null = null;
// Doubles as "Cancel" (pre-confirm: discard the review batch, translate
// nothing) and "Stop" (post-confirm: same tab-wide stop the old simple
// progress panel's button always did) -- see updateReviewHeader, which
// relabels it, and cancelOrStopReview below, which dispatches by phase.
// Found 2026-10-25: hiding this unconditionally once translation started
// (matching the confirm button, which really should disappear then) left
// a batch with no way to stop at all, since the plain progress panel's
// own Stop button is suppressed whenever the review panel is active (see
// updateProgressPanel's reviewPanelActive guard) -- there was nothing
// left on screen to click.
let reviewCancelBtn: HTMLButtonElement | null = null;
let reviewListEl: HTMLDivElement | null = null;
let reviewHideTimer: ReturnType<typeof setTimeout> | null = null;

function ensureReviewPanel(): void {
  if (reviewPanel) return;
  reviewPanel = document.createElement('div');
  reviewPanel.className = 'mwt-review-panel';
  reviewPanel.hidden = true;

  const header = document.createElement('div');
  header.className = 'mwt-review-panel__header';
  reviewHeaderText = document.createElement('span');
  header.appendChild(reviewHeaderText);
  reviewSelectAllBtn = document.createElement('button');
  reviewSelectAllBtn.type = 'button';
  reviewSelectAllBtn.className = 'mwt-review-panel__select-all';
  reviewSelectAllBtn.title = 'Select/deselect all';
  reviewSelectAllBtn.addEventListener('click', () => {
    const next = !reviewRows.every((r) => r.include);
    for (const row of reviewRows) {
      row.include = next;
      row.checkboxEl.checked = next;
    }
    updateReviewHeader();
  });
  header.appendChild(reviewSelectAllBtn);
  reviewConfirmBtn = document.createElement('button');
  reviewConfirmBtn.type = 'button';
  reviewConfirmBtn.className = 'mwt-review-panel__confirm';
  reviewConfirmBtn.addEventListener('click', confirmReview);
  header.appendChild(reviewConfirmBtn);
  reviewCancelBtn = document.createElement('button');
  reviewCancelBtn.type = 'button';
  reviewCancelBtn.className = 'mwt-review-panel__cancel';
  reviewCancelBtn.addEventListener('click', cancelOrStopReview);
  header.appendChild(reviewCancelBtn);
  reviewPanel.appendChild(header);

  reviewListEl = document.createElement('div');
  reviewListEl.className = 'mwt-review-panel__list';
  reviewPanel.appendChild(reviewListEl);

  document.documentElement.appendChild(reviewPanel);
}

function scrollToImage(img: HTMLImageElement): void {
  img.scrollIntoView({ behavior: 'smooth', block: 'center' });
  const target = img.parentElement?.classList.contains('mwt-wrapper') ? img.parentElement : img;
  target.classList.add('mwt-highlight-flash');
  setTimeout(() => target.classList.remove('mwt-highlight-flash'), 1500);
}

const ROW_STATUS_LABEL: Record<Exclude<ReviewRowState, 'review'>, string> = {
  queued: 'Queued…',
  translating: 'Translating…',
  done: 'Done',
  error: 'Failed',
  cancelled: 'Cancelled',
};

function updateRowStatusUI(row: ReviewRow): void {
  if (row.state === 'review') return;
  row.checkboxEl.style.visibility = 'hidden';
  row.statusEl.hidden = false;
  row.statusEl.textContent = ROW_STATUS_LABEL[row.state];
  row.statusEl.className = `mwt-review-row__status mwt-review-row__status--${row.state}`;
}

function buildReviewRow(img: HTMLImageElement): ReviewRow {
  const rowEl = document.createElement('div');
  rowEl.className = 'mwt-review-row';

  const checkboxEl = document.createElement('input');
  checkboxEl.type = 'checkbox';
  checkboxEl.checked = true;
  checkboxEl.className = 'mwt-review-row__checkbox';
  checkboxEl.addEventListener('click', (e) => e.stopPropagation());
  checkboxEl.addEventListener('change', () => {
    row.include = checkboxEl.checked;
    // Without this, the confirm button's count/enabled state only ever
    // refreshed on the "All"/"None" button's own click -- toggling one
    // checkbox by hand left it frozen wherever that last bulk toggle set
    // it (e.g. stuck disabled at "Translate (0)" after "None" followed by
    // re-checking a single row).
    updateReviewHeader();
  });
  rowEl.appendChild(checkboxEl);

  const thumb = document.createElement('img');
  thumb.className = 'mwt-review-row__thumb';
  thumb.src = img.currentSrc || img.src;
  thumb.alt = '';
  rowEl.appendChild(thumb);

  const caption = document.createElement('span');
  caption.className = 'mwt-review-row__caption';
  caption.textContent = `${img.naturalWidth}×${img.naturalHeight}`;
  rowEl.appendChild(caption);

  const statusEl = document.createElement('span');
  statusEl.className = 'mwt-review-row__status';
  statusEl.hidden = true;
  rowEl.appendChild(statusEl);

  const row: ReviewRow = { img, include: true, state: 'review', rowEl, checkboxEl, statusEl };
  rowEl.addEventListener('click', () => scrollToImage(img));
  return row;
}

function updateReviewHeader(): void {
  if (!reviewHeaderText || !reviewConfirmBtn || !reviewCancelBtn || !reviewSelectAllBtn) return;
  const translating = reviewRows.some((r) => r.state !== 'review');
  if (translating) {
    const done = reviewRows.filter((r) => r.state !== 'queued' && r.state !== 'translating').length;
    const allDone = done >= reviewRows.length;
    reviewHeaderText.textContent = allDone ? `Translated ${done}/${reviewRows.length}.` : `Translating ${done}/${reviewRows.length}…`;
    reviewConfirmBtn.hidden = true;
    reviewSelectAllBtn.hidden = true;
    // Nothing left to stop once every row reached a terminal state.
    reviewCancelBtn.hidden = allDone;
    reviewCancelBtn.textContent = 'Stop';
  } else {
    const included = reviewRows.filter((r) => r.include).length;
    reviewHeaderText.textContent = `Found ${reviewRows.length} image(s)`;
    reviewConfirmBtn.hidden = false;
    reviewConfirmBtn.textContent = `Translate (${included})`;
    reviewConfirmBtn.disabled = included === 0;
    reviewSelectAllBtn.hidden = false;
    reviewSelectAllBtn.textContent = included === reviewRows.length && reviewRows.length > 0 ? 'None' : 'All';
    reviewCancelBtn.hidden = false;
    reviewCancelBtn.textContent = 'Cancel';
  }
}

/** Dispatches the one header button by phase -- 'Cancel' (discard the
 * pending review batch, nothing gets translated) before confirm, 'Stop'
 * (same tab-wide stop the old simple progress panel's button always did)
 * after. */
function cancelOrStopReview(): void {
  if (reviewRows.some((r) => r.state !== 'review')) {
    stopAllTranslations();
  } else {
    cancelReview();
  }
}

function addReviewCandidate(img: HTMLImageElement): void {
  if (processed.has(img)) return;
  processed.add(img);
  ensureReviewPanel();
  const row = buildReviewRow(img);
  reviewRows.push(row);
  reviewListEl!.appendChild(row.rowEl);
  reviewPanel!.hidden = false;
  reviewPanelActive = true;
  updateReviewHeader();
}

function cancelReview(): void {
  for (const row of reviewRows) processed.delete(row.img);
  reviewRows = [];
  reviewListEl!.innerHTML = '';
  reviewPanel!.hidden = true;
  reviewPanelActive = false;
}

function confirmReview(): void {
  const included = reviewRows.filter((r) => r.include);
  const excluded = reviewRows.filter((r) => !r.include);
  // Excluded rows stay in `processed` (added back in addReviewCandidate) --
  // an explicit exclusion is remembered for the rest of this page session
  // instead of being re-offered on the next manual scan.
  for (const row of excluded) row.rowEl.remove();

  reviewRows = included;
  for (const row of reviewRows) {
    row.state = 'queued';
    updateRowStatusUI(row);
    activeReviewRows.set(row.img, row);
    // enqueueTranslation itself re-adds this to `processed` -- undo the
    // add-at-review-time marking first so it doesn't just no-op there.
    processed.delete(row.img);
    enqueueTranslation(row.img);
  }
  updateReviewHeader();

  if (reviewHideTimer) clearTimeout(reviewHideTimer);
  const checkDone = () => {
    updateReviewHeader();
    if (reviewRows.every((r) => r.state !== 'queued' && r.state !== 'translating')) {
      reviewHideTimer = setTimeout(() => {
        reviewPanel!.hidden = true;
        reviewListEl!.innerHTML = '';
        reviewRows = [];
        reviewPanelActive = false;
      }, 4000);
    }
  };
  reviewDoneCheck = checkDone;
}
// Called from pumpTranslationQueue's completion handler below (defined
// after this point in the file) -- kept as a reassignable top-level `let`
// rather than a forward reference so both halves stay simple functions.
let reviewDoneCheck: (() => void) | null = null;

/** Drops everything not yet started, tells the background script to cancel
 * whatever it's already created jobs for (freeing the backend's queue slot
 * for other tabs too), and marks still-pending images as cancelled rather
 * than leaving them stuck on a dashed "in progress" border forever. */
function stopAllTranslations(): void {
  console.log(LOG, 'user requested stop:', translationQueue.length, 'queued,', runningTranslations, 'running');
  const dropped = translationQueue.splice(0, translationQueue.length);
  totalFinished += dropped.length;
  for (const img of dropped) {
    const wrapper = img.parentElement;
    if (wrapper?.classList.contains('mwt-wrapper')) {
      wrapper.classList.remove('mwt-border--pending');
      wrapper.classList.add('mwt-border--cancelled');
      wrapper.title = 'Cancelled.';
    }
    // Still-queued review rows never reach pumpTranslationQueue's own
    // finally-block status update (that only fires for ones it actually
    // dequeued) -- update them here instead, or they'd be stuck showing
    // "Queued…" forever even though nothing is coming for them anymore.
    const reviewRow = activeReviewRows.get(img);
    if (reviewRow) {
      reviewRow.state = 'cancelled';
      updateRowStatusUI(reviewRow);
      activeReviewRows.delete(img);
    }
  }
  updateProgressPanel();
  updateReviewHeader();
  reviewDoneCheck?.();
  void browser.runtime.sendMessage({ type: 'STOP_TRANSLATIONS' }).catch((err) => {
    console.error(LOG, 'failed to send STOP_TRANSLATIONS', err);
  });
}

async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

function wrapImage(img: HTMLImageElement): HTMLElement {
  if (img.parentElement?.classList.contains('mwt-wrapper')) {
    return img.parentElement;
  }
  const wrapper = document.createElement('span');
  wrapper.className = 'mwt-wrapper';
  img.replaceWith(wrapper);
  wrapper.appendChild(img);
  return wrapper;
}

/**
 * blob:/data: URLs only exist inside this page -- the desktop app's server
 * can never fetch them over the network. Read them here, in the page's own
 * context, and ship the bytes instead of the URL. Regular http(s) URLs are
 * left for the background script to fetch server-side (cheaper: no bytes
 * cross the extension messaging boundary for the common case).
 */
async function readLocalImageAsBase64(url: string): Promise<{ base64: string; mimeType: string } | null> {
  try {
    // credentials: 'include' matters for the IMAGE_FETCH_FAILED retry path
    // below -- some sites only serve their images to a request carrying the
    // reader's own session cookies, which our desktop app's server-side
    // fetch (a separate OS process, no browser cookies at all) can never
    // have. Harmless for blob:/data: URLs, which ignore it entirely.
    const res = await fetch(url, { credentials: 'include' });
    const blob = await res.blob();
    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
    const match = /^data:([^;]+);base64,(.*)$/s.exec(dataUrl);
    if (!match) return null;
    return { mimeType: match[1] || blob.type || 'image/png', base64: match[2] ?? '' };
  } catch (err) {
    console.error(LOG, 'failed to read local image bytes for', url, err);
    return null;
  }
}

/**
 * @param force Skip every dedup/heuristic guard -- used for an explicit,
 * single-image user action (right-click "Translate this image") where
 * re-processing on purpose (e.g. retrying a failed one) is exactly the point.
 */
async function translateImage(img: HTMLImageElement, force = false): Promise<void> {
  // Element-identity dedup (the `processed` WeakSet) happens at enqueue
  // time now -- see enqueueTranslation -- not here, so a second scan
  // triggered by the MutationObserver while this image is still sitting in
  // the queue (not yet started) can't push a duplicate entry for it.

  const imageUrl = img.currentSrc || img.src;
  const isLocalOnly = imageUrl.startsWith('blob:') || imageUrl.startsWith('data:');

  if (!isLocalOnly && !imageUrl.startsWith('http')) {
    const wrapper = wrapImage(img);
    console.warn(LOG, 'skipping: unsupported image source', imageUrl);
    wrapper.classList.add('mwt-border--error');
    wrapper.title = `Unsupported image source: ${imageUrl || '(empty)'}`;
    return;
  }

  // http(s) URLs are stable across remounts -- dedup before doing anything.
  if (!isLocalOnly && !force) {
    if (processedKeys.has(imageUrl)) {
      console.debug(LOG, 'skipping duplicate (same URL already queued)', imageUrl);
      return;
    }
    processedKeys.add(imageUrl);
  }

  const wrapper = wrapImage(img);
  wrapper.classList.add('mwt-border--pending');
  console.log(LOG, 'translating candidate', { imageUrl, w: img.naturalWidth, h: img.naturalHeight });

  let imageBytesBase64: string | undefined;
  let mimeType: string | undefined;

  if (isLocalOnly) {
    const read = await readLocalImageAsBase64(imageUrl);
    if (!read) {
      wrapper.classList.remove('mwt-border--pending');
      wrapper.classList.add('mwt-border--error');
      wrapper.title = 'Could not read this image from the page.';
      return;
    }
    // blob:/data: URLs are regenerated per mount, so the URL itself isn't a
    // stable dedup key here -- hash the actual bytes instead.
    if (!force) {
      const contentKey = await sha256Hex(read.base64);
      if (processedKeys.has(contentKey)) {
        console.debug(LOG, 'skipping duplicate (same image bytes already queued)', imageUrl);
        wrapper.classList.remove('mwt-border--pending');
        return;
      }
      processedKeys.add(contentKey);
    }
    imageBytesBase64 = read.base64;
    mimeType = read.mimeType;
  }

  const sendTranslateRequest = async (bytesBase64?: string, bytesMimeType?: string): Promise<TranslateImageResponse> => {
    try {
      const result = (await browser.runtime.sendMessage({
        type: 'TRANSLATE_IMAGE',
        imageUrl,
        pageUrl: location.href,
        imageBytesBase64: bytesBase64,
        mimeType: bytesMimeType,
      })) as TranslateImageResponse;
      console.log(LOG, 'response for', imageUrl, result);
      return result;
    } catch (err) {
      console.error(LOG, 'sendMessage failed for', imageUrl, err);
      return {
        ok: false,
        errorCode: 'EXTENSION_MESSAGE_FAILED',
        errorMessage: err instanceof Error ? err.message : String(err),
      };
    }
  };

  let response = await sendTranslateRequest(imageBytesBase64, mimeType);

  // The desktop app's server fetches by URL from outside the browser, so it
  // has no cookies -- if that's exactly why it failed, retry once with the
  // page's own fetch (credentials included), same as readLocalImageAsBase64
  // above does for blob:/data: sources. Only for the URL-only path: if we
  // already sent bytes there's nothing left to retry with.
  if (!response.ok && response.errorCode === 'IMAGE_FETCH_FAILED' && !imageBytesBase64) {
    console.warn(LOG, 'server-side fetch failed, retrying with page-fetched bytes (may need session cookies)', imageUrl);
    const read = await readLocalImageAsBase64(imageUrl);
    if (read) {
      response = await sendTranslateRequest(read.base64, read.mimeType);
    }
  }

  wrapper.classList.remove('mwt-border--pending');

  if (!response.ok || !response.translatedImageUrl) {
    wrapper.classList.add(response.cancelled ? 'mwt-border--cancelled' : 'mwt-border--error');
    wrapper.title = response.cancelled ? 'Cancelled.' : (response.errorMessage ?? response.errorCode ?? 'Translation failed');
    return;
  }

  const overlay = document.createElement('img');
  overlay.className = 'mwt-overlay';
  overlay.src = response.translatedImageUrl;
  overlay.alt = `${img.alt ?? ''} (translated)`;
  wrapper.appendChild(overlay);

  const toggle = document.createElement('button');
  toggle.className = 'mwt-toggle';
  toggle.type = 'button';
  toggle.textContent = 'EN/原';
  toggle.title = 'Toggle original / translated';
  toggle.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    overlay.style.display = overlay.style.display === 'none' ? '' : 'none';
  });
  wrapper.appendChild(toggle);
}

// The desktop app's local engine only runs one translation pipeline at a
// time (CPU-heavy ONNX inference; see state.rs's local_engine_slots), so
// firing every candidate on a page at once -- a long scan page can easily
// have 20-30 panels -- just means most of them sit blocked behind the
// backend's own queue instead of this one. Throttling job creation here
// too keeps a slow page's backlog from ever building past a few images
// deep, so each individual job's wait before it actually starts stays
// short and predictable.
const MAX_CONCURRENT_TRANSLATIONS = 3;
let runningTranslations = 0;
const translationQueue: HTMLImageElement[] = [];

function pumpTranslationQueue(): void {
  while (runningTranslations < MAX_CONCURRENT_TRANSLATIONS && translationQueue.length > 0) {
    const img = translationQueue.shift()!;
    runningTranslations++;
    updateProgressPanel();
    const reviewRow = activeReviewRows.get(img);
    if (reviewRow) {
      reviewRow.state = 'translating';
      updateRowStatusUI(reviewRow);
      updateReviewHeader();
    }
    void translateImage(img).finally(() => {
      runningTranslations--;
      totalFinished++;
      updateProgressPanel();
      if (reviewRow) {
        // translateImage already recorded the outcome as a CSS class on
        // the image's wrapper -- read it back instead of threading a
        // separate callback through translateImage's own signature.
        const wrapper = img.parentElement;
        reviewRow.state = wrapper?.classList.contains('mwt-border--error')
          ? 'error'
          : wrapper?.classList.contains('mwt-border--cancelled')
            ? 'cancelled'
            : 'done';
        updateRowStatusUI(reviewRow);
        activeReviewRows.delete(img);
        reviewDoneCheck?.();
      }
      pumpTranslationQueue();
    });
  }
}

function enqueueTranslation(img: HTMLImageElement): void {
  if (processed.has(img)) return;
  processed.add(img);
  translationQueue.push(img);
  totalQueued++;
  updateProgressPanel();
  pumpTranslationQueue();
}

/**
 * @param mode 'auto' (default) enqueues every match straight away, same as
 * always -- used by the MutationObserver on enabledOrigins, where the
 * per-site opt-in already is the user's confirmation. 'manual' (the
 * "Translate this page now" button/shortcut) instead collects matches into
 * the review panel so the user can exclude false positives before any job
 * is created for them.
 */
function scanForCandidates(mode: 'auto' | 'manual' = 'auto'): void {
  const images = document.querySelectorAll<HTMLImageElement>('img');
  let candidates = 0;
  for (const img of images) {
    if (processed.has(img)) continue;
    const run = () => {
      if (isCandidateImage(img)) {
        candidates++;
        if (mode === 'manual') {
          addReviewCandidate(img);
        } else {
          enqueueTranslation(img);
        }
      } else {
        console.debug(LOG, 'skipped (too small/odd shape)', {
          src: img.currentSrc || img.src,
          w: img.naturalWidth,
          h: img.naturalHeight,
        });
      }
    };
    if (img.complete && img.naturalWidth > 0) {
      run();
    } else {
      img.addEventListener('load', run, { once: true });
    }
  }
  console.log(LOG, `scan: ${images.length} <img> on page, ${candidates} matched size heuristic so far`);
}

// Auto-scan is opt-in per origin (see lib/settings.ts) -- never blanket-on
// across every open tab. A manual trigger (popup button / keyboard shortcut)
// always bypasses this and scans regardless, scoped to just the active tab
// that sent it.
function isAutoEnabledHere(): boolean {
  return settings.enabledOrigins.includes(location.origin);
}

let scanScheduled = false;
function scheduleScan(): void {
  if (scanScheduled) return;
  scanScheduled = true;
  setTimeout(() => {
    scanScheduled = false;
    if (isAutoEnabledHere()) scanForCandidates();
  }, 300);
}

export default defineContentScript({
  matches: ['<all_urls>'],
  async main() {
    settings = await getSettings();
    console.log(LOG, 'content script loaded on', location.href, 'auto-enabled:', isAutoEnabledHere(), settings);
    onSettingsChanged((next) => {
      const wasEnabled = isAutoEnabledHere();
      settings = next;
      // Flipped on for this origin while the page was already open: start
      // right away instead of waiting for the next DOM mutation / reload.
      if (!wasEnabled && isAutoEnabledHere()) scanForCandidates();
    });

    if (isAutoEnabledHere()) scanForCandidates();

    const observer = new MutationObserver(() => scheduleScan());
    observer.observe(document.documentElement, { childList: true, subtree: true });

    browser.runtime.onMessage.addListener((message: unknown) => {
      const msg = message as { type?: string; srcUrl?: string };
      if (msg?.type === 'TRANSLATE_PAGE_NOW') {
        console.log(LOG, 'manual translate requested for this tab');
        scanForCandidates('manual');
      } else if (msg?.type === 'TRANSLATE_ONE_IMAGE' && msg.srcUrl) {
        const img = [...document.querySelectorAll<HTMLImageElement>('img')].find(
          (el) => el.src === msg.srcUrl || el.currentSrc === msg.srcUrl,
        );
        if (img) {
          console.log(LOG, 'manual single-image translate requested', msg.srcUrl);
          // Bypasses the throttled queue on purpose (translateImage's own
          // `force` param, right-click is always meant to run immediately)
          // but still needs to feed the same counters that drive the
          // progress panel -- otherwise this path (unlike enqueueTranslation/
          // pumpTranslationQueue) never touches totalQueued/totalFinished,
          // and the panel just never shows up for a right-click translation.
          totalQueued++;
          updateProgressPanel();
          void translateImage(img, true).finally(() => {
            totalFinished++;
            updateProgressPanel();
          });
        } else {
          console.warn(LOG, 'right-clicked image not found on page (anymore?)', msg.srcUrl);
        }
      }
    });
  },
});
