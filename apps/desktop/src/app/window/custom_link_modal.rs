//! Native GPUI "Add link", "Edit link", and "Change icon" dialog, from the rail's ⋯ menu and a
//! pinned link's right-click menu: an address (which takes `localhost:8317` as well as a full
//! address), an optional name, and a searchable icon grid (site icon first as default, followed by
//! the app's 59-icon set). Enter saves; Tab moves between fields; Escape closes.
//!
//! The links themselves: CDXC:Sidebar 2026-11-22 DECISION in packages/gx-core/src/custom_links.rs.
//! SEE-ALSO: apps/desktop/src/app/custom_link_modal_lifecycle.rs (open, save), apps/desktop/src/bin/native_modal_demo/custom_links.rs (the preview).
use super::native_modal_kit::*;
use super::space_editor_modal::{SPACE_EDITOR_ICONS, command_score};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, Image,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px, svg,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;
use std::sync::Arc;

pub(crate) const CUSTOM_LINK_MODAL_WIDTH: f32 = 460.0;
pub(crate) const CUSTOM_LINK_MODAL_INITIAL_HEIGHT: f32 = 480.0;
pub(crate) const CUSTOM_LINK_ICON_MODAL_INITIAL_HEIGHT: f32 = 380.0;

const ADDRESS_PLACEHOLDER: &str = "localhost:8317 or https://example.com";
const NAME_PLACEHOLDER: &str = "Optional";
const ICON_SEARCH_PLACEHOLDER: &str = "Search icons";
const BAD_ADDRESS: &str = "Enter a web address, such as localhost:8317 or example.com.";
const SITE_ICON_LABEL: &str = "Site icon (default)";

/// Whether the dialog edits the whole link (address, name, icon) or just picks its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CustomLinkModalMode {
    Full,
    IconOnly,
}

/// What the dialog tells its host once it is gone.
pub(crate) enum CustomLinkModalCommand {
    /// Add (no `id`) or save (`id`) the link, its address already made whole.
    Save {
        id: Option<String>,
        name: String,
        url: String,
        icon: Option<String>,
    },
    Cancel,
}

pub(crate) type CustomLinkModalHost = Rc<dyn Fn(CustomLinkModalCommand, &mut App)>;

/// Turns what was typed into a page address, `None` when it is not one.
pub(crate) type CustomLinkAddressReader = fn(&str) -> Option<String>;

pub(crate) struct CustomLinkModalConfig {
    /// The link being edited; `None` adds one.
    pub(crate) id: Option<String>,
    pub(crate) mode: CustomLinkModalMode,
    pub(crate) name: String,
    pub(crate) url: String,
    /// Optional app icon id (`None` = site favicon / globe default).
    pub(crate) icon: Option<String>,
    /// The link's cached favicon, shown on the default "Site icon" tile when available.
    pub(crate) site_favicon: Option<Arc<Image>>,
    pub(crate) read_address: CustomLinkAddressReader,
    pub(crate) palette: ModalPalette,
}

/// The outline `titlebar/` SVG path for an app icon id (matching
/// `gpui_sidebar_command_icon_asset_path` in helpers/sidebar/icons_and_indicators.rs).
fn custom_link_icon_asset(icon: &str) -> SharedString {
    if icon == "terminal" {
        return SharedString::new_static("titlebar/terminal-2.svg");
    }
    let mut asset = String::with_capacity(icon.len() + 1);
    for character in icon.chars() {
        if character.is_ascii_uppercase() {
            asset.push('-');
            asset.push(character.to_ascii_lowercase());
        } else {
            asset.push(character);
        }
    }
    SharedString::from(format!("titlebar/{asset}.svg"))
}

fn custom_link_icon_label(icon: Option<&str>) -> &'static str {
    let Some(icon) = icon else {
        return SITE_ICON_LABEL;
    };
    SPACE_EDITOR_ICONS
        .iter()
        .find(|entry| entry.id == icon)
        .map_or(SITE_ICON_LABEL, |entry| entry.label)
}

pub(crate) struct GpuiCustomLinkModalWindow {
    host: CustomLinkModalHost,
    palette: ModalPalette,
    id: Option<String>,
    mode: CustomLinkModalMode,
    read_address: CustomLinkAddressReader,
    address: Entity<InputState>,
    name: Entity<InputState>,
    icon_search: Entity<InputState>,
    address_text: String,
    link_label: String,
    icon: Option<String>,
    icon_query: String,
    site_favicon: Option<Arc<Image>>,
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
        let icon_search =
            cx.new(|cx| InputState::new(window, cx).placeholder(ICON_SEARCH_PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe_in(
                &address,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.address_text = input.read(cx).value().to_string();
                        this.error = None;
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &icon_search,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.icon_query = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
        ];
        match config.mode {
            CustomLinkModalMode::Full => {
                address.update(cx, |input, cx| input.focus(window, cx));
            }
            CustomLinkModalMode::IconOnly => {
                icon_search.update(cx, |input, cx| input.focus(window, cx));
            }
        }
        let link_label = if !config.name.trim().is_empty() {
            config.name.trim().to_string()
        } else {
            config.url.clone()
        };
        let initial_icon = config
            .icon
            .filter(|id| SPACE_EDITOR_ICONS.iter().any(|entry| entry.id == id));
        let mut this = Self {
            host,
            palette: config.palette,
            id: config.id,
            mode: config.mode,
            read_address: config.read_address,
            address,
            name,
            icon_search,
            address_text: config.url,
            link_label,
            icon: initial_icon,
            icon_query: String::new(),
            site_favicon: config.site_favicon,
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
        let icon = self.icon.clone();
        window.remove_window();
        (self.host)(
            CustomLinkModalCommand::Save {
                id: self.id.clone(),
                name,
                url,
                icon,
            },
            cx,
        );
    }

    /// Tab moves between the visible input fields.
    fn switch_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == CustomLinkModalMode::IconOnly {
            self.icon_search
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return;
        }
        let next = if self.address.read(cx).focus_handle(cx).is_focused(window) {
            self.name.clone()
        } else if self.name.read(cx).focus_handle(cx).is_focused(window) {
            self.icon_search.clone()
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

    fn filtered_icons(&self) -> (bool, Vec<usize>) {
        let query = self.icon_query.trim();
        if query.is_empty() {
            return (true, (0..SPACE_EDITOR_ICONS.len()).collect());
        }
        let show_site_default = command_score("Site icon default favicon globe", query) > 0.0;
        let mut scored: Vec<(usize, f64)> = SPACE_EDITOR_ICONS
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let score = command_score(entry.label, query).max(command_score(entry.id, query));
                (score > 0.0).then_some((index, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        (
            show_site_default,
            scored.into_iter().map(|(index, _)| index).collect(),
        )
    }

    fn render_icon_picker(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (show_default, icons) = self.filtered_icons();
        let tile_bg = if p.light {
            modal_rgba(0x000000, 0.05)
        } else {
            modal_rgba(0xffffff, 0.06)
        };
        let tile_selected_bg = if p.light {
            modal_rgba(0x000000, 0.09)
        } else {
            modal_rgba(0xffffff, 0.12)
        };
        let mut tiles: Vec<AnyElement> = Vec::with_capacity(icons.len() + 1);
        if show_default {
            let selected = self.icon.is_none();
            let artwork = match &self.site_favicon {
                Some(image) => gpui::img(image.clone())
                    .size(px(18.0))
                    .rounded(px(3.0))
                    .into_any_element(),
                None => svg()
                    .path("titlebar/world.svg")
                    .size(px(18.0))
                    .text_color(hsla(if selected { p.foreground } else { p.muted }))
                    .into_any_element(),
            };
            tiles.push(
                div()
                    .id("custom-link-icon-site-default")
                    .role(gpui::Role::Button)
                    .aria_label(SITE_ICON_LABEL)
                    .aria_selected(selected)
                    .size(px(34.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(hsla(if selected { p.primary } else { p.hairline }))
                    .when(selected, |tile| tile.bg(hsla(tile_selected_bg)))
                    .cursor_pointer()
                    .hover(move |tile| {
                        tile.bg(hsla(if selected { tile_selected_bg } else { tile_bg }))
                    })
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.icon = None;
                        cx.notify();
                    }))
                    .child(artwork)
                    .into_any_element(),
            );
        }
        for index in icons {
            let entry = &SPACE_EDITOR_ICONS[index];
            let selected = self.icon.as_deref() == Some(entry.id);
            let icon_id = entry.id;
            tiles.push(
                div()
                    .id(SharedString::from(format!("custom-link-icon-{icon_id}")))
                    .role(gpui::Role::Button)
                    .aria_label(entry.label)
                    .aria_selected(selected)
                    .size(px(34.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(hsla(if selected { p.primary } else { p.hairline }))
                    .when(selected, |tile| tile.bg(hsla(tile_selected_bg)))
                    .cursor_pointer()
                    .hover(move |tile| {
                        tile.bg(hsla(if selected { tile_selected_bg } else { tile_bg }))
                    })
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.icon = Some(icon_id.to_string());
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path(custom_link_icon_asset(entry.id))
                            .size(px(18.0))
                            .text_color(hsla(if selected { p.foreground } else { p.muted })),
                    )
                    .into_any_element(),
            );
        }
        let max_height = if self.mode == CustomLinkModalMode::IconOnly {
            220.0
        } else {
            148.0
        };
        let grid: AnyElement = if tiles.is_empty() {
            div()
                .w_full()
                .py(px(16.0))
                .text_center()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child("No matching icons")
                .into_any_element()
        } else {
            div()
                .id("custom-link-icon-grid")
                .w_full()
                .max_h(px(max_height))
                .overflow_y_scroll()
                .child(h_flex().w_full().flex_wrap().gap(px(6.0)).children(tiles))
                .into_any_element()
        };
        let selected_label = custom_link_icon_label(self.icon.as_deref());
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(modal_section_title(&p, "Icon"))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(p.muted))
                            .child(selected_label),
                    ),
            )
            .child(modal_text_input(&p, &self.icon_search, false, window, cx))
            .child(grid)
            .into_any_element()
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if self.mode == CustomLinkModalMode::IconOnly {
            return self.render_icon_picker(window, cx);
        }
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
            .child(self.render_icon_picker(window, cx))
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
        let (title, description): (&str, String) = match self.mode {
            CustomLinkModalMode::IconOnly => (
                "Change icon",
                format!(
                    "Choose an icon for {}, or keep the site icon.",
                    self.link_label
                ),
            ),
            CustomLinkModalMode::Full if self.id.is_some() => (
                "Edit link",
                "Change this link's address, name, or icon.".to_string(),
            ),
            CustomLinkModalMode::Full => (
                "Add link",
                "Open any page from the rail menu, such as a local dashboard.".to_string(),
            ),
        };
        let content = vec![
            modal_header(&p, title, Some(description.as_str())),
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
