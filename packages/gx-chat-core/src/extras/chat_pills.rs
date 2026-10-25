//! The pills on the working card: one small pill per panel that used to stand above the composer.
//! Each opens its own menu with that panel's rows, drawn from the panel values beside them
//! (`coordinatorThreadsPanel`, `agentTasksPanel`, `agentFleetStrip`, `backgroundTasksPanel`). The
//! renderers put two icon pills of their own before these: "Scroll to bottom" (it depends on their
//! scroll) and "Previous question".
//!
//! CDXC:SessionChat 2026-10-18 DECISION:
//! User (2026-10-15): "i want threads to be a small pill above the chat and i want other things to also be pills above the chat that are aligned to the right on the same row as "working" and scrollable horizontally when there's no space", "i want to reduce the amout of vertical space taken up by these", and later "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons". So Threads, Tasks, Subagents and Background are pills in this order, right-aligned beside the label, with scroll-to-bottom (a down arrow) and Previous question (a ?) leftmost. User (2026-10-17, the working indicator review): the pills ride on the working card ("on the pill", Q259; also while idle, Q260), "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears", and "shouldn't allow wrapping in the pill itself, we should truncate the text on the left to make space for the pills (... at the end)": one line, the pills at full width and the label cut with an ellipsis. User (2026-10-18): "why is the threads showing as part of the expanded working card? they shouldn't, they should show as menu for each one, let's make the pill itself not expandable at all, instead it should show a menu like we used to have for 'Threads' please": each pill opens its own floating menu again (a sheet on the phone), and the card never opens (extras/working_card.rs). Supersedes the 2026-10-17 pills as a summary of a card that opened to every section (Q256), and still the 2026-10-15 own line under the word on a narrow chat and the phone.
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User (2026-10-23): "make running thread / running sub agents here have an indicator about this please (yellow color) because right now there's no color which is bad". From the mockups they picked a pulsing dot (Q449 "A"), pulsing rather than still (Q450 "pulse"), the same on the phone (Q451 "yes"), and for the Subagents number "running" (Q452: the running count while any run, the total once none do). Then (2026-10-24): "we have the yellow dot showing 2 ways here in the pills above the composer. let's unify to look just like the one that question shows". So a pill whose menu has a working subagent or thread is `running`: the renderers draw the ? pill's own corner dot (same spot inside the pill's top right edge, same size, same amber #f5a524, one shared helper on each platform), fading on the 1.6 s beat of the Subagents menu's running dot, and still with reduce motion on. Supersedes the 2026-10-23 dot before the icon in the sidebar's working yellow. The Threads number stays the in-flight count above.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: "we're showing the tasks twice. i think let's make clicking the tasks button not show the floating one, instead it should hide that pill and just show the session details sidebar". While this chat's Session details panel is on screen, docked or floating, its Tasks card lists the same tasks, so the Tasks pill is left out (`tasks_in_session_details`) and comes back when the panel closes. Where the panel can open, a click on the Tasks pill opens it instead of the pill's menu (apps/desktop/src/app/native_chat/session_details/mod.rs, `show_session_details_for_tasks`). Supersedes the 2026-10-18 menu above for the Tasks pill only: with no panel for the chat (the extension off, a chat that is not Claude's, the phone, which has no panel yet) Tasks still opens its menu.
//!
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_card.rs and chat_pills.rs (desktop and web), apps/mobile/app/src/chat/native/cards/WorkingCard.tsx and ChatPills.tsx (phone) draw this list and must match.

use serde_json::Value;

use crate::document::ChatPill;

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

/// The pills, in the order the row shows them, from the four projected panel values (`null` when
/// the panel is absent). `tasks_in_session_details`: the chat's Session details panel is on screen
/// and lists the tasks itself.
pub fn chat_pills(
    threads: &Value,
    tasks: &Value,
    fleet: &Value,
    background: &Value,
    tasks_in_session_details: bool,
) -> Vec<ChatPill> {
    let mut pills = Vec::new();
    if threads.is_object() {
        // The count is what the menu lists first: the threads working or waiting on the user.
        let in_flight = count(threads, "inFlight");
        let meta = text(threads, "meta");
        pills.push(ChatPill {
            id: "threads".to_string(),
            label: "Threads".to_string(),
            count: if in_flight > 0 {
                in_flight.to_string()
            } else {
                String::new()
            },
            attention: threads.get("attention") == Some(&Value::Bool(true)),
            running: count(threads, "working") > 0,
            aria_label: format!("Threads, {meta}"),
            meta,
        });
    }
    if tasks.is_object() && !tasks_in_session_details {
        let meta = text(tasks, "meta");
        pills.push(ChatPill {
            id: "tasks".to_string(),
            label: "Tasks".to_string(),
            count: format!("{}/{}", count(tasks, "done"), count(tasks, "total")),
            attention: false,
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
            attention: false,
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
            attention: false,
            running: false,
            aria_label: format!("Background tasks, {meta}"),
            meta,
        });
    }
    pills
}

#[cfg(test)]
mod tests {
    use super::chat_pills;
    use serde_json::{json, Value};

    #[test]
    fn subagents_count_the_running_ones_while_any_run() {
        let fleet = json!({"rows": [{"working": true}, {"working": false}, {"working": false}]});
        let pill = &chat_pills(&Value::Null, &Value::Null, &fleet, &Value::Null, false)[0];
        assert_eq!((pill.count.as_str(), pill.running), ("1", true));

        let finished = json!({"rows": [{"working": false}, {"working": false}]});
        let pill = &chat_pills(&Value::Null, &Value::Null, &finished, &Value::Null, false)[0];
        assert_eq!((pill.count.as_str(), pill.running), ("2", false));
    }

    #[test]
    fn threads_run_only_while_one_is_working() {
        let waiting = json!({"inFlight": 1, "working": 0, "attention": true});
        let pill = &chat_pills(&waiting, &Value::Null, &Value::Null, &Value::Null, false)[0];
        assert_eq!((pill.count.as_str(), pill.running), ("1", false));

        let working = json!({"inFlight": 2, "working": 2});
        let pill = &chat_pills(&working, &Value::Null, &Value::Null, &Value::Null, false)[0];
        assert!(pill.running);
    }
}
