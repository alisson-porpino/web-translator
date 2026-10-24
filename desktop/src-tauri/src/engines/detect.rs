//! Local, embedded text-region detection: ogkalu/comic-text-and-bubble-detector
//! (RT-DETR-v2, fine-tuned on ~11k manga/manhwa/manhua/western-comic images),
//! exported to ONNX fp16 by hon-lu (Apache-2.0, see assets/LICENSE-comic-detector.txt),
//! run locally via ONNX Runtime -- no network call, no separate process.
//!
//! This replaces asking Gemini to both *find* and *read* text regions in one
//! shot, which is where nearly every bug so far has come from (missed
//! bubbles, wrong/overlapping boxes). A model actually trained for spatial
//! detection does that part; Gemini is only asked to read + translate text
//! inside boxes this already found, which it's reliable at.
//!
//! Preprocessing/postprocessing here mirrors the reference C# implementation
//! in Hon-Lu/OverTranslate (MangaTextDetector.cs) as closely as possible,
//! since the model's own card doesn't document these details and getting
//! them wrong silently produces garbage detections.

use image::RgbaImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::sync::{Mutex, OnceLock};

const INPUT_SIZE: u32 = 640;
/// Matches the reference implementation; the model card separately confirms
/// 0.3 is where real detections end and noise begins. This is also the
/// default for the user-configurable threshold `detect_text_regions` now
/// takes (see Settings::detection_score_threshold) -- a page with faint or
/// unusual lettering may need it lowered, a noisy page may need it raised.
#[allow(dead_code)] // used by manual tests (cfg(test)); production code reads the configured value from Settings instead
pub const DEFAULT_SCORE_THRESHOLD: f32 = 0.3;
/// Boxes thinner than this are slivers, not text.
const MIN_SIDE: f32 = 4.0;
/// RT-DETR sometimes answers text_bubble and text_free for the same text;
/// when two kept boxes overlap this much (relative to the smaller one's
/// area), drop the lower-scoring one.
const DUPLICATE_OVERLAP: f32 = 0.7;

static MODEL_BYTES: &[u8] = include_bytes!("../../assets/detector.fp16.onnx");
static SESSION: OnceLock<Result<Mutex<Session>, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Bubble,
    TextInBubble,
    TextOutsideBubble,
}

impl RegionKind {
    fn from_label(label: i64) -> Option<Self> {
        match label {
            0 => Some(Self::Bubble),
            1 => Some(Self::TextInBubble),
            2 => Some(Self::TextOutsideBubble),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DetectedRegion {
    pub kind: RegionKind,
    pub score: f32,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

fn build_session() -> ort::Result<Session> {
    let environment = super::ort_env::shared().map_err(ort::Error::new)?;
    let mut builder = Session::builder(&environment)?;
    builder = builder.with_optimization_level(GraphOptimizationLevel::Level3)?;
    builder = builder.with_intra_threads(crate::resource_limits::effective_thread_count() as usize)?;
    builder.commit_from_memory(MODEL_BYTES)
}

fn session() -> Result<&'static Mutex<Session>, String> {
    let result = SESSION.get_or_init(|| {
        build_session().map(Mutex::new).map_err(|e| format!("Could not load the bundled detection model: {e}"))
    });
    result.as_ref().map_err(|e| e.clone())
}

/// Returns every detected text region (text_bubble + text_free), in the
/// original image's pixel coordinates, deduplicated. Bubble outlines
/// (class 0) are detected too but not returned -- we only need where the
/// text itself is to erase and redraw it. `score_threshold` is the
/// user-configurable confidence floor (see DEFAULT_SCORE_THRESHOLD) --
/// detections below it are discarded as noise.
pub fn detect_text_regions(image: &RgbaImage, score_threshold: f32) -> Result<Vec<DetectedRegion>, String> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Ok(Vec::new());
    }

    let resized = image::imageops::resize(image, INPUT_SIZE, INPUT_SIZE, image::imageops::FilterType::Triangle);
    let plane = (INPUT_SIZE * INPUT_SIZE) as usize;
    let mut input = vec![0f32; 3 * plane];
    for (i, pixel) in resized.pixels().enumerate() {
        input[i] = pixel[0] as f32 / 255.0;
        input[plane + i] = pixel[1] as f32 / 255.0;
        input[2 * plane + i] = pixel[2] as f32 / 255.0;
    }

    let images_tensor = Tensor::from_array(([1usize, 3, INPUT_SIZE as usize, INPUT_SIZE as usize], input))
        .map_err(|e| format!("Could not build input tensor: {e}"))?;
    // [width, height], matching the reference implementation exactly --
    // this is NOT [height, width].
    let sizes_tensor = Tensor::from_array(([1usize, 2], vec![width as i64, height as i64]))
        .map_err(|e| format!("Could not build size tensor: {e}"))?;

    let session_mutex = session()?;
    let mut session = session_mutex.lock().map_err(|e| format!("Detection session lock poisoned: {e}"))?;
    let outputs = session
        .run(ort::inputs! {
            "images" => images_tensor,
            "orig_target_sizes" => sizes_tensor,
        })
        .map_err(|e| format!("Detection inference failed: {e}"))?;

    let (_, labels) =
        outputs["labels"].try_extract_tensor::<i64>().map_err(|e| format!("Could not read labels output: {e}"))?;
    let (_, boxes) =
        outputs["boxes"].try_extract_tensor::<f32>().map_err(|e| format!("Could not read boxes output: {e}"))?;
    let (_, scores) =
        outputs["scores"].try_extract_tensor::<f32>().map_err(|e| format!("Could not read scores output: {e}"))?;

    let mut found = Vec::new();
    for i in 0..scores.len() {
        if scores[i] < score_threshold {
            continue;
        }
        let Some(kind) = RegionKind::from_label(labels[i]) else { continue };
        let x0 = boxes[i * 4].max(0.0);
        let y0 = boxes[i * 4 + 1].max(0.0);
        let x1 = boxes[i * 4 + 2].min(width as f32);
        let y1 = boxes[i * 4 + 3].min(height as f32);
        if x1 - x0 < MIN_SIDE || y1 - y0 < MIN_SIDE {
            continue;
        }
        found.push(DetectedRegion {
            kind,
            score: scores[i],
            x: x0 as i64,
            y: y0 as i64,
            width: (x1 - x0) as i64,
            height: (y1 - y0) as i64,
        });
    }

    let text: Vec<DetectedRegion> =
        found.into_iter().filter(|r| r.kind != RegionKind::Bubble).collect();
    Ok(deduplicate(text))
}

/// Highest score first; a box mostly covered by one already kept is the
/// same text reported under a different class.
fn deduplicate(mut regions: Vec<DetectedRegion>) -> Vec<DetectedRegion> {
    regions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<DetectedRegion> = Vec::new();
    for region in regions {
        let overlaps = kept.iter().any(|other| overlap_ratio(*other, region) > DUPLICATE_OVERLAP);
        if !overlaps {
            kept.push(region);
        }
    }
    kept
}

fn overlap_ratio(a: DetectedRegion, b: DetectedRegion) -> f32 {
    let ix0 = a.x.max(b.x);
    let iy0 = a.y.max(b.y);
    let ix1 = (a.x + a.width).min(b.x + b.width);
    let iy1 = (a.y + a.height).min(b.y + b.height);
    if ix1 <= ix0 || iy1 <= iy0 {
        return 0.0;
    }
    let intersection = ((ix1 - ix0) * (iy1 - iy0)) as f32;
    let area_a = (a.width * a.height) as f32;
    let area_b = (b.width * b.height) as f32;
    intersection / area_a.min(area_b).max(1.0)
}

#[cfg(test)]
mod manual_tests {
    use super::*;
    use image::Rgba;
    use imageproc::drawing::draw_hollow_rect_mut;
    use imageproc::rect::Rect;

    /// Not run in normal `cargo test` -- set MWT_DETECT_TEST_IMAGE to an
    /// absolute path to a real manga page to visually sanity-check the
    /// detector before spending any API quota on it.
    #[test]
    fn visualize_detections_on_real_page() {
        let Ok(path) = std::env::var("MWT_DETECT_TEST_IMAGE") else {
            eprintln!("skipping: set MWT_DETECT_TEST_IMAGE to run this");
            return;
        };
        let img = image::open(&path).expect("open test image").to_rgba8();
        let regions = detect_text_regions(&img, DEFAULT_SCORE_THRESHOLD).expect("detection failed");
        eprintln!("detected {} text regions:", regions.len());
        let mut out = img.clone();
        for r in &regions {
            eprintln!(
                "  {:?} score={:.2} x={} y={} w={} h={}",
                r.kind, r.score, r.x, r.y, r.width, r.height
            );
            let color = match r.kind {
                RegionKind::TextInBubble => Rgba([255, 0, 0, 255]),
                RegionKind::TextOutsideBubble => Rgba([0, 120, 255, 255]),
                RegionKind::Bubble => Rgba([0, 200, 0, 255]),
            };
            draw_hollow_rect_mut(
                &mut out,
                Rect::at(r.x as i32, r.y as i32).of_size(r.width.max(1) as u32, r.height.max(1) as u32),
                color,
            );
        }
        let out_path = std::env::var("MWT_DETECT_TEST_OUT").unwrap_or_else(|_| "/tmp/mwt_detect_test.png".into());
        out.save(&out_path).expect("save visualization");
        eprintln!("wrote visualization to {out_path}");
    }
}
