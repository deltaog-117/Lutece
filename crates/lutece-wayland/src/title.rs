//! Writing a window's title in its title bar.
//!
//! The text is rasterized on the CPU with a font shipped inside the crate, so a title looks
//! the same on every machine and a test can check its pixels. The picture is kept per title
//! and bar size, and handed to the renderer as a texture; it is made again only when the
//! title, the bar or the scale changes.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use lutece::Rgba;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;
use std::collections::HashMap;

/// JetBrains Mono Regular, under the SIL Open Font License (see `JetBrainsMono-OFL.txt`).
/// Chosen over a system font so titles do not depend on the machine; a character the font
/// lacks is drawn as its "missing glyph" box.
const FONT: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");

/// The text is this fraction of the bar's height tall, which leaves room above and below.
const TEXT_PER_BAR: f32 = 0.6;

/// Space between the bar's edge and the text, as a fraction of the bar's height.
const PADDING_PER_BAR: f32 = 0.5;

/// Below this many pixels of bar height the text would not be readable, so none is drawn.
const MIN_BAR_PX: u32 = 8;

/// The most pictures kept. Titles can change all the time (a clock in a terminal's title),
/// so the cache is emptied when it fills instead of growing with every new one.
const MAX_CACHED: usize = 256;

/// The colour of the text for a bar of colour `bar`: near black on a light bar, near white
/// on a dark one, so a title is readable without a setting of its own.
pub(crate) fn text_color(bar: Rgba) -> Rgba {
    let luminance = 0.299 * f32::from(bar.r) + 0.587 * f32::from(bar.g) + 0.114 * f32::from(bar.b);
    if luminance > 140.0 {
        Rgba::new(20, 20, 20, 255)
    } else {
        Rgba::new(235, 235, 235, 255)
    }
}

/// A picture of a title: `width` by `height` pixels of premultiplied R, G, B, A bytes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Raster {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

/// Draws `title` into a picture of `width` by `height` pixels, in `color`, as the bar of that
/// size would hold it: vertically centred, with padding at the left, and cut with an ellipsis
/// where it would not fit. `None` when there is nothing to draw: a blank title, a bar too
/// small to read or one too narrow for a single character.
pub(crate) fn rasterize(
    font: &FontRef<'_>,
    title: &str,
    width: u32,
    height: u32,
    color: Rgba,
) -> Option<Raster> {
    if height < MIN_BAR_PX || width == 0 || title.trim().is_empty() {
        return None;
    }
    let scale = PxScale::from(height as f32 * TEXT_PER_BAR);
    let scaled = font.as_scaled(scale);
    let padding = height as f32 * PADDING_PER_BAR;
    let room = width as f32 - 2.0 * padding;

    let glyphs = fit(&scaled, title, room)?;
    let ascent = scaled.ascent();
    let text_height = ascent - scaled.descent();
    let baseline = (height as f32 - text_height) / 2.0 + ascent;

    // Coverage first, colour after: one pass per glyph adds into a single-channel picture.
    let mut coverage = vec![0u8; width as usize * height as usize];
    for (id, x) in glyphs {
        let glyph = id.with_scale_and_position(scale, point(padding + x, baseline));
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, c| {
            let px = bounds.min.x as i64 + i64::from(gx);
            let py = bounds.min.y as i64 + i64::from(gy);
            let inside =
                (0..i64::from(width)).contains(&px) && (0..i64::from(height)).contains(&py);
            if inside {
                let at = py as usize * width as usize + px as usize;
                let added = u16::from(coverage[at]) + (c.clamp(0.0, 1.0) * 255.0).round() as u16;
                coverage[at] = added.min(255) as u8;
            }
        });
    }

    let mut pixels = Vec::with_capacity(coverage.len() * 4);
    for c in coverage {
        // Alpha is `coverage * color.a`; the colour is premultiplied by it.
        let alpha = u32::from(c) * u32::from(color.a) / 255;
        let premultiplied = |channel: u8| (u32::from(channel) * alpha / 255) as u8;
        pixels.extend([
            premultiplied(color.r),
            premultiplied(color.g),
            premultiplied(color.b),
            alpha as u8,
        ]);
    }
    Some(Raster {
        width,
        height,
        pixels,
    })
}

/// The glyphs of `title` and where each starts (from the text's own origin), cut with an
/// ellipsis if the whole does not fit in `room` pixels. Control characters become spaces so a
/// title cannot carry a newline into the bar. `None` when not even one character fits.
fn fit<F: Font>(
    scaled: &ab_glyph::PxScaleFont<&F>,
    title: &str,
    room: f32,
) -> Option<Vec<(ab_glyph::GlyphId, f32)>> {
    if room <= 0.0 {
        return None;
    }
    let ids: Vec<_> = title
        .chars()
        .map(|c| scaled.glyph_id(if c.is_control() { ' ' } else { c }))
        .collect();

    let mut placed = Vec::with_capacity(ids.len());
    let mut x = 0.0_f32;
    let mut previous = None;
    for id in &ids {
        if let Some(before) = previous {
            x += scaled.kern(before, *id);
        }
        placed.push((*id, x));
        x += scaled.h_advance(*id);
        previous = Some(*id);
    }
    if x <= room {
        return Some(placed);
    }

    // Too long: keep the longest start that leaves room for the ellipsis after it.
    let ellipsis = scaled.glyph_id('\u{2026}');
    let ellipsis_width = scaled.h_advance(ellipsis);
    let mut kept: Vec<_> = placed
        .iter()
        .zip(ids.iter())
        .take_while(|((_, start), id)| start + scaled.h_advance(**id) + ellipsis_width <= room)
        .map(|(glyph, _)| *glyph)
        .collect();
    let end = kept
        .last()
        .map_or(0.0, |(id, start)| start + scaled.h_advance(*id));
    if end + ellipsis_width > room {
        return None;
    }
    kept.push((ellipsis, end));
    Some(kept)
}

/// What a picture was made for: anything that changes it is part of the key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    title: String,
    width: u32,
    height: u32,
    color: [u8; 4],
}

/// The pictures of titles already drawn, ready for the renderer.
pub(crate) struct TitleCache {
    /// `None` only if the embedded font cannot be read, which a test rules out; no title is
    /// drawn then and the bar stays a plain band.
    font: Option<FontRef<'static>>,
    entries: HashMap<Key, Option<MemoryRenderBuffer>>,
}

impl TitleCache {
    pub(crate) fn new() -> Self {
        Self {
            font: FontRef::try_from_slice(FONT).ok(),
            entries: HashMap::new(),
        }
    }

    /// The picture of `title` for a bar `width` by `height` physical pixels, on a bar of colour
    /// `bar`. `None` when there is nothing to show.
    pub(crate) fn buffer(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        bar: Rgba,
    ) -> Option<MemoryRenderBuffer> {
        let font = self.font.as_ref()?;
        let color = text_color(bar);
        let key = Key {
            title: title.to_owned(),
            width,
            height,
            color: [color.r, color.g, color.b, color.a],
        };
        if let Some(known) = self.entries.get(&key) {
            return known.clone();
        }
        if self.entries.len() >= MAX_CACHED {
            self.entries.clear();
        }
        let made = rasterize(font, title, width, height, color).and_then(|raster| {
            let size = (
                i32::try_from(raster.width).ok()?,
                i32::try_from(raster.height).ok()?,
            );
            Some(MemoryRenderBuffer::from_slice(
                &raster.pixels,
                // Bytes in memory are R, G, B, A, which is what DRM calls ABGR.
                Fourcc::Abgr8888,
                size,
                1,
                Transform::Normal,
                None,
            ))
        });
        self.entries.insert(key, made.clone());
        made
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn font() -> FontRef<'static> {
        FontRef::try_from_slice(FONT).expect("the embedded font is a valid font file")
    }

    const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    fn ink(raster: &Raster) -> usize {
        raster.pixels.chunks(4).filter(|p| p[3] > 0).count()
    }

    #[test]
    fn the_embedded_font_loads() {
        assert!(TitleCache::new().font.is_some());
    }

    #[test]
    fn a_title_leaves_ink_inside_the_bar() {
        let raster = rasterize(&font(), "zsh", 200, 24, WHITE).expect("a bar this size holds text");

        assert_eq!((raster.width, raster.height), (200, 24));
        assert_eq!(raster.pixels.len(), 200 * 24 * 4);
        assert!(
            ink(&raster) > 20,
            "a three letter title has more ink than this"
        );
    }

    #[test]
    fn a_blank_title_or_a_tiny_bar_draws_nothing() {
        let f = font();

        assert_eq!(rasterize(&f, "", 200, 24, WHITE), None);
        assert_eq!(rasterize(&f, "   ", 200, 24, WHITE), None);
        assert_eq!(rasterize(&f, "zsh", 200, MIN_BAR_PX - 1, WHITE), None);
        assert_eq!(rasterize(&f, "zsh", 0, 24, WHITE), None);
    }

    #[test]
    fn a_bar_too_narrow_for_one_character_draws_nothing() {
        assert_eq!(rasterize(&font(), "zsh", 10, 24, WHITE), None);
    }

    #[test]
    fn a_long_title_is_cut_so_it_stays_inside_the_padding() {
        let long = "a very long title that cannot possibly fit in a bar this narrow";
        let raster = rasterize(&font(), long, 120, 24, WHITE).expect("some of it fits");
        let padding = (24.0 * PADDING_PER_BAR) as usize;

        let rightmost = raster
            .pixels
            .chunks(4)
            .enumerate()
            .filter(|(_, p)| p[3] > 0)
            .map(|(i, _)| i % 120)
            .max()
            .expect("there is ink");

        // The ellipsis ends within the right padding, give or take the glyph's own bearing.
        assert!(
            rightmost <= 120 - padding + 2,
            "ink reached column {rightmost}"
        );
    }

    #[test]
    fn a_newline_in_a_title_does_not_break_it() {
        let one = rasterize(&font(), "a\nb", 200, 24, WHITE).expect("fits");
        let two = rasterize(&font(), "a b", 200, 24, WHITE).expect("fits");

        assert_eq!(one, two);
    }

    #[test]
    fn the_text_is_light_on_a_dark_bar_and_dark_on_a_light_one() {
        assert_eq!(text_color(Rgba::new(10, 10, 10, 255)).r, 235);
        assert_eq!(text_color(Rgba::new(250, 250, 250, 255)).r, 20);
    }

    #[test]
    fn a_translucent_text_colour_is_premultiplied() {
        let half = Rgba::new(200, 100, 50, 128);
        let raster = rasterize(&font(), "mmm", 200, 24, half).expect("fits");

        // A premultiplied pixel never has a colour channel above its alpha.
        assert!(
            raster
                .pixels
                .chunks(4)
                .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
        );
        assert!(raster.pixels.chunks(4).all(|p| p[3] <= 128));
    }

    #[test]
    fn the_same_title_is_made_once() {
        let mut cache = TitleCache::new();
        let bar = Rgba::new(30, 30, 30, 255);

        cache.buffer("zsh", 200, 24, bar).expect("drawn");
        cache.buffer("zsh", 200, 24, bar).expect("drawn");
        assert_eq!(
            cache.entries.len(),
            1,
            "asking again must not make it again"
        );

        cache.buffer("vim", 200, 24, bar).expect("drawn");
        cache.buffer("zsh", 300, 24, bar).expect("drawn");
        assert_eq!(
            cache.entries.len(),
            3,
            "a new title or bar size is a new picture"
        );
    }

    #[test]
    fn the_cache_does_not_grow_without_end() {
        let mut cache = TitleCache::new();
        let bar = Rgba::new(30, 30, 30, 255);

        for n in 0..(MAX_CACHED * 3) {
            let _ = cache.buffer(&format!("title {n}"), 200, 24, bar);
        }

        assert!(cache.entries.len() <= MAX_CACHED);
    }

    proptest! {
        #[test]
        fn any_title_and_size_gives_a_picture_of_exactly_that_size(
            title in ".{0,80}",
            width in 0u32..400,
            height in 0u32..64,
        ) {
            if let Some(raster) = rasterize(&font(), &title, width, height, WHITE) {
                prop_assert_eq!((raster.width, raster.height), (width, height));
                prop_assert_eq!(raster.pixels.len(), width as usize * height as usize * 4);
            }
        }
    }
}
