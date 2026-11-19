//! An approval request as one plain question, for every place that shows it to a person: the
//! desktop card, the Touch ID / Windows Hello prompt and the terminal.
//!
//! gxserver keeps writing its long sentences into `summary` and `detail` (older apps show them as
//! they are, and the desktop reads the asker and computer out of them); this reads those
//! sentences into a question that names both sides, the words to check and one line saying how.
//!
//! CDXC:Security 2026-11-17 DECISION:
//! User, on the approval cards page (docs/2026-11-17/approval-ux/index.html): Q3720 "A card in the
//! middle", Q3721 "Big boxes" for the words, Q3722 the "Let … ?" question. One plain question per
//! card naming both sides ("Let m7-arch connect to M7-Desktop?"), one line on what Approve lets it
//! do, the words to check big with "Approve only if m7-arch shows the same words.", then Deny and
//! Approve. No ids, keys, addresses, "owner", "Easy Connect" or "Ghostex server S1u". The Touch ID
//! prompt reads the same question ("Ghostex is trying to let m7-arch connect to M7-Desktop.").

use crate::approval::{ApprovalRequest, SensitiveAction};

/// How gxserver ends the summary of a computer asking for a server link (GitHub #674): after the
/// computer's own quoted name, so a name can't make a plain pairing read as a link or the other
/// way round.
pub const SERVER_LINK_SUMMARY_TAIL: &str =
    "\" link its agents with this computer through Easy Connect";

/// A request worded for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlainAsk {
    /// "Let m7-arch connect to M7-Desktop?"
    pub question: String,
    /// What Approve lets it do: "It can then open your chats and terminals on M7-Desktop."
    pub body: Option<String>,
    /// The words both screens show; empty when there are none to check.
    pub words: Vec<String>,
    /// "Approve only if m7-arch shows the same words."
    pub check: Option<String>,
}

impl PlainAsk {
    /// What the system prompt says after "Ghostex is trying to": the question, lower-case first,
    /// without its question mark.
    pub fn prompt_reason(&self) -> String {
        let reason = self.question.trim().trim_end_matches('?');
        let mut chars = reason.chars();
        match chars.next() {
            Some(first) => first.to_lowercase().chain(chars).collect(),
            None => "approve a request".to_string(),
        }
    }
}

/// `request` as one plain question, for the computer it waits on, named `computer` ("this Mac",
/// "M7-Desktop"). `None` for a request this doesn't know how to word (it then shows as gxserver
/// wrote it).
pub fn plain_ask(request: &ApprovalRequest, computer: &str) -> Option<PlainAsk> {
    let summary = request.summary.trim();
    match request.action {
        SensitiveAction::PairingApproveDevice => {
            let name = quoted_name(summary)?;
            let words = detail_words(request.detail.as_deref());
            if summary.starts_with("Pair ") {
                // gxserver ends an SSH phone's sentence with this; the phone's own name sits
                // before it, so a name saying "signs in over SSH" can't make a new phone read as
                // one that is already in (review of 8abb0a8db).
                let body = if summary.ends_with("\" and signs in over SSH") {
                    format!(
                        "It already reads and replies on {computer} over SSH; adding it also lets it approve asks like this one."
                    )
                } else {
                    format!(
                        "It can then use Ghostex on {computer} from anywhere, and approve asks like this one."
                    )
                };
                return Some(PlainAsk {
                    question: format!("Add {name} to {computer}?"),
                    body: Some(body),
                    check: (!words.is_empty())
                        .then(|| "Approve only if your phone shows the same words.".to_string()),
                    words,
                });
            }
            if summary.ends_with(SERVER_LINK_SUMMARY_TAIL) {
                return Some(PlainAsk {
                    question: format!("Let {name} link its agents with {computer}?"),
                    body: Some(format!(
                        "Agents on {name} can then start, message and read agents on {computer}. It gets no sign-in to {computer}."
                    )),
                    check: (!words.is_empty())
                        .then(|| format!("Approve only if {name} shows the same words.")),
                    words,
                });
            }
            Some(PlainAsk {
                question: format!("Let {name} connect to {computer}?"),
                body: Some(format!(
                    "It can then open your chats and terminals on {computer}."
                )),
                check: (!words.is_empty())
                    .then(|| format!("Approve only if {name} shows the same words.")),
                words,
            })
        }
        SensitiveAction::PairingShowCode => Some(PlainAsk {
            question: format!("Show {}'s pairing code?", computer),
            body: Some(format!(
                "Someone ran ghostex computer-code on {computer}. Approve if it was you."
            )),
            words: Vec::new(),
            check: None,
        }),
        SensitiveAction::AddApproverKey => {
            let rest = summary.strip_prefix("Let ")?;
            // The asking app writes its own label, which comes first; gxserver's phrase is the
            // last one, so a label saying "show the pairing code of" can't make an "act as you"
            // ask read as the door-only kind (review of 8abb0a8db).
            let door = rest.rfind(" show the pairing code of ");
            let everything = rest.rfind(" act as you on ");
            let (label, does) = match (door, everything) {
                (Some(door), Some(everything)) if everything > door => {
                    (&rest[..everything], ManageKind::Everything)
                }
                (Some(door), _) => (&rest[..door], ManageKind::Door),
                (None, Some(everything)) => (&rest[..everything], ManageKind::Everything),
                (None, None) => return None,
            };
            let asker = one_line(&app_name(label));
            let words = key_words(summary);
            if asker.is_empty() || words.is_empty() {
                return None;
            }
            let body = match does {
                ManageKind::Door => format!(
                    "{asker} can then show {computer}'s pairing code and add phones and computers to it."
                ),
                ManageKind::Everything => {
                    format!("{asker} can then do everything you can do on {computer}.")
                }
            };
            Some(PlainAsk {
                question: format!("Let {asker} manage {computer}?"),
                body: Some(body),
                check: Some(format!("Approve only if {asker} shows the same words.")),
                words,
            })
        }
        _ => None,
    }
}

enum ManageKind {
    Door,
    Everything,
}

/// The name in `… calls itself "m7-arch" …`.
fn quoted_name(summary: &str) -> Option<String> {
    let name = summary
        .split_once("calls itself \"")?
        .1
        .split_once('"')?
        .0
        .trim();
    let name = one_line(name);
    (!name.is_empty()).then_some(name)
}

/// Longest name a question shows: past it the rest is cut ("…").
const NAME_MAX_CHARS: usize = 80;

/// A name the asker chose, as one short line: line breaks and other control characters become
/// spaces and it stops at [`NAME_MAX_CHARS`], so it can't push the question's own words off the
/// card or the Touch ID prompt (review of 8abb0a8db; an app's key label has no limit).
fn one_line(name: &str) -> String {
    let flat = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= NAME_MAX_CHARS {
        return flat;
    }
    let mut cut: String = flat.chars().take(NAME_MAX_CHARS).collect();
    cut.push('…');
    cut
}

/// The three words in `… the same three words: a · b · c. …`.
fn detail_words(detail: Option<&str>) -> Vec<String> {
    detail
        .and_then(|detail| detail.split_once("three words: "))
        .and_then(|(_, rest)| rest.split_once('.'))
        .map(|(words, _)| {
            words
                .split('·')
                .map(|word| word.trim().to_string())
                .filter(|word| !word.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The words in `… (key words: angle easel magnet monkey)`.
fn key_words(summary: &str) -> Vec<String> {
    summary
        .rsplit_once("(key words: ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(words, _)| words.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

/// "m7-arch" from "m7-arch (Ghostex app)".
fn app_name(label: &str) -> String {
    label
        .trim()
        .trim_end_matches("(Ghostex app)")
        .trim_end_matches("(approvals)")
        .trim()
        .to_string()
}
