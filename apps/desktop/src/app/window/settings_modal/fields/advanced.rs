//! The Power User Flags fold at the bottom of a page: one clickable card row that opens the page's
//! Power User Flags under it (CDXC:Settings 2026-11-27 DECISION in search.rs; it was the Advanced
//! fold of CDXC:Settings 2026-10-29). Cloud Boxes keeps an Advanced fold of its own for its older
//! providers (`advanced_fold_header`). The sections themselves are ordinary `settings_section`s the page appends
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

/// The Power User Flags fold row (`ghostex_settings_catalog::power_user_flags::SECTION_TITLE`).
pub(crate) fn power_user_flags_fold_header<V: SettingsPage>(
    p: &SettingsPalette,
    open: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_fold_row(
        p,
        FoldRow {
            id: "settings-power-user-flags-fold",
            label: ghostex_settings_catalog::power_user_flags::SECTION_TITLE,
            open,
            frame: FoldFrame::Card,
            trailing: None,
            held_open: None,
        },
        on_toggle,
        cx,
    )
}

/// Cloud Boxes' Advanced fold (its older agentbox providers, CDXC:CloudBoxes 2026-11-02).
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

/// Appends page `tab`'s Power User Flags fold to `blocks`: its row, and `flag_blocks` under it while
/// it is open. Nothing is added while a search runs (the matches show in place), while the page's
/// flags are hidden (`SettingsStore::power_user_flags_shown`, CDXC:Settings 2026-11-27 in
/// search.rs), or when this pass drew no flag. A deep link to a section only the fold holds opens it.
pub(crate) fn push_power_user_flags<V: SettingsPage>(
    store: &gpui::Entity<super::super::store::SettingsStore>,
    p: &SettingsPalette,
    tab: super::super::model::SettingsTabId,
    blocks: &mut Vec<super::super::page::PageBlock>,
    flag_blocks: Vec<super::super::page::PageBlock>,
    cx: &mut Context<V>,
) {
    let shown = {
        let store = store.read(cx);
        !store.is_searching() && store.power_user_flags_shown(tab)
    };
    if !shown || flag_blocks.is_empty() {
        return;
    }
    let pending = store.update(cx, |store, _| {
        store.tracker(tab).borrow().pending_scroll.clone()
    });
    if let Some(anchor) = pending
        && !blocks
            .iter()
            .any(|block| block.anchor.as_deref() == Some(&anchor))
        && flag_blocks
            .iter()
            .any(|block| block.anchor.as_deref() == Some(&anchor))
    {
        store.update(cx, |store, _| store.open_power_user_fold(tab));
    }
    let open = store.read(cx).power_user_fold_open(tab);
    blocks.push(super::super::page::PageBlock::section(
        "powerUserFlagsFold",
        power_user_flags_fold_header(
            p,
            open,
            move |page: &mut V, _window, cx| {
                let store = page.settings_store().clone();
                store.update(cx, |store, cx| store.toggle_power_user_fold(tab, cx));
            },
            cx,
        ),
    ));
    if open {
        blocks.extend(flag_blocks);
    }
}
