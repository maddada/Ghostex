//! A panel's icons: a Tabler name from the app's own icon set (`titlebar/<name>.svg`), or a
//! one-colour SVG of the extension's (`/ext/<id>/<file>.svg` or a path in its folder), tinted like
//! the sidebar's own icons.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{AnyElement, Hsla, Image, IntoElement, Styled, px};

use crate::app::helpers::gpui_sidebar_command_icon_asset_path;

/// A panel icon at `size` points, or `None` for a block that names none.
pub(super) fn panel_icon(
    icon: Option<&str>,
    extension_id: &str,
    size: f32,
    color: Hsla,
) -> Option<AnyElement> {
    let icon = icon?.trim();
    if icon.is_empty() {
        return None;
    }
    if !icon.to_ascii_lowercase().ends_with(".svg") {
        return Some(
            gpui::svg()
                .size(px(size))
                .flex_shrink_0()
                .path(gpui_sidebar_command_icon_asset_path(Some(icon)))
                .text_color(color)
                .into_any_element(),
        );
    }
    let relative = icon
        .strip_prefix(&format!("/ext/{extension_id}/"))
        .unwrap_or(icon);
    Some(match extension_svg(extension_id, relative) {
        Some(image) => gpui::img(gpui::ImageSource::Image(image))
            .size(px(size))
            .flex_shrink_0()
            .into_any_element(),
        // The page has no access to the extension's folder (the web build draws no panels yet,
        // Q27), and a file that won't load keeps the row's place.
        None => gpui::svg()
            .size(px(size))
            .flex_shrink_0()
            .path(gpui_sidebar_command_icon_asset_path(Some("puzzle")))
            .text_color(color)
            .into_any_element(),
    })
}

fn extension_svg(extension_id: &str, relative: &str) -> Option<Arc<Image>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Image>>>>> = OnceLock::new();
    let key = format!("{extension_id}/{relative}");
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().ok()?.get(&key) {
        return found.clone();
    }
    let loaded = load_extension_svg(extension_id, relative);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, loaded.clone());
    }
    loaded
}

#[cfg(not(ghostex_web))]
fn load_extension_svg(extension_id: &str, relative: &str) -> Option<Arc<Image>> {
    use std::path::{Component, Path};
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::RootDir))
    {
        return None;
    }
    let bytes = std::fs::read(
        crate::shared_settings::ghostex_storage_paths()
            .extensions_dir()
            .join("installed")
            .join(extension_id)
            .join(path),
    )
    .ok()?;
    if bytes.is_empty() || bytes.len() > 256 * 1024 {
        return None;
    }
    // Drawn in the sidebar's icon grey, as the view tabs draw extension icons.
    let svg = String::from_utf8(bytes)
        .ok()?
        .replace("currentColor", "#b9b9b9");
    Some(Arc::new(Image::from_bytes(
        gpui::ImageFormat::Svg,
        svg.into_bytes(),
    )))
}

#[cfg(ghostex_web)]
fn load_extension_svg(_extension_id: &str, _relative: &str) -> Option<Arc<Image>> {
    None
}
