//! The two panels that stand above the composer, outside it, ported from
//! `packages/shared/session-chat-controller/native-panels.ts`.
//!
//! Both folds are the user's, so the state lives here and the renderer only draws what this
//! projects. The fleet card's default is "expanded unless Simple mode", which only the renderer
//! knows, so the override is published as is and the host resolves it.

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::event::StorageKey;
use crate::extras::agent_fleet::agent_fleet_rows;
use crate::extras::agent_tasks::agent_task_panel;
use crate::state::{ChatContext, ChatState, PanelsState};

/// How often the subagent clocks re-publish between server samples.
pub const FLEET_CLOCK_TICK_MS: f64 = 1_000.0;

/// The store and key the fold is remembered under.
///
/// "I want the plan out of the way" is a preference, not a per-session view state, so the fold
/// survives a restart. Same store and same key React's panel used, so a fold saved before the
/// port is still remembered.
pub fn tasks_collapsed_key() -> StorageKey {
    StorageKey {
        store: "tasksCollapsed".to_string(),
        suffix: String::new(),
    }
}

/// `readTasksCollapsed`: the stored fold, which is `'1'` or nothing at all.
pub fn read_tasks_collapsed(value: Option<&str>) -> bool {
    value == Some("1")
}

/// `writeTasksCollapsed`: `'1'` when collapsed, a delete when not.
pub fn write_tasks_collapsed(collapsed: bool) -> Effect {
    Effect::WriteStorage {
        key: tasks_collapsed_key(),
        value: collapsed.then(|| "1".to_string()),
        durable: false,
    }
}

/// The fold reset a fresh plan gets: a new plan's done pile starts closed.
pub fn settle_task_signature(panels: &mut PanelsState, tasks: Option<&Value>) {
    let signature = tasks
        .and_then(|tasks| tasks.get("tasks"))
        .and_then(Value::as_array)
        .map(|list| list.len() as i64)
        .unwrap_or(0);
    if signature != panels.task_signature {
        panels.task_signature = signature;
        panels.tasks_show_completed = false;
    }
}

/// `project`: the two document values, `agentFleetStrip` and `agentTasksPanel`.
///
/// Returns `(strip, panel, ticking)`. `ticking` says a clock is still moving, so the host has to
/// re-publish once a second.
pub fn project(state: &ChatState, context: &ChatContext) -> (Value, Value, bool) {
    let panels = &state.extras.panels;
    // `sessionChatAgentFleetRows(fleet, provider, Date.now())` inside `publish`: the read comes
    // after everything else the call did before it published, so the call's LAST read is the
    // one the published elapsed labels were measured at, not its first.
    let mut clock = context.clone();
    clock.now_ms = context.clock_read(usize::MAX);
    let strip = agent_fleet_rows(
        state.session.agent_fleet.as_ref(),
        state.session.agent.as_deref(),
        &clock,
    );
    let ticking = strip.as_ref().is_some_and(|strip| strip.ticking);
    let panel = agent_task_panel(
        state.session.agent_tasks.as_ref(),
        panels.tasks_collapsed,
        panels.tasks_show_completed,
    );
    let strip_value = match strip {
        Some(strip) => merge(
            serde_json::to_value(strip).unwrap_or(Value::Null),
            json!({
                "openOverride": match panels.fleet_open {
                    Some(open) => Value::Bool(open),
                    None => Value::Null,
                }
            }),
        ),
        None => Value::Null,
    };
    let panel_value = match panel {
        Some(panel) => merge(
            serde_json::to_value(panel).unwrap_or(Value::Null),
            json!({
                "collapsed": panels.tasks_collapsed,
                "showCompleted": panels.tasks_show_completed,
            }),
        ),
        None => Value::Null,
    };
    (strip_value, panel_value, ticking)
}

/// `{ ...base, ...extra }`.
fn merge(base: Value, extra: Value) -> Value {
    let (Value::Object(mut base), Value::Object(extra)) = (base, extra) else {
        return Value::Null;
    };
    for (key, value) in extra {
        base.insert(key, value);
    }
    Value::Object(base)
}
