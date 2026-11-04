//! Claude Code's Bypass Permissions warning, consented to on the launch of a Claude session that
//! already starts in bypass.

use serde_json::json;

use super::quote_shell_arg;

/// Claude Code's own record of a user accepting its Bypass Permissions warning.
pub(crate) const CLAUDE_BYPASS_CONSENT_SETTING: &str = "skipDangerousModePermissionPrompt";

/// CDXC:AgentProviders 2026-10-31 DECISION:
/// User: "please when u see this warning just accept it automatically for the user" (Claude Code's "WARNING: Claude Code running in Bypass Permissions mode" screen, which every cswap account's config folder asks once and which blocks a session Ghostex started in bypass). A Claude command that starts in bypass (`--dangerously-skip-permissions`, or `--permission-mode bypassPermissions`, from Accept All or the user's own command) is given `--settings '{"skipDangerousModePermissionPrompt":true}'` on this run only, so the warning never paints and no shared config file is written while other sessions run on it. Nothing here turns bypass on: a command that does not already start in bypass is returned unchanged, and so is one with `--allow-dangerously-skip-permissions` alone, which starts in the default mode.
/// WHY: Claude Code 2.1.311 and 2.1.312 skip the warning when that key is true in user, local, flag (`--settings`) or policy settings, and accepting the warning writes the same key into the account's `settings.json`; a flag consent does not count only for `--bg` workers, which Ghostex does not start. Claude keeps only the last `--settings`, so a command that already names one is left alone here (the Connectors flag adds the key to its own JSON) and the on-screen answer in session_chat_claude_bypass_consent.rs is the backstop.
/// SEE-ALSO: server/src/connectors/cli/launch.rs (`with_env`), server/src/agents/launch_plan.rs, server/src/agents/resume_plan.rs, server/src/session_chat_claude_bypass_consent.rs.
pub(crate) fn with_claude_bypass_consent(family: Option<&str>, command: &str) -> String {
    if family != Some("claude")
        || command.contains("--settings")
        || !claude_command_starts_in_bypass(command)
    {
        return command.to_string();
    }
    let settings = json!({ CLAUDE_BYPASS_CONSENT_SETTING: true }).to_string();
    format!(
        "{} --settings {}",
        command.trim_end(),
        quote_shell_arg(&settings)
    )
}

/// Whether a Claude command line starts the agent in Bypass Permissions mode.
pub(crate) fn claude_command_starts_in_bypass(command: &str) -> bool {
    let tokens: Vec<&str> = command
        .split_whitespace()
        .map(|token| token.trim_matches(['"', '\'']))
        .collect();
    tokens.iter().enumerate().any(|(index, token)| {
        *token == "--dangerously-skip-permissions"
            || token.strip_prefix("--permission-mode=") == Some("bypassPermissions")
            || (*token == "--permission-mode"
                && tokens
                    .get(index + 1)
                    .is_some_and(|value| *value == "bypassPermissions"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_launches_get_the_consent_flag() {
        for command in [
            "claude --dangerously-skip-permissions",
            "claude --model opus --permission-mode bypassPermissions",
            "claude --permission-mode=bypassPermissions",
            "cswap run 2 --share-history -- --dangerously-skip-permissions --model opus",
        ] {
            assert_eq!(
                with_claude_bypass_consent(Some("claude"), command),
                format!(r#"{command} --settings '{{"skipDangerousModePermissionPrompt":true}}'"#),
                "{command}"
            );
        }
    }

    #[test]
    fn other_launches_are_left_alone() {
        for (family, command) in [
            (Some("claude"), "claude"),
            (Some("claude"), "claude --permission-mode acceptEdits"),
            (Some("claude"), "claude --permission-mode plan"),
            (
                Some("claude"),
                "claude --allow-dangerously-skip-permissions",
            ),
            (
                Some("claude"),
                r#"claude --dangerously-skip-permissions --settings '{"env":{}}'"#,
            ),
            (Some("codex"), "codex --dangerously-skip-permissions"),
            (None, "claude --dangerously-skip-permissions"),
        ] {
            assert_eq!(with_claude_bypass_consent(family, command), command);
        }
    }
}
