//! The About page (packages/core-ui/settings-modal/tabs/about.tsx (deleted 2026-10-01)): the Ghostex mark, the version,
//! and the Discord, GitHub and Sponsor links, which open in the system browser; then Experimental
//! and Reset to defaults, which came here from General's Advanced group (CDXC:Settings 2026-10-29
//! in model.rs).
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, FieldStates, SettingsDialogSpec, SettingsPage, dialog_footer, settings_button,
    settings_dialog, settings_icon,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, page_separator, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, post_store_message};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
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

/// CDXC:Growth 2026-11-14 DECISION: User (star request page): a Star on GitHub row stays in About
/// for anyone who dismissed the sidebar's ask. It opens the repository in the system browser and
/// counts as a star, so the sidebar never asks for one again (native_sidebar/github_star.rs).
const STAR_LABEL: &str = "Star on GitHub";

const LINKS: [(&str, &str, &str); 4] = [
    (
        STAR_LABEL,
        "Stars help more people find Ghostex.",
        GHOSTEX_GITHUB_URL,
    ),
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
    /// The Reset all settings? confirmation is open.
    confirming_reset: bool,
    confirm_focus: FocusHandle,
    preview_applied: bool,
}

impl AboutTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            fields: FieldStates::default(),
            confirming_reset: false,
            confirm_focus: cx.focus_handle(),
            preview_applied: false,
        }
    }

    /// The preview binary's page state (`preview_state`): `about-reset-confirm` opens the
    /// confirmation, focused as the button leaves it.
    fn apply_preview_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_applied {
            return;
        }
        self.preview_applied = true;
        if self.store.read(cx).request().preview_state.as_deref() == Some("about-reset-confirm") {
            self.confirming_reset = true;
            self.confirm_focus.focus(window, cx);
        }
    }

    fn close_reset_dialog(&mut self, cx: &mut Context<Self>) {
        self.confirming_reset = false;
        cx.notify();
    }

    /// The Reset all settings? confirmation, drawn like Agents' Skip permissions?.
    fn render_reset_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.confirming_reset {
            return None;
        }
        let p = &p.nested_dialog();
        let cancel = settings_button(
            p,
            "settings-reset-to-defaults-cancel",
            "Cancel",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| page.close_reset_dialog(cx),
            cx,
        );
        let confirm = settings_button(
            p,
            "settings-reset-to-defaults-confirm",
            "Reset all settings",
            None,
            ButtonVariant::DestructiveDialog,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.close_reset_dialog(cx);
                reset_all(page, cx);
            },
            cx,
        );
        let focus = self.confirm_focus.clone();
        Some(settings_dialog(
            p,
            SettingsDialogSpec::new("settings-reset-to-defaults-dialog", "Reset all settings?")
                .width(400.0)
                .spacing(20.0, 16.0)
                .description(
                    "Every setting and the app icon go back to their defaults. Your saved remote machines are kept. This can't be undone."
                        .into_any_element(),
                ),
            Vec::new(),
            Some(dialog_footer(vec![cancel, confirm])),
            Some(&focus),
            |page: &mut Self, _window, cx| page.close_reset_dialog(cx),
            window,
            cx,
        ))
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

impl SettingsPage for AboutTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl Render for AboutTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_preview_state(window, cx);
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let search = store.tab_search(SettingsTabId::About);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (store.palette(), search, matching)
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
                let is_star = *label == STAR_LABEL;
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
                        // `githubStar`: the app counts it as a star (native_sidebar/github_star.rs).
                        post_store_message(
                            &store,
                            json!({ "type": "openExternalUrl", "url": url, "githubStar": is_star }),
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
        // About has no Advanced section any more: Faster rendering, its last row, is a switch in
        // Settings > Extensions > Experimental (CDXC:Extensions 2026-11-09 in the settings catalog).
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
                    |page: &mut Self, window, cx| {
                        page.confirming_reset = true;
                        page.confirm_focus.focus(window, cx);
                        cx.notify();
                    },
                    cx,
                )),
        ));
        let dialog = self.render_reset_dialog(&p, window, cx);
        div()
            .size_full()
            .child(settings_page(
                &self.store,
                SettingsTabId::About,
                &p,
                blocks,
                cx,
            ))
            .children(dialog)
            .into_any_element()
    }
}

impl super::HoldsUnsavedInput for AboutTab {
    /// The Reset all settings? confirmation is open.
    fn holds_unsaved_input(&self, _cx: &App) -> bool {
        self.confirming_reset
    }
}
