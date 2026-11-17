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
                let body = if summary.contains("signs in over SSH") {
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
            let (label, does) =
                if let Some((label, _)) = rest.split_once(" show the pairing code of ") {
                    (label, ManageKind::Door)
                } else if let Some((label, _)) = rest.split_once(" act as you on ") {
                    (label, ManageKind::Everything)
                } else {
                    return None;
                };
            let asker = app_name(label);
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
    (!name.is_empty()).then(|| name.to_string())
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
