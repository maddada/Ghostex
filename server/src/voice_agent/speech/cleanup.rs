//! What happens to a transcript after the recognizer and before the client types it: the
//! dictionary's aliases fix words it keeps mishearing, filler words go, and an AI polish fixes
//! punctuation and the corrections the reader spoke. The same steps run for every engine.
//!
//! CDXC:Dictate 2026-11-05 DECISION:
//! User (Q1264): a dictionary (recognition hints, and aliases that fix misheard words), filler removal and AI polish, in that order. AI polish is on for a new install, with Claude Sonnet 5.5 (Q1273); the chat box shows Polishing with a Skip button, and if polish fails the unpolished text goes in.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;
use tokio::sync::Notify;

use super::settings::{DictateSettings, DictionaryWord};
use crate::server::AppState;

/// Every alias of every word, longest first so "g x server" wins over "x server".
fn alias_pairs(dictionary: &[DictionaryWord]) -> Vec<(String, &str)> {
    let mut pairs: Vec<(String, &str)> = dictionary
        .iter()
        .flat_map(|entry| {
            entry
                .aliases
                .iter()
                .map(move |alias| (alias.to_lowercase(), entry.word.as_str()))
        })
        .collect();
    pairs.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.chars().count()));
    pairs
}

/// Replaces every whole-word, case-insensitive match of an alias with its word, spelled as given.
pub(crate) fn apply_aliases(text: &str, dictionary: &[DictionaryWord]) -> String {
    let pairs = alias_pairs(dictionary);
    if pairs.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = chars.iter().flat_map(|c| c.to_lowercase()).collect();
    // Lowercasing can change the length (rare letters such as 'İ'); fall back to no aliasing then.
    if lower.len() != chars.len() {
        return text.to_string();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    'outer: while index < chars.len() {
        let at_start = index == 0 || !is_word(chars[index - 1]);
        if at_start {
            for (alias, word) in &pairs {
                let alias: Vec<char> = alias.chars().collect();
                let end = index + alias.len();
                if end <= lower.len()
                    && lower[index..end] == alias[..]
                    && (end == chars.len() || !is_word(chars[end]))
                {
                    out.push_str(word);
                    index = end;
                    continue 'outer;
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

/// The transcript without "um", "uh" and stutters, for English (or auto-detected) speech; other
/// languages keep their text, since the rules are English ones.
pub(crate) fn remove_fillers(text: &str, language: Option<&str>) -> String {
    let language = language.unwrap_or("en");
    match telmo::Fillers::for_language(language) {
        Some(fillers) if !text.trim().is_empty() => {
            let cleaned = fillers.remove(text);
            if cleaned.trim().is_empty() {
                text.trim().to_string()
            } else {
                cleaned
            }
        }
        _ => text.to_string(),
    }
}

/// Aliases, then fillers: the steps that need no model.
pub(crate) fn clean(text: &str, settings: &DictateSettings) -> String {
    let text = apply_aliases(text.trim(), &settings.dictionary);
    if settings.remove_fillers {
        remove_fillers(&text, settings.language_hint())
    } else {
        text
    }
}

/// The built-in instruction AI polish gives the model.
pub(crate) const POLISH_PROMPT: &str = "You clean up dictated text before it is typed into a chat box where the user writes to an AI coding agent.
Fix punctuation, capitalization and obvious recognition mistakes.
When the speaker corrected themselves (\"no, I mean…\", \"scratch that\", \"actually…\"), keep only the corrected version.
Keep the speaker's words, tone, language and meaning. Do not answer, summarize, shorten, translate or add anything.
Keep file names, paths, code, commands and the terms below exactly as written.
Print only the cleaned text, with no quotes and no preamble.";

/// The whole prompt: the instruction, the words to keep, and the text between markers.
pub(crate) fn polish_prompt(instruction: &str, words: &[String], text: &str) -> String {
    let mut prompt = instruction.trim().to_string();
    if !words.is_empty() {
        prompt.push_str("\n\nTerms: ");
        prompt.push_str(&words.join(", "));
    }
    prompt.push_str("\n\n<dictation>\n");
    prompt.push_str(text);
    prompt.push_str("\n</dictation>");
    prompt
}

/// The model's answer as text to insert, or `None` when it doesn't look like a cleaned-up copy
/// of the dictation (an answer, an apology, a summary).
pub(crate) fn polished_text(answer: &str, original: &str) -> Option<String> {
    let mut text = answer.trim();
    for marker in ["<dictation>", "</dictation>"] {
        text = text
            .trim_start_matches(marker)
            .trim_end_matches(marker)
            .trim();
    }
    if let Some(inner) = text.strip_prefix("```") {
        text = inner
            .trim_start_matches(|c: char| c.is_ascii_alphanumeric())
            .trim_end_matches("```")
            .trim();
    }
    let text = text.to_string();
    let original_length = original.chars().count();
    let length = text.chars().count();
    let plausible = !text.is_empty()
        && length <= original_length * 2 + 40
        && length * 2 + 20 >= original_length;
    plausible.then_some(text)
}

/// Polishes in flight, by recording id: Skip wakes the waiting transcription.
fn polishing() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Notify>>> {
    static POLISHING: std::sync::OnceLock<Mutex<HashMap<String, Arc<Notify>>>> =
        std::sync::OnceLock::new();
    POLISHING
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The reader pressed Skip: the polish of `id` stops and the cleaned text goes in. Answers
/// whether one was running.
pub(crate) fn skip_polish(id: &str) -> bool {
    match polishing().get(id) {
        Some(notify) => {
            notify.notify_one();
            true
        }
        None => false,
    }
}

/// What became of the polish.
pub(crate) enum PolishOutcome {
    Polished(String),
    Skipped,
    /// It ran and failed; the reason is for the log and the Dictation history.
    Failed(String),
    /// No Claude CLI on this computer.
    Unavailable,
}

/// How long polish may take before the cleaned text goes in without it.
const POLISH_TIMEOUT: Duration = Duration::from_secs(20);

/// Whether a Claude CLI is installed for polish to use.
pub(crate) fn polish_available(state: &AppState) -> bool {
    crate::agent_hooks::probing::command_exists("claude", &state.paths.home_dir)
}

/// Runs AI polish for the recording `id`, unless the reader skips it.
///
/// CDXC:Dictate 2026-11-05 WHY:
/// Polish runs the Claude CLI the way commit message generation and Catch me up do (`-p`, the prompt on stdin, the job's own account through `one_shot_command`, the internal environment that keeps hooks from attaching it to a session), so it works with the subscription the user already signed in with and needs no key. Effort is low because the reader is waiting for the text; the model is the user's choice (Q1273).
pub(crate) async fn polish(
    state: &AppState,
    id: &str,
    settings: &DictateSettings,
    words: &[String],
    text: &str,
) -> PolishOutcome {
    if !polish_available(state) {
        return PolishOutcome::Unavailable;
    }
    let notify = Arc::new(Notify::new());
    polishing().insert(id.to_string(), notify.clone());
    let instruction = settings.polish.prompt.as_deref().unwrap_or(POLISH_PROMPT);
    let prompt = polish_prompt(instruction, words, text);
    let outcome = tokio::select! {
        answer = run_claude(state, &settings.polish.model, prompt) => match answer {
            Ok(answer) => match polished_text(&answer, text) {
                Some(polished) => PolishOutcome::Polished(polished),
                None => PolishOutcome::Failed("The polish answer didn't look like the dictation, so it was left out.".to_string()),
            },
            Err(message) => PolishOutcome::Failed(message),
        },
        _ = notify.notified() => PolishOutcome::Skipped,
    };
    polishing().remove(id);
    outcome
}

async fn run_claude(state: &AppState, model: &str, prompt: String) -> Result<String, String> {
    let command = crate::accounts::launch::one_shot_command(state, "claude", "claude", None);
    // Read-only like the other one-shot text jobs (CDXC in agents/accept_all.rs): the dictation is text the user spoke, and polish needs no tools.
    let command = crate::agents::one_shot_text_job_command(&command, "claude");
    let shell_command = format!("{command} -p --permission-mode dontAsk --model {model} --effort low");
    let shell = crate::platform::shell::command_shell();
    let mut child = tokio::process::Command::new(&shell.executable);
    crate::platform::process::NoConsoleWindow::no_console_window(&mut child);
    child
        .args(shell.interactive_script_args(&shell_command))
        .current_dir(&state.paths.home_dir)
        .envs(crate::server::internal_prompt_generation_environment(
            &state.paths.home_dir,
        ))
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut process = child
        .spawn()
        .map_err(|error| format!("Claude couldn't start: {error}"))?;
    if let Some(mut pipe) = process.stdin.take() {
        // A CLI that exits before reading its prompt closes the pipe; its exit status says why.
        let _ = pipe.write_all(prompt.as_bytes()).await;
        drop(pipe);
    }
    let output = tokio::time::timeout(POLISH_TIMEOUT, process.wait_with_output())
        .await
        .map_err(|_| {
            format!(
                "Claude took longer than {} seconds.",
                POLISH_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| format!("Claude couldn't run: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    let said = [&output.stdout, &output.stderr]
        .into_iter()
        .flat_map(|stream| {
            String::from_utf8_lossy(stream)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .next_back()
        .map(|line| line.chars().take(200).collect::<String>())
        .unwrap_or_else(|| format!("exit code {:?}", output.status.code()));
    Err(format!("Claude couldn't polish the text: {said}"))
}
