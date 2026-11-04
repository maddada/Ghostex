//! Draws a transcript diagram with the renderer the Docs view and the diagram popup use
//! (mermaid-rs-renderer to SVG, then resvg), so a diagram reads the same in all three.
//!
//! SEE-ALSO: apps/gpui-web/src/app/native_chat/mermaid_render.rs (the browser build) is the other
//! copy of this file.

use crate::app::native_docs::blocks::{mermaid_svg, rasterize_svg_scaled, svg_natural_size};
use crate::app::window::unsupported_note;
use gpui::RenderImage;
use std::sync::Arc;

/// The drawing's SVG and its own size, or why the source is shown instead. Runs off the UI thread.
pub(super) fn svg(source: &str, light: bool) -> Result<(Arc<str>, f32, f32), String> {
    if let Some(note) = unsupported_note(source, light) {
        return Err(note.to_string());
    }
    let svg = mermaid_svg(source, light)
        .map_err(|error| format!("This diagram could not be drawn: {error}"))?;
    let (width, height) = svg_natural_size(&svg)
        .ok_or_else(|| "The diagram image could not be displayed.".to_string())?;
    Ok((svg.into(), width, height))
}

/// The SVG painted at `scale` device pixels per SVG unit. Runs off the UI thread.
pub(super) fn raster(svg: &str, scale: f32) -> Option<Arc<RenderImage>> {
    rasterize_svg_scaled(svg, scale)
}

/// Whether this host can open the larger diagram viewer.
pub(super) const HAS_VIEWER: bool = true;
