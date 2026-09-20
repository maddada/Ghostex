use crate::*;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement as _;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;
use serde_json::{Value, json};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn titlebar_entry(account: &Value, machine: &str) -> Value {
    let mut account = account.clone();
    let key = machine
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    account["titlebarKey"] = json!(format!("account-{key}-{}", text(&account, "id")));
    account["titlebarMachine"] = json!(machine);
    account
}

/// CDXC:AgentProviders 2026-09-09 WHY:
/// The Codex artwork matches the shared chat SVG; the old 100px SVG had internal padding that made its titlebar icon visibly smaller than Claude.
fn icon(codex: bool) -> Arc<gpui::Image> {
    static CODEX: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    static CLAUDE: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    static CODEX_LIGHT: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    static CLAUDE_LIGHT: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
    let (slot, svg) = if codex {
        (
            if light { &CODEX_LIGHT } else { &CODEX },
            include_str!("../../../assets/account-usage/codex.svg"),
        )
    } else {
        (
            if light { &CLAUDE_LIGHT } else { &CLAUDE },
            include_str!("../../../assets/account-usage/claude.svg"),
        )
    };
    slot.get_or_init(|| {
        let svg = if light {
            if codex {
                svg.replace("#ffffff", "#285b8c")
            } else {
                svg.replace("#d97757", "#9c4328")
            }
        } else {
            svg.to_string()
        };
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            svg.into_bytes(),
        ))
    })
    .clone()
}

/// The account's name as the user wants it shown: masked when Settings hides
/// account emails. Shared by the titlebar account popup and the New Thread picker.
pub(crate) fn account_display_name(account: &Value) -> String {
    account_display_text(text(account, "name"))
}

/// CDXC:AgentProviders 2026-09-10 WHY: Account errors can include email addresses too; mask each address while preserving the surrounding explanation in native pickers and usage popups.
pub(crate) fn account_display_text(value: &str) -> String {
    let hidden = shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("hideAccountEmails")
        .and_then(Value::as_bool)
        == Some(true);
    if !hidden {
        return value.to_string();
    }
    value
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let token = part.trim_end_matches(char::is_whitespace);
            let whitespace = &part[token.len()..];
            match token.split_once('@') {
                Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
                    let mut characters = local.chars();
                    let first = characters.next().unwrap();
                    let last = characters.last().map(|c| c.to_string()).unwrap_or_default();
                    format!("{first}•••{last}@•••••.•••{whitespace}")
                }
                _ => part.to_string(),
            }
        })
        .collect()
}

pub(crate) fn popup_account(account: &Value) -> Value {
    let mut account = account.clone();
    account["displayName"] = json!(account_display_name(&account));
    if let Some(error) = account["usageError"].as_str() {
        account["usageError"] = json!(account_display_text(error));
    }
    account
}

fn badge_lines(account: &Value) -> Vec<String> {
    let windows = account["usage"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let pct = |w: &Value| w["usedPercent"].as_f64().map(|v| format!("{:.0}", v));
    if text(account, "provider") == "codex" {
        let main: Vec<_> = windows.iter().filter(|w| w["model"].is_null()).collect();
        let session = main
            .iter()
            .find(|w| w["limitWindowSeconds"].as_i64() == Some(18000));
        let weekly = main.iter().find(|w| {
            w["limitWindowSeconds"]
                .as_i64()
                .is_some_and(|s| s >= 604800)
        });
        let usage = [session, weekly]
            .into_iter()
            .flatten()
            .filter_map(|w| pct(w))
            .collect::<Vec<_>>();
        let mut lines = Vec::new();
        if !usage.is_empty() {
            lines.push(format!("{}%", usage.join("/")));
        }
        if let Some(resets) = account["resetCredits"].as_u64() {
            lines.push(format!("{resets}rs"));
        }
        lines
    } else {
        claude_headline_windows(windows)
            .into_iter()
            .filter_map(|w| pct(w).map(|v| format!("{v}%")))
            .collect()
    }
}

/// How close an account is to running out, as the highest used percentage among the
/// same windows its two badge numbers come from. The collapsed sidebar strip shows
/// only as many meters as fit, so it orders them by this and drops the coolest.
fn account_pressure(account: &Value) -> f64 {
    let windows = account["usage"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let percent = |window: &Value| window["usedPercent"].as_f64().unwrap_or(0.0);
    let headline: Vec<&Value> = if text(account, "provider") == "codex" {
        windows
            .iter()
            .filter(|window| window["model"].is_null())
            .filter(|window| {
                window["limitWindowSeconds"].as_i64() == Some(18000)
                    || window["limitWindowSeconds"]
                        .as_i64()
                        .is_some_and(|seconds| seconds >= 604800)
            })
            .collect()
    } else {
        claude_headline_windows(windows)
    };
    headline
        .into_iter()
        .map(percent)
        .fold(0.0_f64, |highest, value| highest.max(value))
}

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: for Claude accounts the Fable limit is the most important number and must never be hidden. Wherever a Claude account shows two percentages, show the two tightest of the weekly, five-hour, and Fable limits, in that fixed order, so the number about to run out is always one of them. Port of `accountHeadlineWindows` in packages/shared/account-usage-windows.ts; the popup in apps/desktop/src/app/window/account_usage/limits.rs shows all three as main bars.
pub(crate) fn claude_headline_windows(windows: &[Value]) -> Vec<&Value> {
    let main: Vec<&Value> = windows.iter().filter(|w| w["model"].is_null()).collect();
    let weekly = main.iter().copied().find(|w| {
        text(w, "id") == "sevenDay" || w["limitWindowSeconds"].as_i64().unwrap_or(0) >= 604_800
    });
    let five_hour = main
        .iter()
        .copied()
        .find(|w| text(w, "id") == "fiveHour" || w["limitWindowSeconds"].as_i64() == Some(18_000));
    let scoped: Vec<&Value> = windows.iter().filter(|w| w["model"].is_string()).collect();
    let fable = scoped
        .iter()
        .copied()
        .find(|w| text(w, "model").to_lowercase().contains("fable"))
        .or_else(|| scoped.first().copied());
    let candidates: Vec<&Value> = [weekly, five_hour, fable].into_iter().flatten().collect();
    let mut by_usage = candidates.clone();
    by_usage.sort_by(|a, b| {
        b["usedPercent"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["usedPercent"].as_f64().unwrap_or(0.))
    });
    let tightest: Vec<*const Value> = by_usage
        .iter()
        .take(2)
        .map(|w| *w as *const Value)
        .collect();
    candidates
        .into_iter()
        .filter(|w| tightest.contains(&(*w as *const Value)))
        .collect()
}

/// One account's meter: the provider glyph with the account label on it and the two
/// tightest numbers beside it, without any of the chrome its host puts around it.
pub(crate) struct GpuiAccountUsageMeter {
    pub(crate) id: ExtensionId,
    /// The tooltip, already masked when Settings hides account emails.
    pub(crate) title: String,
    pub(crate) codex: bool,
    pub(crate) indicator: Option<String>,
    pub(crate) badge_lines: Vec<String>,
    pub(crate) pressure: f64,
}

/// The chrome the hosting surface wants around a meter. The meter renderer takes this
/// rather than reading the titlebar constants, so one implementation serves every host.
pub(crate) struct GpuiAccountUsageMeterHost {
    pub(crate) element_id_prefix: &'static str,
    pub(crate) anchor_key_prefix: &'static str,
    pub(crate) height: f32,
    pub(crate) padding_x: f32,
    pub(crate) corner_radius: f32,
    /// The meter's resting fill. Hosts that want the meters to read as cards pass a
    /// visible one; a transparent value leaves the meter on the host's own background.
    pub(crate) background: gpui::Hsla,
    pub(crate) hover_background: gpui::Hsla,
    /// The fill for the meter whose usage popup is open. It must stay distinct from the
    /// host's own hovered background rather than being swallowed by it.
    pub(crate) open_background: gpui::Hsla,
    /// Whether the meter fills the cell it is given instead of hugging its content. The
    /// meter centres its content either way, so a filled cell centres it in the column.
    pub(crate) fill_width: bool,
    pub(crate) tooltip_placement: ManagedTooltipPlacement,
    pub(crate) tooltip_delay: Duration,
    pub(crate) scale: f32,
}

impl GhostexGpuiApp {
    pub(crate) fn sync_titlebar_account_privacy(&self, cx: &mut gpui::Context<Self>) {
        let Some(GpuiTitlebarPopupKind::AccountUsage(id)) =
            self.titlebar_popup_menu.as_ref().map(|state| state.kind)
        else {
            return;
        };
        if let Some(account) = self
            .titlebar_accounts
            .iter()
            .find(|account| text(account, "titlebarKey") == id.as_str())
            && let Some(popup) = self.titlebar_popup_window
        {
            let account = popup_account(account);
            let _ = popup.update(cx, |popup, _, cx| popup.update_account_usage(account, cx));
        }
    }

    pub(crate) fn update_titlebar_account_from_ui(
        &mut self,
        message: &Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = text(message, "machineId");
        let account = &message["account"];
        if machine != "local" && !self.remote_gxserver_connections.contains_key(machine) {
            return;
        }
        if !matches!(text(account, "provider"), "claude" | "codex") || account["registered"] != true
        {
            return;
        }
        let account = titlebar_entry(account, machine);
        let Some(id) = ExtensionId::new(text(&account, "titlebarKey")) else {
            return;
        };
        self.titlebar_accounts_revision = self.titlebar_accounts_revision.wrapping_add(1);
        self.titlebar_accounts
            .retain(|a| text(a, "titlebarKey") != id.as_str());
        if account["showInTitlebar"] == true {
            self.titlebar_accounts.push(account);
        } else if self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::AccountUsage(id)) {
            self.close_gpui_titlebar_popup(None, window, cx);
        }
        self.refresh_titlebar_accounts(cx);
        cx.notify();
    }

    pub(crate) fn start_titlebar_account_polling(&mut self, cx: &mut gpui::Context<Self>) {
        self.refresh_titlebar_accounts(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this
                    .update(cx, |this, cx| this.refresh_titlebar_accounts(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// CDXC:AgentProviders 2026-09-09 WHY:
    /// Show saved identities before polling usage, and publish each machine independently so a slow helper or offline remote cannot hide local accounts at startup.
    /// Usage still comes from gxserver's shared discovery cache.
    pub(crate) fn refresh_titlebar_accounts(&mut self, cx: &mut gpui::Context<Self>) {
        let revision = self.titlebar_accounts_revision;
        let mut targets = vec![("local".to_string(), None)];
        targets.extend(
            self.remote_gxserver_connections
                .iter()
                .map(|(id, connection)| (id.clone(), Some(connection.request_target()))),
        );
        for (machine, target) in targets {
            if !self
                .titlebar_accounts_refresh_in_flight
                .insert(machine.clone())
            {
                continue;
            }
            cx.spawn(async move |this, cx| {
                for cached_only in [true, false] {
                    let target = target.clone();
                    let result = cx.background_executor().spawn(async move {
                        let params = json!({"operation":"titlebar", "cachedOnly":cached_only});
                        match target {
                            Some(target) => gpui_remote_gxserver_rpc_result(&target, "/api/agentAccounts", &params, Duration::from_secs(60)),
                            None => gpui_gxserver_rpc_result("/api/agentAccounts", &params, Duration::from_secs(60)),
                        }
                    }).await;
                    if this.update_in(cx, |this, window, cx| {
                        if this.titlebar_accounts_revision != revision {
                            return;
                        }
                        match result {
                            Ok(result) => {
                                this.titlebar_accounts.retain(|a| text(a, "titlebarMachine") != machine);
                                for account in result["accounts"].as_array().into_iter().flatten()
                                    .filter(|a| a["registered"] == true && a["showInTitlebar"] == true) {
                                    let mut entry = titlebar_entry(account, &machine);
                                    entry["usageHistory"] = result.get("usageHistory")
                                        .map(|history| history[text(account, "provider")].clone())
                                        .unwrap_or_else(|| json!({"status":"unavailable"}));
                                    entry["providerAccountCount"] = result["accountCounts"][text(account, "provider")].clone();
                                    this.titlebar_accounts.push(entry);
                                }
                            }
                            Err(_) => {
                                for account in this.titlebar_accounts.iter_mut().filter(|a| text(a, "titlebarMachine") == machine) {
                                    account["usageError"] = json!("Could not refresh account usage. Showing the last received snapshot.");
                                }
                            }
                        }
                        this.titlebar_accounts.sort_by(|a, b| text(a,"titlebarMachine").cmp(text(b,"titlebarMachine"))
                            .then(text(a,"provider").cmp(text(b,"provider")))
                            .then(text(a,"selector").parse::<u64>().unwrap_or(0).cmp(&text(b,"selector").parse::<u64>().unwrap_or(0))));
                        if let Some(GpuiTitlebarPopupKind::AccountUsage(id)) = this.titlebar_popup_menu.as_ref().map(|state| state.kind) {
                            if this.titlebar_accounts.iter().any(|a| text(a, "titlebarKey") == id.as_str()) {
                                this.sync_titlebar_account_privacy(cx);
                            } else { this.close_gpui_titlebar_popup(None, window, cx); }
                        }
                        cx.notify();
                    }).is_err() {
                        return;
                    }
                }
                let _ = this.update(cx, |this, cx| {
                    this.titlebar_accounts_refresh_in_flight.remove(&machine);
                    if this.titlebar_accounts_revision != revision {
                        this.refresh_titlebar_accounts(cx);
                    }
                });
            }).detach();
        }
    }

    /// Every account the user asked to see, in the order gxserver publishes them.
    pub(crate) fn account_usage_meters(&self) -> Vec<GpuiAccountUsageMeter> {
        self.titlebar_accounts
            .iter()
            .filter_map(|account| {
                let id = ExtensionId::new(text(account, "titlebarKey"))?;
                let codex = text(account, "provider") == "codex";
                let indicator = match text(account, "indicator") {
                    "" => text(account, "selector"),
                    value => value,
                };
                Some(GpuiAccountUsageMeter {
                    id,
                    title: format!(
                        "{} Usage · {}",
                        if codex { "Codex" } else { "Claude" },
                        text(&popup_account(account), "displayName")
                    ),
                    codex,
                    indicator: (!indicator.is_empty() && indicator != "-")
                        .then(|| indicator.to_string()),
                    badge_lines: badge_lines(account),
                    pressure: account_pressure(account),
                })
            })
            .collect()
    }

    /// CDXC:AgentProviders 2026-09-20 DECISION:
    /// User: the account usage meters live at the bottom of the sidebar, not in the titlebar; this supersedes the 2026-09-13 rule that placed them before the extension buttons in the titlebar strip.
    /// Their shape is unchanged: darker Claude/Codex colors in light mode, the account label centered over a 19.2px background icon, a 9.9px label and 9.5px usage text in the chat indicator's monospace font, and the usage percentages or reset counts beside it. A meter still opens its account's usage popup.
    /// The host owns the geometry so the meter has one implementation wherever it is drawn.
    pub(crate) fn render_account_usage_meter(
        &self,
        meter: &GpuiAccountUsageMeter,
        host: &GpuiAccountUsageMeterHost,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let account_id = meter.id;
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::AccountUsage(account_id));
        let anchor_state = window.use_keyed_state(
            format!("{}-{}", host.anchor_key_prefix, account_id.as_str()),
            cx,
            |_, _| GpuiTitlebarPopupAnchorState::default(),
        );
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds = anchor_state
            .read(cx)
            .trigger_bounds_captured
            .then_some(anchor_bounds);
        let badge_lines = meter
            .badge_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        let show_badge = !badge_lines.is_empty();
        let scale = host.scale;
        let icon_image = icon(meter.codex);
        let indicator = meter.indicator.clone();
        let indicator_color = if meter.codex {
            chrome_color(0x7db8fb, 0x285b8c)
        } else {
            chrome_color(0xa4a8af, 0x9c4328)
        };
        let tooltip = meter.title.clone();
        let tooltip_placement = host.tooltip_placement;
        let tooltip_delay = host.tooltip_delay;
        let hover_background = host.hover_background;
        let open_background = host.open_background;

        let glyph = |size: f32| {
            let size = if indicator.is_some() { 19.2 } else { size } * scale;
            div()
                .relative()
                .size(px(size))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .child(
                    img(icon_image.clone())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(size))
                        .when(indicator.is_some(), |this| this.opacity(0.3)),
                )
                .when_some(indicator.clone(), |this, indicator| {
                    this.child(
                        div()
                            .relative()
                            .text_color(indicator_color)
                            .text_size(px(9.9 * scale))
                            .line_height(px(9.9 * scale))
                            .font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_center()
                            .child(indicator),
                    )
                })
        };

        div()
            .id(format!(
                "{}-{}",
                host.element_id_prefix,
                account_id.as_str()
            ))
            .when(!host.fill_width, |this| this.flex_shrink_0())
            .when(host.fill_width, |this| {
                this.w_full().min_w_0().overflow_hidden()
            })
            .relative()
            .flex()
            .h(px(host.height))
            .px(px(host.padding_x))
            .rounded(px(host.corner_radius))
            .items_center()
            .justify_center()
            .cursor_default()
            .bg(host.background)
            .when(open, |this| this.bg(open_background))
            .hover(move |this| {
                if open {
                    this.bg(open_background)
                } else {
                    this.bg(hover_background)
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    let Some(trigger_bounds) = trigger_bounds else {
                        window.request_animation_frame();
                        return;
                    };
                    this.open_titlebar_account_usage(account_id, trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_account_menu(event.position, window, cx);
                }),
            )
            /*
            CDXC:AgentProviders 2026-09-20 WHY:
            A press that lands on a meter opens that account's usage popup and then
            swallows its click, so the sidebar's double-click-to-create-session cannot
            fire on a meter. The mouse-down still bubbles, so the sidebar menu closes as
            it does for every other sidebar click.
            */
            .on_click(cx.listener(|_, _: &gpui::ClickEvent, _, cx| cx.stop_propagation()))
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    tooltip_placement,
                    tooltip_delay,
                    move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let request_frame = anchor_state.update(cx, |state, _| {
                        let changed = !state.trigger_bounds_captured || state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        changed
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .map(|this| {
                if show_badge {
                    this.child(
                        h_flex().gap(px(4.0 * scale)).child(glyph(14.0)).child(
                            v_flex()
                                .text_size(px(9.5 * scale))
                                .line_height(px(9.5 * scale))
                                .font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(chrome_color(0xb9b9b9, 0x404040))
                                .children(badge_lines),
                        ),
                    )
                } else {
                    this.child(glyph(18.0))
                }
            })
            .into_any_element()
    }

    /// CDXC:Titlebar 2026-09-12 DECISION:
    /// User: clicking an open usage button again closes its dropdown; clicking away, including in Session Chat, dismisses Usage and Tips.
    pub(crate) fn open_titlebar_account_usage(
        &mut self,
        id: ExtensionId,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiTitlebarPopupKind::AccountUsage(id);
        if !self
            .titlebar_accounts
            .iter()
            .any(|account| text(account, "titlebarKey") == id.as_str())
        {
            return;
        }
        let opening = !self.titlebar_popup_menu_open(kind);
        let previous_focus = window.focused(cx);
        self.set_gpui_titlebar_popup_open(kind, opening, Some(trigger_bounds), window, cx);
        if opening {
            self.titlebar_dropdown_previous_focus_handle = previous_focus;
            self.titlebar_dropdown_focus_handle.focus(window, cx);
            #[cfg(target_os = "macos")]
            {
                self.begin_programmatic_focus();
                unsafe extern "C" {
                    fn GhostexGpuiBeginUsageKeyboardFocus(view: *mut std::ffi::c_void);
                }
                unsafe { GhostexGpuiBeginUsageKeyboardFocus(self.parent_ns_view) };
                // CDXC:FocusRouting 2026-09-12 WHY:
                // Leaving this guard active suppresses later real CEF clicks, preventing chat clicks from dismissing Usage and Tips.
                self.end_programmatic_focus();
            }
            #[cfg(target_os = "windows")]
            cef::focus_gpui_root_view(self.parent_ns_view);
        }
    }

    /// CDXC:FocusRouting 2026-09-14 WHY:
    /// Restoring chat focus when Usage closes queues a responder event that can arrive after another dropdown opens and dismiss it as an outside click. Mark only the restoration as programmatic so real outside clicks still dismiss dropdowns.
    pub(crate) fn restore_account_usage_keyboard_focus(&mut self) {
        #[cfg(target_os = "macos")]
        {
            unsafe extern "C" {
                fn GhostexGpuiEndUsageKeyboardFocus(view: *mut std::ffi::c_void);
            }
            self.begin_programmatic_focus();
            unsafe { GhostexGpuiEndUsageKeyboardFocus(self.parent_ns_view) };
            self.end_programmatic_focus();
        }
    }

    /// CDXC:AgentProviders 2026-09-12 WHY:
    /// Native titlebar panels keep the main window active; route usage keyboard controls to their owning popup so Tab and Space cannot operate the underlying chat.
    pub(crate) fn forward_account_usage_key(
        &self,
        event: &gpui::KeyDownEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform
            || modifiers.control
            || modifiers.alt
            || !matches!(
                event.keystroke.key.as_str(),
                "tab" | "enter" | "space" | "up" | "down" | "pageup" | "pagedown" | "home" | "end"
            )
        {
            return;
        }
        if let Some(popup) = self.titlebar_popup_window {
            let event = event.clone();
            let _ = popup.update(cx, |_, window, cx| {
                window.dispatch_event(gpui::PlatformInput::KeyDown(event), cx);
            });
            cx.stop_propagation();
        }
    }
}
