use crate::data::*;
use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "agents",
        title: "Agents",
        sections: vec![agent_list(), config(), agent_hooks(), home_folder()],
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
            row("trustProjectFoldersAutomatically", "Trust project folders automatically", "Mark each project and worktree folder as trusted before its agent starts, so no session stops at a \"Do you trust this folder?\" question. Off by default: the chat shows that question as a card with Trust folder and Don't trust. Turning it on asks for Touch ID, Windows Hello or your password on this computer."),
            // CDXC:Settings 2026-11-11 DECISION in apps/desktop settings_modal/model.rs (`RailTier`, Q2511): these two moved here from Chat; they set what an agent is told and given when it starts, not how the chat looks. Claude task list still shows only while the Session details panel is on (its `settingKeys`).
            row("agentGhostexNote", "Tell agents about Ghostex", "Agent sessions Ghostex starts get a short note that they run in Ghostex: it can show Ghostex pages, forms and visuals in the chat, install Ghostex skills, and `ghostex agent-guide` explains the rest. Agents use it only when you ask for one of those things. Applies to sessions started or resumed after you change it."),
            row("sessionDetailsClaudeTaskTools", "Claude task list", "Turn on Claude's task list tools, so Claude can keep a checklist that the Session details panel shows. Recent Claude models only get them when Ghostex asks. Applies to Claude sessions started or woken after you change it. Shown while the Session details panel extension is on."),
        ],
    )
}

/// Main Hub's home: the project at the home folder whose header is Chief's row
/// (server/src/chief/home.rs), and the folders agents started there can't read.
pub(crate) fn home_folder() -> Section {
    section(
        "homeFolder",
        "Home folder",
        vec![
            row("chiefShowOnAllSpaces", "Show Main Hub on all Spaces", "Show Main Hub and your home folder's sessions at the top of every Space. Off, they stay at the top of the Space you moved them to."),
            row("chiefConfirmNewChat", "Ask before New Chat", "Ask before New Chat or Change Agent starts a new chat with Chief of Staff. Your earlier chats stay in Main Hub either way."),
            row("gbotsChiefAddsOnItsOwn", "Chief of Staff may add GBots on its own", "With GBots on, Chief of Staff adds a GBot when a job needs one and tells you in a card you can undo. Off, it asks you before adding each one."),
            row("homeSecretFolders", "Folders agents can't read in your home folder", "Agents Ghostex starts in your home folder are kept out of these folders and files. Claude Code's file tools and every Codex command are held to the list; Claude's shell commands and other agents are not."),
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
            // Claude Code's row panel, its Context group (apps/desktop settings_modal/tabs/agents/claude_context.rs).
            row("claudeIdleCompaction", "Compact idle chats before the prompt cache expires", "Claude Code compacts a long chat that has been idle for about 54 minutes, just before its 1-hour prompt cache expires, so your next message doesn't re-read the whole chat uncached. Runs only on a 1-hour cache. In Claude Code's row. Applies to Claude sessions started or resumed after you change it."),
            row("claudeIdleCompactAbove", "Only above", "Compact only chats whose context is at least this size. Claude Code's default is 200K. Shown while Compact idle chats is on.").options(CLAUDE_IDLE_COMPACT_MIN_TOKENS_OPTIONS),
            row("claudeOneHourPromptCache", "Use a 1-hour prompt cache", "Ask for Claude's 1-hour prompt cache on an API key, Bedrock, Vertex or Foundry, which otherwise get 5 minutes, so idle compaction runs there too. 1-hour cache writes cost more than 5-minute ones. A subscription already gets 1 hour."),
            row("preferredAgentInterfaceOverrides", "Default view per agent", "Agents that support Ghostex's Chat View are marked with a chat bubble and can open in Chat or Terminal regardless of the global Default Agent View. Inherit keeps following that global setting."),
        ],
    )
}

pub(crate) fn agent_hooks() -> Section {
    section(
        "agentHooks",
        "Session resume hooks",
        vec![
            row("agentResumeHooks", "Session resume hooks", "Claude Code, Codex and OpenCode need no hook: Ghostex follows them from the files they write themselves. Other agents use a hook so Ghostex can capture their native session id and resume the exact conversation after sleep, reload, or app restart. Install one agent's hook from its row, fix every agent that is on with Fix all, install every missing hook with Install all (shown only when a hook is missing), or remove every Ghostex hook with Uninstall all in the ⋯ menu.").options_of(AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS, "name", "name"),
            row("agentHooksAutoInstall", "Install the hook when I turn on an agent", "Install an agent's session resume hook as soon as you turn it on, without asking."),
        ],
    )
}
