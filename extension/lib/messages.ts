// Message contract between the content script and the background script.
// Kept intentionally small: the background script owns all network I/O
// (talking to the local backend), the content script only owns the DOM.

export interface TranslateImageRequest {
  type: 'TRANSLATE_IMAGE';
  imageUrl: string;
  pageUrl: string;
  /**
   * Raw image bytes (base64, no data: prefix), read in the content script's
   * own page context. Required for blob:/data: image sources, which only
   * exist inside the page and can never be fetched by the desktop app's
   * server -- see entrypoints/content/index.ts.
   */
  imageBytesBase64?: string;
  mimeType?: string;
}

export interface TranslateImageResponse {
  ok: boolean;
  translatedImageUrl?: string;
  errorCode?: string;
  errorMessage?: string;
  engineUsed?: string;
  /** True when this came back because the user hit Stop, not a real failure. */
  cancelled?: boolean;
}

/** Sent by the content script's "Stop" control. No tabId: the background
 * script reads it off the message sender, since this is always sent from
 * inside the tab whose translations should stop. */
export interface StopTranslationsRequest {
  type: 'STOP_TRANSLATIONS';
}

export interface JobProgressNotification {
  type: 'JOB_PROGRESS';
  imageUrl: string;
  stage?: string;
  progressMessage?: string;
  progressPercent?: number;
}

export type ExtensionMessage =
  | TranslateImageRequest
  | StopTranslationsRequest
  | JobProgressNotification;
