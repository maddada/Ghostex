//! `gxserver --at-login`: what the login entry runs when the user logs in.

use std::time::Duration;

use anyhow::{anyhow, Result};

use super::ComputerStartupSettings;
use crate::{
    cli::start_gxserver_background, constants::GXSERVER_VERSION, paths::get_gxserver_paths,
    runtime::read_current_build_identity,
};

/// Right after login the disk, the network and the Keychain may still be settling, so a start that
/// does not answer gets about a minute more, like the login-recovery agent.
const START_ATTEMPTS: u32 = 12;
const START_RETRY_DELAY: Duration = Duration::from_secs(5);

/// Starts gxserver, then opens the app when Also open the Ghostex app is on. The settings are read
/// again here, so an entry left behind by a switch turned off while gxserver was down does nothing
/// and is removed.
pub(crate) async fn run() -> Result<()> {
    let paths = get_gxserver_paths(None);
    let settings = ComputerStartupSettings::read(&paths);
    if !settings.start_server {
        return super::login_entry::apply(&paths, false);
    }
    let version = GXSERVER_VERSION.to_string();
    let build_identity = read_current_build_identity(&version)?;
    let mut last_problem = None;
    for attempt in 0..START_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(START_RETRY_DELAY).await;
        }
        match start_gxserver_background(&build_identity, &version).await {
            Ok(status) if status.ok && status.state == "running" => {
                last_problem = None;
                break;
            }
            Ok(status) => last_problem = Some(status.message),
            Err(error) => last_problem = Some(format!("{error:#}")),
        }
    }
    if settings.open_app {
        // The app connects to the server started above; it starts its own when that one failed.
        let opened =
            tokio::task::spawn_blocking(crate::ghostex_cli::launchers::ghostex_desktop_command)
                .await
                .map_err(|error| anyhow!("open the Ghostex app: {error}"))?;
        if let Err(error) = opened {
            eprintln!("gxserver --at-login could not open the Ghostex app: {error}");
        }
    }
    match last_problem {
        Some(problem) => Err(anyhow!("gxserver did not start at login: {problem}")),
        None => Ok(()),
    }
}
