//! What a `caption` or `tool` frame from gxserver makes of the voice panel's rows: the paragraphs a
//! long agent reply is drawn in, and the row a tool call lands on (its run's, when gxserver groups
//! calls of one tool). gxserver decides both (`server/src/voice_agent/paragraphs.rs`,
//! `tool_runs.rs`, where the user's decision is); the panel only follows.
//!
//! CDXC:VoiceMode 2026-10-24 SEE-ALSO: the phone's `src/voice/lines.ts` reads the same frames the same way, with the same tests.

use serde_json::Value;

/// gxserver's `paragraphs` of an agent caption; empty when the caption is one paragraph (a short
/// reply, a user caption, or a gxserver from before paragraphs).
pub(crate) fn caption_paragraphs(event: &Value) -> Vec<String> {
    event["paragraphs"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The row a `tool` frame updates, and what it shows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ToolRowFrame {
    /// The run's id when the call is part of a run (the run's first call), else the call's id.
    pub(crate) key: String,
    pub(crate) label: String,
    /// `None` while it (or any call of its run) runs.
    pub(crate) ok: Option<bool>,
    pub(crate) started: bool,
}

pub(crate) fn tool_row_frame(event: &Value) -> ToolRowFrame {
    let started = event["phase"].as_str() == Some("started");
    let run = &event["run"];
    if let Some(id) = run["id"].as_str() {
        return ToolRowFrame {
            key: id.to_string(),
            label: run["label"].as_str().unwrap_or_default().to_string(),
            ok: (run["finished"].as_bool() == Some(true))
                .then(|| run["ok"].as_bool().unwrap_or(true)),
            started,
        };
    }
    ToolRowFrame {
        key: event["callId"].as_str().unwrap_or_default().to_string(),
        label: event["label"].as_str().unwrap_or_default().to_string(),
        ok: (!started).then(|| event["ok"].as_bool().unwrap_or(true)),
        started,
    }
}

/// A `confirm` frame: the card gxserver puts up when the voice agent is unsure which session the
/// user wants stopped (`server/src/voice_agent/control.rs`). The panel draws it as a row with one
/// Stop button per session and a Cancel button, until a `confirmDone` frame with its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfirmCard {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) choices: Vec<ConfirmChoice>,
    pub(crate) confirm_label: String,
    pub(crate) cancel_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfirmChoice {
    /// The session's ref, sent back when its button is pressed.
    pub(crate) session: String,
    pub(crate) title: String,
    /// Its project and state ("Ghostex · working").
    pub(crate) detail: String,
}

pub(crate) fn confirm_card(event: &Value) -> Option<ConfirmCard> {
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_string();
    let choices: Vec<ConfirmChoice> = event["choices"]
        .as_array()?
        .iter()
        .filter(|choice| choice["session"].as_str().is_some())
        .map(|choice| ConfirmChoice {
            session: text(choice, "session"),
            title: text(choice, "title"),
            detail: text(choice, "detail"),
        })
        .collect();
    let id = event["id"].as_str()?.to_string();
    if choices.is_empty() {
        return None;
    }
    let label = |key: &str, fallback: &str| {
        event[key]
            .as_str()
            .filter(|label| !label.is_empty())
            .unwrap_or(fallback)
            .to_string()
    };
    Some(ConfirmCard {
        id,
        title: text(event, "title"),
        choices,
        confirm_label: label("confirmLabel", "Stop"),
        cancel_label: label("cancelLabel", "Cancel"),
    })
}

/// What pressing a card's button sends gxserver: the session for Stop, `null` for Cancel.
pub(crate) fn confirm_answer(id: &str, session: Option<&str>) -> Value {
    serde_json::json!({"type": "confirm", "id": id, "session": session})
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ConfirmChoice, ToolRowFrame, caption_paragraphs, confirm_answer, confirm_card,
        tool_row_frame,
    };

    #[test]
    fn a_long_caption_brings_its_paragraphs_and_a_short_one_none() {
        let long = json!({"type": "caption", "role": "agent", "itemId": "i1", "text": "One. Two.", "final": true, "paragraphs": ["One.", "Two."]});
        assert_eq!(caption_paragraphs(&long), vec!["One.", "Two."]);
        let short = json!({"type": "caption", "role": "agent", "itemId": "i2", "text": "Done.", "final": true});
        assert!(caption_paragraphs(&short).is_empty());
    }

    #[test]
    fn a_single_call_is_its_own_row() {
        let started = json!({"type": "tool", "phase": "started", "callId": "c1", "name": "read_session", "label": "Reading the session"});
        assert_eq!(
            tool_row_frame(&started),
            ToolRowFrame {
                key: "c1".into(),
                label: "Reading the session".into(),
                ok: None,
                started: true
            }
        );
        let finished = json!({"type": "tool", "phase": "finished", "callId": "c1", "name": "read_session", "label": "Reading the session", "ok": false});
        assert_eq!(tool_row_frame(&finished).ok, Some(false));
    }

    #[test]
    fn a_call_of_a_run_lands_on_the_runs_row() {
        let second = json!({"type": "tool", "phase": "started", "callId": "c2", "name": "read_session", "label": "Reading the session",
            "run": {"id": "c1", "count": 2, "label": "Reading 2 sessions", "finished": false, "ok": true}});
        assert_eq!(
            tool_row_frame(&second),
            ToolRowFrame {
                key: "c1".into(),
                label: "Reading 2 sessions".into(),
                ok: None,
                started: true
            }
        );
        let done = json!({"type": "tool", "phase": "finished", "callId": "c21", "name": "read_session", "label": "Reading the session", "ok": true,
            "run": {"id": "c1", "count": 21, "label": "Read 21 sessions", "finished": true, "ok": true}});
        assert_eq!(
            tool_row_frame(&done),
            ToolRowFrame {
                key: "c1".into(),
                label: "Read 21 sessions".into(),
                ok: Some(true),
                started: false
            }
        );
    }

    #[test]
    fn a_confirm_frame_is_a_card_and_its_buttons_answer_it() {
        let frame = json!({"type": "confirm", "id": "c7", "action": "stopSession", "title": "Stop Discord triage?",
            "choices": [{"session": "P1:S1", "title": "Discord triage", "detail": "Ghostex · working"}],
            "confirmLabel": "Stop", "cancelLabel": "Cancel"});
        let card = confirm_card(&frame).expect("a card");
        assert_eq!(card.id, "c7");
        assert_eq!(card.title, "Stop Discord triage?");
        assert_eq!(
            card.choices,
            vec![ConfirmChoice {
                session: "P1:S1".into(),
                title: "Discord triage".into(),
                detail: "Ghostex · working".into()
            }]
        );
        assert_eq!(
            confirm_answer("c7", Some("P1:S1")),
            json!({"type": "confirm", "id": "c7", "session": "P1:S1"})
        );
        assert_eq!(
            confirm_answer("c7", None),
            json!({"type": "confirm", "id": "c7", "session": null})
        );
        assert!(confirm_card(&json!({"type": "confirm", "id": "c8", "choices": []})).is_none());
    }
}
