use crate::json::J;

/// CDXC:AgentSkills 2026-05-31-09:18:
/// Bundled Ghostex skills must be visible as individual user-installed items in
/// first launch and Settings. Keep the product copy and install commands in one
/// shared catalog so onboarding, settings, and status checks describe the same
/// bundled skills without hiding them behind CLI installation.
///
/// CDXC:AgentSkills 2026-06-26-13:24:
/// Bundle the Codex session-move guidance as its own installable skill so first
/// launch and Settings can install it with the app's other agent-facing skills.
///
/// CDXC:AgentSkills 2026-10-06 DECISION:
/// User: agents make chat visuals "only when I mention the skill that can be installed to do this", so Ghostex Visuals is a bundled skill installed from Settings like the others and invoked by name ($ghostex-visuals), never on the agent's own initiative.
///
/// CDXC:ProjectBoard 2026-08-24:
/// The Project Board beads skill shipped in the bundle with no way to install it,
/// so agents never learned to put the session they are working in on the card.
/// It belongs in this catalog like every other bundled skill.
pub const BUNDLED_GHOSTEX_AGENT_SKILLS: J = J::Arr(&[
    J::Obj(&[
        ("command", J::Str("ghostex cli install-skill")),
        ("description", J::Str("The entry point for everything Ghostex: teaches agents help-first `ghostex` CLI discovery for sessions, orchestration, automations, projects, quick actions, chat queues, prompt history, and diagnostics.")),
        ("id", J::Str("cli")),
        ("name", J::Str("Ghostex CLI")),
        ("skillName", J::Str("ghostex-cli")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex guide install-skill")),
        ("description", J::Str("Let agents explain Ghostex and change its settings for you: ask how a feature works or what a setting does, and the agent answers from the built-in guide and applies the change through the ghostex CLI.")),
        ("id", J::Str("help")),
        ("name", J::Str("Ghostex Help")),
        ("skillName", J::Str("ghostex-help")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex agent-guide install-skill")),
        ("description", J::Str("Tell agents that can't get Ghostex's launch note (OpenCode, Cursor, Copilot and others) what Ghostex can do for them: Ghostex pages, forms and visuals in the chat, its browser, other agents and its skills. They read the details from ghostex agent-guide only when you ask for one of those things.")),
        ("id", J::Str("agentGuide")),
        ("name", J::Str("Ghostex Agent Guide")),
        ("skillName", J::Str("ghostex-agent-guide")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex visual install-skill")),
        ("description", J::Str("Let agents show Ghostex visuals (charts, stats and tables) and Ghostex forms (questions) right in the chat, and Ghostex pages (HTML mockups) from a card. Agents use it only when you call $ghostex-visuals or ask for a gx page, gx form or gx visuals.")),
        ("id", J::Str("visuals")),
        ("name", J::Str("Ghostex Visuals")),
        ("skillName", J::Str("ghostex-visuals")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex secret install-skill")),
        ("description", J::Str("Let agents ask you for a password, API key or token in a private card above the chat box instead of in the chat. What you type goes to a private file the agent's commands read; it never reaches the chat or the model.")),
        ("id", J::Str("secrets")),
        ("name", J::Str("Ghostex Secrets")),
        ("skillName", J::Str("ghostex-secrets")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex computer-use install-skill")),
        ("description", J::Str("Let agents control your machine: click, type, and see the screen in native apps. Runs through Fast Computer & Browser Use and also installs its cua-driver skill, which the agent reads first; your operating system may ask for accessibility and screen recording permissions.")),
        ("id", J::Str("computerUse")),
        ("name", J::Str("Ghostex Computer Use")),
        ("requiresCuaDriver", J::Bool(true)),
        ("skillName", J::Str("ghostex-computer-use")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex spaceo install-skill")),
        ("description", J::Str("Let agents use Mac apps on their own hidden screen: SpaceO opens apps on a virtual display, so agents click, type and take screenshots there while you keep your own screen, pointer and focus. Needs SpaceO, an Apple Silicon Mac and macOS 14 or later.")),
        ("id", J::Str("spaceo")),
        ("macOSOnly", J::Bool(true)),
        ("name", J::Str("Ghostex SpaceO")),
        ("requiresSpaceo", J::Bool(true)),
        ("skillName", J::Str("ghostex-spaceo")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex browser-use install-skill")),
        ("description", J::Str("Let agents control your browser: open pages, click, fill forms, and read what is on screen in supported external browsers. Runs through Fast Computer & Browser Use and also installs its cua-driver skill, which the agent reads first.")),
        ("id", J::Str("browserUse")),
        ("name", J::Str("Ghostex Browser Use")),
        ("requiresCuaDriver", J::Bool(true)),
        ("skillName", J::Str("ghostex-browser-use")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex browser install-skill")),
        ("description", J::Str("Let agents control the browser panes built into Ghostex: read console logs, capture screenshots, and interact with pages.")),
        ("id", J::Str("embeddedBrowserUse")),
        ("name", J::Str("Ghostex Embedded Browser Use")),
        ("skillName", J::Str("ghostex-embedded-browser-use")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex agents-orchestration install-skill")),
        ("description", J::Str("Let agents work as a team: teaches agents to launch other agents with the model and effort you ask for, message each other to hand off tasks and coordinate work, read the replies, and check the results, all through the `ghostex` CLI help.")),
        ("id", J::Str("agentsOrchestration")),
        ("name", J::Str("Ghostex Agents")),
        ("skillName", J::Str("ghostex-agents")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex generate-title install-skill")),
        ("description", J::Str("Teaches agents how to generate concise Ghostex session titles and submit the rename command in the current session.")),
        ("hiddenFromUi", J::Bool(true)),
        ("id", J::Str("generateTitle")),
        ("name", J::Str("Ghostex Auto Rename Session")),
        ("skillName", J::Str("ghostex-auto-rename-session")),
        ("tier", J::Str("optional")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex board install-skill")),
        ("description", J::Str("Teaches agents to work a project board bead: move it through the board's statuses, comment progress on it, and link the session they are working in to the card so the board shows who has it.")),
        ("id", J::Str("manageBeads")),
        ("name", J::Str("Ghostex Project Board Beads")),
        ("skillName", J::Str("ghostex-manage-beads")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex move-codex-session install-skill")),
        ("description", J::Str("Teaches agents how to fork a Codex conversation into another folder with the correct session id, target root, and optional full-access mode.")),
        ("hiddenFromUi", J::Bool(true)),
        ("id", J::Str("moveCodexSession")),
        ("name", J::Str("Ghostex Move Codex Session")),
        ("skillName", J::Str("ghostex-move-codex-session")),
        ("tier", J::Str("optional")),
    ]),
]);

/// The product name Settings shows for SpaceO (github.com/ParthJadhav/SpaceO).
pub const GHOSTEX_SPACEO_PRODUCT_NAME: &str = "SpaceO";

/// CDXC:Extensions 2026-10-05 DECISION:
/// User: "We need to name this Fast Computer & Browser Use (trycua/cua ↗)", and clicking the link opens the repository. User-facing surfaces say "Fast Computer & Browser Use"; the Settings row follows the name with a `trycua/cua ↗` link to GitHub. Supersedes the 2026-09-30 "Fast Computer Use" name, which kept the repository slug out of the UI.
pub const GHOSTEX_TRYCUA_PRODUCT_NAME: &str = "Fast Computer & Browser Use";

/// The link Settings shows after the product name.
pub const GHOSTEX_TRYCUA_REPOSITORY_LABEL: &str = "trycua/cua ↗";
pub const GHOSTEX_TRYCUA_REPOSITORY_URL: &str = "https://github.com/trycua/cua";

/// The bundled skills that app surfaces (onboarding, Settings, search) may show.
///
/// CDXC:AgentSkills 2026-10-06 DECISION:
/// User: "keep the 2 shown in ui and hide the cua-driver one" because its row did not explain what it does, and "make the browser/computer use ones install cua-driver and also tell the agent to read that skill". Trycua's own `cua-driver` skill is not a row here: installing Ghostex Computer Use or Ghostex Browser Use also runs `cua-driver skills install`, and both skills send the agent to it first. The Fast Computer & Browser Use row keeps its Install skill button for when it is missing. Supersedes the 2026-10-05 decision that listed it as its own Cua Driver row.
pub const VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS: J = J::Arr(&[
    J::Obj(&[
        ("command", J::Str("ghostex cli install-skill")),
        ("description", J::Str("The entry point for everything Ghostex: teaches agents help-first `ghostex` CLI discovery for sessions, orchestration, automations, projects, quick actions, chat queues, prompt history, and diagnostics.")),
        ("id", J::Str("cli")),
        ("name", J::Str("Ghostex CLI")),
        ("skillName", J::Str("ghostex-cli")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex guide install-skill")),
        ("description", J::Str("Let agents explain Ghostex and change its settings for you: ask how a feature works or what a setting does, and the agent answers from the built-in guide and applies the change through the ghostex CLI.")),
        ("id", J::Str("help")),
        ("name", J::Str("Ghostex Help")),
        ("skillName", J::Str("ghostex-help")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex agent-guide install-skill")),
        ("description", J::Str("Tell agents that can't get Ghostex's launch note (OpenCode, Cursor, Copilot and others) what Ghostex can do for them: Ghostex pages, forms and visuals in the chat, its browser, other agents and its skills. They read the details from ghostex agent-guide only when you ask for one of those things.")),
        ("id", J::Str("agentGuide")),
        ("name", J::Str("Ghostex Agent Guide")),
        ("skillName", J::Str("ghostex-agent-guide")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex visual install-skill")),
        ("description", J::Str("Let agents show Ghostex visuals (charts, stats and tables) and Ghostex forms (questions) right in the chat, and Ghostex pages (HTML mockups) from a card. Agents use it only when you call $ghostex-visuals or ask for a gx page, gx form or gx visuals.")),
        ("id", J::Str("visuals")),
        ("name", J::Str("Ghostex Visuals")),
        ("skillName", J::Str("ghostex-visuals")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex secret install-skill")),
        ("description", J::Str("Let agents ask you for a password, API key or token in a private card above the chat box instead of in the chat. What you type goes to a private file the agent's commands read; it never reaches the chat or the model.")),
        ("id", J::Str("secrets")),
        ("name", J::Str("Ghostex Secrets")),
        ("skillName", J::Str("ghostex-secrets")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex computer-use install-skill")),
        ("description", J::Str("Let agents control your machine: click, type, and see the screen in native apps. Runs through Fast Computer & Browser Use and also installs its cua-driver skill, which the agent reads first; your operating system may ask for accessibility and screen recording permissions.")),
        ("id", J::Str("computerUse")),
        ("name", J::Str("Ghostex Computer Use")),
        ("requiresCuaDriver", J::Bool(true)),
        ("skillName", J::Str("ghostex-computer-use")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex spaceo install-skill")),
        ("description", J::Str("Let agents use Mac apps on their own hidden screen: SpaceO opens apps on a virtual display, so agents click, type and take screenshots there while you keep your own screen, pointer and focus. Needs SpaceO, an Apple Silicon Mac and macOS 14 or later.")),
        ("id", J::Str("spaceo")),
        ("macOSOnly", J::Bool(true)),
        ("name", J::Str("Ghostex SpaceO")),
        ("requiresSpaceo", J::Bool(true)),
        ("skillName", J::Str("ghostex-spaceo")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex browser-use install-skill")),
        ("description", J::Str("Let agents control your browser: open pages, click, fill forms, and read what is on screen in supported external browsers. Runs through Fast Computer & Browser Use and also installs its cua-driver skill, which the agent reads first.")),
        ("id", J::Str("browserUse")),
        ("name", J::Str("Ghostex Browser Use")),
        ("requiresCuaDriver", J::Bool(true)),
        ("skillName", J::Str("ghostex-browser-use")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex browser install-skill")),
        ("description", J::Str("Let agents control the browser panes built into Ghostex: read console logs, capture screenshots, and interact with pages.")),
        ("id", J::Str("embeddedBrowserUse")),
        ("name", J::Str("Ghostex Embedded Browser Use")),
        ("skillName", J::Str("ghostex-embedded-browser-use")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex agents-orchestration install-skill")),
        ("description", J::Str("Let agents work as a team: teaches agents to launch other agents with the model and effort you ask for, message each other to hand off tasks and coordinate work, read the replies, and check the results, all through the `ghostex` CLI help.")),
        ("id", J::Str("agentsOrchestration")),
        ("name", J::Str("Ghostex Agents")),
        ("skillName", J::Str("ghostex-agents")),
        ("tier", J::Str("recommended")),
    ]),
    J::Obj(&[
        ("command", J::Str("ghostex board install-skill")),
        ("description", J::Str("Teaches agents to work a project board bead: move it through the board's statuses, comment progress on it, and link the session they are working in to the card so the board shows who has it.")),
        ("id", J::Str("manageBeads")),
        ("name", J::Str("Ghostex Project Board Beads")),
        ("skillName", J::Str("ghostex-manage-beads")),
        ("tier", J::Str("recommended")),
    ]),
]);
