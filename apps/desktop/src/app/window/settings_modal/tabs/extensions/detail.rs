//! An extension's page, drawn in place of the whole Extensions page
//! (extensions-modal/extension-detail.tsx (deleted 2026-10-01) and preferences-form.tsx (deleted 2026-10-01)): the header with Back, then
//! for an installed extension its placement, preferences, permissions, status (enabled, pinned,
//! author, version, update) and Uninstall; for a Store entry its screenshots, README, changelog,
//! details, permissions and Install. The install consent (install-consent.tsx (deleted 2026-10-01)) sits over either.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    FieldStates, SizedButtonSize, SizedButtonVariant, settings_icon, settings_select,
    settings_sized_button, settings_square_button, settings_text_input, small_switch_control,
    stock_segmented, switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::ExtensionsTab;
use super::browser::{ScreenshotSlot, screenshot_urls};
use super::cards::{detail_group, extension_icon_tile, section_label};
use super::data::{
    CatalogEntry, InstalledExtension, Preference, capitalize, is_version_newer,
    missing_required_preferences, permission_description, permission_label, preference_string,
    preference_values,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, img, px,
};
use gpui_component::text::TextView;
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};

/// `lg:grid-cols-[minmax(0,1fr)_300px]` applies from the `lg` breakpoint (a 1024px window).
const LG_BREAKPOINT: f32 = 1024.0;

impl ExtensionsTab {
    pub(crate) fn render_detail(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut children: Vec<AnyElement> = Vec::new();
        if let Some(error) = self.browser.error.clone() {
            children.push(super::sections::error_banner(p, error));
        }
        if let Some(extension) = self.browser.selected_installed().cloned() {
            children.push(self.render_installed_detail(p, &extension, window, cx));
        } else if let Some(entry) = self.browser.selected_store().cloned() {
            children.push(self.render_store_detail(p, &entry, window, cx));
        }
        v_flex()
            .w_full()
            .gap(px(12.0))
            .children(children)
            .into_any_element()
    }

    /// `DetailHeader`.
    fn detail_header(
        &mut self,
        p: &SettingsPalette,
        icon: AnyElement,
        title: String,
        description: String,
        on_back: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let back = settings_square_button(
            p,
            "extension-detail-back",
            "modals/settings/arrow-left.svg",
            None,
            SizedButtonVariant::Ghost,
            28.0,
            None,
            false,
            None,
            on_back,
            cx,
        );
        h_flex()
            .w_full()
            .items_start()
            .gap(px(12.0))
            .child(back)
            .child(icon)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(16.0))
                            .line_height(px(24.0))
                            .text_color(hsla(p.foreground))
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .max_w(px(768.0))
                            .text_size(px(13.0))
                            .line_height(px(21.13))
                            .text_color(hsla(p.muted))
                            .child(description),
                    ),
            )
            .into_any_element()
    }

    /// The main column and the 300px aside, side by side from the `lg` breakpoint.
    fn detail_columns(
        &self,
        main: Vec<AnyElement>,
        aside: Vec<AnyElement>,
        aside_gap: f32,
        window: &Window,
    ) -> AnyElement {
        let wide = f32::from(window.viewport_size().width) >= LG_BREAKPOINT;
        let main = v_flex().flex_1().min_w_0().gap(px(24.0)).children(main);
        let aside = v_flex()
            .when(wide, |this| this.w(px(300.0)).flex_shrink_0())
            .when(!wide, |this| this.w_full())
            .gap(px(aside_gap))
            .children(aside);
        if wide {
            h_flex()
                .w_full()
                .items_start()
                .gap(px(24.0))
                .child(main)
                .child(aside)
                .into_any_element()
        } else {
            v_flex()
                .w_full()
                .gap(px(24.0))
                .child(main)
                .child(aside)
                .into_any_element()
        }
    }

    /// `DetailRow`: a label and a value.
    fn detail_row(p: &SettingsPalette, label: &str, value: String) -> AnyElement {
        h_flex()
            .w_full()
            .min_h(px(44.0))
            .px(px(16.0))
            .py(px(10.0))
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground_alpha(0.9)))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .child(value),
            )
            .into_any_element()
    }

    /// A row with a title, a caption under it and a control on the right.
    fn detail_control_row(
        p: &SettingsPalette,
        leading: Option<AnyElement>,
        title: &str,
        caption: &str,
        control: AnyElement,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .min_h(px(56.0))
            .px(px(16.0))
            .py(px(12.0))
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                h_flex()
                    .min_w_0()
                    .items_start()
                    .gap(px(10.0))
                    .children(leading)
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(hsla(p.foreground_alpha(0.9)))
                                    .child(title.to_string()),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.57))
                                    .text_color(hsla(p.muted))
                                    .child(caption.to_string()),
                            ),
                    ),
            )
            .child(div().flex_shrink_0().child(control))
            .into_any_element()
    }

    /// `PermissionsList`.
    fn permissions_section(p: &SettingsPalette, permissions: &[String]) -> AnyElement {
        let rows: Vec<AnyElement> = if permissions.is_empty() {
            vec![
                h_flex()
                    .w_full()
                    .min_h(px(44.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .items_center()
                    .text_size(px(14.0))
                    .text_color(hsla(p.muted))
                    .child("No additional permissions requested.")
                    .into_any_element(),
            ]
        } else {
            permissions
                .iter()
                .map(|permission| {
                    h_flex()
                        .w_full()
                        .min_h(px(40.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(px(6.0))
                                .rounded_full()
                                .bg(hsla(p.foreground_alpha(0.2))),
                        )
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .text_color(hsla(p.muted))
                                .child(permission_label(permission)),
                        )
                        .into_any_element()
                })
                .collect()
        };
        v_flex()
            .w_full()
            .gap(px(10.0))
            .child(section_label(p, "Permissions"))
            .child(detail_group(p, rows))
            .into_any_element()
    }

    /// `InstalledExtensionDetail`.
    fn render_installed_detail(
        &mut self,
        p: &SettingsPalette,
        extension: &InstalledExtension,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = extension.id();
        let pending = self.browser.pending.contains(&id);
        let icon_bytes = self.extension_icon(&id, &extension.icon(), cx);
        let has_source = self.has_transport(cx) && !extension.icon().is_empty();
        let header = self.detail_header(
            p,
            extension_icon_tile(p, icon_bytes, has_source, true),
            extension.title(),
            extension.description(),
            |page: &mut Self, _window, cx| {
                page.browser.selected_installed = None;
                page.preferences_draft = None;
                cx.notify();
            },
            cx,
        );
        let definitions = extension.preferences();
        // `useState(() => preferenceValues(definitions, stored))`, reset when the stored values change.
        let stored = preference_values(&definitions, &extension.stored_preferences());
        let draft = match &self.preferences_draft {
            Some((draft_id, draft)) if *draft_id == id => draft.clone(),
            _ => {
                self.preferences_draft = Some((id.clone(), stored.clone()));
                stored.clone()
            }
        };
        let mut main: Vec<AnyElement> = Vec::new();
        if extension.is_terminal_pane() {
            let control = {
                let id = id.clone();
                stock_segmented(
                    p,
                    "extension-terminal-placement",
                    &[
                        ("splitRight".to_string(), "Split right".to_string()),
                        ("tab".to_string(), "New tab".to_string()),
                    ],
                    Some(extension.terminal_placement().as_str()),
                    false,
                    false,
                    move |page: &mut Self, value, _window, cx| {
                        page.set_extension_state(
                            id.clone(),
                            json!({ "terminalPlacement": value }),
                            cx,
                        )
                    },
                    cx,
                )
            };
            main.push(
                v_flex()
                    .w_full()
                    .gap(px(10.0))
                    .child(section_label(p, "Terminal placement"))
                    .child(detail_group(
                        p,
                        vec![Self::detail_control_row(
                            p,
                            None,
                            "Open location",
                            "Choose how its terminal pane opens.",
                            control,
                        )],
                    ))
                    .into_any_element(),
            );
        } else {
            let options: Vec<(String, String)> = extension
                .placements()
                .into_iter()
                .map(|placement| {
                    let label = if placement == "chat-bar" {
                        "Chat bar".to_string()
                    } else {
                        capitalize(&placement)
                    };
                    (placement, label)
                })
                .collect();
            let control = {
                let id = id.clone();
                stock_segmented(
                    p,
                    "extension-placement",
                    &options,
                    Some(extension.placement().as_str()),
                    false,
                    false,
                    move |page: &mut Self, value, _window, cx| {
                        page.set_extension_state(id.clone(), json!({ "placement": value }), cx)
                    },
                    cx,
                )
            };
            main.push(
                v_flex()
                    .w_full()
                    .gap(px(10.0))
                    .child(section_label(p, "Placement"))
                    .child(detail_group(
                        p,
                        vec![Self::detail_control_row(
                            p,
                            None,
                            "Open location",
                            "Choose where this extension opens.",
                            control,
                        )],
                    ))
                    .into_any_element(),
            );
        }
        if !definitions.is_empty() {
            let missing = missing_required_preferences(&definitions, &draft);
            let form = self.render_preferences_form(p, &id, &definitions, &draft, window, cx);
            let save = {
                let id = id.clone();
                settings_sized_button(
                    p,
                    "extension-save-preferences",
                    "Save preferences",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    pending || !missing.is_empty(),
                    None,
                    move |page: &mut Self, _window, cx| {
                        let values = page
                            .preferences_draft
                            .as_ref()
                            .filter(|(draft_id, _)| *draft_id == id)
                            .map(|(_, values)| values.clone())
                            .unwrap_or_default();
                        page.set_extension_state(
                            id.clone(),
                            json!({ "preferences": Value::Object(values) }),
                            cx,
                        )
                    },
                    cx,
                )
            };
            main.push(
                v_flex()
                    .w_full()
                    .gap(px(10.0))
                    .child(section_label(p, "Preferences"))
                    .child(
                        v_flex()
                            .w_full()
                            .p(px(16.0))
                            .rounded(px(MODAL_RADIUS_SECTION))
                            .border_1()
                            .border_color(hsla(p.modal.hairline))
                            .bg(hsla(p.modal.panel))
                            .child(
                                div()
                                    .mb(px(16.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.57))
                                    .text_color(hsla(p.muted))
                                    .child(
                                        "Required preferences must be completed before first use.",
                                    ),
                            )
                            .child(form)
                            .child(div().mt(px(16.0)).flex().child(save)),
                    )
                    .into_any_element(),
            );
        }
        main.push(Self::permissions_section(
            p,
            &extension.granted_permissions(),
        ));
        let catalog_version = self.browser.catalog_entry(&id).map(CatalogEntry::version);
        let update_available = catalog_version
            .as_deref()
            .is_some_and(|version| is_version_newer(version, &extension.version()));
        let enabled_switch = {
            let id = id.clone();
            small_switch_control(
                p,
                "extension-detail-enabled",
                "Enabled",
                extension.enabled(),
                pending,
                None,
                move |page: &mut Self, next, _window, cx| {
                    page.set_extension_state(id.clone(), json!({ "enabled": next }), cx)
                },
                cx,
            )
        };
        let pinned_switch = {
            let id = id.clone();
            small_switch_control(
                p,
                "extension-detail-pinned",
                "Pinned",
                extension.pinned(),
                pending,
                None,
                move |page: &mut Self, next, _window, cx| {
                    page.set_extension_state(id.clone(), json!({ "pinned": next }), cx)
                },
                cx,
            )
        };
        let dot = div()
            .mt(px(6.0))
            .flex_shrink_0()
            .size(px(6.0))
            .rounded_full()
            .bg(hsla(if extension.enabled() {
                modal_rgba(0x34d399, 0.8)
            } else {
                p.foreground_alpha(0.2)
            }))
            .into_any_element();
        let pin = div()
            .mt(px(2.0))
            .flex_shrink_0()
            .child(settings_icon("modals/settings/pin.svg", 16.0, p.muted))
            .into_any_element();
        let mut status_rows = vec![
            Self::detail_control_row(
                p,
                Some(dot),
                "Enabled",
                "Available from its configured placement.",
                enabled_switch,
            ),
            Self::detail_control_row(
                p,
                Some(pin),
                "Pinned",
                "Show its icon in the titlebar.",
                pinned_switch,
            ),
            Self::detail_row(p, "Author", extension.author()),
            Self::detail_row(p, "Installed version", format!("v{}", extension.version())),
        ];
        if update_available {
            let name = id.clone();
            status_rows.push(
                div()
                    .w_full()
                    .p(px(12.0))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .child(settings_sized_button(
                                p,
                                "extension-detail-update",
                                format!(
                                    "Update to v{}",
                                    catalog_version.clone().unwrap_or_default()
                                ),
                                None,
                                None,
                                SizedButtonVariant::Outline,
                                SizedButtonSize::Sm,
                                pending,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.install_extension(name.clone(), cx)
                                },
                                cx,
                            )),
                    )
                    .into_any_element(),
            );
        }
        let uninstall = {
            let id = id.clone();
            settings_sized_button(
                p,
                "extension-detail-uninstall",
                "Uninstall",
                Some(super::super::super::fields::icon::TRASH),
                None,
                SizedButtonVariant::Destructive,
                SizedButtonSize::Sm,
                pending,
                None,
                move |page: &mut Self, _window, cx| page.uninstall_extension(id.clone(), cx),
                cx,
            )
        };
        let aside = vec![
            section_label(p, "Status"),
            detail_group(p, status_rows),
            div().mt(px(8.0)).flex().child(uninstall).into_any_element(),
        ];
        let columns = self.detail_columns(main, aside, 10.0, window);
        v_flex()
            .w_full()
            .gap(px(24.0))
            .child(header)
            .child(columns)
            .into_any_element()
    }

    /// `PreferencesForm`.
    fn render_preferences_form(
        &mut self,
        p: &SettingsPalette,
        extension_id: &str,
        definitions: &[Preference],
        values: &Map<String, Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let missing = missing_required_preferences(definitions, values);
        let mut fields: Vec<AnyElement> = Vec::new();
        for definition in definitions {
            let invalid = missing.contains(&definition.name);
            let name = definition.name.clone();
            let value = values
                .get(&definition.name)
                .cloned()
                .or_else(|| definition.default.clone())
                .unwrap_or_else(|| {
                    if definition.kind == "checkbox" {
                        json!(false)
                    } else {
                        json!("")
                    }
                });
            let control = match definition.kind.as_str() {
                "checkbox" => {
                    let name = name.clone();
                    switch_control(
                        p,
                        SharedString::from(format!("preference-{extension_id}-{name}")),
                        definition.title.clone(),
                        value.as_bool() == Some(true),
                        false,
                        None,
                        move |page: &mut Self, next, _window, cx| {
                            page.set_preference(&name, json!(next), cx)
                        },
                        cx,
                    )
                }
                "dropdown" => {
                    let options: Vec<SettingOption> = definition
                        .options
                        .iter()
                        .map(|(title, value)| SettingOption {
                            label: title.clone(),
                            value: value.clone(),
                        })
                        .collect();
                    let name = name.clone();
                    settings_select(
                        self,
                        p,
                        SharedString::from(format!(
                            "preference-{extension_id}-{}",
                            definition.name
                        )),
                        &options,
                        &preference_string(Some(&value)),
                        None,
                        false,
                        None,
                        move |page: &mut Self, next, _window, cx| {
                            page.set_preference(&name, json!(next), cx)
                        },
                        window,
                        cx,
                    )
                }
                _ => {
                    let placeholder =
                        definition
                            .placeholder
                            .clone()
                            .or_else(|| match definition.kind.as_str() {
                                "file" => Some("Choose or enter a file path".to_string()),
                                "directory" => Some("Choose or enter a directory path".to_string()),
                                _ => None,
                            });
                    let field_id = SharedString::from(format!(
                        "preference-{extension_id}-{}",
                        definition.name
                    ));
                    let input = FieldStates::text_state(
                        self,
                        &field_id,
                        &preference_string(Some(&value)),
                        placeholder.as_deref(),
                        move |page: &mut Self, text, _window, cx| {
                            page.set_preference(&name, json!(text), cx)
                        },
                        window,
                        cx,
                    );
                    super::super::accounts::widgets::sync_masked(
                        &mut self.masked_inputs,
                        &field_id,
                        &input,
                        definition.kind == "password",
                        window,
                        cx,
                    );
                    settings_text_input(p, &input, None, false, window, cx)
                }
            };
            let label = h_flex()
                .gap(px(2.0))
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(if invalid { p.destructive } else { p.muted }))
                .child(definition.title.clone())
                .when(definition.required, |this| this.child("*"));
            fields.push(
                v_flex()
                    .w_full()
                    .gap(px(8.0))
                    .child(label)
                    .when(!definition.description.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.85))
                                .text_color(hsla(p.muted))
                                .child(definition.description.clone()),
                        )
                    })
                    .child(div().w_full().flex().child(control))
                    .when(invalid, |this| {
                        this.child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.57))
                                .text_color(hsla(p.destructive))
                                .child(format!("{} is required.", definition.title)),
                        )
                    })
                    .into_any_element(),
            );
        }
        v_flex()
            .w_full()
            .gap(px(16.0))
            .children(fields)
            .into_any_element()
    }

    fn set_preference(&mut self, name: &str, value: Value, cx: &mut Context<Self>) {
        if let Some((_, draft)) = self.preferences_draft.as_mut() {
            draft.insert(name.to_string(), value);
            cx.notify();
        }
    }

    /// `StoreExtensionDetail`.
    fn render_store_detail(
        &mut self,
        p: &SettingsPalette,
        entry: &CatalogEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = entry.name();
        let installed_version = self
            .browser
            .installed_extension(&name)
            .map(InstalledExtension::version);
        let icon_bytes = if installed_version.is_some() {
            self.extension_icon(&name, &entry.icon(), cx)
        } else {
            None
        };
        let has_source = installed_version.is_some() && self.has_transport(cx);
        let header = self.detail_header(
            p,
            extension_icon_tile(p, icon_bytes, has_source, true),
            entry.title(),
            entry.description(),
            |page: &mut Self, _window, cx| page.select_store_entry(None, cx),
            cx,
        );
        let update_available = installed_version
            .as_deref()
            .is_some_and(|installed| is_version_newer(&entry.version(), installed));
        let action_label = if update_available {
            format!("Update to v{}", entry.version())
        } else if installed_version.is_some() {
            "Installed".to_string()
        } else {
            "Install".to_string()
        };
        let mut main: Vec<AnyElement> = Vec::new();
        let urls = screenshot_urls(&self.browser, entry);
        if !urls.is_empty() {
            let mut shots: Vec<AnyElement> = Vec::new();
            for url in &urls {
                if let ScreenshotSlot::Ready(image, width, height) = self.screenshot(url, cx) {
                    let shown_width = if height > 0.0 {
                        176.0 * width / height
                    } else {
                        176.0
                    };
                    shots.push(
                        img(image)
                            .flex_shrink_0()
                            .h(px(176.0))
                            .w(px(shown_width))
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .border_1()
                            .border_color(hsla(p.modal.hairline))
                            .bg(hsla(p.modal.raised))
                            .into_any_element(),
                    );
                }
            }
            main.push(
                h_flex()
                    .id("extension-screenshots")
                    .w_full()
                    .gap(px(12.0))
                    .p(px(12.0))
                    .overflow_x_scroll()
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hsla(p.modal.hairline))
                    .bg(hsla(p.modal.panel))
                    .children(shots)
                    .into_any_element(),
            );
        }
        let readme: AnyElement = if self.browser.loading_content {
            div()
                .text_size(px(14.0))
                .text_color(hsla(p.muted))
                .child("Loading extension details…")
                .into_any_element()
        } else if let Some(markdown) = self.browser.readme.clone() {
            markdown_view(
                p,
                SharedString::from(format!("extension-readme-{name}")),
                markdown,
            )
        } else {
            div()
                .text_size(px(14.0))
                .text_color(hsla(p.muted))
                .child("Extension README could not be loaded.")
                .into_any_element()
        };
        main.push(
            v_flex()
                .w_full()
                .gap(px(10.0))
                .child(section_label(p, format!("About {}", entry.title())))
                .child(markdown_group(p, readme))
                .into_any_element(),
        );
        if let Some(changelog) = self.browser.changelog.clone() {
            main.push(
                v_flex()
                    .w_full()
                    .gap(px(10.0))
                    .child(section_label(p, "Changelog"))
                    .child(markdown_group(
                        p,
                        markdown_view(
                            p,
                            SharedString::from(format!("extension-changelog-{name}")),
                            changelog,
                        ),
                    ))
                    .into_any_element(),
            );
        }
        let install = {
            let name = name.clone();
            settings_sized_button(
                p,
                "extension-store-install",
                action_label,
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                installed_version.is_some() && !update_available,
                None,
                move |page: &mut Self, window, cx| page.open_consent(name.clone(), window, cx),
                cx,
            )
        };
        let aside = vec![
            v_flex()
                .w_full()
                .gap(px(10.0))
                .child(section_label(p, "Details"))
                .child(detail_group(
                    p,
                    vec![
                        Self::detail_row(p, "Author", entry.author()),
                        Self::detail_row(p, "Version", format!("v{}", entry.version())),
                    ],
                ))
                .into_any_element(),
            Self::permissions_section(p, &entry.permissions()),
            div().flex().child(install).into_any_element(),
        ];
        let columns = self.detail_columns(main, aside, 24.0, window);
        v_flex()
            .w_full()
            .gap(px(24.0))
            .child(header)
            .child(columns)
            .into_any_element()
    }

    /// `InstallConsentDialog`.
    pub(crate) fn render_consent(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let entry = self.browser.consent_entry()?.clone();
        let name = entry.name();
        let installing = self.browser.pending.contains(&name);
        let permissions = entry.permissions();
        let background_process = entry.runs_background_process();
        let remote_url = entry.remote_url();
        let m = p.modal;
        let item = |glyph: &'static str, title: String, description: String| -> AnyElement {
            h_flex()
                .w_full()
                .items_start()
                .gap(px(12.0))
                .p(px(12.0))
                .rounded(px(MODAL_RADIUS_SECTION))
                .border_1()
                .border_color(hsla(m.hairline))
                .bg(hsla(m.panel))
                .text_size(px(13.0))
                .child(
                    div()
                        .mt(px(2.0))
                        .flex_shrink_0()
                        .child(settings_icon(glyph, 24.0, m.muted)),
                )
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .text_color(hsla(m.foreground))
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(hsla(m.muted))
                                .child(description),
                        ),
                )
                .into_any_element()
        };
        let body: AnyElement = if permissions.is_empty()
            && !background_process
            && remote_url.is_none()
        {
            div()
                .text_size(px(13.0))
                .text_color(hsla(m.muted))
                .child("This extension does not request additional permissions.")
                .into_any_element()
        } else {
            let mut items: Vec<AnyElement> = permissions
                .iter()
                .map(|permission| {
                    item(
                        "modals/settings/shield-check.svg",
                        capitalize(permission),
                        permission_description(permission).to_string(),
                    )
                })
                .collect();
            if background_process {
                items.push(item(
                    super::super::super::fields::icon::ALERT_TRIANGLE,
                    "Runs a background process".into(),
                    "Server extensions run outside a sandbox. Open-source review remains the primary trust boundary.".into(),
                ));
            }
            if let Some(url) = remote_url {
                items.push(item(
                    super::super::super::fields::icon::ALERT_TRIANGLE,
                    "Loads a remote website".into(),
                    format!("This extension opens {url} directly. The page runs outside Ghostex and cannot use the extension bridge, but it sees whatever you type into it."),
                ));
            }
            v_flex()
                .w_full()
                .gap(px(8.0))
                .children(items)
                .into_any_element()
        };
        let cancel = modal_action_button(
            &m,
            "extension-consent-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            installing,
            |page: &mut Self, _window, cx| {
                page.browser.consent = None;
                cx.notify();
            },
            cx,
        );
        let confirm = {
            let name = name.clone();
            modal_action_button(
                &m,
                "extension-consent-install",
                if installing {
                    "Installing…"
                } else {
                    "Install"
                },
                None,
                ModalButtonTone::Neutral,
                installing,
                move |page: &mut Self, _window, cx| page.install_extension(name.clone(), cx),
                cx,
            )
        };
        let focus = self.consent_focus.clone();
        let sheet = v_flex()
            .id("extension-consent")
            .track_focus(&focus)
            .occlude()
            .w(px(520.0))
            .max_w(window.viewport_size().width - px(32.0))
            .p(px(MODAL_WINDOW_PADDING))
            .gap(px(MODAL_SECTION_GAP))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(m.hairline))
            .bg(hsla(m.solid_surface))
            .shadow_xl()
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(m.foreground))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(
                cx.listener(move |page, event: &gpui::KeyDownEvent, _window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        let installing = page
                            .browser
                            .consent
                            .as_ref()
                            .is_some_and(|name| page.browser.pending.contains(name));
                        if !installing {
                            page.browser.consent = None;
                            cx.notify();
                        }
                    }
                }),
            )
            .child(modal_header(
                &m,
                format!("Install {}?", entry.title()),
                Some("Review the access this audited extension declares before installing it."),
            ))
            .child(body)
            .child(modal_footer(vec![cancel, confirm]))
            .into_any_element();
        Some(super::super::super::fields::settings_dialog_layer(
            "extension-consent",
            sheet,
            0.0,
            |page: &mut Self, _window, cx| {
                let installing = page
                    .browser
                    .consent
                    .as_ref()
                    .is_some_and(|name| page.browser.pending.contains(name));
                if !installing {
                    page.browser.consent = None;
                    cx.notify();
                }
            },
            window,
            cx,
        ))
    }
}

/// `ExtensionGroup className='divide-y-0 p-5'` around a Markdown document.
fn markdown_group(p: &SettingsPalette, content: AnyElement) -> AnyElement {
    div()
        .w_full()
        .p(px(20.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.modal.hairline))
        .bg(hsla(p.modal.panel))
        .child(content)
        .into_any_element()
}

/// `SessionChatMarkdown` of a README or changelog, on GPUI-Kit's Markdown `TextView`.
fn markdown_view(p: &SettingsPalette, id: SharedString, markdown: String) -> AnyElement {
    TextView::markdown(id, markdown)
        .w_full()
        .min_w_0()
        .selectable(true)
        .style(markdown_style(p))
        .text_size(px(14.0))
        .line_height(px(22.75))
        .text_color(hsla(p.foreground))
        .into_any_element()
}

/// The chat Markdown metrics (`.ghostex-chat-markdown`, shared through
/// packages/gx-chat-core/visual/markdown-visual.json) at the Settings type weight: headings
/// 20/18/16px on a 1.3 line with 20px above and 8px below, 10.4px between blocks, the 20px list
/// gutter with 4px between items, and inline code as a small bordered chip. Strong text is 500, not
/// bold: `.ghostex-settings-shadcn.ghostex-settings-shadcn :is(b, strong) { font-weight: 500 }` in
/// packages/core-ui/styles.css.
fn markdown_style(p: &SettingsPalette) -> gpui_component::text::TextViewStyle {
    use gpui::{StyleRefinement, relative, rems};
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Visual {
        paragraph_gap: f32,
        heading_gap_before: f32,
        heading_line_height: f32,
        heading_font_sizes: [f32; 6],
    }
    static VISUAL: std::sync::LazyLock<Option<Visual>> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../../../../../../packages/gx-chat-core/visual/markdown-visual.json"
        ))
        .ok()
    });
    let (gap, before, line, sizes) = VISUAL
        .as_ref()
        .map(|visual| {
            (
                visual.paragraph_gap,
                visual.heading_gap_before,
                visual.heading_line_height,
                visual.heading_font_sizes,
            )
        })
        .unwrap_or((10.4, 20.0, 1.3, [20.0, 18.0, 16.0, 14.0, 14.0, 14.0]));
    let mut style = gpui_component::text::TextViewStyle::default()
        .paragraph_gap(rems(gap / 16.0))
        .heading_font_size(move |level, base| base * sizes[(level.clamp(1, 6) - 1) as usize] / 14.0)
        .strong_weight(gpui::FontWeight::MEDIUM);
    style.heading_base_font_size = px(14.0);
    style.heading = StyleRefinement::default()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(hsla(p.foreground))
        .line_height(relative(line))
        .pt(px((before - gap).max(0.0)))
        .pb(px(8.0));
    style.list = StyleRefinement::default().gap(px(4.0));
    style.list_marker = StyleRefinement::default().min_w(px(20.0));
    style.inline_code_style = Some(gpui_component::text::InlineCodeStyle {
        font_family: MODAL_MONO_FONT.into(),
        font_scale: 0.9,
        padding_x: px(5.6),
        padding_y: px(1.6),
        border_width: px(1.0),
        radius: px(6.0),
        background: hsla(gpui::rgb(if p.light { 0xefeff0 } else { 0x272727 })),
        border_color: hsla(p.foreground_alpha(if p.light { 0.16 } else { 0.18 })),
        swatches: false,
        swatch: None,
        swatch_border_width: px(1.0),
        swatch_border_color: gpui::transparent_black(),
        prose: false,
    });
    style
}
