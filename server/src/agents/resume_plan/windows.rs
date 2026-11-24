use super::*;

/// CDXC:PlatformSupport 2026-09-14 WHY:
/// PowerShell 5.1 cannot execute the POSIX assignment and conditional chains used to validate saved agent conversations before resuming them.
/// CDXC:PlatformSupport 2026-11-24 WHY:
/// `Write-Error` is a cmdlet, not a native executable, so quoting its message with `quote_shell_arg`
/// (which emits `powershell_native_arg` for strings containing `"`) mangled quotes on PowerShell 5.1.
/// Use `powershell_single_quote` for the cmdlet argument, capture `$LASTEXITCODE` before trimming
/// `$ghostexResumeId`, and keep `Write-Error` so `$?` still goes false for the restore fallback.
fn lookup(lookup_command: String, resume_command: String, failure: String) -> String {
    format!(
        "$ghostexResumeId = {lookup_command}; $ghostexLookupExit = $LASTEXITCODE; $ghostexResumeId = \"$ghostexResumeId\".Trim(); if ($ghostexLookupExit -eq 0 -and -not [string]::IsNullOrWhiteSpace($ghostexResumeId)) {{ {resume_command} }} else {{ Write-Error {} }}",
        crate::platform::shell::powershell_single_quote(&failure)
    )
}

pub(crate) fn build_codex_validated_resume_command(command: &str, reference: &str) -> String {
    lookup(
        format!(
            "{} codex --exact {}",
            build_resume_lookup_command(),
            quote_shell_arg(reference)
        ),
        build_codex_resume_invocation(command, "\"$ghostexResumeId\""),
        format!("Unable to restore Codex session \"{reference}\"."),
    )
}

pub(crate) fn build_codex_resume_lookup_command(command: &str, title: &str) -> String {
    lookup(
        format!(
            "{} codex --title {}",
            build_resume_lookup_command(),
            quote_shell_arg(title)
        ),
        build_codex_resume_invocation(command, "\"$ghostexResumeId\""),
        format!("Unable to find restorable Codex session id for \"{title}\"."),
    )
}

pub(crate) fn build_claude_resume_lookup_command(
    command: &str,
    input: &AgentResumeInput,
    title: &str,
) -> String {
    lookup(
        format!(
            "{} claude {} {} {}",
            build_resume_lookup_command(),
            quote_shell_arg(input.project_path.as_deref().unwrap_or_default()),
            quote_shell_arg(title),
            quote_shell_arg(input.first_user_message.as_deref().unwrap_or_default())
        ),
        build_claude_resume_invocation(command, "\"$ghostexResumeId\""),
        format!("Unable to find restorable Claude session id for \"{title}\"."),
    )
}

pub(crate) fn build_cursor_resume_lookup_command(
    command: &str,
    project: &str,
    title: &str,
) -> String {
    lookup(
        format!(
            "{} cursor {} {}",
            build_resume_lookup_command(),
            quote_shell_arg(project),
            quote_shell_arg(title)
        ),
        format!("{command} --resume \"$ghostexResumeId\""),
        format!("Unable to find Cursor chat id for \"{title}\"."),
    )
}
