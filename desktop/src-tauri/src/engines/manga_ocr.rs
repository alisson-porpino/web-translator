//! Local, offline Japanese text recognition: kha-white/manga-ocr-base
//! (Apache-2.0), exported to ONNX fp16 by hon-lu (same author/lineage as
//! the bundled detector model). Vision-encoder-decoder: a crop of a text
//! region goes in, the recognized string comes out. Japanese only -- see
//! module docs on nllb.rs for what that means for the rest of the
//! pipeline.
//!
//! The decoder is split into two small graphs (decoder-cross, computed
//! once per image; decoder-step, run once per generated character) rather
//! than one "merged" graph like nllb.rs's -- a deliberately simpler export
//! by the model's author, with the per-layer K/V stacked into a single
//! tensor (dim 0 = 2 decoder layers) instead of named per-layer tensors.

use image::RgbaImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

const INPUT_SIZE: u32 = 224;
const CLS: i64 = 2;
const SEP: i64 = 3;
const FIRST_REAL_TOKEN_ID: i64 = 5; // 0..=4 are [PAD] [UNK] [CLS] [SEP] [MASK]
const MAX_NEW_TOKENS: usize = 64;
const NUM_DECODER_LAYERS: i64 = 2;
const NUM_HEADS: i64 = 12;
const HEAD_DIM: i64 = 64;

pub struct MangaOcr {
    encoder: Mutex<Session>,
    decoder_cross: Mutex<Session>,
    decoder_step: Mutex<Session>,
    vocab: Vec<String>,
}

impl MangaOcr {
    pub fn load(
        encoder_path: &Path,
        decoder_cross_path: &Path,
        decoder_step_path: &Path,
        vocab_path: &Path,
    ) -> Result<Self, String> {
        let environment = super::ort_env::shared()?;

        let build = |path: &Path| -> Result<Session, String> {
            let mut builder = Session::builder(&environment).map_err(|e| e.to_string())?;
            builder = builder.with_optimization_level(GraphOptimizationLevel::Level3).map_err(|e| e.to_string())?;
            builder = builder
                .with_intra_threads(crate::resource_limits::effective_thread_count() as usize)
                .map_err(|e| e.to_string())?;
            builder.commit_from_file(path).map_err(|e| e.to_string())
        };

        let encoder = build(encoder_path)?;
        let decoder_cross = build(decoder_cross_path)?;
        let decoder_step = build(decoder_step_path)?;

        let vocab_text = std::fs::read_to_string(vocab_path).map_err(|e| e.to_string())?;
        let vocab: Vec<String> = vocab_text.lines().map(|s| s.to_string()).collect();

        Ok(Self { encoder: Mutex::new(encoder), decoder_cross: Mutex::new(decoder_cross), decoder_step: Mutex::new(decoder_step), vocab })
    }

    /// Reads whatever Japanese text is in `image` (expected to already be
    /// cropped to roughly one text region -- see engines/detect.rs).
    pub fn read(&self, image: &RgbaImage) -> Result<String, String> {
        let pixel_values = preprocess(image);

        let encoder_hidden: (Vec<i64>, Vec<f32>) = {
            let mut encoder = self.encoder.lock().map_err(|e| e.to_string())?;
            let tensor = Tensor::from_array(([1i64, 3, INPUT_SIZE as i64, INPUT_SIZE as i64], pixel_values))
                .map_err(|e| e.to_string())?;
            let outputs = encoder.run(ort::inputs! { "pixel_values" => tensor }).map_err(|e| e.to_string())?;
            let (shape, data) = outputs["last_hidden_state"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            (shape.iter().map(|&d| d as i64).collect(), data.to_vec())
        };
        let seq_len = encoder_hidden.0[1];

        let (cross_k, cross_v): (Vec<f32>, Vec<f32>) = {
            let mut decoder_cross = self.decoder_cross.lock().map_err(|e| e.to_string())?;
            let tensor =
                Tensor::from_array(([1i64, seq_len, 768], encoder_hidden.1)).map_err(|e| e.to_string())?;
            let outputs =
                decoder_cross.run(ort::inputs! { "encoder_hidden_states" => tensor }).map_err(|e| e.to_string())?;
            let (_, k) = outputs["cross_k"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (_, v) = outputs["cross_v"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            (k.to_vec(), v.to_vec())
        };

        let mut decoder_step = self.decoder_step.lock().map_err(|e| e.to_string())?;
        let mut past_k: Vec<f32> = Vec::new();
        let mut past_v: Vec<f32> = Vec::new();
        let mut past_len: i64 = 0;
        let mut token: i64 = CLS;
        let mut output_ids: Vec<i64> = Vec::new();

        for step in 0..MAX_NEW_TOKENS {
            let input_ids = Tensor::from_array(([1i64, 1], vec![token])).map_err(|e| e.to_string())?;
            let position_ids = Tensor::from_array(([1i64, 1], vec![step as i64])).map_err(|e| e.to_string())?;
            let past_k_t =
                Tensor::from_array(([NUM_DECODER_LAYERS, 1, NUM_HEADS, past_len, HEAD_DIM], past_k.clone())).map_err(|e| e.to_string())?;
            let past_v_t =
                Tensor::from_array(([NUM_DECODER_LAYERS, 1, NUM_HEADS, past_len, HEAD_DIM], past_v.clone())).map_err(|e| e.to_string())?;
            let cross_k_t =
                Tensor::from_array(([NUM_DECODER_LAYERS, 1, NUM_HEADS, seq_len, HEAD_DIM], cross_k.clone())).map_err(|e| e.to_string())?;
            let cross_v_t =
                Tensor::from_array(([NUM_DECODER_LAYERS, 1, NUM_HEADS, seq_len, HEAD_DIM], cross_v.clone())).map_err(|e| e.to_string())?;

            let outputs = decoder_step
                .run(ort::inputs! {
                    "input_ids" => input_ids,
                    "position_ids" => position_ids,
                    "past_k" => past_k_t,
                    "past_v" => past_v_t,
                    "cross_k" => cross_k_t,
                    "cross_v" => cross_v_t,
                })
                .map_err(|e| e.to_string())?;

            let (_, logits) = outputs["logits"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let next_token = argmax(logits) as i64;

            let (_, present_k) = outputs["present_k"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (_, present_v) = outputs["present_v"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            past_k = present_k.to_vec();
            past_v = present_v.to_vec();
            past_len += 1;
            drop(outputs);

            if next_token == SEP {
                break;
            }
            output_ids.push(next_token);
            token = next_token;
        }

        let text: String = output_ids
            .into_iter()
            .filter(|&id| id >= FIRST_REAL_TOKEN_ID)
            .filter_map(|id| self.vocab.get(id as usize).cloned())
            .collect();
        Ok(text)
    }
}

/// grayscale -> RGB (channel-replicated) -> resize 224x224 bilinear ->
/// normalize to [-1, 1], exactly matching the documented preprocessing
/// (see module docs) -- not the usual ImageNet mean/std, this model was
/// trained with simple /127.5 - 1 scaling.
fn preprocess(image: &RgbaImage) -> Vec<f32> {
    let gray = image::imageops::grayscale(image);
    let rgb = image::DynamicImage::ImageLuma8(gray).to_rgb8();
    let resized = image::imageops::resize(&rgb, INPUT_SIZE, INPUT_SIZE, image::imageops::FilterType::Triangle);

    let plane = (INPUT_SIZE * INPUT_SIZE) as usize;
    let mut out = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        out[i] = px[0] as f32 / 127.5 - 1.0;
        out[plane + i] = px[1] as f32 / 127.5 - 1.0;
        out[2 * plane + i] = px[2] as f32 / 127.5 - 1.0;
    }
    out
}

fn argmax(values: &[f32]) -> usize {
    let mut best_idx = 0;
    let mut best_val = f32::NEG_INFINITY;
    for (i, &v) in values.iter().enumerate() {
        if v > best_val {
            best_val = v;
            best_idx = i;
        }
    }
    best_idx
}

#[cfg(test)]
mod manual_tests {
    use super::*;

    /// Not run in normal `cargo test`. Set MWT_MANGA_OCR_TEST_IMAGE to a
    /// cropped text-region image (a real manga speech bubble works best)
    /// and MWT_MODELS_DIR to the directory holding the downloaded model
    /// files to sanity-check OCR output before wiring this into the app.
    #[test]
    fn read_real_crop() {
        let (Ok(image_path), Ok(models_dir)) =
            (std::env::var("MWT_MANGA_OCR_TEST_IMAGE"), std::env::var("MWT_MODELS_DIR"))
        else {
            eprintln!("skipping: set MWT_MANGA_OCR_TEST_IMAGE and MWT_MODELS_DIR to run this");
            return;
        };
        let dir = std::path::Path::new(&models_dir);
        let ocr = MangaOcr::load(
            &dir.join("manga-ocr-encoder.onnx"),
            &dir.join("manga-ocr-decoder-cross.onnx"),
            &dir.join("manga-ocr-decoder-step.onnx"),
            &dir.join("manga-ocr-vocab.txt"),
        )
        .expect("load manga-ocr");

        let image = image::open(&image_path).expect("open crop").to_rgba8();
        let text = ocr.read(&image).expect("ocr read");
        eprintln!("OCR result: {text:?}");
        assert!(!text.is_empty(), "expected non-empty OCR output for a real text crop");
    }
}
