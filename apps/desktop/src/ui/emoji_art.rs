//! Draw the bundled Noto emoji through Rune's image API. The surrounding controls and text
//! retain the Unicode sequences they send, copy and expose to accessibility tools.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

use rune::{Color, El, Pixels, SurfaceFit, TextStyle, el, text};
use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};
use swash::{FontRef, shape::ShapeContext};

use crate::state::EmojiStyle;

const COLOUR: &[u8] = include_bytes!("../../../../assets/emoji/NotoColorEmoji.ttf");
const MONOCHROME: &[u8] = include_bytes!("../../../../assets/emoji/BetterMatrixNotoEmoji.ttf");
const RASTER_SIZE: f32 = 56.0;

thread_local! {
    static ART: RefCell<Artwork> = RefCell::new(Artwork {
        colour: FontRef::from_index(COLOUR, 0),
        monochrome: FontRef::from_index(MONOCHROME, 0),
        shape: ShapeContext::new(),
        scale: ScaleContext::new(),
        images: HashMap::new(),
    });
}

struct Artwork {
    colour: Option<FontRef<'static>>,
    monochrome: Option<FontRef<'static>>,
    shape: ShapeContext,
    scale: ScaleContext,
    images: HashMap<(&'static str, EmojiStyle, u8), Option<Pixels>>,
}

/// One emoji in a control, with a text fallback for reactions that contain other text.
pub(super) fn glyph(value: &str, style: EmojiStyle, ink: Color, size: f32) -> El {
    match emojis::get(value).and_then(|emoji| artwork(emoji, style, ink)) {
        Some(pixels) => el().size(size, size).pixels_fit(&pixels, SurfaceFit::Fit),
        None => text(value.to_owned()).font(TextStyle::new(size, 400.0)).color(ink),
    }
}

pub(super) fn artwork(emoji: &'static emojis::Emoji, style: EmojiStyle, ink: Color) -> Option<Pixels> {
    // Monochrome outlines use the theme's text brightness. The colour bitmap is shared by
    // both themes, and the finite emoji catalogue bounds this cache.
    let ink = match style {
        EmojiStyle::Color => 0,
        EmojiStyle::Monochrome => (ink.premul_srgb(1.0)[0] * 255.0).round() as u8,
    };
    let key = (emoji.as_str(), style, ink);
    ART.with_borrow_mut(|art| {
        if let Some(cached) = art.images.get(&key) {
            return cached.clone();
        }
        let pixels = art.rasterise(emoji.as_str(), style, ink).map(|(w, h, rgba)| {
            let image = Pixels::new();
            image.set(w, h, rgba);
            image
        });
        art.images.insert(key, pixels.clone());
        pixels
    })
}

impl Artwork {
    fn rasterise(&mut self, sequence: &str, style: EmojiStyle, ink: u8) -> Option<(u32, u32, Vec<u8>)> {
        let font = self.colour?;
        let mut shaper = self.shape.builder(font).size(RASTER_SIZE).build();
        // The keycap ligature in these fonts matches the base digit and enclosing mark.
        let glyph_text = if sequence.ends_with('\u{20e3}') {
            Cow::Owned(sequence.replace('\u{fe0f}', ""))
        } else {
            Cow::Borrowed(sequence)
        };
        shaper.add_str(&glyph_text);
        let mut glyph = 0;
        let mut count = 0;
        shaper.shape_with(|cluster| {
            for item in cluster.glyphs.iter().filter(|item| item.advance != 0.0 || item.id == 0) {
                glyph = item.id;
                count += 1;
            }
        });
        if glyph == 0 || count != 1 {
            return None;
        }
        // The derived monochrome font keeps the colour font's glyph IDs. If it has no
        // outline for this glyph, read the original bitmap and give it a greyscale palette.
        let outline = (style == EmojiStyle::Monochrome)
            .then(|| {
                let mut scaler = self.scale.builder(self.monochrome?).size(RASTER_SIZE).build();
                Render::new(&[Source::Outline]).render(&mut scaler, glyph).filter(|image| !image.data.is_empty())
            })
            .flatten();
        let rendered = outline.or_else(|| {
            let mut scaler = self.scale.builder(font).size(RASTER_SIZE).build();
            Render::new(&[Source::ColorBitmap(StrikeWith::BestFit)]).render(&mut scaler, glyph)
        })?;
        let rgba = match rendered.content {
            Content::Color => {
                let mut rgba = rendered.data;
                if style == EmojiStyle::Monochrome {
                    for [r, g, b, _] in rgba.as_chunks_mut::<4>().0 {
                        let grey =
                            ((u32::from(*r) * 2126 + u32::from(*g) * 7152 + u32::from(*b) * 722 + 5000) / 10_000) as u8;
                        (*r, *g, *b) = (grey, grey, grey);
                    }
                }
                rgba
            }
            Content::Mask => rendered.data.into_iter().flat_map(|alpha| [ink, ink, ink, alpha]).collect(),
            Content::SubpixelMask => return None,
        };
        let (w, h) = (rendered.placement.width, rendered.placement.height);
        (w > 0 && h > 0).then_some((w, h, rgba))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_fonts_should_draw_emoji_sequences_in_both_styles() {
        ART.with_borrow_mut(|art| {
            for sequence in ["🙂", "👍🏽", "👩‍💻", "🇬🇧", "1️⃣", "🫩"] {
                for style in EmojiStyle::ALL {
                    let (w, h, rgba) =
                        art.rasterise(sequence, style, 32).unwrap_or_else(|| panic!("{sequence} {style:?}"));
                    let (pixels, remainder) = rgba.as_chunks::<4>();
                    assert_eq!(pixels.len(), (w * h) as usize);
                    assert!(remainder.is_empty() && pixels.iter().any(|pixel| pixel[3] > 0));
                    if style == EmojiStyle::Monochrome {
                        assert!(pixels.iter().all(|[r, g, b, _]| r == g && g == b), "{sequence}");
                    } else {
                        assert!(pixels.iter().any(|[r, g, b, a]| *a > 0 && (r != g || g != b)), "{sequence}");
                    }
                }
            }
        });
    }
}
