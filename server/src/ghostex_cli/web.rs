use std::process::Command;

use crate::ghostex_cli::rpc::{CliError, CliResult};

const GHOSTEX_WEB_URL: &str = "http://127.0.0.1:58744/";

pub fn web_command(args: &[String]) -> CliResult<()> {
    if matches!(
        args.first().map(String::as_str),
        Some("help") | Some("-h") | Some("--help")
    ) {
        println!("Usage: ghostex web\n\nOpen {GHOSTEX_WEB_URL} in the default browser.");
        return Ok(());
    }
    if let Some(argument) = args.first() {
        return Err(CliError::Other(format!(
            "Unknown web argument: {argument}\n\nUsage: ghostex web"
        )));
    }

    println!("{GHOSTEX_WEB_URL}");
    let mut command = browser_open_command()?;
    command
        .arg(GHOSTEX_WEB_URL)
        .spawn()
        .map_err(|error| CliError::Other(format!("Failed to open {GHOSTEX_WEB_URL}: {error}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn browser_open_command() -> CliResult<Command> {
    Ok(Command::new("open"))
}

#[cfg(target_os = "linux")]
fn browser_open_command() -> CliResult<Command> {
    Ok(Command::new("xdg-open"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn browser_open_command() -> CliResult<Command> {
    Err(CliError::Other(
        "ghostex web supports browser opening on macOS and Linux.".to_string(),
    ))
}
