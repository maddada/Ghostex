use super::appearance::ChatAppearance;
use super::state::NativeChatView;
use gpui::{Hsla, px, rgb};
use gpui_component::text::InlineLink;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReferenceVisual {
    gap: f32,
    pub(super) icon_em: f32,
    dark_white_mix: f32,
    colors: HashMap<String, String>,
    transcript: TranscriptPalette,
    composer_url: String,
    web: WebVisual,
}

/// The reference colours of the transcript, one set per theme, used as written.
#[derive(Deserialize)]
struct TranscriptPalette {
    dark: HashMap<String, String>,
    light: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebVisual {
    gap_em: f32,
}

pub(super) static VISUAL: LazyLock<ReferenceVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/reference-visual.json"
    ))
    .expect("shared reference appearance")
});

/// Blend a shared hex color the way the chat stylesheet does: raw in light mode, lightened toward
/// white in dark mode so a pill keeps its hue without going muddy on the dark surface.
fn blended(hex: &str, appearance: &ChatAppearance) -> Option<Hsla> {
    let color = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    let mix = if appearance.light {
        0.0
    } else {
        VISUAL.dark_white_mix
    };
    let channel = |shift: u32| {
        (((color >> shift) & 255u32) as f32 * (1.0 - mix) + 255.0 * mix).round() as u32
    };
    Some(rgb(channel(16) << 16 | channel(8) << 8 | channel(0)).into())
}

fn hex_color(hex: &str) -> Option<Hsla> {
    Some(rgb(u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?).into())
}

/// The color a reference pill uses inside an editable composer.
///
/// CDXC:SessionChat 2026-09-18 WHY:
/// A link inside the composer is dimmer than the same link in the transcript, which is why the
/// composer reads `composerUrl` instead of the transcript's `web` colors.
pub(super) fn composer_color(kind: &str, appearance: &ChatAppearance) -> Option<Hsla> {
    let hex = if kind == "url" {
        &VISUAL.composer_url
    } else {
        VISUAL.colors.get(kind)?
    };
    blended(hex, appearance)
}

/// The transcript's right-click menu on a reference, wired into the markdown view's secondary-click
/// hook because these pills are inline links inside gpui-component's text, not elements of our own.
/// The rows and the ordering are the composer's (`reference_menu.rs`), the way React shared
/// `session-chat-reference-menu-items.tsx` between both. On the main transcript the pill leads the
/// transcript menu instead (`transcript_menu.rs`), which adds Copy and Add to Chat when text is
/// selected, as React's transcript menu did.
pub(super) fn secondary_click(
    chat: gpui::WeakEntity<NativeChatView>,
) -> impl Fn(&str, gpui::Modifiers, &mut gpui::Window, &mut gpui::App) + Send + Sync + 'static {
    move |href, _, window, cx| {
        let href = href.to_owned();
        let _ = chat.update(cx, |chat, cx| {
            let at = window.mouse_position();
            if chat.in_main_transcript(at) {
                chat.show_transcript_menu(Some(href), at, window, cx);
            } else {
                chat.show_reference_menu(href, at, window, cx);
            }
        });
    }
}

/// CDXC:SessionChat 2026-09-26 DECISION:
/// User: references in the transcript keep their shape (icon and coloured label, no chip) but take brighter, more saturated colours, because the muted ones read like normal text. The composer keeps its dimmer pills (`composer_color`).
/// SEE-ALSO: `transcript` in `packages/gx-chat-core/visual/reference-visual.json`, and `link` in `apps/mobile/app/src/chat/native/transcript/theme.ts`, which the phone chat must keep equal.
pub(super) fn presentations(
    references: &Value,
    appearance: &ChatAppearance,
) -> HashMap<(String, String), InlineLink> {
    let palette = if appearance.light {
        &VISUAL.transcript.light
    } else {
        &VISUAL.transcript.dark
    };
    references
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reference| {
            let kind = reference["kind"].as_str()?;
            let color = hex_color(palette.get(kind)?)?;
            Some((
                (
                    reference["href"].as_str()?.to_owned(),
                    reference["sourceLabel"].as_str()?.to_owned(),
                ),
                InlineLink {
                    label: reference["label"].as_str()?.to_owned().into(),
                    title: reference["title"].as_str()?.to_owned().into(),
                    icon: format!("chat-references/{kind}.svg").into(),
                    icon_size: px(14.0 * appearance.scale * VISUAL.icon_em),
                    gap: px(if kind == "url" {
                        14.0 * VISUAL.web.gap_em
                    } else {
                        VISUAL.gap
                    } * appearance.scale),
                    color,
                    // A path keeps its file name when the pill is wider than the line
                    // (`reference_path_label` in gx-chat-core).
                    truncate_start: matches!(kind, "file" | "folder" | "image"),
                },
            ))
        })
        .collect()
}
