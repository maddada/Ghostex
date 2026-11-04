use super::rpc::{CliError, CliResult};
use crate::accounts::helpers;
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::process::Stdio;
use std::{path::Path, process::Command};

const HELP: &str = "Usage: ghostex account-login claude [--account NUMBER] [--email EMAIL] [--json] [--result-file PATH]\n\nSign in with a separate Claude profile and save it with cswap.\nWithout --account, saves the account you choose in the browser; --email pre-fills it and requires that account.\nWith --account, reconnects only that saved account.\n--result-file writes the saved account, or the reason the login failed, to PATH as JSON.";

/// CDXC:AgentProviders 2026-10-28 WHY:
/// Settings read the login's result from its terminal screen, and on Windows ConPTY can drop a process's last output when it exits, so a saved account was reported as "Login did not complete" with only the `{` of the result left on screen (#347). The result now also goes to a file gxserver names; the terminal stays for the person watching.
/// SEE-ALSO: `claude_outcome` in server/src/accounts/setup.rs reads this file.
pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let result_index = args.iter().position(|arg| arg == "--result-file");
    let result_file = result_index.and_then(|index| args.get(index + 1));
    if result_index.is_some() && result_file.is_none() {
        return Err(failure("--result-file requires a path"));
    }
    let rest: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(index, _)| result_index.is_none_or(|at| *index != at && *index != at + 1))
        .map(|(_, arg)| arg.clone())
        .collect();
    let result = login(&rest);
    if let Some(path) = result_file {
        let report = match &result {
            Ok(Some(saved)) => saved.clone(),
            Ok(None) => return Ok(()),
            Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
        };
        if let Err(error) = std::fs::write(path, report.to_string()) {
            // A failed login keeps its own reason; a saved one must say the report is missing.
            if result.is_ok() {
                return Err(failure(format!(
                    "Could not write the login result: {error}"
                )));
            }
        }
    }
    result.map(|_| ())
}

/// CDXC:AgentProviders 2026-09-07 WHY:
/// Running `claude auth login && cswap add` against the active profile refreshed the last-used account when browser login reused its identity. New logins need a fresh profile and an identity check before cswap captures anything; reconnect must match the selected saved identity.
/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User picked "Add account without typing the email" for Settings > Accounts. A new account no longer asks for an email: the browser sign-in decides which account is saved, and an optional --email only pre-fills it and requires that account. Reconnect always pre-fills and requires the saved account's own email.
/// The saved account as `{"schemaVersion":1,"account":{"number","email"}}`, or `None` for `--help`.
fn login(args: &[String]) -> CliResult<Option<Value>> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{HELP}");
        return Ok(None);
    }
    let json_output = args.iter().any(|arg| arg == "--json");
    let email_index = args.iter().position(|arg| arg == "--email");
    let supplied_email = email_index.and_then(|index| args.get(index + 1)).cloned();
    if email_index.is_some() && supplied_email.is_none() {
        return Err(failure("--email requires an address"));
    }
    let filtered: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| {
            arg.as_str() != "--json"
                && email_index
                    .is_none_or(|email_index| *index != email_index && *index != email_index + 1)
        })
        .map(|(_, arg)| arg.clone())
        .collect();
    let account = match filtered.as_slice() {
        [provider] if provider == "claude" => None,
        [provider, flag, slot]
            if provider == "claude"
                && flag == "--account"
                && slot.parse::<u64>().is_ok_and(|number| number > 0) =>
        {
            Some(slot.as_str())
        }
        _ => return Err(failure(HELP)),
    };
    // Windows has no HOME; USERPROFILE is the home the account helpers themselves resolve.
    let home = ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .find(|path| path.is_absolute())
        .ok_or_else(|| failure("The home directory is unavailable."))?;
    let claude = helpers::executable(&home, "claude")
        .ok_or_else(|| failure("Install Claude Code first."))?;
    let cswap = helpers::executable(&home, "cswap")
        .ok_or_else(|| failure("Install Claude Swap in Settings > Accounts first."))?;
    println!("Reading saved Claude accounts...");
    let roster = helpers::json_command(&home, "cswap", &["list", "--json"]).map_err(failure)?;
    let rows = roster["accounts"].as_array().ok_or_else(|| {
        failure("Claude Swap didn't list your accounts. Update it in Settings > Accounts and try again.")
    })?;
    let target = account
        .map(|slot| {
            rows.iter()
                .find(|row| row["number"].as_u64().map(|n| n.to_string()).as_deref() == Some(slot))
                .ok_or_else(|| {
                    failure("The selected saved account no longer exists. Refresh Accounts first.")
                })
        })
        .transpose()?;
    let email: Option<String> = if let Some(target) = target {
        Some(
            target["email"]
                .as_str()
                .filter(|email| !email.is_empty())
                .ok_or_else(|| {
                    failure("The saved account has no email identity. Refresh Accounts first.")
                })?
                .to_string(),
        )
    } else {
        supplied_email.map(|email| email.trim().to_string())
    };
    if email
        .as_deref()
        .is_some_and(|email| !email.contains('@') || email.chars().any(char::is_whitespace))
    {
        return Err(failure(
            "Enter the email address of the Claude account you want to connect.",
        ));
    }
    // Use a private ASCII path on macOS so the profile's Keychain service hash is unambiguous.
    let mut builder = tempfile::Builder::new();
    builder.prefix("ghostex-claude-login-");
    #[cfg(target_os = "macos")]
    let directory = builder.tempdir_in("/private/tmp");
    #[cfg(not(target_os = "macos"))]
    let directory = builder.tempdir();
    let profile = LoginProfile(directory.map_err(|e| failure(e.to_string()))?);
    println!(
        "Choose the requested account in the browser. If another account appears, choose a different account before authorizing."
    );
    let mut login = profile.command(&claude);
    login.args(["auth", "login", "--claudeai"]);
    if let Some(email) = &email {
        login.args(["--email", email]);
    }
    let status = login
        .status()
        .map_err(|e| failure(format!("Could not start Claude login: {e}")))?;
    if !status.success() {
        return Err(failure(
            "Claude login was cancelled or failed. No saved account was changed.",
        ));
    }
    let config: Value = serde_json::from_slice(
        &std::fs::read(profile.0.path().join(".claude.json")).map_err(|_| {
            failure("Claude did not save the new profile's identity. No saved account was changed.")
        })?,
    )
    .map_err(|_| failure("Claude's new profile identity could not be read."))?;
    let identity = &config["oauthAccount"];
    let signed_in_email = identity["emailAddress"].as_str().unwrap_or_default();
    let organization = identity["organizationUuid"].as_str().unwrap_or_default();
    if signed_in_email.is_empty() {
        return Err(failure(
            "Claude did not report which account signed in. No saved account was changed.",
        ));
    }
    // SEE-ALSO: `wrong_account` in server/src/accounts/setup.rs reads this sentence.
    if let Some(email) = email
        .as_deref()
        .filter(|email| !signed_in_email.eq_ignore_ascii_case(email))
    {
        return Err(failure(format!(
            "The browser signed in to {signed_in_email} instead of {email}. Run login again and choose the requested account. No saved account was changed."
        )));
    }
    let matches_identity = |row: &Value| {
        row["email"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(signed_in_email))
            && row["organizationUuid"].as_str().unwrap_or_default() == organization
    };
    // Login is interactive; another account may have been saved or moved while the browser was open.
    let latest_roster =
        helpers::json_command(&home, "cswap", &["list", "--json"]).map_err(failure)?;
    let latest_rows = latest_roster["accounts"].as_array().ok_or_else(|| {
        failure("Your accounts could not be checked, so no saved account was changed.")
    })?;
    if let Some(target) = target {
        let still_selected = latest_rows
            .iter()
            .any(|row| row["number"] == target["number"] && matches_identity(row));
        if !matches_identity(target) || !still_selected {
            return Err(failure(
                "The login belongs to a different organization than the selected account. No saved account was changed.",
            ));
        }
    } else if latest_rows.iter().any(&matches_identity) {
        return Err(failure(
            "This account is already saved. Use its Reconnect action in Settings, or run login again with a different account. No saved account was changed.",
        ));
    }
    let status = profile
        .command(&cswap)
        .arg("add")
        .status()
        .map_err(|e| failure(format!("Could not save the login: {e}")))?;
    // A new account is saved once cswap lists it, even when cswap then exits with an error;
    // a reconnect only refreshes an existing slot, so there the exit status is the only proof.
    if !status.success() && target.is_some() {
        return Err(failure(
            "Claude Swap could not save this login. Check the message above and try again.",
        ));
    }
    let saved_roster = read_saved_roster(&home)?;
    let saved_rows = saved_roster["accounts"].as_array().ok_or_else(|| {
        failure("Claude Swap didn't list your accounts. Update it in Settings > Accounts and try again.")
    })?;
    let saved = saved_row(
        latest_rows,
        saved_rows,
        target.map(|target| &target["number"]),
        signed_in_email,
        organization,
    )
    .ok_or_else(|| {
        failure(if status.success() {
            "cswap saved the login, but its account list does not show it. Refresh Accounts and try again."
        } else {
            "Claude Swap could not save this login. Check the message above and try again."
        })
    })?;
    let report = serde_json::json!({"schemaVersion":1,"account":{"number":saved["number"],"email":saved["email"]}});
    if json_output {
        println!("{report}");
    } else {
        println!(
            "Login saved. Ghostex will finish connecting this account automatically when login was started in Settings."
        );
    }
    Ok(Some(report))
}

/// `cswap list --json` after the save. A list that times out or fails right after `cswap add` is
/// read again, so a slow first read does not report a saved account as failed.
fn read_saved_roster(home: &Path) -> CliResult<Value> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match helpers::json_command(home, "cswap", &["list", "--json"]) {
            Ok(roster) => return Ok(roster),
            Err(error) if attempt >= 3 => return Err(failure(error)),
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(2)),
        }
    }
}

/// The roster row this login was saved in. On reconnect it is the selected slot, still holding the
/// signed-in identity. For a new account it is the row with the signed-in email and organization,
/// or else the slot this `cswap add` created for the signed-in email.
///
/// CDXC:AgentProviders 2026-10-28 WHY:
/// A saved account this lookup missed made the login report failure and a retry re-save the same slot (#347). cswap keys a slot by the email and organization it read itself, so the slot that appeared during this add with the signed-in email is this login whichever organization cswap recorded.
fn saved_row<'a>(
    before: &[Value],
    after: &'a [Value],
    target_number: Option<&Value>,
    email: &str,
    organization: &str,
) -> Option<&'a Value> {
    let same_email = |row: &Value| {
        row["email"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(email))
    };
    let same_identity = |row: &Value| {
        same_email(row) && row["organizationUuid"].as_str().unwrap_or_default() == organization
    };
    if let Some(number) = target_number {
        return after
            .iter()
            .find(|row| &row["number"] == number && same_identity(row));
    }
    after.iter().find(|row| same_identity(row)).or_else(|| {
        after.iter().find(|row| {
            same_email(row)
                && !row["number"].is_null()
                && !before.iter().any(|old| old["number"] == row["number"])
        })
    })
}

struct LoginProfile(tempfile::TempDir);
impl LoginProfile {
    fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
        ] {
            command.env_remove(key);
        }
        command
            .env("CLAUDE_CONFIG_DIR", self.0.path())
            .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", self.0.path());
        command
    }
}
impl Drop for LoginProfile {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use sha2::{Digest, Sha256};
            let hash = hex::encode(Sha256::digest(self.0.path().as_os_str().as_encoded_bytes()));
            let _ = Command::new("/usr/bin/security")
                .args([
                    "delete-generic-password",
                    "-s",
                    &format!("Claude Code-credentials-{}", &hash[..8]),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}
fn failure(message: impl Into<String>) -> CliError {
    CliError::Other(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(number: u64, email: &str, organization: &str) -> Value {
        json!({"number": number, "email": email, "organizationUuid": organization})
    }

    #[test]
    fn new_account_is_found_by_its_identity() {
        let before = [row(1, "first@example.com", "org-1")];
        let after = [
            row(1, "first@example.com", "org-1"),
            row(2, "Second@Example.com", "org-2"),
        ];
        let saved = saved_row(&before, &after, None, "second@example.com", "org-2");
        assert_eq!(saved.map(|row| row["number"].clone()), Some(json!(2)));
    }

    #[test]
    fn new_account_is_found_when_cswap_recorded_another_organization() {
        let before = [row(1, "first@example.com", "org-1")];
        let after = [
            row(1, "first@example.com", "org-1"),
            row(2, "second@example.com", ""),
        ];
        let saved = saved_row(&before, &after, None, "second@example.com", "org-2");
        assert_eq!(saved.map(|row| row["number"].clone()), Some(json!(2)));
    }

    #[test]
    fn an_older_slot_with_the_same_email_is_not_this_login() {
        let before = [row(1, "second@example.com", "personal-org")];
        let after = before.clone();
        assert!(saved_row(&before, &after, None, "second@example.com", "team-org").is_none());
    }

    #[test]
    fn reconnect_needs_the_selected_slot_with_the_signed_in_identity() {
        let rows = [
            row(1, "first@example.com", "org-1"),
            row(2, "second@example.com", "org-2"),
        ];
        let found = saved_row(&rows, &rows, Some(&json!(2)), "second@example.com", "org-2");
        assert_eq!(found.map(|row| row["number"].clone()), Some(json!(2)));
        assert!(saved_row(&rows, &rows, Some(&json!(1)), "second@example.com", "org-2").is_none());
    }

    #[test]
    fn a_failed_login_writes_its_reason_to_the_result_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("result.json");
        let args: Vec<String> = ["claude", "--account", "0", "--json", "--result-file"]
            .into_iter()
            .map(String::from)
            .chain([path.to_string_lossy().into_owned()])
            .collect();
        assert!(run(&args).is_err());
        let report: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["ok"], json!(false));
        assert_eq!(report["error"], json!(HELP));
    }
}
