//! The page's Privacy & Analytics dialog, opened from the work area header's shield button.
//!
//! CDXC:Telemetry 2026-11-26 DECISION (Q33): the page has no Settings window, so a user of
//! `ghostex web` had no switch for the connected computer's `analyticsEnabled` and no way to see
//! what it sends. This dialog is the desktop's Settings > About compact toggle plus its
//! "View what's sent" sheet in one card (apps/desktop/src/app/window/settings_modal/tabs/about.rs):
//! the same title, subtitle, seven NEVER SENT chips and live `POST /api/telemetryPreview` JSON. It
//! adds one truthful line the desktop does not need: the page itself sends no analytics and sets no
//! cookies, and the switch belongs to the computer's gxserver. The switch saves through gxserver
//! the way the page's other settings writes do (`updateSettingsPatch`, see toasts.rs), then reads
//! the preview again so the JSON shows the state the computer is in now.
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;

const ANALYTICS_DOCS_URL: &str = "https://github.com/maddada/Ghostex/blob/main/ANALYTICS.md";
const ANALYTICS_SETTING: &str = "analyticsEnabled";
const TELEMETRY_PREVIEW_PATH: &str = "/api/telemetryPreview";

/// The desktop About page's toggle title and subtitle, word for word.
const ANALYTICS_TOGGLE_TITLE: &str =
    "Share 100% Fully Anonymous Usage Analytics to Support Ghostex Development";
const ANALYTICS_TOGGLE_SUBTITLE: &str = "Zero personal data. Helps us continue supporting and building Ghostex. Turning this off stops all usage tracking and keeps only 1 anonymous weekly active count (stop all with DO_NOT_TRACK=1 or --no-analytics).";
/// What only the page has to say: where the switch acts.
const WEB_SCOPE_NOTE: &str = "ghostex web itself sends zero browser analytics and sets no cookies. This switch controls the connected computer's gxserver analytics. When switched off, the weekly count moves to a new random ID that is not linked to earlier usage; DO_NOT_TRACK=1 or --no-analytics stops all events completely.";

const NEVER_SENT_BADGES: [&str; 7] = [
    "Prompts",
    "Code",
    "File paths",
    "Emails",
    "Login files",
    "Custom CLI/tool names",
    "Raw IPs",
];

const MONO_FONT: &str = "JetBrains Mono";

#[derive(Clone, Debug, Default)]
pub(crate) enum WebPrivacyPreview {
    #[default]
    Loading,
    Loaded(Value),
    /// The computer's gxserver predates the preview endpoint.
    Unsupported,
    Failed(String),
}

#[derive(Default)]
pub(crate) struct WebPrivacyState {
    pub(crate) open: bool,
    preview: WebPrivacyPreview,
    /// Counts reads so an older answer never replaces a newer one.
    read: u64,
}

fn analytics_enabled() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get(ANALYTICS_SETTING)
        .and_then(Value::as_bool)
        != Some(false)
}

/// The desktop sheet's reading of the preview (`formatted_preview_payload`): the mode and the JSON
/// for that mode.
fn formatted_preview(value: &Value) -> (String, String) {
    let mode = value
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if value.get("enabled").and_then(Value::as_bool) == Some(true) {
                "full"
            } else {
                "weekly_count_only"
            }
        })
        .to_string();
    let shown = match mode.as_str() {
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
            for key in ["distinctId", "heartbeat", "weeklyCount", "settingsSnapshot"] {
                if let Some(part) = value.get(key).filter(|part| !part.is_null()) {
                    map.insert(key.to_string(), part.clone());
                }
            }
            Value::Object(map)
        }
    };
    let pretty = serde_json::to_string_pretty(&shown).unwrap_or_else(|_| shown.to_string());
    (mode, pretty)
}

fn mode_description(mode: Option<&str>) -> &'static str {
    match mode {
        Some("off") => {
            "Nothing is sent (DO_NOT_TRACK / GHOSTEX_TELEMETRY_DISABLED / --no-analytics). 0 events ever."
        }
        Some("weekly_count_only") => {
            "Usage tracking is off. Ghostex sends at most 1 anonymous weekly active count per 7 active days (random install ID + analytics_enabled: false, zero other properties). Set DO_NOT_TRACK=1 or --no-analytics to stop all events."
        }
        Some(_) => {
            "Exact live JSON your install sends when analytics is on. Uses a random per-install UUIDv4 only."
        }
        None => "Live JSON from the connected computer's gxserver.",
    }
}

impl GhostexGpuiApp {
    pub(crate) fn web_open_privacy(&mut self, cx: &mut Context<Self>) {
        self.web_host.privacy.open = true;
        self.web_refresh_privacy_preview(cx);
    }

    fn web_close_privacy(&mut self, cx: &mut Context<Self>) {
        self.web_host.privacy.open = false;
        cx.notify();
    }

    fn web_refresh_privacy_preview(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.web_host.privacy;
        state.read += 1;
        state.preview = WebPrivacyPreview::Loading;
        let read = state.read;
        cx.notify();
        cx.spawn(async move |app, cx| {
            let result =
                crate::app::gx_store::gx_rpc(None, TELEMETRY_PREVIEW_PATH, json!({})).await;
            let _ = app.update(cx, |app, cx| {
                let state = &mut app.web_host.privacy;
                if state.read != read {
                    return;
                }
                state.preview = match result {
                    Ok(value) => WebPrivacyPreview::Loaded(value),
                    Err(error)
                        if matches!(
                            error.code.as_deref(),
                            Some("notFound" | "methodNotAllowed")
                        ) =>
                    {
                        WebPrivacyPreview::Unsupported
                    }
                    Err(error) => WebPrivacyPreview::Failed(error.message),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Saves the flipped switch on the computer, then reads the preview again.
    fn web_toggle_analytics(&mut self, cx: &mut Context<Self>) {
        let next = !analytics_enabled();
        let mut patch = serde_json::Map::new();
        patch.insert(ANALYTICS_SETTING.to_string(), Value::Bool(next));
        let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.extend(patch.clone());
        crate::shared_settings::install(settings);
        self.web_host.privacy.preview = WebPrivacyPreview::Loading;
        cx.notify();
        cx.spawn(async move |app, cx| {
            let saved = crate::app::gx_store::gx_rpc(
                None,
                "/api/dispatchRendererCommand",
                json!({
                    "action": "updateSettingsPatch",
                    "payload": { "patch": patch, "source": "web:privacy" },
                }),
            )
            .await;
            if let Err(error) = &saved {
                log::warn!("could not save analyticsEnabled: {}", error.message);
            }
            let _ = app.update(cx, |app, cx| {
                if app.web_host.privacy.open {
                    app.web_refresh_privacy_preview(cx);
                }
            });
        })
        .detach();
    }

    /// The header's shield button.
    pub(crate) fn render_web_privacy_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = titlebar_active_text_color();
        let open = self.web_host.privacy.open;
        div()
            .id("web-header-privacy")
            .role(gpui::Role::Button)
            .aria_label("Privacy & Analytics")
            .size(px(crate::app::consts::TITLEBAR_CONTROL_HEIGHT))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .when(open, |button| button.bg(titlebar_popup_menu_hover_color()))
            .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
            .child(titlebar_svg_icon("modals/settings/shield.svg", 15.0, text))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Privacy & Analytics").build(window, cx)
            })
            .on_click(cx.listener(|app, _, _, cx| {
                if app.web_host.privacy.open {
                    app.web_close_privacy(cx);
                } else {
                    app.web_open_privacy(cx);
                }
            }))
            .into_any_element()
    }

    pub(crate) fn render_web_privacy_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.web_host.privacy.open {
            return None;
        }
        let fg = titlebar_popup_menu_foreground();
        let muted = fg.opacity(0.62);
        let surface = titlebar_popup_menu_background();
        let border = titlebar_popup_menu_border_color();
        let hover = titlebar_popup_menu_hover_color();
        let light = titlebar_background().l > 0.5;
        let code_bg: Hsla = if light {
            gpui::rgb(0xf4f4f6).into()
        } else {
            gpui::rgb(0x111114).into()
        };
        let enabled = analytics_enabled();

        let (mode, json_text, copyable) = match &self.web_host.privacy.preview {
            WebPrivacyPreview::Loading => (
                None,
                "Loading live preview from gxserver...".to_string(),
                None,
            ),
            WebPrivacyPreview::Unsupported => (
                None,
                "The connected computer runs an older gxserver without the live preview. Update Ghostex on that computer to see the exact JSON; the switch above still works.".to_string(),
                None,
            ),
            WebPrivacyPreview::Failed(error) => (
                None,
                format!("Could not load live preview from gxserver: {error}"),
                None,
            ),
            WebPrivacyPreview::Loaded(value) => {
                let (mode, pretty) = formatted_preview(value);
                (Some(mode), pretty.clone(), Some(pretty))
            }
        };

        let button = |id: &'static str, label: &'static str, icon: Option<&'static str>| {
            h_flex()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .flex_none()
                .h(px(26.0))
                .px(px(10.0))
                .gap(px(6.0))
                .items_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .cursor_pointer()
                .hover(move |button| button.bg(hover))
                .text_size(px(12.0))
                .text_color(fg)
                .when_some(icon, |button, icon| {
                    button.child(titlebar_svg_icon(icon, 13.0, fg))
                })
                .child(label)
        };

        // The switch row: the desktop About page's compact toggle.
        let track = if enabled { fg } else { fg.opacity(0.22) };
        let knob = if enabled { surface } else { fg };
        let switch = div()
            .flex_none()
            .w(px(32.0))
            .h(px(18.0))
            .p(px(2.0))
            .rounded_full()
            .bg(track)
            .flex()
            .when(enabled, |switch| switch.justify_end())
            .child(div().size(px(14.0)).rounded_full().bg(knob));
        let toggle_row = h_flex()
            .id("web-privacy-analytics-switch")
            .role(gpui::Role::Switch)
            .aria_label(ANALYTICS_TOGGLE_TITLE)
            .aria_toggled(if enabled {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .w_full()
            .items_start()
            .gap(px(14.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .on_click(cx.listener(|app, _, _, cx| app.web_toggle_analytics(cx)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(fg)
                            .child(ANALYTICS_TOGGLE_TITLE),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(muted)
                            .child(ANALYTICS_TOGGLE_SUBTITLE),
                    ),
            )
            .child(switch);

        let scope_note = h_flex()
            .w_full()
            .items_start()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(8.0))
            .bg(code_bg)
            .border_1()
            .border_color(border)
            .child(titlebar_svg_icon(
                "modals/settings/shield-check.svg",
                15.0,
                fg,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(fg)
                    .child(WEB_SCOPE_NOTE),
            );

        let chips = NEVER_SENT_BADGES.iter().map(|label| {
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .bg(surface)
                .child(div().size(px(5.0)).rounded_full().flex_shrink_0().bg(muted))
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(fg)
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
            .border_color(border)
            .bg(code_bg)
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child("NEVER SENT"),
            )
            .child(h_flex().w_full().flex_wrap().gap(px(6.0)).children(chips));

        let json_box = div()
            .id("web-privacy-preview-json")
            .w_full()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(border)
            .bg(code_bg)
            .font_family(MONO_FONT)
            .text_size(px(11.5))
            .line_height(px(17.0))
            .text_color(fg)
            .child(SharedString::from(json_text));

        let mode_label = mode.clone().unwrap_or_else(|| "unknown".to_string());
        let preview_header = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(fg)
                                    .child("What Ghostex sends"),
                            )
                            .when(mode.is_some(), |row| {
                                row.child(
                                    div()
                                        .px(px(6.0))
                                        .rounded(px(4.0))
                                        .bg(code_bg)
                                        .border_1()
                                        .border_color(border)
                                        .font_family(MONO_FONT)
                                        .text_size(px(10.5))
                                        .text_color(muted)
                                        .child(SharedString::from(mode_label)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(muted)
                            .child(mode_description(mode.as_deref())),
                    ),
            );

        let copy_text = copyable.clone();
        let footer = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(
                h_flex()
                    .min_w_0()
                    .gap(px(10.0))
                    .items_center()
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(11.0))
                            .text_color(muted)
                            .child("POST /api/telemetryPreview"),
                    )
                    .child(
                        h_flex()
                            .id("web-privacy-analytics-docs")
                            .role(gpui::Role::Link)
                            .aria_label("ANALYTICS.md")
                            .gap(px(4.0))
                            .items_center()
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(fg)
                            .hover(|link| link.underline())
                            .child(titlebar_svg_icon(
                                "modals/settings/external-link.svg",
                                12.0,
                                fg,
                            ))
                            .child("ANALYTICS.md")
                            .on_click(cx.listener(|_, _, _, cx| {
                                gpui_open_external_url(ANALYTICS_DOCS_URL, cx)
                            })),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(8.0))
                    .child(
                        button(
                            "web-privacy-refresh",
                            "Refresh",
                            Some("modals/settings/refresh.svg"),
                        )
                        .on_click(cx.listener(|app, _, _, cx| app.web_refresh_privacy_preview(cx))),
                    )
                    .child(
                        button(
                            "web-privacy-copy",
                            "Copy JSON",
                            Some("modals/settings/copy.svg"),
                        )
                        .when(copy_text.is_none(), |button| {
                            button.opacity(0.45).cursor_not_allowed()
                        })
                        .on_click(cx.listener(move |_, _, _, cx| {
                            if let Some(text) = copy_text.clone() {
                                gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(text), cx);
                            }
                        })),
                    ),
            );

        let card = v_flex()
            .id("web-privacy-dialog")
            .role(gpui::Role::Dialog)
            .aria_label("Privacy & Analytics")
            .w(px(560.0))
            .max_w_full()
            .max_h(gpui::relative(0.92))
            .overflow_y_scroll()
            .p(px(18.0))
            .gap(px(12.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(border)
            .bg(surface)
            .shadow_lg()
            // A click inside the card stays in it.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(titlebar_svg_icon("modals/settings/shield.svg", 16.0, fg))
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child("Privacy & Analytics"),
                            ),
                    )
                    .child(
                        div()
                            .id("web-privacy-close")
                            .role(gpui::Role::Button)
                            .aria_label("Close")
                            .size(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(border)
                            .cursor_pointer()
                            .hover(move |button| button.bg(hover))
                            .child(titlebar_svg_icon("modals/settings/x.svg", 14.0, fg))
                            .on_click(cx.listener(|app, _, _, cx| app.web_close_privacy(cx))),
                    ),
            )
            .child(toggle_row)
            .child(scope_note)
            .child(div().h(px(1.0)).w_full().bg(border))
            .child(preview_header)
            .child(never_box)
            .child(json_box)
            .child(footer);

        Some(
            div()
                .id("web-privacy-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(0.35))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|app, _, _, cx| app.web_close_privacy(cx)),
                )
                .child(card)
                .into_any_element(),
        )
    }
}
