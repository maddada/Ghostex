//! A coordinator's board, beside its chat: its lines of work grouped by who has the ball, which the
//! Board pill opens. gxserver decides each thread's and item's group (server/src/coordinators/board.rs)
//! and sends the board inside `coordinatorThreads` (`board`); this adds the chat's own unanswered
//! questions to Needs you, folds and caps the groups, words every row and tag once for every
//! renderer, and counts the pill.
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, after the design pages (Q1236 side panel, Q1243 "Yes, build it"): one list grouped by
//! status beside the chat, groups in the order Later, Needs you, Blocked, In progress, Done ("shift
//! Later to be most left one"), and after the first build "please move ideas to the top above
//! later", so Ideas comes first, folded; tags say what a row is (its thread, question or
//! item, its PR with a CI dot, its release, its computer, untested); and "keep this board light":
//! a few rows per group with "N more", Done for today only and folded. A row opens its thread, its
//! question card or its link. The user's words on the groups are in server/src/coordinators/board.rs.
//! SEE-ALSO: apps/desktop/src/app/native_chat/coordinator_board.rs (the side panel),
//! apps/mobile/app/src/chat/native/cards/CoordinatorBoard.tsx (the phone's sheet).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::document::ChatPill;
use crate::state::{ChatContext, ChatState, Stamped};

/// The pill's id; its renderers open the board instead of a pill menu.
pub const BOARD_PILL: &str = "board";

/// Rows a group shows before its "N more".
pub const GROUP_ROWS: usize = 5;

/// Each group's id and title, in the board's order.
const GROUPS: [(&str, &str); 8] = [
    // The `ideas` group, shown as Backlog. User: "rename Ideas to Backlog in the board please".
    ("ideas", "Backlog"),
    ("later", "To do"),
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    // Waiting on something outside: the user's 2026-11-06 words on server/src/coordinators/board.rs
    // `BOARD_GROUPS`. No pill of its own: nothing in it needs the user.
    ("hold", "On hold"),
    // CDXC:Coordinators 2026-11-07 DECISION:
    // User, with a screenshot of the board's side list showing "Moving 3": "rename to in progress
    // please". The board's `moving` group is read as In progress everywhere (side list, Columns
    // overlay, pill tooltip, phone sheet, `ghostex coordinator board` and its help, the playbook
    // and the docs). The stored key stays `moving`; `in-progress` is accepted as an alias by
    // `parse_group` (server/src/coordinators/board.rs). The Unified sidebar's Moving shelf is a
    // different thing and keeps its name.
    ("moving", "In progress"),
    ("done", "Done today"),
    ("removed", "Removed"),
];

/// Groups folded until the reader opens them.
fn folded_by_default(group: &str) -> bool {
    matches!(group, "done" | "ideas" | "removed")
}

/// One chip on a row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardTag {
    /// `pr`, `issue`, `linear`, `link`, `branch`, `release`, or `question` (its Q tags).
    pub kind: String,
    pub label: String,
    /// How it is tinted: `question`, `good`, `bad`, `running`, `merged`, `closed`, `held` or ""
    /// (muted).
    pub tone: String,
    /// What a click on the chip does (a PR opens), or `null`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
    /// The chip's full words for its tooltip (a PR's title, a release's whole name), "" for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// A `userTag` chip's own colour (`#rrggbb`) and glyph (`titlebar/<icon>.svg`), as the
    /// sidebar draws the tag; "" for every other chip.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

/// One row of a card's menu (right-click, the ⋯ button, a long press on the phone).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardMenuItem {
    pub label: String,
    /// `titlebar/<icon>.svg` on the desktop, the phone's glyph of the same name.
    pub icon: String,
    /// What picking it sends, `null` for `Tags`, which lists the board's `tagOptions` instead.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
    /// `tags` for the row that opens the tag list, "" otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub submenu: String,
    /// Remove: drawn in the destructive red where the platform does.
    #[serde(default)]
    pub destructive: bool,
}

/// One tag the Tags list offers: the sidebar's session tags (server/src/coordinators/board.rs
/// `add_tag_looks`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardTagOption {
    pub value: String,
    pub label: String,
    pub icon: String,
    pub color: String,
}

/// One line of work.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardRow {
    pub key: String,
    pub title: String,
    /// Its latest status in one line.
    pub detail: String,
    /// How long ago it last moved ("4m", "yesterday").
    pub age: String,
    pub tags: Vec<BoardTag>,
    /// What a click on the card itself does (a question jumps to its card), or `null`: a thread
    /// opens only from its agent button, a link only from its chip.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
    /// What the agent button sends to open the thread, or `null` for a row that is no thread.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub open_thread: Value,
    /// The thread's agent icon (`agent-icons/<icon>.svg`), "" for a row that is no thread.
    #[serde(default)]
    pub agent_icon: String,
    /// It waits on an approval, which takes the sidebar's red.
    pub needs_approval: bool,
    /// What the two-click × sends (`coordinatorBoardChange`, op `drop`), or `null` for a row that
    /// cannot be removed (a question, a row already under Removed).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub remove: Value,
    /// What Restore sends on a row under Removed, else `null`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub restore: Value,
    /// The Markdown link Mention in chat (or an ⌥-click) puts in the chat box, so the user can
    /// comment on it:
    /// `[title](board:B4)` for an item, the thread's session ref for a thread (the + mention's
    /// form); "" for a question, whose click jumps to its card instead.
    #[serde(default)]
    pub reference: String,
    /// The coordinator's own line for it, which the second line shows when it has no links.
    #[serde(default)]
    pub note: String,
    /// One-click answers for an item that asks the user something; the second line shows them in
    /// place of the links.
    #[serde(default)]
    pub answers: Vec<BoardAnswer>,
    /// Muted words before the answers: the work a Work panel ask belongs to and whether it holds
    /// it up ("iPhone push · Holds it up", work_panel.rs); "" elsewhere.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lead: String,
    /// A work item ask's longer explanation under its short title (`item ask --detail`), which
    /// an opened card shows in the quieter colour; "" elsewhere.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    /// A work item ask's title's links as word runs (`questions::form_links::word_runs`), null
    /// when it has none.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub title_runs: Value,
    /// The words a Reply gave a work item ask (none of its answers), which its Reply button
    /// shows; "" when an answer was picked or none was given.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub replied: String,
    /// How much a Ghostex page still asks, drawn after its title in the quieter colour
    /// ("(14 questions)", "(10/16 answered)", "(not seen yet)"; work_cards.rs `page_count`); ""
    /// for anything else.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub count: String,
    /// The card is open: it shows its whole line of work and its buttons (one card at a time).
    #[serde(default)]
    pub expanded: bool,
    /// What a plain click on the card sends: it opens the card, or closes it when it is open.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub toggle: Value,
    /// What the menu's first row opens (its thread, its link, its question), with its words ("Open
    /// thread", "Open PR #214"); `null` and "" for neither. The opened card itself shows no Open
    /// button: its link chip and agent button already open these (each thing once, 2026-11-06).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub open: Value,
    #[serde(default)]
    pub open_label: String,
    /// What Mark done sends, `null` for a card that is done already (or a question).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub done: Value,
    /// What a drag of the card onto another group sends once the renderer adds `"group"`, `null`
    /// for a card that cannot move (a question).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub move_to: Value,
    /// The user's tag (one of the sidebar's), "" for none.
    #[serde(default)]
    pub tag: String,
    /// What picking a tag sends once the renderer adds `"tag"` ("" clears it), `null` for a card
    /// that cannot be tagged here (a question, a thread on another computer).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub set_tag: Value,
    /// The card's menu, in order: Open, Mention in chat, Tags, Mark done, Remove (or Restore).
    #[serde(default)]
    pub menu: Vec<BoardMenuItem>,
    /// What the opened card offers besides Mention, the same as the dock's opened row
    /// (`work_cards::panel_actions`): `{reply, open, showInChat, dismiss, snooze}`, `null` for none.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub actions: Value,
}

/// One answer on a card that asks the user something.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardAnswer {
    pub label: String,
    pub recommended: bool,
    /// The answer given for the card, here or by any client.
    pub picked: bool,
    /// The card is answered: every answer is shown but none can be clicked again.
    pub locked: bool,
    /// What a click sends (`coordinatorBoardAnswer`), `null` once the card is answered.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
}

/// One group.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardGroup {
    /// `later`, `needs`, `blocked`, `moving`, `done` or `ideas`.
    pub id: String,
    pub title: String,
    pub count: usize,
    pub open: bool,
    pub rows: Vec<BoardRow>,
    /// "4 more" or "Show fewer" under the rows, "" for neither.
    pub more_label: String,
    /// Rows gxserver left out of the chat's copy, which `ghostex coordinator board` lists.
    pub not_shown: usize,
    /// Every row the chat has for the group, whatever its fold: one column of the columns view.
    pub column_rows: Vec<BoardRow>,
}

/// The `coordinatorBoard` document value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorBoard {
    /// The panel's name: "Work" when it lists the coordinator's work items
    /// (`extras::work_panel`), "Board" from a gxserver that sends only the board.
    #[serde(default)]
    pub title: String,
    pub groups: Vec<BoardGroup>,
    /// "2 need you · 1 blocked · 4 in progress", the panel's header line.
    pub meta: String,
    /// The pill's coloured counts, in the order it shows them: needs, blocked, moving, each only
    /// when above zero.
    pub pill_counts: Vec<(String, usize)>,
    /// Nothing open and nothing done today.
    pub empty: bool,
    /// The row a board pill in the chat asked to see, `{key, seq}`, or `null`: the renderers open
    /// the board and scroll to the row `key` names each time `seq` changes (`show_item`).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub focus: Value,
    /// The tags a card's Tags list offers, the sidebar's own (gxserver's `tagOptions`).
    #[serde(default)]
    pub tag_options: Vec<BoardTagOption>,
    /// Every item on the board by its ref (`B4`) with its title now, the ones the folded and
    /// capped groups leave out too (gxserver's `items`): a board mention in a message
    /// ([`board_reference`]) reads the title from it, and draws dimmed when its ref is not in it.
    /// `None` when gxserver sends no index, which dims nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_titles: Option<std::collections::BTreeMap<String, String>>,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn tag(kind: &str, label: impl Into<String>, tone: &str) -> BoardTag {
    BoardTag {
        kind: kind.to_string(),
        label: label.into(),
        tone: tone.to_string(),
        command: Value::Null,
        title: String::new(),
        color: String::new(),
        icon: String::new(),
    }
}

/// When an ISO time was, in epoch milliseconds.
fn at_ms(at: &str) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(at).map(|at| at as i64)
}

/// The change `op` of a row gxserver sent: the item it names (`B7`), or its thread.
fn change_command(row: &Value, op: &str) -> Value {
    let mut command = json!({"type": "coordinatorBoardChange", "op": op});
    match text(row, "kind") {
        "item" => command["item"] = json!(text(row, "ref")),
        "thread" => {
            command["threadServerId"] = json!(text(row, "serverId"));
            command["threadProjectId"] = json!(text(row, "projectId"));
            command["threadSessionId"] = json!(text(row, "sessionId"));
        }
        _ => return Value::Null,
    }
    command
}

/// The release chip's short words: "Held \u{b7} <release>" while the release's name is short, its
/// first part when it names several ("next release + TestFlight"), else just "Held"; the chip's
/// tooltip has the whole name.
///
/// CDXC:Coordinators 2026-11-05 WHY:
/// User, of "Held \u{b7} next full release + Tes" cut off at a card's edge: "stuff cut off man". A
/// chip line must fit its card, so a long release name gives way and its words move to the tooltip.
fn release_chip_label(release: &str) -> String {
    const SHORT: usize = 16;
    let first = release
        .split(['+', ',', '(', ';'])
        .next()
        .unwrap_or_default()
        .trim()
        .replace("full ", "");
    if first.is_empty() {
        "Held".to_string()
    } else if first.chars().count() <= SHORT {
        format!("Held \u{b7} {first}")
    } else {
        "Held".to_string()
    }
}

/// A Markdown link for the chat box, its label escaped the way the + mention escapes one.
fn markdown_link(label: &str, destination: &str) -> String {
    let label: String = label
        .chars()
        .flat_map(|character| {
            if matches!(character, '\\' | '[' | ']') {
                vec!['\\', character]
            } else {
                vec![character]
            }
        })
        .collect();
    format!("[{label}]({destination})")
}

/// A GitHub PR or issue link's colour, the same on a board chip and a work item card's part:
/// merged, closed, failing or running checks, open, or neutral while its state is unknown.
pub fn link_tone(link: &Value) -> &'static str {
    let kind = text(link, "kind");
    let state = text(link, "state");
    match (kind, state, text(link, "checks")) {
        ("pr", "merged", _) => "merged",
        ("pr", "closed", _) | ("issue", "closed", _) => "closed",
        // A draft, or a PR not read yet: neutral until its state is known (Q2167 follow-up,
        // the user's "#536 grey, #526 green"; server/src/coordinators/board.rs
        // `note_item_github_link`).
        ("pr", "draft" | "", _) => "",
        ("pr", _, "failing") => "bad",
        ("pr", _, "pending") => "running",
        ("pr", _, _) | ("issue", "open", _) => "good",
        _ => "",
    }
}

/// The key a row's answer is locked under: its own, or a work item's Done's (`answerKey`, shared
/// with its dock row).
fn answer_key(row: &Value) -> &str {
    match text(row, "answerKey") {
        "" => text(row, "key"),
        key => key,
    }
}

/// A row gxserver sent, worded.
fn board_row(row: &Value, now_ms: i64, picked: Option<&str>) -> BoardRow {
    let open = |url: &str| json!({"type": "openMarkdownLink", "href": url});
    let mut tags = Vec::new();
    let open_thread_command = json!({
        "type": "openCoordinatorThread",
        "serverId": row.get("serverId").cloned().unwrap_or(json!("")),
        "projectId": row["projectId"],
        "sessionId": row["sessionId"],
    });
    // A thread waiting on the user says what to do about it, first on its second line.
    //
    // CDXC:Coordinators 2026-11-05 DECISION:
    // User, of a Needs you card that said only "No links yet": "Wtf can i do with this card???????".
    // A Needs you card always says what the user can do: a thread waiting on them opens from
    // "Answer in thread" or "Approve in thread", an item that asks something shows its answers, and
    // an item's own note stands in for links it does not have.
    // A thread not moving even after Ghostex nudged it: the red chip, which opens the thread
    // (CDXC:Coordinators 2026-11-05 in server/src/coordinators/stall.rs).
    if row["stuck"] == true {
        let mut chip = tag("action", "Not moving", "bad");
        chip.command = open_thread_command.clone();
        chip.title = "No progress for a while, even after Ghostex nudged it".to_string();
        tags.push(chip);
    }
    // What a thread that ended its turn still owes (server/src/coordinators/board.rs): a chip that
    // opens it, so a thread put to sleep mid-task never looks parked.
    let flag = text(row, "flag");
    if !flag.is_empty() && row["stuck"] != true {
        let mut chip = tag("action", flag, text(row, "flagTone"));
        chip.command = open_thread_command.clone();
        chip.title = match text(row, "unfinished") {
            "jobRunning" => {
                "It ended its turn waiting on a background job that still runs".to_string()
            }
            "waitsOnJob" | "queued" => {
                "Its background job ended and it has not read the result".to_string()
            }
            "carriesOn" => "Its last message says what it does next, but it stopped".to_string(),
            "cutOff" => {
                "Its turn was cut off before it finished (put to sleep, or its agent stopped)"
                    .to_string()
            }
            _ => String::new(),
        };
        tags.push(chip);
    }
    // A finished thread nothing is expected of, under Done for the day.
    if text(row, "reason") == "idle" {
        let mut chip = tag("state", "Idle", "");
        chip.title = "Finished its turn with nothing pending; not marked done".to_string();
        tags.push(chip);
    }
    match text(row, "waiting") {
        "approval" => {
            let mut chip = tag("action", "Approve in thread", "bad");
            chip.command = open_thread_command.clone();
            tags.push(chip);
        }
        "question" => {
            let mut chip = tag("action", "Answer in thread", "question");
            chip.command = open_thread_command.clone();
            tags.push(chip);
        }
        _ => {}
    }
    let mut has_pr = false;
    for link in row["links"].as_array().into_iter().flatten() {
        let kind = text(link, "kind");
        let tone = link_tone(link);
        has_pr |= kind == "pr";
        let mut chip = tag(kind, text(link, "label"), tone);
        if let Some(url) = link.get("url").and_then(Value::as_str) {
            chip.command = open(url);
            chip.title = url.to_string();
        }
        let title = text(link, "title");
        if !title.is_empty() {
            chip.title = title.to_string();
        }
        tags.push(chip);
    }
    // A PR stands for its branch; with none, the branch says where the work is.
    let branch = text(row, "branch");
    if !has_pr && !branch.is_empty() {
        tags.push(tag("branch", branch, ""));
    }
    if text(row, "reason") == "held" || !text(row, "release").is_empty() {
        let release = text(row, "release");
        let mut chip = tag("release", release_chip_label(release), "held");
        chip.title = match release {
            "" => "Held for release".to_string(),
            release => format!("Held for {release}"),
        };
        tags.push(chip);
    }
    // The user's own tag (one of the sidebar's), first: it is the chip they chose.
    if let Some(look) = row.get("tagLook").filter(|look| look.is_object()) {
        let mut chip = tag("userTag", text(look, "label"), "");
        chip.color = text(look, "color").to_string();
        chip.icon = text(look, "icon").to_string();
        chip.title = format!("Tagged {}", text(look, "label"));
        tags.insert(0, chip);
    }
    // A work item's own chips: who has the ball, its steps, what it asks (work_panel.rs).
    for chip in row["workChips"].as_array().into_iter().flatten() {
        tags.push(tag(
            text(chip, "kind"),
            text(chip, "label"),
            text(chip, "tone"),
        ));
    }
    let open_thread = match text(row, "kind") {
        "thread" => open_thread_command,
        _ => Value::Null,
    };
    // What Open opens: the thread, else the item's link, worded by what the link is.
    let (open_command, open_label) = if open_thread.is_object() {
        (open_thread.clone(), "Open thread".to_string())
    } else if let Some(link) = row["links"].as_array().and_then(|links| links.first()) {
        let label = text(link, "label");
        let words = match text(link, "kind") {
            "pr" => format!("Open PR {label}"),
            "issue" => format!("Open issue {label}"),
            "linear" => format!("Open {label}"),
            _ => "Open link".to_string(),
        };
        match link.get("url").and_then(Value::as_str) {
            Some(url) => (open(url), words),
            None => (Value::Null, String::new()),
        }
    } else {
        (Value::Null, String::new())
    };
    let group = text(row, "group");
    // A thread on another computer is done and tagged from its own sidebar.
    let here = text(row, "kind") == "item" || text(row, "serverId").is_empty();
    let agent = text(row, "agent");
    let title = text(row, "title");
    let reference = match text(row, "kind") {
        "item" => markdown_link(title, &format!("board:{}", text(row, "ref"))),
        "thread" if !text(row, "globalRef").is_empty() => {
            markdown_link(title, text(row, "globalRef"))
        }
        _ => String::new(),
    };
    // The answer recorded by gxserver wins over this client's own pick; either locks the card.
    let given = Some(text(row, "answered"))
        .filter(|answered| !answered.is_empty())
        .or(picked);
    let answers = row["answers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|answer| {
            let label = text(answer, "label");
            BoardAnswer {
                label: label.to_string(),
                recommended: answer["recommended"] == true,
                picked: given == Some(label),
                locked: given.is_some(),
                command: if given.is_some() {
                    Value::Null
                } else {
                    match text(row, "ask") {
                        // A work item's ask names its tag (work_panel.rs, as its chat card does).
                        "" => json!({
                            "type": "coordinatorBoardAnswer",
                            "key": answer_key(row),
                            "item": text(row, "ref"),
                            "label": label,
                            "text": format!("{}: {label}", text(row, "ref")),
                        }),
                        ask => json!({
                            "type": "coordinatorBoardAnswer",
                            "key": text(row, "key"),
                            "item": text(row, "ref"),
                            "ask": ask,
                            "label": label,
                            "text": format!("{} {ask}: {label}", text(row, "ref")),
                        }),
                    }
                },
            }
        })
        .collect();
    BoardRow {
        reference,
        // A finished thread with no links of its own says how its last report began, so a Done or
        // To do card's second line is still worth reading.
        //
        // CDXC:Coordinators 2026-11-05 WHY:
        // User, of Done cards that all read "No links yet": the line should be useful.
        //
        // CDXC:Coordinators 2026-11-06 WHY:
        // A card with no links read "No links yet" although the thread was fine (the user's
        // "Confusing notices audit" card). An In progress card falls back to its detail too, which is
        // the line it is on now, not its last report (server/src/coordinators/live.rs); a Needs
        // you card keeps its "Answer in thread" chip alone.
        note: match text(row, "note") {
            "" if text(row, "kind") == "thread"
                && matches!(
                    text(row, "group"),
                    "done" | "later" | "moving" | "blocked" | "hold"
                ) =>
            {
                text(row, "detail").to_string()
            }
            note => note.to_string(),
        },
        answers,
        lead: String::new(),
        body: text(row, "body").to_string(),
        title_runs: row.get("titleRuns").cloned().unwrap_or(Value::Null),
        count: String::new(),
        replied: given
            .filter(|given| {
                !row["answers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|answer| text(answer, "label") == *given)
            })
            .unwrap_or_default()
            .to_string(),
        key: text(row, "key").to_string(),
        title: title.to_string(),
        detail: text(row, "detail").to_string(),
        age: at_ms(text(row, "at"))
            .map(|at| crate::transcript::message_time::age_label(now_ms, at))
            .unwrap_or_default(),
        tags,
        // A card opens nothing by itself: its links open their pages, and its agent button its
        // thread.
        command: Value::Null,
        agent_icon: if open_thread.is_null() {
            String::new()
        } else {
            crate::extras::welcome::welcome_agent_icon((!agent.is_empty()).then_some(agent), None)
                .unwrap_or_else(|| "claude".to_string())
        },
        open_thread,
        needs_approval: row["needsApproval"] == true,
        remove: if text(row, "group") == "removed" {
            Value::Null
        } else {
            change_command(row, "drop")
        },
        restore: if text(row, "group") == "removed" {
            change_command(row, "restore")
        } else {
            Value::Null
        },
        expanded: false,
        toggle: json!({
            "type": "toggleCoordinatorBoardGroup",
            "group": format!("{CARD_KEY_PREFIX}{}", text(row, "key")),
        }),
        open: open_command,
        open_label,
        done: if here && !matches!(group, "done" | "removed") {
            change_command(row, "done")
        } else {
            Value::Null
        },
        move_to: change_command(row, "move"),
        tag: row
            .get("tagLook")
            .filter(|look| look.is_object())
            .map(|look| text(look, "value").to_string())
            .unwrap_or_default(),
        set_tag: if here {
            change_command(row, "tag")
        } else {
            Value::Null
        },
        menu: Vec::new(),
        actions: Value::Null,
    }
}

/// The `board_toggled` entries that name an open card rather than a group: `card:<row key>`.
pub const CARD_KEY_PREFIX: &str = "card:";

/// A card's menu from what it can do, in the order every renderer shows it.
///
/// CDXC:Coordinators 2026-11-06 DECISION:
/// User, on the card-actions page (Q1452 "shorter"): "keep in mind please we shouldn't move stuff
/// almost at all. coordinator should move it. and we should move with drag drop not picking option
/// in 'move' menu". The menu is Open (the thread or the link), Mention in chat, Tags, Mark done and
/// Remove (Restore under Removed), with no Move to: a card changes group only by a drag, and
/// rarely, since the coordinator places cards (`ghostex coordinator board move` stays its verb).
/// The same menu opens on a right-click, from the ⋯ that takes the age's place on a hovered card
/// (Q1454), and on a long press on the phone (Q1455).
fn card_menu(row: &BoardRow, work: bool) -> Vec<BoardMenuItem> {
    let mut menu = Vec::new();
    if row.open.is_object() {
        menu.push(BoardMenuItem {
            label: row.open_label.clone(),
            icon: if row.open_thread.is_object() {
                "agent".to_string()
            } else {
                "external-link".to_string()
            },
            command: row.open.clone(),
            ..BoardMenuItem::default()
        });
    }
    if !row.reference.is_empty() {
        menu.push(BoardMenuItem {
            label: "Mention in chat".to_string(),
            icon: "message-plus".to_string(),
            command: json!({"type": "insertBoardReference", "reference": row.reference}),
            ..BoardMenuItem::default()
        });
    }
    if row.set_tag.is_object() {
        menu.push(BoardMenuItem {
            label: "Tags".to_string(),
            icon: "tag".to_string(),
            submenu: "tags".to_string(),
            ..BoardMenuItem::default()
        });
    }
    if row.done.is_object() {
        menu.push(BoardMenuItem {
            label: "Mark done".to_string(),
            icon: "circle-check".to_string(),
            command: row.done.clone(),
            ..BoardMenuItem::default()
        });
    }
    if row.remove.is_object() {
        menu.push(BoardMenuItem {
            label: if work { "Drop" } else { "Remove from board" }.to_string(),
            icon: "trash".to_string(),
            command: row.remove.clone(),
            destructive: true,
            ..BoardMenuItem::default()
        });
    }
    if row.restore.is_object() {
        menu.push(BoardMenuItem {
            label: if work { "Restore" } else { "Restore to board" }.to_string(),
            icon: "arrow-back-up".to_string(),
            command: row.restore.clone(),
            ..BoardMenuItem::default()
        });
    }
    menu
}

/// `coordinatorBoardChange {op, item | threadServerId, threadProjectId, threadSessionId}`: a row
/// moved to Removed (`drop`) or restored, marked done (a thread's is its resolve), dragged to
/// another group (`move` with `group`; onto Done or Removed it is those) or tagged (`tag`, "" to
/// clear), asked of gxserver for this coordinator's board. The
/// board itself changes when the next frame brings it.
pub fn change(
    state: &mut ChatState,
    params: &serde_json::Map<String, Value>,
    request_id: u64,
) -> Option<crate::effect::Effect> {
    let group = params
        .get("group")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // A drag onto Done marks the card done, onto Removed removes it.
    let op = match (params.get("op").and_then(Value::as_str)?, group) {
        ("move", "") => return None,
        ("move", "done") => "done",
        ("move", "removed") => "drop",
        (op, _) => op,
    };
    if !matches!(op, "drop" | "restore" | "done" | "move" | "tag") {
        return None;
    }
    let thread = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or_default();
    // A thread's Done is its resolve, as the sidebar's Mark Thread Done and `ghostex coordinator
    // board done` do it: gxserver resolves it (server/src/server/route_http/sessions.rs
    // `board_card_resolves_thread`). A thread on another computer is done from its own sidebar.
    let resolves = op == "done" && !params.contains_key("item");
    if resolves && (!thread("threadServerId").is_empty() || thread("threadSessionId").is_empty()) {
        return None;
    }
    let mut body = json!({
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        "op": op,
    });
    if resolves {
        body["resolve"] = json!(true);
    }
    if op == "move" {
        body["group"] = json!(group);
    }
    if op == "tag" {
        body["tag"] = json!(params
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or_default());
    }
    for key in [
        "item",
        "threadServerId",
        "threadProjectId",
        "threadSessionId",
    ] {
        if let Some(value) = params.get(key).filter(|value| value.is_string()) {
            body[key] = value.clone();
        }
    }
    state.extras.panels.board_change_requests.insert(request_id);
    Some(crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::CoordinatorBoard,
        params: Box::new(body),
    })
}

/// Takes the answer to [`change`]: nothing to do when it worked, a toast when gxserver refused.
/// `None` when `request_id` is not a board change.
pub fn settle_change(
    panels: &mut crate::state::PanelsState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<crate::effect::Effect>> {
    if !panels.board_change_requests.remove(&request_id) {
        return None;
    }
    Some(match answer {
        Ok(_) => Vec::new(),
        Err(message) => vec![crate::effect::Effect::Toast {
            level: "error".to_string(),
            message: if message.is_empty() {
                "Couldn't change the board.".to_string()
            } else {
                message
            },
        }],
    })
}

/// A chip for each question this chat asked that an item names in its title or note (`Q12`),
/// tinted by where the question stands; a click jumps to it. It tells an item waiting on an answer
/// from a task for the user, and gxserver moves the item to Done once the question is settled
/// (server/src/coordinators/board.rs `settle_answered_items`).
fn question_chips(state: &ChatState, row: &Value) -> Vec<BoardTag> {
    use crate::transcript::open_questions::{tag_state, tag_words, TagState, QUESTION_LINK_PREFIX};
    let askers = &state.transcript_view.askers;
    tag_words(&format!("{}\n\n{}", text(row, "title"), text(row, "note")).replace('`', ""))
        .into_iter()
        .filter_map(|word| {
            let tone = match tag_state(askers, &word)? {
                TagState::Open => "question",
                TagState::Answered => "good",
                TagState::Withdrawn | TagState::Skipped => "",
            };
            let mut chip = tag("question", word.clone(), tone);
            chip.command = json!({"type": "openMarkdownLink", "href": format!("{QUESTION_LINK_PREFIX}{word}")});
            chip.title = format!("Go to {word}");
            Some(chip)
        })
        .collect()
}

/// The chat's own unanswered questions (forms and question pages not put off For later), as
/// Needs you rows, newest first.
fn question_rows(state: &ChatState, now_ms: i64) -> Vec<BoardRow> {
    let mut rows: Vec<(Option<i64>, BoardRow)> =
        crate::questions::open_questions::unanswered(state)
            .into_iter()
            .map(|asker| {
                let tags = match asker.tags.as_slice() {
                    [] => Vec::new(),
                    [only] => vec![tag("question", only.clone(), "question")],
                    [first, .., last] => {
                        vec![tag(
                            "question",
                            format!("{first}\u{2013}{last}"),
                            "question",
                        )]
                    }
                };
                let questions = asker.tags.len().max(1);
                let title = crate::questions::form_links::plain_words(&asker.title);
                (
                    asker.at,
                    BoardRow {
                        key: format!("question:{}", asker.id),
                        title: if title.is_empty() {
                            "Questions for you".to_string()
                        } else {
                            title
                        },
                        detail: if questions == 1 {
                            "1 question waiting".to_string()
                        } else {
                            format!("{questions} questions waiting")
                        },
                        age: asker
                            .at
                            .map(|at| crate::transcript::message_time::age_label(now_ms, at))
                            .unwrap_or_default(),
                        tags,
                        command: json!({"type": "jumpToQuestion", "id": asker.id}),
                        needs_approval: false,
                        open_thread: Value::Null,
                        agent_icon: String::new(),
                        reference: String::new(),
                        note: String::new(),
                        answers: Vec::new(),
                        lead: String::new(),
                        body: String::new(),
                        title_runs: Value::Null,
                        replied: String::new(),
                        count: if matches!(
                            asker.kind,
                            crate::transcript::open_questions::AskerKind::Page
                        ) {
                            crate::extras::work_cards::page_count(state, &asker)
                        } else {
                            String::new()
                        },
                        // A question leaves by being answered, or put off For later in Resources.
                        remove: Value::Null,
                        restore: Value::Null,
                        // A question's card opens its form, which is where it is answered.
                        expanded: false,
                        // A click opens the card in place, as every card does (the user's "make it
                        // be cohesive, so clicking doesn't sometimes expand and sometimes just
                        // mentions"); Open and Reply are in it.
                        toggle: json!({
                            "type": "toggleCoordinatorBoardGroup",
                            "group": format!("{CARD_KEY_PREFIX}question:{}", asker.id),
                        }),
                        open: json!({"type": "jumpToQuestion", "id": asker.id}),
                        open_label: "Go to question".to_string(),
                        done: Value::Null,
                        move_to: Value::Null,
                        tag: String::new(),
                        set_tag: Value::Null,
                        menu: Vec::new(),
                        actions: Value::Null,
                    },
                )
            })
            .collect();
    rows.sort_by(|left, right| right.0.cmp(&left.0));
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The board, or `None` on a chat that is not a coordinator or whose gxserver sends no board.
pub fn coordinator_board(state: &ChatState, context: &ChatContext) -> Option<CoordinatorBoard> {
    coordinator_board_with(
        state,
        context,
        &crate::extras::work_cards::turns_for_work(state),
    )
}

/// [`coordinator_board`], over the chat's turns the caller already read
/// (`work_cards::turns_for_work`): every work row's Show in chat reads them.
pub fn coordinator_board_with(
    state: &ChatState,
    context: &ChatContext,
    turns: &[crate::extras::work_cards::Turn],
) -> Option<CoordinatorBoard> {
    let threads = state.session.coordinator_threads.as_deref()?;
    // The work items, when gxserver sends them, laid out as the board was (work_panel.rs).
    let work_mode = threads.get("work").is_some_and(Value::is_object);
    let built;
    let board = if work_mode {
        built = crate::extras::work_panel::work_board_at(&threads["work"], context.now_millis());
        &built
    } else {
        threads.get("board")?
    };
    let groups = board.get("groups")?.as_array()?;
    let group_list: &[(&str, &str)] = if work_mode {
        &crate::extras::work_panel::WORK_GROUPS
    } else {
        &GROUPS
    };
    let now_ms = context.now_millis();
    let panels = &state.extras.panels;
    let mut out = Vec::new();
    // How many of the Needs you rows are the chat's own open questions (`question_rows`).
    let mut asked = 0;
    for &(id, title) in group_list {
        let sent = groups.iter().find(|group| text(group, "id") == id);
        let mut rows: Vec<BoardRow> = sent
            .and_then(|group| group.get("rows"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|row| {
                let mut out = board_row(
                    row,
                    now_ms,
                    panels
                        .board_answers
                        .get(answer_key(row))
                        .map(String::as_str),
                );
                if text(row, "kind") == "item" {
                    let chips = question_chips(state, row);
                    out.tags.splice(0..0, chips);
                }
                // The opened card's actions, the dock's (Reply, Show in chat, Dismiss, Snooze).
                if work_mode {
                    out.actions =
                        crate::extras::work_cards::panel_actions_with(state, context, row, turns);
                }
                // A work item's ask is answered or done, never dropped, moved or tagged by itself:
                // those act on its item (work_panel.rs).
                if row["askRow"] == true {
                    out.lead = text(row, "detail").to_string();
                    // The lead says it; an opened card does not repeat it as its line.
                    out.detail = String::new();
                    out.note = String::new();
                    out.remove = Value::Null;
                    out.restore = Value::Null;
                    out.done = Value::Null;
                    out.move_to = Value::Null;
                    out.set_tag = Value::Null;
                }
                out
            })
            .collect();
        let mut total = sent
            .and_then(|group| group.get("total"))
            .and_then(Value::as_u64)
            .map_or(rows.len(), |total| total as usize);
        if id == "needs" {
            let mut questions = question_rows(state, now_ms);
            // Each with the dock's actions for its form or page (Reply, Open, Dismiss, Snooze).
            for question in &mut questions {
                if let Some(id) = question.key.strip_prefix("question:") {
                    question.actions =
                        crate::extras::work_cards::question_actions(state, context, id);
                }
            }
            asked = questions.len();
            total += questions.len();
            rows.splice(0..0, questions);
        }
        for row in &mut rows {
            row.expanded = row.toggle.is_object()
                && panels
                    .board_toggled
                    .contains(&format!("{CARD_KEY_PREFIX}{}", row.key));
            row.menu = card_menu(row, work_mode);
        }
        // A group shows only while it holds something.
        //
        // CDXC:Coordinators 2026-11-05 DECISION:
        // User: "lets not show the empty statuses in the list (to make it less crowded and not have
        // bunch of Blocked 0 Moving 0 etc." A group with nothing in it is left out of every view and
        // comes back when something lands in it.
        if total == 0 {
            continue;
        }
        let not_shown = total.saturating_sub(rows.len());
        let folded = if work_mode {
            crate::extras::work_panel::folded_by_default(id)
        } else {
            folded_by_default(id)
        };
        let open = folded == panels.board_toggled.contains(id);
        let expanded = panels.board_expanded.contains(id);
        let more_label = if rows.len() <= GROUP_ROWS {
            String::new()
        } else if expanded {
            "Show fewer".to_string()
        } else {
            format!("{} more", rows.len() - GROUP_ROWS)
        };
        let column_rows = rows.clone();
        if !expanded {
            rows.truncate(GROUP_ROWS);
        }
        out.push(BoardGroup {
            id: id.to_string(),
            title: title.to_string(),
            count: total,
            open,
            rows: if open { rows } else { Vec::new() },
            more_label: if open { more_label } else { String::new() },
            not_shown: if open && expanded { not_shown } else { 0 },
            column_rows,
        });
    }
    let count = |id: &str| {
        out.iter()
            .find(|group| group.id == id)
            .map_or(0, |group| group.count)
    };
    let (needs, blocked, moving) = (count("needs"), count("blocked"), count("moving"));
    let mut meta = Vec::new();
    // The Work panel's header: "3 for you · 1 blocked · 2 in progress".
    if work_mode {
        if needs > 0 {
            // The pill's words ("1 needs you", "4 need you"), so the header and the pill read the
            // same.
            meta.push(needs_you(needs));
        }
        if blocked > 0 {
            meta.push(format!("{blocked} blocked"));
        }
        if moving > 0 {
            meta.push(format!("{moving} in progress"));
        }
        let idle = count("idle");
        if meta.is_empty() && idle > 0 {
            meta.push(format!("{idle} not being worked on"));
        }
        let later = count("later");
        if meta.is_empty() && later > 0 {
            meta.push(format!("{later} later"));
        }
    }
    if needs > 0 && !work_mode {
        meta.push(needs_you(needs));
    }
    if !work_mode {
        if blocked > 0 {
            meta.push(format!("{blocked} blocked"));
        }
        if moving > 0 {
            meta.push(format!("{moving} in progress"));
        }
        let later = count("later");
        if meta.is_empty() && later > 0 {
            meta.push(format!("{later} to do"));
        }
    }
    // CDXC:Coordinators 2026-11-06 DECISION:
    // User, of the strip with the ? pill's pink dot and the Board pill's pink count: "if we show the
    // ? with pink dot no need to show it again on the board yea?" The pill's Needs you count leaves
    // out the chat's own open questions, which the ? pill already counts; the board still lists
    // them as rows, and its groups and header keep counting them.
    //
    // CDXC:Coordinators 2026-11-06 DECISION:
    // User, of "Board •1 •1 •5" in pink, red and amber: "also red for blocked need to be it's own
    // pill :))". The Blocked count leaves the Board pill for a Blocked pill beside it
    // (`blocked_pill`), which opens the board at Blocked.
    // Superseded 2026-11-10: the ? pill no longer counts what the dock above the chat box shows
    // (`questions::open_questions` `pill_count`), so the pill's Needs you is the group's whole
    // count, the same number the panel's header and groups show (the user's "the counts
    // disagree").
    let _ = asked;
    let pill_counts = [("needs", needs), ("blocked", blocked), ("moving", moving)]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(id, count)| (id.to_string(), count))
        .collect();
    let empty = out
        .iter()
        .filter(|group| !matches!(group.id.as_str(), "ideas" | "removed"))
        .all(|group| group.count == 0);
    Some(CoordinatorBoard {
        title: if work_mode { "Work" } else { "Board" }.to_string(),
        groups: out,
        meta: meta.join(" \u{b7} "),
        pill_counts,
        empty,
        focus: panels
            .board_focus
            .as_ref()
            .map_or(Value::Null, |(key, seq)| json!({"key": key, "seq": seq})),
        tag_options: board
            .get("tagOptions")
            .cloned()
            .and_then(|options| serde_json::from_value(options).ok())
            .unwrap_or_default(),
        item_titles: board.get("items").and_then(Value::as_object).map(|items| {
            items
                .iter()
                .map(|(reference, title)| {
                    let title = crate::transcript::session_mentions::pill_label(
                        title.as_str().unwrap_or_default(),
                    );
                    (reference.clone(), title)
                })
                .collect()
        }),
    })
}

/// Everything [`coordinator_board`] reads besides the clock, kept beside the board it built
/// (`document::AssembleCache`).
///
/// CDXC:Coordinators 2026-11-09 WHY:
/// User: "ghostex app gets slower with time until it becomes super slow". The board was projected and written out to JSON on every event of a coordinator chat, a large part of each event's document. It reads the clock only for its rows' ages, which move in whole minutes (`age_label`), so a kept board stands until the first of its rows' next minute, or until anything else it reads changes.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardInputs {
    threads: Stamped<Option<crate::document::SharedValue>>,
    panels: Stamped<crate::state::PanelsState>,
    askers: Stamped<Vec<crate::transcript::open_questions::Asker>>,
    questions: crate::questions::forms::QuestionsInputs,
    resource_index: Stamped<crate::questions::resource_index::ResourceIndex>,
    later_question_ids: Stamped<Vec<String>>,
    removed_question_ids: Stamped<Vec<String>>,
    /// Where the rows' Show in chat point (`work_cards::turns_mark`).
    turns: (usize, String, String, Option<String>),
}

impl BoardInputs {
    fn of(state: &ChatState, turns: &[crate::extras::work_cards::Turn]) -> Self {
        Self {
            turns: crate::extras::work_cards::turns_mark(turns),
            threads: state.session.coordinator_threads.clone(),
            panels: state.extras.panels.clone(),
            askers: state.transcript_view.askers.clone(),
            questions: crate::questions::forms::QuestionsInputs::of(&state.questions),
            resource_index: state.session.resource_index.clone(),
            later_question_ids: state.session.later_question_ids.clone(),
            removed_question_ids: state.session.removed_question_ids.clone(),
        }
    }

    /// By stamp where it can, by value where a stamp moved ([`Stamped::same_as`]).
    fn matches(&mut self, state: &ChatState, turns: &[crate::extras::work_cards::Turn]) -> bool {
        self.turns == crate::extras::work_cards::turns_mark(turns)
            && self.threads.same_as(&state.session.coordinator_threads)
            && self.panels.same_as(&state.extras.panels)
            && self.askers.same_as(&state.transcript_view.askers)
            && self.questions.matches(&state.questions)
            && self.resource_index.same_as(&state.session.resource_index)
            && self
                .later_question_ids
                .same_as(&state.session.later_question_ids)
            && self
                .removed_question_ids
                .same_as(&state.session.removed_question_ids)
    }
}

/// A kept board: its inputs, the clock it was built at, the moment its first age label changes,
/// and the value.
pub type KeptBoard = (BoardInputs, i64, Option<i64>, crate::document::SharedValue);

/// When the board next reads differently with nothing but the clock moving: the first age label on
/// it to change (`age_label_changes_at`, over every row the board can show), or the first snooze
/// to end, which moves its ask back under Needs you.
///
/// CDXC:SessionChat 2026-11-11 WHY:
/// It was the next whole minute since ANY item's time. A coordinator with 231 items has one of
/// those about every quarter of a second, so the kept board never stood and every event of the
/// chat built the whole panel again (`work_cards.rs` `turns`). An age reads in minutes for an
/// hour, then in hours, then in days, so most rows change hourly or daily.
fn ages_change_at(state: &ChatState, now_ms: i64) -> Option<i64> {
    let next = |at: i64| crate::transcript::message_time::age_label_changes_at(now_ms, at);
    let board_rows = state
        .session
        .coordinator_threads
        .as_deref()
        .and_then(|threads| threads.get("board"))
        .and_then(|board| board.get("groups"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("rows").and_then(Value::as_array))
        .flatten()
        .filter_map(|row| at_ms(text(row, "at")));
    let work = state
        .session
        .coordinator_threads
        .as_deref()
        .and_then(|threads| threads.get("work"))
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| {
            std::iter::once(text(item, "at")).chain(
                item["asks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|ask| text(ask, "askedAt")),
            )
        })
        .filter_map(at_ms);
    let board_rows = board_rows.chain(work);
    let questions = crate::questions::open_questions::unanswered(state)
        .into_iter()
        .filter_map(|asker| asker.at);
    board_rows
        .chain(questions)
        .map(next)
        .chain(crate::extras::work_cards::snoozes_end_at(state, now_ms))
        .min()
}

/// [`project`], reused while its inputs are the ones `kept` was built from and no age on it moved.
pub fn project_kept(
    state: &ChatState,
    context: &ChatContext,
    kept: &mut Option<KeptBoard>,
    turns: &[crate::extras::work_cards::Turn],
) -> crate::document::SharedValue {
    let now_ms = context.now_millis();
    if let Some((inputs, built_at, changes_at, board)) = kept.as_mut() {
        if now_ms >= *built_at
            && changes_at.is_none_or(|at| now_ms < at)
            && inputs.matches(state, turns)
        {
            return board.clone();
        }
    }
    let board = crate::document::SharedValue::from(
        coordinator_board_with(state, context, turns)
            .and_then(|board| serde_json::to_value(board).ok())
            .unwrap_or(Value::Null),
    );
    *kept = Some((
        BoardInputs::of(state, turns),
        now_ms,
        ages_change_at(state, now_ms),
        board.clone(),
    ));
    board
}

/// A Needs you count in words that agree with it: "1 needs you", "2 need you".
///
/// CDXC:Coordinators 2026-11-11 DECISION:
/// User, of the dock header's "1 need you": it should say "needs you". Every place that writes
/// this count (the Work pill, the Work panel's header, the dock's header and its "N more" line)
/// agrees with its number. Supersedes "need you" for any number.
pub fn needs_you(count: usize) -> String {
    if count == 1 {
        "1 needs you".to_string()
    } else {
        format!("{count} need you")
    }
}

/// The document value, `null` when there is no board.
pub fn project(state: &ChatState, context: &ChatContext) -> Value {
    coordinator_board(state, context)
        .and_then(|board| serde_json::to_value(board).ok())
        .unwrap_or(Value::Null)
}

/// The Board pill, ahead of Resources and Workers (extras/document.rs). Its renderers draw `pillCounts` beside its label, so it has no
/// count of its own and no corner dot.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of the pill with its coloured counts and the strip's amber and pink corner dots: "pls don't
/// show the top right dots on the board". The counts already say what needs the user and what
/// moves, so the Board pill alone never takes the waiting or running dot; other pills keep theirs.
pub fn pill(board: &Value) -> Option<ChatPill> {
    if !board.is_object() {
        return None;
    }
    let meta = text(board, "meta").to_string();
    let title = match text(board, "title") {
        "" => "Board".to_string(),
        title => title.to_string(),
    };
    // The counts in words while the strip has room ("Work · 4 need you · 7 in progress"); a tight
    // strip drops the label and keeps the coloured counts (the renderers), the tooltip saying them.
    let count_of = |id: &str| {
        board["pillCounts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|pair| pair[0] == id)
            .and_then(|pair| pair[1].as_u64())
            .unwrap_or(0)
    };
    let mut words = vec![title.clone()];
    let needs = count_of("needs");
    if needs > 0 {
        words.push(needs_you(needs as usize));
    }
    let moving = count_of("moving");
    if moving > 0 {
        words.push(format!("{moving} in progress"));
    }
    let label = words.join(" \u{b7} ");
    Some(ChatPill {
        switch: None,
        menu: Vec::new(),
        id: BOARD_PILL.to_string(),
        label,
        count: String::new(),
        waiting: String::new(),
        running: false,
        aria_label: if meta.is_empty() {
            title.clone()
        } else {
            format!("{title}, {meta}")
        },
        meta,
        // Needs you (pink) and In progress (amber), the order the board lists them; Blocked has its own
        // pill (`blocked_pill`).
        dots: board["pillCounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|pair| {
                let tone = match pair[0].as_str()? {
                    "needs" => "question",
                    "blocked" => return None,
                    _ => "working",
                };
                Some(crate::document::PillDot {
                    tone: tone.to_string(),
                    count: pair[1].as_u64()?.to_string(),
                })
            })
            .collect(),
    })
}

/// The Blocked pill's id; a click opens the board at its Blocked group (`show_group`).
pub const BLOCKED_PILL: &str = "boardBlocked";

/// The Blocked pill beside the Board pill: a red dot and how many cards are blocked, only while
/// any is.
///
/// CDXC:Coordinators 2026-11-06 DECISION:
/// User, of the strip reading "? •1" then "Board •1 •1 •5" in pink, red and amber: "also red for
/// blocked need to be it's own pill :))". Blocked is a pill of its own next to Board, shown only
/// when something is blocked, and a click opens the board at the Blocked group; the Board pill
/// keeps only its pink Needs you and amber In progress counts. Same on the phone.
///
/// CDXC:Coordinators 2026-11-06 DECISION:
/// User, of the strip reading "? •1 | Board •4 | Blocked •1 | Workers | How to use": "why blocked
/// is to the right of the board and question is to the left of it. also please remove the word
/// Blocked from the pill for this case". The pill sits left of Board, next to the ? pill (?,
/// Blocked, Board, ...), and is drawn like the ? pill: its icon, the red dot and the count, with
/// no label. "Blocked" stays in its tooltip (`meta`) and its accessibility label. Same on the phone.
pub fn blocked_pill(board: &Value) -> Option<ChatPill> {
    let blocked = board["pillCounts"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|pair| pair[0] == "blocked")
        .and_then(|pair| pair[1].as_u64())
        .filter(|count| *count > 0)?;
    Some(ChatPill {
        switch: None,
        menu: Vec::new(),
        id: BLOCKED_PILL.to_string(),
        // An icon pill like the ? pill: no word, the word is its tooltip.
        label: String::new(),
        count: String::new(),
        waiting: String::new(),
        running: false,
        meta: "Blocked".to_string(),
        aria_label: format!("Blocked, {blocked}"),
        dots: vec![crate::document::PillDot {
            tone: "blocked".to_string(),
            count: blocked.to_string(),
        }],
    })
}

/// The Work panel opens at the row `key` (an item's `work:12`, an ask's `work-ask:W3:Q12`): its
/// group unfolds and shows every row, and the renderers open the panel and ring the row, the chat
/// staying where it is (`open_questions::show`).
pub fn focus_row(state: &mut ChatState, key: &str) -> Vec<crate::effect::Effect> {
    let group = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .map(crate::extras::work_panel::work_board)
        .and_then(|board| {
            board["groups"].as_array()?.iter().find_map(|group| {
                group["rows"]
                    .as_array()?
                    .iter()
                    .any(|row| row["key"] == key)
                    .then(|| text(group, "id").to_string())
            })
        });
    let Some(group) = group else {
        return vec![crate::effect::Effect::Toast {
            level: "info".to_string(),
            message: "That isn't on this chat's Work panel any more.".to_string(),
        }];
    };
    let panels = &mut state.extras.panels;
    if crate::extras::work_panel::folded_by_default(&group) != panels.board_toggled.contains(&group)
    {
        toggle(&mut panels.board_toggled, &group);
    }
    panels.board_expanded.insert(group);
    let seq = panels.board_focus.as_ref().map_or(0, |(_, seq)| *seq) + 1;
    panels.board_focus = Some((key.to_string(), seq));
    Vec::new()
}

/// `showCoordinatorBoardGroup {group}`: the board opens at `group` (the Blocked pill), unfolded
/// and showing every row, scrolled to its first card.
pub fn show_group(state: &mut ChatState, group: &str) -> Vec<crate::effect::Effect> {
    // The Work panel's own groups when the chat has work items (`work_panel::work_board`), as the
    // dock's status line counts them; else gxserver's board.
    let threads = state.session.coordinator_threads.as_deref();
    let built = threads
        .and_then(|threads| threads.get("work"))
        .map(crate::extras::work_panel::work_board);
    let key = built
        .as_ref()
        .or_else(|| threads.and_then(|threads| threads.get("board")))
        .and_then(|board| board.get("groups"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|sent| text(sent, "id") == group)
        .and_then(|sent| {
            sent["rows"]
                .as_array()?
                .first()
                .map(|row| text(row, "key").to_string())
        });
    let Some(key) = key else {
        return Vec::new();
    };
    let work_mode = built.is_some();
    let panels = &mut state.extras.panels;
    let folded = if work_mode {
        crate::extras::work_panel::folded_by_default(group)
    } else {
        folded_by_default(group)
    };
    if folded != panels.board_toggled.contains(group) {
        toggle(&mut panels.board_toggled, group);
    }
    panels.board_expanded.insert(group.to_string());
    let seq = panels.board_focus.as_ref().map_or(0, |(_, seq)| *seq) + 1;
    panels.board_focus = Some((key, seq));
    Vec::new()
}

/// `insertBoardReference {reference, text, start, end}`: a card's reference put in the chat box at
/// the caret (`text` is the box's text, `start`..`end` its selection, in UTF-16 units), so the user
/// can comment on it.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User: "we need to be able to click on a card to add a reference to it in the chat so i can
/// comment on it". A card's Markdown link, `[title](board:B4)` for an item or the thread's session
/// ref, goes in the chat box, and the renderers focus the box. Since 2026-11-06 a plain click opens
/// the card instead (`toggle`); this runs on an ⌥-click, the open card's Mention button and the
/// menu's Mention in chat.
pub fn insert_reference(params: &serde_json::Map<String, Value>) -> Vec<crate::effect::Effect> {
    let reference = params
        .get("reference")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reference.is_empty() {
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
    // `insert_reference` spaces it off the words around it, as a mention is.
    let edit = crate::composer::references::insert_reference(
        text,
        reference,
        offset("start"),
        offset("end"),
    );
    vec![crate::effect::Effect::SetComposerText {
        content: edit.text,
        caret: Some(edit.caret),
        from_history: false,
    }]
}

/// The `kind` of a board item's reference entry.
pub const BOARD_REFERENCE_KIND: &str = "board";

/// What a board pill says on hover.
pub const SHOW_ON_BOARD: &str = "Show on the board";

/// What a board pill says on hover when its card has left the board (it draws dimmed).
pub const BOARD_ITEM_GONE: &str = "No longer on the board";

/// The item number of a board link's destination, `board:B4` (what a card's click puts in the chat
/// box, [`insert_reference`]).
pub fn board_item(href: &str) -> Option<u64> {
    let number = crate::transcript::jsstr::js_trim(href)
        .strip_prefix("board:")?
        .strip_prefix(['B', 'b'])?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().filter(|item| *item > 0)
}

/// A Markdown link to a board item as a `board` reference: the board's icon and the link's text
/// (the item's title when a card put it there) in a pill, which shows the item on the board.
///
/// The renderers draw it as the board's own tinted chip, and on a chat with a board they read
/// its label from the board's `itemTitles` under `item` (the card's title now), or draw it dimmed
/// with `goneTitle` as its tooltip when the board no longer has it (CDXC:Coordinators 2026-11-08
/// on `board_value` in server/src/coordinators/board.rs). The look is
/// `board_mention_link` in apps/desktop/src/app/native_chat/markdown_links.rs and the phone's
/// `SessionPill.tsx`.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of a card click that left `[Release 10.47.0](board:B8)` as raw text in the chat box while
/// a thread's card left a pill: "why does this not show as a pill?". A board item's link is a pill
/// wherever a session mention is one: in the chat box, in the sent message and in the transcript,
/// with the board's icon, and a click on a sent one opens the board scrolled to that item.
pub fn board_reference(href: &str, label: &str) -> Option<Value> {
    let item = board_item(href)?;
    Some(json!({
        "href": href,
        "sourceLabel": label,
        "label": crate::transcript::session_mentions::pill_label(label),
        "title": SHOW_ON_BOARD,
        "goneTitle": BOARD_ITEM_GONE,
        "kind": BOARD_REFERENCE_KIND,
        "item": format!("B{item}"),
    }))
}

/// A board pill was clicked: the board opens at item `item`. Its group unfolds and shows every
/// row, so the list has the card to scroll to; an item the chat's board does not have (gone, or a
/// chat with no board) is said in a toast instead.
pub fn show_item(state: &mut ChatState, item: u64) -> Vec<crate::effect::Effect> {
    let reference = format!("B{item}");
    let group = state
        .session
        .coordinator_threads
        .as_deref()
        .and_then(|threads| threads.get("board"))
        .and_then(|board| board.get("groups"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|group| {
            group["rows"]
                .as_array()?
                .iter()
                .find(|row| text(row, "kind") == "item" && text(row, "ref") == reference)
                .map(|row| (text(group, "id").to_string(), text(row, "key").to_string()))
        });
    let Some((group, key)) = group else {
        return vec![crate::effect::Effect::Toast {
            level: "info".to_string(),
            message: format!("{reference} is not on this chat's board."),
        }];
    };
    let panels = &mut state.extras.panels;
    if folded_by_default(&group) != panels.board_toggled.contains(&group) {
        // Folded: open it.
        toggle(&mut panels.board_toggled, &group);
    }
    panels.board_expanded.insert(group);
    // The card opens too, so the pill shows what it names.
    let card = format!("{CARD_KEY_PREFIX}{key}");
    if !panels.board_toggled.contains(&card) {
        toggle(&mut panels.board_toggled, &card);
    }
    let seq = panels.board_focus.as_ref().map_or(0, |(_, seq)| *seq) + 1;
    panels.board_focus = Some((key, seq));
    Vec::new()
}

/// `coordinatorBoardAnswer {key, item, label, text, ask?}`: one of a card's answers (a board card's,
/// or a work item ask card's, which names its `ask` tag; "Done" on a task). It is recorded with
/// gxserver first (op `answer`, which refuses a second answer from any client), and only then sent
/// to the coordinator as a message ("B4: Yes"), without touching the chat box. A card already
/// answered here takes no second click.
///
/// CDXC:Coordinators 2026-11-05 WHY:
/// The coordinator received "B7: Yes" three times for one decision: three clicks a second and six
/// seconds apart, each its own send, while the busy coordinator queued them and the card showed no
/// sign the first had gone. Now the first click locks the card (the pick marked, the rest faded),
/// gxserver keeps the answer so a reload or another client cannot give a second, and the message's
/// send id is the item and answer, so the send ledger drops any repeat.
pub fn answer(
    state: &mut ChatState,
    params: &serde_json::Map<String, Value>,
    request_id: u64,
) -> Option<crate::effect::Effect> {
    let field = |name: &str| params.get(name).and_then(Value::as_str).unwrap_or_default();
    let (key, item, label, text) = (field("key"), field("item"), field("label"), field("text"));
    if text.is_empty() || item.is_empty() || state.extras.panels.board_answers.contains_key(key) {
        return None;
    }
    let reply = field("op") == "reply";
    let panels = &mut state.extras.panels;
    panels
        .board_answers
        .insert(key.to_string(), label.to_string());
    // Recorded already and only its message failed: this try sends the message alone.
    if panels.board_answer_unsent.contains(key) {
        return Some(answer_message(state, request_id, key, label, text, reply));
    }
    panels.board_answer_requests.insert(
        request_id,
        (key.to_string(), label.to_string(), text.to_string(), reply),
    );
    let mut body = json!({
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        // `cancel` withdraws a work ask for good (work.rs); its message goes the same way.
        // `reply` records the user's own words (the dock's Reply).
        "op": match field("op") { "cancel" => "cancel", "reply" => "reply", _ => "answer" },
        "item": item,
        "answer": label,
    });
    // A work item card's ask names its question tag (`extras::work_cards`).
    let ask = field("ask");
    if !ask.is_empty() {
        body["ask"] = json!(ask);
    }
    state
        .extras
        .panels
        .board_answer_calls
        .insert(request_id, body.clone());
    Some(crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::CoordinatorBoard,
        params: Box::new(body),
    })
}

/// The message that tells the coordinator a card's answer, once gxserver has recorded it.
fn answer_message(
    state: &mut ChatState,
    request_id: u64,
    key: &str,
    label: &str,
    text: &str,
    reply: bool,
) -> crate::effect::Effect {
    let params = json!({
        "text": text,
        "sendRequestId": answer_send_id(&state.identity.session_id, key, label),
    });
    let panels = &mut state.extras.panels;
    panels
        .board_answer_sends
        .insert(request_id, (key.to_string(), label.to_string(), reply));
    panels.board_answer_calls.insert(request_id, params.clone());
    crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(params),
    }
}

/// Fires when the board answers gxserver never answered go again.
pub const BOARD_ANSWER_RETRY_TIMER: &str = "extras.boardAnswerRetry";

/// A board answer's call (its record or its message) got no answer from Ghostex's service, or
/// met it starting: the answer stays given, its card stays locked, and the same call goes again
/// every two seconds until the service answers. `None` when `request_id` is not such a call or
/// the outcome is an answer of gxserver's own, which [`settle_answer`] takes.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User: "you should self heal, don't show me this message, why do i need to press enter again
/// when you can press it for me". An answer given in the dock or the Work panel (a pick, Done, a
/// Reply) that met a restarting gxserver unlocked its card with an error toast, and a Reply
/// opened its box again for the user to send a second time. Time fixes that, so the app waits it
/// out, as a message from the chat box does (`composer/delivery.rs`). The record refuses a
/// second answer and the message carries the card's own send id, so a call that did land is not
/// applied twice.
pub fn wait_out_answer(
    state: &mut ChatState,
    request_id: u64,
    outcome: &crate::wire::RpcOutcome,
    context: &ChatContext,
) -> Option<Vec<crate::effect::Effect>> {
    let crate::wire::RpcOutcome::Err { code, .. } = outcome else {
        return None;
    };
    if !matches!(
        code.as_deref(),
        None | Some("unreachable") | Some("serverStarting")
    ) {
        return None;
    }
    let panels = &mut state.extras.panels;
    let waiting = if let Some((key, label, reply)) = panels.board_answer_sends.remove(&request_id) {
        (key, label, String::new(), reply, true)
    } else {
        let (key, label, text, reply) = panels.board_answer_requests.remove(&request_id)?;
        (key, label, text, reply, false)
    };
    let params = panels
        .board_answer_calls
        .remove(&request_id)
        .unwrap_or(Value::Null);
    let (key, label, text, reply, message) = waiting;
    panels
        .board_answer_waiting
        .push((key, label, text, reply, message, params));
    state.core.timers.arm_once(
        BOARD_ANSWER_RETRY_TIMER,
        context.now_ms,
        crate::session::constants::UNREACHABLE_RETRY_DELAY_MS as f64,
    );
    Some(Vec::new())
}

/// The retry clock fired: every waiting board answer call goes again as it was.
pub fn resend_waiting_answers(
    state: &mut ChatState,
    mut next_request_id: impl FnMut() -> u64,
) -> Vec<crate::effect::Effect> {
    let waiting = std::mem::take(&mut state.extras.panels.board_answer_waiting);
    let panels = &mut state.extras.panels;
    waiting
        .into_iter()
        .map(|(key, label, text, reply, message, params)| {
            let request_id = next_request_id();
            panels.board_answer_calls.insert(request_id, params.clone());
            let method = if message {
                panels
                    .board_answer_sends
                    .insert(request_id, (key, label, reply));
                crate::wire::ChatRpcMethod::SendSessionChatMessage
            } else {
                panels
                    .board_answer_requests
                    .insert(request_id, (key, label, text, reply));
                crate::wire::ChatRpcMethod::CoordinatorBoard
            };
            crate::effect::Effect::SendRpc {
                request_id,
                method,
                params: Box::new(params),
            }
        })
        .collect()
}

/// The message's send id: the card and its answer, so the send ledger drops a repeat of the same
/// answer. gxserver takes 1 to 128 characters, and a Reply's answer is the user's own words, so a
/// longer one is the card and words hashed instead.
///
/// CDXC:Coordinators 2026-11-10 WHY:
/// The user's typed Reply on a Work panel item failed with "sendRequestId must be a string of 1
/// to 128 characters." and his text was gone: the id held the whole reply.
fn answer_send_id(session: &str, key: &str, label: &str) -> String {
    let plain = format!("board-answer-{session}-{key}-{label}");
    if plain.chars().count() <= 128 {
        return plain;
    }
    // FNV-1a, so the same answer has the same id on every client and run.
    let hash = plain.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    let short: String = session.chars().take(40).collect();
    format!("board-answer-{short}-{hash:016x}")
}

/// A Reply that did not go: the card unlocks, and the Reply box opens again with the words and
/// why (desktop `workReplyFailed`), so nothing typed is lost.
fn reply_failed(key: &str, label: &str, message: &str) -> crate::effect::Effect {
    crate::effect::Effect::HostAction {
        action: "workReplyFailed".to_string(),
        params: Box::new(json!({ "key": key, "text": label, "message": message })),
    }
}

/// Takes gxserver's answer to [`answer`]'s record: once recorded, the message goes; refused because
/// the card was already answered, nothing goes and the card stays locked; refused otherwise, the
/// card unlocks and says why. A Reply refused either way opens again with its words. `None` when
/// `request_id` is not such a record.
pub fn settle_answer(
    state: &mut ChatState,
    request_id: u64,
    answer: Result<&Value, String>,
    next_request_id: u64,
) -> Option<Vec<crate::effect::Effect>> {
    state.extras.panels.board_answer_calls.remove(&request_id);
    if let Some((key, label, reply)) = state.extras.panels.board_answer_sends.remove(&request_id) {
        return Some(settle_answer_message(state, &key, &label, reply, answer));
    }
    let (key, label, text, reply) = state
        .extras
        .panels
        .board_answer_requests
        .remove(&request_id)?;
    Some(match answer {
        Ok(_) => vec![answer_message(
            state,
            next_request_id,
            &key,
            &label,
            &text,
            reply,
        )],
        Err(message) if message.contains("already answered") && !reply => Vec::new(),
        Err(message) => {
            let message = if message.is_empty() {
                "Couldn't send that answer.".to_string()
            } else {
                message
            };
            if !message.contains("already answered") {
                state.extras.panels.board_answers.remove(&key);
            }
            let mut effects = vec![crate::effect::Effect::Toast {
                level: "error".to_string(),
                message: message.clone(),
            }];
            if reply {
                effects.push(reply_failed(&key, &label, &message));
            }
            effects
        }
    })
}

/// Takes the answer to an answer's message: sent, nothing more; failed, the card unlocks and a try
/// again sends the message alone (it is recorded), and a Reply opens again with its words.
fn settle_answer_message(
    state: &mut ChatState,
    key: &str,
    label: &str,
    reply: bool,
    answer: Result<&Value, String>,
) -> Vec<crate::effect::Effect> {
    let panels = &mut state.extras.panels;
    match answer {
        Ok(_) => {
            panels.board_answer_unsent.remove(key);
            Vec::new()
        }
        Err(message) => {
            panels.board_answers.remove(key);
            panels.board_answer_unsent.insert(key.to_string());
            let message = if message.is_empty() {
                "Couldn't send that answer.".to_string()
            } else {
                message
            };
            let mut effects = vec![crate::effect::Effect::Toast {
                level: "error".to_string(),
                message: message.clone(),
            }];
            if reply {
                effects.push(reply_failed(key, label, &message));
            }
            effects
        }
    }
}

/// `toggleCoordinatorBoardGroup {group}`: folds or unfolds a group; `coordinatorBoardMore
/// {group}`: shows all of a group's rows, or back to the first few.
///
/// A `card:<key>` entry opens that card and closes any other: one card is open at a time.
///
/// CDXC:Coordinators 2026-11-06 DECISION:
/// User, on the card-actions page (Q1450 "B", the card opens in place): "clicking on another one
/// should open that one instead ok?" A plain click opens the card in place (its whole line, its
/// links, its tags and its Open / Mention buttons), a click on another card opens that one and
/// closes the first, and a click on the open card closes it. Supersedes the 2026-11-05 decision
/// that a click puts the card in the chat box (10.47.0): mentioning is now an ⌥-click, the open
/// card's Mention button, or Mention in chat in its menu (Q1451, all three).
pub fn toggle(set: &mut std::collections::BTreeSet<String>, group: &str) {
    if group.is_empty() {
        return;
    }
    if set.remove(group) {
        return;
    }
    if group.starts_with(CARD_KEY_PREFIX) {
        set.retain(|entry| !entry.starts_with(CARD_KEY_PREFIX));
    }
    set.insert(group.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(board: Value) -> ChatState {
        let mut state = ChatState::default();
        *state.session.coordinator_threads =
            Some(json!({"threads": [], "total": 0, "board": board}).into());
        state
    }

    fn context() -> ChatContext {
        ChatContext {
            now_ms: 1_800_000_000_000.0,
            ..ChatContext::default()
        }
    }

    fn row(id: &str, group: &str) -> Value {
        json!({"key": format!("thread::P:{id}"), "kind": "thread", "ref": id, "group": group, "title": id, "detail": "d", "projectId": "P", "sessionId": id})
    }

    /// The groups come in the board's order, Done and Backlog folded; a group past `GROUP_ROWS`
    /// says how many more; the pill counts Needs you, Blocked and Moving.
    #[test]
    fn groups_fold_cap_and_count() {
        let moving: Vec<Value> = (0..7)
            .map(|index| row(&format!("G{index}"), "moving"))
            .collect();
        let mut red = row("Gred", "blocked");
        red["links"] = json!([{"kind": "pr", "label": "#12", "state": "open", "checks": "failing", "url": "https://github.com/o/r/pull/12"}]);
        let state = state_with(json!({"groups": [
            {"id": "later", "total": 1, "rows": [{"key": "item:1", "kind": "item", "ref": "B1", "group": "later", "reason": "held", "release": "full release", "title": "Help text"}]},
            {"id": "blocked", "total": 1, "rows": [red]},
            {"id": "moving", "total": 7, "rows": moving},
            {"id": "done", "total": 1, "rows": [row("Gdone", "done")]},
        ]}));
        let board = coordinator_board(&state, &context()).unwrap();
        let ids: Vec<&str> = board.groups.iter().map(|group| group.id.as_str()).collect();
        assert_eq!(
            ids,
            ["later", "blocked", "moving", "done"],
            "in the board's order, empty groups left out"
        );
        let group = |id: &str| board.groups.iter().find(|group| group.id == id).unwrap();
        assert_eq!(group("later").rows[0].tags[0].label, "Held \u{b7} release");
        assert_eq!(
            group("later").rows[0].tags[0].title,
            "Held for full release"
        );
        assert_eq!(
            release_chip_label("next full release + TestFlight"),
            "Held \u{b7} next release"
        );
        assert_eq!(
            release_chip_label("the one after the big refactor lands"),
            "Held"
        );
        // The second line is the work's links, each opening its page.
        let pr = &group("blocked").rows[0].tags[0];
        assert_eq!(
            (pr.kind.as_str(), pr.label.as_str(), pr.tone.as_str()),
            ("pr", "#12", "bad")
        );
        assert_eq!(pr.command["href"], "https://github.com/o/r/pull/12");
        assert_eq!(group("moving").rows.len(), GROUP_ROWS);
        assert_eq!(group("moving").more_label, "2 more");
        assert!(!group("done").open && group("done").rows.is_empty());
        // The columns view has every row, folded or capped in the list.
        assert_eq!(group("done").column_rows.len(), 1);
        assert_eq!(group("moving").column_rows.len(), 7);
        assert_eq!(group("done").count, 1);
        assert_eq!(
            board.pill_counts,
            vec![("blocked".to_string(), 1), ("moving".to_string(), 7)]
        );
        assert_eq!(board.meta, "1 blocked \u{b7} 7 in progress");
        assert_eq!(
            group("moving").rows[0].open_thread["type"],
            "openCoordinatorThread"
        );
        // The card itself opens nothing; the agent button opens the thread.
        assert!(group("moving").rows[0].command.is_null());
        assert_eq!(group("moving").rows[0].agent_icon, "claude");
        // Every thread and item row can go to Removed; the change names the thread or the item.
        let remove = &group("moving").rows[0].remove;
        assert_eq!(remove["op"], "drop");
        assert_eq!(remove["threadSessionId"], "G0");
        assert_eq!(group("later").rows[0].remove["item"], "B1");
        assert_eq!(group("later").title, "To do");
    }

    /// Opening a folded group and showing a group's every row are the reader's choices.
    #[test]
    fn the_reader_unfolds_and_expands() {
        let moving: Vec<Value> = (0..7)
            .map(|index| row(&format!("G{index}"), "moving"))
            .collect();
        let mut state = state_with(json!({"groups": [
            {"id": "moving", "total": 9, "rows": moving},
            {"id": "done", "total": 1, "rows": [row("Gdone", "done")]},
        ]}));
        assert_eq!(GROUPS[0], ("ideas", "Backlog"), "Backlog comes first");
        toggle(&mut state.extras.panels.board_toggled, "done");
        toggle(&mut state.extras.panels.board_expanded, "moving");
        let board = coordinator_board(&state, &context()).unwrap();
        let group = |id: &str| board.groups.iter().find(|group| group.id == id).unwrap();
        assert!(group("done").open);
        assert_eq!(group("moving").rows.len(), 7);
        assert_eq!(group("moving").more_label, "Show fewer");
        assert_eq!(group("moving").not_shown, 2);
    }

    /// What a Needs you card offers: a waiting thread's "Answer in thread", an item's answers and
    /// note; and what a click on a card puts in the chat box.
    #[test]
    fn needs_you_cards_say_what_to_do() {
        let mut waiting = row("Gask", "needs");
        waiting["waiting"] = json!("question");
        waiting["globalRef"] = json!("S90:P:Gask");
        let item = json!({"key": "item:4", "kind": "item", "ref": "B4", "group": "needs",
            "title": "Delete branch [wip]", "note": "Its commits are on main",
            "answers": [{"label": "Yes", "recommended": true}, {"label": "No", "recommended": false}]});
        let mut state =
            state_with(json!({"groups": [{"id": "needs", "total": 2, "rows": [waiting, item]}]}));
        state
            .extras
            .panels
            .board_answers
            .insert("item:4".to_string(), "Yes".to_string());
        let board = coordinator_board(&state, &context()).unwrap();
        let rows = &board.groups[0].rows;
        assert_eq!(rows[0].tags[0].label, "Answer in thread");
        assert_eq!(rows[0].tags[0].command["type"], "openCoordinatorThread");
        assert_eq!(rows[0].reference, "[Gask](S90:P:Gask)");
        assert_eq!(rows[1].reference, "[Delete branch \\[wip\\]](board:B4)");
        assert_eq!(rows[1].note, "Its commits are on main");
        assert_eq!(
            rows[0].note, "",
            "a thread at work has no report line in its card"
        );
        // Answered here: Yes is marked, and no answer can be clicked again.
        assert!(rows[1].answers[0].recommended && rows[1].answers[0].picked);
        assert!(rows[1]
            .answers
            .iter()
            .all(|answer| answer.locked && answer.command.is_null()));

        let params =
            json!({"reference": "[Gask](S90:P:Gask)", "text": "look at ", "start": 8, "end": 8});
        let effects = insert_reference(params.as_object().unwrap());
        let [crate::effect::Effect::SetComposerText { content, .. }] = effects.as_slice() else {
            panic!("the reference goes in the chat box");
        };
        assert_eq!(content, "look at [Gask](S90:P:Gask) ");
    }

    /// The chat's own questions join Needs you only while the ? pill counts them (answered by an
    /// inline link or a form, or withdrawn, they leave both), and an item that names a question
    /// carries a chip for it, tinted by where it stands, that jumps to it.
    #[test]
    fn answered_questions_leave_needs_you_with_the_pill() {
        use crate::transcript::open_questions::{Asker, AskerKind};
        let asker = |id: &str, tag: &str, at: i64| Asker {
            id: id.to_string(),
            kind: AskerKind::Form,
            message_id: format!("m-{id}"),
            tags: vec![tag.to_string()],
            answered: false,
            obsolete: false,
            withdrawn: Vec::new(),
            keys: Vec::new(),
            skipped: Vec::new(),
            superseded: Vec::new(),
            inherited: false,
            seen_only: false,
            said: Vec::new(),
            title: format!("About {tag}?"),
            at: Some(at),
        };
        let item = json!({"key": "item:1", "kind": "item", "ref": "B1", "group": "needs",
            "title": "Reformat My Passport (Q1)", "note": "Answer Q2 in chat"});
        let mut state =
            state_with(json!({"groups": [{"id": "needs", "total": 1, "rows": [item]}]}));
        let mut link = asker("link", "Q1", 1);
        let form = asker("form", "Q2", 2);
        let mut withdrawn = asker("gone", "Q3", 3);
        link.answered = true;
        withdrawn.obsolete = true;
        *state.transcript_view.askers = vec![link, form, withdrawn];
        let pill = crate::questions::open_questions::project(&state, &context());
        let board = coordinator_board(&state, &context()).unwrap();
        let rows = &board.groups[0].rows;
        let questions: Vec<&str> = rows
            .iter()
            .filter(|row| row.key.starts_with("question:"))
            .map(|row| row.key.as_str())
            .collect();
        assert_eq!(questions, vec!["question:form"]);
        assert_eq!(pill.count as usize, questions.len());
        let chips: Vec<(&str, &str)> = rows[1]
            .tags
            .iter()
            .map(|chip| (chip.label.as_str(), chip.tone.as_str()))
            .collect();
        assert_eq!(chips, vec![("Q1", "good"), ("Q2", "question")]);
        assert_eq!(rows[1].tags[1].command["href"], "#gx-question=Q2");
    }

    /// One answer sends once: the first click records it with gxserver and, once recorded, sends
    /// the message with a send id fixed by the item and answer; any later click, the same answer or
    /// another, sends nothing.
    #[test]
    fn a_card_answer_sends_once() {
        let mut state = ChatState::default();
        state.identity.session_id = "Gcoord".to_string();
        let click = |label: &str| json!({"key": "item:7", "item": "B7", "label": label, "text": format!("B7: {label}")});
        let first = answer(&mut state, click("Yes").as_object().unwrap(), 1).expect("records");
        let crate::effect::Effect::SendRpc { params, .. } = &first else {
            panic!()
        };
        assert_eq!(
            (params["op"].as_str(), params["answer"].as_str()),
            (Some("answer"), Some("Yes"))
        );
        assert!(
            answer(&mut state, click("Yes").as_object().unwrap(), 2).is_none(),
            "a second click"
        );
        assert!(
            answer(&mut state, click("No").as_object().unwrap(), 3).is_none(),
            "another answer"
        );
        let sent = settle_answer(&mut state, 1, Ok(&json!({})), 4).unwrap();
        let [crate::effect::Effect::SendRpc { params, .. }] = sent.as_slice() else {
            panic!()
        };
        assert_eq!(params["text"], "B7: Yes");
        assert_eq!(params["sendRequestId"], "board-answer-Gcoord-item:7-Yes");
        // A record gxserver refuses because another client answered first sends nothing.
        let mut other = ChatState::default();
        answer(&mut other, click("No").as_object().unwrap(), 9).unwrap();
        let refused = settle_answer(
            &mut other,
            9,
            Err("B7 was already answered: Yes.".into()),
            10,
        );
        assert_eq!(refused, Some(Vec::new()));
    }

    /// An answer whose call gxserver never answered stays given and goes again by itself, record
    /// and message alike, as the same call (CDXC:SessionChat 2026-11-11 on `wait_out_answer`).
    #[test]
    fn a_card_answer_ghostex_did_not_answer_goes_again_by_itself() {
        let unanswered = crate::wire::RpcOutcome::Err {
            code: None,
            message: "Could not read gxserver response.".into(),
            endpoint: None,
        };
        let click = json!({"key": "item:7", "item": "B7", "label": "Yes", "text": "B7: Yes"});
        let mut state = ChatState::default();
        state.identity.session_id = "Gcoord".into();
        let Some(crate::effect::Effect::SendRpc { params: record, .. }) =
            answer(&mut state, click.as_object().unwrap(), 1)
        else {
            panic!("the record goes")
        };
        // gxserver is restarting: no toast, the card stays locked, the record waits.
        let waited = wait_out_answer(&mut state, 1, &unanswered, &context());
        assert_eq!(waited, Some(Vec::new()), "nothing is shown");
        assert_eq!(
            state
                .extras
                .panels
                .board_answers
                .get("item:7")
                .map(String::as_str),
            Some("Yes"),
            "the answer stays given"
        );
        assert!(state.core.timers.is_armed(BOARD_ANSWER_RETRY_TIMER));
        let mut next = 10;
        let again = resend_waiting_answers(&mut state, || {
            next += 1;
            next
        });
        let [crate::effect::Effect::SendRpc {
            request_id,
            method,
            params,
        }] = again.as_slice()
        else {
            panic!("the record goes again")
        };
        assert_eq!(*method, crate::wire::ChatRpcMethod::CoordinatorBoard);
        assert_eq!(params, &record, "as the call it was");
        // Recorded now: its message goes, and that too is waited out under the card's own id.
        let sent = settle_answer(&mut state, *request_id, Ok(&json!({})), 20).unwrap();
        let [crate::effect::Effect::SendRpc {
            params: message, ..
        }] = sent.as_slice()
        else {
            panic!("the message goes")
        };
        assert_eq!(
            wait_out_answer(&mut state, 20, &unanswered, &context()),
            Some(Vec::new())
        );
        let again = resend_waiting_answers(&mut state, || 30);
        let [crate::effect::Effect::SendRpc { method, params, .. }] = again.as_slice() else {
            panic!("the message goes again")
        };
        assert_eq!(*method, crate::wire::ChatRpcMethod::SendSessionChatMessage);
        assert_eq!(params, message);
        assert_eq!(params["sendRequestId"], "board-answer-Gcoord-item:7-Yes");
        // It lands: nothing waits, nothing is in flight.
        assert_eq!(
            settle_answer(&mut state, 30, Ok(&json!({})), 40),
            Some(Vec::new())
        );
        let panels = &state.extras.panels;
        assert!(panels.board_answer_waiting.is_empty() && panels.board_answer_calls.is_empty());
        // A refusal of gxserver's own is not waited out.
        answer(
            &mut state,
            json!({"key": "item:8", "item": "B8", "label": "No", "text": "B8: No"})
                .as_object()
                .unwrap(),
            50,
        );
        let refused = crate::wire::RpcOutcome::Err {
            code: Some("invalidState".into()),
            message: "B8 is closed.".into(),
            endpoint: None,
        };
        assert_eq!(wait_out_answer(&mut state, 50, &refused, &context()), None);
    }

    /// Blocked has its own pill, only while anything is blocked; the Board pill keeps Needs you and
    /// Moving; a click on Blocked opens the board at its first card.
    #[test]
    fn blocked_is_its_own_pill() {
        let mut state = state_with(json!({"groups": [
            {"id": "needs", "total": 1, "rows": [row("Gn", "needs")]},
            {"id": "blocked", "total": 1, "rows": [row("Gb", "blocked")]},
            {"id": "moving", "total": 1, "rows": [row("Gm", "moving")]},
        ]}));
        let board = project(&state, &context());
        let tones: Vec<String> = pill(&board)
            .unwrap()
            .dots
            .into_iter()
            .map(|dot| dot.tone)
            .collect();
        assert_eq!(tones, vec!["question", "working"]);
        let blocked = blocked_pill(&board).unwrap();
        // Drawn like the ? pill: no word, which stays in its tooltip and accessibility label.
        assert_eq!(
            (
                blocked.label.as_str(),
                blocked.meta.as_str(),
                blocked.aria_label.as_str()
            ),
            ("", "Blocked", "Blocked, 1")
        );
        assert_eq!(
            (blocked.id.as_str(), blocked.dots[0].count.as_str()),
            (BLOCKED_PILL, "1")
        );
        show_group(&mut state, "blocked");
        assert_eq!(
            state
                .extras
                .panels
                .board_focus
                .as_ref()
                .map(|(key, _)| key.as_str()),
            Some("thread::P:Gb")
        );
        let calm = state_with(
            json!({"groups": [{"id": "moving", "total": 1, "rows": [row("Gm", "moving")]}]}),
        );
        assert!(blocked_pill(&project(&calm, &context())).is_none());
    }

    /// One card is open at a time: a click opens it, another card's click moves the open card,
    /// and a click on the open card closes it. Group folds are untouched.
    #[test]
    fn one_card_opens_at_a_time() {
        let mut state = state_with(json!({"groups": [
            {"id": "moving", "total": 2, "rows": [row("Ga", "moving"), row("Gb", "moving")]},
        ]}));
        let board = coordinator_board(&state, &context()).unwrap();
        let first = board.groups[0].rows[0].toggle["group"]
            .as_str()
            .unwrap()
            .to_string();
        let second = board.groups[0].rows[1].toggle["group"]
            .as_str()
            .unwrap()
            .to_string();
        let open = |state: &ChatState| -> Vec<bool> {
            coordinator_board(state, &context()).unwrap().groups[0]
                .rows
                .iter()
                .map(|row| row.expanded)
                .collect()
        };
        toggle(&mut state.extras.panels.board_toggled, "done");
        toggle(&mut state.extras.panels.board_toggled, &first);
        assert_eq!(open(&state), vec![true, false]);
        toggle(&mut state.extras.panels.board_toggled, &second);
        assert_eq!(open(&state), vec![false, true]);
        toggle(&mut state.extras.panels.board_toggled, &second);
        assert_eq!(open(&state), vec![false, false]);
        assert!(
            state.extras.panels.board_toggled.contains("done"),
            "the fold stays"
        );
    }

    /// The menu: Open, Mention in chat, Tags, Mark done, Remove, with no Move to; the user's tag is
    /// the first chip; a question's card only goes to its question.
    #[test]
    fn a_cards_menu_and_tag() {
        let mut item = json!({"key": "item:3", "kind": "item", "ref": "B3", "group": "later", "title": "Help text", "detail": "", "links": [{"kind": "pr", "label": "#214", "url": "https://github.com/o/r/pull/214"}], "tag": "bug", "tagLook": {"value": "bug", "label": "Bug", "icon": "bug", "color": "#a54646"}});
        item["at"] = json!("2026-11-06T00:00:00Z");
        let state = state_with(json!({"groups": [
            {"id": "later", "total": 1, "rows": [item]},
            {"id": "moving", "total": 1, "rows": [row("Ga", "moving")]},
        ], "tagOptions": [{"value": "bug", "label": "Bug", "icon": "bug", "color": "#a54646"}]}));
        let board = coordinator_board(&state, &context()).unwrap();
        let card = &board.groups[0].rows[0];
        let labels: Vec<&str> = card.menu.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Open PR #214",
                "Mention in chat",
                "Tags",
                "Mark done",
                "Remove from board"
            ]
        );
        assert_eq!(card.tags[0].kind, "userTag");
        assert_eq!(card.tags[0].color, "#a54646");
        assert_eq!(card.tag, "bug");
        assert_eq!(board.tag_options.len(), 1);
        let thread = &board.groups[1].rows[0];
        assert_eq!(thread.menu[0].label, "Open thread");
        assert_eq!(thread.done["op"], "done");
    }

    /// A drag onto a group moves the card; onto Done it is Mark done, onto Removed it is Remove; a
    /// thread's Done resolves it; a tag goes to gxserver.
    #[test]
    fn drags_done_and_tags_ask_gxserver() {
        let mut state = ChatState::default();
        state.identity.session_id = "Gcoord".to_string();
        let send = |state: &mut ChatState, params: Value| {
            let Some(crate::effect::Effect::SendRpc { method, params, .. }) =
                change(state, params.as_object().unwrap(), 1)
            else {
                return None;
            };
            Some((method.as_str().to_string(), *params))
        };
        let (_, moved) = send(
            &mut state,
            json!({"op": "move", "item": "B3", "group": "blocked"}),
        )
        .unwrap();
        assert_eq!(
            (moved["op"].as_str(), moved["group"].as_str()),
            (Some("move"), Some("blocked"))
        );
        let (_, done) = send(
            &mut state,
            json!({"op": "move", "item": "B3", "group": "done"}),
        )
        .unwrap();
        assert_eq!(done["op"], "done");
        let (_, dropped) = send(
            &mut state,
            json!({"op": "move", "item": "B3", "group": "removed"}),
        )
        .unwrap();
        assert_eq!(dropped["op"], "drop");
        let (method, resolved) = send(&mut state, json!({"op": "done", "threadServerId": "", "threadProjectId": "P", "threadSessionId": "Ga"})).unwrap();
        assert_eq!(method, "coordinatorBoard");
        assert_eq!(
            (
                resolved["resolve"].as_bool(),
                resolved["threadSessionId"].as_str()
            ),
            (Some(true), Some("Ga"))
        );
        let (_, tagged) = send(&mut state, json!({"op": "tag", "item": "B3", "tag": ""})).unwrap();
        assert_eq!(
            (tagged["op"].as_str(), tagged["tag"].as_str()),
            (Some("tag"), Some(""))
        );
        assert!(send(&mut state, json!({"op": "move", "item": "B3"})).is_none());
    }

    /// A chat that is no coordinator, or a gxserver that sends no board, has no board and no pill.
    #[test]
    fn no_board_no_pill() {
        let mut state = ChatState::default();
        assert!(coordinator_board(&state, &context()).is_none());
        *state.session.coordinator_threads = Some(json!({"threads": [], "total": 0}).into());
        assert!(coordinator_board(&state, &context()).is_none());
        assert!(pill(&Value::Null).is_none());
    }
    /// A card's `[title](board:B8)` is a `board` pill in the chat box and in a message, and a click
    /// on one opens the item's folded group (Done) in full with the board's focus on it, once per
    /// click; an item the board does not have is toasted.
    #[test]
    fn a_board_link_is_a_pill_that_shows_its_item() {
        assert_eq!(board_item("board:B8"), Some(8));
        assert_eq!(board_item(" board:b12 "), Some(12));
        assert_eq!(board_item("board:B"), None);
        assert_eq!(board_item("board:B0"), None);
        assert_eq!(board_item("board:8"), None);
        assert_eq!(board_item("board:B8x"), None);

        let reference =
            crate::transcript::markdown_links::markdown_link("board:B8", "Release 10.47.0")
                .unwrap();
        assert_eq!(reference["kind"], BOARD_REFERENCE_KIND);
        assert_eq!(reference["label"], "Release 10.47.0");
        assert_eq!(reference["title"], SHOW_ON_BOARD);

        let draft = "see [Release 10.47.0](board:B8) now";
        let pills = crate::composer::reference_pills::composer_references(draft);
        assert_eq!(pills.len(), 1);
        assert_eq!(
            pills[0].kind,
            crate::composer::reference_pills::ReferenceKind::Board
        );
        assert_eq!(pills[0].path, "board:B8");
        assert_eq!(pills[0].label, "Release 10.47.0");

        let mut done = row("Gdone", "done");
        done["key"] = json!("item:8");
        done["kind"] = json!("item");
        done["ref"] = json!("B8");
        let mut state = state_with(json!({"groups": [
            {"id": "done", "total": 1, "rows": [done]},
        ]}));
        assert!(!coordinator_board(&state, &context()).unwrap().groups[0].open);
        assert!(show_item(&mut state, 8).is_empty());
        let board = coordinator_board(&state, &context()).unwrap();
        assert!(board.groups[0].open);
        assert_eq!(board.focus, json!({"key": "item:8", "seq": 1}));
        // A second click scrolls again, and leaves the group open.
        show_item(&mut state, 8);
        let board = coordinator_board(&state, &context()).unwrap();
        assert!(board.groups[0].open);
        assert_eq!(board.focus["seq"], 2);

        let missing = show_item(&mut state, 99);
        assert!(
            matches!(&missing[..], [crate::effect::Effect::Toast { message, .. }] if message.contains("B99"))
        );
        assert_eq!(
            coordinator_board(&state, &context()).unwrap().focus["seq"],
            2
        );
    }

    /// 11 Nov 2026 01:00:00 UTC.
    const NOW: i64 = 1_794_358_800_000;

    /// `at_ms` as gxserver writes a time, for a day of November 2026.
    fn iso(at_ms: i64) -> String {
        // 1 Nov 2026 00:00:00 UTC.
        let since = at_ms - 1_793_491_200_000;
        assert!(
            (0..30 * 86_400_000).contains(&since),
            "a time in November 2026"
        );
        let seconds = since / 1000;
        format!(
            "2026-11-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            seconds / 86_400 + 1,
            seconds % 86_400 / 3600,
            seconds % 3600 / 60,
            seconds % 60,
            since % 1000
        )
    }

    fn work_item(number: usize, aged_ms: i64, asks: Value) -> Value {
        let at = iso(NOW - aged_ms);
        json!({
            "key": format!("work:{number}"), "ref": format!("W{number}"), "kind": "item",
            "title": format!("Work item {number}"),
            "status": if asks.as_array().is_some_and(|asks| !asks.is_empty()) { "needs" } else { "moving" },
            "at": at, "now": "A thread is on it", "today": true, "asks": asks, "threads": [],
            "waits": [], "steps": [],
            "owner": { "kind": "coordinator", "label": "Coordinator" },
            "history": [{ "kind": "add", "at": at, "text": "added" }],
        })
    }

    fn work_state(items: Vec<Value>) -> ChatState {
        let mut state = ChatState::default();
        *state.session.coordinator_threads =
            Some(json!({"threads": [], "total": 0, "work": { "items": items }}).into());
        let message = |id: &str, role: &str, at: i64| -> crate::state::ChatRow {
            std::sync::Arc::new(
                serde_json::from_value(json!({
                    "id": id, "role": role, "timestamp": at, "source": "transcript",
                    "blocks": [{ "type": "text", "text": "words" }],
                }))
                .unwrap(),
            )
        };
        state.messages.composed = vec![
            message("u1", "user", NOW - 30 * 86_400_000),
            message("a1", "assistant", NOW - 30 * 86_400_000 + 1_000),
        ];
        state
    }

    fn at(now_ms: i64) -> ChatContext {
        ChatContext {
            now_ms: now_ms as f64,
            ..ChatContext::default()
        }
    }

    /// The kept Work panel is the same value until a label on it changes: hours and days old, its
    /// rows change by the hour, not with every minute of every row (CDXC:SessionChat 2026-11-11
    /// on `ages_change_at`).
    #[test]
    fn a_kept_work_panel_stands_until_a_label_on_it_changes() {
        let hour = 3_600_000;
        let state = work_state(vec![
            work_item(1, 3 * hour + 20 * 60_000, json!([])),
            work_item(2, 50 * hour, json!([])),
        ]);
        let turns = crate::extras::work_cards::turns_for_work(&state);
        let mut kept = None;
        let built = project_kept(&state, &at(NOW), &mut kept, &turns);
        assert!(built.is_object(), "a coordinator with work has a panel");
        // Thirty nine minutes on: the first row still reads 3h, the second 2d.
        for minutes in [1, 7, 39] {
            let later = project_kept(&state, &at(NOW + minutes * 60_000), &mut kept, &turns);
            assert!(
                std::sync::Arc::ptr_eq(&built.0, &later.0),
                "kept {minutes} minutes on"
            );
        }
        // At 4h the first row's age changes, and the panel is built again.
        let changed = project_kept(&state, &at(NOW + 40 * 60_000), &mut kept, &turns);
        assert!(!std::sync::Arc::ptr_eq(&built.0, &changed.0));
        assert_ne!(*built.0, *changed.0, "the age on it moved");

        // A row younger than an hour changes with its minute, and only then.
        let state = work_state(vec![work_item(1, 5 * 60_000 + 10_000, json!([]))]);
        let mut kept = None;
        let built = project_kept(&state, &at(NOW), &mut kept, &turns);
        let same = project_kept(&state, &at(NOW + 49_000), &mut kept, &turns);
        assert!(std::sync::Arc::ptr_eq(&built.0, &same.0));
        let next = project_kept(&state, &at(NOW + 50_000), &mut kept, &turns);
        assert!(!std::sync::Arc::ptr_eq(&built.0, &next.0));
    }

    /// A snooze ending brings its ask back under Needs you when it ends, however old its rows.
    #[test]
    fn a_kept_work_panel_ends_with_a_snooze() {
        let until = iso(NOW + 90_000);
        let asked = iso(NOW - 50 * 3_600_000);
        let state = work_state(vec![work_item(
            1,
            50 * 3_600_000,
            json!([{ "tag": "Q1", "open": true, "askedAt": asked, "title": "Which one?",
                     "text": "Which one?", "snoozedUntil": until }]),
        )]);
        let turns = crate::extras::work_cards::turns_for_work(&state);
        assert_eq!(ages_change_at(&state, NOW), Some(NOW + 90_000));
        let mut kept = None;
        let snoozed = project_kept(&state, &at(NOW), &mut kept, &turns);
        let still = project_kept(&state, &at(NOW + 89_999), &mut kept, &turns);
        assert!(std::sync::Arc::ptr_eq(&snoozed.0, &still.0));
        let back = project_kept(&state, &at(NOW + 90_000), &mut kept, &turns);
        assert_ne!(*snoozed.0, *back.0, "the ask is back under Needs you");
    }

    /// Every work row's actions are the ones it had when each row walked the chat itself.
    #[test]
    fn one_walk_of_the_chat_serves_every_work_row() {
        let asked = iso(NOW - 60_000);
        let state = work_state(vec![
            work_item(1, 60_000, json!([])),
            work_item(
                2,
                60_000,
                json!([{ "tag": "Q2", "open": true, "askedAt": asked, "title": "Which one?",
                         "text": "Which one?" }]),
            ),
            work_item(3, 120_000, json!([])),
        ]);
        let board = coordinator_board(&state, &at(NOW)).unwrap();
        let built = crate::extras::work_panel::work_board_at(
            &state.session.coordinator_threads.as_ref().unwrap()["work"],
            NOW,
        );
        let mut rows = 0;
        for group in built["groups"].as_array().unwrap() {
            for row in group["rows"].as_array().into_iter().flatten() {
                let own = crate::extras::work_cards::panel_actions(&state, &at(NOW), row);
                let drawn = board
                    .groups
                    .iter()
                    .flat_map(|group| group.rows.iter())
                    .find(|drawn| drawn.key == row["key"].as_str().unwrap_or_default())
                    .expect("the row is on the panel");
                assert_eq!(drawn.actions, own, "{}", row["key"]);
                assert_eq!(
                    drawn.actions["showInChat"]["messageId"], "a1",
                    "it points at the reply of its turn"
                );
                rows += 1;
            }
        }
        assert_eq!(rows, 3, "two items, and the third as its ask");
    }
}
