//! `ghostex coordinator setup`: the coordinator's setup interview (server/src/coordinators/setup.rs
//! keeps it). `page` writes the interview Live page from what this computer has signed in and
//! working, prefilled from this coordinator's setup, else the user's default, else the playbook's
//! picks; `save` stores the page's answers as the user's default and as this coordinator's "Setup:"
//! memory notes; `copy` takes another coordinator's notes; `show` prints the setup in effect;
//! `explain` shows the coordinator flow page Ghostex ships.
//!
//! CDXC:Coordinators 2026-11-26 DECISION:
//! The user, for the interview: each kind of work "a choice of agent + model + effort, only from agents and models signed in and working", "Offer only current models: keep the newest generation of each family", never Flash, Lite or Mini tiers or Cursor Composer, simple work able to take Opus 5.5 Low and Medium, GPT-6.2 Astra Low and Gemini 4 Neon Low and Medium. After the mockup they asked for "EVERY agent the user has enabled that a coordinator can start threads on" with its logo and its real models and efforts, usage "only where Ghostex knows it", no quick picks, and each kind of work on its own full-width card with the agent picked from logo tiles and the model and effort as Ghostex's segmented buttons, not plain selects. When usage runs low they picked "Switch when low" with the other agent's models "matched by kind", so the fallback takes the other agent's main model at High for large work, Medium for hard and Low for simple; a free-text rule (theirs: "split usage between Opus 5.5 and Antigravity sessions, but if all Claude sessions are running low, only use Antigravity for all threads") is kept word for word. Accounts never to use are asked only for an agent with more than one account, and map to "Only when I pick it" in Settings > Accounts, which only the user can set. Computers default to all.
//! SEE-ALSO: server/src/coordinators/setup.rs (storage), server/src/coordinators/playbook.md (First turn: setup), server/src/visual_pages/kit.rs (`picked`, menus questions and `data-look`).

use std::path::PathBuf;

use serde_json::{json, Value};

use super::command::{
    flag_text, options_view, read_view, resolve_session, target_coordinator, text_or_file,
};
use super::usage_room;
use crate::coordinators::{COORDINATOR_FLOW_PAGE_HTML, SETUP_NOTE_PREFIX, SETUP_VERSION};
use crate::ghostex_cli::{
    agents,
    args::{parse_args, Flags, ParsedArgs},
    output::print_json,
    rpc::{call_gxserver_rpc, gxserver_root, CliError, CliResult},
};

/// The three kinds of work: key, title, what it covers, and the effort the fallback agent matches
/// it with.
const KINDS: [(&str, &str, &str, &str); 3] = [
    (
        "large",
        "Large, complex work",
        "Features and refactors across many files.",
        "high",
    ),
    (
        "hard",
        "Hard, focused work",
        "A tricky bug or a careful change in a few files.",
        "medium",
    ),
    (
        "simple",
        "Simple, contained work",
        "A one-file fix, a copy change, a quick look.",
        "low",
    ),
];

/// The playbook's picks before the user answers: agent family, model value, effort.
const DEFAULT_WORK: [(&str, &str, &str); 3] = [
    ("claude", "opus[1m]", "high"),
    ("claude", "opus[1m]", "medium"),
    ("claude", "sonnet", "high"),
];

const KIT_OTHER: &str = "Other / none of these";
const KIT_SKIPPED: &str = "skipped";

/// Model tiers the interview never offers (the user: never Flash, Lite or Mini tiers, or Cursor
/// Composer), plus the ones that are not a model to work on (auto, orchestrators, fast and
/// special-purpose variants).
const NEVER_OFFERED: [&str; 13] = [
    "flash",
    "lite",
    "mini",
    "nano",
    "haiku",
    "composer",
    "orchestrator",
    "healthcare",
    "fast",
    "ultrafast",
    "cyber",
    "oss",
    "auto",
];

const NO_MODEL: &str = "Its own model";
const NO_EFFORT: &str = "No effort choice";

pub(super) fn setup(args: &[String]) -> CliResult<()> {
    let verb = args.first().map(String::as_str).unwrap_or("show");
    let rest = args.get(1..).unwrap_or_default();
    let parsed = parse_args(rest);
    match verb {
        "page" => page(&parsed),
        "save" => save(&parsed),
        "copy" => copy(&parsed),
        "show" => show(&parsed),
        "explain" => explain(rest),
        other => Err(CliError::Other(format!(
            "Unknown setup command: {other}. Use page, save, copy, show or explain (ghostex coordinator setup --help)."
        ))),
    }
}

// ---------------------------------------------------------------------------------------------
// The lineup: agents, their current models and efforts.

#[derive(Clone, Debug)]
struct Model {
    value: String,
    label: String,
    efforts: Vec<(String, String)>,
    default_effort: Option<String>,
}

#[derive(Clone, Debug)]
struct Agent {
    id: String,
    name: String,
    family: String,
    icon: String,
    /// The current models, the ones the page offers.
    models: Vec<Model>,
    /// Every model, to read back a pick the page no longer offers.
    all_models: Vec<Model>,
    default_model: Option<String>,
    /// What Ghostex knows of its usage, for its tile.
    note: String,
}

impl Agent {
    fn short(&self) -> String {
        self.name
            .strip_suffix(" CLI")
            .unwrap_or(&self.name)
            .to_string()
    }

    /// Its main model among the offered ones.
    fn main_model(&self) -> Option<&Model> {
        self.default_model
            .as_deref()
            .and_then(|value| self.models.iter().find(|model| model.value == value))
            .or_else(|| self.models.first())
    }

    fn model(&self, wanted: &str) -> Option<&Model> {
        let wanted = wanted.trim();
        self.models
            .iter()
            .chain(self.all_models.iter())
            .find(|model| model.value == wanted || model.label.eq_ignore_ascii_case(wanted))
    }
}

fn model_rows(value: &Value) -> Vec<Model> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let value = model["value"].as_str()?.to_string();
            Some(Model {
                label: model["label"].as_str().unwrap_or(&value).to_string(),
                efforts: model["efforts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|effort| {
                        let value = effort["value"].as_str()?.to_string();
                        let label = effort["label"].as_str().unwrap_or(&value).to_string();
                        Some((value, label))
                    })
                    .collect(),
                default_effort: model["defaultEffort"].as_str().map(str::to_string),
                value,
            })
        })
        .collect()
}

fn tokens(value: &str) -> Vec<String> {
    value
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// A model's family and version: the vendor (each Claude tier on its own, since Fable 5.2 sits
/// beside Opus 5.5, while GPT 6.2 Sol and Astra share one generation) and every number in its id.
fn family_and_version(value: &str) -> (String, Vec<u64>) {
    let (route, id) = value.split_once('/').unwrap_or(("", value));
    let id = id.split('[').next().unwrap_or(id);
    let id = id.strip_prefix("cursor-").unwrap_or(id).to_ascii_lowercase();
    let words: Vec<String> = id
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    let numbers: Vec<u64> = id
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|number| number.parse().ok())
        .collect();
    let vendor = words.first().cloned().unwrap_or_else(|| id.clone());
    let family = if vendor == "claude" {
        words
            .iter()
            .filter(|word| matches!(word.as_str(), "claude" | "opus" | "sonnet" | "fable"))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        vendor
    };
    (format!("{route}|{family}"), numbers)
}

/// The models worth offering: no never-offered tier, no catch-all OpenRouter route (hundreds of
/// models), and of each family only its newest generation. A model without a version (an alias
/// such as `opus[1m]`) always follows the newest, so it stays.
fn current_models(models: &[Model]) -> Vec<Model> {
    let offered: Vec<&Model> = models
        .iter()
        .filter(|model| !model.value.is_empty() && !model.value.starts_with("openrouter/"))
        .filter(|model| {
            !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(&model.value)
        })
        .filter(|model| {
            !tokens(&model.value)
                .iter()
                .any(|token| NEVER_OFFERED.contains(&token.as_str()))
        })
        .collect();
    let mut newest: std::collections::BTreeMap<String, Vec<u64>> = Default::default();
    for model in &offered {
        let (family, version) = family_and_version(&model.value);
        if version.is_empty() {
            continue;
        }
        let entry = newest.entry(family).or_default();
        if version > *entry {
            *entry = version;
        }
    }
    offered
        .into_iter()
        .filter(|model| {
            let (family, version) = family_and_version(&model.value);
            version.is_empty() || newest.get(&family) == Some(&version)
        })
        .cloned()
        .collect()
}

/// What Ghostex knows of an agent's usage, for its tile (nothing when it has no reading).
fn usage_note(room: &Value, agent_id: &str) -> String {
    let Some(provider) = usage_room::provider_of(agent_id) else {
        return String::new();
    };
    let Some(row) = room["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["agent"] == provider)
    else {
        return String::new();
    };
    let accounts = row["accounts"].as_u64().unwrap_or(0);
    if accounts == 0 {
        return String::new();
    }
    let count = if accounts == 1 {
        "1 account".to_string()
    } else {
        format!("{accounts} accounts")
    };
    if row["usable"].as_u64() == Some(0) {
        return format!("{count} · no room now");
    }
    match row["best"]["usedPercent"].as_f64() {
        Some(used) => format!("{count} · {}% used", used.round() as i64),
        None => count,
    }
}

fn lineup(options: &Value, room: &Value) -> Vec<Agent> {
    options["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let id = row["agentId"].as_str()?.to_string();
            let all_models = model_rows(&row["models"]);
            Some(Agent {
                name: row["name"].as_str().unwrap_or(&id).to_string(),
                family: row["family"].as_str().unwrap_or_default().to_string(),
                icon: row["icon"].as_str().unwrap_or_default().to_string(),
                models: current_models(&all_models),
                all_models,
                default_model: row["defaultModel"].as_str().map(str::to_string),
                note: usage_note(room, &id),
                id,
            })
        })
        .collect()
}

fn find_agent<'a>(agents: &'a [Agent], wanted: &str) -> Option<&'a Agent> {
    let wanted = wanted.trim();
    agents
        .iter()
        .find(|agent| agent.id == wanted)
        .or_else(|| {
            agents.iter().find(|agent| {
                agent.name.eq_ignore_ascii_case(wanted) || agent.short().eq_ignore_ascii_case(wanted)
            })
        })
        .or_else(|| agents.iter().find(|agent| agent.family == wanted))
}

// ---------------------------------------------------------------------------------------------
// One pick of agent, model and effort, and the setup as saved.

#[derive(Clone, Debug)]
struct Pick {
    agent: String,
    agent_label: String,
    model: String,
    model_label: String,
    effort: String,
    effort_label: String,
}

impl Pick {
    fn of(agent: &Agent, model: Option<&Model>, effort: Option<&str>) -> Pick {
        let effort = model.and_then(|model| {
            let wanted = effort
                .map(str::to_string)
                .or_else(|| model.default_effort.clone())?;
            nearest_effort(model, &wanted)
        });
        Pick {
            agent: agent.id.clone(),
            agent_label: agent.name.clone(),
            model: model.map(|model| model.value.clone()).unwrap_or_default(),
            model_label: model.map(|model| model.label.clone()).unwrap_or_default(),
            effort: effort.as_ref().map(|e| e.0.clone()).unwrap_or_default(),
            effort_label: effort.map(|e| e.1).unwrap_or_default(),
        }
    }

    fn from_json(value: &Value) -> Option<Pick> {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let agent = text("agent");
        (!agent.is_empty()).then(|| Pick {
            agent_label: Some(text("agentLabel"))
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| agent.clone()),
            agent,
            model: text("model"),
            model_label: text("modelLabel"),
            effort: text("effort"),
            effort_label: text("effortLabel"),
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "agent": self.agent,
            "agentLabel": self.agent_label,
            "model": self.model,
            "modelLabel": self.model_label,
            "effort": self.effort,
            "effortLabel": self.effort_label,
        })
    }

    /// "Claude · Opus 5.5 · High"
    fn words(&self) -> String {
        let mut parts = vec![self.agent_label.clone()];
        parts.push(if self.model_label.is_empty() {
            NO_MODEL.to_ascii_lowercase()
        } else {
            self.model_label.clone()
        });
        if !self.effort_label.is_empty() {
            parts.push(self.effort_label.clone());
        }
        parts.join(" · ")
    }

    /// The `start-thread` flags for it.
    fn flags(&self) -> String {
        let mut flags = format!("--agent {}", self.agent);
        if !self.model.is_empty() {
            flags.push_str(&format!(" --model {}", self.model));
        }
        if !self.effort.is_empty() {
            flags.push_str(&format!(" --effort {}", self.effort));
        }
        flags
    }
}

/// `wanted` when the model has it, else the closest level it has (Low is closest to Medium, XHigh
/// to High), else none.
fn nearest_effort(model: &Model, wanted: &str) -> Option<(String, String)> {
    const ORDER: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];
    let wanted = wanted.trim().to_ascii_lowercase();
    if let Some(found) = model
        .efforts
        .iter()
        .find(|(value, label)| *value == wanted || label.eq_ignore_ascii_case(&wanted))
    {
        return Some(found.clone());
    }
    let at = ORDER.iter().position(|level| *level == wanted)? as i64;
    model
        .efforts
        .iter()
        .filter_map(|effort| {
            let rank = ORDER.iter().position(|level| *level == effort.0)? as i64;
            Some(((rank - at).abs(), -rank, effort))
        })
        .min_by_key(|(distance, rank, _)| (*distance, *rank))
        .map(|(_, _, effort)| effort.clone())
}

#[derive(Clone, Debug)]
struct Setup {
    work: [Pick; 3],
    /// "stay", "switch", "share", "ask" or "own".
    low: String,
    other: Option<Agent>,
    other_work: Vec<Pick>,
    rule: String,
    never_use: Vec<String>,
    /// `None` is all computers.
    computers: Option<Vec<String>>,
}

impl Setup {
    fn main_label(&self) -> String {
        short_name(&self.work[0].agent_label)
    }

    fn to_json(&self) -> Value {
        json!({
            "version": SETUP_VERSION,
            "work": {
                "large": self.work[0].to_json(),
                "hard": self.work[1].to_json(),
                "simple": self.work[2].to_json(),
            },
            "lowUsage": {
                "choice": self.low,
                "otherAgent": self.other.as_ref().map(|agent| agent.id.clone()),
                "otherAgentLabel": self.other.as_ref().map(|agent| agent.name.clone()),
                "otherWork": self.other_work.iter().map(Pick::to_json).collect::<Vec<_>>(),
                "rule": self.rule,
            },
            "neverUse": self.never_use,
            "computers": match &self.computers {
                None => json!("all"),
                Some(list) => json!(list),
            },
            "updatedAt": chrono::Utc::now().to_rfc3339(),
        })
    }

    /// The coordinator's memory notes for it, one plain line each.
    fn notes(&self) -> Vec<String> {
        let mut notes: Vec<String> = KINDS
            .iter()
            .zip(&self.work)
            .map(|((_, title, _, _), pick)| {
                format!(
                    "{SETUP_NOTE_PREFIX} {}: {} (start-thread {})",
                    title.to_ascii_lowercase(),
                    pick.words(),
                    pick.flags()
                )
            })
            .collect();
        let mut low = format!("{SETUP_NOTE_PREFIX} when usage runs low: {}", self.rule);
        if matches!(self.low.as_str(), "switch" | "share") {
            if let Some(first) = self.other_work.first() {
                let efforts: Vec<&str> = self
                    .other_work
                    .iter()
                    .map(|pick| pick.effort.as_str())
                    .filter(|effort| !effort.is_empty())
                    .collect();
                let mut flags = format!("--agent {}", first.agent);
                if !first.model.is_empty() {
                    flags.push_str(&format!(" --model {}", first.model));
                }
                if !efforts.is_empty() {
                    flags.push_str(&format!(" --effort {}", efforts.join("|")));
                }
                low.push_str(&format!(" (start-thread {flags}, by kind of work)"));
            }
        }
        notes.push(low);
        notes.push(if self.never_use.is_empty() {
            format!("{SETUP_NOTE_PREFIX} accounts threads never use: none")
        } else {
            format!(
                "{SETUP_NOTE_PREFIX} accounts threads never use: {} (only the user can set them to \"Only when I pick it\" in Settings > Accounts; ask them to)",
                self.never_use.join(", ")
            )
        });
        notes.push(format!(
            "{SETUP_NOTE_PREFIX} computers threads may run on: {}",
            match &self.computers {
                None => "all".to_string(),
                Some(list) => list.join(", "),
            }
        ));
        notes
    }
}

fn short_name(name: &str) -> String {
    name.strip_suffix(" CLI").unwrap_or(name).to_string()
}

/// The fallback agent's models, matched by kind of work: its main model at the kind's effort.
fn matched_work(other: &Agent) -> Vec<Pick> {
    KINDS
        .iter()
        .map(|(_, _, _, effort)| Pick::of(other, other.main_model(), Some(effort)))
        .collect()
}

/// The sentence the coordinator follows when usage runs low.
fn rule_for(low: &str, main: &str, other: Option<&Agent>, other_work: &[Pick]) -> String {
    let other_name = other
        .map(|agent| agent.short())
        .unwrap_or_else(|| "another agent".to_string());
    let lineup = other_work
        .iter()
        .zip(KINDS.iter())
        .map(|(pick, (key, _, _, _))| {
            let model = if pick.model_label.is_empty() {
                NO_MODEL.to_ascii_lowercase()
            } else {
                pick.model_label.clone()
            };
            let effort = if pick.effort_label.is_empty() {
                String::new()
            } else {
                format!(" {}", pick.effort_label)
            };
            format!("{key} work {model}{effort}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let lineup = if lineup.is_empty() {
        String::new()
    } else {
        format!(" ({lineup})")
    };
    match low {
        "stay" => format!("stay on {main}: once every {main} account is low, threads wait for the reset; never switch agent."),
        "switch" => format!("{main} until every {main} account is low, then every new thread on {other_name}{lineup}."),
        "share" => format!("split new threads between {main} and {other_name}; once every {main} account is low, every new thread on {other_name}{lineup}."),
        _ => format!("ask the user each time, before {main} runs out."),
    }
}

/// The setup before any answer: the playbook's picks on this computer's lineup.
fn playbook_setup(agents: &[Agent]) -> Setup {
    let work = DEFAULT_WORK.map(|(family, model, effort)| {
        let agent = find_agent(agents, family).or_else(|| agents.first());
        match agent {
            Some(agent) => {
                let model = agent
                    .models
                    .iter()
                    .find(|m| m.value == model)
                    .or_else(|| agent.main_model());
                Pick::of(agent, model, Some(effort))
            }
            None => Pick {
                agent: family.to_string(),
                agent_label: "Claude".to_string(),
                model: model.to_string(),
                model_label: String::new(),
                effort: effort.to_string(),
                effort_label: String::new(),
            },
        }
    });
    let main = work[0].agent.clone();
    let other = agents
        .iter()
        .filter(|agent| agent.id != main && !agent.note.is_empty())
        .find(|agent| !agent.note.contains("no room"))
        .or_else(|| agents.iter().find(|agent| agent.id != main))
        .cloned();
    let other_work = other.as_ref().map(matched_work).unwrap_or_default();
    let mut setup = Setup {
        work,
        low: "switch".to_string(),
        other,
        other_work,
        rule: String::new(),
        never_use: Vec::new(),
        computers: None,
    };
    setup.rule = rule_for(&setup.low, &setup.main_label(), setup.other.as_ref(), &setup.other_work);
    setup
}

/// A saved setup (the user's default, or a normalized one handed to `save`), read onto this
/// computer's lineup; what it names that is gone here falls back to `base`.
fn setup_from_json(value: &Value, agents: &[Agent], base: &Setup) -> Setup {
    let mut setup = base.clone();
    for (index, (key, _, _, _)) in KINDS.iter().enumerate() {
        if let Some(pick) = Pick::from_json(&value["work"][key]) {
            setup.work[index] = match find_agent(agents, &pick.agent) {
                Some(agent) => {
                    let model = agent.model(&pick.model).or_else(|| agent.main_model());
                    Pick::of(agent, model, Some(&pick.effort).filter(|e| !e.is_empty()).map(String::as_str))
                }
                None => pick,
            };
        }
    }
    let low = &value["lowUsage"];
    if let Some(choice) = low["choice"].as_str() {
        setup.low = choice.to_string();
    }
    if let Some(other) = low["otherAgent"]
        .as_str()
        .and_then(|other| find_agent(agents, other))
    {
        setup.other = Some(other.clone());
        setup.other_work = matched_work(other);
    }
    setup.rule = match low["rule"].as_str() {
        Some(rule) if setup.low == "own" && !rule.trim().is_empty() => rule.trim().to_string(),
        _ => rule_for(&setup.low, &setup.main_label(), setup.other.as_ref(), &setup.other_work),
    };
    if let Some(never) = value["neverUse"].as_array() {
        setup.never_use = never
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    setup.computers = match &value["computers"] {
        Value::Array(list) => Some(
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    };
    setup
}

// ---------------------------------------------------------------------------------------------
// Where the setup lives.

/// The coordinator's "Setup:" notes.
fn setup_notes(view: &Value) -> Vec<String> {
    view["coordinator"]["memory"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|note| note["text"].as_str().or_else(|| note.as_str()))
        .filter(|text| text.trim_start().starts_with(SETUP_NOTE_PREFIX))
        .map(str::to_string)
        .collect()
}

/// The user's default, `None` before the first interview (or from a gxserver too old to keep it).
fn read_default(flags: &Flags) -> Option<Value> {
    call_gxserver_rpc("/api/readCoordinatorSetup", &json!({}), flags)
        .ok()
        .map(|answer| answer["default"].clone())
        .filter(Value::is_object)
}

/// A coordinator's own notes read back into a setup: each kind of work from the `start-thread`
/// flags its line names, the rule word for word, the accounts and computers as listed.
fn setup_from_notes(notes: &[String], agents: &[Agent], base: &Setup) -> Setup {
    let mut setup = base.clone();
    let flag = |text: &str, name: &str| -> Option<String> {
        let at = text.find(&format!("--{name} "))? + name.len() + 3;
        Some(
            text[at..]
                .split(|c: char| c.is_whitespace() || c == ')' || c == ',')
                .next()?
                .to_string(),
        )
    };
    for note in notes {
        let body = note.trim_start()[SETUP_NOTE_PREFIX.len()..].trim();
        let Some((head, rest)) = body.split_once(": ") else {
            continue;
        };
        if let Some(index) = KINDS
            .iter()
            .position(|(_, title, _, _)| head.eq_ignore_ascii_case(title))
        {
            if let Some(agent) = flag(rest, "agent").and_then(|id| find_agent(agents, &id)) {
                let model = flag(rest, "model")
                    .and_then(|model| agent.model(&model).cloned())
                    .or_else(|| agent.main_model().cloned());
                setup.work[index] =
                    Pick::of(agent, model.as_ref(), flag(rest, "effort").as_deref());
            }
        } else if head == "when usage runs low" {
            setup.low = "own".to_string();
            setup.rule = rest
                .split(" (start-thread ")
                .next()
                .unwrap_or(rest)
                .trim()
                .to_string();
            if let Some(agent) = flag(rest, "agent").and_then(|id| find_agent(agents, &id)) {
                setup.other = Some(agent.clone());
                setup.other_work = matched_work(agent);
            }
        } else if head == "accounts threads never use" {
            let list = rest.split(" (").next().unwrap_or(rest).trim();
            setup.never_use = if list == "none" {
                Vec::new()
            } else {
                list.split(", ").map(str::to_string).collect()
            };
        } else if head == "computers threads may run on" {
            setup.computers =
                (rest.trim() != "all").then(|| rest.split(", ").map(str::to_string).collect());
        }
    }
    setup
}

/// The setup in effect for a coordinator and where it came from: its own notes (read through the
/// user's default when they are that default's lines), else the default, else the playbook's.
fn effective_setup(view: &Value, flags: &Flags, agents: &[Agent]) -> (Setup, &'static str) {
    let base = playbook_setup(agents);
    let notes = setup_notes(view);
    let default = read_default(flags).map(|value| setup_from_json(&value, agents, &base));
    if !notes.is_empty() {
        if let Some(default) = default.filter(|default| default.notes() == notes) {
            return (default, "this coordinator");
        }
        return (setup_from_notes(&notes, agents, &base), "this coordinator");
    }
    match default {
        Some(default) => (default, "your default"),
        None => (base, "the playbook"),
    }
}

// ---------------------------------------------------------------------------------------------
// The page.

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An agent's logo (packages/core-ui/assets) and brand colour (the desktop's
/// settings_modal/tabs/agents/logos.rs `brand_color`; white brands take the page's ink).
fn logo(icon: &str) -> Option<(&'static str, &'static str)> {
    macro_rules! asset {
        ($name:literal) => {
            include_str!(concat!("../../../../packages/core-ui/assets/", $name, ".svg"))
        };
    }
    Some(match icon {
        "claude" => (asset!("claude"), "#d97757"),
        "claude-cloud" => (asset!("claude-cloud"), "#d67556"),
        "codex" => (asset!("codex"), ""),
        "opencode" => (asset!("opencode"), "#6d96c0"),
        "cursor-cli" | "cursor" => (asset!("cursor-cli"), ""),
        "grok-build" | "grok" => (asset!("grok-build"), ""),
        "hermes-agent" => (asset!("hermes-agent"), "#f3c46b"),
        "pi" => (asset!("pi"), "#c8ff62"),
        "omp" => (asset!("omp"), "#a663ed"),
        "antigravity-cli" | "antigravity" => (asset!("antigravity-cli"), "#749bff"),
        "dsh" => (asset!("dsh"), "#4d6bfe"),
        "gemini" => (asset!("gemini"), ""),
        "copilot" => (asset!("copilot"), ""),
        "kimi" => (asset!("kimi"), "#7b6cf6"),
        "factory-droid" => (asset!("factory-droid"), "#ff7a1a"),
        "kiro" => (asset!("kiro"), "#a6e3ff"),
        "qoder" => (asset!("qoder"), "#a991ff"),
        "amp-cli" | "amp" => (asset!("amp-cli"), ""),
        "devin" => (asset!("devin"), "#3ea6ff"),
        "empryo" => (asset!("empryo"), "#1fa31d"),
        "rovo-dev" => (asset!("rovo-dev"), "#4fc3a1"),
        "codebuddy" => (asset!("codebuddy"), "#72d6ff"),
        "command-code" => (asset!("command-code"), "#22d3ee"),
        "openclaude" => (asset!("openclaude"), "#f0a68a"),
        "mastra" => (asset!("mastra"), ""),
        "zcode" => (asset!("zcode"), ""),
        _ => return None,
    })
}

fn data_uri(svg: &str) -> String {
    let mut out = String::from("data:image/svg+xml,");
    for byte in svg.trim().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b' ' | b'=' | b':'
            | b'/' | b',' | b'\'' | b'(' | b')' | b';' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The CSS class of an agent's logo (`l-<n>`), its colour and the rule that draws it.
fn logo_class(agents: &[Agent]) -> (Vec<Option<String>>, String) {
    let mut css = String::new();
    let classes = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            let icon = if agent.icon.is_empty() { &agent.family } else { &agent.icon };
            let (svg, color) = logo(icon).or_else(|| logo(&agent.family))?;
            let class = format!("l-{index}");
            let color = if color.is_empty() {
                String::new()
            } else {
                format!("--c:{color};")
            };
            let uri = data_uri(svg);
            css.push_str(&format!(
                ".{class}{{{color}-webkit-mask-image:url(\"{uri}\");mask-image:url(\"{uri}\")}}\n"
            ));
            Some(class)
        })
        .collect();
    (classes, css)
}

/// One kind of work's card: its drawing and title, then the agent tiles, the model and the effort.
fn kind_card(
    (key, title, about, _): (&str, &str, &str, &str),
    drawing: &str,
    agents: &[Agent],
    classes: &[Option<String>],
    pick: &Pick,
) -> String {
    let agent_menu = format!("{key} agent");
    let mut agent_options = String::new();
    let mut model_options = String::new();
    let mut effort_options = String::new();
    for (agent, class) in agents.iter().zip(classes) {
        let picked_agent = agent.id == pick.agent;
        agent_options.push_str(&format!(
            "<option value=\"{name}\"{logo}{note}{selected}>{name}</option>",
            name = esc(&agent.name),
            logo = class
                .as_ref()
                .map(|class| format!(" data-logo=\"{class}\""))
                .unwrap_or_default(),
            note = if agent.note.is_empty() {
                String::new()
            } else {
                format!(" data-note=\"{}\"", esc(&agent.note))
            },
            selected = if picked_agent { " selected" } else { "" },
        ));
        let when = format!("{} agent={}", key, esc(&agent.name));
        let main = agent.main_model().map(|model| model.value.clone());
        if agent.models.is_empty() {
            model_options.push_str(&format!(
                "<option value=\"{NO_MODEL}\" data-when=\"{when}\">{NO_MODEL}</option>"
            ));
            effort_options.push_str(&format!(
                "<option value=\"{NO_EFFORT}\" data-when=\"{when}\">{NO_EFFORT}</option>"
            ));
            continue;
        }
        for model in &agent.models {
            let picked_model = picked_agent && model.value == pick.model;
            model_options.push_str(&format!(
                "<option value=\"{label}\" data-when=\"{when}\"{default}{selected}>{label}</option>",
                label = esc(&model.label),
                default = if Some(&model.value) == main.as_ref() { " data-default" } else { "" },
                selected = if picked_model { " selected" } else { "" },
            ));
            let model_when = format!("{when}; {key} model={}", esc(&model.label));
            if model.efforts.is_empty() {
                effort_options.push_str(&format!(
                    "<option value=\"{NO_EFFORT}\" data-when=\"{model_when}\">{NO_EFFORT}</option>"
                ));
            }
            for (value, label) in &model.efforts {
                effort_options.push_str(&format!(
                    "<option value=\"{label}\" data-when=\"{model_when}\"{default}{selected}>{label}</option>",
                    label = esc(label),
                    default = if model.default_effort.as_deref() == Some(value) { " data-default" } else { "" },
                    selected = if picked_model && *value == pick.effort { " selected" } else { "" },
                ));
            }
        }
    }
    format!(
        r#"<section class="kind">
      <div class="kind-top">{drawing}<div><h3>{title}</h3><p>{about}</p></div></div>
      <div class="gx-menu"><span class="gx-menu-label">Agent</span><select name="{agent_menu}" aria-label="{title}: agent" data-look="tiles">{agent_options}</select></div>
      <div class="gx-menu"><span class="gx-menu-label">Model</span><select name="{key} model" aria-label="{title}: model" data-look="seg">{model_options}</select></div>
      <div class="gx-menu"><span class="gx-menu-label">Effort</span><select name="{key} effort" aria-label="{title}: effort" data-look="seg">{effort_options}</select></div>
    </section>
"#
    )
}

const DRAWINGS: [&str; 3] = [
    r#"<svg viewBox="0 0 64 48" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="4" width="12" height="15" rx="2"/><rect x="26" y="4" width="12" height="15" rx="2" class="hot"/><rect x="49" y="4" width="12" height="15" rx="2"/><rect x="3" y="29" width="12" height="15" rx="2" class="hot"/><rect x="26" y="29" width="12" height="15" rx="2"/><rect x="49" y="29" width="12" height="15" rx="2" class="hot"/><path d="M15 11.5h11M38 11.5h11M32 19v10M15 36.5h11M38 36.5h11" stroke-dasharray="2 2" class="hot"/></svg>"#,
    r#"<svg viewBox="0 0 64 48" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="6" y="6" width="18" height="24" rx="2"/><rect x="14" y="14" width="18" height="24" rx="2"/><circle cx="44" cy="24" r="10" class="hot"/><circle cx="44" cy="24" r="3.5" class="hot"/><path d="M44 10v5M44 33v5M30 24h5M53 24h5" class="hot"/></svg>"#,
    r#"<svg viewBox="0 0 64 48" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M20 5h16l8 8v30H20z"/><path d="M36 5v8h8"/><path d="M26 27l5 5 9-10" class="hot" stroke-width="2"/></svg>"#,
];

/// The four low-usage timelines: the main agent's lane and the other agent's, on one axis whose
/// dashed line is the moment every main account is low.
fn lanes(main: &str, other: &str) -> [String; 4] {
    let lane = |name: &str, class: &str, track: &str| {
        format!(
            r#"<span class="who"><i class="gx-logo {class}-logo"></i><span class="{class}-name">{}</span></span><div class="track">{track}</div>"#,
            esc(name)
        )
    };
    let seg = |class: &str, left: u32, width: u32| {
        format!(r#"<i class="seg {class}" style="left:{left}%;width:{width}%"></i>"#)
    };
    let both = |main_track: String, other_track: String| {
        format!(
            r#"<div class="lanes">{}{}<div class="axis"><em>low</em></div></div>"#,
            lane(main, "main", &main_track),
            lane(other, "other", &other_track)
        )
    };
    [
        both(
            seg("m", 0, 62) + r#"<i class="seg wait" style="left:63%;width:37%"></i>"#,
            String::new(),
        ),
        both(seg("m", 0, 62), seg("o", 63, 37)),
        both(
            seg("m", 0, 18) + &seg("m", 30, 14) + &seg("m", 52, 10),
            seg("o", 18, 12) + &seg("o", 44, 8) + &seg("o", 63, 37),
        ),
        both(seg("m", 0, 62) + r#"<i class="ask">?</i>"#, String::new()),
    ]
}

struct PageData {
    agents: Vec<Agent>,
    room: Value,
    computers: Vec<(String, bool)>,
    setup: Setup,
    source: &'static str,
}

const PAGE_STYLE: &str = r#"
  gx-questions>header{margin-bottom:22px}
  h2{margin-top:34px}
  .kinds{display:flex;flex-direction:column;gap:14px;margin:14px 0 6px}
  .kind{min-width:0;padding:16px 16px 18px;border-radius:16px;background:var(--gx-surface);border:1px solid var(--gx-line)}
  .kind-top{display:flex;align-items:center;gap:14px}
  .kind-top svg{flex:none;width:56px;height:42px;color:var(--gx-muted)}
  .kind-top svg .hot{color:var(--gx-pick)}
  .kind h3{margin:0;font-size:15.5px}
  .kind p{margin:3px 0 0;font-size:12.5px;line-height:1.4;color:var(--gx-muted)}
  .muted{color:var(--gx-muted);font-size:13px}
  .how{margin:-2px 0 0;font-size:13px;line-height:1.45;color:var(--gx-muted)}
  .how b{color:var(--gx-fg);font-weight:600}
  .lanes{display:grid;grid-template-columns:max-content minmax(0,1fr);gap:8px;align-items:center;padding:11px 11px 8px;border-radius:12px;background:var(--gx-surface);border:1px solid var(--gx-line)}
  .who{display:flex;align-items:center;gap:6px;font-size:11px;font-weight:550;color:var(--gx-text);white-space:nowrap}
  .who .gx-logo{width:12px;height:12px}
  .track{position:relative;height:9px;border-radius:99px;background:var(--gx-line)}
  .track::after{content:"";position:absolute;left:62%;top:-5px;bottom:-5px;border-left:1px dashed var(--gx-muted)}
  .seg{position:absolute;top:0;bottom:0;border-radius:99px}
  .seg.m{background:var(--main-c,var(--gx-fg))}
  .seg.o{background:var(--other-c,var(--gx-muted))}
  .seg.wait{background:repeating-linear-gradient(90deg,var(--gx-muted) 0 2px,transparent 2px 5px);opacity:.7}
  .axis{grid-column:2;display:flex;font-size:9.5px;color:var(--gx-muted)}
  .axis em{font-style:normal;margin-left:58%}
  .ask{position:absolute;left:calc(62% - 8px);top:-4px;width:16px;height:16px;border-radius:50%;display:grid;place-items:center;font-size:10px;font-weight:700;font-style:normal;color:var(--gx-fg);background:var(--gx-raised);border:1px solid var(--gx-line-strong)}
  .acct{display:flex;flex-wrap:wrap;align-items:center;gap:4px 10px;color:var(--gx-muted);font-size:12.5px;overflow-wrap:anywhere}
  .acct .tag{flex:none;white-space:nowrap;font-size:11px;padding:1px 7px;border-radius:99px;border:1px solid var(--gx-line-strong)}
  .never .gx-row[aria-checked=true] .gx-row-label{text-decoration:line-through;text-decoration-color:var(--gx-muted)}
  .never .gx-row[aria-checked=true] .gx-row-label::after{content:"Won't use";display:inline-block;margin-left:8px;padding:1px 8px;border-radius:99px;font-size:11px;font-weight:600;text-decoration:none;vertical-align:1px;color:var(--gx-fg);border:1px solid var(--gx-pick)}
  .pcs{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,180px),1fr));gap:8px}
  .pc{min-width:0;padding:10px 12px;border-radius:12px;background:var(--gx-surface);border:1px solid var(--gx-line)}
  .pc b{display:block;color:var(--gx-fg);font-size:13.5px}
  .pc small{color:var(--gx-muted);font-size:11.5px}
  details.change{margin-top:8px}
  details.change>summary{cursor:pointer;font-size:13px;color:var(--gx-muted)}
"#;

/// Keeps the low-usage timelines on the agents picked above: the main agent is the large-work
/// pick, the other the fallback pick.
const PAGE_SCRIPT: &str = r#"(function(){
  var A=JSON.parse(document.getElementById("setup-agents").textContent||"{}");
  function val(n){var s=document.querySelector('select[name="'+n+'"]');return s?s.value:"";}
  function put(cls,a){
    Array.prototype.forEach.call(document.querySelectorAll("."+cls+"-name"),function(e){e.textContent=a.short||"";});
    Array.prototype.forEach.call(document.querySelectorAll("."+cls+"-logo"),function(e){e.className="gx-logo "+cls+"-logo "+(a.logo||"");});
    document.documentElement.style.setProperty("--"+cls+"-c",a.color||"var(--gx-fg)");
  }
  function update(){put("main",A[val("large agent")]||{});put("other",A[val("other agent")]||{});}
  ["click","change","keyup"].forEach(function(t){document.addEventListener(t,function(){setTimeout(update,0);},true);});
  update();setTimeout(update,300);setTimeout(update,1200);
})();"#;

fn page_html(data: &PageData) -> String {
    let PageData {
        agents,
        room,
        computers,
        setup,
        source,
    } = data;
    let (classes, logo_css) = logo_class(agents);
    let agent_json: serde_json::Map<String, Value> = agents
        .iter()
        .zip(&classes)
        .map(|(agent, class)| {
            let icon = if agent.icon.is_empty() { &agent.family } else { &agent.icon };
            let color = logo(icon)
                .or_else(|| logo(&agent.family))
                .map(|(_, color)| color)
                .filter(|color| !color.is_empty());
            (
                agent.name.clone(),
                json!({ "short": agent.short(), "logo": class, "color": color }),
            )
        })
        .collect();
    let mut cards = String::new();
    for (index, kind) in KINDS.iter().enumerate() {
        cards.push_str(&kind_card(*kind, DRAWINGS[index], agents, &classes, &setup.work[index]));
    }
    let lede = match *source {
        "this coordinator" => "Filled in from this coordinator's setup. Change anything, or just send it.",
        "your default" => "Filled in from your saved setup. Change anything, or just send it.",
        _ => "Filled in with the recommended setup. Change anything, or just send it.",
    };
    let main = setup.main_label();
    let other = setup
        .other
        .as_ref()
        .map(Agent::short)
        .unwrap_or_else(|| "Another agent".to_string());
    let [lane_stay, lane_switch, lane_share, lane_ask] = lanes(&main, &other);
    let picked = |value: &str| {
        let on = match setup.low.as_str() {
            "stay" => value == "Stay",
            "switch" => value == "Switch",
            "share" => value == "Share",
            "ask" => value == "Ask me",
            _ => false,
        };
        if on { " picked" } else { "" }
    };
    let own_rule = if setup.low == "own" && !setup.rule.is_empty() {
        format!(
            "\n    <gx-choice value=\"{rule}\" picked>Your own rule<small>{rule}</small></gx-choice>",
            rule = esc(&setup.rule)
        )
    } else {
        String::new()
    };
    let mut other_options = String::new();
    for (agent, class) in agents.iter().zip(&classes) {
        let matched = matched_work(agent);
        let note = match agent.main_model() {
            Some(model) => {
                let efforts: Vec<String> = matched
                    .iter()
                    .map(|pick| pick.effort_label.clone())
                    .filter(|label| !label.is_empty())
                    .collect();
                if efforts.is_empty() {
                    model.label.clone()
                } else {
                    format!("{} · {}", model.label, efforts.join(", "))
                }
            }
            None => NO_MODEL.to_string(),
        };
        other_options.push_str(&format!(
            "<option value=\"{name}\"{logo} data-note=\"{note}\"{selected}>{name}</option>",
            name = esc(&agent.name),
            logo = class
                .as_ref()
                .map(|class| format!(" data-logo=\"{class}\""))
                .unwrap_or_default(),
            note = esc(&note),
            selected = if setup.other.as_ref().map(|o| &o.id) == Some(&agent.id) {
                " selected"
            } else {
                ""
            },
        ));
    }
    let never = never_use_section(room, &setup.never_use);
    let computers_part = computers_section(computers, setup.computers.as_deref());
    let signed_in = agents.iter().filter(|agent| !agent.note.is_empty()).count();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="ghostex-page" content="v2">
<title>Coordinator setup: which agents do which work</title>
<style>{PAGE_STYLE}{logo_css}</style>
</head>
<body>
<gx-questions submit="Send to the coordinator">
  <header>
    <span class="gx-eyebrow">Ghostex coordinator · setup</span>
    <h1>Coordinator setup: which agents do which work</h1>
    <p class="gx-lede">{lede}</p>
    <div class="gx-chips">
      <span class="gx-chip"><b>{agent_count}</b> agents</span>
      <span class="gx-chip"><b>{signed_in}</b> with usage readings</span>
      <span class="gx-chip"><b>{computer_count}</b> {computer_word}</span>
      <span class="gx-chip">Saved as your default</span>
    </div>
  </header>

  <div class="gx-callout">
    <b>Set up another coordinator already?</b> Skip this page: type <code>+</code> in my chat box, pick that coordinator, and ask me to copy its setup.
  </div>

  <h2>Who does which work</h2>
  <gx-question type="menus" name="Who does which work" title="Which agent, model and effort for each kind of work?">
    <div class="kinds">
{cards}    </div>
  </gx-question>

  <h2>When usage runs low</h2>
  <p class="muted">Ghostex already moves a thread to another of your accounts of the same agent when one is full. This is what I do once every <span class="main-name">{main_html}</span> account is low (the dashed line).</p>
  <gx-wide>
    <div class="gx-options" style="--gx-option-min:250px">
      <section data-question="Low usage" data-choice="Stay">
        <h3>A. Stay on <span class="main-name">{main_html}</span></h3>
        <p class="how">Threads <b>wait for the reset</b>; I never switch agent.</p>
        {lane_stay}
      </section>
      <section data-question="Low usage" data-choice="Switch">
        <h3>B. Switch when low</h3>
        <p class="how">All <span class="main-name">{main_html}</span> until it is low, then <b>every new thread</b> on <span class="other-name">{other_html}</span>.</p>
        {lane_switch}
      </section>
      <section data-question="Low usage" data-choice="Share">
        <h3>C. Share, then switch</h3>
        <p class="how">New threads <b>split between both</b>; all on <span class="other-name">{other_html}</span> once <span class="main-name">{main_html}</span> is low.</p>
        {lane_share}
      </section>
      <section data-question="Low usage" data-choice="Ask me">
        <h3>D. Ask me each time</h3>
        <p class="how">I stop and ask in your Inbox <b>before</b> <span class="main-name">{main_html}</span> runs out.</p>
        {lane_ask}
      </section>
    </div>
  </gx-wide>
  <gx-question name="Low usage" title="When your main agent runs low, what should I do?">
    <gx-choice value="Stay"{p_stay}>A. Stay on <span class="main-name">{main_html}</span></gx-choice>
    <gx-choice value="Switch"{p_switch}>B. Switch when low</gx-choice>
    <gx-choice value="Share"{p_share}>C. Share, then switch</gx-choice>
    <gx-choice value="Ask me"{p_ask}>D. Ask me each time</gx-choice>{own_rule}
  </gx-question>
  <gx-question type="menus" name="Other agent" title="For B and C, which agent takes over?">
    <p class="muted">Its models are matched by kind of work: its main model at High for large work, Medium for hard and Low for simple.</p>
    <div class="gx-menu"><select name="other agent" aria-label="The agent that takes over" data-look="tiles">{other_options}</select></div>
  </gx-question>
{never}{computers_part}
<script type="application/json" id="setup-agents">{agent_json}</script>
<script>{PAGE_SCRIPT}</script>
</gx-questions>
</body>
</html>
"#,
        agent_count = agents.len(),
        computer_count = computers.len().max(1),
        computer_word = if computers.len() > 1 { "computers" } else { "computer" },
        main_html = esc(&main),
        other_html = esc(&other),
        p_stay = picked("Stay"),
        p_switch = picked("Switch"),
        p_share = picked("Share"),
        p_ask = picked("Ask me"),
        agent_json = Value::Object(agent_json).to_string().replace("</", "<\\/"),
    )
}

/// Accounts threads never use: only for an agent with more than one account.
fn never_use_section(room: &Value, never_use: &[String]) -> String {
    let mut groups = String::new();
    for agent in room["agents"].as_array().into_iter().flatten() {
        let details = agent["accountDetails"].as_array().cloned().unwrap_or_default();
        if details.len() < 2 {
            continue;
        }
        let label = agents::text(agent, "label");
        let label = label.split(" (").next().unwrap_or(label);
        let mut rows = String::new();
        for detail in &details {
            let name = format!("{label} {}", agents::text(detail, "account"));
            let mut tags = String::new();
            if detail["default"] == true {
                tags.push_str(r#" <span class="tag">default</span>"#);
            }
            if detail["manualOnly"] == true {
                tags.push_str(r#" <span class="tag">only when I pick it</span>"#);
            }
            let used = detail["windows"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|window| window["label"] != "5h")
                .filter_map(|window| {
                    Some(format!(
                        "{}% of {}",
                        window["usedPercent"].as_f64()?.round() as i64,
                        window["label"].as_str().unwrap_or("its window")
                    ))
                })
                .next()
                .map(|text| format!(" · {text}"))
                .unwrap_or_default();
            rows.push_str(&format!(
                "\n      <article data-pick=\"{name}\"{picked}><h4>{name}</h4><div class=\"acct\"><span>{email}{used}</span>{tags}</div></article>",
                name = esc(&name),
                picked = if never_use.contains(&name) { " data-picked" } else { "" },
                email = esc(agents::text(detail, "email")),
                used = esc(&used),
            ));
        }
        groups.push_str(&format!(
            "\n    <div data-pick-group>\n      <h3>{}</h3>{rows}\n    </div>",
            esc(label)
        ));
    }
    if groups.is_empty() {
        return String::new();
    }
    format!(
        r#"
  <h2>Accounts threads never use</h2>
  <div class="never" data-question="Never use">{groups}
  </div>
  <gx-question name="Never use" multiple title="Which accounts should threads never use?">
    <p>Picked ones need "Only when I pick it" in Settings &gt; Accounts. Only you can set that, so I ask you to.</p>
  </gx-question>
"#
    )
}

/// The computers threads may run on, all unless the user changes it.
fn computers_section(computers: &[(String, bool)], picked: Option<&[String]>) -> String {
    if computers.len() < 2 {
        return String::new();
    }
    let cards: String = computers
        .iter()
        .map(|(name, local)| {
            format!(
                "\n    <div class=\"pc\"><b>{}</b><small>{}</small></div>",
                esc(name),
                if *local { "This computer" } else { "Connected" }
            )
        })
        .collect();
    let rows: String = computers
        .iter()
        .map(|(name, _)| {
            let on = picked.is_none_or(|list| list.contains(name));
            format!(
                "\n      <article data-pick=\"{name}\"{picked}><h4>{name}</h4></article>",
                name = esc(name),
                picked = if on { " data-picked" } else { "" }
            )
        })
        .collect();
    format!(
        r#"
  <h2>Computers</h2>
  <div class="pcs">{cards}
  </div>
  <p class="muted">Threads may run on all of them unless you change it; I spread work by what each has free.</p>
  <details class="change">
    <summary>Change computers</summary>
    <div data-question="Computers">{rows}
    </div>
    <gx-question name="Computers" multiple title="Which computers may threads run on?"></gx-question>
  </details>
"#
    )
}

/// This computer's name and the connected ones agents may use.
fn computer_names() -> Vec<(String, bool)> {
    let mut names = vec![(crate::connected_computers::this_computer_name(), true)];
    if let Ok(local) = crate::ghostex_cli::rpc::resolve_local_gxserver_target() {
        for computer in crate::ghostex_cli::computers::connected_computers(&local) {
            if computer.agent_access && computer.waiting.is_none() && !computer.name.is_empty() {
                names.push((computer.name, false));
            }
        }
    }
    names
}

fn setup_dir() -> PathBuf {
    gxserver_root().join("coordinator-setup")
}

/// Everything the page and `save` read: the coordinator, its lineup and usage.
fn context(parsed: &ParsedArgs) -> CliResult<(Value, Flags, Value, Vec<Agent>, Value)> {
    let (session, flags) = target_coordinator(parsed)?;
    let view = read_view(&session, &flags)?;
    let room = usage_room::read(&view, &flags);
    let options = options_view(&flags)?;
    let agents = lineup(&options, &room);
    if agents.is_empty() {
        return Err(CliError::Other(
            "No agent here can run a coordinator's threads. Turn one on in Settings > Agents first."
                .into(),
        ));
    }
    Ok((session, flags, view, agents, room))
}

/// `page [--out <file>] [--setup-file <json>]`: writes the interview and prints its path.
/// `--setup-file` prefills it from that setup instead (to preview one).
fn page(parsed: &ParsedArgs) -> CliResult<()> {
    let (session, flags, view, agents, room) = context(parsed)?;
    let (setup, source) = match flag_text(&parsed.flags, "setupFile") {
        Some(path) => {
            let text = crate::ghostex_cli::args::read_body_file(&path, "setup")
                .map_err(CliError::Other)?;
            let value: Value = serde_json::from_str(&text)
                .map_err(|error| CliError::Other(format!("The setup is not JSON: {error}")))?;
            (
                setup_from_json(&value, &agents, &playbook_setup(&agents)),
                "your default",
            )
        }
        None => effective_setup(&view, &flags, &agents),
    };
    let html = page_html(&PageData {
        agents,
        room,
        computers: computer_names(),
        setup,
        source,
    });
    let path = match flag_text(&parsed.flags, "out") {
        Some(out) => PathBuf::from(out),
        None => setup_dir().join(format!(
            "setup-{}.html",
            agents::text(&session, "sessionId")
        )),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::Other(format!("Could not make {}: {error}", parent.display())))?;
    }
    std::fs::write(&path, html)
        .map_err(|error| CliError::Other(format!("Could not write {}: {error}", path.display())))?;
    println!("{}", path.display());
    Ok(())
}

/// The answers JSON out of what the coordinator got: the bare JSON, or the whole
/// `<ghostex-page-answers>` message.
fn answers_json(text: &str) -> CliResult<Value> {
    let inner = match text.find("<ghostex-page-answers") {
        Some(start) => {
            let after = &text[start..];
            let body = after.find('>').map(|at| &after[at + 1..]).unwrap_or(after);
            body.split("</ghostex-page-answers>").next().unwrap_or(body)
        }
        None => text,
    };
    serde_json::from_str(inner.trim()).map_err(|error| {
        CliError::Other(format!(
            "--body-file needs the page's answers as JSON (or the normalized setup from setup show --json): {error}"
        ))
    })
}

fn answered<'a>(answers: &'a Value, key: &str) -> Option<&'a Value> {
    answers
        .get(key)
        .filter(|value| !value.is_null() && value.as_str() != Some(KIT_SKIPPED))
}

/// The page's answers read onto `base` (what the page was prefilled with): a skipped question
/// keeps `base`.
fn setup_from_answers(answers: &Value, agents: &[Agent], base: &Setup, all: &[String]) -> Setup {
    let mut setup = base.clone();
    if let Some(menus) = answered(answers, "Who does which work") {
        for (index, (key, _, _, _)) in KINDS.iter().enumerate() {
            let text = |what: &str| menus[format!("{key} {what}")].as_str().unwrap_or_default();
            let Some(agent) = find_agent(agents, text("agent")) else {
                continue;
            };
            let model = agent.model(text("model")).or_else(|| {
                (text("model") != NO_MODEL).then(|| agent.main_model()).flatten()
            });
            let effort = Some(text("effort")).filter(|e| !e.is_empty() && *e != NO_EFFORT);
            setup.work[index] = Pick::of(agent, model, effort);
        }
    }
    if let Some(other) = answered(answers, "Other agent")
        .and_then(|value| value["other agent"].as_str().or_else(|| value.as_str()))
        .and_then(|name| find_agent(agents, name))
    {
        setup.other = Some(other.clone());
        setup.other_work = matched_work(other);
    }
    let note = answers["Low usage (note)"]
        .as_str()
        .map(str::trim)
        .unwrap_or_default();
    let choice = answers["Low usage"].as_str().unwrap_or_default();
    let low = match choice {
        "Stay" => Some("stay"),
        "Switch" => Some("switch"),
        "Share" => Some("share"),
        "Ask me" => Some("ask"),
        KIT_SKIPPED | "" if note.is_empty() => None,
        _ => Some("own"),
    };
    if let Some(low) = low {
        setup.low = low.to_string();
        setup.rule = if low == "own" {
            if !note.is_empty() && (choice.is_empty() || choice == KIT_OTHER || choice == KIT_SKIPPED) {
                note.to_string()
            } else {
                choice.to_string()
            }
        } else {
            let mut rule = rule_for(low, &setup.main_label(), setup.other.as_ref(), &setup.other_work);
            if !note.is_empty() {
                rule.push_str(&format!(" The user added: {note}"));
            }
            rule
        };
    } else if setup.low != "own" {
        setup.rule = rule_for(&setup.low, &setup.main_label(), setup.other.as_ref(), &setup.other_work);
    }
    match answers.get("Never use") {
        Some(Value::Array(list)) => {
            setup.never_use = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
        }
        Some(Value::String(skipped)) if skipped == KIT_SKIPPED => setup.never_use.clear(),
        _ => {}
    }
    if let Some(Value::Array(list)) = answers.get("Computers") {
        let picked: Vec<String> = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
        setup.computers = (!all.iter().all(|name| picked.contains(name))).then_some(picked);
    }
    setup
}

/// `save --body-file <file|->`: the page's answers (or a normalized setup) become the user's
/// default and this coordinator's "Setup:" notes.
fn save(parsed: &ParsedArgs) -> CliResult<()> {
    let text = text_or_file(parsed, "answers", "nothing was saved")?.ok_or_else(|| {
        CliError::Other(
            "Usage: ghostex coordinator setup save --body-file <file|-> (the page's answers JSON)"
                .into(),
        )
    })?;
    let answers = answers_json(&text)?;
    let (session, flags, view, agents, _room) = context(parsed)?;
    let (base, _) = effective_setup(&view, &flags, &agents);
    let setup = if answers.get("work").is_some() && answers.get("version").is_some() {
        setup_from_json(&answers, &agents, &base)
    } else {
        let all: Vec<String> = computer_names().into_iter().map(|(name, _)| name).collect();
        setup_from_answers(&answers, &agents, &base, &all)
    };
    let notes = setup.notes();
    let mut memory: Vec<String> = view["coordinator"]["memory"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|note| note["text"].as_str().or_else(|| note.as_str()))
        .filter(|text| !text.trim_start().starts_with(SETUP_NOTE_PREFIX))
        .map(str::to_string)
        .collect();
    memory.extend(notes.iter().cloned());
    call_gxserver_rpc(
        "/api/updateCoordinator",
        &json!({
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "memory": memory,
        }),
        &flags,
    )?;
    let saved = setup.to_json();
    let default_saved = call_gxserver_rpc(
        "/api/setCoordinatorSetup",
        &json!({ "default": saved }),
        &flags,
    );
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "setup": saved,
            "notes": notes,
            "defaultSaved": default_saved.is_ok(),
        }));
        return Ok(());
    }
    println!("Saved this coordinator's setup:");
    for note in &notes {
        println!("  {note}");
    }
    match default_saved {
        Ok(_) => println!("Also saved as the user's default for new coordinators."),
        Err(error) => println!(
            "Not saved as the user's default ({error}). Update Ghostex on the computer that runs this coordinator."
        ),
    }
    if !setup.never_use.is_empty() {
        println!(
            "Ask the user to set {} to \"Only when I pick it\" in Settings > Accounts: agents can't change that setting.",
            setup.never_use.join(", ")
        );
    }
    Ok(())
}

/// `copy <coordinator ref>`: another coordinator's "Setup:" notes become this one's.
fn copy(parsed: &ParsedArgs) -> CliResult<()> {
    let Some(reference) = parsed.rest.first() else {
        return Err(CliError::Other(
            "Usage: ghostex coordinator setup copy <coordinator ref>".into(),
        ));
    };
    let (session, flags) = target_coordinator(parsed)?;
    let (source, source_flags) = resolve_session(reference, &flags)?;
    let source_view = read_view(&source, &source_flags)?;
    let notes = setup_notes(&source_view);
    if notes.is_empty() {
        return Err(CliError::Other(format!(
            "{} has no setup to copy. Run the interview instead: ghostex coordinator setup page.",
            agents::text(&source, "title")
        )));
    }
    let view = read_view(&session, &flags)?;
    let mut memory: Vec<String> = view["coordinator"]["memory"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|note| note["text"].as_str().or_else(|| note.as_str()))
        .filter(|text| !text.trim_start().starts_with(SETUP_NOTE_PREFIX))
        .map(str::to_string)
        .collect();
    memory.extend(notes.iter().cloned());
    call_gxserver_rpc(
        "/api/updateCoordinator",
        &json!({
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "memory": memory,
        }),
        &flags,
    )?;
    println!(
        "Copied the setup of {} to this coordinator:",
        agents::text(&source, "title")
    );
    for note in &notes {
        println!("  {note}");
    }
    println!("The user's default is unchanged; setup save changes it.");
    Ok(())
}

/// `show [--json]`: the setup in effect for this coordinator and where it came from.
fn show(parsed: &ParsedArgs) -> CliResult<()> {
    let (_session, flags, view, agents, _room) = context(parsed)?;
    let (setup, source) = effective_setup(&view, &flags, &agents);
    let notes = setup_notes(&view);
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "source": source,
            "done": !notes.is_empty(),
            "setup": setup.to_json(),
            "notes": if notes.is_empty() { setup.notes() } else { notes },
        }));
        return Ok(());
    }
    if notes.is_empty() {
        println!(
            "This coordinator has no setup yet; {source} would give it the lines below. Run ghostex coordinator setup page and show it to the user."
        );
        for note in setup.notes() {
            println!("  {note}");
        }
    } else {
        for note in notes {
            println!("{note}");
        }
    }
    Ok(())
}

/// `explain [--json] [--browser]`: shows the coordinator flow page Ghostex ships in this chat,
/// the way `ghostex show` does.
fn explain(args: &[String]) -> CliResult<()> {
    let path = setup_dir().join("coordinator-flow.html");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::Other(format!("Could not make {}: {error}", parent.display())))?;
    }
    if std::fs::read_to_string(&path).ok().as_deref() != Some(COORDINATOR_FLOW_PAGE_HTML) {
        std::fs::write(&path, COORDINATOR_FLOW_PAGE_HTML).map_err(|error| {
            CliError::Other(format!("Could not write {}: {error}", path.display()))
        })?;
    }
    let mut show_args = vec![path.to_string_lossy().into_owned(), "--no-check".to_string()];
    show_args.extend(args.iter().cloned());
    crate::ghostex_cli::show::show_command(&show_args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(value: &str, label: &str) -> Model {
        Model {
            value: value.into(),
            label: label.into(),
            efforts: vec![
                ("low".into(), "Low".into()),
                ("medium".into(), "Medium".into()),
                ("high".into(), "High".into()),
            ],
            default_effort: Some("high".into()),
        }
    }

    #[test]
    fn only_current_models_are_offered() {
        let models = [
            ("claude-fable-5-2", "Fable 5.2"),
            ("opus[1m]", "Opus 5.5"),
            ("haiku", "Haiku 5.5"),
            ("gpt-6.2-sol", "GPT 6.2 Sol"),
            ("gpt-6.2-astra", "GPT 6.2 Astra"),
            ("gpt-6.1-terra", "GPT 6.1 Terra"),
            ("gemini-4-neon", "Gemini 4 Neon"),
            ("gemini-3.8-flash", "Gemini 3.8 Flash"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-4.6-opus", "Opus 4.6"),
            ("composer-2.5", "Composer 2.5"),
            ("gpt-5.4-mini", "GPT-5.4 Mini"),
            ("openrouter/x/y-9", "Y 9"),
            ("auto", "Auto"),
        ]
        .map(|(value, label)| model(value, label));
        let kept: Vec<String> = current_models(&models)
            .into_iter()
            .map(|model| model.label)
            .collect();
        assert_eq!(
            kept,
            [
                "Fable 5.2",
                "Opus 5.5",
                "GPT 6.2 Sol",
                "GPT 6.2 Astra",
                "Gemini 4 Neon",
                "Opus 5.5"
            ]
        );
    }

    #[test]
    fn efforts_fall_to_the_nearest_level() {
        let mut m = model("gemini-4-neon", "Gemini 4 Neon");
        assert_eq!(nearest_effort(&m, "low").unwrap().1, "Low");
        assert_eq!(nearest_effort(&m, "xhigh").unwrap().1, "High");
        m.efforts.clear();
        assert!(nearest_effort(&m, "high").is_none());
    }
}
