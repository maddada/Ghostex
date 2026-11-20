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
///
/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help, keeping one tiny `ghostex` skill (once Ghostex Agent Guide) that sends the agent to `ghostex agent-guide` and the guide it names. So this catalog, and the visible list below, hold only that skill; the playbooks moved into `ghostex … guide`, and browser and computer use go to Trycua's own `cua-driver` skill, which the Fast Computer & Browser Use row installs.
pub const BUNDLED_GHOSTEX_AGENT_SKILLS: J = J::Arr(&[
    J::Obj(&[
        ("command", J::Str("ghostex agent-guide install-skill")),
        ("description", J::Str("One small skill for any agent: type /ghostex ($ghostex in Codex) and it sends the agent to ghostex agent-guide, which says what Ghostex can do (Live pages, Ghostex forms and visuals, its browser, other agents, secrets, help with its settings) and which built-in guide to read for each job. The guides come from the app, so they always match the Ghostex you run.")),
        ("id", J::Str("agentGuide")),
        ("name", J::Str("Ghostex")),
        ("skillName", J::Str("ghostex")),
        ("tier", J::Str("recommended")),
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
        ("command", J::Str("ghostex agent-guide install-skill")),
        ("description", J::Str("One small skill for any agent: type /ghostex ($ghostex in Codex) and it sends the agent to ghostex agent-guide, which says what Ghostex can do (Live pages, Ghostex forms and visuals, its browser, other agents, secrets, help with its settings) and which built-in guide to read for each job. The guides come from the app, so they always match the Ghostex you run.")),
        ("id", J::Str("agentGuide")),
        ("name", J::Str("Ghostex")),
        ("skillName", J::Str("ghostex")),
        ("tier", J::Str("recommended")),
    ]),
]);
