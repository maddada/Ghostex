//! The Live styles' posters (assets/modals/settings/glass-live, rendered from the glass shaders by
//! tooling/glass-live/render-posters.swift in a sample theme), one per style and appearance. The
//! React gallery shows these stills, never the animation itself.
use gpui::{Image, ImageFormat};
use std::sync::Arc;

macro_rules! poster {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../../../assets/modals/settings/glass-live/",
            $name,
            ".webp"
        ))
    };
}

/// The WebP of `style` (`WINDOW_GLASS_LIVE_STYLE_OPTIONS`) in dark or light mode.
fn poster_bytes(style: &str, light: bool) -> Option<&'static [u8]> {
    Some(match (style, light) {
        ("aurora", false) => poster!("aurora-dark"),
        ("aurora", true) => poster!("aurora-light"),
        ("bokeh", false) => poster!("bokeh-dark"),
        ("bokeh", true) => poster!("bokeh-light"),
        ("drift", false) => poster!("drift-dark"),
        ("drift", true) => poster!("drift-light"),
        ("ink", false) => poster!("ink-dark"),
        ("ink", true) => poster!("ink-light"),
        ("mesh", false) => poster!("mesh-dark"),
        ("mesh", true) => poster!("mesh-light"),
        ("nebula", false) => poster!("nebula-dark"),
        ("nebula", true) => poster!("nebula-light"),
        ("silk", false) => poster!("silk-dark"),
        ("silk", true) => poster!("silk-light"),
        ("waves", false) => poster!("waves-dark"),
        ("waves", true) => poster!("waves-light"),
        _ => return None,
    })
}

/// The poster image of `style`, decoded by GPUI when first drawn.
pub(crate) fn poster(style: &str, light: bool) -> Option<Arc<Image>> {
    poster_bytes(style, light)
        .map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Webp, bytes.to_vec())))
}
