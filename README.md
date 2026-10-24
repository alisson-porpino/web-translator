# Manga Web Translator (self-hosted)

A personal, open-source alternative to commercial "scan translator" browser
extensions: detects manga/manhwa/webtoon panel images on any page you're
reading and overlays a translated version, produced by a pipeline you run
and control yourself (no account, no credits, no third-party server seeing
your reading activity).

Two pieces:

- **`extension/`** — a WXT-based browser extension (builds for both Chrome
  and Firefox/Zen from one codebase). Detects candidate images on the page,
  asks the local desktop app to translate them, and overlays the result in
  place.
- **`desktop/`** — a small Tauri (Rust) desktop app: a native settings
  window plus the actual background server that does the OCR +
  translation + inpainting, via a pluggable "engine" (a free local
  self-hosted pipeline, your own Google Gemini API key, or try-one-then-
  the-other). No Electron, no Python, no Node needed to run it — see
  `desktop/README.md`.

## Quick start

1. **Desktop app**: `cd desktop && cargo tauri dev` (first time: install
   Rust + `cargo install tauri-cli` — see `desktop/README.md`). A window
   opens for engine/API-key settings; the app keeps running in the system
   tray after you close that window. Building `cargo tauri build` instead
   produces a real double-click installer, so end users never need Rust,
   Node, or a terminal at all.
2. **Extension**:
   ```bash
   cd extension
   npm install
   npm run dev          # Chrome, loads unpacked in dev mode
   npm run dev:firefox  # Zen/Firefox
   ```
   Open the extension popup and confirm the server URL matches the desktop
   app's (default `http://127.0.0.1:8000` for both — works out of the box).
3. Open a page with manga/manhwa/webtoon images and watch for the orange
   detection outline, then the translated overlay.

## How detection works

A `MutationObserver` watches every `<img>` on the page; each one is scored
against two size heuristics (tuned to catch tall scan pages / webtoon strips
and smaller speech-bubble crops, while skipping icons/avatars/ads) before
being sent off for translation. See `extension/entrypoints/content/index.ts`.

## Status / next steps

This is a working first pass, smoke-tested end to end (job creation →
image fetch → cache → engine dispatch → error surfacing; the desktop app's
embedded server and settings window both run and respond), but not yet
used on a real manga site for hours of reading. Rough edges to expect:

- The local engine needs you to separately install and run an open-source
  OCR/translation tool (see `desktop/docs/LOCAL_ENGINE_SETUP.md`) — its
  exact API shape wasn't verified against a live instance.
- The Google engine's REST call was written from general knowledge, not
  tested against a live API key — if it errors, the error message from
  Gemini will say why.
- No cookie-aware upload fallback yet for images that require the reader
  site's own login session to fetch (see `desktop/README.md`).
- The extension and desktop app aren't packaged together yet (two
  separate installs); see "Next" below.

## Next

- Package a release: `cargo tauri build` for the desktop app, `wxt zip`
  for the extension, linked from one GitHub release.
- Add the cookie-aware upload fallback (browser fetches protected images
  itself, server accepts raw bytes instead of a URL).
