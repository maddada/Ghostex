use super::*;

// ---------------------------------------------------------------------------
// Claude / OpenClaude grammar
//   Ctx Used: 11.0% | 13.5% | $261.54 | Fable 5 | high
// Model family and effort are independent segments, matched independently.
// ---------------------------------------------------------------------------

/// `(family segment prefix, pill value)` — mirrors the Claude models in the
/// published agent model catalog (`agent-model-catalog.json`).
const CLAUDE_MODEL_FAMILIES: &[(&str, &str)] = &[
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
    ("Haiku", "haiku"),
];

/// Rendered lowercase by the TUI; mirrors the Claude efforts in the published
/// agent model catalog (`agent-model-catalog.json`).
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultracode"];

/// `` or ` 5` or ` 4.5` — the family's optional version suffix.
fn is_model_version_suffix(rest: &str) -> bool {
    if rest.is_empty() {
        return true;
    }
    let Some(version) = rest.strip_prefix(' ') else {
        return false;
    };
    let (major, minor) = match version.split_once('.') {
        Some((major, minor)) => (major, Some(minor)),
        None => (version, None),
    };
    if major.is_empty() || !major.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    match minor {
        None => true,
        Some(minor) => !minor.is_empty() && minor.chars().all(|ch| ch.is_ascii_digit()),
    }
}

/// CDXC:AgentProviders 2026-09-29 WHY:
/// Claude Code moved the `opus` alias onto Opus 5.5 (2.1.280) and `sonnet` onto
/// Sonnet 5.5 (2.1.284), leaving Opus 5 and Sonnet 5 reachable only as
/// `claude-opus-5` and `claude-sonnet-5`. Each pair prints as "<Family>
/// <version>", so the version digits are the only thing that keeps a session
/// still on the older model from reading as the alias's row.
///
/// CDXC:AgentProviders 2026-11-09 WHY:
/// Claude Code 2.1.319 added Fable 5.2 as a row of its own (`claude-fable-5-2`)
/// while the `fable` alias still runs Fable 5.1, so a statusline id of
/// `claude-fable-5-2` read as the `fable` row and the pill showed Fable 5.1
/// picked. A pinned id the live catalog lists by its full name is that row, so
/// the next pinned model needs only a catalog edit.
pub(super) fn claude_model_value(family_value: &str, version: &str) -> String {
    if !version.is_empty() {
        let pinned = format!("claude-{family_value}-{}", version.replace('.', "-"));
        if crate::agent_model_catalog::catalog_model("claude", &pinned).is_some() {
            return pinned;
        }
    }
    match (family_value, version) {
        ("opus", "5") => "claude-opus-5",
        ("sonnet", "5") => "claude-sonnet-5",
        _ => match family_value {
            "fable" => "fable",
            "opus" => "opus",
            "sonnet" => "sonnet",
            "haiku" => "haiku",
            _ => "",
        },
    }
    .to_string()
}

pub(super) fn match_claude_model(segment: &str) -> Option<SessionChatDetectedChoice> {
    let says_long_context = segment.ends_with(" (1M)") || segment.ends_with(" (1M context)");
    // While the catalog lists both a `[1m]` row and its standard twin under one
    // label, a footer without the 1M marker is the twin, never the `[1m]` row.
    // Opus 5.5 is a single `opus[1m]` row since Claude Code 2.1.284 offers no
    // 200K Opus, so its bare "Opus 5.5" is that row.
    if let Some(value) = crate::agent_model_catalog::model_value_for_label("claude", segment)
        .filter(|value| {
            says_long_context
                || value.strip_suffix("[1m]").is_none_or(|base| {
                    crate::agent_model_catalog::catalog_model("claude", base).is_none()
                })
        })
    {
        return Some(SessionChatDetectedChoice {
            value,
            label: segment.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        });
    }
    let variant_model = segment
        .strip_suffix(" (1M)")
        .or_else(|| segment.strip_suffix(" (1M context)"));
    let model = variant_model.unwrap_or(segment);
    CLAUDE_MODEL_FAMILIES
        .iter()
        .find(|(family, _)| {
            model
                .strip_prefix(*family)
                .is_some_and(is_model_version_suffix)
        })
        .map(|(family, value)| {
            let version = model.strip_prefix(*family).unwrap_or("").trim();
            let value = claude_model_value(value, version);
            SessionChatDetectedChoice {
                value: if variant_model.is_some() {
                    format!("{value}[1m]")
                } else {
                    value.to_string()
                },
                label: segment.to_string(),
                source: SessionChatOptionEvidence::Terminal,
            }
        })
}

pub(super) fn match_claude_effort(segment: &str) -> Option<SessionChatDetectedChoice> {
    CLAUDE_EFFORTS
        .contains(&segment)
        .then(|| SessionChatDetectedChoice {
            value: segment.to_string(),
            label: segment.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        })
}

/*
Claude's bottom row is outside the custom statusline:

    ⏵⏵ bypass permissions on (shift+tab to cycle)
    ⏸ plan mode on (shift+tab to cycle)

The leading glyph pair and the complete trailing grammar are required. This
keeps ordinary prose containing "plan mode" or "manual mode" from becoming
agent-owned state merely because it appears near the bottom of the terminal.
*/
pub(super) fn match_claude_mode(segment: &str) -> Option<SessionChatDetectedChoice> {
    let status = segment
        .strip_prefix("⏵⏵ ")
        .or_else(|| segment.strip_prefix("⏸ "))?;
    let status = strip_mode_cycle_hint(status);
    let (value, label) = match status {
        "auto mode on" => ("auto", "Auto"),
        "bypass permissions on" => ("bypass", "Bypass permissions"),
        "plan mode on" => ("plan", "Plan"),
        "accept edits on" => ("accept-edits", "Accept edits"),
        "manual mode on" => ("manual", "Manual"),
        _ => return None,
    };
    Some(SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Terminal,
    })
}

// ---------------------------------------------------------------------------
// Codex grammar
//   <Title> · gpt-5.6-sol high fast · 225K used · … · Context 26% used · …
// Model + effort (+ the `fast` modifier) live in ONE segment.
// ---------------------------------------------------------------------------

/// Mirrors the Codex efforts in the published agent model catalog
/// (`agent-model-catalog.json`); `max` and `ultra` sit behind the picker's
/// "More reasoning…" row.
pub(crate) const CODEX_EFFORTS: &[&str] =
    &["minimal", "low", "medium", "high", "xhigh", "max", "ultra"];

/// `gpt-` + a digit + id characters, lowercase and case-sensitive so an
/// uppercase "GPT-5.6" in prose or a title cannot match.
fn is_codex_model_id(token: &str) -> bool {
    let Some(rest) = token.strip_prefix("gpt-") else {
        return false;
    };
    rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && rest
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
}

/// The model id a footer token names: the id itself (`gpt-6-sol`), or the
/// display name Codex 0.156 prints instead (`GPT-6-Sol`), which is the id in
/// upper case.
///
/// CDXC:AgentScreenDetection 2026-09-23 WHY:
/// Codex 0.156 moved its footer from ids to display names, which made every
/// Codex session's model unreadable. A display name only counts when its
/// lower case is a Codex model in the live catalog, so a session titled
/// "GPT-5.5" is still never read as a model, and a model pushed to the
/// catalog is recognised without a release.
fn codex_model_value(token: &str) -> Option<String> {
    if is_codex_model_id(token) {
        return Some(token.to_string());
    }
    let id = token.to_ascii_lowercase();
    (token.starts_with("GPT-")
        && is_codex_model_id(&id)
        && crate::agent_model_catalog::catalog_model("codex", &id).is_some())
    .then_some(id)
}

/// CDXC:AgentScreenDetection 2026-10-28 WHY:
/// Codex 0.165 names GPT-6.1 Terra "GPT-6.1 Terra" (a space, where every other display name is hyphenated: "GPT-6.1-Sol"), so its footer reads `GPT-6.1 Terra medium · …` and taking the first word as the model dropped the whole footer. A two-word name counts only when its hyphenated lower case is a Codex model in the live catalog, so an id followed by its effort (`gpt-6-sol high`) is never read as one name.
fn codex_footer_model(segment: &str) -> Option<(String, String, std::str::Split<'_, char>)> {
    let mut tokens = segment.split(' ');
    let first = tokens.next()?;
    let mut lookahead = tokens.clone();
    if let Some(second) = lookahead.next() {
        let id = format!("{first}-{second}").to_ascii_lowercase();
        if first.starts_with("GPT-")
            && is_codex_model_id(&id)
            && crate::agent_model_catalog::catalog_model("codex", &id).is_some()
        {
            return Some((id, format!("{first} {second}"), lookahead));
        }
    }
    Some((codex_model_value(first)?, first.to_string(), tokens))
}

/// CDXC:AgentScreenDetection 2026-10-28 WHY:
/// Codex 0.164 paints `default` where the effort goes when its config names no reasoning effort
/// (`GPT-6.1-Sol default · ~/proj`). Read as prose, the word dropped the whole footer, so every new
/// Codex session on such a config showed "Model" and no usage until its first turn wrote a
/// transcript (found testing Codex on Linux; the reader is shared by every platform). The word
/// names no effort, so none is reported and the model's own default stands.
const CODEX_DEFAULT_EFFORT_WORD: &str = "default";

/// CDXC:AgentScreenDetection 2026-11-02 WHY: Newer Codex prints "Fast" with a capital F ("GPT-6.1-Sol xhigh Fast"); an exact lowercase match dropped the whole segment, so the model and effort were lost and a pending Fast or model change blocked the queue. Effort and default words get the same case-insensitive reading so their casing cannot discard the model either.
pub(super) fn match_codex_segment(segment: &str) -> Option<SessionChatDetectedSelection> {
    let (model, label, mut tokens) = codex_footer_model(segment)?;
    let mut selection = SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model,
            label,
            source: SessionChatOptionEvidence::Terminal,
        }),
        ..SessionChatDetectedSelection::default()
    };
    let mut next = tokens.next();
    if let Some(effort) = next.and_then(|token| {
        CODEX_EFFORTS
            .iter()
            .copied()
            .find(|effort| token.eq_ignore_ascii_case(effort))
    }) {
        selection.effort = Some(SessionChatDetectedChoice {
            value: effort.to_string(),
            label: effort.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        });
        next = tokens.next();
    } else if next.is_some_and(|token| token.eq_ignore_ascii_case(CODEX_DEFAULT_EFFORT_WORD)) {
        next = tokens.next();
    }
    if next.is_some_and(|token| token.eq_ignore_ascii_case("fast")) {
        selection.fast = Some(true);
        next = tokens.next();
    }
    // Anything left over means this was prose that merely started with an id.
    next.is_none().then_some(selection)
}

/*
CDXC:AgentScreenDetection 2026-09-04 DECISION:
User: the Codex options dropdown shows a "Plan mode" row with a check mark
when Codex is in Plan mode, and the options pill carries a map icon next to
the fast bolt. Codex paints its collaboration mode right-aligned on the SAME
footer line as the model segment:

    gpt-5.6-sol high · <thread id> · Ghostex · main · … · weekly 25% left        Plan mode (shift+tab to cycle)

and paints nothing there in its default mode, so the marker is stripped off
the footer line before the segments are split (a narrow footer can put it in
the model segment itself, which would otherwise fail the exact-tokens rule)
and only counts when that line also names the model. Absence on a matched
footer means default mode: the terminal layer reports no mode, and Codex has
no transcript or statusline mode to fall back to.
*/
const CODEX_PLAN_MODE_MARKER: &str = "Plan mode";

/// CDXC:AgentScreenDetection 2026-10-17 WHY:
/// Codex 0.163 prints its mode hint as `(⇧tab to cycle)` where earlier releases printed `(shift+tab to cycle)`. Matching only the old spelling read every Plan-mode footer as default mode, so the chat showed Plan mode off while Codex was in it, `--mode default` answered ok without leaving it, and turning it on timed out waiting for a change it never saw. Any `(<key> to cycle)` hint is stripped, whatever the CLI calls the key.
pub(super) fn strip_mode_cycle_hint(text: &str) -> &str {
    text.strip_suffix(" to cycle)")
        .and_then(|rest| rest.rsplit_once(" ("))
        .filter(|(_, key)| !key.is_empty() && !key.contains(['(', ')']))
        .map_or(text, |(before, _)| before)
}

pub(super) fn strip_codex_plan_mode_marker(line: &str) -> (&str, bool) {
    let trimmed = line.trim_end();
    let without_hint = strip_mode_cycle_hint(trimmed);
    match without_hint.strip_suffix(CODEX_PLAN_MODE_MARKER) {
        Some(rest) if rest.ends_with(' ') => (rest, true),
        _ => (line, false),
    }
}

pub(super) fn codex_plan_mode_choice() -> SessionChatDetectedChoice {
    SessionChatDetectedChoice {
        value: "plan".to_string(),
        label: "Plan".to_string(),
        source: SessionChatOptionEvidence::Terminal,
    }
}

/// Codex's built-in permission modes: the chat's value, the name Codex prints for it, and the
/// profile `/permissions` takes. Codex 0.164 lists them in this order.
pub(crate) const CODEX_PERMISSION_MODES: [(&str, &str, &str); 4] = [
    ("ask-for-approval", "Ask for approval", ":ask-for-approval"),
    ("approve-for-me", "Approve for me", ":approve-for-me"),
    ("full-access", "Full Access", ":danger-full-access"),
    ("read-only", "Read Only", ":read-only"),
];

/// The chat value for a name Codex printed, or the name itself for a profile from the user's own
/// config, which the menu does not list.
pub(crate) fn codex_permissions_choice(
    label: &str,
    source: SessionChatOptionEvidence,
) -> Option<SessionChatDetectedChoice> {
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    let value = CODEX_PERMISSION_MODES
        .iter()
        .find(|(_, name, _)| *name == label)
        .map_or(label, |(value, _, _)| value);
    Some(SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source,
    })
}

/// The `/permissions` argument for a chat value, `None` for a value the chat does not offer.
pub(crate) fn codex_permissions_command(value: &str) -> Option<String> {
    CODEX_PERMISSION_MODES
        .iter()
        .find(|(known, _, _)| *known == value)
        .map(|(_, _, profile)| format!("/permissions {profile}"))
}

const CODEX_SESSION_TITLE_PREFIX: &str = ">_ OpenAI Codex (v";

/// The permission mode one line of Codex's history names, when it is one of the lines Codex
/// itself prints for it.
fn match_codex_permissions_line(line: &str) -> Option<SessionChatDetectedChoice> {
    let line = line.trim();
    if line == "permissions: YOLO mode" {
        return codex_permissions_choice("Full Access", SessionChatOptionEvidence::Terminal);
    }
    let text = line.strip_prefix("• ")?;
    let label = text
        .strip_prefix("Permission selection requested: ")
        .or_else(|| text.strip_prefix("Permissions updated to "))?;
    codex_permissions_choice(label, SessionChatOptionEvidence::Terminal)
}

/*
CDXC:AgentScreenDetection 2026-10-27 WHY:
Codex 0.164 shows its permission mode in the footer only when the user lists `permissions` or
`approval-mode` in `tui.status_line`, and records it in the transcript only at the start of each
turn (`turn_context`). Every way it changes mid-session (the `/permissions` menu, `/permissions
<profile>`, the permission-mode keys) prints `• Permission selection requested: <name>` into the
history, and a session started with `--yolo` prints `permissions: YOLO mode` under its title, so
the newest such line since the last Codex title is the current mode. The scan stops at the title so
a line an earlier Codex run printed into the same terminal never describes this one; nothing found
leaves the transcript and the launch flags to answer.
SEE-ALSO: `codex_permissions` (the Mode button) in packages/gx-chat-core/src/menus/option_catalog.rs, `drive_permissions_option` in server/src/session_chat_selection_options.rs.
*/
pub(crate) fn codex_permissions_on_screen(text: &str) -> Option<SessionChatDetectedChoice> {
    match newest_codex_permissions_sign(text)? {
        CodexPermissionsSign::Line(choice) => Some(choice),
        CodexPermissionsSign::Title => None,
    }
}

enum CodexPermissionsSign {
    Line(SessionChatDetectedChoice),
    Title,
}

/// The bottom-most permission line or Codex title on screen. The capture keeps Codex's colours,
/// which split the title's words, so every line is read without them.
fn newest_codex_permissions_sign(text: &str) -> Option<CodexPermissionsSign> {
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw));
        if line.contains(CODEX_SESSION_TITLE_PREFIX) {
            return Some(CodexPermissionsSign::Title);
        }
        if let Some(choice) = match_codex_permissions_line(&line) {
            return Some(CodexPermissionsSign::Line(choice));
        }
    }
    None
}

/// Whether the newest permission sign on a Codex screen is a title Codex drew without its
/// `permissions: YOLO mode` line. Codex adds that line under every title it draws (at start and
/// after `/clear`) exactly when the session has Full Access, so such a title rules Full Access
/// out, whatever the launch flags or an older reading said.
pub(crate) fn codex_title_rules_out_full_access(text: &str) -> bool {
    matches!(
        newest_codex_permissions_sign(text),
        Some(CodexPermissionsSign::Title)
    )
}

/// The mode a Codex `turn_context` record ran with. Built-in profiles are named by id; a record
/// without one (written before profiles existed) is read from its sandbox and approval policy.
pub(crate) fn codex_turn_context_permissions(
    payload: &Map<String, Value>,
) -> Option<SessionChatDetectedChoice> {
    let reviewer = payload.get("approvals_reviewer").and_then(Value::as_str);
    let approval = payload.get("approval_policy").and_then(Value::as_str);
    let profile = payload
        .get("active_permission_profile")
        .and_then(|profile| profile.get("id"))
        .and_then(Value::as_str);
    let sandbox = payload
        .get("sandbox_policy")
        .and_then(|policy| policy.get("type"))
        .and_then(Value::as_str);
    let label = match (profile, sandbox, approval, reviewer) {
        (Some(":workspace"), _, Some("on-request"), Some("auto_review"))
        | (None, Some("workspace-write"), Some("on-request"), Some("auto_review")) => {
            "Approve for me"
        }
        (Some(":workspace"), _, Some("on-request"), _)
        | (None, Some("workspace-write"), Some("on-request"), _) => "Ask for approval",
        (Some(":read-only"), _, Some("on-request"), _)
        | (None, Some("read-only"), Some("on-request"), _) => "Read Only",
        (Some(":danger-full-access"), _, Some("never"), _)
        | (None, Some("danger-full-access"), Some("never"), _) => "Full Access",
        // A profile from the user's own config is named by its id.
        (Some(id), _, _, _) if !id.starts_with(':') => id,
        _ => return None,
    };
    codex_permissions_choice(label, SessionChatOptionEvidence::Transcript)
}

/// The mode a Codex command line starts in, when its flags name one whole: `--yolo`,
/// `--approve-for-me`, or a sandbox together with an approval policy.
pub(crate) fn codex_launch_permissions(
    flag_values: &[(String, Option<String>)],
) -> Option<SessionChatDetectedChoice> {
    let mut sandbox = None;
    let mut approval = None;
    let mut named = None;
    for (flag, value) in flag_values {
        match (flag.as_str(), value.as_deref()) {
            ("--yolo" | "--dangerously-bypass-approvals-and-sandbox", None) => {
                named = Some("Full Access")
            }
            ("--approve-for-me", None) => named = Some("Approve for me"),
            ("-s" | "--sandbox", Some(value)) => sandbox = Some(value),
            ("-a" | "--ask-for-approval", Some(value)) => approval = Some(value),
            _ => {}
        }
    }
    let label = named.or(match (sandbox, approval) {
        (Some("read-only"), Some("on-request")) => Some("Read Only"),
        (Some("workspace-write"), Some("on-request")) => Some("Ask for approval"),
        (Some("danger-full-access"), Some("never")) => Some("Full Access"),
        _ => None,
    })?;
    codex_permissions_choice(label, SessionChatOptionEvidence::Launch)
}

// ---------------------------------------------------------------------------
// Cursor Agent grammar
//   <Title> · GPT-5.6 Sol 272K Medium · 26K used
//
// Cursor paints the current model, context window, and reasoning effort in one
// segment between the chat title and a strict token-usage segment. Model ids
// are account-dependent, so known labels map to values the client can dispatch
// and unknown labels remain honest readbacks.
// ---------------------------------------------------------------------------

fn is_cursor_usage_segment(segment: &str) -> bool {
    let Some(number) = segment.strip_suffix(" used") else {
        return false;
    };
    let number = number.strip_suffix(['K', 'M']).unwrap_or(number);
    !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

const CURSOR_EFFORT_LABELS: &[(&str, &str)] = &[
    ("Extra High", "xhigh"),
    ("xHigh", "xhigh"),
    ("XHigh", "xhigh"),
    ("Medium", "medium"),
    ("Minimal", "minimal"),
    ("Ultra", "ultra"),
    ("None", "none"),
    ("Med", "medium"),
    ("Low", "low"),
    ("High", "high"),
    ("Max", "max"),
];

/// `26K`, `1.2M` or `830` as a token count.
fn cursor_token_count(token: &str) -> Option<u64> {
    let (number, scale) = match token.strip_suffix('K') {
        Some(number) => (number, 1_000.0),
        None => match token.strip_suffix('M') {
            Some(number) => (number, 1_000_000.0),
            None => (token, 1.0),
        },
    };
    let value = number.parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then(|| (value * scale).round() as u64)
}

fn is_cursor_context_window(token: &str) -> bool {
    let number = token.strip_suffix(['K', 'M']).unwrap_or(token);
    token.len() > number.len()
        && !number.is_empty()
        && number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

fn split_cursor_model_context_and_effort(
    segment: &str,
) -> (String, Option<String>, Option<SessionChatDetectedChoice>) {
    for (label, value) in CURSOR_EFFORT_LABELS {
        let Some(before_effort) = segment.strip_suffix(label) else {
            continue;
        };
        if !before_effort.ends_with(char::is_whitespace) {
            continue;
        }
        let before_effort = before_effort.trim_end();
        if before_effort.is_empty() {
            continue;
        }
        let (model, context_window) = match before_effort.rsplit_once(' ') {
            Some((model, context_window)) if is_cursor_context_window(context_window) => {
                (model.trim_end(), Some(context_window.to_string()))
            }
            _ => (before_effort, None),
        };
        if model.is_empty() {
            continue;
        }
        return (
            model.to_string(),
            context_window,
            Some(SessionChatDetectedChoice {
                value: (*value).to_string(),
                label: (*label).to_string(),
                source: SessionChatOptionEvidence::Terminal,
            }),
        );
    }
    (segment.to_string(), None, None)
}

pub(crate) fn match_cursor_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let segments = line_segments(line);
    if segments.len() < 3 || !is_cursor_usage_segment(segments.last()?) {
        return None;
    }
    // CDXC:AgentScreenDetection 2026-11-05 WHY: Cursor ends some names with zero-width spaces
    // ("Grok 4.7  High Fast\u{200b}\u{200b}", as `--list-models` prints them), which hid the " Fast"
    // and effort suffixes, so the whole name became the model and was pinned as `--model` (#422).
    let combined: String = segments
        .get(segments.len() - 2)?
        .chars()
        .filter(|ch| !matches!(ch, '\u{200b}'..='\u{200d}' | '\u{feff}'))
        .collect();
    let combined = combined.trim();
    if combined.is_empty() || combined == "\u{2014}" {
        return None;
    }
    // CDXC:AgentScreenDetection 2026-10-17 WHY: Cursor paints "Fast" only on models that run fast
    // ("Grok 4.7 256K Medium Fast"), so a footer without it means Fast is off. Reporting nothing
    // there kept the chat's Fast mode "On" after a switch from Grok to Sonnet.
    let (combined, fast) = if let Some(without_fast) = combined
        .strip_suffix(" Fast")
        .or_else(|| combined.strip_suffix(" fast"))
        .or_else(|| combined.strip_suffix(" (Fast)"))
        .or_else(|| combined.strip_suffix(" (fast)"))
    {
        (without_fast.trim_end(), Some(true))
    } else {
        (combined, Some(false))
    };
    let (model_label, context_window, effort) = split_cursor_model_context_and_effort(combined);
    // CDXC:SessionChatDetectedOptions 2026-09-23 WHY: Cursor reports no context payload, so the
    // footer's `26K used` and the model's `272K` window are the context meter's and the Context
    // details rows' only source.
    let context_usage = SessionChatContextUsage {
        used_percentage: None,
        used_tokens: segments
            .last()
            .and_then(|usage| usage.strip_suffix(" used"))
            .and_then(cursor_token_count),
        window_size: context_window.as_deref().and_then(cursor_token_count),
    };
    // Known names map to the catalog value the client can dispatch (the live
    // catalog, so a model added to it is recognised without a release; an
    // older spelling is a row's `terminalLabels`); unknown names remain
    // honest readbacks.
    // Cursor names Claude models "Claude Fable 5" where the catalog says "Fable 5" (see
    // `cursor_row_name_matches` in session_chat_provider_model_picker.rs).
    let value = crate::agent_model_catalog::model_value_for_label("cursor", &model_label)
        .or_else(|| {
            model_label.strip_prefix("Claude ").and_then(|label| {
                crate::agent_model_catalog::model_value_for_label("cursor", label)
            })
        })
        .unwrap_or_else(|| model_label.to_string());
    let display_model_label = model_label
        .strip_prefix("Cursor ")
        .or_else(|| model_label.strip_prefix("cursor "))
        .unwrap_or(&model_label)
        .to_string();
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value,
            label: display_model_label,
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort,
        context_window,
        terminal_status_line: Some(line.trim().to_string()),
        fast,
        context_usage: (!context_usage.is_empty()).then_some(context_usage),
        claude_status: None,
        codex_status: None,
        cursor_status: None,
        model_catalog: None,
        ..SessionChatDetectedSelection::default()
    })
}

/// The mode Cursor shows on the row above its footer, and whether that row is the mode's own.
///
/// CDXC:AgentScreenDetection 2026-10-17 WHY: Cursor (2026.10.16) prints `Plan (shift+tab to
/// cycle)`, `Debug …` or `Ask …` between its composer and its footer, and nothing there in Agent
/// mode, where the composer's bottom border sits directly above the footer. Any other row (an
/// approval panel, a picker) says nothing about the mode.
pub(crate) fn match_cursor_mode_line(line: &str) -> Option<(SessionChatDetectedChoice, bool)> {
    let line = line.trim();
    let choice = |value: &str, label: &str| SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Terminal,
    };
    if !line.is_empty() && line.chars().all(|ch| ch == '\u{2580}') {
        return Some((choice("default", "Agent"), false));
    }
    let label = line.strip_suffix(" (shift+tab to cycle)")?;
    let value = match label {
        "Plan" => "plan",
        "Debug" => "debug",
        "Ask" => "search",
        _ => return None,
    };
    Some((choice(value, label), true))
}

// ---------------------------------------------------------------------------
// Grok grammar
//   ╰──────────────────────── Grok 4.6 (medium) · always-approve ─╯
// Model and effort share ONE segment, and that segment is drawn INSIDE the
// bottom border of the composer box — so the rule has to come off the line
// before it can be read at all, and before `is_divider_line` would skip it.
// ---------------------------------------------------------------------------

/// The values grok's model catalog offers (`reasoning_efforts` in
/// `~/.grok/models_cache.json`); mirrors the Grok efforts in the published
/// agent model catalog (`agent-model-catalog.json`).
const GROK_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

/// Box-drawing runs are chrome, not content: fold them to spaces so the
/// statusline drawn on a border reads like any other line.
pub(super) fn strip_box_drawing(line: &str) -> String {
    line.chars()
        .map(|ch| {
            if matches!(ch, '\u{2500}'..='\u{257f}') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

/// `Grok 4.6 (medium)`, or `Grok 4.6` on a model with no reasoning effort.
/// Anything else in the parentheses means this was not the statusline.
pub(super) fn match_grok_segment(segment: &str) -> Option<SessionChatDetectedSelection> {
    let (name, effort) = match segment.split_once('(') {
        None => (segment.trim(), None),
        Some((name, rest)) => (name.trim(), Some(rest.strip_suffix(')')?.trim())),
    };
    // Grok 1.0.40 added "Grok 4.7 Fast", whose id in `models_cache.json` is
    // `grok-4.7-build-fast` rather than the name lowercased.
    let (base, fast) = name
        .strip_suffix(" Fast")
        .map_or((name, false), |base| (base, true));
    let catalog_value = crate::agent_model_catalog::model_value_for_label("grok", name);
    if catalog_value.is_none()
        && !base
            .strip_prefix("Grok")
            .is_some_and(is_model_version_suffix)
    {
        return None;
    }
    let effort = match effort {
        None => None,
        Some(effort) => {
            Some(
                GROK_EFFORTS
                    .contains(&effort)
                    .then(|| SessionChatDetectedChoice {
                        value: effort.to_string(),
                        label: effort.to_string(),
                        source: SessionChatOptionEvidence::Terminal,
                    })?,
            )
        }
    };
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            // The catalog's id for the name, else the id derived from it
            // (`Grok 4.6` ⇒ `grok-4.6`), which is what grok's own
            // `models_cache.json` keys models by.
            value: catalog_value.unwrap_or_else(|| {
                format!(
                    "{}{}",
                    base.to_ascii_lowercase().replace(' ', "-"),
                    if fast { "-build-fast" } else { "" }
                )
            }),
            label: name.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort,
        mode: None,
        permissions: None,
        permissions_recorded_at: None,
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
        codex_status: None,
        cursor_status: None,
        hermes_status: None,
        pi_status: None,
        checkout_status: None,
        model_catalog: None,
    })
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// Grok Build's Shift+Tab cycles default → plan → auto-review → always-approve (1.0.62), and the composer border names every mode but the default after the model (`Grok 4.7 (high) · plan`), so a model segment with no mode beside it is the default mode. The chat's Mode pill steps the cycle from this reading.
/// SEE-ALSO: `grok_mode` in packages/gx-chat-core/src/menus/option_catalog.rs (the same values, in cycle order).
pub(super) fn grok_mode_choice(segments: &[String]) -> SessionChatDetectedChoice {
    let value = segments
        .iter()
        .map(|segment| segment.trim())
        .find(|segment| matches!(*segment, "plan" | "auto-review" | "always-approve"))
        .unwrap_or("default");
    SessionChatDetectedChoice {
        value: value.to_string(),
        label: value.to_string(),
        source: SessionChatOptionEvidence::Terminal,
    }
}

// ---------------------------------------------------------------------------
// Antigravity CLI grammar
//   ? for shortcuts                                    Gemini 3.8 Flash · high
//   Gemini 3.8 Flash (High)                       (startup banner, same values)
//
// CDXC:AgentScreenDetection 2026-09-03: the footer's right edge is
// `<model> · <effort>` for the Gemini rows, whose ids are model and effort
// flattened (`gemini-3.8-flash-high`, see `agy models`), and a bare `<model>`
// for the rows without an effort slider. The pill values are the catalog's
// model part, exactly what `antigravity_model_command` re-flattens when it types
// `/model` (packages/gx-chat-core/src/menus/option_catalog.rs). Only names
// the catalog knows are accepted, so a prose line that ends in `· high` never
// becomes state.
// ---------------------------------------------------------------------------

/// CDXC:AgentScreenDetection 2026-11-13 WHY:
/// Antigravity 1.5.6's Gemini 4 Hydrogen adds an Extra High stop (`gemini-4-hydrogen-xhigh`, footer `· xhigh`).
const ANTIGRAVITY_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

/// The footer names a non-default mode in front of the model (`plan · Gemini 3.8 Flash · high`).
/// The values match the mode pill's choices in `packages/gx-chat-core/src/menus/option_catalog.rs`.
///
/// CDXC:AgentScreenDetection 2026-10-17 WHY:
/// Antigravity CLI 1.4.0 added `auto` (edits and safe commands auto-approved) to its Shift+Tab cycle, which is now default → accept-edits → auto → plan. The chat counted presses through the old three-mode order, so picking Plan left agy in `auto` while the pill said Plan, and an unknown `auto` footer read as default. gxserver now presses once at a time and reads this footer back until it names the mode that was asked for (`drive_mode_option`), so a mode agy adds later is never landed on by mistake.
const ANTIGRAVITY_DEFAULT_MODE: (&str, &str) = ("default", "Default");
const ANTIGRAVITY_MODES: &[(&str, &str)] = &[
    ("accept-edits", "Accept edits"),
    ("auto", "Auto"),
    ("plan", "Plan"),
];

/// `Gemini 3.8 Flash` ⇒ `gemini-3.8-flash`; `Gemini 3.1 Pro` ⇒ `gemini-3.1-pro`.
///
/// The live catalog answers first: `agy models` folds a fixed reasoning mode
/// into some ids (`GPT-OSS 120B` ⇒ `gpt-oss-120b-medium`), so only a
/// Gemini name not in it yet is derived.
fn antigravity_model_id(name: &str) -> Option<String> {
    if let Some(value) = crate::agent_model_catalog::model_value_for_label("antigravity", name) {
        return Some(value);
    }
    let rest = name.strip_prefix("Gemini")?;
    let (version, tier) = rest.trim_start().split_once(' ')?;
    if !is_model_version_suffix(&format!(" {version}")) || !matches!(tier, "Flash" | "Pro") {
        return None;
    }
    Some(name.to_ascii_lowercase().replace(' ', "-"))
}

/// The model name is right-aligned after the shortcut hint, so it is the text
/// after the last run of two or more spaces (or the whole segment when the
/// footer has nothing on its left).
fn antigravity_trailing_name(segment: &str) -> &str {
    let trimmed = segment.trim();
    match trimmed.rfind("  ") {
        Some(index) => trimmed[index..].trim(),
        None => trimmed,
    }
}

/// CDXC:AgentScreenDetection 2026-10-11 WHY:
/// While background tasks run, Antigravity CLI (measured on 1.3.3) appends their count and the command that lists them to the footer: `Gemini 3.6 Flash · medium · 1 task(s) · /tasks`. Read as part of the selection, the footer named no model, so a chat model change could never be confirmed and the messages queued behind it waited for good. Those trailing segments are dropped before the model and effort are read.
fn antigravity_footer_extra(segment: &str) -> bool {
    let counter = segment.split_once(' ').is_some_and(|(count, noun)| {
        !count.is_empty()
            && count.bytes().all(|byte| byte.is_ascii_digit())
            && noun.ends_with("(s)")
    });
    counter || (segment.starts_with('/') && !segment.contains(' '))
}

pub(super) fn match_antigravity_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let mut segments = line_segments(line);
    while segments.len() > 1
        && segments
            .last()
            .is_some_and(|last| antigravity_footer_extra(last))
    {
        segments.pop();
    }
    let (effort, rest) = match segments.split_last() {
        Some((last, rest))
            if !rest.is_empty()
                && ANTIGRAVITY_EFFORTS.contains(&last.to_ascii_lowercase().as_str()) =>
        {
            (Some(last.to_ascii_lowercase()), rest)
        }
        _ => (None, segments.as_slice()),
    };
    let (model, before) = rest.split_last()?;
    let mode = before
        .last()
        .map(|segment| antigravity_trailing_name(segment))
        .and_then(|name| {
            ANTIGRAVITY_MODES
                .iter()
                .find(|(value, _)| *value == name)
                .copied()
        });
    if effort.is_none() && !before.is_empty() && mode.is_none() {
        return None;
    }
    let (name, effort, banner) = match (effort, before.is_empty()) {
        (None, true) => match model
            .trim_end()
            .strip_suffix(')')
            .and_then(|rest| rest.rsplit_once(" ("))
        {
            // Startup banner: `Gemini 3.8 Flash (High)`. A model without an effort slider prints
            // its whole name that way in the footer too (`GPT-OSS 120B (Medium)`, its
            // `pickerLabel`), and that footer still means default mode.
            Some((name, effort))
                if ANTIGRAVITY_EFFORTS.contains(&effort.to_ascii_lowercase().as_str())
                    && crate::agent_model_catalog::model_value_for_label(
                        "antigravity",
                        antigravity_trailing_name(model),
                    )
                    .is_none() =>
            {
                (
                    antigravity_trailing_name(name),
                    Some(effort.to_ascii_lowercase()),
                    true,
                )
            }
            _ => (antigravity_trailing_name(model), None, false),
        },
        (effort, _) => (antigravity_trailing_name(model), effort, false),
    };
    // The banner says nothing about the mode; the footer leaves the default one unnamed.
    let mode = (!banner).then(|| mode.unwrap_or(ANTIGRAVITY_DEFAULT_MODE));
    let value = antigravity_model_id(name)?;
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value,
            label: name.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort: effort.map(|effort| SessionChatDetectedChoice {
            label: effort.clone(),
            value: effort,
            source: SessionChatOptionEvidence::Terminal,
        }),
        mode: mode.map(|(value, label)| SessionChatDetectedChoice {
            value: value.to_string(),
            label: label.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        permissions: None,
        permissions_recorded_at: None,
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
        codex_status: None,
        cursor_status: None,
        hermes_status: None,
        pi_status: None,
        checkout_status: None,
        model_catalog: None,
    })
}

// ---------------------------------------------------------------------------
// Pi grammar
//   0.0%/300k (auto)              claude-fable-5@300k • medium
//   0.0%/256k (auto)          (openai-codex) gpt-5.5 • thinking off
//
// Pi's statusline is configurable, so require both pieces from the measured
// default layout: a context meter on the left and the model, right-aligned
// after at least two spaces. This keeps a prose line that happens to mention
// `model • medium` from becoming state.
// ---------------------------------------------------------------------------

/// Pi's thinking levels (`EXTENDED_THINKING_LEVELS`), which OMP's statusline shares.
const PI_FAMILY_EFFORTS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const OMP_MIN_HEADER_RULE_CHARS: usize = 20;

fn is_pi_context_meter(token: &str) -> bool {
    let Some((used, total)) = token.split_once('/') else {
        return false;
    };
    let Some(used) = used.strip_suffix('%') else {
        return false;
    };
    if used.parse::<f64>().is_err() {
        return false;
    }
    let total = total.strip_suffix(['k', 'K', 'm', 'M']).unwrap_or(total);
    !total.is_empty() && total.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

fn is_pi_family_model_id(token: &str) -> bool {
    !token.is_empty()
        && token.chars().any(|ch| ch.is_ascii_alphanumeric())
        && token.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/' | '@' | '+')
        })
}

fn pi_family_selection(
    model: String,
    label: String,
    effort: Option<String>,
) -> SessionChatDetectedSelection {
    SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model,
            label,
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort: effort.map(|effort| SessionChatDetectedChoice {
            value: effort.clone(),
            label: effort,
            source: SessionChatOptionEvidence::Terminal,
        }),
        ..SessionChatDetectedSelection::default()
    }
}

/// CDXC:AgentProviders 2026-09-30 WHY:
/// Pi's footer prepends `(<provider>)` to the model once more than one provider is logged in (a Codex login next to the Cursor extension, for example), prints `• thinking off` for the off level and no level at all for a model without reasoning (`FooterComponent` in pi-coding-agent 0.87). The model is the part after the right-aligning padding, so Pi's own `(auto)` compaction marker on the left is never read as a provider. With the provider shown the model reads `provider/id`, the picker's own key.
pub(super) fn match_pi_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let (stats, right) = line.trim_end().rsplit_once("  ")?;
    if !stats.split_whitespace().any(is_pi_context_meter) {
        return None;
    }
    let tokens = right.split_whitespace().collect::<Vec<_>>();
    let (head, effort) = match tokens.iter().position(|token| *token == "•") {
        Some(bullet) => {
            let effort = match &tokens[bullet + 1..] {
                [level] => level.to_ascii_lowercase(),
                ["thinking", "off"] => "off".to_string(),
                _ => return None,
            };
            if !PI_FAMILY_EFFORTS.contains(&effort.as_str()) {
                return None;
            }
            (&tokens[..bullet], Some(effort))
        }
        None => (&tokens[..], None),
    };
    let (provider, model) = match head {
        [model] => (None, *model),
        [provider, model] => {
            let provider = provider.strip_prefix('(')?.strip_suffix(')')?;
            (Some(provider), *model)
        }
        _ => return None,
    };
    if !is_pi_family_model_id(model) || provider.is_some_and(|p| !is_pi_family_model_id(p)) {
        return None;
    }
    let value = provider.map_or_else(|| model.to_string(), |p| format!("{p}/{model}"));
    Some(pi_family_selection(value, model.to_string(), effort))
}

// ---------------------------------------------------------------------------
// DeepSeek Harness grammar
//    deepseek-flash · max · cache 98.3% · ≈¥0.02 peak · repo        (chat page)
//    deepseek-flash  ·  Max  ·  standard  ·  Workspace read/write    (launchpad)
// ---------------------------------------------------------------------------

/// The reasoning levels dsh-tui's DeepSeek routes take (`/effort off|low|high|max`).
pub(crate) const DSH_EFFORTS: &[&str] = &["off", "low", "high", "max"];

/// CDXC:AgentProviders 2026-10-28 WHY:
/// dsh-tui 0.17 prints the route's model id, then its reasoning level, as the first two ` · ` fields of its status line, below its input box: lower-case on the chat page, capitalised (with wider spacing) on the launchpad. The line names no provider, so the id maps to the catalog's row through `terminalLabels`, which only the API-key rows carry; an account session's pill shows the same model under the API-key row's name.
pub(super) fn match_dsh_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let fields: Vec<&str> = line.trim().split('\u{b7}').map(str::trim).collect();
    let [model, effort, _, ..] = fields.as_slice() else {
        return None;
    };
    let effort = effort.to_ascii_lowercase();
    if !DSH_EFFORTS.contains(&effort.as_str())
        || model.contains(char::is_whitespace)
        || !is_pi_family_model_id(model)
    {
        return None;
    }
    let value = crate::agent_model_catalog::model_value_for_label("dsh", model)
        .unwrap_or_else(|| model.to_string());
    Some(pi_family_selection(value, model.to_string(), Some(effort)))
}

// ---------------------------------------------------------------------------
// Omp grammar
//   ╭── π > ⬢ GPT-5.6-Sol · ◒ high > … ▶────────────────────────╮
//   ╭── π > ◒ GLM-5.2 > 📁 ~/dev/repo > ⑂ main > … ▶──1.0%──1M─◀ title ──╮
//
// Both values live on the rounded composer head. Require that complete chrome
// plus Omp's glyph labels so ordinary terminal output cannot match.
// ---------------------------------------------------------------------------

/// OMP's thinking glyphs (pi-tui's unicode `thinking.*` symbols and `status.disabled` for off),
/// with the level each stands for; `None` is `⟳`, auto thinking before its classifier resolves.
const OMP_THINKING_GLYPHS: &[(char, Option<&str>)] = &[
    ('\u{25cb}', Some("minimal")),
    ('\u{25d4}', Some("low")),
    ('\u{25d1}', Some("medium")),
    ('\u{25d2}', Some("high")),
    ('\u{25d5}', Some("xhigh")),
    ('\u{25c9}', Some("max")),
    ('\u{27f3}', None),
    ('\u{29b8}', Some("off")),
];

/// CDXC:AgentProviders 2026-10-17 WHY:
/// OMP's model segment is `⬢ <name>` then ` · <glyph> <level>`, where the glyph and label vary by level (`○ min`, `◔ low`, `◑ med`, `◒ high`, `◕ xhigh`, `◉ max`, `off` behind the disabled glyph, `auto` while its classifier decides; pi-tui's `thinking.*` symbols). With `compactThinkingLevel` on (what omp 18.11 draws here) the level's glyph replaces `⬢` and the ` · <level>` tail is gone (`π > ◒ GLM-5.2 > …`), and reading only the `⬢` form left every model pick from chat unconfirmed and retyped `/switch` forever. The name can hold spaces and is followed by the fast and advisor icons, so the segment is cut at OMP's ` > ` separator and trailing icon tokens are dropped.
pub(super) fn match_omp_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let trimmed = line.trim();
    if !trimmed.starts_with('\u{256d}')
        || !trimmed.ends_with('\u{256e}')
        || trimmed.chars().filter(|ch| *ch == '\u{2500}').count() < OMP_MIN_HEADER_RULE_CHARS
    {
        return None;
    }
    let mut segments = trimmed.split(" > ");
    if !segments
        .next()?
        .split_whitespace()
        .any(|token| token == "\u{3c0}")
    {
        return None;
    }
    let (glyph, rest) = segments.next()?.split_once(' ')?;
    let mut glyph_chars = glyph.chars();
    let glyph = glyph_chars.next()?;
    if glyph_chars.next().is_some() {
        return None;
    }
    let (name, effort) = if glyph == '\u{2b22}' {
        let (name, thinking) = match rest.rsplit_once(" \u{b7} ") {
            Some((name, thinking)) => (name, Some(thinking)),
            None => (rest, None),
        };
        let effort = match thinking {
            Some(thinking) => {
                let level = thinking.split_whitespace().last()?.to_ascii_lowercase();
                match level.as_str() {
                    "min" => Some("minimal".to_string()),
                    "med" => Some("medium".to_string()),
                    // The level its classifier resolves to shows once a turn has run.
                    "auto" => None,
                    level if PI_FAMILY_EFFORTS.contains(&level) => Some(level.to_string()),
                    _ => return None,
                }
            }
            None => None,
        };
        (name, effort)
    } else {
        let (_, level) = OMP_THINKING_GLYPHS
            .iter()
            .find(|(candidate, _)| *candidate == glyph)?;
        (rest, level.map(str::to_string))
    };
    let name_tokens = name.split_whitespace().collect::<Vec<_>>();
    let keep = name_tokens
        .iter()
        .rposition(|token| token.chars().any(|ch| ch.is_ascii_alphanumeric()))?;
    let name = name_tokens[..=keep].join(" ");
    // `no-model` is what OMP draws while no provider is logged in.
    if name == "no-model" || !name.split(' ').all(is_pi_family_model_id) {
        return None;
    }
    Some(pi_family_selection(name.clone(), name, effort))
}

// ---------------------------------------------------------------------------
// Empryo grammar
//   󰧑 Subscriptions/gpt-6-luna [medium] ⌁ ⎇ master ⌁ mcp 4        … ⌁ ctrl+k 󰩟
//   󰉋 repo          󰧑 OpenCode Go/glm-5.2 [high] ⌁ ⎇ master   (header, while a side panel is open)
// ---------------------------------------------------------------------------

/// CDXC:AgentScreenDetection 2026-10-06 WHY:
/// Empryo 3.9.0-beta prints the model as `<provider name>/<model id>` (the name `--list-models` heads each provider with, which may hold spaces) after its provider icon, then the tab's effort in brackets, as the first `⌁`-separated segment of its statusline; a side panel cuts that segment off the footer and Empryo repeats it in its header row instead. The provider name maps back to the picker's `provider/id` through the lineup's `terminalLabels`.
pub(super) fn match_empryo_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let (head, _) = line.split_once('\u{2301}')?;
    // The text after the last icon glyph (Nerd Font private-use characters).
    let segment = head.rsplit(is_nerd_font_icon).next()?.trim();
    let (name, effort) = match segment.rsplit_once(" [") {
        Some((name, level)) => {
            let level = level.strip_suffix(']')?;
            if !PI_FAMILY_EFFORTS.contains(&level) && !matches!(level, "none" | "auto") {
                return None;
            }
            (name.trim(), (level != "auto").then(|| level.to_string()))
        }
        None => (segment, None),
    };
    let (provider, model) = name.rsplit_once('/')?;
    if provider.trim().is_empty() || provider.contains(['[', ']']) || !is_pi_family_model_id(model)
    {
        return None;
    }
    Some(pi_family_selection(
        name.to_string(),
        name.to_string(),
        effort,
    ))
}

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// Empryo 3.9.1-beta moved the model off its statusline onto the input box's top border (`╭─ OpenAI-sub/GPT-6 Luna · ● medium · YOLO ──── 󰑮 Cache idle ─╮`): `<vendor>/<display name>` (only the name when the box is narrow), then ` · `-separated segments for the effort (a level glyph and its name, the glyph alone while the tab keeps the model's default effort), the mode, `as <agent>` and `YOLO`. The caller passes only the input box's own top border, since Empryo's panels are rounded boxes too. The display name maps back to the picker's `provider/id` in `pi_family_catalog_value`.
pub(super) fn match_empryo_input_head(line: &str) -> Option<SessionChatDetectedSelection> {
    let head = line
        .trim()
        .strip_prefix('\u{256d}')?
        .trim_start_matches('\u{2500}')
        .trim_start();
    let head = head.split(" \u{2500}").next()?.trim();
    let mut segments = head.split(" \u{b7} ").map(str::trim);
    let name = segments.next().filter(|name| !name.is_empty())?;
    let effort = segments.find_map(|segment| {
        let (glyph, level) = segment.split_once(' ')?;
        (glyph.chars().count() == 1 && (PI_FAMILY_EFFORTS.contains(&level) || level == "none"))
            .then(|| level.to_string())
    });
    Some(SessionChatDetectedSelection {
        terminal_status_line: Some(head.to_string()),
        ..pi_family_selection(name.to_string(), name.to_string(), effort)
    })
}

// ---------------------------------------------------------------------------
// Hermes grammar
//   ⚕ grok-4.6 │ ctx -- │ [░░░░░░░░░░] -- │ 34s │ ⏲ 0s
//   ☤ gpt-6-sol │ ~26.2K/900K pinned │ [█░░░░░░░░░] ~3% │ ◎ 99.3% │ 42m │ ⏱ 12s
//
// The model is the first `│` segment: one single-glyph marker, then the id
// (measured 2026-08-29, Hermes Agent v0.20.4), keeping the variant tag of an
// id Hermes resolved to (`claude-opus-5-5[1m]`). The timer segment (`⏲` idle,
// `⏱` while a turn runs) plus the segment count keep prose from matching; the
// context segment cannot anchor anything because it changes shape after the
// first exchange (`ctx --` becomes `26.2K/900K`). No reasoning effort is drawn
// anywhere on screen.
// ---------------------------------------------------------------------------

/// CDXC:AgentScreenDetection 2026-09-26 WHY:
/// Hermes reports context use only on screen: `USED/WINDOW` (a leading `~` marks an estimate, a trailing `pinned` a fixed window) and the bar's rounded `N%`, which the meter shows as Hermes prints it.
fn hermes_context_usage(segments: &[&str]) -> Option<SessionChatContextUsage> {
    let (used_tokens, window_size) = segments
        .iter()
        .find_map(|segment| {
            let segment = segment.trim_start_matches('~');
            let (used, window) = segment
                .strip_suffix(" pinned")
                .unwrap_or(segment)
                .split_once('/')?;
            Some((cursor_token_count(used)?, cursor_token_count(window)?))
        })
        .unzip();
    let used_percentage = segments.iter().find_map(|segment| {
        let (bar, percent) = segment.rsplit_once(' ')?;
        if !bar.starts_with('[') || !bar.ends_with(']') {
            return None;
        }
        percent
            .trim_start_matches('~')
            .strip_suffix('%')?
            .parse()
            .ok()
    });
    let usage = SessionChatContextUsage {
        used_percentage,
        used_tokens,
        window_size,
    };
    (!usage.is_empty()).then_some(usage)
}

/// Hermes's status bar (`☤ model │ … │ ⏲ 3s │ …`), which it repaints under its output.
pub(crate) fn is_hermes_statusline(line: &str) -> bool {
    line.contains('\u{2502}') && match_hermes_statusline(line).is_some()
}

/// The hint Hermes paints above its status bar while a slash command runs.
pub(crate) fn is_hermes_busy_hint(line: &str) -> bool {
    line.contains("command in progress · ")
}

pub(super) fn match_hermes_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let segments: Vec<&str> = line.split('\u{2502}').map(str::trim).collect();
    if segments.len() < 4
        || !segments[1..]
            .iter()
            .any(|segment| segment.starts_with(['\u{23f2}', '\u{23f1}']))
    {
        return None;
    }
    let mut head = segments[0].split_whitespace();
    let marker = head.next()?;
    let model = head.next()?;
    if head.next().is_some()
        || marker.chars().count() != 1
        || marker
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric())
        || !is_pi_family_model_id(crate::session_chat_hermes_status::hermes_untagged_model(
            model,
        ))
    {
        return None;
    }
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model.to_string(),
            label: model.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        context_usage: hermes_context_usage(&segments[1..]),
        ..SessionChatDetectedSelection::default()
    })
}
