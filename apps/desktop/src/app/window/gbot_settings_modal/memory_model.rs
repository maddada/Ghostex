//! The Memory tab's data: a GBot's memory as `/api/gbots {op:"memoryShow"}` answers it.
//! SEE-ALSO: server/src/gbots/memory/ops.rs (`show`), the phone's contract/gbotMemory.ts.
use serde_json::Value;

/// How full `MEMORY.md` is against what loads at the start of a chat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GbotMemoryLevel {
    #[default]
    Ok,
    /// 80% of the budget or more.
    Amber,
    /// Over the budget: the rest does not load.
    Red,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GbotMemoryTopic {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) bytes: u64,
    pub(crate) path: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GbotMemoryChange {
    pub(crate) id: String,
    /// When, as the computer's local time ("9 Nov, 14:05").
    pub(crate) when: String,
    pub(crate) by: String,
    pub(crate) what: String,
    pub(crate) undone: bool,
    /// The newest change not undone yet, whose files still read as it left them.
    pub(crate) undoable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GbotMemoryDay {
    pub(crate) date: String,
    pub(crate) lines: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GbotMemoryView {
    pub(crate) text: String,
    /// What the text was read at; a save sends it so gxserver refuses to overwrite a newer write.
    pub(crate) hash: String,
    pub(crate) path: String,
    pub(crate) lines: u64,
    pub(crate) bytes: u64,
    pub(crate) max_lines: u64,
    pub(crate) max_bytes: u64,
    pub(crate) share: f64,
    pub(crate) level: GbotMemoryLevel,
    pub(crate) archive_notes: u64,
    pub(crate) instructions_lines: u64,
    pub(crate) preferences_lines: u64,
    pub(crate) topics_indexed: u64,
    pub(crate) team: u64,
    pub(crate) topics: Vec<GbotMemoryTopic>,
    pub(crate) changes: Vec<GbotMemoryChange>,
    pub(crate) log: Vec<GbotMemoryDay>,
    pub(crate) today: String,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// An ISO time as the computer's local "9 Nov, 14:05".
fn local_when(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%-d %b, %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| iso.to_string())
}

impl GbotMemoryView {
    /// The view in a `memoryShow` answer; `None` without its `memory`.
    pub(crate) fn from_show(answer: &Value) -> Option<Self> {
        let memory = answer.get("memory").filter(|memory| memory.is_object())?;
        let loads = &answer["loads"];
        let list = |key: &str| answer[key].as_array().cloned().unwrap_or_default();
        Some(Self {
            text: text(memory, "text"),
            hash: text(memory, "hash"),
            path: text(memory, "path"),
            lines: number(memory, "lines"),
            bytes: number(memory, "bytes"),
            max_lines: number(memory, "maxLines").max(1),
            max_bytes: number(memory, "maxBytes").max(1),
            share: memory["share"].as_f64().unwrap_or(0.0),
            level: match memory["level"].as_str() {
                Some("red") => GbotMemoryLevel::Red,
                Some("amber") => GbotMemoryLevel::Amber,
                _ => GbotMemoryLevel::Ok,
            },
            archive_notes: number(&answer["archive"], "notes"),
            instructions_lines: number(loads, "instructionsLines"),
            preferences_lines: number(loads, "preferencesLines"),
            topics_indexed: number(loads, "topicsIndexed"),
            team: number(loads, "team"),
            topics: list("topics")
                .iter()
                .map(|topic| GbotMemoryTopic {
                    name: text(topic, "name"),
                    title: text(topic, "title"),
                    description: text(topic, "description"),
                    bytes: number(topic, "bytes"),
                    path: text(topic, "path"),
                })
                .collect(),
            changes: list("journal")
                .iter()
                .map(|change| GbotMemoryChange {
                    id: text(change, "id"),
                    when: local_when(&text(change, "at")),
                    by: text(change, "byLabel"),
                    what: text(change, "what"),
                    undone: change["undone"].as_bool() == Some(true),
                    undoable: change["undoable"].as_bool() == Some(true),
                })
                .collect(),
            log: list("log")
                .iter()
                .map(|day| GbotMemoryDay {
                    date: text(day, "date"),
                    lines: day["lines"]
                        .as_array()
                        .map(|lines| {
                            lines
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect(),
            today: text(answer, "today"),
        })
    }

    /// "34 of 200 lines · 3.1 of 24 KB".
    pub(crate) fn gauge_label(&self) -> String {
        format!(
            "{} of {} lines · {:.1} of {} KB",
            self.lines,
            self.max_lines,
            self.bytes as f64 / 1024.0,
            self.max_bytes / 1024
        )
    }
}

/// "1.2 KB" / "640 bytes".
pub(crate) fn gbot_memory_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}
