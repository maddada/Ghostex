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
    ListItemStatus, SizedButtonSize, SizedButtonVariant, settings_icon, settings_list_item,
    settings_list_item_wrapped_with_icon, settings_sized_button, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{post_store_message, store_copy_to_clipboard};
use super::AccountsTab;
use super::data::{HelperTool, action_words, helper_label, provider_label};
use super::widgets::account_logo;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, div, px,
};
use gpui_component::h_flex;
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
    pub(crate) fn run_helper_action(
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

    /// The Accounts header's ⋯ menu: Refresh accounts, How accounts work, and Claude Swap's and
    /// Codex Swap's own actions.
    ///
    /// CDXC:Settings 2026-11-05 DECISION:
    /// User picked "In the page's ⋯ menu" for Claude Swap and Codex Swap (Q1333), and wants one Updates page for everything later, so their update UI here stays minimal: each helper is a quiet version line in this menu with Update (or Check for updates), Reinstall and Uninstall…, and only a running action or the Uninstall confirmation shows on the page. Supersedes the 2026-10-14 "one quiet line at the end" footer.
    /// Since 2026-11-17 (Q3727 "Keep Install, one plain line") a missing helper's Install keeps its row here with one plain line instead of the install plan, and the page also shows the setup row (`render_helper_setup_rows`) while saved accounts wait for it.
    pub(crate) fn render_page_menu(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::menus::MenuRow;
        let (busy, refreshing) = {
            let client = self.client.read(cx);
            (client.busy, client.refreshing)
        };
        let mut rows = vec![
            MenuRow::action(
                if refreshing {
                    "Refreshing…"
                } else {
                    "Refresh accounts"
                },
                |page, cx| {
                    let this = cx.weak_entity();
                    page.account_request(
                        json!({ "operation": "list", "refresh": true }),
                        Some(Box::new(move |ok, cx| {
                            if ok {
                                let _ = this.update(cx, |page, cx| {
                                    page.flow_toast(
                                        "Accounts refreshed",
                                        "Saved accounts and usage are up to date.",
                                        cx,
                                    )
                                });
                            }
                        })),
                        cx,
                    );
                },
            )
            .enabled(!busy && !refreshing),
            MenuRow::action("How accounts work", |page, cx| {
                page.guide = Some("claude".into());
                cx.notify();
            }),
        ];
        for provider in super::data::PROVIDERS {
            let Some(tool) = self
                .helpers
                .tools
                .iter()
                .find(|tool| tool.provider() == provider)
                .cloned()
            else {
                continue;
            };
            let name = helper_label(provider);
            let running = tool.running_action();
            if tool.offers_install() {
                rows.push(
                    MenuRow::action(format!("Install {name}"), move |page, cx| {
                        page.run_helper_action(provider.to_string(), "install", cx)
                    })
                    .detail(tool.unavailable_reason().unwrap_or_else(|| {
                        format!(
                            "Needed for more than one {} account.",
                            provider_label(provider)
                        )
                    }))
                    .enabled(running.is_none() && tool.unavailable_reason().is_none())
                    .group(),
                );
                continue;
            }
            if !tool.installed() {
                continue;
            }
            let version = tool
                .version()
                .map(|version| format!(" {version}"))
                .unwrap_or_default();
            let state = match (&running, tool.update_available()) {
                (Some(action), _) => format!("{} {name}…", action_words(action).0),
                (None, Some(true)) => format!(
                    "{name}{version} · {} is available",
                    tool.latest_version().unwrap_or_default()
                ),
                (None, Some(false)) => format!("{name}{version} · up to date"),
                (None, None) => format!("{name}{version}"),
            };
            rows.push(MenuRow::note(state).group());
            let idle = running.is_none() && !self.helpers.checking;
            if tool.update_available() == Some(true) {
                rows.push(
                    MenuRow::action(
                        format!("Update to {}", tool.latest_version().unwrap_or_default()),
                        move |page, cx| page.run_helper_action(provider.to_string(), "update", cx),
                    )
                    .enabled(idle && tool.can("update")),
                );
            } else {
                rows.push(
                    MenuRow::action("Check for updates", move |page, cx| {
                        page.check_helper_updates(provider.to_string(), cx)
                    })
                    .enabled(idle),
                );
            }
            rows.push(
                MenuRow::action("Reinstall", move |page, cx| {
                    page.run_helper_action(provider.to_string(), "reinstall", cx)
                })
                .enabled(idle && tool.can("reinstall")),
            );
            rows.push(
                MenuRow::action("Uninstall…", move |page, cx| {
                    page.helpers.confirm_uninstall = Some(provider.to_string());
                    cx.notify();
                })
                .detail("Keeps saved logins and shared conversations")
                .destructive()
                .enabled(idle && tool.can("uninstall")),
            );
        }
        self.render_more_menu(p, "page", "More account actions", rows, cx)
    }

    /// The missing helper of `provider` and how many saved accounts (paused ones aside) wait for
    /// it, while the Accounts section shows its setup row.
    pub(crate) fn helper_setup_needed(
        &self,
        provider: &str,
        cx: &gpui::App,
    ) -> Option<(HelperTool, usize)> {
        let tool = self
            .helpers
            .tools
            .iter()
            .find(|tool| tool.provider() == provider && tool.offers_install())
            .cloned()?;
        let waiting = self
            .client
            .read(cx)
            .data
            .as_ref()?
            .registered(provider)
            .iter()
            .filter(|account| !account.user_disabled())
            .count();
        (waiting > 0).then_some((tool, waiting))
    }

    /// "this computer", or the remote machine the page shows.
    pub(crate) fn helper_place(&self, cx: &gpui::App) -> String {
        self.shown_machine_name(cx)
            .unwrap_or_else(|| "this computer".to_string())
    }

    /// What an account's sign-in box (and a sign-in that failed) says while its helper is
    /// missing: it names the setup row, never a Settings page, and follows that row's state.
    pub(crate) fn helper_missing_sentence(&self, provider: &str, cx: &gpui::App) -> Option<String> {
        let (tool, _) = self.helper_setup_needed(provider, cx)?;
        let name = helper_label(provider);
        let place = self.helper_place(cx);
        Some(if tool.running_action().is_some() {
            format!("{name} is installing on {place}. You can sign in here when it finishes.")
        } else if tool.failed_install().is_some() {
            format!("{name} didn't install on {place}. Try again in the {name} row at the top.")
        } else {
            format!(
                "Signing in here needs {name}, which isn't on {place} yet. Install it with the button in the {name} row at the top."
            )
        })
    }

    /// The Accounts section's first rows: one per missing Claude Swap or Codex Swap that saved
    /// accounts wait for.
    ///
    /// CDXC:AgentProviders 2026-11-17 DECISION:
    /// User, of a synced Claude account's sign-in failing with "Install Claude Swap in Settings > Accounts first." while on that page, Install only in the ⋯ menu: "how do u expect people to find this install button????? and why does this warning/error refer to the same page i'm already in". They picked a setup row at the top of Accounts (Q3724 C) that says the helper is missing, how many accounts wait for it and what it is (that sentence is the consent, Q3725 A), installs in place and shows the install running; a failed install says why in plain words with Try again and Copy details (Q3726), or the fix Ghostex can run itself (curl and certificates for Codex Swap's installer). "We need exact same ux for codex swap also". The row owns Install on the page: the account's box, its failed sign-in, Add account and the account list name the row instead of a Settings page.
    /// SEE-ALSO: `needsSystemTools` in server/src/accounts/helper_tools.rs `tool_view`; editor.rs, connect.rs, list.rs and manager.rs read `helper_setup_needed`.
    pub(crate) fn render_helper_setup_rows(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for provider in super::data::PROVIDERS {
            if let Some((tool, waiting)) = self.helper_setup_needed(provider, cx) {
                rows.push(self.render_helper_setup_row(p, provider, &tool, waiting, cx));
            }
        }
        rows
    }

    fn render_helper_setup_row(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        tool: &HelperTool,
        waiting: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = helper_label(provider);
        let label = provider_label(provider);
        let remote = self.shown_machine_name(cx).is_some();
        let place = self.helper_place(cx);
        let accounts = if waiting == 1 {
            format!("1 {label} account needs it")
        } else {
            format!("{waiting} {label} accounts need it")
        };
        let missing = format!("{name} isn't on {place}");
        let button = |id: &str,
                      text: &'static str,
                      primary: bool,
                      disabled: Option<String>,
                      press: Box<dyn Fn(&mut Self, &mut Context<Self>)>,
                      cx: &mut Context<Self>| {
            settings_sized_button(
                p,
                SharedString::from(format!("helper-{provider}-setup-{id}")),
                text,
                None,
                None,
                if primary {
                    SizedButtonVariant::Primary
                } else {
                    SizedButtonVariant::Outline
                },
                SizedButtonSize::Sm,
                disabled.is_some(),
                disabled.map(SharedString::from),
                move |page: &mut Self, _window, cx| press(page, cx),
                cx,
            )
        };
        let check_again = |cx: &mut Context<Self>| {
            button(
                "check",
                "Check again",
                false,
                None,
                Box::new(|page, cx| page.load_helper_tools(true, false, cx)),
                cx,
            )
        };
        let (icon, title, detail, controls) = if tool.running_action().is_some() {
            (
                settings_icon("modals/settings/loader-2.svg", 18.0, p.muted).into_any_element(),
                format!("Installing {name}…"),
                format!(
                    "About a minute. Your {label} accounts can sign in on {place} when it finishes."
                ),
                Vec::new(),
            )
        } else if tool.needs_system_tools() {
            let mut controls = Vec::new();
            let detail = if remote {
                format!(
                    "{accounts} there. Its installer needs curl and security certificates first: install them from a terminal on {place}, then check again."
                )
            } else {
                controls.push(button(
                    "system-tools",
                    "Install curl and certificates",
                    true,
                    None,
                    Box::new(|page, cx| {
                        post_store_message(
                            &page.store,
                            json!({ "type": "runManagedToolTerminalCommand", "toolId": "systemTools" }),
                            cx,
                        )
                    }),
                    cx,
                ));
                format!(
                    "{accounts} here. Its installer needs curl and security certificates first; Ghostex installs them in a terminal, where your password is asked."
                )
            };
            controls.push(check_again(cx));
            (account_logo(p, provider, 20.0), missing, detail, controls)
        } else if let Some((error, output)) = tool.failed_install() {
            let details = format!("{error}\n\n{output}").trim().to_string();
            let blocked = tool.unavailable_reason().or_else(|| {
                (!tool.can("install")).then(|| format!("{name} can't be installed here."))
            });
            let controls = vec![
                button(
                    "retry",
                    "Try again",
                    true,
                    blocked,
                    Box::new(move |page, cx| {
                        page.run_helper_action(provider.to_string(), "install", cx)
                    }),
                    cx,
                ),
                button(
                    "copy",
                    "Copy details",
                    false,
                    None,
                    Box::new(move |page, cx| {
                        store_copy_to_clipboard(&page.store, details.clone(), cx)
                    }),
                    cx,
                ),
            ];
            (
                account_logo(p, provider, 20.0),
                format!("Couldn't install {name}"),
                plain_install_failure(&format!("{error} {output}")).to_string(),
                controls,
            )
        } else if let Some(reason) = tool.unavailable_reason() {
            (
                account_logo(p, provider, 20.0),
                missing,
                format!("{accounts} on {place}, but {reason}"),
                Vec::new(),
            )
        } else {
            (
                account_logo(p, provider, 20.0),
                missing,
                format!(
                    "{accounts} on {place}. It's a free helper that keeps several {label} logins apart, and installing takes about a minute."
                ),
                self.render_helper_install_button(p, provider, "setup-row", cx)
                    .into_iter()
                    .collect(),
            )
        };
        div()
            .id(SharedString::from(format!("helper-{provider}-setup-row")))
            .w_full()
            .child(settings_list_item_wrapped_with_icon(
                p,
                icon,
                title,
                div().child(detail).into_any_element(),
                (!controls.is_empty()).then(|| {
                    h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .children(controls)
                        .into_any_element()
                }),
            ))
            .into_any_element()
    }

    /// While Claude Swap or Codex Swap works, its status line, and the Uninstall confirmation.
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
        let provider_name = if provider == "claude" {
            "Claude"
        } else {
            "Codex"
        };
        if tool.offers_install() && running.is_none() {
            // Installing is in the setup row, the page's ⋯ menu and Add account.
            return Vec::new();
        }
        if !tool.installed() && self.helper_setup_needed(provider, cx).is_some() {
            // The setup row at the top shows the running install.
            return Vec::new();
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
        let mut rows = Vec::new();
        if let Some(action) = &running {
            rows.push(footer_line(
                p,
                provider,
                ListItemStatus::Warning,
                format!("{} {name}…", action_words(action).0),
                Vec::new(),
            ));
        }
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

/// A failed install's cause in plain words; its raw text stays behind Copy details.
fn plain_install_failure(raw: &str) -> &'static str {
    let raw = raw.to_ascii_lowercase();
    let unreachable = [
        "timed out",
        "timeout",
        "could not resolve",
        "couldn't resolve",
        "failed to fetch",
        "connection",
        "network",
        "unreachable",
    ];
    if unreachable.iter().any(|word| raw.contains(word)) {
        "This computer couldn't reach the download. Check your connection, then try again."
    } else if raw.contains("permission denied") || raw.contains("access is denied") {
        "This computer didn't allow the install. Try again, or copy the details to ask for help."
    } else {
        "The install didn't finish. Try again, or copy the details to ask for help."
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
