//! The coordinator's role: the playbook it runs by (the bundled `ghostex-orchestrator` skill), how a
//! new orchestrator is handed it, and the older channels that orchestrators created before
//! 2026-10-17 still read it through.

use std::path::{Path, PathBuf};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// CDXC:Coordinators 2026-10-17 DECISION:
/// User: "we should just include a line that can't be removed saying: "Read and adhere to this skill: /ghostex-orchestrator" and we should install this orchestrator skill when user picks the orchstrator option so no need to inject things the other way." The playbook is the bundled skill `skills/ghostex-orchestrator/SKILL.md`; creating or promoting an orchestrator installs it for that agent ([`ensure_orchestrator_skill_installed`]) and queues [`orchestrator_first_message`], which always starts with [`orchestrator_skill_line`] in the agent's own skill spelling. Nothing is added to the agent's command any more. Supersedes the 2026-09-30 WHY that carried the role as a system prompt flag (Claude's role file, Codex `developer_instructions`, ZCode's SessionStart context, Empryo's `/agent` profile): those channels now serve only orchestrators created before this change, whose saved commands or records still name them, so they keep their role on resume, sleep and wake, and account switches.
/// SEE-ALSO: server/src/server/coordinator_runtime.rs (`start_orchestrator_role`), server/src/agents/marked_handovers.rs and server/src/agents/activity.rs (the older channels), apps/desktop/src/app/window/new_coordinator_modal.rs (the locked line in the dialog).
///
/// CDXC:Coordinators 2026-10-09 DECISION:
/// User: "If a tool call got rejected with ("STOP and wait") just as a thread report arrives then this is Ghostex's delivery Escape, not the owner pausing your work: retry the tool call and carry on." The playbook's "Waiting means ending your turn" section says so.
///
/// CDXC:Coordinators 2026-10-11 DECISION:
/// User: "pls make sure future orchestrators on customers pcs on 10.20.0 would behave like you are behaving now". The playbook's "Asking the user" part has orchestrators ask plain questions as an in-chat ```questions form (all open questions at once, never typed answers or the one-question-at-a-time built-in tool), visual questions as a page, and never resend an answered question.
///
/// CDXC:Coordinators 2026-10-11 DECISION:
/// User: "i should never have to go into a inner thread to see questions or mockups or whatever ... I should only talk to u and you bring what needs my attentino from the threads under you since you're the orchestrator". The playbook's "Everything the user sees comes through you" section has the orchestrator ask a thread's questions and republish its pages from its own session (so answers come back to it), and the thread brief (brief.rs `thread_brief`) tells threads not to ask the user or publish pages themselves.
///
/// CDXC:Coordinators 2026-11-04 DECISION:
/// User, on a form asking about "#407" with no way to open it: "we should always link so the user can check what you're asking about". The playbook's "Asking the user" section, the ghostex-visuals skill, `ghostex visual --help`, the agent guide and the thread brief (brief.rs) tell agents to link what a question is about in the question itself; the chat's form and the page kit open those links (packages/gx-chat-core/src/questions/form_links.rs).
///
/// CDXC:Coordinators 2026-10-12 DECISION:
/// User, on question numbers restarting at Q1 in a new form: "save this in the memory for this thread and for the orchestrator skill/instructinos please so users get this too!" The playbook's "Asking the user" section has the orchestrator tag every question Q1, Q2, … and keep counting across the whole conversation (forms, pages, questions asked again, a thread's questions brought to the user), never restarting, so a number always means one question.
///
/// CDXC:Coordinators 2026-10-12 DECISION:
/// User: "when you say 'In progress' but don't share all of the actual in progress tasks this isn't nice because it makes me think some tasks aren't running when they actually are running", and for the threads started this turn: "let's call it 'Kicked off'". Then (2026-10-12), on a thread listed under both: "let's not share same update in 2 places pls". The playbook's "Talking to the user" section has a handover list the threads started this turn under Kicked off and every other thread still working (from `ghostex orchestrator status`) under In progress, so each running thread appears exactly once; the Marked handovers note gives the two their 🛫 and 🔄 markers.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User: "You need to add to the coordinator playbook to suggest to the user to adopt a thread when it makes sense", and that the orchestrator suggests Ghostex improvements it notices (Q1232, Q1233). The playbook's "Sessions that are not your threads" section has it suggest `ghostex coordinator link` for a session whose work it relies on, "Suggesting improvements" has it list friction fixes under 💡 Suggestions and offer research threads, and "Talking to the user" puts the actionable sections last and Parked apart from In progress. Two threads ended turns mid-task while a background job ran and looked finished, so the thread brief (brief.rs) tells threads to wait in the background or say the job still runs, and the playbook tells the orchestrator to check such a report.
///
/// CDXC:Coordinators 2026-10-10 DECISION:
/// User: "let's please rename "Coordinator" to "Orchestrator" everywhere so it's clearer to everyone." The playbook and the guide pointer call the role an orchestrator and name `ghostex orchestrator` verbs; the role file's name on disk and every internal name keep "coordinator".
const ORCHESTRATOR_SKILL_MD: &str = include_str!("../../../skills/ghostex-orchestrator/SKILL.md");
const ORCHESTRATOR_SKILL_OPENAI_YAML: &str =
    include_str!("../../../skills/ghostex-orchestrator/agents/openai.yaml");

/// The bundled skill every orchestrator works by.
pub const ORCHESTRATOR_SKILL_NAME: &str = "ghostex-orchestrator";

/// The session runtime setting that marks an orchestrator as taking its role from the skill
/// (created or promoted since 2026-10-17); the older role channels skip such a session.
pub const COORDINATOR_ROLE_SETTING: &str = "coordinatorRole";
pub const COORDINATOR_ROLE_SKILL: &str = "skill";

/// The session runtime setting `true` on an orchestrator whose first message is only the skill
/// line (a handoff stages its message as a draft instead, an unnamed `create` may carry no task).
/// See `keeps_its_given_title` in title.rs.
pub const COORDINATOR_SKILL_ONLY_START_SETTING: &str = "coordinatorSkillOnlyStart";

/// True when the session takes its orchestrator role from the skill.
pub fn coordinator_role_is_skill(session: &serde_json::Value) -> bool {
    session
        .get("runtimeSettings")
        .and_then(|runtime| runtime.get(COORDINATOR_ROLE_SETTING))
        .and_then(serde_json::Value::as_str)
        == Some(COORDINATOR_ROLE_SKILL)
}

/// The playbook: the skill's body without its frontmatter.
pub fn coordinator_playbook() -> &'static str {
    ORCHESTRATOR_SKILL_MD
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(ORCHESTRATOR_SKILL_MD, |(_, body)| body)
        .trim_start()
}

/// What `ghostex orchestrator guide` prints, and what the older role channels carry: the playbook
/// and then the reply format Orchestrators answer without markers (`orchestratorPlainReplies`)
/// picks. A static skill cannot read that setting, so the skill sends the orchestrator here.
pub fn coordinator_guide(paths: &GxserverPaths) -> String {
    coordinator_guide_for(coordinator_playbook(), "", paths)
}

/// The heading the user's playbook overrides go under at the end of the guide.
pub const PLAYBOOK_OVERRIDES_HEADING: &str = "## The user's overrides";

/// The line under [`PLAYBOOK_OVERRIDES_HEADING`] that ranks the overrides above the playbook.
pub const PLAYBOOK_OVERRIDES_PRECEDENCE: &str = "The user wrote these overrides for every coordinator. They take precedence over the playbook instructions above: the playbook is general and written for everyone who uses Ghostex, while these overrides are this user's own. Where the two disagree, follow the overrides.";

/// [`coordinator_guide`] for a given playbook and the user's overrides: the playbook in effect for
/// an orchestrator's project (playbook.rs `resolve_playbook`), then the reply format, then the
/// overrides, last so nothing above outranks them. `ghostex orchestrator guide` prints it. The
/// older role channels, written once for every orchestrator they serve, keep the shipped playbook
/// without overrides; their playbook still sends them to the guide.
///
/// CDXC:Coordinators 2026-11-07 DECISION:
/// User: "Let us actually make people override parts of the original playbook, not edit it ok? So they have a section for overriding any part of the playbook so this way updates still work and all that. we need to add a note that what the user mentions in the override below takes higher precedence than the playbook instructions since the playbook instructions are general while the overrides are specific to the actual current user". Ghostex's playbook is never edited any more; the user's overrides (one text for every coordinator) follow it under their own heading with that precedence line.
pub fn coordinator_guide_for(playbook: &str, overrides: &str, paths: &GxserverPaths) -> String {
    let playbook = playbook.trim();
    let mut guide = if crate::agents::orchestrator_plain_replies(paths) {
        format!(
            "{playbook}\n\nReply format: the user chose plain answers for coordinators. Write your handovers as ordinary text with your own headings, without section marker emoji.\n"
        )
    } else {
        format!("{playbook}\n\n{}\n", crate::agents::MARKED_HANDOVERS_NOTE)
    };
    let overrides = overrides.trim();
    if !overrides.is_empty() {
        guide.push_str(&format!(
            "\n{PLAYBOOK_OVERRIDES_HEADING}\n\n{PLAYBOOK_OVERRIDES_PRECEDENCE}\n\n{overrides}\n"
        ));
    }
    guide
}

/// The line every new orchestrator's first message starts with (`/ghostex-orchestrator` for Claude
/// Code and Copilot, `$ghostex-orchestrator` for every other agent); the dialogs show the same
/// line from the same function.
pub fn orchestrator_skill_line(family: &str) -> String {
    ghostex_gx_chat_core::composer::skill_invocation::orchestrator_skill_line(Some(family))
}

/// A new orchestrator's first message: the skill line, then the user's first request when the
/// create carried one.
pub fn orchestrator_first_message(family: &str, first_request: &str) -> String {
    let line = orchestrator_skill_line(family);
    match first_request.trim() {
        "" => line,
        request => format!("{line}\n\n{request}"),
    }
}

pub use ghostex_gx_chat_core::protocol::{
    coordinator_agent_families_text, coordinator_agent_family_supported, COORDINATOR_AGENT_FAMILIES,
};

/// The effort `ghostex orchestrator create` starts an orchestrator on when none is named, for the
/// families whose launch takes one: high (CDXC:Coordinators 2026-10-29 DECISION: User: "allow picking medium here but default to high"; the picker offers Low, Medium, High and XHigh since 2026-10-31).
/// SEE-ALSO: `orchestrator_efforts` in packages/gx-chat-core/src/menus/orchestrator_effort.rs (the dialog and the chat composer).
pub fn default_orchestrator_effort(family: &str) -> Option<&'static str> {
    match family {
        "claude" | "codex" | "antigravity" => Some("high"),
        _ => None,
    }
}

/// The Agent Sync catalog id of a family's own skills folder; `None` for agents that read
/// `~/.agents/skills` themselves (OpenCode).
fn orchestrator_skill_agent(family: &str) -> Option<&'static str> {
    match family {
        "claude" => Some("claude-code"),
        "codex" => Some("codex"),
        "zcode" => Some("zcode"),
        "cursor" => Some("cursor"),
        "grok" => Some("grok"),
        "copilot" => Some("github-copilot"),
        "antigravity" => Some("antigravity-cli"),
        "pi" => Some("pi"),
        _ => None,
    }
}

/// Installs the orchestrator skill for `family`: one copy in `~/.agents/skills`, written from this
/// build when it is missing, and a link in the agent's own skills folder when the agent does not
/// read `~/.agents/skills` itself (the shape Settings installs make). An existing copy is left as
/// it is: the startup refresh keeps it equal to GitHub main (agent_skills_remote.rs).
pub fn ensure_orchestrator_skill_installed(
    paths: &GxserverPaths,
    family: &str,
) -> Result<PathBuf, DomainStateError> {
    let home = paths.agent_config_home_dir();
    let canonical = home
        .join(crate::agent_skills::CANONICAL_AGENT_SKILLS_DIR)
        .join(ORCHESTRATOR_SKILL_NAME);
    let install_error = |path: &Path, error: std::io::Error| DomainStateError {
        code: "internalError",
        message: format!(
            "Could not install the {ORCHESTRATOR_SKILL_NAME} skill at {}: {error}",
            path.display()
        ),
    };
    if !canonical.join("SKILL.md").is_file() {
        write_if_changed(
            &canonical.join("agents").join("openai.yaml"),
            ORCHESTRATOR_SKILL_OPENAI_YAML,
        )
        .map_err(|error| install_error(&canonical, error))?;
        write_if_changed(&canonical.join("SKILL.md"), ORCHESTRATOR_SKILL_MD)
            .map_err(|error| install_error(&canonical, error))?;
    }
    let agent_root = orchestrator_skill_agent(family).and_then(|agent_id| {
        ghostex_agent_sync::agent_catalog(home)
            .into_iter()
            .find(|spec| spec.id == agent_id && !spec.universal)
            .and_then(|spec| spec.skills_dir)
    });
    if let Some(root) = agent_root {
        crate::agent_skills::link_skill_into_agent_root(
            &canonical,
            &root.join(ORCHESTRATOR_SKILL_NAME),
        )?;
    }
    Ok(canonical)
}

/// Where gxserver keeps the role file Claude coordinators created before 2026-10-17 load at
/// every launch (their saved command names it).
pub fn coordinator_role_file(paths: &GxserverPaths) -> PathBuf {
    paths
        .root_dir
        .join("coordinators")
        .join("coordinator-role.md")
}

/// Writes the older Claude coordinators' role file when it is missing or out of date, so an
/// upgrade or a change to Orchestrators answer without markers reaches each of them at its next
/// launch or resume.
pub fn ensure_coordinator_role_file(paths: &GxserverPaths) -> std::io::Result<PathBuf> {
    let path = coordinator_role_file(paths);
    write_if_changed(&path, &coordinator_guide(paths))?;
    Ok(path)
}

/// The pointer Codex coordinators created before 2026-10-17 carry inline
/// (`-c developer_instructions=...`) and ZCode ones get as SessionStart hook context; it sends them
/// to `ghostex orchestrator guide` for the playbook.
pub const GUIDE_POINTER_COORDINATOR_INSTRUCTIONS: &str = "You are a Ghostex coordinator: the user talks only to you, and you hand real work to thread sessions instead of doing it yourself, so you stay free to talk. Before your first reply, and again after any context compaction, run `ghostex coordinator guide` and follow it for the whole session. Run `ghostex coordinator status` at the start of every request.";

fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
    }
    Ok(())
}

/// The custom agent profile an Empryo coordinator created before 2026-10-17 runs as.
pub const EMPRYO_COORDINATOR_AGENT_NAME: &str = "ghostex-coordinator";

/// The line that hands an older Empryo coordinator its role again when its tab lost it.
///
/// CDXC:Coordinators 2026-10-06 WHY:
/// Empryo 3.9.0-beta's interactive app reads neither `--agent` nor `--system` (its argv parser returns early without `--headless`) and ignores hook `additionalContext`, so a launch flag cannot carry the role. The role spike (card agent-bo-95422941) proved the TUI's `/agent <name>` command does: the profile's body joins the tab's system prompt (35,520 to 44,990 characters), and the tab keeps it across `--session` resume. So gxserver writes the playbook as the global profile `~/.empryo/agents/ghostex-coordinator.md`; since 2026-10-17 no new Empryo orchestrator is made, and the supervisor only keeps the line typed into the ones that exist.
/// SEE-ALSO: server/src/server/coordinator_runtime.rs (`repair_empryo_coordinator_roles`).
pub fn coordinator_role_queued_command(family: &str) -> Option<String> {
    (family == "empryo").then(|| format!("/agent {EMPRYO_COORDINATOR_AGENT_NAME}"))
}

/// Writes the older Empryo coordinators' profile when it is missing or out of date, like the
/// Claude role file. The description keeps Empryo from delegating ordinary tasks to it.
pub fn ensure_empryo_coordinator_agent_file(paths: &GxserverPaths) -> std::io::Result<PathBuf> {
    let hook_paths = crate::agent_hooks::config::HookPaths::from_paths(paths);
    // Empryo 3.9.1 reads global profiles from `<home>/.empryo/agents` on every platform, also on
    // Windows, where its config and hooks live in `%LOCALAPPDATA%\Empryo` (`empryo_home`).
    let path = hook_paths
        .home_dir
        .join(".empryo")
        .join("agents")
        .join(format!("{EMPRYO_COORDINATOR_AGENT_NAME}.md"));
    write_if_changed(
        &path,
        &format!(
            "---\nname: {EMPRYO_COORDINATOR_AGENT_NAME}\ndescription: The Ghostex coordinator role. Ghostex starts a coordinator session with it; never delegate a task to it.\n---\n\n{}",
            coordinator_guide(paths)
        ),
    )?;
    Ok(path)
}

/// The `-c developer_instructions=…` argument a Codex coordinator created before 2026-10-17
/// carries. Marked handovers (agents/marked_handovers.rs) finds it by this exact text to add its
/// note to it.
pub fn codex_developer_instructions_arg(text: &str) -> String {
    let value = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "-c {}",
        crate::agents::quote_shell_arg(&format!("developer_instructions={value}"))
    )
}
