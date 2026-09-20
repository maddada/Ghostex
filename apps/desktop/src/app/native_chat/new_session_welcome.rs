/*!
The empty transcript region: the new-session welcome, the loading hold (drawn by
transcript_skeleton.rs), and the plain empty-state copy.

CDXC:SessionChat 2026-09-18 WHY:
GPUI chat used to print `emptyState.title` / `.detail` for every transcript with
no rows, so a brand new session sat on "Loading conversation… / Reading the agent
transcript." while React showed the agent mark and "What should we build with X?".
The host now projects `newSessionWelcome` and `loadingStage` from the same shared
rules React reads, and this module renders them.
SEE-ALSO: packages/shared/session-chat-presentation/new-session-welcome.ts,
packages/core-ui/chat/session-chat-new-session-welcome.tsx (and its
`.ghostex-chat-new-session*` rules in packages/core-ui/styles/chat.css).
*/

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::helpers::*;
use crate::app::window::native_modal_kit::css_mix;
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, Hsla, IntoElement, ParentElement as _, Styled as _};
use gpui::{div, img, px, rgb, svg};
use serde_json::{Value, json};

/// `.ghostex-chat-new-session-agent`: the 3rem rounded card the mark sits in.
const AGENT_CARD_SIZE: f32 = 48.0;
/// `.ghostex-chat-new-session-agent-logo`: 1.75rem of artwork inside it.
const AGENT_LOGO_SIZE: f32 = 28.0;
/// `.ghostex-chat-new-session-title`: 1.375rem / 600.
///
/// React also tightens it with `letter-spacing: -0.025em`, which GPUI has no style for, so the
/// headline measures a few percent wider here at the same glyph size. Do not shrink the size to
/// compensate: that trades a width difference for a cap-height one.
const TITLE_TEXT_SIZE: f32 = 22.0;

/// Port of `getBrandAgentLogoStyle` (packages/core-ui/agent-logos.ts) against the
/// chat's own theme tokens rather than the app chrome's.
pub(super) fn brand_logo_color(icon: &str, p: &ChatAppearance) -> Hsla {
    match icon {
        // chat.css: `--ghostex-zcode-logo` is black on light chat, white on dark.
        "zcode" => {
            if p.light {
                rgb(0x000000).into()
            } else {
                rgb(0xffffff).into()
            }
        }
        // chat.css: `--ghostex-codex-logo` resolves to the foreground on light chat.
        "codex" if p.light => p.foreground,
        _ => {
            let accent = workspace_tab_agent_icon_accent_color(icon);
            // `--ghostex-light-icon-color`: the white and near-white marks would be
            // invisible on light chat, so they take the foreground there.
            if p.light && matches!(accent, 0xffffff | 0xedecec) {
                p.foreground
            } else {
                rgb(accent).into()
            }
        }
    }
}

impl NativeChatView {
    /// Everything the chat draws when the transcript has no rows.
    pub(crate) fn render_empty_transcript_region(
        &mut self,
        state: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        /*
        The loading hold. A chat whose host has not published its first snapshot yet is still
        reading its transcript too, so it draws the skeleton instead of a blank region.
        */
        let stage = state["loadingStage"]
            .as_str()
            .or_else(|| (state["status"].is_null() && self.error.is_none()).then_some("indicator"));
        if let Some(stage) = stage {
            return self.render_transcript_skeleton(stage, p, cx);
        }
        let s = p.scale;
        // `.ghostex-chat-new-session`: `padding: 1.5rem 1.5rem var(--ghostex-chat-composer-overlay)`.
        // The band below already reserves its own height, so the welcome has no bottom padding of
        // its own; adding one lifts the whole block off centre.
        let region = div()
            .flex_1()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.0 * s))
            .pt(px(24.0 * s))
            .px(px(24.0 * s));

        if let Some(welcome) = state["newSessionWelcome"].as_object() {
            let icon = welcome
                .get("icon")
                .and_then(Value::as_str)
                .filter(|icon| !icon.is_empty());
            return region
                .child(
                    div()
                        .size(px(AGENT_CARD_SIZE * s))
                        .rounded(px(16.0 * s))
                        .border_1()
                        .border_color(p.border)
                        .bg(css_mix(p.card_background.into(), 0.88, p.foreground.into()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(new_session_agent_mark(icon, p)),
                )
                .when(
                    welcome.get("showTitle").and_then(Value::as_bool) != Some(false),
                    |this| {
                        this.child(
                            div()
                                .w_full()
                                .min_w_0()
                                .text_center()
                                .whitespace_nowrap()
                                .text_size(px(TITLE_TEXT_SIZE * s))
                                .line_height(px(TITLE_TEXT_SIZE * 1.25 * s))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(p.foreground)
                                .child(new_session_welcome_title_wrap(
                                    welcome
                                        .get("title")
                                        .and_then(Value::as_str)
                                        .unwrap_or("What should we work on?"),
                                )),
                        )
                    },
                )
                .into_any_element();
        }

        // `.ghostex-chat-empty-state` keeps the bottom padding the welcome drops.
        let region = region.pb(px(24.0 * s));

        /*
        A transcript that is ready but has no rows yet is React's message list with nothing in it,
        so it draws nothing rather than the empty-state copy ("Start a chat with …").
        */
        if state["view"]["kind"] == "ready" {
            return region.into_any_element();
        }

        region
            .child(
                state["emptyState"]["title"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
            .child(
                div().text_color(p.muted).child(
                    state["emptyState"]["detail"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                ),
            )
            .when(state["status"] == "error", |this| {
                this.child(self.chat_button(
                    "retry-chat".into(),
                    "Retry".into(),
                    json!({"type":"retry"}),
                    p,
                    cx,
                ))
            })
            .into_any_element()
    }
}

/// CDXC:SessionChat 2026-09-20 DECISION:
/// User: when GPUI chat is very narrow, the welcome title wraps, is center aligned, and the second line has 2 or 3 words, never 1.
/// GPUI treats U+00A0 as a wrap point, so a hard newline is the break; `whitespace_nowrap` keeps the last line from splitting again. A 6+ word headline keeps 3 words on the last line ("What should we" / "build with Codex?"); shorter ones keep 2.
/// SEE-ALSO: packages/shared/session-chat-presentation/new-session-welcome.ts (`wrapNewSessionWelcomeTitle`).
fn new_session_welcome_title_wrap(title: &str) -> String {
    if title.contains('\n') {
        return title.to_owned();
    }
    let words: Vec<&str> = title.split_whitespace().collect();
    if words.len() < 4 {
        return title.to_owned();
    }
    let last_count = if words.len() >= 6 { 3 } else { 2 };
    let split = words.len() - last_count;
    format!("{}\n{}", words[..split].join(" "), words[split..].join(" "))
}

/// The agent's brand mark, or the generic robot glyph when the agent has no artwork.
fn new_session_agent_mark(icon: Option<&str>, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let Some(icon) = icon else {
        return svg()
            .size(px(AGENT_LOGO_SIZE * s))
            .path("titlebar/robot.svg")
            .text_color(p.foreground)
            .into_any_element();
    };
    let Some(asset) = workspace_tab_agent_icon_path(icon) else {
        return svg()
            .size(px(AGENT_LOGO_SIZE * s))
            .path("titlebar/robot.svg")
            .text_color(p.foreground)
            .into_any_element();
    };
    // OMP's artwork is deliberately multicolor, so it is drawn as an image rather
    // than flattened to one brand color (the `getBrandAgentLogoStyle` OMP branch).
    if icon == "omp" {
        return img(asset).size(px(AGENT_LOGO_SIZE * s)).into_any_element();
    }
    svg()
        .size(px(AGENT_LOGO_SIZE * s))
        .path(asset)
        .text_color(brand_logo_color(icon, p))
        .into_any_element()
}
