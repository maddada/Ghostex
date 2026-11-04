//! The About page (packages/core-ui/settings-modal/tabs/about.tsx (deleted 2026-10-01)): the Ghostex mark, the version,
//! and the Discord, GitHub and Sponsor links, which open in the system browser; then Experimental
//! and Reset to defaults, which came here from General's Advanced group (CDXC:Settings 2026-10-29
//! in model.rs).
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, FieldStates, RowSpec, SettingsPage, advanced_fold_header, card_inset,
    settings_button, settings_icon, settings_section, toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, page_separator, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsValues, post_store_message};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

/// `GHOSTEX_DISCORD_URL` (packages/shared/sidebar-commands.ts (deleted 2026-10-01)).
const GHOSTEX_DISCORD_URL: &str = "https://discord.gg/df7b3G92CS";
/// `GHOSTEX_GITHUB_URL`.
const GHOSTEX_GITHUB_URL: &str = "https://github.com/maddada/Ghostex";
/// `GHOSTEX_SPONSOR_URL`.
const GHOSTEX_SPONSOR_URL: &str = "https://github.com/sponsors/maddada";

/// The app version the About page names, baked in by apps/desktop/build.rs (`package.json`).
const APP_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

const LINKS: [(&str, &str, &str); 3] = [
    (
        "Join Discord",
        "Chat with the community and get help.",
        GHOSTEX_DISCORD_URL,
    ),
    (
        "View on GitHub",
        "View the source, releases, and report issues.",
        GHOSTEX_GITHUB_URL,
    ),
    (
        "Sponsor Ghostex",
        "Support the continued development of Ghostex.",
        GHOSTEX_SPONSOR_URL,
    ),
];

pub(crate) fn about_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| AboutTab::new(store.clone(), cx)).into()
}

pub(crate) struct AboutTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
}

impl AboutTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            fields: FieldStates::default(),
        }
    }
}

/// `resetSettings`: the bundled app icon, then every setting back to its default except the
/// saved remote machines.
fn reset_all(page: &mut AboutTab, cx: &mut Context<AboutTab>) {
    let store = page.store.clone();
    store.update(cx, |store, cx| {
        store.post_message(json!({ "sourceId": "", "type": "setAppIcon" }), cx);
        let mut defaults = settings_catalog().defaults().clone();
        defaults.insert("remoteMachines".to_string(), store.value("remoteMachines"));
        store.apply_settings(defaults, "settings:bulk", cx);
    });
}

impl AboutTab {
    /// The Experimental section (General's old `beta` section). Its rows are advanced
    /// (`ADVANCED_MAIN_SETTING_KEYS`): `visible` says whether each shows.
    fn experimental_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        visible: &dyn Fn(&str) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        if visible("showBetaFeatures") {
            // CDXC:Settings 2026-06-28-07:41: the inventory of what Enable Experimental Features turns on.
            // CDXC:Automations 2026-07-26: GPUI graduated project Automate, so it lists only All Automations.
            rows.push(toggle_field(
                self,
                p,
                "showBetaFeatures",
                RowSpec::new("Enable Experimental Features")
                    .description(
                        "Show experimental settings, All Automations, and the Keep Awake menu.",
                    )
                    .keyed(values, "showBetaFeatures"),
                values.bool("showBetaFeatures"),
                cx,
            ));
            let muted_fill = if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                gpui::rgb(0x262626)
            };
            rows.push(card_inset(
                v_flex()
                    .w_full()
                    .px(px(16.0))
                    .py(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(css_fade(muted_fill, 0.2)))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .child(
                        div()
                            .mb(px(8.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Enabled when on"),
                    )
                    .child(
                        v_flex()
                            .gap(px(6.0))
                            .child("OS Integration settings tab")
                            .child("All Automations")
                            .child("Power settings and the sidebar menu: Keep Awake")
                            .children(
                                cfg!(target_os = "macos")
                                    .then(|| div().child("Terminal custom shaders (macOS only)")),
                            ),
                    ),
            ));
        }
        if visible("fasterRendering") {
            rows.push(toggle_field(
                self,
                p,
                "fasterRendering",
                RowSpec::new("Faster rendering")
                    .description("Redraw only the parts of a window that changed, which uses less CPU while agents stream and you scroll. Experimental: turn it off if part of a window stops updating.")
                    .keyed(values, "fasterRendering"),
                values.bool("fasterRendering"),
                cx,
            ));
        }
        settings_section(p, "Experimental", None, None, rows).map(IntoElement::into_any_element)
    }
}

impl SettingsPage for AboutTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl Render for AboutTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching, values) = {
            let store = self.store.read(cx);
            let search = store.tab_search(SettingsTabId::About);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (store.palette(), search, matching, store.values())
        };
        let mut blocks = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            // `searchEmptyState` inside `settings-page-width px-5 py-5`.
            blocks.push(PageBlock::plain(div().pt(px(20.0)).child(
                render_no_matches(
                    &p,
                    SettingsTabId::About,
                    &matching,
                    move |tab, _window, cx| {
                        store.update(cx, |store, cx| store.set_active_tab(tab, cx))
                    },
                ),
            )));
            return settings_page(&self.store, SettingsTabId::About, &p, blocks, cx);
        }
        let hover = p.raised_hover;
        let links = LINKS
            .iter()
            .enumerate()
            .map(|(index, (label, description, url))| {
                let store = self.store.clone();
                let url = url.to_string();
                h_flex()
                    .id(SharedString::from(format!("settings-about-link-{index}")))
                    .w_full()
                    .min_h(px(64.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .gap(px(16.0))
                    .items_center()
                    .justify_between()
                    .when(index > 0, |this| {
                        this.border_t_1().border_color(hsla(p.hairline))
                    })
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(move |_: &ClickEvent, _window, cx| {
                        post_store_message(
                            &store,
                            json!({ "type": "openExternalUrl", "url": url }),
                            cx,
                        );
                    })
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(hsla(p.foreground))
                                    .child(*label),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.2))
                                    .text_color(hsla(p.muted))
                                    .child(*description),
                            ),
                    )
                    .child(settings_icon(
                        "modals/settings/external-link.svg",
                        16.0,
                        p.foreground,
                    ))
            });
        let page = v_flex()
            .w_full()
            .pt(px(32.0))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(img("app-icon.png").flex_shrink_0().size(px(52.0)))
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .line_height(px(21.6))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(p.foreground))
                                    .child("Ghostex"),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(20.0))
                                    .text_color(hsla(p.muted))
                                    .child(format!("Version {APP_VERSION}")),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(20.0))
                    .mb(px(24.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(hsla(p.muted))
                    .child("A workspace for building with coding agents."),
            )
            .child(
                v_flex()
                    .w_full()
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .overflow_hidden()
                    .children(links),
            );
        blocks.push(PageBlock::plain(page));
        // Experimental's rows are advanced: a search shows the matching ones in place, otherwise
        // they sit in the collapsed Advanced fold (CDXC:Settings 2026-10-29 in search.rs).
        let searching = search.tab.is_searching;
        let visible = |key: &str| search.row_visible("beta", key);
        if searching {
            if let Some(section) = self.experimental_section(&p, &values, &visible, cx) {
                blocks.push(PageBlock::section("beta", section));
            }
        } else {
            let open = self.store.read(cx).advanced_fold_open(SettingsTabId::About);
            blocks.push(PageBlock::section(
                "advancedFold",
                advanced_fold_header(
                    &p,
                    open,
                    |page: &mut Self, _window, cx| {
                        let store = page.store.clone();
                        store.update(cx, |store, cx| {
                            store.toggle_advanced_fold(SettingsTabId::About, cx)
                        });
                    },
                    cx,
                ),
            ));
            if open && let Some(section) = self.experimental_section(&p, &values, &visible, cx) {
                blocks.push(PageBlock::section("beta", section));
            }
        }
        blocks.push(PageBlock::plain(page_separator(&p)));
        blocks.push(PageBlock::plain(
            h_flex()
                .w_full()
                .justify_between()
                .gap_3()
                .child(settings_button(
                    &p,
                    "settings-reset-to-defaults",
                    "Reset all settings to defaults",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| reset_all(page, cx),
                    cx,
                )),
        ));
        settings_page(&self.store, SettingsTabId::About, &p, blocks, cx)
    }
}
