//! Deterministic, local text-region redraw: given bounding boxes + already
//! -translated text (from an OCR/translation call, never from an image-edit
//! model), erase just those regions and typeset the translation there. This
//! guarantees nothing outside the given boxes is ever touched -- unlike
//! asking a generative image model to "edit only the text", which is not a
//! deterministic operation and (observed in testing) sometimes silently
//! no-ops or refuses instead.

use ab_glyph::{Font, FontArc, PxScale};
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_ellipse_mut, draw_filled_rect_mut, draw_text_mut, text_size};
use imageproc::rect::Rect;
use std::sync::OnceLock;

use super::inpaint::LamaInpainter;

// Bold, not Regular: comic/manga lettering is conventionally bold and
// uppercase -- this alone goes a long way toward not looking like plain
// body text pasted over art. This bundled font is a deliberately generic,
// openly-licensed fallback (Bitstream Vera/DejaVu, see assets/LICENSE-*) --
// the genuinely "authentic" comic-lettering fonts (CC Wild Words, Anime
// Ace) are commercial products (Comicraft/Blambot) that can't legally be
// redistributed in an open-source repo. load_font_stack() below lets a
// user who has personally obtained one of those point this app at their
// own copy (or several, as a stack) instead, without us ever shipping it.
static FONT_BYTES: &[u8] = include_bytes!("../../assets/DejaVuSans-Bold.ttf");

// A translation occasionally still contains a non-Latin character -- a
// proper noun the translator left as-is, a stray title/logo glyph OCR'd
// off a cover page, etc. A user's own Latin-only lettering font (Wild
// Words and similar) has no glyph for those, and silently fell back to
// the font's .notdef glyph: a hollow "tofu" box, rendered right into the
// final image. Koharu's own font settings solve this with a configurable
// font stack, tried in order, used only for the specific characters
// earlier fonts in the stack lack -- see `load_font_stack` below. This
// bundled Noto Sans JP (SIL OFL, see assets/LICENSE-NotoSansJP.txt)
// covers Japanese and is always appended as the final, guaranteed entry
// of that stack, so there's always *something* with a glyph for common
// CJK characters even if the user's own stack is empty or Latin-only.
static FALLBACK_FONT_BYTES: &[u8] = include_bytes!("../../assets/NotoSansJP-Bold.ttf");

fn fallback_font() -> &'static FontArc {
    static FONT: OnceLock<FontArc> = OnceLock::new();
    FONT.get_or_init(|| FontArc::try_from_slice(FALLBACK_FONT_BYTES).expect("bundled fallback font must be valid"))
}

/// True if `font` has a real glyph for `ch` (not just its .notdef/"tofu"
/// placeholder, glyph id 0, which every font reports for a codepoint it
/// doesn't support).
fn has_glyph(font: &FontArc, ch: char) -> bool {
    font.glyph_id(ch).0 != 0
}

/// First font in `stack` that has a real glyph for `ch`, or the stack's
/// last entry (always the bundled fallback -- see `load_font_stack`) if
/// none do. `stack` is never empty in practice, but a 1-element stack
/// (also) just returns that element for every character, same as before
/// font stacks existed.
fn pick_font<'a>(stack: &'a [FontArc], ch: char) -> &'a FontArc {
    stack.iter().find(|font| has_glyph(font, ch)).unwrap_or_else(|| stack.last().expect("font stack is never empty"))
}

/// `text_size`, but each character is measured against the first font in
/// `stack` that actually has a glyph for it. Per-character rather than
/// splitting into same-font runs: simpler, and comic lettering here is
/// already laid out approximately (see draw_fitted_text), so the small
/// kerning loss from measuring one character at a time doesn't matter.
fn measure_mixed(stack: &[FontArc], scale: PxScale, text: &str) -> u32 {
    text.chars().map(|ch| text_size(scale, pick_font(stack, ch), &ch.to_string()).0).sum()
}

/// `draw_text_mut`, but each character is drawn with the first font in
/// `stack` that has a glyph for it, advancing the cursor by that
/// character's own measured width.
fn draw_mixed(image: &mut RgbaImage, color: Rgba<u8>, x: i32, y: i32, scale: PxScale, stack: &[FontArc], text: &str) {
    let mut cursor_x = x;
    for ch in text.chars() {
        let font = pick_font(stack, ch);
        let s = ch.to_string();
        draw_text_mut(image, color, cursor_x, y, scale, font, &s);
        cursor_x += text_size(scale, font, &s).0 as i32;
    }
}

/// Below this, no translated word can render legibly regardless of font
/// size -- typically a vertical side-strip of source text (publisher
/// blurbs, etc.), not real dialogue. Leave those untouched rather than
/// stacking one word per line down an unreadable column.
const MIN_DRAWABLE_WIDTH: u32 = 28;
const MIN_DRAWABLE_HEIGHT: u32 = 16;

/// How much larger than the reported text box to make the *erased* area,
/// as a fraction of each dimension. The detector's box is sometimes tightly
/// (or slightly incorrectly) fit to only part of the original text -- this
/// margin catches stray original-language pixels just outside it. Kept
/// small now that boxes come from a trained detector rather than an LLM's
/// guess: a bigger margin was needed before, but risks eating into a
/// neighboring, separate piece of text on a dense page.
const ERASE_PADDING_RATIO: f32 = 0.08;

pub struct TextRegion {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub translated_text: String,
    /// True for text found inside a speech bubble (RegionKind::TextInBubble)
    /// -- those are drawn on an ellipse inscribed in the box instead of a
    /// hard rectangle, since real speech bubbles are round/oval and a
    /// rectangular fill's corners poke out past the bubble's actual edge
    /// into the surrounding art as a visible blank patch. Free-floating
    /// text (narration boxes, SFX) keeps the rectangle -- narration boxes
    /// are themselves usually hard-cornered already.
    pub is_round: bool,
}

/// Loads an ordered stack of user-supplied fonts (e.g. a personally-
/// licensed copy of Wild Words for Latin text, plus Adobe Std for kanji --
/// same multi-font idea as Koharu's own font stack setting) -- each
/// character is drawn with the first font in the stack that has a glyph
/// for it (see `pick_font`). A path that can't be read/parsed is skipped
/// (logged, not fatal) rather than discarding the rest of the stack.
///
/// `paths` empty means "use the bundled font" (DejaVu, open-licensed --
/// the genuinely "authentic" comic-lettering fonts are commercial
/// products this app can't legally bundle/redistribute). The bundled
/// Noto Sans JP fallback (see FALLBACK_FONT_BYTES) is always appended as
/// the stack's last entry, so there's always something with a glyph for
/// common CJK characters regardless of what the user configured.
pub fn load_font_stack(paths: &[String]) -> Vec<FontArc> {
    let mut stack: Vec<FontArc> = paths
        .iter()
        .filter(|p| !p.trim().is_empty())
        .filter_map(|path| match std::fs::read(path) {
            Ok(bytes) => match FontArc::try_from_vec(bytes) {
                Ok(font) => Some(font),
                Err(e) => {
                    eprintln!("Configured font at '{path}' failed to parse ({e}); skipping it.");
                    None
                }
            },
            Err(e) => {
                eprintln!("Configured font at '{path}' could not be read ({e}); skipping it.");
                None
            }
        })
        .collect();

    if stack.is_empty() {
        stack.push(FontArc::try_from_slice(FONT_BYTES).expect("bundled font must be valid"));
    }
    stack.push(fallback_font().clone());
    stack
}

/// Two reported regions overlapping more than this (as a fraction of the
/// smaller one's area) are treated as the same text reported twice --
/// drawing both just garbles two translations on top of each other.
const DUPLICATE_OVERLAP_RATIO: f32 = 0.3;

pub fn redraw_regions(image: &mut RgbaImage, regions: &[TextRegion], fonts: &[FontArc], inpainter: Option<&LamaInpainter>) {
    let (img_w, img_h) = image.dimensions();
    // Tight (unpadded) rects of every region we've decided to draw, kept so
    // later regions' erase padding can be clamped away from them -- without
    // this, two genuinely separate, nearby pieces of text can have their
    // padded erase areas collide into each other's space on a dense page.
    let mut accepted: Vec<(u32, u32, u32, u32)> = Vec::new();

    for region in regions {
        if region.translated_text.trim().is_empty() {
            continue;
        }
        let Some((x, y, w, h)) = clamp_rect(region.x, region.y, region.width, region.height, img_w, img_h) else {
            continue;
        };
        if w < MIN_DRAWABLE_WIDTH || h < MIN_DRAWABLE_HEIGHT {
            continue;
        }
        if accepted.iter().any(|&r| overlap_ratio((x, y, w, h), r) > DUPLICATE_OVERLAP_RATIO) {
            continue;
        }

        let (ex, ey, ew, eh) = pad_rect_clamped((x, y, w, h), &accepted, img_w, img_h, ERASE_PADDING_RATIO);
        accepted.push((x, y, w, h));

        // Real background reconstruction when a (downloaded, optional)
        // inpainting model is available -- a plain color/ellipse fill
        // otherwise. Reconstruction already produces the right shape
        // (bubble outline and all) on its own, since it's inferred from
        // genuine surrounding context rather than approximated as an
        // ellipse, so there's nothing round-vs-rect-specific left to do
        // once it succeeds.
        let inpainted = inpainter.and_then(|lama| lama.reconstruct(image, ex, ey, ew, eh).ok());
        let used_inpainting = if let Some(patch) = &inpainted {
            image::imageops::replace(image, patch, ex as i64, ey as i64);
            true
        } else {
            false
        };

        if !used_inpainting {
            let fill = sample_fill_color(image, ex, ey, ew, eh);
            if region.is_round {
                let center = (ex as i32 + ew as i32 / 2, ey as i32 + eh as i32 / 2);
                draw_filled_ellipse_mut(image, center, (ew / 2) as i32, (eh / 2) as i32, fill);
            } else {
                draw_filled_rect_mut(image, Rect::at(ex as i32, ey as i32).of_size(ew, eh), fill);
            }
        }

        // Text contrast is decided from whatever's actually under it now
        // (inpainted texture or flat fill, doesn't matter which).
        let text_color = contrasting_color(sample_fill_color(image, ex, ey, ew, eh));
        let text = region.translated_text.to_uppercase();

        if region.is_round {
            // Safe text area: a rectangle inscribed in the ellipse (corners
            // touching it) so text never sits on the un-erased art outside
            // the oval -- that's width/sqrt(2) x height/sqrt(2), centered.
            // Kept even for a successful inpaint: the reconstructed patch
            // is a rectangle, but the bubble it reconstructed is still
            // round, so text still needs to stay inside that oval.
            let safe_w = ((ew as f32) * std::f32::consts::FRAC_1_SQRT_2).round() as u32;
            let safe_h = ((eh as f32) * std::f32::consts::FRAC_1_SQRT_2).round() as u32;
            let safe_x = ex + (ew.saturating_sub(safe_w)) / 2;
            let safe_y = ey + (eh.saturating_sub(safe_h)) / 2;
            draw_fitted_text_clipped(image, fonts, &text, safe_x, safe_y, safe_w.max(1), safe_h.max(1), text_color);
        } else {
            draw_fitted_text_clipped(image, fonts, &text, ex, ey, ew, eh, text_color);
        }
    }
}

/// Intersection area as a fraction of the smaller rect's area (0 = no
/// overlap, 1 = one fully contains the other).
fn overlap_ratio(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> f32 {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let ix0 = ax.max(bx);
    let iy0 = ay.max(by);
    let ix1 = (ax + aw).min(bx + bw);
    let iy1 = (ay + ah).min(by + bh);
    if ix1 <= ix0 || iy1 <= iy0 {
        return 0.0;
    }
    let intersection = (ix1 - ix0) as f32 * (iy1 - iy0) as f32;
    let smaller_area = (aw as f32 * ah as f32).min(bw as f32 * bh as f32);
    if smaller_area <= 0.0 {
        0.0
    } else {
        intersection / smaller_area
    }
}

/// Pads `rect` outward by `ratio`, clamped to the image bounds and, on any
/// side where that would cross into another already-accepted region's own
/// (unpadded) box, clamped to stop just short of it instead -- padding
/// exists to catch stray pixels around *this* region's own text, never to
/// eat into a neighboring, separate piece of text.
fn pad_rect_clamped(
    rect: (u32, u32, u32, u32),
    others: &[(u32, u32, u32, u32)],
    img_w: u32,
    img_h: u32,
    ratio: f32,
) -> (u32, u32, u32, u32) {
    let (x, y, w, h) = rect;
    let (xi, yi, wi, hi) = (x as i64, y as i64, w as i64, h as i64);
    let pad_x = (w as f32 * ratio).round() as i64;
    let pad_y = (h as f32 * ratio).round() as i64;
    let mut x0 = (xi - pad_x).max(0);
    let mut y0 = (yi - pad_y).max(0);
    let mut x1 = (xi + wi + pad_x).min(img_w as i64);
    let mut y1 = (yi + hi + pad_y).min(img_h as i64);

    for &(ox, oy, ow, oh) in others {
        let (ox0, oy0, ox1, oy1) = (ox as i64, oy as i64, (ox + ow) as i64, (oy + oh) as i64);

        // Horizontal (left/right) padding only matters against a neighbor
        // that overlaps this region's own row range.
        if oy0 < yi + hi && oy1 > yi {
            if ox1 <= xi {
                x0 = x0.max(ox1);
            }
            if ox0 >= xi + wi {
                x1 = x1.min(ox0);
            }
        }
        // Vertical (top/bottom) padding only matters against a neighbor
        // that overlaps this region's own column range.
        if ox0 < xi + wi && ox1 > xi {
            if oy1 <= yi {
                y0 = y0.max(oy1);
            }
            if oy0 >= yi + hi {
                y1 = y1.min(oy0);
            }
        }
    }

    let x0 = x0.clamp(0, xi) as u32;
    let y0 = y0.clamp(0, yi) as u32;
    let x1 = x1.max(xi + wi) as u32;
    let y1 = y1.max(yi + hi) as u32;
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

fn clamp_rect(x: i64, y: i64, w: i64, h: i64, img_w: u32, img_h: u32) -> Option<(u32, u32, u32, u32)> {
    if w <= 0 || h <= 0 || img_w == 0 || img_h == 0 {
        return None;
    }
    let x = x.clamp(0, img_w as i64) as u32;
    let y = y.clamp(0, img_h as i64) as u32;
    let w = (w as u32).min(img_w.saturating_sub(x));
    let h = (h as u32).min(img_h.saturating_sub(y));
    if w == 0 || h == 0 {
        None
    } else {
        Some((x, y, w, h))
    }
}

/// Approximate the region's background from a ring of samples just inside
/// its edges. Median, not mean: a speech bubble's own outline stroke is
/// often several px of near-black right at the boundary, and averaging
/// those in with genuine (usually white or solid-color) background pixels
/// is exactly what was producing muddy gray fills. The median just ignores
/// that minority of outline pixels as long as most of the ring is real
/// background, which it is for any normal bubble.
fn sample_fill_color(image: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Rgba<u8> {
    let inset_x = (w / 8).min(w.saturating_sub(1) / 2);
    let inset_y = (h / 8).min(h.saturating_sub(1) / 2);
    let left = x + inset_x;
    let right = x + w.saturating_sub(1) - inset_x;
    let top = y + inset_y;
    let bottom = y + h.saturating_sub(1) - inset_y;

    let mut samples: Vec<[u8; 3]> = Vec::new();
    let get = |px: u32, py: u32| -> Option<[u8; 3]> {
        if px < image.width() && py < image.height() {
            let p = image.get_pixel(px, py);
            Some([p[0], p[1], p[2]])
        } else {
            None
        }
    };
    if right >= left && bottom >= top {
        let mut px = left;
        while px <= right {
            samples.extend(get(px, top));
            samples.extend(get(px, bottom));
            px += 2;
        }
        let mut py = top;
        while py <= bottom {
            samples.extend(get(left, py));
            samples.extend(get(right, py));
            py += 2;
        }
    }

    if samples.is_empty() {
        return Rgba([255, 255, 255, 255]);
    }
    let mut rs: Vec<u8> = samples.iter().map(|p| p[0]).collect();
    let mut gs: Vec<u8> = samples.iter().map(|p| p[1]).collect();
    let mut bs: Vec<u8> = samples.iter().map(|p| p[2]).collect();
    Rgba([median(&mut rs), median(&mut gs), median(&mut bs), 255])
}

fn median(values: &mut [u8]) -> u8 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn contrasting_color(fill: Rgba<u8>) -> Rgba<u8> {
    let luminance = 0.299 * fill[0] as f32 + 0.587 * fill[1] as f32 + 0.114 * fill[2] as f32;
    if luminance > 140.0 {
        Rgba([20, 20, 20, 255])
    } else {
        Rgba([245, 245, 245, 255])
    }
}

fn wrap_text(fonts: &[FontArc], text: &str, scale: PxScale, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        let w = measure_mixed(fonts, scale, &candidate);
        if w as f32 > max_width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = candidate;
        }
        // A single word wider than the whole box (long compound word, or the
        // shrink loop already down near its floor) would otherwise sit on
        // its own oversized line forever -- break it by character so it at
        // least respects max_width. draw_fitted_text_clipped's buffer stops
        // anything from bleeding outside the box regardless, but this keeps
        // that word legible instead of just chopped off mid-glyph.
        if current == word {
            let word_w = measure_mixed(fonts, scale, &current);
            if word_w as f32 > max_width {
                let mut piece = String::new();
                for ch in word.chars() {
                    let candidate_piece = format!("{piece}{ch}");
                    let pw = measure_mixed(fonts, scale, &candidate_piece);
                    if pw as f32 > max_width && !piece.is_empty() {
                        lines.push(piece);
                        piece = ch.to_string();
                    } else {
                        piece = candidate_piece;
                    }
                }
                current = piece;
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() && !text.is_empty() {
        lines.push(text.to_string());
    }
    lines
}

/// Draws into a `w`x`h` scratch buffer pre-filled with the region's own
/// background color, then pastes that buffer back at `(x, y)` with a plain
/// (non-blending) copy. Text can never land outside the given box this way,
/// however badly draw_fitted_text's shrink-to-fit math misjudges an
/// unusually-shaped region -- the failure mode becomes "truncated/clipped
/// text", not "text sprayed across the rest of the page" (the actual bug
/// this was added for: a long translation in a narrow/misdetected region
/// drew dozens of px past its box, over neighboring art and page margin).
fn draw_fitted_text_clipped(image: &mut RgbaImage, fonts: &[FontArc], text: &str, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    if w == 0 || h == 0 {
        return;
    }
    // Background for the layer comes from whatever's already under this
    // rect in `image` -- the flat fill or inpainted patch the caller just
    // painted there -- rather than a solid color passed in separately, so
    // this works unchanged for either erase path.
    let mut layer = image::imageops::crop_imm(image, x, y, w, h).to_image();
    draw_fitted_text(&mut layer, fonts, text, 0, 0, w, h, color);
    image::imageops::replace(image, &layer, x as i64, y as i64);
}

/// Shrinks the font size until the wrapped translation fits the box height,
/// then draws it centered both ways. Each character is drawn with the
/// first font in `fonts` that has a glyph for it (see `pick_font`) -- the
/// stack's own last entry is always the bundled fallback (see
/// `load_font_stack`), so this never has nothing to fall back to.
#[allow(clippy::too_many_arguments)]
fn draw_fitted_text(image: &mut RgbaImage, fonts: &[FontArc], text: &str, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let padding = (w as f32 * 0.06).clamp(1.0, 8.0);
    let max_width = (w as f32 - padding * 2.0).max(1.0);
    let max_height = (h as f32 - padding * 2.0).max(1.0);

    let mut scale_px = (h as f32 * 0.22).clamp(9.0, 48.0);
    let mut lines = wrap_text(fonts, text, PxScale::from(scale_px), max_width);
    for _ in 0..12 {
        let line_height = scale_px * 1.25;
        let total_height = line_height * lines.len().max(1) as f32;
        if total_height <= max_height || scale_px <= 8.0 {
            break;
        }
        scale_px *= 0.88;
        lines = wrap_text(fonts, text, PxScale::from(scale_px), max_width);
    }

    let scale = PxScale::from(scale_px);
    let line_height = scale_px * 1.25;
    let total_height = line_height * lines.len().max(1) as f32;
    let mut cursor_y = y as f32 + padding + (max_height - total_height).max(0.0) / 2.0;

    for line in &lines {
        let line_w = measure_mixed(fonts, scale, line);
        let line_x = x as f32 + padding + (max_width - line_w as f32).max(0.0) / 2.0;
        draw_mixed(image, color, line_x.round() as i32, cursor_y.round() as i32, scale, fonts, line);
        cursor_y += line_height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn smoke_test_redraw() {
        let mut img = RgbaImage::from_pixel(600, 300, Rgba([255, 255, 255, 255]));
        // Draw a dark border rectangle to simulate a speech bubble outline.
        for x in 40..360u32 {
            img.put_pixel(x, 40, Rgba([0, 0, 0, 255]));
            img.put_pixel(x, 159, Rgba([0, 0, 0, 255]));
        }
        for y in 40..160u32 {
            img.put_pixel(40, y, Rgba([0, 0, 0, 255]));
            img.put_pixel(359, y, Rgba([0, 0, 0, 255]));
        }

        let regions = vec![
            TextRegion {
                x: 45,
                y: 45,
                width: 310,
                height: 110,
                translated_text: "Hello world, this is a test of wrapped text rendering across a few lines.".into(),
                is_round: true,
            },
            TextRegion { x: 500, y: 20, width: 80, height: 40, translated_text: "SFX!".into(), is_round: false },
            // Out-of-bounds region should be silently skipped, not panic.
            TextRegion { x: 10_000, y: 10_000, width: 50, height: 50, translated_text: "offscreen".into(), is_round: false },
        ];

        let fonts = load_font_stack(&[]);
        redraw_regions(&mut img, &regions, &fonts, None);

        let out_path = std::env::var("MWT_TEST_OUT").unwrap_or_else(|_| "/tmp/mwt_compose_test.png".into());
        img.save(&out_path).expect("save test output");
        assert_eq!(img.dimensions(), (600, 300));
    }

    /// Regression test for a real bug: a long translation in a narrow
    /// region drew text dozens of pixels past the region's own box, over
    /// unrelated page content. Everything outside the region (plus its
    /// small erase-padding margin) must be untouched after redraw.
    #[test]
    fn text_never_escapes_its_region() {
        let mut img = RgbaImage::from_pixel(400, 400, Rgba([255, 255, 255, 255]));
        let region =
            TextRegion { x: 150, y: 150, width: 40, height: 60, translated_text: "THERE IS NO ONE ON THE DAY OF IKEA SOMETHING".into(), is_round: false };
        let fonts = load_font_stack(&[]);
        redraw_regions(&mut img, std::slice::from_ref(&region), &fonts, None);

        // Generous margin beyond the padded erase box -- anything further
        // out than this must still be pure white.
        let margin = 20i64;
        let (x0, y0) = ((region.x - margin).max(0), (region.y - margin).max(0));
        let (x1, y1) = ((region.x + region.width + margin).min(400), (region.y + region.height + margin).min(400));
        for y in 0..400u32 {
            for x in 0..400u32 {
                let inside_checked_area = (x as i64) >= x0 && (x as i64) < x1 && (y as i64) >= y0 && (y as i64) < y1;
                if inside_checked_area {
                    continue;
                }
                assert_eq!(
                    img.get_pixel(x, y),
                    &Rgba([255, 255, 255, 255]),
                    "pixel ({x},{y}) outside the region (+margin) was touched -- text escaped its box"
                );
            }
        }
    }

    /// Regression test for a real bug: a translated_text that still
    /// contained a Japanese character (a proper noun the translator left
    /// untranslated, in this real-world case) rendered as a hollow "tofu"
    /// box instead of the character itself, because the bundled Latin-only
    /// font has no glyph for it.
    #[test]
    fn primary_font_has_no_japanese_glyphs_but_fallback_does() {
        let stack = load_font_stack(&[]);
        // A kanji from the exact title text that triggered this bug.
        let ch = '勇';
        assert!(!has_glyph(&stack[0], ch), "test assumption broken: bundled DejaVu now has a glyph for {ch}");
        assert!(
            has_glyph(stack.last().unwrap(), ch),
            "fallback font has no glyph for {ch} either -- fallback itself is broken"
        );
        // pick_font should land on the fallback entry for this character.
        assert!(std::ptr::eq(pick_font(&stack, ch), stack.last().unwrap()));
    }

    /// `pick_font`/`load_font_stack` only had a 2-element (primary +
    /// fallback) stack to exercise before font stacks became
    /// user-configurable (multiple entries, see settings.rs's
    /// `font_paths`). Builds a genuine 3-entry stack (two Latin-only
    /// fonts, neither with a Japanese glyph, then the bundled fallback)
    /// to confirm `pick_font` walks past *more than one* glyph-less entry
    /// correctly, not just a single primary/fallback pair.
    #[test]
    fn pick_font_walks_past_multiple_glyph_less_entries_in_a_stack() {
        let latin_only = FontArc::try_from_slice(FONT_BYTES).expect("bundled font must be valid");
        let stack = vec![latin_only.clone(), latin_only, fallback_font().clone()];
        let ch = '勇';
        assert!(!has_glyph(&stack[0], ch));
        assert!(!has_glyph(&stack[1], ch));
        assert!(has_glyph(&stack[2], ch));
        assert!(std::ptr::eq(pick_font(&stack, ch), &stack[2]), "expected pick_font to land on the 3rd stack entry");

        // An ASCII character every entry has a glyph for should still pick
        // the *first* matching entry, not skip ahead to the fallback.
        assert!(std::ptr::eq(pick_font(&stack, 'A'), &stack[0]));
    }

    #[test]
    fn redraw_with_mixed_latin_and_japanese_text_does_not_panic() {
        let mut img = RgbaImage::from_pixel(400, 300, Rgba([255, 255, 255, 255]));
        let region = TextRegion {
            x: 40,
            y: 40,
            width: 200,
            height: 120,
            translated_text: "The hero 勇者 arrives".into(),
            is_round: false,
        };
        let fonts = load_font_stack(&[]);
        redraw_regions(&mut img, std::slice::from_ref(&region), &fonts, None);

        if let Ok(out_path) = std::env::var("MWT_TEST_OUT") {
            img.save(&out_path).expect("save test output");
        }
    }
}
