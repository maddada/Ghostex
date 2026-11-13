//! The Tree's rules as JSON in and JSON out, for the phone: its Beads screen hands gxserver's
//! snapshot rows and its options to [`tree`] through the WebAssembly core page (`beadsTree`), and
//! draws what comes back, so the phone and the desktop arrange beads with the same code.
//!
//! The phone shows no agents on rows (it has no session store for the project), so rows carry
//! none; everything else is [`build_tree`] as the desktop runs it.
//!
//! CDXC:ProjectBoard 2026-11-13 WHY: the phone's Beads view first ported these rules to
//! TypeScript; review held that a second owner of gx-core's rules breaks the Rust-core rule, so the
//! phone reaches them here instead (an over-the-air asset, like `gx_cloud_call`).
//! SEE-ALSO: packages/gx-chat-wasm/src/lib.rs (`gx_beads_tree`), apps/mobile/app/src/beads/tree.ts

use std::collections::HashSet;

use ghostex_gx_protocol::beads_view::{BeadsReadiness, BeadsViewRow};
use serde::Deserialize;
use serde_json::{json, Value};

use super::agents::BeadAgents;
use super::time::BeadsClock;
use super::tree::{
    bead_details, build_tree, status_label, BeadGlyph, BeadMeta, BeadRowModel, BeadsFilter,
    BeadsSectionKind, BeadsSort, BeadsTreeOptions, EpicSummary,
};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Options {
    /// Omitted means All; a key the core doesn't know is an argument error.
    filter: Option<String>,
    /// Omitted means Priority; anything but `priority` or `lastChanged` is an argument error.
    sort: Option<String>,
    show_done: bool,
    query: String,
    open_done: Vec<String>,
    collapsed: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Arguments {
    #[serde(default)]
    beads: Vec<BeadsViewRow>,
    /// Required: without bd's readiness answer the tree can't say what is ready.
    readiness: BeadsReadiness,
    #[serde(default)]
    options: Options,
    #[serde(default)]
    now_ms: i64,
    #[serde(default)]
    offset_ms: i64,
}

fn filter_key(filter: BeadsFilter) -> &'static str {
    match filter {
        BeadsFilter::All => "all",
        BeadsFilter::NeedsDecision => "needsDecision",
        BeadsFilter::Ready => "ready",
        BeadsFilter::InProgress => "inProgress",
        BeadsFilter::Blocked => "blocked",
        BeadsFilter::Deferred => "deferred",
    }
}

fn filter_from(key: Option<&str>) -> Result<BeadsFilter, String> {
    let Some(key) = key else {
        return Ok(BeadsFilter::All);
    };
    BeadsFilter::ALL
        .into_iter()
        .find(|filter| filter_key(*filter) == key)
        .ok_or_else(|| format!("Unknown beadsTree filter: {key}"))
}

fn sort_from(key: Option<&str>) -> Result<BeadsSort, String> {
    match key {
        None | Some("priority") => Ok(BeadsSort::Priority),
        Some("lastChanged") => Ok(BeadsSort::LastChanged),
        Some(other) => Err(format!("Unknown beadsTree sort: {other}")),
    }
}

fn argument_error(message: String) -> String {
    json!({ "error": { "message": message } }).to_string()
}

fn glyph_key(glyph: BeadGlyph) -> &'static str {
    match glyph {
        BeadGlyph::Open => "open",
        BeadGlyph::InProgress => "inProgress",
        BeadGlyph::Blocked => "blocked",
        BeadGlyph::Snoozed => "snoozed",
        BeadGlyph::OnIce => "onIce",
        BeadGlyph::Review => "review",
        BeadGlyph::Decision => "decision",
        BeadGlyph::Closed => "closed",
        BeadGlyph::Epic => "epic",
    }
}

/// The row's words, as the desktop tree says them (`native_beads/tree.rs`).
fn meta_text(meta: &BeadMeta) -> String {
    match meta {
        BeadMeta::Type(label) => label.clone(),
        BeadMeta::InEpic(title) => format!("In {title}"),
        BeadMeta::WaitingOn { title, more } if *more > 0 => format!("Waiting on {title} +{more}"),
        BeadMeta::WaitingOn { title, .. } => format!("Waiting on {title}"),
        BeadMeta::Until(date) => format!("Until {date}"),
        BeadMeta::OnIce => "On ice".to_string(),
        BeadMeta::Due(date) => format!("Due {date}"),
        BeadMeta::Status(status) => status.clone(),
    }
}

fn summary_json(summary: &EpicSummary) -> Value {
    json!({ "total": summary.total, "done": summary.done, "text": summary.text })
}

fn row_json(row: &BeadRowModel) -> Value {
    json!({
        "id": row.id,
        "title": row.title,
        "glyph": glyph_key(row.glyph),
        "meta": row.meta.iter().map(meta_text).collect::<Vec<_>>(),
        "labels": row.labels,
        "commentCount": row.comment_count,
        "priority": row.priority,
        "changed": row.changed,
        "needsDecision": row.needs_decision,
        "closed": row.closed,
        "claimable": row.claimable,
    })
}

/// `{beads, readiness, options: {filter, sort, showDone, query, openDone, collapsed}, nowMs,
/// offsetMs}` → `{ok: {sections, counts, readinessOk, filters, filter}}`, or `{error: {message}}`.
/// `filters` are the filters to offer (Ready and Blocked only while bd's readiness can be
/// trusted, as the desktop's strip shows them) and `filter` the one the tree applied.
pub fn tree(arguments: &str) -> String {
    let arguments: Arguments = match serde_json::from_str(arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            return json!({ "error": { "message": format!("Invalid beadsTree arguments: {error}") } })
                .to_string()
        }
    };
    let requested = match filter_from(arguments.options.filter.as_deref()) {
        Ok(filter) => filter,
        Err(message) => return argument_error(message),
    };
    let sort = match sort_from(arguments.options.sort.as_deref()) {
        Ok(sort) => sort,
        Err(message) => return argument_error(message),
    };
    let options = BeadsTreeOptions {
        filter: requested,
        sort,
        show_done: arguments.options.show_done,
        query: arguments.options.query,
        open_done: arguments
            .options
            .open_done
            .into_iter()
            .collect::<HashSet<_>>(),
        collapsed: arguments
            .options
            .collapsed
            .into_iter()
            .collect::<HashSet<_>>(),
    };
    let clock = BeadsClock {
        now_ms: arguments.now_ms,
        offset_ms: arguments.offset_ms,
    };
    let model = build_tree(
        &arguments.beads,
        &arguments.readiness,
        &BeadAgents::new(),
        &options,
        &clock,
    );
    let applied = if !model.readiness_ok && requested.needs_readiness() {
        BeadsFilter::All
    } else {
        requested
    };
    let filters: Vec<Value> = BeadsFilter::ALL
        .into_iter()
        .filter(|filter| model.readiness_ok || !filter.needs_readiness())
        .map(|filter| {
            json!({
                "key": filter_key(filter),
                "label": filter.label(),
                "count": model.counts.get(filter),
            })
        })
        .collect();
    let sections: Vec<Value> = model
        .sections
        .iter()
        .map(|section| {
            let (kind, epic) = match &section.kind {
                BeadsSectionKind::NeedsDecision => ("needsDecision", Value::Null),
                BeadsSectionKind::Epic(epic) => ("epic", row_json(epic)),
                BeadsSectionKind::NotInEpic => ("notInEpic", Value::Null),
            };
            json!({
                "key": section.key,
                "kind": kind,
                "title": section.title,
                "epic": epic,
                "count": section.count,
                "summary": section.summary.as_ref().map(summary_json),
                "rows": section.rows.iter().map(row_json).collect::<Vec<_>>(),
                "doneRows": section.done_rows.iter().map(row_json).collect::<Vec<_>>(),
                "doneCount": section.done_count,
                "doneOpen": section.done_open,
                "collapsed": section.collapsed,
            })
        })
        .collect();
    json!({
        "ok": {
            "sections": sections,
            "readinessOk": model.readiness_ok,
            "filters": filters,
            "filter": filter_key(applied),
        }
    })
    .to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeadArguments {
    #[serde(default)]
    beads: Vec<BeadsViewRow>,
    /// Required: without bd's readiness answer the tree can't say what is ready.
    readiness: BeadsReadiness,
    id: String,
    #[serde(default)]
    now_ms: i64,
    #[serde(default)]
    offset_ms: i64,
}

/// `{beads, readiness, id, nowMs, offsetMs}` → `{ok: {row, status, epic, summary}}` (`row` null
/// when the bead is not in the list), or `{error: {message}}`: one bead as its page shows it.
pub fn bead(arguments: &str) -> String {
    let arguments: BeadArguments = match serde_json::from_str(arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            return json!({ "error": { "message": format!("Invalid beadsBead arguments: {error}") } })
                .to_string()
        }
    };
    let clock = BeadsClock {
        now_ms: arguments.now_ms,
        offset_ms: arguments.offset_ms,
    };
    let Some(details) = bead_details(
        &arguments.beads,
        &arguments.readiness,
        &arguments.id,
        &clock,
    ) else {
        return json!({ "ok": { "row": Value::Null } }).to_string();
    };
    let status = arguments
        .beads
        .iter()
        .find(|row| row.id == arguments.id)
        .map(|row| status_label(&row.status))
        .unwrap_or_default();
    json!({
        "ok": {
            "row": row_json(&details.row),
            "status": status,
            "epic": details.epic.as_ref().map(|(id, title, summary)| json!({
                "id": id,
                "title": title,
                "summary": summary_json(summary),
            })),
            "summary": details.summary.as_ref().map(summary_json),
        }
    })
    .to_string()
}
