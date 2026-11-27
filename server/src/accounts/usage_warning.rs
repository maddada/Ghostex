//! The early warning before the account a coordinator runs on runs out, and the one Needs you line
//! for a coordinator whose threads are out of usage.
//!
//! CDXC:AgentProviders 2026-11-27 DECISION:
//! User, after their coordinator and five threads stopped on Claude #1's weekly limit while #4 and
//! #5 were signed out on this computer: "i want a warning when you're about to run out of usage on
//! the coordinator and the account for it isn't logged in!! u just went dead all of a sudden without
//! the app saying anything". On the account-switch page they picked: the warning shows in all four
//! places (Q5294: a card in the coordinator's chat, a Needs you item, the bell with a system
//! notification, and a phone push), at 70%, 85% and 95% (Q5295), and only while no other automatic
//! account can take over on this computer; a stopped thread's row reads "Out of usage" and the
//! coordinator says why once, in one Needs you line, with no message to the threads (Q5297).
//!
//! How it reads: every minute, for each running coordinator, the accounts it and its running
//! threads use (the account panel's identity, `session_identity`). An account counts as close to
//! out at a threshold when a shared window that is not the 5h one (7d, a month) is at it, or the 5h
//! one is with an hour or more before it resets (a 5h wall that frees within the hour is a short
//! wait, never a warning). A window at 100% is the limit itself, which the no-room card covers
//! (limit_wait.rs). "No other account can take over" means no other automatic account
//! (`auto_switchable`, in the workspace's pool) is ready with a trusted reading under 100%.
//! Each account window (keyed by its reset time) warns once per threshold: one bell row and push
//! per account, one Needs you item per coordinator (kept until the user presses Done or the warning
//! ends), and the card until Not now, which hides it until the next threshold. Everything ends when
//! the account is no longer close to out, another account can take over, or the coordinator moves.
//! SEE-ALSO: packages/gx-chat-core/src/questions/account_wait.rs (`decorate_usage_warning`, the
//! card's dropdown), server/src/session_chat_notice/watchdog_store.rs (where the card joins the
//! chat), server/src/ghostex_cli/coordinator/usage_room.rs (the coordinator's own LOW line).

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{endpoint, launch, model::*, runtime, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    notification_feed::{
        broadcast_notification_feed_changed,
        store::{
            account_notification_session_id, insert_notification_feed_row, NewNotificationFeedRow,
            ACCOUNT_NOTIFICATION_PROJECT_ID, NOTIFICATION_FEED_KIND_FAILED,
        },
    },
    server::AppState,
    session_chat_notice::{
        SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
        SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
        SessionChatTerminalNoticeSource,
    },
};

/// The card's notice kind (gx-chat-core `USAGE_WARNING_KIND`).
pub(crate) const NOTICE_KIND: &str = "usageWarning";
/// The card's Not now.
pub(crate) const ACTION_DISMISS: &str = "dismissUsageWarning";
/// The bell row's account id prefix: the desktop turns a new one into a system notification
/// (apps/desktop/src/notification_feed/reset_banners.rs).
pub(crate) const BELL_ACCOUNT_PREFIX: &str = "low:";

const THRESHOLDS: [f64; 3] = [70.0, 85.0, 95.0];
const SHORT_RESET_HOURS: i64 = 1;
const TICK: Duration = Duration::from_secs(60);
const FIRST_TICK: Duration = Duration::from_secs(45);
const STATE_KEY: &str = "accounts.usageWarnings.v1";

/// What was already said about one account window (`<account id>|<window id>|<resets at>`).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Episode {
    /// The highest threshold the bell and the phone heard about.
    announced: f64,
    /// The threshold the card was put away at (Not now): it shows again past the next one.
    dismissed: f64,
    /// When the window resets (RFC3339), so a past episode is forgotten.
    resets_at: Option<String>,
}

/// The Needs you item Ghostex keeps on a coordinator: which warning it is about and its tag.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
struct Item {
    /// `low:<episode>` or `out:<provider>`.
    about: String,
    tag: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    episodes: BTreeMap<String, Episode>,
    /// By coordinator (`<project>/<session>`).
    items: BTreeMap<String, Item>,
}

/// The card a coordinator shows now.
#[derive(Clone, Debug, PartialEq)]
struct Card {
    episode: String,
    threshold: f64,
    title: String,
    detail: String,
    facts: Value,
}

fn cards() -> &'static Mutex<HashMap<(String, String), Card>> {
    static CARDS: OnceLock<Mutex<HashMap<(String, String), Card>>> = OnceLock::new();
    CARDS.get_or_init(Default::default)
}

fn read_saved(db: &Connection) -> Saved {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [STATE_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|raw| serde_json::from_str(&raw).ok())
    .unwrap_or_default()
}

fn write_saved(db: &Connection, saved: &Saved) {
    let Ok(raw) = serde_json::to_string(saved) else {
        return;
    };
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![STATE_KEY, raw, Utc::now().to_rfc3339()],
    );
}

fn log(state: &AppState, level: LogLevel, event: &str, error: Option<String>, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error,
        details: Some(details),
    });
}

/// The coordinator's card, when it has one showing (`resolve_session_chat_terminal_notice`).
pub(crate) fn card(project_id: &str, session_id: &str) -> Option<SessionChatTerminalNotice> {
    let card = cards()
        .lock()
        .ok()?
        .get(&(project_id.to_string(), session_id.to_string()))?
        .clone();
    let mut notice = SessionChatTerminalNotice::new(
        NOTICE_KIND,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Watchdog,
        card.title,
    )
    .with_detail(card.detail)
    .with_actions(vec![SessionChatTerminalNoticeAction {
        id: ACTION_DISMISS.to_string(),
        label: "Not now".to_string(),
        kind: SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    }])
    .with_input_blocking(false);
    notice.usage_warning = Some(card.facts);
    Some(notice)
}

/// The card's Not now: hidden until the account passes its next threshold.
pub(crate) async fn answer_dismiss(
    state: &AppState,
    params: &serde_json::Map<String, Value>,
) -> Result<(), DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let Some(card) = cards()
        .lock()
        .ok()
        .and_then(|mut cards| cards.remove(&(project_id.clone(), session_id.clone())))
    else {
        return Ok(());
    };
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let mut saved = read_saved(&db);
    let episode = saved.episodes.entry(card.episode).or_default();
    episode.dismissed = episode.dismissed.max(card.threshold);
    write_saved(&db, &saved);
    crate::server::republish_session_chat(state, &project_id, &session_id);
    Ok(())
}

pub(crate) fn start(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut delay = FIRST_TICK;
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(delay) => {}
            }
            delay = TICK;
            let pass_state = state.clone();
            match tokio::task::spawn_blocking(move || pass(&pass_state)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log(
                    &state,
                    LogLevel::Warn,
                    "accountUsageWarningFailed",
                    Some(error.message),
                    json!({}),
                ),
                Err(join) => log(
                    &state,
                    LogLevel::Error,
                    "accountUsageWarningPanicked",
                    Some(join.to_string()),
                    json!({}),
                ),
            }
        }
    });
}

/// How close one account is to out, read off its shared windows.
#[derive(Clone, Debug)]
struct Low {
    window_id: String,
    /// "week" for 7d, "5-hour window", or the window's own label.
    window_words: String,
    percent: f64,
    threshold: f64,
    resets_at: Option<DateTime<Utc>>,
}

fn parse_time(text: Option<&str>) -> Option<DateTime<Utc>> {
    text.and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc))
}

/// The fullest window of `account` that counts, at or past the first threshold and under 100%.
fn low(account: &DiscoveredAccount, now: DateTime<Utc>) -> Option<Low> {
    if account.status != "ready" || account.usage_error.is_some() {
        return None;
    }
    account
        .usage
        .iter()
        .filter(|w| w.model.is_none() && w.id != "spend")
        .filter(|w| w.used_percent < 100.0)
        .filter_map(|w| {
            let resets_at = parse_time(w.resets_at.as_deref());
            if w.label == "5h"
                && resets_at.is_some_and(|at| at - now < chrono::Duration::hours(SHORT_RESET_HOURS))
            {
                return None;
            }
            let threshold = THRESHOLDS
                .iter()
                .copied()
                .filter(|t| w.used_percent >= *t)
                .fold(None, |_, t| Some(t))?;
            Some(Low {
                window_id: w.id.clone(),
                window_words: match w.label.as_str() {
                    "7d" => "week".to_string(),
                    "5h" => "5-hour window".to_string(),
                    other => format!("{other} window"),
                },
                percent: w.used_percent,
                threshold,
                resets_at,
            })
        })
        .max_by(|a, b| a.percent.total_cmp(&b.percent))
}

/// Whether another automatic account of `provider` could take over now.
fn another_can_take_over(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: &str,
    pool: Option<&[String]>,
) -> bool {
    registry
        .accounts
        .iter()
        .filter(|a| {
            a.provider == provider
                && a.id != current
                && a.auto_switchable()
                && super::workspace_pool::in_pool(&a.id, pool)
        })
        .any(|a| {
            snapshot
                .accounts
                .iter()
                .any(|d| endpoint::account_id(d) == a.id && super::recovery::trusted_room(d, ""))
        })
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

/// `#4`, as every surface of this warning names an account.
fn number(registry: &Registry, id: &str) -> String {
    registry
        .accounts
        .iter()
        .find(|a| a.id == id)
        .map(|a| format!("#{}", a.selector))
        .unwrap_or_else(|| "another account".to_string())
}

fn join_names(names: &[String]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Why each other account can't take over, in one sentence, from the no-room card's own facts.
fn blocked_sentence(blocked: &[Value]) -> String {
    let names = |why: &str| -> Vec<String> {
        blocked
            .iter()
            .filter(|entry| entry["why"] == why)
            .filter(|entry| why != "manual" || entry["room"] == true)
            .filter_map(|entry| entry["account"].as_str().map(str::to_string))
            .collect()
    };
    let mut parts = Vec::new();
    let are = |names: &[String]| if names.len() == 1 { "isn't" } else { "aren't" };
    let signed_out = names("signIn");
    if !signed_out.is_empty() {
        parts.push(format!(
            "{} {} signed in on this computer",
            join_names(&signed_out),
            are(&signed_out)
        ));
    }
    let off = names("off");
    if !off.is_empty() {
        parts.push(format!(
            "{} {} turned off here",
            join_names(&off),
            if off.len() == 1 { "is" } else { "are" }
        ));
    }
    let full = names("full");
    if !full.is_empty() {
        parts.push(format!(
            "{} {} full",
            join_names(&full),
            if full.len() == 1 { "is" } else { "are" }
        ));
    }
    let manual = names("manual");
    if !manual.is_empty() {
        parts.push(format!(
            "{} {} room but {} manual only",
            join_names(&manual),
            if manual.len() == 1 { "has" } else { "have" },
            if manual.len() == 1 { "is" } else { "are" }
        ));
    }
    match parts.split_last() {
        None => "No other account can take over on this computer".to_string(),
        Some((last, [])) => upper_first(last),
        Some((last, rest)) => upper_first(&format!("{}, and {last}", rest.join(", "))),
    }
}

fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn local_clock(at: DateTime<Utc>) -> String {
    at.with_timezone(&chrono::Local)
        .format("%a %-d %b, %-I:%M %p")
        .to_string()
}

/// The no-room card's facts for `current`, with each account named by its number.
fn blocked_facts(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: &str,
    pool: Option<&[String]>,
    now: DateTime<Utc>,
) -> Vec<Value> {
    let wait = super::limit_wait::describe_in_pool(
        registry,
        snapshot,
        provider,
        Some(current),
        "",
        None,
        None,
        pool,
        now,
    );
    wait["blocked"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut entry| {
            if let Some(id) = entry["accountId"].as_str().map(str::to_string) {
                entry["account"] = json!(number(registry, &id));
            }
            entry
        })
        .collect()
}

/// One running session's agent account: provider and account id.
fn session_account(
    state: &AppState,
    repository: &DomainRepository<'_>,
    registry: &Registry,
    session: &Value,
) -> Option<(Provider, String)> {
    let project = repository
        .get_project(session["projectId"].as_str()?)
        .ok()
        .flatten()?;
    let provider = launch::provider(&project, session)?;
    if launch::session_uses_own_login(provider, session) {
        return None;
    }
    let id = super::session_identity::display_account_id(
        registry,
        provider,
        session,
        &state.paths.home_dir,
    )?;
    Some((provider, id.to_string()))
}

/// A thread or coordinator waiting at its limit with no account to move to (limit_wait.rs).
fn out_of_usage(session: &Value) -> Option<&Value> {
    let wait = session.pointer("/runtimeSettings/accountRecovery/accountWait")?;
    (wait["provider"].as_str().is_some_and(|p| !p.is_empty()) && wait["mode"].is_null())
        .then_some(wait)
}

/// What a coordinator's Needs you line should say now, if anything: (about, title, detail).
type Wanted = Option<(String, String, String)>;

fn pass(state: &AppState) -> Result<(), DomainStateError> {
    let snapshot = runtime::current_snapshot();
    if snapshot.fetched_at.is_none() {
        return Ok(());
    }
    let now = Utc::now();
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let mut saved = read_saved(&db);
    let before = saved.clone();
    let mut next_cards: HashMap<(String, String), Card> = HashMap::new();
    let mut wanted_items: BTreeMap<String, Wanted> = BTreeMap::new();
    // Bell and phone once per account episode, however many coordinators run on it.
    let mut announce: BTreeMap<String, (String, String, String, String, f64)> = BTreeMap::new();

    for coordinator in crate::coordinators::list_coordinators(&db)? {
        let key = (
            coordinator.project_id.clone(),
            coordinator.session_id.clone(),
        );
        let item_key = format!("{}/{}", key.0, key.1);
        let Some(session) = repository.get_session(&key.0, &key.1)? else {
            continue;
        };
        if session["lifecycleState"].as_str() != Some("running") {
            wanted_items.insert(item_key, None);
            continue;
        }
        let threads: Vec<Value> = crate::coordinators::list_threads_for(&db, &key.0, &key.1)?
            .into_iter()
            .filter(|thread| thread.resolved_at.is_none())
            .filter_map(|thread| {
                repository
                    .get_session(&thread.project_id, &thread.session_id)
                    .ok()
                    .flatten()
            })
            .filter(|thread| thread["lifecycleState"].as_str() == Some("running"))
            .collect();

        // Threads (or the coordinator) already out with nowhere to go: one line for them all.
        let out: Vec<&Value> = std::iter::once(&session)
            .chain(threads.iter())
            .filter(|member| out_of_usage(member).is_some())
            .collect();
        let mut wanted: Wanted = None;
        if let Some(first) = out.first().and_then(|member| out_of_usage(member)) {
            let provider = first["provider"].as_str().unwrap_or_default();
            let label = match provider {
                "codex" => "Codex",
                _ => "Claude",
            };
            let count = out.len();
            let who = if count == 1 {
                "1 session is".to_string()
            } else {
                format!("{count} sessions are")
            };
            let blocked: Vec<Value> = first["blocked"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|mut entry| {
                    if let Some(id) = entry["accountId"].as_str().map(str::to_string) {
                        entry["account"] = json!(number(&registry, &id));
                    }
                    entry
                })
                .collect();
            let when = parse_time(first["resetsAt"].as_str())
                .filter(|at| *at > now)
                .map(|at| format!("Otherwise they wait until {}.", local_clock(at)))
                .unwrap_or_else(|| "Otherwise they wait until an account has room.".to_string());
            wanted = Some((
                format!("out:{provider}"),
                format!("{who} out of {label} usage"),
                format!(
                    "Couldn't move them to another {label} account. {}. Sign in to one in Settings > Accounts and they carry on by themselves. {when}",
                    blocked_sentence(&blocked)
                ),
            ));
        }

        // The early warning: the fullest account this coordinator or its threads use.
        let mut in_use: BTreeMap<(String, String), (Provider, bool, usize)> = BTreeMap::new();
        if let Some((provider, id)) = session_account(state, &repository, &registry, &session) {
            in_use
                .entry((provider.id().to_string(), id))
                .or_insert((provider, false, 0))
                .1 = true;
        }
        for thread in &threads {
            if let Some((provider, id)) = session_account(state, &repository, &registry, thread) {
                in_use
                    .entry((provider.id().to_string(), id))
                    .or_insert((provider, false, 0))
                    .2 += 1;
            }
        }
        let pool = repository
            .get_project(&key.0)
            .ok()
            .flatten()
            .and_then(|project| {
                launch::provider(&project, &session).and_then(|provider| {
                    super::workspace_pool::session_pool(&db, &project, &session, provider)
                })
            });
        let warning = in_use
            .iter()
            .filter_map(|((_, id), (provider, on_it, threads_on_it))| {
                let reading = snapshot
                    .accounts
                    .iter()
                    .find(|d| endpoint::account_id(d) == *id)?;
                let low = low(reading, now)?;
                if another_can_take_over(&registry, &snapshot, *provider, id, pool.as_deref()) {
                    return None;
                }
                Some((id.clone(), *provider, *on_it, *threads_on_it, low))
            })
            .max_by(|a, b| a.4.percent.total_cmp(&b.4.percent));
        if let Some((id, provider, on_it, threads_on_it, low)) = warning {
            let episode_key = format!(
                "{id}|{}|{}",
                low.window_id,
                low.resets_at.map(|at| at.to_rfc3339()).unwrap_or_default()
            );
            let account = number(&registry, &id);
            let blocked = blocked_facts(&registry, &snapshot, provider, &id, pool.as_deref(), now);
            let label = provider_label(provider);
            let title = format!(
                "{label} {account} is at {}% of its {}",
                low.percent.floor(),
                low.window_words
            );
            let who = match (on_it, threads_on_it) {
                (true, 0) => "This coordinator runs".to_string(),
                (true, 1) => "This coordinator and 1 thread run".to_string(),
                (true, n) => format!("This coordinator and {n} threads run"),
                (false, 1) => "1 of its threads runs".to_string(),
                (false, n) => format!("{n} of its threads run"),
            };
            let until = low
                .resets_at
                .map(|at| format!(" until {}", local_clock(at)))
                .unwrap_or_default();
            let detail = format!(
                "{who} on {account}. {}, so when {account} runs out they stop{until}.",
                blocked_sentence(&blocked)
            );
            let episode = saved.episodes.entry(episode_key.clone()).or_default();
            episode.resets_at = low.resets_at.map(|at| at.to_rfc3339());
            if low.threshold > episode.announced {
                announce.entry(episode_key.clone()).or_insert((
                    key.0.clone(),
                    key.1.clone(),
                    title.clone(),
                    detail.clone(),
                    low.threshold,
                ));
            }
            if low.threshold > episode.dismissed {
                next_cards.insert(
                    key.clone(),
                    Card {
                        episode: episode_key.clone(),
                        threshold: low.threshold,
                        title: title.clone(),
                        detail: detail.clone(),
                        facts: json!({
                            "provider": provider.id(),
                            "accountId": id,
                            "account": account,
                            "usedPercent": low.percent,
                            "threshold": low.threshold,
                            "resetsAt": low.resets_at.map(|at| at.to_rfc3339()),
                            "coordinator": on_it,
                            "blocked": blocked,
                        }),
                    },
                );
            }
            if wanted.is_none() {
                wanted = Some((
                    format!("low:{episode_key}"),
                    format!("Sign in to another {label} account before {account} runs out"),
                    format!("{title}. {detail} Sign in in Settings > Accounts."),
                ));
            }
        }
        wanted_items.insert(item_key, wanted);
    }

    // The bell row (the desktop also raises it as a system notification) and the phone's push.
    for (episode_key, (project_id, session_id, title, detail, threshold)) in announce {
        let account_id = episode_key
            .split('|')
            .next()
            .unwrap_or_default()
            .to_string();
        let notification_id = insert_notification_feed_row(
            &db,
            NewNotificationFeedRow {
                project_id: ACCOUNT_NOTIFICATION_PROJECT_ID.to_string(),
                session_id: account_notification_session_id(&format!(
                    "{BELL_ACCOUNT_PREFIX}{account_id}"
                )),
                kind: NOTIFICATION_FEED_KIND_FAILED,
                title: title.clone(),
                subtitle: "Running low".to_string(),
                body: detail,
                agent_name: None,
                attention_event_id: None,
            },
        )
        .ok()
        .and_then(|row| row["id"].as_str().map(str::to_string));
        if let Some(id) = notification_id.as_deref() {
            broadcast_notification_feed_changed(state);
            crate::push::announce_usage_low(state, &project_id, &session_id, id, &title);
        }
        saved
            .episodes
            .entry(episode_key.clone())
            .or_default()
            .announced = threshold;
        log(
            state,
            LogLevel::Warn,
            "accountUsageWarning",
            None,
            json!({ "projectId": project_id, "sessionId": session_id, "episode": episode_key, "threshold": threshold }),
        );
    }

    // One Needs you line per coordinator: added when a warning or an out-of-usage wait starts,
    // dropped once it ends; a line the user pressed Done on is not added again for the same one.
    for (item_key, wanted) in wanted_items {
        let Some((project_id, session_id)) = item_key.split_once('/') else {
            continue;
        };
        let current = saved.items.get(&item_key).cloned();
        let same = matches!((&current, &wanted), (Some(item), Some((about, _, _))) if item.about == *about);
        if same {
            continue;
        }
        if let Some(item) = current {
            let session = repository.get_session(project_id, session_id)?;
            if session.as_ref().is_some_and(|session| {
                crate::session_needs_you::ghostex_item_open(session, &item.tag)
            }) {
                let _ = crate::session_needs_you::drop_ghostex_item(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                    &item.tag,
                );
            }
            saved.items.remove(&item_key);
        }
        if let Some((about, title, detail)) = wanted {
            match crate::session_needs_you::add_ghostex_item(
                state,
                &db,
                &repository,
                project_id,
                session_id,
                &title,
                &detail,
            ) {
                Ok(tag) => {
                    saved.items.insert(item_key.clone(), Item { about, tag });
                }
                Err(error) => log(
                    state,
                    LogLevel::Warn,
                    "accountUsageWarningNeedsYouFailed",
                    Some(error.message),
                    json!({ "projectId": project_id, "sessionId": session_id }),
                ),
            }
        }
    }

    // Past windows are forgotten.
    saved
        .episodes
        .retain(|_, episode| parse_time(episode.resets_at.as_deref()).is_none_or(|at| at > now));
    if serde_json::to_value(&saved).ok() != serde_json::to_value(&before).ok() {
        write_saved(&db, &saved);
    }

    // The cards: republish every coordinator whose card came, went or changed.
    let changed: Vec<(String, String)> = {
        let Ok(mut cards) = cards().lock() else {
            return Ok(());
        };
        let mut changed: Vec<(String, String)> = cards
            .iter()
            .filter(|(key, card)| next_cards.get(*key) != Some(*card))
            .map(|(key, _)| key.clone())
            .collect();
        changed.extend(
            next_cards
                .iter()
                .filter(|(key, card)| cards.get(*key) != Some(*card))
                .map(|(key, _)| key.clone()),
        );
        *cards = next_cards;
        changed
    };
    for (project_id, session_id) in changed {
        crate::server::republish_session_chat(state, &project_id, &session_id);
    }
    Ok(())
}
