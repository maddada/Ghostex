//! The board data the native Kanban draws: Beads issues as `bd` returns them, the lanes a board's
//! status config yields, and the display ids and labels the board shows. Ported from the deleted
//! apps/desktop/views/project-board-shared.ts (in git history); this file is now the reference.

use serde_json::Value;

/// Lanes draw at most this many cards; the rest are summarised under the last card.
pub(crate) const MAX_VISIBLE_TICKETS_PER_LANE: usize = 120;

/// `PRIORITY_OPTIONS`: user-facing urgency labels over bd's numeric priorities.
pub(crate) const PRIORITY_OPTIONS: [(&str, &str); 4] = [
    ("Urgent", "0"),
    ("High", "1"),
    ("Medium", "2"),
    ("Low", "3"),
];

/// `TSHIRT_OPTIONS`: t-shirt estimates stored as minutes.
pub(crate) const TSHIRT_OPTIONS: [(&str, i64); 5] =
    [("XS", 15), ("S", 30), ("M", 60), ("L", 120), ("XL", 240)];

/// The statuses Ghostex keeps in every board's `status.custom` config.
const REQUIRED_CUSTOM_STATUS_CONFIG: [&str; 3] = ["backlog", "test", "review"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaneTone {
    Muted,
    Neutral,
    Blue,
    Amber,
    Violet,
    Green,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoardColumn {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) beads_status: String,
    pub(crate) tone: LaneTone,
}

const BUILTIN_COLUMNS: [(&str, &str, &str, LaneTone); 6] = [
    ("backlog", "Backlog", "backlog", LaneTone::Muted),
    ("todo", "Todo", "open", LaneTone::Neutral),
    ("in_progress", "In Progress", "in_progress", LaneTone::Blue),
    ("test", "Test", "test", LaneTone::Amber),
    ("review", "Review", "review", LaneTone::Violet),
    ("done", "Done", "closed", LaneTone::Green),
];

fn is_builtin_status_name(name: &str) -> bool {
    BUILTIN_COLUMNS
        .iter()
        .any(|(key, _, status, _)| *key == name || *status == name)
}

/// `buildBoardColumns`: the six built-in lanes, then one muted lane per extra configured status.
pub(crate) fn build_board_columns(custom_status_config: &str) -> Vec<BoardColumn> {
    let mut columns = BUILTIN_COLUMNS
        .iter()
        .map(|(key, label, status, tone)| BoardColumn {
            key: (*key).to_string(),
            label: (*label).to_string(),
            beads_status: (*status).to_string(),
            tone: *tone,
        })
        .collect::<Vec<_>>();
    for entry in parse_board_column_config(custom_status_config) {
        if is_builtin_status_name(&entry.name) {
            continue;
        }
        columns.push(BoardColumn {
            label: status_name_to_label(&entry.name),
            key: entry.name.clone(),
            beads_status: entry.name,
            tone: LaneTone::Muted,
        });
    }
    columns
}

pub(crate) fn status_name_to_label(name: &str) -> String {
    name.split(|ch: char| ch.is_whitespace() || ch == '_' || ch == '-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoardColumnConfigEntry {
    pub(crate) name: String,
    /// The entry's optional `:<bd category>` suffix, preserved on every write.
    pub(crate) suffix: String,
}

pub(crate) fn parse_board_column_config(config: &str) -> Vec<BoardColumnConfigEntry> {
    let mut entries = Vec::<BoardColumnConfigEntry>::new();
    for raw in config.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let (name, suffix) = match entry.find(':') {
            Some(index) => (entry[..index].trim(), &entry[index..]),
            None => (entry, ""),
        };
        if name.is_empty() || entries.iter().any(|existing| existing.name == name) {
            continue;
        }
        entries.push(BoardColumnConfigEntry {
            name: name.to_string(),
            suffix: suffix.to_string(),
        });
    }
    entries
}

fn serialize_board_column_config(entries: &[BoardColumnConfigEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}{}", entry.name, entry.suffix))
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn managed_board_column_names(config: &str) -> Vec<String> {
    parse_board_column_config(config)
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| !is_builtin_status_name(name))
        .collect()
}

/// `boardColumnNameError`: the same rule gxserver applies to a status value.
pub(crate) fn board_column_name_error(name: &str, config: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Some("Enter a column name.".to_string());
    }
    if trimmed.chars().count() > 64 {
        return Some("Column names are limited to 64 characters.".to_string());
    }
    let valid = trimmed.chars().enumerate().all(|(index, ch)| {
        if index == 0 {
            ch.is_ascii_alphabetic()
        } else {
            ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
        }
    });
    if !valid {
        return Some(
            "Use letters, numbers, hyphens and underscores, starting with a letter.".to_string(),
        );
    }
    if is_builtin_status_name(trimmed) {
        return Some(format!(
            "{} is a built-in lane.",
            status_name_to_label(trimmed)
        ));
    }
    if parse_board_column_config(config)
        .iter()
        .any(|entry| entry.name == trimmed)
    {
        return Some("That column already exists.".to_string());
    }
    None
}

pub(crate) fn add_board_column(config: &str, name: &str) -> String {
    let mut entries = parse_board_column_config(config);
    entries.push(BoardColumnConfigEntry {
        name: name.trim().to_string(),
        suffix: String::new(),
    });
    serialize_board_column_config(&entries)
}

pub(crate) fn remove_board_column(config: &str, name: &str) -> String {
    let entries = parse_board_column_config(config)
        .into_iter()
        .filter(|entry| !(entry.name == name && !is_builtin_status_name(&entry.name)))
        .collect::<Vec<_>>();
    serialize_board_column_config(&entries)
}

/// `moveBoardColumn`: swaps a managed column with its managed neighbour, leaving built-ins in place.
pub(crate) fn move_board_column(config: &str, name: &str, delta: isize) -> String {
    let mut entries = parse_board_column_config(config);
    let managed = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !is_builtin_status_name(&entry.name))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let Some(position) = managed
        .iter()
        .position(|index| entries[*index].name == name)
    else {
        return serialize_board_column_config(&entries);
    };
    let target = position as isize + delta;
    if target < 0 || target as usize >= managed.len() {
        return serialize_board_column_config(&entries);
    }
    entries.swap(managed[position], managed[target as usize]);
    serialize_board_column_config(&entries)
}

/// `ensureWorkflowStatuses`' reconciliation: the required entries lose any suffix and are added
/// when missing. Returns the config to write, or `None` when it is already reconciled.
pub(crate) fn reconciled_workflow_statuses(current: &str) -> Option<String> {
    let current_entries = current
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    let entry_name = |entry: &str| entry.split(':').next().unwrap_or_default().to_string();
    let mut next = current_entries
        .iter()
        .map(|entry| {
            let name = entry_name(entry);
            if REQUIRED_CUSTOM_STATUS_CONFIG.contains(&name.as_str()) {
                name
            } else {
                (*entry).to_string()
            }
        })
        .collect::<Vec<_>>();
    for required in REQUIRED_CUSTOM_STATUS_CONFIG {
        if !current_entries
            .iter()
            .any(|entry| entry_name(entry) == required)
        {
            next.push(required.to_string());
        }
    }
    let next = next.join(",");
    (next != current).then_some(next)
}

/// `beadsStatusToBoardStatus`: a status with no lane is drawn in Todo.
pub(crate) fn board_status_for(status: &str, columns: &[BoardColumn]) -> String {
    columns
        .iter()
        .find(|column| column.beads_status == status)
        .map(|column| column.key.clone())
        .unwrap_or_else(|| "todo".to_string())
}

pub(crate) fn board_status_label(key: &str, columns: &[BoardColumn]) -> String {
    columns
        .iter()
        .find(|column| column.key == key)
        .map(|column| column.label.clone())
        .unwrap_or_else(|| "Todo".to_string())
}

pub(crate) fn board_status_beads_value(key: &str, columns: &[BoardColumn]) -> String {
    columns
        .iter()
        .find(|column| column.key == key)
        .map(|column| column.beads_status.clone())
        .unwrap_or_else(|| "open".to_string())
}

pub(crate) fn columns_signature(columns: &[BoardColumn]) -> String {
    columns
        .iter()
        .map(|column| column.key.as_str())
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BeadsComment {
    pub(crate) author: String,
    pub(crate) created_at: Option<String>,
    pub(crate) text: String,
}

/// One of bd's `dependencies[]`: the issue waits on `depends_on_id`. `kind` is bd's dependency
/// type; only `blocks` takes part in bd's cycle check (`blockers.rs`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BeadsDependency {
    pub(crate) depends_on_id: String,
    pub(crate) kind: String,
}

pub(crate) const BLOCKS_DEPENDENCY: &str = "blocks";

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BeadsIssue {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) description: String,
    pub(crate) priority: Option<i64>,
    pub(crate) estimate: Option<i64>,
    pub(crate) labels: Vec<String>,
    pub(crate) assignee: Option<String>,
    pub(crate) created_by: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) closed_at: Option<String>,
    pub(crate) comment_count: Option<u64>,
    pub(crate) comments: Vec<BeadsComment>,
    pub(crate) dependency_count: Option<u64>,
    pub(crate) dependent_count: Option<u64>,
    /// `dependencies[]`: what this issue waits on, of every kind bd records.
    pub(crate) dependencies: Vec<BeadsDependency>,
}

impl BeadsIssue {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let id = object.get("id")?.as_str()?.to_string();
        let text = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let number = |key: &str| object.get(key).and_then(Value::as_f64).map(|v| v as i64);
        let count = |key: &str| object.get(key).and_then(Value::as_u64);
        let labels = object
            .get("labels")
            .and_then(Value::as_array)
            .map(|labels| {
                labels
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let comments = object
            .get("comments")
            .and_then(Value::as_array)
            .map(|comments| {
                comments
                    .iter()
                    .filter_map(|comment| {
                        Some(BeadsComment {
                            author: comment
                                .get("author")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            created_at: comment
                                .get("created_at")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            text: comment.get("text")?.as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let dependencies = object
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|dependencies| {
                dependencies
                    .iter()
                    .filter_map(|dependency| {
                        let depends_on_id = dependency.get("depends_on_id")?.as_str()?;
                        (!depends_on_id.is_empty()).then(|| BeadsDependency {
                            depends_on_id: depends_on_id.to_string(),
                            kind: dependency
                                .get("type")
                                .and_then(Value::as_str)
                                .filter(|kind| !kind.is_empty())
                                .unwrap_or(BLOCKS_DEPENDENCY)
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            title: text("title").unwrap_or_default(),
            status: text("status").unwrap_or_else(|| "open".to_string()),
            description: text("description").unwrap_or_default(),
            priority: number("priority"),
            estimate: number("estimate"),
            labels,
            assignee: text("assignee"),
            created_by: text("created_by"),
            created_at: text("created_at"),
            updated_at: text("updated_at"),
            closed_at: text("closed_at"),
            comment_count: count("comment_count"),
            comments,
            dependency_count: count("dependency_count"),
            dependent_count: count("dependent_count"),
            dependencies,
            id,
        })
    }

    pub(crate) fn comment_total(&self) -> u64 {
        self.comment_count.unwrap_or(self.comments.len() as u64)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoardTicket {
    pub(crate) issue: BeadsIssue,
    pub(crate) board_status: String,
    pub(crate) display_id: String,
}

/// `normalizeIssuePrefix`.
pub(crate) fn normalize_issue_prefix(value: &str) -> String {
    let normalized = value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '-')
        .take(8)
        .collect::<String>();
    if normalized.is_empty() {
        return "zmux".to_string();
    }
    if normalized.starts_with(|ch: char| ch.is_ascii_lowercase()) {
        normalized
    } else {
        format!("p-{normalized}")
    }
}

/// Stale bootstrap prefixes the board replaces with the project's own; any other established
/// prefix is kept (see `ensureIssuePrefix`).
pub(crate) fn is_bootstrap_issue_prefix(prefix: &str) -> bool {
    prefix == "gxserver" || prefix == "zmux"
}

/// `normalizeDisplayIssueKey`.
pub(crate) fn normalize_display_issue_key(value: &str) -> String {
    let normalized = value
        .trim()
        .to_uppercase()
        .chars()
        .filter(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        .take(3)
        .collect::<String>();
    if normalized.is_empty() {
        "PRJ".to_string()
    } else {
        normalized
    }
}

pub(crate) fn parse_time_ms(value: Option<&str>) -> Option<i64> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp_millis())
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|time| time.and_utc().timestamp_millis())
        })
}

/// `toBoardTickets`: lane keys against the board's columns, and `KEY-<serial>` ids numbered by
/// creation order.
pub(crate) fn to_board_tickets(
    issues: &[BeadsIssue],
    display_key: &str,
    columns: &[BoardColumn],
) -> Vec<BoardTicket> {
    let mut order = issues.iter().collect::<Vec<_>>();
    order.sort_by(|left, right| {
        let left_time = parse_time_ms(left.created_at.as_deref());
        let right_time = parse_time_ms(right.created_at.as_deref());
        match (left_time, right_time) {
            (Some(left_time), Some(right_time)) if left_time != right_time => {
                left_time.cmp(&right_time)
            }
            _ => left.id.cmp(&right.id),
        }
    });
    let serials = order
        .iter()
        .enumerate()
        .map(|(index, issue)| (issue.id.as_str(), index + 1))
        .collect::<std::collections::HashMap<_, _>>();
    issues
        .iter()
        .map(|issue| BoardTicket {
            board_status: board_status_for(&issue.status, columns),
            display_id: serials
                .get(issue.id.as_str())
                .map(|serial| format!("{display_key}-{serial}"))
                .unwrap_or_else(|| issue.id.clone()),
            issue: issue.clone(),
        })
        .collect()
}

/// `createIssuesSignature`: what a refresh compares before replacing the board.
pub(crate) fn issues_signature(issues: &[BeadsIssue]) -> String {
    issues
        .iter()
        .map(|issue| {
            [
                issue.id.clone(),
                issue.status.clone(),
                issue.updated_at.clone().unwrap_or_default(),
                issue.title.clone(),
                issue.priority.map(|v| v.to_string()).unwrap_or_default(),
                issue.estimate.map(|v| v.to_string()).unwrap_or_default(),
                issue.comment_total().to_string(),
                issue
                    .dependency_count
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                issue
                    .dependent_count
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                issue.labels.join(","),
                // bd leaves `updated_at` alone on `dep add`/`dep remove`, so a blocker swapped for
                // another (same counts) would otherwise never reach the board.
                issue
                    .dependencies
                    .iter()
                    .map(|dependency| format!("{}:{}", dependency.kind, dependency.depends_on_id))
                    .collect::<Vec<_>>()
                    .join(","),
            ]
            .join("\u{1f}")
        })
        .collect::<Vec<_>>()
        .join("\u{1e}")
}

/// Labels the loaded tickets carry, for the label suggestions in the ticket panel.
pub(crate) fn known_labels(issues: &[BeadsIssue]) -> Vec<String> {
    let mut labels = issues
        .iter()
        .flat_map(|issue| issue.labels.iter())
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    labels
}

/// `ticketCreatorName`: the creator is hidden when it is also the assignee.
pub(crate) fn ticket_creator_name(issue: &BeadsIssue) -> Option<&str> {
    let creator = issue.created_by.as_deref()?;
    (Some(creator) != issue.assignee.as_deref()).then_some(creator)
}

pub(crate) fn estimate_to_tshirt(estimate: Option<i64>) -> Option<&'static str> {
    let estimate = estimate?;
    TSHIRT_OPTIONS
        .iter()
        .find(|(_, minutes)| *minutes == estimate)
        .map(|(label, _)| *label)
}

pub(crate) fn tshirt_to_estimate(label: Option<&str>) -> Option<i64> {
    let label = label?;
    TSHIRT_OPTIONS
        .iter()
        .find(|(option, _)| *option == label)
        .map(|(_, minutes)| *minutes)
}

/// `prioritySelectValue`: the visible tier, with the legacy lowest value folded into Low.
pub(crate) fn priority_select_value(priority: Option<i64>) -> &'static str {
    match priority.unwrap_or(2) {
        0 => "0",
        1 => "1",
        2 => "2",
        _ => "3",
    }
}

pub(crate) fn priority_label(priority: Option<i64>) -> &'static str {
    let value = priority_select_value(priority);
    PRIORITY_OPTIONS
        .iter()
        .find(|(_, option)| *option == value)
        .map(|(label, _)| *label)
        .unwrap_or("Low")
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KanbanAgent {
    pub(crate) agent_id: String,
    pub(crate) label: String,
    pub(crate) command: Option<String>,
}

/// A bead's linked agent conversation, as the board conversation state reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KanbanConversationLink {
    pub(crate) id: String,
    pub(crate) bead_id: String,
    pub(crate) ghostex_session_id: String,
    pub(crate) label: String,
    pub(crate) openable: bool,
    pub(crate) resumable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KanbanConversationState {
    pub(crate) agents: Vec<KanbanAgent>,
    pub(crate) default_agent_id: Option<String>,
    pub(crate) links: Vec<KanbanConversationLink>,
}

impl KanbanConversationState {
    pub(crate) fn from_value(value: &Value) -> Self {
        let string = |value: &Value, key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let agents = value
            .get("agents")
            .and_then(Value::as_array)
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|agent| {
                        let agent_id = string(agent, "agentId")?;
                        Some(KanbanAgent {
                            label: string(agent, "label").unwrap_or_else(|| agent_id.clone()),
                            command: string(agent, "command"),
                            agent_id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut links = value
            .get("links")
            .and_then(Value::as_array)
            .map(|links| {
                links
                    .iter()
                    .filter_map(|link| {
                        let flag = |key: &str| link.get(key).and_then(Value::as_bool) == Some(true);
                        Some((
                            string(link, "createdAt").unwrap_or_default(),
                            KanbanConversationLink {
                                id: string(link, "id")?,
                                bead_id: string(link, "beadId")?,
                                ghostex_session_id: string(link, "ghostexSessionId")
                                    .unwrap_or_default(),
                                label: string(link, "sessionTitle")
                                    .or_else(|| string(link, "agentName"))
                                    .or_else(|| string(link, "agentId"))
                                    .or_else(|| string(link, "agentSessionId"))
                                    .unwrap_or_else(|| "Agent session".to_string()),
                                openable: flag("isLive") || flag("isRestorable"),
                                resumable: flag("isResumable"),
                            },
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Newest first, like `compareConversationLinksNewestFirst`.
        links.sort_by(|left, right| right.0.cmp(&left.0));
        Self {
            agents,
            default_agent_id: string(value, "defaultAgentId"),
            links: links.into_iter().map(|(_, link)| link).collect(),
        }
    }

    /// `selectBeadConversationLinks` + `getPrimaryUsableConversationLink`.
    pub(crate) fn primary_link_for(&self, bead_id: &str) -> Option<&KanbanConversationLink> {
        let key = bead_link_match_key(bead_id);
        self.links
            .iter()
            .filter(|link| bead_link_match_key(&link.bead_id) == key)
            .find(|link| link.openable || link.resumable)
    }

    /// The agent a ticket starts with: its assignee when that names a configured agent, else the
    /// board's default (`resolveAssignedAgentId`).
    pub(crate) fn start_agent_for(&self, assignee: Option<&str>) -> Option<&KanbanAgent> {
        let assigned = assignee
            .map(|assignee| assignee.trim().to_lowercase())
            .filter(|assignee| !assignee.is_empty())
            .and_then(|assignee| {
                let find = |candidate: &str| {
                    self.agents.iter().find(|agent| {
                        agent.label.trim().to_lowercase() == candidate
                            || agent.agent_id.trim().to_lowercase() == candidate
                    })
                };
                find(&assignee).or_else(|| {
                    ["-code", "-cli"].iter().find_map(|suffix| {
                        assignee
                            .strip_suffix(suffix)
                            .filter(|rest| !rest.is_empty())
                            .and_then(find)
                    })
                })
            });
        assigned
            .or_else(|| {
                self.default_agent_id
                    .as_deref()
                    .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id))
            })
            .or_else(|| self.agents.first())
    }
}

/// `beadConversationLinkMatchKey`: the part after the last `-`, lowercased.
fn bead_link_match_key(bead_id: &str) -> String {
    let normalized = bead_id.trim().to_lowercase();
    match normalized.rfind('-') {
        Some(index) if index > 0 && index + 1 < normalized.len() => {
            normalized[index + 1..].to_string()
        }
        _ => normalized,
    }
}
