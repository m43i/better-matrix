//! Thumbnails that fit the box they were asked for.
//!
//! A homeserver answers a thumbnail request with the nearest size it has made, which can be
//! well over the box (Synapse answers 128×128 with its 320×240 rendition), and the SDK answers
//! one for an encrypted file with the whole original. Apps size their image budgets by the box,
//! so a thumbnail larger than it is scaled down here, once, off the async threads, before it
//! leaves the engine.

use std::{io::Cursor, sync::Arc};

use image::{
    DynamicImage, ExtendedColorType, ImageBuffer, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits, Rgba,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::{self, FilterType},
    metadata::Orientation,
};

/// The largest image scaled, a side: what the apps' decoders take too (Rune's limit). Larger
/// ones pass through for the app to refuse.
const MAX_SIDE: u32 = 8192;
/// The largest decoded image scaled (Rune's limit too).
const MAX_ALLOC: u64 = 256 << 20;
/// What a decoder may allocate for itself while it reads a header (an embedded colour profile,
/// say): the png crate's own default.
const DECODER_ALLOC: u64 = 64 << 20;
/// Images scaled at once: a large photo takes a few hundred megabytes while it decodes.
static SCALING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// [`fit`] on a blocking thread, a couple at a time.
pub(crate) async fn fit_off_thread(bytes: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    let permit = SCALING.acquire().await;
    let bytes = Arc::new(bytes);
    let source = Arc::clone(&bytes);
    // The permit goes with the work: a fetch given up on leaves its decode counted until it ends.
    let scaling = move || {
        let _permit = permit;
        scaled_safely(&source, width, height)
    };
    let scaled = tokio::task::spawn_blocking(scaling).await.ok().flatten();
    scaled.unwrap_or_else(|| Arc::try_unwrap(bytes).unwrap_or_else(|shared| shared.as_ref().clone()))
}

/// `bytes` scaled down to fit `width`×`height` when the image is larger, keeping its aspect
/// ratio, colour profile and alpha, and turned upright by its EXIF orientation; else `bytes`
/// unchanged. Only still PNGs and JPEGs up to [`MAX_SIDE`] a side are scaled (other formats,
/// animated PNGs among them, pass through), and anything that fails to decode or encode passes
/// through too.
#[cfg(test)]
fn fit(bytes: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    scaled_safely(&bytes, width, height).unwrap_or(bytes)
}

/// [`scaled`], with a codec that panics on a malformed file counting as a failure.
fn scaled_safely(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scaled(bytes, width, height))).ok().flatten()
}

fn scaled(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let format = reader.format().filter(|format| matches!(format, ImageFormat::Png | ImageFormat::Jpeg))?;
    if format == ImageFormat::Png && animated_png(bytes) {
        return None;
    }
    // Bounded from the start: a decoder reads a header with the limits it is made with.
    let mut header_limits = Limits::default();
    header_limits.max_image_width = Some(MAX_SIDE);
    header_limits.max_image_height = Some(MAX_SIDE);
    header_limits.max_alloc = Some(DECODER_ALLOC);
    reader.limits(header_limits);
    let mut decoder = reader.into_decoder().ok()?;
    // Read from the header: an image that already fits is never decoded.
    let (w, h) = decoder.dimensions();
    if w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (shown_w, shown_h) = if swaps_sides(orientation) { (h, w) } else { (w, h) };
    let scale = (f64::from(width) / f64::from(shown_w.max(1))).min(f64::from(height) / f64::from(shown_h.max(1)));
    if scale >= 1.0 {
        // It fits: a JPEG turned by EXIF keeps its bytes too, since the apps' decoders read EXIF.
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    // The decoded image itself counts against the limit, not just the decoder's own buffers.
    limits.reserve(decoder.total_bytes()).ok()?;
    decoder.set_limits(limits).ok()?;
    let icc = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let image = resized(image, side(shown_w), side(shown_h));
    let mut out = Vec::new();
    match format {
        ImageFormat::Jpeg => {
            let rgb = image.into_rgb8();
            let mut encoder = JpegEncoder::new_with_quality(&mut out, 85);
            if let Some(icc) = icc.filter(|icc| profile_fits(icc, false)) {
                let _ = encoder.set_icc_profile(icc);
            }
            encoder.write_image(&rgb, rgb.width(), rgb.height(), ExtendedColorType::Rgb8).ok()?;
        }
        _ => {
            let mut encoder = PngEncoder::new(&mut out);
            let grey = !image.color().has_color();
            if let Some(icc) = icc.filter(|icc| profile_fits(icc, grey)) {
                let _ = encoder.set_icc_profile(icc);
            }
            let (w, h) = (image.width(), image.height());
            encoder.write_image(image.as_bytes(), w, h, image.color().into()).ok()?;
        }
    }
    Some(out)
}

/// A picture waiting to be sent, as a `side`×`side` square for the composer: turned upright by
/// its EXIF orientation, its longer side cropped evenly, then scaled down (a smaller one keeps
/// its size). Decoding is bounded as for thumbnails ([`MAX_SIDE`] a side, [`MAX_ALLOC`] in all,
/// formats the app's `image` build reads); `None` past that, or for anything that fails, a
/// malformed file panicking its codec included. Blocking: call it off the UI thread.
pub fn upload_thumbnail(bytes: &[u8], side: u32) -> Option<image::RgbaImage> {
    std::panic::catch_unwind(|| upload_square(bytes, side)).ok().flatten()
}

fn upload_square(bytes: &[u8], side: u32) -> Option<image::RgbaImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(DECODER_ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits.reserve(decoder.total_bytes()).ok()?;
    decoder.set_limits(limits).ok()?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let (w, h) = (image.width(), image.height());
    let cut = w.min(h);
    let square = image.crop_imm((w - cut) / 2, (h - cut) / 2, cut, cut);
    let square = if cut > side { resized(square, side, side) } else { square };
    Some(square.into_rgba8())
}

/// `image` at `width`×`height`. One with alpha is scaled premultiplied, so its transparent
/// pixels don't darken the edges of what shows.
fn resized(image: DynamicImage, width: u32, height: u32) -> DynamicImage {
    if !image.color().has_alpha() {
        return image.resize_exact(width, height, FilterType::Triangle);
    }
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let mut scaled: ImageBuffer<Rgba<u8>, Vec<u8>> = imageops::resize(&rgba, width, height, FilterType::Triangle);
    for pixel in scaled.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    DynamicImage::ImageRgba8(scaled)
}

/// Whether a PNG is animated: an `acTL` chunk comes before its first `IDAT`.
fn animated_png(bytes: &[u8]) -> bool {
    let mut at = 8;
    while let Some(header) = bytes.get(at..at + 8) {
        match &header[4..8] {
            b"acTL" => return true,
            b"IDAT" => return false,
            _ => {}
        }
        let Ok(length) = <[u8; 4]>::try_from(&header[..4]) else { return false };
        let Some(next) = usize::try_from(u32::from_be_bytes(length))
            .ok()
            .and_then(|length| length.checked_add(12))
            .and_then(|length| at.checked_add(length))
        else {
            return false;
        };
        at = next;
    }
    false
}

/// Whether an ICC profile describes the colour model a scaled image is written in: grey, or
/// RGB. One for another (a CMYK photo's, a grey one's on an image now in colour) is left out.
fn profile_fits(icc: &[u8], grey: bool) -> bool {
    icc.get(16..20) == Some(if grey { b"GRAY" } else { b"RGB " })
}

/// Whether showing an image upright swaps its width and height.
fn swaps_sides(orientation: Orientation) -> bool {
    use Orientation::{FlipHorizontal, FlipVertical, NoTransforms, Rotate180};
    !matches!(orientation, NoTransforms | Rotate180 | FlipHorizontal | FlipVertical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_upload_thumbnail_should_be_a_square_of_the_side_asked_for() {
        let square = upload_thumbnail(&png(400, 200), 128).expect("a thumbnail");
        assert_eq!(square.dimensions(), (128, 128));
    }

    #[test]
    fn a_small_upload_thumbnail_should_keep_its_size() {
        let square = upload_thumbnail(&png(40, 90), 128).expect("a thumbnail");
        assert_eq!(square.dimensions(), (40, 40));
    }

    #[test]
    fn an_upload_too_long_a_side_should_get_no_thumbnail() {
        assert!(upload_thumbnail(&png(1, MAX_SIDE + 1), 128).is_none());
    }

    /// A 40×20 JPEG, red on its left half and blue on its right, whose EXIF says to show it
    /// turned a quarter clockwise (orientation 6): upright, red above blue.
    fn turned_jpeg() -> Vec<u8> {
        let image =
            ImageBuffer::from_fn(40, 20, |x, _| if x < 20 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) });
        let mut jpeg = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image).write_to(&mut jpeg, ImageFormat::Jpeg).expect("encodes");
        let jpeg = jpeg.into_inner();
        // TIFF, little-endian: one IFD entry, Orientation (0x0112), a SHORT, 6.
        let tiff =
            [b"II*\0".as_slice(), &[8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0]].concat();
        let exif = [b"Exif\0\0".as_slice(), &tiff].concat();
        let length = u16::try_from(exif.len() + 2).expect("short").to_be_bytes();
        [&jpeg[..2], &[0xFF, 0xE1], &length, &exif, &jpeg[2..]].concat()
    }

    #[test]
    fn an_upload_thumbnail_should_be_upright_by_its_exif_orientation() {
        let square = upload_thumbnail(&turned_jpeg(), 128).expect("a thumbnail");
        let (top, bottom) = (square.get_pixel(10, 2), square.get_pixel(10, 17));
        assert!(top[0] > 200 && bottom[2] > 200, "red above blue: {top:?} over {bottom:?}");
    }

    #[test]
    fn an_upload_that_is_not_a_picture_should_get_no_thumbnail() {
        assert!(upload_thumbnail(b"%PDF-1.7", 128).is_none());
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::new_rgba8(width, height).write_to(&mut out, ImageFormat::Png).expect("encodes");
        out.into_inner()
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(width, height).write_to(&mut out, ImageFormat::Jpeg).expect("encodes");
        out.into_inner()
    }

    fn decoded(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).expect("decodes")
    }

    fn size(bytes: &[u8]) -> (u32, u32) {
        decoded(bytes).to_rgba8().dimensions()
    }

    /// A PNG chunk: length, type, data and a CRC the scan doesn't check.
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(data.len()).expect("a short chunk").to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0; 4]);
        out
    }

    #[test]
    fn a_thumbnail_larger_than_its_box_should_be_scaled_to_fit_it() {
        assert_eq!(size(&fit(png(320, 240), 128, 128)), (128, 96));
    }

    #[test]
    fn a_jpeg_larger_than_its_box_should_stay_a_jpeg() {
        let scaled = fit(jpeg(640, 480), 128, 128);
        assert_eq!((image::guess_format(&scaled).expect("a format"), size(&scaled)), (ImageFormat::Jpeg, (128, 96)));
    }

    #[test]
    fn a_tall_image_should_fit_the_height_of_its_box() {
        assert_eq!(size(&fit(jpeg(320, 640), 128, 128)), (64, 128));
    }

    #[test]
    fn a_thumbnail_that_fits_its_box_should_keep_its_bytes() {
        let small = png(96, 96);
        assert_eq!(fit(small.clone(), 128, 128), small);
    }

    #[test]
    fn bytes_that_are_not_an_image_should_pass_through() {
        assert_eq!(fit(b"not an image".to_vec(), 16, 16), b"not an image");
    }

    #[test]
    fn a_gif_should_pass_through_so_it_keeps_its_frames() {
        // A GIF's header, which is all the format is told by.
        let gif = b"GIF89a\x2c\x01\x2c\x01\x00\x00\x00;".to_vec();
        assert_eq!(fit(gif.clone(), 64, 64), gif);
    }

    #[test]
    fn an_image_too_large_to_decode_should_pass_through() {
        // Its header claims more pixels a side than any app decodes.
        let mut huge = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = 20_000u32.to_be_bytes().to_vec();
        header.extend_from_slice(&20_000u32.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        huge.extend(chunk(b"IHDR", &header));
        huge.extend(chunk(b"IEND", &[]));
        assert_eq!(fit(huge.clone(), 128, 128), huge);
    }

    #[test]
    fn an_animated_png_should_be_told_apart_from_a_still_one() {
        let still = png(4, 4);
        let mut animated = still[..8].to_vec();
        animated.extend(chunk(b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]));
        animated.extend_from_slice(&still[8..]);
        assert_eq!((animated_png(&still), animated_png(&animated)), (false, true));
    }

    /// A stand-in ICC profile of `space` (`b"RGB "` or `b"GRAY"`): only its header is read.
    fn profile(space: &[u8; 4]) -> Vec<u8> {
        let mut icc = vec![0; 128];
        icc[16..20].copy_from_slice(space);
        icc
    }

    fn png_with_profile(width: u32, height: u32, icc: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = PngEncoder::new(&mut out);
        encoder.set_icc_profile(icc).expect("takes a profile");
        let pixels = vec![200; (width * height * 3) as usize];
        encoder.write_image(&pixels, width, height, ExtendedColorType::Rgb8).expect("encodes");
        out
    }

    fn profile_of(bytes: &[u8]) -> Option<Vec<u8>> {
        let mut decoder = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?.into_decoder().ok()?;
        decoder.icc_profile().ok().flatten()
    }

    #[test]
    fn a_scaled_image_should_keep_its_colour_profile() {
        let rgb = profile(b"RGB ");
        assert_eq!(profile_of(&fit(png_with_profile(256, 256, rgb.clone()), 64, 64)), Some(rgb));
    }

    #[test]
    fn a_profile_for_another_colour_model_should_be_left_out() {
        assert_eq!(profile_of(&fit(png_with_profile(256, 256, profile(b"CMYK")), 64, 64)), None);
    }

    #[test]
    fn an_image_too_large_to_hold_decoded_should_pass_through() {
        // 8192 a side at 16-bit RGBA: 512 MiB decoded, past the limit, though its sides are not.
        let mut huge = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = 8192u32.to_be_bytes().to_vec();
        header.extend_from_slice(&8192u32.to_be_bytes());
        header.extend_from_slice(&[16, 6, 0, 0, 0]);
        huge.extend(chunk(b"IHDR", &header));
        huge.extend(chunk(b"IEND", &[]));
        assert_eq!(fit(huge.clone(), 128, 128), huge);
    }

    #[test]
    fn a_chunk_claiming_more_bytes_than_there_are_should_end_the_scan() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&u32::MAX.to_be_bytes());
        png.extend_from_slice(b"tEXt");
        assert!(!animated_png(&png));
    }

    #[test]
    fn a_scaled_picture_with_transparent_edges_should_keep_its_colour() {
        // Red at 40% opacity, with fully transparent pixels around it.
        let mut picture = image::RgbaImage::new(256, 256);
        for (x, y, pixel) in picture.enumerate_pixels_mut() {
            let inside = (64..192).contains(&x) && (64..192).contains(&y);
            *pixel = if inside { Rgba([255, 0, 0, 102]) } else { Rgba([0; 4]) };
        }
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(picture).write_to(&mut out, ImageFormat::Png).expect("encodes");
        let scaled = decoded(&fit(out.into_inner(), 64, 64)).to_rgba8();
        // An edge pixel, partly covered: its colour stays red, only its alpha falls.
        let edge = scaled.get_pixel(16, 32);
        assert!(edge[3] > 0 && edge[0] >= 250, "{edge:?}");
    }
}
