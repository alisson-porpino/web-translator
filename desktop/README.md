# Desktop app

A small Tauri (Rust) app that is both the settings UI and the local
background server the browser extension talks to. No Electron, no
Python, no Node runtime needed to *run* it — just one native binary.

- Opening the app shows a settings window (engine choice, Google API key,
  local engine URL).
- Closing that window doesn't quit the app — it keeps running from the
  system tray (menu bar icon on macOS), because that's the actual server
  the extension depends on. Use the tray menu's "Quit" to fully stop it.
- The embedded server speaks the same HTTP API the Python prototype did:
  `POST /api/jobs`, `GET /api/jobs/{id}`, static files under `/cache`.

## Run from source (for development)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # if you don't have Rust
cargo install tauri-cli --version "^2.0" --locked                 # once
cd desktop
cargo tauri dev
```

## Build a real, double-click app

```bash
cd desktop
cargo tauri build
```

This produces a native installer/bundle under `src-tauri/target/release/bundle/`
(`.app`/`.dmg` on macOS, `.msi`/`.exe` on Windows, `.deb`/`.AppImage` on
Linux) — the "double-click .exe" experience, no separate install steps for
whoever runs it.

## Settings

Everything is configured from the app's own window — no `.env` file, no
manual JSON editing:

- **Translation engine**: local-only, Google-only, or either as primary
  with the other as fallback.
- **Google API key**: your own Gemini key (from
  [aistudio.google.com/apikey](https://aistudio.google.com/apikey)),
  stored only in this app's local config file on your machine.
- **Advanced**: Gemini model name, local engine URL/path/upload-field-name
  (see `docs/LOCAL_ENGINE_SETUP.md`), server port.

Settings are read fresh for every translation job, so changes apply
immediately — no restart needed (except changing the port, which the
server binds at startup).

## Why Rust instead of Python

The previous prototype used Python/FastAPI, but nothing here actually
needs Python's ecosystem: the "local" engine is just an HTTP proxy, and
the "google" engine is a single REST call. Python's runtime and packaging
story (virtualenvs, multi-hundred-MB installs, no real path to a single
double-clickable binary) was pure cost with no benefit for this. Rust
compiles to one small, dependency-free native binary — which is also what
made embedding a system-tray settings UI without Electron's footprint
possible in the first place.

## Known limitations (by design, for now)

- Images that require the *reader site's own cookies/session* to fetch
  (e.g. a paywalled or login-gated page) will fail with `IMAGE_FETCH_FAILED`
  — the app fetches the image itself, with no browser session. The
  original extension this project is inspired by has a second path where
  the browser uploads the raw bytes instead; that fallback isn't built yet.
- The local engine's exact HTTP contract depends on which open-source tool
  you install and its version — see `docs/LOCAL_ENGINE_SETUP.md`.
- The Gemini model/call was written from general knowledge of the REST
  API, not tested against a live key from this environment.
