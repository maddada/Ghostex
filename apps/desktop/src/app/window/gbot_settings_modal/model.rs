//! What the GBot Settings window edits: the bot as `/api/gbots {op:"show"}` answered it, the
//! choices its controls offer, and the fields a Save sends.
use serde_json::{Map, Value, json};

pub(crate) const GBOT_SETTINGS_MODAL_WIDTH: f32 = 880.0;
pub(crate) const GBOT_SETTINGS_MODAL_INITIAL_HEIGHT: f32 = 660.0;

/// A GBot's colours: gxserver's id and the hex it is drawn in.
/// SEE-ALSO: `COLORS` in server/src/gbots/record.rs (the ids gxserver accepts, CDXC:GBots 2026-11-08).
pub(crate) const GBOT_COLORS: [(&str, u32); 8] = [
    ("periwinkle", 0x9fb0ff),
    ("sky", 0x59d9ff),
    ("mint", 0x4ee6b8),
    ("gold", 0xf3cc5f),
    ("coral", 0xff8b6b),
    ("lavender", 0xd2a7ff),
    ("pink", 0xff9ee7),
    ("rose", 0xff5f73),
];

/// The icons the Look picker offers, each a file in apps/desktop/assets/titlebar/; `robot` (the
/// default every new bot gets) first.
pub(crate) const GBOT_ICONS: [&str; 16] = [
    "robot",
    "sparkles",
    "search",
    "pencil",
    "code",
    "bug",
    "rocket",
    "brain",
    "briefcase",
    "chart-bar",
    "bulb",
    "microscope",
    "book",
    "palette",
    "world",
    "shield-check",
];

/// The approval levels in gxserver's order, with their labels and the line under the control.
pub(crate) const GBOT_APPROVALS: [(&str, &str, &str); 4] = [
    (
        "ask",
        "Ask first",
        "It asks before anything that needs your approval.",
    ),
    (
        "edits",
        "Allow edits",
        "It edits files without asking and asks before anything else.",
    ),
    (
        "auto",
        "Autopilot",
        "For now this works like Allow edits: it edits files and asks before anything else.",
    ),
    (
        "full",
        "Full access",
        "It does everything without asking. Only you can choose this.",
    ),
];

/// Why a Codex GBot starts at Full access, shown under the approval control (the level itself is
/// gxserver's, server/src/gbots/session.rs `default_approval`).
pub(crate) const GBOT_CODEX_APPROVAL_NOTE: &str =
    "Codex GBots start at Full access: below it, Codex's sandbox stops them reaching Ghostex.";

/// The svg asset of a Ghostex icon name.
pub(crate) fn gbot_icon_path(icon: &str) -> String {
    let icon = if icon.trim().is_empty() {
        GBOT_ICONS[0]
    } else {
        icon.trim()
    };
    format!("titlebar/{icon}.svg")
}

/// The hex of colour `id`; the first colour for one gxserver does not know.
pub(crate) fn gbot_color_hex(id: &str) -> u32 {
    GBOT_COLORS
        .iter()
        .find(|(color, _)| *color == id)
        .map(|(_, hex)| *hex)
        .unwrap_or(GBOT_COLORS[0].1)
}

/// The editable part of a GBot as `show` answered it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GbotSettingsRecord {
    pub(crate) bot_id: String,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) soul: String,
    pub(crate) icon: String,
    pub(crate) color: String,
    pub(crate) agent_id: String,
    /// Empty for the agent's own default.
    pub(crate) model: String,
    pub(crate) approval: String,
    pub(crate) department_id: Option<String>,
    /// Why SOUL.md no longer matches the saved instructions (gxserver's sentence).
    pub(crate) soul_drift_message: Option<String>,
}

impl GbotSettingsRecord {
    /// The record in a `show` answer's `bot`; `None` without a bot id.
    pub(crate) fn from_show(bot: &Value) -> Option<Self> {
        let text = |key: &str| {
            bot.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let optional = |key: &str| {
            bot.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        let bot_id = text("botId");
        if bot_id.trim().is_empty() {
            return None;
        }
        Some(Self {
            bot_id,
            name: text("name"),
            title: text("title"),
            description: text("description"),
            soul: text("soul"),
            icon: optional("icon").unwrap_or_else(|| GBOT_ICONS[0].to_string()),
            color: optional("color").unwrap_or_else(|| GBOT_COLORS[0].0.to_string()),
            agent_id: text("agentId"),
            model: optional("model").unwrap_or_default(),
            approval: optional("approval").unwrap_or_else(|| "ask".to_string()),
            department_id: optional("departmentId"),
            soul_drift_message: optional("soulDriftMessage"),
        })
    }

    /// The `edit` fields that differ from `before`, trimmed the way gxserver stores them; a model
    /// cleared back to the agent's default is sent as `null`.
    pub(crate) fn changes_from(&self, before: &Self) -> Map<String, Value> {
        let mut changes = Map::new();
        let mut text = |key: &str, now: &str, then: &str| {
            if now.trim() != then.trim() {
                changes.insert(key.to_string(), json!(now.trim()));
            }
        };
        text("name", &self.name, &before.name);
        text("title", &self.title, &before.title);
        text("description", &self.description, &before.description);
        text("soul", &self.soul, &before.soul);
        text("icon", &self.icon, &before.icon);
        text("color", &self.color, &before.color);
        text("agentId", &self.agent_id, &before.agent_id);
        text("approval", &self.approval, &before.approval);
        if self.model.trim() != before.model.trim() {
            let model = self.model.trim();
            changes.insert(
                "model".to_string(),
                if model.is_empty() {
                    Value::Null
                } else {
                    json!(model)
                },
            );
        }
        changes
    }
}

/// The agents a GBot may run on, as `(agentId, name)`: the rows of `/api/chief {op:"status"}`
/// (the list gxserver checks a GBot's agent against), plus the bot's own agent when that list
/// lacks it, so the picker always shows what the bot runs on now.
pub(crate) fn gbot_agent_choices(status: Option<&Value>, current: &str) -> Vec<(String, String)> {
    let mut agents: Vec<(String, String)> = status
        .and_then(|status| status["agents"].as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let id = row["agentId"].as_str()?.trim();
                    if id.is_empty() {
                        return None;
                    }
                    let name = row["name"]
                        .as_str()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .unwrap_or(id);
                    Some((id.to_string(), name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let current = current.trim();
    if !current.is_empty() && !agents.iter().any(|(id, _)| id == current) {
        agents.insert(0, (current.to_string(), current.to_string()));
    }
    agents
}
