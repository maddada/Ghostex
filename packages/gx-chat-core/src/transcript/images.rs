//! Turning a transcript image block into something a renderer can show.
//!
//! Ported from `packages/shared/session-chat-presentation/images.ts`.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The one rule for turning a transcript image block into something a renderer can actually show.
//! GPUI chat reads the fields off the projected wire item in
//! apps/desktop/src/app/native_chat/images.rs. A picture that renders on one platform and falls
//! back to a named chip on another is a bug here.

use serde_json::Value;

use crate::transcript::jsstr::{ascii_lower, js_trim, last_path_segment};

/// One transcript image block, as either a wire block or a Markdown destination supplies it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageRef {
    pub path: Option<String>,
    pub url: Option<String>,
    pub alt: Option<String>,
}

/// A composer paste writes `ghostex-paste-*.png`, which reads as "Pasted image" rather than a
/// machine file name.
fn is_pasted_image_name(segment: &str) -> bool {
    let lower = ascii_lower(segment);
    lower.starts_with("ghostex-paste-")
        && lower.ends_with(".png")
        && lower.len() > "ghostex-paste-.png".len()
}

pub fn is_pasted_image_path(path: Option<&str>) -> bool {
    path.is_some_and(|path| is_pasted_image_name(last_path_segment(path)))
}

/// The name a picture answers to: its chip when the bytes cannot be read, and the viewer's title.
pub fn image_label(block: &ImageRef) -> String {
    if is_pasted_image_path(block.path.as_deref()) {
        return "Pasted image".to_string();
    }
    if let Some(path) = block.path.as_deref() {
        return last_path_segment(path).to_string();
    }
    block
        .alt
        .clone()
        .or_else(|| block.url.clone())
        .unwrap_or_else(|| "Image".to_string())
}

/// `/\.[a-z0-9]{2,5}$/i`: a plausible file extension at the end of a segment.
fn has_short_extension(segment: &str) -> bool {
    let Some(dot) = segment.rfind('.') else {
        return false;
    };
    let extension = &segment[dot + 1..];
    (2..=5).contains(&extension.len()) && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn download_file_name(label: &str, path: &str, url: &str) -> String {
    let source = if path.is_empty() { url } else { path };
    let without_query = source.split(['?', '#']).next().unwrap_or_default();
    let segment = last_path_segment(without_query);
    if has_short_extension(segment) {
        return segment.to_string();
    }
    let mut base = String::with_capacity(label.len());
    let mut in_run = false;
    for character in label.chars() {
        if "\\/:*?\"<>|".contains(character) {
            if !in_run {
                base.push('-');
                in_run = true;
            }
            continue;
        }
        in_run = false;
        base.push(character);
    }
    let base = js_trim(&base);
    let base = if base.is_empty() { "image" } else { base };
    if has_short_extension(base) {
        base.to_string()
    } else {
        format!("{base}.png")
    }
}

/// Classifies one transcript image block into a source both renderers can load.
///
/// `transport` says how a renderer obtains the bytes: `url` renders the address as-is, `data`
/// already carries them inline, `read` is a path on the session's machine that only
/// `readSessionChatImage` can open, and `none` is a block with neither, which stays a named chip.
pub fn image_source(block: &ImageRef) -> Value {
    Value::Object(
        image_source_pairs(block, None)
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

/// The same source, as an ordered key list.
///
/// The native Markdown marks serialize this INTO the Markdown string, where the key order is part
/// of the text the renderer parses back, so it cannot go through a sorted map.
/// `label_override` is the stand-in text React used, which the inline-image mark substitutes.
pub fn image_source_pairs(
    block: &ImageRef,
    label_override: Option<&str>,
) -> Vec<(&'static str, Value)> {
    let url = block.url.clone().unwrap_or_default();
    let path = block.path.clone().unwrap_or_default();
    let alt = block.alt.clone().unwrap_or_default();
    let label = image_label(block);
    let lower = ascii_lower(&url);
    let transport = if lower.starts_with("data:") {
        "data"
    } else if lower.starts_with("http:") || lower.starts_with("https:") {
        "url"
    } else if !path.is_empty() {
        "read"
    } else {
        "none"
    };
    let file_name = download_file_name(&label, &path, &url);
    vec![
        ("transport", transport.into()),
        (
            "path",
            if transport == "read" {
                path.as_str()
            } else {
                ""
            }
            .into(),
        ),
        (
            "url",
            if transport == "url" || transport == "data" {
                url.as_str()
            } else {
                ""
            }
            .into(),
        ),
        ("alt", alt.into()),
        ("label", label_override.unwrap_or(&label).into()),
        // A data URL is bytes rather than a location, so it offers nothing to copy.
        (
            "copyPath",
            match transport {
                "read" => path.as_str(),
                "url" => url.as_str(),
                _ => "",
            }
            .into(),
        ),
        ("fileName", file_name.into()),
    ]
}
