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
    /// even if every path here is Latin-only or the stack is empty.
    /// #[serde(default)] for the same forward-compat reason as above.
    #[serde(default)]
    pub font_paths: Vec<String>,
    /// Which catalog option (see models.rs) the local engine uses for each
    /// stage. #[serde(default = ...)] so an older settings.json (saved
    /// before this field existed) still loads instead of resetting every
    /// other setting -- see models.rs's module docs for why there's only
    /// one real choice per stage today despite this being a free-text id.
    #[serde(default = "default_ocr_model")]
    pub ocr_model: String,
    #[serde(default = "default_inpainting_model")]
    pub inpainting_model: String,
    #[serde(default = "default_translation_model")]
    pub translation_model: String,
    /// Caps on how much CPU/RAM the local engine (llama.cpp + ONNX
    /// Runtime) is allowed to use -- see resource_limits.rs. 0 means no
    /// cap for either. #[serde(default)] (both are 0/u32::default) so an
    /// older settings.json keeps loading as uncapped, matching today's
    /// actual (unbounded) behavior rather than silently clamping an
    /// existing install the first time it starts after this update.
    #[serde(default)]
    pub max_cpu_threads: u32,
    #[serde(default)]
    pub max_memory_mb: u32,
    /// VRAM budget (MB) for llama.cpp-based engines (translation,
    /// PaddleOCR-VL) -- 0 means no budget, offload every layer to the
    /// GPU (today's unchanged default). Unlike CPU/RAM, there's no
    /// OS-level "cap VRAM usage" API -- this is an *estimate*: read the
    /// model's real layer count from its GGUF header
    /// (engines/gguf_meta.rs) and offload roughly
    /// `budget / model_file_size` of them, assuming layers are close to
    /// equal size. Fewer layers on the GPU means more goes through
    /// system RAM instead (counted against max_memory_mb, not this) and
    /// runs slower -- not a hard, precise byte-for-byte limit the way
    /// max_memory_mb is.
    #[serde(default)]
    pub max_vram_mb: u32,
    /// Confidence floor for the detection model (0.0-1.0) -- see
    /// engines/detect.rs's DEFAULT_SCORE_THRESHOLD. Lower catches more
    /// (but noisier) regions; higher is stricter. #[serde(default)] so an
    /// older settings.json keeps today's fixed 0.3 behavior.
    #[serde(default = "default_detection_score_threshold")]
    pub detection_score_threshold: f32,
    /// Local translation LLM sampling knobs (engines/local_llm.rs), all
    /// optional -- `None` means "use today's proven behavior" (greedy/
    /// deterministic decoding, unchanged default), matching the Koharu
    /// pattern of per-model generation defaults the user can override.
    /// `llm_temperature: None` or `Some(0.0)` means greedy; any other
    /// value switches to a full sampler chain (penalties -> top_k ->
    /// top_p -> min_p -> temp -> random pick), using sensible fallbacks
    /// for whichever of the other knobs are left unset.
    #[serde(default)]
    pub llm_temperature: Option<f32>,
    #[serde(default)]
    pub llm_top_k: Option<u32>,
    #[serde(default)]
    pub llm_top_p: Option<f32>,
    #[serde(default)]
    pub llm_min_p: Option<f32>,
    #[serde(default)]
    pub llm_max_tokens: Option<u32>,
    #[serde(default)]
    pub llm_repeat_penalty: Option<f32>,
    #[serde(default)]
    pub llm_frequency_penalty: Option<f32>,
    #[serde(default)]
    pub llm_presence_penalty: Option<f32>,
}

fn default_ocr_model() -> String {
    // PaddleOCR-VL 1.6 is Koharu's own default OCR engine -- switched
    // from manga-ocr at Alisson's explicit request, same pattern as
    // translation's default switching to Gemma 4 12B Instruct. manga-ocr
    // remains selectable, just no longer pre-selected on a fresh install.
    "paddle-ocr-vl-1.6".into()
}

fn default_inpainting_model() -> String {
    "lama".into()
}

fn default_translation_model() -> String {
    // Koharu's own default local translation model (DEFAULT_MODEL /
    // DEFAULT_QUANTIZATION in its catalog.rs is "gemma4-12b-it" /
    // "Q4_K_XL") -- matching it here per explicit request, so a fresh
    // install's pre-selected choice is the same one Koharu ships with.
    "gemma-4-12b-it".into()
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
            ocr_model: default_ocr_model(),
            inpainting_model: default_inpainting_model(),
            translation_model: default_translation_model(),
            max_cpu_threads: 0,
            max_memory_mb: 0,
            max_vram_mb: 0,
            detection_score_threshold: default_detection_score_threshold(),
            llm_temperature: None,
            llm_top_k: None,
            llm_top_p: None,
            llm_min_p: None,
            llm_max_tokens: None,
            llm_repeat_penalty: None,
            llm_frequency_penalty: None,
            llm_presence_penalty: None,
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
