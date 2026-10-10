//! The Extensions page's sections: Views (Arrange views), Built-in (`BuiltInExtensionGroups` of
//! tabs/extensions/built-in-cards.tsx (deleted 2026-10-01)), Extensions Store (`ExtensionsBrowserList` / `StoreTab` of
//! extensions-modal/), and Your views (`CustomViewCard`, `AddCustomViewCard` of
//! tabs/extensions/custom-view-cards.tsx (deleted 2026-10-01), sortable by drag).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonVariant, ReorderOptions, SizedButtonSize, SizedButtonVariant, icon, reorder_handle,
    reorder_options, reorder_order, reorder_row, settings_button, settings_list_item,
    settings_section, settings_sized_button, settings_square_button, small_switch_control,
    tooltip_text,
};
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::post_store_message;
use super::ExtensionsTab;
use super::cards::{
    GridCardSpec, GridCell, add_view_card, card_grid, card_group, card_option_row, card_status,
    empty_state, extension_icon_tile, grid_card, grid_columns, page_column_width, tabler_tile,
};
use super::data::{
    CEF_DESCRIPTION, CEF_TITLE, InstalledExtension, OfficialExtension, SHARED_RUNTIME_LABEL,
    built_in_filter_subject, cef_filter_subject, custom_views, extension_view_scope_key,
    filter_store, is_official_enabled, is_version_newer, merged_view_order, move_id,
    normalize_custom_views, official_blocked_by, official_categories, official_extensions,
    official_icon, official_switch_value, official_view_scope_key, scope_projects_and_spaces,
    titlebar_view_order, view_order_items, view_scope, view_scope_description,
};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

const GHOSTEX_EXTENSIONS_REPO_URL: &str = "https://github.com/maddada/ghostex-extensions";

/// `SettingsSection plain` with `descriptionClassName='pb-2'`: the heading (and header actions)
/// over content laid straight on the page.
pub(crate) fn plain_section(
    p: &SettingsPalette,
    title: impl Into<SharedString>,
    description: AnyElement,
    actions: Option<AnyElement>,
    body: AnyElement,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(12.0))
        .child(
            h_flex()
                .w_full()
                .items_end()
                .justify_between()
                .gap(px(16.0))
                .px(px(2.0))
                .child(
                    v_flex()
                        .min_w_0()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(16.0))
                                .line_height(px(20.8))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child(title.into()),
                        )
                        .child(
                            div()
                                .pb(px(8.0))
                                .text_size(px(13.0))
                                .line_height(px(18.85))
                                .text_color(hsla(p.muted))
                                .child(description),
                        ),
                )
                .children(actions.map(|actions| {
                    h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(8.0))
                        .child(actions)
                })),
        )
        .child(body)
        .into_any_element()
}

/// The drag-to-reorder list of the custom view cards (fields/reorder.rs).
const CUSTOM_VIEW_LIST: &str = "custom-views";

/// The drag payload of a custom view card.
#[derive(Clone)]
pub(crate) struct CustomViewDrag {
    pub(crate) index: usize,
    pub(crate) title: SharedString,
    pub(crate) palette: SettingsPalette,
}

/// The chip that follows the pointer while a custom view card is dragged.
struct CustomViewDragView {
    title: SharedString,
    palette: SettingsPalette,
}

impl Render for CustomViewDragView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        div()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised_hover))
            .shadow_md()
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .text_color(hsla(p.foreground))
            .opacity(0.92)
            .child(self.title.clone())
    }
}

impl ExtensionsTab {
    /// Views: the View order row with Arrange views.
    pub(crate) fn render_views_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let button = settings_button(
            p,
            "extensions-arrange-views",
            "Arrange views",
            Some("modals/settings/arrows-sort.svg"),
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, window, cx| page.open_arrange(window, cx),
            cx,
        );
        let row = settings_list_item(p, None, None, "View order", None, Some(button));
        settings_section(
            p,
            "Views",
            Some("Choose the order of built-in, extension, and custom views.".into()),
            None,
            vec![row],
        )
        .map(IntoElement::into_any_element)
    }

    /// `scopeControls.describe(key)`: the card's scope label while the view is narrowed.
    pub(crate) fn describe_scope(&self, key: &str, cx: &gpui::App) -> Option<SharedString> {
        let store = self.store.read(cx);
        let scope = view_scope(&store.values().value("viewScopes"), key);
        if scope.is_default() {
            return None;
        }
        let (projects, spaces) = scope_projects_and_spaces(store.hud());
        Some(view_scope_description(&scope, &projects, &spaces).into())
    }

    /// `scopeControls.edit(key, title)`.
    pub(crate) fn edit_scope(&mut self, key: String, title: String, cx: &mut Context<Self>) {
        let draft = view_scope(&self.store.read(cx).values().value("viewScopes"), &key);
        self.scope_editor = Some(super::ScopeEditorState { draft, key, title });
        self.scope_targets.open = false;
        self.scope_keep.open = false;
        cx.notify();
    }

    fn pencil_button(
        &self,
        p: &SettingsPalette,
        id: SharedString,
        disabled: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_square_button(
            p,
            id,
            "modals/settings/pencil.svg",
            None,
            SizedButtonVariant::Ghost,
            24.0,
            None,
            disabled,
            None,
            on_click,
            cx,
        )
    }

    /// CDXC:Extensions 2026-09-24 DECISION:
    /// User: the built-in extensions are grouped by category, each category a labelled grid of cards. The Refresh action (component status for the Code editor and CEF) stays on the Built-in section header.
    /// Built-in: the category groups and the Shared runtime (CEF) card.
    pub(crate) fn render_built_in_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = grid_columns(page_column_width(window));
        let values = self.store.read(cx).values();
        let status = self
            .store
            .read(cx)
            .host_payload("pluginSettingsStatus")
            .cloned();
        let plugin = |id: &str| -> Option<Value> {
            status
                .as_ref()
                .and_then(|status| status["plugins"].as_array())
                .and_then(|plugins| plugins.iter().find(|plugin| plugin["id"] == id))
                .cloned()
        };
        let mut groups: Vec<AnyElement> = Vec::new();
        for category in official_categories() {
            let visible: Vec<&OfficialExtension> = official_extensions()
                .iter()
                .filter(|extension| {
                    extension.category == category.id
                        && self.show_official(&extension.id, cx)
                        && self.filter.matches(&built_in_filter_subject(extension))
                })
                .collect();
            if visible.is_empty() {
                continue;
            }
            let mut cells: Vec<GridCell> = Vec::new();
            for extension in &visible {
                let runtime = (extension.id == "code").then(|| plugin("code")).flatten();
                let scope_key = official_view_scope_key(&extension.id);
                let scoped = !extension.app_wide;
                let blocked = official_blocked_by(&values, extension);
                let editing = scoped
                    && self
                        .scope_editor
                        .as_ref()
                        .is_some_and(|editor| editor.key == scope_key);
                let meta = if extension.placement == "sidebar" {
                    "Sidebar".to_string()
                } else if extension.app_wide {
                    format!("{} · Every project", category.type_label)
                } else {
                    category.type_label.clone()
                };
                let enabled = is_official_enabled(&values, extension) && blocked.is_none();
                let settings_key = extension.settings_key.clone();
                let switch_extension = (*extension).clone();
                let title = extension.title.clone();
                let control = small_switch_control(
                    p,
                    SharedString::from(format!("official-{}-switch", extension.id)),
                    title.clone(),
                    enabled,
                    blocked.is_some(),
                    blocked.map(|blocked| format!("Turn on {} first", blocked.title).into()),
                    move |page: &mut Self, next, _window, cx| {
                        let store = page.store.clone();
                        let key = settings_key.clone();
                        let value = official_switch_value(&switch_extension, next);
                        store.update(cx, |store, cx| store.update_setting(&key, json!(value), cx));
                    },
                    cx,
                );
                let mut actions: Vec<AnyElement> = Vec::new();
                // `OFFICIAL_EXTENSION_RUNTIME_IDS`: the Code editor's component can be reinstalled.
                if extension.id == "code" {
                    let status = runtime.clone().unwrap_or(Value::Null);
                    actions.push(self.reinstall_button(p, "code", &title, &status, cx));
                }
                if scoped {
                    let key = scope_key.clone();
                    let scope_title = title.clone();
                    actions.push(self.pencil_button(
                        p,
                        SharedString::from(format!("official-{}-scope", extension.id)),
                        false,
                        move |page: &mut Self, _window, cx| {
                            page.edit_scope(key.clone(), scope_title.clone(), cx)
                        },
                        cx,
                    ));
                }
                let runtime_meta = runtime
                    .as_ref()
                    .map(runtime_meta)
                    .filter(|meta| !meta.is_empty());
                let blocked_reason =
                    blocked.map(|blocked| format!("Turn on {} first", blocked.title));
                let card = grid_card(
                    p,
                    GridCardSpec {
                        id: SharedString::from(format!("official-{}", extension.id)),
                        icon: tabler_tile(p, official_icon(&extension.id), false),
                        control: Some(control),
                        leading: None,
                        title: extension.title.clone().into(),
                        description: extension.description.clone().into(),
                        extra: None,
                        scope_summary: scoped
                            .then(|| self.describe_scope(&scope_key, cx))
                            .flatten(),
                        meta: runtime_meta.or(blocked_reason).unwrap_or(meta).into(),
                        actions,
                        editing,
                        enabled,
                        dragging: false,
                    },
                );
                let wide = if scoped {
                    self.render_scope_editor_for(p, &scope_key, window, cx)
                } else {
                    None
                };
                cells.push(GridCell { card, wide });
            }
            let first = groups.is_empty();
            groups.push(card_group(
                p,
                category.label.clone(),
                Some(visible.len()),
                first,
                card_grid(cells, None, columns),
            ));
        }
        if self.show_official("cef", cx) && self.filter.matches(&cef_filter_subject()) {
            let cef = plugin("cef").unwrap_or(Value::Null);
            let card = grid_card(
                p,
                GridCardSpec {
                    id: "official-cef".into(),
                    icon: tabler_tile(p, official_icon("cef"), false),
                    // The web runtime is optional (CDXC:CefRuntime 2026-09-28 in
                    // app/helpers/web_runtime.rs): the switch shows whether it is installed.
                    control: Some(small_switch_control(
                        p,
                        "official-cef-switch",
                        CEF_TITLE,
                        cef["status"].as_str() != Some("notInstalled"),
                        true,
                        None,
                        |_: &mut Self, _, _, _| {},
                        cx,
                    )),
                    leading: None,
                    title: CEF_TITLE.into(),
                    description: CEF_DESCRIPTION.into(),
                    extra: None,
                    scope_summary: None,
                    meta: runtime_meta(&cef).into(),
                    actions: std::iter::once(self.reinstall_button(p, "cef", CEF_TITLE, &cef, cx))
                        .chain(
                            (cef["canUninstall"].as_bool() == Some(true))
                                .then(|| self.uninstall_button(p, "cef", cx)),
                        )
                        .collect(),
                    editing: false,
                    enabled: true,
                    dragging: false,
                },
            );
            let first = groups.is_empty();
            groups.push(card_group(
                p,
                SHARED_RUNTIME_LABEL,
                Some(1),
                first,
                card_grid(vec![GridCell::card(card)], None, columns),
            ));
        }
        let loading = self.plugin_status_loading;
        let refresh = settings_sized_button(
            p,
            "extensions-status-refresh",
            "Refresh",
            Some("modals/settings/refresh.svg"),
            None,
            SizedButtonVariant::Ghost,
            SizedButtonSize::Default,
            loading,
            Some("Component status is being checked.".into()),
            |page: &mut Self, _window, cx| page.request_plugin_status(cx),
            cx,
        );
        plain_section(
            p,
            "Built-in",
            div()
                .child("Extensions Ghostex ships and maintains.")
                .into_any_element(),
            Some(refresh),
            v_flex().w_full().children(groups).into_any_element(),
        )
    }

    /// The Install / Reinstall button of a runtime component (Code editor, CEF).
    fn reinstall_button(
        &self,
        p: &SettingsPalette,
        plugin_id: &'static str,
        title: &str,
        runtime: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = runtime["status"].as_str();
        let busy =
            status.is_some_and(|status| !matches!(status, "installed" | "notInstalled" | "failed"));
        let available = runtime["canReinstall"].as_bool() == Some(true);
        let label = if status == Some("notInstalled") {
            "Install"
        } else {
            "Reinstall"
        };
        let reason: SharedString = if busy {
            format!("{title} is being installed.").into()
        } else {
            "This build does not provide a reinstallable remote component.".into()
        };
        settings_sized_button(
            p,
            SharedString::from(format!("official-{plugin_id}-reinstall")),
            label,
            Some("modals/settings/refresh.svg"),
            None,
            SizedButtonVariant::Ghost,
            SizedButtonSize::Xs,
            busy || !available,
            Some(reason),
            move |page: &mut Self, _window, cx| page.reinstall_plugin(plugin_id, cx),
            cx,
        )
    }

    /// The Uninstall button of an optional runtime component (the web runtime).
    fn uninstall_button(
        &self,
        p: &SettingsPalette,
        plugin_id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_sized_button(
            p,
            SharedString::from(format!("official-{plugin_id}-uninstall")),
            "Uninstall",
            None,
            None,
            SizedButtonVariant::Ghost,
            SizedButtonSize::Xs,
            false,
            None,
            move |page: &mut Self, _window, cx| page.uninstall_plugin(plugin_id, cx),
            cx,
        )
    }

    /// Extensions Store: the error banner, the loading and error states, and the Installed and
    /// Available groups.
    pub(crate) fn render_store_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted_link = p.foreground_alpha(0.9);
        let foreground = p.foreground;
        let link = h_flex()
            .id("extensions-repo-link")
            .items_center()
            .gap(px(2.0))
            .text_color(hsla(muted_link))
            .underline()
            .cursor_pointer()
            .hover(move |this| this.text_color(hsla(foreground)))
            .on_click(cx.listener(|page, _: &ClickEvent, _window, cx| {
                post_store_message(
                    &page.store,
                    json!({ "type": "openExternalUrl", "url": GHOSTEX_EXTENSIONS_REPO_URL }),
                    cx,
                );
            }))
            .child("ghostex-extensions")
            .child(super::super::super::fields::settings_icon(
                "modals/settings/external-link.svg",
                12.0,
                muted_link,
            ));
        let description = h_flex()
            .flex_wrap()
            .items_center()
            .child("Extensions published to the\u{a0}")
            .child(link)
            .child("\u{a0}repo. Reviewed and tested by @maddada.")
            .into_any_element();
        let body = self.render_store_list(p, window, cx);
        plain_section(p, "Extensions Store", description, None, body)
    }

    fn render_store_list(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut children: Vec<AnyElement> = Vec::new();
        if let Some(error) = self.browser.error.clone() {
            children.push(error_banner(p, error));
        }
        if self.browser.loading && self.browser.catalog.is_none() {
            children.push(empty_state(
                p,
                "modals/settings/puzzle.svg",
                "Loading extensions…",
                "Reading the installed registry and extension catalog.",
                None,
            ));
        } else if let (Some(error), None) =
            (self.browser.error.clone(), self.browser.catalog.as_ref())
        {
            let retry = settings_sized_button(
                p,
                "extensions-try-again",
                "Try again",
                Some("modals/settings/refresh.svg"),
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.load_browser(cx),
                cx,
            );
            children.push(empty_state(
                p,
                "modals/settings/puzzle.svg",
                "Extensions unavailable",
                error,
                Some(retry),
            ));
        } else {
            children.push(self.render_store_tab(p, window, cx));
        }
        v_flex()
            .w_full()
            .gap(px(12.0))
            .children(children)
            .into_any_element()
    }

    /// `StoreTab`: Installed, then Available.
    fn render_store_tab(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = grid_columns(page_column_width(window));
        let (installed_matches, store_matches) = filter_store(
            &self.filter,
            self.browser.catalog_entries(),
            &self.browser.installed,
        );
        if installed_matches.is_empty() && store_matches.is_empty() {
            return empty_state(
                p,
                super::super::super::fields::icon::SEARCH,
                "No matching extensions",
                "Try a different search or clear one of the filters.",
                None,
            );
        }
        let mut groups: Vec<AnyElement> = Vec::new();
        if !installed_matches.is_empty() {
            let mut cells = Vec::new();
            for index in &installed_matches {
                let extension = self.browser.installed[*index].clone();
                let card = self.installed_card(p, &extension, cx);
                let key = extension_view_scope_key(&extension.id());
                let wide = self.render_scope_editor_for(p, &key, window, cx);
                cells.push(GridCell { card, wide });
            }
            groups.push(card_group(
                p,
                "Installed",
                Some(installed_matches.len()),
                true,
                card_grid(cells, None, columns),
            ));
        }
        if !store_matches.is_empty() {
            let mut cells = Vec::new();
            for index in &store_matches {
                let entry = self.browser.catalog_entries()[*index].clone();
                cells.push(GridCell::card(self.store_card(p, &entry, cx)));
            }
            let first = groups.is_empty();
            groups.push(card_group(
                p,
                "Available",
                Some(store_matches.len()),
                first,
                card_grid(cells, None, columns),
            ));
        }
        v_flex().w_full().children(groups).into_any_element()
    }

    /// CDXC:Extensions 2026-09-18 DECISION:
    /// User: an installed extension gets the same Edit button and the same scope editor as a built-in view, so it can be limited to selected projects or spaces.
    /// `InstalledExtensionCard`.
    fn installed_card(
        &mut self,
        p: &SettingsPalette,
        extension: &InstalledExtension,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = extension.id();
        let title = extension.title();
        let pending = self.browser.pending.contains(&id);
        let key = extension_view_scope_key(&id);
        let icon_bytes = self.extension_icon(&id, &extension.icon(), cx);
        let has_source = self.has_transport(cx) && !extension.icon().is_empty();
        let mut actions = Vec::new();
        {
            let id = id.clone();
            actions.push(settings_sized_button(
                p,
                SharedString::from(format!("installed-{id}-details")),
                "Details",
                None,
                None,
                SizedButtonVariant::Ghost,
                SizedButtonSize::Xs,
                pending,
                None,
                move |page: &mut Self, _window, cx| {
                    page.browser.selected_installed = Some(id.clone());
                    cx.notify();
                },
                cx,
            ));
        }
        {
            let key = key.clone();
            let title = title.clone();
            actions.push(self.pencil_button(
                p,
                SharedString::from(format!("installed-{id}-scope")),
                pending,
                move |page: &mut Self, _window, cx| page.edit_scope(key.clone(), title.clone(), cx),
                cx,
            ));
        }
        {
            let id = id.clone();
            actions.push(settings_square_button(
                p,
                SharedString::from(format!("installed-{id}-remove")),
                icon::TRASH,
                None,
                SizedButtonVariant::Ghost,
                24.0,
                None,
                pending,
                None,
                move |page: &mut Self, _window, cx| page.uninstall_extension(id.clone(), cx),
                cx,
            ));
        }
        let control = {
            let id = id.clone();
            small_switch_control(
                p,
                SharedString::from(format!("installed-{id}-switch")),
                title.clone(),
                extension.enabled(),
                pending,
                None,
                move |page: &mut Self, next, _window, cx| {
                    page.set_extension_state(id.clone(), json!({ "enabled": next }), cx)
                },
                cx,
            )
        };
        let extra = extension
            .placements()
            .iter()
            .any(|placement| placement == "chat-bar")
            .then(|| {
                let id = id.clone();
                card_option_row(
                    p,
                    "Open automatically in sessions",
                    small_switch_control(
                        p,
                        SharedString::from(format!("installed-{id}-autoopen")),
                        "Open automatically in sessions",
                        extension.chat_bar_auto_open(),
                        pending,
                        None,
                        move |page: &mut Self, next, _window, cx| {
                            page.set_extension_state(
                                id.clone(),
                                json!({ "chatBarAutoOpen": next }),
                                cx,
                            )
                        },
                        cx,
                    ),
                )
            });
        let editing = self
            .scope_editor
            .as_ref()
            .is_some_and(|editor| editor.key == key);
        grid_card(
            p,
            GridCardSpec {
                id: SharedString::from(format!("installed-{id}")),
                icon: extension_icon_tile(p, icon_bytes, has_source, false),
                control: Some(control),
                leading: None,
                title: title.into(),
                description: extension.description().into(),
                extra,
                scope_summary: self.describe_scope(&key, cx),
                meta: [
                    extension.author(),
                    format!("v{}", extension.version()),
                    extension.placement_label(),
                ]
                .join(" · ")
                .into(),
                actions,
                editing,
                enabled: extension.enabled(),
                dragging: false,
            },
        )
    }

    /// `StoreExtensionCard`.
    fn store_card(
        &mut self,
        p: &SettingsPalette,
        entry: &super::data::CatalogEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = entry.name();
        let installed_version = self
            .browser
            .installed_extension(&name)
            .map(InstalledExtension::version);
        let installing = self.browser.pending.contains(&name);
        let icon_bytes = if installed_version.is_some() {
            self.extension_icon(&name, &entry.icon(), cx)
        } else {
            None
        };
        let has_source = installed_version.is_some() && self.has_transport(cx);
        let control = match &installed_version {
            Some(version) => card_status(
                p,
                if *version == entry.version() {
                    "Up to date".to_string()
                } else {
                    format!("Installed v{version}")
                },
            ),
            None => {
                let name = name.clone();
                settings_sized_button(
                    p,
                    SharedString::from(format!("store-{name}-install")),
                    if installing {
                        "Installing…"
                    } else {
                        "Install"
                    },
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Xs,
                    installing,
                    None,
                    move |page: &mut Self, window, cx| page.open_consent(name.clone(), window, cx),
                    cx,
                )
            }
        };
        let details = {
            let name = name.clone();
            settings_sized_button(
                p,
                SharedString::from(format!("store-{name}-details")),
                "Details",
                None,
                Some("modals/settings/arrow-up-right.svg"),
                SizedButtonVariant::Ghost,
                SizedButtonSize::Xs,
                false,
                None,
                move |page: &mut Self, _window, cx| page.select_store_entry(Some(name.clone()), cx),
                cx,
            )
        };
        let mut meta = vec![entry.author(), format!("v{}", entry.version())];
        meta.extend(entry.categories().into_iter().take(1));
        grid_card(
            p,
            GridCardSpec {
                id: SharedString::from(format!("store-{name}")),
                icon: extension_icon_tile(p, icon_bytes, has_source, false),
                control: Some(control),
                leading: None,
                title: entry.title().into(),
                description: entry.description().into(),
                extra: None,
                scope_summary: None,
                meta: meta.join(" · ").into(),
                actions: vec![details],
                editing: false,
                enabled: true,
                dragging: false,
            },
        )
    }

    /// Your views: the templates, the sortable custom view cards with their editor, and Add view.
    pub(crate) fn render_custom_views_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = grid_columns(page_column_width(window));
        let filter_active = self.filter.is_active();
        let visible: Vec<super::data::CustomView> = self
            .ordered_custom_views(cx)
            .into_iter()
            .filter(|view| self.filter.matches(&view.filter_subject()))
            .collect();
        // `.extension-grid-card-sortable[data-dragging='true'] { opacity: 0.6 }`; the page is the
        // auto-scroll container (fields/reorder.rs).
        let scroll = self.store.update(cx, |store, _| {
            store.scroll_handle(SettingsTabId::Extensions)
        });
        reorder_options(
            self,
            CUSTOM_VIEW_LIST,
            ReorderOptions {
                lifted_opacity: 0.6,
                activation_distance: 0.0,
                scroll: Some(scroll),
                fill: true,
            },
        );
        let order = reorder_order(self, CUSTOM_VIEW_LIST, visible.len(), cx);
        let editing_id = self
            .view_editor
            .as_ref()
            .and_then(|editor| editor.id.clone());
        let mut cells: Vec<GridCell> = Vec::new();
        for (slot, index) in order.iter().copied().enumerate() {
            let view = visible[index].clone();
            let card = self.custom_view_card(p, &view, index, slot, !filter_active, false, cx);
            let wide = if editing_id.as_deref() == Some(view.id().as_str()) {
                Some(self.render_view_editor(p, window, cx))
            } else {
                None
            };
            cells.push(GridCell { card, wide });
        }
        let trailing = if self
            .view_editor
            .as_ref()
            .is_some_and(|editor| editor.id.is_none())
        {
            Some(self.render_view_editor(p, window, cx))
        } else {
            None
        };
        if trailing.is_none() && !filter_active && !self.choosing_template {
            cells.push(GridCell::card(add_view_card(
                p,
                |page: &mut Self, window, cx| page.start_choosing_template(window, cx),
                cx,
            )));
        }
        let mut body: Vec<AnyElement> = Vec::new();
        if self.choosing_template {
            body.push(
                div()
                    .w_full()
                    .pb(px(12.0))
                    .child(self.render_templates(p, window, cx))
                    .into_any_element(),
            );
        }
        body.push(card_grid(cells, trailing, columns));
        plain_section(
            p,
            "Your views",
            div()
                .child("Websites, project dev servers, and HTML reports. Add from a template or configure your own.")
                .into_any_element(),
            None,
            v_flex().w_full().children(body).into_any_element(),
        )
    }

    /// `CustomViewCard`.
    #[allow(clippy::too_many_arguments)]
    fn custom_view_card(
        &mut self,
        p: &SettingsPalette,
        view: &super::data::CustomView,
        index: usize,
        slot: usize,
        sortable: bool,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = view.id();
        let name = view.name();
        let edit = {
            let view = view.clone();
            settings_square_button(
                p,
                SharedString::from(format!("custom-{id}-edit")),
                "modals/settings/pencil.svg",
                None,
                SizedButtonVariant::Ghost,
                24.0,
                None,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.view_editor = Some(super::view_editor::ViewEditorState::edit(&view));
                    cx.notify();
                },
                cx,
            )
        };
        let remove = {
            let id = id.clone();
            settings_square_button(
                p,
                SharedString::from(format!("custom-{id}-remove")),
                icon::TRASH,
                None,
                SizedButtonVariant::Ghost,
                24.0,
                None,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    let values = page.store.read(cx).values();
                    let views: Vec<_> = custom_views(&values)
                        .into_iter()
                        .filter(|candidate| candidate.id() != id)
                        .collect();
                    page.save_custom_views(&views, cx);
                },
                cx,
            )
        };
        let control = {
            let id = id.clone();
            small_switch_control(
                p,
                SharedString::from(format!("custom-{id}-switch")),
                name.clone(),
                view.enabled(),
                false,
                None,
                move |page: &mut Self, next, _window, cx| {
                    let values = page.store.read(cx).values();
                    let views: Vec<_> = custom_views(&values)
                        .into_iter()
                        .map(|mut candidate| {
                            if candidate.id() == id {
                                candidate.raw.insert("enabled".into(), json!(next));
                            }
                            candidate
                        })
                        .collect();
                    page.save_custom_views(&views, cx);
                },
                cx,
            )
        };
        let grip = sortable.then(|| {
            let muted = p.muted;
            let hover = if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                css_fade(gpui::rgb(0x262626), 0.5)
            };
            let grip = div()
                .id(SharedString::from(format!("custom-{id}-grip")))
                .ml(px(-4.0))
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .hover(move |this| this.bg(hsla(hover)))
                .tooltip(tooltip_text(format!("Reorder {name}")))
                .child(super::super::super::fields::settings_icon(
                    icon::GRIP_VERTICAL,
                    12.0,
                    muted,
                ))
                .into_any_element();
            reorder_handle(p, CUSTOM_VIEW_LIST, index, name.clone(), grip)
        });
        let card = grid_card(
            p,
            GridCardSpec {
                id: SharedString::from(format!("custom-{id}")),
                icon: tabler_tile(p, "modals/settings/world.svg", false),
                control: Some(control),
                leading: grip,
                title: view.name().into(),
                description: view.description().into(),
                extra: None,
                scope_summary: None,
                meta: view.kind_label().into(),
                actions: vec![edit, remove],
                editing: self
                    .view_editor
                    .as_ref()
                    .is_some_and(|editor| editor.id.as_deref() == Some(id.as_str())),
                enabled: view.enabled(),
                dragging,
            },
        );
        let row = div().w_full().flex().child(card).into_any_element();
        reorder_row(
            self,
            CUSTOM_VIEW_LIST,
            index,
            slot,
            row,
            |page: &mut Self, from, to, _window, cx| page.move_custom_view(from, to, cx),
            cx,
        )
    }

    /// `handleCustomViewDragEnd`: the custom views take their new order within the view order.
    fn move_custom_view(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let ordered = self.ordered_custom_views(cx);
        let ordered_ids: Vec<String> = ordered
            .iter()
            .map(|view| format!("extension:{}", view.id()))
            .collect();
        let reordered = move_id(&ordered_ids, from, to);
        let mut custom_index = 0;
        let ids: Vec<String> = view_order_items(&values, &self.browser.installed)
            .into_iter()
            .map(|item| {
                if ordered_ids.contains(&item.id) {
                    let id = reordered[custom_index].clone();
                    custom_index += 1;
                    id
                } else {
                    item.id
                }
            })
            .collect();
        let order = merged_view_order(ids, &titlebar_view_order(&values));
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting("titlebarViewOrder", order, cx)
        });
    }

    /// `updateCustomViews(customViews)`: saves the normalized list.
    pub(crate) fn save_custom_views(
        &mut self,
        views: &[super::data::CustomView],
        cx: &mut Context<Self>,
    ) {
        let normalized = normalize_custom_views(views);
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting("customViews", normalized, cx)
        });
    }
}

/// `runtimeMeta`: the component's status label, version and error.
fn runtime_meta(runtime: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(label) = runtime["statusLabel"]
        .as_str()
        .filter(|label| !label.is_empty())
    {
        parts.push(label.to_string());
    }
    if let Some(version) = runtime["version"]
        .as_str()
        .filter(|version| !version.is_empty())
    {
        parts.push(format!("v{version}"));
    }
    if let Some(error) = runtime["errorMessage"]
        .as_str()
        .filter(|error| !error.is_empty())
    {
        parts.push(error.to_string());
    }
    parts.join(" · ")
}

/// `ExtensionsErrorBanner`.
pub(crate) fn error_banner(p: &SettingsPalette, error: String) -> AnyElement {
    div()
        .w_full()
        .px(px(16.0))
        .py(px(10.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.modal.hairline))
        .bg(hsla(css_fade(p.destructive, 0.1)))
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.destructive))
        .child(error)
        .into_any_element()
}

/// Whether an update is available for an installed extension.
pub(crate) fn update_available(entry_version: &str, installed_version: &str) -> bool {
    is_version_newer(entry_version, installed_version)
}
