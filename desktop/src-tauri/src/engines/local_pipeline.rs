//! The fully-local engine: detect.rs (bundled, always available) finds the
//! text regions; manga_ocr.rs reads them; local_llm.rs translates the
//! result; compose.rs draws it back in. No network call anywhere in this
//! path -- everything after the one-time model download runs entirely
//! on-device. Runs on a blocking thread pool (tokio::task::spawn_blocking):
//! both ONNX and llama.cpp inference here are CPU/GPU-bound and can take
//! real time, which must not stall the async runtime the rest of the
//! app's HTTP server depends on.
//!
//! Which concrete model backs each stage is a user choice (see
//! models.rs's catalog + Settings::{ocr,inpainting,translation}_model) --
//! build_models below is the one place that maps a chosen option id to an
//! actual engine instance, and is exactly what a future catalog entry
//! needs a matching arm in to become usable.

use ab_glyph::FontArc;
use image::RgbaImage;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

use super::baberu_ocr::BaberuOcr;
use super::compose::{self, TextRegion};
use super::detect::{self, RegionKind};
use super::inpaint::LamaInpainter;
use super::local_llm::{self, LocalLlm};
use super::manga_ocr::MangaOcr;
use super::paddle_ocr_vl::PaddleOcrVl;
use super::{EngineError, EngineResult};
use crate::models::{self, ModelCategory};
use crate::resource_limits;
use crate::settings::Settings;

/// Bump this any time a change here could change this engine's output for
/// the same input -- a new translation/OCR/detector/inpainting model, a
/// decoding fix, a compose.rs layout change. jobs.rs folds this into its
/// cache key so a swap like NLLB -> a local LLM can never silently keep
/// serving an old model's cached result for a page the user already
/// translated once. Adding a new catalog *option* for an existing stage
/// does NOT need a bump on its own -- jobs.rs's cache key already
/// includes each stage's selected option id, so a new option can never
/// collide with an old one's cache entry; only bump this when the code
/// itself changes behavior for an option that already existed.
pub const PIPELINE_VERSION: &str = "v7-inpaint-coreml-fix";

/// Two architecturally different OCR backends today (manga_ocr.rs's ONNX
/// encoder-decoder vs. paddle_ocr_vl.rs's llama.cpp/mtmd vision-language
/// model) -- a plain enum with match-dispatch, not a trait object, to
/// match this codebase's existing style (config-driven `match
/// key.xxx_model.as_str() { ... }` everywhere else; no `dyn`/`trait`
/// anywhere in this crate).
enum OcrModel {
    MangaOcr(MangaOcr),
    PaddleOcrVl(PaddleOcrVl),
    BaberuOcr(BaberuOcr),
}

impl OcrModel {
    fn read(&self, image: &RgbaImage) -> Result<String, String> {
        match self {
            OcrModel::MangaOcr(m) => m.read(image),
            OcrModel::PaddleOcrVl(m) => m.read(image),
            OcrModel::BaberuOcr(m) => m.read(image),
        }
    }
}

struct LocalModels {
    ocr: OcrModel,
    translator: LocalLlm,
    inpainter: LamaInpainter,
}

#[derive(Clone, PartialEq, Eq)]
struct ModelsCacheKey {
    ocr_model: String,
    inpainting_model: String,
    translation_model: String,
    // Part of the cache key (not just a load-time parameter) because
    // changing the VRAM budget has to force a reload with the new
    // n_gpu_layers -- otherwise an already-cached combination would keep
    // running with whatever budget was in effect when it was first loaded.
    max_vram_mb: u32,
}

impl ModelsCacheKey {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            ocr_model: settings.ocr_model.clone(),
            inpainting_model: settings.inpainting_model.clone(),
            translation_model: settings.translation_model.clone(),
            max_vram_mb: settings.max_vram_mb,
        }
    }
}

// Holds at most one loaded combination at a time -- these are multi-GB
// models, so switching a selection in settings drops the old combo
// (freeing that memory) rather than keeping every combo ever used loaded
// forever. Re-selecting a previous combo pays the load cost again; that
// trade favors not silently holding onto gigabytes of RAM for a choice
// the user already moved away from.
static MODELS: Mutex<Option<(ModelsCacheKey, Arc<LocalModels>)>> = Mutex::new(None);

fn load_models(app: &AppHandle, settings: &Settings) -> Result<Arc<LocalModels>, String> {
    let key = ModelsCacheKey::from_settings(settings);

    if let Some((cached_key, models)) = MODELS.lock().unwrap().as_ref() {
        if cached_key == &key {
            return Ok(models.clone());
        }
    }

    let built = Arc::new(build_models(app, &key)?);
    *MODELS.lock().unwrap() = Some((key, built.clone()));
    Ok(built)
}

/// The one place a catalog option id (models.rs) turns into an actual
/// loaded engine. Each arm here is a real, working integration -- adding a
/// new catalog entry for a stage without a matching arm here would just
/// fail at load time with "Unknown ... model", not silently do nothing.
fn build_models(app: &AppHandle, key: &ModelsCacheKey) -> Result<LocalModels, String> {
    let path = |name: &str| models::model_path(app, name);
    // Resolves to "every layer" (today's unchanged behavior) when
    // key.max_vram_mb is 0 -- see resource_limits::gpu_layers_for_budget.
    // Takes the *main* model file's path, not the mmproj -- the mmproj's
    // vision tower is comparatively tiny, not worth budgeting separately.
    let gpu_layers = |main_model_name: &str| resource_limits::gpu_layers_for_budget(&path(main_model_name), key.max_vram_mb);

    let ocr = match key.ocr_model.as_str() {
        "manga-ocr" => OcrModel::MangaOcr(MangaOcr::load(
            &path("manga-ocr-encoder.onnx"),
            &path("manga-ocr-decoder-cross.onnx"),
            &path("manga-ocr-decoder-step.onnx"),
            &path("manga-ocr-vocab.txt"),
        )?),
        "paddle-ocr-vl-1.6" => OcrModel::PaddleOcrVl(PaddleOcrVl::load(
            &path("paddle-ocr-vl-1.6.gguf"),
            &path("paddle-ocr-vl-1.6-mmproj.gguf"),
            gpu_layers("paddle-ocr-vl-1.6.gguf"),
        )?),
        "baberu-ocr" => OcrModel::BaberuOcr(BaberuOcr::load(
            &path("baberu-ocr-vision.onnx"),
            &path("baberu-ocr-decoder-prefill.onnx"),
            &path("baberu-ocr-decoder-step.onnx"),
            &path("baberu-ocr-vocab.json"),
        )?),
        other => return Err(format!("Unknown OCR model '{other}'.")),
    };

    let translator = match key.translation_model.as_str() {
        "qwen2.5-vl-7b-instruct" => LocalLlm::load(
            &path("qwen2.5-vl-7b-instruct-q4_k_m.gguf"),
            Some(&path("qwen2.5-vl-7b-instruct-mmproj.gguf")),
            gpu_layers("qwen2.5-vl-7b-instruct-q4_k_m.gguf"),
        )?,
        "gemma-4-12b-it" => LocalLlm::load(
            &path("gemma-4-12b-it-q4_k_xl.gguf"),
            Some(&path("gemma-4-12b-it-mmproj.gguf")),
            gpu_layers("gemma-4-12b-it-q4_k_xl.gguf"),
        )?,
        "gemma-4-12b-uncensored" => LocalLlm::load(
            &path("gemma-4-12b-uncensored-q4_k_m.gguf"),
            Some(&path("gemma-4-12b-uncensored-mmproj.gguf")),
            gpu_layers("gemma-4-12b-uncensored-q4_k_m.gguf"),
        )?,
        "gemma-4-e2b-it" => LocalLlm::load(
            &path("gemma-4-e2b-it-q4_k_xl.gguf"),
            Some(&path("gemma-4-e2b-it-mmproj.gguf")),
            gpu_layers("gemma-4-e2b-it-q4_k_xl.gguf"),
        )?,
        "gemma-4-e2b-uncensored" => LocalLlm::load(
            &path("gemma-4-e2b-uncensored-q4_k_p.gguf"),
            Some(&path("gemma-4-e2b-uncensored-mmproj.gguf")),
            gpu_layers("gemma-4-e2b-uncensored-q4_k_p.gguf"),
        )?,
        "gemma-4-e4b-it" => LocalLlm::load(
            &path("gemma-4-e4b-it-q4_k_xl.gguf"),
            Some(&path("gemma-4-e4b-it-mmproj.gguf")),
            gpu_layers("gemma-4-e4b-it-q4_k_xl.gguf"),
        )?,
        "gemma-4-e4b-uncensored" => LocalLlm::load(
            &path("gemma-4-e4b-uncensored-q4_k_p.gguf"),
            Some(&path("gemma-4-e4b-uncensored-mmproj.gguf")),
            gpu_layers("gemma-4-e4b-uncensored-q4_k_p.gguf"),
        )?,
        "gemma-4-26b-a4b-it" => LocalLlm::load(
            &path("gemma-4-26b-a4b-it-q4_k_xl.gguf"),
            Some(&path("gemma-4-26b-a4b-it-mmproj.gguf")),
            gpu_layers("gemma-4-26b-a4b-it-q4_k_xl.gguf"),
        )?,
        "gemma-4-26b-a4b-uncensored" => LocalLlm::load(
            &path("gemma-4-26b-a4b-uncensored-q4_k_m.gguf"),
            Some(&path("gemma-4-26b-a4b-uncensored-mmproj.gguf")),
            gpu_layers("gemma-4-26b-a4b-uncensored-q4_k_m.gguf"),
        )?,
        "gemma-4-31b-it" => LocalLlm::load(
            &path("gemma-4-31b-it-q4_k_xl.gguf"),
            Some(&path("gemma-4-31b-it-mmproj.gguf")),
            gpu_layers("gemma-4-31b-it-q4_k_xl.gguf"),
        )?,
        "gemma-4-31b-uncensored" => LocalLlm::load(
            &path("gemma-4-31b-uncensored-q4_k_m.gguf"),
            Some(&path("gemma-4-31b-uncensored-mmproj.gguf")),
            gpu_layers("gemma-4-31b-uncensored-q4_k_m.gguf"),
        )?,
        // Text-only (no vision tower) -- see local_llm.rs's mtmd_ctx: None
        // path and models.rs's catalog comment on why these two lack a
        // projector file.
        "lfm2.5-1.2b-instruct" => {
            LocalLlm::load(&path("lfm2.5-1.2b-instruct-q4_k_m.gguf"), None, gpu_layers("lfm2.5-1.2b-instruct-q4_k_m.gguf"))?
        }
        "ministral-3-8b-instruct" => LocalLlm::load(
            &path("ministral-3-8b-instruct-q4_k_m.gguf"),
            None,
            gpu_layers("ministral-3-8b-instruct-q4_k_m.gguf"),
        )?,
        other => return Err(format!("Unknown translation model '{other}'.")),
    };

    let inpainter = match key.inpainting_model.as_str() {
        "lama" => LamaInpainter::load(&path("lama-inpainting-fp32.onnx"))?,
        other => return Err(format!("Unknown inpainting model '{other}'.")),
    };

    Ok(LocalModels { ocr, translator, inpainter })
}

/// Which of the user's currently-selected options (one per stage) aren't
/// downloaded yet, as human-readable labels -- empty means the local
/// engine is ready to run as configured.
fn missing_selected_options(app: &AppHandle, settings: &Settings) -> Vec<String> {
    [
        (ModelCategory::Ocr, settings.ocr_model.as_str()),
        (ModelCategory::Inpainting, settings.inpainting_model.as_str()),
        (ModelCategory::Translation, settings.translation_model.as_str()),
    ]
    .into_iter()
    .filter_map(|(_, id)| match models::find_option(id) {
        Some(opt) if models::is_downloaded(app, opt) => None,
        Some(opt) => Some(opt.label.to_string()),
        None => Some(format!("unknown model '{id}'")),
    })
    .collect()
}

pub async fn translate(
    app_handle: AppHandle,
    image_bytes: Vec<u8>,
    target_lang: &str,
) -> Result<EngineResult, EngineError> {
    let settings = crate::settings::load(&app_handle);
    let missing = missing_selected_options(&app_handle, &settings);
    if !missing.is_empty() {
        return Err(EngineError::new(
            "ENGINE_UNAVAILABLE",
            format!(
                "The local engine's selected model(s) aren't downloaded yet: {}. Open the app's \
                 settings window and download them.",
                missing.join(", ")
            ),
        ));
    }

    let target_lang = target_lang.to_string();
    tokio::task::spawn_blocking(move || run_blocking(&app_handle, &settings, image_bytes, &target_lang))
        .await
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Local engine task panicked: {e}")))?
}

fn run_blocking(
    app: &AppHandle,
    settings: &Settings,
    image_bytes: Vec<u8>,
    target_lang: &str,
) -> Result<EngineResult, EngineError> {
    let models = load_models(app, settings).map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e))?;
    let generation = local_llm::GenerationParams {
        temperature: settings.llm_temperature,
        top_k: settings.llm_top_k,
        top_p: settings.llm_top_p,
        min_p: settings.llm_min_p,
        max_tokens: settings.llm_max_tokens,
        repeat_penalty: settings.llm_repeat_penalty,
        frequency_penalty: settings.llm_frequency_penalty,
        presence_penalty: settings.llm_presence_penalty,
    };
    let fonts = compose::load_font_stack(&settings.font_paths);
    translate_image(&models, image_bytes, target_lang, &fonts, settings.detection_score_threshold, &generation)
}

fn translate_image(
    models: &LocalModels,
    image_bytes: Vec<u8>,
    target_lang: &str,
    fonts: &[FontArc],
    detection_score_threshold: f32,
    generation: &local_llm::GenerationParams,
) -> Result<EngineResult, EngineError> {
    let mut image: RgbaImage = image::load_from_memory(&image_bytes)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Could not decode source image: {e}")))?
        .to_rgba8();
    let (img_w, img_h) = image.dimensions();

    let detections = detect::detect_text_regions(&image, detection_score_threshold)
        .map_err(|e| EngineError::new("OCR_FAILED", format!("Local text detection failed: {e}")))?;
    if detections.is_empty() {
        return Err(EngineError::new("NO_TRANSLATION_NEEDED", "No text regions detected in this image."));
    }

    // OCR every region first; translation happens once, in a single call
    // grounded in the whole page image (see local_llm.rs's module docs for
    // why that's worth the restructuring -- scene/character context a
    // per-region text-only call can't have).
    let mut ocr_regions: Vec<&detect::DetectedRegion> = Vec::new();
    let mut original_texts: Vec<String> = Vec::new();
    for d in &detections {
        let x = d.x.clamp(0, img_w as i64) as u32;
        let y = d.y.clamp(0, img_h as i64) as u32;
        let w = (d.width as u32).min(img_w.saturating_sub(x)).max(1);
        let h = (d.height as u32).min(img_h.saturating_sub(y)).max(1);
        let crop = image::imageops::crop_imm(&image, x, y, w, h).to_image();

        match models.ocr.read(&crop) {
            Ok(t) if !t.trim().is_empty() => {
                ocr_regions.push(d);
                original_texts.push(t);
            }
            Ok(_) => continue,
            Err(e) => {
                eprintln!("[local engine] OCR failed for a region, skipping it: {e}");
                continue;
            }
        }
    }

    if ocr_regions.is_empty() {
        return Err(EngineError::new(
            "NO_TRANSLATION_NEEDED",
            "Detected regions, but none contained readable text.",
        ));
    }

    let translated_texts = models
        .translator
        .translate_batch(&image, &original_texts, target_lang, generation)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", e))?;

    let mut regions: Vec<TextRegion> = Vec::new();
    for (d, translated_text) in ocr_regions.into_iter().zip(translated_texts.into_iter()) {
        if translated_text.trim().is_empty() {
            continue;
        }
        regions.push(TextRegion {
            x: d.x,
            y: d.y,
            width: d.width,
            height: d.height,
            translated_text,
            is_round: d.kind == RegionKind::TextInBubble,
        });
    }

    if regions.is_empty() {
        return Err(EngineError::new(
            "NO_TRANSLATION_NEEDED",
            "Detected regions, but the translator returned nothing usable for any of them.",
        ));
    }

    compose::redraw_regions(&mut image, &regions, fonts, Some(&models.inpainter));

    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Could not encode result image: {e}")))?;

    Ok(EngineResult { image_bytes: out, engine_name: "local" })
}

#[cfg(test)]
mod manual_tests {
    use super::*;

    /// Not run in normal `cargo test`. Exercises the full local pipeline
    /// (detect -> OCR -> translate -> compose) against a real manga page,
    /// bypassing the AppHandle-based model loader so it can run outside a
    /// Tauri app. Set MWT_MODELS_DIR, MWT_TEST_IMAGE and MWT_TEST_OUT.
    #[test]
    fn translate_real_page() {
        let (Ok(models_dir), Ok(image_path), Ok(out_path)) = (
            std::env::var("MWT_MODELS_DIR"),
            std::env::var("MWT_TEST_IMAGE"),
            std::env::var("MWT_TEST_OUT"),
        ) else {
            eprintln!("skipping: set MWT_MODELS_DIR, MWT_TEST_IMAGE, MWT_TEST_OUT to run this");
            return;
        };
        let dir = std::path::Path::new(&models_dir);
        let ocr = OcrModel::MangaOcr(
            MangaOcr::load(
                &dir.join("manga-ocr-encoder.onnx"),
                &dir.join("manga-ocr-decoder-cross.onnx"),
                &dir.join("manga-ocr-decoder-step.onnx"),
                &dir.join("manga-ocr-vocab.txt"),
            )
            .expect("load manga-ocr"),
        );
        let translator = LocalLlm::load(
            &dir.join("qwen2.5-vl-7b-instruct-q4_k_m.gguf"),
            Some(&dir.join("qwen2.5-vl-7b-instruct-mmproj.gguf")),
            1000,
        )
        .expect("load local llm");
        let inpainter = LamaInpainter::load(&dir.join("lama-inpainting-fp32.onnx")).expect("load lama");
        let models = LocalModels { ocr, translator, inpainter };

        let image_bytes = std::fs::read(&image_path).expect("read test image");
        let fonts = compose::load_font_stack(&[]);
        let result = translate_image(
            &models,
            image_bytes,
            "en",
            &fonts,
            detect::DEFAULT_SCORE_THRESHOLD,
            &local_llm::GenerationParams::default(),
        )
        .expect("translate_image");
        std::fs::write(&out_path, &result.image_bytes).expect("write output image");
        eprintln!("wrote {out_path}");
    }
}
