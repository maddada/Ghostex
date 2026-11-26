//! Every call the Connectors page makes to gxserver, and what it does with each answer. Each
//! action marks its button busy, shows a failure as a toast, and reads the connectors again.
use super::super::super::store::{post_store_message, store_gxserver_rpc};
use super::ConnectorsTab;
use super::data::{self, busy_key};
use super::executor::EXECUTOR_KEY;
use super::signin_panel::{DoneWhen, PendingSignIn};
use gpui::Context;
use serde_json::{Value, json};
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// Composio is called over the network.
const COMPOSIO_TIMEOUT: Duration = Duration::from_secs(120);
/// The Executor CLI's sign-in and sign-out reach its host.
const EXECUTOR_TIMEOUT: Duration = Duration::from_secs(90);
/// While the Executor CLI installs (which sends no event until it is done), the page reads again
/// this often.
const PENDING_REREAD: Duration = Duration::from_secs(3);

impl ConnectorsTab {
    /// Reads every workspace's connectors.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        self.loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readConnectors",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.loading = false;
                    match result {
                        Ok(result) => {
                            page.connectors = Some(result);
                            page.error = None;
                            page.drop_finished_sign_ins();
                        }
                        Err(error) => page.error = Some(error),
                    }
                    page.reread_while_pending(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Reads again in a moment while the Executor CLI installs.
    fn reread_while_pending(&mut self, cx: &mut Context<Self>) {
        let pending = self
            .connectors
            .as_ref()
            .and_then(|connectors| connectors.get("executor"))
            .is_some_and(super::executor::installing);
        if !pending || self.pending_reread.is_some() {
            return;
        }
        self.pending_reread = Some(cx.spawn(async move |page, cx| {
            cx.background_executor().timer(PENDING_REREAD).await;
            let _ = page.update(cx, |page, cx| {
                page.pending_reread = None;
                page.reload(cx);
            });
        }));
    }

    fn toast(&self, level: &str, title: &str, description: &str, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.toast(level, title, description, cx));
    }

    pub(super) fn open_url(&self, url: &str, cx: &mut Context<Self>) {
        post_store_message(
            &self.store,
            json!({ "type": "openExternalUrl", "url": url }),
            cx,
        );
    }

    fn note(&mut self, workspace: &str, section: &str, text: impl Into<String>) {
        self.notes.insert(busy_key(workspace, section), text.into());
    }

    fn clear_note(&mut self, workspace: &str, section: &str) {
        self.notes.remove(&busy_key(workspace, section));
    }

    /// One action for `workspace`: `action` names its busy button, a failure is a toast titled
    /// `failure`, and `done` gets the answer before the page reads the connectors again.
    #[allow(clippy::too_many_arguments)]
    fn act(
        &mut self,
        workspace: &str,
        action: &str,
        path: &'static str,
        mut params: Value,
        timeout: Duration,
        failure: &'static str,
        done: impl FnOnce(&mut Self, &str, Value, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let key = busy_key(workspace, action);
        if !self.busy.insert(key.clone()) {
            return;
        }
        params["workspaceId"] = json!(workspace);
        cx.notify();
        let this = cx.weak_entity();
        let workspace = workspace.to_string();
        store_gxserver_rpc(
            &self.store.clone(),
            path,
            params,
            timeout,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.busy.remove(&key);
                    match result {
                        Ok(result) => done(page, &workspace, result, cx),
                        Err(error) => page.toast("error", failure, &error, cx),
                    }
                    page.reload(cx);
                });
            },
            cx,
        );
    }

    /// Opens what a sign-in or connect answered (`url`), or says it was already done.
    fn open_answer(
        &mut self,
        workspace: &str,
        section: &str,
        result: &Value,
        what: &str,
        cx: &mut Context<Self>,
    ) {
        let name = Some(data::text(result, "name"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| what.to_string());
        match result.get("url").and_then(Value::as_str) {
            Some(url) if !data::flag(result, "alreadyConnected") => {
                self.clear_note(workspace, section);
                let done_when = DoneWhen::ComposioApp(what.to_string());
                self.begin_sign_in(
                    workspace,
                    section,
                    PendingSignIn {
                        title: format!("Finish connecting {name}"),
                        url: url.to_string(),
                        code: None,
                        done_when,
                    },
                    cx,
                );
            }
            _ => self.note(workspace, section, format!("{name} is already connected.")),
        }
    }

    // ---- Composio ------------------------------------------------------------------------------

    pub(super) fn composio_sign_in(&mut self, workspace: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            "composio-sign-in",
            "/api/signInComposio",
            json!({}),
            COMPOSIO_TIMEOUT,
            "Couldn't start the Composio sign-in",
            |page, workspace, result, cx| match result.get("signInUrl").and_then(Value::as_str) {
                Some(url) => {
                    page.clear_note(workspace, "composio");
                    page.begin_sign_in(
                        workspace,
                        "composio",
                        PendingSignIn {
                            title: "Finish signing in to Composio".to_string(),
                            url: url.to_string(),
                            code: None,
                            done_when: DoneWhen::ComposioSignedIn,
                        },
                        cx,
                    );
                }
                None => page.note(workspace, "composio", "Already signed in to Composio."),
            },
            cx,
        );
    }

    pub(super) fn composio_sign_out(&mut self, workspace: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            "composio-sign-out",
            "/api/signOutComposio",
            json!({}),
            READ_TIMEOUT,
            "Couldn't sign out of Composio",
            |page, workspace, _, _| page.clear_note(workspace, "composio"),
            cx,
        );
    }

    pub(super) fn composio_refresh(&mut self, workspace: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            "composio-refresh",
            "/api/refreshComposio",
            json!({}),
            COMPOSIO_TIMEOUT,
            "Couldn't read your Composio apps",
            |_, _, _, _| {},
            cx,
        );
    }

    pub(super) fn composio_connect(&mut self, workspace: &str, app: &str, cx: &mut Context<Self>) {
        let app_name = app.to_string();
        self.act(
            workspace,
            &format!("composio-connect-{app}"),
            "/api/connectComposioApp",
            json!({ "app": app }),
            COMPOSIO_TIMEOUT,
            "Couldn't connect the app",
            move |page, workspace, result, cx| {
                page.open_answer(workspace, "composio", &result, &app_name, cx)
            },
            cx,
        );
    }

    // ---- Executor CLI --------------------------------------------------------------------------

    pub(super) fn executor_install(&mut self, cx: &mut Context<Self>) {
        self.act(
            EXECUTOR_KEY,
            "install",
            "/api/installExecutorCli",
            json!({}),
            READ_TIMEOUT,
            "Couldn't start Executor's installer",
            |_, _, _, _| {},
            cx,
        );
    }

    /// Starts `executor login` (to `host`, a self-hosted Executor) and shows its code.
    pub(super) fn executor_sign_in(&mut self, host: Option<String>, cx: &mut Context<Self>) {
        let mut params = json!({});
        if let Some(host) = host.filter(|host| !host.is_empty()) {
            params["host"] = json!(host);
        }
        self.act(
            EXECUTOR_KEY,
            "sign-in",
            "/api/signInExecutorCli",
            params,
            EXECUTOR_TIMEOUT,
            "Couldn't start the Executor sign-in",
            |page, workspace, result, cx| {
                let url = data::text(&result, "url");
                if url.is_empty() {
                    return;
                }
                page.executor_host_open = false;
                let code = Some(data::text(&result, "code")).filter(|code| !code.is_empty());
                page.begin_sign_in(
                    workspace,
                    "executor",
                    PendingSignIn {
                        title: "Finish signing in to the Executor CLI".to_string(),
                        url,
                        code,
                        done_when: DoneWhen::ExecutorCli,
                    },
                    cx,
                );
            },
            cx,
        );
    }

    pub(super) fn executor_sign_out(&mut self, cx: &mut Context<Self>) {
        self.act(
            EXECUTOR_KEY,
            "sign-out",
            "/api/signOutExecutorCli",
            json!({}),
            EXECUTOR_TIMEOUT,
            "Couldn't sign the Executor CLI out",
            |_, _, _, _| {},
            cx,
        );
    }

    pub(super) fn executor_remove_old(&mut self, cx: &mut Context<Self>) {
        self.act(
            EXECUTOR_KEY,
            "remove-old",
            "/api/removeOldExecutor",
            json!({}),
            READ_TIMEOUT,
            "Couldn't remove the old Executor's data",
            |_, _, _, _| {},
            cx,
        );
    }

    // ---- Rules ---------------------------------------------------------------------------------

    /// `tool: None` sets the workspace's rule for every call; `rule` `default` drops a tool's own.
    pub(super) fn set_rule(
        &mut self,
        workspace: &str,
        tool: Option<&str>,
        rule: &str,
        cx: &mut Context<Self>,
    ) {
        let mut params = json!({ "rule": rule });
        if let Some(tool) = tool {
            params["tool"] = json!(tool);
        }
        self.act(
            workspace,
            &format!("rule-{}", tool.unwrap_or("all")),
            "/api/setConnectorRule",
            params,
            READ_TIMEOUT,
            "Couldn't change the rule",
            |_, _, _, _| {},
            cx,
        );
    }

    // ---- Command-line sign-ins -----------------------------------------------------------------

    pub(super) fn cli_sign_in(&mut self, workspace: &str, tool: &str, cx: &mut Context<Self>) {
        let tool_id = tool.to_string();
        let tool_name = match tool {
            "gh" => "the GitHub CLI",
            "gws" => "the Google Workspace CLI",
            _ => tool,
        }
        .to_string();
        self.act(
            workspace,
            &format!("cli-sign-in-{tool}"),
            "/api/signInConnectorCli",
            json!({ "tool": tool }),
            COMPOSIO_TIMEOUT,
            "Couldn't start the sign-in",
            move |page, workspace, result, cx| {
                let url = data::text(&result, "url");
                if url.is_empty() {
                    page.note(workspace, "cli", data::text(&result, "message"));
                    return;
                }
                let code = Some(data::text(&result, "code")).filter(|code| !code.is_empty());
                page.begin_sign_in(
                    workspace,
                    &format!("cli-{tool_id}"),
                    PendingSignIn {
                        title: format!("Finish signing in to {tool_name}"),
                        url,
                        code,
                        done_when: DoneWhen::Cli(tool_id.clone()),
                    },
                    cx,
                );
            },
            cx,
        );
    }

    /// Asks for a client_secret.json and gives the workspace's gws that Google OAuth client.
    pub(super) fn cli_choose_client(&mut self, workspace: &str, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use this client".into()),
        });
        let workspace = workspace.to_string();
        cx.spawn(async move |page, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = page.update(cx, |page, cx| {
                page.cli_set_client(&workspace, Some(path.to_string_lossy().into_owned()), cx)
            });
        })
        .detach();
    }

    /// The workspace's own gws client (`Some(path)`), or the usual one again (`None`).
    pub(super) fn cli_set_client(
        &mut self,
        workspace: &str,
        path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let params = match path {
            Some(path) => json!({ "path": path }),
            None => json!({}),
        };
        self.act(
            workspace,
            "cli-client-gws",
            "/api/setConnectorCliClient",
            params,
            READ_TIMEOUT,
            "Couldn't change the Google OAuth client",
            |page, workspace, _, cx| {
                page.pending.remove(&busy_key(workspace, "cli-gws"));
                page.note(
                    workspace,
                    "cli",
                    "Google OAuth client changed. Sign in to the Google Workspace CLI again.",
                );
                cx.notify();
            },
            cx,
        );
    }

    pub(super) fn cli_sign_out(&mut self, workspace: &str, tool: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            &format!("cli-sign-out-{tool}"),
            "/api/signOutConnectorCli",
            json!({ "tool": tool }),
            READ_TIMEOUT,
            "Couldn't sign out",
            |page, workspace, _, _| page.clear_note(workspace, "cli"),
            cx,
        );
    }
}
