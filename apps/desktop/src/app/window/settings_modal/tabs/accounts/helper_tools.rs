//! `useAccountHelperTools` and `AccountHelperToolRow` (accounts/helper-tools.tsx (deleted 2026-10-01)): Claude Swap and
//! Codex Swap as installed on this computer, one quiet line each under the account manager: its
//! version and Update (or Check for updates), a ⋯ menu with Reinstall and Uninstall whose items
//! carry the versions, a confirmation before Uninstall, a poll while one runs, and its result as
//! a toast.
//!
//! CDXC:AgentProviders 2026-10-14 DECISION:
//! User picked "Claude Swap / Codex Swap as one quiet line at the end": each helper is one line under the account manager with its version, an Update button when a newer release exists (otherwise Check for updates), and a ⋯ menu holding Reinstall and Uninstall. Uninstall still asks first. Supersedes the 2026-09-28 row of three icon buttons.
//! SEE-ALSO: server/src/accounts/helper_tools.rs, the Trycua row (Computer & browser use) in tabs/integrations.rs.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ListItemStatus, SizedButtonSize, SizedButtonVariant, settings_list_item, settings_sized_button,
    settings_square_button, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::AccountsTab;
use super::data::{HelperTool, action_words, helper_command, helper_label};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, anchored, deferred, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

/// `setInterval(load, 2000)` while an action runs.
const RUNNING_POLL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct HelperToolsState {
    pub(crate) tools: Vec<HelperTool>,
    pub(crate) checking: bool,
    /// The last `finishedAt` announced per provider.
    seen_finish: HashMap<String, String>,
    pub(crate) poll: Option<Task<()>>,
    /// The provider whose Uninstall confirmation shows.
    pub(crate) confirm_uninstall: Option<String>,
    /// The provider whose ⋯ menu is open.
    pub(crate) menu: Option<String>,
}

impl AccountsTab {
    fn helper_toast(&self, level: &str, title: &str, description: &str, cx: &mut gpui::App) {
        self.store
            .update(cx, |store, cx| store.toast(level, title, description, cx));
    }

    /// `apply(next, announce)`: stores the tools and toasts each newly finished action.
    fn apply_helper_tools(
        &mut self,
        next: Vec<HelperTool>,
        announce: bool,
        cx: &mut Context<Self>,
    ) {
        let mut finished = false;
        for tool in &next {
            let Some(job) = tool.job() else {
                continue;
            };
            let Some(finished_at) = job["finishedAt"].as_str().filter(|at| !at.is_empty()) else {
                continue;
            };
            let provider = tool.provider();
            if self.helpers.seen_finish.get(&provider).map(String::as_str) == Some(finished_at) {
                continue;
            }
            self.helpers
                .seen_finish
                .insert(provider.clone(), finished_at.to_string());
            if !announce {
                continue;
            }
            finished = true;
            let name = helper_label(&provider);
            let (_, done, failed) = action_words(job["action"].as_str().unwrap_or_default());
            if job["status"] == "complete" {
                if let Some(message) = tool.pending_switch_message() {
                    self.helper_toast("success", &format!("{name} is ready"), &message, cx);
                    continue;
                }
                let description = if job["action"] == "uninstall" {
                    "Saved logins and shared conversations were kept.".to_string()
                } else if let Some(version) = tool.version() {
                    format!("Version {version} is installed.")
                } else {
                    format!("{name} is installed.")
                };
                self.helper_toast("success", &format!("{name} {done}"), &description, cx);
            } else {
                self.helper_toast(
                    "error",
                    &format!("{name} {failed}"),
                    job["error"]
                        .as_str()
                        .unwrap_or("The command did not finish."),
                    cx,
                );
            }
        }
        self.helpers.tools = next;
        if finished {
            self.account_request(json!({ "operation": "list", "refresh": true }), None, cx);
        }
        self.sync_helper_poll(cx);
        cx.notify();
    }

    /// Polls while any action runs.
    fn sync_helper_poll(&mut self, cx: &mut Context<Self>) {
        let running = self
            .helpers
            .tools
            .iter()
            .any(|tool| tool.running_action().is_some());
        if !running {
            self.helpers.poll = None;
            return;
        }
        if self.helpers.poll.is_some() {
            return;
        }
        self.helpers.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RUNNING_POLL).await;
                if this
                    .update(cx, |page, cx| page.load_helper_tools(false, true, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    /// `load(fresh, announce)`: `helperStatus`.
    pub(crate) fn load_helper_tools(
        &mut self,
        fresh: bool,
        announce: bool,
        cx: &mut Context<Self>,
    ) {
        self.load_helper_tools_then(fresh, announce, None, cx);
    }

    fn load_helper_tools_then(
        &mut self,
        fresh: bool,
        announce: bool,
        then: Option<
            Box<dyn FnOnce(&mut Self, Result<Vec<HelperTool>, String>, &mut Context<Self>)>,
        >,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({ "operation": "helperStatus", "fresh": fresh }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let tools = result.map(|state| parse_tools(&state));
                    if let Ok(tools) = &tools
                        && !tools.is_empty()
                    {
                        page.apply_helper_tools(tools.clone(), announce, cx);
                    }
                    if let Some(then) = then {
                        then(page, tools, cx);
                    }
                });
            },
            cx,
        );
    }

    /// `checkForUpdates(provider)`.
    fn check_helper_updates(&mut self, provider: String, cx: &mut Context<Self>) {
        self.helpers.checking = true;
        cx.notify();
        self.load_helper_tools_then(
            true,
            false,
            Some(Box::new(move |page, result, cx| {
                page.helpers.checking = false;
                cx.notify();
                let name = helper_label(&provider);
                match result {
                    Ok(tools) => {
                        let Some(tool) = tools.into_iter().find(|tool| tool.provider() == provider)
                        else {
                            return;
                        };
                        match tool.update_available() {
                            Some(true) => page.helper_toast(
                                "info",
                                &format!("{name} update available"),
                                &format!(
                                    "Version {} is available; {} is installed.",
                                    tool.latest_version().unwrap_or_default(),
                                    tool.version().unwrap_or_default()
                                ),
                                cx,
                            ),
                            Some(false) => page.helper_toast(
                                "success",
                                &format!("{name} is up to date"),
                                &format!(
                                    "Version {} is the latest release.",
                                    tool.version().unwrap_or_default()
                                ),
                                cx,
                            ),
                            None => page.helper_toast(
                                "warning",
                                &format!("Couldn't check for {name} updates"),
                                &tool
                                    .check_error()
                                    .unwrap_or_else(|| "Try again in a moment.".into()),
                                cx,
                            ),
                        }
                    }
                    Err(error) => page.helper_toast(
                        "error",
                        "Update check failed",
                        if error.is_empty() {
                            "Try again in a moment."
                        } else {
                            &error
                        },
                        cx,
                    ),
                }
            })),
            cx,
        );
    }

    /// `run(provider, action)`: `helperAction`.
    fn run_helper_action(
        &mut self,
        provider: String,
        action: &'static str,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({ "operation": "helperAction", "provider": provider, "action": action }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| match result {
                    Ok(state) => {
                        let tools = parse_tools(&state);
                        if !tools.is_empty() {
                            page.apply_helper_tools(tools, true, cx);
                        }
                    }
                    Err(error) => {
                        let (_, _, failed) = action_words(action);
                        page.helper_toast(
                            "error",
                            &format!("{} {failed}", helper_label(&provider)),
                            if error.is_empty() {
                                "The command could not start."
                            } else {
                                &error
                            },
                            cx,
                        );
                    }
                });
            },
            cx,
        );
    }

    /// `accountHelperOffersInstall`.
    pub(crate) fn helper_offers_install(&self, provider: &str) -> bool {
        self.helpers
            .tools
            .iter()
            .find(|tool| tool.provider() == provider)
            .is_some_and(HelperTool::offers_install)
    }

    /// `AccountHelperInstallButton` without its copy-command fallback (callers keep theirs):
    /// Install with gxserver's plan as the tooltip, or the running install. `None` when gxserver
    /// cannot install the helper.
    ///
    /// CDXC:ManagedTools 2026-09-29 DECISION:
    /// User: "when they click on something, we help them install it on windows/macos/linux automatically (show a button with a tooltip explaining how we'll install) but 1 click installs it for them as much as possible". A missing Claude Swap or Codex Swap is one Install button whose tooltip is gxserver's `installPlan`; the job runs on the computer the accounts belong to. Supersedes the copyable install command the Accounts page, the Add account flow and the connection guide showed, which stays only for a computer whose gxserver cannot install helpers yet.
    /// SEE-ALSO: server/src/accounts/helper_tools.rs (`install_plan`, `Work::UvInstall`); used by the helper row, the Add account flow (manager.rs) and the guide (guide.rs).
    pub(crate) fn render_helper_install_button(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        id_prefix: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tool = self
            .helpers
            .tools
            .iter()
            .find(|tool| tool.provider() == provider)
            .cloned()
            .filter(HelperTool::offers_install)?;
        let name = helper_label(provider);
        let running = tool.running_action();
        let installing = running.as_deref() == Some("install");
        let unavailable = tool.unavailable_reason();
        let tooltip: SharedString = unavailable
            .clone()
            .or_else(|| tool.install_plan())
            .unwrap_or_else(|| format!("Install {name}"))
            .into();
        let button = settings_sized_button(
            p,
            SharedString::from(format!("{id_prefix}-{provider}-install")),
            if installing {
                format!("Installing {name}…")
            } else {
                format!("Install {name}")
            },
            Some(if installing {
                "modals/settings/loader-2.svg"
            } else {
                "modals/settings/download.svg"
            }),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            installing || unavailable.is_some() || running.is_some(),
            Some(if installing {
                format!("Installing {name}…").into()
            } else {
                unavailable
                    .map(SharedString::from)
                    .unwrap_or_else(|| format!("Installing {name}…").into())
            }),
            move |page: &mut Self, _window, cx| {
                page.run_helper_action(provider.to_string(), "install", cx)
            },
            cx,
        );
        Some(
            div()
                .id(SharedString::from(format!(
                    "{id_prefix}-{provider}-install-tip"
                )))
                .flex_shrink_0()
                .tooltip(tooltip_text(tooltip))
                .child(button)
                .into_any_element(),
        )
    }

    /// `AccountHelperToolRow`: the helper's quiet line and, while asked, the Uninstall
    /// confirmation.
    pub(crate) fn render_helper_footer(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(tool) = self
            .helpers
            .tools
            .iter()
            .find(|tool| tool.provider() == provider)
            .cloned()
        else {
            return Vec::new();
        };
        let running = tool.running_action();
        let name = helper_label(provider);
        let command = helper_command(provider);
        let provider_name = if provider == "claude" {
            "Claude"
        } else {
            "Codex"
        };
        if tool.offers_install() {
            let detail = if running.is_some() {
                format!("Installing {name}…")
            } else {
                format!(
                    "{name} isn't installed. It's needed to add and switch {provider_name} accounts."
                )
            };
            let button = self.render_helper_install_button(p, provider, "helper-row", cx);
            return vec![footer_line(
                p,
                provider,
                ListItemStatus::Warning,
                detail,
                button.into_iter().collect(),
            )];
        }
        if !tool.installed() && running.is_none() {
            return Vec::new();
        }
        // A switch that waits for running sessions offers no action: each would replace files they use.
        if let (Some(message), None) = (tool.pending_switch_message(), &running) {
            return vec![footer_line(
                p,
                provider,
                ListItemStatus::Warning,
                message,
                Vec::new(),
            )];
        }
        let checking = self.helpers.checking;
        let installed_suffix = tool
            .version()
            .map(|version| format!(" (installed v{version})"))
            .unwrap_or_default();
        let unmanaged: SharedString = match tool.path() {
            Some(path) => format!(
                "Ghostex can't tell how {command} at {path} was installed. Use the tool you installed it with."
            )
            .into(),
            None => format!("Ghostex can't tell how {command} was installed.").into(),
        };
        let busy_reason: SharedString = match &running {
            Some(action) => format!("{} {name}…", action_words(action).0).into(),
            None => unmanaged,
        };
        let version = tool
            .version()
            .map(|version| format!(" {version}"))
            .unwrap_or_default();
        let detail = match (&running, tool.update_available()) {
            (Some(action), _) => format!("{} {name}…", action_words(action).0),
            (None, Some(true)) => format!(
                "{provider_name} accounts are kept by {name}{version} · {} is available",
                tool.latest_version().unwrap_or_default()
            ),
            (None, Some(false)) => {
                format!("{provider_name} accounts are kept by {name}{version} · up to date")
            }
            (None, None) => format!("{provider_name} accounts are kept by {name}{version}"),
        };
        let update_is_update = tool.update_available() == Some(true);
        let update = settings_sized_button(
            p,
            SharedString::from(format!("helper-{provider}-update")),
            if checking {
                "Checking…".to_string()
            } else if running.as_deref() == Some("update") {
                "Updating…".to_string()
            } else if update_is_update {
                "Update".to_string()
            } else {
                "Check for updates".to_string()
            },
            None,
            None,
            if update_is_update {
                SizedButtonVariant::Primary
            } else {
                SizedButtonVariant::Outline
            },
            SizedButtonSize::Xs,
            running.is_some() || checking || (update_is_update && !tool.can("update")),
            Some(if checking {
                format!("Checking for {name} updates…").into()
            } else {
                busy_reason.clone()
            }),
            move |page: &mut Self, _window, cx| {
                if update_is_update {
                    page.run_helper_action(provider.to_string(), "update", cx);
                } else {
                    page.check_helper_updates(provider.to_string(), cx);
                }
            },
            cx,
        );
        let update = div()
            .id(SharedString::from(format!("helper-{provider}-update-tip")))
            .tooltip(tooltip_text(if update_is_update {
                format!(
                    "Update {name} to v{}{installed_suffix}",
                    tool.latest_version().unwrap_or_default()
                )
            } else {
                format!("Check for {name} updates{installed_suffix}")
            }))
            .child(update)
            .into_any_element();
        let menu_open = self.helpers.menu.as_deref() == Some(provider);
        let more = settings_square_button(
            p,
            SharedString::from(format!("helper-{provider}-more")),
            "modals/settings/dots.svg",
            Some(p.muted),
            SizedButtonVariant::Outline,
            24.0,
            Some(format!("More {name} actions").into()),
            running.is_some(),
            Some(busy_reason.clone()),
            move |page: &mut Self, _window, cx| {
                page.helpers.menu = if page.helpers.menu.as_deref() == Some(provider) {
                    None
                } else {
                    Some(provider.to_string())
                };
                cx.notify();
            },
            cx,
        );
        let menu = menu_open.then(|| {
            let items = [
                (
                    "reinstall",
                    format!("Reinstall the latest {name}"),
                    tool.version()
                        .map(|version| format!("Installed v{version}"))
                        .unwrap_or_default(),
                    tool.can("reinstall"),
                ),
                (
                    "uninstall",
                    format!("Uninstall {name}"),
                    "Keeps saved logins and shared conversations".to_string(),
                    tool.can("uninstall"),
                ),
            ];
            let rows = items.into_iter().map(|(action, label, note, enabled)| {
                h_flex()
                    .id(SharedString::from(format!(
                        "helper-{provider}-menu-{action}"
                    )))
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .gap(px(8.0))
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(hsla(if action == "uninstall" {
                        p.destructive
                    } else {
                        p.foreground
                    }))
                    .when(!enabled, |this| this.opacity(0.5))
                    .when(enabled, |this| {
                        this.cursor_pointer()
                            .hover(move |this| this.bg(hsla(p.popup_hover)))
                            .on_click(cx.listener(
                                move |page, _: &gpui::ClickEvent, _window, cx| {
                                    page.helpers.menu = None;
                                    if action == "uninstall" {
                                        page.helpers.confirm_uninstall = Some(provider.to_string());
                                        cx.notify();
                                    } else {
                                        page.run_helper_action(provider.to_string(), action, cx);
                                    }
                                },
                            ))
                    })
                    .child(
                        v_flex().child(label).child(
                            div()
                                .text_size(px(11.5))
                                .text_color(hsla(p.muted))
                                .child(note),
                        ),
                    )
            });
            div().absolute().top_full().right_0().child(
                deferred(
                    anchored()
                        .anchor(gpui::Anchor::TopRight)
                        .snap_to_window_with_margin(px(8.0))
                        .child(
                            v_flex()
                                .id(SharedString::from(format!("helper-{provider}-menu")))
                                .occlude()
                                .mt(px(4.0))
                                .w(px(280.0))
                                .p(px(4.0))
                                .rounded(px(MODAL_RADIUS_CONTROL))
                                .border_1()
                                .border_color(hsla(p.popup_border))
                                .bg(hsla(p.popup_background))
                                .shadow_md()
                                .on_mouse_down_out(cx.listener(
                                    |page, _: &gpui::MouseDownEvent, _window, cx| {
                                        page.helpers.menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(rows),
                        ),
                )
                .with_priority(1),
            )
        });
        let more = div()
            .relative()
            .child(more)
            .children(menu)
            .into_any_element();
        let mut rows = vec![footer_line(
            p,
            provider,
            if update_is_update {
                ListItemStatus::Warning
            } else {
                ListItemStatus::Success
            },
            detail,
            vec![update, more],
        )];
        if self.helpers.confirm_uninstall.as_deref() == Some(provider) && running.is_none() {
            let cancel = settings_sized_button(
                p,
                SharedString::from(format!("helper-{provider}-uninstall-cancel")),
                "Cancel",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| {
                    page.helpers.confirm_uninstall = None;
                    cx.notify();
                },
                cx,
            );
            let confirm = settings_sized_button(
                p,
                SharedString::from(format!("helper-{provider}-uninstall-confirm")),
                "Uninstall",
                None,
                None,
                SizedButtonVariant::Destructive,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.helpers.confirm_uninstall = None;
                    page.run_helper_action(provider.to_string(), "uninstall", cx);
                },
                cx,
            );
            rows.push(
                div()
                    .w_full()
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.raised))
                    .child(settings_list_item(
                        p,
                        Some(ListItemStatus::Warning),
                        None,
                        format!("Uninstall {name}?"),
                        Some(
                            div()
                                .whitespace_normal()
                                .child(format!(
                                    "Saved logins and shared conversations stay. Ghostex can't add, reconnect or switch {provider_name} accounts until you install it again."
                                ))
                                .into_any_element(),
                        ),
                        Some(
                            h_flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(cancel)
                                .child(confirm)
                                .into_any_element(),
                        ),
                    ))
                    .into_any_element(),
            );
        }
        rows
    }
}

/// One helper's quiet line: status dot, text, and its buttons.
fn footer_line(
    p: &SettingsPalette,
    provider: &str,
    status: ListItemStatus,
    text: String,
    buttons: Vec<AnyElement>,
) -> AnyElement {
    let dot = match status {
        ListItemStatus::Warning => modal_rgba(0xfbbf24, 0.85),
        ListItemStatus::Success => modal_rgba(0x34d399, 0.8),
        ListItemStatus::Neutral => modal_rgba(0xffffff, 0.2),
    };
    h_flex()
        .id(SharedString::from(format!("helper-{provider}-line")))
        .w_full()
        .min_h(px(32.0))
        .px(px(4.0))
        .gap(px(8.0))
        .items_center()
        .child(
            div()
                .flex_shrink_0()
                .size(px(6.0))
                .rounded_full()
                .bg(hsla(dot)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.5))
                .text_color(hsla(p.muted))
                .child(text),
        )
        .children(buttons)
        .into_any_element()
}

fn parse_tools(state: &Value) -> Vec<HelperTool> {
    state["helperTools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .map(|raw| HelperTool { raw: raw.clone() })
                .collect()
        })
        .unwrap_or_default()
}
