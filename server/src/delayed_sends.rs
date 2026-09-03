use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{DomainRepository, DomainStateError},
    events::GxserverEventHub,
    paths::GxserverPaths,
    presentation::{
        build_presentation_session_delta, effective_lifecycle_state,
        increment_presentation_revision, presentation_activity,
    },
    session_chat_queue::{SessionChatQueuePublisherFactory, SessionChatQueueSenderFactory},
    storage::open_gxserver_database,
};

const DELAYED_SEND_TICK_SECONDS: u64 = 1;
const DELAYED_SEND_STABILITY_MS: i64 = 10_000;
const DELAYED_SEND_MIN_DELAY_MS: u64 = 60_000;
const DELAYED_SEND_MAX_DELAY_MS: u64 = 2_147_483_647;

#[derive(Clone)]
pub struct DelayedSendRuntime {
    event_hub: GxserverEventHub,
    paths: GxserverPaths,
    presentation_event_sequence: Arc<Mutex<()>>,
    server_id: String,
}

#[derive(Clone)]
struct DelayedSendRecord {
    deadline_at: Option<String>,
    non_working_since_at: Option<String>,
    project_id: String,
    session_id: String,
    trigger: String,
}

impl DelayedSendRuntime {
    pub fn new(
        paths: GxserverPaths,
        server_id: impl Into<String>,
        event_hub: GxserverEventHub,
        presentation_event_sequence: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            event_hub,
            paths,
            presentation_event_sequence,
            server_id: server_id.into(),
        }
    }

    pub fn start(
        &self,
        mut shutdown_rx: broadcast::Receiver<()>,
        chat_sender_factory: SessionChatQueueSenderFactory,
        chat_publisher_factory: SessionChatQueuePublisherFactory,
    ) {
        /*
        CDXC:DelayedSend 2026-08-17:
        The session-hosting daemon owns the clock and activity watcher. On a
        daemon restart, absolute timers keep elapsed wall time while conditional
        triggers restart their ten-second observation window. A row left in
        `firing` is ambiguous after a crash, so expire it rather than risking a
        duplicate Enter.
        */
        let runtime = self.clone();
        tokio::spawn(async move {
            let _ = runtime.recover_after_restart();
            let mut interval =
                tokio::time::interval(Duration::from_secs(DELAYED_SEND_TICK_SECONDS));
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = interval.tick() => {
                        let _ = runtime
                            .run_scheduler_tick(&chat_sender_factory, &chat_publisher_factory);
                    }
                }
            }
        });
    }

    pub fn handle_endpoint(
        &self,
        endpoint_path: &str,
        params: &Map<String, Value>,
    ) -> Result<Value, DomainStateError> {
        match endpoint_path {
            "/api/scheduleDelayedSend" => self.schedule(params),
            "/api/cancelDelayedSend" => self.cancel(params),
            "/api/readDelayedSends" => self.read(params),
            _ => Err(DomainStateError::not_found(format!(
                "{endpoint_path} is not a gxserver delayed-send endpoint."
            ))),
        }
    }

    fn schedule(&self, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        let (project_id, session_id) = require_target(&repository, params)?;
        let delay_ms = params.get("delayMs").and_then(Value::as_u64);
        let when_agent_stops =
            params.get("sendWhenAgentStops").and_then(Value::as_bool) == Some(true);
        let when_all_stop = params
            .get("sendWhenAllProjectSessionsStop")
            .and_then(Value::as_bool)
            == Some(true);
        if usize::from(delay_ms.is_some())
            + usize::from(when_agent_stops)
            + usize::from(when_all_stop)
            != 1
        {
            return Err(DomainStateError::bad_request(
                "Choose exactly one Delayed Send trigger.",
            ));
        }
        if let Some(delay_ms) = delay_ms {
            if !(DELAYED_SEND_MIN_DELAY_MS..=DELAYED_SEND_MAX_DELAY_MS).contains(&delay_ms)
                || delay_ms % DELAYED_SEND_MIN_DELAY_MS != 0
            {
                return Err(DomainStateError::bad_request(
                    "Delayed Send delay must be a whole number of minutes between 1 minute and 24 days.",
                ));
            }
        }
        let now = Utc::now();
        let trigger = if delay_ms.is_some() {
            "timer"
        } else if when_agent_stops {
            "agentStops"
        } else {
            "allAgentsStop"
        };
        let deadline_at = delay_ms.map(|delay_ms| {
            (now + chrono::Duration::milliseconds(delay_ms as i64))
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        });
        let timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        db.execute(
            r#"
            INSERT INTO delayed_sends (
              projectId, sessionId, trigger, deadlineAt, nonWorkingSinceAt,
              state, errorMessage, createdAt, updatedAt
            )
            VALUES (?1, ?2, ?3, ?4, NULL, 'armed', NULL, ?5, ?5)
            ON CONFLICT(projectId, sessionId) DO UPDATE SET
              trigger = excluded.trigger,
              deadlineAt = excluded.deadlineAt,
              nonWorkingSinceAt = NULL,
              state = 'armed',
              errorMessage = NULL,
              updatedAt = excluded.updatedAt
            "#,
            params![project_id, session_id, trigger, deadline_at, timestamp],
        )
        .map_err(sql_error)?;
        self.publish_session_change(&db, &project_id, &session_id)?;
        Ok(json!({
            "delayedSend": read_delayed_send_projection(&db, &project_id, &session_id, Utc::now())?,
            "projectId": project_id,
            "sessionId": session_id,
        }))
    }

    fn cancel(&self, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        let (project_id, session_id) = require_target(&repository, params)?;
        let changed = db
            .execute(
                "DELETE FROM delayed_sends WHERE projectId = ?1 AND sessionId = ?2 AND state IN ('armed', 'firing')",
                params![project_id, session_id],
            )
            .map_err(sql_error)?
            > 0;
        if changed {
            self.publish_session_change(&db, &project_id, &session_id)?;
        }
        Ok(json!({
            "changed": changed,
            "projectId": project_id,
            "sessionId": session_id,
        }))
    }

    fn read(&self, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let project_id = params
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let records = read_armed_records(&db)?
            .into_iter()
            .filter(|record| project_id.is_none_or(|project_id| record.project_id == project_id))
            .filter_map(|record| {
                read_delayed_send_projection(
                    &db,
                    &record.project_id,
                    &record.session_id,
                    Utc::now(),
                )
                .transpose()
                .map(|result| {
                    result.map(|projection| {
                        json!({
                            "delayedSend": projection,
                            "projectId": record.project_id,
                            "sessionId": record.session_id,
                        })
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "delayedSends": records }))
    }

    fn recover_after_restart(&self) -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let timestamp = now_iso();
        db.execute(
            r#"
            UPDATE delayed_sends
            SET state = 'expired',
                errorMessage = 'gxserver restarted while this Delayed Send was firing.',
                updatedAt = ?1
            WHERE state = 'firing'
            "#,
            [&timestamp],
        )
        .map_err(sql_error)?;
        db.execute(
            r#"
            UPDATE delayed_sends
            SET nonWorkingSinceAt = NULL,
                updatedAt = ?1
            WHERE state = 'armed'
              AND trigger IN ('agentStops', 'allAgentsStop')
              AND nonWorkingSinceAt IS NOT NULL
            "#,
            [&timestamp],
        )
        .map_err(sql_error)?;
        Ok(())
    }

    fn run_scheduler_tick(
        &self,
        chat_sender_factory: &SessionChatQueueSenderFactory,
        chat_publisher_factory: &SessionChatQueuePublisherFactory,
    ) -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        for record in read_armed_records(&db)? {
            self.refresh_record(&db, record, chat_sender_factory, chat_publisher_factory)?;
        }
        Ok(())
    }

    fn refresh_record(
        &self,
        db: &Connection,
        record: DelayedSendRecord,
        chat_sender_factory: &SessionChatQueueSenderFactory,
        chat_publisher_factory: &SessionChatQueuePublisherFactory,
    ) -> Result<(), DomainStateError> {
        let repository = DomainRepository::new(db, self.server_id.as_str());
        let Some(target) = repository.get_session(&record.project_id, &record.session_id)? else {
            self.expire_record(db, &record, "The target session no longer exists.")?;
            return Ok(());
        };
        let now = Utc::now();
        let due = match record.trigger.as_str() {
            "timer" => record
                .deadline_at
                .as_deref()
                .and_then(parse_timestamp)
                .is_some_and(|deadline| now >= deadline),
            "agentStops" | "allAgentsStop" => {
                // A session counts as working only while it is effectively
                // running: stopped rows keep their last agentActivity payload
                // forever, so a stale 'working' on a dead session must never
                // hold the ten-second stability window open.
                let session_is_working = |session: &Value| {
                    effective_lifecycle_state(session) == "running"
                        && presentation_activity(session, &now_iso()) == "working"
                };
                let working = if record.trigger == "agentStops" {
                    session_is_working(&target)
                } else {
                    repository
                        .list_sessions(Some(&record.project_id))?
                        .iter()
                        .filter(|session| {
                            matches!(
                                session.get("kind").and_then(Value::as_str),
                                Some("terminal" | "agent")
                            )
                        })
                        .any(session_is_working)
                };
                if working {
                    if record.non_working_since_at.is_some() {
                        self.update_non_working_since(db, &record, None)?;
                    }
                    false
                } else if let Some(since) = record
                    .non_working_since_at
                    .as_deref()
                    .and_then(parse_timestamp)
                {
                    let due = now.signed_duration_since(since).num_milliseconds()
                        >= DELAYED_SEND_STABILITY_MS;
                    if !due {
                        // The projected remaining label is computed from `now`,
                        // so the ten-second countdown only moves on clients if
                        // every tick republishes the session while the
                        // stability window is running.
                        self.publish_session_change(db, &record.project_id, &record.session_id)?;
                    }
                    due
                } else {
                    self.update_non_working_since(db, &record, Some(now_iso()))?;
                    false
                }
            }
            _ => false,
        };
        if due {
            self.fire_record(db, &record, chat_sender_factory, chat_publisher_factory)?;
        }
        Ok(())
    }

    fn update_non_working_since(
        &self,
        db: &Connection,
        record: &DelayedSendRecord,
        value: Option<String>,
    ) -> Result<(), DomainStateError> {
        let changed = db
            .execute(
                r#"
                UPDATE delayed_sends
                SET nonWorkingSinceAt = ?3, updatedAt = ?4
                WHERE projectId = ?1 AND sessionId = ?2 AND state = 'armed'
                "#,
                params![record.project_id, record.session_id, value, now_iso()],
            )
            .map_err(sql_error)?
            > 0;
        if changed {
            self.publish_session_change(db, &record.project_id, &record.session_id)?;
        }
        Ok(())
    }

    fn fire_record(
        &self,
        db: &Connection,
        record: &DelayedSendRecord,
        chat_sender_factory: &SessionChatQueueSenderFactory,
        chat_publisher_factory: &SessionChatQueuePublisherFactory,
    ) -> Result<(), DomainStateError> {
        let claimed = db
            .execute(
                r#"
                UPDATE delayed_sends
                SET state = 'firing', updatedAt = ?3
                WHERE projectId = ?1 AND sessionId = ?2 AND state = 'armed'
                "#,
                params![record.project_id, record.session_id, now_iso()],
            )
            .map_err(sql_error)?
            > 0;
        if !claimed {
            return Ok(());
        }
        self.publish_session_change(db, &record.project_id, &record.session_id)?;
        let repository = DomainRepository::new(db, self.server_id.as_str());
        let Some(session) = repository.get_session(&record.project_id, &record.session_id)? else {
            return self.finish_record(
                db,
                record,
                "failed",
                Some("The target session no longer exists."),
            );
        };
        /*
        CDXC:DelayedSend 2026-08-24:
        Chat clients hand the terminal composer's text off into the synced chat
        draft, so by the time a Delayed Send fires the message the user staged
        often no longer sits on the input line — a bare Enter would submit an
        empty prompt and silently drop it. When a non-empty draft exists, the
        fire delivers it through the full internal chat send (paste + submit +
        delivery watchdog) and clears the draft on success; only a draftless
        session gets the historical bare Enter into whatever is on its input
        line.
        */
        let draft_text = crate::session_chat_queue::armed_delayed_send_draft_text(
            db,
            &record.project_id,
            &record.session_id,
        )?;
        if let Some(text) = draft_text {
            let sender = chat_sender_factory(&record.project_id, &record.session_id);
            let publisher = chat_publisher_factory(&record.project_id, &record.session_id);
            let runtime = self.clone();
            let record = record.clone();
            tokio::spawn(async move {
                let outcome = sender(text).await;
                let Ok(db) = open_gxserver_database(&runtime.paths) else {
                    return;
                };
                let _ = match outcome {
                    Ok(()) => {
                        let _ = crate::session_chat_queue::clear_session_chat_draft_after_delivery(
                            &db,
                            &record.project_id,
                            &record.session_id,
                        );
                        publisher();
                        runtime.finish_record(&db, &record, "completed", None)
                    }
                    Err(message) => {
                        runtime.finish_record(&db, &record, "failed", Some(message.as_str()))
                    }
                };
            });
            return Ok(());
        }
        let zmx_name = match crate::zmx::provider_zmx_session_name(&session) {
            Ok(zmx_name) => zmx_name,
            Err(error) => {
                return self.finish_record(db, record, "failed", Some(error.message.as_str()));
            }
        };
        /*
        CDXC:SessionChat 2026-08-24:
        A Delayed Send is a bare `\r` into a session's input line, and its idle
        condition ("the agent stopped working") says nothing about whether a
        chat send is mid-sequence — the send queue can be paced across seconds
        while the agent looks idle. So the Enter rides that queue instead of
        being written straight to the pty, and the row is settled from the
        queue's answer rather than from a subprocess exit code. Waiting our turn
        only ever delays the Enter, which the trigger already tolerates.
        */
        let runtime = self.clone();
        let record = record.clone();
        tokio::spawn(async move {
            let outcome = crate::session_chat_send::execute_session_chat_send(
                &record.project_id,
                &record.session_id,
                &zmx_name,
                "delayed-send-submit",
                vec![crate::session_chat_send::SessionChatSendStep::Write(
                    crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
                )],
            )
            .await;
            let Ok(db) = open_gxserver_database(&runtime.paths) else {
                return;
            };
            let _ = match outcome {
                Ok(()) => runtime.finish_record(&db, &record, "completed", None),
                Err(error) => {
                    runtime.finish_record(&db, &record, "failed", Some(error.message.as_str()))
                }
            };
        });
        Ok(())
    }

    fn finish_record(
        &self,
        db: &Connection,
        record: &DelayedSendRecord,
        state: &str,
        error_message: Option<&str>,
    ) -> Result<(), DomainStateError> {
        db.execute(
            r#"
            UPDATE delayed_sends
            SET state = ?3, errorMessage = ?4, updatedAt = ?5
            WHERE projectId = ?1 AND sessionId = ?2 AND state = 'firing'
            "#,
            params![
                record.project_id,
                record.session_id,
                state,
                error_message.map(|message| message.chars().take(300).collect::<String>()),
                now_iso(),
            ],
        )
        .map_err(sql_error)?;
        Ok(())
    }

    fn expire_record(
        &self,
        db: &Connection,
        record: &DelayedSendRecord,
        reason: &str,
    ) -> Result<(), DomainStateError> {
        db.execute(
            r#"
            UPDATE delayed_sends
            SET state = 'expired', errorMessage = ?3, updatedAt = ?4
            WHERE projectId = ?1 AND sessionId = ?2 AND state = 'armed'
            "#,
            params![record.project_id, record.session_id, reason, now_iso()],
        )
        .map_err(sql_error)?;
        Ok(())
    }

    fn publish_session_change(
        &self,
        db: &Connection,
        project_id: &str,
        session_id: &str,
    ) -> Result<(), DomainStateError> {
        let _sequence = self.presentation_event_sequence.lock().map_err(|_| {
            DomainStateError::corrupt_state("Presentation event sequencer is poisoned.")
        })?;
        let repository = DomainRepository::new(db, self.server_id.as_str());
        let delta = build_presentation_session_delta(db, &repository, project_id, session_id)?;
        let revision = increment_presentation_revision(db)?;
        self.event_hub.broadcast(json!({
            "delta": delta,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "revision": revision,
            "serverId": self.server_id,
            "type": "presentationDelta",
        }));
        Ok(())
    }
}

pub fn insert_delayed_send_presentation_payload(
    db: &Connection,
    snapshot: &mut Value,
) -> Result<(), DomainStateError> {
    let projections = read_all_delayed_send_projections(db, Utc::now())?;
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for session in sessions {
        let Some(object) = session.as_object_mut() else {
            continue;
        };
        let key = (
            object
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            object
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        if let Some(projection) = projections.get(&key).and_then(Value::as_object) {
            merge_projection(object, projection);
        }
    }
    Ok(())
}

pub fn insert_delayed_send_session_projection(
    db: &Connection,
    session: &mut Value,
) -> Result<(), DomainStateError> {
    let Some(object) = session.as_object_mut() else {
        return Ok(());
    };
    let project_id = object
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let session_id = object
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(projection) =
        read_delayed_send_projection(db, &project_id, &session_id, Utc::now())?
            .and_then(|value| value.as_object().cloned())
    {
        merge_projection(object, &projection);
    }
    Ok(())
}

fn merge_projection(session: &mut Map<String, Value>, projection: &Map<String, Value>) {
    for key in [
        "delayedSendDeadlineAt",
        "delayedSendRemainingLabel",
        "delayedSendRemainingMs",
        "sendWhenAllProjectSessionsStopActive",
        "sendWhenAgentStopsActive",
    ] {
        if let Some(value) = projection.get(key) {
            session.insert(key.to_string(), value.clone());
        }
    }
}

fn read_all_delayed_send_projections(
    db: &Connection,
    now: DateTime<Utc>,
) -> Result<HashMap<(String, String), Value>, DomainStateError> {
    read_armed_records(db)?
        .into_iter()
        .map(|record| {
            let key = (record.project_id.clone(), record.session_id.clone());
            Ok((key, record_projection(&record, now)))
        })
        .collect()
}

fn read_delayed_send_projection(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<Value>, DomainStateError> {
    let record = db
        .query_row(
            r#"
            SELECT projectId, sessionId, trigger, deadlineAt, nonWorkingSinceAt
            FROM delayed_sends
            WHERE projectId = ?1 AND sessionId = ?2 AND state = 'armed'
            "#,
            params![project_id, session_id],
            record_from_row,
        )
        .optional()
        .map_err(sql_error)?;
    Ok(record.map(|record| record_projection(&record, now)))
}

fn record_projection(record: &DelayedSendRecord, now: DateTime<Utc>) -> Value {
    let mut output = Map::new();
    let effective_deadline = if record.trigger == "timer" {
        record.deadline_at.clone()
    } else {
        record
            .non_working_since_at
            .as_deref()
            .and_then(parse_timestamp)
            .map(|since| {
                (since + chrono::Duration::milliseconds(DELAYED_SEND_STABILITY_MS))
                    .to_rfc3339_opts(SecondsFormat::Millis, true)
            })
    };
    if let Some(deadline_at) = effective_deadline {
        let remaining_ms = parse_timestamp(&deadline_at)
            .map(|deadline| {
                deadline
                    .signed_duration_since(now)
                    .num_milliseconds()
                    .max(0) as u64
            })
            .unwrap_or(0);
        output.insert(
            "delayedSendDeadlineAt".to_string(),
            Value::String(deadline_at),
        );
        output.insert("delayedSendRemainingMs".to_string(), json!(remaining_ms));
        output.insert(
            "delayedSendRemainingLabel".to_string(),
            Value::String(countdown_label(remaining_ms)),
        );
    } else {
        output.insert(
            "delayedSendRemainingLabel".to_string(),
            Value::String(if record.trigger == "agentStops" {
                "Waiting for agent".to_string()
            } else {
                "Waiting for agents".to_string()
            }),
        );
    }
    match record.trigger.as_str() {
        "agentStops" => {
            output.insert("sendWhenAgentStopsActive".to_string(), Value::Bool(true));
        }
        "allAgentsStop" => {
            output.insert(
                "sendWhenAllProjectSessionsStopActive".to_string(),
                Value::Bool(true),
            );
        }
        _ => {}
    }
    Value::Object(output)
}

fn read_armed_records(db: &Connection) -> Result<Vec<DelayedSendRecord>, DomainStateError> {
    let mut statement = db
        .prepare(
            r#"
            SELECT projectId, sessionId, trigger, deadlineAt, nonWorkingSinceAt
            FROM delayed_sends
            WHERE state = 'armed'
            ORDER BY updatedAt, projectId, sessionId
            "#,
        )
        .map_err(sql_error)?;
    let records = statement
        .query_map([], record_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(records)
}

fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DelayedSendRecord> {
    Ok(DelayedSendRecord {
        project_id: row.get(0)?,
        session_id: row.get(1)?,
        trigger: row.get(2)?,
        deadline_at: row.get(3)?,
        non_working_since_at: row.get(4)?,
    })
}

fn require_target(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<(String, String), DomainStateError> {
    let project_id = required_text(params, "projectId")?;
    let session_id = required_text(params, "sessionId")?;
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::not_found("Delayed Send target session was not found."))?;
    if !matches!(
        session.get("kind").and_then(Value::as_str),
        Some("terminal" | "agent")
    ) {
        return Err(DomainStateError::bad_request(
            "Delayed Send requires a terminal or agent session.",
        ));
    }
    Ok((project_id, session_id))
}

fn required_text(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}

fn countdown_label(remaining_ms: u64) -> String {
    let total_seconds = remaining_ms.saturating_add(999) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn internal_error(error: anyhow::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite delayed-send state error: {error}"),
    }
}
