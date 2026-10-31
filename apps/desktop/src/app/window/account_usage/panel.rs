use super::super::resources_style::{
    RESOURCE_CONTROL_RADIUS, RESOURCE_PANEL_RADIUS, resource_header, resource_panel_frame,
};
use super::{data::*, style::*};
use crate::app::titlebar::account_usage::popup_account;
use crate::*;
use gpui::{SharedString, img};
use gpui_component::scroll::Scrollbar;
use serde_json::Value;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

pub(crate) struct AccountUsagePanel {
    pub(super) main_app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(super) id: ExtensionId,
    pub(super) account: Value,
    pub(super) palette: Palette,
    pub(super) models_open: bool,
    pub(super) resets_open: bool,
    /// The reset awaiting the inline "Use this reset?" confirmation, by `credit_keys` key.
    pub(super) confirming: Option<String>,
    pub(super) claiming: Option<String>,
    /// Resets claimed from this panel, hidden until the post-claim account refresh arrives.
    pub(super) claimed: HashSet<String>,
    /// The last claim's `(outcome, message)`, shown above the reset list.
    pub(super) banner: Option<(String, String)>,
    /// One idempotency key per reset, minted when it first enters confirmation and reused for every retry, so a retried claim can never spend a second reset.
    request_ids: HashMap<String, String>,
    pub(super) model_focus: FocusHandle,
    pub(super) reset_focus: FocusHandle,
    pub(super) confirm_focus: FocusHandle,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// The panel height last sent to the main window to fit the popup to.
    reported_height: Rc<Cell<f32>>,
    logo: Arc<gpui::Image>,
    pub(super) now: i64,
}

impl AccountUsagePanel {
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        id: ExtensionId,
        account: Value,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let codex = text(&account, "provider") == "codex";
        let logo = if codex {
            crate::app::titlebar::account_usage::account_logo_svg(true)
                .replace("#ffffff", "#8cbbe8")
        } else {
            crate::app::titlebar::account_usage::account_logo_svg(false).to_string()
        };
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.now = now();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            main_app,
            id,
            account: popup_account(&account),
            palette: Palette::new(codex),
            models_open: false,
            resets_open: false,
            confirming: None,
            claiming: None,
            claimed: HashSet::new(),
            banner: None,
            request_ids: HashMap::new(),
            model_focus: cx.focus_handle().tab_index(0),
            reset_focus: cx.focus_handle().tab_index(0),
            confirm_focus: cx.focus_handle().tab_index(0),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            reported_height: Rc::new(Cell::new(0.0)),
            logo: Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                logo.into_bytes(),
            )),
            now: now(),
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub(crate) fn update_account(&mut self, account: Value, cx: &mut gpui::Context<Self>) {
        self.account = popup_account(&account);
        self.now = now();
        if self.claiming.is_none() {
            self.claimed.clear();
        }
        if let Some(key) = &self.confirming
            && !credit_keys(&self.account).iter().any(|(k, _)| k == key)
        {
            self.confirming = None;
        }
        cx.notify();
    }

    pub(crate) fn dismiss_reset_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.resets_open {
            return false;
        }
        if self.claiming.is_some() {
            return true;
        }
        if self.confirming.take().is_some() {
            cx.notify();
            return true;
        }
        self.resets_open = false;
        self.reset_focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn toggle_models(&mut self, cx: &mut gpui::Context<Self>) {
        self.models_open = !self.models_open;
        self.resets_open = false;
        cx.notify();
    }

    pub(super) fn toggle_resets(&mut self, cx: &mut gpui::Context<Self>) {
        self.resets_open = !self.resets_open;
        cx.notify();
    }

    pub(super) fn confirm_reset(
        &mut self,
        key: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.claiming.is_some() || self.account["status"] != "ready" {
            return;
        }
        self.request_ids
            .entry(key.clone())
            .or_insert_with(|| uuid::Uuid::new_v4().to_string());
        self.confirming = Some(key);
        self.banner = None;
        self.confirm_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn cancel_reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.confirming = None;
        cx.notify();
    }

    pub(super) fn claim_reset(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(key) = self.confirming.take() else {
            return;
        };
        let (Some(credit_id), Some(request_id)) = (
            credit_keys(&self.account)
                .into_iter()
                .find(|(k, _)| *k == key)
                .map(|(_, credit)| text(credit, "id").to_string()),
            self.request_ids.get(&key).cloned(),
        ) else {
            cx.notify();
            return;
        };
        self.claiming = Some(key);
        cx.notify();
        let (id, app, panel) = (self.id, self.main_app.clone(), cx.entity().downgrade());
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                app.redeem_account_reset(id.as_str(), credit_id, request_id, panel, cx);
            });
        });
    }

    pub(crate) fn finish_claim(
        &mut self,
        outcome: &str,
        message: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(key) = self.claiming.take()
            && matches!(outcome, "success" | "noCredit")
        {
            self.request_ids.remove(&key);
            self.claimed.insert(key);
        }
        let message = if message.is_empty() {
            "Couldn't use the reset. Please try again.".to_string()
        } else {
            message
        };
        self.banner = Some((outcome.to_string(), message));
        cx.notify();
    }

    /// Notifications' title row: logo, "Claude usage" and the account badge, then the two link
    /// cells the panel's bottom buttons used to be, divided by the header's hairlines.
    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        let provider = if p.codex { "Codex" } else { "Claude" };
        let indicator = badge(&self.account);
        let links = if p.codex {
            [
                "https://status.openai.com",
                "https://chatgpt.com/codex/settings/usage",
            ]
        } else {
            [
                "https://status.anthropic.com",
                "https://claude.ai/settings/usage",
            ]
        };
        resource_header()
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .items_center()
                    .gap(px(7.0))
                    .pl(px(12.0))
                    .pr(px(8.0))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(chrome_ink().opacity(0.96))
                    .child(img(self.logo.clone()).flex_shrink_0().size(px(16.0)))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(format!("{provider} usage")),
                    )
                    .when_some(indicator, |this, indicator| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .h(px(16.0))
                                .px(px(5.0))
                                .rounded(px(RESOURCE_CONTROL_RADIUS - 1.0))
                                .border_1()
                                .border_color(chrome_ink().opacity(0.18))
                                .text_size(px(10.5))
                                .line_height(px(14.0))
                                .text_color(chrome_ink().opacity(0.80))
                                .child(indicator),
                        )
                    }),
            )
            .children(
                [
                    ("account-usage-status", "Status"),
                    ("account-usage-dashboard", "Dashboard"),
                ]
                .into_iter()
                .zip(links)
                .enumerate()
                .map(|(index, ((id, title), url))| {
                    // Notifications' header action cell, with its arrow after the label.
                    h_flex()
                        .id(id)
                        .tab_index(0)
                        .when(index == 1, |this| {
                            this.rounded_tr(px(RESOURCE_PANEL_RADIUS - 1.0))
                        })
                        .h_full()
                        .flex_shrink_0()
                        .px(px(11.0))
                        .items_center()
                        .gap(px(4.0))
                        .border_l_1()
                        .border_color(chrome_ink().opacity(0.12))
                        .whitespace_nowrap()
                        .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
                        .text_color(chrome_ink().opacity(0.78))
                        .cursor_pointer()
                        .hover(|this| {
                            this.bg(chrome_ink().opacity(0.14))
                                .text_color(chrome_ink().opacity(0.94))
                        })
                        .focus_visible(|this| this.bg(chrome_ink().opacity(0.14)))
                        .on_click(move |_, _, _| {
                            let _ = gpui_open_external_http_url(url);
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                let _ = gpui_open_external_http_url(url);
                            }
                        })
                        .child(title)
                        .child(titlebar_svg_icon(
                            "titlebar/arrow-up-right.svg",
                            TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE - 3.0,
                            chrome_ink().opacity(0.78).into(),
                        ))
                }),
            )
            .into_any_element()
    }

    /// The account's section label: "THIS ACCOUNT", its email when the badge doesn't already
    /// name it, and Live limits (Last known when stale) with the reading's time as its tooltip.
    fn render_account_label(&self) -> AnyElement {
        let p = self.palette;
        let stale = self.account["usageStale"] == true;
        let stamp = text(&self.account, "usageUpdatedAt");
        let updated = if stale {
            match age_text(stamp).as_str() {
                "" => String::new(),
                age => format!("Last updated {age} ago"),
            }
        } else {
            match local_date(stamp, false).as_str() {
                "" => String::new(),
                time => format!("Updated {time}"),
            }
        };
        // CDXC:AgentProviders 2026-10-11 DECISION:
        // User: the usage card must not contradict itself. When gxserver marks the reading stale (refreshing has failed since it was taken), the pill says "Last known" in amber and its tooltip says how long ago it was updated; "Live limits" shows only for a fresh reading.
        let (color, pill) = if stale {
            (amber(), "Last known")
        } else {
            (p.accent_text, "Live limits")
        };
        section_label(account_section_title(&self.account))
            .child(
                h_flex()
                    .id("account-usage-live")
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color)
                    .when(!updated.is_empty(), |this| {
                        this.tooltip(move |window, cx| {
                            Tooltip::new(updated.clone()).build(window, cx)
                        })
                    })
                    .child(div().size(px(6.0)).rounded_full().bg(color))
                    .child(pill),
            )
            .into_any_element()
    }

    /// Asks the main window to fit the popup to `height` once it changes.
    fn fit(&mut self, height: f32, cx: &mut gpui::Context<Self>) {
        let id = self.id;
        let _ = self.main_app.update(cx, |app, cx| {
            app.fit_account_usage_popup(id, height, cx);
        });
    }
}

/// The account's badge: its indicator, or its slot number when it has none; "-" hides it.
fn badge(account: &Value) -> Option<String> {
    let indicator = match text(account, "indicator") {
        "" => text(account, "selector"),
        value => value,
    };
    (!indicator.is_empty() && indicator != "-").then(|| indicator.to_string())
}

/// "THIS ACCOUNT", then " · " and the account's (masked) name unless its badge already shows it.
fn account_section_title(account: &Value) -> String {
    match (text(account, "displayName"), badge(account)) {
        ("", _) => "THIS ACCOUNT".to_string(),
        (name, Some(badge)) if badge == name => "THIS ACCOUNT".to_string(),
        (name, _) => format!("THIS ACCOUNT \u{b7} {name}"),
    }
}

/// Notifications' READ / UNREAD heading, with an optional right-hand part.
pub(super) fn section_label(title: impl Into<SharedString>) -> gpui::Div {
    h_flex()
        .w_full()
        .h(px(SECTION_LABEL_HEIGHT))
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .px(px(2.0))
        .pt(px(4.0))
        .pb(px(7.0))
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(chrome_ink().opacity(0.62))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.into()),
        )
}

pub(super) const SECTION_LABEL_HEIGHT: f32 = 24.0;
/// The Notifications body's padding.
const BODY_PADDING: f32 = 10.0;
const BODY_PADDING_TOP: f32 = 8.0;
/// Between the account section and the history section, like UNREAD and READ.
pub(super) const SECTION_GAP: f32 = 10.0;

impl Render for AccountUsagePanel {
    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User wants text in the usage dropdowns to be non-selectable.
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The theme and window glass can change while the panel is open.
        self.palette = Palette::new(self.palette.codex);
        let notice = match text(&self.account, "usageError") {
            "" => match text(&self.account, "status") {
                "loading" => "Loading account usage…",
                "ready" => "",
                _ => "Reconnect this account in Settings > Accounts.",
            },
            error => error,
        }
        .to_string();
        let panel = cx.entity().downgrade();
        let reported = self.reported_height.clone();
        let body = v_flex()
            .relative()
            .w_full()
            .p(px(BODY_PADDING))
            .pt(px(BODY_PADDING_TOP))
            .child(self.render_account_label())
            .child(self.render_limits(cx))
            .when(!notice.is_empty(), |this| this.child(notice_box(notice)))
            .child(self.render_history())
            // CDXC:AgentProviders 2026-10-30 DECISION:
            // User: the panel's height fits its content, up to what fits on screen, then it
            // scrolls: no empty gap under Codex, no cut-off buttons on Claude. The body reports
            // its laid-out height and the main window re-places the popup above the tile.
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        let height = TITLEBAR_POPUP_READING_HEADER_HEIGHT
                            + bounds.size.height.as_f32()
                            + 2.0;
                        if (reported.get() - height).abs() > 0.5 {
                            reported.set(height);
                            let panel = panel.clone();
                            cx.defer(move |cx| {
                                let _ = panel.update(cx, |panel, cx| panel.fit(height, cx));
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        resource_panel_frame().child(
            v_flex()
                .id("account-usage-panel")
                .size_full()
                .overflow_hidden()
                .font_family(if cfg!(target_os = "macos") {
                    ".AppleSystemUIFont"
                } else {
                    "Segoe UI"
                })
                .font_weight(FontWeight::NORMAL)
                .font_smoothing(false)
                .text_size(px(12.0))
                .line_height(px(17.4))
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "tab" {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                    } else {
                        let page = window.viewport_size().height * 0.875;
                        let mut offset = this.scroll.offset();
                        match event.keystroke.key.as_str() {
                            "up" => offset.y += px(40.0),
                            "down" => offset.y -= px(40.0),
                            "pageup" => offset.y += page,
                            "pagedown" | "space" => {
                                offset.y -= if event.keystroke.modifiers.shift {
                                    -page
                                } else {
                                    page
                                }
                            }
                            "home" => offset.y = px(0.0),
                            "end" => offset.y = -this.scroll.max_offset().y,
                            _ => return,
                        }
                        offset.y = offset.y.clamp(-this.scroll.max_offset().y, px(0.0));
                        this.scroll.set_offset(offset);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.resets_open && this.confirming.is_none() && this.claiming.is_none() {
                        this.resets_open = false;
                        cx.notify();
                    }
                }))
                .child(self.render_header())
                .child(
                    div()
                        .relative()
                        .w_full()
                        .min_h_0()
                        .flex_1()
                        .child(
                            div()
                                .id("account-usage-scroll")
                                .size_full()
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .child(body),
                        )
                        .child(
                            Scrollbar::vertical(&self.scroll)
                                .thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH)),
                        ),
                ),
        )
    }
}

/// The amber line under the account's card: a usage error, loading, or reconnect. Notifications'
/// offline notice.
fn notice_box(notice: String) -> AnyElement {
    let color = amber();
    div()
        .w_full()
        .mt(px(NOTICE_GAP))
        .px(px(9.0))
        .py(px(7.0))
        .rounded(px(RESOURCE_CONTROL_RADIUS))
        .border_1()
        .border_color(color.opacity(0.30))
        .bg(color.opacity(0.10))
        .text_size(px(12.0))
        .line_height(px(NOTICE_LINE))
        .text_color(color.opacity(0.85))
        .child(notice)
        .into_any_element()
}

const NOTICE_GAP: f32 = 8.0;
const NOTICE_LINE: f32 = 17.0;

/// The panel's height before it has drawn, from the same rows it draws, so the popup opens at
/// about the right size; the body's measured height then fits it exactly (fine print or a notice
/// that wraps, an open reset list).
pub(crate) fn estimated_height(account: &Value) -> f32 {
    use super::history::*;
    use super::limits::*;
    let account = popup_account(account);
    let codex = text(&account, "provider") == "codex";
    let windows = array(&account, "usage");
    let scoped = windows.iter().any(|w| !text(w, "model").is_empty());
    let bars = 2 + usize::from(!codex && scoped);
    let shown_others = windows
        .iter()
        .filter(|w| w["id"] != "spend")
        .count()
        .saturating_sub(bars);
    let card_chrome = CARD_PADDING_Y * 2.0 + 2.0;
    let mut limits = bars as f32 * (LIMIT_LINE + LIMIT_BAR_GAP + LIMIT_BAR_HEIGHT)
        + (bars - 1) as f32 * LIMIT_GAP;
    if shown_others > 0 {
        limits += LIMIT_GAP + 22.0;
    }
    let resets = codex
        || account["resetCredits"].is_u64()
        || !text(&account, "resetCreditsError").is_empty();
    let rows = usize::from(!codex) + usize::from(resets);
    if rows > 0 {
        limits += 1.0 + SEPARATOR_MARGIN * 2.0 + rows as f32 * KV_ROW_HEIGHT;
    }
    let notice = !text(&account, "usageError").is_empty() || account["status"] != "ready";
    let history = &account["usageHistory"];
    // The same cases `render_history` writes its notice for.
    let history_notice = history.is_null()
        || ["loading", "unavailable", "partial"].contains(&text(history, "status"))
        || history["hasData"] != true;
    let history_card = CHART_HEIGHT
        + 4.0
        + AXIS_LINE
        + STATS_GAP
        + STAT_LABEL_LINE
        + 1.0
        + STAT_VALUE_LINE
        + FINE_PRINT_GAP
        + FINE_PRINT_LINE
        + if history_notice {
            FINE_PRINT_GAP + NOTICE_LINE
        } else {
            0.0
        };
    TITLEBAR_POPUP_READING_HEADER_HEIGHT
        + 2.0
        + BODY_PADDING_TOP
        + BODY_PADDING
        + SECTION_LABEL_HEIGHT
        + card_chrome
        + limits
        + if notice {
            NOTICE_GAP + 14.0 + 2.0 + NOTICE_LINE
        } else {
            0.0
        }
        + SECTION_GAP
        + SECTION_LABEL_HEIGHT
        + card_chrome
        + history_card
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude() -> Value {
        json!({
            "provider": "claude", "indicator": "ST", "name": "ST", "status": "ready",
            "usage": [
                {"id": "fiveHour", "usedPercent": 1, "limitWindowSeconds": 18000},
                {"id": "sevenDay", "usedPercent": 76, "limitWindowSeconds": 604800},
                {"id": "sevenDayFable", "model": "Fable", "usedPercent": 0},
                {"id": "spend", "usedPercent": null},
            ],
            "usageHistory": {"status": "complete", "hasData": true, "days": []},
        })
    }

    fn codex() -> Value {
        json!({
            "provider": "codex", "indicator": "SP", "name": "yahia@shortpoint.com",
            "email": "yahia@shortpoint.com", "status": "ready", "resetCredits": 1,
            "usage": [
                {"id": "fiveHour", "usedPercent": null, "limitWindowSeconds": 18000},
                {"id": "sevenDay", "usedPercent": 0, "limitWindowSeconds": 604800},
            ],
            "usageHistory": {"status": "complete", "hasData": true, "days": []},
        })
    }

    #[test]
    fn the_account_label_adds_the_name_only_when_the_badge_does_not_show_it() {
        let named = json!({"displayName": "ST", "indicator": "ST"});
        assert_eq!(account_section_title(&named), "THIS ACCOUNT");
        let email = json!({"displayName": "y•••a@•••••.•••", "indicator": "SP"});
        assert_eq!(
            account_section_title(&email),
            "THIS ACCOUNT · y•••a@•••••.•••"
        );
        let slot = json!({"displayName": "MS", "selector": "4"});
        assert_eq!(account_section_title(&slot), "THIS ACCOUNT · MS");
        let hidden = json!({"displayName": "", "indicator": "-"});
        assert_eq!(account_section_title(&hidden), "THIS ACCOUNT");
    }

    #[test]
    fn the_fine_print_carries_the_tokens_unit() {
        assert_eq!(
            super::super::history::fine_print("+04:00"),
            "Tokens from conversation logs · Cached tokens included · Days in +04:00"
        );
        assert_eq!(
            super::super::history::fine_print(""),
            "Tokens from conversation logs · Cached tokens included"
        );
    }

    /// Codex has two limits and no Extra usage row, so it opens shorter than Claude instead of
    /// both taking a fixed 640px; a notice or a history warning adds its line.
    #[test]
    fn the_opening_height_follows_the_rows_the_panel_draws() {
        let claude_height = estimated_height(&claude());
        let codex_height = estimated_height(&codex());
        assert!(
            codex_height < claude_height,
            "{codex_height} < {claude_height}"
        );
        assert!(claude_height < 640.0, "{claude_height}");
        // Claude: one more limit (Fable) and the Extra usage row; Codex has its resets row.
        let limit = super::super::limits::LIMIT_LINE
            + super::super::limits::LIMIT_BAR_GAP
            + super::super::limits::LIMIT_BAR_HEIGHT
            + super::super::limits::LIMIT_GAP;
        assert_eq!(claude_height - codex_height, limit);
        let mut failing = codex();
        failing["usageError"] = json!("Could not read usage.");
        assert!(estimated_height(&failing) > codex_height);
        let mut loading = codex();
        loading["usageHistory"] = json!({"status": "loading"});
        assert!(estimated_height(&loading) > codex_height);
    }
}
