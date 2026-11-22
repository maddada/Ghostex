//! `gx spaceo status` (read-only, anyone) and `gx spaceo repair` (the person at the keyboard
//! only), over packages/spaceo-heal.

use std::io::{BufRead, IsTerminal, Write};

use ghostex_spaceo_heal::{repair, Outcome, Spaceo, State};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{ghostex_home, home_dir, CliError, CliResult};

/// Environment variables coding agents set for the commands they run.
const AGENT_ENV_MARKERS: [&str; 7] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX",
    "GEMINI_CLI",
    "OPENCODE",
    "CURSOR_AGENT",
];

fn spaceo() -> CliResult<Spaceo> {
    if !cfg!(target_os = "macos") {
        return Err(CliError::Other(
            crate::agent_skills::SPACEO_MACOS_ONLY_MESSAGE.to_string(),
        ));
    }
    let home = home_dir();
    let exe = ghostex_spaceo_heal::locate(&home).ok_or_else(|| {
        CliError::Other(
            "SpaceO isn't installed. Install it from Settings > Integrations.".to_string(),
        )
    })?;
    Ok(Spaceo {
        exe,
        home,
        state_dir: ghostex_home(),
    })
}

/// `gx spaceo status [--json]`: what state SpaceO is in, read-only.
pub fn status_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let spaceo = spaceo()?;
    let health = spaceo.check();
    if parsed.flags.truthy("json") {
        println!("{}", health.json());
    } else {
        println!("{}", health.summary());
        if let Some(reason) = health.spaceo_reason.as_deref() {
            println!("SpaceO's reason: {reason}");
        }
        if health.state == State::Stuck {
            println!(
                "Fix it with Repair in Settings > Integrations, or run `ghostex spaceo repair` yourself in a terminal."
            );
        }
    }
    Ok(())
}

/// CDXC:ManagedTools 2026-11-16 WHY:
/// Repair runs SpaceO's `--operator` recovery, which SpaceO reserves for the operator. Agents run commands without a terminal on stdin and with their own environment markers, so the command refuses both and asks the person to type `repair`; an agent that needs SpaceO fixed tells the user instead.
fn refuse_unless_person_at_terminal() -> CliResult<()> {
    let agent = AGENT_ENV_MARKERS
        .iter()
        .find(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
    if let Some(name) = agent {
        return Err(CliError::Other(format!(
            "ghostex spaceo repair runs SpaceO's operator recovery, so only you can start it, not an agent ({name} is set). Click Repair in Ghostex (Settings > Integrations > SpaceO), or run it in a plain terminal."
        )));
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CliError::Other(
            "ghostex spaceo repair asks you to confirm, so run it in a terminal (or click Repair in Settings > Integrations > SpaceO).".to_string(),
        ));
    }
    Ok(())
}

/// `gx spaceo repair`: SpaceO's operator recovery, after the person types `repair`.
pub fn repair_command(_args: &[String]) -> CliResult<()> {
    refuse_unless_person_at_terminal()?;
    let spaceo = spaceo()?;
    let health = spaceo.check();
    println!("{}", health.summary());
    if !health.repairable {
        return Ok(());
    }
    if health.state == State::Stuck {
        println!(
            "Repair stops SpaceO (its background service and SpaceO Viewer), runs `spaceo safety clear-host-health --operator`, and starts SpaceO again."
        );
    }
    print!("Type repair to continue: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().lock().read_line(&mut answer);
    if answer.trim() != "repair" {
        println!("Nothing changed.");
        return Ok(());
    }
    let outcome = repair(&spaceo, "cli", &mut |step| println!("{step}"));
    println!("{}", outcome.message());
    match outcome {
        Outcome::Fixed(_) | Outcome::NothingToDo(_) => Ok(()),
        _ => Err(CliError::Other(format!(
            "SpaceO is not fixed. Ghostex's log of what it did: {}",
            spaceo.heal_log().display()
        ))),
    }
}
