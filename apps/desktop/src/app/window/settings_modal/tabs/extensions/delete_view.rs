//! Deleting one of Your views: the confirmation sheet opened by a card's trash button and by the
//! view editor's Delete button, and the delete itself.
use super::super::super::super::native_modal_kit::*;
use super::super::super::palette::SettingsPalette;
use super::ExtensionsTab;
use super::data::custom_views;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    Window, px,
};
use gpui_component::v_flex;

/// The view a delete is waiting on: its id and the name the sheet shows.
#[derive(Clone, Debug)]
pub(crate) struct DeleteViewState {
    pub(crate) id: String,
    pub(crate) name: String,
}

impl ExtensionsTab {
    /// CDXC:Extensions 2026-10-11 DECISION:
    /// User: deleting one of Your views is easy to find (the editor has a Delete button, not only the card's hover-only trash) and asks first: "Delete <view name>? This removes the view from every project." Built-in views and templates stay undeletable.
    pub(crate) fn request_delete_view(
        &mut self,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_view = Some(DeleteViewState { id, name });
        self.delete_view_focus.focus(window, cx);
        cx.notify();
    }

    /// Removes the view from `customViews` (the record the editor saves), and closes its editor.
    /// The desktop drops the view's tab once the setting changes, as it does for a view turned off.
    fn delete_custom_view(&mut self, id: &str, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let views: Vec<_> = custom_views(&values)
            .into_iter()
            .filter(|candidate| candidate.id() != id)
            .collect();
        self.save_custom_views(&views, cx);
        if self
            .view_editor
            .as_ref()
            .is_some_and(|editor| editor.id.as_deref() == Some(id))
        {
            self.view_editor = None;
        }
        self.delete_view = None;
        cx.notify();
    }

    fn cancel_delete_view(&mut self, cx: &mut Context<Self>) {
        self.delete_view = None;
        cx.notify();
    }

    /// The confirmation sheet, drawn like the install consent.
    pub(crate) fn render_delete_view(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self.delete_view.clone()?;
        let m = p.modal;
        let cancel = modal_action_button(
            &m,
            "custom-view-delete-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |page: &mut Self, _window, cx| page.cancel_delete_view(cx),
            cx,
        );
        let confirm = {
            let id = pending.id.clone();
            modal_action_button(
                &m,
                "custom-view-delete-confirm",
                "Delete",
                None,
                ModalButtonTone::Danger,
                false,
                move |page: &mut Self, _window, cx| page.delete_custom_view(&id, cx),
                cx,
            )
        };
        let name = if pending.name.trim().is_empty() {
            "this view".to_string()
        } else {
            pending.name.trim().to_string()
        };
        let sheet = super::super::super::fields::settings_dialog_surface(p, v_flex())
            .id("custom-view-delete")
            .track_focus(&self.delete_view_focus)
            .occlude()
            .w(px(440.0))
            .max_w(window.viewport_size().width - px(32.0))
            .p(px(MODAL_WINDOW_PADDING))
            .gap(px(MODAL_SECTION_GAP))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(m.foreground))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(
                cx.listener(|page, event: &gpui::KeyDownEvent, _window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            cx.stop_propagation();
                            page.cancel_delete_view(cx);
                        }
                        "enter" => {
                            cx.stop_propagation();
                            if let Some(id) = page.delete_view.as_ref().map(|view| view.id.clone())
                            {
                                page.delete_custom_view(&id, cx);
                            }
                        }
                        _ => {}
                    }
                }),
            )
            .child(modal_header(
                &m,
                format!("Delete {name}?"),
                Some("This removes the view from every project."),
            ))
            .child(modal_footer(vec![cancel, confirm]))
            .into_any_element();
        Some(super::super::super::fields::settings_dialog_overlay(
            "custom-view-delete",
            sheet,
            |page: &mut Self, _window, cx| page.cancel_delete_view(cx),
            window,
            cx,
        ))
    }
}
