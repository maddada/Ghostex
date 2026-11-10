//! The Advanced fold at the bottom of a page: one clickable card row that opens the page's
//! advanced sections under it (CDXC:Settings 2026-10-29 DECISION in search.rs: the rail's Show
//! Advanced switch is gone; user: "a new section that's collapsed by default at the bottom ...
//! called Advanced"). The sections themselves are ordinary `settings_section`s the page appends
//! after this header while it is open. Settings > Projects draws its Global settings fold with the
//! same row (`settings_fold_row`), as the header of the panel that holds those sections.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::icon;
use super::row::{ROW_MIN_HEIGHT, ROW_PADDING_X, settings_icon};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;

pub(crate) fn advanced_fold_header<V: SettingsPage>(
    p: &SettingsPalette,
    open: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_fold_row(
        p,
        FoldRow {
            id: "settings-advanced-fold",
            label: "Advanced",
            open,
            frame: FoldFrame::Card,
            trailing: None,
            held_open: None,
        },
        on_toggle,
        cx,
    )
}

/// Where a fold row sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldFrame {
    /// Its own bordered card (Advanced at the bottom of a page).
    Card,
    /// The header row of a hairline panel that holds the fold's sections (Settings > Projects'
    /// Global settings): no border of its own, a hairline under it while open.
    PanelHeader,
}

pub(crate) struct FoldRow {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) open: bool,
    pub(crate) frame: FoldFrame,
    /// Muted content at the row's right end, such as a summary of what the fold holds.
    pub(crate) trailing: Option<AnyElement>,
    /// Held open by something else (a Settings search that matched its rows), and why: the row
    /// is then not a button, has no pointer and no hover, and says why it stays open.
    pub(crate) held_open: Option<&'static str>,
}

/// One clickable fold row: the chevron, the label and its optional trailing content.
pub(crate) fn settings_fold_row<V: SettingsPage>(
    p: &SettingsPalette,
    row: FoldRow,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let FoldRow {
        id,
        label,
        open,
        frame,
        trailing,
        held_open,
    } = row;
    let pressable = held_open.is_none();
    // GPUI has no disabled state to announce, so a held-open row is plain text, not a button.
    let base = h_flex()
        .id(id)
        .aria_label(label)
        .aria_expanded(open)
        .accessibility_id(id)
        .w_full()
        .gap(px(10.0))
        .items_center()
        .when_some(held_open, |row, reason| row.aria_description(reason))
        .when(pressable, |row| {
            row.role(gpui::Role::Button)
                .cursor_pointer()
                .on_press(cx, move |page, window, cx| on_toggle(page, window, cx))
        });
    let framed = match frame {
        FoldFrame::Card => {
            let hover = p.raised_hover;
            base.min_h(px(ROW_MIN_HEIGHT))
                .px(px(ROW_PADDING_X))
                .rounded(px(MODAL_RADIUS_SECTION))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.raised))
                .when(pressable, |row| row.hover(move |this| this.bg(hsla(hover))))
        }
        FoldFrame::PanelHeader => {
            let hover = p.foreground_alpha(0.02);
            // The panel's border is 1px, so its inside corners are 1px tighter.
            let radius = px(MODAL_RADIUS_SECTION - 1.0);
            let header = base
                .min_h(px(PANEL_HEADER_MIN_HEIGHT))
                .px(px(PANEL_HEADER_PADDING_X))
                .rounded_t(radius)
                .when(pressable, |row| row.hover(move |this| this.bg(hsla(hover))));
            if open {
                header.border_b_1().border_color(hsla(p.hairline))
            } else {
                header.rounded_b(radius)
            }
        }
    };
    framed
        .child(settings_icon(
            if open {
                icon::CHEVRON_DOWN
            } else {
                icon::CHEVRON_RIGHT
            },
            16.0,
            p.muted,
        ))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(14.0))
                .line_height(px(18.9))
                .text_color(hsla(p.foreground))
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .justify_end()
                .children(trailing),
        )
        .into_any_element()
}

/// A fold row that heads a panel (the mockups' 52px row with 20px sides).
const PANEL_HEADER_MIN_HEIGHT: f32 = 52.0;
const PANEL_HEADER_PADDING_X: f32 = 20.0;
