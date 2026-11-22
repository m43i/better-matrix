//! QR codes for another device to scan: dark modules on white whatever the theme (scanners need
//! the contrast), inside a four-module quiet zone, drawn as one pixel image.

use std::cell::RefCell;

use matrix_core::QrImage;
use rune::{Color, El, el};

/// Modules of light margin around a code, as the QR standard asks.
const QUIET_ZONE: u32 = 4;

/// How many codes keep their drawn image: the one on screen and the one before it.
const KEPT: usize = 2;

thread_local! {
    /// Codes already drawn, newest last: a frame that shows the same code uploads nothing.
    static DRAWN: RefCell<Vec<(QrImage, rune::Pixels)>> = const { RefCell::new(Vec::new()) };
}

/// `code`, `side` points square, on white.
pub fn qr_code(key: &str, code: &QrImage, side: f32) -> El {
    el().key(key.to_owned())
        .size(side, side)
        .radius(8.0)
        .fill(Color::WHITE)
        .role("img")
        .child(el().w_full().h_full().pixels(&pixels(code, side)))
}

/// The drawn image of `code`, from the cache or drawn now.
fn pixels(code: &QrImage, side: f32) -> rune::Pixels {
    DRAWN.with_borrow_mut(|drawn| {
        if let Some((_, pixels)) = drawn.iter().find(|(known, _)| known == code) {
            return pixels.clone();
        }
        let pixels = rune::Pixels::new();
        let (size, rgba) = rgba(code, side);
        pixels.set(size, size, rgba);
        if drawn.len() == KEPT {
            drawn.remove(0);
        }
        drawn.push((code.clone(), pixels.clone()));
        pixels
    })
}

/// The code as straight RGBA, with whole pixels per module and enough of them to stay crisp at
/// twice `side` (sharp screens): scaling an image down blurs far less than scaling one up.
fn rgba(code: &QrImage, side: f32) -> (u32, Vec<u8>) {
    let modules = code.width + 2 * QUIET_ZONE;
    let wanted = (side * 2.0).ceil().max(1.0) as u32;
    let scale = wanted.div_ceil(modules).max(1);
    let size = modules * scale;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        let row = (y / scale).wrapping_sub(QUIET_ZONE);
        for x in 0..size {
            let column = (x / scale).wrapping_sub(QUIET_ZONE);
            let shade = if code.is_dark(column, row) { 0 } else { 255 };
            rgba.extend_from_slice(&[shade, shade, shade, 255]);
        }
    }
    (size, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> QrImage {
        QrImage::encode(b"better matrix").expect("encodes")
    }

    #[test]
    fn a_code_should_draw_with_a_light_quiet_zone() {
        let (size, rgba) = rgba(&code(), 100.0);
        let scale = size / (code().width + 2 * QUIET_ZONE);
        let margin = (QUIET_ZONE * scale) as usize;
        let row = &rgba[..size as usize * 4];
        assert!(row.iter().all(|byte| *byte == 255), "the top row is margin");
        let first_module = (margin * size as usize + margin) * 4;
        assert_eq!(rgba[first_module], 0, "a finder pattern starts dark at the corner");
    }

    #[test]
    fn a_code_should_draw_at_least_twice_its_size_in_whole_pixels_per_module() {
        let (size, _) = rgba(&code(), 200.0);
        assert!(size >= 400 && size % (code().width + 2 * QUIET_ZONE) == 0);
    }
}
