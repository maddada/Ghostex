//! A coordinator's Threads panel, the Threads pill's menu: one list of its threads, the
//! working and the recently active ones, with the rest a click away.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! The panel shows a coordinator's threads without opening the sidebar, which the phone does not have. Order, labels and the fold are decided here once for the desktop, web and phone renderers; gxserver only sends each thread's state, activity time and one line of detail (`coordinatorThreads`).
//!
//! CDXC:Coordinators 2026-10-18 DECISION:
//! The user: "please show only working here, and "Show recent" to show more recent ones, then "199 more" after that as 3rd step (should load just 30 more and show "169 more")". So the panel opens in three steps: first only the threads in flight (working, plus any waiting on an answer or an approval, since those need the user), or one quiet "No threads working" line; then "Show recent" adds the finished and sleeping threads active in the last 2 hours; then "N more" adds 30 rows per click until none are left, and "Show fewer" goes back to the first step. Closing the menu or opening another chat also goes back to the first step. The header stays "2 working · 212 threads". This supersedes the 2026-10-07 decision that listed the recent threads by default. On 2026-10-15 the panel became the Threads pill's floating menu (extras/chat_pills.rs) and the user retired the remembered fold (`threadsCollapsed`); on 2026-10-17 (the working indicator review, Q256) the list briefly became a section of a working card that opened, and on 2026-10-18 the user brought the menu back: "they should show as menu for each one, let's make the pill itself not expandable at all, instead it should show a menu like we used to have for 'Threads' please" (the pill rides on the working card, extras/working_card.rs). The menu always opens at the first step. Still standing: no Finished / Sleeping / Done groups ("done and finished mean the same thing"); every thread reachable ("Don't actually 'hide' them please"); a thread blocked on something only the user can grant (a permission or folder-trust prompt) is listed right after the working ones with an amber "Needs your approval" tag. The sidebar applies the same two hours (packages/gx-core/src/sidebar_view/threads.rs).
//!
//! CDXC:Coordinators 2026-10-07 WHY:
//! gxserver's frames carry only a summary (the in-flight and recent rows, each `recent` one marked by gxserver, the total and a revision), because the whole list was ~30KB on every state frame. The first "N more" reads the full list once (`readCoordinatorThreads`), keeps it while the list is open, reads it again when a frame brings a new revision, and drops it when the list goes back to the first step. Until the read answers, the summary rows stay listed.
//! SEE-ALSO: server/src/coordinators/panel.rs (the field, the summary and the read), apps/desktop/src/app/native_chat/coordinator_threads.rs and apps/mobile/app/src/chat/native/cards/AgentPanels.tsx (the renderers), packages/gx-core/src/sidebar_view/threads.rs (the sidebar's two-hour rule).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::PanelsState;
use crate::wire::ChatRpcMethod;

/// One thread row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorThreadRow {
    /// `<projectId>:<sessionId>`, stable across frames.
    pub key: String,
    pub project_id: String,
    pub session_id: String,
    pub title: String,
    /// One line: its task while working, the prompt it is blocked on, or its last report.
    pub detail: String,
    pub working: bool,
    /// Blocked on something only the user can grant; drawn with the amber tag.
    pub needs_approval: bool,
    /// A question or another prompt waiting for an answer is on its screen; listed in flight.
    #[serde(skip)]
    pub waiting: bool,
    /// Active in the last two hours, as gxserver judged it; listed without "N more".
    #[serde(skip)]
    pub recent: bool,
    /// The worktree branch, when it has one.
    pub branch: String,
    pub lifecycle_state: String,
    /// The gxserver of the computer the thread runs on, for a thread on another computer than its
    /// coordinator's; empty for one on the coordinator's own computer.
    #[serde(default)]
    pub server_id: String,
    /// That computer's name, which the row shows.
    #[serde(default)]
    pub computer: String,
    /// The first [`ITEMS_SHOWN`] work items the thread is on, as pills on a second line under the
    /// row ([`attach_items`]); empty for a thread on none, which keeps one line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<WorkPill>,
    /// "+2" for the items past those, or "".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub more_items: String,
    /// The "+N" pill's hover: one line per item it stands for.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub more_items_tooltip: String,
    /// The thread's last report (gxserver's `lastReport`), which an item's card quotes.
    #[serde(skip)]
    pub last_report: String,
}

/// A work item a thread is on, as a pill in its Workers row, and the card that pill shows on hover
/// (a tap on the phone).
///
/// CDXC:Coordinators 2026-11-13 DECISION:
/// User, of the Workers pill's list: "when I click on Workers here, can I see pills for each item
/// they're working on so I can hover over them to read more?" On the design page they picked
/// (Q3020 B) a second line under each worker with its items as pills, two then "+N", the first
/// line unchanged and a thread on no item keeping one line; a click shows the item in the Work
/// panel (Q3021); on the phone a tap shows the card with a Show in Work button (Q3022); finished
/// threads under Show recent list all their items with the finished ones dimmed (Q3023); the card
/// quotes the thread's last report (Q3024); the pill is neutral with the Work panel's teal icon
/// (Q3025). Dropped items are left out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkPill {
    /// The Work panel row's key, `work:402`, which the click focuses.
    pub key: String,
    /// `W402`.
    #[serde(rename = "ref")]
    pub reference: String,
    pub title: String,
    pub status: String,
    /// "In progress", "Waiting", "Finished" (work_cards.rs `status_label`).
    pub status_label: String,
    /// Finished: drawn dimmed.
    pub done: bool,
    /// Where it stands now, as the coordinator last said.
    pub now: String,
    /// "3 of 5 steps", or "".
    pub steps: String,
    /// Its questions and tasks still open for the user.
    pub asks: Vec<WorkPillAsk>,
    /// The thread's last report, or "".
    pub report: String,
    /// The whole card as text, its first line the heading (the desktop's card tooltip).
    pub tooltip: String,
    /// What a click sends: `jumpToWorkCard` at `key`.
    pub command: Value,
}

/// One open ask on a pill's card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkPillAsk {
    pub tag: String,
    pub text: String,
}

/// The items a row shows as pills before "+N".
pub const ITEMS_SHOWN: usize = 2;

/// The last line of an item's card: what a click on its pill does.
pub const SHOW_IN_WORK_HINT: &str = "Click to show it in Work";

/// The panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorThreadsPanel {
    /// "1 active · 94 total".
    pub meta: String,
    pub rows: Vec<CoordinatorThreadRow>,
    /// A thread needs the user's approval, which marks the header (red, as the sidebar's approval
    /// dot).
    pub attention: bool,
    /// A thread waits on an answer that is no approval (a question), which marks the header pink
    /// when no thread needs an approval.
    #[serde(default)]
    pub question: bool,
    /// The step the list is at: 0 in flight only, 1 with the recent ones, 2+ with 30 more per step.
    pub step: u32,
    /// "Show recent", "169 more" or "Show fewer"; "" when there is nothing to add or take back.
    pub more_label: String,
    /// The step the link moves to (0 for "Show fewer").
    pub more_step: u32,
    /// The quiet line when nothing is listed: "No workers working".
    pub empty_label: String,
    /// Threads working or waiting on the user, what the first step lists and the Workers pill counts.
    pub in_flight: usize,
    /// Threads working right now, which mark the Workers pill as running (extras/chat_pills.rs).
    pub working: usize,
}

/// The rows each "N more" click adds.
pub const THREADS_PAGE: usize = 30;

/// The tag a row blocked on the user carries.
pub const NEEDS_APPROVAL_LABEL: &str = "Needs your approval";

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `true` when the session is a coordinator, whatever its thread count.
pub fn is_coordinator(threads: Option<&Value>) -> bool {
    threads.is_some_and(Value::is_object)
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The pill of `item` (a row of `coordinatorThreads.work.items`) on a thread whose last report is
/// `report`.
fn work_pill(item: &Value, report: &str) -> WorkPill {
    let status = text(item, "status");
    let reference = text(item, "ref");
    let title = text(item, "title");
    let now = text(item, "now");
    let (done, total) = (
        item["steps"]["done"].as_u64().unwrap_or(0),
        item["steps"]["total"].as_u64().unwrap_or(0),
    );
    let steps = if total > 0 {
        format!("{done} of {total} steps")
    } else {
        String::new()
    };
    let asks: Vec<WorkPillAsk> = item["asks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|ask| ask["open"] == true)
        .map(|ask| WorkPillAsk {
            tag: text(ask, "tag"),
            text: text(ask, "text"),
        })
        .collect();
    let status_label = crate::extras::work_cards::status_label(&status).to_string();
    let mut lines = vec![format!("{reference} {title}")];
    lines.push(
        [status_label.as_str(), steps.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} "),
    );
    if !now.is_empty() {
        lines.push(format!("Now: {now}"));
    }
    for ask in &asks {
        lines.push(if ask.tag.is_empty() {
            ask.text.clone()
        } else {
            format!("{}: {}", ask.tag, ask.text)
        });
    }
    if !report.is_empty() {
        lines.push(format!("Last report: {report}"));
    }
    lines.push(SHOW_IN_WORK_HINT.to_string());
    let key = text(item, "key");
    WorkPill {
        command: json!({"type": "jumpToWorkCard", "key": key}),
        key,
        reference,
        title,
        done: status == "finished",
        status,
        status_label,
        now,
        steps,
        asks,
        report: report.to_string(),
        tooltip: lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Puts on each row the work items its thread is on (`work.items[].threads`): the open ones first,
/// then the finished ones, each newest first; dropped items and threads on no item get none.
fn attach_items(rows: &mut [CoordinatorThreadRow], work: Option<&Value>) {
    let items: Vec<&Value> = work
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| text(item, "kind") == "item" && text(item, "status") != "dropped")
        .collect();
    if items.is_empty() {
        return;
    }
    for row in rows {
        let mut on: Vec<&Value> = items
            .iter()
            .copied()
            .filter(|item| {
                item["threads"].as_array().is_some_and(|threads| {
                    threads.iter().any(|thread| {
                        text(thread, "projectId") == row.project_id
                            && text(thread, "sessionId") == row.session_id
                            && text(thread, "serverId") == row.server_id
                    })
                })
            })
            .collect();
        on.sort_by(|left, right| {
            (text(left, "status") == "finished")
                .cmp(&(text(right, "status") == "finished"))
                .then_with(|| text(right, "at").cmp(&text(left, "at")))
        });
        // gxserver's `lastReport`; a gxserver from before it sends none, and a thread at rest
        // shows its last report as its line, so that line stands in.
        let report = if !row.last_report.is_empty() {
            row.last_report.clone()
        } else if !row.working && !row.waiting && !row.needs_approval {
            row.detail.clone()
        } else {
            String::new()
        };
        let rest = on.split_off(on.len().min(ITEMS_SHOWN));
        row.items = on.iter().map(|item| work_pill(item, &report)).collect();
        if !rest.is_empty() {
            row.more_items = format!("+{}", rest.len());
            row.more_items_tooltip = rest
                .iter()
                .map(|item| format!("{} {}", text(item, "ref"), text(item, "title")))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
}

/// Moves the list to `step`; the first two steps drop the full list "N more" read.
pub fn set_step(panels: &mut PanelsState, step: u32) {
    panels.threads_step = step;
    if step < 2 {
        panels.threads_all = None;
        panels.threads_all_request = None;
    }
}

/// The read "N more" needs now: the list is past "Show recent" and what it holds (if anything) is not the
/// revision the latest frame names. A gxserver that sends no revision sends every thread on its
/// frames, so there is nothing to read.
pub fn read_all_threads(
    panels: &mut crate::state::Stamped<PanelsState>,
    threads: Option<&Value>,
    next_request_id: impl FnOnce() -> u64,
) -> Option<Effect> {
    if panels.threads_step < 2 || panels.threads_all_request.is_some() {
        return None;
    }
    let revision = threads?
        .get("revision")
        .and_then(Value::as_str)
        .filter(|revision| !revision.is_empty())?;
    if panels
        .threads_all
        .as_ref()
        .is_some_and(|(loaded, _)| loaded == revision)
    {
        return None;
    }
    let request_id = next_request_id();
    panels.threads_all_request = Some((request_id, revision.to_string()));
    Some(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadCoordinatorThreads,
        params: Box::new(json!({})),
    })
}

/// Takes the answer to [`read_all_threads`]; `false` when `request_id` is not that read.
pub fn settle_read(
    panels: &mut PanelsState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> bool {
    let asked = match panels.threads_all_request.take() {
        Some((id, asked)) if id == request_id => asked,
        other => {
            panels.threads_all_request = other;
            return false;
        }
    };
    panels.threads_all = Some(match answer {
        Ok(result) => (
            result
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or(&asked)
                .to_string(),
            result.get("threads").cloned().unwrap_or(Value::Null),
        ),
        Err(_) => (asked, Value::Null),
    });
    true
}

/// A thread row was clicked: gxserver resumes the thread when its session is closed
/// (`openCoordinatorThread`), and [`settle_open`] then asks the host to focus it.
///
/// CDXC:Coordinators 2026-10-08 WHY:
/// Each host used to decide on its own whether a clicked thread needed waking, and the web build never woke one, so a closed thread opened as a dead transcript. The decision is gxserver's now (server/src/server/coordinator_open_http.rs) and every host only focuses the session it is handed.
///
/// `server_id` names the computer of a thread on another computer than the coordinator's (the row's
/// `serverId`); gxserver resumes it there, and the host opens it on that computer.
pub fn open_thread(
    panels: &mut PanelsState,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    request_id: u64,
) -> Option<Effect> {
    let (project_id, session_id) = (project_id.trim(), session_id.trim());
    if project_id.is_empty() || session_id.is_empty() {
        return None;
    }
    panels.threads_open_request = Some(request_id);
    let mut params = json!({
        "threadProjectId": project_id,
        "threadSessionId": session_id,
    });
    if !server_id.trim().is_empty() {
        params["threadServerId"] = json!(server_id.trim());
    }
    Some(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::OpenCoordinatorThread,
        params: Box::new(params),
    })
}

/// Takes the answer to [`open_thread`]: the host's focus, or a toast when the thread could not be
/// resumed. `None` when `request_id` is not that call; an empty list when a newer click replaced it.
pub fn settle_open(
    panels: &mut PanelsState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    if panels.threads_open_request != Some(request_id) {
        return None;
    }
    panels.threads_open_request = None;
    Some(vec![match answer {
        Ok(result) => {
            let mut params = json!({
                "projectId": text(result, "projectId"),
                "sessionId": text(result, "sessionId"),
            });
            // A thread on another computer: the host opens it on the computer this names.
            let server_id = text(result, "serverId");
            if !server_id.is_empty() {
                params["serverId"] = json!(server_id);
            }
            Effect::HostAction {
                action: "openCoordinatorThread".to_string(),
                params: Box::new(params),
            }
        }
        Err(message) => Effect::Toast {
            level: "error".to_string(),
            message: if message.is_empty() {
                "Could not open that worker.".to_string()
            } else {
                message
            },
        },
    }])
}

fn parse_rows(rows: Option<&Value>) -> Vec<(String, CoordinatorThreadRow)> {
    rows.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let state = text(row, "state");
            let working = state == "working";
            (
                text(row, "activeAt"),
                CoordinatorThreadRow {
                    // A thread on another computer keeps that computer in its key: ids are only
                    // unique within one gxserver.
                    key: match text(row, "serverId") {
                        server_id if server_id.is_empty() => {
                            format!("{}:{}", text(row, "projectId"), text(row, "sessionId"))
                        }
                        server_id => format!(
                            "{server_id}:{}:{}",
                            text(row, "projectId"),
                            text(row, "sessionId")
                        ),
                    },
                    project_id: text(row, "projectId"),
                    session_id: text(row, "sessionId"),
                    title: text(row, "title"),
                    detail: text(row, "detail"),
                    working,
                    needs_approval: !working
                        && row.get("needsApproval") == Some(&Value::Bool(true)),
                    waiting: state == "waiting",
                    recent: row.get("recent") == Some(&Value::Bool(true)),
                    branch: text(row, "branch"),
                    lifecycle_state: text(row, "lifecycleState"),
                    server_id: text(row, "serverId"),
                    computer: text(row, "computer"),
                    last_report: text(row, "lastReport"),
                    ..CoordinatorThreadRow::default()
                },
            )
        })
        .collect()
}

/// The panel, or `None` for a session that is not a coordinator or has no threads yet. `threads`
/// is the frames' summary; `all` the full list the first "N more" read, while the list is past
/// "Show recent".
pub fn coordinator_threads_panel(
    threads: Option<&Value>,
    step: u32,
    all: Option<&Value>,
) -> Option<CoordinatorThreadsPanel> {
    let threads = threads?;
    let summary = parse_rows(threads.get("threads"));
    let all = all.filter(|all| all.is_array()).filter(|_| step >= 2);
    let mut rows = match all {
        Some(all) => parse_rows(Some(all)),
        None => summary,
    };
    // An older gxserver sends every thread and no total.
    let total = threads
        .get("total")
        .and_then(Value::as_u64)
        .map_or(0, |total| total as usize)
        .max(rows.len());
    if total == 0 {
        return None;
    }
    // Working, then waiting on the user, then the rest; each newest first (ISO times sort as text).
    // The summary holds the in-flight and recent rows; the full list holds every thread, so its
    // recent rows are the newest of the rest and the rows after them are the next pages.
    let in_flight = |row: &CoordinatorThreadRow| row.working || row.needs_approval || row.waiting;
    let rank = |row: &CoordinatorThreadRow| match (row.working, in_flight(row)) {
        (true, _) => 0,
        (false, true) => 1,
        (false, false) => 2,
    };
    rows.sort_by(|(left_at, left), (right_at, right)| {
        rank(left)
            .cmp(&rank(right))
            .then_with(|| right_at.cmp(left_at))
    });
    let working = rows.iter().filter(|(_, row)| row.working).count();
    let approval = rows.iter().filter(|(_, row)| row.needs_approval).count();
    let question = rows
        .iter()
        .any(|(_, row)| row.waiting && !row.needs_approval && !row.working);
    let flying = rows.iter().filter(|(_, row)| in_flight(row)).count();
    let recent = rows
        .iter()
        .filter(|(_, row)| !in_flight(row) && row.recent)
        .count();
    let listed = match step {
        0 => flying,
        1 => flying + recent,
        _ => (flying + recent + THREADS_PAGE * (step as usize - 1)).min(total),
    };
    let hidden = total.saturating_sub(listed);
    let mut meta = Vec::new();
    // CDXC:Coordinators 2026-10-29 DECISION: User: "make this 2 active • 429 total": the header counts
    // the workers running now as "active" and every worker as "total".
    if working > 0 {
        meta.push(format!("{working} active"));
    }
    if approval > 0 {
        meta.push(plural(
            approval,
            "needs your approval",
            "need your approval",
        ));
    }
    meta.push(format!("{total} total"));
    let rows: Vec<CoordinatorThreadRow> = match all {
        // The full list: its first `listed` rows.
        Some(_) => rows.into_iter().take(listed).map(|(_, row)| row).collect(),
        // The summary (or the full list still being read): the in-flight rows, and the recent ones
        // past the first step.
        None => rows
            .into_iter()
            .map(|(_, row)| row)
            .filter(|row| in_flight(row) || (step > 0 && row.recent))
            .collect(),
    };
    let mut rows = rows;
    attach_items(&mut rows, threads.get("work"));
    // With no recent rows "Show recent" would add nothing, so the first link is already "N more".
    let (more_label, more_step) = if hidden == 0 {
        if step > 0 && total > flying {
            ("Show fewer".to_string(), 0)
        } else {
            (String::new(), 0)
        }
    } else if step == 0 && recent > 0 {
        ("Show recent".to_string(), 1)
    } else {
        (format!("{hidden} more"), step.max(1) + 1)
    };
    Some(CoordinatorThreadsPanel {
        meta: meta.join(" \u{b7} "),
        attention: approval > 0,
        question,
        step,
        more_label,
        more_step,
        empty_label: if rows.is_empty() {
            "No workers working".to_string()
        } else {
            String::new()
        },
        in_flight: flying,
        working,
        rows,
    })
}

/// The document value, `null` when there is no panel. Each row's title draws as its session's pill
/// (CDXC:SessionChat 2026-11-13 on `mention_card` in transcript/session_mentions.rs), so a row the
/// host's list `sessions` knows carries its agent's `agentIcon` and `coordinator`.
pub fn project(
    threads: Option<&Value>,
    step: u32,
    all: Option<&Value>,
    sessions: &[crate::event::KnownSession],
) -> Value {
    let mut panel = coordinator_threads_panel(threads, step, all)
        .and_then(|panel| serde_json::to_value(panel).ok())
        .unwrap_or(Value::Null);
    if let Some(rows) = panel.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows {
            let found = sessions.iter().find(|session| {
                session.project_id == text(row, "projectId")
                    && session.session_id == text(row, "sessionId")
                    && match text(row, "serverId") {
                        server if server.is_empty() => true,
                        server => session.global_ref.starts_with(&format!("{server}:")),
                    }
            });
            if let Some(session) = found {
                row["agentIcon"] = crate::transcript::agent_message_card::agent_icon(
                    &session.agent_id,
                    &session.agent_name,
                );
                row["coordinator"] = session.coordinator.into();
            }
        }
    }
    panel
}

#[cfg(test)]
mod tests {
    use super::coordinator_threads_panel;
    use serde_json::json;

    /// The header's waiting dot: an approval marks `attention` (red), a thread waiting on any other
    /// answer marks `question` (pink); a working thread marks neither.
    #[test]
    fn panel_tells_an_approval_from_a_question() {
        let row = |id: &str, state: &str, approval: bool| json!({"projectId": "p", "sessionId": id, "state": state, "needsApproval": approval});
        let panel = |rows: Vec<serde_json::Value>| {
            coordinator_threads_panel(Some(&json!({"threads": rows})), 0, None).unwrap()
        };
        let asking = panel(vec![row("a", "working", false), row("b", "waiting", false)]);
        assert!((!asking.attention, asking.question) == (true, true));
        let stuck = panel(vec![row("a", "waiting", true)]);
        assert!(stuck.attention && !stuck.question);
        let busy = panel(vec![row("a", "working", false)]);
        assert!(!busy.attention && !busy.question);
    }

    /// A thread on another computer is listed with that computer's name, keeps the computer in its
    /// key, and its click asks gxserver to open it there and the host to open it on that computer.
    #[test]
    fn a_thread_on_another_computer_names_it_and_opens_there() {
        let panel = coordinator_threads_panel(
            Some(&json!({"threads": [
                {"projectId": "p", "sessionId": "a", "state": "working"},
                {"serverId": "S8g", "computer": "Windows", "projectId": "p", "sessionId": "a", "state": "working"},
            ]})),
            0,
            None,
        )
        .unwrap();
        assert_eq!(
            panel.rows.len(),
            2,
            "same ids on two computers are two threads"
        );
        let remote = panel
            .rows
            .iter()
            .find(|row| row.server_id == "S8g")
            .unwrap();
        assert_eq!(remote.key, "S8g:p:a");
        assert_eq!(remote.computer, "Windows");

        let mut panels = crate::state::PanelsState::default();
        let Some(crate::effect::Effect::SendRpc { params, .. }) =
            super::open_thread(&mut panels, "S8g", "p", "a", 7)
        else {
            panic!("the click asks gxserver");
        };
        assert_eq!(params["threadServerId"], "S8g");
        let effects = super::settle_open(
            &mut panels,
            7,
            Ok(&json!({"serverId": "S8g", "projectId": "p", "sessionId": "a"})),
        )
        .unwrap();
        let [crate::effect::Effect::HostAction { params, .. }] = effects.as_slice() else {
            panic!("the host opens it");
        };
        assert_eq!(params["serverId"], "S8g");

        // A thread on the coordinator's own computer sends no server id.
        let Some(crate::effect::Effect::SendRpc { params, .. }) =
            super::open_thread(&mut panels, "", "p", "a", 8)
        else {
            panic!("the click asks gxserver");
        };
        assert!(params.get("threadServerId").is_none());
    }
}
