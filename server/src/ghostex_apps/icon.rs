//! The icon an agent draws for its app: a square SVG or PNG file beside the app's HTML, named by
//! `<link rel="icon">`. `ghostex app show` checks it before it publishes, and gxserver draws it
//! into the PNG it keeps.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! User picked "the agent draws it" for an installed app's icon (Q4975), not an icon Ghostex makes up and not one the user picks. So an app must carry its own icon: `ghostex app show` refuses an app without a usable one and says how to add it, instead of falling back to a made-up icon.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's security review (finding 6) beat the first check, a list of things an SVG must not contain (`<svg/onload=…>`, `java&#115;cript:`, CSS `url(//host)`, `<feImage href="file:///…">`, an outside DOCTYPE, a PNG header claiming 60000 px). So the agent's file is never kept or served: gxserver draws it once into a 1024 px PNG and keeps only that. An SVG is drawn by resvg with no fonts, no outside files and no linked or embedded images (every picture `href` resolves to nothing), so nothing in it can run or reach anything; a PNG is decoded whole under a size cap and written out again. The Dock, the taskbar, `.icns` and Settings all start from that PNG.

use std::{io::Cursor, sync::Arc};

/// The two kinds of icon an agent may draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconKind {
    Svg,
    Png,
}

impl IconKind {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            IconKind::Svg => "svg",
            IconKind::Png => "png",
        }
    }

    pub(crate) fn from_extension(extension: &str) -> Option<IconKind> {
        match extension.to_ascii_lowercase().as_str() {
            "svg" => Some(IconKind::Svg),
            "png" => Some(IconKind::Png),
            _ => None,
        }
    }
}

pub(crate) const SVG_ICON_MAX_BYTES: usize = 64 * 1024;
pub(crate) const PNG_ICON_MAX_BYTES: usize = 512 * 1024;
/// A PNG icon must be at least this wide (and as tall), so the Dock's large sizes stay sharp.
pub(crate) const PNG_ICON_MIN_SIDE: u32 = 256;
/// And at most this wide, so decoding it stays small.
pub(crate) const PNG_ICON_MAX_SIDE: u32 = 2048;
/// The side of the PNG gxserver keeps (the largest `.icns` size, 512 pt at 2x).
pub(crate) const ICON_SIDE: u32 = 1024;
/// How far from square a drawing may be (its width against its height).
const SQUARE_TOLERANCE: f32 = 0.01;

/// What an agent is told when its app has no usable icon.
pub(crate) const ICON_HOW_TO: &str = "Draw the app's icon as a square SVG file (a viewBox like \"0 0 512 512\") beside the HTML and name it in the head: <link rel=\"icon\" href=\"icon.svg\">. A square PNG of 256 to 2048 px works too. See `ghostex app guide`.";

/// Checks an icon, and gives the reason when it cannot be used.
pub(crate) fn check_icon(kind: IconKind, bytes: &[u8]) -> Result<(), String> {
    icon_png(kind, bytes).map(|_| ())
}

/// The icon drawn into the [`ICON_SIDE`] px PNG gxserver keeps.
pub(crate) fn icon_png(kind: IconKind, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match kind {
        IconKind::Svg => svg_png(bytes),
        IconKind::Png => png_png(bytes),
    }
}

fn svg_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > SVG_ICON_MAX_BYTES {
        return Err(format!(
            "The icon is {} KiB; an SVG icon may be at most {} KiB.",
            bytes.len() / 1024,
            SVG_ICON_MAX_BYTES / 1024
        ));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "The SVG icon is not UTF-8 text.".to_string())?;
    let options = resvg::usvg::Options {
        resources_dir: None,
        // No fonts: text in an icon is not drawn (the guide says no words).
        fontdb: Arc::new(resvg::usvg::fontdb::Database::new()),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        style_sheet: None,
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(text, &options)
        .map_err(|error| format!("The SVG icon could not be read: {error}."))?;
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if (width - height).abs() > width.max(height) * SQUARE_TOLERANCE {
        return Err(format!(
            "The icon is {width}x{height}; it must be square (a viewBox like \"0 0 512 512\")."
        ));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIDE, ICON_SIDE)
        .ok_or_else(|| "The icon could not be drawn.".to_string())?;
    let scale = ICON_SIDE as f32 / width.max(height);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    if pixmap.pixels().iter().all(|pixel| pixel.alpha() == 0) {
        return Err(
            "The icon draws nothing: only shapes, paths and gradients are drawn, no text or images."
                .to_string(),
        );
    }
    pixmap
        .encode_png()
        .map_err(|error| format!("The icon could not be written: {error}."))
}

fn png_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > PNG_ICON_MAX_BYTES {
        return Err(format!(
            "The icon is {} KiB; a PNG icon may be at most {} KiB.",
            bytes.len() / 1024,
            PNG_ICON_MAX_BYTES / 1024
        ));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(PNG_ICON_MAX_SIDE);
    limits.max_image_height = Some(PNG_ICON_MAX_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| {
        format!("The icon is not a PNG that can be read (at most {PNG_ICON_MAX_SIDE} px a side).")
    })?;
    let (width, height) = (decoded.width(), decoded.height());
    if width != height || width < PNG_ICON_MIN_SIDE {
        return Err(format!(
            "The icon is {width}x{height}; a PNG icon must be square and at least {PNG_ICON_MIN_SIDE} px."
        ));
    }
    let resized = image::imageops::resize(
        &decoded.to_rgba8(),
        ICON_SIDE,
        ICON_SIDE,
        image::imageops::FilterType::Lanczos3,
    );
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(resized)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|error| format!("The icon could not be written: {error}."))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="118" fill="#f4b24c"/></svg>"##;

    fn is_png(bytes: &[u8]) -> bool {
        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    }

    #[test]
    fn draws_a_plain_square_svg_into_a_png() {
        let png = icon_png(IconKind::Svg, PLAIN).unwrap();
        assert!(is_png(&png));
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (ICON_SIDE, ICON_SIDE));
    }

    #[test]
    fn what_slipped_past_the_old_check_comes_out_as_a_plain_png_or_is_refused() {
        // Each of these got through the old deny-list (finding 6). Now what is kept is a PNG
        // gxserver drew, whatever the SVG held, so nothing of the SVG is ever served.
        for svg in [
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/onload="alert(1)"><rect width="1" height="1"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><a href="java&#115;cript:alert(1)"><rect width="8" height="8"/></a></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><style>rect{fill:url("//example.com/x")}</style><rect width="8" height="8" fill="red"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><filter id="f"><feImage href="file:///etc/hosts"/></filter><rect width="8" height="8" fill="red" filter="url(#f)"/></svg>"##,
            r##"<!DOCTYPE svg SYSTEM "http://example.com/x.dtd"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><image href="file:///etc/hosts" width="8" height="8"/><rect width="8" height="8"/></svg>"##,
        ] {
            if let Ok(png) = icon_png(IconKind::Svg, svg.as_bytes()) {
                assert!(is_png(&png), "{svg}");
            }
        }
    }

    #[test]
    fn refuses_odd_shapes_empty_drawings_and_big_files() {
        let wide = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect width="100" height="50"/></svg>"#;
        assert!(icon_png(IconKind::Svg, wide).is_err());
        let text_only = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><text y="8">Hi</text></svg>"#;
        assert!(icon_png(IconKind::Svg, text_only).is_err());
        let image_only = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><image href="file:///etc/hosts" width="8" height="8"/></svg>"#;
        assert!(icon_png(IconKind::Svg, image_only).is_err());
        assert!(icon_png(IconKind::Svg, b"not svg").is_err());
        let big = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"><!--{}--></svg>",
            "x".repeat(SVG_ICON_MAX_BYTES)
        );
        assert!(icon_png(IconKind::Svg, big.as_bytes()).is_err());
    }

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([10, 20, 30, 255]),
        ))
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
        out
    }

    #[test]
    fn decodes_pngs_whole_and_checks_their_size() {
        let png = icon_png(IconKind::Png, &png_of(256, 256)).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().width(), ICON_SIDE);
        assert!(icon_png(IconKind::Png, &png_of(128, 128)).is_err());
        assert!(icon_png(IconKind::Png, &png_of(300, 256)).is_err());
        // A header claiming 60000 px followed by junk (the review's case).
        let mut lying = png_of(256, 256)[..33].to_vec();
        lying[16..20].copy_from_slice(&60000u32.to_be_bytes());
        lying[20..24].copy_from_slice(&60000u32.to_be_bytes());
        lying.extend_from_slice(&[0u8; 64]);
        assert!(icon_png(IconKind::Png, &lying).is_err());
        let mut truncated = png_of(256, 256);
        truncated.truncate(truncated.len() / 2);
        assert!(icon_png(IconKind::Png, &truncated).is_err());
    }
}
