use crate::*;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement as _;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::v_flex;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::Duration,
};

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// One entry per account across every machine, this computer's copy first.
///
/// CDXC:AgentProviders 2026-10-30 WHY:
/// User: "why do i see accounts doubled up here in the sidebar". A provider login signed in on
/// this computer and on a connected machine comes back from both gxservers under the same account
/// id (the id is derived from the login, not the machine) with the same limits, but each machine's
/// copy carries its own `titlebarKey`, so the sidebar drew it twice. The limits belong to the
/// provider account, so it shows once: this computer's copy when it has one, else the first
/// machine's in the published order. An account only another machine has still shows.
///
/// CDXC:AgentProviders 2026-11-19 DECISION:
/// A CLIProxyAPI hub account (`hub: true`, server/src/accounts/hub/sidebar.rs) whose login is also
/// a Claude Swap or Codex Swap account on any computer shows once, as the Swap copy, which has the
/// account's actions (the coordinator's interim answer while the user's own is pending: "assume
/// 'once, the Swap copy' for now and keep it easy to flip"). [`HUB_COPY_YIELDS_TO_SWAP`] flips it.
fn distinct_accounts(accounts: &[Value]) -> Vec<&Value> {
    distinct_account_copies(accounts)
        .into_iter()
        .map(|copies| copies[0])
        .collect()
}

/// [`distinct_accounts`] with every computer's copy of each account: the shown copy first, then
/// the other computers' copies in the published order.
///
/// CDXC:AgentProviders 2026-11-19 DECISION:
/// User (Q4198 A, the signed-out accounts page): a signed-out account's card is marked, and only
/// its icon's dot says it is signed out on another computer only, so the card has to know every
/// computer's copy, not only the one it shows. Since 2026-11-21 the mark is a dot on the icon's
/// top-right corner, not amber letters and numbers (`render_account_usage_meter`).
fn distinct_account_copies(accounts: &[Value]) -> Vec<Vec<&Value>> {
    let saved = swap_logins();
    let swap_login = |hub: &Value| {
        HUB_COPY_YIELDS_TO_SWAP
            && hub["hub"] == true
            && (saved.iter().any(|login| same_login(login, hub))
                // A computer whose gxserver sends no `swapLogins` yet: its shown Swap rows.
                || accounts
                    .iter()
                    .any(|other| other["hub"] != true && same_login(other, hub)))
    };
    let mut shown: Vec<Vec<&Value>> = Vec::with_capacity(accounts.len());
    for account in accounts.iter().filter(|account| !swap_login(account)) {
        let same = shown.iter().position(|copies| {
            let seen = copies[0];
            text(seen, "provider") == text(account, "provider")
                && !text(account, "id").is_empty()
                && text(seen, "id") == text(account, "id")
        });
        match same {
            None => shown.push(vec![account]),
            Some(index)
                if text(account, "titlebarMachine") == "local"
                    && text(shown[index][0], "titlebarMachine") != "local" =>
            {
                shown[index].insert(0, account);
            }
            Some(index) => shown[index].push(account),
        }
    }
    shown
}

/// Whether gxserver reads this copy of an account as needing a sign-in: its saved login is dead
/// or a session on it reported "Login expired" (`loginRequired`, server/src/accounts/login_expired.rs),
/// the login now belongs to someone else (`identityChanged`), or it was synced from another
/// computer and was never signed in on this one (`unavailable` with `syncedFrom`,
/// server/src/accounts/endpoint.rs). A plain `unavailable` (the saved login is missing for another
/// reason) is not drawn as signed out. A copy whose computer's connection was stopped
/// (`connectionStopped`, [`flag_stopped_machine_accounts`]) is not either: its state is from before
/// the connection ended, so it reads as a computer that can't be reached.
fn signed_out(account: &Value) -> bool {
    needs_sign_in(account) && account["signInIgnored"] != true
}

/// [`signed_out`] before the user's "Don't use on <computer>" (`signInIgnored`, gxserver's
/// `SavedAccount::sign_in_ignored`): a copy that needs a sign-in the user said it won't get.
fn needs_sign_in(account: &Value) -> bool {
    if account["connectionStopped"] == true {
        return false;
    }
    match text(account, "status") {
        "loginRequired" | "identityChanged" => true,
        "unavailable" => !text(account, "syncedFrom").is_empty(),
        _ => false,
    }
}

/// A computer's accounts whose connection was stopped: they stay (an account only that computer
/// has keeps its card), but faded and never amber, until the computer answers again and its rows
/// are replaced (`refresh_titlebar_accounts`).
pub(crate) fn flag_stopped_machine_accounts(accounts: &mut [Value], machine: &str) {
    for account in accounts
        .iter_mut()
        .filter(|account| text(account, "titlebarMachine") == machine)
    {
        account["connectionStopped"] = json!(true);
        account["machineUnreachable"] = json!(true);
    }
}

/// How long a computer's reads may keep failing before its cards fade, even if only one failed.
const UNREACHABLE_FADE_AFTER_MS: f64 = 90_000.0;

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Marks a computer's copy after a read of it failed. `counted` is whether this failure counts:
/// one per refresh, and none when that refresh's cached read already answered (a slow full read
/// is not a computer that can't be reached). The card fades only after two counted failures in a
/// row, or once failures have gone on for [`UNREACHABLE_FADE_AFTER_MS`], so one dropped read
/// doesn't flicker it.
fn note_failed_read(account: &mut Value, counted: bool, now: f64) {
    if counted {
        // Capped at 2, where the card fades: a computer that keeps failing changes nothing more,
        // so the sidebar doesn't redraw for it on every 30-second poll.
        let reads = (account["unreachableReads"].as_u64().unwrap_or(0) + 1).min(2);
        account["unreachableReads"] = json!(reads);
        if !account["unreachableSince"].is_number() {
            account["unreachableSince"] = json!(now);
        }
    }
    let reads = account["unreachableReads"].as_u64().unwrap_or(0);
    let long = account["unreachableSince"]
        .as_f64()
        .is_some_and(|since| now - since >= UNREACHABLE_FADE_AFTER_MS);
    if reads >= 2 || long {
        account["machineUnreachable"] = json!(true);
    }
}

/// A computer's name as the user saved it in Settings > Remote, or this computer's own name.
fn machine_label(machine: &str) -> String {
    if matches!(machine, "" | "local") {
        return crate::host_name::local_computer_name();
    }
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(Value::as_array)
        .and_then(|machines| machines.iter().find(|entry| text(entry, "id") == machine))
        .map(|entry| text(entry, "name").trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Another computer".to_string())
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "codex" => "Codex",
        "cursor" => "Cursor",
        "copilot" => "Copilot",
        _ => "Claude",
    }
}

/// The amber Settings > Accounts gives "Needs sign-in" (settings_modal/tabs/accounts/widgets.rs
/// `TagTone::Warning`): the dot on a signed-out card's icon (its letters and numbers were this
/// colour until 2026-11-21).
///
/// CDXC:AgentProviders 2026-11-19 WHY:
/// In light mode a Claude card's letters are Claude's own #9c4328, and the Settings amber #b45309
/// sat too close to it to tell a signed-out Claude card at a glance (the review of 03107bf0d). A
/// light Claude card takes a darker ochre, #785800: hue 44 against Claude's 14, and 5.2:1 on the
/// light card (#e5e5e6, the 96% light chrome under the card's 5% black). Everywhere else keeps the
/// Settings amber.
pub(crate) fn needs_sign_in_amber(claude: bool) -> gpui::Rgba {
    chrome_color(0xfcd34d, if claude { 0x785800 } else { 0xb45309 })
}

/// How a card shows that its account needs signing in somewhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MeterSignIn {
    #[default]
    SignedIn,
    /// The copy the card shows (this computer's, when it has one) is signed out: the icon's dot.
    SignedOut,
    /// The shown copy is signed in, another computer's is not: the same dot, and a tooltip that
    /// names the computer.
    SignedOutElsewhere,
}

/// A card's sign-in state, whether its shown copy's computer could not be reached, and the
/// tooltip that lists every computer's copy, for a card that draws amber or faded only.
///
/// CDXC:AgentProviders 2026-11-19 DECISION:
/// User (Q4198 A): a card with a problem gets a tooltip naming each computer and the account's
/// state there; a card without one keeps no tooltip (the 2026-09-21 "please don't show tooltips
/// for the account usage cards" still holds for them). A card whose computer did not answer fades
/// its numbers, which are that computer's last reading.
fn meter_state(label: &str, copies: &[&Value]) -> (MeterSignIn, bool, Option<String>) {
    let unreachable = |account: &Value| account["machineUnreachable"] == true;
    let sign_in = if signed_out(copies[0]) {
        MeterSignIn::SignedOut
    } else if copies[1..].iter().any(|copy| signed_out(copy)) {
        MeterSignIn::SignedOutElsewhere
    } else {
        MeterSignIn::SignedIn
    };
    // Only a card that looks different (amber, or faded) explains itself: another computer that
    // didn't answer leaves this card as it is, and so without a tooltip.
    let problem = sign_in != MeterSignIn::SignedIn || unreachable(copies[0]);
    let tooltip = problem.then(|| {
        let mut lines = vec![format!(
            "{label} · {}",
            provider_label(text(copies[0], "provider"))
        )];
        for copy in copies {
            let state = if signed_out(copy) {
                "signed out"
            } else if needs_sign_in(copy) {
                "signed out, not used there"
            } else if unreachable(copy) {
                "can't reach it, showing its last reading"
            } else {
                "signed in"
            };
            lines.push(format!(
                "{}: {state}",
                machine_label(text(copy, "titlebarMachine"))
            ));
        }
        let shown_here = matches!(text(copies[0], "titlebarMachine"), "" | "local");
        match sign_in {
            // The card's popup has the account's Reconnect.
            MeterSignIn::SignedOut if shown_here => lines.push("Click to reconnect it.".into()),
            // The popup signs it in on that computer, from Settings > Accounts' machine picker.
            MeterSignIn::SignedOut | MeterSignIn::SignedOutElsewhere => {
                lines.push("Click to sign it in again there.".into())
            }
            MeterSignIn::SignedIn => {}
        }
        lines.join("\n")
    });
    (sign_in, unreachable(copies[0]), tooltip)
}

/// The account the usage popup `key` opens on, with every computer's copy of it listed in
/// `signInComputers`: `{machine, name, here, shown, state}`, `state` being `signedOut`,
/// `ignored` (signed out, and the user said it isn't used there), `unreachable` or `signedIn`,
/// the card's first.
///
/// CDXC:AgentProviders 2026-11-20 DECISION:
/// User, of an amber card whose popup said nothing about it: "I see it's yellow so it's not
/// connected, but when I open it it doesn't say anything about this so I can fix it with 1 click".
/// The popup opens on the card's copy, so it has to be told how the account stands on every
/// computer to say where it is signed out, offer the sign-in there, and say where it is fine.
pub(crate) fn usage_popup_account(accounts: &[Value], key: &str) -> Option<Value> {
    let is_key = |account: &&Value| text(account, "titlebarKey") == key;
    let shown = accounts.iter().find(is_key)?;
    let copies = distinct_account_copies(accounts)
        .into_iter()
        .find(|copies| copies.iter().any(is_key))
        .unwrap_or_else(|| vec![shown]);
    let computers: Vec<Value> = copies
        .iter()
        .map(|copy| {
            let machine = match text(copy, "titlebarMachine") {
                "" => "local",
                machine => machine,
            };
            let state = if signed_out(copy) {
                "signedOut"
            } else if needs_sign_in(copy) {
                "ignored"
            } else if copy["machineUnreachable"] == true {
                "unreachable"
            } else {
                "signedIn"
            };
            json!({
                "machine": machine,
                "name": machine_label(machine),
                "here": machine == "local",
                "shown": is_key(copy),
                "state": state,
            })
        })
        .collect();
    let mut account = shown.clone();
    account["signInComputers"] = json!(computers);
    Some(account)
}

/// Whether a hub account's meter gives way to the Swap account of the same login (see
/// [`distinct_accounts`]); `false` shows both.
const HUB_COPY_YIELDS_TO_SWAP: bool = true;

/// Each computer's saved Claude Swap and Codex Swap logins that aren't turned off (`swapLogins`
/// of its accounts answer), shown in the sidebar or not: hiding a Swap account's meter must not
/// bring its hub twin's meter up instead.
fn swap_logins_by_machine()
-> &'static std::sync::Mutex<std::collections::BTreeMap<String, Vec<Value>>> {
    static LOGINS: OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, Vec<Value>>>> =
        OnceLock::new();
    LOGINS.get_or_init(Default::default)
}

fn swap_logins() -> Vec<Value> {
    swap_logins_by_machine()
        .lock()
        .map(|logins| logins.values().flatten().cloned().collect())
        .unwrap_or_default()
}

/// Keeps `machine`'s Swap logins; whether they changed.
fn remember_swap_logins(machine: &str, logins: Vec<Value>) -> bool {
    let Ok(mut all) = swap_logins_by_machine().lock() else {
        return false;
    };
    all.insert(machine.to_string(), logins.clone()) != Some(logins)
}

/// A Swap login and a hub account are one login: same provider and email, and the same Codex
/// workspace when both name one.
fn same_login(login: &Value, hub: &Value) -> bool {
    let email = text(hub, "email");
    let (left, right) = (text(login, "workspace"), text(hub, "workspace"));
    !email.is_empty()
        && text(login, "provider") == text(hub, "provider")
        && text(login, "email").eq_ignore_ascii_case(email)
        && (left.is_empty() || right.is_empty() || left == right)
}

fn titlebar_entry(account: &Value, machine: &str) -> Value {
    let mut account = account.clone();
    let key = machine
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    account["titlebarKey"] = json!(format!("account-{key}-{}", text(&account, "id")));
    account["titlebarMachine"] = json!(machine);
    account
}

/// The account logo the titlebar entry and the Account Usage popup draw, embedded once for both:
/// Codex's own artwork, and for Claude the agent logo (the same bytes the sidebar draws).
///
/// CDXC:AgentProviders 2026-09-09 WHY:
/// The Codex artwork matches the shared chat SVG; the old 100px SVG had internal padding that made its titlebar icon visibly smaller than Claude.
pub(crate) fn account_logo_svg(codex: bool) -> &'static str {
    if codex {
        include_str!("../../../assets/account-usage/codex.svg")
    } else {
        ghostex_gx_core::agent_logo_svg("claude").expect("the Claude agent logo")
    }
}

/// The Cursor and Copilot accounts only show usage (server/src/accounts/usage_only/). Ghostex reads
/// no Antigravity usage (server/src/accounts/usage_only/retire.rs), so an Antigravity meter from an
/// older gxserver on another computer is not shown.
fn usage_only_provider(provider: &str) -> bool {
    matches!(provider, "cursor" | "copilot")
}

/// A usage-only provider's agent logo, in the chrome's ink where the artwork is plain white.
pub(crate) fn usage_only_icon(provider: &str) -> Arc<gpui::Image> {
    static ICONS: OnceLock<
        std::sync::Mutex<std::collections::HashMap<(String, bool), Arc<gpui::Image>>>,
    > = OnceLock::new();
    let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
    let mut icons = ICONS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    icons
        .entry((provider.to_string(), light))
        .or_insert_with(|| {
            let key = if provider == "cursor" {
                "cursor-cli"
            } else {
                provider
            };
            let ink = if light { "#333333" } else { "#edecec" };
            let svg = ghostex_gx_core::agent_logo_svg(key)
                .unwrap_or_default()
                .replace("currentColor", ink)
                .replace("fill=\"#fff\"", &format!("fill=\"{ink}\""));
            Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                svg.into_bytes(),
            ))
        })
        .clone()
}

fn icon(codex: bool) -> Arc<gpui::Image> {
    static CODEX: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    static CLAUDE: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    static CODEX_LIGHT: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    static CLAUDE_LIGHT: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
    let (slot, svg) = if codex {
        (
            if light { &CODEX_LIGHT } else { &CODEX },
            account_logo_svg(true),
        )
    } else {
        (
            if light { &CLAUDE_LIGHT } else { &CLAUDE },
            account_logo_svg(false),
        )
    };
    slot.get_or_init(|| {
        let svg = if light {
            if codex {
                svg.replace("#ffffff", "#285b8c")
            } else {
                svg.replace("#d97757", "#9c4328")
            }
        } else {
            svg.to_string()
        };
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            svg.into_bytes(),
        ))
    })
    .clone()
}

/// The account's name as the user wants it shown: masked when Settings hides
/// account emails. Shared by the titlebar account popup and the New Thread picker.
pub(crate) fn account_display_name(account: &Value) -> String {
    // Screenshot mode names no account, only its slot (CDXC:ScreenshotMode 2026-11-20 in
    // app/screenshot_mode.rs).
    if crate::app::screenshot_mode::screenshot_mode_on() {
        return match text(account, "selector") {
            "" => "Account".to_string(),
            selector => format!("Account {selector}"),
        };
    }
    account_display_text(text(account, "name"))
}

/// CDXC:AgentProviders 2026-09-10 WHY: Account errors can include email addresses too; mask each address while preserving the surrounding explanation in native pickers and usage popups.
pub(crate) fn account_display_text(value: &str) -> String {
    let hidden = crate::app::screenshot_mode::account_emails_hidden();
    if !hidden {
        return value.to_string();
    }
    value
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let token = part.trim_end_matches(char::is_whitespace);
            let whitespace = &part[token.len()..];
            match token.split_once('@') {
                Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
                    let mut characters = local.chars();
                    let first = characters.next().unwrap();
                    let last = characters.last().map(|c| c.to_string()).unwrap_or_default();
                    format!("{first}•••{last}@•••••.•••{whitespace}")
                }
                _ => part.to_string(),
            }
        })
        .collect()
}

pub(crate) fn popup_account(account: &Value) -> Value {
    let mut account = account.clone();
    account["displayName"] = json!(account_display_name(&account));
    if let Some(error) = account["usageError"].as_str() {
        account["usageError"] = json!(account_display_text(error));
    }
    account
}

fn badge_lines(account: &Value) -> Vec<String> {
    let windows = account["usage"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let pct = |w: &Value| w["usedPercent"].as_f64().map(|v| format!("{:.0}", v));
    if usage_only_provider(text(account, "provider")) {
        // CDXC:AgentProviders 2026-11-07 DECISION:
        // User, with a screenshot of the sidebar's usage meters (Claude and Codex stacked "74%" over "0%", Gemini packed as "0/0%"): "pls make the gemini one 2 lines like the others". A usage-only provider draws one stacked "N%" line per window, in the order gxserver lists them (Gemini's 5h then 7d); Cursor's and Copilot's single Mo window stays one line. The tile has no tooltip (2026-09-21), so the window labels stay in the usage popup.
        // CDXC:AgentProviders 2026-11-08 DECISION:
        // Gemini also listed Antigravity's Claude/GPT group as model-scoped 5h and 7d windows (both are retired since 2026-11-27, server/src/accounts/usage_only/retire.rs). The tile keeps one line per main window, and each line shows the most used window of that length across every group, so the limit agy hits first is the one on the tile; the settings pane and the chat's accounts panel list each group.
        return windows
            .iter()
            .filter(|w| w["model"].is_null())
            .filter_map(|main| {
                let seconds = main["limitWindowSeconds"].as_i64();
                windows
                    .iter()
                    .filter(|w| {
                        std::ptr::eq(*w, main)
                            || (seconds.is_some() && w["limitWindowSeconds"].as_i64() == seconds)
                    })
                    .filter_map(|w| w["usedPercent"].as_f64())
                    .reduce(f64::max)
                    .map(|v| format!("{v:.0}%"))
            })
            .collect();
    }
    if text(account, "provider") == "codex" {
        let main: Vec<_> = windows.iter().filter(|w| w["model"].is_null()).collect();
        let session = main
            .iter()
            .find(|w| w["limitWindowSeconds"].as_i64() == Some(18000));
        let weekly = main.iter().find(|w| {
            w["limitWindowSeconds"]
                .as_i64()
                .is_some_and(|s| s >= 604800)
        });
        let usage = [session, weekly]
            .into_iter()
            .flatten()
            .filter_map(|w| pct(w))
            .collect::<Vec<_>>();
        let mut lines = Vec::new();
        if !usage.is_empty() {
            lines.push(format!("{}%", usage.join("/")));
        }
        if let Some(resets) = account["resetCredits"].as_u64() {
            lines.push(format!("{resets}rs"));
        }
        lines
    } else {
        let headline = claude_headline_windows(windows);
        // CDXC:AgentProviders 2026-11-21 DECISION:
        // User: "why can't u see that they have resets btw? please add to sessions in ghostex the ability to see the resets for my claude accounts". The tile has two lines, both Claude's headline percentages. Once one of them is used up and the account still has a limit reset, the other line becomes the reset count ("100%" over "1rs", as a Codex tile writes it), so a stuck account says at a glance that a reset can free it; the usage popup lists the resets with their Use buttons.
        let resets = account["resetCredits"].as_u64().filter(|n| *n > 0);
        if let (Some(full), Some(resets)) = (
            headline
                .iter()
                .find(|w| w["usedPercent"].as_f64().is_some_and(|v| v >= 100.0)),
            resets,
        ) {
            return pct(full)
                .map(|v| format!("{v}%"))
                .into_iter()
                .chain([format!("{resets}rs")])
                .collect();
        }
        headline
            .into_iter()
            .filter_map(|w| pct(w).map(|v| format!("{v}%")))
            .collect()
    }
}

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: for Claude accounts the Fable limit is the most important number and must never be hidden. Wherever a Claude account shows two percentages, show the two tightest of the five-hour, weekly, and Fable limits, in that fixed order (five-hour first, as the usage popup lists them; changed from weekly first on 2026-11-22), so the number about to run out is always one of them. Port of `accountHeadlineWindows` in packages/shared/account-usage-windows.ts (deleted 2026-10-01); the popup in apps/desktop/src/app/window/account_usage/limits.rs shows all three as main bars.
pub(crate) fn claude_headline_windows(windows: &[Value]) -> Vec<&Value> {
    let main: Vec<&Value> = windows.iter().filter(|w| w["model"].is_null()).collect();
    let weekly = main.iter().copied().find(|w| {
        text(w, "id") == "sevenDay" || w["limitWindowSeconds"].as_i64().unwrap_or(0) >= 604_800
    });
    let five_hour = main
        .iter()
        .copied()
        .find(|w| text(w, "id") == "fiveHour" || w["limitWindowSeconds"].as_i64() == Some(18_000));
    let scoped: Vec<&Value> = windows.iter().filter(|w| w["model"].is_string()).collect();
    let fable = scoped
        .iter()
        .copied()
        .find(|w| text(w, "model").to_lowercase().contains("fable"))
        .or_else(|| scoped.first().copied());
    let candidates: Vec<&Value> = [five_hour, weekly, fable].into_iter().flatten().collect();
    let mut by_usage = candidates.clone();
    by_usage.sort_by(|a, b| {
        b["usedPercent"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["usedPercent"].as_f64().unwrap_or(0.))
    });
    let tightest: Vec<*const Value> = by_usage
        .iter()
        .take(2)
        .map(|w| *w as *const Value)
        .collect();
    candidates
        .into_iter()
        .filter(|w| tightest.contains(&(*w as *const Value)))
        .collect()
}

/// One account's meter: the provider glyph with the account label on it and the two
/// tightest numbers beside it, without any of the chrome its host puts around it.
pub(crate) struct GpuiAccountUsageMeter {
    pub(crate) id: ExtensionId,
    pub(crate) codex: bool,
    /// `claude`, `codex`, or a usage-only provider (`cursor`, `copilot`).
    pub(crate) provider: String,
    pub(crate) indicator: Option<String>,
    pub(crate) badge_lines: Vec<String>,
    pub(crate) sign_in: MeterSignIn,
    /// The shown copy's computer did not answer: its numbers are its last reading, faded.
    pub(crate) unreachable: bool,
    /// Only on a card with a problem ([`meter_state`]).
    pub(crate) tooltip: Option<String>,
}

/// The badge text size beside a meter's glyph. A host that lines its meters up in columns
/// needs it, and the renderer below, to agree on how wide a character cell is.
pub(crate) const ACCOUNT_USAGE_BADGE_TEXT_SIZE: f32 = 9.5;

/// The glyph box drawn beside the badge, and the gap between the two.
pub(crate) const ACCOUNT_USAGE_BADGE_GLYPH_WIDTH: f32 = 19.2;
pub(crate) const ACCOUNT_USAGE_BADGE_GAP: f32 = 4.0;

/// The chrome the hosting surface wants around a meter. The meter renderer takes this
/// rather than reading the titlebar constants, so one implementation serves every host.
pub(crate) struct GpuiAccountUsageMeterHost {
    pub(crate) element_id_prefix: &'static str,
    pub(crate) anchor_key_prefix: &'static str,
    pub(crate) height: f32,
    pub(crate) padding_x: f32,
    pub(crate) corner_radius: f32,
    /// The meter's resting fill. Hosts that want the meters to read as cards pass a
    /// visible one; a transparent value leaves the meter on the host's own background.
    pub(crate) background: gpui::Hsla,
    pub(crate) hover_background: gpui::Hsla,
    /// The fill for the meter whose usage popup is open. It must stay distinct from the
    /// host's own hovered background rather than being swallowed by it.
    pub(crate) open_background: gpui::Hsla,
    /// Whether the meter fills the cell it is given instead of hugging its content. The
    /// meter centres its content either way, so a filled cell centres it in the column.
    pub(crate) fill_width: bool,
    pub(crate) scale: f32,
    /// Set when the meters are drawn in a window other than the one their popup and menu belong
    /// to (the sidebar's frosted usage strip): presses are handed to that window.
    pub(crate) route: Option<GpuiAccountUsageMeterRoute>,
    /// The host's tooltip delay, for a card that has a tooltip.
    pub(crate) tooltip_delay: Duration,
}

/// Where a meter drawn in a frosted host window sends its presses: the window the account's usage
/// popup and menu belong to, and where the drawing window's content origin sits in it.
#[derive(Clone, Copy)]
pub(crate) struct GpuiAccountUsageMeterRoute {
    pub(crate) window: gpui::AnyWindowHandle,
    pub(crate) offset: gpui::Point<Pixels>,
}

impl GhostexGpuiApp {
    /// Re-places the open usage popup for its panel's measured `height`: above its tile, as tall
    /// as the content or the room there (CDXC:AgentProviders 2026-10-30 on
    /// `account_usage_popup_frame`). Moved where the platform can move a window, else resized.
    pub(crate) fn fit_account_usage_popup(
        &mut self,
        id: ExtensionId,
        height: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiTitlebarPopupKind::AccountUsage(id);
        let (Some(state), Some(popup), Some(main)) = (
            self.titlebar_popup_menu
                .as_ref()
                .filter(|state| state.kind == kind),
            self.titlebar_popup_window,
            self.main_window_handle,
        ) else {
            return;
        };
        let trigger = state.trigger_bounds;
        let Ok((frame, parent)) = cx.update_window(main, |_, window, _| {
            let screen = titlebar_popup_window_bounds_for_trigger_bounds(
                kind,
                trigger,
                height,
                titlebar_popup_menu_width(kind),
                window,
            );
            let origin = window.bounds().origin;
            (
                Bounds::new(screen.origin - origin, screen.size),
                crate::app::native_chat::child_window::window_native_view(window),
            )
        }) else {
            return;
        };
        let current = popup.update(cx, |_, window, _| window.bounds().size).ok();
        if current == Some(frame.size) {
            return;
        }
        let moved = parent.is_some_and(|parent| {
            crate::app::native_chat::child_window::move_child_window(
                popup.into(),
                parent,
                frame,
                cx,
            )
        });
        if !moved {
            let _ = popup.update(cx, |_, window, _| window.resize(frame.size));
        }
    }

    /// Shows the usage popup `id` once it has fitted itself: its glass and fade, on the next frame
    /// of the popup's own window.
    pub(crate) fn reveal_account_usage_popup(
        &mut self,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.reveal_titlebar_popup(GpuiTitlebarPopupKind::AccountUsage(id), cx);
    }

    /// A usage notice's button: the popup closes and Settings > Accounts opens, on `reconnect`'s
    /// sign-in when it names an account, on `machine` (a remote computer's id) when it names one.
    pub(crate) fn open_accounts_from_usage_popup(
        &mut self,
        id: ExtensionId,
        reconnect: Option<String>,
        machine: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let popup = self.titlebar_popup_window;
        self.clear_gpui_titlebar_popup_from_window(GpuiTitlebarPopupKind::AccountUsage(id), cx);
        if let Some(popup) = popup {
            let _ = popup.update(cx, |_, window, _| window.remove_window());
        }
        self.open_gpui_settings_accounts_reconnect(
            reconnect.as_deref(),
            machine.as_deref(),
            None,
            cx,
        );
    }

    pub(crate) fn sync_titlebar_account_privacy(&self, cx: &mut gpui::Context<Self>) {
        let Some(GpuiTitlebarPopupKind::AccountUsage(id)) =
            self.titlebar_popup_menu.as_ref().map(|state| state.kind)
        else {
            return;
        };
        if let Some(account) = usage_popup_account(&self.titlebar_accounts, id.as_str())
            && let Some(popup) = self.titlebar_popup_window
        {
            let account = popup_account(&account);
            let _ = popup.update(cx, |popup, _, cx| popup.update_account_usage(account, cx));
        }
    }

    /// An account's sidebar meter changed in Settings: shown, hidden, or (`added`) just added.
    ///
    /// CDXC:Sidebar 2026-11-05 DECISION:
    /// User: "we should show the account usage pinned when a new account is added to it so ppl know where th account got aded". An account added in Settings > Accounts pins the sidebar's account usage strip, even after the user unpinned it; the pin is saved as usual, so unpinning again is remembered until the next account is added.
    pub(crate) fn update_titlebar_account_from_ui(
        &mut self,
        message: &Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = text(message, "machineId");
        let account = &message["account"];
        if machine != "local" && !self.remote_gxserver_connections.contains_key(machine) {
            return;
        }
        if !matches!(
            text(account, "provider"),
            "claude" | "codex" | "cursor" | "copilot"
        ) || account["registered"] != true
        {
            return;
        }
        let account = titlebar_entry(account, machine);
        let Some(id) = ExtensionId::new(text(&account, "titlebarKey")) else {
            return;
        };
        self.titlebar_accounts_revision = self.titlebar_accounts_revision.wrapping_add(1);
        self.titlebar_accounts
            .retain(|a| text(a, "titlebarKey") != id.as_str());
        if account["showInTitlebar"] == true {
            self.titlebar_accounts.push(account);
            if message["added"] == true && !self.sidebar_usage_visible {
                self.sidebar_usage_visible = true;
                self.persist_shell_layout_state();
            }
        } else if self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::AccountUsage(id)) {
            self.close_gpui_titlebar_popup(None, window, cx);
        }
        self.refresh_titlebar_accounts(cx);
        cx.notify();
    }

    /// CDXC:AgentProviders 2026-10-23 WHY:
    /// The meters are only seen in a shown window, so a minimized or covered window skips the 30-second refresh (two gxserver reads per machine) and refreshes once as soon as it is shown again.
    pub(crate) fn start_titlebar_account_polling(&mut self, cx: &mut gpui::Context<Self>) {
        self.refresh_titlebar_accounts(cx);
        let skipped = Rc::new(Cell::new(false));
        if let Some(main) = self.main_window_handle {
            let app = cx.weak_entity();
            let skipped = skipped.clone();
            // Deferred: the window is still being opened while the app is built.
            gpui::App::defer(cx, move |cx| {
                let _ = main.update(cx, |_, window, _| {
                    window
                        .observe_window_visibility(move |visibility, _, cx| {
                            if visibility.is_visible() && skipped.replace(false) {
                                let _ = app.update(cx, |app, cx| app.refresh_titlebar_accounts(cx));
                            }
                        })
                        .detach();
                });
            });
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                let Ok(main) = this.read_with(cx, |this, _| this.main_window_handle) else {
                    break;
                };
                let hidden = main
                    .and_then(|main| {
                        cx.update_window(main, |_, window, _| !window.is_visible())
                            .ok()
                    })
                    .unwrap_or(false);
                if hidden {
                    skipped.set(true);
                    continue;
                }
                if this
                    .update(cx, |this, cx| this.refresh_titlebar_accounts(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// CDXC:AgentProviders 2026-09-09 WHY:
    /// Show saved identities before polling usage, and publish each machine independently so a slow helper or offline remote cannot hide local accounts at startup.
    /// Usage still comes from gxserver's shared discovery cache.
    pub(crate) fn refresh_titlebar_accounts(&mut self, cx: &mut gpui::Context<Self>) {
        let revision = self.titlebar_accounts_revision;
        let mut targets = vec![("local".to_string(), None)];
        targets.extend(
            self.remote_gxserver_connections
                .iter()
                .map(|(id, connection)| (id.clone(), Some(connection.request_target()))),
        );
        for (machine, target) in targets {
            if !self
                .titlebar_accounts_refresh_in_flight
                .insert(machine.clone())
            {
                continue;
            }
            cx.spawn(async move |this, cx| {
                // The fade's debounce (`note_failed_read`): whether this refresh's cached read
                // answered, and whether one of its failures was counted already.
                let mut cached_answered = false;
                let mut failure_counted = false;
                for cached_only in [true, false] {
                    let target = target.clone();
                    let result = cx.background_executor().spawn(async move {
                        let params = json!({"operation":"titlebar", "cachedOnly":cached_only});
                        match target {
                            Some(target) => gpui_remote_gxserver_rpc_result(&target, "/api/agentAccounts", &params, Duration::from_secs(60)),
                            None => gpui_gxserver_rpc_result("/api/agentAccounts", &params, Duration::from_secs(60)),
                        }
                    }).await;
                    let count_failure = result.is_err() && !cached_answered && !failure_counted;
                    failure_counted |= count_failure;
                    cached_answered |= cached_only && result.is_ok();
                    if this.update_in(cx, |this, window, cx| {
                        if this.titlebar_accounts_revision != revision {
                            return;
                        }
                        // Most refreshes return what is already shown; only a change redraws the window.
                        let previous_accounts = this.titlebar_accounts.clone();
                        match result {
                            // A computer whose connection stopped while this read was out: its
                                // rows stay flagged (`flag_stopped_machine_accounts`).
                                Ok(_) if machine != "local" && !this.remote_gxserver_connections.contains_key(&machine) => return,
                            Ok(result) => {
                                let logins_changed = remember_swap_logins(
                                    &machine,
                                    result["swapLogins"].as_array().cloned().unwrap_or_default(),
                                );
                                if logins_changed {
                                    cx.notify();
                                }
                                this.titlebar_accounts.retain(|a| text(a, "titlebarMachine") != machine);
                                // The usage-only accounts (Cursor, Copilot) and the CLIProxyAPI hub
                                // accounts (server/src/accounts/hub/sidebar.rs) come under their own keys.
                                for account in ["accounts", "usageAccounts", "hubAccounts"].into_iter()
                                    .flat_map(|key| result[key].as_array().into_iter().flatten())
                                    .filter(|a| a["registered"] == true && a["showInTitlebar"] == true)
                                    .filter(|a| a["usageOnly"] != true || usage_only_provider(text(a, "provider"))) {
                                    let mut entry = titlebar_entry(account, &machine);
                                    // A hub account's usage history is the hub's, which Ghostex doesn't keep.
                                    entry["usageHistory"] = result.get("usageHistory")
                                        .filter(|_| account["hub"] != true)
                                        .map(|history| history[text(account, "provider")].clone())
                                        .unwrap_or_else(|| json!({"status":"unavailable"}));
                                    entry["providerAccountCount"] = result["accountCounts"][text(account, "provider")].clone();
                                    this.titlebar_accounts.push(entry);
                                }
                            }
                            Err(_) => {
                                let now = now_ms();
                                for account in this.titlebar_accounts.iter_mut().filter(|a| text(a, "titlebarMachine") == machine) {
                                    account["usageError"] = json!("Could not refresh account usage. Showing the last received snapshot.");
                                    account["usageStale"] = json!(true);
                                    note_failed_read(account, count_failure, now);
                                }
                            }
                        }
                        this.titlebar_accounts.sort_by(|a, b| text(a,"titlebarMachine").cmp(text(b,"titlebarMachine"))
                            .then(text(a,"provider").cmp(text(b,"provider")))
                            .then((a["hub"] == true).cmp(&(b["hub"] == true)))
                            .then(text(a,"selector").parse::<u64>().unwrap_or(0).cmp(&text(b,"selector").parse::<u64>().unwrap_or(0))));
                        if this.titlebar_accounts == previous_accounts {
                            return;
                        }
                        if let Some(GpuiTitlebarPopupKind::AccountUsage(id)) = this.titlebar_popup_menu.as_ref().map(|state| state.kind) {
                            if this.titlebar_accounts.iter().any(|a| text(a, "titlebarKey") == id.as_str()) {
                                this.sync_titlebar_account_privacy(cx);
                            } else { this.close_gpui_titlebar_popup(None, window, cx); }
                        }
                        cx.notify();
                    }).is_err() {
                        return;
                    }
                }
                let _ = this.update(cx, |this, cx| {
                    this.titlebar_accounts_refresh_in_flight.remove(&machine);
                    if this.titlebar_accounts_revision != revision {
                        this.refresh_titlebar_accounts(cx);
                    }
                });
            }).detach();
        }
    }

    /// Every account the user asked to see, in the order gxserver publishes them, once each; none
    /// in screenshot mode, which hides the account tiles (CDXC:ScreenshotMode 2026-11-20 in
    /// app/screenshot_mode.rs).
    pub(crate) fn account_usage_meters(&self) -> Vec<GpuiAccountUsageMeter> {
        if crate::app::screenshot_mode::screenshot_mode_on() {
            return Vec::new();
        }
        distinct_account_copies(&self.titlebar_accounts)
            .into_iter()
            .filter_map(|copies| {
                let account = copies[0];
                let id = ExtensionId::new(text(account, "titlebarKey"))?;
                let provider = text(account, "provider");
                let codex = provider == "codex";
                let indicator = match text(account, "indicator") {
                    "" => text(account, "selector"),
                    value => value,
                };
                let indicator =
                    (!indicator.is_empty() && indicator != "-").then(|| indicator.to_string());
                let label = indicator
                    .clone()
                    .unwrap_or_else(|| account_display_name(account));
                let (sign_in, unreachable, tooltip) = meter_state(&label, &copies);
                Some(GpuiAccountUsageMeter {
                    id,
                    codex,
                    provider: provider.to_string(),
                    indicator,
                    badge_lines: badge_lines(account),
                    sign_in,
                    unreachable,
                    tooltip,
                })
            })
            .collect()
    }

    /// CDXC:AgentProviders 2026-09-20 DECISION:
    /// User: the account usage meters live at the bottom of the sidebar, not in the titlebar; this supersedes the 2026-09-13 rule that placed them before the extension buttons in the titlebar strip.
    /// Their shape is unchanged: darker Claude/Codex colors in light mode, the account label centered over a 19.2px background icon, a 9.9px label and 9.5px usage text in the chat indicator's monospace font, and the usage percentages or reset counts beside it. A meter still opens its account's usage popup.
    /// CDXC:AgentProviders 2026-09-09 DECISION:
    /// User wants the same account-label font centered over a larger provider icon as its background. Account icons always use original provider colors; labels and adjacent usage figures share the chat indicator's monospace font. Claude keeps its label color; Codex uses #7db8fb. The background icon is 19.2px (20% smaller) and the label is 9.9px (10% bigger). This replaces the label-underneath design. Sidebar icons and account menus have no indicator. Labels accept up to two letters or digits; - hides the label and an empty setting uses the slot number.
    /// The host owns the geometry so the meter has one implementation wherever it is drawn.
    pub(crate) fn render_account_usage_meter(
        &self,
        meter: &GpuiAccountUsageMeter,
        host: &GpuiAccountUsageMeterHost,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let account_id = meter.id;
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::AccountUsage(account_id));
        let anchor_state = window.use_keyed_state(
            format!("{}-{}", host.anchor_key_prefix, account_id.as_str()),
            cx,
            |_, _| GpuiTitlebarPopupAnchorState::default(),
        );
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds = anchor_state
            .read(cx)
            .trigger_bounds_captured
            .then_some(anchor_bounds);
        let badge_lines = meter
            .badge_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        let show_badge = !badge_lines.is_empty();
        let scale = host.scale;
        let icon_image = if usage_only_provider(&meter.provider) {
            usage_only_icon(&meter.provider)
        } else {
            icon(meter.codex)
        };
        let indicator = meter.indicator.clone();
        let claude_card = meter.provider == "claude";
        // CDXC:AgentProviders 2026-11-21 DECISION:
        // User: "change how we indicate that an account has issues: instead of the text name of the
        // account becoming yellow, add a small dot top right of the account's icon (like the one we
        // have on top right of space icon now)". A card whose account needs a sign-in, here or on
        // another computer, keeps its letters, numbers and logo in their usual colours and its icon
        // wears the Space tile's 4pt dot in its top-right corner, in the amber the letters used to
        // turn (`needs_sign_in_amber`); the tooltip (`meter_state`) says which computer. This
        // supersedes the amber letters, numbers and logo of 2026-11-19 (Q4198 A).
        let indicator_color = if meter.codex {
            chrome_color(0x7db8fb, 0x285b8c)
        } else {
            chrome_color(0xa4a8af, 0x9c4328)
        };
        let badge_color = chrome_color(0xb9b9b9, 0x404040);
        let unreachable = meter.unreachable;
        let issue_dot = (meter.sign_in != MeterSignIn::SignedIn).then(|| {
            let ring_rest = if open {
                host.open_background
            } else {
                host.background
            };
            let ring_hover = if open {
                host.open_background
            } else {
                host.hover_background
            };
            (
                gpui::Hsla::from(needs_sign_in_amber(claude_card)),
                ring_rest,
                ring_hover,
            )
        });
        let dot_group = format!("{}-{}", host.element_id_prefix, account_id.as_str());
        let tooltip = meter.tooltip.clone();
        let tooltip_delay = host.tooltip_delay;
        let hover_background = host.hover_background;
        let open_background = host.open_background;
        let route = host.route;

        let glyph = |size: f32| {
            let size = if indicator.is_some() {
                ACCOUNT_USAGE_BADGE_GLYPH_WIDTH
            } else {
                size
            } * scale;
            div()
                .relative()
                .size(px(size))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .child(
                    img(icon_image.clone())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(size))
                        .when(indicator.is_some(), |this| this.opacity(0.3)),
                )
                .when_some(indicator.clone(), |this, indicator| {
                    this.child(
                        div()
                            .relative()
                            .text_color(indicator_color)
                            .text_size(px(9.9 * scale))
                            .line_height(px(9.9 * scale))
                            .font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_center()
                            .child(indicator),
                    )
                })
                .children(issue_dot.clone().map(|(color, rest, hover)| {
                    // The ring is the sidebar's colour under the card's own fill.
                    let base = crate::app::helpers::titlebar_background();
                    crate::app::native_sidebar::status::corner_dot(
                        color,
                        scale,
                        base.blend(rest),
                        Some((base.blend(hover), dot_group.clone().into())),
                    )
                }))
        };

        div()
            .id(format!(
                "{}-{}",
                host.element_id_prefix,
                account_id.as_str()
            ))
            .group(dot_group.clone())
            .when(!host.fill_width, |this| this.flex_shrink_0())
            .when(host.fill_width, |this| {
                this.w_full().min_w_0().overflow_hidden()
            })
            .relative()
            .flex()
            .h(px(host.height))
            .px(px(host.padding_x))
            .rounded(px(host.corner_radius))
            .items_center()
            .justify_center()
            .cursor_default()
            .bg(host.background)
            .when(open, |this| this.bg(open_background))
            .hover(move |this| {
                if open {
                    this.bg(open_background)
                } else {
                    this.bg(hover_background)
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    let Some(trigger_bounds) = trigger_bounds else {
                        window.request_animation_frame();
                        return;
                    };
                    if let Some(route) = route {
                        let app = cx.entity();
                        let bounds =
                            Bounds::new(trigger_bounds.origin + route.offset, trigger_bounds.size);
                        cx.defer(move |cx| {
                            let _ = route.window.update(cx, |_, window, cx| {
                                app.update(cx, |app, cx| {
                                    app.open_titlebar_account_usage(account_id, bounds, window, cx)
                                })
                            });
                        });
                        return;
                    }
                    this.open_titlebar_account_usage(account_id, trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if let Some(route) = route {
                        let app = cx.entity();
                        let position = event.position + route.offset;
                        cx.defer(move |cx| {
                            let _ = route.window.update(cx, |_, window, cx| {
                                app.update(cx, |app, cx| {
                                    app.show_gpui_titlebar_account_menu(position, window, cx)
                                })
                            });
                        });
                        return;
                    }
                    this.show_gpui_titlebar_account_menu(event.position, window, cx);
                }),
            )
            /*
            CDXC:AgentProviders 2026-09-20 WHY:
            A press that lands on a meter opens that account's usage popup and then
            swallows its click, so the sidebar's double-click-to-create-session cannot
            fire on a meter. The mouse-down still bubbles, so the sidebar menu closes as
            it does for every other sidebar click.
            */
            .on_click(cx.listener(|_, _: &gpui::ClickEvent, _, cx| cx.stop_propagation()))
            // CDXC:AgentProviders 2026-09-21 DECISION: "please don't show tooltips for the account usage cards at the bottom of the gpui sidebar". A press opens the account's usage popup, which says everything the tooltip did.
            // Since 2026-11-19 a card with a problem has one (`meter_state`): which computer its account is signed out on, or can't reach.
            .when_some(tooltip, |this, tooltip| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Auto,
                    tooltip_delay,
                    move |window, cx| {
                        crate::app::native_sidebar::tooltips::compact_card_tooltip(
                            tooltip.clone(),
                            260.0,
                            window,
                            cx,
                        )
                    },
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let request_frame = anchor_state.update(cx, |state, _| {
                        let changed = !state.trigger_bounds_captured || state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        changed
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .map(|this| {
                if show_badge {
                    this.child(
                        h_flex()
                            .gap(px(ACCOUNT_USAGE_BADGE_GAP * scale))
                            .child(glyph(14.0))
                            .child(
                                v_flex()
                                    .text_size(px(ACCOUNT_USAGE_BADGE_TEXT_SIZE * scale))
                                    .line_height(px(ACCOUNT_USAGE_BADGE_TEXT_SIZE * scale))
                                    .font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(badge_color)
                                    .when(unreachable, |this| this.opacity(0.45))
                                    .children(badge_lines),
                            ),
                    )
                } else {
                    this.child(glyph(18.0))
                }
            })
            .into_any_element()
    }

    /// CDXC:Titlebar 2026-09-12 DECISION:
    /// User: clicking an open usage button again closes its dropdown; clicking away, including in Session Chat, dismisses Usage and Tips.
    pub(crate) fn open_titlebar_account_usage(
        &mut self,
        id: ExtensionId,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiTitlebarPopupKind::AccountUsage(id);
        if !self
            .titlebar_accounts
            .iter()
            .any(|account| text(account, "titlebarKey") == id.as_str())
        {
            return;
        }
        let opening = !self.titlebar_popup_menu_open(kind);
        let previous_focus = window.focused(cx);
        self.set_gpui_titlebar_popup_open(kind, opening, Some(trigger_bounds), window, cx);
        if opening {
            self.titlebar_dropdown_previous_focus_handle = previous_focus;
            self.titlebar_dropdown_focus_handle.focus(window, cx);
            #[cfg(target_os = "macos")]
            {
                self.begin_programmatic_focus();
                unsafe extern "C" {
                    fn GhostexGpuiBeginUsageKeyboardFocus(view: *mut std::ffi::c_void);
                }
                unsafe { GhostexGpuiBeginUsageKeyboardFocus(self.parent_ns_view) };
                // CDXC:FocusRouting 2026-09-12 WHY:
                // Leaving this guard active suppresses later real CEF clicks, preventing chat clicks from dismissing Usage and Tips.
                self.end_programmatic_focus();
            }
            #[cfg(target_os = "windows")]
            cef::focus_gpui_root_view(self.parent_ns_view);
        }
    }

    /// CDXC:FocusRouting 2026-09-14 WHY:
    /// Restoring chat focus when Usage closes queues a responder event that can arrive after another dropdown opens and dismiss it as an outside click. Mark only the restoration as programmatic so real outside clicks still dismiss dropdowns.
    pub(crate) fn restore_account_usage_keyboard_focus(&mut self) {
        #[cfg(target_os = "macos")]
        {
            unsafe extern "C" {
                fn GhostexGpuiEndUsageKeyboardFocus(view: *mut std::ffi::c_void);
            }
            self.begin_programmatic_focus();
            unsafe { GhostexGpuiEndUsageKeyboardFocus(self.parent_ns_view) };
            self.end_programmatic_focus();
        }
    }

    /// CDXC:AgentProviders 2026-09-12 WHY:
    /// Native titlebar panels keep the main window active; route usage keyboard controls to their owning popup so Tab and Space cannot operate the underlying chat.
    pub(crate) fn forward_account_usage_key(
        &self,
        event: &gpui::KeyDownEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform
            || modifiers.control
            || modifiers.alt
            || !matches!(
                event.keystroke.key.as_str(),
                "tab" | "enter" | "space" | "up" | "down" | "pageup" | "pagedown" | "home" | "end"
            )
        {
            return;
        }
        if let Some(popup) = self.titlebar_popup_window {
            let event = event.clone();
            let _ = popup.update(cx, |_, window, cx| {
                window.dispatch_event(gpui::PlatformInput::KeyDown(event), cx);
            });
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(machine: &str, provider: &str, id: &str, name: &str) -> Value {
        titlebar_entry(
            &json!({"provider": provider, "id": id, "name": name, "registered": true, "showInTitlebar": true}),
            machine,
        )
    }

    fn shown(accounts: &[Value]) -> Vec<(String, String)> {
        distinct_accounts(accounts)
            .into_iter()
            .map(|account| {
                (
                    text(account, "titlebarMachine").to_string(),
                    text(account, "name").to_string(),
                )
            })
            .collect()
    }

    /// The user's sidebar on 2026-10-30: MS and ST are signed in here and on the Windows machine.
    #[test]
    fn an_account_signed_in_on_two_machines_shows_once() {
        let windows = "remote-mvb5laq3-4aol2";
        let accounts = vec![
            account("local", "claude", "claude-sp", "SP"),
            account("local", "claude", "claude-ms", "MS"),
            account("local", "claude", "claude-st", "ST"),
            account("local", "codex", "codex-sp", "SP"),
            account(windows, "claude", "claude-ms", "MS"),
            account(windows, "claude", "claude-st", "ST"),
            account(windows, "claude", "claude-other", "OT"),
        ];
        assert_eq!(
            shown(&accounts),
            [
                ("local", "SP"),
                ("local", "MS"),
                ("local", "ST"),
                ("local", "SP"),
                (windows, "OT"),
            ]
            .map(|(machine, name)| (machine.to_string(), name.to_string()))
        );
    }

    fn with(mut account: Value, key: &str, value: Value) -> Value {
        account[key] = value;
        account
    }

    fn state(accounts: &[Value]) -> (MeterSignIn, bool, Option<String>) {
        let copies = distinct_account_copies(accounts).remove(0);
        meter_state("ST", &copies)
    }

    /// Q4198 A: signed out on this computer marks the card (the icon's dot since 2026-11-21).
    #[test]
    fn a_signed_out_copy_shown_here_is_signed_out() {
        let here = with(
            account("local", "claude", "claude-st", "ST"),
            "status",
            json!("loginRequired"),
        );
        let (sign_in, unreachable, tooltip) = state(&[here]);
        assert_eq!(sign_in, MeterSignIn::SignedOut);
        assert!(!unreachable);
        assert_eq!(
            tooltip,
            Some(format!(
                "ST · Claude\n{}: signed out\nClick to reconnect it.",
                crate::host_name::local_computer_name()
            ))
        );
    }

    /// Signed in here, signed out on the Windows machine: the card is marked, and shows this
    /// computer's copy whatever the order.
    #[test]
    fn signed_out_on_another_computer_only_is_elsewhere() {
        let windows = "remote-mvb5laq3-4aol2";
        let accounts = vec![
            with(
                account(windows, "claude", "claude-st", "ST"),
                "status",
                json!("loginRequired"),
            ),
            with(
                account("local", "claude", "claude-st", "ST"),
                "status",
                json!("ready"),
            ),
        ];
        let copies = distinct_account_copies(&accounts);
        assert_eq!(copies.len(), 1);
        assert_eq!(text(copies[0][0], "titlebarMachine"), "local");
        let (sign_in, _, tooltip) = state(&accounts);
        assert_eq!(sign_in, MeterSignIn::SignedOutElsewhere);
        let tooltip = tooltip.unwrap();
        assert!(
            tooltip.contains(&format!(
                "{}: signed in",
                crate::host_name::local_computer_name()
            )),
            "{tooltip}"
        );
        assert!(
            tooltip.contains(": signed out\nClick to sign it in again there."),
            "{tooltip}"
        );
    }

    #[test]
    fn a_computer_that_did_not_answer_fades_and_explains() {
        let stale = with(
            account("local", "cursor", "cursor-1", "G"),
            "machineUnreachable",
            json!(true),
        );
        let (sign_in, unreachable, tooltip) = state(&[stale]);
        assert_eq!(sign_in, MeterSignIn::SignedIn);
        assert!(unreachable);
        assert_eq!(
            tooltip,
            Some(format!(
                "ST · Cursor\n{}: can't reach it, showing its last reading",
                crate::host_name::local_computer_name()
            ))
        );
    }

    /// Another computer that didn't answer leaves a signed-in card as it is: no tooltip.
    #[test]
    fn another_computer_not_answering_leaves_the_card_alone() {
        let accounts = vec![
            account("local", "claude", "claude-ms", "MS"),
            with(
                account("remote-a", "claude", "claude-ms", "MS"),
                "machineUnreachable",
                json!(true),
            ),
        ];
        assert_eq!(state(&accounts), (MeterSignIn::SignedIn, false, None));
    }

    /// `unavailable` is signed out only for an account synced from another computer.
    #[test]
    fn unavailable_is_signed_out_only_when_synced() {
        let synced = with(
            with(
                account("local", "claude", "claude-st", "ST"),
                "status",
                json!("unavailable"),
            ),
            "syncedFrom",
            json!("Windows PC"),
        );
        assert_eq!(state(&[synced]).0, MeterSignIn::SignedOut);
        let missing = with(
            account("local", "claude", "claude-st", "ST"),
            "status",
            json!("unavailable"),
        );
        assert_eq!(state(&[missing]), (MeterSignIn::SignedIn, false, None));
    }

    /// A stopped computer's stale "signed out" can't keep a card amber: it fades instead.
    #[test]
    fn a_stopped_computers_copy_is_faded_not_amber() {
        let mut accounts = vec![
            account("local", "claude", "claude-st", "ST"),
            with(
                account("remote-a", "claude", "claude-st", "ST"),
                "status",
                json!("loginRequired"),
            ),
            with(
                account("remote-a", "claude", "claude-only", "OT"),
                "status",
                json!("loginRequired"),
            ),
        ];
        flag_stopped_machine_accounts(&mut accounts, "remote-a");
        let copies = distinct_account_copies(&accounts);
        let (sign_in, unreachable, tooltip) = meter_state("ST", &copies[0]);
        assert_eq!(
            (sign_in, unreachable, tooltip),
            (MeterSignIn::SignedIn, false, None)
        );
        let (sign_in, unreachable, tooltip) = meter_state("OT", &copies[1]);
        assert_eq!(sign_in, MeterSignIn::SignedIn);
        assert!(unreachable);
        assert!(tooltip.unwrap().contains("can't reach it"));
    }

    /// One failed read doesn't fade a card; two counted ones in a row, or 90 seconds, do. A failure
    /// that isn't counted (the refresh's cached read answered) adds nothing.
    #[test]
    fn the_fade_waits_for_a_second_failed_read() {
        let mut row = account("remote-a", "claude", "claude-ms", "MS");
        note_failed_read(&mut row, false, 1_000.0);
        assert!(row["machineUnreachable"].is_null() && row["unreachableReads"].is_null());
        note_failed_read(&mut row, true, 1_000.0);
        assert!(row["machineUnreachable"].is_null());
        note_failed_read(&mut row, true, 31_000.0);
        assert_eq!(row["machineUnreachable"], true);
        let mut row = account("remote-a", "claude", "claude-ms", "MS");
        note_failed_read(&mut row, true, 1_000.0);
        note_failed_read(&mut row, false, 1_000.0 + UNREACHABLE_FADE_AFTER_MS);
        assert_eq!(row["machineUnreachable"], true);
    }

    /// A card without a problem keeps no tooltip (2026-09-21).
    #[test]
    fn the_usage_popup_learns_every_computers_state() {
        let accounts = vec![
            account("local", "claude", "claude-a", "SP"),
            with(
                with(
                    account("remote-1", "claude", "claude-a", "SP"),
                    "status",
                    json!("loginRequired"),
                ),
                "titlebarKey",
                json!("remote-1:claude-a"),
            ),
            with(
                account("remote-2", "claude", "claude-a", "SP"),
                "machineUnreachable",
                json!(true),
            ),
        ];
        let key = text(&accounts[0], "titlebarKey").to_string();
        let popup = usage_popup_account(&accounts, &key).unwrap();
        let states: Vec<(&str, bool, bool, &str)> = popup["signInComputers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    text(c, "machine"),
                    c["here"] == true,
                    c["shown"] == true,
                    text(c, "state"),
                )
            })
            .collect();
        assert_eq!(
            states,
            vec![
                ("local", true, true, "signedIn"),
                ("remote-1", false, false, "signedOut"),
                ("remote-2", false, false, "unreachable"),
            ]
        );
        assert_eq!(usage_popup_account(&accounts, "nope"), None);
    }

    #[test]
    fn a_signed_in_account_has_no_tooltip() {
        let accounts = vec![
            account("local", "claude", "claude-ms", "MS"),
            account("remote-a", "claude", "claude-ms", "MS"),
        ];
        assert_eq!(state(&accounts), (MeterSignIn::SignedIn, false, None));
    }

    #[test]
    fn this_computers_copy_wins_whatever_the_order() {
        let accounts = vec![
            account("a-machine", "claude", "claude-ms", "MS remote"),
            account("local", "claude", "claude-ms", "MS"),
        ];
        assert_eq!(shown(&accounts), [("local".to_string(), "MS".to_string())]);
    }
}
