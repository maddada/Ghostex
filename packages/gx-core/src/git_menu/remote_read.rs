//! What a failed Git read on another computer does next: read again quietly, or tell the user
//! which computer it was and why.
//!
//! CDXC:Git 2026-11-04 WHY:
//! A remote project's Git state is read on every project switch, every Changes-panel file change
//! and every menu open, so a tunnel that blinks for a second, a slow link or a remote gxserver
//! restarting after an update turned into a bare "Remote Git unavailable" toast at random times,
//! naming neither the computer nor the reason (GitHub #405). A read that fails now reads again
//! after a short wait, and only a read that keeps failing shows one toast that names the computer
//! and says why in plain words.

use super::text::user_visible_git_error;

/// The waits before each quiet re-read of a remote project's Git state. A read that still fails
/// after the last one is shown.
pub const REMOTE_GIT_READ_RETRY_DELAYS_MS: [u64; 2] = [2_000, 6_000];

/// Why a read of another computer's Git state failed, in the terms the toast needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteGitReadFailure {
    /// No answer came back: the tunnel is down, the link is slow, or its gxserver is restarting.
    NotAnswering,
    /// The computer answered and refused the read, with its own reason.
    Refused(String),
    /// The computer answered with something this app could not read.
    UnexpectedAnswer,
}

/// What the host does after a failed read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteGitReadStep {
    /// Read again after this many milliseconds, showing nothing.
    RetryAfter(u64),
    /// Stop reading and show this toast.
    Toast { title: String, description: String },
}

/// The step after failed read number `attempt` (0 for the first read).
pub fn plan_remote_git_read_failure(
    attempt: usize,
    machine_name: Option<&str>,
    failure: &RemoteGitReadFailure,
) -> RemoteGitReadStep {
    match REMOTE_GIT_READ_RETRY_DELAYS_MS.get(attempt) {
        Some(&delay_ms) => RemoteGitReadStep::RetryAfter(delay_ms),
        None => {
            let (title, description) = remote_git_unavailable_toast(machine_name, failure);
            RemoteGitReadStep::Toast { title, description }
        }
    }
}

/// The toast for a remote Git read that kept failing: the title names the computer, the
/// description says why.
pub fn remote_git_unavailable_toast(
    machine_name: Option<&str>,
    failure: &RemoteGitReadFailure,
) -> (String, String) {
    let name = machine_name.map(str::trim).filter(|name| !name.is_empty());
    let title = match name {
        Some(name) => format!("Git unavailable on {name}"),
        None => "Git unavailable on remote computer".to_string(),
    };
    let subject = name.unwrap_or("The remote computer");
    let description = match failure {
        RemoteGitReadFailure::NotAnswering => {
            format!("{subject} is not answering. Check that it is awake and online.")
        }
        RemoteGitReadFailure::Refused(reason) => {
            let reason = user_visible_git_error(reason, "");
            let reason = reason.trim_end_matches('.');
            match reason.is_empty() {
                true => format!("{subject} could not read this project's Git state."),
                false => format!("{subject} could not read this project's Git state: {reason}."),
            }
        }
        RemoteGitReadFailure::UnexpectedAnswer => {
            format!("{subject} sent an answer this app could not read.")
        }
    };
    (title, description)
}

/// The toast when a remote project's Git state is asked for while its computer is not connected.
pub fn remote_git_disconnected_toast(machine_name: Option<&str>) -> (String, String) {
    let name = machine_name.map(str::trim).filter(|name| !name.is_empty());
    let title = match name {
        Some(name) => format!("Git unavailable on {name}"),
        None => "Git unavailable on remote computer".to_string(),
    };
    let subject = name.unwrap_or("The remote computer");
    (
        title,
        format!("{subject} is not connected. Reconnect it to see its Git state."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs scripted read results through the plan the way the host's read loop does, returning
    /// the waits it took and the toast it ended on, if any.
    fn drive(reads: &[Result<(), RemoteGitReadFailure>]) -> (Vec<u64>, Option<String>) {
        let mut waits = Vec::new();
        for (attempt, read) in reads.iter().enumerate() {
            let Err(failure) = read else {
                return (waits, None);
            };
            match plan_remote_git_read_failure(attempt, Some("Studio"), failure) {
                RemoteGitReadStep::RetryAfter(delay_ms) => waits.push(delay_ms),
                RemoteGitReadStep::Toast { title, .. } => return (waits, Some(title)),
            }
        }
        (waits, None)
    }

    #[test]
    fn a_blip_that_recovers_shows_nothing() {
        let blip = Err(RemoteGitReadFailure::NotAnswering);
        assert_eq!(drive(&[blip.clone(), Ok(())]), (vec![2_000], None));
        assert_eq!(
            drive(&[blip.clone(), blip.clone(), Ok(())]),
            (vec![2_000, 6_000], None)
        );
    }

    #[test]
    fn a_read_that_keeps_failing_is_shown_once_after_the_retries() {
        let down = Err(RemoteGitReadFailure::NotAnswering);
        assert_eq!(
            drive(&[down.clone(), down.clone(), down]),
            (
                vec![2_000, 6_000],
                Some("Git unavailable on Studio".to_string())
            )
        );
    }

    #[test]
    fn a_read_that_stays_down_names_the_computer_and_the_reason() {
        let step = plan_remote_git_read_failure(
            REMOTE_GIT_READ_RETRY_DELAYS_MS.len(),
            Some("Studio"),
            &RemoteGitReadFailure::NotAnswering,
        );
        assert_eq!(
            step,
            RemoteGitReadStep::Toast {
                title: "Git unavailable on Studio".to_string(),
                description: "Studio is not answering. Check that it is awake and online."
                    .to_string(),
            }
        );
    }

    #[test]
    fn a_refusal_carries_the_computers_own_reason() {
        let (title, description) = remote_git_unavailable_toast(
            Some("  Build box "),
            &RemoteGitReadFailure::Refused("Project not found.\n".to_string()),
        );
        assert_eq!(title, "Git unavailable on Build box");
        assert_eq!(
            description,
            "Build box could not read this project's Git state: Project not found."
        );
    }

    #[test]
    fn an_unnamed_computer_still_reads_as_a_sentence() {
        let (title, description) =
            remote_git_unavailable_toast(Some(""), &RemoteGitReadFailure::Refused(String::new()));
        assert_eq!(title, "Git unavailable on remote computer");
        assert_eq!(
            description,
            "The remote computer could not read this project's Git state."
        );
        let (_, description) =
            remote_git_unavailable_toast(None, &RemoteGitReadFailure::UnexpectedAnswer);
        assert!(description.starts_with("The remote computer sent an answer"));
    }

    #[test]
    fn a_disconnected_computer_is_named() {
        assert_eq!(
            remote_git_disconnected_toast(Some("Studio")),
            (
                "Git unavailable on Studio".to_string(),
                "Studio is not connected. Reconnect it to see its Git state.".to_string()
            )
        );
    }
}
