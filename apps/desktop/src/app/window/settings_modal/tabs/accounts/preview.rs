//! The preview binary's states (bin/native_modal_demo/settings_f1.rs): the pane, flow or menu each
//! named state opens, with the sign-in jobs gxserver would report.
use super::connect::FlowState;
use super::editor::EditorDraft;
use super::{AccountsTab, Pane};
use gpui::{Context, Window};
use serde_json::{Value, json};

/// A sign-in job as `setupStatus` reports it.
fn job(provider: &str, email: &str, status: &str, extra: Value) -> Value {
    let mut job = json!({
        "id": format!("preview-{provider}-{status}"),
        "provider": provider,
        "email": email,
        "status": status,
        "url": "https://claude.ai/oauth/authorize",
        "output": "Opening the sign-in page in your browser…\nPaste code here if prompted >",
        "acknowledged": false,
        "createdAt": 1,
    });
    if let (Some(job), Some(extra)) = (job.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            job.insert(key.clone(), value.clone());
        }
    }
    job
}

impl AccountsTab {
    /// The account's pane draft, as its first render would make it, changed by `apply`.
    fn preview_draft(
        &mut self,
        id: &str,
        apply: impl FnOnce(&mut EditorDraft),
        cx: &mut Context<Self>,
    ) {
        let Some(account) = self.client.read(cx).data.as_ref().and_then(|data| {
            data.accounts()
                .into_iter()
                .find(|account| account.id() == id)
        }) else {
            return;
        };
        let mut draft = EditorDraft {
            name: account.name(),
            indicator: account.indicator(),
            eligible: account.eligible(),
            remove: false,
            advanced: false,
            swap_target: String::new(),
        };
        apply(&mut draft);
        self.editors.insert(id.to_string(), draft);
    }

    pub(crate) fn apply_preview_pane(
        &mut self,
        state: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let flow = |email: &str, job: Option<Value>, terminal: bool| {
            let mut flow = FlowState::new(email.to_string(), true, job);
            flow.terminal = terminal;
            flow
        };
        match state {
            "accounts-editor" => self.pane = Some(Pane::Account("claude-1".into())),
            "accounts-hubs" => self.pane = Some(Pane::Hub("hub-preview".into())),
            "accounts-hub-add-account" => {
                self.pane = Some(Pane::Hub("hub-preview".into()));
                self.cliproxy_card_collapsed = true;
                self.hub_drafts.insert(
                    "hub-preview".into(),
                    super::hubs::HubDraft {
                        url: "http://127.0.0.1:8317".into(),
                        label: "Home hub".into(),
                        pick_add_account: true,
                        ..Default::default()
                    },
                );
            }
            "accounts-hub-oauth" => {
                self.pane = Some(Pane::Hub("hub-preview".into()));
                self.cliproxy_card_collapsed = true;
                self.hub_oauth = Some(super::hubs::HubOAuthFlow {
                    id: 1,
                    hub_id: "hub-preview".into(),
                    provider: "claude",
                    url: "https://claude.ai/oauth/authorize?client_id=cliproxy&state=preview-state"
                        .into(),
                    state: "preview-state".into(),
                    is_local: true,
                    status: "waiting".into(),
                    error: None,
                    input: String::new(),
                    submitting: false,
                    poll: None,
                });
            }
            "accounts-cliproxy-guide" => self.open_cliproxy_guide(window, cx),
            "accounts-hub-account" => self.pane = Some(Pane::HubAccount("hub-preview:a1".into())),
            "accounts-hub-add" => self.pane = Some(Pane::AddHub),
            "accounts-editor-actions" | "accounts-advanced" => {
                self.pane = Some(Pane::Account("claude-1".into()));
                self.preview_draft("claude-1", |draft| draft.advanced = true, cx);
            }
            "accounts-defaults" | "accounts-rules" => self.pane = Some(Pane::Rules("claude")),
            "accounts-add" => self.pane = Some(Pane::Add("claude")),
            "accounts-add-codex" => self.pane = Some(Pane::Add("codex")),
            "accounts-guide" => self.open_guide("claude".into(), window, cx),
            "accounts-uninstall" => {
                self.helpers.confirm_uninstall = Some("claude".into());
            }
            "accounts-menu" => self.helpers.menu = Some("claude".into()),
            "accounts-needs-sign-in" => self.pane = Some(Pane::Account("codex-1".into())),
            "accounts-disabled" => self.pane = Some(Pane::Account("claude-5".into())),
            "accounts-reconnect" => {
                self.pane = Some(Pane::Account("codex-1".into()));
                self.flows.insert(
                    "reconnect:codex-1".into(),
                    flow(
                        "team@example.com",
                        Some(job(
                            "codex",
                            "team@example.com",
                            "signingIn",
                            json!({ "output": "Opening the sign-in page in your browser…" }),
                        )),
                        false,
                    ),
                );
            }
            "accounts-code" => {
                self.pane = Some(Pane::Account("claude-1".into()));
                self.flows.insert(
                    "reconnect:claude-1".into(),
                    flow(
                        "mohamad.work@example.com",
                        Some(job(
                            "claude",
                            "mohamad.work@example.com",
                            "signingIn",
                            json!({}),
                        )),
                        true,
                    ),
                );
            }
            "accounts-connected" => {
                self.pane = Some(Pane::Account("claude-1".into()));
                self.flows.insert(
                    "reconnect:claude-1".into(),
                    flow(
                        "mohamad.work@example.com",
                        Some(job(
                            "claude",
                            "mohamad.work@example.com",
                            "complete",
                            json!({ "accountId": "claude-1", "output": "", "url": null }),
                        )),
                        false,
                    ),
                );
                self.connected = Some(("claude-1".into(), gpui::Task::ready(())));
            }
            "accounts-wrong" => {
                self.pane = Some(Pane::Account("codex-1".into()));
                self.flows.insert(
                    "reconnect:codex-1".into(),
                    flow(
                        "team@example.com",
                        Some(job(
                            "codex",
                            "team@example.com",
                            "failed",
                            json!({ "error": "The browser signed in to a different account.", "wrongAccount": { "signedIn": "other@example.com" } }),
                        )),
                        false,
                    ),
                );
            }
            "accounts-blockers" => {
                self.pane = Some(Pane::Account("codex-1".into()));
                self.flows.insert(
                    "reconnect:codex-1".into(),
                    flow(
                        "team@example.com",
                        Some(job(
                            "codex",
                            "team@example.com",
                            "failed",
                            json!({
                                "error": "Codex is still running and has to stop before this login can finish.",
                                "blockers": { "sessions": [
                                    { "projectId": "p", "sessionId": "s1", "title": "Fix the login check" },
                                    { "projectId": "p", "sessionId": "s2", "title": "Write release notes" }
                                ], "others": 0 }
                            }),
                        )),
                        false,
                    ),
                );
            }
            "accounts-add-waiting" => {
                self.pane = Some(Pane::Add("claude"));
                self.flows.insert(
                    "setup:claude:new".into(),
                    flow(
                        "",
                        Some(job(
                            "claude",
                            "",
                            "signingIn",
                            json!({ "email": null, "output": "Opening the sign-in page in your browser…" }),
                        )),
                        false,
                    ),
                );
            }
            "accounts-remove" => {
                self.pane = Some(Pane::Account("claude-1".into()));
                self.preview_draft("claude-1", |draft| draft.remove = true, cx);
            }
            _ => {}
        }
    }
}
