//! Local image inpainting via LaMa ("big-lama"), replacing compose.rs's
//! flat-color-fill erase with real background reconstruction -- the
//! "inpainting"/CLRD step Koharu (koharu.rs) uses, which this app didn't
//! have at all before. ONNX export: Carve/LaMa-ONNX (Apache-2.0), a port
//! of advimman/lama's "big-lama" checkpoint -- a general-purpose photo
//! inpainting model (not manga-specific), fixed 512x512 input.
//!
//! Tile-based: the model only ever sees one 512x512 window centered on
//! the region being erased, never the whole page -- the model's own
//! input is fixed-size, so every inpainting tool built on it works this
//! way. Pre/post-processing confirmed against the model's own demo space
//! (Carve/LaMa-Demo-ONNX's app.py): image /255 to [0,1] in, mask
//! binarized to {0,1} (1 = reconstruct this pixel) -- but the graph's
//! *output* is already in [0,255] (`output.astype(np.uint8)` there, no
//! further scaling), despite the symmetric-looking input normalization.
//! An earlier version of this file multiplied the output by 255 again by
//! analogy with the input side, which clamped virtually every pixel to
//! flat white (found 2026-10-24 by comparing a real reconstructed region
//! against this exact reference demo script).

use image::{Rgba, RgbaImage};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

const TILE: u32 = 512;

pub struct LamaInpainter {
    session: Mutex<Session>,
}

impl LamaInpainter {
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let environment = super::ort_env::shared()?;
        let mut builder = Session::builder(&environment).map_err(|e| e.to_string())?;
        builder = builder.with_optimization_level(GraphOptimizationLevel::Level3).map_err(|e| e.to_string())?;
        builder = builder
            .with_intra_threads(crate::resource_limits::effective_thread_count() as usize)
            .map_err(|e| e.to_string())?;
        // CPU-only, deliberately -- CoreML was tried here (see git
        // history) to bring this 512x512 conv-heavy network's 10+
        // seconds/region on CPU down to roughly the OCR/translation
        // steps' own per-region cost, the same way it helps detect.rs/
        // manga_ocr.rs. It doesn't work for *this* model: ONNX Runtime
        // auto-selects CoreML for any compatible session on macOS
        // whenever the "coreml" Cargo feature is compiled in, even
        // without this file ever asking for it (confirmed by explicitly
        // forcing an empty execution-provider list here, which is the
        // only thing that actually stops it) -- and CoreML's compiled
        // graph for this specific model is broken: it resolves the
        // "image" input's spatial dims to (1, 3, 0, 0) internally
        // (`Espresso exception: "Invalid blob shape"`, surfaced with
        // graph optimization disabled) and, with optimization on, that
        // doesn't error at all -- it just silently returns a uniform
        // 255.0 for every single output pixel. Real-world symptom: every
        // "reconstructed" region came back flat white with no bubble
        // outline, texture, or anything else, confirmed against several
        // real page regions (2026-10-24). The explicit empty list below
        // is what makes this reliably CPU-only instead of hoping no EP
        // auto-registers; do not remove it to "simplify" this without
        // re-confirming CoreML has become compatible with this model.
        builder = builder.with_execution_providers(Vec::new()).map_err(|e| e.to_string())?;
        let session = builder.commit_from_file(model_path).map_err(|e| e.to_string())?;
        Ok(Self { session: Mutex::new(session) })
    }

    /// Reconstructs what the background would look like under the
    /// `width x height` rect at `(x, y)` in `image`. Returns the
    /// reconstructed pixels, sized exactly `width x height`, ready to
    /// paste over the erased region -- the caller still draws the
    /// translated text on top.
    ///
    /// A region that fits in one TILExTILE window (the common case --
    /// most speech bubbles) goes straight through `reconstruct_tile`.
    /// A region bigger than that (wide caption boxes spanning much of a
    /// panel, common on dense text-heavy pages) is split into a grid of
    /// TILExTILE sub-tiles, each reconstructed independently against its
    /// own surrounding context and stitched back together -- until
    /// 2026-10-24 this just failed outright for anything over 512px in
    /// either dimension, silently falling back to compose.rs's flat-fill
    /// path (a plain rectangle/ellipse in whatever flat color the page's
    /// background happened to sample as), which is exactly the "white
    /// box with hard edges cutting across the art" look Alisson reported
    /// seeing instead of the Koharu reference tool's clean reconstructed
    /// background. Tile seams aren't feathered/blended here -- each
    /// tile's own surrounding real-page context still makes this far
    /// closer to Koharu's result than a flat fill, and a visible seam on
    /// an oversized region is a much smaller defect than a solid white
    /// rectangle over character art.
    pub fn reconstruct(
        &self,
        image: &RgbaImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, String> {
        if width == 0 || height == 0 {
            return Err("empty region".into());
        }
        if width <= TILE && height <= TILE {
            return self.reconstruct_tile(image, x, y, width, height);
        }

        let mut result = RgbaImage::new(width, height);
        let mut ty = 0u32;
        while ty < height {
            let th = (height - ty).min(TILE);
            let mut tx = 0u32;
            while tx < width {
                let tw = (width - tx).min(TILE);
                let tile = self.reconstruct_tile(image, x + tx, y + ty, tw, th)?;
                image::imageops::replace(&mut result, &tile, tx as i64, ty as i64);
                tx += tw;
            }
            ty += th;
        }
        Ok(result)
    }

    /// Reconstructs a region that fits within one TILExTILE window --
    /// see `reconstruct`'s docs for the multi-tile case this backs.
    fn reconstruct_tile(
        &self,
        image: &RgbaImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, String> {
        debug_assert!(width <= TILE && height <= TILE);
        let (img_w, img_h) = image.dimensions();
        let plane = (TILE * TILE) as usize;

        // Center a TILE x TILE window on the region, clamped so it never
        // runs off the image (and pinned to (0,0) if the page itself is
        // smaller than one tile -- pixel sampling below edge-extends for
        // anything still outside the real image).
        let cx = x as i64 + width as i64 / 2;
        let cy = y as i64 + height as i64 / 2;
        let half = TILE as i64 / 2;
        let win_x = (cx - half).clamp(0, (img_w as i64 - TILE as i64).max(0));
        let win_y = (cy - half).clamp(0, (img_h as i64 - TILE as i64).max(0));

        let mut tile_rgb = vec![0f32; 3 * plane];
        let mut tile_mask = vec![0f32; plane];
        for ty in 0..TILE {
            for tx in 0..TILE {
                let src_x = (win_x + tx as i64).clamp(0, img_w as i64 - 1) as u32;
                let src_y = (win_y + ty as i64).clamp(0, img_h as i64 - 1) as u32;
                let px = image.get_pixel(src_x, src_y);
                let idx = (ty * TILE + tx) as usize;
                tile_rgb[idx] = px[0] as f32 / 255.0;
                tile_rgb[plane + idx] = px[1] as f32 / 255.0;
                tile_rgb[2 * plane + idx] = px[2] as f32 / 255.0;

                let abs_x = win_x + tx as i64;
                let abs_y = win_y + ty as i64;
                let in_mask =
                    abs_x >= x as i64 && abs_x < (x + width) as i64 && abs_y >= y as i64 && abs_y < (y + height) as i64;
                tile_mask[idx] = if in_mask { 1.0 } else { 0.0 };
            }
        }

        let output: Vec<f32> = {
            let mut session = self.session.lock().map_err(|e| e.to_string())?;
            let image_tensor =
                Tensor::from_array(([1i64, 3, TILE as i64, TILE as i64], tile_rgb)).map_err(|e| e.to_string())?;
            let mask_tensor =
                Tensor::from_array(([1i64, 1, TILE as i64, TILE as i64], tile_mask)).map_err(|e| e.to_string())?;
            let outputs = session
                .run(ort::inputs! { "image" => image_tensor, "mask" => mask_tensor })
                .map_err(|e| e.to_string())?;
            let (_, data) = outputs["output"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            data.to_vec()
        };

        // Crop the region back out of the 512x512 output, mapping from
        // image space back into tile-local coordinates.
        let mut result = RgbaImage::new(width, height);
        for ry in 0..height {
            for rx in 0..width {
                let tile_x = (x as i64 + rx as i64 - win_x).clamp(0, TILE as i64 - 1) as usize;
                let tile_y = (y as i64 + ry as i64 - win_y).clamp(0, TILE as i64 - 1) as usize;
                let idx = tile_y * TILE as usize + tile_x;
                // The ONNX graph's own output is already in [0,255] --
                // confirmed against the model's reference demo
                // (Carve/LaMa-Demo-ONNX's app.py: `output.astype(np.uint8)`,
                // no further *255 anywhere), unlike the [0,1]-normalized
                // input this mirrors in spirit. Multiplying by 255 again
                // here (the bug until 2026-10-24) pushed virtually every
                // pixel past 255 and clamped flat white -- exactly the
                // "white box with no texture, not even the bubble outline"
                // Alisson reported seeing instead of real reconstruction.
                let r = output[idx].clamp(0.0, 255.0) as u8;
                let g = output[plane + idx].clamp(0.0, 255.0) as u8;
                let b = output[2 * plane + idx].clamp(0.0, 255.0) as u8;
                result.put_pixel(rx, ry, Rgba([r, g, b, 255]));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod manual_tests {
    use super::*;

    /// Not run in normal `cargo test`. Set MWT_LAMA_MODEL, MWT_TEST_IMAGE
    /// and MWT_TEST_OUT to sanity-check reconstruction quality on a real
    /// page before wiring this into compose.rs.
    #[test]
    fn reconstruct_real_region() {
        let (Ok(model_path), Ok(image_path), Ok(out_path)) = (
            std::env::var("MWT_LAMA_MODEL"),
            std::env::var("MWT_TEST_IMAGE"),
            std::env::var("MWT_TEST_OUT"),
        ) else {
            eprintln!("skipping: set MWT_LAMA_MODEL, MWT_TEST_IMAGE, MWT_TEST_OUT to run this");
            return;
        };
        let inpainter = LamaInpainter::load(std::path::Path::new(&model_path)).expect("load lama");
        let image = image::open(&image_path).expect("open test image").to_rgba8();
        let (w, h) = image.dimensions();

        // A region roughly in the middle of the page by default, sized
        // like a typical speech bubble; override via env vars to target
        // a specific textured area worth eyeballing.
        let rw = std::env::var("MWT_TEST_RW").ok().and_then(|v| v.parse().ok()).unwrap_or(220u32).min(w);
        let rh = std::env::var("MWT_TEST_RH").ok().and_then(|v| v.parse().ok()).unwrap_or(140u32).min(h);
        let rx = std::env::var("MWT_TEST_RX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or((w.saturating_sub(rw)) / 2);
        let ry = std::env::var("MWT_TEST_RY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or((h.saturating_sub(rh)) / 2);

        let patch = inpainter.reconstruct(&image, rx, ry, rw, rh).expect("reconstruct");
        let mut out = image.clone();
        image::imageops::replace(&mut out, &patch, rx as i64, ry as i64);
        out.save(&out_path).expect("save output");
        eprintln!("wrote {out_path}");
    }
}
