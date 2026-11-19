//! Boosts: one click in the chat box adds a short instruction to the next message, so the agent
//! answers with a Live page, Ghostex visuals, options side by side, a map or a triage page instead
//! of only words.
//!
//! The Boost pill in the chat box's strip opens a menu of the Boosts; picking one puts a Boost pill
//! in the message (`[Live page](boost:live-page)`, a reference pill of kind `boost`), and picking it
//! again takes it out. gxserver adds each Boost's prompt under the message on its way to the agent,
//! inside a `<ghostex-system>` block every chat already hides (server/src/boosts/mod.rs
//! `with_boosts`), so the bubble shows the user's words and the pill. A Boost lasts that one
//! message. Hovering the pill shows its prompt; a double click opens it for editing. An edited
//! prompt is kept by gxserver per Boost for every chat on that computer, until Reset to default.
//!
//! CDXC:Boosts 2026-11-20 DECISION:
//! The user answered the Boosts design page (Q4213 to Q4218): the name is Boosts; the first ones
//! are Live page, Chart it, Side by side, Map it (on a Live page) and a new one, Triage requests
//! ("asks the agent to look at GitHub issues/PRs, Linear or Jira tickets etc. and use a Live page
//! to explain each of them well (what each one asks for as bullet points, visuals where possible,
//! a decision per item), like the coordinator's triage pages"); one Boost menu pill in the chat
//! box; a Boost lasts one message; people find them through a one-time hint and "Try a Boost" on
//! an empty chat; agents learn how from the complete CLI help (`ghostex visual guide`), not
//! skills, so a Boost's prompt only points at it. Also: "when a Boost is in the chat box, hovering
//! it shows its prompt in the tooltip with 'Double click to edit prompt'; double click opens the
//! prompt for editing".
//! SEE-ALSO: server/src/boosts/ (the prompts, `/api/boosts`, the block under the message),
//! apps/desktop/src/app/native_chat/boosts.rs (the pill, menu, hint, Try a Boost and the editor),
//! composer/reference_pills.rs (`ReferenceKind::Boost`), transcript/markdown_links.rs (the pill in
//! a sent message).

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The scheme of a Boost pill's link: `[Live page](boost:live-page)`.
pub const BOOST_LINK_SCHEME: &str = "boost:";

/// One Boost: what the menu shows and the prompt the agent gets unless the user edited it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boost {
    pub id: &'static str,
    pub label: &'static str,
    /// The icon, a path under the desktop's assets (`titlebar/<name>.svg`).
    pub icon: &'static str,
    /// One line on what the agent does, for the menu and the hint.
    pub description: &'static str,
    /// What the agent is told, unless the user edited it.
    pub prompt: &'static str,
}

/// The Boosts, in the menu's order.
pub const BOOSTS: [Boost; 5] = [
    Boost {
        id: "live-page",
        label: "Live page",
        icon: "titlebar/browser.svg",
        description: "The answer comes back as a page you open: mockups, comparisons, small tools.",
        prompt: "Answer with a Live page: one self-contained HTML page you publish with `ghostex show`, then paste the block it prints into your reply. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "chart",
        label: "Chart it",
        icon: "titlebar/chart-bar.svg",
        description: "Numbers come back as charts, stat tiles and tables, not paragraphs.",
        prompt: "Show the numbers in your answer as Ghostex visuals: charts, stat tiles and tables in a ```visual block, each with one plain sentence on what it shows. If you haven't in this conversation, run `ghostex visual guide visuals` first, and check the block with `ghostex visual check` before you reply.",
    },
    Boost {
        id: "compare",
        label: "Side by side",
        icon: "titlebar/layout-columns.svg",
        description: "Two to four real options drawn next to each other, picked with a click.",
        prompt: "Lay out two to four real options side by side on a Live page, each drawn as it would really look or work, with its trade-offs, and under them one question with your recommended pick so I can choose with a click. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "map",
        label: "Map it",
        icon: "titlebar/sitemap.svg",
        description: "How the parts connect, drawn as a diagram on a Live page.",
        prompt: "Explain it as a map on a Live page: a diagram of the parts and how they connect (a Mermaid diagram works), with a short plain note on each part and on what flows between them. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
    Boost {
        id: "triage",
        label: "Triage requests",
        icon: "titlebar/checklist.svg",
        description: "Issues, pull requests or tickets explained one by one, with a decision for each.",
        prompt: "Triage the requests this message points at: GitHub issues and pull requests, Linear or Jira tickets, or whatever I name (if I named none, the open ones of this project). Read each one, then put them all on one Live page, one card per item: a link to it, what it asks for as short bullet points, a picture of it wherever one helps (a screenshot, a mockup or a small diagram), your recommendation, and a question with the choices for that item so I can decide each with a click. If you haven't in this conversation, run `ghostex visual guide pages` first and follow it.",
    },
];

/// The Boost `id` names, if any.
pub fn boost(id: &str) -> Option<&'static Boost> {
    BOOSTS.iter().find(|boost| boost.id == id)
}

/// The Boost a link's destination names (`boost:live-page`), if any.
pub fn boost_of_href(href: &str) -> Option<&'static Boost> {
    boost(href.trim().strip_prefix(BOOST_LINK_SCHEME)?)
}

/// The pill a Boost puts in the message.
pub fn boost_link(boost: &Boost) -> String {
    format!("[{}]({BOOST_LINK_SCHEME}{})", boost.label, boost.id)
}

/// Every `[label](boost:id)` link in `text`: its byte range and its Boost, in order. A link to an
/// unknown Boost is left as it is.
fn boost_links(text: &str) -> Vec<(std::ops::Range<usize>, &'static Boost)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("](boost:") {
        let close = from + at;
        let Some(open) = text[..close].rfind('[') else {
            from = close + 2;
            continue;
        };
        let href_start = close + 2;
        let Some(end) = text[href_start..].find(')').map(|end| href_start + end) else {
            break;
        };
        if let Some(boost) = boost_of_href(&text[href_start..end]) {
            found.push((open..end + 1, boost));
        }
        from = end + 1;
    }
    found
}

/// The Boosts a message carries, each once, in the order they appear.
pub fn boosts_in(text: &str) -> Vec<&'static Boost> {
    let mut boosts: Vec<&'static Boost> = Vec::new();
    for (_, boost) in boost_links(text) {
        if !boosts.iter().any(|seen| seen.id == boost.id) {
            boosts.push(boost);
        }
    }
    boosts
}

/// `text` without its Boost pills: what the message says, for a draft's title in the sidebar.
pub fn without_boost_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (range, _) in boost_links(text) {
        out.push_str(&text[cursor..range.start]);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// The opening line of the block gxserver adds under a Boosted message.
const NOTE_OPENING: &str =
    "The user added Boosts to this message. Do what each one asks as part of your answer:";

/// The note under a message for `boosts`, each with the prompt `prompt_of` gives it (an edited one
/// or the shipped one), `None` for none.
pub fn boost_note(boosts: &[&Boost], prompt_of: impl Fn(&Boost) -> String) -> Option<String> {
    if boosts.is_empty() {
        return None;
    }
    let lines: Vec<String> = boosts
        .iter()
        .map(|boost| {
            let prompt = prompt_of(boost);
            let prompt = prompt.trim();
            let prompt = if prompt.is_empty() {
                boost.prompt
            } else {
                prompt
            };
            format!("- {}: {}", boost.label, prompt.replace('\n', " "))
        })
        .collect();
    Some(format!("{NOTE_OPENING}\n{}", lines.join("\n")))
}

/// `text` as the agent gets it: the message, then a `<ghostex-system>` block with each Boost's
/// prompt (the same block every chat hides, `session::appended_notes::without_notes`, so even a
/// phone with an older chat core draws only the user's words). A message with no Boost, or a
/// slash or shell command, is left as it is.
pub fn with_boosts(text: &str, prompt_of: impl Fn(&Boost) -> String) -> String {
    if text.trim_start().starts_with(['/', '!']) {
        return text.to_string();
    }
    match boost_note(&boosts_in(text), prompt_of) {
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

/// The draft with Boost `id` taken out when it is in it, or put at its end when it is not; `None`
/// for an unknown Boost.
pub fn toggle_boost(text: &str, id: &str) -> Option<String> {
    let boost = boost(id)?;
    let present: Vec<_> = boost_links(text)
        .into_iter()
        .filter(|(_, found)| found.id == boost.id)
        .map(|(range, _)| range)
        .collect();
    if present.is_empty() {
        let body = text.trim_end();
        return Some(if body.is_empty() {
            format!("{} ", boost_link(boost))
        } else {
            format!("{body} {} ", boost_link(boost))
        });
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for range in present {
        let mut start = range.start;
        // The space that stood before the pill goes with it.
        if text[..start].ends_with(' ') && text[cursor..start].len() > 0 {
            start -= 1;
        }
        out.push_str(&text[cursor..start]);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    Some(if out.trim().is_empty() {
        String::new()
    } else {
        out
    })
}

/// What the chat remembers of the Boosts between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoostsState {
    /// The prompts the user edited, by Boost id, as gxserver last said.
    pub edited: HashMap<String, String>,
    /// Whether the one-time hint was seen or dismissed (on any client of this computer); `None`
    /// until gxserver answered, so the hint never flashes.
    pub hint_seen: Option<bool>,
    /// The call in flight; an answer to another id is dropped.
    pub request: Option<u64>,
    /// That call saves an edited prompt (a refusal says so), rather than a read.
    pub saving: bool,
    /// The first read went out.
    pub started: bool,
    /// gxserver is older than Boosts: no pill, no reads.
    pub unsupported: bool,
    /// The prompt being edited, if the editor is open.
    pub editor: Option<BoostEditor>,
}

/// The prompt editor a double click on a Boost pill opens.
#[derive(Clone, Debug, PartialEq)]
pub struct BoostEditor {
    pub id: String,
    pub text: String,
}

impl BoostsState {
    /// The prompt the agent gets for `boost` now.
    pub fn prompt(&self, boost: &Boost) -> String {
        self.edited
            .get(boost.id)
            .filter(|prompt| !prompt.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| boost.prompt.to_string())
    }
}

fn read(state: &mut ChatState, request_id: u64, params: Value) -> Effect {
    state.extras.boosts.saving = matches!(params["op"].as_str(), Some("set" | "reset"));
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

/// Takes gxserver's `{boosts: [{id, prompt, edited}], hintSeen}`.
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
        RpcOutcome::Ok { result } => {
            apply_answer(state, result);
            state.core.request_publish();
            Some(Vec::new())
        }
        // A read that fails (an older gxserver has no `/api/boosts`) leaves the chat without
        // Boosts, quietly; only a save the user made says it failed.
        RpcOutcome::Err { .. } if !state.extras.boosts.saving => {
            if state.extras.boosts.hint_seen.is_none() {
                state.extras.boosts.unsupported = true;
                state.core.request_publish();
            }
            Some(Vec::new())
        }
        RpcOutcome::Err { message, .. } => Some(vec![Effect::Toast {
            level: "error".to_string(),
            message: if message.is_empty() {
                "Couldn't save the Boost's prompt.".to_string()
            } else {
                message.clone()
            },
        }]),
    }
}

fn param<'a>(action: &'a UserAction, name: &str) -> Option<&'a str> {
    action.param(name).and_then(Value::as_str)
}

/// The Boost actions: `toggleBoost {id, text?}` (in or out of the draft; `text` is the chat box's
/// text when the renderer has it, else the core's copy), `openBoostEditor {id}`,
/// `editBoostPrompt {text}`, `saveBoostPrompt`, `resetBoostPrompt`, `closeBoostEditor` and
/// `dismissBoostHint`.
pub fn handle(
    state: &mut ChatState,
    action: &UserAction,
    next_request_id: impl FnOnce() -> u64,
) -> Vec<Effect> {
    use crate::action::ActionKind;
    match action.kind {
        ActionKind::ToggleBoost => {
            let id = param(action, "id").unwrap_or_default();
            let text = param(action, "text")
                .map(str::to_string)
                .unwrap_or_else(|| state.composer.text.clone());
            let Some(content) = toggle_boost(&text, id) else {
                return Vec::new();
            };
            let mut effects = Vec::new();
            // Using a Boost is what the hint asked for: it does not come back.
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
        ActionKind::OpenBoostEditor => {
            let Some(boost) = param(action, "id").and_then(boost) else {
                return Vec::new();
            };
            let text = state.extras.boosts.prompt(boost);
            state.extras.boosts.editor = Some(BoostEditor {
                id: boost.id.to_string(),
                text,
            });
            state.core.request_publish();
            Vec::new()
        }
        ActionKind::EditBoostPrompt => {
            if let Some(editor) = state.extras.boosts.editor.as_mut() {
                editor.text = param(action, "text").unwrap_or_default().to_string();
            }
            Vec::new()
        }
        ActionKind::CloseBoostEditor => {
            state.extras.boosts.editor = None;
            state.core.request_publish();
            Vec::new()
        }
        ActionKind::SaveBoostPrompt | ActionKind::ResetBoostPrompt => {
            let Some(editor) = state.extras.boosts.editor.take() else {
                return Vec::new();
            };
            let Some(boost) = boost(&editor.id) else {
                return Vec::new();
            };
            let reset =
                action.kind == ActionKind::ResetBoostPrompt || editor.text.trim() == boost.prompt;
            // Shown at once; gxserver's answer confirms it (or a toast says why it could not).
            if reset {
                state.extras.boosts.edited.remove(boost.id);
            } else if !editor.text.trim().is_empty() {
                state
                    .extras
                    .boosts
                    .edited
                    .insert(boost.id.to_string(), editor.text.trim().to_string());
            }
            state.core.request_publish();
            let params = if reset || editor.text.trim().is_empty() {
                json!({ "op": "reset", "id": boost.id })
            } else {
                json!({ "op": "set", "id": boost.id, "prompt": editor.text.trim() })
            };
            vec![read(state, next_request_id(), params)]
        }
        ActionKind::DismissBoostHint => {
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

/// The `boosts` document key: `null` while gxserver has no Boosts (older, or not answered yet),
/// else `{pill, list, editor, hint}`:
/// - `pill`: the strip's Boost pill (`label`, `ariaLabel`, `menu`, its rows in the sidebar menu's
///   shape, the Boosts in the draft `checked`).
/// - `list`: every Boost with `id`, `label`, `iconPath`, `description`, `prompt`, `edited`,
///   `tooltip` and `inDraft`, for the chat box's pills, the hint and Try a Boost.
/// - `editor`: the open prompt editor (`id`, `label`, `value`, `edited`), or `null`.
/// - `hint`: the one-time hint's words while it has not been seen, or `null`.
pub fn document(state: &ChatState) -> Value {
    let boosts = &state.extras.boosts;
    if boosts.unsupported || boosts.hint_seen.is_none() {
        return Value::Null;
    }
    let in_draft = boosts_in(&state.composer.text);
    let list: Vec<Value> = BOOSTS
        .iter()
        .map(|boost| {
            let prompt = boosts.prompt(boost);
            json!({
                "id": boost.id,
                "label": boost.label,
                "iconPath": boost.icon,
                "description": boost.description,
                "edited": boosts.edited.contains_key(boost.id),
                "tooltip": pill_tooltip(&prompt),
                "prompt": prompt,
                "inDraft": in_draft.iter().any(|found| found.id == boost.id),
            })
        })
        .collect();
    let menu: Vec<Value> = BOOSTS
        .iter()
        .map(|boost| {
            json!({
                "label": boost.label,
                "description": boost.description,
                "iconPath": boost.icon,
                "checked": in_draft.iter().any(|found| found.id == boost.id),
                "command": { "type": "toggleBoost", "id": boost.id },
            })
        })
        .collect();
    let editor = boosts.editor.as_ref().and_then(|editor| {
        let boost = boost(&editor.id)?;
        Some(json!({
            "id": boost.id,
            "label": boost.label,
            "iconPath": boost.icon,
            "value": editor.text,
            "edited": boosts.edited.contains_key(boost.id),
            "default": boost.prompt,
        }))
    });
    json!({
        "pill": {
            "id": BOOST_PILL,
            "label": "Boost",
            "ariaLabel": "Boosts: add one to your message",
            "active": !in_draft.is_empty(),
            "menu": menu,
        },
        "list": list,
        "editor": editor,
        "hint": (boosts.hint_seen == Some(false)).then_some(HINT),
    })
}

/// The Boost pill's id in the strip.
pub const BOOST_PILL: &str = "boost";

/// The one-time hint's words.
const HINT: &str = "New: Boosts. Pick one from the Boost pill above and the agent answers with a Live page, a chart or a triage page instead of only words.";
