//! Reads an agent CLI's own `/model` picker in a hidden terminal, for the CLIs whose list command
//! is missing or does not match what the picker shows (Claude Code, Cursor, Antigravity).
//!
//! CDXC:AgentProviders 2026-11-13 DECISION:
//! User (Q2981, Q2983, Q2985, Q2986 on the live model lists page): Claude Code's model list is read
//! from its `/model` picker; Cursor's rows and order come from its picker and its efforts and Fast
//! from `cursor-agent --list-models`; every installed, signed-in CLI is read; Antigravity lists Gemini
//! 4 Carbon, which its picker shows and `agy models` leaves out. A picker only changes with a new CLI
//! version (or a login), so it is read once per version and kept in gxserver's cache folder; a CLI
//! that is missing, signed out or does not reach its picker reads nothing, and the published catalog
//! (or the list command alone) stays in effect. The read starts the CLI in a scratch folder of
//! gxserver's own, opens `/model`, reads the rows and leaves with Escape, so nothing is selected or
//! saved; Claude runs with its hooks off so the read never reaches Ghostex as a session.
//! SEE-ALSO: server/src/agent_model_lineups.rs (the lineups these rows order and complete),
//! docs/important/agent-model-catalog.md (how the same pickers are read by hand).

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde_json::{json, Value};

use crate::accounts::setup_terminal::LoginTerminal;

const ROWS: u16 = 60;
const COLS: u16 = 160;
/// The longest a whole read may take, start to Escape.
const READ_LIMIT: Duration = Duration::from_secs(90);
const DOWN: &[u8] = b"\x1b[B";
const UP: &[u8] = b"\x1b[A";
pub(crate) const ENTER: &[u8] = b"\r";
pub(crate) const ESCAPE: &[u8] = b"\x1b";

/// One row of a picker, in the picker's order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PickerRow {
    /// The row's text as the picker draws it ("Opus 4.8", "Claude Opus 5.5").
    pub label: String,
    pub description: Option<String>,
    /// Listed under the picker's own "More models" (Claude Code).
    pub more: bool,
    /// `Some(false)` when the picker says the model takes no effort.
    pub effort: Option<bool>,
    /// The picker's default row points at this model.
    pub default: bool,
}

impl PickerRow {
    fn named(label: &str) -> Self {
        Self {
            label: label.to_string(),
            description: None,
            more: false,
            effort: None,
            default: false,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "label": self.label,
            "description": self.description,
            "more": self.more,
            "effort": self.effort,
            "default": self.default,
        })
    }

    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            label: value.get("label")?.as_str()?.to_string(),
            description: value
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            more: value.get("more").and_then(Value::as_bool).unwrap_or(false),
            effort: value.get("effort").and_then(Value::as_bool),
            default: value
                .get("default")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// The CLI each picker read starts, by agent.
fn program_name(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("claude"),
        "cursor" => Some("cursor-agent"),
        "antigravity" => Some("agy"),
        _ => None,
    }
}

/// The agent's picker rows for the installed CLI version: the cached read when the version is the
/// one it was read on, else a fresh read (cached on success). `None` while the CLI is missing,
/// signed out, or its picker could not be read.
pub(crate) fn picker_rows(agent: &str, home: &Path, cache_dir: &Path) -> Option<Vec<PickerRow>> {
    let program = crate::agent_hooks::probing::resolve_probe_command(program_name(agent)?, home)?;
    let version = cli_version(agent, &program, home)?;
    let cache_path = cache_dir.join("agent-model-pickers.json");
    let mut cache: BTreeMap<String, Value> = std::fs::read_to_string(&cache_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if let Some(saved) = cache
        .get(agent)
        .filter(|saved| saved.get("version").and_then(Value::as_str) == Some(version.as_str()))
    {
        let rows: Vec<PickerRow> = saved
            .get("rows")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(PickerRow::from_json)
            .collect();
        if !rows.is_empty() {
            return Some(rows);
        }
    }
    let scratch = cache_dir.join("model-picker-scratch");
    std::fs::create_dir_all(&scratch).ok()?;
    let rows = read_picker(agent, &program, &scratch).filter(|rows| !rows.is_empty())?;
    cache.insert(
        agent.to_string(),
        json!({
            "version": version,
            "readAt": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "rows": rows.iter().map(PickerRow::to_json).collect::<Vec<_>>(),
        }),
    );
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let staged = cache_path.with_extension("json.tmp");
        if std::fs::write(&staged, text).is_ok() {
            let _ = std::fs::rename(&staged, &cache_path);
        }
    }
    Some(rows)
}

/// The CLI's `--version` line, which keys the cached read.
fn cli_version(agent: &str, program: &str, home: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command
        .arg("--version")
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("NO_COLOR", "1");
    if agent == "antigravity" {
        let (key, value) = crate::agent_hooks::probing::AGY_BACKGROUND_AUTO_UPDATE_OFF;
        command.env(key, value);
    }
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(20) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    child.stdout.take()?.read_to_string(&mut text).ok()?;
    let line = text.lines().find(|line| !line.trim().is_empty())?.trim();
    Some(line.to_string())
}

fn read_picker(agent: &str, program: &str, scratch: &Path) -> Option<Vec<PickerRow>> {
    match agent {
        "claude" => read_claude(program, scratch),
        "cursor" => read_cursor(program, scratch),
        "antigravity" => read_antigravity(program, scratch),
        _ => None,
    }
}

/// A CLI running in a hidden terminal, killed when dropped. Also reads Claude Code's `/usage`
/// screen (accounts/usage_screen.rs).
pub(crate) struct Hidden {
    terminal: Arc<Mutex<LoginTerminal>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    started: Instant,
}

impl Hidden {
    fn spawn(program: &str, args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> Option<Self> {
        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.cwd(cwd);
        for (key, value) in env {
            command.env(key, value);
        }
        Self::spawn_command(command)
    }

    pub(crate) fn spawn_command(mut command: CommandBuilder) -> Option<Self> {
        command.env("TERM", "xterm-256color");
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .ok()?;
        let mut reader = pair.master.try_clone_reader().ok()?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(pair.master.take_writer().ok()?));
        let mut child = pair.slave.spawn_command(command).ok()?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let terminal = Arc::new(Mutex::new(LoginTerminal::sized(ROWS, COLS)));
        let (screen, replies_to) = (terminal.clone(), writer.clone());
        std::thread::spawn(move || {
            let mut bytes = [0; 8192];
            while let Ok(n) = reader.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                let replies = match screen.lock() {
                    Ok(mut terminal) => terminal.process(&bytes[..n]),
                    Err(_) => break,
                };
                if !replies.is_empty() {
                    if let Ok(mut writer) = replies_to.lock() {
                        let _ = writer.write_all(&replies);
                    }
                }
            }
        });
        let master = pair.master;
        std::thread::spawn(move || {
            let _ = child.wait();
            drop(master);
        });
        Some(Self {
            terminal,
            writer,
            killer,
            started: Instant::now(),
        })
    }

    pub(crate) fn text(&self) -> String {
        self.terminal
            .lock()
            .map(|terminal| terminal.text())
            .unwrap_or_default()
    }

    fn send(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    /// Sends `bytes` and waits `pause` for the CLI to redraw.
    pub(crate) fn press(&self, bytes: &[u8], pause_ms: u64) {
        self.send(bytes);
        std::thread::sleep(Duration::from_millis(pause_ms));
    }

    pub(crate) fn out_of_time(&self) -> bool {
        self.started.elapsed() > READ_LIMIT
    }

    /// Polls the screen until `done` holds, for at most `limit`.
    pub(crate) fn wait_for(&self, limit: Duration, done: impl Fn(&str) -> bool) -> Option<String> {
        let started = Instant::now();
        while started.elapsed() < limit && !self.out_of_time() {
            let text = self.text();
            if done(&text) {
                return Some(text);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }
}

impl Drop for Hidden {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}

/// The text after the last line containing `marker`, or the whole screen.
fn after<'a>(text: &'a str, marker: &str) -> &'a str {
    text.rfind(marker).map_or(text, |at| &text[at..])
}

/// Splits a picker row into its label and the description after a run of two or more spaces.
fn split_columns(rest: &str) -> (String, Option<String>) {
    let rest = rest.trim();
    match rest.find("  ") {
        Some(at) => {
            let description = rest[at..].trim();
            (
                rest[..at].trim().to_string(),
                (!description.is_empty()).then(|| description.to_string()),
            )
        }
        None => (rest.to_string(), None),
    }
}

// ---------------------------------------------------------------------------
// Claude Code: "Select model", numbered rows, an unnumbered "More models" row that expands the older
// ones, and an effort line under the list for the focused row.
// ---------------------------------------------------------------------------

/// Waits until Claude Code in `cli` takes input, trusting its scratch folder on the way. `None`
/// when it is signed out or does not get there in time.
pub(crate) fn wait_for_claude_prompt(cli: &Hidden) -> Option<()> {
    loop {
        if cli.out_of_time() {
            return None;
        }
        let text = cli.text();
        if text.contains("Yes, I trust this folder") {
            // "No, exit" is focused first, and a key sent before the prompt listens is lost, so
            // Enter waits until the focus is on "Yes".
            std::thread::sleep(Duration::from_millis(800));
            let focused_yes = cli
                .text()
                .lines()
                .any(|line| line.trim_start().starts_with('❯') && line.contains("Yes, I trust"));
            if focused_yes {
                cli.press(ENTER, 1500);
            } else {
                cli.press(DOWN, 300);
            }
            continue;
        }
        if text.contains("Select login method")
            || (text.contains("/login") && text.contains("Not logged in"))
        {
            return None;
        }
        if text.contains("Claude Code v") && !text.contains("trust this folder") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Some(())
}

fn read_claude(program: &str, scratch: &Path) -> Option<Vec<PickerRow>> {
    let cli = Hidden::spawn(
        program,
        &["--settings", r#"{"disableAllHooks":true}"#],
        scratch,
        &[],
    )?;
    wait_for_claude_prompt(&cli)?;
    std::thread::sleep(Duration::from_millis(2000));
    cli.press(b"/model", 700);
    cli.press(ENTER, 300);
    cli.wait_for(Duration::from_secs(10), |text| {
        text.contains("Select model")
    })?;
    let focused = |cli: &Hidden| -> String {
        after(&cli.text(), "Select model")
            .lines()
            .find(|line| line.trim_start().starts_with('❯'))
            .unwrap_or_default()
            .to_string()
    };
    // Expand More models.
    for _ in 0..30 {
        if focused(&cli).contains("More models") {
            break;
        }
        cli.press(DOWN, 150);
    }
    if focused(&cli).contains("More models") && focused(&cli).contains('▸') {
        cli.press(ENTER, 900);
    }
    // Walk every row from the top to read which take an effort.
    for _ in 0..30 {
        cli.press(UP, 60);
    }
    std::thread::sleep(Duration::from_millis(400));
    let mut no_effort: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..40 {
        let text = cli.text();
        let picker = after(&text, "Select model");
        let line = picker
            .lines()
            .find(|line| line.trim_start().starts_with('❯'))
            .unwrap_or_default()
            .to_string();
        if seen.contains(&line) {
            break;
        }
        seen.push(line.clone());
        if let Some((_, label, _)) = claude_numbered_row(&line) {
            if picker.contains("Effort not supported") {
                no_effort.push(label);
            }
        }
        cli.press(DOWN, 220);
    }
    let text = cli.text();
    cli.press(ESCAPE, 300);
    cli.press(ESCAPE, 300);
    let rows = parse_claude_picker(after(&text, "Select model"), &no_effort);
    (!rows.is_empty()).then_some(rows)
}

/// `  6.  Fable 5.1   Most capable …` (focused rows start with `❯`): number, label, description.
fn claude_numbered_row(line: &str) -> Option<(u32, String, Option<String>)> {
    let line = line.trim_start().trim_start_matches('❯').trim_start();
    let (number, rest) = line.split_once('.')?;
    let number: u32 = number.trim().parse().ok()?;
    let (label, description) = split_columns(rest);
    let label = label.replace('✔', "").trim().to_string();
    (!label.is_empty()).then_some((number, label, description))
}

fn parse_claude_picker(picker: &str, no_effort: &[String]) -> Vec<PickerRow> {
    let mut rows: Vec<PickerRow> = Vec::new();
    let mut default_label: Option<String> = None;
    let mut more = false;
    for line in picker.lines() {
        if line.contains("More models") && claude_numbered_row(line).is_none() {
            more = true;
            continue;
        }
        let Some((_, label, description)) = claude_numbered_row(line) else {
            continue;
        };
        if label.starts_with("Default") {
            // "Default (recommended)  Opus 5.5 · Best for …"
            default_label = description
                .as_deref()
                .and_then(|text| text.split(" · ").next())
                .map(|name| name.trim().to_string());
            continue;
        }
        if rows.iter().any(|row| row.label == label) {
            continue;
        }
        rows.push(PickerRow {
            effort: no_effort.contains(&label).then_some(false),
            default: false,
            more,
            description,
            label,
        });
    }
    if let Some(default) = default_label {
        for row in rows.iter_mut() {
            row.default = row.label == default;
        }
    }
    rows
}

// ---------------------------------------------------------------------------
// Cursor: "Available models", ten rows at a time with an "a-b of N" counter, `→` on the focused row;
// the list wraps at both ends.
// ---------------------------------------------------------------------------

fn read_cursor(program: &str, scratch: &Path) -> Option<Vec<PickerRow>> {
    let cli = Hidden::spawn(program, &[], scratch, &[])?;
    loop {
        if cli.out_of_time() {
            return None;
        }
        let text = cli.text();
        if text.contains("Trust this workspace") && !text.contains("Trusting workspace") {
            cli.press(b"a", 1500);
            continue;
        }
        if text.contains("Please login") || text.contains("cursor-agent login") {
            return None;
        }
        if text.contains("Ctx ") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    std::thread::sleep(Duration::from_millis(1500));
    cli.press(b"/model", 800);
    cli.press(ENTER, 300);
    let text = cli.wait_for(Duration::from_secs(10), |text| {
        text.contains("Available models") && cursor_counter(text).is_some()
    })?;
    let (_, total) = cursor_counter(&text)?;
    // (label, focused row is the first one shown and the counter starts at 1)
    let mut walk: Vec<(String, bool)> = Vec::new();
    for _ in 0..total {
        let text = cli.text();
        let list = after(&text, "Available models");
        let rows: Vec<&str> = list
            .lines()
            .skip_while(|line| !line.trim_start().starts_with("Filter:"))
            .skip(1)
            .take_while(|line| cursor_counter(line).is_none())
            .filter(|line| !line.trim().is_empty())
            .collect();
        let focused = rows
            .iter()
            .position(|line| line.trim_start().starts_with('→'));
        if let Some(index) = focused {
            let label = cursor_row_label(rows[index]);
            let first = index == 0 && cursor_counter(list).is_some_and(|(start, _)| start == 1);
            walk.push((label, first));
        }
        cli.press(DOWN, 200);
    }
    cli.press(ESCAPE, 300);
    cli.press(ESCAPE, 300);
    let start = walk.iter().position(|(_, first)| *first).unwrap_or(0);
    let mut labels: Vec<String> = Vec::new();
    for (label, _) in walk[start..].iter().chain(walk[..start].iter()) {
        if !label.is_empty() && !labels.contains(label) {
            labels.push(label.clone());
        }
    }
    (labels.len() == total as usize)
        .then(|| labels.iter().map(|label| PickerRow::named(label)).collect())
}

/// `1-10 of 47` as (first shown, total).
fn cursor_counter(text: &str) -> Option<(u32, u32)> {
    text.lines().rev().find_map(|line| {
        let line = line.trim();
        let (range, total) = line.split_once(" of ")?;
        let (first, _) = range.split_once('-')?;
        Some((first.trim().parse().ok()?, total.trim().parse().ok()?))
    })
}

/// ` →  Claude Opus 5.5          300K Medium (Tab to modify)` ⇒ `Claude Opus 5.5`.
fn cursor_row_label(line: &str) -> String {
    let line = line.trim_start().trim_start_matches('→').trim();
    split_columns(line.split(" (Tab to modify)").next().unwrap_or(line)).0
}

// ---------------------------------------------------------------------------
// Antigravity: "Switch Model", a search box, then every model on its own line (`>` on the focused
// one, `(current)` after the model in use), then the focused row's Effort slider.
// ---------------------------------------------------------------------------

fn read_antigravity(program: &str, scratch: &Path) -> Option<Vec<PickerRow>> {
    let (key, value) = crate::agent_hooks::probing::AGY_BACKGROUND_AUTO_UPDATE_OFF;
    let cli = Hidden::spawn(program, &[], scratch, &[(key, value)])?;
    loop {
        if cli.out_of_time() {
            return None;
        }
        let text = cli.text();
        if text.contains("Yes, I trust this folder") {
            cli.press(ENTER, 1500);
            continue;
        }
        if text.contains("Sign in") && !text.contains("? for shortcuts") {
            return None;
        }
        if text.contains("? for shortcuts") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    std::thread::sleep(Duration::from_millis(1000));
    if cli.text().contains("esc to dismiss") {
        cli.press(ESCAPE, 500);
    }
    cli.press(b"/model", 700);
    cli.press(ENTER, 300);
    let text = cli.wait_for(Duration::from_secs(10), |text| {
        text.contains("Switch Model") && (text.contains("Keyboard:") || text.contains("Effort"))
    })?;
    cli.press(ESCAPE, 300);
    cli.press(ESCAPE, 300);
    let rows = parse_antigravity_picker(after(&text, "Switch Model"));
    (!rows.is_empty()).then_some(rows)
}

fn parse_antigravity_picker(picker: &str) -> Vec<PickerRow> {
    let mut rows = Vec::new();
    let mut started = false;
    for line in picker
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Search:"))
        .skip(1)
    {
        let trimmed = line.trim();
        if trimmed.starts_with('─') {
            continue;
        }
        if trimmed.is_empty() {
            if started {
                break;
            }
            continue;
        }
        if trimmed.starts_with("Effort") || trimmed.starts_with("Keyboard:") {
            break;
        }
        started = true;
        let label = trimmed
            .trim_start_matches('>')
            .trim()
            .trim_end_matches("(current)")
            .trim();
        if !label.is_empty() {
            rows.push(PickerRow::named(label));
        }
    }
    rows
}

// ---------------------------------------------------------------------------
// Turning picker rows into catalog rows.
// ---------------------------------------------------------------------------

fn row_names<'a>(row: &'a Value) -> impl Iterator<Item = &'a str> {
    ["pickerLabel", "label"]
        .into_iter()
        .filter_map(|key| row.get(key).and_then(Value::as_str))
        .chain(
            row.get("terminalLabels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
}

fn find_row<'a>(rows: &'a [Value], label: &str) -> Option<&'a Value> {
    rows.iter()
        .find(|row| row.get("pickerLabel").and_then(Value::as_str) == Some(label))
        .or_else(|| {
            rows.iter()
                .find(|row| row_names(row).any(|name| name == label))
        })
}

/// `Opus 4.8` ⇒ `claude-opus-4-8`; `GLM 5.3 Flash` ⇒ `glm-5.3-flash` (a name the catalog never had).
///
/// CDXC:AgentProviders 2026-11-13 WHY: Claude Code 2.1.323's picker lists bare families ("Opus",
/// "Fable", the version only in the description), and `claude-opus` is no model Claude knows:
/// `/model claude-opus` answered "Model 'claude-opus' not found" in the user's session (G7zwb) on
/// every retry. A bare family is Claude's own alias (`opus`), which `/model` takes.
fn derived_value(agent: &str, label: &str) -> String {
    let slug = label.to_ascii_lowercase().replace(' ', "-");
    if agent == "claude" {
        if !slug.is_empty() && slug.chars().all(|c| c.is_ascii_alphabetic()) {
            return slug;
        }
        format!("claude-{}", slug.replace('.', "-"))
    } else {
        slug
    }
}

/// The model a Claude picker row names: its label ("Opus 5.5"), or for a bare family row
/// ("Opus") the description's first part ("Opus 5.5 · Best for everyday, complex tasks").
fn claude_row_names(row: &PickerRow) -> Vec<String> {
    let mut names = vec![row.label.clone()];
    if let Some(first) = row
        .description
        .as_deref()
        .and_then(|description| description.split(" \u{b7} ").next())
        .map(str::trim)
        .filter(|first| !first.is_empty() && *first != row.label)
    {
        names.push(first.to_string());
    }
    names
}

/// Puts a list command's lineup in the picker's order and adds the rows only the picker shows,
/// described by the published catalog's row of the same name when it has one. Rows the picker does
/// not show are dropped: the picker is what the user can pick from.
pub(crate) fn order_by_picker(
    agent: &str,
    mut entry: Value,
    picker: &[PickerRow],
    published: Option<&Value>,
) -> Value {
    let listed: Vec<Value> = entry
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let published_rows: Vec<Value> = published
        .and_then(|agent| agent.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut models: Vec<Value> = Vec::new();
    for row in picker {
        // A label that fills its column runs into the row's settings with one space
        // ("DeepSeek V4.1 Flash 1017 High"), so the longest leading words that name a row win.
        let words: Vec<&str> = row.label.split_whitespace().collect();
        let known = (1..=words.len()).rev().find_map(|count| {
            let name = words[..count].join(" ");
            find_row(&listed, &name).or_else(|| find_row(&published_rows, &name))
        });
        let mut model = known.cloned().unwrap_or_else(|| {
            json!({
                "value": derived_value(agent, &row.label),
                "label": row.label,
                "pickerLabel": row.label,
                "efforts": [],
                "fastMode": false,
            })
        });
        if let Some(object) = model.as_object_mut() {
            object.remove("group");
        }
        let value = model.get("value").cloned();
        if !models
            .iter()
            .any(|known| known.get("value") == value.as_ref())
        {
            models.push(model);
        }
    }
    if !models.is_empty() {
        entry["models"] = json!(models);
    }
    entry
}

/// Claude Code's lineup from its picker: each row keeps the published catalog's value, efforts and
/// Fast when the catalog names it, takes the picker's description, and the rows under More models
/// are its Legacy models.
pub(crate) fn claude_lineup(picker: &[PickerRow], published: &Value) -> Option<Value> {
    let published_rows: Vec<Value> = published
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut models: Vec<Value> = Vec::new();
    for row in picker {
        let mut model = claude_row_names(row)
            .iter()
            .find_map(|name| find_row(&published_rows, name))
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "value": derived_value("claude", &row.label),
                    "label": row.label,
                    "fastMode": false,
                })
            });
        let object = model.as_object_mut()?;
        object.remove("group");
        object.remove("default");
        if let Some(description) = &row.description {
            object.insert("description".to_string(), json!(description));
        }
        if row.effort == Some(false) {
            object.insert("efforts".to_string(), json!([]));
        }
        if row.default {
            object.insert("default".to_string(), json!(true));
        }
        if row.more {
            object.insert(
                "group".to_string(),
                json!(ghostex_gx_chat_core::menus::catalog::LEGACY_GROUP_ID),
            );
        }
        let value = object.get("value").cloned();
        if !models
            .iter()
            .any(|known| known.get("value") == value.as_ref())
        {
            models.push(model);
        }
    }
    if models.is_empty() {
        return None;
    }
    // A current catalog model the picker read did not show stays offered: the read runs Claude
    // signed in as the computer's own login, which may not list a model the user's accounts run
    // (Claude Code 2.1.323 listed Fable 5.1 alone while the user's sessions ran Fable 5.2).
    for row in &published_rows {
        let value = row.get("value");
        if row.get("group").is_none() && !models.iter().any(|known| known.get("value") == value) {
            models.push(row.clone());
        }
    }
    let mut entry = published.clone();
    let object = entry.as_object_mut()?;
    object.insert("models".to_string(), json!(models));
    object.insert("legacyRule".to_string(), json!("newestPerFamily"));
    if picker.iter().any(|row| row.more) {
        let groups = object.entry("groups").or_insert_with(|| json!([]));
        if let Some(groups) = groups.as_array_mut() {
            let legacy = ghostex_gx_chat_core::menus::catalog::LEGACY_GROUP_ID;
            if !groups
                .iter()
                .any(|group| group.get("id").and_then(Value::as_str) == Some(legacy))
            {
                groups.push(json!({
                    "id": legacy,
                    "label": ghostex_gx_chat_core::menus::catalog::LEGACY_GROUP_LABEL,
                }));
            }
        }
    }
    Some(entry)
}
