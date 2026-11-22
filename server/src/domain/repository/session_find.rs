//! The sessions a CLI selector can name by an id, without listing every session.
//!
//! CDXC:Cli 2026-11-10 WHY:
//! A session action sent with a bare `--session-id`, and `agents send` or `coordinator reopen` of a
//! closed session, listed every session with `includeStopped` to resolve one id: 10,034 rows and
//! 16 MB of text on the profiled Mac, about 240 MB of transient memory in gxserver and over a
//! second per call, the likely cause of its 466 MB peaks. `/api/findSessions` returns only the rows
//! the CLI's id rules could match (ghostex_cli/selector.rs `id_rule_matches`): the session id, the
//! global ref, the Routing ID (`project:session`), the agent session id and the zmx name. Each
//! rule here is a superset of the CLI's, so the CLI's own rules decide among the rows exactly as
//! they would over the whole list. Titles and aliases need the whole list and are not served here.
//! SEE-ALSO: server/src/ghostex_cli/sessions/inventory.rs (`fetch_selector_candidates`).

use std::collections::HashSet;

use rusqlite::params_from_iter;
use serde_json::Value;

use crate::domain::{
    session_from_row, session_row_from_sql, sql_error, DomainRepository, DomainResult,
};

/// What `/api/findSessions` found: the candidate rows in `list_sessions`' order, and whether they
/// cover every id rule (false when the selector holds a character the agent session id search
/// cannot look for in the stored JSON text, so the caller lists every session instead).
pub struct FoundSessions {
    pub sessions: Vec<Value>,
    pub complete: bool,
}

impl DomainRepository<'_> {
    /// `extra_zmx_names` adds the sessions with those zmx names (the surfaces Codex's hook last
    /// reported under the selector as its agent session id, which the live identity repair has not
    /// copied into the row yet).
    pub fn find_session_candidates(
        &self,
        selector: &str,
        extra_zmx_names: &[String],
    ) -> DomainResult<FoundSessions> {
        let trimmed = selector.trim();
        // The CLI reads a pasted Copy Details Session ID without its prefix; both forms are served.
        let forms: Vec<&str> = [Some(trimmed), trimmed.strip_prefix("combined-session:")]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|form| !form.is_empty())
            .collect();
        let mut keys: Vec<(String, String)> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut add = |keys: &mut Vec<(String, String)>, project_id: String, session_id: String| {
            if seen.insert((project_id.clone(), session_id.clone())) {
                keys.push((project_id, session_id));
            }
        };
        let mut complete = true;
        for form in &forms {
            // The session id (idx_sessions_session_id).
            for key in self.session_keys_where("sessionId = ?1", &[form])? {
                add(&mut keys, key.0, key.1);
            }
            // A Routing ID `project:session`, and a global ref `server:project:session`, split at
            // every colon so any split the CLI makes is covered (primary key lookups).
            for (project_id, session_id) in splits(form, ':') {
                if self.session_exists(project_id, session_id)? {
                    add(&mut keys, project_id.to_string(), session_id.to_string());
                }
            }
            if let Some(rest) = form
                .strip_prefix(self.server_id.as_str())
                .and_then(|rest| rest.strip_prefix(':'))
            {
                for (project_id, session_id) in splits(rest, ':') {
                    if self.session_exists(project_id, session_id)? {
                        add(&mut keys, project_id.to_string(), session_id.to_string());
                    }
                }
            }
            // The zmx name `server-project-session` gxserver gives every session
            // (`create_zmx_session_name`), bare or as `zmx/<name>`.
            let names = [
                Some(*form),
                form.split_once('/').map(|(_, name)| name.trim()),
            ];
            for name in names.into_iter().flatten() {
                for (project_id, session_id) in self.keys_for_zmx_name(name)? {
                    add(&mut keys, project_id, session_id);
                }
            }
            // The agent session id, which lives in the runtime settings JSON: the rows whose text
            // holds it, kept when the parsed value matches. A character JSON escapes would not be
            // found in the text, so such a selector is answered as incomplete.
            if form
                .chars()
                .any(|character| character == '"' || character == '\\' || character.is_control())
            {
                complete = false;
                continue;
            }
            let mut statement = self
                .db
                .prepare_cached(
                    "SELECT projectId, sessionId, runtimeSettingsJson FROM sessions WHERE instr(runtimeSettingsJson, ?1) > 0",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([form], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(sql_error)?;
            for row in rows {
                let (project_id, session_id, runtime_settings) = row.map_err(sql_error)?;
                let agent_session_id = serde_json::from_str::<Value>(&runtime_settings)
                    .ok()
                    .and_then(|settings| {
                        settings
                            .get("agentSessionId")
                            .and_then(Value::as_str)
                            .map(|value| value.trim() == *form)
                    });
                if agent_session_id == Some(true) {
                    add(&mut keys, project_id, session_id);
                }
            }
        }
        for name in extra_zmx_names {
            for (project_id, session_id) in self.keys_for_zmx_name(name)? {
                add(&mut keys, project_id, session_id);
            }
        }
        Ok(FoundSessions {
            sessions: self.sessions_in_list_order(&keys)?,
            complete,
        })
    }

    /// The sessions whose zmx name `server-project-session` (`create_zmx_session_name`) is `name`.
    fn keys_for_zmx_name(&self, name: &str) -> DomainResult<Vec<(String, String)>> {
        let Some(rest) = name
            .strip_prefix(self.server_id.as_str())
            .and_then(|rest| rest.strip_prefix('-'))
        else {
            return Ok(Vec::new());
        };
        let mut keys = Vec::new();
        for (project_id, session_id) in splits(rest, '-') {
            if self.session_exists(project_id, session_id)? {
                keys.push((project_id.to_string(), session_id.to_string()));
            }
        }
        Ok(keys)
    }

    fn session_keys_where(
        &self,
        condition: &str,
        values: &[&str],
    ) -> DomainResult<Vec<(String, String)>> {
        let mut statement = self
            .db
            .prepare_cached(&format!(
                "SELECT projectId, sessionId FROM sessions WHERE {condition}"
            ))
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
    }

    fn session_exists(&self, project_id: &str, session_id: &str) -> DomainResult<bool> {
        self.db
            .prepare_cached("SELECT 1 FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
            .and_then(|mut statement| statement.exists([project_id, session_id]))
            .map_err(sql_error)
    }

    /// The rows of `keys`, hydrated and ordered as `list_sessions(None)` orders every row.
    fn sessions_in_list_order(&self, keys: &[(String, String)]) -> DomainResult<Vec<Value>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let pairs = vec!["(?, ?)"; keys.len()].join(", ");
        let mut statement = self
            .db
            .prepare(&format!(
                "SELECT * FROM sessions WHERE (projectId, sessionId) IN (VALUES {pairs}) ORDER BY updatedAt DESC, projectId ASC, sessionId ASC"
            ))
            .map_err(sql_error)?;
        let rows = statement
            .query_map(
                params_from_iter(
                    keys.iter()
                        .flat_map(|(project_id, session_id)| [project_id, session_id]),
                ),
                session_row_from_sql,
            )
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| session_from_row(&self.server_id, row))
            .collect()
    }
}

/// `text` split in two at each `separator`, both halves non-empty.
fn splits(text: &str, separator: char) -> Vec<(&str, &str)> {
    text.match_indices(separator)
        .map(|(index, _)| (&text[..index], &text[index + separator.len_utf8()..]))
        .filter(|(left, right)| !left.is_empty() && !right.is_empty())
        .collect()
}
