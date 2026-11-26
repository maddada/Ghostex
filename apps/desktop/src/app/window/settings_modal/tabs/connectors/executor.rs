//! The Executor section: the computer's Executor CLI (installed or not, its one sign-in, which
//! every workspace and every harness on the computer shares), the apps that sign-in reaches with a
//! link to manage them in Executor, a self-hosted sign-in, and the apps the old built-in Executor
//! left until they are cleaned up.
//!
//! CDXC:Connectors 2026-11-27 DECISION:
//! User (Executor CLI page): one section for the whole computer, as the mockups "Signed in", "Signing in" and "Not installed" showed, with the old apps listed once (PLACEHOLDER-4). The catalog search, the remote MCP server row, the per-workspace switch and the callback address are gone: apps and their accounts are added in Executor itself.
use super::super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, ROW_PADDING_X, ROW_PADDING_Y, settings_button,
    settings_list_item, settings_list_item_wrapped_with_status, settings_section,
    settings_text_input,
};
use super::super::super::palette::SettingsPalette;
use super::ConnectorsTab;
use super::data::{busy_key, flag, list, text};
use super::widgets::{detail, note_row};
use gpui::{
    AnyElement, Context, Div, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    div, px,
};
use gpui_component::h_flex;
use serde_json::Value;

/// The key the Executor section's busy buttons, notes and sign-in panel go under: the CLI is the
/// computer's, not a workspace's.
pub(super) const EXECUTOR_KEY: &str = "executor-cli";

fn host_input_id() -> SharedString {
    SharedString::from("connectors-executor-host")
}

/// "A, B and C".
fn joined(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The installer is running.
pub(super) fn installing(executor: &Value) -> bool {
    executor
        .get("install")
        .and_then(|install| install.get("status"))
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "queued" | "running"))
}

impl ConnectorsTab {
    /// The first row: the CLI's state and the one action it needs next.
    fn executor_cli_row(
        &mut self,
        p: &SettingsPalette,
        executor: &Value,
        offline: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = text(executor, "version");
        let installed = flag(executor, "installed");
        let title = if installed && !version.is_empty() {
            format!("Executor CLI {version}")
        } else {
            "Executor CLI".to_string()
        };
        if !installed {
            let running = installing(executor) || self.is_busy(EXECUTOR_KEY, "install");
            let install_error = executor
                .get("install")
                .and_then(|install| install.get("error"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let problem = text(executor, "problem");
            let (status, line) = if running {
                (
                    ListItemStatus::Neutral,
                    "Running Executor's installer…".to_string(),
                )
            } else if !problem.is_empty() {
                (ListItemStatus::Warning, problem)
            } else if let Some(error) = install_error {
                (ListItemStatus::Warning, error)
            } else {
                (
                    ListItemStatus::Neutral,
                    "Not installed on this computer".to_string(),
                )
            };
            let install = settings_button(
                p,
                "connectors-executor-install",
                if running { "Installing…" } else { "Install" },
                Some("modals/settings/download.svg"),
                ButtonVariant::Outline,
                offline || running,
                None,
                |page: &mut Self, _window, cx| page.executor_install(cx),
                cx,
            );
            return settings_list_item_wrapped_with_status(
                p,
                status,
                title,
                detail(line),
                Some(install),
            );
        }
        if flag(executor, "signedIn") {
            let organization = text(executor, "organization");
            let account = if organization.is_empty() {
                format!("Signed in to {}", text(executor, "hostLabel"))
            } else {
                format!(
                    "Signed in to {} as @{organization}",
                    text(executor, "hostLabel")
                )
            };
            let line = format!(
                "{account} · {} · approvals in {} mode",
                text(executor, "access"),
                text(executor, "mode")
            );
            let error = text(executor, "error");
            let signing_out = self.is_busy(EXECUTOR_KEY, "sign-out");
            let sign_out = settings_button(
                p,
                "connectors-executor-sign-out",
                if signing_out {
                    "Signing out…"
                } else {
                    "Sign out"
                },
                None,
                ButtonVariant::Outline,
                offline || signing_out,
                None,
                |page: &mut Self, _window, cx| page.executor_sign_out(cx),
                cx,
            );
            if error.is_empty() {
                return settings_list_item(
                    p,
                    Some(ListItemStatus::Success),
                    None,
                    title,
                    Some(detail(line)),
                    Some(sign_out),
                );
            }
            return settings_list_item_wrapped_with_status(
                p,
                ListItemStatus::Warning,
                title,
                detail(format!("{line}. {error}")),
                Some(sign_out),
            );
        }
        let signing_in = self.is_busy(EXECUTOR_KEY, "sign-in");
        let self_hosted = settings_button(
            p,
            "connectors-executor-self-hosted",
            "Self-hosted…",
            None,
            ButtonVariant::Ghost,
            offline || signing_in,
            None,
            |page: &mut Self, _window, cx| {
                page.executor_host_open = !page.executor_host_open;
                cx.notify();
            },
            cx,
        );
        let sign_in = settings_button(
            p,
            "connectors-executor-sign-in",
            if signing_in { "Opening…" } else { "Sign in" },
            None,
            ButtonVariant::Outline,
            offline || signing_in,
            None,
            |page: &mut Self, _window, cx| page.executor_sign_in(None, cx),
            cx,
        );
        let error = text(executor, "error");
        let line = if error.is_empty() {
            "Not signed in. Agents get Executor's tools once you sign in.".to_string()
        } else {
            error
        };
        settings_list_item_wrapped_with_status(
            p,
            ListItemStatus::Warning,
            title,
            detail(line),
            Some(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self_hosted)
                    .child(sign_in)
                    .into_any_element(),
            ),
        )
    }

    /// The address of a self-hosted Executor to sign in to.
    fn executor_host_row(
        &mut self,
        p: &SettingsPalette,
        offline: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = FieldStates::text_state(
            self,
            &host_input_id(),
            "",
            Some("https://… address of your Executor"),
            |_: &mut Self, _, _, _| {},
            window,
            cx,
        );
        let signing_in = self.is_busy(EXECUTOR_KEY, "sign-in");
        let sign_in = settings_button(
            p,
            "connectors-executor-host-sign-in",
            "Sign in",
            None,
            ButtonVariant::Outline,
            offline || signing_in,
            None,
            |page: &mut Self, _window, cx| {
                let host = page
                    .fields
                    .texts
                    .get(&host_input_id())
                    .map(|state| state.input.read(cx).value().trim().to_string())
                    .unwrap_or_default();
                page.executor_sign_in(Some(host), cx);
            },
            cx,
        );
        h_flex()
            .w_full()
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y))
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(settings_text_input(p, &input, None, false, window, cx)),
            )
            .child(sign_in)
            .into_any_element()
    }

    /// The apps the sign-in reaches, and where to manage them.
    fn executor_apps_row(
        &mut self,
        p: &SettingsPalette,
        executor: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let names: Vec<String> = list(executor, "apps")
            .iter()
            .map(|app| text(app, "name"))
            .filter(|name| !name.is_empty())
            .collect();
        let apps_error = text(executor, "appsError");
        let line = if !names.is_empty() {
            names.join(", ")
        } else if !apps_error.is_empty() {
            apps_error
        } else {
            "None yet".to_string()
        };
        let manage_url = text(executor, "manageUrl");
        let manage = (!manage_url.is_empty()).then(|| {
            settings_button(
                p,
                "connectors-executor-manage",
                "Manage in Executor",
                Some("modals/settings/external-link.svg"),
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| page.open_url(&manage_url, cx),
                cx,
            )
        });
        settings_list_item(p, None, None, "Apps", Some(detail(line)), manage)
    }

    /// The apps the old built-in Executor had, until their data is removed.
    fn executor_old_apps_row(
        &mut self,
        p: &SettingsPalette,
        old: &Value,
        offline: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let names: Vec<String> = list(old, "apps")
            .iter()
            .filter_map(|name| name.as_str().map(str::to_string))
            .collect();
        let removing = self.is_busy(EXECUTOR_KEY, "remove-old");
        let remove = settings_button(
            p,
            "connectors-executor-remove-old",
            if removing { "Removing…" } else { "Remove" },
            None,
            ButtonVariant::Outline,
            offline || removing,
            None,
            |page: &mut Self, _window, cx| page.executor_remove_old(cx),
            cx,
        );
        // Apps whose names couldn't be read (`git` couldn't run) still count, so the row stays.
        let unreadable = old.get("unreadable").and_then(Value::as_u64).unwrap_or(0);
        let which = match (names.is_empty(), unreadable) {
            (true, _) => "Some apps".to_string(),
            (false, 0) => joined(&names),
            (false, 1) => format!("{} and 1 app Ghostex couldn't name", joined(&names)),
            (false, more) => format!("{} and {more} apps Ghostex couldn't name", joined(&names)),
        };
        settings_list_item_wrapped_with_status(
            p,
            ListItemStatus::Warning,
            "Apps on the old Executor",
            detail(format!(
                "{which} ran on Ghostex's own Executor. Add them again in Executor; their sign-ins can't move."
            )),
            Some(remove),
        )
    }

    pub(super) fn executor_section(
        &mut self,
        p: &SettingsPalette,
        executor: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let offline = !self.rpc_available(cx);
        let mut rows: Vec<AnyElement> = vec![self.executor_cli_row(p, executor, offline, cx)];
        let signed_in = flag(executor, "signedIn");
        if flag(executor, "installed") && !signed_in && self.executor_host_open {
            rows.push(self.executor_host_row(p, offline, window, cx));
        }
        rows.extend(self.sign_in_panel(p, EXECUTOR_KEY, "executor", cx));
        if signed_in {
            rows.push(self.executor_apps_row(p, executor, cx));
        }
        if let Some(old) = executor.get("oldApps").filter(|old| !old.is_null()) {
            rows.push(self.executor_old_apps_row(p, old, offline, cx));
        }
        if let Some(note) = self.notes.get(&busy_key(EXECUTOR_KEY, "executor")) {
            rows.push(note_row(p, note));
        }
        settings_section(
            p,
            "Executor".to_string(),
            Some("Your Executor CLI sign-in, shared with every agent on this computer. Add apps and their accounts in Executor.".into()),
            None,
            rows,
        )
    }
}
