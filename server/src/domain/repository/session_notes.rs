use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::{
    now_iso, optional_trimmed_string_param, read_unvalidated_project_lookup_id,
    read_unvalidated_session_lookup_id, sql_error, DomainRepository, DomainResult,
    DomainStateError,
};
use crate::presentation::read_runtime_text;

/*
Session notes ride every presentation snapshot, so their persisted size must be
bounded at the write boundary. Count Unicode scalar values rather than UTF-8
bytes so the user-facing contract is stable across scripts and emoji.
*/
const SESSION_AGENT_NOTE_MAX_CHARS: usize = 4_096;

/*
CDXC:SessionNotes 2026-08-24:
"What to do next when I come back" text attached to a conversation, not to a
ghostex session row. The key is the provider resume id (`agentSessionId`), so
closing the session and resuming the same agent conversation later still finds
the note — and so the same note is the same on every surface and every client.

The note body is user-authored prose: it is returned only through the
authenticated RPC response and must never be logged or echoed.
*/
impl DomainRepository<'_> {
    pub fn save_session_agent_note(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        let session_id = read_unvalidated_session_lookup_id(params);
        let session = self.require_session_for_agent_note(&project_id, &session_id)?;
        let agent_session_id = require_session_agent_session_id(&session)?;
        /*
        The result carries the CANONICAL session-row ids (not the raw request
        params) so the endpoint can schedule its presentation delta for the
        session that actually stored the note.
        */
        let canonical_project_id =
            read_session_text(&session, "projectId").unwrap_or_else(|| project_id.clone());
        let canonical_session_id =
            read_session_text(&session, "sessionId").unwrap_or_else(|| session_id.clone());
        let note = optional_trimmed_string_param(params, "note")?;
        if note
            .as_ref()
            .is_some_and(|note| note.chars().count() > SESSION_AGENT_NOTE_MAX_CHARS)
        {
            return Err(DomainStateError::bad_request(format!(
                "note must be at most {SESSION_AGENT_NOTE_MAX_CHARS} characters."
            )));
        }
        let Some(note) = note else {
            /*
            An emptied note is a deletion, not an empty row: the table's CHECK
            forbids `note = ''`, and presentation publishes the key as ABSENT
            when there is no note.
            */
            self.db
                .execute(
                    "DELETE FROM session_agent_notes WHERE agentSessionId = ?1",
                    params![agent_session_id],
                )
                .map_err(sql_error)?;
            return Ok(json!({
                "agentSessionId": agent_session_id,
                "note": "",
                "projectId": canonical_project_id,
                "sessionId": canonical_session_id,
            }));
        };
        let agent = read_runtime_text(&session, "agentName")
            .or_else(|| read_session_text(&session, "agentId"));
        let timestamp = now_iso();
        self.db
            .execute(
                r#"
                INSERT INTO session_agent_notes (
                  agentSessionId, note, agent, projectId, sessionId, createdAt, updatedAt
                ) VALUES (
                  ?1, ?2, ?3, ?4, ?5, ?6, ?6
                )
                ON CONFLICT(agentSessionId) DO UPDATE SET
                  note = excluded.note,
                  agent = excluded.agent,
                  projectId = excluded.projectId,
                  sessionId = excluded.sessionId,
                  updatedAt = excluded.updatedAt
                "#,
                params![
                    agent_session_id,
                    note,
                    agent,
                    project_id,
                    session_id,
                    timestamp
                ],
            )
            .map_err(sql_error)?;
        Ok(json!({
            "agentSessionId": agent_session_id,
            "note": note,
            "projectId": canonical_project_id,
            "sessionId": canonical_session_id,
        }))
    }

    pub fn read_session_agent_note(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        let session_id = read_unvalidated_session_lookup_id(params);
        let session = self.require_session_for_agent_note(&project_id, &session_id)?;
        /*
        A session that has not yet proven a provider conversation simply has no
        note to read. That is a normal state (a terminal that has not started
        its agent), not a bad request, so both keys are absent.
        */
        let Some(agent_session_id) = read_runtime_text(&session, "agentSessionId") else {
            return Ok(json!({}));
        };
        let note = read_session_agent_note_text(self.db, &agent_session_id)?;
        let mut result = Map::new();
        result.insert("agentSessionId".to_string(), json!(agent_session_id));
        if let Some(note) = note {
            result.insert("note".to_string(), json!(note));
        }
        Ok(Value::Object(result))
    }

    /*
    CDXC:SessionNotes 2026-08-24 (successor re-key):
    Claude Code and Codex mint a NEW conversation id on compaction/resume, and
    the daemon adopts that successor as the session's `agentSessionId`. Without
    this re-key the note would stay stranded on the dead id and the session
    would look like it never had one. If the successor id ALREADY carries a
    note, that one wins and nothing is deleted — the note the user wrote most
    recently against the live conversation is the truth.
    */
    pub(crate) fn rekey_session_agent_note(
        &self,
        previous_agent_session_id: &str,
        agent_session_id: &str,
    ) -> DomainResult<bool> {
        if previous_agent_session_id.is_empty()
            || agent_session_id.is_empty()
            || previous_agent_session_id == agent_session_id
        {
            return Ok(false);
        }
        let updated = self
            .db
            .execute(
                r#"
                UPDATE session_agent_notes
                SET agentSessionId = ?2, updatedAt = ?3
                WHERE agentSessionId = ?1
                  AND NOT EXISTS (
                    SELECT 1 FROM session_agent_notes WHERE agentSessionId = ?2
                  )
                "#,
                params![previous_agent_session_id, agent_session_id, now_iso()],
            )
            .map_err(sql_error)?;
        Ok(updated > 0)
    }

    fn require_session_for_agent_note(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> DomainResult<Value> {
        self.get_session(project_id, session_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })
    }
}

fn require_session_agent_session_id(session: &Value) -> DomainResult<String> {
    read_runtime_text(session, "agentSessionId")
        .ok_or_else(|| DomainStateError::bad_request("session has no agent session id"))
}

fn read_session_text(session: &Value, key: &str) -> Option<String> {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_session_agent_note_text(
    db: &rusqlite::Connection,
    agent_session_id: &str,
) -> DomainResult<Option<String>> {
    if agent_session_id.is_empty() {
        return Ok(None);
    }
    let note = db
        .query_row(
            "SELECT note FROM session_agent_notes WHERE agentSessionId = ?1",
            params![agent_session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(note
        .map(|note| note.trim().to_string())
        .filter(|note| !note.is_empty()))
}
