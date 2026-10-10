//! A Work workspace's Team rows: its connection to the team's own Convex project (join with an
//! invite link, copy an invite link, leave), the team's Slack app (manifest, whether its secrets
//! are stored, this person's Slack user), the team-wide Linear key and this member's own key. It
//! reads `/api/readTeamSyncStatus` and writes through `/api/joinTeamSync`,
//! `/api/createTeamSyncInvite`, `/api/leaveTeamSync`, `/api/setTeamSyncIdentity`,
//! `/api/readSlackManifest`, `/api/setTeamLinearKey` and `/api/setOwnLinearKey`.
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! Setting up a team and storing the Slack secrets run the Convex CLI with the login of the person
//! who deployed the team, which only a terminal on their computer has, so those rows show the one
//! `ghostex team …` command to run instead of a field. The team's Linear key is stored through the
//! owner's member token instead, so owners get a field. No secret value ever comes back from the
//! team, only whether it is set.
//! SEE-ALSO: server/src/team_sync/operations.rs, server/src/ghostex_cli/team_slack.rs,
//! packages/team-sync/convex/teams.ts (`info.secrets`).
use super::super::super::fields::{
    ButtonVariant, RowSpec, setting_row, settings_button, switch_control,
};
use super::super::super::store::{store_copy_to_clipboard, store_gxserver_rpc};
use super::*;

/// Every team call is a round trip to the team's Convex project.
pub(super) const TEAM_TIMEOUT: Duration = Duration::from_secs(30);

/// What Settings knows about one workspace's team.
#[derive(Default)]
pub(crate) struct TeamConnectionState {
    loading: bool,
    loaded: bool,
    /// The connection summary (with `team`, or `teamError` when the team did not answer); `None`
    /// when the workspace is not connected.
    connection: Option<Value>,
    error: Option<String>,
    busy: bool,
    confirm_leave: bool,
}

impl TeamConnectionState {
    pub(super) fn connected(&self) -> bool {
        self.connection.is_some()
    }

    fn me(&self, key: &str) -> Option<&Value> {
        self.connection
            .as_ref()?
            .pointer(&format!("/team/me/{key}"))
    }

    fn is_owner(&self) -> bool {
        self.me("role").and_then(Value::as_str) == Some("owner")
    }

    /// Whether the team's deployment holds a secret; `None` when its functions are too old to say.
    fn secret(&self, key: &str) -> Option<bool> {
        self.connection
            .as_ref()?
            .pointer(&format!("/team/secrets/{key}"))?
            .as_bool()
    }
}

/// `ghostex team <verb> --workspace <name>`, quoting a name with spaces.
pub(super) fn team_command(verb: &str, workspace_name: &str) -> String {
    let name = if workspace_name.chars().any(char::is_whitespace) {
        format!("\"{workspace_name}\"")
    } else {
        workspace_name.to_string()
    };
    format!("ghostex team {verb} --workspace {name}")
}

impl WorkspacesTab {
    /// Reads the workspace's team once, the first time its rows show.
    pub(super) fn ensure_team_loaded(&mut self, workspace_id: &str, cx: &mut Context<Self>) {
        let state = self.team.entry(workspace_id.to_string()).or_default();
        if state.loaded || state.loading {
            return;
        }
        self.load_team(workspace_id.to_string(), cx);
    }

    pub(super) fn load_team(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        self.team.entry(workspace_id.clone()).or_default().loading = true;
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readTeamSyncStatus",
            json!({ "workspaceId": workspace_id, "live": true }),
            TEAM_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let state = page.team.entry(workspace_id.clone()).or_default();
                    state.loading = false;
                    state.loaded = true;
                    match result {
                        Ok(result) => {
                            state.error = None;
                            state.connection = result
                                .get("connections")
                                .and_then(Value::as_array)
                                .and_then(|connections| connections.first())
                                .cloned();
                            if state.connection.is_some() {
                                page.load_slack_flow(workspace_id.clone(), cx);
                            }
                        }
                        Err(error) => state.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// One team call that changes something, then a fresh read of the team.
    fn team_write(
        &mut self,
        workspace_id: String,
        path: &'static str,
        params: Value,
        failure: &'static str,
        on_success: impl FnOnce(&mut Self, Value, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let state = self.team.entry(workspace_id.clone()).or_default();
        if state.busy {
            return;
        }
        state.busy = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            path,
            params,
            TEAM_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.team.entry(workspace_id.clone()).or_default().busy = false;
                    match result {
                        Ok(result) => on_success(page, result, cx),
                        Err(error) => page.toast("error", failure, &error, cx),
                    }
                    page.load_team(workspace_id, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn join_team(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let input_id = SharedString::from(format!("team-invite-{workspace_id}"));
        let link = self.draft(&input_id).trim().to_string();
        if link.is_empty() {
            return;
        }
        self.team_write(
            workspace_id.clone(),
            "/api/joinTeamSync",
            json!({ "workspaceId": workspace_id, "inviteLink": link }),
            "Couldn't join the team",
            move |page, result, cx| {
                page.drafts.remove(&input_id);
                page.fields.texts.remove(&input_id);
                let team = text(&result, "teamName");
                page.toast(
                    "success",
                    "Joined the team",
                    &if team.is_empty() {
                        "This workspace is connected to your team.".to_string()
                    } else {
                        format!("This workspace is connected to {team}.")
                    },
                    cx,
                );
            },
            cx,
        );
    }

    fn copy_invite_link(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        self.team_write(
            workspace_id.clone(),
            "/api/createTeamSyncInvite",
            json!({ "workspaceId": workspace_id }),
            "Couldn't create an invite link",
            |page, result, cx| {
                let link = text(&result, "inviteLink");
                if link.is_empty() {
                    return;
                }
                store_copy_to_clipboard(&page.store, link, cx);
                page.toast(
                    "success",
                    "Invite link copied",
                    "It works once. Send it to one teammate; they paste it in Settings → Workspaces.",
                    cx,
                );
            },
            cx,
        );
    }

    fn leave_team(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let state = self.team.entry(workspace_id.clone()).or_default();
        if !state.confirm_leave {
            state.confirm_leave = true;
            cx.notify();
            return;
        }
        state.confirm_leave = false;
        self.slack_flows.remove(&workspace_id);
        self.team_write(
            workspace_id.clone(),
            "/api/leaveTeamSync",
            json!({ "workspaceId": workspace_id }),
            "Couldn't leave the team",
            |page, result, cx| {
                if let Some(error) = result.get("remoteError").and_then(Value::as_str) {
                    page.toast(
                        "warning",
                        "Left the team on this computer",
                        &format!("The team could not be told: {error}"),
                        cx,
                    );
                }
            },
            cx,
        );
    }

    fn save_slack_user(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let input_id = SharedString::from(format!("team-slack-user-{workspace_id}"));
        let Some(slack_user) = self.drafts.get(&input_id).map(|id| id.trim().to_string()) else {
            return;
        };
        self.team_write(
            workspace_id.clone(),
            "/api/setTeamSyncIdentity",
            json!({ "workspaceId": workspace_id, "slackUserId": slack_user }),
            "Couldn't save your Slack user",
            move |page, _, _| {
                page.drafts.remove(&input_id);
            },
            cx,
        );
    }

    fn copy_slack_manifest(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readSlackManifest",
            json!({ "workspaceId": workspace_id }),
            TEAM_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| match result {
                    Ok(result) => {
                        let manifest = result.get("manifest").cloned().unwrap_or(Value::Null);
                        store_copy_to_clipboard(
                            &page.store,
                            serde_json::to_string_pretty(&manifest).unwrap_or_default(),
                            cx,
                        );
                        page.toast(
                            "success",
                            "Slack app manifest copied",
                            "At api.slack.com/apps: Create New App → From a manifest, paste it, and install the app to your Slack.",
                            cx,
                        );
                    }
                    Err(error) => page.toast("error", "Couldn't read the Slack manifest", &error, cx),
                });
            },
            cx,
        );
    }

    fn copy_command(&mut self, command: String, cx: &mut Context<Self>) {
        store_copy_to_clipboard(&self.store, command, cx);
    }

    /// A Copy command button for a `ghostex team …` command.
    fn command_button(
        &mut self,
        p: &SettingsPalette,
        id: String,
        command: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_button(
            p,
            SharedString::from(id),
            "Copy command",
            Some("modals/settings/copy.svg"),
            ButtonVariant::Outline,
            false,
            None,
            move |page: &mut Self, _window, cx| page.copy_command(command.clone(), cx),
            cx,
        )
    }

    /// The Team, Slack and team Linear rows of a Work workspace.
    pub(super) fn team_rows(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        workspace_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.ensure_team_loaded(workspace_id, cx);
        let mut rows = Vec::new();
        let (loaded, connected, busy, error) = {
            let state = self.team.entry(workspace_id.to_string()).or_default();
            (
                state.loaded,
                state.connected(),
                state.busy,
                state.error.clone(),
            )
        };
        if !loaded {
            rows.push(setting_row(
                p,
                format!("team-convex-{workspace_id}"),
                RowSpec::new("Team (Convex)")
                    .readout(if error.is_some() {
                        "Couldn't read the team"
                    } else {
                        "Checking…"
                    })
                    .description(match error {
                        Some(error) => format!("Couldn't read the team: {error}"),
                        None => "Checking your team…".to_string(),
                    }),
                None,
                div().into_any_element(),
                cx,
            ));
            return rows;
        }
        if !connected {
            rows.push(self.join_row(p, workspace_id, busy, window, cx));
            let command = team_command("deploy", workspace_name);
            let refresh_workspace = workspace_id.to_string();
            let control = h_flex()
                .gap(px(8.0))
                .child(self.command_button(
                    p,
                    format!("team-deploy-copy-{workspace_id}"),
                    command.clone(),
                    cx,
                ))
                .child(settings_button(
                    p,
                    SharedString::from(format!("team-deploy-check-{workspace_id}")),
                    "Check again",
                    None,
                    ButtonVariant::Ghost,
                    busy,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.load_team(refresh_workspace.clone(), cx)
                    },
                    cx,
                ));
            rows.push(setting_row(
                p,
                format!("team-setup-{workspace_id}"),
                RowSpec::new("Set up a new team").description(format!(
                    "One person creates the team's own Convex project and deploys Ghostex's functions to it. That needs your Convex CLI login, so run this in a terminal: {command}. It connects this workspace as the team's owner."
                )),
                None,
                control.into_any_element(),
                cx,
            ));
            return rows;
        }

        rows.push(self.connected_row(p, workspace_id, busy, cx));
        let owner = self
            .team
            .get(workspace_id)
            .is_some_and(TeamConnectionState::is_owner);
        if owner {
            let command = team_command("deploy", workspace_name);
            rows.push(setting_row(
                p,
                format!("team-setup-{workspace_id}"),
                RowSpec::new("Update the team's functions").description(format!(
                    "After a Ghostex update, deploy its functions to your team's Convex project again from a terminal: {command}"
                )),
                None,
                self.command_button(p, format!("team-deploy-copy-{workspace_id}"), command, cx),
                cx,
            ));
        }
        rows.push(self.slack_row(p, workspace_id, cx));
        rows.push(self.slack_secrets_row(p, workspace_id, workspace_name, cx));
        rows.push(self.slack_user_row(p, workspace_id, busy, window, cx));
        rows.push(self.team_linear_row(p, workspace_id, workspace_name, busy, window, cx));
        rows.push(self.own_linear_key_row(p, workspace_id, busy, cx));
        rows
    }

    fn join_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input_id = SharedString::from(format!("team-invite-{workspace_id}"));
        let input = self.draft_input(&input_id, "", "Invite link", window, cx);
        super::super::accounts::widgets::sync_masked(
            &mut self.masked,
            &input_id,
            &input,
            true,
            window,
            cx,
        );
        let empty = self.draft(&input_id).trim().is_empty();
        let join_workspace = workspace_id.to_string();
        let control = h_flex()
            .gap(px(8.0))
            .child(settings_text_input(
                p,
                &input,
                Some(220.0),
                true,
                window,
                cx,
            ))
            .child(settings_button(
                p,
                SharedString::from(format!("team-join-{workspace_id}")),
                if busy { "Joining…" } else { "Join" },
                None,
                ButtonVariant::Outline,
                busy || empty,
                None,
                move |page: &mut Self, _window, cx| page.join_team(join_workspace.clone(), cx),
                cx,
            ));
        setting_row(
            p,
            input_id,
            RowSpec::new("Team (Convex)").readout("Not connected").description(
                "Not connected. Paste the invite link a teammate sent you to join your team's own Convex project: it keeps tickets, Slack threads and commands in sync for the whole team.",
            ),
            None,
            control.into_any_element(),
            cx,
        )
    }

    fn connected_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.team.get(workspace_id);
        let connection = state
            .and_then(|state| state.connection.clone())
            .unwrap_or_default();
        let owner = state.is_some_and(TeamConnectionState::is_owner);
        let confirm_leave = state.is_some_and(|state| state.confirm_leave);
        let member = connection
            .pointer("/team/me/name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| text(&connection, "memberName"));
        let team_name = connection
            .pointer("/team/team/name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| text(&connection, "teamName"));
        let members = connection
            .pointer("/team/members")
            .and_then(Value::as_array)
            .map(Vec::len);
        let mut status = format!(
            "{} · {} ({})",
            if team_name.is_empty() {
                "Connected"
            } else {
                &team_name
            },
            if member.is_empty() { "you" } else { &member },
            if owner { "owner" } else { "member" },
        );
        if let Some(members) = members {
            status.push_str(&format!(
                " · {members} {}",
                if members == 1 { "member" } else { "members" }
            ));
        }
        let mut description = "Your team's own Convex project keeps tickets, Slack threads and commands in sync for the whole team. Copy invite link makes a one-time link for one teammate.".to_string();
        if let Some(error) = connection.get("teamError").and_then(Value::as_str) {
            status = "The team did not answer".to_string();
            description =
                format!("Connected, but the team's Convex project did not answer: {error}");
        }
        if confirm_leave {
            status = "Click Leave again to leave".to_string();
            description =
                "This workspace stops receiving the team's tickets and Slack commands.".to_string();
        }
        let invite_workspace = workspace_id.to_string();
        let leave_workspace = workspace_id.to_string();
        let control = h_flex()
            .gap(px(8.0))
            .when(owner, |row| {
                row.child(settings_button(
                    p,
                    SharedString::from(format!("team-invite-copy-{workspace_id}")),
                    "Copy invite link",
                    Some("modals/settings/copy.svg"),
                    ButtonVariant::Outline,
                    busy,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.copy_invite_link(invite_workspace.clone(), cx)
                    },
                    cx,
                ))
            })
            .child(settings_button(
                p,
                SharedString::from(format!("team-leave-{workspace_id}")),
                if confirm_leave { "Leave" } else { "Leave…" },
                None,
                ButtonVariant::Ghost,
                busy,
                None,
                move |page: &mut Self, _window, cx| page.leave_team(leave_workspace.clone(), cx),
                cx,
            ));
        setting_row(
            p,
            format!("team-convex-{workspace_id}"),
            RowSpec::new("Team (Convex)")
                .readout(status)
                .description(description),
            None,
            control.into_any_element(),
            cx,
        )
    }

    fn slack_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.team.get(workspace_id);
        let token = state.and_then(|state| state.secret("slackBotToken"));
        let secret = state.and_then(|state| state.secret("slackSigningSecret"));
        let slack_team = state
            .and_then(|state| state.connection.as_ref())
            .and_then(|connection| connection.pointer("/team/team/slackTeamId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let (readout, status) = match (token, secret, slack_team) {
            (Some(true), Some(true), Some(team)) => (
                "Connected".to_string(),
                format!("Connected to Slack team {team}."),
            ),
            (Some(true), Some(true), None) => (
                "Waiting for Slack".to_string(),
                "The tokens are stored. Waiting for Slack's first event: invite @Ghostex to a channel.".to_string(),
            ),
            (None, _, _) | (_, None, _) => (
                "Unknown".to_string(),
                "Deploy the team's functions again to see whether Slack is connected.".to_string(),
            ),
            _ => (
                "Not connected".to_string(),
                "Copy the app manifest, create the app at api.slack.com/apps (From a manifest), install it, then store its tokens below.".to_string(),
            ),
        };
        let manifest_workspace = workspace_id.to_string();
        setting_row(
            p,
            format!("team-slack-{workspace_id}"),
            RowSpec::new("Slack").readout(readout).description(format!(
                "{status} One Slack app for the whole team; it sends its events to your team's Convex project."
            )),
            None,
            settings_button(
                p,
                SharedString::from(format!("team-slack-manifest-{workspace_id}")),
                "Copy app manifest",
                Some("modals/settings/copy.svg"),
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.copy_slack_manifest(manifest_workspace.clone(), cx)
                },
                cx,
            ),
            cx,
        )
    }

    fn slack_secrets_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        workspace_name: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.team.get(workspace_id);
        let token = state.and_then(|state| state.secret("slackBotToken"));
        let secret = state.and_then(|state| state.secret("slackSigningSecret"));
        let shown = |stored: Option<bool>, prefix: &str| match stored {
            Some(true) => format!("{prefix}••••••••••••"),
            Some(false) => "not set".to_string(),
            None => "unknown".to_string(),
        };
        let command = team_command("slack-connect", workspace_name);
        setting_row(
            p,
            format!("team-slack-secrets-{workspace_id}"),
            RowSpec::new("Slack bot token and signing secret")
                .readout(match (token, secret) {
                    (Some(true), Some(true)) => "Stored",
                    (Some(false), _) | (_, Some(false)) => "Missing",
                    _ => "Unknown",
                })
                .description(format!(
                "Bot token {} · Signing secret {}. They are kept in your team's Convex project, never on this computer. Storing them needs the Convex CLI login of the person who deployed the team: run {command} in a terminal there and paste both when asked.",
                shown(token, "xoxb-"),
                shown(secret, ""),
            )),
            None,
            self.command_button(p, format!("team-slack-connect-copy-{workspace_id}"), command, cx),
            cx,
        )
    }

    fn slack_user_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saved = self
            .team
            .get(workspace_id)
            .and_then(|state| state.me("slackUserId"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let input_id = SharedString::from(format!("team-slack-user-{workspace_id}"));
        let input = self.draft_input(&input_id, &saved, "U0123456789", window, cx);
        let changed = self.draft_changed(&input_id, &saved);
        let save_workspace = workspace_id.to_string();
        let control = h_flex()
            .gap(px(8.0))
            .child(settings_text_input(
                p,
                &input,
                Some(160.0),
                true,
                window,
                cx,
            ))
            .child(settings_button(
                p,
                SharedString::from(format!("team-slack-user-save-{workspace_id}")),
                "Save",
                None,
                ButtonVariant::Outline,
                busy || !changed,
                None,
                move |page: &mut Self, _window, cx| {
                    page.save_slack_user(save_workspace.clone(), cx)
                },
                cx,
            ));
        setting_row(
            p,
            input_id,
            RowSpec::new("Your Slack user").description(if saved.is_empty() {
                "Not set. Your Slack member ID (your Slack profile → ⋯ → Copy member ID), so @Ghostex commands you send reach this computer."
            } else {
                "@Ghostex commands you send from this Slack user reach this computer."
            }),
            None,
            control.into_any_element(),
            cx,
        )
    }

    fn save_team_linear_key(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let input_id = SharedString::from(format!("team-linear-key-{workspace_id}"));
        let key = self.draft(&input_id).trim().to_string();
        if key.is_empty() {
            return;
        }
        self.team_write(
            workspace_id.clone(),
            "/api/setTeamLinearKey",
            json!({ "workspaceId": workspace_id, "apiKey": key }),
            "Couldn't save the team's Linear key",
            move |page, _, _| {
                page.drafts.remove(&input_id);
                page.fields.texts.remove(&input_id);
            },
            cx,
        );
    }

    fn remove_team_linear_key(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        self.team_write(
            workspace_id.clone(),
            "/api/setTeamLinearKey",
            json!({ "workspaceId": workspace_id }),
            "Couldn't remove the team's Linear key",
            |_, _, _| {},
            cx,
        );
    }

    fn set_own_linear_key(&mut self, workspace_id: String, enabled: bool, cx: &mut Context<Self>) {
        self.team_write(
            workspace_id.clone(),
            "/api/setOwnLinearKey",
            json!({ "workspaceId": workspace_id, "enabled": enabled }),
            if enabled {
                "Couldn't use your own Linear key"
            } else {
                "Couldn't stop using your own Linear key"
            },
            |_, _, _| {},
            cx,
        );
    }

    /// The team's Linear key: owners paste, replace or remove it; members see whether it is set.
    ///
    /// CDXC:TeamSync 2026-10-09 DECISION:
    /// User: "let's just use 1 key from the owner but also allow the user to override by setting
    /// their own key". Only owners set or remove the team's key; tickets created with it show the
    /// key's owner as the creator in Linear, which the row's tooltip says.
    fn team_linear_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        workspace_name: &str,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.team.get(workspace_id);
        let owner = state.is_some_and(TeamConnectionState::is_owner);
        let stored = state.and_then(|state| state.secret("linearApiKey"));
        let team_key = state
            .and_then(|state| state.connection.as_ref())
            .and_then(|connection| connection.pointer("/team/linearKeys/team"))
            .filter(|key| key.is_object())
            .cloned();
        let named = |key: &str| {
            team_key
                .as_ref()
                .and_then(|team| team.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let account = named("linearUserName");
        let creator = account
            .clone()
            .or_else(|| named("setByName"))
            .unwrap_or_else(|| "the key's owner".to_string());
        let set_by = named("setByName")
            .map(|name| format!("Set by {name}"))
            .unwrap_or_else(|| "Set".to_string());
        let on_account = account
            .map(|account| format!(" with {account}'s Linear account"))
            .unwrap_or_default();
        let description = match (stored, owner) {
            (Some(true), true) => format!(
                "{set_by}{on_account}. Slack commands find and create tickets with it, so tickets created with it show {creator} as the creator in Linear. Paste a new key to replace it, or run {} in a terminal.",
                team_command("linear-connect", workspace_name)
            ),
            (Some(true), false) => format!(
                "{set_by}{on_account}. Tickets created from Slack use it and show {creator} as the creator in Linear, unless you create yours with your own key below. Only the team's owners can change it."
            ),
            (Some(false), true) => "Not set: Slack commands can't find or create tickets while your computer is off. Paste a Linear API key (Linear → Settings → Security & access → Personal API keys); tickets created with it show the key's owner as the creator in Linear.".to_string(),
            (Some(false), false) => "Not set: Slack commands can't find or create tickets while your computer is off. Ask one of the team's owners to set it.".to_string(),
            (None, _) => "Deploy the team's functions again to see whether it is set.".to_string(),
        };
        let readout = match stored {
            Some(true) => set_by.clone(),
            Some(false) => "Not set".to_string(),
            None => "Unknown".to_string(),
        };
        let control = if owner && stored.is_some() {
            let input_id = SharedString::from(format!("team-linear-key-{workspace_id}"));
            let input = self.draft_input(&input_id, "", "lin_api_…", window, cx);
            super::super::accounts::widgets::sync_masked(
                &mut self.masked,
                &input_id,
                &input,
                true,
                window,
                cx,
            );
            let empty = self.draft(&input_id).trim().is_empty();
            let save_workspace = workspace_id.to_string();
            let remove_workspace = workspace_id.to_string();
            h_flex()
                .gap(px(8.0))
                .child(settings_text_input(
                    p,
                    &input,
                    Some(180.0),
                    true,
                    window,
                    cx,
                ))
                .child(settings_button(
                    p,
                    SharedString::from(format!("team-linear-save-{workspace_id}")),
                    "Save",
                    None,
                    ButtonVariant::Outline,
                    busy || empty,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.save_team_linear_key(save_workspace.clone(), cx)
                    },
                    cx,
                ))
                .when(stored == Some(true), |row| {
                    row.child(settings_button(
                        p,
                        SharedString::from(format!("team-linear-remove-{workspace_id}")),
                        "Remove",
                        None,
                        ButtonVariant::Ghost,
                        busy,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.remove_team_linear_key(remove_workspace.clone(), cx)
                        },
                        cx,
                    ))
                })
                .into_any_element()
        } else {
            div().into_any_element()
        };
        setting_row(
            p,
            format!("team-linear-{workspace_id}"),
            RowSpec::new("Linear for the team")
                .readout(readout)
                .description(description),
            None,
            control,
            cx,
        )
    }

    /// "Create my Slack tickets with my own Linear key": this workspace's Linear key, kept in the
    /// team's Convex project for this member only (server/src/team_sync/linear_keys.rs).
    fn own_linear_key_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connection = self
            .team
            .get(workspace_id)
            .and_then(|state| state.connection.clone())
            .unwrap_or_default();
        let enabled = connection.get("ownLinearKey").and_then(Value::as_bool) == Some(true);
        let stored = connection
            .pointer("/team/linearKeys/mine")
            .is_some_and(Value::is_object);
        let has_key =
            self.has_linear_key(workspace_id) || self.has_linear_key(DEFAULT_WORKSPACE_ID);
        let admins = "The key is stored in your team's Convex project for you only; the team's Convex admins can technically read it.";
        let description = match (enabled, stored, has_key) {
            (true, true, _) => format!(
                "On: tickets you request from Slack are created with this workspace's Linear key, so Linear shows you as their creator; everything else uses the team's key. Ghostex updates it when you change the key above and removes it when you turn this off or leave the team. {admins}"
            ),
            (true, false, false) => "On, but this workspace has no Linear key, so your tickets use the team's key. Save your Linear API key above.".to_string(),
            (true, false, true) => "On, but your key isn't stored in the team yet. Turn it off and on again to retry.".to_string(),
            (false, _, _) => format!(
                "Off: tickets you request from Slack are created with the team's key. Turn on to have Linear show you as their creator: Ghostex stores this workspace's Linear key in the team and keeps it up to date. {admins}"
            ),
        };
        let disabled_reason = (!enabled && !has_key)
            .then(|| SharedString::from("Save this workspace's Linear API key first."));
        let toggle_workspace = workspace_id.to_string();
        setting_row(
            p,
            format!("team-own-linear-{workspace_id}"),
            RowSpec::new("Create my Slack tickets with my own Linear key").description(description),
            None,
            switch_control(
                p,
                SharedString::from(format!("team-own-linear-switch-{workspace_id}")),
                "Create my Slack tickets with my own Linear key",
                enabled,
                busy || disabled_reason.is_some(),
                disabled_reason,
                move |page: &mut Self, checked, _window, cx| {
                    page.set_own_linear_key(toggle_workspace.clone(), checked, cx)
                },
                cx,
            ),
            cx,
        )
    }
}
