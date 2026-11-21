//! Ask Ghostex AI's welcome card: what its empty chat shows instead of the agent mark, the
//! headline, the tips card and Try a Boost. A new user gets First steps, ticked off from Ghostex's
//! own state; once they have a project and have started an agent, a live status line and prompts
//! about their work and about Ghostex. Every prompt only fills the chat box.
//!
//! gxserver sends the state as `coordinatorThreads.chiefWelcome` on Ask Ghostex AI's own chat
//! (server/src/chief/welcome.rs): `projects`, `signedIn`, `agentStarted`, `phone`, `running`,
//! `runningProjects`, `needsYou` and `asked` (the prompts below already sent to it).
//!
//! CDXC:Chief 2026-11-21 DECISION:
//! User (relayed by Chief of Staff, the approved spec cos-welcome-card-v3): a new welcome card for Ask Ghostex AI's empty chat only (other chats keep today's welcome): the clipboard ghost, "Welcome to Ghostex!", a First steps card (add a project, sign in to Claude or Codex, start an agent, put Ghostex on your phone; each ticks itself off from real state), a white "Ask me anything about Ghostex" button and four chips; once there is a project and an agent has been started, "Hi again!" with a live status line, the ask button, two work prompts and two teaching prompts. Tapping a prompt only fills the chat box, never sends. The spec's "I'm your Chief of Staff" line becomes "I'm Ghostex AI. Ask me anything, or let's get you set up."
//! SEE-ALSO: apps/desktop/src/app/native_chat/chief_welcome.rs and apps/mobile/app/src/chat/native/cards/ChiefWelcome.tsx (the two cards), server/src/chief/welcome.rs (the state).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A prompt the card offers: its label, an icon (`apps/desktop/assets/titlebar/<icon>.svg`) and
/// the text a tap puts in the chat box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomePrompt {
    pub label: String,
    pub icon: String,
    pub fill: String,
}

/// A First steps row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomeStep {
    pub label: String,
    pub fill: String,
    pub done: bool,
}

/// The set-up card's status pill.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomeStatus {
    /// "4 agents running in 2 projects", or "All quiet. No agents running."
    pub text: String,
    /// "1 needs you", drawn bold after a dot; `null` when nothing does.
    pub needs: Option<String>,
    /// The dot is amber while something needs the user, grey otherwise.
    pub attention: bool,
    pub fill: String,
}

/// The card, as both renderers draw it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChiefWelcome {
    /// `newUser` or `setUp`.
    pub card: String,
    pub title: String,
    /// The line under the title (new user only).
    pub line: Option<String>,
    /// First steps (new user only), in their fixed order.
    pub steps: Vec<WelcomeStep>,
    pub ask_title: String,
    pub ask_detail: String,
    /// The small asks under the ask button (new user only).
    pub chips: Vec<WelcomePrompt>,
    /// The live status line (set up only).
    pub status: Option<WelcomeStatus>,
    /// Two work prompts, then two teaching prompts (set up only).
    pub prompts: Vec<WelcomePrompt>,
}

/// What the chat box says in Ask Ghostex AI's chat.
pub const PLACEHOLDER: &str = "Ask me anything about Ghostex";

const ADD_PROJECT: &str = "Help me add my first project";
const SIGN_IN: &str = "Help me sign in to Claude or Codex";
const START_AGENT: &str = "Help me start my first agent";
const PHONE: &str = "Help me put Ghostex on my phone";
const SET_UP: &str = "Help me set up Ghostex";
const WHAT_CAN: &str = "What can Ghostex do?";
const COORDINATORS: &str = "How do coordinators work?";
const LOOKS: &str = "Change how Ghostex looks";
const GOING_ON: &str = "What's going on right now?";
const STUCK: &str = "Which agents are stuck?";
const CATCH_UP: &str = "Catch me up on a project";
const NEEDS_ME: &str = "What needs me?";

/// Every text a tap can put in the chat box; gxserver remembers which of them the user sent, so a
/// teaching prompt already asked is not offered again.
pub const PROMPT_FILLS: &[&str] = &[
    ADD_PROJECT,
    SIGN_IN,
    START_AGENT,
    PHONE,
    SET_UP,
    WHAT_CAN,
    COORDINATORS,
    LOOKS,
    GOING_ON,
    STUCK,
    CATCH_UP,
    NEEDS_ME,
];

/// Whether `text`, as sent, is one of the card's prompts (trimmed, exact).
pub fn is_welcome_prompt(text: &str) -> bool {
    PROMPT_FILLS.contains(&text.trim())
}

fn prompt(label: &str, icon: &str, fill: &str) -> WelcomePrompt {
    WelcomePrompt {
        label: label.to_string(),
        icon: icon.to_string(),
        fill: fill.to_string(),
    }
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The card for Ask Ghostex AI's empty chat, from gxserver's `chiefWelcome` state.
pub fn build(data: &Value) -> ChiefWelcome {
    let flag = |key: &str| data.get(key).and_then(Value::as_bool) == Some(true);
    let count = |key: &str| data.get(key).and_then(Value::as_u64).unwrap_or(0);
    let asked: Vec<&str> = data
        .get("asked")
        .and_then(Value::as_array)
        .map(|asked| asked.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let (project, signed_in, agent, phone) = (
        count("projects") > 0,
        flag("signedIn"),
        flag("agentStarted"),
        flag("phone"),
    );
    if !(project && agent) {
        let step = |label: &str, fill: &str, done: bool| WelcomeStep {
            label: label.to_string(),
            fill: fill.to_string(),
            done,
        };
        return ChiefWelcome {
            card: "newUser".to_string(),
            title: "Welcome to Ghostex!".to_string(),
            line: Some("I'm Ghostex AI. Ask me anything, or let's get you set up.".to_string()),
            steps: vec![
                step("Add your first project", ADD_PROJECT, project),
                step("Sign in to Claude or Codex", SIGN_IN, signed_in),
                step("Start your first agent", START_AGENT, agent),
                step("Put Ghostex on your phone", PHONE, phone),
            ],
            ask_title: "Ask me anything about Ghostex".to_string(),
            ask_detail: "How it works, settings, agents, projects".to_string(),
            chips: vec![
                prompt(SET_UP, "settings", SET_UP),
                prompt(WHAT_CAN, "sparkles", WHAT_CAN),
                prompt(COORDINATORS, "coordinator-crown-points", COORDINATORS),
                prompt(LOOKS, "palette", LOOKS),
            ],
            status: None,
            prompts: Vec::new(),
        };
    }
    let running = count("running");
    let needs = count("needsYou");
    let status = WelcomeStatus {
        text: if running == 0 {
            "All quiet. No agents running.".to_string()
        } else {
            format!(
                "{} running in {}",
                plural(running, "agent", "agents"),
                plural(count("runningProjects").max(1), "project", "projects")
            )
        },
        needs: (needs > 0).then(|| format!("{needs} needs you")),
        attention: needs > 0,
        fill: if needs > 0 { NEEDS_ME } else { GOING_ON }.to_string(),
    };
    let not_asked = |fill: &str| !asked.contains(&fill);
    let mut teaching: Vec<WelcomePrompt> = Vec::new();
    for (open, label, icon, fill) in [
        (!signed_in, "Sign in to Claude or Codex", "check", SIGN_IN),
        (!phone, "Put Ghostex on my phone", "devices", PHONE),
        (true, LOOKS, "palette", LOOKS),
        (true, COORDINATORS, "coordinator-crown-points", COORDINATORS),
        (true, WHAT_CAN, "sparkles", WHAT_CAN),
    ] {
        if open && not_asked(fill) && teaching.len() < 2 {
            teaching.push(prompt(label, icon, fill));
        }
    }
    let mut prompts = vec![
        prompt(GOING_ON, "layout-dashboard", GOING_ON),
        prompt(STUCK, "alert-circle", STUCK),
    ];
    // With fewer than two teaching prompts left, the work prompts make up the four.
    let extra = [
        prompt(CATCH_UP, "message-circle", CATCH_UP),
        prompt(NEEDS_ME, "alert-circle", NEEDS_ME),
    ];
    prompts.extend(extra.into_iter().take(2 - teaching.len()));
    prompts.extend(teaching);
    ChiefWelcome {
        card: "setUp".to_string(),
        title: "Hi again!".to_string(),
        line: None,
        steps: Vec::new(),
        ask_title: "Ask me anything".to_string(),
        ask_detail: "About Ghostex, or any agent, project or computer".to_string(),
        chips: Vec::new(),
        status: Some(status),
        prompts,
    }
}
