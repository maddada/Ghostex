//! What a card for a message between agents says: one another agent sent this session, one this
//! session's agent sent another (or several others at once), and a subagent's reply. Each card is
//! one `card` object (on `interAgentMessage`, on a `sentMessages` entry, or on an `agent-message`
//! system card) that the renderers lay out as is.
//!
//! CDXC:SessionChat 2026-10-12 DECISION:
//! User: the morning's "proposed" agent message cards (a 240px card with a quote box and a footer) were "too large now ... make them smaller like before. i dont like the new style". From the mockup page (docs/2026-10-11/agent-message-cards/smaller-received-cards.html) the user picked D, one line: every message between agents (received, sent, and a subagent's reply alike) is a single row with the agent's own logo (a small arrow badge: prose grey coming in, sky going out), its sentence, then the time and the status; clicking the row opens the whole message under it. Go to session shows on hover (on the phone, inside the opened message). Queued, Waiting for agent and Not sent say so in their colour. User (2026-10-18), for every one-line card: "no need for the chevron for all of them and i like the pills on the right side instead of this", so the row has no chevron and the status is a pill shaped like the turn fold's "1 tool call" (the SessionChat 2026-10-18 DECISION on `SINGLE_LINE_CARD` in apps/desktop/src/app/native_chat/cards.rs), superseding the chevron and the bare status; then "these dont look all the same too", so Delivered is a green check with its word in that pill, like "Answered", superseding the first round's bare green check. Kept from the first round: "please show the icons of the different agents", "instead of Open api refactor just say 'Go to session'", and no purple for incoming messages ("use color usually used for incoming chat messages"). The row reads the same collapsed and expanded ("keep the text when the message from x agent the same when it's collapsed as the one we show when it's expanded"). Supersedes the same day's "<session title> + first line" rows: "This should say: Message from "Section labels wrap and link" and remove the "Ghostex thread report: finished its turn. Its fin..." part (summary of message) do same thing for Message to "Xyz" also please". So the row is one sentence with no preview: `Message from "<session title>"` / `Message to "<session title>"`, the title quoted; with no title the agent's name, unquoted (`Message from Codex`); nothing known reads `Message from another agent` / `Message to another agent`, never the raw reference; one command that sends the same message to several sessions (a loop, or several sends in one call) is ONE row, `Message to 2 sessions`, whose opened message lists each recipient; a subagent's reply is `Message from Explore subagent` (`Message from a subagent` untyped). The title is the other session's, from the host's session list first.
//! SEE-ALSO: agent_message.rs and sent_message.rs read the messages and name the other session (the host's session list first, `Event::SessionDirectoryChanged`); apps/desktop/src/app/native_chat/inter_agent_message.rs and apps/mobile/app/src/chat/native/transcript/AgentMessageCards.tsx draw the card.

use serde_json::{json, Value};

use crate::extras::agents::{agent_icon_id, default_agent_by_id};
use crate::transcript::jsstr::js_trim;
use crate::transcript::session_mentions::card_pill;

/// The jump button's label, whichever session it opens.
pub const GO_TO_SESSION: &str = "Go to session";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(js_trim)
        .unwrap_or_default()
}

/// The agent's artwork id: the agent id the header or the send's result named, else its name.
pub(crate) fn agent_icon(agent_id: &str, agent_name: &str) -> Value {
    default_agent_by_id(Some(agent_id))
        .or_else(|| default_agent_by_id(Some(agent_name)))
        .map(|(icon, _)| icon)
        .or_else(|| agent_icon_id(Some(agent_name)))
        .map_or(Value::Null, Value::from)
}

/// An agent's name as the app writes it (`claude` is "Claude"), or the name as given.
pub fn agent_display_name(agent_id: &str, agent_name: &str) -> String {
    let agent_name = js_trim(agent_name);
    default_agent_by_id(Some(agent_name))
        .or_else(|| default_agent_by_id(Some(agent_id)))
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| agent_name.to_string())
}

/// `label` is what the row's pill says beside its icon, `hint` its tooltip; `tone` is `good`,
/// `waiting` or `failed`; `icon` is `check`, `clock` or `alert`.
fn status(label: &str, hint: &str, tone: &str, icon: &str) -> Value {
    json!({
        "label": label,
        "hint": hint,
        "tone": tone,
        "icon": icon,
    })
}

/// The session a `<machine>:<project>:<session>` reference names, for the jump button.
fn session_target(reference: &str) -> Value {
    let reference = js_trim(reference);
    let parts: Vec<&str> = reference.split(':').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Value::Null;
    }
    json!({ "projectId": parts[1], "sessionId": parts[2], "label": GO_TO_SESSION })
}

/// The other session's pill after `prefix`, when the card knows its title and can open it; `Null`
/// otherwise, and the renderers draw the plain sentence (`name`).
fn titled_pill(
    prefix: &str,
    title: &str,
    agent_icon: &Value,
    coordinator: bool,
    session: &Value,
    gone: bool,
) -> Value {
    if js_trim(title).is_empty() || !session.is_object() {
        return Value::Null;
    }
    card_pill(
        prefix,
        title,
        agent_icon.clone(),
        coordinator,
        session,
        gone,
    )
}

/// A session title in a row's sentence, quoted so it reads as a name (`Message from "Fix login"`).
fn quoted(title: &str) -> String {
    format!("\"{title}\"")
}

/// One card. `name` is the row's whole sentence, the same collapsed and opened, and `title` what
/// an assistive reader hears. `recipients` lists each session of a message sent to several (empty
/// otherwise).
#[allow(clippy::too_many_arguments)]
fn card(
    direction: &str,
    agent_icon: Value,
    name: String,
    title: String,
    time: &str,
    status: Value,
    failure: &str,
    retry_prompt_id: Value,
    session: Value,
    recipients: Vec<Value>,
) -> Value {
    json!({
        "direction": direction,
        "agentIcon": agent_icon,
        "name": name,
        "title": title,
        "time": time,
        "status": status,
        "failure": failure,
        "retryPromptId": retry_prompt_id,
        "session": session,
        "recipients": recipients,
    })
}

/// Marks a card whose other session is a coordinator, which the renderers draw with the crown in
/// place of its agent's logo (the arrow badge stays: it says which way the message went).
///
/// CDXC:Coordinators 2026-10-15 DECISION:
/// User, on a thread's `Message from "Main Coordinator"` card that showed the generic robot: "show crown icon for orchestrator messages not this". A card whose other session is a coordinator (the same `coordinatorRole` test as the sidebar's crown, read from the host's session list) draws the sidebar's crown (`titlebar/coordinator-crown.svg`, white on dark, black on light) instead of the agent logo, on the desktop, the web build and the phone; messages from any other agent keep their logo. Messages to a coordinator get the crown too, so the card always shows the other session the same way.
fn crowned(mut card: Value, coordinator: bool) -> Value {
    card["coordinator"] = coordinator.into();
    card
}

/// A subagent's name as its cards say it: "Explore subagent", or "a subagent" for an untyped one.
pub fn subagent_label(name: &str) -> String {
    let name = js_trim(name);
    if name.is_empty() || name.eq_ignore_ascii_case("subagent") {
        "a subagent".to_string()
    } else {
        format!("{name} subagent")
    }
}

/// A subagent's reply: the sparkle in place of a logo.
pub fn subagent_card(name: &str) -> Value {
    let sentence = format!("Message from {}", subagent_label(name));
    card(
        "subagent",
        Value::Null,
        sentence.clone(),
        sentence,
        "",
        Value::Null,
        "",
        Value::Null,
        Value::Null,
        Vec::new(),
    )
}

/// Writes `interAgentMessage.card` for a projected message from another agent. Run again whenever
/// the transcript or the host's session list names the sender (`sent_message::resolve_recipients`).
pub fn dress_received(message: &mut Value) {
    let inter = &message["interAgentMessage"];
    if !inter.is_object() {
        return;
    }
    if inter["subagent"] == true {
        let card = subagent_card(text(inter, "agentName"));
        message["interAgentMessage"]["card"] = card;
        return;
    }
    let agent = agent_display_name(text(inter, "agentId"), text(inter, "agentName"));
    let session_title = text(inter, "sessionTitle").to_string();
    let name = match (text(inter, "sessionTitle"), agent.as_str()) {
        ("", "") => "Message from another agent".to_string(),
        ("", agent) => format!("Message from {agent}"),
        (session, _) => format!("Message from {}", quoted(session)),
    };
    let delivery = &message["startupDelivery"];
    let failed = delivery.is_object() && delivery["state"] == "failed";
    let status = if failed {
        status("Not sent", "Not sent", "failed", "alert")
    } else if delivery["state"] == crate::session::constants::DELIVERY_UNCONFIRMED {
        status("Not confirmed", "Not confirmed", "waiting", "clock")
    } else if delivery.is_object() {
        status("Waiting for agent", "Waiting for agent", "waiting", "clock")
    } else if message["queued"] == true {
        status("Queued", "Queued until this turn ends", "waiting", "clock")
    } else {
        status("Delivered", "Delivered", "good", "check")
    };
    let failure = if !failed {
        ""
    } else {
        match text(delivery, "errorMessage") {
            "" => "Message could not be delivered.",
            error => error,
        }
    };
    let icon = agent_icon(text(inter, "agentId"), &agent);
    let session = session_target(text(inter, "replyTo"));
    let coordinator = inter["coordinator"] == true;
    let pill = titled_pill(
        "Message from",
        &session_title,
        &icon,
        coordinator,
        &session,
        inter["gone"] == true,
    );
    let mut card = card(
        "received",
        icon,
        name.clone(),
        name,
        text(&message["time"], "label"),
        status,
        failure,
        if failed {
            delivery["promptId"].clone()
        } else {
            Value::Null
        },
        session,
        Vec::new(),
    );
    card["pill"] = pill;
    // Why a received message still waits for this agent, once gxserver has held it a minute.
    if !failed {
        if let Some(reason) = delivery["waitReason"]
            .as_str()
            .filter(|reason| !reason.is_empty())
        {
            card["waitReason"] = reason.into();
            // A row held long enough to say why, or one that left unconfirmed, can be removed.
            if matches!(
                delivery["state"].as_str(),
                Some("queued" | crate::session::constants::DELIVERY_UNCONFIRMED)
            ) {
                card["removePromptId"] = delivery["promptId"].clone();
            }
        }
    }
    message["interAgentMessage"]["card"] = crowned(card, coordinator);
}

/// Brings one recipient's status up to date from gxserver's delivery watch: a send that answered
/// `pending`, `accepted` or `queued` becomes `delivered` once the recipient's transcript records
/// the message, failed with the watch's reason once Ghostex gives up on it, and `untracked` when
/// gxserver follows no such send (or stopped following it).
///
/// CDXC:SessionChat 2026-10-14 DECISION:
/// Asked "Should a sent message's card update once the other session actually receives it?", the user answered "yes, update it to Delivered". A card whose send answered `pending`, `accepted` or `queued` (seen: an orchestrator's sends to two busy threads kept "Waiting for agent" after both had arrived, and read as an error) changes to Delivered when the message reaches the other session, and to Not sent with the reason when Ghostex gives up on it; it stays so after a reload, a restart and on the phone. gxserver decides (server/src/agent_message_deliveries.rs); the card only applies its outcome. A message sent to several sessions keeps one combined status, from each recipient's live one.
/// CDXC:SessionChat 2026-10-31 WHY: the card used to find its outcome by the send's `sendRequestId` alone, which an agent that filters the send's output (`| grep -m1 '"status"'`) never prints, so it said "Queued until its turn ends" for good. Without the id it is found by the message's fingerprint, the recipient and a send time inside its tool call's run (`delivery_key`); a waiting card gxserver does not follow says Sent instead of claiming to be on its way (the 2026-10-31 note in packages/gx-protocol/src/agent_message_delivery.rs).
/// SEE-ALSO: server/src/server/agent_message_delivery_runtime.rs (the watch), server/src/ghostex_cli/agents/delivery.rs (registers each waiting send).
pub fn apply_live_delivery(recipient: &mut Value, deliveries: Option<&Value>) {
    let Some(key) = live_delivery_key(recipient) else {
        return;
    };
    let Some(outcome) = delivery_lookup(deliveries, &key) else {
        return;
    };
    match text(&outcome, "state") {
        "delivered" => recipient["status"] = "delivered".into(),
        "failed" => {
            recipient["failed"] = true.into();
            recipient["failure"] = text(&outcome, "failure").to_string().into();
        }
        "untracked" => recipient["status"] = "untracked".into(),
        _ => {}
    }
}

/// How far a send's time may sit outside its tool call's run: both are taken on the machine that
/// runs the agent, so only the gap between writing a row and stamping it needs covering.
const SEND_TIME_SLACK_MS: i64 = 1_000;

/// What [`apply_live_delivery`] looks a recipient up by, or `None` when its send already settled
/// and the watch's word cannot change it: the send's `sendRequestId`, else `?` and the message's
/// fingerprint, the recipient's project and session and the tool call's run
/// (`?<fingerprint>|<project>|<session>|<called>|<answered>`, an unknown part left empty).
pub fn live_delivery_key(recipient: &Value) -> Option<String> {
    if recipient["failed"] == true
        || !matches!(text(recipient, "status"), "pending" | "accepted" | "queued")
    {
        return None;
    }
    let id = text(recipient, "sendRequestId");
    if !id.is_empty() {
        return Some(id.to_string());
    }
    let fingerprint = text(recipient, "fingerprint");
    if fingerprint.is_empty() {
        return None;
    }
    let reference = [text(recipient, "globalRef"), text(recipient, "target")]
        .into_iter()
        .find_map(ghostex_gx_protocol::parse_global_session_ref);
    let (project, session) = match &reference {
        Some(reference) => (
            reference.project_id.to_string(),
            reference.session_id.to_string(),
        ),
        None => (String::new(), String::new()),
    };
    let time = |key: &str| {
        recipient
            .get(key)
            .and_then(Value::as_i64)
            .map(|at| at.to_string())
            .unwrap_or_default()
    };
    Some(format!(
        "?{fingerprint}|{project}|{session}|{}|{}",
        time("calledAt"),
        time("answeredAt")
    ))
}

/// gxserver's word on the send a [`live_delivery_key`] names: its outcome, a waiting entry, or
/// `{ "state": "untracked" }` when gxserver lists its sends and follows none such. `None` when
/// gxserver says nothing (no field yet, or one from before it listed its sends and no id to go by).
pub fn delivery_lookup(deliveries: Option<&Value>, key: &str) -> Option<Value> {
    let deliveries = deliveries?;
    let sends = deliveries
        .get(ghostex_gx_protocol::agent_message_delivery::SENDS_KEY)
        .and_then(Value::as_array);
    let untracked = || json!({ "state": "untracked" });
    if let Some(query) = key.strip_prefix('?') {
        return Some(
            matching_send(sends?, query)
                .cloned()
                .unwrap_or_else(untracked),
        );
    }
    if let Some(outcome) = deliveries.get(key) {
        return Some(outcome.clone());
    }
    Some(
        sends?
            .iter()
            .find(|send| text(send, "sendRequestId") == key)
            .cloned()
            .unwrap_or_else(untracked),
    )
}

/// The send a `?` key names: the message's fingerprint, the recipient when the card knows it, and
/// the latest send inside the tool call's run (a repeat of the same message in a later call sits
/// in that call's run); with no run to go by, only a send that is the one such.
fn matching_send<'a>(sends: &'a [Value], query: &str) -> Option<&'a Value> {
    let parts: Vec<&str> = query.split('|').collect();
    let [fingerprint, project, session, called, answered] = parts[..] else {
        return None;
    };
    let called = called.parse::<i64>().ok();
    let answered = answered.parse::<i64>().ok();
    let candidates: Vec<&Value> = sends
        .iter()
        .filter(|send| {
            text(send, "fingerprint") == fingerprint
                && (project.is_empty() || text(send, "projectId") == project)
                && (session.is_empty() || text(send, "sessionId") == session)
        })
        .collect();
    let sent_at = |send: &Value| send.get("sentAtMs").and_then(Value::as_i64);
    if called.is_none() && answered.is_none() {
        return match candidates[..] {
            [only] => Some(only),
            _ => None,
        };
    }
    candidates
        .into_iter()
        .filter(|send| {
            sent_at(send).is_some_and(|at| {
                called.is_none_or(|called| at >= called - SEND_TIME_SLACK_MS)
                    && answered.is_none_or(|answered| at <= answered + SEND_TIME_SLACK_MS)
            })
        })
        .max_by_key(|send| sent_at(send))
}

/// Where one recipient's send left the message, as the result said; `Null` when nothing was read
/// back.
fn delivery_status(recipient: &Value) -> Value {
    if recipient["failed"] == true {
        let failure = text(recipient, "failure");
        return status(
            "Not sent",
            if failure.is_empty() {
                "Not sent"
            } else {
                failure
            },
            "failed",
            "alert",
        );
    }
    match text(recipient, "status") {
        "delivered" => status("Delivered", "Delivered", "good", "check"),
        "queued" => status("Queued", "Queued until its turn ends", "waiting", "clock"),
        "pending" => status("Waiting for agent", "Waiting for agent", "waiting", "clock"),
        "accepted" => status("Sent", "Sent", "good", "check"),
        // A waiting send Ghostex does not follow: it went out, and nothing says where it is now.
        "untracked" => status(
            "Sent",
            "Sent. Ghostex is not following whether it arrived.",
            "good",
            "check",
        ),
        _ => Value::Null,
    }
}

/// One status for a message sent to several sessions: Not sent when any send failed (with how
/// many), Delivered once every one was, else the first one still waiting; nothing while any is
/// unknown.
fn combined_status(statuses: &[Value]) -> Value {
    let count = statuses.len();
    let failed = statuses
        .iter()
        .filter(|status| status["tone"] == "failed")
        .count();
    if failed == count {
        return status("Not sent", "Not sent", "failed", "alert");
    }
    if failed > 0 {
        return status(
            &format!("{failed} not sent"),
            &format!("{failed} of {count} not sent"),
            "failed",
            "alert",
        );
    }
    if let Some(waiting) = statuses.iter().find(|status| status["tone"] == "waiting") {
        return waiting.clone();
    }
    if statuses.iter().all(|status| status["tone"] == "good") {
        return status(
            "Delivered",
            &format!("Delivered to all {count}"),
            "good",
            "check",
        );
    }
    Value::Null
}

/// The jump target of one recipient: its global reference, else the reference it was sent to.
fn recipient_session(recipient: &Value) -> Value {
    match session_target(text(recipient, "globalRef")) {
        Value::Null => session_target(text(recipient, "target")),
        target => target,
    }
}

fn recipient_icon(recipient: &Value) -> Value {
    agent_icon(text(recipient, "agentId"), text(recipient, "agentName"))
}

/// What a recipient is called on the card: its `name`, which `sent_message::resolve_recipients`
/// wrote, or "another agent" when nothing names it.
fn recipient_label(recipient: &Value) -> String {
    match text(recipient, "name") {
        "" => "another agent".to_string(),
        name => name.to_string(),
    }
}

/// A recipient in the row's sentence: its title quoted, an agent's name as it is.
fn recipient_phrase(recipient: &Value) -> String {
    let label = recipient_label(recipient);
    if recipient["titled"] == true {
        quoted(&label)
    } else {
        label
    }
}

/// A recipient's pill, when a session title names it.
fn recipient_pill(prefix: &str, recipient: &Value, icon: &Value, session: &Value) -> Value {
    if recipient["titled"] != true {
        return Value::Null;
    }
    titled_pill(
        prefix,
        &recipient_label(recipient),
        icon,
        recipient["coordinator"] == true,
        session,
        recipient["gone"] == true,
    )
}

/// Writes a sent message's `card` from its `recipients`, once each one is named:
/// `Message to "<session>"`, or `Message to N sessions` with each recipient listed inside.
pub fn dress_sent(sent: &mut Value) {
    let recipients = sent["recipients"].as_array().cloned().unwrap_or_default();
    if recipients.len() <= 1 {
        let recipient = recipients.first().unwrap_or(sent);
        let sentence = format!("Message to {}", recipient_phrase(recipient));
        let failure = if recipient["failed"] == true {
            text(recipient, "failure")
        } else {
            ""
        }
        .to_string();
        let coordinator = recipient["coordinator"] == true;
        let icon = recipient_icon(recipient);
        let session = recipient_session(recipient);
        let pill = recipient_pill("Message to", recipient, &icon, &session);
        let mut row = card(
            "sent",
            icon,
            sentence.clone(),
            sentence,
            "",
            delivery_status(recipient),
            &failure,
            Value::Null,
            session,
            Vec::new(),
        );
        row["pill"] = pill;
        sent["card"] = crowned(row, coordinator);
        return;
    }
    let count = recipients.len();
    let statuses: Vec<Value> = recipients.iter().map(delivery_status).collect();
    let icons: Vec<Value> = recipients.iter().map(recipient_icon).collect();
    // One logo when every recipient runs the same agent; a mixed list shows the generic one.
    let icon = if icons.windows(2).all(|pair| pair[0] == pair[1]) {
        icons[0].clone()
    } else {
        Value::Null
    };
    let listed: Vec<Value> = recipients
        .iter()
        .zip(statuses.iter().zip(icons))
        .map(|(recipient, (status, icon))| {
            let session = recipient_session(recipient);
            json!({
                "direction": "sent",
                "name": recipient_label(recipient),
                "pill": recipient_pill("", recipient, &icon, &session),
                "agentIcon": icon,
                "coordinator": recipient["coordinator"] == true,
                "status": status,
                "session": session,
            })
        })
        .collect();
    let sentence = format!("Message to {count} sessions");
    // The row's own mark is the crown only when every recipient is a coordinator.
    let coordinator = recipients
        .iter()
        .all(|recipient| recipient["coordinator"] == true);
    sent["card"] = crowned(
        card(
            "sent",
            icon,
            sentence.clone(),
            sentence,
            "",
            combined_status(&statuses),
            "",
            Value::Null,
            Value::Null,
            listed,
        ),
        coordinator,
    );
}

#[cfg(test)]
mod pill_logo_tests {
    use super::*;

    /// One message sent to two sessions: the row has no pill (it names a count), and each listed
    /// recipient's pill sits beside that recipient's own tile, so none draws the logo again.
    #[test]
    fn card_pills_leave_the_logo_to_the_tile_beside_them() {
        let recipient = |id: &str, title: &str| {
            json!({
                "name": title,
                "titled": true,
                "agentId": "codex",
                "globalRef": format!("S90:P3lv0:{id}"),
                "status": "delivered",
            })
        };
        let mut sent = json!({
            "recipients": [recipient("G1", "Fix login"), recipient("G2", "Sidebar")],
        });
        dress_sent(&mut sent);
        let card = &sent["card"];
        assert!(card["pill"].is_null());
        for listed in card["recipients"].as_array().unwrap() {
            assert_eq!(listed["agentIcon"], "codex");
            assert_eq!(listed["pill"]["logo"], false);
            // The title alone, no `+` (CDXC:SessionChat 2026-10-28 on `pill_label`).
            assert!(!listed["pill"]["label"].as_str().unwrap().starts_with('+'));
        }

        let mut single = json!({ "recipients": [recipient("G1", "Fix login")] });
        dress_sent(&mut single);
        assert_eq!(single["card"]["agentIcon"], "codex");
        assert_eq!(single["card"]["pill"]["logo"], false);
    }
}

#[cfg(test)]
mod wait_reason_tests {
    use super::dress_received;
    use serde_json::json;

    #[test]
    fn a_held_message_from_another_agent_says_why_it_waits() {
        let reason = "Waiting for the agent to finish its turn.";
        let mut message = json!({
            "interAgentMessage": {"agentId": "claude", "sessionTitle": "Thread"},
            "startupDelivery": {"promptId": "Q1", "state": "queued", "waitReason": reason},
        });
        dress_received(&mut message);
        let card = &message["interAgentMessage"]["card"];
        assert_eq!(card["waitReason"], json!(reason));
        assert_eq!(card["removePromptId"], json!("Q1"));
        let mut typing = json!({
            "interAgentMessage": {"agentId": "claude"},
            "startupDelivery": {"promptId": "Q1", "state": "sending", "waitReason": reason},
        });
        dress_received(&mut typing);
        assert!(typing["interAgentMessage"]["card"]["removePromptId"].is_null());
        let mut unconfirmed = json!({
            "interAgentMessage": {"agentId": "claude"},
            "startupDelivery": {"promptId": "Q1", "state": "unconfirmed", "waitReason": "It left."},
        });
        dress_received(&mut unconfirmed);
        let card = &unconfirmed["interAgentMessage"]["card"];
        assert_eq!(card["removePromptId"], json!("Q1"));
        assert_eq!(card["status"]["label"], json!("Not confirmed"));
        let mut failed = json!({
            "interAgentMessage": {"agentId": "claude"},
            "startupDelivery": {"promptId": "Q1", "state": "failed", "errorMessage": "No box.", "waitReason": reason},
        });
        dress_received(&mut failed);
        assert!(failed["interAgentMessage"]["card"]["waitReason"].is_null());
    }
}
