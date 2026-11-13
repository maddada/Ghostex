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
    serde_json::from_str(crate::assets::chat_references::REFERENCE_VISUAL_JSON)
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

/// A plain markdown link's color: the transcript's web-reference tone for this chat's own light or
/// dark mode, not the app theme's, which a chat drawn in the other mode paints near-invisible.
pub(super) fn transcript_link_color(appearance: &ChatAppearance) -> Option<Hsla> {
    let palette = if appearance.light {
        &VISUAL.transcript.light
    } else {
        &VISUAL.transcript.dark
    };
    hex_color(palette.get("url")?)
}

/// The Work panel's teal (`board` in the shared reference palette) for this chat's light or dark
/// mode: a work item pill's icon in the Workers list (coordinator_threads.rs).
pub(super) fn board_tint(appearance: &ChatAppearance) -> Option<Hsla> {
    let palette = if appearance.light {
        &VISUAL.transcript.light
    } else {
        &VISUAL.transcript.dark
    };
    hex_color(palette.get("board")?)
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
///
/// `board` is the chat's `coordinatorBoard` document value (`null` on a chat without a board),
/// which names the cards a board mention points at (`board_mention_link`).
pub(super) fn presentations(
    references: &Value,
    board: &Value,
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
            if kind == "board" {
                return Some((
                    (
                        reference["href"].as_str()?.to_owned(),
                        reference["sourceLabel"].as_str()?.to_owned(),
                    ),
                    board_mention_link(reference, board, palette, appearance)?,
                ));
            }
            if kind == "session" {
                return Some((
                    (
                        reference["href"].as_str()?.to_owned(),
                        reference["sourceLabel"].as_str()?.to_owned(),
                    ),
                    session_pill_link(reference, appearance, reference["missing"] == true),
                ));
            }
            if kind == "question" {
                let link = super::open_questions::question_tag_link(reference, appearance)?;
                return Some((
                    (
                        reference["href"].as_str()?.to_owned(),
                        reference["sourceLabel"].as_str()?.to_owned(),
                    ),
                    link,
                ));
            }
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
                    ..Default::default()
                },
            ))
        })
        .collect()
}

/// The icon of a board item's pill, the Board pill's own.
pub(super) const BOARD_ICON: &str = "titlebar/layout-board-split.svg";

/// The agent logo a session pill draws: the outline crown for a coordinator, the agent's artwork tinted
/// in its brand colour, else the generic robot; with the tint the label leaves to it.
pub(super) fn session_pill_icon(
    agent_icon: &str,
    coordinator: bool,
    appearance: &ChatAppearance,
) -> (&'static str, Hsla) {
    let plain = if appearance.light {
        rgb(0x000000).into()
    } else {
        rgb(0xffffff).into()
    };
    if coordinator {
        // The sidebar's coordinator crown (native_sidebar/threads.rs `coordinator_icon`).
        return ("titlebar/coordinator-crown-points.svg", plain);
    }
    match crate::app::helpers::workspace_tab_agent_icon_path(agent_icon) {
        Some(path) => (
            path,
            super::new_session_welcome::brand_logo_color(agent_icon, appearance),
        ),
        None => ("titlebar/robot.svg", appearance.foreground),
    }
}

/// A board card named in a message (`board` references, gx-chat-core `board_reference`, where the
/// user's decision lives): the session pill's shape with the Board pill's icon, tinted in the
/// board's own colour (`board` in the shared reference palette) so it reads as a card and not as
/// another session, and labelled with the card's title now. A card the board no longer has
/// draws as a muted session pill does, with the link's own text. A click goes to the core, which
/// shows the card on the board.
fn board_mention_link(
    reference: &Value,
    board: &Value,
    palette: &HashMap<String, String>,
    appearance: &ChatAppearance,
) -> Option<InlineLink> {
    let titles = &board["itemTitles"];
    let item = reference["item"].as_str().unwrap_or_default();
    let current = titles[item].as_str().filter(|title| !title.is_empty());
    let missing = titles.is_object() && current.is_none();
    let mut link = session_pill_link(reference, appearance, missing);
    link.icon = BOARD_ICON.into();
    if let Some(title) = current {
        link.label = title.to_owned().into();
    }
    if missing {
        link.icon_color = Some(appearance.foreground.opacity(0.45));
        if let Some(gone) = reference["goneTitle"].as_str() {
            link.title = gone.to_owned().into();
        }
        return Some(link);
    }
    let tint = hex_color(palette.get("board")?)?;
    link.icon_color = Some(tint);
    if let Some(frame) = link.frame.as_mut() {
        frame.border_color = tint.opacity(0.42);
        frame.background = Some(tint.opacity(0.1));
        frame.hover_background = Some(tint.opacity(0.18));
    }
    Some(link)
}

/// Another session named in a message (`session` references, gx-chat-core
/// `transcript/session_mentions.rs`, where the user's decision lives): the working card's pill
/// shape (a hairline rounded frame, chat_pills.rs) around the agent's logo and `+<title>`, muted
/// when the session is gone. A click goes to the core as any link does, which switches to it.
/// A board card's chip (`board_mention_link`) starts from it.
fn session_pill_link(reference: &Value, appearance: &ChatAppearance, missing: bool) -> InlineLink {
    let s = appearance.scale;
    let (icon, icon_color) = session_pill_icon(
        reference["agentIcon"].as_str().unwrap_or_default(),
        reference["coordinator"] == true,
        appearance,
    );
    let border = appearance.foreground.opacity(0.14);
    InlineLink {
        label: reference["label"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
            .into(),
        title: reference["title"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
            .into(),
        icon: icon.into(),
        icon_size: px(12.0 * s),
        gap: px(4.0 * s),
        color: if missing {
            appearance.card_muted
        } else {
            appearance.foreground
        },
        icon_color: Some(if missing {
            icon_color.opacity(0.45)
        } else {
            icon_color
        }),
        frame: Some(gpui_component::text::InlineLinkFrame {
            padding_x: px(6.0 * s),
            border_width: px(1.0),
            border_color: border,
            radius: px(999.0),
            hover_background: (!missing).then(|| appearance.foreground.opacity(0.06)),
            background: Some(appearance.foreground.opacity(0.03)),
        }),
        font_weight: Some(gpui::FontWeight::MEDIUM),
        font_scale: Some(0.93),
        ..Default::default()
    }
}

impl NativeChatView {
    /// CDXC:SessionChat 2026-10-09 DECISION:
    /// User: "for the #123 thing please make those show as links at all times when mentioned and clicking on them should open that pr (when not 6 or 8 chars for the #1234 text (in the gpui chat view", then "for #123 if there's a letter in it then it's def a color so show the color thing if you can for that if u get me". A `#` and digits in prose (whole token, not in code, not a six- or eight-digit hex colour, which keeps its swatch; a token with a letter a-f at 3, 4, 6 or 8 hex digits is a colour too and never a link) links to `https://github.com/<owner>/<repo>/issues/<n>` of the project's `origin`, which GitHub redirects to the pull request when it is one; a project without a GitHub origin keeps it plain text. The rule (which `#numbers` link, and the address from the `origin`) is gx-chat-core's `transcript/issue_links.rs` since 2026-11-11, shared with the phone; this view applies it in `rich_markdown.rs` (`text_view_look`).
    ///
    /// The origin comes from the sidebar store's project row (gxserver's cached `gitRemoteOriginUrl`), read in a deferred call because the app entity may be mid-update while the chat renders.
    pub(super) fn refresh_issue_links(&self, cx: &mut gpui::Context<Self>) {
        let Some(app) = self.config.app.clone() else {
            return;
        };
        let key = ghostex_gx_core::ProjectKey {
            machine: if self.config.machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
                ghostex_gx_core::MachineId::Local
            } else {
                ghostex_gx_core::MachineId::Remote(self.config.machine_id.clone())
            },
            project_id: self.config.project_id.clone(),
        };
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let base = app
                .read(cx)
                .gx_store
                .core
                .presentation()
                .project(&key)
                .and_then(|project| project.git_remote_origin_url.value())
                .and_then(|origin| {
                    ghostex_gx_chat_core::transcript::issue_links::github_issues_url(origin)
                })
                .map(gpui::SharedString::from);
            let _ = chat.update(cx, |chat, cx| {
                if chat.issue_link_base != base {
                    chat.issue_link_base = base;
                    cx.notify();
                }
            });
        });
    }
}
