use crate::data::*;
use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "agents",
        title: "Agents",
        sections: vec![agent_list(), config(), agent_hooks()],
    }
}

pub(crate) fn config() -> Section {
    section(
        "config",
        "Defaults",
        vec![
            row("newSessionAgentScope", "Remember the agent you pick", "Which agent a project's new-session button (and New Agent Session) starts: the one you last picked in that project, in its Space, or one choice for every project. A project with no pick yet starts the agent you picked last.").options(NEW_SESSION_AGENT_SCOPE_OPTIONS),
            row("defaultPromptAgent", "Default Prompt Agent", "Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection."),
            row("titleGenerationAgent", "Title Generation Agent", "Choose the headless agent Ghostex uses for first-prompt session title generation. Hover the info icon to see the exact command Ghostex sends.").options(SESSION_TITLE_GENERATION_AGENT_OPTIONS),
            row("customTitleCommand", "Custom Title Command", "Run this command with the title prompt on stdin. It should print only the title."),
            row("acceptAll", "Agent approvals", "Choose whether supported agents ask before editing files or running commands. Per-agent settings can override this default."),
            row("trustProjectFoldersAutomatically", "Trust project folders automatically", "Mark each project and worktree folder as trusted before its agent starts, so no session stops at a \"Do you trust this folder?\" prompt. Turn it off to answer that prompt yourself."),
        ],
    )
}

pub(crate) fn agent_list() -> Section {
    section(
        "agentList",
        "Agents",
        vec![
            row("agentSwitches", "Turn agents on or off", "Turn on the agents you use; they appear in the New session menu, the sidebar's Select Agent and on your phone. Drag to set their order. Agents you turned off but used before stay dimmed in the list; agents you never used wait under More agents. Expand a row to edit its command, permission mode and default view, or to install or update its CLI.").options_of(DEFAULT_SIDEBAR_AGENTS, "name", "name"),
            row("addCustomAgent", "Add custom agent", "Add your own command, or a variant of a built-in agent with other flags. Works like gives it that agent's logo, chat view, resume hook and permission handling. Custom agents can be turned off or deleted."),
            row("preferredAgentInterfaceOverrides", "Default view per agent", "Agents that support Ghostex's Chat View are marked with a chat bubble and can open in Chat or Terminal regardless of the global Default Agent View. Inherit keeps following that global setting."),
        ],
    )
}

pub(crate) fn agent_hooks() -> Section {
    section(
        "agentHooks",
        "Session resume hooks",
        vec![
            row("agentResumeHooks", "Session resume hooks", "Hooks let Ghostex capture each agent's native session id and resume the exact conversation after sleep, reload, or app restart. Install one agent's hook from its row, fix every agent that is on with Fix all, install every missing hook with Install all (shown only when a hook is missing), or remove every Ghostex hook with Uninstall all in the ⋯ menu.").options_of(AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS, "name", "name"),
            row("agentHooksAutoInstall", "Install the hook when I turn on an agent", "Install an agent's session resume hook as soon as you turn it on, without asking."),
        ],
    )
}
