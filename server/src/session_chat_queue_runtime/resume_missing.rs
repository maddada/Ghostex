//! An agent whose resume command could not find the conversation it was asked to reopen, and
//! what an automatic restart does about it (GitHub #456).
//!
//! CDXC:SessionChat 2026-11-07 WHY:
//! GitHub #456: an OMP session's agent stopped, the send heal restarted it with `omp --session
//! '<id>'`, and OMP no longer had that session: it printed `Error: Session "<id>" not found.` and
//! went back to PowerShell. The queue then waited for an input box that could never come, and
//! every ten minutes (the heal's cooldown) Ghostex restarted it with the same id, for over four
//! hours, while the user's message showed as still waiting. An automatic restart now reads the
//! screen first: when the agent printed that its conversation is gone, the same resume is never
//! run again. OMP (and Empryo, which shares its `--session` grammar and words) start fresh in the
//! same session: the agent itself says nothing is left to reopen under that reference, so a fresh
//! start loses nothing and the held message is delivered to it. The rejected reference is kept in
//! the session's `agentResumeRejected`, so a gxserver restart does not try it again either; the
//! saved conversation file, if one exists, is tried by path before the fresh start. Claude and
//! Codex already start fresh when their transcript was never written (agents/resume_plan.rs); one
//! whose transcript is there but which still could not find it (another account's home, a moved
//! folder) is not restarted again on its own, and the message is marked Not sent with Retry,
//! since a fresh start there would leave the real conversation behind.
//! SEE-ALSO: send_heal.rs (`restart_agent_for_send`), agents/resume_plan.rs (omp arm),
//! session_chat_agent_restart.rs (`heal_failed_start`).

/// The runtime setting that lists the resume references an agent said it could not find.
pub(crate) const AGENT_RESUME_REJECTED: &str = "agentResumeRejected";
/// How many rejected references a session keeps; each one is tried at most once, so a few cover
/// an id, its file and a later id.
const MAX_REJECTED_REFERENCES: usize = 8;
/// How many of the screen's last non-empty lines may hold the error: the agent's own two lines,
/// the shell's prompt, and room for a wrapped id or a prompt that draws more than one line.
const SCAN_LINES: usize = 12;

/// What the screen says about the conversation the agent was asked to resume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MissingResume {
    /// The reference the agent named, when it names one.
    pub(crate) reference: Option<String>,
}

/// Whether `screen` ends with `agent` saying it could not find the conversation it was asked to
/// resume: OMP's and Empryo's `Error: Session "<ref>" not found.`, Claude's `No conversation found
/// with session ID: <id>`, Codex's `No saved session found` (or Ghostex's own "Unable to restore
/// Codex session"). Only the last lines count, so an old error above a later run is not read.
pub(crate) fn missing_resume_on_screen(agent: Option<&str>, screen: &str) -> Option<MissingResume> {
    let agent = crate::agents::identity::normalize_agent_id(agent)?;
    let lines: Vec<&str> = screen
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(SCAN_LINES)..];
    tail.iter().rev().find_map(|line| match agent.as_str() {
        "omp" | "empryo" => omp_missing_session(line),
        "claude" => line
            .find("No conversation found with session ID")
            .map(|start| MissingResume {
                reference: line[start..]
                    .split_once(':')
                    .map(|(_, id)| id.trim().to_string())
                    .filter(|id| !id.is_empty()),
            }),
        "codex" => (line.contains("No saved session found")
            || line.contains("Unable to restore Codex session"))
        .then(|| MissingResume {
            reference: crate::agents::resume_plan::get_uuid_from_text(line),
        }),
        _ => None,
    })
}

/// `Error: Session "<ref>" not found.` (oh-my-pi 18.16 `SessionResolutionError`), with or without
/// the `Error: ` its CLI puts in front.
fn omp_missing_session(line: &str) -> Option<MissingResume> {
    let start = line.find("Session \"")? + "Session \"".len();
    let rest = &line[start..];
    let end = rest.rfind("\" not found")?;
    let reference = rest[..end].trim();
    (!reference.is_empty()).then(|| MissingResume {
        reference: Some(reference.to_string()),
    })
}

/// What an automatic restart does once the screen shows the last resume could not find its
/// conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MissingResumeRestart {
    /// Remember `reject` as gone, so the wake reopens the conversation another way or starts the
    /// agent fresh; the restart is a different command, so the cooldown does not hold it back.
    Reject(String),
    /// The wake would not run the failed resume again (it starts fresh, or the error is about a
    /// reference already set aside), so the ordinary restart rules decide.
    Restart,
    /// Restarting would run the same resume that just failed: stop and mark the message Not sent.
    GiveUp,
}

/// The decision for `agent`, whose screen says `missing`. `stored` holds the references the
/// session's wake can resume by (its id and its saved file), `rejected` the ones already set
/// aside, and `conversation_written` the wake's own transcript rule for Claude and Codex
/// (`agent_conversation_written`: `Some(false)` means the wake starts the agent fresh).
pub(crate) fn restart_after_missing_resume(
    agent: Option<&str>,
    missing: &MissingResume,
    stored: &[&str],
    rejected: &[String],
    conversation_written: Option<bool>,
) -> MissingResumeRestart {
    let already_rejected = |reference: &str| rejected.iter().any(|seen| seen == reference);
    match crate::agents::identity::normalize_agent_id(agent).as_deref() {
        Some("omp" | "empryo") => match missing.reference.as_deref() {
            Some(reference) if already_rejected(reference) => MissingResumeRestart::Restart,
            Some(reference) if stored.contains(&reference) => {
                MissingResumeRestart::Reject(reference.to_string())
            }
            // The agent named something the session does not resume by, so setting it aside
            // would not change the command.
            _ => MissingResumeRestart::GiveUp,
        },
        Some("claude" | "codex") if conversation_written == Some(false) => {
            MissingResumeRestart::Restart
        }
        _ => MissingResumeRestart::GiveUp,
    }
}

/// The references a session's wake set aside (`agentResumeRejected`).
pub(crate) fn rejected_resume_references(
    runtime_settings: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    runtime_settings
        .get(AGENT_RESUME_REJECTED)
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `runtime_settings` with `reference` added to `agentResumeRejected`, keeping the newest few.
pub(crate) fn with_rejected_resume_reference(
    runtime_settings: &mut serde_json::Map<String, serde_json::Value>,
    reference: &str,
) {
    let mut rejected = rejected_resume_references(runtime_settings);
    if !rejected.iter().any(|seen| seen == reference) {
        rejected.push(reference.to_string());
    }
    let skip = rejected.len().saturating_sub(MAX_REJECTED_REFERENCES);
    runtime_settings.insert(
        AGENT_RESUME_REJECTED.to_string(),
        serde_json::Value::from(rejected.split_off(skip)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const OMP_SCREEN: &str =
        "PS C:\\Users\\me\\proj> omp --session '01a1aad2-1111-4000-8000-000000000000'\n\
Error: Session \"01a1aad2-1111-4000-8000-000000000000\" not found.\n\
Run `omp --resume` without an argument to pick from recent sessions, or `omp` to start a new one.\n\
PS C:\\Users\\me\\proj> \n\n";

    #[test]
    fn omp_session_not_found_names_the_reference() {
        assert_eq!(
            missing_resume_on_screen(Some("omp"), OMP_SCREEN),
            Some(MissingResume {
                reference: Some("01a1aad2-1111-4000-8000-000000000000".to_string())
            })
        );
        // A file path the agent was given is named the same way.
        let by_path = "Error: Session \"/Users/me/.omp/agent/sessions/x/a b.jsonl\" not found.\n$ ";
        assert_eq!(
            missing_resume_on_screen(Some("omp"), by_path).and_then(|missing| missing.reference),
            Some("/Users/me/.omp/agent/sessions/x/a b.jsonl".to_string())
        );
        assert!(missing_resume_on_screen(Some("empryo"), OMP_SCREEN).is_some());
    }

    #[test]
    fn other_agents_words_and_old_errors_are_not_read_as_omps() {
        // Another agent's screen with the same words is not OMP's error.
        assert_eq!(missing_resume_on_screen(Some("gemini"), OMP_SCREEN), None);
        assert_eq!(missing_resume_on_screen(None, OMP_SCREEN), None);
        // An error that scrolled above a later run of the agent is not the last start's.
        let later_run = format!(
            "{OMP_SCREEN}{}",
            (0..20)
                .map(|row| format!("omp output row {row}\n"))
                .collect::<String>()
        );
        assert_eq!(missing_resume_on_screen(Some("omp"), &later_run), None);
        // The ordinary shell, and OMP's own words about something else, are not a missing session.
        assert_eq!(
            missing_resume_on_screen(Some("omp"), "PS C:\\proj> omp\nGoodbye.\nPS C:\\proj> "),
            None
        );
        assert_eq!(
            missing_resume_on_screen(Some("omp"), "Error: Session \"\" not found.\n$ "),
            None
        );
    }

    #[test]
    fn claude_and_codex_say_it_their_way() {
        let claude =
            "No conversation found with session ID: 9f0c2a4e-1111-4000-8000-000000000000\n% ";
        assert_eq!(
            missing_resume_on_screen(Some("claude"), claude),
            Some(MissingResume {
                reference: Some("9f0c2a4e-1111-4000-8000-000000000000".to_string())
            })
        );
        let codex =
            "Error: No saved session found with ID 019a-x\nUnable to restore Codex session\n% ";
        assert!(missing_resume_on_screen(Some("codex"), codex).is_some());
        assert_eq!(missing_resume_on_screen(Some("codex"), claude), None);
    }

    #[test]
    fn omp_sets_aside_its_own_reference_and_never_retries_it() {
        let id = "01a1aad2-1111-4000-8000-000000000000";
        let missing = MissingResume {
            reference: Some(id.to_string()),
        };
        let stored = [id, "/sessions/a.jsonl"];
        assert_eq!(
            restart_after_missing_resume(Some("omp"), &missing, &stored, &[], None),
            MissingResumeRestart::Reject(id.to_string())
        );
        // Once set aside, the wake no longer runs it: an error still on screen from that run is
        // left to the ordinary restart rules.
        assert_eq!(
            restart_after_missing_resume(Some("omp"), &missing, &stored, &[id.to_string()], None),
            MissingResumeRestart::Restart
        );
        // A reference the session does not resume by cannot be set aside: setting it aside would
        // leave the same command, so the restart stops.
        let foreign = MissingResume {
            reference: Some("someone-else".to_string()),
        };
        assert_eq!(
            restart_after_missing_resume(Some("omp"), &foreign, &stored, &[], None),
            MissingResumeRestart::GiveUp
        );
    }

    #[test]
    fn claude_and_codex_restart_only_when_the_wake_starts_fresh() {
        let missing = MissingResume { reference: None };
        for agent in ["claude", "codex"] {
            assert_eq!(
                restart_after_missing_resume(Some(agent), &missing, &[], &[], Some(false)),
                MissingResumeRestart::Restart
            );
            // A transcript is there, so the wake would resume it the same way again.
            assert_eq!(
                restart_after_missing_resume(Some(agent), &missing, &[], &[], Some(true)),
                MissingResumeRestart::GiveUp
            );
        }
        assert_eq!(
            restart_after_missing_resume(Some("pi"), &missing, &[], &[], None),
            MissingResumeRestart::GiveUp
        );
    }

    #[test]
    fn rejected_references_are_kept_once_and_capped() {
        let mut settings = serde_json::Map::new();
        with_rejected_resume_reference(&mut settings, "a");
        with_rejected_resume_reference(&mut settings, "a");
        assert_eq!(rejected_resume_references(&settings), vec!["a".to_string()]);
        for index in 0..20 {
            with_rejected_resume_reference(&mut settings, &format!("r{index}"));
        }
        let kept = rejected_resume_references(&settings);
        assert_eq!(kept.len(), MAX_REJECTED_REFERENCES);
        assert_eq!(kept.last().map(String::as_str), Some("r19"));
    }
}
