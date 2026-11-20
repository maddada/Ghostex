//! Boosts: one click in the chat box adds a short instruction to the next message, so the agent
//! answers with a Live page, a chart, a form, does it in a browser or an app, plans it with workers,
//! and so on, instead of only words.
//!
//! The Boost pill in the chat box's strip opens a menu of the Boosts in four tabs (Show, Do, Work
//! and Custom); picking one puts a Boost pill in the message (`[Live page](boost:live-page)`, a
//! reference pill of kind `boost`), and picking it again takes it out. `*` at the start of a word
//! opens the same Boosts as a picker above the chat box. gxserver adds each Boost's prompt under
//! the message on its way to the agent, inside a `<ghostex-system>` block every chat already hides
//! (server/src/boosts.rs `with_boosts`), so the bubble shows the user's words and the pill. A
//! Boost lasts that one message. Hovering a pill shows its prompt; a double click opens Settings >
//! Boosts, where every Boost's prompt is edited or reset and the user's own Boosts are made,
//! edited, ordered and deleted. gxserver keeps all of it per computer.
//!
//! CDXC:Boosts 2026-11-20 DECISION:
//! The user answered the Boosts design page (Q4213 to Q4218): the name is Boosts; the first ones
//! are Live page, Chart it, Side by side, Map it (on a Live page) and a new one, Triage requests
//! ("asks the agent to look at GitHub issues/PRs, Linear or Jira tickets etc. and use a Live page
//! to explain each of them well (what each one asks for as bullet points, visuals where possible,
//! a decision per item), like the coordinator's triage pages"); one Boost menu pill in the chat
//! box; a Boost lasts one message; agents learn how from the complete CLI help (`ghostex visual
//! guide`), not skills, so a Boost's prompt only points at it. Also: "when a Boost is in the chat
//! box, hovering it shows its prompt in the tooltip with 'Double click to edit prompt'".
//! CDXC:Boosts 2026-11-20 DECISION:
//! Then, on the menu and hint page (Q4257 to Q4261): the menu is a 280px list with every
//! description in full and a preview card beside it (A); the first-time hint is a one-time callout
//! bubble pointing at the Boost pill, nothing inside the chat box (C); the gallery holds Show, Do,
//! Work and Custom tabs over that list (A), Computer use and Browser use telling the agent to read
//! the `cua-driver` skill; `*` at the start of any word opens the Boost picker and a space right
//! after it closes it (A); the user's own Boosts come from "Save as Boost" on selected text and
//! the Custom tab (A). Plus "a new Settings page for Boosts, where editing happens (every Boost's
//! prompt, reset, custom Boosts: add/edit/delete/reorder, maybe turn Boosts on/off); the menu's
//! edit actions open it". Supersedes the prompt editor that opened inside the chat box and the
//! two-line hint inside it.
//! SEE-ALSO: server/src/boosts.rs (the prompts, the user's Boosts, `/api/boosts`, the block under
//! the message), apps/desktop/src/app/native_chat/boosts.rs (pill, menu, preview, callout, Try a
//! Boost), apps/desktop/src/app/window/settings_modal/tabs/boosts.rs (Settings > Boosts),
//! composer/trigger.rs and composer/suggestions.rs (`*`), composer/reference_pills.rs
//! (`ReferenceKind::Boost`), transcript/markdown_links.rs (the pill in a sent message), the
//! phone's apps/mobile/app/src/chat/native/cards/Boosts.tsx.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The scheme of a Boost pill's link: `[Live page](boost:live-page)`.
pub const BOOST_LINK_SCHEME: &str = "boost:";

/// One of Ghostex's Boosts: what the menu shows and the prompt the agent gets unless the user
/// edited it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boost {
    pub id: &'static str,
    pub label: &'static str,
    /// The icon, a path under the desktop's assets (`titlebar/<name>.svg`).
    pub icon: &'static str,
    /// Its tab: `show`, `do` or `work`.
    pub category: &'static str,
    /// One line on what the agent does, for the menu, the picker and the callout.
    pub description: &'static str,
    /// What the little preview beside the menu draws (`page`, `chart`, `compare`, `map`,
    /// `triage`, `form`, `app`, `browser`, `shots`, `second`, `workers`, `worktree`, `docs`,
    /// `steps`, `notify`).
    pub preview: &'static str,
    /// What the agent is told, unless the user edited it.
    pub prompt: &'static str,
}

/// The menu's tabs, in order, with what each one is called.
pub const TABS: [(&str, &str); 4] = [
    ("show", "Show"),
    ("do", "Do"),
    ("work", "Work"),
    ("custom", "Custom"),
];

/// Ghostex's Boosts, in the menu's order.
pub const BOOSTS: [Boost; 15] = [
    Boost {
        id: "live-page",
        label: "Live page",
        icon: "titlebar/browser.svg",
        category: "show",
        description: "The answer comes back as a page you open: mockups, comparisons, small tools.",
        preview: "page",
        prompt: "Answer with a Live page: one self-contained HTML page you publish with `ghostex show`, then paste the block it prints into your reply. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "chart",
        label: "Chart it",
        icon: "titlebar/chart-bar.svg",
        category: "show",
        description: "Numbers come back as charts, stat tiles and tables, not paragraphs.",
        preview: "chart",
        prompt: "Show the numbers in your answer as Ghostex visuals: charts, stat tiles and tables in a ```visual block, each with one plain sentence on what it shows. If you haven't in this conversation, run `ghostex visual guide visuals` first, and check the block with `ghostex visual check` before you reply.",
    },
    Boost {
        id: "compare",
        label: "Side by side",
        icon: "titlebar/layout-columns.svg",
        category: "show",
        description: "Two to four real options drawn next to each other, picked with a click.",
        preview: "compare",
        prompt: "Lay out two to four real options side by side on a Live page, each drawn as it would really look or work, with its trade-offs, and under them one question with your recommended pick so I can choose with a click. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "map",
        label: "Map it",
        icon: "titlebar/sitemap.svg",
        category: "show",
        description: "How the parts connect, drawn as a diagram on a Live page.",
        preview: "map",
        prompt: "Explain it as a map on a Live page: a diagram of the parts and how they connect (a Mermaid diagram works), with a short plain note on each part and on what flows between them. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "form",
        label: "Ask me in a form",
        icon: "titlebar/list-check.svg",
        category: "show",
        description: "Questions come back as a Ghostex form, so you answer with clicks.",
        preview: "form",
        prompt: "Before you start, ask me what you need to know as a Ghostex form (a ```questions block) with your recommended pick marked on each question, instead of questions in your text. If you haven't in this conversation, run `ghostex visual guide forms` first and follow it.",
    },
    Boost {
        id: "computer-use",
        label: "Computer use",
        icon: "titlebar/device-desktop.svg",
        category: "do",
        description: "The agent drives a desktop app for you with Cua Driver and shows what it did.",
        preview: "app",
        prompt: "Do this by driving the app yourself with computer use. Read the cua-driver skill first and follow it (if it isn't installed, `ghostex computer-use install-skill` adds Ghostex's copy), then show me what you did with a screenshot.",
    },
    Boost {
        id: "browser-use",
        label: "Browser use",
        icon: "titlebar/world.svg",
        category: "do",
        description: "The agent opens and drives pages in Ghostex's own browser: clicks, forms, screenshots.",
        preview: "browser",
        prompt: "Do this in a browser yourself. Read the cua-driver skill first and follow it (if it isn't installed, `ghostex browser-use install-skill` adds Ghostex's copy); open pages in Ghostex's browser with `ghostex browser open <url>`, and show me what you did with a screenshot.",
    },
    Boost {
        id: "screenshots",
        label: "Show screenshots",
        icon: "titlebar/photo.svg",
        category: "do",
        description: "Every change comes back with a screenshot of how it looks.",
        preview: "shots",
        prompt: "Show every visible change you make with a screenshot of how it looks now (and how it looked before, when you changed something that was there), linked in your reply as image file paths.",
    },
    Boost {
        id: "second-opinion",
        label: "Second opinion",
        icon: "titlebar/users.svg",
        category: "do",
        description: "Another agent, on another model, checks the answer before it reaches you.",
        preview: "second",
        prompt: "Before you answer, get a second opinion: start another agent on another model with `ghostex agents create` (see `ghostex agents --help`), have it check your work, and tell me what it found and what you changed because of it.",
    },
    Boost {
        id: "triage",
        label: "Triage requests",
        icon: "titlebar/checklist.svg",
        category: "work",
        description: "Issues, pull requests or tickets explained one by one, with a decision for each.",
        preview: "triage",
        prompt: "Triage the requests this message points at: GitHub issues and pull requests, Linear or Jira tickets, or whatever I name (if I named none, the open ones of this project). Read each one, then put them all on one Live page, one card per item: a link to it, what it asks for as short bullet points, a picture of it wherever one helps (a screenshot, a mockup or a small diagram), your recommendation, and a question with the choices for that item so I can decide each with a click. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "workers",
        label: "Plan with workers",
        icon: "titlebar/users-group.svg",
        category: "work",
        description: "A coordinator splits the job, runs workers on it and sends you one report.",
        preview: "workers",
        prompt: "Treat this as a bigger job for workers: unless this session already is a coordinator, make it one with `ghostex coordinator promote`, split the work into threads with `ghostex coordinator start-thread`, and send me one report when they finish. Read `ghostex coordinator --help` first.",
    },
    Boost {
        id: "worktree",
        label: "Try it in a worktree",
        icon: "titlebar/git-branch.svg",
        category: "work",
        description: "The agent works in its own worktree, so your folder stays as it is.",
        preview: "worktree",
        prompt: "Do this work in a new git worktree of this project on its own branch, not in the current folder, so my checkout stays as it is; tell me the worktree's path and branch when you're done.",
    },
    Boost {
        id: "docs",
        label: "Save to Docs",
        icon: "titlebar/file-text.svg",
        category: "work",
        description: "The answer is saved as a Markdown doc in the project's Docs.",
        preview: "docs",
        prompt: "Save your answer as a Markdown file in this project (Ghostex's Docs view lists the project's Markdown files) and link it in your reply.",
    },
    Boost {
        id: "steps-for-me",
        label: "Steps for me",
        icon: "titlebar/list-details.svg",
        category: "work",
        description: "Anything only you can do goes on your Needs you list, with a Done button.",
        preview: "steps",
        prompt: "Put every step only I can do (a sudo command, a sign-in, a tap on my phone, a check by eye) on my Needs you list with `ghostex needs-you add`, so each one gets a Done button, and list them under Needs you in your reply.",
    },
    Boost {
        id: "notify",
        label: "Tell me when done",
        icon: "titlebar/bell.svg",
        category: "work",
        description: "A notification on your computer and phone when the work is finished.",
        preview: "notify",
        prompt: "When the work is finished, or if you get stuck and need me, send me a notification with `ghostex notify --title \"<what happened>\"`.",
    },
];

/// One of the user's own Boosts, as gxserver keeps it (`custom` in `/api/boosts`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomBoost {
    pub id: String,
    pub label: String,
    /// A desktop icon path (`titlebar/<name>.svg`), one of [`CUSTOM_ICONS`].
    #[serde(default)]
    pub icon: String,
    pub prompt: String,
}

/// The icons a user's Boost can take.
pub const CUSTOM_ICONS: [&str; 10] = [
    "titlebar/sparkles.svg",
    "titlebar/browser.svg",
    "titlebar/layout-columns.svg",
    "titlebar/chart-bar.svg",
    "titlebar/sitemap.svg",
    "titlebar/checklist.svg",
    "titlebar/file-text.svg",
    "titlebar/bulb.svg",
    "titlebar/wand.svg",
    "titlebar/rocket.svg",
];

/// Ghostex's Boost `id` names, if any.
pub fn boost(id: &str) -> Option<&'static Boost> {
    BOOSTS.iter().find(|boost| boost.id == id)
}

/// Whether `id` can name a Boost: lower-case letters, digits and dashes.
fn is_boost_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// The Boost id a link's destination names (`boost:live-page`, `boost:custom-3`), if any.
pub fn boost_id_of_href(href: &str) -> Option<&str> {
    href.trim()
        .strip_prefix(BOOST_LINK_SCHEME)
        .filter(|id| is_boost_id(id))
}

/// Ghostex's Boost a link's destination names, if any (a user's Boost is `None`).
pub fn boost_of_href(href: &str) -> Option<&'static Boost> {
    boost(boost_id_of_href(href)?)
}

/// The pill a Boost puts in the message: `[label](boost:id)`, with the label's brackets escaped.
pub fn boost_link(id: &str, label: &str) -> String {
    let label = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    format!("[{label}]({BOOST_LINK_SCHEME}{id})")
}

/// Every `[label](boost:id)` link in `text`: its byte range, its id and its label.
fn boost_links(text: &str) -> Vec<(std::ops::Range<usize>, String, String)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("](boost:") {
        let close = from + at;
        // The link's own `[`: the last one before `](` that is not escaped (`\[` in a name).
        let mut search = close;
        let open = loop {
            match text[..search].rfind('[') {
                Some(at) if at > 0 && text.as_bytes()[at - 1] == b'\\' => search = at,
                found => break found,
            }
        };
        let Some(open) = open else {
            from = close + 2;
            continue;
        };
        let href_start = close + 2;
        let Some(end) = text[href_start..].find(')').map(|end| href_start + end) else {
            break;
        };
        if let Some(id) = boost_id_of_href(&text[href_start..end]) {
            let label = text[open + 1..close]
                .replace("\\[", "[")
                .replace("\\]", "]");
            found.push((open..end + 1, id.to_string(), label));
        }
        from = end + 1;
    }
    found
}

/// The Boosts a message carries, each once (its id and the label its pill shows), in order.
pub fn boosts_in(text: &str) -> Vec<(String, String)> {
    let mut boosts: Vec<(String, String)> = Vec::new();
    for (_, id, label) in boost_links(text) {
        if !boosts.iter().any(|(seen, _)| *seen == id) {
            boosts.push((id, label));
        }
    }
    boosts
}

/// The opening line of the block gxserver adds under a Boosted message.
const NOTE_OPENING: &str =
    "The user added Boosts to this message. Do what each one asks as part of your answer:";

/// The note under a message for `boosts` (each its name and its prompt), `None` for none.
pub fn boost_note(boosts: &[(String, String)]) -> Option<String> {
    let lines: Vec<String> = boosts
        .iter()
        .filter(|(_, prompt)| !prompt.trim().is_empty())
        .map(|(label, prompt)| format!("- {label}: {}", prompt.trim().replace('\n', " ")))
        .collect();
    (!lines.is_empty()).then(|| format!("{NOTE_OPENING}\n{}", lines.join("\n")))
}

/// `text` as the agent gets it: the message, then a `<ghostex-system>` block with each Boost's
/// prompt (`prompt_of` gives a Boost's prompt by id: the edited one, the shipped one or the
/// user's; `None` for a Boost that is gone). Every chat hides that block
/// (`session::appended_notes::without_notes`), so even a phone with an older chat core draws only
/// the user's words. A message with no Boost, or a slash or shell command, is left as it is.
pub fn with_boosts(text: &str, prompt_of: impl Fn(&str) -> Option<String>) -> String {
    if text.trim_start().starts_with(['/', '!']) {
        return text.to_string();
    }
    let named: Vec<(String, String)> = boosts_in(text)
        .into_iter()
        .filter_map(|(id, label)| {
            let label = boost(&id).map_or(label, |boost| boost.label.to_string());
            Some((label, prompt_of(&id)?))
        })
        .collect();
    match boost_note(&named) {
        Some(note) => {
            use crate::session::appended_notes::{SYSTEM_CLOSE, SYSTEM_OPEN};
            format!(
                "{}\n\n{SYSTEM_OPEN}\n{note}\n{SYSTEM_CLOSE}",
                text.trim_end()
            )
        }
        None => text.to_string(),
    }
}

/// `text` without its Boost pills: what the message says, for a draft's title in the sidebar.
pub fn without_boost_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (range, _, _) in boost_links(text) {
        out.push_str(&text[cursor..range.start]);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// The draft with Boost `id` taken out when it is in it, or put at its end (as `label`) when it is
/// not.
pub fn toggle_boost(text: &str, id: &str, label: &str) -> String {
    let present: Vec<_> = boost_links(text)
        .into_iter()
        .filter(|(_, found, _)| found == id)
        .map(|(range, _, _)| range)
        .collect();
    if present.is_empty() {
        let body = text.trim_end();
        return if body.is_empty() {
            format!("{} ", boost_link(id, label))
        } else {
            format!("{body} {} ", boost_link(id, label))
        };
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for range in present {
        let mut start = range.start;
        // The space that stood before the pill goes with it.
        if start > cursor && text[..start].ends_with(' ') {
            start -= 1;
        }
        out.push_str(&text[cursor..start]);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    if out.trim().is_empty() {
        String::new()
    } else {
        out
    }
}

/// What the chat remembers of the Boosts between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoostsState {
    /// The prompts the user edited, by Boost id, as gxserver last said.
    pub edited: HashMap<String, String>,
    /// The user's own Boosts, in their order.
    pub custom: Vec<CustomBoost>,
    /// Boosts are switched off in Settings > Boosts: no pill, no picker.
    pub off: bool,
    /// Whether the one-time callout was seen (on any client of this computer); `None` until
    /// gxserver answered, so it never flashes.
    pub hint_seen: Option<bool>,
    /// The call in flight; an answer to another id is dropped.
    pub request: Option<u64>,
    /// The first read went out.
    pub started: bool,
    /// gxserver is older than Boosts: no pill, no reads.
    pub unsupported: bool,
    /// The menu's open tab, the one used last in this chat.
    pub tab: Option<String>,
}

impl BoostsState {
    /// The prompt the agent gets for Ghostex's `boost` now.
    pub fn prompt(&self, boost: &Boost) -> String {
        self.edited
            .get(boost.id)
            .filter(|prompt| !prompt.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| boost.prompt.to_string())
    }

    /// Every Boost the menu, the picker and the hovers can show: Ghostex's, then the user's, each
    /// as `(id, label, icon, category, description, preview, prompt, edited)`.
    fn rows(&self) -> Vec<BoostView> {
        BOOSTS
            .iter()
            .map(|boost| BoostView {
                id: boost.id.to_string(),
                label: boost.label.to_string(),
                icon: boost.icon.to_string(),
                category: boost.category.to_string(),
                description: boost.description.to_string(),
                preview: boost.preview.to_string(),
                prompt: self.prompt(boost),
                edited: self.edited.contains_key(boost.id),
                custom: false,
            })
            .chain(self.custom.iter().map(|boost| BoostView {
                id: boost.id.clone(),
                label: boost.label.clone(),
                icon: if boost.icon.is_empty() {
                    CUSTOM_ICONS[0].to_string()
                } else {
                    boost.icon.clone()
                },
                category: "custom".to_string(),
                description: custom_description(&boost.prompt),
                preview: "custom".to_string(),
                prompt: boost.prompt.clone(),
                edited: false,
                custom: true,
            }))
            .collect()
    }
}

/// One Boost as the chat shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct BoostView {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub category: String,
    pub description: String,
    pub preview: String,
    pub prompt: String,
    pub edited: bool,
    pub custom: bool,
}

/// A user's Boost's line in the menu: its prompt's start, cut at a word.
fn custom_description(prompt: &str) -> String {
    const MAX: usize = 110;
    let flat = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX {
        return flat;
    }
    let cut: String = flat.chars().take(MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}\u{2026}", cut.trim_end_matches([',', '.', ';', ':']))
}

/// The Boosts the `*` picker filters, from the chat's state: each `(id, label, description)`.
/// Empty while Boosts are off or gxserver has not answered.
pub fn picker_choices(state: &BoostsState) -> Vec<(String, String, String)> {
    if state.off || state.unsupported || state.hint_seen.is_none() {
        return Vec::new();
    }
    state
        .rows()
        .into_iter()
        .map(|row| (row.id, row.label, row.description))
        .collect()
}

/// The Boosts whose name has a word starting with `query` (any case), in the menu's order; every
/// one for an empty query.
pub fn filter_choices<'a>(
    choices: &'a [(String, String, String)],
    query: &str,
) -> Vec<&'a (String, String, String)> {
    let query = query.to_lowercase();
    choices
        .iter()
        .filter(|(_, label, _)| {
            query.is_empty()
                || label
                    .to_lowercase()
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word.starts_with(&query))
        })
        .collect()
}

fn read(state: &mut ChatState, request_id: u64, params: Value) -> Effect {
    state.extras.boosts.request = Some(request_id);
    state.extras.boosts.started = true;
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::Boosts,
        params: Box::new(params),
    }
}

/// The first read, once the chat has started.
pub fn read_if_due(state: &mut ChatState, next_request_id: impl FnOnce() -> u64) -> Option<Effect> {
    let boosts = &state.extras.boosts;
    if boosts.started || boosts.unsupported || !state.core.controller_started {
        return None;
    }
    Some(read(state, next_request_id(), json!({ "op": "list" })))
}

/// Takes gxserver's `{boosts: [{id, prompt, edited}], custom: [...], on, hintSeen}`.
fn apply_answer(state: &mut ChatState, result: &Value) {
    let boosts = &mut state.extras.boosts;
    boosts.edited = result
        .get("boosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|row| row.get("edited") == Some(&Value::Bool(true)))
        .filter_map(|row| {
            Some((
                row.get("id")?.as_str()?.to_string(),
                row.get("prompt")?.as_str()?.to_string(),
            ))
        })
        .collect();
    boosts.custom = result
        .get("custom")
        .cloned()
        .and_then(|custom| serde_json::from_value(custom).ok())
        .unwrap_or_default();
    boosts.off = result.get("on") == Some(&Value::Bool(false));
    boosts.hint_seen = Some(result.get("hintSeen") == Some(&Value::Bool(true)));
}

/// An answer to a Boosts call, or `None` when it was not one.
pub fn settle_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if state.extras.boosts.request != Some(request_id) {
        return None;
    }
    state.core.claim_refusal();
    state.extras.boosts.request = None;
    match outcome {
        RpcOutcome::Ok { result } => apply_answer(state, result),
        // A read that fails (an older gxserver has no `/api/boosts`) leaves the chat without
        // Boosts, quietly.
        RpcOutcome::Err { .. } => {
            if state.extras.boosts.hint_seen.is_none() {
                state.extras.boosts.unsupported = true;
            }
        }
    }
    state.core.request_publish();
    Some(Vec::new())
}

fn param<'a>(action: &'a UserAction, name: &str) -> Option<&'a str> {
    action.param(name).and_then(Value::as_str)
}

/// The Boost actions: `toggleBoost {id, text?}` (in or out of the draft; `text` is the chat box's
/// text when the renderer has it, else the core's copy), `setBoostTab {tab}`, `refreshBoosts`
/// (the menu opened: read the prompts and the user's Boosts again, which Settings may have changed)
/// and `dismissBoostHint`.
pub fn handle(
    state: &mut ChatState,
    action: &UserAction,
    next_request_id: impl FnOnce() -> u64,
) -> Vec<Effect> {
    use crate::action::ActionKind;
    match action.kind {
        ActionKind::ToggleBoost => {
            let id = param(action, "id").unwrap_or_default();
            let Some(row) = state
                .extras
                .boosts
                .rows()
                .into_iter()
                .find(|row| row.id == id)
            else {
                return Vec::new();
            };
            let text = param(action, "text")
                .map(str::to_string)
                .unwrap_or_else(|| state.composer.text.clone());
            let content = toggle_boost(&text, &row.id, &row.label);
            let mut effects = Vec::new();
            // Using a Boost is what the callout asked for: it does not come back.
            if state.extras.boosts.hint_seen == Some(false) && state.extras.boosts.request.is_none()
            {
                state.extras.boosts.hint_seen = Some(true);
                effects.push(read(state, next_request_id(), json!({ "op": "hintSeen" })));
            }
            let caret = content.encode_utf16().count();
            effects.push(Effect::SetComposerText {
                content,
                caret: Some(caret),
                from_history: false,
            });
            effects
        }
        ActionKind::SetBoostTab => {
            let tab = param(action, "tab").unwrap_or_default();
            if TABS.iter().any(|(id, _)| *id == tab) {
                state.extras.boosts.tab = Some(tab.to_string());
                state.core.request_publish();
            }
            Vec::new()
        }
        ActionKind::RefreshBoosts => {
            if state.extras.boosts.request.is_some() || state.extras.boosts.unsupported {
                return Vec::new();
            }
            vec![read(state, next_request_id(), json!({ "op": "list" }))]
        }
        ActionKind::DismissBoostHint => {
            if state.extras.boosts.hint_seen == Some(true) {
                return Vec::new();
            }
            state.extras.boosts.hint_seen = Some(true);
            state.core.request_publish();
            vec![read(state, next_request_id(), json!({ "op": "hintSeen" }))]
        }
        _ => Vec::new(),
    }
}

/// The words a Boost pill's hover shows: its prompt, then how to edit it.
pub fn pill_tooltip(prompt: &str) -> String {
    format!("{}\n\nDouble click to edit prompt", prompt.trim())
}

/// The Boost pill's id in the strip.
pub const BOOST_PILL: &str = "boost";

/// The `boosts` document key: `null` while there are no Boosts here (an older gxserver, no answer
/// yet, or Boosts switched off), else `{pill, tabs, tab, list, hint}`:
/// - `pill`: the strip's Boost pill (`label`, `ariaLabel`, `active` while the draft has a Boost).
/// - `tabs`: `[{id, label, count}]`, Custom only once the user has a Boost of their own; `tab`
///   the open one.
/// - `list`: every Boost with `id`, `label`, `iconPath`, `category`, `description`, `preview`,
///   `prompt`, `edited`, `custom`, `tooltip` and `inDraft`.
/// - `hint`: the one-time callout's `{title, text}` while it has not been seen, or `null`.
pub fn document(state: &ChatState) -> Value {
    let boosts = &state.extras.boosts;
    if boosts.unsupported || boosts.off || boosts.hint_seen.is_none() {
        return Value::Null;
    }
    let in_draft: Vec<String> = boosts_in(&state.composer.text)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let rows = boosts.rows();
    let list: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "label": row.label,
                "iconPath": row.icon,
                "category": row.category,
                "description": row.description,
                "preview": row.preview,
                "edited": row.edited,
                "custom": row.custom,
                "tooltip": pill_tooltip(&row.prompt),
                "prompt": row.prompt,
                "inDraft": in_draft.contains(&row.id),
            })
        })
        .collect();
    let tabs: Vec<Value> = TABS
        .iter()
        .filter(|(id, _)| *id != "custom" || !boosts.custom.is_empty())
        .map(|(id, label)| {
            json!({
                "id": id,
                "label": label,
                "count": rows.iter().filter(|row| row.category == *id).count(),
            })
        })
        .collect();
    let tab = boosts
        .tab
        .clone()
        .filter(|tab| tabs.iter().any(|shown| shown["id"] == tab.as_str()))
        .unwrap_or_else(|| TABS[0].0.to_string());
    json!({
        "pill": {
            "id": BOOST_PILL,
            "label": "Boost",
            "ariaLabel": "Boosts: add one to your message",
            "active": !in_draft.is_empty(),
        },
        "tabs": tabs,
        "tab": tab,
        "list": list,
        "hint": (boosts.hint_seen == Some(false)).then(|| json!({
            "title": "New: Boosts",
            "text": "Get the answer as a page, a chart or a map.",
        })),
    })
}
