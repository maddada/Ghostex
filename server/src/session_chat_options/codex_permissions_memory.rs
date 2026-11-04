use super::*;

/// The newest permission mode read off each Codex session's screen, and when it was read.
static SEEN: std::sync::OnceLock<
    Mutex<HashMap<String, (SessionChatDetectedChoice, chrono::DateTime<chrono::Utc>)>>,
> = std::sync::OnceLock::new();

fn parse_time(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&chrono::Utc))
}

/*
CDXC:AgentScreenDetection 2026-10-27 WHY:
Codex prints `• Permission selection requested: <name>` once, and the detector reads only the visible screen, so the line is gone as soon as output scrolls it away or `/clear` wipes the terminal (the mode stays the same after `/clear`). Falling back to the transcript then showed the mode of the last turn, and in a session with no turn yet the launch flags, so a chat opened after `/clear` called a session switched to Read Only Full Access because it was started with `--yolo`. The screen reading is kept with the time it was read and stands until the transcript records a turn after it, whose `turn_context` names the mode that turn ran with. It lives only as long as this gxserver; a Codex restarted in the same session keeps it until its first turn or its own `--yolo` header replaces it.
SEE-ALSO: `codex_permissions_on_screen` in agent_matchers.rs.
*/
pub(crate) fn with_remembered_codex_permissions(
    project_id: &str,
    session_id: &str,
    terminal: Option<SessionChatDetectedSelection>,
    transcript: Option<&SessionChatDetectedSelection>,
) -> Option<SessionChatDetectedSelection> {
    remember_at(
        &format!("{project_id}:{session_id}"),
        terminal,
        transcript,
        chrono::Utc::now(),
    )
}

fn remember_at(
    key: &str,
    terminal: Option<SessionChatDetectedSelection>,
    transcript: Option<&SessionChatDetectedSelection>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<SessionChatDetectedSelection> {
    let Ok(mut seen) = SEEN.get_or_init(Mutex::default).lock() else {
        return terminal;
    };
    if let Some(choice) = terminal
        .as_ref()
        .and_then(|terminal| terminal.permissions.clone())
    {
        seen.insert(key.to_string(), (choice, now));
        return terminal;
    }
    let Some((choice, seen_at)) = seen.get(key).cloned() else {
        return terminal;
    };
    let recorded_later = transcript
        .and_then(|transcript| transcript.permissions_recorded_at.as_deref())
        .and_then(parse_time)
        .is_some_and(|recorded| recorded > seen_at);
    if recorded_later {
        seen.remove(key);
        return terminal;
    }
    let mut terminal = terminal.unwrap_or_default();
    terminal.permissions = Some(choice);
    Some(terminal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(value: &str, source: SessionChatOptionEvidence) -> SessionChatDetectedSelection {
        SessionChatDetectedSelection {
            permissions: Some(SessionChatDetectedChoice {
                value: value.to_string(),
                label: value.to_string(),
                source,
            }),
            ..SessionChatDetectedSelection::default()
        }
    }

    #[test]
    fn a_screen_reading_stands_until_a_later_turn_records_the_mode() {
        let key = "test:codex-permissions-memory";
        let read_at = parse_time("2026-10-27T10:00:00Z").unwrap();
        let later = parse_time("2026-10-27T10:05:00Z").unwrap();
        let terminal = reading("read-only", SessionChatOptionEvidence::Terminal);
        assert_eq!(
            remember_at(key, Some(terminal.clone()), None, read_at),
            Some(terminal)
        );
        // The line scrolled away (or /clear wiped it); the older transcript does not win.
        let mut older_turn = reading("full-access", SessionChatOptionEvidence::Transcript);
        older_turn.permissions_recorded_at = Some("2026-10-27T09:00:00.000Z".to_string());
        let kept = remember_at(key, None, Some(&older_turn), later).unwrap();
        assert_eq!(kept.permissions.unwrap().value, "read-only");
        // A turn recorded after the reading names the mode that turn ran with.
        let mut newer_turn = reading("approve-for-me", SessionChatOptionEvidence::Transcript);
        newer_turn.permissions_recorded_at = Some("2026-10-27T10:01:00.000Z".to_string());
        assert_eq!(remember_at(key, None, Some(&newer_turn), later), None);
        assert_eq!(remember_at(key, None, None, later), None);
    }
}
