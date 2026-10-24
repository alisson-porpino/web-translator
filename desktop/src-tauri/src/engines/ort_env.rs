//! ONNX Runtime allows exactly one `Environment` per process (a second
//! `ort::init().build()` call anywhere errors with "only one environment is
//! allowed per process") -- so detect.rs, manga_ocr.rs and nllb.rs, which
//! can all be loaded in the same process once the local engine is in use,
//! must share a single instance rather than each building their own.
//! `Environment` is `Clone` (it's `Arc`-backed internally), so this just
//! builds it once and hands out clones.

use ort::environment::Environment;
use std::sync::OnceLock;

static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

pub fn shared() -> Result<Environment, String> {
    ENVIRONMENT
        .get_or_init(|| ort::init().with_name("manga-web-translator").build().map_err(|e| e.to_string()))
        .clone()
}
