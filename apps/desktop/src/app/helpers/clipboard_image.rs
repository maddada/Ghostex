use gpui::ImageFormat;
use image::{DynamicImage, ImageEncoder as _, imageops::FilterType};
use std::io::Cursor;

/// CDXC:Clipboard 2026-11-01 DECISION: User chose to convert BMP and TIFF to PNG with the same pixels before upload, and to downscale only as a last resort above 6000 px or 32 MiB. This helper serves chat and terminal paste in the desktop and GPUI web build.
pub(crate) const CHAT_IMAGE_MAX_BYTES: usize = 32 * 1024 * 1024;
const CHAT_IMAGE_MAX_SIDE: u32 = 6000;

pub(crate) struct PreparedClipboardImage {
    pub(crate) bytes: Vec<u8>,
    pub(crate) extension: &'static str,
}

pub(crate) fn prepare_clipboard_image(
    bytes: &[u8],
    format: ImageFormat,
) -> Result<PreparedClipboardImage, String> {
    if bytes.is_empty() {
        return Err("The pasted image is empty.".to_string());
    }
    let extension = match format {
        ImageFormat::Bmp | ImageFormat::Tiff => "png",
        _ => format.extension(),
    };
    let needs_png = matches!(format, ImageFormat::Bmp | ImageFormat::Tiff);
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "The pasted image could not be decoded.".to_string())?;
    let dimensions = reader.into_dimensions().ok();
    let too_wide =
        dimensions.is_some_and(|(width, height)| width.max(height) > CHAT_IMAGE_MAX_SIDE);
    if !needs_png && !too_wide && bytes.len() <= CHAT_IMAGE_MAX_BYTES {
        return Ok(PreparedClipboardImage {
            bytes: bytes.to_vec(),
            extension,
        });
    }

    let mut decoded = image::load_from_memory(bytes).map_err(|_| {
        if bytes.len() > CHAT_IMAGE_MAX_BYTES {
            too_large_message(bytes.len())
        } else {
            "The pasted image could not be decoded.".to_string()
        }
    })?;
    if too_wide {
        decoded = fit_long_side(decoded, CHAT_IMAGE_MAX_SIDE);
    }
    for _ in 0..4 {
        let mut output = Vec::new();
        decoded
            .write_to(&mut Cursor::new(&mut output), image::ImageFormat::Png)
            .map_err(|_| "The pasted image could not be converted to PNG.".to_string())?;
        if output.len() <= CHAT_IMAGE_MAX_BYTES {
            return Ok(PreparedClipboardImage {
                bytes: output,
                extension: "png",
            });
        }
        let next_side = (u64::from(decoded.width().max(decoded.height())) * 3 / 4).max(1) as u32;
        decoded = fit_long_side(decoded, next_side);
    }

    // A photographic PNG can still exceed the upload cap after the dimension bound.
    let mut output = Vec::new();
    let rgb = decoded.to_rgb8();
    for quality in [85, 70, 55] {
        output.clear();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality)
            .write_image(
                &rgb,
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|_| "The pasted image could not be reduced.".to_string())?;
        if output.len() <= CHAT_IMAGE_MAX_BYTES {
            return Ok(PreparedClipboardImage {
                bytes: output,
                extension: "jpg",
            });
        }
    }
    Err(too_large_message(bytes.len()))
}

fn too_large_message(bytes: usize) -> String {
    format!(
        "Image too large: {:.1} MB. Limit: 32 MiB. Try a smaller capture.",
        bytes as f64 / 1_000_000.0
    )
}

fn fit_long_side(image: DynamicImage, max_side: u32) -> DynamicImage {
    let long_side = image.width().max(image.height());
    if long_side <= max_side {
        return image;
    }
    let width =
        (u64::from(image.width()) * u64::from(max_side) / u64::from(long_side)).max(1) as u32;
    let height =
        (u64::from(image.height()) * u64::from(max_side) / u64::from(long_side)).max(1) as u32;
    image.resize_exact(width, height, FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_and_tiff_become_pixel_identical_pngs() {
        let original = DynamicImage::ImageRgb8(image::RgbImage::from_fn(3, 2, |x, y| {
            image::Rgb([(x * 70) as u8, (y * 90) as u8, 123])
        }));
        for (source, format) in [
            (image::ImageFormat::Bmp, ImageFormat::Bmp),
            (image::ImageFormat::Tiff, ImageFormat::Tiff),
        ] {
            let mut bytes = Vec::new();
            original
                .write_to(&mut Cursor::new(&mut bytes), source)
                .unwrap();
            let prepared = prepare_clipboard_image(&bytes, format).unwrap();
            assert_eq!(prepared.extension, "png");
            assert_eq!(
                image::guess_format(&prepared.bytes).unwrap(),
                image::ImageFormat::Png
            );
            assert_eq!(
                image::load_from_memory(&prepared.bytes).unwrap().to_rgb8(),
                original.to_rgb8()
            );
        }
    }

    #[test]
    fn over_6000_pixels_is_reduced_and_small_png_is_preserved() {
        let original = DynamicImage::ImageRgb8(image::RgbImage::new(6001, 1));
        let mut bytes = Vec::new();
        original
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let prepared = prepare_clipboard_image(&bytes, ImageFormat::Png).unwrap();
        assert_eq!(
            image::load_from_memory(&prepared.bytes).unwrap().width(),
            6000
        );
        let small = DynamicImage::ImageRgb8(image::RgbImage::new(2, 2));
        let mut small_bytes = Vec::new();
        small
            .write_to(&mut Cursor::new(&mut small_bytes), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(
            prepare_clipboard_image(&small_bytes, ImageFormat::Png)
                .unwrap()
                .bytes,
            small_bytes
        );
    }

    #[test]
    fn oversized_bmp_that_compresses_to_png_keeps_its_dimensions() {
        let original = DynamicImage::ImageRgb8(image::RgbImage::new(4000, 3000));
        let mut bytes = Vec::new();
        original
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Bmp)
            .unwrap();
        assert!(bytes.len() > CHAT_IMAGE_MAX_BYTES);
        let prepared = prepare_clipboard_image(&bytes, ImageFormat::Bmp).unwrap();
        assert_eq!(prepared.extension, "png");
        let decoded = image::load_from_memory(&prepared.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4000, 3000));
    }
}
