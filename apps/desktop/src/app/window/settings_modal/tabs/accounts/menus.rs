//! The page's ⋯ menus: a square ⋯ button and, while open, its popup of actions (the page menu in
//! the Accounts header, each account's menu in its pane).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{SizedButtonVariant, settings_square_button};
use super::super::super::palette::SettingsPalette;
use super::AccountsTab;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, anchored, deferred, div, px,
};
use gpui_component::v_flex;
use std::rc::Rc;

type Action = Rc<dyn Fn(&mut AccountsTab, &mut Context<AccountsTab>)>;

/// One row of a menu; a row without an action is a quiet note (a helper's version).
pub(crate) struct MenuRow {
    label: String,
    note: Option<String>,
    destructive: bool,
    action: Option<Action>,
    separator_before: bool,
}

impl MenuRow {
    pub(crate) fn action(
        label: impl Into<String>,
        action: impl Fn(&mut AccountsTab, &mut Context<AccountsTab>) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            note: None,
            destructive: false,
            action: Some(Rc::new(action)),
            separator_before: false,
        }
    }
    pub(crate) fn note(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            note: None,
            destructive: false,
            action: None,
            separator_before: false,
        }
    }
    pub(crate) fn detail(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
    pub(crate) fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
    /// Draws a line above the row, starting a new group.
    pub(crate) fn group(mut self) -> Self {
        self.separator_before = true;
        self
    }
    /// Greys the row out and takes its action away.
    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        if !enabled {
            self.action = None;
        }
        self
    }
}

impl AccountsTab {
    /// A ⋯ button that opens `rows` under it; `key` tells the page's menus apart.
    pub(crate) fn render_more_menu(
        &mut self,
        p: &SettingsPalette,
        key: &str,
        tooltip: &str,
        rows: Vec<MenuRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.open_menu.as_deref() == Some(key);
        let toggle_key = key.to_string();
        let button = settings_square_button(
            p,
            SharedString::from(format!("accounts-menu-{key}")),
            "modals/settings/dots.svg",
            Some(p.muted),
            SizedButtonVariant::Outline,
            26.0,
            Some(tooltip.to_string().into()),
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.open_menu = if page.open_menu.as_deref() == Some(toggle_key.as_str()) {
                    None
                } else {
                    Some(toggle_key.clone())
                };
                cx.notify();
            },
            cx,
        );
        let popup = open.then(|| {
            let items = rows.into_iter().enumerate().map(|(index, row)| {
                let enabled = row.action.is_some();
                let action = row.action.clone();
                let quiet = row.note.is_none() && !enabled && !row.destructive;
                div()
                    .w_full()
                    .when(row.separator_before && index > 0, |this| {
                        this.mt(px(4.0))
                            .pt(px(4.0))
                            .border_t_1()
                            .border_color(hsla(p.hairline))
                    })
                    .child(
                        v_flex()
                            .id(SharedString::from(format!("accounts-menu-{key}-{index}")))
                            .w_full()
                            .px(px(8.0))
                            .py(px(if quiet { 4.0 } else { 6.0 }))
                            .rounded(px(6.0))
                            .text_size(px(if quiet { 11.5 } else { 13.0 }))
                            .text_color(hsla(if row.destructive {
                                p.destructive
                            } else if quiet {
                                p.muted
                            } else {
                                p.foreground
                            }))
                            .when(!enabled && !quiet, |this| this.opacity(0.5))
                            .when_some(action, |this, action| {
                                this.cursor_pointer()
                                    .hover(move |this| this.bg(hsla(p.popup_hover)))
                                    .on_click(cx.listener(
                                        move |page, _: &gpui::ClickEvent, _window, cx| {
                                            page.open_menu = None;
                                            action(page, cx);
                                            cx.notify();
                                        },
                                    ))
                            })
                            .child(row.label)
                            .children(row.note.map(|note| {
                                div()
                                    .text_size(px(11.5))
                                    .text_color(hsla(p.muted))
                                    .child(note)
                            })),
                    )
            });
            div().absolute().top_full().right_0().child(
                deferred(
                    anchored()
                        .anchor(gpui::Anchor::TopRight)
                        .snap_to_window_with_margin(px(8.0))
                        .child(
                            v_flex()
                                .id(SharedString::from(format!("accounts-menu-{key}-popup")))
                                .occlude()
                                .mt(px(4.0))
                                .w(px(260.0))
                                .p(px(4.0))
                                .rounded(px(MODAL_RADIUS_CONTROL))
                                .border_1()
                                .border_color(hsla(p.popup_border))
                                .bg(hsla(p.popup_background))
                                .shadow_md()
                                .on_mouse_down_out(cx.listener(
                                    |page, _: &gpui::MouseDownEvent, _window, cx| {
                                        page.open_menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(items),
                        ),
                )
                .with_priority(1),
            )
        });
        div()
            .relative()
            .flex_shrink_0()
            .child(button)
            .children(popup)
            .into_any_element()
    }
}
