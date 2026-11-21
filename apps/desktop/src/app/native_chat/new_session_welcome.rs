/*!
The empty transcript region: the new-session welcome and the plain empty-state copy. The
loading hold that comes before either is transcript_reveal.rs.

CDXC:SessionChat 2026-09-18 WHY:
GPUI chat used to print `emptyState.title` / `.detail` for every transcript with
no rows, so a brand new session sat on "Loading conversation… / Reading the agent
transcript." while React showed the agent mark and "What should we build with X?".
The core now projects `newSessionWelcome` and `loadingStage` (packages/gx-chat-core/src/extras/welcome.rs),
and this module renders them. The sizes below come from the React welcome's
`.ghostex-chat-new-session*` rules in packages/core-ui/styles/chat.css (deleted 2026-09-25).
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
/// React also tightened it with `letter-spacing: -0.025em`, which GPUI has no style for, so the
/// headline measures a few percent wider here at the same glyph size. Do not shrink the size to
/// compensate: that trades a width difference for a cap-height one.
const TITLE_TEXT_SIZE: f32 = 22.0;

/// Port of `getBrandAgentLogoStyle` (packages/core-ui/agent-logos.ts (deleted 2026-10-01)) against the
/// chat's own theme tokens rather than the app chrome's.
/// CDXC:SessionChat 2026-09-13 DECISION:
/// User: the GPT/Codex icon is dark in light chat. Share the palette token across model pills, account panels and agent menus instead of colouring each instance separately. The model menu's `agent_logo` (option_menu/model_menu/render.rs) still carries its own copy of the white-mark rule.
/// CDXC:Icons 2026-09-15 DECISION:
/// User: the Z.ai/ZCode logo must be black on light UI and white on dark UI, including independently themed chat.
pub(super) fn brand_logo_color(icon: &str, p: &ChatAppearance) -> Hsla {
    match icon {
        // `--ghostex-zcode-logo` (styles/theme.css) is black on light chat, white on dark.
        "zcode" => {
            if p.light {
                rgb(0x000000).into()
            } else {
                rgb(0xffffff).into()
            }
        }
        // `--ghostex-codex-logo` resolved to the foreground on the React chat's light theme.
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

        // Ask Ghostex AI's own card in place of the mark, the headline and the tips
        // (chief_welcome.rs).
        if let Some(card) = state
            .get("chiefWelcome")
            .filter(|card| card.is_object())
            .and_then(|card| {
                serde_json::from_value::<ghostex_gx_chat_core::extras::chief_welcome::ChiefWelcome>(
                    card.clone(),
                )
                .ok()
            })
        {
            return self.render_chief_welcome(&card, p, cx);
        }

        if let Some(welcome) = state["newSessionWelcome"].as_object() {
            // The width the headline may take: the pane less the region's padding.
            let room = (self.bounds.get().size.width.as_f32() - 48.0 * s).max(0.0);
            let title = welcome
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("What should we work on?")
                .replace('\n', " ");
            let title_size = welcome_title_size(&title, room, s);
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
                                .truncate()
                                .text_size(px(title_size))
                                .line_height(px(title_size * 1.25))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(p.foreground)
                                .child(title),
                        )
                    },
                )
                // The tips card (new_chat_tips.rs): under a regular new chat's headline only; a
                // coordinator's welcome hides the headline and has its own intro.
                .when(
                    welcome.get("showTitle").and_then(Value::as_bool) != Some(false),
                    |this| this.children(self.render_new_chat_tips(p, cx)),
                )
                .into_any_element();
        }

        // `.ghostex-chat-empty-state` keeps the bottom padding the welcome drops.
        let region = region.pb(px(24.0 * s));

        /*
        A transcript that is ready but has no rows yet was React's message list with nothing in it,
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
                this.child(
                    self.chat_button(
                        "retry-chat".into(),
                        state["emptyState"]["action"]
                            .as_str()
                            .unwrap_or("Try again")
                            .to_owned(),
                        json!({"type":"retry"}),
                        p,
                        cx,
                    ),
                )
            })
            .into_any_element()
    }
}

/// The headline's size (scaled): `TITLE_TEXT_SIZE`, or smaller when the title, on one line, would
/// not fit `room` (about 0.54em a glyph at this weight), never under 14px.
/// CDXC:SessionChat 2026-11-20 DECISION: User, of "What should we / build with Claude?": "this
/// looks very ugly, make the text there not wrap". The headline is one line at every width; a
/// narrow chat shrinks it instead of breaking it (supersedes the 2026-09-20 two-line wrap), and
/// below 14px it ends in an ellipsis.
/// SEE-ALSO: packages/gx-chat-core/src/extras/welcome.rs (the title, no longer wrapped).
fn welcome_title_size(title: &str, room: f32, s: f32) -> f32 {
    let full = TITLE_TEXT_SIZE * s;
    let natural = title.chars().count() as f32 * 0.54 * full;
    if natural <= room || natural <= 0.0 {
        full
    } else {
        (full * room / natural).max(14.0 * s)
    }
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
