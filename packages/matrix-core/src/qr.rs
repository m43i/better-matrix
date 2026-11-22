//! QR codes: drawing one for another device to scan (verification, signing in with a QR code),
//! and reading one from a camera picture. Matrix's codes carry binary data, which the phones'
//! barcode scanners hand over as lossy text; the engine reads the picture itself instead.

use std::path::Path;

use crate::Error;

/// A QR code to draw: `width` × `width` modules, row by row, `true` where a module is dark.
/// Draw it with a quiet zone of four light modules around it.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct QrImage {
    /// Modules per side.
    pub width: u32,
    /// The modules, row by row: `width * width` of them.
    pub dark: Vec<bool>,
}

// Hand-written: the modules are noise in a log line.
impl std::fmt::Debug for QrImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "QrImage({}×{})", self.width, self.width)
    }
}

impl QrImage {
    /// The code for `data`, at the error correction Matrix's codes use (low: they are big).
    pub fn encode(data: &[u8]) -> Result<Self, Error> {
        let code = qrcode::QrCode::with_error_correction_level(data, qrcode::EcLevel::L)
            .map_err(|e| Error::QrCode(e.to_string()))?;
        Ok(Self::from_code(&code))
    }

    /// The modules of a code the SDK drew.
    pub(crate) fn from_code(code: &qrcode::QrCode) -> Self {
        let width = u32::try_from(code.width()).unwrap_or(0);
        Self { width, dark: code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect() }
    }

    /// Whether the module at column `x`, row `y` is dark; outside the code, light.
    pub fn is_dark(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.width && self.dark.get((y * self.width + x) as usize).copied().unwrap_or(false)
    }
}

/// The raw bytes of the first QR code found in the picture at `path` (a PNG or JPEG), or `None`
/// when it shows none that reads. Blocking: decodes the whole picture.
pub fn read_picture(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    let picture = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|source| Error::ReadFile { path: path.to_owned(), source })?
        .decode()
        .map_err(|e| Error::QrCode(e.to_string()))?;
    // Camera pictures are large; a QR code filling part of the frame still reads at ~1000 px.
    let picture = if picture.width().max(picture.height()) > 1200 {
        picture.resize(1200, 1200, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    Ok(read_luma(&picture.to_luma8()))
}

/// The raw bytes of the first QR code that reads in a greyscale picture.
pub(crate) fn read_luma(picture: &image::GrayImage) -> Option<Vec<u8>> {
    let (width, height) = (picture.width() as usize, picture.height() as usize);
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| picture.get_pixel(x as u32, y as u32).0[0]);
    prepared.detect_grids().into_iter().find_map(|grid| {
        let mut bytes = Vec::new();
        grid.decode_to(&mut bytes).ok().map(|_| bytes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws `code` as a greyscale picture: `scale` pixels per module, four modules of margin.
    fn picture(code: &QrImage, scale: u32) -> image::GrayImage {
        let side = (code.width + 8) * scale;
        image::GrayImage::from_fn(side, side, |x, y| {
            let (mx, my) = ((x / scale).wrapping_sub(4), (y / scale).wrapping_sub(4));
            image::Luma([if code.is_dark(mx, my) { 0 } else { 255 }])
        })
    }

    #[test]
    fn an_encoded_code_should_be_square() {
        let code = QrImage::encode(b"MATRIX").expect("encodes");
        assert_eq!(code.dark.len(), (code.width * code.width) as usize);
    }

    #[test]
    fn binary_data_should_read_back_unchanged_from_a_picture_of_its_code() {
        let data: Vec<u8> = (0..=255).collect();
        let code = QrImage::encode(&data).expect("encodes");
        assert_eq!(read_luma(&picture(&code, 4)), Some(data));
    }

    #[test]
    fn a_picture_without_a_code_should_read_as_none() {
        let blank = image::GrayImage::from_pixel(200, 200, image::Luma([255]));
        assert_eq!(read_luma(&blank), None);
    }

    #[test]
    fn a_module_outside_the_code_should_be_light() {
        let code = QrImage::encode(b"x").expect("encodes");
        assert!(!code.is_dark(code.width, 0));
    }
}
