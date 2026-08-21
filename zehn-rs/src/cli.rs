//! Command-line entry point shared by the standalone `zehn` binary and by
//! Ghostex's `gx f` / `gx find`, which calls `run` in-process.

use std::io::{IsTerminal, Write};

use crate::actions;
use crate::agent::Agent;
use crate::index::SearchIndex;
use crate::tui::{ActionKind, Tui};

pub const HELP: &str = concat!(
    "zehn (ذهن, \"the mind\") — fuzzy finder & resumer for AI agent sessions\n",
    "\n",
    "Sources: claude (~/.claude), codex (~/.codex), pi (~/.pi),\n",
    "         opencode (~/.local/share/opencode/opencode.db),\n",
    "         cursor (~/.cursor/projects), grok (~/.grok/sessions)\n",
    "\n",
    "Usage:\n",
    "  zehn            find a prompt, then RESUME that session in its agent\n",
    "  zehn --print    just print the selected prompt text (no resume)\n",
    "  zehn --project  print agent<TAB>project<TAB>text (implies --print)\n",
    "  zehn --agent claude   show only one agent (claude/codex/pi/opencode/cursor/grok)\n",
    "  zehn --claude         shorthand for --agent claude\n",
    "  zehn --accept-all     resume with supported permission-bypass flags\n",
    "  zehn --list     dump all records\n",
    "\n",
    "Resume:  claude [--dangerously-skip-permissions] --resume <id>\n",
    "         codex [--yolo] resume <id>\n",
    "         pi --session <id>\n",
    "         opencode [--dangerously-skip-permissions] --session <id>\n",
    "         cursor-agent [--yolo] --resume <id>\n",
    "         grok [--permission-mode bypassPermissions] --resume <id>\n",
    "(run from the session's project directory)\n",
    "\n",
    "Keys: type to filter · ↑/↓ or ^p/^n move · Enter resume\n",
    "      mouse wheel moves · clicks do not select · ^d day grouping\n",
    "      PgUp/PgDn day · ^g agents · ^j projects\n",
    "      ^f favorite · ^e view · ^y copy · ^o fork\n",
    "      Esc/^c quit\n",
    "\n",
    "Favorites are stored in $XDG_CONFIG_HOME/zehn/favorites (or ~/.config/zehn).\n",
);

const USAGE_LINE: &str = "usage: zehn [--agent claude|codex|pi|opencode|cursor|grok] [--accept-all|--no-accept-all] [--print|--project|--list]";

#[derive(Default)]
struct Options {
    print_project: bool,
    print_only: bool,
    list_only: bool,
    accept_all: bool,
    agent_filter: Option<Agent>,
}

fn parse_agent_flag(arg: &str) -> Option<Agent> {
    Agent::parse(arg.strip_prefix("--")?)
}

/// Run the picker. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut options = Options::default();
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--version" | "-v" => {
                println!("zehn {} (bundled in Ghostex)", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            "update" => {
                println!("zehn now ships inside Ghostex — update Ghostex to update it.");
                return 0;
            }
            "--list" => options.list_only = true,
            "--project" => {
                options.print_project = true;
                options.print_only = true;
            }
            "--print" => options.print_only = true,
            "--accept-all" => options.accept_all = true,
            "--no-accept-all" => options.accept_all = false,
            "--help" | "-h" => {
                print!("{HELP}");
                return 0;
            }
            "--agent" => {
                i += 1;
                match args.get(i).and_then(|name| Agent::parse(name)) {
                    Some(agent) => options.agent_filter = Some(agent),
                    None => {
                        return usage_error(
                            "--agent needs one of: claude, codex, pi, opencode, cursor, grok",
                        )
                    }
                }
            }
            _ => {
                if let Some(rest) = arg.strip_prefix("--agent=") {
                    match Agent::parse(rest) {
                        Some(agent) => options.agent_filter = Some(agent),
                        None => return usage_error("unknown agent"),
                    }
                } else if let Some(agent) = parse_agent_flag(arg) {
                    options.agent_filter = Some(agent);
                }
            }
        }
        i += 1;
    }

    // CDXC:AgentHistorySearch 2026-06-07-14:59:
    // Interactive startup can spend visible time indexing previous agent prompts
    // and metadata before the picker appears. Show a transient terminal status
    // line while scanning so users are not left staring at a blank launch.
    let interactive = !options.list_only;
    let show_indexing = interactive && std::io::stdout().is_terminal();
    if show_indexing {
        print!("\r\x1b[2KLoading: Indexing Previous User Prompts...");
        let _ = std::io::stdout().flush();
    }

    let Some(mut index) = SearchIndex::build_from_env() else {
        eprintln!("zehn: HOME not set");
        return 1;
    };
    if show_indexing {
        print!("\r\x1b[2K");
        let _ = std::io::stdout().flush();
    }
    if let Some(agent) = options.agent_filter {
        index.retain_agent(agent);
    }

    if options.list_only {
        let mut out = String::new();
        for rec in &index.records {
            out.push_str(rec.agent.label());
            out.push('\t');
            out.push_str(&rec.project);
            out.push('\t');
            push_sanitized(&mut out, &rec.text);
            out.push('\n');
        }
        print!("{out}");
        return 0;
    }

    if let Some(err) = &index.opencode_error {
        eprintln!("zehn: opencode history found but could not be read — {err}");
    }

    if index.records.is_empty() {
        match options.agent_filter {
            Some(agent) => println!("zehn: no {} history found", agent.label()),
            None => println!("zehn: no history found"),
        }
        return 0;
    }

    let picked = match Tui::new(&mut index).run() {
        Ok(picked) => picked,
        Err(err) => {
            eprintln!("zehn: {err}");
            return 1;
        }
    };
    let Some(action) = picked else { return 0 };
    let rec = index.records[action.index].clone();

    if options.print_only {
        let mut out = String::new();
        if options.print_project {
            out.push_str(rec.agent.label());
            out.push('\t');
            out.push_str(&rec.project);
            out.push('\t');
        }
        push_sanitized(&mut out, &rec.text);
        println!("{out}");
        return 0;
    }

    let outcome = match action.kind {
        ActionKind::ResumeSession => actions::resume_session(&rec, options.accept_all),
        ActionKind::Copy => actions::copy_to_clipboard(&rec.text).map(|_| {
            println!("\x1b[90m→ copied {} prompt to clipboard\x1b[0m", rec.agent.label());
        }),
        ActionKind::View => actions::view_prompt(&rec),
        ActionKind::Fork => actions::fork_session(&rec, action.fork_agent),
    };
    match outcome {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("zehn: {err}");
            1
        }
    }
}

fn usage_error(msg: &str) -> i32 {
    eprintln!("zehn: {msg}");
    eprintln!("{USAGE_LINE}");
    1
}

fn push_sanitized(out: &mut String, text: &str) {
    for ch in text.chars() {
        out.push(if ch == '\n' || ch == '\t' || ch == '\r' { ' ' } else { ch });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_shorthand_flags_resolve() {
        assert_eq!(parse_agent_flag("--codex"), Some(Agent::Codex));
        assert_eq!(parse_agent_flag("--cursor"), Some(Agent::Cursor));
        assert_eq!(parse_agent_flag("codex"), None);
        assert_eq!(parse_agent_flag("--nope"), None);
    }

    #[test]
    fn sanitizing_flattens_newlines_and_tabs() {
        let mut out = String::new();
        push_sanitized(&mut out, "a\nb\tc\rd");
        assert_eq!(out, "a b c d");
    }

    #[test]
    fn help_documents_the_remapped_hotkeys() {
        assert!(HELP.contains("^g agents"));
        assert!(HELP.contains("^j projects"));
        assert!(!HELP.contains("^t agents"));
        assert!(!HELP.contains("^r projects"));
    }
}
