//! The Defaults card of Settings > Agents: Remember the agent you pick, Default Prompt Agent, Title Generation Agent (the exact
//! command in its info tooltip), Custom Title Command, Agent approvals
//! (`AgentApprovalPolicyControl`, packages/core-ui/agent-approval-policy-control.tsx (deleted 2026-10-01)) with its
//! Skip permissions? confirmation, and Trust project folders automatically.
use super::super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::super::fields::{
    ButtonVariant, PageAction, RowSpec, SELECT_WIDTH, SettingsDialogSpec, dialog_footer,
    home_secret_folders_field, select_field, setting_row, settings_button, settings_dialog,
    settings_section, settings_segmented, settings_select, static_note, text_field, toggle_field,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::search::{TabSearch, should_show_setting};
use super::AgentsTab;
use super::model::{
    agents_from_hud, resolve_title_generation_command, title_generation_options,
    title_generation_preview,
};
use super::select::DropdownOption;
use gpui::{AnyElement, Context, Div, IntoElement, Window};
use serde_json::{Value, json};
use std::rc::Rc;

const DEFAULT_PROMPT_AGENT: &str = "defaultPromptAgentId";
const TITLE_AGENT: &str = "sessionTitleGenerationAgent";
const CUSTOM_TITLE_COMMAND: &str = "customSessionTitleGenerationCommand";
const ACCEPT_ALL: &str = "agentAcceptAllEnabled";
const TRUST_PROJECT_FOLDERS: &str = "trustProjectFoldersAutomatically";
const SHOW_ON_ALL_SPACES: &str = "chiefShowOnAllSpaces";
const CONFIRM_NEW_CHAT: &str = "chiefConfirmNewChat";
const HOME_SECRET_FOLDERS: &str = "homeSecretFolders";
/// How the agent a project's new-session button starts is remembered (gx-core `agent_choice.rs`).
const NEW_SESSION_AGENT_SCOPE: &str = "newSessionAgentScope";
/// A shadcn `Select` with this many items becomes the searchable dropdown.
const SEARCHABLE_SELECT_ITEMS: usize = 8;

fn default_text(key: &str) -> String {
    settings_catalog()
        .default_value(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn reset_to(key: &'static str) -> PageAction<AgentsTab> {
    Rc::new(move |page: &mut AgentsTab, _window, cx| {
        let default = settings_catalog()
            .default_value(key)
            .cloned()
            .unwrap_or(Value::Null);
        page.save(key, default, cx);
    })
}

impl AgentsTab {
    pub(super) fn render_config(
        &mut self,
        p: &SettingsPalette,
        search: &TabSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let section = search.section("config");
        let (values, agents) = {
            let store = self.store.read(cx);
            (store.values(), agents_from_hud(store.hud()))
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        if should_show_setting(&section, NEW_SESSION_AGENT_SCOPE, None) {
            let options =
                settings_catalog().options(module::SETTINGS, "NEW_SESSION_AGENT_SCOPE_OPTIONS");
            let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            let scope = values.choice(NEW_SESSION_AGENT_SCOPE, &allowed);
            rows.push(select_field(
                self,
                p,
                NEW_SESSION_AGENT_SCOPE,
                RowSpec::new("Remember the agent you pick")
                    .description(
                        "Which agent a project's new-session button (and New Agent Session) starts: the one you last picked in that project, in its Space, or one choice for every project. A project with no pick yet starts the agent you picked last.",
                    )
                    .modified(scope != default_text(NEW_SESSION_AGENT_SCOPE)),
                Some(reset_to(NEW_SESSION_AGENT_SCOPE)),
                &options,
                &scope,
                None,
                |page: &mut Self, value, _window, cx| {
                    page.save(NEW_SESSION_AGENT_SCOPE, json!(value), cx)
                },
                window,
                cx,
            ));
        }
        if should_show_setting(&section, "defaultPromptAgent", None) {
            let description = "Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection.";
            let prompt_options: Vec<DropdownOption> = agents
                .iter()
                .filter(|agent| {
                    agent
                        .command
                        .as_deref()
                        .is_some_and(|command| !command.trim().is_empty())
                })
                .map(|agent| {
                    let name = agent.name.trim();
                    DropdownOption::plain(
                        agent.agent_id.clone(),
                        if name.is_empty() {
                            agent.agent_id.clone()
                        } else {
                            name.to_string()
                        },
                    )
                })
                .collect();
            if prompt_options.is_empty() {
                rows.push(setting_row(
                    p,
                    "defaultPromptAgent",
                    RowSpec::new("Default Prompt Agent").description(
                        "Configure at least one CLI agent before selecting a default prompt agent.",
                    ),
                    None,
                    static_note(p, "Not available", true),
                    cx,
                ));
            } else {
                let default = default_text(DEFAULT_PROMPT_AGENT);
                let raw = values.string(DEFAULT_PROMPT_AGENT);
                let selected = if raw.trim().is_empty() {
                    default.clone()
                } else {
                    raw.trim().to_string()
                };
                let mut options = prompt_options;
                // CDXC:AgentProviders 2026-06-19-08:58 (agents.tsx): a saved default the launcher registry does not know yet shows as unavailable instead of silently reading as Codex.
                if !options.iter().any(|option| option.value == selected) {
                    options.insert(
                        0,
                        DropdownOption::plain(
                            selected.clone(),
                            format!("Unavailable ({selected})"),
                        ),
                    );
                }
                let control = if options.len() >= SEARCHABLE_SELECT_ITEMS {
                    self.dropdown(
                        p,
                        "defaultPromptAgent",
                        &options,
                        Some(&selected),
                        "",
                        true,
                        Some(SELECT_WIDTH),
                        false,
                        None,
                        |page, value, _window, cx| {
                            page.save(DEFAULT_PROMPT_AGENT, json!(value), cx)
                        },
                        window,
                        cx,
                    )
                } else {
                    let options: Vec<SettingOption> = options
                        .iter()
                        .map(|option| SettingOption {
                            label: option.label.clone(),
                            value: option.value.clone(),
                        })
                        .collect();
                    settings_select(
                        self,
                        p,
                        "defaultPromptAgent",
                        &options,
                        &selected,
                        Some(SELECT_WIDTH),
                        false,
                        None,
                        |page: &mut Self, value, _window, cx| {
                            page.save(DEFAULT_PROMPT_AGENT, json!(value), cx)
                        },
                        window,
                        cx,
                    )
                };
                rows.push(setting_row(
                    p,
                    "defaultPromptAgent",
                    RowSpec::new("Default Prompt Agent")
                        .description(description)
                        .modified(raw != default),
                    Some(reset_to(DEFAULT_PROMPT_AGENT)),
                    control,
                    cx,
                ));
            }
        }
        let options = title_generation_options();
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let title_agent = values.choice(TITLE_AGENT, &allowed);
        // Only agents that are on are offered (the saved choice stays listed so it still shows).
        let options: Vec<SettingOption> = options
            .into_iter()
            .filter(|option| {
                option.value == "custom"
                    || option.value == title_agent
                    || agents.iter().any(|agent| agent.agent_id == option.value)
            })
            .collect();
        if should_show_setting(&section, "titleGenerationAgent", None) {
            let custom = values.string(CUSTOM_TITLE_COMMAND);
            let command = resolve_title_generation_command(&title_agent, &agents, &custom);
            let preview = title_generation_preview(&title_agent, command.as_deref());
            // CDXC:SessionTitles 2026-09-09 DECISION (agents.tsx): the exact command lives in this row's info tooltip; there is no preview area.
            let description = format!(
                "Choose the headless agent Ghostex uses for first-prompt session title generation.\n\nCommand Ghostex sends:\n{preview}"
            );
            rows.push(select_field(
                self,
                p,
                TITLE_AGENT,
                RowSpec::new("Title Generation Agent")
                    .description(description)
                    .modified(title_agent != default_text(TITLE_AGENT)),
                Some(reset_to(TITLE_AGENT)),
                &options,
                &title_agent,
                None,
                |page: &mut Self, value, _window, cx| page.save(TITLE_AGENT, json!(value), cx),
                window,
                cx,
            ));
        }
        if title_agent == "custom" && should_show_setting(&section, "customTitleCommand", None) {
            let value = values.string(CUSTOM_TITLE_COMMAND);
            rows.push(text_field(
                self,
                p,
                CUSTOM_TITLE_COMMAND,
                RowSpec::new("Custom Title Command")
                    .description(
                        "Run this command with the title prompt on stdin. It should print only the title.",
                    )
                    .dependent()
                    .modified(value != default_text(CUSTOM_TITLE_COMMAND)),
                Some(reset_to(CUSTOM_TITLE_COMMAND)),
                &value,
                Some("title-generator"),
                None,
                None,
                window,
                cx,
            ));
        }
        if should_show_setting(&section, "acceptAll", None) {
            let enabled = values.bool(ACCEPT_ALL);
            let options = [
                SettingOption {
                    label: "Keep default".to_string(),
                    value: "ask".to_string(),
                },
                SettingOption {
                    label: "Skip permissions".to_string(),
                    value: "bypass".to_string(),
                },
            ];
            let control = settings_segmented(
                p,
                "agent-approvals",
                &options,
                Some(if enabled { "bypass" } else { "ask" }),
                |page: &mut Self, value, window, cx| {
                    if value == "bypass" {
                        page.confirming_bypass = true;
                        page.confirm_focus.focus(window, cx);
                        cx.notify();
                    } else {
                        page.save(ACCEPT_ALL, json!(false), cx);
                    }
                },
                cx,
            );
            rows.push(setting_row(
                p,
                "agentApprovals",
                RowSpec::new("Agent approvals").description(
                    "Choose whether supported agents ask before editing files or running commands. Per-agent settings can override this default.",
                ),
                None,
                control,
                cx,
            ));
        }
        if should_show_setting(&section, TRUST_PROJECT_FOLDERS, None) {
            rows.push(toggle_field(
                self,
                p,
                TRUST_PROJECT_FOLDERS,
                RowSpec::new("Trust project folders automatically").description(
                    "Mark each project and worktree folder as trusted before its agent starts, so no session stops at a \"Do you trust this folder?\" prompt. Turn it off to answer that prompt yourself.",
                ),
                values.bool(TRUST_PROJECT_FOLDERS),
                cx,
            ));
        }
        settings_section(p, "Defaults", None, None, rows)
    }

    /// The Home folder card: where Main Hub's home shows, whether New Chat asks, and the
    /// folders agents started in the home folder can't read (server/src/agents/home_secret_folders.rs).
    pub(super) fn render_home_folder(
        &mut self,
        p: &SettingsPalette,
        search: &TabSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let section = search.section("homeFolder");
        let values = self.store.read(cx).values();
        // The card belongs to the Main Hub beta extension: off, nothing of it shows.
        let main_hub_on = ghostex_settings_catalog::built_in_extensions::on_with(
            ghostex_settings_catalog::built_in_extensions::CHIEF,
            |key| values.value(key).as_bool(),
        );
        if !main_hub_on {
            return None;
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        if should_show_setting(&section, SHOW_ON_ALL_SPACES, None) {
            rows.push(toggle_field(
                self,
                p,
                SHOW_ON_ALL_SPACES,
                RowSpec::new("Show Main Hub on all Spaces").description(
                    "Show Main Hub and your home folder's sessions at the top of every Space. Off, they stay at the top of the Space you moved them to.",
                ),
                values.bool(SHOW_ON_ALL_SPACES),
                cx,
            ));
        }
        if should_show_setting(&section, CONFIRM_NEW_CHAT, None) {
            rows.push(toggle_field(
                self,
                p,
                CONFIRM_NEW_CHAT,
                RowSpec::new("Ask before New Chat").description(
                    "Ask before New Chat or Change Agent starts a new chat with Chief of Staff. Your earlier chats stay in Main Hub either way.",
                ),
                values.bool(CONFIRM_NEW_CHAT),
                cx,
            ));
        }
        if should_show_setting(&section, HOME_SECRET_FOLDERS, None) {
            let value = values.value(HOME_SECRET_FOLDERS);
            let default = settings_catalog()
                .default_value(HOME_SECRET_FOLDERS)
                .cloned()
                .unwrap_or(Value::Null);
            let modified = !value.is_null() && value != default;
            let found_by_search =
                section.is_searching && section.visible_keys.contains(HOME_SECRET_FOLDERS);
            rows.push(home_secret_folders_field(
                self,
                p,
                found_by_search,
                RowSpec::new("Folders agents can't read in your home folder")
                    .description(
                        "Agents Ghostex starts in your home folder are kept out of these folders and files. Claude Code's file tools and every Codex command are held to the list; Claude's shell commands and other agents are not.",
                    )
                    .modified(modified),
                modified.then(|| reset_to(HOME_SECRET_FOLDERS)),
                &value,
                window,
                cx,
            ));
        }
        if rows.is_empty() {
            return None;
        }
        settings_section(p, "Home folder", None, None, rows)
    }

    /// The Skip permissions? confirmation (`w-[25rem] gap-4 p-5`, nested).
    pub(super) fn render_bypass_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = &p.nested_dialog();
        if !self.confirming_bypass {
            return None;
        }
        let cancel = settings_button(
            p,
            "agents-skip-permissions-cancel",
            "Cancel",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.confirming_bypass = false;
                cx.notify();
            },
            cx,
        );
        let confirm = settings_button(
            p,
            "agents-skip-permissions-confirm",
            "Skip permissions",
            None,
            ButtonVariant::DestructiveDialog,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.confirming_bypass = false;
                page.save(ACCEPT_ALL, json!(true), cx);
                cx.notify();
            },
            cx,
        );
        let focus = self.confirm_focus.clone();
        Some(settings_dialog(
            p,
            SettingsDialogSpec::new("agents-skip-permissions", "Skip permissions?")
                .width(400.0)
                .spacing(20.0, 16.0)
                .description(
                    "Supported agents may edit files and run commands without asking you first. Only enable this for agents and projects you trust."
                        .into_any_element(),
                ),
            Vec::new(),
            Some(dialog_footer(vec![cancel, confirm])),
            Some(&focus),
            |page: &mut Self, _window, cx| {
                page.confirming_bypass = false;
                cx.notify();
            },
            window,
            cx,
        ))
    }
}
