//! Another session named in a chat: a mention link (`[<title>](<Global Ref>)`, from Copy ID or the
//! composer's `+` picker) and the other session of a message between agents, both drawn as that
//! session's agent pill: its logo and `+<title>`, a click switches to it, muted when it is gone.
//! Also the `+` picker's list of the project's agent sessions.
//!
//! CDXC:SessionChat 2026-10-28 DECISION:
//! User: "we need to be able to do + to mention agents in the same project", the mention "Rendered like new Agent Pill look", and then "we should use this + thing to show agent pills in all of the messages from and to different agents. clicking on this +xyz in the chat transcript should switch me to that thread please". So a link whose destination is a Global Ref is a `session` reference, and the one-line cards of messages between agents (a message from another agent or a thread report, a message sent with `ghostex agents send`, `agents create --task` or `orchestrator start-thread`) carry a `pill` for the other session: the agent's logo and `+<session title>`, the title the host's session list shows now, else the text the link or the message carried. A click switches to that session as its sidebar row would (`openAgentSession`); a session the list no longer has draws the pill muted and the click does nothing. What the agents read is unchanged: this is only how the chat draws it. Typing `+` at the start of the draft or after a space opens a picker of the agent sessions in this chat's project, newest first and filtered as you type, the chat's own session left out, and a pick writes the same mention plus a space.
//! SEE-ALSO: packages/gx-protocol/src/session_ref.rs (the ref and the mention), sent_message.rs (names each pill from the host's session list), agent_message_card.rs (the cards' `pill`), composer/trigger.rs and composer/suggestions.rs (the `+` picker), apps/desktop/src/app/native_chat/markdown_links.rs and inter_agent_message.rs (desktop and web), apps/mobile/app/src/chat/native/transcript/AgentMessageCards.tsx and the phone's markdown links (phone).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::event::KnownSession;
use crate::transcript::agent_message_card::{agent_icon, GO_TO_SESSION};
use crate::transcript::jsstr::js_trim;

/// The `kind` of a mention's reference entry.
pub const SESSION_REFERENCE_KIND: &str = "session";

/// The tooltip of a pill whose session the host's list no longer has.
pub const SESSION_GONE: &str = "This session is gone";

/// What a pill reads: the session's title.
///
/// CDXC:SessionChat 2026-10-28 DECISION: User, on a `+Incoming Issues…` pill in the chat box: "please remove the + in this case", and of the "Message from +Mr Big Chungus" card's pill: "make this pill look like [the chat box's] after removing the +". A session pill reads the title alone, in the chat box, in a message and on a card; `+` stays only the key that opens the picker. Supersedes the `+<title>` of the decision above.
pub fn pill_label(title: &str) -> String {
    js_trim(title).to_string()
}

/// A Markdown link to a Global Ref as a `session` reference, named by its link text until the
/// host's session list names it ([`dress_session_reference`]).
pub fn session_reference(href: &str, label: &str) -> Option<Value> {
    let parsed = ghostex_gx_protocol::parse_global_session_ref(js_trim(href))?;
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    entry.insert("label".to_string(), pill_label(label).into());
    entry.insert("title".to_string(), GO_TO_SESSION.into());
    entry.insert("kind".to_string(), SESSION_REFERENCE_KIND.into());
    entry.insert("projectId".to_string(), parsed.project_id.into());
    entry.insert("sessionId".to_string(), parsed.session_id.into());
    entry.insert("agentIcon".to_string(), Value::Null);
    entry.insert("coordinator".to_string(), false.into());
    entry.insert("missing".to_string(), false.into());
    Some(Value::Object(entry))
}

/// Names one `session` reference from the host's list: its title and agent now, or muted when a
/// loaded list no longer has it. An empty list (still loading) leaves the link's own text.
pub fn dress_session_reference(
    reference: &mut Value,
    found: Option<&KnownSession>,
    list_loaded: bool,
) {
    let source = reference["sourceLabel"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let missing = list_loaded && found.is_none();
    let title = found
        .map(|session| js_trim(&session.title))
        .filter(|title| !title.is_empty())
        .unwrap_or(&source)
        .to_string();
    reference["label"] = pill_label(&title).into();
    reference["title"] = if missing { SESSION_GONE } else { GO_TO_SESSION }.into();
    reference["missing"] = missing.into();
    if let Some(session) = found {
        reference["agentIcon"] = agent_icon(&session.agent_id, &session.agent_name);
        reference["coordinator"] = session.coordinator.into();
        reference["projectId"] = session.project_id.clone().into();
        reference["sessionId"] = session.session_id.clone().into();
    }
}

/// The pill a card of a message between agents draws for the other session after `prefix`
/// (`Message from`, `Message to`). `session` is the card's jump target (`projectId`, `sessionId`).
///
/// CDXC:SessionChat 2026-10-28 DECISION:
/// User, on a "Message from +<title>" card whose tile and pill both drew the Claude logo: "let's not show the icon of the agent here twice pls". Every card pill sits right beside the card's own agent tile (the row's, or a listed recipient's), so it is `logo: false` and draws only its frame and `+<title>`; a mention pill in a message's text has no tile next to it and keeps its logo.
pub fn card_pill(
    prefix: &str,
    title: &str,
    agent_icon: Value,
    coordinator: bool,
    session: &Value,
    missing: bool,
) -> Value {
    json!({
        "prefix": prefix,
        "label": pill_label(title),
        // The card's tile beside it already shows the agent's logo.
        "logo": false,
        "agentIcon": agent_icon,
        "coordinator": coordinator,
        "missing": missing,
        "title": if missing { SESSION_GONE } else { GO_TO_SESSION },
        "session": if missing { Value::Null } else { session.clone() },
    })
}

/// One row of the `+` picker.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChoice {
    pub title: String,
    pub global_ref: String,
    /// The artwork id the renderers draw the logo from.
    pub agent_icon: Option<String>,
    pub agent_name: String,
    pub coordinator: bool,
    pub working: bool,
    /// `Working`, `Needs you`, `Idle` or `Sleeping`.
    pub state: String,
    pub active_at: String,
}

/// The agent sessions of `project_id` the `+` picker offers, newest first: every session with an
/// agent and a Global Ref, except `session_id` (the chat's own).
pub fn session_choices(
    sessions: &[KnownSession],
    project_id: &str,
    session_id: &str,
) -> Vec<SessionChoice> {
    let mut choices: Vec<SessionChoice> = sessions
        .iter()
        .filter(|session| {
            session.project_id == project_id
                && session.session_id != session_id
                && !session.global_ref.is_empty()
                && !session.agent_id.is_empty()
                && !matches!(session.agent_id.as_str(), "browser" | "terminal")
                && session.lifecycle_state != "stopped"
        })
        .map(|session| {
            let working = session.activity == "working";
            let state = if session.lifecycle_state == "sleeping" {
                "Sleeping"
            } else if working {
                "Working"
            } else if session.activity == "attention" {
                "Needs you"
            } else {
                "Idle"
            };
            let title = js_trim(&session.title);
            // A custom agent the catalog does not know reads by its state alone, never its raw id.
            let agent_name = crate::transcript::agent_message_card::agent_display_name(
                &session.agent_id,
                &session.agent_name,
            );
            let agent_name = if agent_name.is_empty()
                || (agent_name == session.agent_id
                    && crate::extras::agents::default_agent_by_id(Some(&session.agent_id))
                        .is_none())
            {
                String::new()
            } else {
                agent_name
            };
            SessionChoice {
                title: if title.is_empty() { "Session" } else { title }.to_string(),
                global_ref: session.global_ref.clone(),
                agent_icon: agent_icon(&session.agent_id, &session.agent_name)
                    .as_str()
                    .map(str::to_string),
                agent_name,
                coordinator: session.coordinator,
                working,
                state: state.to_string(),
                active_at: session.active_at.clone(),
            }
        })
        .collect();
    // ISO times sort as text; the newest first, ties in the list's own order.
    choices.sort_by(|a, b| b.active_at.cmp(&a.active_at));
    choices
}

/// The choices whose title has every word of the query, titles that start with it first.
pub fn filter_session_choices(choices: &[SessionChoice], query: &str) -> Vec<SessionChoice> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return choices.to_vec();
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut matched: Vec<(bool, &SessionChoice)> = choices
        .iter()
        .filter_map(|choice| {
            let title = choice.title.to_lowercase();
            words
                .iter()
                .all(|word| title.contains(word))
                .then(|| (title.starts_with(&query), choice))
        })
        .collect();
    matched.sort_by_key(|(prefix, _)| !*prefix);
    matched
        .into_iter()
        .map(|(_, choice)| choice.clone())
        .collect()
}

/// The mention a pick writes (without the space the picker adds after it).
pub fn choice_mention(choice: &SessionChoice) -> String {
    ghostex_gx_protocol::session_mention(&choice.title, &choice.global_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(session_id: &str, title: &str, active_at: &str) -> KnownSession {
        KnownSession {
            global_ref: format!("S90:P3lv0:{session_id}"),
            project_id: "P3lv0".to_string(),
            session_id: session_id.to_string(),
            title: title.to_string(),
            agent_id: "claude".to_string(),
            agent_name: "Claude".to_string(),
            activity: "idle".to_string(),
            lifecycle_state: "running".to_string(),
            active_at: active_at.to_string(),
            ..KnownSession::default()
        }
    }

    #[test]
    fn a_link_to_a_global_ref_is_a_session_reference() {
        let reference = session_reference("S90:P3lv0:G7hgi", "Model inventory").expect("a ref");
        assert_eq!(reference["kind"], "session");
        assert_eq!(reference["label"], "Model inventory");
        assert_eq!(reference["projectId"], "P3lv0");
        assert_eq!(reference["sessionId"], "G7hgi");
        assert!(session_reference("https://example.com", "x").is_none());
        assert!(session_reference("src/main.rs", "main").is_none());
    }

    #[test]
    fn the_host_list_names_the_pill_or_mutes_it() {
        let mut reference = session_reference("S90:P3lv0:G7hgi", "Old title").unwrap();
        let session = known("G7hgi", "Renamed title", "");
        dress_session_reference(&mut reference, Some(&session), true);
        assert_eq!(reference["label"], "Renamed title");
        assert_eq!(reference["agentIcon"], "claude");
        assert_eq!(reference["missing"], false);

        let mut gone = session_reference("S90:P3lv0:G0000", "Closed one").unwrap();
        dress_session_reference(&mut gone, None, true);
        assert_eq!(
            (gone["label"].as_str(), gone["missing"].as_bool()),
            (Some("Closed one"), Some(true))
        );

        // A list still loading does not call every pill gone.
        let mut loading = session_reference("S90:P3lv0:G0000", "Closed one").unwrap();
        dress_session_reference(&mut loading, None, false);
        assert_eq!(loading["missing"], false);
    }

    #[test]
    fn the_picker_lists_this_projects_agents_newest_first_without_this_chat() {
        let mut other_project = known("G9", "Elsewhere", "2026-10-28T09:00:00Z");
        other_project.project_id = "P2".to_string();
        let mut terminal = known("G8", "Shell", "2026-10-28T09:00:00Z");
        terminal.agent_id = "terminal".to_string();
        let mut working = known("G2", "Fix login", "2026-10-28T08:00:00Z");
        working.activity = "working".to_string();
        let sessions = vec![
            known("G1", "Older", "2026-10-27T08:00:00Z"),
            working,
            known("Gme", "This chat", "2026-10-28T10:00:00Z"),
            other_project,
            terminal,
        ];
        let choices = session_choices(&sessions, "P3lv0", "Gme");
        let titles: Vec<&str> = choices.iter().map(|choice| choice.title.as_str()).collect();
        assert_eq!(titles, vec!["Fix login", "Older"]);
        assert_eq!(
            (choices[0].state.as_str(), choices[0].working),
            ("Working", true)
        );
        assert_eq!(choice_mention(&choices[0]), "[Fix login](S90:P3lv0:G2)");
    }

    #[test]
    fn the_picker_filters_by_title_words_prefix_first() {
        let sessions = vec![
            known("G1", "Review the login fix", "3"),
            known("G2", "Login page polish", "2"),
            known("G3", "Sidebar", "1"),
        ];
        let choices = session_choices(&sessions, "P3lv0", "Gme");
        let titles = |query: &str| -> Vec<String> {
            filter_session_choices(&choices, query)
                .into_iter()
                .map(|choice| choice.title)
                .collect()
        };
        assert_eq!(
            titles("login"),
            vec!["Login page polish", "Review the login fix"]
        );
        assert_eq!(titles("fix login"), vec!["Review the login fix"]);
        assert_eq!(
            titles(""),
            vec!["Review the login fix", "Login page polish", "Sidebar"]
        );
    }
}
