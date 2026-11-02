//! The pills on the working card: one small pill per panel that used to stand above the composer.
//! Each opens its own menu with that panel's rows, drawn from the panel values beside them
//! (`coordinatorThreadsPanel`, `agentTasksPanel`, `agentFleetStrip`, `backgroundTasksPanel`). The
//! renderers put two icon pills of their own before these: "Scroll to bottom" (it depends on their
//! scroll) and "Previous question".
//!
//! CDXC:SessionChat 2026-10-18 DECISION:
//! User (2026-10-15): "i want threads to be a small pill above the chat and i want other things to also be pills above the chat that are aligned to the right on the same row as "working" and scrollable horizontally when there's no space", "i want to reduce the amout of vertical space taken up by these", and later "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons". So Threads, Tasks, Subagents and Background are pills in this order, right-aligned beside the label, with scroll-to-bottom (a down arrow) and Previous question (a ?) leftmost. User (2026-10-17, the working indicator review): the pills ride on the working card ("on the pill", Q259; also while idle, Q260), "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears", and "shouldn't allow wrapping in the pill itself, we should truncate the text on the left to make space for the pills (... at the end)": one line, the pills at full width and the label cut with an ellipsis (the Links pill the renderers add is gx-core's `SessionLinks::chat_pill`, the strip's last pill since 2026-10-29 (apps/desktop/src/app/native_chat/links_pill.rs); since 2026-10-29 the pills sit in the chat box's strip, below). User (2026-10-18): "why is the threads showing as part of the expanded working card? they shouldn't, they should show as menu for each one, let's make the pill itself not expandable at all, instead it should show a menu like we used to have for 'Threads' please": each pill opens its own floating menu again (a sheet on the phone), and the card never opens (extras/working_card.rs). Supersedes the 2026-10-17 pills as a summary of a card that opened to every section (Q256), and still the 2026-10-15 own line under the word on a narrow chat and the phone.
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
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_card.rs and chat_pills.rs (desktop and web), apps/mobile/app/src/chat/native/cards/WorkingCard.tsx and ChatPills.tsx (phone) draw this list and must match.

use serde_json::Value;

use serde_json::json;

use crate::document::{ChatPill, ViewChip};
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
/// projected panel values (`null` when the panel is absent).
///
/// CDXC:SessionChat 2026-10-28 DECISION:
/// User: "show the new resources button between threads and ? pills in the bar and give resources a different icon". Resources is the first pill after the renderers' ? pill, ahead of Threads; it opens the Chat Resources list (questions/open_questions.rs) and shows while the list has any row, with the row count.
pub fn chat_pills(
    resources: &OpenQuestions,
    threads: &Value,
    tasks: &Value,
    fleet: &Value,
    background: &Value,
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
    if tasks.is_object() {
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

#[cfg(test)]
mod tests {
    use super::{chat_pills, view_chips, OpenQuestions};
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
        let ids: Vec<String> = chat_pills(&resources, &threads, &tasks, &Value::Null, &Value::Null)
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
        )[0];
        assert_eq!((pill.count.as_str(), pill.running), ("1", true));

        let finished = json!({"rows": [{"working": false}, {"working": false}]});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &Value::Null,
            &Value::Null,
            &finished,
            &Value::Null,
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
        )[0];
        assert_eq!((pill.running, pill.waiting.as_str()), (true, "approval"));

        let asking = json!({"inFlight": 2, "working": 1, "question": true});
        let pill = &chat_pills(
            &OpenQuestions::default(),
            &asking,
            &Value::Null,
            &Value::Null,
            &Value::Null,
        )[0];
        assert_eq!((pill.running, pill.waiting.as_str()), (true, "question"));
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
}
