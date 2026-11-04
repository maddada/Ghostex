//! The Integrations page with a remote machine picked (../machine_picker.rs): that machine's
//! Ghostex CLI and bundled agent skills from its gxserver's `/api/machineIntegrations`, its Tools
//! from its `/api/managedTools` (tools.rs), Fast Computer & Browser Use and SpaceO among them, each
//! in its own section with its skills (tool_sections.rs; CDXC:ManagedTools 2026-10-26 in
//! server/src/managed_tools/desktop_control.rs: Macs only, and an install without its macOS grants
//! says who must allow them). Skills install and uninstall through that gxserver; a skill the machine's platform can't run says so instead
//! of offering an install.
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_machine_gxserver_rpc;
use super::tool_sections::{self, SkillHome, skill_home};
use super::{
    ICON_DEVICE_DESKTOP, ICON_DOWNLOAD, ICON_REFRESH, ICON_TERMINAL, ICON_TRASH, IntegrationsTab,
    RowTitle, SKILL_MAIN_BUTTON_WIDTH, Skill, catalog_skills, ghost_icon_button, integration_row,
    skill_icon, skill_invocation, trycua_name,
};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::v_flex;
use serde_json::{Value, json};
use std::time::Duration;

const ENDPOINT: &str = "/api/machineIntegrations";
/// An install downloads the skill from GitHub first.
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(super) struct RemoteIntegrations {
    /// The machine's last `status` answer.
    status: Option<Value>,
    error: Option<String>,
    loading: bool,
    /// The skill whose install or uninstall is in flight.
    busy_skill: Option<String>,
}

/// One skill row's state on the machine.
fn remote_skill<'a>(status: Option<&'a Value>, skill_name: &str) -> Option<&'a Value> {
    status?
        .get("skills")?
        .as_array()?
        .iter()
        .find(|skill| skill.get("skillName").and_then(Value::as_str) == Some(skill_name))
}

/// Which machines can have them.
fn platform_note(platform: Option<&str>) -> &'static str {
    match platform {
        Some("macos") | None => "Macs only.",
        Some(_) => "Macs only, so not on this machine.",
    }
}

impl RemoteIntegrations {
    /// The machine's platform from its last answer (`windows`, `macos`, `linux`).
    pub(super) fn platform(&self) -> Option<String> {
        self.status
            .as_ref()?
            .get("platform")?
            .as_str()
            .map(str::to_string)
    }
}

impl IntegrationsTab {
    fn remote_request(
        &mut self,
        params: Value,
        on_done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let machine = self.shown_machine.clone();
        self.remote.loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_machine_gxserver_rpc(
            &self.store.clone(),
            machine.clone().as_deref(),
            ENDPOINT,
            params,
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if page.shown_machine != machine {
                        return;
                    }
                    page.remote.loading = false;
                    match result {
                        Ok(status) => {
                            page.remote.status = Some(status);
                            page.remote.error = None;
                        }
                        Err(error) => page.remote.error = Some(error),
                    }
                    on_done(page, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    pub(super) fn load_remote_integrations(&mut self, cx: &mut Context<Self>) {
        if self.shown_machine.is_some() {
            self.remote_request(json!({ "action": "status" }), |_, _| {}, cx);
        }
    }

    fn remote_skill_action(
        &mut self,
        action: &'static str,
        skill: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        self.remote.busy_skill = Some(skill.clone());
        let machine = self.shown_machine_label();
        self.remote_request(
            json!({ "action": action, "skillName": skill }),
            move |page, cx| {
                page.remote.busy_skill = None;
                let (level, title, description) = match (&page.remote.error, action) {
                    (Some(error), "installSkill") => {
                        ("warning", format!("{name} install failed"), error.clone())
                    }
                    (Some(error), _) => {
                        ("warning", format!("{name} uninstall failed"), error.clone())
                    }
                    (None, "installSkill") => (
                        "success",
                        format!("{name} installed"),
                        format!("Agents on {machine} can use it now."),
                    ),
                    (None, _) => (
                        "success",
                        format!("{name} uninstalled"),
                        format!("Removed from {machine}."),
                    ),
                };
                page.store
                    .update(cx, |store, cx| store.toast(level, &title, &description, cx));
            },
            cx,
        );
    }

    pub(super) fn shown_machine_label(&self) -> String {
        self.shown_machine_name
            .clone()
            .unwrap_or_else(|| "the remote machine".to_string())
    }

    /// The sections of a picked remote machine, before its Tools.
    pub(super) fn remote_sections(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Vec<(&'static str, AnyElement)> {
        let machine = self.shown_machine_label();
        let status = self.remote.status.clone();
        let status = status.as_ref();
        let loading = self.remote.loading;
        let mut sections = Vec::new();
        if let Some(error) = self.remote.error.clone() {
            let row = integration_row(
                p,
                "remote-integrations-error",
                Some(ListItemStatus::Warning),
                None,
                RowTitle {
                    label: error,
                    description: format!("Ghostex asked {machine}'s Ghostex for its integrations."),
                    badge: None,
                    pill: None,
                    link: None,
                },
                vec![settings_button(
                    p,
                    "remote-integrations-retry",
                    "Try again",
                    Some(ICON_REFRESH),
                    ButtonVariant::Outline,
                    loading,
                    None,
                    |page: &mut Self, _window, cx| page.load_remote_integrations(cx),
                    cx,
                )],
            );
            sections.extend(
                settings_section(
                    p,
                    format!("Integrations on {machine}"),
                    None,
                    None,
                    vec![row],
                )
                .map(|section| ("remoteError", section.into_any_element())),
            );
        }
        let platform = status
            .and_then(|status| status.get("platform"))
            .and_then(Value::as_str);
        // Ghostex CLI: installed with the machine's Ghostex, so there is nothing to repair from here.
        if let Some(status) = status {
            let cli = status.get("ghostexCli");
            let installed = cli
                .and_then(|cli| cli.get("installed"))
                .and_then(Value::as_bool)
                == Some(true);
            let path = cli
                .and_then(|cli| cli.get("path"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let row = integration_row(
                p,
                "remote-ghostex-cli",
                Some(if installed {
                    ListItemStatus::Success
                } else {
                    ListItemStatus::Warning
                }),
                Some(ICON_TERMINAL),
                RowTitle {
                    label: "Command line tool".to_string(),
                    description: if installed {
                        format!("New terminals on {machine} run {path}.")
                    } else {
                        format!(
                            "New terminals on {machine} don't find a ghostex command. Ghostex links it when it installs there; reconnect from Settings > Remote to set it up again."
                        )
                    },
                    badge: None,
                    pill: (!installed).then(|| "Not found".to_string()),
                    link: None,
                },
                vec![settings_button(
                    p,
                    "remote-ghostex-cli-refresh",
                    "Refresh",
                    Some(ICON_REFRESH),
                    ButtonVariant::Ghost,
                    loading,
                    Some("Checking…".into()),
                    |page: &mut Self, _window, cx| page.load_remote_integrations(cx),
                    cx,
                )],
            );
            sections.extend(
                settings_section(p, "Ghostex CLI", None, None, vec![row])
                    .map(|section| ("ghostexCli", section.into_any_element())),
            );
            if let Some(skills) = self.remote_skills_section(p, Some(status), &machine, cx) {
                sections.push(("agentSkills", skills));
            }
            // The managed tools with their own sections (server/src/managed_tools/desktop_control.rs).
            let mut trycua_rows = self.managed_rows_for(p, &["trycua"], cx);
            let mut spaceo_rows = self.managed_rows_for(p, &["spaceo"], cx);
            // A gxserver from before these tools lists the others without them.
            if trycua_rows.is_empty() && spaceo_rows.is_empty() && self.managed.has_tools() {
                trycua_rows.push(integration_row(
                    p,
                    "remote-desktop-control-old",
                    Some(ListItemStatus::Warning),
                    Some(ICON_DEVICE_DESKTOP),
                    RowTitle {
                        label: format!("Update Ghostex on {machine} first"),
                        description: format!(
                            "{machine} runs an older Ghostex that can't install {} or SpaceO from here. Update it in Settings > Remote, then come back.",
                            trycua_name()
                        ),
                        badge: None,
                        pill: None,
                        link: None,
                    },
                    Vec::new(),
                ));
            }
            let grants_note = format!(
                "Only someone at {machine} can allow Accessibility and Screen Recording after an install. {}",
                platform_note(platform)
            );
            trycua_rows.extend(self.remote_skill_rows(
                p,
                SkillHome::ComputerBrowserUse,
                Some(status),
                &machine,
                cx,
            ));
            sections.extend(
                settings_section(
                    p,
                    tool_sections::COMPUTER_BROWSER_USE_TITLE,
                    Some(
                        format!("Lets agents use {machine}'s apps and browser. {grants_note}")
                            .into(),
                    ),
                    None,
                    trycua_rows,
                )
                .map(|section| ("computerBrowserUse", section.into_any_element())),
            );
            spaceo_rows.extend(self.remote_skill_rows(
                p,
                SkillHome::HiddenScreen,
                Some(status),
                &machine,
                cx,
            ));
            sections.extend(
                settings_section(
                    p,
                    tool_sections::HIDDEN_SCREEN_TITLE,
                    Some(grants_note.into()),
                    None,
                    spaceo_rows,
                )
                .map(|section| ("hiddenScreen", section.into_any_element())),
            );
            let embedded_rows =
                self.remote_skill_rows(p, SkillHome::EmbeddedBrowser, Some(status), &machine, cx);
            sections.extend(
                settings_section(
                    p,
                    tool_sections::EMBEDDED_BROWSER_TITLE,
                    None,
                    None,
                    embedded_rows,
                )
                .map(|section| ("embeddedBrowser", section.into_any_element())),
            );
        } else if self.remote.error.is_none() {
            let row = integration_row(
                p,
                "remote-integrations-loading",
                Some(ListItemStatus::Neutral),
                None,
                RowTitle {
                    label: format!("Reading the integrations on {machine}…"),
                    description: String::new(),
                    badge: None,
                    pill: None,
                    link: None,
                },
                Vec::new(),
            );
            sections.extend(
                settings_section(
                    p,
                    format!("Integrations on {machine}"),
                    None,
                    None,
                    vec![row],
                )
                .map(|section| ("remoteLoading", section.into_any_element())),
            );
        }
        sections
    }

    /// The rows of the bundled skills that sit in `home` and that this machine's answer lists.
    fn remote_skill_rows(
        &mut self,
        p: &SettingsPalette,
        home: SkillHome,
        status: Option<&Value>,
        machine: &str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for skill in catalog_skills(true) {
            if skill_home(&skill.id) != home {
                continue;
            }
            let Some(remote) = remote_skill(status, &skill.skill_name) else {
                continue;
            };
            rows.push(self.remote_skill_row(p, &skill, remote, machine, cx));
        }
        rows
    }

    fn remote_skill_row(
        &mut self,
        p: &SettingsPalette,
        skill: &Skill,
        remote: &Value,
        machine: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.remote.loading || self.remote.busy_skill.is_some();
        let installed = remote.get("installed").and_then(Value::as_bool) == Some(true);
        let unavailable = remote
            .get("unavailableReason")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut controls = Vec::new();
        if unavailable.is_none() {
            if installed {
                let skill_name = skill.skill_name.clone();
                let name = skill.name.clone();
                controls.push(ghost_icon_button(
                    p,
                    SharedString::from(format!("remote-skill-uninstall-{}", skill.id)),
                    ICON_TRASH,
                    p.foreground,
                    format!("Uninstall {} from {machine}", skill.name),
                    busy,
                    "Wait for the current change to finish.".to_string(),
                    move |page, _window, cx| {
                        page.remote_skill_action(
                            "uninstallSkill",
                            skill_name.clone(),
                            name.clone(),
                            cx,
                        )
                    },
                    cx,
                ));
            }
            let skill_name = skill.skill_name.clone();
            let name = skill.name.clone();
            let working = self.remote.busy_skill.as_deref() == Some(skill.skill_name.as_str());
            controls.push(
                v_flex()
                    .flex_shrink_0()
                    .w(px(SKILL_MAIN_BUTTON_WIDTH))
                    .child(settings_button(
                        p,
                        SharedString::from(format!("remote-skill-install-{}", skill.id)),
                        if working {
                            "Working…"
                        } else if installed {
                            "Reinstall"
                        } else {
                            "Install"
                        },
                        Some(if installed {
                            ICON_REFRESH
                        } else {
                            ICON_DOWNLOAD
                        }),
                        if installed {
                            ButtonVariant::Ghost
                        } else {
                            ButtonVariant::Outline
                        },
                        busy,
                        Some("Wait for the current change to finish.".into()),
                        move |page: &mut Self, _window, cx| {
                            page.remote_skill_action(
                                "installSkill",
                                skill_name.clone(),
                                name.clone(),
                                cx,
                            )
                        },
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        integration_row(
            p,
            &format!("remote-skill-{}", skill.id),
            Some(if installed {
                ListItemStatus::Success
            } else {
                ListItemStatus::Neutral
            }),
            Some(skill_icon(&skill.id)),
            RowTitle {
                link: (installed && !skill.skill_name.is_empty())
                    .then(|| skill_invocation(p, &skill.skill_name)),
                label: skill.name.clone(),
                description: format!("{}\n\n{}", skill.description, skill.command),
                badge: None,
                pill: unavailable.or_else(|| {
                    skill
                        .requires_cua_driver
                        .then(|| format!("Needs {} there", trycua_name()))
                }),
            },
            controls,
        )
    }

    fn remote_skills_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        machine: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let rows = self.remote_skill_rows(p, SkillHome::AgentSkills, status, machine, cx);
        settings_section(
            p,
            "Agent skills",
            Some(
                format!(
                    "Installed for every agent on {machine}. {}",
                    tool_sections::TOOL_SKILLS_POINTER
                )
                .into(),
            ),
            None,
            rows.into_iter()
                .map(|row| div().w_full().child(row).into_any_element())
                .collect(),
        )
        .map(IntoElement::into_any_element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skill_row_reads_its_own_entry() {
        let status = json!({
            "platform": "linux",
            "skills": [
                { "skillName": "ghostex-cli", "installed": true, "unavailableReason": null },
                { "skillName": "ghostex-spaceo", "installed": false, "unavailableReason": "SpaceO is only available on macOS." },
            ],
        });
        assert_eq!(
            remote_skill(Some(&status), "ghostex-cli")
                .and_then(|skill| skill["installed"].as_bool()),
            Some(true)
        );
        assert_eq!(
            remote_skill(Some(&status), "ghostex-spaceo")
                .and_then(|skill| skill["unavailableReason"].as_str()),
            Some("SpaceO is only available on macOS.")
        );
        assert!(remote_skill(Some(&status), "ghostex-help").is_none());
        assert_eq!(
            platform_note(Some("linux")),
            "Macs only, so not on this machine."
        );
    }
}
