//! The commands Ghostex itself shows a user to run, each known by an id gxserver resolves.
//!
//! CDXC:ServerApi 2026-11-25 WHY: Run in terminal types one of these into a Main Hub terminal (terminal.rs). A client names only the id and gxserver works out the text itself, so no client (another computer, the phone, an agent with a session token) can have Ghostex type text of its own choosing into a terminal. To add a command, add its id here with the terminal's title and the function that returns the text Ghostex shows for it now, then show Run in terminal wherever that command is shown, with this id, only when the status that carries the command also carries the id (an older gxserver has no such id).
use crate::domain::DomainStateError;
use crate::server::AppState;

/// The Linux update command Settings > Updates and the update dialog show
/// (app_update/linux_install.rs).
pub(crate) const LINUX_UPDATE: &str = "linuxUpdate";

pub(crate) struct ShownCommand {
    /// The new terminal's title in the sidebar.
    pub(crate) title: &'static str,
    pub(crate) command: String,
}

pub(crate) async fn resolve(state: &AppState, id: &str) -> Result<ShownCommand, DomainStateError> {
    match id {
        LINUX_UPDATE => {
            let command = crate::app_update::shown_update_command(state)
                .await
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "There is no Ghostex update to install on this computer right now.",
                    )
                })?;
            Ok(ShownCommand {
                title: "Update Ghostex",
                command,
            })
        }
        _ => Err(DomainStateError::bad_request(format!(
            "Ghostex has no command called \"{id}\" to run."
        ))),
    }
}
