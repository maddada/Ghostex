//! The About page (packages/core-ui/settings-modal/tabs/about.tsx (deleted 2026-10-01)): the Ghostex mark, the version,
//! and the Discord, GitHub and Sponsor links, which open in the system browser; then Experimental
//! and Reset to defaults, which came here from General's Advanced group (CDXC:Settings 2026-10-29
//! in model.rs).
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, FieldStates, SettingsDialogSpec, SettingsPage, settings_button, settings_dialog,
    settings_icon, settings_icon_button, switch_control,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, page_separator, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{
    SettingsStore, post_store_message, store_copy_to_clipboard, store_gxserver_rpc,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

/// `GHOSTEX_DISCORD_URL` (packages/shared/sidebar-commands.ts (deleted 2026-10-01)).
const GHOSTEX_DISCORD_URL: &str = "https://discord.gg/df7b3G92CS";
/// `GHOSTEX_GITHUB_URL`.
const GHOSTEX_GITHUB_URL: &str = "https://github.com/maddada/Ghostex";
/// `GHOSTEX_SPONSOR_URL`.
const GHOSTEX_SPONSOR_URL: &str = "https://github.com/sponsors/maddada";
/// Public analytics & privacy documentation on GitHub.
const GHOSTEX_ANALYTICS_DOCS_URL: &str =
    "https://github.com/maddada/Ghostex/blob/main/ANALYTICS.md";

/// Unified analytics toggle title (Onboarding Step 2 and Desktop Settings > About).
const ANALYTICS_TOGGLE_TITLE: &str =
    "Share 100% Fully Anonymous Usage Analytics to Support Ghostex Development";
const ANALYTICS_TOGGLE_SUBTITLE: &str = "Zero personal data. Helps us continue supporting and building Ghostex. Turning this off stops all usage tracking and keeps only 1 anonymous weekly active count (stop all with DO_NOT_TRACK=1 or --no-analytics).";

/// Seven data categories listed in the "Never sent" summary of the payload inspector (Q26).
const NEVER_SENT_BADGES: [&str; 7] = [
    "Prompts",
    "Code",
    "File paths",
    "Emails",
    "Login files",
    "Custom CLI/tool names",
    "Raw IPs",
];

const TELEMETRY_PREVIEW_MONO_FONT: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "monospace"
};

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

#[derive(Clone, Debug)]
enum TelemetryPreviewState {
    Idle,
    Loading,
    Loaded(Value),
    Failed(String),
}

pub(crate) struct AboutTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The Reset all settings? confirmation is open.
    confirming_reset: bool,
    confirm_focus: FocusHandle,
    /// The "View what's sent" payload inspector dialog is open (Q26).
    telemetry_preview_open: bool,
    telemetry_preview_focus: FocusHandle,
    telemetry_preview_state: TelemetryPreviewState,
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
            telemetry_preview_open: false,
            telemetry_preview_focus: cx.focus_handle(),
            telemetry_preview_state: TelemetryPreviewState::Idle,
            preview_applied: false,
        }
    }

    /// The preview binary's page states (`preview_state`):
    /// - `about-reset-confirm`: opens the Reset all settings? confirmation.
    /// - `about-telemetry-preview` / `about-telemetry-preview-weekly` / `about-telemetry-preview-off`:
    ///   opens the "View what's sent" badges + JSON sheet.
    fn apply_preview_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_applied {
            return;
        }
        self.preview_applied = true;
        match self.store.read(cx).request().preview_state.as_deref() {
            Some("about-reset-confirm") => {
                self.confirming_reset = true;
                self.confirm_focus.focus(window, cx);
            }
            Some(
                "about-telemetry-preview"
                | "about-telemetry-preview-weekly"
                | "about-telemetry-preview-off",
            ) => {
                self.open_telemetry_preview(window, cx);
            }
            _ => {}
        }
    }

    fn close_reset_dialog(&mut self, cx: &mut Context<Self>) {
        self.confirming_reset = false;
        cx.notify();
    }

    fn open_telemetry_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.telemetry_preview_open = true;
        self.telemetry_preview_state = TelemetryPreviewState::Loading;
        self.telemetry_preview_focus.focus(window, cx);
        cx.notify();
        let store = self.store.clone();
        let weak = cx.entity().downgrade();
        store_gxserver_rpc(
            &store,
            "/api/telemetryPreview",
            json!({}),
            Duration::from_secs(10),
            move |result, cx| {
                let _ = weak.update(cx, |page, cx| {
                    page.telemetry_preview_state = match result {
                        Ok(value) => TelemetryPreviewState::Loaded(value),
                        Err(error) => TelemetryPreviewState::Failed(error),
                    };
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn close_telemetry_preview(&mut self, cx: &mut Context<Self>) {
        self.telemetry_preview_open = false;
        cx.notify();
    }

    fn formatted_preview_payload(&self) -> (Option<&str>, String, Option<String>) {
        match &self.telemetry_preview_state {
            TelemetryPreviewState::Idle | TelemetryPreviewState::Loading => (
                None,
                "Loading live preview from gxserver...".to_string(),
                None,
            ),
            TelemetryPreviewState::Failed(error) => (
                None,
                format!("Could not load live preview from gxserver: {error}"),
                None,
            ),
            TelemetryPreviewState::Loaded(value) => {
                let mode = value
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| {
                        if value.get("enabled").and_then(Value::as_bool) == Some(true) {
                            "full"
                        } else {
                            "weekly_count_only"
                        }
                    });
                let display_value = match mode {
                    "off" => json!({
                        "mode": "off",
                        "reason": value.get("reason").cloned().unwrap_or(Value::Null),
                        "eventsSent": [],
                    }),
                    "weekly_count_only" => value
                        .get("weeklyCount")
                        .cloned()
                        .unwrap_or_else(|| value.clone()),
                    _ => {
                        let mut map = serde_json::Map::new();
                        map.insert("mode".to_string(), json!("full"));
                        if let Some(id) = value.get("distinctId") {
                            map.insert("distinctId".to_string(), id.clone());
                        }
                        if let Some(heartbeat) = value.get("heartbeat") {
                            if !heartbeat.is_null() {
                                map.insert("heartbeat".to_string(), heartbeat.clone());
                            }
                        }
                        if let Some(weekly) = value.get("weeklyCount") {
                            if !weekly.is_null() {
                                map.insert("weeklyCount".to_string(), weekly.clone());
                            }
                        }
                        if let Some(snapshot) = value.get("settingsSnapshot") {
                            if !snapshot.is_null() {
                                map.insert("settingsSnapshot".to_string(), snapshot.clone());
                            }
                        }
                        Value::Object(map)
                    }
                };
                let pretty = serde_json::to_string_pretty(&display_value)
                    .unwrap_or_else(|_| display_value.to_string());
                (Some(mode), pretty.clone(), Some(pretty))
            }
        }
    }

    /// CDXC:Telemetry 2026-11-25 DECISION (Q26 = badges-and-json):
    /// Nested Settings dialog showing the seven "Never sent" summary chips above the live
    /// formatted JSON from `/api/telemetryPreview` and a Copy JSON button.
    fn render_telemetry_preview_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.telemetry_preview_open {
            return None;
        }
        let p = &p.nested_dialog();
        let (mode, preview_text, copyable_json) = self.formatted_preview_payload();
        let description_text = match mode {
            Some("off") => {
                "Nothing is sent (DO_NOT_TRACK / GHOSTEX_TELEMETRY_DISABLED / --no-analytics). 0 events ever."
            }
            Some("weekly_count_only") => {
                "Usage tracking is off. Ghostex sends at most 1 anonymous weekly active count per 7 active days (random install ID + analytics_enabled: false, zero other properties). Set DO_NOT_TRACK=1 or --no-analytics to stop all events."
            }
            _ => {
                "Exact live JSON your install sends when analytics is on. Uses a random per-install UUIDv4 only."
            }
        };
        let code_bg = if p.light {
            gpui::rgb(0xf4f4f6)
        } else {
            gpui::rgb(0x111114)
        };
        let close_btn = settings_icon_button(
            p,
            "settings-about-telemetry-preview-close",
            "modals/settings/x.svg",
            14.0,
            26.0,
            ButtonVariant::Outline,
            Some("Close".into()),
            false,
            |page: &mut Self, _window, cx| page.close_telemetry_preview(cx),
            cx,
        );
        let never_chips = NEVER_SENT_BADGES.iter().map(|label| {
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.raised))
                .child(
                    div()
                        .size(px(5.0))
                        .rounded_full()
                        .flex_shrink_0()
                        .bg(hsla(p.muted)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.foreground))
                        .whitespace_nowrap()
                        .child(*label),
                )
        });
        let never_box = v_flex()
            .w_full()
            .gap(px(7.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(code_bg))
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(p.muted))
                    .child("NEVER SENT"),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(never_chips),
            )
            .into_any_element();

        let status_banner = (mode == Some("off")).then(|| {
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(9.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.raised))
                .child(settings_icon(
                    "modals/settings/shield-check.svg",
                    15.0,
                    p.foreground,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.5))
                        .line_height(px(18.0))
                        .text_color(hsla(p.foreground))
                        .child("Nothing is sent (DO_NOT_TRACK / --no-analytics)."),
                )
                .into_any_element()
        });

        let json_box = div()
            .id("settings-about-telemetry-preview-json")
            .w_full()
            .max_h(px(290.0))
            .overflow_y_scroll()
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(code_bg))
            .font_family(TELEMETRY_PREVIEW_MONO_FONT)
            .text_size(px(11.5))
            .line_height(px(17.0))
            .text_color(hsla(p.foreground))
            .child(preview_text)
            .into_any_element();

        let mut body = vec![never_box];
        if let Some(banner) = status_banner {
            body.push(banner);
        }
        body.push(json_box);

        let copy_text = copyable_json.clone();
        let copy_btn = settings_button(
            p,
            "settings-about-telemetry-preview-copy",
            "Copy JSON",
            Some("modals/settings/copy.svg"),
            ButtonVariant::Outline,
            copyable_json.is_none(),
            None,
            move |page: &mut Self, _window, cx| {
                if let Some(text) = copy_text.clone() {
                    store_copy_to_clipboard(&page.store, text, cx);
                }
            },
            cx,
        );
        let done_btn = settings_button(
            p,
            "settings-about-telemetry-preview-done",
            "Done",
            None,
            ButtonVariant::Primary,
            false,
            None,
            |page: &mut Self, _window, cx| page.close_telemetry_preview(cx),
            cx,
        );
        let footer = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .pt(px(2.0))
            .child(
                div()
                    .min_w_0()
                    .font_family(TELEMETRY_PREVIEW_MONO_FONT)
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.muted))
                    .child("POST /api/telemetryPreview"),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(copy_btn)
                    .child(done_btn),
            )
            .into_any_element();

        let focus = self.telemetry_preview_focus.clone();
        Some(settings_dialog(
            p,
            SettingsDialogSpec::new(
                "settings-about-telemetry-preview-dialog",
                "What Ghostex sends",
            )
            .width(548.0)
            .spacing(18.0, 12.0)
            .header_trailing(close_btn)
            .description(description_text.into_any_element()),
            body,
            Some(footer),
            Some(&focus),
            |page: &mut Self, _window, cx| page.close_telemetry_preview(cx),
            window,
            cx,
        ))
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
                    "Every setting and the app icon go back to their defaults. Your saved remote machines and your analytics choice are kept. This can't be undone."
                        .into_any_element(),
                ),
            Vec::new(),
            Some(super::super::fields::dialog_footer(vec![cancel, confirm])),
            Some(&focus),
            |page: &mut Self, _window, cx| page.close_reset_dialog(cx),
            window,
            cx,
        ))
    }
}

/// `resetSettings`: the bundled app icon, then every setting back to its default except the
/// saved remote machines and the user's analytics choice (`analyticsEnabled`).
fn reset_all(page: &mut AboutTab, cx: &mut Context<AboutTab>) {
    let store = page.store.clone();
    store.update(cx, |store, cx| {
        store.post_message(json!({ "sourceId": "", "type": "setAppIcon" }), cx);
        let mut defaults = settings_catalog().defaults().clone();
        defaults.insert("remoteMachines".to_string(), store.value("remoteMachines"));
        defaults.insert(
            "analyticsEnabled".to_string(),
            store.value("analyticsEnabled"),
        );
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
        let show_about = !search.tab.is_searching || search.section("about").has_visible();
        let show_privacy = !search.tab.is_searching || search.section("privacy").has_visible();
        let analytics_enabled = self.store.read(cx).bool("analyticsEnabled");

        if show_about {
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
        }

        // CDXC:Telemetry 2026-11-25 DECISION (Q25 = compact, Q29):
        // Compact Privacy & Analytics block with inline "View what's sent" and ANALYTICS.md links.
        if show_privacy {
            if show_about {
                blocks.push(PageBlock::plain(page_separator(&p)));
            }
            let analytics_switch = switch_control(
                &p,
                "settings-about-analytics-switch",
                ANALYTICS_TOGGLE_TITLE,
                analytics_enabled,
                false,
                None,
                |page: &mut Self, next, _window, cx| {
                    page.store.update(cx, |store, cx| {
                        store.update_setting("analyticsEnabled", json!(next), cx);
                    });
                },
                cx,
            );
            let (switch_el, row_toggle) = analytics_switch.into_row_parts();
            let wash = switch_row_wash(p.foreground);
            let docs_store = self.store.clone();
            let view_sent_link = h_flex()
                .id("settings-about-view-whats-sent")
                .role(gpui::Role::Button)
                .aria_label("View what's sent")
                .items_center()
                .gap(px(5.0))
                .cursor_pointer()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .underline()
                .on_click(cx.listener(|page, _event: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    page.open_telemetry_preview(window, cx);
                }))
                .child(settings_icon("modals/settings/eye.svg", 14.0, p.foreground))
                .child("View what's sent");
            let analytics_docs_link = h_flex()
                .id("settings-about-analytics-docs")
                .role(gpui::Role::Button)
                .aria_label("ANALYTICS.md")
                .items_center()
                .gap(px(5.0))
                .cursor_pointer()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .underline()
                .on_click(move |_event: &ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    post_store_message(
                        &docs_store,
                        json!({ "type": "openExternalUrl", "url": GHOSTEX_ANALYTICS_DOCS_URL }),
                        cx,
                    );
                })
                .child(settings_icon(
                    "modals/settings/external-link.svg",
                    13.0,
                    p.foreground,
                ))
                .child("ANALYTICS.md");
            let privacy_row = h_flex()
                .id("settings-about-analytics-row")
                .w_full()
                .px(px(16.0))
                .py(px(14.0))
                .gap(px(16.0))
                .items_start()
                .justify_between()
                .when_some(row_toggle, |this, toggle| {
                    this.switch_row_press(wash, false, toggle)
                })
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child(ANALYTICS_TOGGLE_TITLE),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.5))
                                .text_color(hsla(p.muted))
                                .child(ANALYTICS_TOGGLE_SUBTITLE),
                        )
                        .child(
                            h_flex()
                                .mt(px(6.0))
                                .flex_wrap()
                                .items_center()
                                .gap(px(16.0))
                                .child(view_sent_link)
                                .child(analytics_docs_link),
                        ),
                )
                .child(div().pt(px(2.0)).flex_shrink_0().child(switch_el));
            let privacy_card = v_flex()
                .w_full()
                .when(!show_about, |this| this.pt(px(20.0)))
                .child(
                    v_flex()
                        .w_full()
                        .rounded(px(MODAL_RADIUS_SECTION))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .overflow_hidden()
                        .child(privacy_row),
                );
            blocks.push(PageBlock::plain(privacy_card));
        }

        // About has no Advanced section any more: Faster rendering, its last row, is a switch in
        // Settings > Extensions > Experimental (CDXC:Extensions 2026-11-09 in the settings catalog).
        if !search.tab.is_searching {
            blocks.push(PageBlock::plain(page_separator(&p)));
            blocks.push(PageBlock::plain(
                v_flex()
                    .w_full()
                    .items_start()
                    .gap(px(8.0))
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
                    ))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(hsla(p.muted))
                            .child("Keeps your saved remote machines and your analytics choice."),
                    ),
            ));
        }
        let reset_dialog = self.render_reset_dialog(&p, window, cx);
        let preview_dialog = self.render_telemetry_preview_dialog(&p, window, cx);
        div()
            .size_full()
            .child(settings_page(
                &self.store,
                SettingsTabId::About,
                &p,
                blocks,
                cx,
            ))
            .children(reset_dialog)
            .children(preview_dialog)
            .into_any_element()
    }
}

impl super::HoldsUnsavedInput for AboutTab {
    /// The Reset all settings? confirmation or "View what's sent" dialog is open.
    fn holds_unsaved_input(&self, _cx: &App) -> bool {
        self.confirming_reset || self.telemetry_preview_open
    }
}
