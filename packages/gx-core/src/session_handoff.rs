//! A handoff's follow-up session: the text its message box starts with, and the order of the steps
//! when a session is dragged onto another project.
//!
//! CDXC:TranscriptExport 2026-10-31 DECISION:
//! User (Q863): "put may caret 2 lines below the handoff document pls" and "please put some text before the handoff document that say something about 'you are continuing from xyz'". Every handoff's first message reads the same: a lead-in naming the session and project it came from, the document's link, a blank line, and the caret on the line after it. It replaces the bare link for all of them (Handoff to an agent wherever it goes, the model picker's and the drag's), because the lead-in tells the new agent what the link is in each case, and one draft keeps them from drifting.
//!
//! CDXC:Sidebar 2026-10-31 DECISION:
//! User (Q866): "stop-first". User (Q864): a "Close source session" toggle, on by default. A dropped session that is working is stopped first, then its conversation is written and the new session started; the source closes last and only when the toggle is on. Any step that fails ends the handoff with its reason, so a failure never closes the source and nothing after a failed stop runs.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/session_handoff.rs (which drops hand off),
//! apps/desktop/src/app/gx_store/git/session_handoff.rs (runs the steps),
//! apps/desktop/src/app/gx_store/git/export_transcript.rs (the other handoffs).

/// `You are continuing from "<session>" in <project>.`
pub fn handoff_lead_in(session_title: &str, project_title: &str) -> String {
    let session_title = one_line(session_title);
    let project_title = one_line(project_title);
    match (session_title.is_empty(), project_title.is_empty()) {
        (false, false) => {
            format!("You are continuing from \"{session_title}\" in {project_title}.")
        }
        (false, true) => format!("You are continuing from \"{session_title}\"."),
        (true, false) => format!("You are continuing from a session in {project_title}."),
        (true, true) => "You are continuing from another session.".to_string(),
    }
}

/// What a handoff's new message box holds: the lead-in, the document's link, and the two line
/// breaks that leave the caret two lines below it. Never sent for the user.
///
/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: Ghostex-generated file references use descriptive Markdown links instead of @filepath,
/// with a purpose and reference number or a session-title Handoff label.
/// SEE-ALSO: packages/shared/session-chat-file-references.ts.
pub fn handoff_draft(path: &str, session_title: &str, project_title: &str) -> String {
    let label = one_line(&format!("{session_title} Handoff"));
    let mut escaped_label = String::new();
    for ch in label.chars() {
        if matches!(ch, '\\' | '[' | ']') {
            escaped_label.push('\\');
        }
        escaped_label.push(ch);
    }
    let destination = if path
        .chars()
        .any(|ch| ch.is_whitespace() || ch == '<' || ch == '>')
    {
        let mut escaped = String::from("<");
        for ch in path.chars() {
            if matches!(ch, '\\' | '<' | '>') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        escaped.push('>');
        escaped
    } else {
        let mut escaped = String::new();
        for ch in path.chars() {
            if matches!(ch, '\\' | '(' | ')') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        escaped
    };
    let lead_in = handoff_lead_in(session_title, project_title);
    // The line breaks must survive untrimmed all the way to the daemon.
    format!("{lead_in}\n\n[{escaped_label}]({destination})\n\n")
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a confirmed drop does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionHandoffOptions {
    /// The source's agent is working, so its turn is stopped first.
    pub stop_first: bool,
    /// The "Close source session" toggle.
    pub close_source: bool,
}

/// The step that just ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionHandoffOutcome {
    Confirmed,
    Stopped,
    StopFailed(String),
    Exported,
    ExportFailed(String),
    Started,
    StartFailed(String),
}

/// What to do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionHandoffStep {
    StopTurn,
    ExportTranscript,
    StartSession,
    CloseSource,
    Finished,
    /// The handoff ends here, with this reason. The source is not closed.
    Failed(String),
}

pub fn next_session_handoff_step(
    options: SessionHandoffOptions,
    outcome: SessionHandoffOutcome,
) -> SessionHandoffStep {
    match outcome {
        SessionHandoffOutcome::Confirmed if options.stop_first => SessionHandoffStep::StopTurn,
        SessionHandoffOutcome::Confirmed | SessionHandoffOutcome::Stopped => {
            SessionHandoffStep::ExportTranscript
        }
        SessionHandoffOutcome::Exported => SessionHandoffStep::StartSession,
        SessionHandoffOutcome::Started if options.close_source => SessionHandoffStep::CloseSource,
        SessionHandoffOutcome::Started => SessionHandoffStep::Finished,
        SessionHandoffOutcome::StopFailed(why)
        | SessionHandoffOutcome::ExportFailed(why)
        | SessionHandoffOutcome::StartFailed(why) => SessionHandoffStep::Failed(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks a handoff to its end, answering each step from `answers` (success when it has none).
    fn run(
        options: SessionHandoffOptions,
        fail_at: Option<SessionHandoffStep>,
    ) -> Vec<SessionHandoffStep> {
        let mut steps = Vec::new();
        let mut step = next_session_handoff_step(options, SessionHandoffOutcome::Confirmed);
        loop {
            steps.push(step.clone());
            let failed = fail_at.as_ref() == Some(&step);
            let outcome = match (&step, failed) {
                (SessionHandoffStep::StopTurn, false) => SessionHandoffOutcome::Stopped,
                (SessionHandoffStep::StopTurn, true) => {
                    SessionHandoffOutcome::StopFailed("no stop".to_string())
                }
                (SessionHandoffStep::ExportTranscript, false) => SessionHandoffOutcome::Exported,
                (SessionHandoffStep::ExportTranscript, true) => {
                    SessionHandoffOutcome::ExportFailed("no file".to_string())
                }
                (SessionHandoffStep::StartSession, false) => SessionHandoffOutcome::Started,
                (SessionHandoffStep::StartSession, true) => {
                    SessionHandoffOutcome::StartFailed("no session".to_string())
                }
                _ => return steps,
            };
            step = next_session_handoff_step(options, outcome);
        }
    }

    const IDLE_CLOSING: SessionHandoffOptions = SessionHandoffOptions {
        stop_first: false,
        close_source: true,
    };
    const BUSY_CLOSING: SessionHandoffOptions = SessionHandoffOptions {
        stop_first: true,
        close_source: true,
    };

    #[test]
    fn the_toggle_closes_the_source_after_the_new_session_exists() {
        use SessionHandoffStep::*;
        assert_eq!(
            run(IDLE_CLOSING, None),
            vec![ExportTranscript, StartSession, CloseSource]
        );
    }

    #[test]
    fn with_the_toggle_off_the_source_is_kept() {
        use SessionHandoffStep::*;
        let keep = SessionHandoffOptions {
            stop_first: false,
            close_source: false,
        };
        assert_eq!(
            run(keep, None),
            vec![ExportTranscript, StartSession, Finished]
        );
    }

    #[test]
    fn a_busy_session_is_stopped_before_anything_else() {
        use SessionHandoffStep::*;
        assert_eq!(
            run(BUSY_CLOSING, None),
            vec![StopTurn, ExportTranscript, StartSession, CloseSource]
        );
    }

    #[test]
    fn a_failed_stop_hands_nothing_over_and_says_why() {
        use SessionHandoffStep::*;
        assert_eq!(
            run(BUSY_CLOSING, Some(StopTurn)),
            vec![StopTurn, Failed("no stop".to_string())]
        );
    }

    #[test]
    fn a_failure_at_any_step_leaves_the_source_untouched() {
        use SessionHandoffStep::*;
        for (failing, reason) in [(ExportTranscript, "no file"), (StartSession, "no session")] {
            let steps = run(IDLE_CLOSING, Some(failing));
            assert_eq!(steps.last(), Some(&Failed(reason.to_string())));
            assert!(!steps.contains(&CloseSource), "{steps:?}");
        }
    }

    #[test]
    fn the_draft_holds_the_lead_in_then_the_link_with_the_caret_two_lines_below() {
        let draft = handoff_draft("/tmp/a.md", "Landing page ideas", "Scratch");
        assert_eq!(
            draft,
            "You are continuing from \"Landing page ideas\" in Scratch.\n\n\
             [Landing page ideas Handoff](/tmp/a.md)\n\n"
        );
        // The text after the link is two line breaks and nothing else: the caret, at the end of
        // the text, sits two lines below the link.
        let after_link = draft.rsplit_once(')').expect("a link").1;
        assert_eq!(after_link, "\n\n");
    }

    #[test]
    fn the_lead_in_never_names_what_it_does_not_know() {
        assert_eq!(
            handoff_lead_in("  A\n title ", ""),
            "You are continuing from \"A title\"."
        );
        assert_eq!(
            handoff_lead_in("", "Scratch"),
            "You are continuing from a session in Scratch."
        );
        assert_eq!(
            handoff_lead_in(" ", " "),
            "You are continuing from another session."
        );
    }

    #[test]
    fn the_link_stays_a_link_whatever_the_title_and_path_hold() {
        let draft = handoff_draft("/a b/(c).md", "Fix [x]", "P");
        assert!(
            draft.contains("[Fix \\[x\\] Handoff](</a b/(c).md>)"),
            "{draft}"
        );
        let draft = handoff_draft("/a/(c).md", "T", "P");
        assert!(draft.contains("[T Handoff](/a/\\(c\\).md)"), "{draft}");
    }
}
