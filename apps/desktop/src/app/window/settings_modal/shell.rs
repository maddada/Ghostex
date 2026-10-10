//! The Settings window: the rail on the left, the search field over the content column, and the
//! active page's view (`packages/core-ui/settings-modal.tsx` (deleted 2026-10-01), layout from
//! `.ghostex-settings-shadcn .settings-modal-*` in packages/core-ui/styles.css).
use super::super::native_modal_kit::*;
use super::fields::{icon, settings_icon};
use super::model::{SettingsModalHost, SettingsOpenRequest, SettingsTabId};
use super::page::{CONTENT_GUTTER, CONTENT_MAX_WIDTH};
use super::palette::{SETTINGS_FONT, SettingsPalette};
use super::rail::{RailState, rail_pages, render_rail};
use super::store::{SettingsStore, new_settings_store};
use super::tabs::settings_tab_view;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, NavigationDirection,
    ParentElement as _, Render, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use serde_json::Value;
use std::collections::HashMap;

/// The Settings frame (CDXC:AppModal 2026-10-01 in app/model/app_modal_kind.rs):
/// `APP_MODAL_HOST_SETTINGS_WINDOW_WIDTH` x `APP_MODAL_HOST_SETTINGS_WINDOW_HEIGHT`.
pub(crate) const SETTINGS_MODAL_WIDTH: f32 = 900.0;
pub(crate) const SETTINGS_MODAL_HEIGHT: f32 = 663.0;
/// The hidden title row above the body (`.ghostex-modal-heading-bar` with an sr-only title).
const HEADING_HEIGHT: f32 = 20.0;
/// `.settings-modal-body-layout { padding: 0 1rem 1rem; gap: 1rem }`.
pub(crate) const BODY_PADDING: f32 = 16.0;

pub(crate) struct SettingsModalConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) request: SettingsOpenRequest,
    /// The `sidebarState` hydrate the React modal was opened with.
    pub(crate) sidebar_state: Value,
}

pub(crate) struct GpuiSettingsModalWindow {
    pub(crate) store: Entity<SettingsStore>,
    pub(crate) rail: RailState,
    pub(crate) rail_scroll: ScrollHandle,
    tabs: HashMap<SettingsTabId, AnyView>,
    search_input: Entity<InputState>,
    focus_handle: FocusHandle,
    last_show_advanced: bool,
    _subscriptions: Vec<Subscription>,
}

impl GpuiSettingsModalWindow {
    pub(crate) fn new(
        config: SettingsModalConfig,
        host: SettingsModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = SettingsPalette::resolve(&config.palette);
        super::fields::set_settings_tooltip_theme(palette.light);
        let store = new_settings_store(host, palette, config.request, config.sidebar_state, cx);
        let initial_query = store.read(cx).search_query().to_string();
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search settings")
                .default_value(initial_query.clone())
        });
        let search_subscription = cx.subscribe_in(
            &search_input,
            window,
            |shell: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    let store = shell.store.clone();
                    store.update(cx, |store, cx| store.set_search_query(query, cx));
                }
            },
        );
        let store_subscription =
            cx.observe_in(&store, window, |shell: &mut Self, store, window, cx| {
                let (show_advanced, searching, active) = {
                    let store = store.read(cx);
                    (
                        store.show_advanced(),
                        store.is_searching(),
                        store.active_tab(),
                    )
                };
                // Turning Show Advanced off while Debugging is open leaves the page it just hid.
                if shell.last_show_advanced
                    && !show_advanced
                    && active == SettingsTabId::Debugging
                    && !searching
                {
                    store.update(cx, |store, cx| {
                        store.set_active_tab(SettingsTabId::General, cx)
                    });
                }
                // OS Integration hides with Enable Experimental Features, and a page of a built-in
                // extension that was just turned off (Actions, Open In) hides with it.
                if (active == SettingsTabId::OsIntegration
                    && !store.read(cx).os_integration_visible())
                    || !store.read(cx).built_in_extension_allows_page(active)
                {
                    store.update(cx, |store, cx| {
                        store.set_active_tab(SettingsTabId::General, cx)
                    });
                }
                shell.last_show_advanced = show_advanced;
                // A page that searches for the user (Theme's Related settings links) shows its query.
                let query = store.read(cx).search_query().to_string();
                if shell.search_input.read(cx).value().as_ref() != query.as_str() {
                    shell
                        .search_input
                        .update(cx, |input, cx| input.set_value(query, window, cx));
                }
                shell.ensure_tab_view(store.read(cx).active_tab(), window, cx);
                cx.notify();
            });
        // Settings opens with the caret in the search field, the query (a deep link's) selected.
        search_input.update(cx, |input, cx| {
            input.focus(window, cx);
            let end = input.value().len();
            input.set_selected_range(0..end, cx);
        });
        let last_show_advanced = store.read(cx).show_advanced();
        let mut shell = Self {
            store,
            rail: RailState::new(),
            rail_scroll: ScrollHandle::new(),
            tabs: HashMap::new(),
            search_input,
            focus_handle: cx.focus_handle(),
            last_show_advanced,
            _subscriptions: vec![search_subscription, store_subscription],
        };
        shell
            ._subscriptions
            .extend(super::super::popup_dismissal::close_app_modal_on_click_away(window, cx));
        let active = shell.store.read(cx).active_tab();
        shell.ensure_tab_view(active, window, cx);
        // A deep link to a section (General's, or Integrations' Agent skills from the ⋯ menus'
        // Skills > Configure / Install more) lands on it.
        if let Some(section) = shell.store.read(cx).request().initial_section.clone() {
            let store = shell.store.clone();
            store.update(cx, |store, cx| store.scroll_to_section(active, &section, cx));
        }
        shell
    }

    fn ensure_tab_view(&mut self, tab: SettingsTabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.contains_key(&tab) {
            return;
        }
        let view = settings_tab_view(tab, &self.store, window, cx);
        self.tabs.insert(tab, view);
    }

    /// A rail page title: open that page.
    pub(crate) fn select_page(
        &mut self,
        tab: SettingsTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_tab_view(tab, window, cx);
        let store = self.store.clone();
        store.update(cx, |store, cx| store.set_active_tab(tab, cx));
        cx.notify();
    }

    /// The mouse Back/Forward buttons walk the pages this open visited (`SettingsStore::navigate_page_history`).
    fn navigate_page_history(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let store = self.store.clone();
        store.update(cx, |store, cx| store.navigate_page_history(back, cx));
        self.ensure_tab_view(self.store.read(cx).active_tab(), window, cx);
        cx.notify();
    }

    /// A rail section or subsection: open its page and scroll it into view.
    pub(crate) fn select_section(
        &mut self,
        tab: SettingsTabId,
        section: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_tab_view(tab, window, cx);
        let store = self.store.clone();
        store.update(cx, |store, cx| store.scroll_to_section(tab, section, cx));
        cx.notify();
    }

    /// The app pushes a new hydrate after a save anywhere.
    pub(crate) fn receive_sidebar_state(&mut self, message: Value, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| store.receive_sidebar_state(message, cx));
    }

    /// A colour the system colour panel picked for `key` (`ColorField`'s `onChange`).
    pub(crate) fn receive_system_color(&mut self, key: &str, hex: String, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            // Ignore an already-open system picker once this override is disabled.
            if cfg!(target_os = "macos")
                && key == "workspaceBackgroundColor"
                && store.bool("terminalShadersEnabled")
            {
                return;
            }
            if store.string(key) != hex {
                store.update_setting(key, Value::String(hex), cx);
            }
        });
    }

    /// Where the host has no system colour panel (Linux): the field's in-app Pick Color dialog.
    pub(crate) fn open_in_app_color_picker(&mut self, key: &str, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.request_mut().open_color_picker = Some(key.to_string());
            cx.notify();
        });
    }

    /// A transient payload (`agentHookStatus`, `ghostexCliStatus`, a toast request, ...).
    pub(crate) fn receive_host_payload(&mut self, payload: Value, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| store.receive_host_payload(payload, cx));
    }

    pub(crate) fn store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        window.remove_window();
        store.update(cx, |store, cx| store.close(cx));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let search_focused = self
            .search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        if keystroke.key == "escape" {
            // Escape on a search with text clears it and keeps the field focused.
            if search_focused && !self.search_input.read(cx).value().is_empty() {
                cx.stop_propagation();
                self.search_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                let store = self.store.clone();
                store.update(cx, |store, cx| store.set_search_query(String::new(), cx));
                return;
            }
            cx.stop_propagation();
            self.close(window, cx);
            return;
        }
        // A printable key typed outside every text field goes into the search field.
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return;
        }
        let Some(text) = keystroke
            .key_char
            .clone()
            .filter(|text| text.chars().count() == 1)
        else {
            return;
        };
        if window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"))
        {
            return;
        }
        cx.stop_propagation();
        let next = format!("{}{text}", self.search_input.read(cx).value());
        self.search_input.update(cx, |input, cx| {
            input.set_value(next.clone(), window, cx);
            input.focus(window, cx);
            let end = next.len();
            input.set_selected_range(end..end, cx);
        });
        let store = self.store.clone();
        store.update(cx, |store, cx| store.set_search_query(next, cx));
    }

    fn render_search(
        &self,
        p: &SettingsPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self
            .search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let has_query = !self.search_input.read(cx).value().is_empty();
        h_flex()
            .w_full()
            .justify_center()
            .pr(px(BODY_PADDING))
            .mb(px(12.0))
            .child(
                div()
                    .w_full()
                    .max_w(px(CONTENT_MAX_WIDTH + CONTENT_GUTTER * 2.0))
                    .px(px(CONTENT_GUTTER))
                    .child(
                        h_flex()
                            .relative()
                            .w_full()
                            .h(px(32.0))
                            .pl(px(12.0))
                            .pr(px(36.0))
                            .items_center()
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .border_1()
                            .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
                            .bg(hsla(p.raised))
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.search_input)
                                        .with_size(ComponentSize::Small)
                                        .appearance(false)
                                        .bordered(false)
                                        .focus_bordered(false)
                                        .w_full()
                                        .px(px(0.0))
                                        .py(px(0.0))
                                        .text_size(px(14.0))
                                        .text_color(hsla(p.foreground)),
                                ),
                            )
                            .child(if has_query {
                                div()
                                    .id("settings-search-clear")
                                    .role(gpui::Role::Button)
                                    .aria_label("Clear search")
                                    .absolute()
                                    .right(px(8.0))
                                    .top(px(3.0))
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .on_press(cx, move |shell, window, cx| {
                                        shell.search_input.update(cx, |input, cx| {
                                            input.set_value("", window, cx);
                                            input.focus(window, cx);
                                        });
                                        let store = shell.store.clone();
                                        store.update(cx, |store, cx| {
                                            store.set_search_query(String::new(), cx)
                                        });
                                    })
                                    .child(settings_icon(icon::X, 16.0, p.muted))
                                    .into_any_element()
                            } else {
                                div()
                                    .absolute()
                                    .right(px(12.0))
                                    .top(px(7.0))
                                    .child(settings_icon(icon::SEARCH, 16.0, p.muted))
                                    .into_any_element()
                            }),
                    ),
            )
            .into_any_element()
    }
}

impl Focusable for GpuiSettingsModalWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GpuiSettingsModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, pages, active_tab, show_advanced) = {
            let store = self.store.read(cx);
            (
                store.palette(),
                rail_pages(store),
                store.active_tab(),
                store.show_advanced(),
            )
        };
        self.ensure_tab_view(active_tab, window, cx);
        let rail = render_rail(self, &p, pages, active_tab, show_advanced, cx);
        let search = self.render_search(&p, window, cx);
        let page = self.tabs.get(&active_tab).cloned();
        div()
            .id("settings-modal")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .font_family(SETTINGS_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|shell, _, window, cx| shell.navigate_page_history(true, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|shell, _, window, cx| shell.navigate_page_history(false, window, cx)),
            )
            .child(
                v_flex().size_full().pt(px(HEADING_HEIGHT)).child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .items_stretch()
                        // No right padding: the page's scroll area reaches the window's right
                        // edge so its scrollbar sits flush there (CDXC:AppModal 2026-10-08 on
                        // `modal_edge_scrollbar`); the search row and the page column pad it back.
                        .pl(px(BODY_PADDING))
                        .pb(px(BODY_PADDING))
                        .gap(px(BODY_PADDING))
                        .child(rail)
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .child(search)
                                .child(div().flex_1().min_h_0().w_full().children(page)),
                        ),
                ),
            )
    }
}

impl ModalCornerClose for GpuiSettingsModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }

    /// A page opened during this visit that holds a form, edit or confirmation not finished yet
    /// keeps Settings open on a click away; settings that apply at once never do.
    fn keeps_open_on_click_away(&self, _window: &Window, cx: &App) -> bool {
        self.tabs
            .values()
            .any(|view| super::tabs::settings_tab_holds_unsaved_input(view, cx))
    }
}
