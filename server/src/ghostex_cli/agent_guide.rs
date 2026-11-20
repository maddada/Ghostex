use crate::agents::{agent_preview_browser, PreviewBrowser};
use crate::ghostex_cli::rpc::{CliError, CliResult};

/// CDXC:AgentGuide 2026-11-02 DECISION:
/// User (Q1065): the note every agent session gets stays "very short and point[s] to `ghostex agent-guide`", which holds the how-tos: other agents split by size (Q1071: built-in subagents for quick help, offer a coordinator for bigger jobs), the browser choice and how to save it (Q1069), pages, questions and charts in the chat, secrets, installing Ghostex skills for the user, and `ghostex help ask` (Q1070). Keep it compact: an agent reads it whole, and the full CLI help is about 8,000 tokens.
///
/// CDXC:AgentGuide 2026-11-06 DECISION:
/// User: "i hate saying 'inline html' and 'inline q' and inline graphs", then "ghostex page / gx page, ghostex form / gx form, ghostex visuals / gx visuals", everywhere: agents understand the new words, the old ones keep working quietly, and agents use the new names when they talk to the user.
/// CDXC:AgentGuide 2026-11-19 DECISION:
/// User picked "Live page" (Q4207, Q4208) for what the decision above named Ghostex page / gx page, superseding that part; the guide names Live pages and lists the old words beside them.
/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help, superseding the "installing Ghostex skills for the user" part of Q1065 above: the guide's Guides section names the `ghostex … guide` to read for each job instead, and the one remaining skill, `ghostex`, only sends an agent here.
/// SEE-ALSO: server/src/agents/ghostex_note.rs (the note and the browser setting), skills/ghostex/SKILL.md.
const GUIDE: &str = "Ghostex agent guide

You run inside Ghostex, an app where the user runs coding agents in sessions listed in a sidebar, as a chat or a terminal. Use this only as the user asks.

Show things in the chat (use these names with the user; older words mean the same)
- Ghostex visuals (gx visuals; \"inline graphs\"): charts, stat tiles, tables, in a ```visual block.
- A Ghostex form (gx form; \"inline questions\"): questions with choices, a ```questions block; link what a question is about (Markdown link with the full URL, or a file path link).
- A Live page (\"gx page\", \"inline HTML\"): mockups, small tools, picture questions; publish a self-contained HTML file with `ghostex show page.html`, paste the block it prints.
- Check first with `ghostex visual check <file>`.
- Read `ghostex visual guide visuals|forms|pages` before your first. A Boost note: do what it asks.

Open a page you built (a site or app preview)
{browser}
- Ghostex browser: `ghostex browser open <url>` opens or reuses a tab in the project's Browser. To inspect or drive it (snapshot, console, click, fill, screenshot) read `ghostex browser --help`.
- Their default browser: the system opener (open, xdg-open or start).
- If the Ghostex browser can't open it, say what failed; don't switch browsers on your own.

Other agents
- Quick help in your turn: built-in subagents.
- Agents the user can see and talk to (another model, e.g. Codex): `ghostex agents --help` (create, send, close; read: `ghostex read-session-chat <ref> --json`).
- A bigger job across several agents: offer `ghostex coordinator promote` to make this session a coordinator. It starts threads with `ghostex coordinator start-thread` and is told when each finishes, so it never polls. Already a coordinator or thread? Skip this.
- On a connected computer (`ghostex computers`): add `--computer <name>` to `ghostex sessions`, `agents create` and `coordinator start-thread`; its session refs work as they are.

Secrets
- A password, API key or token: `ghostex secret request --label <what> --reason <why>`, never in chat.

Steps only the user can do
- A sudo command, a phone tap, a sign-in, a check by eye: unasked, run `ghostex needs-you add \"<what to do>\" --detail \"<why>\" [--command \"…\"]` and list it under Needs you in your reply. It sits above the chat box with a Done button; the answer arrives as \"Q<n>: Done\". Done: `ghostex needs-you done Q<n>`; not needed: `ghostex needs-you drop Q<n>`.

Guides (built into this Ghostex; read the one for the job)
- `ghostex agents guide` (other agents, reading a thread, renaming a session), `ghostex coordinator guide` (if you are one), `ghostex help guide` (explain Ghostex, change its settings for the user), `ghostex board guide` (Project Board beads), `ghostex secret guide`, `ghostex spaceo guide` (a Mac app on a hidden display).
- Pages in Chrome, Edge or an Electron app, and desktop apps: read the `cua-driver` skill.

Anything else
- `ghostex guide` explains Ghostex; `ghostex settings get <key>` / `set <key> <value>` read and change settings; `ghostex --help` lists every command, `ghostex <command> --help` one area.
- Still unsure: `ghostex help ask \"<question>\"` asks a Ghostex Help session.
";

/// The browser lines, which follow the user's Agents open pages in choice.
fn browser_lines(browser: PreviewBrowser) -> &'static str {
    match browser {
        PreviewBrowser::Ask => "- No browser picked yet: ask once whether they want the Ghostex browser or their default browser, and offer to remember it with `ghostex settings set agentPreviewBrowser internal-browser` (or `system-default-browser`).",
        PreviewBrowser::Ghostex => "- The user picked the Ghostex browser (Settings > Browser > Agents open pages in). Use it, no asking.",
        PreviewBrowser::System => "- The user picked their default browser (Settings > Browser > Agents open pages in). Use it, no asking.",
    }
}

pub(crate) fn agent_guide_text(browser: PreviewBrowser) -> String {
    GUIDE.replace("{browser}", browser_lines(browser))
}

pub fn agent_guide_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("install-skill") => {
            crate::ghostex_cli::skills::install_agent_guide_skill_command(&args[1..])
        }
        None => {
            let paths = crate::paths::get_gxserver_paths(None);
            print!("{}", agent_guide_text(agent_preview_browser(&paths)));
            Ok(())
        }
        Some("help" | "-h" | "--help") => {
            println!("Usage: ghostex agent-guide\n       ghostex agent-guide install-skill [--json]\n\nPrint what an agent running in Ghostex can do with the ghostex CLI: Live pages, Ghostex forms and visuals in the chat, the browser, other agents, secrets, the guide for each job and Ghostex Help. install-skill installs the ghostex skill (/ghostex, $ghostex in Codex), which sends any agent here.");
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "ghostex agent-guide takes no arguments (got \"{other}\")."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_states_the_browser_choice() {
        assert!(agent_guide_text(PreviewBrowser::Ask).contains("ask once"));
        assert!(agent_guide_text(PreviewBrowser::Ghostex).contains("picked the Ghostex browser"));
        assert!(agent_guide_text(PreviewBrowser::System).contains("picked their default browser"));
        assert!(!agent_guide_text(PreviewBrowser::Ask).contains("{browser}"));
    }

    #[test]
    fn the_guide_stays_compact() {
        // About 750 tokens (characters / 4) against about 8,000 for `ghostex --help`.
        let length = agent_guide_text(PreviewBrowser::Ask).len();
        assert!(length < 3_400, "{length}");
    }
}
