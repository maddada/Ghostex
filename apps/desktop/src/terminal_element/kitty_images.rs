//! GPUI upload, lifetime and painting for owned Kitty image snapshots.
use super::TerminalLayout;
use crate::terminal_model::TerminalSnapshot;
use gpui::{App, Bounds, ContentMask, Corners, Pixels, Point, RenderImage, Window, point, size};
use image::Frame;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

// Bound this view's BGRA uploads separately from Ghostty's retained pixels.
// Count the replicated border too; retain visible cached generations and
// omit new uploads over either limit until space is freed offscreen.
const MAX_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_IMAGE_CACHE_ENTRIES: usize = 256;

pub(super) fn release_image_cache(cache: &mut HashMap<u64, Arc<RenderImage>>, cx: &mut App) {
    if cache.is_empty() {
        return;
    }
    let images = std::mem::take(cache);
    // During an update the current window is absent from App.windows.
    // Defer removal until it is returned so every atlas is reached.
    cx.defer(move |cx| {
        for (_, image) in images {
            cx.drop_image(image, None);
        }
    });
}

pub(super) fn layout_images(
    cache: &mut HashMap<u64, Arc<RenderImage>>,
    frame: &TerminalSnapshot,
    window: &mut Window,
    cx: &mut App,
) -> Vec<(crate::ghostty_vt::VtImagePlacement, Arc<RenderImage>)> {
    let active_images: HashSet<_> = frame.images.iter().map(|p| p.image.generation).collect();
    cache.retain(|generation, image| {
        if active_images.contains(generation) {
            return true;
        }
        cx.drop_image(Arc::clone(image), Some(window));
        false
    });
    let mut cache_bytes: usize = cache
        .values()
        .filter_map(|image| image.as_bytes(0))
        .map(|pixels| pixels.len())
        .sum();
    frame
        .images
        .iter()
        .filter_map(|placement| {
            if !cache.contains_key(&placement.image.generation) {
                let source = &placement.image;
                let bytes = (source.width as usize + 2) * (source.height as usize + 2) * 4;
                if cache.len() >= MAX_IMAGE_CACHE_ENTRIES
                    || bytes > MAX_IMAGE_CACHE_BYTES.saturating_sub(cache_bytes)
                {
                    return None;
                }
                cache_bytes += bytes;
            }
            let image = cache.entry(placement.image.generation).or_insert_with(|| {
                // A duplicated one-pixel border gives linear filtering the
                // same edge clamp as Ghostty's standalone image textures.
                // GPUI shares an atlas, whose transparent gutter otherwise
                // bleeds into enlarged images (especially 1x1 assets).
                let source = &placement.image;
                let pixels =
                    image::RgbaImage::from_fn(source.width + 2, source.height + 2, |x, y| {
                        let x = x.saturating_sub(1).min(source.width - 1);
                        let y = y.saturating_sub(1).min(source.height - 1);
                        let i = ((y * source.width + x) * 4) as usize;
                        let p = &source.rgba[i..i + 4];
                        image::Rgba([p[2], p[1], p[0], p[3]])
                    });
                Arc::new(RenderImage::new(vec![Frame::new(pixels)]))
            });
            Some((placement.clone(), Arc::clone(image)))
        })
        .collect()
}

pub(super) enum TerminalImageLayer {
    BelowBackground,
    BelowText,
    AboveText,
}

/// GPUI samples the whole uploaded image. Position that whole image so the
/// requested source rectangle maps to the destination, then clip to that
/// destination and the pane's existing content mask.
pub(super) fn paint_terminal_images(
    layout: &TerminalLayout,
    origin: Point<Pixels>,
    layer: TerminalImageLayer,
    window: &mut Window,
) {
    // Ghostty uses integer cell pixels; GPUI's logical cell metrics can be
    // fractional. Map those coordinate spaces per axis so fragments remain
    // aligned with the text grid across font and display-scale changes.
    let x_unit = layout.metrics.cell_width / layout.cell_size_px.0.max(1) as f32;
    let y_unit = layout.metrics.line_height / layout.cell_size_px.1.max(1) as f32;
    for (p, image) in &layout.images {
        let in_layer = match layer {
            TerminalImageLayer::BelowBackground => p.z < -(1 << 30),
            TerminalImageLayer::BelowText => (-(1 << 30)..0).contains(&p.z),
            TerminalImageLayer::AboveText => p.z >= 0,
        };
        if !in_layer || p.source_width <= 0.0 || p.source_height <= 0.0 {
            continue;
        }
        let destination = Bounds::new(
            point(
                origin.x + layout.metrics.cell_width * p.col as f32 + x_unit * p.offset_x as f32,
                origin.y + layout.metrics.line_height * p.row as f32 + y_unit * p.offset_y as f32,
            ),
            size(x_unit * p.width as f32, y_unit * p.height as f32),
        );
        let x_scale = destination.size.width / p.source_width as f32;
        let y_scale = destination.size.height / p.source_height as f32;
        let full_image = Bounds::new(
            point(
                destination.origin.x - x_scale * (p.source_x as f32 + 1.0),
                destination.origin.y - y_scale * (p.source_y as f32 + 1.0),
            ),
            size(
                x_scale * (p.image.width + 2) as f32,
                y_scale * (p.image.height + 2) as f32,
            ),
        );
        // Clip with the content mask rather than paint_image's integer
        // atlas sub-tile. A placeholder run can span less than one source
        // pixel; rounding that crop makes small images disappear or streak.
        window.with_content_mask(
            Some(ContentMask {
                bounds: destination,
            }),
            |window| {
                let _ = window.paint_image(
                    full_image,
                    full_image,
                    Corners::default(),
                    Arc::clone(image),
                    0,
                    false,
                );
            },
        );
    }
}
