//! Another session named in a chat: a mention link (`[<title>](<Global Ref>)`, from Copy ID or the
//! composer's `+` picker) and the other session of a message between agents, both drawn as that
//! session's agent pill: its logo and `+<title>`, a click switches to it, muted when it is gone.
//! Also the `+` picker's list: the project's agent sessions, then every other reachable one by
//! computer.
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
    /// Its project's main coordinator on its computer: the row's crown is gold.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub main_coordinator: bool,
    pub working: bool,
    /// `Working`, `Needs you`, `Idle` or `Sleeping`.
    pub state: String,
    pub active_at: String,
    /// The computer's name, for a session outside the chat's project; empty for one in it. The
    /// query matches it; the row's section heading shows it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub computer: String,
    /// The project's name, for a session outside the chat's project.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// The heading of the part of the list the row sits in: [`HERE_SECTION`], a computer's name,
    /// or [`OTHER_SECTION`].
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub section: String,
}

/// The `+` list's first part: the other agents of the chat's own project on its computer.
pub const HERE_SECTION: &str = "Agents in this project";

/// The `+` list's part for sessions whose computer the host does not name.
pub const OTHER_SECTION: &str = "Other agents";

/// The server part of a Global Ref (`S90` of `S90:P3lv0:G7hgi`); empty for anything else.
fn server_of(global_ref: &str) -> &str {
    ghostex_gx_protocol::parse_global_session_ref(global_ref)
        .map(|parsed| parsed.server_id)
        .unwrap_or_default()
}

/// The gxserver the chat's own session runs on, read off the host's list: its own row, else a row
/// of its project. Empty when the list has neither.
pub(crate) fn chat_server<'a>(
    sessions: &'a [KnownSession],
    project_id: &str,
    session_id: &str,
) -> &'a str {
    let own = sessions
        .iter()
        .find(|session| session.project_id == project_id && session.session_id == session_id);
    own.into_iter()
        .chain(
            sessions
                .iter()
                .filter(|session| session.project_id == project_id),
        )
        .map(|session| server_of(&session.global_ref))
        .find(|server| !server.is_empty())
        .unwrap_or_default()
}

/// The agent sessions the `+` picker offers, in sections: the other sessions of `project_id` on the
/// chat's own computer ([`HERE_SECTION`]), then the chat's computer's other projects under its
/// name, then each other connected computer under its name (by name), then the sessions of a
/// computer the host does not name ([`OTHER_SECTION`]); each section newest first. `session_id`
/// (the chat's own) and a computer that is not connected now are left out. Nothing is capped: the
/// popup scrolls.
///
/// CDXC:SessionChat 2026-11-19 DECISION:
/// User, in a chat of another project whose `+` list lacked their Ghostex coordinator on the same computer: "why this session can't mention you ( +Big Chungus )??". Every agent session the user can reach is offered: this computer's other projects too (the 2026-11-05 rule below had left them out), and every connected computer's, whatever the project. The list is honest about where each one is: this project's agents first under "Agents in this project", then one section per computer headed by its name (the chat's own computer first; CDXC:RemoteMachines 2026-11-19: never "This computer"), each row saying its project; "Other agents" when the host does not name the computer. Typing filters every section by title, project and computer, and keeps the sections. Supersedes the 2026-11-05 "this computer's other projects stay out" and the single "Agents in this project" heading over sessions of other projects.
/// CDXC:SessionChat 2026-11-05 DECISION:
/// User: "also please let me mention threads from other machines that are connected when i use + thingy". A computer's project ids are its own gxserver's, so the project filter dropped every session on another computer, though the host's list already held them. Now a session whose Global Ref names another gxserver than the chat's is offered whatever its project, as long as its computer is connected (the sidebar's test: `offline` comes from the host), and its row says its project (`suggestions.rs`); the query also matches the computer and the project. The pick writes the same mention with the full ref, which `ghostex agents send` and `read-session-chat` route to that computer (server/src/ghostex_cli/computers.rs `ref_target`).
pub fn session_choices(
    sessions: &[KnownSession],
    project_id: &str,
    session_id: &str,
) -> Vec<SessionChoice> {
    let own_server = chat_server(sessions, project_id, session_id);
    let mut seen = std::collections::HashSet::new();
    // (section rank, computer name for ordering, choice)
    let mut choices: Vec<((u8, String), SessionChoice)> = sessions
        .iter()
        .filter_map(|session| {
            let server = server_of(&session.global_ref);
            let elsewhere = !own_server.is_empty() && !server.is_empty() && server != own_server;
            let here = !elsewhere && session.project_id == project_id;
            let offered = !session.global_ref.is_empty()
                && !session.agent_id.is_empty()
                && !matches!(session.agent_id.as_str(), "browser" | "terminal")
                && session.lifecycle_state != "stopped"
                && !session.offline
                && !(here && session.session_id == session_id);
            // A computer the host lists twice (a saved machine that is this one) offers each once.
            if !(offered && seen.insert(session.global_ref.as_str())) {
                return None;
            }
            let mut choice = session_choice(session, elsewhere);
            let computer = js_trim(&session.computer);
            let rank = if here {
                choice.project_name.clear();
                choice.section = HERE_SECTION.to_string();
                0
            } else {
                choice.project_name = js_trim(&session.project_name).to_string();
                choice.computer = computer.to_string();
                choice.section = if computer.is_empty() {
                    OTHER_SECTION
                } else {
                    computer
                }
                .to_string();
                match (computer.is_empty(), elsewhere) {
                    (true, _) => 3,
                    (false, false) => 1,
                    (false, true) => 2,
                }
            };
            Some(((rank, computer.to_lowercase()), choice))
        })
        .collect();
    // Section by section; ISO times sort as text, the newest first, ties in the list's order.
    choices.sort_by(|(a_key, a), (b_key, b)| {
        a_key.cmp(b_key).then_with(|| b.active_at.cmp(&a.active_at))
    });
    choices.into_iter().map(|(_, choice)| choice).collect()
}

fn session_choice(session: &KnownSession, elsewhere: bool) -> SessionChoice {
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
            && crate::extras::agents::default_agent_by_id(Some(&session.agent_id)).is_none())
    {
        String::new()
    } else {
        agent_name
    };
    let (computer, project_name) = if elsewhere {
        let computer = js_trim(&session.computer);
        (
            if computer.is_empty() {
                "Another computer"
            } else {
                computer
            }
            .to_string(),
            js_trim(&session.project_name).to_string(),
        )
    } else {
        (String::new(), String::new())
    };
    SessionChoice {
        title: if title.is_empty() { "Session" } else { title }.to_string(),
        global_ref: session.global_ref.clone(),
        agent_icon: agent_icon(&session.agent_id, &session.agent_name)
            .as_str()
            .map(str::to_string),
        agent_name,
        coordinator: session.coordinator,
        main_coordinator: session.coordinator && session.main_coordinator,
        working,
        state: state.to_string(),
        active_at: session.active_at.clone(),
        computer,
        project_name,
        section: String::new(),
    }
}

/// What a session pill shows beside its title (the computer icon) and what its hover card
/// says (long press on the phone), read live from the host's list at draw time, never from the
/// settled transcript, whose sessions' activity changes would rebuild it (`KnownSession::same_names`).
///
/// CDXC:SessionChat 2026-11-13 DECISION:
/// User, answering the session-mention design page: "the rich pill, in both the chat box and the sent message. But for a session on another computer, show only that computer's icon, not its whole name"; "hovering a session mention shows the hover card (long press on the phone)"; and "the same look spreads to message cards and the Workers popover, where they name a session". A session pill is the framed pill with the agent's logo, its title, a dot for its state (working, needs you, idle) and, for a session on another computer than the chat's home one, that computer's icon alone. Its hover card names the session, its state, agent, project and computer.
/// CDXC:SessionChat 2026-11-15 DECISION (superseded 2026-11-16): a mentioned session was its title with a faint dotted underline, no frame or fill, a state dot while working (amber) or needing you (red), then the computer icon.
/// CDXC:SessionChat 2026-11-16 DECISION: User, on the session-tag design page (option A, "Soft chip"), with notes: "put the computer icon to the LEFT of the agent icon", "don't show the status dot on session tags at all", and always a space before and after every tag (a tag at the start of a message, or typed against text in the chat box, stuck to the next word: "⊞why did u use..."). A mentioned session, in a message and in the chat box, is a soft chip: a faint fill with rounded corners and no border, then, for a session on another computer, that computer's icon, then the agent's logo, then the title, cut with an ellipsis past about 30 characters (the full title is in the hover card). No state dot anywhere a session is named, and no underline on the chip. The chip keeps clear room on both sides even when the text has none, and the chat box inserts a real space after a picked mention and before it when the character before is not a space (desktop composer_references.rs `session_replacement` and markdown_links.rs `session_pill_link`, phone SessionPill.tsx and ComposerInput.tsx).
/// SEE-ALSO: apps/desktop/src/app/native_chat/markdown_links.rs and session_card.rs (desktop and web), apps/mobile/app/src/chat/native/transcript/SessionPill.tsx (phone, which reads the same rule from its own session list).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionCard {
    pub title: String,
    pub agent_icon: Option<String>,
    pub agent_name: String,
    pub coordinator: bool,
    pub project_name: String,
    /// The computer's name, as the host's machine tab spells it.
    pub computer: String,
    /// The session runs on another computer than the home one: its pill draws the computer icon.
    pub elsewhere: bool,
    /// The gxserver the session runs on (`S1u` of `S1u:P2alb:G5a3u`), which the renderers draw
    /// that computer's own icon for; empty when its ref names none.
    pub server_id: String,
    /// `Working`, `Needs you`, `Idle` or `Sleeping`.
    pub state: String,
}

/// The hover card's footer.
pub const MENTION_CARD_HINT: &str = "Click to open";

/// The session `global_ref` names in the host's list: the same ref, else (a list whose rows carry no
/// server part, or a ref that omits it) the same project and session.
pub fn find_known_session<'a>(
    sessions: &'a [KnownSession],
    global_ref: &str,
) -> Option<&'a KnownSession> {
    let target = ghostex_gx_protocol::parse_global_session_ref(js_trim(global_ref))?;
    sessions
        .iter()
        .find(|session| session.global_ref == js_trim(global_ref))
        .or_else(|| {
            sessions.iter().find(|session| {
                session.project_id == target.project_id
                    && session.session_id == target.session_id
                    && (target.server_id.is_empty()
                        || session.global_ref.is_empty()
                        || server_of(&session.global_ref) == target.server_id)
            })
        })
}

/// The pill's extras and card for `global_ref`, or `None` when the list does not have it.
/// `home_server` is the gxserver the drawing side counts as here (the desktop's own, the phone's
/// chat's computer); a session whose ref names another one is `elsewhere`. Empty knows no home, and
/// nothing is elsewhere then.
pub fn mention_card(
    sessions: &[KnownSession],
    global_ref: &str,
    home_server: &str,
) -> Option<MentionCard> {
    let session = find_known_session(sessions, global_ref)?;
    let server = server_of(&session.global_ref);
    let elsewhere = !home_server.is_empty() && !server.is_empty() && server != home_server;
    let choice = session_choice(session, elsewhere);
    Some(MentionCard {
        title: choice.title,
        agent_icon: choice.agent_icon,
        agent_name: choice.agent_name,
        coordinator: session.coordinator,
        project_name: js_trim(&session.project_name).to_string(),
        computer: js_trim(&session.computer).to_string(),
        elsewhere,
        server_id: server.to_string(),
        state: choice.state,
    })
}

/// The card's second line: agent, project and computer, by its name wherever it is (CDXC:RemoteMachines
/// 2026-11-19 in extras/where_pill.rs: never "This computer").
pub fn mention_card_meta(card: &MentionCard) -> String {
    let computer = if card.elsewhere && card.computer.is_empty() {
        "Another computer"
    } else {
        card.computer.as_str()
    };
    [
        card.agent_name.as_str(),
        card.project_name.as_str(),
        computer,
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// The choices that have every word of the query in their title, project or computer, section by
/// section in the list's order, titles that start with the query first within each.
pub fn filter_session_choices(choices: &[SessionChoice], query: &str) -> Vec<SessionChoice> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return choices.to_vec();
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut sections: Vec<&str> = Vec::new();
    let mut matched: Vec<(usize, bool, &SessionChoice)> = choices
        .iter()
        .filter_map(|choice| {
            let title = choice.title.to_lowercase();
            let haystack = format!(
                "{title} {} {}",
                choice.project_name.to_lowercase(),
                choice.computer.to_lowercase()
            );
            if !words.iter().all(|word| haystack.contains(word)) {
                return None;
            }
            let section = match sections.iter().position(|seen| *seen == choice.section) {
                Some(index) => index,
                None => {
                    sections.push(&choice.section);
                    sections.len() - 1
                }
            };
            Some((section, !title.starts_with(&query), choice))
        })
        .collect();
    matched.sort_by_key(|(section, later, _)| (*section, *later));
    matched
        .into_iter()
        .map(|(_, _, choice)| choice.clone())
        .collect()
}

/// What a `+` row says under its title: its project when outside the chat's, then its agent and
/// state. The computer is its section's heading, so the row does not repeat it.
///
/// CDXC:Coordinators 2026-11-12 DECISION:
/// User: "please dont' show Main for main coordinators, instead make the color of their icon gold", then "Yes, both" to the chat's `+` list following the sidebar. A main coordinator's row no longer starts its detail with "Main"; the row's `mainCoordinator` draws its crown gold instead (desktop native_chat/suggestions/render.rs, phone composer/Suggestions.tsx), in the sidebar's golds. This supersedes the "Main" word of 2026-11-10.
pub fn choice_detail(choice: &SessionChoice) -> String {
    [
        choice.project_name.as_str(),
        choice.agent_name.as_str(),
        choice.state.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
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
        let mut terminal = known("G8", "Shell", "2026-10-28T09:00:00Z");
        terminal.agent_id = "terminal".to_string();
        let mut working = known("G2", "Fix login", "2026-10-28T08:00:00Z");
        working.activity = "working".to_string();
        let sessions = vec![
            known("G1", "Older", "2026-10-27T08:00:00Z"),
            working,
            known("Gme", "This chat", "2026-10-28T10:00:00Z"),
            terminal,
        ];
        let choices = session_choices(&sessions, "P3lv0", "Gme");
        let titles: Vec<&str> = choices.iter().map(|choice| choice.title.as_str()).collect();
        assert_eq!(titles, vec!["Fix login", "Older"]);
        assert_eq!(choices[0].section, HERE_SECTION);
        assert_eq!(
            (choices[0].state.as_str(), choices[0].working),
            ("Working", true)
        );
        assert_eq!(choice_mention(&choices[0]), "[Fix login](S90:P3lv0:G2)");
    }

    /// A session on another computer's gxserver (`S8g`), in its own project there.
    fn remote(session_id: &str, title: &str, active_at: &str) -> KnownSession {
        KnownSession {
            global_ref: format!("S8g:P5zq6:{session_id}"),
            project_id: "P5zq6".to_string(),
            computer: "Omarchy".to_string(),
            project_name: "dotfiles".to_string(),
            ..known(session_id, title, active_at)
        }
    }

    /// A session on the chat's own computer (`S90`, named `Studio`) in `project`.
    fn local(project: &str, name: &str, session_id: &str, title: &str, at: &str) -> KnownSession {
        KnownSession {
            global_ref: format!("S90:{project}:{session_id}"),
            project_id: project.to_string(),
            computer: "Studio".to_string(),
            project_name: name.to_string(),
            ..known(session_id, title, at)
        }
    }

    #[test]
    fn the_picker_offers_every_reachable_agent_by_computer_after_this_project() {
        let mut offline = remote("G5off", "Offline one", "2026-10-28T11:00:00Z");
        offline.offline = true;
        offline.global_ref = "S7a:P2:G5off".to_string();
        let mut stopped = remote("G5stp", "Stopped one", "2026-10-28T11:00:00Z");
        stopped.lifecycle_state = "stopped".to_string();
        // The chat is "SP Coordinator" in shortpoint; Big Chungus coordinates Ghostex on the
        // same computer, the case the 2026-11-05 list left out.
        let mut chungus = local(
            "P3lv0",
            "Ghostex",
            "G9jt7",
            "Big Chungus",
            "2026-10-28T12:00:00Z",
        );
        chungus.coordinator = true;
        chungus.main_coordinator = true;
        let mut windows = remote("G7win", "Win Chungus", "2026-10-28T13:00:00Z");
        windows.global_ref = "S3w:P1:G7win".to_string();
        windows.computer = "Win".to_string();
        windows.project_name = "Ghostex".to_string();
        let sessions = vec![
            local(
                "P5umy",
                "shortpoint",
                "Gme",
                "SP Coordinator",
                "2026-10-28T10:00:00Z",
            ),
            local(
                "P5umy",
                "shortpoint",
                "G1",
                "Local work",
                "2026-10-27T08:00:00Z",
            ),
            windows,
            remote("G4snt", "Remote build", "2026-10-28T11:30:00Z"),
            // The same computer saved as a machine of its own lists its rows twice.
            remote("G4snt", "Remote build", "2026-10-28T11:30:00Z"),
            offline,
            stopped,
            chungus,
        ];
        let choices = session_choices(&sessions, "P5umy", "Gme");
        let rows: Vec<(&str, &str, String)> = choices
            .iter()
            .map(|choice| {
                (
                    choice.section.as_str(),
                    choice.title.as_str(),
                    choice_detail(choice),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                (HERE_SECTION, "Local work", "Claude \u{b7} Idle".to_string()),
                (
                    "Studio",
                    "Big Chungus",
                    "Ghostex \u{b7} Claude \u{b7} Idle".to_string()
                ),
                (
                    "Omarchy",
                    "Remote build",
                    "dotfiles \u{b7} Claude \u{b7} Idle".to_string()
                ),
                (
                    "Win",
                    "Win Chungus",
                    "Ghostex \u{b7} Claude \u{b7} Idle".to_string()
                ),
            ]
        );
        assert!(choices[1].coordinator && choices[1].main_coordinator);
        // What the agent reads names the other computer's session by its full ref.
        assert_eq!(
            choice_mention(&choices[2]),
            "[Remote build](S8g:P5zq6:G4snt)"
        );
        // The query reaches every section by title, project and computer, and keeps the sections.
        let titles = |query: &str| -> Vec<String> {
            filter_session_choices(&choices, query)
                .into_iter()
                .map(|choice| choice.title)
                .collect()
        };
        assert_eq!(titles("Big"), vec!["Big Chungus"]);
        assert_eq!(titles("big chungus"), vec!["Big Chungus"]);
        assert_eq!(titles("chungus"), vec!["Big Chungus", "Win Chungus"]);
        assert_eq!(titles("omarchy"), vec!["Remote build"]);
        assert_eq!(titles("dotfiles build"), vec!["Remote build"]);
        assert_eq!(titles("ghostex"), vec!["Big Chungus", "Win Chungus"]);
        assert!(titles("omarchy local").is_empty());
    }

    #[test]
    fn without_the_chats_own_server_the_picker_still_offers_every_agent() {
        // A host whose list does not have the chat's row nor any row of its project.
        let sessions = vec![remote("G4snt", "Remote build", "2026-10-28T11:30:00Z")];
        let choices = session_choices(&sessions, "P3lv0", "Gme");
        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.section.as_str(), choice.title.as_str()))
                .collect::<Vec<_>>(),
            vec![("Omarchy", "Remote build")]
        );
        let mut unnamed = remote("G4snt", "Remote build", "2026-10-28T11:30:00Z");
        unnamed.computer = String::new();
        assert_eq!(
            session_choices(&[unnamed], "P3lv0", "Gme")[0].section,
            OTHER_SECTION
        );
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
