//! Which file Windows OpenSSH Server reads this account's keys from, and the
//! elevated add/remove for a file outside the profile.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};

use super::authorized_keys::{authorized_keys_path, AdministratorApprovalDeclined};
use super::windows_elevation::{
    run_elevated_powershell, EXIT_LAUNCH_FAILED, EXIT_NO_DESKTOP, EXIT_NO_EXIT_CODE,
    EXIT_NO_PROCESS, EXIT_UAC_CANCELLED,
};

/*
CDXC:RemotePairing 2026-10-08 WHY:
Windows OpenSSH Server does not read `%USERPROFILE%\.ssh\authorized_keys` for members of the Administrators group, which is most Windows accounts: the `sshd_config` Windows ships ends with `Match Group administrators` / `AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys`, and sshd ignores that file unless only SYSTEM and Administrators can change it (Microsoft's documented fix is `icacls … /inheritance:r /grant Administrators:F /grant SYSTEM:F`). Pairing therefore reads the live `sshd_config` to find the file sshd really uses for this account, and when that is not the profile's own file it adds or removes the phone's line through one UAC prompt and reapplies that ACL; it never edits `sshd_config` and never touches the other lines. A config gxserver cannot read is taken to be the one Windows ships, because Windows creates `sshd_config` from that default.
*/
pub(crate) enum KeysFile {
    /// `%USERPROFILE%\.ssh\authorized_keys`, written without elevation.
    User,
    /// A file outside the profile (normally `administrators_authorized_keys`).
    Elevated(PathBuf),
}

/// The `AuthorizedKeysFile` lines of the `sshd_config` Windows ships.
const WINDOWS_DEFAULT_SSHD_CONFIG: &str = "AuthorizedKeysFile .ssh/authorized_keys\nMatch Group administrators\n  AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys\n";

const ADMINISTRATORS_GROUP_SID: &str = "S-1-5-32-544";

pub(crate) fn keys_file_for_this_account() -> Result<KeysFile> {
    let program_data = PathBuf::from(
        std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string()),
    );
    let config = std::fs::read_to_string(program_data.join("ssh").join("sshd_config"))
        .unwrap_or_else(|_| WINDOWS_DEFAULT_SSHD_CONFIG.to_string());
    let username = super::identity::read_remote_access_identity()?.username;
    let home = crate::ghostex_cli::rpc::home_dir();
    let groups = read_group_names()?;
    let files = authorized_keys_files(
        &config,
        &SshdAccount {
            username: &username,
            groups: &groups,
            home: &home,
            program_data: &program_data,
        },
    );
    let Some(first) = files.first() else {
        bail!("SSH on this computer has key sign-in turned off (`AuthorizedKeysFile none` in sshd_config), so the phone's key cannot be added.");
    };
    let user_file = authorized_keys_path();
    if files.iter().any(|file| same_windows_path(file, &user_file)) {
        Ok(KeysFile::User)
    } else {
        Ok(KeysFile::Elevated(first.clone()))
    }
}

/// Lowercase group names without their domain, plus `administrators` for a
/// member of the built-in group: Windows OpenSSH matches that name to the
/// group's SID, so it works on Windows installs in any language.
fn read_group_names() -> Result<Vec<String>> {
    let mut command = std::process::Command::new("whoami");
    command.args(["/groups", "/fo", "csv", "/nh"]);
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
    let output = command
        .output()
        .with_context(|| "read group membership with whoami")?;
    if !output.status.success() {
        bail!("whoami /groups failed with {}", output.status);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut groups = Vec::new();
    for line in text.lines() {
        let fields = line
            .trim()
            .trim_matches('"')
            .split("\",\"")
            .collect::<Vec<_>>();
        if let Some(name) = fields.first().filter(|name| !name.is_empty()) {
            groups.push(name.rsplit('\\').next().unwrap_or(name).to_lowercase());
        }
        if fields.contains(&ADMINISTRATORS_GROUP_SID) {
            groups.push("administrators".to_string());
        }
    }
    Ok(groups)
}

pub(crate) struct SshdAccount<'a> {
    pub username: &'a str,
    /// Lowercase group names.
    pub groups: &'a [String],
    pub home: &'a Path,
    pub program_data: &'a Path,
}

/// The files sshd checks for `account`, in order. Global keywords keep their
/// first value; a satisfied `Match` block overrides them, and among satisfied
/// blocks the first value wins (sshd_config(5)). Criteria other than `User`,
/// `Group` and `All` depend on the connection, so their blocks are skipped.
pub(crate) fn authorized_keys_files(config: &str, account: &SshdAccount) -> Vec<PathBuf> {
    let mut global: Option<Vec<String>> = None;
    let mut matched: Option<Vec<String>> = None;
    let mut in_match = false;
    let mut match_active = false;
    for line in config.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let keyword_end = line
            .find(|character: char| character.is_whitespace() || character == '=')
            .unwrap_or(line.len());
        let keyword = &line[..keyword_end];
        let rest = line[keyword_end..]
            .trim_start_matches(|character: char| character.is_whitespace() || character == '=');
        if keyword.eq_ignore_ascii_case("match") {
            in_match = true;
            match_active = match_criteria_hold(&split_arguments(rest), account);
        } else if keyword.eq_ignore_ascii_case("authorizedkeysfile") {
            let slot = if in_match {
                if !match_active {
                    continue;
                }
                &mut matched
            } else {
                &mut global
            };
            if slot.is_none() {
                *slot = Some(split_arguments(rest));
            }
        }
    }
    let values = matched.or(global).unwrap_or_else(|| {
        vec![
            ".ssh/authorized_keys".to_string(),
            ".ssh/authorized_keys2".to_string(),
        ]
    });
    if values.len() == 1 && values[0].eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    values
        .iter()
        .map(|value| expand_keys_file(value, account))
        .collect()
}

fn match_criteria_hold(arguments: &[String], account: &SshdAccount) -> bool {
    if arguments.len() == 1 && arguments[0].eq_ignore_ascii_case("all") {
        return true;
    }
    if arguments.is_empty() || arguments.len() % 2 != 0 {
        return false;
    }
    arguments.chunks(2).all(|pair| {
        let patterns = &pair[1];
        match pair[0].to_ascii_lowercase().as_str() {
            "user" => pattern_list_matches(account.username, patterns),
            "group" => account
                .groups
                .iter()
                .any(|group| pattern_list_matches(group, patterns)),
            _ => false,
        }
    })
}

/// OpenSSH pattern lists: comma-separated `*`/`?` globs, where a matching
/// `!pattern` rejects outright. Windows OpenSSH compares names
/// case-insensitively.
fn pattern_list_matches(value: &str, list: &str) -> bool {
    let value = value.to_lowercase().chars().collect::<Vec<_>>();
    let mut matched = false;
    for pattern in list.split(',') {
        let pattern = pattern.trim().to_lowercase();
        let (negated, pattern) = match pattern.strip_prefix('!') {
            Some(rest) => (true, rest.to_string()),
            None => (false, pattern),
        };
        if glob_matches(&value, &pattern.chars().collect::<Vec<_>>()) {
            if negated {
                return false;
            }
            matched = true;
        }
    }
    matched
}

fn glob_matches(text: &[char], pattern: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| glob_matches(&text[skip..], rest)),
        Some((first, rest)) => match text.split_first() {
            Some((character, text_rest)) if *first == '?' || first == character => {
                glob_matches(text_rest, rest)
            }
            _ => false,
        },
    }
}

/// Whitespace-separated arguments; double quotes group one with spaces.
fn split_arguments(text: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut has_argument = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                has_argument = true;
            }
            character if character.is_whitespace() && !quoted => {
                if has_argument {
                    arguments.push(std::mem::take(&mut current));
                    has_argument = false;
                }
            }
            character => {
                current.push(character);
                has_argument = true;
            }
        }
    }
    if has_argument {
        arguments.push(current);
    }
    arguments
}

/// Expands `%%`, `%h`, `%u` and Windows OpenSSH's `__PROGRAMDATA__`; a
/// relative result is relative to the home folder.
fn expand_keys_file(value: &str, account: &SshdAccount) -> PathBuf {
    let mut expanded = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            expanded.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => expanded.push('%'),
            Some('h') => expanded.push_str(&account.home.to_string_lossy()),
            Some('u') => expanded.push_str(account.username),
            Some(other) => {
                expanded.push('%');
                expanded.push(other);
            }
            None => expanded.push('%'),
        }
    }
    let expanded = expanded
        .replace("__PROGRAMDATA__", &account.program_data.to_string_lossy())
        .replace('/', "\\");
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        account.home.join(path)
    }
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| path.to_string_lossy().replace('/', "\\").to_lowercase();
    normalize(left) == normalize(right)
}

// Exit codes chosen by the elevated script.
const EXIT_WRITE_FAILED: i32 = 2;
const EXIT_ACL_FAILED: i32 = 3;
const EXIT_NOT_ELEVATED: i32 = 6;
const EXIT_NOTHING_REMOVED: i32 = 10;

// Adds the line (`append`) or deletes every line that carries the marker as
// a whitespace-separated field (`remove`), keeping every other line and the
// file's line endings byte for byte, then applies Microsoft's ACL for this
// file. The line or marker arrives base64-encoded so device names never meet
// command-line quoting.
const ELEVATED_SCRIPT: &str = r#"param([string]$LogPath, [string]$Mode, [string]$KeysPath, [string]$ValueBase64)
$ErrorActionPreference = 'Stop'
function Note([string]$Text) { Add-Content -LiteralPath $LogPath -Value $Text }
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { exit 6 }
$utf8 = New-Object System.Text.UTF8Encoding($false)
try {
    $value = $utf8.GetString([Convert]::FromBase64String($ValueBase64)).Trim()
    if ($value.Length -eq 0 -or $value.Contains("`n") -or $value.Contains("`r")) { Note 'the key line is not a single line'; exit 2 }
    $existing = ''
    if (Test-Path -LiteralPath $KeysPath) { $existing = [IO.File]::ReadAllText($KeysPath, $utf8) }
    if ($Mode -eq 'append') {
        $newline = "`n"
        if ($existing.Contains("`r`n")) { $newline = "`r`n" }
        $prefix = ''
        if ($existing.Length -gt 0 -and -not $existing.EndsWith("`n")) { $prefix = $newline }
        $directory = Split-Path -Parent $KeysPath
        if (-not (Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Path $directory -Force | Out-Null }
        [IO.File]::AppendAllText($KeysPath, $prefix + $value + $newline, $utf8)
    } elseif ($Mode -eq 'remove') {
        $kept = New-Object System.Text.StringBuilder
        $removed = 0
        foreach ($line in [regex]::Split($existing, '(?<=\n)')) {
            if ($line.Length -eq 0) { continue }
            $fields = $line.Trim() -split '\s+'
            if ($fields -ccontains $value) { $removed++ } else { [void]$kept.Append($line) }
        }
        if ($removed -eq 0) { exit 10 }
        [IO.File]::WriteAllText($KeysPath, $kept.ToString(), $utf8)
    } else {
        Note "unknown mode $Mode"
        exit 2
    }
} catch {
    Note $_.Exception.Message
    exit 2
}
$acl = & icacls.exe $KeysPath /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' 2>&1
if ($LASTEXITCODE -ne 0) {
    Note (($acl | Out-String).Trim())
    exit 3
}
exit 0
"#;

/// Appends `line` to `path` through the UAC prompt.
pub(crate) fn append_elevated(path: &Path, line: &str) -> Result<()> {
    run_keys_script(path, "append", line).map(|_| ())
}

/// Removes the lines carrying `marker` from `path` through the UAC prompt.
/// Returns whether any line was removed.
pub(crate) fn remove_elevated(path: &Path, marker: &str) -> Result<bool> {
    run_keys_script(path, "remove", marker)
}

fn run_keys_script(path: &Path, mode: &str, value: &str) -> Result<bool> {
    let keys_path = path.to_string_lossy();
    let encoded = STANDARD.encode(value.trim().as_bytes());
    let run = run_elevated_powershell(
        "ghostex-ssh-keys",
        ELEVATED_SCRIPT,
        &[mode, &keys_path, &encoded],
    )
    .map_err(anyhow::Error::msg)?;
    let detail = |lead: &str, detail: &str| {
        let detail = detail.trim();
        if detail.is_empty() {
            format!("{lead}.")
        } else {
            format!("{lead}: {detail}.")
        }
    };
    match run.code {
        Some(0) => Ok(true),
        Some(EXIT_NOTHING_REMOVED) => Ok(false),
        Some(EXIT_UAC_CANCELLED) => Err(AdministratorApprovalDeclined {
            keys_file: path.to_path_buf(),
        }
        .into()),
        Some(EXIT_WRITE_FAILED) => bail!(detail(&format!("Changing {} failed", path.display()), &run.log)),
        Some(EXIT_ACL_FAILED) => bail!(detail(
            &format!("The key was written, but SSH will ignore {} until only SYSTEM and Administrators can change it, and setting that failed", path.display()),
            &run.log
        )),
        Some(EXIT_NOT_ELEVATED) => bail!(
            "The administrator prompt did not give Windows PowerShell administrator rights, so {} could not be changed.",
            path.display()
        ),
        Some(EXIT_NO_DESKTOP) => bail!(
            "Windows cannot show the administrator prompt because Ghostex's background service is not running in your desktop session (it was started from an SSH or remote connection). Quit Ghostex together with its background service, open Ghostex from the Start menu, then try again."
        ),
        Some(EXIT_LAUNCH_FAILED) => bail!(detail("Could not open the administrator prompt", &run.stderr)),
        Some(EXIT_NO_PROCESS) | Some(EXIT_NO_EXIT_CODE) => {
            bail!("The elevated PowerShell did not report a result.")
        }
        Some(code) => bail!(detail(
            &format!("Changing {} failed with exit code {code}", path.display()),
            if run.log.is_empty() { &run.stderr } else { &run.log }
        )),
        None => bail!("The elevated PowerShell was terminated."),
    }
}
