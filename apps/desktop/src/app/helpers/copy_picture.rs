//! Copy image: puts a picture on the clipboard as PNG, checks it got there, and says which.

use gpui::{App, ClipboardItem, Image, ImageFormat};
use std::io::Cursor;

/// The longest side an SVG is drawn at for the clipboard.
const SVG_MAX_SIDE: f32 = 4096.0;

/// CDXC:Clipboard 2026-11-11 WHY: Copy image in the Files picture viewer did nothing on Windows
/// (user: "the copy image button does nothing (on windows, didn't check on mac)"): GPUI put the
/// picture on the clipboard as a registered "PNG" format only, which Paint and most Windows apps
/// do not read, and nothing told the user. The GPUI fork now adds the bitmap formats Windows
/// apps read (`gpui_windows/src/clipboard.rs`); here every picture copy goes on as PNG, the one
/// format every app on every platform reads (a WebP, BMP, TIFF or SVG was offered under its own
/// type only, which most macOS and Linux apps skip), and the app then reads the clipboard back
/// where that is reliable (Windows and macOS; a Wayland read right after a write still sees the
/// previous owner) so a copy that did not land says so in plain words instead of showing
/// "Copied!". Returns whether the picture is on the clipboard.
pub(crate) fn gpui_copy_picture_to_clipboard(picture: &Image, cx: &mut App) -> bool {
    let Some(png) = picture_as_png(picture) else {
        gpui_copy_failed_feedback("Couldn't read this image", cx);
        return false;
    };
    cx.write_to_clipboard(ClipboardItem::new_image(&png));
    if !clipboard_holds(&png, cx) {
        log::warn!(
            "Copy image: the clipboard did not take the {} byte PNG",
            png.bytes().len()
        );
        gpui_copy_failed_feedback("Couldn't copy the image", cx);
        return false;
    }
    super::gpui_copy_feedback(cx);
    true
}

/// Tells the user a copy did not happen, where "Copied!" would have shown.
pub(crate) fn gpui_copy_failed_feedback(message: &'static str, cx: &mut App) {
    crate::app::window::copied_indicator::show_copy_failed_indicator(message, cx);
}

fn picture_as_png(picture: &Image) -> Option<Image> {
    let bytes = picture.bytes();
    if bytes.is_empty() {
        return None;
    }
    if picture.format == ImageFormat::Png {
        return Some(picture.clone());
    }
    let png = if picture.format == ImageFormat::Svg {
        svg_png(bytes)
    } else {
        let decoded = image::load_from_memory(bytes)
            .map_err(|error| log::warn!("Copy image: cannot decode the picture: {error}"))
            .ok()?;
        let mut png = Vec::new();
        decoded
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()
            .map(|_| png)
    }?;
    Some(Image::from_bytes(ImageFormat::Png, png))
}

fn svg_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let tree = crate::app::native_docs::blocks::svg_tree(std::str::from_utf8(bytes).ok()?)?;
    let natural = tree.size();
    let scale = (SVG_MAX_SIDE / natural.width().max(natural.height())).min(2.0);
    let width = (natural.width() * scale).ceil().max(1.0) as u32;
    let height = (natural.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    let mut png = Vec::new();
    image::RgbaImage::from_raw(width, height, rgba)?
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn clipboard_holds(png: &Image, cx: &mut App) -> bool {
    // Windows hands a clipboard block back rounded up to its allocation size, so compare the
    // start.
    cx.read_from_clipboard().is_some_and(|item| {
        item.entries().iter().any(|entry| {
            matches!(entry, gpui::ClipboardEntry::Image(image) if image.bytes().starts_with(png.bytes()))
        })
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn clipboard_holds(_: &Image, _: &mut App) -> bool {
    true
}
