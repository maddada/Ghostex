//! Native GPUI "Add link" and "Edit link" dialog, from the sidebar's ⋯ menus (Add link…, a link
//! row's Edit…): an address, which takes `localhost:8317` as well as a full address, and an
//! optional name. Enter saves; Tab moves between the two fields; Escape closes.
//!
//! The links themselves: CDXC:Sidebar 2026-11-22 DECISION in packages/gx-core/src/custom_links.rs.
//! SEE-ALSO: apps/desktop/src/app/custom_link_modal_lifecycle.rs (open, save), apps/desktop/src/bin/native_modal_demo/custom_links.rs (the preview).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _,
    Subscription, Window, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState};
use gpui_component::v_flex;
use std::rc::Rc;

pub(crate) const CUSTOM_LINK_MODAL_WIDTH: f32 = 440.0;
pub(crate) const CUSTOM_LINK_MODAL_INITIAL_HEIGHT: f32 = 300.0;

const ADDRESS_PLACEHOLDER: &str = "localhost:8317 or https://example.com";
const NAME_PLACEHOLDER: &str = "Optional";
const BAD_ADDRESS: &str = "Enter a web address, such as localhost:8317 or example.com.";

/// What the dialog tells its host once it is gone.
pub(crate) enum CustomLinkModalCommand {
    /// Add (no `id`) or save (`id`) the link, its address already made whole.
    Save {
        id: Option<String>,
        name: String,
        url: String,
    },
    Cancel,
}

pub(crate) type CustomLinkModalHost = Rc<dyn Fn(CustomLinkModalCommand, &mut App)>;

/// Turns what was typed into a page address, `None` when it is not one.
pub(crate) type CustomLinkAddressReader = fn(&str) -> Option<String>;

pub(crate) struct CustomLinkModalConfig {
    /// The link being edited; `None` adds one.
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) read_address: CustomLinkAddressReader,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiCustomLinkModalWindow {
    host: CustomLinkModalHost,
    palette: ModalPalette,
    id: Option<String>,
    read_address: CustomLinkAddressReader,
    address: Entity<InputState>,
    name: Entity<InputState>,
    address_text: String,
    error: Option<&'static str>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiCustomLinkModalWindow {
    pub(crate) fn new(
        config: CustomLinkModalConfig,
        host: CustomLinkModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let address = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(ADDRESS_PLACEHOLDER)
                .default_value(&config.url)
        });
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(NAME_PLACEHOLDER)
                .default_value(&config.name)
        });
        let subscriptions = vec![cx.subscribe_in(
            &address,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.address_text = input.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        )];
        address.update(cx, |input, cx| input.focus(window, cx));
        let mut this = Self {
            host,
            palette: config.palette,
            id: config.id,
            read_address: config.read_address,
            address,
            name,
            address_text: config.url,
            error: None,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this._subscriptions
            .extend(super::popup_dismissal::close_app_modal_on_click_away(
                window, cx,
            ));
        this
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(CustomLinkModalCommand::Cancel, cx);
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.address_text.trim().is_empty() {
            return;
        }
        let Some(url) = (self.read_address)(&self.address_text) else {
            self.error = Some(BAD_ADDRESS);
            cx.notify();
            return;
        };
        let name = self.name.read(cx).value().trim().to_string();
        window.remove_window();
        (self.host)(
            CustomLinkModalCommand::Save {
                id: self.id.clone(),
                name,
                url,
            },
            cx,
        );
    }

    /// Tab and Shift-Tab move between the address and the name.
    fn switch_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let address_focused = self.address.read(cx).focus_handle(cx).is_focused(window);
        let next = if address_focused {
            self.name.clone()
        } else {
            self.address.clone()
        };
        next.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn on_enter_action(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.save(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.save(window, cx),
            "tab" => self.switch_field(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut address = v_flex()
            .w_full()
            .gap(px(6.0))
            .child(modal_section_title(&p, "Address"))
            .child(modal_text_input(&p, &self.address, false, window, cx));
        if let Some(error) = self.error {
            address = address.child(modal_error(&p, error));
        }
        let name = v_flex()
            .w_full()
            .gap(px(6.0))
            .child(modal_section_title(&p, "Name"))
            .child(modal_text_input(&p, &self.name, false, window, cx));
        v_flex()
            .w_full()
            .gap(px(14.0))
            .child(address)
            .child(name)
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "custom-link-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let save = modal_action_button(
            &p,
            "custom-link-save",
            if self.id.is_some() { "Save" } else { "Add" },
            None,
            ModalButtonTone::Primary,
            self.address_text.trim().is_empty(),
            |this, window, cx| this.save(window, cx),
            cx,
        );
        modal_footer(vec![cancel, save])
    }
}

impl Render for GpuiCustomLinkModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let (title, description) = if self.id.is_some() {
            (
                "Edit link",
                "Change where this link goes or what the menu calls it.",
            )
        } else {
            (
                "Add link",
                "Open any page from the sidebar menu, such as a local dashboard.",
            )
        };
        let content = vec![
            modal_header(&p, title, Some(description)),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-custom-link-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiCustomLinkModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
