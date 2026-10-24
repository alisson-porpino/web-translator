//! Local, offline multilingual OCR via PaddleOCR-VL-1.6
//! (PaddlePaddle/PaddleOCR-VL-1.6-GGUF, Apache-2.0), run through the same
//! llama.cpp/mtmd machinery local_llm.rs uses for vision-grounded
//! translation models -- mainline llama.cpp gained native support for this
//! architecture (ggml-org/llama.cpp#18825). Unlike manga_ocr.rs
//! (Japanese-only, ONNX encoder-decoder), this model is multilingual and
//! vision-only: there is no text-only fallback path the way local_llm.rs
//! has one, because without vision this engine has no OCR model to run at
//! all.
//!
//! A crop of a single detected text region goes in (same contract as
//! manga_ocr.rs's `read`), the recognized text comes out. Prompted with
//! the model's own plain-text-recognition mode ("OCR:") -- the model also
//! supports Formula/Table/Chart/Seal/Spotting recognition via different
//! prompts, none of which this pipeline stage needs.

use image::RgbaImage;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::mtmd::{mtmd_default_marker, MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText};
use llama_cpp_2::sampling::LlamaSampler;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Mutex;

// A single speech-bubble's worth of recognized text is normally a short
// phrase/sentence, nowhere near local_llm.rs's MAX_NEW_TOKENS=2000 (which
// covers a whole page's numbered translation list).
const MAX_NEW_TOKENS: usize = 200;
// Small context: one cropped region's image tokens + a two-word prompt +
// MAX_NEW_TOKENS output -- much smaller than local_llm.rs's N_CTX=8192
// (whole page image + numbered OCR list).
const N_CTX: u32 = 2048;
// See local_llm.rs's MTMD_EVAL_BATCH doc comment -- same fix, same bug
// (eval_chunks's n_batch hardcoded to 1, forcing one-token-at-a-time image
// decode), just affecting per-region OCR latency here instead of the
// whole-page translation call.
const MTMD_EVAL_BATCH: i32 = 512;

fn n_threads() -> i32 {
    crate::resource_limits::effective_thread_count()
}

pub struct PaddleOcrVl {
    model: LlamaModel,
    mtmd_ctx: Mutex<MtmdContext>,
}

impl PaddleOcrVl {
    /// Vision is not optional here, unlike LocalLlm::load -- there is
    /// nothing for a text-only OCR model to read.
    pub fn load(model_path: &Path, mmproj_path: &Path, gpu_layers: u32) -> Result<Self, String> {
        let backend = super::local_llm::backend()?;
        let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
        let model = LlamaModel::load_from_file(backend, model_path, &model_params).map_err(|e| e.to_string())?;

        let mmproj_str = mmproj_path.to_str().ok_or("mmproj path is not valid UTF-8")?;
        let mtmd_params = MtmdContextParams {
            use_gpu: true,
            print_timings: false,
            n_threads: n_threads(),
            media_marker: CString::new(mtmd_default_marker()).map_err(|e| e.to_string())?,
            image_min_tokens: -1,
            image_max_tokens: -1,
        };
        let ctx = MtmdContext::init_from_file(mmproj_str, &model, &mtmd_params).map_err(|e| e.to_string())?;

        Ok(Self { model, mtmd_ctx: Mutex::new(ctx) })
    }

    /// Reads whatever text is in `image` (expected to already be cropped
    /// to roughly one text region -- see engines/detect.rs). Multilingual,
    /// unlike manga_ocr.rs's Japanese-only `read`.
    pub fn read(&self, image: &RgbaImage) -> Result<String, String> {
        let backend = super::local_llm::backend()?;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_threads(n_threads())
            .with_n_threads_batch(n_threads());
        let mut ctx = self.model.new_context(backend, ctx_params).map_err(|e| e.to_string())?;

        let marker = mtmd_default_marker();
        // Plain-text recognition mode, per the model's own README:
        // `llama-cli ... -p 'OCR:' --image ...`. Other prompts select
        // Formula/Table/Chart/Seal/Spotting recognition instead -- not
        // what this pipeline stage needs.
        //
        // Built by hand, NOT via Model::chat_template()/apply_chat_template()
        // (what local_llm.rs uses for Qwen/Gemma): this model's own
        // chat_template.jinja uses a non-standard "User: ...\nAssistant:\n"
        // format llama.cpp's `llama_chat_apply_template` C function doesn't
        // recognize (it only pattern-matches a small hardcoded list of
        // well-known template families, it's not a Jinja interpreter) --
        // confirmed empirically: calling it here returned FfiError(-1).
        // This mirrors the jinja template's own user/assistant-turn
        // structure by hand instead. No leading BOS token here --
        // `MtmdInputText { add_special: true, .. }` below already adds the
        // model's special BOS at the tokenizer level.
        let prompt = format!("User: {marker}OCR:\nAssistant:\n");

        let rgb = to_rgb_bytes(image);
        let mut mtmd_ctx = self.mtmd_ctx.lock().map_err(|e| e.to_string())?;
        let bitmap = MtmdBitmap::from_image_data(image.width(), image.height(), &rgb).map_err(|e| e.to_string())?;

        let input_text = MtmdInputText { text: prompt, add_special: true, parse_special: true };
        let chunks = mtmd_ctx.tokenize(input_text, &[&bitmap]).map_err(|e| e.to_string())?;
        let mut n_past =
            chunks.eval_chunks(&mut mtmd_ctx, &mut ctx, 0, 0, MTMD_EVAL_BATCH, true).map_err(|e| e.to_string())?;
        drop(mtmd_ctx);

        let mut batch = LlamaBatch::new(512, 1);
        let mut sampler = LlamaSampler::greedy();
        let mut generated: Vec<i64> = Vec::new();
        let mut generated_bytes: Vec<u8> = Vec::new();

        for _ in 0..MAX_NEW_TOKENS {
            let token = sampler.sample(&ctx, -1);
            sampler.accept(token);
            if self.model.vocab().is_eog(token) {
                break;
            }
            generated.push(token.0 as i64);
            if has_degenerate_repetition(&generated) {
                break;
            }
            generated_bytes.extend(self.model.vocab().token_to_piece(token, true, None));

            batch.clear();
            batch.add(token, n_past, &[0], true).map_err(|e| e.to_string())?;
            n_past += 1;
            ctx.decode(&mut batch).map_err(|e| e.to_string())?;
        }

        Ok(String::from_utf8_lossy(&generated_bytes).trim().to_string())
    }
}

/// Converts RGBA to tightly-packed RGB (what `MtmdBitmap::from_image_data`
/// expects), dropping alpha.
fn to_rgb_bytes(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(image.width() as usize * image.height() as usize * 3);
    for px in image.pixels() {
        out.extend_from_slice(&px.0[..3]);
    }
    out
}

/// Same degenerate-loop guard local_llm.rs uses -- copied rather than
/// shared, matching this codebase's existing style of small
/// self-contained engine files (manga_ocr.rs's own `argmax` is likewise
/// not shared with any other ONNX engine).
fn has_degenerate_repetition(generated: &[i64]) -> bool {
    const MIN_REPEATS: usize = 4;
    for period in 1..=3 {
        let window = period * MIN_REPEATS;
        if generated.len() < window {
            continue;
        }
        let tail = &generated[generated.len() - window..];
        let cycle = &tail[tail.len() - period..];
        if tail.chunks(period).all(|chunk| chunk == cycle) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod manual_tests {
    use super::*;
    use ab_glyph::{FontArc, PxScale};
    use imageproc::drawing::draw_text_mut;

    /// Not run in normal `cargo test`. Set MWT_PADDLE_OCR_VL_MODEL (main
    /// GGUF) and MWT_PADDLE_OCR_VL_MMPROJ (vision projector GGUF) to
    /// sanity-check OCR output before wiring this into the app.
    ///
    /// Renders a short, known English phrase onto a synthetic image and
    /// checks the OCR'd text roughly matches it -- real recognition
    /// quality, not just "doesn't crash on a blank image" like a
    /// synthetic no-op check would be, and it also exercises the
    /// multilingual angle this engine adds over manga-ocr. Deliberately
    /// not a real manga crop: no OCR test fixture exists in this repo
    /// (nor should one be added by downloading copyrighted page scans --
    /// see DIRETRIZES.md's licensing/ethics boundary).
    #[test]
    fn reads_rendered_english_text() {
        let (Ok(model_path), Ok(mmproj_path)) =
            (std::env::var("MWT_PADDLE_OCR_VL_MODEL"), std::env::var("MWT_PADDLE_OCR_VL_MMPROJ"))
        else {
            eprintln!("skipping: set MWT_PADDLE_OCR_VL_MODEL and MWT_PADDLE_OCR_VL_MMPROJ to run this");
            return;
        };
        let ocr = PaddleOcrVl::load(std::path::Path::new(&model_path), std::path::Path::new(&mmproj_path), 1000)
            .expect("load paddle-ocr-vl");

        let mut image = RgbaImage::from_pixel(420, 120, image::Rgba([255, 255, 255, 255]));
        let font =
            FontArc::try_from_slice(include_bytes!("../../assets/NotoSansJP-Bold.ttf")).expect("load test font");
        let text = "HELLO FRIEND";
        draw_text_mut(&mut image, image::Rgba([0, 0, 0, 255]), 20, 30, PxScale::from(48.0), &font, text);

        let start = std::time::Instant::now();
        let result = ocr.read(&image).expect("ocr read");
        eprintln!("OCR result: {result:?} ({:?})", start.elapsed());
        assert!(
            result.to_uppercase().contains("HELLO") && result.to_uppercase().contains("FRIEND"),
            "expected the rendered text to be recognized, got {result:?}"
        );
    }
}
