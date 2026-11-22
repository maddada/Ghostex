//! Runs of the same tool: calls of one tool that follow each other with no caption, announcement,
//! error, focus or voice row between them are drawn as one row ("Read 21 sessions"), described by the `run`
//! object gxserver adds to their `tool` frames.
//!
//! CDXC:VoiceMode 2026-10-24 WHY: one call that read 21 sessions drew 21 identical "Reading the session" rows above the reply. gxserver groups the calls because it sees every row-making frame in the order clients draw them (live and in the reattach replay) and owns the tool labels, so the desktop, the GPUI web build and the phone only key the row by `run.id` instead of `callId` and show `run.label`; the `label` of each frame and the call log stay per call. See `paragraphs.rs` for the user's decision.

use std::collections::HashMap;

use serde_json::{json, Value};

/// Runs are forgotten after this many, oldest first (a call's finish comes right after its start).
const KEPT_RUNS: usize = 16;

struct Call {
    call_id: String,
    label: String,
    /// `None` while it runs.
    ok: Option<bool>,
}

struct Run {
    id: String,
    name: String,
    calls: Vec<Call>,
}

#[derive(Default)]
pub(super) struct ToolRuns {
    runs: Vec<Run>,
    /// The last row clients drew is this run's row, so the next call of its tool joins it.
    open: bool,
    /// Caption items clients already have a row for: more of their text adds no row.
    captions: std::collections::HashSet<String>,
    run_of_call: HashMap<String, String>,
}

impl ToolRuns {
    /// Notes a frame on its way to the client, and adds `run` to the `tool` frames of a run of two
    /// or more calls.
    pub(super) fn frame(&mut self, frame: &mut Value) {
        let field = |key: &str| {
            frame
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match field("type").as_str() {
            "caption" => {
                if !field("text").trim().is_empty() && self.captions.insert(field("itemId")) {
                    self.open = false;
                }
            }
            // Clients draw a notice line for a changed focus or voice.
            "announcement" | "focus" | "voice" => self.open = false,
            "error" if frame.get("fatal").and_then(Value::as_bool) == Some(false) => {
                self.open = false
            }
            "tool" => {
                let call_id = field("callId");
                let name = field("name");
                if field("phase") == "started" {
                    self.start(call_id.clone(), name, field("label"));
                } else {
                    let ok = frame.get("ok").and_then(Value::as_bool).unwrap_or(true);
                    self.finish(&call_id, ok);
                }
                if let Some(run) = self.run_for(&call_id).filter(|run| run.calls.len() > 1) {
                    frame["run"] = run.to_frame();
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, call_id: String, name: String, label: String) {
        if self.run_of_call.contains_key(&call_id) {
            return;
        }
        let joins = self.open && self.runs.last().is_some_and(|run| run.name == name);
        if !joins {
            self.runs.push(Run {
                id: call_id.clone(),
                name,
                calls: Vec::new(),
            });
            if self.runs.len() > KEPT_RUNS {
                let dropped = self.runs.remove(0);
                for call in dropped.calls {
                    self.run_of_call.remove(&call.call_id);
                }
            }
        }
        if let Some(run) = self.runs.last_mut() {
            self.run_of_call.insert(call_id.clone(), run.id.clone());
            run.calls.push(Call {
                call_id,
                label,
                ok: None,
            });
        }
        self.open = true;
    }

    fn finish(&mut self, call_id: &str, ok: bool) {
        let Some(run_id) = self.run_of_call.get(call_id).cloned() else {
            return;
        };
        if let Some(call) = self
            .runs
            .iter_mut()
            .find(|run| run.id == run_id)
            .and_then(|run| run.calls.iter_mut().find(|call| call.call_id == call_id))
        {
            call.ok = Some(ok);
        }
    }

    fn run_for(&self, call_id: &str) -> Option<&Run> {
        let run_id = self.run_of_call.get(call_id)?;
        self.runs.iter().find(|run| run.id == *run_id)
    }
}

impl Run {
    fn to_frame(&self) -> Value {
        let finished = self.calls.iter().all(|call| call.ok.is_some());
        let failed = self
            .calls
            .iter()
            .filter(|call| call.ok == Some(false))
            .count();
        json!({
            "id": self.id,
            "count": self.calls.len(),
            "label": self.label(finished, failed),
            "finished": finished,
            "ok": failed == 0,
        })
    }

    fn label(&self, finished: bool, failed: usize) -> String {
        let n = self.calls.len();
        let first = self
            .calls
            .first()
            .map(|call| call.label.as_str())
            .unwrap_or_default();
        // The same named session every time ("Reading Voice mode"): say how often, not how many.
        let same_target =
            self.calls.iter().all(|call| call.label == first) && !first.ends_with("the session");
        let sentence = match (self.name.as_str(), finished) {
            (_, _) if same_target => format!("{first} ({n} times)"),
            ("read_session", false) => format!("Reading {n} sessions"),
            ("read_session", true) => format!("Read {n} sessions"),
            ("send_prompt", false) => format!("Sending to {n} sessions"),
            ("send_prompt", true) => format!("Sent to {n} sessions"),
            ("draft_prompt", false) => format!("Drafting in {n} sessions"),
            ("draft_prompt", true) => format!("Drafted in {n} sessions"),
            ("send_draft", false) => format!("Sending {n} drafts"),
            ("send_draft", true) => format!("Sent {n} drafts"),
            ("get_pending_prompt", false) => format!("Checking what {n} sessions ask"),
            ("get_pending_prompt", true) => format!("Checked what {n} sessions ask"),
            ("answer_prompt", false) => format!("Answering {n} sessions"),
            ("answer_prompt", true) => format!("Answered {n} sessions"),
            ("start_agent", false) => format!("Starting {n} agents"),
            ("start_agent", true) => format!("Started {n} agents"),
            ("show_session", false) => format!("Showing {n} sessions"),
            ("show_session", true) => format!("Showed {n} sessions"),
            _ => format!("{first} ({n} times)"),
        };
        if failed > 0 && failed < n {
            format!("{sentence}, {failed} failed")
        } else {
            sentence
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::ToolRuns;

    fn started(runs: &mut ToolRuns, call_id: &str, name: &str, label: &str) -> Value {
        let mut frame = json!({"type": "tool", "phase": "started", "callId": call_id, "name": name, "label": label});
        runs.frame(&mut frame);
        frame
    }

    fn finished(runs: &mut ToolRuns, call_id: &str, name: &str, ok: bool) -> Value {
        let mut frame = json!({"type": "tool", "phase": "finished", "callId": call_id, "name": name, "label": "", "ok": ok});
        runs.frame(&mut frame);
        frame
    }

    #[test]
    fn calls_of_one_tool_in_a_row_are_one_run() {
        let mut runs = ToolRuns::default();
        let first = started(&mut runs, "c1", "read_session", "Reading the session");
        assert!(first.get("run").is_none(), "a single call is its own row");
        finished(&mut runs, "c1", "read_session", true);
        let second = started(&mut runs, "c2", "read_session", "Reading the session");
        assert_eq!(
            second["run"],
            json!({"id": "c1", "count": 2, "label": "Reading 2 sessions", "finished": false, "ok": true})
        );
        for n in 3..=21 {
            started(
                &mut runs,
                &format!("c{n}"),
                "read_session",
                "Reading the session",
            );
        }
        for n in 2..=20 {
            finished(&mut runs, &format!("c{n}"), "read_session", true);
        }
        let last = finished(&mut runs, "c21", "read_session", true);
        assert_eq!(
            last["run"],
            json!({"id": "c1", "count": 21, "label": "Read 21 sessions", "finished": true, "ok": true})
        );
        assert_eq!(last["label"], "", "each frame keeps its own label");
    }

    #[test]
    fn a_row_between_calls_or_another_tool_starts_a_new_run() {
        let mut runs = ToolRuns::default();
        started(&mut runs, "c1", "list_sessions", "Checking your sessions");
        started(&mut runs, "c2", "list_projects", "Checking your projects");
        let third = started(&mut runs, "c3", "list_sessions", "Checking your sessions");
        assert!(third.get("run").is_none(), "another tool came between");

        let mut caption = json!({"type": "caption", "role": "agent", "itemId": "i1", "text": "Found them.", "final": true});
        runs.frame(&mut caption);
        let fourth = started(&mut runs, "c4", "list_sessions", "Checking your sessions");
        assert!(fourth.get("run").is_none(), "a caption row came between");

        // More text of a caption that already has a row adds no row.
        let mut more = json!({"type": "caption", "role": "agent", "itemId": "i1", "text": "Found them. All of them.", "final": true});
        runs.frame(&mut more);
        let fifth = started(&mut runs, "c5", "list_sessions", "Checking your sessions");
        assert_eq!(fifth["run"]["id"], "c4");
        assert_eq!(fifth["run"]["label"], "Checking your sessions (2 times)");

        let mut announcement =
            json!({"type": "announcement", "text": "\"Voice mode\" finished its turn."});
        runs.frame(&mut announcement);
        let sixth = started(&mut runs, "c6", "list_sessions", "Checking your sessions");
        assert!(
            sixth.get("run").is_none(),
            "an announcement row came between"
        );
    }

    #[test]
    fn failures_and_named_sessions_read_right() {
        let mut runs = ToolRuns::default();
        started(&mut runs, "c1", "read_session", "Reading the session");
        started(&mut runs, "c2", "read_session", "Reading the session");
        started(&mut runs, "c3", "read_session", "Reading the session");
        finished(&mut runs, "c1", "read_session", true);
        finished(&mut runs, "c2", "read_session", false);
        let last = finished(&mut runs, "c3", "read_session", true);
        assert_eq!(last["run"]["label"], "Read 3 sessions, 1 failed");
        assert_eq!(last["run"]["ok"], false);

        let mut runs = ToolRuns::default();
        started(&mut runs, "c1", "read_session", "Reading Voice mode");
        let again = started(&mut runs, "c2", "read_session", "Reading Voice mode");
        assert_eq!(again["run"]["label"], "Reading Voice mode (2 times)");
    }
}
