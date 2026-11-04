//! A session's PR, issue and Linear chips: the line and the badge a sidebar row shows (as the
//! "Session links in the sidebar" setting says), and the chip drawing the work area header shares
//! (render/workarea_header/session_links.rs). Hovering a row opens the floating links card
//! (window/session_links_card.rs on the desktop).
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "also didn't we agree to have a way to see these floating on the right of the sidebar when i hover somewhere? (instead of showing them just below the sidebar item". Hovering a row that has links opens a card beside the sidebar listing them all, after the tooltip delay; the row no longer grows by a line on hover. This supersedes the earlier 2026-10-24 rule that a hovered card expands to show its links below it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/links.rs (`SessionLinksDisplay` and what a row
//! shows, glyphs, tones and words), apps/desktop/src/app/gx_store/session_links.rs (what a click
//! sends), apps/mobile/app/src/chat/native/sessionLinks.ts (the phone's copy of the row rule).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, TextRun, WindowTextSystem, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::json;

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarSession;
use super::tooltips::SidebarTooltipSpan;
pub(crate) use super::tooltips::{
    in_window_bubble, sidebar_style_card_tooltip, sidebar_style_tooltip,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use ghostex_gx_core::{
    SessionLink, SessionLinkGlyph, SessionLinkTone, SessionLinks, SessionLinksDisplay,
    SessionLinksRow,
};

/// The line's height, the Work mode card's chip line's (work_chips.rs).
pub(crate) const LINK_LINE_HEIGHT: f32 = 16.0;
/// A text's width in the sidebar's UI font at `size`, unscaled.
pub(crate) fn ui_text_width(text_system: &WindowTextSystem, text: &str, size: f32) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: gpui::font(crate::ui_fonts::UI_FONT),
        ..Default::default()
    };
    f32::from(
        text_system
            .shape_line(text.to_owned().into(), px(size), &[run], None)
            .width,
    )
}

/// How wide `session_link_chip_body` draws, unscaled: glyph, gap, label, and a PR's checks mark.
fn link_chip_width(text_system: &WindowTextSystem, link: &SessionLink, text_size: f32) -> f32 {
    12.0 + 3.0
        + ui_text_width(text_system, &link.label, text_size)
        + if link.checks.is_some() {
            3.0 + 12.0
        } else {
            0.0
        }
}

/// How wide `session_links_overflow_pill` draws for `hidden`, unscaled: its padding and border,
/// "+N", and the failing dot.
fn overflow_pill_width(
    text_system: &WindowTextSystem,
    hidden: usize,
    failing: bool,
    text_size: f32,
) -> f32 {
    2.0 * 6.0
        + 2.0
        + ui_text_width(text_system, &format!("+{hidden}"), text_size)
        + if failing { 4.0 + 5.0 } else { 0.0 }
}

pub(crate) fn session_link_glyph_icon(glyph: SessionLinkGlyph) -> &'static str {
    match glyph {
        SessionLinkGlyph::Branch => "titlebar/git-branch.svg",
        SessionLinkGlyph::Github => "titlebar/brand-github.svg",
        SessionLinkGlyph::Linear => "titlebar/brand-linear.svg",
        SessionLinkGlyph::Ticket => "titlebar/circle-dot.svg",
        SessionLinkGlyph::ChecksPassing => "titlebar/circle-check.svg",
        SessionLinkGlyph::ChecksFailing => "titlebar/circle-x.svg",
        SessionLinkGlyph::ChecksPending => "titlebar/loader2.svg",
    }
}

/// The work chips' colours (work_chips.rs), the same in both themes.
pub(crate) fn session_link_tone_color(tone: SessionLinkTone) -> Hsla {
    rgb(match tone {
        SessionLinkTone::Open => 0x3fb950,
        SessionLinkTone::Draft => 0x8b949e,
        SessionLinkTone::Merged => 0xa371f7,
        SessionLinkTone::Closed => 0xf85149,
        SessionLinkTone::Pending => 0xd29922,
        SessionLinkTone::Started => 0xf2c94c,
        SessionLinkTone::Review => 0x4cb782,
        SessionLinkTone::Done => 0x5e6ad2,
        SessionLinkTone::Linear => 0x7c84f0,
    })
    .into()
}

/// A link badge's colour (the sidebar's Badge option and the chat's Links pill, links_pill.rs): the
/// state's colour muted to the project header's diff numbers. An open link takes the header's "+N"
/// green and failing checks or changes asked for its "-N" red, at the header's 70%; every other
/// state takes the same softness in its own hue (merged and closed a soft purple, a draft grey).
///
/// CDXC:SessionLinks 2026-10-29 DECISION:
/// User, of the link tag in the sidebar and the Links pill: "the green here is too much fam for the links tag. let's please make it match the color of the + lines that we have there in project header. also same here. also please same for the purple we have for merged or closed". The badge and the pill take `project_diff_additions_color` for open work (the header's own value, light and dark), and a purple of the same saturation and lightness (dark #aa94c8, light #4b2a7a) for merged and closed; pending, started, review, done and Linear keep their hues at that softness, a draft is grey, failing checks or changes asked for take the header's deletions red; all at the header's 70%. Supersedes the 2026-10-29 "much more faint" pill (60% of the vivid colour). A row's links line (its chips) takes the same muted colours, which is where a merged or closed link's purple shows (the badge itself turns grey once every link is finished, the 2026-10-27 decision in gx-core); the links lists' row icons keep the vivid colours.
pub(crate) fn session_link_badge_color(tone: SessionLinkTone) -> Hsla {
    use super::project_status::{
        PROJECT_DIFF_OPACITY, project_diff_additions_color, project_diff_deletions_color,
    };
    let color = match tone {
        SessionLinkTone::Open => project_diff_additions_color(),
        SessionLinkTone::Closed => project_diff_deletions_color(),
        SessionLinkTone::Merged => chrome_color(0xaa94c8, 0x4b2a7a),
        SessionLinkTone::Draft => chrome_color(0xaeaeae, 0x525252),
        SessionLinkTone::Pending => chrome_color(0xc8b794, 0x7a5f2a),
        SessionLinkTone::Started => chrome_color(0xc8be94, 0x7a6a2a),
        SessionLinkTone::Review => chrome_color(0x94c8ae, 0x2a7a52),
        SessionLinkTone::Done | SessionLinkTone::Linear => chrome_color(0x9497c8, 0x2a2f7a),
    };
    Hsla::from(color).opacity(PROJECT_DIFF_OPACITY)
}

/// What clicking a chip of session `session_id` (a sidebar session id) sends: the link opens where
/// Settings says, in that session's project (gx_store/session_links.rs).
pub(crate) fn open_session_link_command(
    link: &SessionLink,
    session_id: &str,
) -> Option<serde_json::Value> {
    link.url
        .as_ref()
        .map(|url| json!({ "type": "openSessionLink", "url": url, "sessionId": session_id }))
}

/// One chip on a sidebar row's links line: the state glyph, the number or ID, and a PR's checks
/// mark, in the badge's muted colours (`session_link_badge_color`).
pub(crate) fn session_link_chip_body(
    link: &SessionLink,
    scale: f32,
    text_size: f32,
    muted: Hsla,
) -> gpui::Div {
    let (glyph, tone) = link.glyph();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * scale))
        .text_size(px(text_size * scale))
        .text_color(muted)
        .whitespace_nowrap()
        .child(titlebar_svg_icon(
            session_link_glyph_icon(glyph),
            12.0 * scale,
            session_link_badge_color(tone),
        ))
        .child(link.label.clone())
        .when_some(link.checks_mark(), |chip, (glyph, tone)| {
            chip.child(titlebar_svg_icon(
                session_link_glyph_icon(glyph),
                12.0 * scale,
                session_link_badge_color(tone),
            ))
        })
}

/// The Badge option's badge: the named link's ID (or the suggestion count) and a "+" glued to it
/// when there are other links, in `color` (gx-core `SessionLinks::badge_label`). A linked badge is
/// bare text in the workers count's size and weight; suggestions keep the link icon and a dashed
/// pill, so their count doesn't read as a link ID.
///
/// CDXC:SessionLinks 2026-10-29 DECISION:
/// User, of the badge's "#270 +30": "i wanna remove the +30 thingy here in the links tag, just make it show + (same font and no space after the #id)". With more than one link the badge and the chat's Links pill read "#270+" (the "+" in the ID's font, weight and colour, no space, no count); one link reads "#270". The hover card and the links list still list them all. Supersedes the lighter "+N" of 2026-10-28.
///
/// CDXC:SessionLinks 2026-10-29 DECISION:
/// User, of the row "Main Coordinator  (🔗 #348)  👥1": "please make the link pill font size and font and font weight same as the font for the number 1 there and also please remove the border around it and the link icon; need t make links look similar to elements next to it". The linked badge drops its pill border, padding and link icon and draws "#348" in the coordinator badge's text size (threads.rs `BADGE_TEXT_SIZE`) at the regular weight, keeping its colour. The phone's SessionRow.tsx follows.
fn session_links_badge_body(
    label: &str,
    more: usize,
    color: Hsla,
    dashed: bool,
    scale: f32,
) -> gpui::Div {
    let text = if more > 0 {
        format!("{label}+")
    } else {
        label.to_string()
    };
    if !dashed {
        return div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .text_size(px(super::threads::BADGE_TEXT_SIZE * scale))
            .text_color(color)
            .whitespace_nowrap()
            .child(text);
    }
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * scale))
        .h(px(16.0 * scale))
        .px(px(5.0 * scale))
        .rounded_full()
        .border_1()
        .border_dashed()
        .border_color(color.opacity(0.45))
        .text_size(px(10.5 * scale))
        .text_color(color)
        .child(titlebar_svg_icon("titlebar/link.svg", 10.0 * scale, color))
        .child(text)
}

/// The "+N" pill: the links that did not fit, with a red dot when one of them has failing checks.
///
/// CDXC:SessionLinks 2026-10-24 SEE-ALSO: the user's "dot-only" answer (server/src/server/session_links_sync.rs); this dot is how a failing PR behind "+N" still shows.
pub(crate) fn session_links_overflow_pill(
    hidden: usize,
    failing: bool,
    scale: f32,
    text_size: f32,
    foreground: Hsla,
    border: Hsla,
) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0 * scale))
        .h(px(15.0 * scale))
        .px(px(6.0 * scale))
        .rounded_full()
        .border_1()
        .border_color(border)
        .text_size(px(text_size * scale))
        .text_color(foreground)
        .whitespace_nowrap()
        .child(format!("+{hidden}"))
        .when(failing, |pill| {
            pill.child(
                div()
                    .size(px(5.0 * scale))
                    .rounded_full()
                    .bg(session_link_tone_color(SessionLinkTone::Closed)),
            )
        })
}

impl GhostexGpuiApp {
    /// A session's links as gxserver last published them.
    pub(crate) fn session_links_of(
        &self,
        key: &ghostex_gx_core::SessionKey,
    ) -> Option<SessionLinks> {
        let session = self.gx_store.core.presentation().session(key)?;
        session
            .links
            .as_ref()
            .map(SessionLinks::from_presentation)
            .filter(|links| !links.is_empty())
    }

    /// The "Session links in the sidebar" setting.
    pub(crate) fn native_session_links_display(&self) -> SessionLinksDisplay {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        SessionLinksDisplay::from_setting(
            settings
                .object()
                .get(SessionLinksDisplay::SETTING_KEY)
                .and_then(serde_json::Value::as_str),
        )
    }

    /// What this row shows of its links (gx-core's rule), the session open in the window counting
    /// as selected.
    pub(crate) fn native_session_links_row(
        &self,
        session: &NativeSidebarSession,
    ) -> SessionLinksRow {
        let Some(links) = session.links.as_deref() else {
            return SessionLinksRow::default();
        };
        let (selected, _) = self.gx_store_sidebar_row_focus(
            &session.session_id,
            session.is_browser(),
            session.is_focused,
            session.is_visible,
        );
        links.row(self.native_session_links_display(), selected)
    }

    /// Whether the row draws a second line: its links line, or a Work mode card's Clean up / Keep
    /// offer for a merged PR (work_chips.rs), which takes the line while it waits.
    pub(crate) fn native_session_has_second_line(&self, session: &NativeSidebarSession) -> bool {
        let row = self.native_session_links_row(session);
        session.work.as_ref().is_some_and(|work| work.offer_cleanup) || !row.line.is_empty()
    }

    /// The badge a row's title shows with the Badge option: the pinned (or only, or most pressing)
    /// link's ID and "+N" for the others, coloured by the most pressing link, or a session's quiet
    /// suggestions badge (muted and dashed). Hidden while the row is hovered, like the row's other
    /// badges.
    ///
    /// CDXC:SessionLinks 2026-10-27 DECISION:
    /// User: "i see the links tag is shown on 2nd row, this is wrong it should always be on same row as header please (so all are singly row in sidebar)". The quiet suggestions badge sits on the title's line between the title and the time, like the linked badge, and the title truncates to make room; every row stays one line high. Supersedes the 2026-10-24 decision that put it on a line under the name ("show the option to link a thread with issues/prs etc under the name of the thread instead of on the same thread title row").
    pub(super) fn render_native_session_links_badge(
        &self,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
    ) -> Option<AnyElement> {
        let badge = self.native_session_links_row(session).badge?;
        let color = if badge.suggested {
            appearance.muted
        } else {
            session_link_badge_color(badge.tone)
        };
        let body = session_links_badge_body(
            &badge.label,
            badge.more,
            color,
            badge.suggested,
            appearance.scale,
        );
        Some(if badge.suggested {
            body.id(format!(
                "native-session-links-suggested-{}",
                session.session_id
            ))
            .role(gpui::Role::Image)
            .aria_label(format!("{} suggested links", badge.count))
            .into_any_element()
        } else {
            body.into_any_element()
        })
    }

    /// The row's links line (the Always, Selected session and Open work only options, and the
    /// pinned links under every option but Off): the chips gx-core picked that fit, pinned first,
    /// then "+N" for the rest (the badge under Badge), which opens the floating card.
    pub(super) fn render_native_session_link_line(
        &self,
        session: &NativeSidebarSession,
        indent: f32,
        tooltip_span: SidebarTooltipSpan,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if session.work.as_ref().is_some_and(|work| work.offer_cleanup) {
            return None;
        }
        let row = self.native_session_links_row(session);
        let scale = appearance.scale;
        if row.line.is_empty() {
            return None;
        }
        let links: &SessionLinks = session.links.as_deref()?;
        let items: Vec<SessionLink> = row
            .line
            .iter()
            .filter_map(|index| links.items.get(*index).cloned())
            .collect();
        // The card's width as it last painted, kept in step with the sidebar's width (the card's
        // inset from the sidebar's edges does not change while the sidebar is dragged wider),
        // less the line's indent under the title and its right padding.
        let card_width = match self
            .native_sidebar
            .session_card_bounds
            .get(&session.session_id)
        {
            Some(card) => {
                let inset =
                    f32::from(self.native_sidebar.bounds.size.width) - f32::from(card.size.width);
                self.sidebar_width - inset
            }
            None => {
                // The row has not painted yet: draw once with the sidebar's own width less the
                // usual insets, then again with the card's.
                let app = cx.entity();
                cx.defer(move |cx| app.update(cx, |_, cx| cx.notify()));
                self.sidebar_width - 40.0 * scale
            }
        };
        let available = card_width / scale - (indent + 15.0 + 6.0) - 6.0;
        let text_system = WindowTextSystem::new(cx.text_system().clone());
        let chip_widths: Vec<f32> = items
            .iter()
            .map(|link| link_chip_width(&text_system, link, 11.5))
            .collect();
        // Under Badge there is no line (the badge names a link and counts the rest).
        let never_shown = if row.badge.is_some() {
            0
        } else {
            links.items.len() - items.len()
        };
        let failing_beyond = |shown: usize| {
            links.items.iter().any(|link| {
                link.checks.as_deref() == Some("failing")
                    && !items.iter().take(shown).any(|item| item.key == link.key)
            })
        };
        let shown = ghostex_gx_core::link_chips_that_fit(
            &chip_widths,
            never_shown,
            available,
            10.0,
            |hidden| {
                let shown = (items.len() + never_shown).saturating_sub(hidden);
                overflow_pill_width(&text_system, hidden, failing_beyond(shown), 11.0)
            },
        );
        let hidden = items.len() + never_shown - shown;
        let show_tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let session_id = session.session_id.clone();
        let chips = items.iter().take(shown).enumerate().map(|(index, link)| {
            let click = open_session_link_command(link, &session_id);
            let tooltip = link.hover_card_text();
            let foreground = appearance.foreground;
            session_link_chip_body(link, scale, 11.5, appearance.muted)
                .id(format!("native-session-link-{index}-{session_id}"))
                .role(gpui::Role::Link)
                .aria_label(link.summary())
                .when_some(click, |chip, click| {
                    chip.cursor_pointer()
                        .hover(move |style| {
                            style
                                .text_color(foreground)
                                .text_decoration_1()
                                .text_decoration_color(foreground)
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(click.clone(), cx);
                        }))
                })
                .when(show_tooltips, |chip| {
                    chip.managed_discrete_tooltip_with_placement(
                        tooltip_span.placement(),
                        appearance.tooltip_delay,
                        move |window, cx| {
                            super::tooltips::sidebar_tooltip(
                                tooltip.clone(),
                                tooltip_span,
                                scale,
                                window,
                                cx,
                            )
                        },
                    )
                })
                .into_any_element()
        });
        let overflow = (hidden > 0).then(|| {
            let key = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&session_id);
            let failing = links.items.iter().any(|link| {
                link.checks.as_deref() == Some("failing")
                    && !items.iter().take(shown).any(|shown| shown.key == link.key)
            });
            session_links_overflow_pill(
                hidden,
                failing,
                scale,
                11.0,
                appearance.foreground,
                appearance.muted.opacity(0.45),
            )
            .id(format!("native-session-links-more-{session_id}"))
            .role(gpui::Role::Button)
            .aria_label(format!("{hidden} more links"))
            .when(key.is_some(), |pill| {
                let session_id = session_id.clone();
                pill.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.open_session_links_card(session_id.clone(), cx);
                    }))
            })
            .into_any_element()
        });
        Some(
            h_flex()
                .h(px(LINK_LINE_HEIGHT * scale))
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                // Under the title.
                .pl(px((indent + 15.0 + 6.0) * scale))
                .pr(px(6.0 * scale))
                .gap(px(10.0 * scale))
                .children(chips)
                .children(overflow)
                .into_any_element(),
        )
    }
}
