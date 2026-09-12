use super::extension_buttons::TitlebarBadgeButton;
use crate::*;
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
    let (slot, bytes): (_, &[u8]) = if codex {
        (
            &CODEX,
            include_bytes!("../../../assets/account-usage/codex.svg"),
        )
    } else {
        (
            &CLAUDE,
            include_bytes!("../../../assets/account-usage/claude.svg"),
        )
    };
    slot.get_or_init(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            bytes.to_vec(),
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

    /// CDXC:AgentProviders 2026-09-09 DECISION:
    /// User wants the account label centered over a larger agent icon, keeping its font and Claude color, with Codex text #7db8fb. Always use original provider colors. Use a 19.2px background icon, 9.9px label, and 9.5px usage text in the chat indicator’s monospace font. Keep usage percentages and reset counts beside it. This replaces the label-underneath design.
    /// Account buttons still precede extensions and open their usage popup.
    pub(crate) fn render_titlebar_account_buttons(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        self.titlebar_accounts
            .iter()
            .filter_map(|account| {
                let id = ExtensionId::new(text(account, "titlebarKey"))?;
                let codex = text(account, "provider") == "codex";
                let indicator = match text(account, "indicator") {
                    "" => text(account, "selector"),
                    value => value,
                };
                Some(
                    self.render_titlebar_badge_button(
                        TitlebarBadgeButton {
                            id,
                            title: format!(
                                "{} Usage · {}",
                                if codex { "Codex" } else { "Claude" },
                                text(&popup_account(account), "displayName")
                            ),
                            icon_image: icon(codex),
                            badge_lines: badge_lines(account),
                            indicator_color: if codex {
                                gpui::rgb(0x7db8fb)
                            } else {
                                gpui::rgb(0xa4a8af)
                            },
                            indicator: (!indicator.is_empty() && indicator != "-")
                                .then(|| indicator.to_string()),
                            account: true,
                        },
                        window,
                        cx,
                    ),
                )
            })
            .collect()
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

    pub(crate) fn restore_account_usage_keyboard_focus(&self) {
        #[cfg(target_os = "macos")]
        {
            unsafe extern "C" {
                fn GhostexGpuiEndUsageKeyboardFocus(view: *mut std::ffi::c_void);
            }
            unsafe { GhostexGpuiEndUsageKeyboardFocus(self.parent_ns_view) };
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
