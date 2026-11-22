//! A hub that has no account left with room answers 429 "All credentials for model … are cooling
//! down". Claude Code shows it as "API Error: Request rejected (429) · All credentials for model …
//! are cooling down (reset after 1h10m0s)"; Codex shows the hub's JSON body, whose
//! `reset_seconds` and `reset_time` say the same. This reads how long until the first account has
//! room again, so the session's limit wait continues it then.

use chrono::Duration;

/// Whether `text` is the hub's every-account-cooling-down refusal.
pub(crate) fn is_cooldown(text: &str) -> bool {
    text.contains("All credentials for model") && text.contains("cooling down")
}

/// How long until the hub has an account with room again, from its refusal: `reset after <Go
/// duration>` (Claude), else `"reset_seconds":<n>` or `"reset_time":"<Go duration>"` (the JSON body
/// Codex shows). `None` when the text names no reset.
pub(crate) fn reset_in(text: &str) -> Option<Duration> {
    if !is_cooldown(text) {
        return None;
    }
    if let Some(at) = text.find("reset after ") {
        let rest = &text[at + "reset after ".len()..];
        let end = rest
            .find(|c: char| {
                !(c.is_ascii_digit() || matches!(c, '.' | 'h' | 'm' | 's' | 'µ' | 'u' | 'n'))
            })
            .unwrap_or(rest.len());
        if let Some(duration) = go_duration(&rest[..end]) {
            return Some(duration);
        }
    }
    if let Some(at) = text.find("\"reset_seconds\":") {
        let digits: String = text[at + "\"reset_seconds\":".len()..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(seconds) = digits.parse::<i64>() {
            return Some(Duration::seconds(seconds));
        }
    }
    let at = text.find("\"reset_time\":\"")?;
    let rest = &text[at + "\"reset_time\":\"".len()..];
    go_duration(&rest[..rest.find('"')?])
}

/// A Go `time.Duration` string (`2h0m0s`, `45m30s`, `1.5s`, `300ms`), rounded up to whole seconds.
pub(crate) fn go_duration(text: &str) -> Option<Duration> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    let mut total = 0.0_f64;
    while !rest.is_empty() {
        let number_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .filter(|end| *end > 0)?;
        let value: f64 = rest[..number_end].parse().ok()?;
        rest = &rest[number_end..];
        let (unit, seconds) = [
            ("ms", 0.001),
            ("µs", 1e-6),
            ("us", 1e-6),
            ("ns", 1e-9),
            ("h", 3600.0),
            ("m", 60.0),
            ("s", 1.0),
        ]
        .into_iter()
        .find(|(unit, _)| rest.starts_with(unit))?;
        total += value * seconds;
        rest = &rest[unit.len()..];
    }
    Some(Duration::seconds(total.ceil() as i64))
}

/// When a session stopped by a usage limit should be continued, if the limit is a hub's: the reset
/// its refusal names (the card's quoted line, then its screen tail, whose lines a narrow terminal
/// may have wrapped), else the hub pool's next free account for a session on hub `hub_id`. A
/// half-minute margin lets the hub take the account back first.
pub(crate) fn reset_after_notice(
    notice: &crate::session_chat_notice::SessionChatTerminalNotice,
    hub_id: Option<&str>,
    provider: crate::accounts::model::Provider,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let margin = Duration::seconds(30);
    let tail = notice.screen_tail.as_deref().unwrap_or_default();
    let texts = [
        notice.details.clone().unwrap_or_default(),
        tail.lines().map(str::trim).collect::<Vec<_>>().join(" "),
        tail.lines().map(str::trim).collect::<String>(),
    ];
    if let Some(wait) = texts.iter().find_map(|text| reset_in(text)) {
        return Some(now + wait + margin);
    }
    let from_hub = texts.iter().any(|text| is_cooldown(text));
    if !from_hub && hub_id.is_none() {
        return None;
    }
    super::pool_free_at(hub_id?, provider).map(|at| at.max(now) + margin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_reset_from_each_shape_of_the_refusal() {
        assert_eq!(
            reset_in(
                "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (reset after 1h10m0s)"
            ),
            Some(Duration::seconds(4200))
        );
        assert_eq!(
            reset_in(
                "All credentials for model claude-sonnet-5-5 are cooling down via provider claude (reset after 2h)"
            ),
            Some(Duration::seconds(7200))
        );
        assert_eq!(
            reset_in("All credentials for model x are cooling down (reset after 1.5s)"),
            Some(Duration::seconds(2))
        );
        assert_eq!(
            reset_in(
                r#"unexpected status 429 Too Many Requests: {"error":{"code":"model_cooldown","message":"All credentials for model gpt-5.5-codex are cooling down via provider codex","model":"gpt-5.5-codex","reset_time":"45m30s","reset_seconds":2730}}"#
            ),
            Some(Duration::seconds(2730))
        );
        assert_eq!(
            reset_in(
                r#"{"error":{"message":"All credentials for model m are cooling down","reset_time":"3m0s"}}"#
            ),
            Some(Duration::seconds(180))
        );
        // The refusal without a reset, and someone else's 429.
        assert_eq!(
            reset_in("All credentials for model m are cooling down"),
            None
        );
        assert_eq!(
            reset_in("API Error: Request rejected (429) · reset after 5m"),
            None
        );
    }

    /// The screen a Claude on a hub shows (its words as Claude Code 2.1 prints them), wrapped as a
    /// narrow terminal wraps it: a usage limit that continues after the reset it names.
    #[test]
    fn a_claude_on_a_cooling_hub_waits_for_the_named_reset() {
        let screen = "\
> say hi
  ⎿  API Error: Request rejected (429) · All credentials for model claude-sonnet-4-5 are cooling down via
     provider claude (last error: {\"type\": \"error\", \"error\": {\"type\": \"rate_limit_error\", \"message\": \"This
     request would exceed your account's rate limit.\"}}) (reset after 1h0m21s)

────────────────────────────────────────────────────────────────────────────────
> 
────────────────────────────────────────────────────────────────────────────────
  ? for shortcuts
";
        let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
            Some("claude"),
            screen,
        )
        .expect("a notice");
        assert_eq!(notice.kind, "usageLimit");
        let now = chrono::Utc::now();
        let at = reset_after_notice(&notice, None, crate::accounts::model::Provider::Claude, now)
            .expect("a reset");
        assert_eq!((at - now).num_seconds(), 3621 + 30);
    }
}
