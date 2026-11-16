//! A notice's checkboxes, as one list the renderers draw: `checkboxes`.
//!
//! CDXC:Coordinators 2026-11-16 DECISION in server/src/coordinators/stopped_by_restart.rs (Q68 "A
//! tick per thread"): the stopped-threads card has one Compact first checkbox per thread. gxserver
//! sends those rows as `checkboxes`, with `checkboxesDetail` (the card's words without the names
//! the rows already show), beside the single `checkbox` and full `detail` that clients older than
//! this core still draw. A card with one checkbox (the limit card's Compact first) arrives as
//! `checkbox` only, so it becomes a list of one here: the renderers draw `checkboxes` and nothing
//! else.

use serde_json::{Map, Value};

/// Folds `checkbox`, `checkboxes` and `checkboxesDetail` into `checkboxes` (and `detail`).
pub fn decorate(object: &mut Map<String, Value>) {
    let detail = object.remove("checkboxesDetail");
    let rows = object
        .get("checkboxes")
        .and_then(Value::as_array)
        .is_some_and(|rows| rows.iter().any(Value::is_object));
    if rows {
        object.remove("checkbox");
        if let Some(detail) = detail.filter(Value::is_string) {
            object.insert("detail".to_string(), detail);
        }
        return;
    }
    object.remove("checkboxes");
    if let Some(single) = object.remove("checkbox").filter(Value::is_object) {
        object.insert("checkboxes".to_string(), Value::Array(vec![single]));
    }
}
