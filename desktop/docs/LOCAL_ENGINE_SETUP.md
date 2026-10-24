# Setting up the local (free, offline) engine

The "local" engine in the desktop app is just a thin HTTP proxy
(`src-tauri/src/engines/local.rs`) to a self-hosted OCR+translation+inpainting
server that you run yourself. It was **not built from scratch here** —
that pipeline is a solved problem with a mature open-source implementation:

- **manga-image-translator** — https://github.com/zyddnys/manga-image-translator
  Handles text detection, OCR, translation, and inpainting end to end, and
  ships its own local web server + API.

## Steps

1. Clone and set it up per its own README (it needs a few GB of model
   weights and, for good speed, a GPU — it also runs on CPU, just slower).
2. Start it in web/server mode (check its README for the current flag name;
   it has changed across versions, e.g. `--mode web` or `-m web`).
3. Note which port it listens on (its default has historically been
   `5003`) and set it in the desktop app's settings window as "Local engine
   URL".
4. **Verify the actual HTTP contract before trusting it blindly.** Send it
   one test image by hand:
   ```bash
   curl -F "image=@test.png" http://127.0.0.1:5003/translate -o result.png
   ```
   `src-tauri/src/engines/local.rs` was written from general knowledge of
   how these tools are typically shaped (multipart upload in, image or
   JSON-with-base64 out), not verified against a running instance. If the
   real endpoint path, form field name, or response shape differs, change
   the matching "Advanced" fields in the settings window (path, upload
   field name), or edit the response parsing in `local.rs` directly.
5. Pick an engine preference that includes "local" in the settings window.
   Changes apply to the next translation immediately — no restart needed.

## Why a proxy instead of bundling it

`manga-image-translator`'s dependencies (PyTorch, detection/OCR/inpainting
model weights) are large and GPU-oriented — bundling them into this small
Rust app would turn a tiny native binary into a multi-gigabyte one. Keeping
it as a separate process you run on your own terms (and can swap for a
different local tool later) keeps the desktop app thin.
