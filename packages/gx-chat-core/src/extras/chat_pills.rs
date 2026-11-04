//! The pills on the working card: one small pill per panel that used to stand above the composer.
//! Each opens its own menu with that panel's rows, drawn from the panel values beside them
//! (`coordinatorThreadsPanel`, `agentTasksPanel`, `agentFleetStrip`, `backgroundTasksPanel`). The
//! renderers put two icon pills of their own before these: "Scroll to bottom" (it depends on their
//! scroll) and "Previous question".
//!
//! CDXC:SessionChat 2026-10-18 DECISION:
//! User (2026-10-15): "i want threads to be a small pill above the chat and i want other things to also be pills above the chat that are aligned to the right on the same row as "working" and scrollable horizontally when there's no space", "i want to reduce the amout of vertical space taken up by these", and later "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons". So Threads, Tasks, Subagents and Background are pills in this order, right-aligned beside the label, with scroll-to-bottom (a down arrow) and Previous question (a ?) leftmost. User (2026-10-17, the working indicator review): the pills ride on the working card ("on the pill", Q259; also while idle, Q260), "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears", and "shouldn't allow wrapping in the pill itself, we should truncate the text on the left to make space for the pills (... at the end)": one line, the pills at full width and the label cut with an ellipsis (the Links pill the renderers add is gx-core's `SessionLinks::chat_pill`, the strip's last pill since 2026-10-29 (apps/desktop/src/app/native_chat/links_pill.rs); since 2026-10-29 the pills sit in the chat box's strip, below). User (2026-10-18): "why is the threads showing as part of the expanded working card? they shouldn't, they should show as menu for each one, let's make the pill itself not expandable at all, instead it should show a menu like we used to have for 'Threads' please": each pill opens its own floating menu again (a sheet on the phone), except the Tasks pill wherever a Session details panel can open (the 2026-10-25 SessionDetails DECISION below), and the card never opens (extras/working_card.rs). Supersedes the 2026-10-17 pills as a summary of a card that opened to every section (Q256), and still the 2026-10-15 own line under the word on a narrow chat and the phone.
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User (2026-10-23): "make running thread / running sub agents here have an indicator about this please (yellow color) because right now there's no color which is bad". From the mockups they picked a pulsing dot (Q449 "A"), pulsing rather than still (Q450 "pulse"), the same on the phone (Q451 "yes"), and for the Subagents number "running" (Q452: the running count while any run, the total once none do). Then (2026-10-24): "we have the yellow dot showing 2 ways here in the pills above the composer. let's unify to look just like the one that question shows". So a pill whose menu has a working subagent or thread is `running`: the renderers draw the ? pill's own corner dot (same spot inside the pill's top right edge, same size, same amber #f5a524, one shared helper on each platform), fading on the 1.6 s beat of the Subagents menu's running dot, and still with reduce motion on. Supersedes the 2026-10-23 dot before the icon in the sidebar's working yellow. The Threads number stays the in-flight count above.
//!
//! CDXC:SessionChat 2026-10-27 DECISION:
//! User: "Did you fix the issue with 2 yellow dots on threads pill?": a Threads pill whose threads ran while one waited on the user drew two amber dots, the running corner dot and an amber attention dot after the count. What waits now takes the sidebar's colours (`waiting`: the red approval dot for an approval, the pink question dot for a question), drawn in the pill's top right corner instead of after the count; with a thread running too, the amber running dot and the waiting dot overlap there with a 1px ring, as the sidebar's overlapping status dots do. Only running: the pulsing amber corner dot above. The Threads menu's header dot takes the same colour.
//!
//! CDXC:SessionChat 2026-10-29 DECISION:
//! User, of today's bar above the composer: "i don't like this way of showing the bar above the composer. let's move the pills to be inside the composer itself aligned to the top right and keep the working/worked text in the bar above the chat composer", then picked option E of the mockup (Q738 "strip"): the pills sit in a soft strip along the chat box's top edge, inside the box (a faintly tinted band with a hairline under it and the box's top corners), aligned to the right; when no pill shows there is no strip, and it comes and goes with the chat box's own short animation. The bar above keeps only the working or worked line, as one row. Supersedes the pills riding on the working card (2026-10-17 and 2026-10-18 above) and the two-row bar of 2026-10-28 (its row rule, divider and pill row are gone). The ↓ jump-to-bottom is the strip's leftmost pill while the strip shows for other pills; with none it floats alone over the transcript's bottom edge, so scrolling up never opens a strip and resizes the box. Armed Delayed Send / Close After Done timers ride in the strip too, before the ?.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: "we're showing the tasks twice. i think let's make clicking the tasks button not show the floating one, instead it should hide that pill and just show the session details sidebar". While this chat's Session details panel is on screen, docked or floating, its Tasks card lists the same tasks, so the Tasks pill is left out (`tasks_in_session_details`) and comes back when the panel closes. Where the panel can open, a click on the Tasks pill opens it instead of the pill's menu (apps/desktop/src/app/native_chat/session_details/mod.rs, `show_session_details_for_tasks`). Supersedes the 2026-10-18 menu above for the Tasks pill only: with no panel for the chat (the extension off, a chat that is not Claude's, the phone, which has no panel yet) Tasks still opens its menu.
//!
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_card.rs and chat_pills.rs (desktop and web), apps/mobile/app/src/chat/native/cards/WorkingCard.tsx and ChatPills.tsx (phone) draw this list and must match.

use serde_json::Value;

use serde_json::json;

use crate::document::{ChatPill, IncomingDraft, SavedDraftPill, ViewChip};
use crate::questions::open_questions::OpenQuestions;

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// The pills, in the order the row shows them, from the chat's resource list and the four
/// projected panel values (`null` when the panel is absent). `tasks_in_session_details`: the chat's
/// Session details panel is on screen and lists the tasks itself.
///
/// CDXC:SessionChat 2026-10-28 DECISION:
/// User: "show the new resources button between threads and ? pills in the bar and give resources a different icon". Resources is the first pill after the renderers' ? pill, ahead of Threads; it opens the Chat Resources list (questions/open_questions.rs) and shows while the list has any row, with the row count.
pub fn chat_pills(
    resources: &OpenQuestions,
    threads: &Value,
    tasks: &Value,
    fleet: &Value,
    background: &Value,
    tasks_in_session_details: bool,
) -> Vec<ChatPill> {
    let mut pills = Vec::new();
    if resources.total > 0 {
        pills.push(ChatPill {
            id: "resources".to_string(),
            label: "Resources".to_string(),
            count: resources.total.to_string(),
            waiting: String::new(),
            running: false,
            aria_label: format!("Resources, {}", resources.summary),
            meta: resources.summary.clone(),
        });
    }
    if threads.is_object() {
        // The count is what the menu lists first: the threads working or waiting on the user.
        let in_flight = count(threads, "inFlight");
        let meta = text(threads, "meta");
        pills.push(ChatPill {
            id: "threads".to_string(),
            label: "Workers".to_string(),
            count: if in_flight > 0 {
                in_flight.to_string()
            } else {
                String::new()
            },
            waiting: if threads.get("attention") == Some(&Value::Bool(true)) {
                "approval"
            } else if threads.get("question") == Some(&Value::Bool(true)) {
                "question"
            } else {
                ""
            }
            .to_string(),
            running: count(threads, "working") > 0,
            aria_label: format!("Workers, {meta}"),
            meta,
        });
    }
    if tasks.is_object() && !tasks_in_session_details {
        let meta = text(tasks, "meta");
        pills.push(ChatPill {
            id: "tasks".to_string(),
            label: "Tasks".to_string(),
            count: format!("{}/{}", count(tasks, "done"), count(tasks, "total")),
            waiting: String::new(),
            running: false,
            aria_label: format!("Tasks, {meta}"),
            meta,
        });
    }
    if fleet.is_object() {
        let rows = fleet
            .get("rows")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let working = rows
            .iter()
            .filter(|row| row.get("working") == Some(&Value::Bool(true)))
            .count();
        let meta = text(fleet, "countLabel");
        pills.push(ChatPill {
            id: "subagents".to_string(),
            label: "Subagents".to_string(),
            // The running ones while any run, every listed one once none do (Q452).
            count: if working > 0 { working } else { rows.len() }.to_string(),
            waiting: String::new(),
            running: working > 0,
            aria_label: format!("Subagents, {meta}"),
            meta,
        });
    }
    if background.is_object() {
        let meta = text(background, "meta");
        pills.push(ChatPill {
            id: "background".to_string(),
            label: "Background".to_string(),
            count: count(background, "count").to_string(),
            waiting: String::new(),
            running: false,
            aria_label: format!("Background tasks, {meta}"),
            meta,
        });
    }
    pills
}

/// The Stashed pill, the last of the core's pills (the Links pill follows it): how many prompts
/// the user stashed from this chat, shown while there is any. A tap opens Saved Prompts.
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User: "i have a stashed prompt but we aren't showing it at all. let's please show stashed pill here in the pills bar above the composer. should just say stashed and the number of stashed prompts for that session and look like the resources one basically". The count used to live only as the badge on the More actions menu's "Stash prompt" row (composerChrome.stashBadge, capped at 9, in a menu nobody opens) since the toolbar's stash button went on 2026-10-22, so a stash sat unseen. The pill reads "Stashed" and the real count, drawn like Resources, and opens the Saved Prompts list the Stash prompt row opens from an empty chat box. It is the last of the core's pills, so it appearing after a stash moves none of the others; the Links pill still follows it.
pub fn stashed_pill(stashed: usize) -> Option<ChatPill> {
    (stashed > 0).then(|| ChatPill {
        id: STASHED_PILL.to_string(),
        label: "Stashed".to_string(),
        count: stashed.to_string(),
        waiting: String::new(),
        running: false,
        aria_label: format!(
            "Stashed, {stashed} saved {}",
            if stashed == 1 { "prompt" } else { "prompts" }
        ),
        meta: String::new(),
    })
}

/// The Stashed pill's id, which the renderers open Saved Prompts for instead of a pill menu.
pub const STASHED_PILL: &str = "stashed";

/// The status chip at the left of the chat box's pill strip for a view level that is not Normal:
/// "Summary view" or "Detailed view", whose X (or a click on it) goes back to Normal.
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User, of the empty left side of the pill strip: "i want to show some status related stuff there, for example if I enabled summary mode, show "Summary view" there with an x that appears next to it when i hover so we can disable it easily. same for if verbose is enabled". Then Verbose was retired and the levels became one choice (transcript/view_level.rs), so the chips are "Summary view" and "Detailed view", at most one at a time, and each one's X sets Normal. A chip alone is enough to show the strip. The chips are quieter than the pills on the right, the same height, and stay pinned left while the pills scroll; the X shows on hover on the desktop and always on the phone (apps/mobile/app/src/chat/native/cards/PillsStrip.tsx).
pub fn view_chips(level: &str) -> Vec<ViewChip> {
    use crate::transcript::view_level::{DETAILED, SUMMARY};
    let (id, label) = match level {
        SUMMARY => (SUMMARY, "Summary view"),
        DETAILED => (DETAILED, "Detailed view"),
        _ => return Vec::new(),
    };
    vec![ViewChip {
        id: id.to_string(),
        label: label.to_string(),
        aria_label: format!("{label}. Turn off for Normal view"),
        command: json!({"type": "setDetailLevel", "level": "normal"}),
    }]
}

/// How much of the draft the Saved draft menu shows (the old notice's tooltip showed as much).
pub const SAVED_DRAFT_PREVIEW_CHARS: usize = 600;

/// The Saved draft pill for the draft another device saved (`incomingDraft`,
/// composer/draft_sync.rs), pinned at the strip's left; its menu shows the draft with Dismiss and
/// Use draft. `composer_text` is what Use draft would replace.
///
/// CDXC:Drafts 2026-11-04 DECISION:
/// User, of the "Another saved draft is available" row between the working bar and the chat box
/// (a bare line with big solid Use and Dismiss buttons): "gimme another design for this, doesn't
/// look good". From the mockups they picked a pill in the pill strip (Q1231 "A") opening a popover
/// with the draft's first lines and Dismiss / Use draft, adding: "i dont like falling back to the
/// strip tho, lets keep it same spot pls if user clicks then we show". So the row is gone on every
/// client. The pill always sits in the same spot, the strip's left before the view chips, pinned
/// while the other pills scroll, and it alone is enough to show the strip. Where the strip is narrow
/// it shortens to "Draft" and then to its icon instead of moving. The menu opens on a click (a sheet
/// on the phone). Supersedes the 2026-09-18 one-line notice whose icon tooltip held the preview.
pub fn saved_draft_pill(
    draft: Option<&IncomingDraft>,
    composer_text: &str,
) -> Option<SavedDraftPill> {
    let draft = draft?;
    let preview: String = draft
        .content
        .trim()
        .chars()
        .take(SAVED_DRAFT_PREVIEW_CHARS)
        .collect();
    if preview.is_empty() {
        return None;
    }
    let replaces = !composer_text.trim().is_empty();
    Some(SavedDraftPill {
        label: "Saved draft".to_string(),
        short_label: "Draft".to_string(),
        aria_label: "Saved draft from another device or the terminal. Show it".to_string(),
        detail: "from another device or the terminal".to_string(),
        preview,
        hint: if replaces {
            "Replaces what's in the box".to_string()
        } else {
            String::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{chat_pills, saved_draft_pill, stashed_pill, view_chips, OpenQuestions};
    use crate::document::IncomingDraft;
    use serde_json::{json, Value};

    /// Resources comes first, ahead of Threads (the renderers' ? pill sits before both), and only
    /// while the list has a row.
    #[test]
    fn resources_sit_between_the_question_pill_and_threads() {
        let resources = OpenQuestions {
            total: 4,
            summary: "2 unanswered".to_string(),
            ..OpenQuestions::default()
        };
        let threads = json!({"inFlight": 1, "working": 0});
        let tasks = json!({"done": 1, "total": 2});
        let ids: Vec<String> = chat_pills(
            &resources,
            &threads,
            &tasks,
            &Value::Null,
            &Value::Null,
            false,
        )
        .into_iter()
        .map(|pill| pill.id)
        .collect();
        assert_eq!(ids, ["resources", "threads", "tasks"]);
        let pill = &chat_pills(
            &resources,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            false,
        )[0];
        assert_eq!(
            (pill.label.as_str(), pill.count.as_str()),
            ("Resources", "4")
        );
        assert_eq!(pill.aria_label, "Resources, 2 unanswered");

        let empty = chat_pills(
            &OpenQuestions::default(),
            &threads,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            false,
        );
        assert_eq!(
            empty[0].id, "threads",
            "an empty list has no Resources pill"
        );
    }

    #[test]
    fn subagents_count_the_running_ones_while_any_run() {
        let fleet = json!({"rows": [{"working": true}, {"working": false}, {"working": false}]});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &Value::Null,
            &Value::Null,
            &fleet,
            &Value::Null,
            false,
        )[0];
        assert_eq!((pill.count.as_str(), pill.running), ("1", true));

        let finished = json!({"rows": [{"working": false}, {"working": false}]});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &Value::Null,
            &Value::Null,
            &finished,
            &Value::Null,
            false,
        )[0];
        assert_eq!((pill.count.as_str(), pill.running), ("2", false));
    }

    #[test]
    fn threads_run_only_while_one_is_working() {
        let waiting = json!({"inFlight": 1, "working": 0, "attention": true});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &waiting,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            false,
        )[0];
        assert_eq!((pill.count.as_str(), pill.running), ("1", false));
        assert_eq!(pill.waiting, "approval");

        let working = json!({"inFlight": 2, "working": 2});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &working,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            false,
        )[0];
        assert!(pill.running);
        assert_eq!(pill.waiting, "");
    }

    /// What waits on the user is one tone: an approval beats a question, and it stands beside a
    /// running thread instead of replacing it (the renderers overlap the two corner dots).
    #[test]
    fn threads_waiting_tone() {
        let both = json!({"inFlight": 3, "working": 1, "attention": true, "question": true});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &both,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            false,
        )[0];
        assert_eq!((pill.running, pill.waiting.as_str()), (true, "approval"));

        let asking = json!({"inFlight": 2, "working": 1, "question": true});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &asking,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            false,
        )[0];
        assert_eq!((pill.running, pill.waiting.as_str()), (true, "question"));
    }

    /// The Stashed pill shows while the chat has a stashed prompt, with the real count (the
    /// More actions badge stops at 9).
    #[test]
    fn stashed_pill_counts_the_chats_stashed_prompts() {
        assert!(stashed_pill(0).is_none(), "nothing stashed, no pill");
        let one = stashed_pill(1).expect("pill");
        assert_eq!(
            (one.id.as_str(), one.label.as_str(), one.count.as_str()),
            ("stashed", "Stashed", "1")
        );
        assert_eq!(one.aria_label, "Stashed, 1 saved prompt");
        let many = stashed_pill(12).expect("pill");
        assert_eq!(many.count, "12");
        assert_eq!(many.aria_label, "Stashed, 12 saved prompts");
        assert!(!many.running && many.waiting.is_empty());
    }

    #[test]
    fn view_chips_name_the_level_unless_it_is_normal() {
        assert!(view_chips("normal").is_empty(), "Normal has no chip");
        let summary = &view_chips("summary")[0];
        assert_eq!(
            (summary.id.as_str(), summary.label.as_str()),
            ("summary", "Summary view")
        );
        let detailed = &view_chips("detailed")[0];
        assert_eq!(
            (detailed.id.as_str(), detailed.label.as_str()),
            ("detailed", "Detailed view")
        );
        // Either X goes back to Normal.
        for chip in [summary, detailed] {
            assert_eq!(
                chip.command,
                serde_json::json!({"type": "setDetailLevel", "level": "normal"})
            );
        }
    }

    fn draft(content: &str) -> IncomingDraft {
        IncomingDraft {
            content: content.to_string(),
            ..IncomingDraft::default()
        }
    }

    #[test]
    fn the_saved_draft_pill_shows_only_while_a_draft_is_offered() {
        assert_eq!(saved_draft_pill(None, ""), None);
        assert_eq!(saved_draft_pill(Some(&draft("  \n ")), ""), None);
        let pill = saved_draft_pill(Some(&draft("  fix the toast\n")), "").unwrap();
        assert_eq!(pill.label, "Saved draft");
        assert_eq!(pill.short_label, "Draft");
        assert_eq!(pill.preview, "fix the toast");
    }

    #[test]
    fn the_saved_draft_menu_warns_only_when_use_would_replace_typed_text() {
        let offered = draft("make the crew count clickable");
        assert_eq!(saved_draft_pill(Some(&offered), " \n").unwrap().hint, "");
        assert_eq!(
            saved_draft_pill(Some(&offered), "half a thought").unwrap().hint,
            "Replaces what's in the box"
        );
    }

    #[test]
    fn the_saved_draft_preview_stops_at_600_characters() {
        let long = "é".repeat(700);
        let pill = saved_draft_pill(Some(&draft(&long)), "").unwrap();
        assert_eq!(pill.preview.chars().count(), super::SAVED_DRAFT_PREVIEW_CHARS);
    }

}
