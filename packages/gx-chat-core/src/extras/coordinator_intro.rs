//! "How to use coordinators": the tips a coordinator's chat offers behind a pill in the chat box's
//! strip. The text lives here once; the desktop and the web build draw it as a floating panel
//! (apps/desktop/src/app/native_chat/coordinator_intro.rs) and the phone as a sheet
//! (apps/mobile/app/src/chat/native/cards/CoordinatorIntro.tsx). gxserver says whether the user
//! turned it off (`coordinatorThreads.introHidden`, server/src/coordinators/intro.rs), one flag for
//! every coordinator it runs, so each window and the phone agree.
//!
//! CDXC:Coordinators 2026-11-06 DECISION:
//! User: "we must print a 'How to use coordinators' explanation when someone first launches them at
//! the top of the convo with all the recommendations, and i want u to put in the things u notice im
//! doing as recommendations". The tips come from how they work in their own coordinator (a page per
//! topic and a pick, more research on a point, screenshots, threads, one-click answers, the board,
//! notes, reading Needs you last) and what they told people on Discord. On the mockup they chose a
//! pill instead of a card at the top (Q1470): "show it in a pill at the bottom that can be clicked
//! anytime to be read, and can be dismissed later (dont show again checkbox there also). and make
//! the design nice please (match how settings looks, and each hint should be in a card pls not bare
//! text. and don't number them". Existing coordinators get the pill too, and "Don't show again"
//! removes it from all of them (Q1472); a session made a coordinator gets it as well (Q1475); only
//! the user sees it, the agent's first message is unchanged (Q1473); clicking an example puts it in
//! the chat box (Q1474); no setting (Q1476); a macOS tip names SpaceO and cua-driver (Q1477). Help
//! has the same tips (skills/ghostex-help/references/features.md, Coordinators). The names are the
//! user's pick on Q1480: gx page, gx form and gx visuals.

use serde_json::{json, Map, Value};

use crate::document::ChatPill;
use crate::effect::Effect;
use crate::state::ChatState;

/// The pill's id in `chatPills`, and the renderers' menu id.
pub const INTRO_PILL: &str = "coordinatorIntro";

pub const TITLE: &str = "How to use coordinators";
const PILL_LABEL: &str = "How to use";
const LEDE: &str = "Talk to this one chat. It answers quick questions, hands real work to threads, and brings back only what needs you.";
const EXAMPLE_HINT: &str = "Click an example to put it in the chat box.";
const DONT_SHOW_AGAIN: &str = "Don't show again";
const DONT_SHOW_AGAIN_NOTE: &str =
    "Removes this pill from every coordinator. Help › How to use coordinators keeps the tips.";

/// One tip: its icon (a titlebar icon's file name, which the phone maps to its own glyph), what to
/// do, why, and a line to type.
pub struct Tip {
    pub icon: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub example: &'static str,
}

/// The tips, in the order they show.
pub const TIPS: &[Tip] = &[
    Tip {
        icon: "coordinator-crown",
        title: "One coordinator per project",
        detail: "Give it everything. It keeps every decision in one chat.",
        example: "Fix the login bug, then plan the release.",
    },
    Tip {
        icon: "file-type-html",
        title: "Ask for a gx page per topic, then pick",
        detail: "Options with pictures (gx visuals for numbers). Your pick comes straight back.",
        example: "Give me a gx page for this so I can pick.",
    },
    Tip {
        icon: "microscope",
        title: "Not sure? Ask it to dig deeper",
        detail: "On any point, before you pick.",
        example: "Research option B more before I pick.",
    },
    Tip {
        icon: "photo",
        title: "Show, don't describe",
        detail: "Paste a screenshot and say what's wrong.",
        example: "This looks off. Give me a gx page with better designs.",
    },
    Tip {
        icon: "users-group",
        title: "Let it start threads",
        detail: "On any agent or computer. You keep talking while they work.",
        example: "Run this in a thread on my Windows PC.",
    },
    Tip {
        icon: "checklist",
        title: "Answer in one click, or later",
        detail: "Use the gx form or the answer links. Skipped questions wait in the ? pill, and you can also reply by tag.",
        example: "Q12: yes, but only on macOS",
    },
    Tip {
        icon: "layout-board-split",
        title: "Run several lines of work at once",
        detail: "The Board pill shows each one and who has the ball.",
        example: "What's moving, and what's blocked?",
    },
    Tip {
        icon: "note",
        title: "Say a rule once",
        detail: "It saves it as a note every new thread gets.",
        example: "Remember: never push without asking me.",
    },
    Tip {
        icon: "bell",
        title: "Read the end of each reply",
        detail: "Needs you comes last, and threads' approvals show right here.",
        example: "Do what you recommend, I'm off to sleep.",
    },
    Tip {
        icon: "device-desktop",
        title: "On a Mac, let threads test in real windows",
        detail: "With SpaceO and cua-driver, a thread clicks through apps in windows of its own, not yours.",
        example: "Test it in the app with SpaceO and cua-driver.",
    },
];

/// Whether this chat offers the tips: a coordinator (gxserver sends it `coordinatorThreads`) whose
/// gxserver has not been told "Don't show again", and that this client has not just hidden.
pub fn offered(state: &ChatState) -> bool {
    let Some(threads) = state.session.coordinator_threads.as_ref() else {
        return false;
    };
    threads.get("introHidden") != Some(&Value::Bool(true)) && !state.extras.panels.intro_hidden
}

/// `coordinatorIntro` on the document: the panel's text, or `null` where the pill does not show.
pub fn project(state: &ChatState) -> Value {
    if !offered(state) {
        return Value::Null;
    }
    json!({
        "title": TITLE,
        "lede": LEDE,
        "exampleHint": EXAMPLE_HINT,
        "dontShowAgain": DONT_SHOW_AGAIN,
        "dontShowAgainNote": DONT_SHOW_AGAIN_NOTE,
        "tips": TIPS
            .iter()
            .map(|tip| json!({
                "icon": tip.icon,
                "title": tip.title,
                "detail": tip.detail,
                "example": tip.example,
            }))
            .collect::<Vec<_>>(),
    })
}

/// The pill, while the tips are offered.
pub fn pill(intro: &Value) -> Option<ChatPill> {
    intro.is_object().then(|| ChatPill {
        switch: None,
        id: INTRO_PILL.to_string(),
        label: PILL_LABEL.to_string(),
        count: String::new(),
        waiting: String::new(),
        running: false,
        meta: String::new(),
        aria_label: TITLE.to_string(),
        dots: Vec::new(),
    })
}

/// `hideCoordinatorIntro`: "Don't show again". The pill goes at once and gxserver remembers it for
/// every coordinator (`/api/coordinatorBoard` op `hideIntro`); a refusal brings it back.
pub fn hide(state: &mut ChatState, request_id: u64) -> Vec<Effect> {
    if state.extras.panels.intro_hidden {
        return Vec::new();
    }
    state.extras.panels.intro_hidden = true;
    state.extras.panels.intro_hide_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::CoordinatorBoard,
        params: Box::new(json!({
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "op": "hideIntro",
        })),
    }]
}

/// Takes gxserver's answer to [`hide`]: nothing more once it is saved; refused, the pill comes back
/// and a toast says why. `None` when `request_id` is not that call.
pub fn settle_hide(
    panels: &mut crate::state::PanelsState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    if panels.intro_hide_request != Some(request_id) {
        return None;
    }
    panels.intro_hide_request = None;
    Some(match answer {
        Ok(_) => Vec::new(),
        Err(message) => {
            panels.intro_hidden = false;
            vec![Effect::Toast {
                level: "error".to_string(),
                message: if message.is_empty() {
                    "Couldn't turn off the coordinator tips.".to_string()
                } else {
                    message
                },
            }]
        }
    })
}

/// `useCoordinatorIntroExample {example, text, start?, end?}`: puts a tip's example in the chat box
/// at the caret (or over the selection), spaced off the words around it; nothing is sent.
pub fn use_example(params: &Map<String, Value>) -> Vec<Effect> {
    let example = params
        .get("example")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if example.is_empty() {
        return Vec::new();
    }
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let units = text.encode_utf16().count();
    let offset = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_u64)
            .map_or(units, |offset| (offset as usize).min(units))
    };
    let edit = crate::composer::references::insert_reference(
        text,
        example,
        offset("start"),
        offset("end"),
    );
    vec![Effect::SetComposerText {
        content: edit.text,
        caret: Some(edit.caret),
        from_history: false,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinator(threads: Value) -> ChatState {
        let mut state = ChatState::default();
        state.session.coordinator_threads = Some(threads);
        state
    }

    #[test]
    fn only_a_coordinator_offers_the_tips() {
        assert!(project(&ChatState::default()).is_null());
        let intro = project(&coordinator(json!({"threads": [], "total": 0})));
        assert_eq!(intro["title"], TITLE);
        assert_eq!(intro["tips"].as_array().unwrap().len(), TIPS.len());
        assert_eq!(pill(&intro).unwrap().id, INTRO_PILL);
    }

    #[test]
    fn gxserver_turning_it_off_hides_the_pill() {
        let state = coordinator(json!({"threads": [], "introHidden": true}));
        assert!(project(&state).is_null());
        assert!(pill(&project(&state)).is_none());
    }

    #[test]
    fn dont_show_again_hides_at_once_and_a_refusal_brings_it_back() {
        let mut state = coordinator(json!({"threads": []}));
        let effects = hide(&mut state, 7);
        assert!(matches!(
            effects.as_slice(),
            [Effect::SendRpc { params, .. }] if params["op"] == "hideIntro"
        ));
        assert!(project(&state).is_null());
        // A second click sends nothing more.
        assert!(hide(&mut state, 8).is_empty());
        assert!(settle_hide(&mut state.extras.panels, 99, Ok(&Value::Null)).is_none());
        let refused = settle_hide(&mut state.extras.panels, 7, Err("old gxserver".into())).unwrap();
        assert!(matches!(refused.as_slice(), [Effect::Toast { .. }]));
        assert!(project(&state).is_object());
    }

    #[test]
    fn an_example_goes_in_the_chat_box_at_the_caret() {
        let params = json!({"example": "What's moving?", "text": "Hi", "start": 2, "end": 2});
        let effects = use_example(params.as_object().unwrap());
        let [Effect::SetComposerText { content, .. }] = effects.as_slice() else {
            panic!("expected the chat box text");
        };
        assert!(content.starts_with("Hi"));
        assert!(content.contains("What's moving?"));
    }

    #[test]
    fn no_tip_text_uses_an_em_dash() {
        for tip in TIPS {
            for text in [tip.title, tip.detail, tip.example] {
                assert!(!text.contains('\u{2014}'), "{text}");
            }
        }
    }
}
