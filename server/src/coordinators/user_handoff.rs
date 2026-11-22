//! The guard that keeps a coordinator from parking the user's part of a work item under In
//! progress or Waiting: `item now`, `item add --now` and `item wait` refuse a line that hands the
//! item to the user, with the `item ask` to use instead.
//!
//! CDXC:Coordinators 2026-11-15 DECISION:
//! User, after a coordinator ran `item now W28 "In your app (rebuild-012); waiting on your try"` and the check never showed under Needs you: "Everything that needs me has to be in "Needs You Work Item" status." and "I want other people to not have this issue also." So whatever the user must try, test, check, confirm, decide, sign in to or approve is an `item ask`; In progress means a thread or the coordinator is working on it right now, and Waiting is for things outside (a rebuild, a release, CI, another item), never the user. The playbook says so (playbook.md, Work items) and gxserver refuses the line, so every coordinator on every computer gets it. The match is a short English phrase list, kept conservative: words inside quotes or backticks are ignored, so a line about this very feature can still quote them, and a bare "Needs you" (the panel's own name, which work on Ghostex itself mentions) never matches.
//! SEE-ALSO: server/src/coordinators/work.rs (`handle_work_request` calls it), server/src/coordinators/playbook.md (the rule in the `item now` bullet and the status list).

/// Phrases that hand an item to the user, matched on whole words in lowercase with punctuation
/// as spaces ("go-ahead" reads "go ahead").
const HANDOFF_PHRASES: &[&str] = &[
    "waiting on you",
    "waiting for you",
    "waiting on your",
    "waiting for your",
    "waits on you",
    "waits for you",
    "waits on your",
    "waits for your",
    "wait on you",
    "wait for you",
    "wait on your",
    "wait for your",
    "waiting on the user",
    "waiting for the user",
    "waits on the user",
    "waits for the user",
    "wait on the user",
    "wait for the user",
    "waiting on user",
    "waiting for user",
    "for you to",
    "for the user to",
    "for user to",
    "needs you to",
    "need you to",
    "needs your",
    "need your",
    "needs the user",
    "ready for you",
    "up to you",
    "your call",
    "your try",
    "your test",
    "your testing",
    "your check",
    "your confirmation",
    "your go ahead",
    "your approval",
    "your decision",
    "your answer",
    "your ok",
    "your okay",
    "your sign off",
    "please try",
    "please test",
    "please check",
    "please confirm",
];

/// `text` without what it quotes: the words between straight or curly double quotes and between
/// backticks.
pub(super) fn unquoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut closing: Option<char> = None;
    for character in text.chars() {
        match closing {
            Some(close) if character == close => closing = None,
            Some(_) => {}
            None => match character {
                '"' => closing = Some('"'),
                '`' => closing = Some('`'),
                '\u{201C}' => closing = Some('\u{201D}'),
                _ => out.push(character),
            },
        }
    }
    out
}

/// Lowercase words separated by single spaces, with a space at each end so a phrase matches only
/// whole words. An apostrophe stays inside its word ("user's" is not "user").
fn words(text: &str) -> String {
    let mut out = String::from(" ");
    for character in unquoted(text).chars() {
        let character = if character == '\u{2019}' {
            '\''
        } else {
            character
        };
        if character.is_alphanumeric() || character == '\'' {
            out.extend(character.to_lowercase());
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

/// The phrase in `text` that hands the item to the user, if any.
fn handoff_phrase(text: &str) -> Option<&'static str> {
    let words = words(text);
    HANDOFF_PHRASES
        .iter()
        .find(|phrase| words.contains(&format!(" {phrase} ")))
        .copied()
}

/// Whether a `wait --on "<words>"` names the user: one of the phrases, or words that start with
/// "you", "user" or "the user" ("the user to try it"); "the user's rebuild" does not.
fn names_the_user(on: &str) -> bool {
    let words = words(on);
    handoff_phrase(on).is_some()
        || [" you ", " user ", " the user "]
            .iter()
            .any(|start| words.starts_with(start))
}

/// The `item ask` (and `item done`) to use instead, for item `name` (W28, or W<n> before it
/// exists).
fn ask_instead(name: &str) -> String {
    format!(
        "use `ghostex coordinator item ask {name} \"<what to try>\" --answer Works --answer \"Something's off\"` (and `ghostex coordinator item done {name}` if the work itself is finished)"
    )
}

/// Why `item now` (or `item add --now`) refuses `line`, or `None` when it may stand. `name` is the
/// item (W28); `adding` when the line came with `item add`.
pub(super) fn now_refusal(name: &str, line: &str, adding: bool) -> Option<String> {
    let phrase = handoff_phrase(line)?;
    let then = if adding {
        format!("add it without --now, then {}", ask_instead("W<n>"))
    } else {
        ask_instead(name)
    };
    Some(format!(
        "\"{phrase}\" hands this to the user, and In progress means a thread or you are working on it right now. That's for the user: {then}."
    ))
}

/// Why `item wait` refuses, or `None`: one of `on` names the user, or its line hands the item to
/// them.
pub(super) fn wait_refusal(name: &str, on: &[String], line: Option<&str>) -> Option<String> {
    let names_user = on.iter().any(|on| names_the_user(on));
    if !names_user && line.and_then(handoff_phrase).is_none() {
        return None;
    }
    Some(format!(
        "Waiting on the user is an ask, not a wait. That's for the user: {}. `item wait` is for a rebuild, a release, CI, another item or something else outside.",
        ask_instead(name)
    ))
}
