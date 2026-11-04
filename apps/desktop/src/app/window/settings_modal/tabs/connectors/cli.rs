//! A workspace's own sign-ins for the GitHub CLI (gh) and the Google Workspace CLI (gws), which
//! agents started in that workspace use.
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_list_item, settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::ConnectorsTab;
use super::data::{busy_key, flag, text};
use super::widgets::{detail, note_row};
use gpui::{AnyElement, Context, Div, IntoElement as _, ParentElement as _, Styled as _, px};
use gpui_component::h_flex;
use serde_json::Value;

const TOOLS: [(&str, &str); 2] = [
    ("gh", "GitHub CLI (gh)"),
    ("gws", "Google Workspace CLI (gws)"),
];

impl ConnectorsTab {
    pub(super) fn cli_section(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        title: String,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let cli = workspace.get("cli")?;
        let id = text(workspace, "workspaceId");
        let offline = !self.rpc_available(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        for (tool, label) in TOOLS {
            let Some(state) = cli.get(tool) else {
                continue;
            };
            let installed = state.get("installed").and_then(Value::as_bool) != Some(false);
            let signed_in = flag(state, "signedIn");
            let account = text(state, "account");
            let line = if !installed {
                "Not installed on this computer".to_string()
            } else if signed_in && !account.is_empty() {
                format!("Signed in as {account}")
            } else if signed_in {
                "Signed in".to_string()
            } else if flag(state, "needsSignInAgain") {
                // The token is in the system keychain, which some shells can't read: agents keep
                // using the usual gh until the workspace signs in again.
                if account.is_empty() {
                    "Sign-in needs redoing: its token is in the keychain, not this workspace's folder. Agents use your usual sign-in until you sign in again".to_string()
                } else {
                    format!("{account}'s sign-in needs redoing: its token is in the keychain, not this workspace's folder. Agents use your usual sign-in until you sign in again")
                }
            } else {
                "Not signed in for this workspace".to_string()
            };
            // gws signs in with a Google OAuth client: the workspace's own, or the user's usual one.
            let client = state.get("client").filter(|client| client.is_object());
            let line = match client {
                Some(client) => {
                    let project = text(client, "project");
                    let whose = if flag(client, "own") {
                        "this workspace's own"
                    } else {
                        "your usual"
                    };
                    if project.is_empty() {
                        format!("{line} · Google OAuth client: {whose}")
                    } else {
                        format!("{line} · Google OAuth client: {whose} ({project})")
                    }
                }
                None => line,
            };
            let control = installed.then(|| {
                let target = id.clone();
                if signed_in {
                    let busy = self.is_busy(&id, &format!("cli-sign-out-{tool}"));
                    settings_button(
                        p,
                        format!("connectors-{id}-cli-sign-out-{tool}"),
                        "Sign out",
                        None,
                        ButtonVariant::Outline,
                        offline || busy,
                        None,
                        move |page: &mut Self, _window, cx| page.cli_sign_out(&target, tool, cx),
                        cx,
                    )
                } else {
                    let busy = self.is_busy(&id, &format!("cli-sign-in-{tool}"));
                    settings_button(
                        p,
                        format!("connectors-{id}-cli-sign-in-{tool}"),
                        if busy { "Opening…" } else { "Sign in" },
                        None,
                        ButtonVariant::Outline,
                        offline || busy,
                        None,
                        move |page: &mut Self, _window, cx| page.cli_sign_in(&target, tool, cx),
                        cx,
                    )
                }
            });
            let control = match (control, client) {
                (Some(control), Some(client)) => {
                    let mut buttons = h_flex().gap(px(8.0)).child(control);
                    let busy = self.is_busy(&id, "cli-client-gws");
                    let target = id.clone();
                    buttons = buttons.child(settings_button(
                        p,
                        format!("connectors-{id}-cli-client-choose"),
                        "Choose client…",
                        None,
                        ButtonVariant::Outline,
                        offline || busy,
                        None,
                        move |page: &mut Self, _window, cx| page.cli_choose_client(&target, cx),
                        cx,
                    ));
                    if flag(client, "own") {
                        let target = id.clone();
                        buttons = buttons.child(settings_button(
                            p,
                            format!("connectors-{id}-cli-client-usual"),
                            "Use usual",
                            None,
                            ButtonVariant::Outline,
                            offline || busy,
                            None,
                            move |page: &mut Self, _window, cx| {
                                page.cli_set_client(&target, None, cx)
                            },
                            cx,
                        ));
                    }
                    Some(buttons.into_any_element())
                }
                (control, _) => control,
            };
            rows.push(settings_list_item(
                p,
                Some(if signed_in {
                    ListItemStatus::Success
                } else {
                    ListItemStatus::Neutral
                }),
                None,
                label,
                Some(detail(line)),
                control,
            ));
        }
        if let Some(note) = self.notes.get(&busy_key(&id, "cli")) {
            rows.push(note_row(p, note));
        }
        for (tool, _) in TOOLS {
            rows.extend(self.sign_in_panel(p, &id, &format!("cli-{tool}"), cx));
        }
        settings_section(
            p,
            title,
            Some("Agents started in this workspace use these sign-ins, so work and personal accounts stay apart.".into()),
            None,
            rows,
        )
    }
}
