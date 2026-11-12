use super::model::*;
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

/// CDXC:AgentProviders 2026-09-18 WHY:
/// Windows reported cswap, xswap, claude and codex as missing while they were installed and on PATH.
/// Two causes: the lookup joined the bare name, so it never matched `xswap.exe`; and a long-running gxserver keeps the PATH it started with, so an installer that appends to the user PATH (codex-swap's install.ps1) stays invisible until the app restarts.
/// `platform::live_path` resolves PATHEXT spellings over the live registry PATH; this adds the helpers' own install directories.
/// SEE-ALSO: server/src/accounts/launch.rs, server/src/agent_hooks/windows.rs.
/// The account tools under the names Settings > Accounts shows them by.
pub(crate) fn tool_label(name: &str) -> &str {
    match name {
        "cswap" => "Claude Swap",
        "xswap" => "Codex Swap",
        other => other,
    }
}

const COULD_NOT_READ_ACCOUNTS: &str = "Could not read your accounts. Try refreshing.";

pub(crate) fn executable(home: &Path, name: &str) -> Option<PathBuf> {
    // Windows: a cswap started while its install is being switched would hold the files mid-move.
    if name == Provider::Claude.helper() {
        super::claude_swap_switch::wait_for_switch();
    }
    crate::platform::live_path::find(name, &install_dirs(home))
}

/// `executable` for a probe Ghostex runs on its own: `None` while `name` is only an install-on-demand
/// wrapper, which the probe would make install the CLI (`agent_hooks/on_demand.rs`).
pub(crate) fn probe_executable(home: &Path, name: &str) -> Option<PathBuf> {
    crate::agent_hooks::on_demand::probe_target_path(executable(home, name)?, name, home)
}

/// The helpers' own install directories, searched after the live PATH.
fn install_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![home.join(".local/bin"), home.join(".cargo/bin")];
    // CDXC:AgentProviders 2026-11-11 WHY:
    // A gxserver that `cargo xtask rebuild` starts as a launchd job has launchd's bare PATH, so a
    // Claude or Codex installed only through mise (no `~/.local/bin` link) read as not installed to
    // every account and usage helper. mise's shims run whichever version mise has now.
    #[cfg(not(windows))]
    dirs.extend([
        crate::agent_cli::launch_path::mise_data_dir(home).join("shims"),
        home.join(".asdf/shims"),
    ]);
    // CDXC:AgentProviders 2026-09-06 WHY:
    // A GUI or systemd launch may omit Homebrew from PATH even after the account helper is installed.
    #[cfg(target_os = "macos")]
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    #[cfg(target_os = "linux")]
    dirs.extend([
        home.join(".linuxbrew/bin"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
    ]);
    // codex-swap's Windows installer owns this directory and adds it to the user PATH.
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        dirs.push(local.join("Programs").join("codex-swap"));
    }
    // uv installed by Ghostex, which installs and manages Claude Swap.
    dirs.extend(crate::managed_tools::paths::path_dirs());
    dirs
}

pub(crate) fn json_command(home: &Path, name: &str, args: &[&str]) -> Result<Value, String> {
    json_command_env(home, name, args, &[])
}
fn json_command_env(
    home: &Path,
    name: &str,
    args: &[&str],
    envs: &[(&str, &Path)],
) -> Result<Value, String> {
    let binary = executable(home, name)
        .ok_or_else(|| format!("Install {} in Settings > Accounts first.", tool_label(name)))?;
    let mut command = crate::platform::process::background_command(binary);
    command.args(args).env("HOME", home);
    command.envs(envs.iter().copied());
    // The Windows helpers resolve their registry from USERPROFILE, so both variables must name the same home.
    #[cfg(windows)]
    command.env("USERPROFILE", home);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|_| format!("Could not start {}.", tool_label(name)))?;
    let stdout = child.stdout.take().ok_or(COULD_NOT_READ_ACCOUNTS)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(2 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "{} did not finish reading your accounts. Try refreshing.",
                    tool_label(name)
                ));
            }
        }
    }?;
    if !status.success() {
        return Err(format!(
            "{} could not read your accounts. Try refreshing, or update it in Settings > Accounts.",
            tool_label(name)
        ));
    }
    let bytes = reader
        .join()
        .map_err(|_| COULD_NOT_READ_ACCOUNTS)?
        .map_err(|_| COULD_NOT_READ_ACCOUNTS)?;
    let data: Value = serde_json::from_slice(&bytes).map_err(|_| {
        format!(
            "{} sent an answer Ghostex can't read. Update it in Settings > Accounts.",
            tool_label(name)
        )
    })?;
    if data.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(format!(
            "Update {name}; its account interface is unsupported."
        ));
    }
    Ok(data)
}
/// A usage window's label: its length as `5h`, `7d` (a week or longer), or `<n>h` / `<n>d` /
/// `<n>m`; a model's window puts the model first ("Fable 7d").
///
/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User: "please use 7d and 5h everywhere never use weekly or five hours etc." gxserver names every usage window by its length (5h, 7d, <Model> 7d), so every client and the CLI show the same short labels; text that needs a noun says "7d limit". Supersedes the Five-hour limit / Weekly limit / <model> weekly labels.
/// SEE-ALSO: packages/gx-core/src/sidebar_accounts/usage.rs (`account_usage_label`, whose compact Fable label stays "Fable" per its 2026-09-12 decision), packages/gx-chat-core/src/menus/context/rows.rs, apps/desktop/src/app/window/account_usage/limits.rs, Settings > Accounts (`usage_line`).
pub(crate) fn duration_label(seconds: i64) -> String {
    if seconds >= 604_800 {
        "7d".into()
    } else if seconds % 86_400 == 0 {
        format!("{}d", seconds / 86_400)
    } else if seconds % 3_600 == 0 {
        format!("{}h", seconds / 3_600)
    } else {
        format!("{}m", (seconds / 60).max(1))
    }
}
fn text(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
fn window(
    id: &str,
    label: &str,
    value: &Value,
    pct: &str,
    model: Option<String>,
) -> Option<UsageWindow> {
    let used = value.get(pct)?.as_f64()?;
    if !used.is_finite() {
        return None;
    }
    Some(UsageWindow {
        id: id.into(),
        label: label.into(),
        used_percent: used.clamp(0., 100.),
        limit_window_seconds: None,
        resets_at: value
            .get("resetsAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        model,
    })
}
/// CDXC:AgentProviders 2026-11-12 WHY:
/// User: a session at account 4's limit sat on "Waiting for an eligible account to have capacity" while its Switch account list said account 5's "saved login expired. Reconnect this account.", although sessions were running on account 5 and Reconnect changed nothing. cswap reports `token_expired` when the access token it holds is past its expiry and the refresh is deferred: another process holds the refresh lock, or a running Claude on that account owns the login and renews it on its next request (claude-swap `json_output.usage_fields`, "refresh deferred this pass; retries automatically"; its own auto-switch waits instead of switching for it). The login works, so the account stays ready; only a dead login (`relogin_required`, `no_credentials`) needs a sign-in. A reading cswap took no longer ago than it polls an idle candidate (`CANDIDATE_MAX_INTERVAL_S`, 600 seconds) still counts as the account's usage, so a switch can pick the account when that reading shows room; an older one is shown as last known and never counts as room (2026-09-18).
const TOKEN_RENEWING_READING_SECONDS: f64 = 600.;
/// The user-facing line for a cswap `usageStatus` other than `ok`. `has_last_good` says whether the row still carries an older reading to show, and `last_good_age` is that reading's age in seconds when cswap reports it.
fn claude_usage_notice(
    status: &str,
    has_last_good: bool,
    last_good_age: Option<f64>,
) -> Option<String> {
    Some(match status {
        "ok" => return None,
        "unavailable" => match (has_last_good, last_good_age) {
            (true, Some(age)) => format!(
                "Usage is from {} ago. Refreshing has failed since then; Ghostex keeps trying.",
                age_text(age)
            ),
            (true, None) => {
                "Usage could not be refreshed. Showing the last reading; Ghostex keeps trying."
                    .into()
            }
            (false, _) => "Usage could not be read yet. Ghostex keeps trying.".into(),
        },
        // Not a dead login (`TOKEN_RENEWING_READING_SECONDS`): no Reconnect.
        "token_expired" => match (has_last_good, last_good_age) {
            (true, Some(age)) => format!(
                "Usage is from {} ago. This login renews its access the next time it is used; Ghostex keeps trying.",
                age_text(age)
            ),
            _ => "Usage can't be read until this login renews its access, the next time it is used. Ghostex keeps trying."
                .into(),
        },
        "relogin_required" => "This account needs to sign in again. Reconnect it.".into(),
        "no_credentials" => "No saved login was found for this account. Reconnect it.".into(),
        "foreign_credential" => {
            "The saved login belongs to a different account. Reconnect this account.".into()
        }
        "api_key" => "This login uses an API key, so it has no subscription usage limits.".into(),
        "keychain_unavailable" => {
            "The saved login could not be read from the Keychain. Unlock it and refresh.".into()
        }
        _ => "Usage could not be read. Ghostex keeps trying.".into(),
    })
}
fn age_text(seconds: f64) -> String {
    let unit = |count: i64, name: &str| {
        if count == 1 {
            format!("1 {name}")
        } else {
            format!("{count} {name}s")
        }
    };
    let minutes = (seconds.max(0.) / 60.).round() as i64;
    if minutes < 1 {
        "under a minute".into()
    } else if minutes < 60 {
        unit(minutes, "minute")
    } else if minutes < 24 * 60 {
        unit(minutes / 60, "hour")
    } else {
        unit(minutes / (24 * 60), "day")
    }
}
/// CDXC:AgentProviders 2026-09-05 DECISION:
/// cswap owns Claude credentials and selected-account launches; gxserver schedules usage reads and decides when sessions switch. xswap supplies Codex account homes and launches.
pub(crate) fn discover(home: &Path, provider: Provider) -> Result<Vec<DiscoveredAccount>, String> {
    let mut data = json_command(home, provider.helper(), &["list", "--json"])?;
    if provider == Provider::Claude {
        reread_signed_out_default_account(home, &mut data);
        // A slot whose login was put back from this computer's newer copy lists again (restore.rs).
        if super::restore::heal_listed(home, &data) {
            if let Ok(listed) = json_command(home, provider.helper(), &["list", "--json"]) {
                data = listed;
                reread_signed_out_default_account(home, &mut data);
            }
        }
    } else {
        fill_codex_members(&mut data);
    }
    let rows = data
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or(COULD_NOT_READ_ACCOUNTS)?;
    // CDXC:AgentProviders 2026-10-01 WHY:
    // Each Codex account reads its usage and reset credits from chatgpt.com, and each Claude account may read its reset credits, so reading the accounts one after another made a forced refresh (Settings > Accounts on open) take about 5 seconds with eight accounts. Every account is read at the same time instead.
    Ok(std::thread::scope(|scope| {
        let tasks: Vec<_> = rows
            .iter()
            .map(|row| scope.spawn(move || discover_row(home, provider, row)))
            .collect();
        tasks
            .into_iter()
            .filter_map(|task| task.join().ok().flatten())
            .collect()
    }))
}
/// CDXC:AgentProviders 2026-10-31 WHY:
/// User, of "The helper could not find this saved login": `ghostex accounts` listed every Codex login twice, once "ready, not added in Settings" and once "needs sign-in", the second being the default. An account's id hashes its identity, and since 2026-10-26 a Codex identity names the member (`userId`), which Codex Swap 0.3.2 and later list. An older xswap earlier on the PATH (`~/.local/bin` 0.3.1 ahead of Homebrew's 0.3.5 on the user's computer) lists no member, so each login read as a new workspace-only id and its saved account read as missing. The member is in the login itself, so a row without one takes it from the row's `auth.json` the way codex-swap does, and one login keeps one id whichever xswap answers.
fn fill_codex_members(data: &mut Value) {
    let Some(rows) = data.get_mut("accounts").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        if !text(row, "userId").trim().is_empty() {
            continue;
        }
        let member = ["home", "savedHome"]
            .iter()
            .map(|key| text(row, key))
            .filter(|home| !home.is_empty())
            .find_map(|home| {
                let raw = std::fs::read(Path::new(&home).join("auth.json")).ok()?;
                let auth: Value = serde_json::from_slice(&raw).ok()?;
                super::codex_identity::codex_auth_user(&auth)
            });
        if let Some(member) = member {
            row["userId"] = Value::String(member);
        }
    }
}

/// CDXC:AgentProviders 2026-10-14 WHY:
/// cswap marks the account the default `claude` profile (`~/.claude.json`) names as `active` and judges that row by the default profile's own live login instead of the slot's saved one. With the default profile signed out, that row read `no_credentials` however often the account signed in again (the sign-in refreshes only the saved login, from a temporary profile), so "Sign in again" ended with "Sign in to this account before adding it." Ghostex starts every account from its saved login (`cswap run`), so that one row is read again with an empty Claude profile, where no slot is active and cswap judges it by its saved login. Only a signed-out default profile is read again: with a live default login the saved and live tokens can share one refresh-token family, and only cswap's active path refreshes that safely.
/// CDXC:AgentProviders 2026-11-01 WHY:
/// A default login cswap quarantined (`relogin_required`) is read again too when the slot's saved copy is a newer generation than it (restore.rs): starts then run in the slot's own profile seeded from that copy (default_login.rs), so the row says what those starts will meet, and the dead default login has no family left to protect.
fn reread_signed_out_default_account(home: &Path, data: &mut Value) {
    let Some(rows) = data.get_mut("accounts").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(row) = rows.iter_mut().find(|row| {
        row["active"] == true
            && (row["usageStatus"] == "no_credentials"
                || row["usageStatus"] == "relogin_required"
                    && row["number"]
                        .as_u64()
                        .and_then(|number| {
                            super::claude_resets::ClaudeSlot::read(home, &number.to_string()).ok()
                        })
                        .is_some_and(|slot| {
                            super::restore::saved_copy_newer_than_default(home, &slot)
                        }))
    }) else {
        return;
    };
    let Ok(profile) = tempfile::Builder::new().prefix("ghostex-cswap-").tempdir() else {
        return;
    };
    let Ok(saved) = json_command_env(
        home,
        "cswap",
        &["list", "--json"],
        &[("CLAUDE_CONFIG_DIR", profile.path())],
    ) else {
        return;
    };
    if let Some(saved_row) = saved["accounts"].as_array().and_then(|saved_rows| {
        saved_rows
            .iter()
            .find(|saved_row| saved_row["number"] == row["number"])
    }) {
        *row = saved_row.clone();
    }
}
fn discover_row(home: &Path, provider: Provider, row: &Value) -> Option<DiscoveredAccount> {
    let number = row.get("number").and_then(Value::as_u64)?;
    let email = text(row, "email");
    let alias = text(row, "alias");
    let mut account = DiscoveredAccount {
        provider,
        selector: number.to_string(),
        identity: String::new(),
        name: if alias.is_empty() {
            email.clone()
        } else {
            alias
        },
        email: email.clone(),
        status: "ready".into(),
        shared_history: false,
        usage: vec![],
        reset_credits: None,
        reset_credit_details: None,
        reset_credits_error: None,
        usage_updated_at: None,
        usage_error: None,
        usage_stale: false,
    };
    if provider == Provider::Claude {
        account.identity = format!("{}:{}", email.to_lowercase(), text(row, "organizationUuid"));
        // Turned off in Settings: no usage, reset or sign-in reading (CDXC:AgentProviders 2026-10-14 on `SavedAccount::user_disabled`).
        if super::store::user_disabled(provider, &account.selector) {
            account.status = "disabled".into();
            return Some(account);
        }
        if email.is_empty() {
            account.status = "loginRequired".into();
        }
        let status = text(row, "usageStatus");
        if matches!(status.as_str(), "relogin_required" | "no_credentials") {
            account.status = "loginRequired".into();
        }
        if status == "foreign_credential" {
            account.status = "identityChanged".into();
        }
        // CDXC:AgentProviders 2026-09-18 WHY:
        // cswap reports `unavailable` with a null `usage` once its last reading is too old to drive a switch, usually because the Anthropic usage endpoint answers 429 with a one-hour retry. Its documented `lastGoodUsage` fields keep that reading for display, so Ghostex shows those bars with their age instead of the bare status code. `usage_error` stays set, and every switch decision (`default_account.rs`, `has_room` in `recovery.rs`) requires it to be absent, so a stale reading never counts as headroom. `usage_stale` marks such a reading so every client labels it as last known (with its age) instead of live.
        let (usage, fetched_at, last_good_age) = if row["usage"].is_object() {
            (&row["usage"], &row["usageFetchedAt"], None)
        } else {
            (
                &row["lastGoodUsage"],
                &row["lastGoodFetchedAt"],
                row.get("lastGoodAgeSeconds").and_then(Value::as_f64),
            )
        };
        let renewing_reading = status == "token_expired"
            && usage.is_object()
            && last_good_age.is_some_and(|age| age <= TOKEN_RENEWING_READING_SECONDS);
        account.usage_error = if renewing_reading {
            None
        } else {
            claude_usage_notice(&status, usage.is_object(), last_good_age)
        };
        // A session reported this login expired, and no reading of the saved login since says it works.
        if account.status == "ready"
            && super::store::login_expired(provider, &account.selector).is_some_and(|marked| {
                !super::login_expired::read_after(
                    &status,
                    row["usage"].is_object(),
                    row["usageFetchedAt"].as_str(),
                    &marked,
                )
            })
        {
            account.status = "loginRequired".into();
            account.usage_error = Some(super::login_expired::NOTICE.into());
        }
        // Whether this computer held a copy of the login to restore (restore.rs).
        if account.status == "loginRequired" {
            if let Some(sentence) =
                super::restore::last_plan(&account.selector).and_then(super::restore::notice)
            {
                account.usage_error = Some(match account.usage_error.take() {
                    Some(notice) => format!("{notice} {sentence}"),
                    None => sentence.to_string(),
                });
            }
        }
        account.usage_stale = !row["usage"].is_object() && usage.is_object() && !renewing_reading;
        if let Some(w) = window("fiveHour", "5h", &usage["fiveHour"], "pct", None) {
            account.usage.push(w);
        }
        if let Some(w) = window("sevenDay", "7d", &usage["sevenDay"], "pct", None) {
            account.usage.push(w);
        }
        if let Some(scoped) = usage.get("scoped").and_then(Value::as_array) {
            for item in scoped {
                let model = text(item, "name");
                if let Some(w) = window(
                    &model,
                    &format!("{model} 7d"),
                    item,
                    "pct",
                    Some(model.clone()),
                ) {
                    account.usage.push(w);
                }
            }
        }
        if let Some(w) = window("spend", "Extra usage", &usage["spend"], "pct", None) {
            account.usage.push(w);
        }
        account.usage_updated_at = fetched_at.as_str().map(str::to_string);
        if account.status == "ready" {
            match super::claude_resets::cached(home, &account.selector) {
                Ok(Some(credits)) => {
                    account.reset_credits = Some(credits.len() as u64);
                    account.reset_credit_details = Some(credits);
                }
                Ok(None) => {}
                Err(error) => account.reset_credits_error = Some(error),
            }
        }
    } else {
        account.identity = super::codex_identity::codex_identity(
            &text(row, "accountId"),
            Some(&text(row, "userId")),
        );
        account.status = match text(row, "loginStatus").as_str() {
            "present" => "ready",
            "identity_changed" => "identityChanged",
            _ => "loginRequired",
        }
        .into();
        account.shared_history = row.get("shareHistory").and_then(Value::as_bool) == Some(true);
        if super::store::user_disabled(provider, &account.selector) {
            account.status = "disabled".into();
            return Some(account);
        }
        if account.status == "ready" {
            // The reset credits are read alongside the usage, and kept only when the usage answered.
            let (usage, credits) = std::thread::scope(|scope| {
                let credits = scope.spawn(|| super::reset_credits::read(row));
                (
                    codex_usage(row),
                    credits
                        .join()
                        .unwrap_or_else(|_| Err("Reset expiry details are unavailable.".into())),
                )
            });
            match usage {
                Ok((usage, reset_credits)) => {
                    account.usage = usage;
                    account.reset_credits = reset_credits;
                    account.usage_updated_at = Some(chrono::Utc::now().to_rfc3339());
                    match credits {
                        Ok(credits) => {
                            account.reset_credits = Some(credits.len() as u64);
                            account.reset_credit_details = Some(credits);
                        }
                        Err(error) => account.reset_credits_error = Some(error),
                    }
                }
                Err(e) => account.usage_error = Some(e),
            }
        }
    }
    Some(account)
}
pub(crate) fn codex_get(row: &Value, path: &str) -> Result<Value, String> {
    let response = match codex_request(row, ureq::Agent::get, path)?.call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(401)) => {
            return Err("Sign in again to refresh account usage.".into());
        }
        Err(ureq::Error::StatusCode(429)) => {
            return Err("Usage requests are temporarily limited. Ghostex will try again.".into());
        }
        Err(_) => return Err("Usage could not be refreshed. Ghostex will try again.".into()),
    };
    crate::http_agent::read_json(response)
        .map_err(|_| "The usage service returned an invalid response.".to_string())
}
/// A ChatGPT backend request authorized as the xswap account in `row`, after checking its saved login still belongs to that account. `method` is `ureq::Agent::get` or `ureq::Agent::post`.
pub(crate) fn codex_request<B>(
    row: &Value,
    method: impl FnOnce(&ureq::Agent, String) -> ureq::RequestBuilder<B>,
    path: &str,
) -> Result<ureq::RequestBuilder<B>, String> {
    let home = PathBuf::from(text(row, "home"));
    if !home.is_absolute() {
        return Err("The saved login's folder can't be found. Reconnect this account.".into());
    }
    let raw = std::fs::read(home.join("auth.json")).map_err(|_| "Sign in again to read usage.")?;
    let auth: Value = serde_json::from_slice(&raw)
        .map_err(|_| "The saved login can't be read. Reconnect this account.")?;
    let expected = text(row, "accountId");
    // The member is checked too: a slot whose login moved to another member of the same Business workspace keeps the workspace's `account_id`.
    let saved = super::codex_identity::codex_identity(&expected, Some(&text(row, "userId")));
    if auth.pointer("/tokens/account_id").and_then(Value::as_str) != Some(expected.as_str())
        || !super::codex_identity::codex_auth_identity(&auth).is_some_and(|login| {
            super::codex_identity::identity_matches(Provider::Codex, &saved, &login)
        })
    {
        return Err("This account's login identity changed. Reconnect it.".into());
    }
    let token = auth
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .ok_or("Sign in again to read usage.")?;
    Ok(method(
        crate::http_agent::agent(),
        format!("https://chatgpt.com/backend-api/wham/{path}"),
    )
    .header("Authorization", format!("Bearer {token}"))
    .header("ChatGPT-Account-Id", &expected)
    .header("Accept", "application/json")
    .header("OpenAI-Beta", "codex-1")
    .header("originator", "Codex Desktop")
    .config()
    .timeout_global(Some(Duration::from_secs(15)))
    .build())
}
fn codex_usage(row: &Value) -> Result<(Vec<UsageWindow>, Option<u64>), String> {
    let value = codex_get(row, "usage")?;
    let mut windows = Vec::new();
    codex_windows(&mut windows, "", &value["rate_limit"]);
    if let Some(extras) = value
        .get("additional_rate_limits")
        .and_then(Value::as_array)
    {
        for extra in extras {
            codex_windows(
                &mut windows,
                &text(extra, "limit_name"),
                &extra["rate_limit"],
            );
        }
    }
    if windows.is_empty() {
        return Err("Codex reported no usage limits for this account.".into());
    }
    let reset_credits = value
        .pointer("/rate_limit_reset_credits/available_count")
        .and_then(Value::as_u64);
    Ok((windows, reset_credits))
}
fn codex_windows(out: &mut Vec<UsageWindow>, model: &str, value: &Value) {
    for key in ["primary_window", "secondary_window"] {
        let v = &value[key];
        let Some(used) = v.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        let seconds = v
            .get("limit_window_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if seconds <= 0 || !used.is_finite() {
            continue;
        }
        let label = duration_label(seconds);
        let reset = v
            .get("reset_at")
            .and_then(Value::as_i64)
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
            .map(|t| t.to_rfc3339());
        out.push(UsageWindow {
            id: format!("{model}:{key}"),
            label: if model.is_empty() {
                label
            } else {
                format!("{model} {label}")
            },
            used_percent: used.clamp(0., 100.),
            limit_window_seconds: Some(seconds),
            resets_at: reset,
            model: (!model.is_empty()).then(|| model.to_string()),
        });
    }
}

#[cfg(test)]
mod account_message_tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde_json::json;

    #[test]
    fn usage_notices_use_no_internal_words() {
        for status in [
            "unavailable",
            "token_expired",
            "relogin_required",
            "no_credentials",
            "foreign_credential",
            "api_key",
            "keychain_unavailable",
            "some_new_status",
        ] {
            for (has_last_good, age) in [(false, None), (true, None), (true, Some(90.0))] {
                let notice = claude_usage_notice(status, has_last_good, age).unwrap();
                for word in ["helper", "status", "some_new_status", "cswap", "xswap"] {
                    assert!(!notice.contains(word), "{status}: {notice}");
                }
            }
        }
        assert_eq!(claude_usage_notice("ok", true, None), None);
    }

    #[test]
    fn the_tools_go_by_their_settings_names() {
        assert_eq!(tool_label("cswap"), "Claude Swap");
        assert_eq!(tool_label("xswap"), "Codex Swap");
    }

    fn auth_with_member(member: &str) -> Value {
        let claims = json!({"https://api.openai.com/auth": {"chatgpt_user_id": member}});
        let token = format!(
            "e30.{}.sig",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        );
        json!({"tokens": {"account_id": "workspace", "id_token": token}})
    }

    /// An older xswap lists no `userId`. The login must keep the id its saved account has.
    #[test]
    fn a_login_keeps_one_id_whichever_xswap_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("auth.json"),
            auth_with_member("member-1").to_string(),
        )
        .unwrap();
        let home = dir.path().to_str().unwrap();
        let mut old = json!({"accounts": [{"accountId": "workspace", "home": home}]});
        fill_codex_members(&mut old);
        assert_eq!(old["accounts"][0]["userId"], "member-1");
        let identity = |row: &Value| {
            super::super::codex_identity::codex_identity(
                &text(row, "accountId"),
                Some(&text(row, "userId")),
            )
        };
        let new = json!({"accountId": "workspace", "home": home, "userId": "member-1"});
        assert_eq!(identity(&old["accounts"][0]), identity(&new));
    }

    #[test]
    fn a_listed_member_stands_and_a_row_without_a_readable_login_stays_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("auth.json"),
            auth_with_member("from-file").to_string(),
        )
        .unwrap();
        let home = dir.path().to_str().unwrap();
        let mut data = json!({"accounts": [
            {"home": home, "userId": "listed"},
            {"home": "/nowhere/at/all"},
            {},
        ]});
        fill_codex_members(&mut data);
        assert_eq!(data["accounts"][0]["userId"], "listed");
        assert!(data["accounts"][1].get("userId").is_none());
        assert!(data["accounts"][2].get("userId").is_none());
    }
}
