use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub port: u16,
    pub engine_preference: String,
    pub google_api_key: String,
    /// A text/vision model used only for OCR + translation (structured JSON
    /// output) -- never an image-generation model. Renamed from
    /// gemini_image_model now that the Google engine no longer asks a
    /// generative model to edit the image itself (see engines/google.rs);
    /// #[serde(default)] so older settings.json files just pick up the new
    /// default here instead of failing to load entirely.
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,
    /// Hard safety cap: max Google API calls allowed per UTC day, enforced
    /// server-side before any request is sent. 0 means unlimited -- an
    /// explicit opt-in, never the default. #[serde(default)] keeps older
    /// settings.json files (saved before this field existed) loadable
    /// instead of silently resetting every other saved setting.
    #[serde(default = "default_daily_cap")]
    pub google_daily_request_cap: u32,
    /// Deprecated single-font field, kept only so `load()` can migrate an
    /// older settings.json into `font_paths` below -- not read anywhere
    /// else. #[serde(default)] for the same forward-compat reason as above.
    #[serde(default)]
    pub font_path: String,
    /// Ordered stack of absolute paths to .ttf/.otf files to use for
    /// typeset text instead of the bundled open-license font -- e.g. a
    /// personally licensed copy of Wild Words for Latin text plus a
    /// separate font for kanji (commercial comic-lettering fonts this app
    /// can't legally bundle/redistribute). Each character is drawn with
    /// the first font in the stack that has a glyph for it (see
    /// engines/compose.rs's `pick_font`) -- the bundled Noto Sans JP
    /// fallback is always appended after these, so CJK text still renders
    /// even if every path here is Latin-only or the stack is empty. Still
    /// used by the Google engine path (engines/google.rs) -- the Koharu
    /// engine (engines/koharu_engine.rs) does its own typesetting and
    /// doesn't read this. #[serde(default)] for the same forward-compat
    /// reason as above.
    #[serde(default)]
    pub font_paths: Vec<String>,
    /// Caps on how much CPU/RAM the Google engine's local text detector
    /// (engines/detect.rs, via ONNX Runtime) is allowed to use -- see
    /// resource_limits.rs. The Koharu engine manages its own resources,
    /// unaffected by these. 0 means no cap for either. #[serde(default)]
    /// (both are 0/u32::default) so an older settings.json keeps loading
    /// as uncapped, matching today's actual (unbounded) behavior rather
    /// than silently clamping an existing install the first time it
    /// starts after this update.
    #[serde(default)]
    pub max_cpu_threads: u32,
    #[serde(default)]
    pub max_memory_mb: u32,
    /// Confidence floor for the detection model (0.0-1.0) -- see
    /// engines/detect.rs's DEFAULT_SCORE_THRESHOLD. Only affects the
    /// Google engine path (the Koharu engine does its own detection).
    /// Lower catches more (but noisier) regions; higher is stricter.
    /// #[serde(default)] so an older settings.json keeps today's fixed
    /// 0.3 behavior.
    #[serde(default = "default_detection_score_threshold")]
    pub detection_score_threshold: f32,
    /// Custom root directory for koharu_runtime::Store -- where the Koharu
    /// engine downloads/caches its models (see engines/koharu_engine.rs).
    /// None keeps Koharu's own default (the OS user cache dir). Store's
    /// configure() is OnceLock-protected (same restriction as `port`): a
    /// change here only takes effect after restarting the app, since the
    /// engine may already have resolved artifacts against the old root in
    /// this process. #[serde(default)] so older settings.json files keep
    /// loading with today's default (unset) behavior.
    #[serde(default)]
    pub models_dir: Option<String>,
}

fn default_daily_cap() -> u32 {
    50
}

/// Mirrors engines::detect::DEFAULT_SCORE_THRESHOLD -- kept as a literal
/// here (rather than importing engines::detect) to avoid coupling this
/// plain data module to the engine implementation.
fn default_detection_score_threshold() -> f32 {
    0.3
}

fn default_gemini_model() -> String {
    // gemini-2.5-flash returns HTTP 404 "no longer available to new users"
    // for freshly-created API keys as of this testing (Oct 2026); Google's
    // own error message points at this replacement.
    "gemini-3.8-flash".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: 8000,
            engine_preference: "local-then-google".into(),
            google_api_key: String::new(),
            gemini_model: default_gemini_model(),
            google_daily_request_cap: default_daily_cap(),
            font_path: String::new(),
            font_paths: Vec::new(),
            max_cpu_threads: 0,
            max_memory_mb: 0,
            detection_score_threshold: default_detection_score_threshold(),
            models_dir: None,
        }
    }
}

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_config_dir().expect("app config dir");
    std::fs::create_dir_all(&dir).ok();
    dir.join("settings.json")
}

pub fn load(app: &tauri::AppHandle) -> Settings {
    let path = settings_path(app);
    let mut settings: Settings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // One-time migration: an older settings.json has `font_path` (a
    // single font) but no `font_paths` (the stack) -- carry that
    // single font forward as the stack's one entry instead of silently
    // dropping a font the user already configured.
    if settings.font_paths.is_empty() && !settings.font_path.trim().is_empty() {
        settings.font_paths.push(settings.font_path.clone());
    }

    settings
}

pub fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app);
    let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

pub fn cache_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app data dir").join("cache");
    std::fs::create_dir_all(&dir).ok();
    dir
}
