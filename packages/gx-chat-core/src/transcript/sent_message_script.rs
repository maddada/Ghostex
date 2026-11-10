//! The script around a `ghostex agents send`: where its message text comes from when the call does
//! not spell it out, and whether a failed call means the send itself failed.
//!
//! CDXC:SessionChat 2026-10-10 WHY:
//! User: "NOT SENT isn't aligned center vertically in the header of the card ... and i see $msg instead of the message". Agents on Windows send with `$msg = @'…'@; ghostex agents send <ref> $msg 2>&1 | Select-String '"status"'`, and on POSIX with `"$(cat file)"`, so the card read the literal `$msg` and, finding no receipt in the filtered output, called a delivered message NOT SENT. The body now comes from the CLI's echoed `message`, else from the variable's assignment, here-string or file in the same script, and never shows a bare `$var` (it says the script set it). NOT SENT needs a printed send error or a failed exit status that belongs to the send itself; a filtered or chained command whose status says nothing about the send shows no status instead of a wrong one.
//! SEE-ALSO: sent_message.rs reads the cards; script_loops.rs unrolls the loops a send sits in; server/src/ghostex_cli/agents/delivery.rs and lifecycle.rs echo `message` / `task` in their JSON.

use crate::transcript::jsstr::ascii_lower;
use crate::transcript::script_loops::{powershell_loops, unroll_loops};
use crate::transcript::shell_script::{parse_shell, ShellCommand};

/// What the card says for a message whose text only a script variable held.
pub const SCRIPTED_BODY: &str = "The message text was set by a script, so it isn't shown here.";

/// The word a lifted here-string reads as in the script the shell reader sees.
const HERE_STRING_MARK: &str = "__gx_here_string_";

/// A send's message as far as the script tells.
pub enum Resolved {
    Text(String),
    /// A variable or substitution whose value the script does not show.
    Unknown,
}

/// One tool call's command line, read for its sends.
pub struct Script<'a> {
    pub commands: Vec<ShellCommand>,
    here_strings: Vec<String>,
    /// The files the same message's tools wrote (`(path, content)`).
    written: &'a [(&'a str, &'a str)],
}

impl<'a> Script<'a> {
    pub fn parse(command: &str, written: &'a [(&'a str, &'a str)]) -> Self {
        let (script, here_strings) = lift_here_strings(command);
        let commands = parse_shell(&powershell_loops(&script))
            .into_iter()
            .map(|mut shell| {
                // `do ghostex agents send …` on a loop's own line: the keyword is no word of the
                // command it starts.
                let keywords = shell
                    .words
                    .iter()
                    .take_while(|word| matches!(word.as_str(), "do" | "then" | "else" | "{"))
                    .count();
                shell.words.drain(..keywords);
                shell
            })
            .collect();
        Script {
            commands: unroll_loops(commands),
            here_strings,
            written,
        }
    }

    /// The text a send's message `word` stands for, reading assignments made before command `at`.
    pub fn value(&self, word: &str, at: usize, send: &ShellCommand) -> Resolved {
        if let Some(body) = self.here_string(word) {
            return Resolved::Text(body.to_string());
        }
        if let Some(name) = variable_name(word) {
            let assigned = self.commands[..at.min(self.commands.len())]
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, shell)| {
                    assignment(shell)
                        .filter(|(assigned, _)| *assigned == name)
                        .map(|(_, value)| (index, value))
                });
            return match assigned {
                Some((index, words)) if words.len() == 1 => {
                    self.value(&words[0], index, &self.commands[index])
                }
                Some((_, words)) => self
                    .file_read(&words)
                    .and_then(|path| self.file_text(path, send))
                    .map_or(Resolved::Unknown, Resolved::Text),
                None => Resolved::Unknown,
            };
        }
        if let Some(inner) = word
            .strip_prefix("$(")
            .and_then(|inner| inner.strip_suffix(')'))
        {
            let read = parse_shell(inner);
            return read
                .first()
                .and_then(|shell| self.file_read(&shell.words))
                .and_then(|path| self.file_text(path, send))
                .map_or(Resolved::Unknown, Resolved::Text);
        }
        if is_expansion(word) {
            return Resolved::Unknown;
        }
        Resolved::Text(self.interpolate(word, at, send))
    }

    /// A word with each `$name` / `${name}` inside it replaced by the variable's value where the
    /// script shows it (`S8g:P2uwo:$t` in a loop over session ids); one it does not show stays as
    /// written.
    fn interpolate(&self, word: &str, at: usize, send: &ShellCommand) -> String {
        let mut text = String::with_capacity(word.len());
        let mut rest = word;
        while let Some(start) = rest.find('$') {
            text.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let (name, length) = match after.strip_prefix('{') {
                Some(braced) => braced
                    .find('}')
                    .map_or(("", 0), |end| (&braced[..end], end + 2)),
                None => {
                    let end = after
                        .find(|character: char| !(character.is_alphanumeric() || character == '_'))
                        .unwrap_or(after.len());
                    (&after[..end], end)
                }
            };
            let value = (!name.is_empty()
                && !name.starts_with(|first: char| first.is_ascii_digit()))
            .then(|| self.value(&format!("${name}"), at, send))
            .and_then(|resolved| match resolved {
                Resolved::Text(value) => Some(value),
                Resolved::Unknown => None,
            });
            match value {
                Some(value) => text.push_str(&value),
                None => text.push_str(&rest[start..start + 1 + length]),
            }
            rest = &rest[start + 1 + length..];
        }
        text.push_str(rest);
        text
    }

    fn here_string(&self, word: &str) -> Option<&str> {
        let index = word
            .strip_prefix(HERE_STRING_MARK)?
            .strip_suffix("__")?
            .parse::<usize>()
            .ok()?;
        self.here_strings.get(index).map(String::as_str)
    }

    /// The path a `cat path` / `Get-Content -Raw path` reads.
    fn file_read<'w>(&self, words: &'w [String]) -> Option<&'w str> {
        let (program, arguments) = words.split_first()?;
        if !matches!(
            ascii_lower(program).as_str(),
            "cat" | "get-content" | "gc" | "type"
        ) {
            return None;
        }
        let mut path = None;
        let mut arguments = arguments.iter();
        while let Some(word) = arguments.next() {
            match ascii_lower(word).as_str() {
                "-path" | "-literalpath" => path = arguments.next().map(String::as_str),
                "-encoding" | "-totalcount" | "-tail" | "-head" => {
                    arguments.next();
                }
                flag if flag.starts_with('-') => {}
                _ => path = path.or(Some(word.as_str())),
            }
        }
        path
    }

    /// What a file held, when the same call or message wrote it (`-` is the send's here-document).
    /// The two may name it differently: a Write tool records the absolute path while the send says
    /// `~/…` ([`same_file`]).
    pub fn file_text(&self, path: &str, send: &ShellCommand) -> Option<String> {
        if matches!(path, "-" | "/dev/stdin") {
            return send.heredoc.clone();
        }
        self.commands
            .iter()
            .rev()
            .find_map(|shell| {
                let writes = match shell.words.first().map(String::as_str) {
                    Some("cat") => shell
                        .stdout_file
                        .as_deref()
                        .is_some_and(|file| same_file(file, path)),
                    Some("tee") => shell.words[1..].iter().any(|word| same_file(word, path)),
                    _ => false,
                };
                writes.then(|| shell.heredoc.clone()).flatten()
            })
            .or_else(|| {
                self.written
                    .iter()
                    .rev()
                    .find(|(file, _)| same_file(file, path))
                    .map(|(_, content)| content.to_string())
            })
    }

    /// Whether a call whose output holds no send result shows that the send at command `at`
    /// failed: the CLI printed a send error, or the call failed with the send's own exit status
    /// (nothing after it but PowerShell cmdlets, which leave the native command's status).
    pub fn send_failed(&self, output: &str, call_failed: bool, at: usize) -> bool {
        if output.contains("\"status\": \"") || output.contains("\"status\":\"") {
            return false;
        }
        if send_error_line(output).is_some() {
            return true;
        }
        call_failed
            && self.commands[at.min(self.commands.len())..]
                .iter()
                .skip(1)
                .all(|shell| {
                    shell.words.first().is_some_and(|program| {
                        matches!(
                            ascii_lower(program).as_str(),
                            "select-string"
                                | "sls"
                                | "select-object"
                                | "select"
                                | "out-string"
                                | "out-host"
                                | "where-object"
                                | "where"
                                | "?"
                                | "foreach-object"
                                | "%"
                                | "format-list"
                                | "format-table"
                        )
                    })
                })
    }
}

/// What the CLI prints when it refuses or loses a send, by phrase.
const SEND_ERRORS: [&str; 8] = [
    "failed or is uncertain",
    "Message body must not be empty",
    "is not an agent session",
    "Could not read message file",
    "Could not read task file",
    "exceeds the send limit",
    "Unknown or hidden agent type",
    "No matching session found",
];

/// The output line holding the CLI's send error, which the sent card shows as why it did not go out.
pub fn send_error_line(output: &str) -> Option<&str> {
    output
        .lines()
        .map(str::trim)
        .find(|line| SEND_ERRORS.iter().any(|error| line.contains(error)))
}

/// PowerShell here-strings (`@'` … `'@`, `@"` … `"@`) lifted out of a script before the POSIX
/// reader sees it, since quotes inside them would throw its quoting off: each becomes the single
/// word `__gx_here_string_<n>__`, its body the `n`th of the returned texts.
fn lift_here_strings(script: &str) -> (String, Vec<String>) {
    let mut lifted = String::with_capacity(script.len());
    let mut bodies = Vec::new();
    let mut lines = script.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let content = line.trim_end_matches(['\r', '\n']);
        let quote = match content.get(content.len().saturating_sub(2)..) {
            Some("@'") => '\'',
            Some("@\"") => '"',
            _ => {
                lifted.push_str(line);
                continue;
            }
        };
        let closer = format!("{quote}@");
        let mut body: Vec<&str> = Vec::new();
        let mut rest = None;
        for next in lines.by_ref() {
            let text = next.trim_end_matches(['\r', '\n']);
            if let Some(after) = text.strip_prefix(closer.as_str()) {
                rest = Some((after, &next[text.len()..]));
                break;
            }
            body.push(next);
        }
        let Some((after, ending)) = rest else {
            // Unterminated: leave it as written.
            lifted.push_str(line);
            body.iter().for_each(|text| lifted.push_str(text));
            continue;
        };
        lifted.push_str(&content[..content.len() - 2]);
        lifted.push_str(&format!("{HERE_STRING_MARK}{}__", bodies.len()));
        lifted.push_str(after);
        lifted.push_str(ending);
        bodies.push(
            body.concat()
                .trim_end_matches(['\r', '\n'])
                .replace("\r\n", "\n"),
        );
    }
    (lifted, bodies)
}

/// The variable a whole word names: `$msg`, `${msg}`, `$env:MSG` (lowercased; PowerShell
/// variables ignore case).
pub(crate) fn variable_name(word: &str) -> Option<String> {
    let name = word.strip_prefix('$')?;
    let name = name
        .strip_prefix('{')
        .and_then(|name| name.strip_suffix('}'))
        .unwrap_or(name);
    let valid = !name.is_empty()
        && !name.starts_with(|first: char| first.is_ascii_digit())
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | ':'));
    valid.then(|| ascii_lower(name))
}

/// `$msg = …` (PowerShell), `$msg=…`, or `msg=…` / `export msg=…` (POSIX): the variable and the
/// words of its value.
pub(crate) fn assignment(shell: &ShellCommand) -> Option<(String, Vec<String>)> {
    let words = match shell.words.first().map(String::as_str) {
        Some("export" | "local" | "declare" | "readonly") => &shell.words[1..],
        _ => &shell.words[..],
    };
    let first = words.first()?;
    if words.get(1).map(String::as_str) == Some("=") {
        return Some((variable_name(first)?, words[2..].to_vec()));
    }
    // `msg=x cmd` sets the variable for `cmd` only.
    let (name, value) = first.split_once('=')?;
    if words.len() != 1 {
        return None;
    }
    let name = if name.starts_with('$') {
        variable_name(name)?
    } else {
        variable_name(&format!("${name}"))?
    };
    Some((name, vec![value.to_string()]))
}

/// A word that is wholly a variable, a substitution or a PowerShell expression.
pub(crate) fn is_expansion(word: &str) -> bool {
    let word = word.trim();
    word.starts_with('`')
        || (word.starts_with('(') && word.ends_with(')'))
        || word.strip_prefix('$').is_some_and(|rest| {
            rest.starts_with(|next: char| next.is_alphabetic() || matches!(next, '_' | '{' | '('))
        })
}

/// Whether two names of a file are the same file: equal, or one under the home folder (`~/`,
/// `$HOME/`, `${HOME}/`) and the other the same path under an absolute home folder
/// (`/Users/<name>`, `/home/<name>`, `/root`, `C:\Users\<name>`). The core does not know the
/// machine's home folder, so any home-shaped prefix stands for it.
///
/// CDXC:SessionChat 2026-11-10 WHY: a coordinator wrote a thread's brief with the Write tool (an
/// absolute path) and started the thread with `--body-file ~/…/brief.md`; the exact match missed,
/// the card had no message and its row did nothing when clicked.
fn same_file(a: &str, b: &str) -> bool {
    a == b
        || under_home(a).is_some_and(|rest| in_some_home(b, rest))
        || under_home(b).is_some_and(|rest| in_some_home(a, rest))
}

/// The rest of a path named from the home folder (`~/rest`, `$HOME/rest`, `${HOME}/rest`).
fn under_home(path: &str) -> Option<&str> {
    ["~/", "$HOME/", "${HOME}/"]
        .iter()
        .find_map(|home| path.strip_prefix(home))
        .filter(|rest| !rest.is_empty())
}

/// Whether `path` is `rest` under an absolute home folder.
fn in_some_home(path: &str, rest: &str) -> bool {
    let path = path.replace('\\', "/");
    let rest = rest.replace('\\', "/");
    let Some(home) = path
        .strip_suffix(rest.as_str())
        .and_then(|home| home.strip_suffix('/'))
    else {
        return false;
    };
    let parts: Vec<&str> = home.split('/').collect();
    match parts.as_slice() {
        ["", "root"] => true,
        ["", "Users" | "home", name] => !name.is_empty(),
        [drive, "Users", name] => drive.len() == 2 && drive.ends_with(':') && !name.is_empty(),
        _ => false,
    }
}
