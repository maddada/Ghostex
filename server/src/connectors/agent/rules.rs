//! What a workspace does when an agent's connector call would change something: ask with
//! Ghostex's card (the default), allow it, or never run it, for every such call or one tool at a
//! time. Reads never ask. Kept in `<gxserver state>/connectors/rules/<workspace>.json`.
//!
//! CDXC:Connectors 2026-10-16 DECISION:
//! User (Q7): before an agent sends, posts or deletes through a connector, Ghostex asks with its own card in that session, and each workspace can set it to Ask, Allow or Never. "Always allow" on a card allows that one tool in that workspace only.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

use super::super::store::file_stem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Rule {
    Ask,
    Allow,
    Never,
}

impl Rule {
    pub(crate) fn parse(text: &str) -> Option<Rule> {
        match text.trim().to_ascii_lowercase().as_str() {
            "ask" => Some(Rule::Ask),
            "allow" => Some(Rule::Allow),
            "never" => Some(Rule::Never),
            _ => None,
        }
    }
}

/// A workspace's rules: one for every call that changes something, and one per tool.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct WorkspaceRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default: Option<Rule>,
    /// Keyed `composio:<TOOL_SLUG>` or `executor:<app>.<tool>`.
    #[serde(default)]
    pub(crate) tools: BTreeMap<String, Rule>,
}

fn rules_path(paths: &GxserverPaths, workspace: &str) -> PathBuf {
    paths
        .root_dir
        .join("connectors")
        .join("rules")
        .join(format!("{}.json", file_stem(workspace)))
}

/// Marks that the old built-in Executor's rules were dropped, in `<gxserver state>/connectors`.
const OLD_EXECUTOR_RULES_DROPPED: &str = "executor-old-rules-dropped";

/// Drops every workspace's `executor:` tool rules once, the first time this build reads rules:
/// they were the old built-in Executor's.
///
/// CDXC:Connectors 2026-11-27 WHY:
/// Rule keys name an app by its address (`executor:linear.save_issue`), so an Always allow given to the old built-in Executor's personal Linear would have let the CLI sign-in's Linear, perhaps a company's, run with no card (security review of 769647fe4e, M4). They go at the switch, before any rule is read; Remove and the 30-day clean-up of the old data drop none, since by then every `executor:` rule is one set for the CLI's apps.
fn drop_old_executor_rules_once(paths: &GxserverPaths) {
    let flag = paths
        .root_dir
        .join("connectors")
        .join(OLD_EXECUTOR_RULES_DROPPED);
    if flag.exists() {
        return;
    }
    let dir = paths.root_dir.join("connectors").join("rules");
    for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(mut rules) = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<WorkspaceRules>(&text).ok())
        else {
            continue;
        };
        let before = rules.tools.len();
        rules.tools.retain(|key, _| !key.starts_with("executor:"));
        if rules.tools.len() != before {
            let temp = path.with_extension("json.tmp");
            let written = serde_json::to_vec_pretty(&rules)
                .ok()
                .is_some_and(|bytes| fs::write(&temp, bytes).is_ok());
            if !written || fs::rename(&temp, &path).is_err() {
                // Tried again at the next read.
                return;
            }
        }
    }
    if let Some(parent) = flag.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&flag, b"");
}

pub(crate) fn load(paths: &GxserverPaths, workspace: &str) -> WorkspaceRules {
    drop_old_executor_rules_once(paths);
    fs::read_to_string(rules_path(paths, workspace))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(paths: &GxserverPaths, workspace: &str, rules: &WorkspaceRules) -> Result<(), String> {
    let path = rules_path(paths, workspace);
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(rules)?)?;
        fs::rename(&temp, &path)
    };
    write().map_err(|error| format!("Couldn't save the Connectors rules: {error}"))
}

/// A tool as a rule names it. A bare name is read as Composio's (`GMAIL_SEND_EMAIL`) or, with a
/// dot, Executor's (`linear.save_issue`).
pub(crate) fn rule_key(tool: &str) -> String {
    let tool = tool.trim();
    if tool.starts_with("composio:") || tool.starts_with("executor:") {
        tool.to_string()
    } else if tool.contains('.') {
        format!("executor:{tool}")
    } else {
        format!("composio:{}", tool.to_uppercase())
    }
}

/// Sets the rule for one tool (`Some(key)`) or for every call (`None`); `None` as the rule goes
/// back to the default (a tool follows the workspace's rule, the workspace asks).
pub(crate) fn set(
    paths: &GxserverPaths,
    workspace: &str,
    key: Option<&str>,
    rule: Option<Rule>,
) -> Result<WorkspaceRules, String> {
    let mut rules = load(paths, workspace);
    match (key, rule) {
        (Some(key), Some(rule)) => {
            rules.tools.insert(rule_key(key), rule);
        }
        (Some(key), None) => {
            rules.tools.remove(&rule_key(key));
        }
        (None, rule) => rules.default = rule,
    }
    save(paths, workspace, &rules)?;
    Ok(rules)
}

/// What a call that would change something does: never when any of its tools says never,
/// allowed when all of them are allowed, else ask.
pub(crate) fn decide(paths: &GxserverPaths, workspace: &str, keys: &[String]) -> Rule {
    let rules = load(paths, workspace);
    let rule_of = |key: &String| {
        rules
            .tools
            .get(key)
            .copied()
            .or(rules.default)
            .unwrap_or(Rule::Ask)
    };
    if keys.iter().any(|key| rule_of(key) == Rule::Never) {
        Rule::Never
    } else if !keys.is_empty() && keys.iter().all(|key| rule_of(key) == Rule::Allow) {
        Rule::Allow
    } else {
        Rule::Ask
    }
}

/// What an Executor program does: never under a Never default; else as [`decide`] for the tools
/// it calls (`keys`) and, for each app it may use any tool of (`apps`), the default and every
/// rule for that app's tools. A program that reaches no app runs.
pub(crate) fn decide_program(
    paths: &GxserverPaths,
    workspace: &str,
    keys: &[String],
    apps: &[String],
) -> Rule {
    let rules = load(paths, workspace);
    let default = rules.default.unwrap_or(Rule::Ask);
    if default == Rule::Never {
        return Rule::Never;
    }
    let mut found: Vec<Rule> = keys
        .iter()
        .map(|key| rules.tools.get(key).copied().unwrap_or(default))
        .collect();
    for app in apps {
        let prefix = format!("executor:{app}.");
        found.push(default);
        found.extend(
            rules
                .tools
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, rule)| *rule),
        );
    }
    if found.contains(&Rule::Never) {
        Rule::Never
    } else if found.iter().all(|rule| *rule == Rule::Allow) {
        Rule::Allow
    } else {
        Rule::Ask
    }
}
