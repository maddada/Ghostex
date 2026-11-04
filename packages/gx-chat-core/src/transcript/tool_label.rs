//! The readable label a tool row carries when the agent wrote no description for the call:
//! "Read src/math.js", "Searched for TODO in src", "Ran node test.js", "Edited math.js",
//! "Opened ghostex.dev". Every agent's calls get one, built from what the call actually did: the
//! tool's name and arguments, the shell command it ran, or the `tools.*` calls inside a Codex
//! code-mode `exec` script.
//!
//! CDXC:SessionChat 2026-10-24 DECISION: User, on a Codex chat in Simple mode: "all i'm seeing is exec exec exec between words. this looks very bad. claude looks much better ... we need chat in codex to match how it does in claude where we don't show just exec exec we actually show the related text there for those tool calls somehow ... do same for all the agents we support chat for please". A call the agent did not describe is named by a summary of what it did, drawn like an agent's description (`tool_description`) in every mode, on the desktop and the phone; opening the row still shows the full command or arguments. A call this module cannot read keeps its tool name and preview.

use serde_json::Value;

use crate::transcript::file_change_rows::file_change_display_path;
use crate::transcript::jsstr::{collapse_whitespace, js_trim, utf16_len};
use crate::transcript::shell_script::{parse_shell, ShellCommand};
use crate::transcript::tool_rows::is_command_tool;
use crate::transcript::tool_summary::{
    command_detail, js_string_value, json_string_literal, key_literals, tool_file_path,
    truncate_tool_preview, MAX_PREVIEW_STRING_INPUT,
};

/// The longest object (a path, a pattern, a command) one action names before it is cut.
const MAX_OBJECT_LENGTH: usize = 56;
/// How many objects one verb lists before "and N more".
const MAX_OBJECTS: usize = 3;

/// One thing a call did: a verb and what it acted on ("Read" + "src/math.js").
#[derive(Clone, Debug, PartialEq, Eq)]
struct Action {
    verb: &'static str,
    object: String,
}

impl Action {
    fn new(verb: &'static str, object: impl Into<String>) -> Self {
        Self {
            verb,
            object: object.into(),
        }
    }
}

/// A call's readable label and the glyph that fits what it did, when the call can be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolLabel {
    pub text: String,
    /// The row's glyph when the label says better than the tool's name what ran (a Codex `exec`
    /// that only edited a file draws the edit glyph).
    pub glyph: Option<&'static str>,
    /// The done-form verb every action shares ("Read", "Ran", "Searched for"), when they share
    /// one: what kind of result the call's output holds (`tool_card.rs`).
    pub verb: Option<&'static str>,
}

/// The label for a call the agent did not describe, or `None` when nothing readable comes out.
///
/// CDXC:SessionChat 2026-10-27 WHY: A label names what a call did, so it reads as done ("Created
/// proof.txt") only once the call has finished well. While a call waits for approval or runs, and
/// after it was declined or failed, the same words read as what it does ("Create proof.txt"): a
/// Codex `touch` blocked in Read Only and then declined showed "Created proof-web-readonly.txt"
/// although no file was ever made.
pub fn tool_label(
    name: &str,
    input: &Value,
    working_directory: Option<&str>,
    finished: bool,
) -> Option<ToolLabel> {
    tool_label_in(
        name,
        input,
        working_directory,
        if finished {
            LabelTense::Done
        } else {
            LabelTense::Base
        },
    )
}

/// The tense a label is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelTense {
    /// What the call does: "Read src/math.js", "Run cargo test" (a call waiting or declined).
    Base,
    /// What the call is doing: "Reading src/math.js", "Running cargo test".
    Running,
    /// What the call did: "Read src/math.js", "Ran cargo test".
    Done,
    /// What the call could not do: "Could not fetch docs.rs". A command that ran and exited
    /// non-zero did run, so it keeps its done form ("Ran cargo test") and its exit code says the rest.
    Failed,
}

/// [`tool_label`] in any [`LabelTense`]; the tool-call cards (`tool_card.rs`) write each row in
/// the tense of its call.
pub fn tool_label_in(
    name: &str,
    input: &Value,
    working_directory: Option<&str>,
    tense: LabelTense,
) -> Option<ToolLabel> {
    let actions = tool_actions(name, input, working_directory);
    let text = join_actions(&actions, tense)?;
    let verb = actions.first().and_then(|first| {
        actions
            .iter()
            .all(|action| action.verb == first.verb)
            .then_some(first.verb)
    });
    Some(ToolLabel {
        text: truncate_tool_preview(&text, MAX_PREVIEW_STRING_INPUT),
        glyph: verb.and_then(verb_glyph),
        verb,
    })
}

fn verb_glyph(verb: &str) -> Option<&'static str> {
    match verb {
        "Edited" | "Wrote" | "Created" | "Deleted" => Some("edit"),
        "Read" | "Listed" | "Found files" | "Viewed" => Some("file"),
        "Opened" | "Fetched" | "Searched the web for" | "Downloaded" => Some("web"),
        _ => None,
    }
}

/// The tool's name with case, `_`, `-` and `.` folded away: `read_file`, `ReadFile` and
/// `read-file` all read as `readfile`.
fn normalized_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn text_field<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        input
            .get(*key)
            .and_then(Value::as_str)
            .map(js_trim)
            .filter(|text| !text.is_empty())
    })
}

fn tool_actions(name: &str, input: &Value, working_directory: Option<&str>) -> Vec<Action> {
    // Codex's code-mode `exec` (and any harness that hands a tool a script) records the script as
    // a string; its `tools.*` calls are what ran.
    if let Value::String(script) = input {
        if script.contains("tools.") {
            let actions = script_actions(script, working_directory);
            if !actions.is_empty() {
                return actions;
            }
        }
    }
    if let Some(server_tool) = name.strip_prefix("mcp__") {
        return mcp_action(server_tool).into_iter().collect();
    }
    let normalized = normalized_name(name);
    if let Some(actions) = named_tool_actions(&normalized, input, working_directory) {
        return actions;
    }
    if is_command_tool(name) {
        if let Some(command) = command_detail(input) {
            return command_actions(&command, working_directory);
        }
    }
    Vec::new()
}

/// What a tool with a well-known name did, read from its arguments. `None` when the name is not
/// one of them, so the command reader gets a turn.
fn named_tool_actions(
    normalized: &str,
    input: &Value,
    working_directory: Option<&str>,
) -> Option<Vec<Action>> {
    let file = || tool_file_path(input).map(|path| short_path(path, working_directory));
    let files = || -> Vec<String> {
        let mut found: Vec<String> = file().into_iter().collect();
        for key in ["paths", "file_paths", "files"] {
            if let Some(Value::Array(items)) = input.get(key) {
                found.extend(
                    items
                        .iter()
                        .filter_map(|item| {
                            item.as_str()
                                .or_else(|| item.get("path").and_then(Value::as_str))
                                .or_else(|| item.get("file_path").and_then(Value::as_str))
                        })
                        .map(|path| short_path(path, working_directory)),
                );
            }
        }
        found
    };
    let each = |verb: &'static str, objects: Vec<String>| -> Vec<Action> {
        objects
            .into_iter()
            .map(|object| Action::new(verb, object))
            .collect()
    };
    let actions = match normalized {
        "read" | "readfile" | "view" | "viewfile" | "readmanyfiles" | "notebookread" | "cat"
        | "openfile" => {
            let found = files();
            if found.is_empty() {
                return None;
            }
            each("Read", found)
        }
        "write" | "writefile" | "create" | "createfile" | "newfile" => each("Wrote", files()),
        "edit"
        | "multiedit"
        | "editfile"
        | "replace"
        | "strreplace"
        | "strreplaceeditor"
        | "strreplacebasededittool"
        | "notebookedit"
        | "searchreplace"
        | "applydiff" => {
            let mut found = files();
            // OMP's edit names its file in a `[src/math.js#55AD]` header line of its `input`.
            if found.is_empty() {
                if let Some(header) = text_field(input, &["input"])
                    .and_then(|text| text.strip_prefix('['))
                    .and_then(|text| text.split([']', '#', '\n']).next())
                {
                    found.push(short_path(header, working_directory));
                }
            }
            each("Edited", found)
        }
        "applypatch" | "patch" => {
            let patch = match input {
                Value::String(text) => Some(text.as_str()),
                _ => text_field(input, &["input", "patch", "patchText", "patch_text"]),
            };
            match patch {
                Some(patch) => patch_actions(patch, working_directory),
                None => each("Edited", files()),
            }
        }
        "grep" | "searchfilecontent" | "grepsearch" | "ripgrep" | "rg" | "codebasesearch"
        | "searchcode" | "search" | "findtext" => {
            let pattern = text_field(input, &["pattern", "query", "regex", "search"]);
            let Some(pattern) = pattern else {
                return None;
            };
            let first_path = input
                .get("paths")
                .and_then(Value::as_array)
                .and_then(|paths| paths.first())
                .and_then(Value::as_str);
            let place = text_field(input, &["path", "dir_path", "directory", "include"])
                .or(first_path)
                .map(|place| short_path(place, working_directory))
                .filter(|place| !matches!(place.as_str(), "" | "." | "./"));
            vec![search_action(pattern, place.as_deref())]
        }
        "glob" | "findfiles" | "filesearch" | "find" | "fd" => {
            let pattern = text_field(input, &["pattern", "glob", "query", "name"])?;
            vec![Action::new(
                "Found files",
                format!("matching {}", short(pattern)),
            )]
        }
        "ls" | "list" | "listdir" | "listdirectory" | "listfiles" | "readdir" => {
            let place = file().filter(|place| !matches!(place.as_str(), "." | "./" | ""));
            vec![Action::new(
                "Listed",
                place.unwrap_or_else(|| "files".to_string()),
            )]
        }
        "webfetch" | "fetch" | "readurl" | "fetchurl" | "urlfetch" | "browse" => {
            let url = text_field(input, &["url", "uri", "href"])?;
            vec![Action::new("Fetched", short_url(url))]
        }
        "websearch" | "googlewebsearch" | "searchweb" | "webquery" | "bravesearch"
        | "exasearch" => {
            let query = text_field(input, &["query", "q", "search_query", "prompt"])
                .map(str::to_string)
                .or_else(|| codex_web_search_query(input))?;
            vec![Action::new("Searched the web for", short(&query))]
        }
        "todowrite" | "updateplan" | "writetodos" | "todo" | "todoread" | "updatetodos"
        | "taskcreate" | "taskupdate" => vec![Action::new("Updated the plan", "")],
        "skill" | "useskill" | "loadskill" | "skillview" => {
            let skill = text_field(input, &["skill", "name", "command"])?;
            vec![Action::new("Used the skill", short(skill))]
        }
        "viewimage" | "readimage" | "image" => {
            vec![Action::new("Viewed", file()?)]
        }
        "toolsearch" => {
            let query = text_field(input, &["query", "q"])?;
            vec![Action::new("Looked up tools for", short(query))]
        }
        "writestdin" => vec![write_stdin_action(input)],
        _ => return None,
    };
    Some(actions)
}

/// Codex's hosted `web_search` call carries `{action: {type: "search", query}}` or `{query}`.
fn codex_web_search_query(input: &Value) -> Option<String> {
    let action = input.get("action").unwrap_or(input);
    text_field(action, &["query", "q"])
        .map(str::to_string)
        .or_else(|| {
            action
                .get("queries")
                .and_then(Value::as_array)
                .and_then(|queries| queries.first())
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| text_field(action, &["url"]).map(short_url))
}

fn write_stdin_action(input: &Value) -> Action {
    let sends = input
        .get("chars")
        .and_then(Value::as_str)
        .is_some_and(|chars| !chars.is_empty());
    if sends {
        Action::new("Sent input to a running command", "")
    } else {
        Action::new("Waited for a running command", "")
    }
}

/// `mcp__spaceo__spaceo_click` reads as "Used spaceo click".
fn mcp_action(server_tool: &str) -> Option<Action> {
    let (server, tool) = server_tool.split_once("__").unwrap_or(("", server_tool));
    let tool = tool
        .strip_prefix(server)
        .map(|rest| rest.trim_start_matches(['_', '-']))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(tool);
    let words = |text: &str| text.replace(['_', '-'], " ").trim().to_string();
    let tool = words(tool);
    if tool.is_empty() {
        return None;
    }
    let server = words(server);
    Some(Action::new(
        "Used",
        if server.is_empty() {
            tool
        } else {
            format!("{server} {tool}")
        },
    ))
}

// ---------------------------------------------------------------------------
// Codex code-mode scripts
// ---------------------------------------------------------------------------

/// The `tools.*` calls of a Codex code-mode script, in the order the script makes them.
fn script_actions(script: &str, working_directory: Option<&str>) -> Vec<Action> {
    let mut actions = Vec::new();
    let mut commands_done = false;
    for (tool, argument) in script_tool_calls(script) {
        match tool.as_str() {
            "exec_command" | "shell" | "local_shell" | "run" | "bash" => {
                // Commands can come from a literal (`{cmd:"…"}`) or a shared list the script maps
                // over (`cmds.map(cmd => tools.exec_command({cmd}))`): read them all once.
                if commands_done {
                    continue;
                }
                commands_done = true;
                let mut commands: Vec<String> = key_literals(script, &["cmd", "command"])
                    .into_iter()
                    .filter_map(js_string_value)
                    .collect();
                if commands.is_empty() {
                    commands = array_string_literals(script);
                }
                for command in commands {
                    actions.extend(command_actions(&command, working_directory));
                }
            }
            "apply_patch" => {
                if let Some(patch) = first_string_literal(argument) {
                    actions.extend(patch_actions(&patch, working_directory));
                }
            }
            "view_image" => {
                if let Some(path) = argument_value(argument, &["path"]) {
                    actions.push(Action::new("Viewed", short_path(&path, working_directory)));
                }
            }
            "write_stdin" => {
                let sends = argument_value(argument, &["chars"]).is_some_and(|c| !c.is_empty());
                actions.push(if sends {
                    Action::new("Sent input to a running command", "")
                } else {
                    Action::new("Waited for a running command", "")
                });
            }
            "update_plan" => actions.push(Action::new("Updated the plan", "")),
            name if name.starts_with("web") => actions.extend(web_run_actions(argument)),
            name => {
                let words = name.replace("__", " ").replace('_', " ");
                actions.push(Action::new("Used", words.trim().to_string()));
            }
        }
    }
    actions
}

/// Every `tools.<name>(<argument>)` in a script, with the argument text up to its closing paren.
fn script_tool_calls(script: &str) -> Vec<(String, &str)> {
    let mut calls = Vec::new();
    let mut from = 0;
    while let Some(found) = script[from..].find("tools.") {
        let start = from + found + "tools.".len();
        let name_len = script[start..]
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(script.len() - start);
        let name = &script[start..start + name_len];
        let after = start + name_len;
        from = after;
        if name.is_empty() || !script[after..].starts_with('(') {
            continue;
        }
        let argument_end = matching_paren(script, after).unwrap_or(script.len());
        calls.push((name.to_string(), &script[after + 1..argument_end]));
        from = argument_end.min(script.len());
    }
    calls
}

/// The `)` that closes the `(` at `open`, skipping string literals.
fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while at < text.len() {
        match bytes[at] {
            b'"' | b'\'' | b'`' => {
                if let Some(literal) = json_string_literal(text, at) {
                    at += literal.len();
                    continue;
                }
                if bytes[at] == b'`' {
                    at += text[at + 1..].find('`').map_or(text.len(), |end| end + 2);
                    continue;
                }
            }
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn argument_value(argument: &str, keys: &[&str]) -> Option<String> {
    key_literals(argument, keys)
        .into_iter()
        .find_map(js_string_value)
}

fn first_string_literal(text: &str) -> Option<String> {
    let at = text.find(['"', '\''])?;
    json_string_literal(text, at).and_then(js_string_value)
}

/// The string literals of the first array literal a script assigns (`const cmds = ["a", "b"]`).
fn array_string_literals(script: &str) -> Vec<String> {
    let Some(open) = script.find("=[").or_else(|| script.find("= [")) else {
        return Vec::new();
    };
    let open = script[open..]
        .find('[')
        .map_or(open, |offset| open + offset);
    let mut values = Vec::new();
    let mut at = open + 1;
    let bytes = script.as_bytes();
    while at < script.len() {
        match bytes[at] {
            b']' => break,
            b'"' | b'\'' => match json_string_literal(script, at) {
                Some(literal) => {
                    at += literal.len();
                    if let Some(value) = js_string_value(literal) {
                        values.push(value);
                    }
                    continue;
                }
                None => break,
            },
            _ => {}
        }
        at += 1;
    }
    values
}

/// `tools.web__run({open:[{ref_id:"https://…"}]})`, `{search_query:[{q:"…"}]}`, `{find:…}`.
fn web_run_actions(argument: &str) -> Vec<Action> {
    let mut actions = Vec::new();
    for query in key_literals(argument, &["q"])
        .into_iter()
        .filter_map(js_string_value)
    {
        actions.push(Action::new("Searched the web for", short(&query)));
    }
    for target in key_literals(argument, &["ref_id", "url"])
        .into_iter()
        .filter_map(js_string_value)
    {
        if target.contains("://") || target.contains('.') {
            actions.push(Action::new("Opened", short_url(&target)));
        }
    }
    if actions.is_empty() {
        for pattern in key_literals(argument, &["pattern"])
            .into_iter()
            .filter_map(js_string_value)
        {
            actions.push(Action::new("Searched the page for", short(&pattern)));
        }
    }
    if actions.is_empty() {
        actions.push(Action::new("Browsed the web", ""));
    }
    actions
}

/// The files an `apply_patch` envelope touches, by what it does to each.
fn patch_actions(patch: &str, working_directory: Option<&str>) -> Vec<Action> {
    let mut actions = Vec::new();
    for line in patch.lines() {
        let line = line.trim();
        let (verb, path) = if let Some(path) = line.strip_prefix("*** Update File:") {
            ("Edited", path)
        } else if let Some(path) = line.strip_prefix("*** Add File:") {
            ("Created", path)
        } else if let Some(path) = line.strip_prefix("*** Delete File:") {
            ("Deleted", path)
        } else {
            continue;
        };
        actions.push(Action::new(
            verb,
            short_path(path.trim(), working_directory),
        ));
    }
    actions
}

// ---------------------------------------------------------------------------
// Shell commands
// ---------------------------------------------------------------------------

/// What a shell command line did, one action per command that did the work: a pipeline's filters
/// (`| head`, `| rg`), `echo` separators and `cd` are left out.
fn command_actions(command: &str, working_directory: Option<&str>) -> Vec<Action> {
    let unwrapped = unwrap_shell_invocation(command);
    let commands = parse_shell(&unwrapped);
    let mut actions = Vec::new();
    for shell in &commands {
        if shell.piped {
            continue;
        }
        if let Some(action) = simple_command_action(shell, working_directory) {
            actions.push(action);
        }
    }
    if actions.is_empty() {
        // Only separators and `cd`: name the line itself.
        let flat = js_trim(&collapse_whitespace(
            unwrapped.lines().next().unwrap_or_default(),
        ))
        .to_string();
        if flat.is_empty() {
            return Vec::new();
        }
        actions.push(Action::new("Ran", short(&flat)));
    }
    actions
}

/// `bash -lc 'cargo test'` and `["zsh", "-c", "…"]` joined into one line both read as the script.
fn unwrap_shell_invocation(command: &str) -> String {
    let trimmed = js_trim(command);
    for shell in ["bash", "zsh", "sh", "/bin/bash", "/bin/zsh", "/bin/sh"] {
        for flag in ["-lc", "-c"] {
            let prefix = format!("{shell} {flag} ");
            if let Some(rest) = trimmed.strip_prefix(&prefix) {
                let words = parse_shell(rest);
                if let Some(script) = words.first().and_then(|first| first.words.first()) {
                    if words.len() == 1 && words[0].words.len() == 1 {
                        return script.clone();
                    }
                }
                return rest.to_string();
            }
        }
    }
    trimmed.to_string()
}

/// A program's own name: `/usr/bin/rg` reads as `rg`.
fn program_name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn simple_command_action(shell: &ShellCommand, working_directory: Option<&str>) -> Option<Action> {
    // Leading `VAR=value`, `sudo`, `time`, `env`, `command`, `exec` and `nohup` hide the program.
    let mut words: &[String] = &shell.words;
    while let Some(first) = words.first() {
        let assignment = first.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if assignment
            || matches!(
                first.as_str(),
                "sudo" | "time" | "env" | "command" | "exec" | "nohup"
            )
        {
            words = &words[1..];
        } else {
            break;
        }
    }
    let (program, arguments) = words.split_first()?;
    let program = program_name(program);
    let path = |text: &str| short_path(text, working_directory);
    let operands = |skip_values: &[&str]| operands(arguments, skip_values);
    match program {
        "cd" | "pushd" | "popd" | "echo" | "printf" | "true" | "false" | ":" | "set" | "export"
        | "unset" | "source" | "." | "clear" => None,
        "cat" | "bat" | "less" | "more" | "nl" | "head" | "tail" | "view" | "type" => {
            // `head -n 20` and `tail -c 5` take a value; `cat -n` and `nl -b a` number lines.
            let files = match program {
                "head" | "tail" => operands(&["-n", "-c", "--lines", "--bytes"]),
                "bat" => operands(&["-r", "--line-range", "-l", "--language"]),
                "nl" => operands(&["-b", "-w", "-s", "-v", "-i"]),
                _ => operands(&[]),
            };
            let files: Vec<&str> = files
                .into_iter()
                .filter(|word| word.parse::<i64>().is_err() && *word != "-")
                .collect();
            if files.is_empty() {
                return None;
            }
            Some(Action::new(
                "Read",
                list(files.iter().map(|file| path(file))),
            ))
        }
        "sed" => {
            let in_place = arguments
                .iter()
                .any(|word| word == "-i" || word.starts_with("-i") || word == "--in-place");
            let printing = arguments
                .iter()
                .any(|word| word == "-n" || word == "--quiet");
            let files = operands(&["-e", "-f"]);
            // The first operand is the sed script unless `-e` or `-f` named it.
            let script_given = arguments.iter().any(|word| word == "-e" || word == "-f");
            let files: Vec<&str> = files.into_iter().skip(usize::from(!script_given)).collect();
            if files.is_empty() {
                return Some(Action::new("Ran", short(&shell.words.join(" "))));
            }
            let objects = list(files.iter().map(|file| path(file)));
            Some(if in_place {
                Action::new("Edited", objects)
            } else if printing {
                Action::new("Read", objects)
            } else {
                Action::new("Ran", short(&shell.words.join(" ")))
            })
        }
        "rg" | "grep" | "egrep" | "fgrep" | "ag" | "ack" | "git-grep" => {
            search_command_action(arguments, working_directory)
        }
        "fd" | "fdfind" => {
            let words = operands(&[
                "-e",
                "--extension",
                "-t",
                "--type",
                "-d",
                "--max-depth",
                "-E",
                "--exclude",
            ]);
            match words.first() {
                Some(pattern) => Some(Action::new(
                    "Found files",
                    format!("matching {}", short(pattern)),
                )),
                None => Some(Action::new("Listed", "files")),
            }
        }
        "find" => {
            let name = arguments
                .windows(2)
                .find(|pair| matches!(pair[0].as_str(), "-name" | "-iname" | "-path" | "-ipath"))
                .map(|pair| pair[1].clone());
            let place = arguments
                .first()
                .filter(|word| !word.starts_with('-') && !matches!(word.as_str(), "." | "./"))
                .map(|word| path(word));
            Some(match (name, place) {
                (Some(name), Some(place)) => {
                    Action::new("Found files", format!("named {} in {place}", short(&name)))
                }
                (Some(name), None) => Action::new("Found files", format!("named {}", short(&name))),
                (None, Some(place)) => Action::new("Listed", format!("files in {place}")),
                (None, None) => Action::new("Listed", "files"),
            })
        }
        "ls" | "tree" | "eza" | "exa" | "lsd" | "dir" => {
            let places = operands(&["-I", "-L", "--level", "--ignore"]);
            let places: Vec<&str> = places
                .into_iter()
                .filter(|place| !matches!(*place, "." | "./"))
                .collect();
            Some(if places.is_empty() {
                Action::new("Listed", "files")
            } else {
                Action::new("Listed", list(places.iter().map(|place| path(place))))
            })
        }
        "wc" => {
            let files = operands(&[]);
            (!files.is_empty()).then(|| {
                Action::new(
                    "Counted lines in",
                    list(files.iter().map(|file| path(file))),
                )
            })
        }
        "git" => git_action(arguments, working_directory),
        "curl" | "wget" | "http" | "xh" => {
            let url = arguments.iter().find(|word| {
                word.contains("://") || (!word.starts_with('-') && word.contains('.'))
            })?;
            let saves = arguments
                .iter()
                .any(|word| matches!(word.as_str(), "-o" | "-O" | "--output" | "--remote-name"));
            Some(Action::new(
                if saves { "Downloaded" } else { "Fetched" },
                short_url(url),
            ))
        }
        "mkdir" => {
            let folders = operands(&["-m", "--mode"]);
            Some(Action::new(
                "Created",
                list(folders.iter().map(|folder| path(folder))),
            ))
        }
        "touch" => {
            let files = operands(&[]);
            Some(Action::new(
                "Created",
                list(files.iter().map(|file| path(file))),
            ))
        }
        "rm" | "rmdir" | "trash" => {
            let files = operands(&[]);
            Some(Action::new(
                "Deleted",
                list(files.iter().map(|file| path(file))),
            ))
        }
        "mv" | "cp" => {
            let files = operands(&["-t", "--target-directory"]);
            let (target, sources) = files.split_last()?;
            Some(Action::new(
                if program == "mv" { "Moved" } else { "Copied" },
                format!(
                    "{} to {}",
                    list(sources.iter().map(|file| path(file))),
                    path(target)
                ),
            ))
        }
        "python" | "python3" | "node" | "bun" | "deno" | "ruby" | "perl" | "php" | "osascript"
        | "swift" => {
            let inline = arguments.is_empty()
                || arguments
                    .iter()
                    .any(|word| matches!(word.as_str(), "-" | "-c" | "-e" | "--eval" | "-p"))
                || shell.heredoc.is_some();
            if inline {
                let language = match program {
                    "python" | "python3" => "Python",
                    "node" | "bun" | "deno" => "JavaScript",
                    "ruby" => "Ruby",
                    "perl" => "Perl",
                    "php" => "PHP",
                    "osascript" => "AppleScript",
                    _ => "Swift",
                };
                Some(Action::new("Ran a script", format!("in {language}")))
            } else {
                Some(Action::new("Ran", short(&shell.words.join(" "))))
            }
        }
        _ => Some(Action::new("Ran", short(&shell.words.join(" ")))),
    }
}

/// `rg -n TODO src` reads as "Searched for TODO in src".
fn search_command_action(arguments: &[String], working_directory: Option<&str>) -> Option<Action> {
    let takes_value = [
        "-g",
        "--glob",
        "-t",
        "--type",
        "-T",
        "--type-not",
        "-C",
        "-A",
        "-B",
        "-m",
        "--max-count",
        "--context",
        "--after-context",
        "--before-context",
        "--include",
        "--exclude",
        "--exclude-dir",
        "-f",
        "--file",
        "-M",
        "--max-columns",
        "--max-depth",
        "-j",
        "--threads",
        "--color",
        "--colors",
        "--sort",
        "--sortr",
        "-r",
        "--replace",
        "--iglob",
    ];
    let mut pattern: Option<String> = None;
    let mut places: Vec<String> = Vec::new();
    let mut at = 0;
    let mut options_done = false;
    while at < arguments.len() {
        let word = &arguments[at];
        at += 1;
        if !options_done && word == "--" {
            options_done = true;
            continue;
        }
        if !options_done && (word == "-e" || word == "--regexp") {
            if pattern.is_none() {
                pattern = arguments.get(at).cloned();
            }
            at += 1;
            continue;
        }
        if !options_done && word.starts_with('-') && word.len() > 1 {
            if takes_value.contains(&word.as_str()) {
                at += 1;
            }
            continue;
        }
        if pattern.is_none() {
            pattern = Some(word.clone());
        } else {
            places.push(word.clone());
        }
    }
    let pattern = pattern?;
    let places: Vec<String> = places
        .into_iter()
        .filter(|place| !matches!(place.as_str(), "." | "./"))
        .map(|place| short_path(&place, working_directory))
        .collect();
    Some(search_action(
        &pattern,
        (!places.is_empty())
            .then(|| list(places.iter().cloned()))
            .as_deref(),
    ))
}

fn search_action(pattern: &str, place: Option<&str>) -> Action {
    let pattern = short(pattern);
    Action::new(
        "Searched for",
        match place {
            Some(place) => format!("{pattern} in {place}"),
            None => pattern,
        },
    )
}

fn git_action(arguments: &[String], working_directory: Option<&str>) -> Option<Action> {
    // `git -C dir status` names its subcommand after the global options.
    let mut at = 0;
    while at < arguments.len() && arguments[at].starts_with('-') {
        at += if matches!(
            arguments[at].as_str(),
            "-C" | "-c" | "--git-dir" | "--work-tree"
        ) {
            2
        } else {
            1
        };
    }
    let subcommand = arguments.get(at)?;
    let rest = &arguments[at + 1..];
    if subcommand == "grep" {
        return search_command_action(rest, working_directory);
    }
    let mut words = vec!["git".to_string(), subcommand.clone()];
    words.extend(rest.iter().take(3).cloned());
    Some(Action::new("Ran", short(&words.join(" "))))
}

/// The operands of a command: its words without options and the values of `skip_values` options.
fn operands<'a>(arguments: &'a [String], skip_values: &[&str]) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut at = 0;
    let mut options_done = false;
    while at < arguments.len() {
        let word = arguments[at].as_str();
        at += 1;
        if !options_done && word == "--" {
            options_done = true;
            continue;
        }
        if !options_done && word.starts_with('-') && word.len() > 1 {
            if skip_values.contains(&word) {
                at += 1;
            }
            continue;
        }
        found.push(word);
    }
    found
}

// ---------------------------------------------------------------------------
// Wording
// ---------------------------------------------------------------------------

/// A path as the reader knows it: relative to the session's folder (or `~/…`), and only its last
/// two parts when the whole would be long.
fn short_path(path: &str, working_directory: Option<&str>) -> String {
    let path = js_trim(path);
    if working_directory
        .is_some_and(|folder| folder.trim_end_matches('/') == path.trim_end_matches('/'))
    {
        return ".".to_string();
    }
    // macOS and Windows folders ignore case, and agents write the same folder both ways
    // (Codex's `…/ghostex/…` under a session folder named `…/Ghostex/…`).
    let inside = working_directory
        .map(|folder| folder.trim_end_matches('/'))
        .filter(|folder| !folder.is_empty() && path.len() > folder.len() + 1)
        .filter(|folder| path.is_char_boundary(folder.len()))
        .and_then(|folder| {
            let (head, rest) = path.split_at(folder.len());
            (head.eq_ignore_ascii_case(folder) && rest.starts_with('/'))
                .then(|| rest[1..].to_string())
        });
    let display = inside.unwrap_or_else(|| file_change_display_path(path, working_directory));
    let display = display.strip_prefix("./").unwrap_or(&display).to_string();
    if utf16_len(&display) <= MAX_OBJECT_LENGTH {
        return display;
    }
    let parts: Vec<&str> = display.trim_end_matches('/').rsplitn(3, '/').collect();
    match parts.as_slice() {
        [file, parent, _] => {
            truncate_tool_preview(&format!("\u{2026}/{parent}/{file}"), MAX_OBJECT_LENGTH)
        }
        _ => truncate_tool_preview(&display, MAX_OBJECT_LENGTH),
    }
}

/// `https://ghostex.dev/og.png?x=1` reads as `ghostex.dev/og.png`.
fn short_url(url: &str) -> String {
    let url = js_trim(url);
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let without_query = without_scheme
        .split(['?', '#'])
        .next()
        .unwrap_or(without_scheme)
        .trim_end_matches('/');
    let without_www = without_query.strip_prefix("www.").unwrap_or(without_query);
    truncate_tool_preview(without_www, MAX_OBJECT_LENGTH)
}

fn short(text: &str) -> String {
    let flat = js_trim(&collapse_whitespace(text)).to_string();
    truncate_tool_preview(&flat, MAX_OBJECT_LENGTH)
}

/// "a", "a and b", "a, b and c", "a, b, c and 2 more".
fn list(objects: impl IntoIterator<Item = String>) -> String {
    let mut items: Vec<String> = Vec::new();
    for item in objects {
        if !item.is_empty() && !items.contains(&item) {
            items.push(item);
        }
    }
    let extra = items.len().saturating_sub(MAX_OBJECTS);
    items.truncate(MAX_OBJECTS);
    if extra > 0 {
        items.push(format!("{extra} more"));
    }
    match items.len() {
        0 => String::new(),
        1 => items.remove(0),
        _ => {
            let last = items.pop().unwrap_or_default();
            format!("{} and {last}", items.join(", "))
        }
    }
}

/// The verb for a call that is running: what it is doing.
fn running_verb(verb: &'static str) -> &'static str {
    match verb {
        "Browsed the web" => "Browsing the web",
        "Copied" => "Copying",
        "Counted lines in" => "Counting lines in",
        "Created" => "Creating",
        "Deleted" => "Deleting",
        "Downloaded" => "Downloading",
        "Edited" => "Editing",
        "Fetched" => "Fetching",
        "Found files" => "Finding files",
        "Listed" => "Listing",
        "Looked up tools for" => "Looking up tools for",
        "Moved" => "Moving",
        "Opened" => "Opening",
        "Ran" => "Running",
        "Ran a script" => "Running a script",
        "Read" => "Reading",
        "Searched for" => "Searching for",
        "Searched the page for" => "Searching the page for",
        "Searched the web for" => "Searching the web for",
        "Sent input to a running command" => "Sending input to a running command",
        "Updated the plan" => "Updating the plan",
        "Used" => "Using",
        "Used the skill" => "Using the skill",
        "Viewed" => "Viewing",
        "Waited for a running command" => "Waiting for a running command",
        "Wrote" => "Writing",
        other => other,
    }
}

/// The verb for a call that has not finished well: what it does rather than what it did.
fn unfinished_verb(verb: &'static str) -> &'static str {
    match verb {
        "Browsed the web" => "Browse the web",
        "Copied" => "Copy",
        "Counted lines in" => "Count lines in",
        "Created" => "Create",
        "Deleted" => "Delete",
        "Downloaded" => "Download",
        "Edited" => "Edit",
        "Fetched" => "Fetch",
        "Found files" => "Find files",
        "Listed" => "List",
        "Looked up tools for" => "Look up tools for",
        "Moved" => "Move",
        "Opened" => "Open",
        "Ran" => "Run",
        "Ran a script" => "Run a script",
        "Searched for" => "Search for",
        "Searched the page for" => "Search the page for",
        "Searched the web for" => "Search the web for",
        "Sent input to a running command" => "Send input to a running command",
        "Updated the plan" => "Update the plan",
        "Used" => "Use",
        "Used the skill" => "Use the skill",
        "Viewed" => "View",
        "Waited for a running command" => "Wait for a running command",
        "Wrote" => "Write",
        // "Read" is spelled the same in both.
        other => other,
    }
}

/// One label for every action: neighbours with the same verb share it ("Read a and b"), and the
/// rest follow in order, lower-cased after the first ("Read a, searched for b, ran c").
fn join_actions(actions: &[Action], tense: LabelTense) -> Option<String> {
    // A command that exited non-zero still ran.
    let tense = match tense {
        LabelTense::Failed
            if actions
                .first()
                .is_some_and(|first| first.verb.starts_with("Ran")) =>
        {
            LabelTense::Done
        }
        other => other,
    };
    let mut groups: Vec<(&'static str, Vec<String>)> = Vec::new();
    for action in actions {
        match groups.last_mut() {
            Some((verb, objects)) if *verb == action.verb => {
                if !action.object.is_empty() && !objects.contains(&action.object) {
                    objects.push(action.object.clone());
                }
            }
            _ => groups.push((
                action.verb,
                if action.object.is_empty() {
                    Vec::new()
                } else {
                    vec![action.object.clone()]
                },
            )),
        }
    }
    let phrases: Vec<String> = groups
        .into_iter()
        .enumerate()
        .map(|(index, (verb, objects))| {
            let verb = match tense {
                LabelTense::Done => verb,
                LabelTense::Running => running_verb(verb),
                LabelTense::Base | LabelTense::Failed => unfinished_verb(verb),
            };
            let verb = if index == 0 {
                verb.to_string()
            } else {
                lower_first(verb)
            };
            let objects = list(objects);
            if objects.is_empty() {
                verb
            } else {
                format!("{verb} {objects}")
            }
        })
        .collect();
    if phrases.is_empty() {
        return None;
    }
    let text = phrases.join(", ");
    Some(if tense == LabelTense::Failed {
        format!("Could not {}", lower_first(&text))
    } else {
        text
    })
}

fn lower_first(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_lowercase().chain(characters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "tool_label_tests.rs"]
mod tests;
