mod model;
mod scan;
mod ui;

use crate::model::Agent;
use crate::model::Session;
use crate::scan::ScanOptions;
use std::env;
use std::io;
use std::io::Write;
use std::path::PathBuf;

/*
CDXC:GhostexHistoryCli 2026-06-25-20:43:
The scriptable --list mode is a companion to the alt-screen viewer for quick filtering and smoke checks.
Treat closed stdout pipes as normal termination so commands like ghostex-history --list | head do not report a panic.

CDXC:GhostexHistoryCli 2026-06-25-21:54:
Ctrl+R resume should follow Zehn's agent command matrix and accept-all option shape.
Codex and Claude always use their required permission flags. Expose
--accept-all/--no-accept-all on ghostex-history so Ghostex-owned launchers can
pass the gxserver global permission policy for the other agents through without
parsing TUI state themselves.
*/

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse()?;
    if cli.help {
        print_help();
        return Ok(());
    }
    let sessions = scan::scan_sessions(&ScanOptions {
        home: cli.home,
        agent_filter: cli.agent,
    });
    if cli.list {
        write_session_list(&sessions)?;
        return Ok(());
    }
    ui::run(sessions, cli.accept_all_resume)?;
    Ok(())
}

struct Cli {
    accept_all_resume: bool,
    agent: Option<Agent>,
    help: bool,
    home: PathBuf,
    list: bool,
}

impl Cli {
    fn parse() -> Result<Self, String> {
        let mut args = env::args().skip(1);
        let mut agent = None;
        let mut help = false;
        let mut home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_string())?;
        let mut list = false;
        let mut accept_all_resume = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--accept-all" => accept_all_resume = true,
                "--agent" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--agent requires a value".to_string())?;
                    agent = Agent::from_filter(&value)
                        .ok_or_else(|| format!("unknown agent '{value}'"))
                        .map(Some)?;
                }
                "--help" | "-h" => help = true,
                "--home" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--home requires a value".to_string())?;
                    home = PathBuf::from(value);
                }
                "--list" => list = true,
                "--no-accept-all" => accept_all_resume = false,
                other => return Err(format!("unknown argument '{other}'")),
            }
        }

        Ok(Self {
            accept_all_resume,
            agent,
            help,
            home,
            list,
        })
    }
}

fn write_session_list(sessions: &[Session]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for session in sessions {
        if let Err(error) = writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            session.agent,
            session.updated_at,
            session.id,
            session.project,
            session.display_title()
        ) {
            if error.kind() == io::ErrorKind::BrokenPipe {
                return Ok(());
            }
            return Err(error);
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "ghostex-history\n\nUSAGE:\n    ghostex-history [--agent codex|claude|pi|cursor|grok] [--home PATH] [--accept-all|--no-accept-all] [--list]\n\nKEYS:\n    type search text, up/down or ctrl+p/ctrl+n move, pageup/pagedown page\n    enter or ctrl+t opens the transcript, ctrl+r resumes the selected agent session\n    ctrl+e expands preview, ctrl+o toggles density\n    tab focuses filter/sort, left/right changes the focused option\n    transcript: up/down, pageup/pagedown, ctrl+u/ctrl+d, home/end scroll; ctrl+r resumes; q/esc/ctrl+t closes\n"
    );
}
