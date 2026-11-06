use crate::agents::{agent_preview_browser, PreviewBrowser};
use crate::ghostex_cli::rpc::{CliError, CliResult};

/// CDXC:AgentGuide 2026-11-02 DECISION:
/// User (Q1065): the note every agent session gets stays "very short and point[s] to `ghostex agent-guide`", which holds the how-tos: other agents split by size (Q1071: built-in subagents for quick help, offer a coordinator for bigger jobs), the browser choice and how to save it (Q1069), pages, questions and charts in the chat, secrets, installing Ghostex skills for the user, and `ghostex help ask` (Q1070). Keep it compact: an agent reads it whole, and the full CLI help is about 8,000 tokens.
///
/// CDXC:AgentGuide 2026-11-06 DECISION:
/// User: "i hate saying 'inline html' and 'inline q' and inline graphs", then "ghostex page / gx page, ghostex form / gx form, ghostex visuals / gx visuals", everywhere: agents understand the new words, the old ones keep working quietly, and agents use the new names when they talk to the user.
/// SEE-ALSO: server/src/agents/ghostex_note.rs (the note and the browser setting), skills/ghostex-agent-guide/SKILL.md.
const GUIDE: &str = "Ghostex agent guide

You run inside Ghostex, an app where the user runs coding agents in sessions they see in a sidebar, as a chat or a terminal. Use this only for what the user asks for.

Show things in the chat (use these names with the user; older words mean the same)
- Ghostex visuals (gx visuals; \"inline graphs\"): charts, stat tiles and tables, a ```visual block in your reply.
- A Ghostex form (gx form; \"inline questions\"): questions with choices, a ```questions block; link what a question is about (a Markdown link with the full URL, or a file path link).
- A Ghostex page (gx page; \"inline HTML\"): mockups, small tools, questions that need a picture; publish a self-contained HTML file with `ghostex show page.html` and paste the block it prints.
- Check a block or page first with `ghostex visual check <file>`.
- The block and page formats are in the ghostex-visuals skill. If it isn't installed, offer to install it: `ghostex visual install-skill`.

Open a page you built (a site or app preview)
{browser}
- Ghostex browser: `ghostex browser open <url>` opens or reuses a tab in this project's Browser. To inspect or drive it (snapshot, console, click, fill, screenshot), use the ghostex-embedded-browser-use skill (`ghostex browser install-skill`) or `ghostex browser --help`.
- Their default browser: open the URL with the system's opener (open, xdg-open or start).
- If the Ghostex browser can't open it, say what failed instead of switching browsers on your own.

Other agents
- Quick help inside your turn: your own built-in subagents.
- Agents the user can see and talk to (another model, Codex beside Claude): `ghostex agents --help` (create, send, close; read replies with `ghostex read-session-chat <ref> --json`).
- A bigger job split across several agents: offer to make this session a coordinator with `ghostex coordinator promote`. A coordinator starts threads with `ghostex coordinator start-thread` and is told when each finishes, so it never polls their terminals. Already a coordinator or a thread? Skip this.
- On another computer the user connected (`ghostex computers`): add `--computer <name>` to `ghostex sessions`, `agents create` and `coordinator start-thread`; its session refs work as they are.

Secrets
- A password, API key or token: `ghostex secret request --label <what> --reason <why>`. Never ask for it in chat.

Ghostex skills
- Install one when the user wants what it does: `ghostex visual install-skill` (ghostex-visuals), `ghostex browser install-skill` (ghostex-embedded-browser-use), `ghostex browser-use install-skill` (other browsers through Cua Driver), `ghostex computer-use install-skill` (drive desktop apps), `ghostex agents-orchestration install-skill` (ghostex-agents), `ghostex guide install-skill` (ghostex-help).

Anything else
- `ghostex guide` explains Ghostex; `ghostex settings get <key>` and `ghostex settings set <key> <value>` read and change a setting; `ghostex --help` lists every command and `ghostex <command> --help` one area.
- Still unsure: `ghostex help ask \"<question>\"` asks a Ghostex Help session and prints its answer.
";

/// The browser lines, which follow the user's Agents open pages in choice.
fn browser_lines(browser: PreviewBrowser) -> &'static str {
    match browser {
        PreviewBrowser::Ask => "- The user hasn't picked a browser yet: ask once whether they want the Ghostex browser or their own default browser, and offer to remember it with `ghostex settings set agentPreviewBrowser internal-browser` (or `system-default-browser`).",
        PreviewBrowser::Ghostex => "- The user picked the Ghostex browser (Settings > Browser > Agents open pages in). Use it without asking.",
        PreviewBrowser::System => "- The user picked their default browser (Settings > Browser > Agents open pages in). Use it without asking.",
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
            println!("Usage: ghostex agent-guide\n       ghostex agent-guide install-skill [--json]\n\nPrint what an agent running in Ghostex can do with the ghostex CLI: Ghostex pages, forms and visuals in the chat, the browser, other agents, secrets, Ghostex skills and Ghostex Help. install-skill installs the ghostex-agent-guide skill, which gives agents Ghostex can't hand a launch note the same short note.");
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
