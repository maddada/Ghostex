//! A workspace's Composio sign-in held by gxserver (Open Dot's `Provider` + `connect`/
//! `ensureClient`): the sealed sign-in file, the live MCP session, and the order of the OAuth
//! steps (renew the token, else hand back an authorization URL). Every workspace has its own.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use subtle::ConstantTimeEq;

use crate::paths::GxserverPaths;

use super::super::store;
use super::mcp_client::{self, McpError, McpSession};
use super::oauth::{self, BearerChallenge, ClientInfo, Discovery, OAuthError, Tokens};

/// Composio For You's hosted MCP server.
pub(crate) const MCP_URL: &str = "https://connect.composio.dev/mcp";
/// The provider folder the sealed sign-ins live in.
const PROVIDER: &str = "composio";

pub(crate) const NOT_CONNECTED: &str =
    "Composio isn't connected in this workspace. Sign in with `ghostex connectors sign-in`.";
pub(crate) const EXPIRED: &str =
    "This workspace's Composio sign-in expired. Sign in again with `ghostex connectors sign-in`.";
pub(crate) const UNREADABLE: &str =
    "This workspace's Composio sign-in can't be read anymore. Sign in again with `ghostex connectors sign-in`.";
const NO_SIGN_IN: &str =
    "No Composio sign-in is in progress. Start it again with `ghostex connectors sign-in`.";
pub(super) const STALE_LINK: &str =
    "This sign-in link is out of date. Start the Composio sign-in again with `ghostex connectors sign-in`.";
/// How long a started sign-in waits for Composio's redirect.
const SIGN_IN_TTL_MS: i64 = 10 * 60 * 1000;
const DIDNT_COMPLETE: &str = "Composio sign-in didn't complete. Try again.";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client: Option<ClientInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<Tokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery: Option<Discovery>,
    /// The OAuth `state` this sign-in sent; the redirect must bring it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    /// When `state` was issued (ms); the redirect is accepted for 10 minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_issued_at: Option<i64>,
    /// The redirect URI the authorization request used; the code exchange must repeat it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    /// App slugs last seen connected, so the app list is right straight after a restart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    connected_apps: Vec<String>,
}

/// What this process knows right now about one workspace's sign-in (Open Dot's
/// `globalThis.__dotsComposio`).
#[derive(Default)]
pub(super) struct Live {
    /// Cache of the sealed file; `None` until read. Only this module writes the file.
    stored: Option<Stored>,
    pub(super) session: Option<McpSession>,
    pub(super) tools: Vec<Value>,
    /// App slugs connected in the workspace's Composio account.
    pub(super) connected: Vec<String>,
    /// `tools/list` succeeded in this process.
    pub(super) listed: bool,
    /// When `tools/list` was last tried, so a Composio outage isn't retried on every read.
    pub(super) list_attempted_at: Option<std::time::Instant>,
    pub(super) background_refresh_running: bool,
}

/// The live state of every workspace, for one gxserver root (a different root starts clean).
struct LiveRoot {
    root: PathBuf,
    workspaces: HashMap<String, Live>,
}

static LIVE: Mutex<Option<LiveRoot>> = Mutex::new(None);
/// Serializes connecting, renewing and finishing a sign-in.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// Composio's sign-in server (WorkOS AuthKit) rotates refresh tokens, so two threads renewing at once would spend the same refresh token twice and the loser would sign the user out.
static AUTH_LOCK: Mutex<()> = Mutex::new(());

type ChangeListener = Arc<dyn Fn(&str) + Send + Sync>;
static CHANGE_LISTENER: Mutex<Option<ChangeListener>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs `f` on the live state of `workspace` in this gxserver.
pub(super) fn with_live<R>(
    paths: &GxserverPaths,
    workspace: &str,
    f: impl FnOnce(&mut Live) -> R,
) -> R {
    let mut live = lock(&LIVE);
    if live.as_ref().map(|live| live.root.as_path()) != Some(paths.root_dir.as_path()) {
        *live = Some(LiveRoot {
            root: paths.root_dir.clone(),
            workspaces: HashMap::new(),
        });
    }
    let root = live.as_mut().expect("live root was just set");
    f(root.workspaces.entry(workspace.to_string()).or_default())
}

/// Called with the workspace id after its sign-in, tool list or connected apps change, so
/// clients refetch.
pub(crate) fn set_changed_listener(listener: Arc<dyn Fn(&str) + Send + Sync>) {
    *lock(&CHANGE_LISTENER) = Some(listener);
}

pub(super) fn notify_changed(workspace: &str) {
    let listener = lock(&CHANGE_LISTENER).clone();
    if let Some(listener) = listener {
        listener(workspace);
    }
}

fn read_stored(paths: &GxserverPaths, workspace: &str) -> Result<Stored, store::ReadError> {
    let Some(plain) = store::read_secret(paths, PROVIDER, workspace)? else {
        return Ok(Stored::default());
    };
    Ok(serde_json::from_str(&plain).unwrap_or_default())
}

/// The saved sign-in. An unreadable one reads as signed out (Open Dot's `load`) and is not cached,
/// so a locked keychain is retried next time.
fn load(paths: &GxserverPaths, workspace: &str) -> Stored {
    if let Some(stored) = with_live(paths, workspace, |live| live.stored.clone()) {
        return stored;
    }
    match read_stored(paths, workspace) {
        Ok(stored) => {
            with_live(paths, workspace, |live| live.stored = Some(stored.clone()));
            stored
        }
        Err(_) => Stored::default(),
    }
}

fn save(paths: &GxserverPaths, workspace: &str, stored: &Stored) -> Result<(), String> {
    let plain = serde_json::to_string(stored).map_err(|error| error.to_string())?;
    store::write_secret(paths, PROVIDER, workspace, &plain)
        .map_err(|error| format!("Couldn't save the Composio sign-in: {error}"))?;
    with_live(paths, workspace, |live| live.stored = Some(stored.clone()));
    Ok(())
}

/// A sign-in is saved but doesn't open any more, so Settings asks for a new one instead of
/// showing the workspace as never signed in. Reads the file only while nothing is cached.
pub(super) fn sign_in_unreadable(paths: &GxserverPaths, workspace: &str) -> bool {
    with_live(paths, workspace, |live| {
        live.session.is_none() && live.stored.is_none()
    }) && matches!(
        read_stored(paths, workspace),
        Err(store::ReadError::Unreadable)
    )
}

/// Signed in: a live session, or saved tokens (Open Dot's `signedIn`).
pub(super) fn signed_in(paths: &GxserverPaths, workspace: &str) -> bool {
    with_live(paths, workspace, |live| live.session.is_some())
        || load(paths, workspace).tokens.is_some()
}

pub(super) fn access_token(paths: &GxserverPaths, workspace: &str) -> Option<String> {
    load(paths, workspace)
        .tokens
        .map(|tokens| tokens.access_token)
}

/// The app slugs last seen connected in the workspace's Composio account.
pub(super) fn remembered_apps(paths: &GxserverPaths, workspace: &str) -> Vec<String> {
    load(paths, workspace).connected_apps
}

/// Saves the connected app slugs when they changed (not after a sign-out).
pub(super) fn remember_apps(paths: &GxserverPaths, workspace: &str, apps: &[String]) {
    let _guard = lock(&AUTH_LOCK);
    let mut stored = load(paths, workspace);
    if stored.tokens.is_none() || stored.connected_apps == apps {
        return;
    }
    stored.connected_apps = apps.to_vec();
    let _ = save(paths, workspace, &stored);
}

pub(super) fn drop_session(paths: &GxserverPaths, workspace: &str) {
    with_live(paths, workspace, |live| live.session = None);
}

pub(super) enum Connected {
    Yes,
    /// Open this URL in the user's browser to sign in.
    SignInAt(String),
}

enum Renewal {
    Renewed,
    /// Composio refused the refresh token (or there is none): a new sign-in is needed.
    Refused,
    /// The renewal didn't get an answer.
    Unavailable(String),
}

fn ensure_discovery(
    paths: &GxserverPaths,
    workspace: &str,
    stored: &mut Stored,
    challenge: Option<&BearerChallenge>,
) -> Result<Discovery, String> {
    if let Some(discovery) = &stored.discovery {
        return Ok(discovery.clone());
    }
    let discovery = oauth::discover(
        MCP_URL,
        challenge.and_then(|challenge| challenge.resource_metadata_url.as_deref()),
    )?;
    stored.discovery = Some(discovery.clone());
    save(paths, workspace, stored)?;
    Ok(discovery)
}

/// The SDK's refresh branch of `auth()`, with its recovery: `invalid_grant` forgets the tokens,
/// `invalid_client` forgets everything.
fn renew(
    paths: &GxserverPaths,
    workspace: &str,
    stored: &mut Stored,
    challenge: Option<&BearerChallenge>,
) -> Result<Renewal, String> {
    let (Some(refresh_token), Some(client)) = (
        stored
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.refresh_token.clone()),
        stored.client.clone(),
    ) else {
        return Ok(Renewal::Refused);
    };
    let discovery = match ensure_discovery(paths, workspace, stored, challenge) {
        Ok(discovery) => discovery,
        Err(error) => return Ok(Renewal::Unavailable(error)),
    };
    let resource = oauth::select_resource(MCP_URL, &discovery)?;
    match oauth::refresh_tokens(&discovery, &client, &refresh_token, resource.as_deref()) {
        Ok(tokens) => {
            stored.tokens = Some(tokens);
            save(paths, workspace, stored)?;
            Ok(Renewal::Renewed)
        }
        Err(OAuthError::InvalidGrant(_)) => {
            stored.tokens = None;
            save(paths, workspace, stored)?;
            Ok(Renewal::Refused)
        }
        Err(OAuthError::InvalidClient(_)) => {
            *stored = Stored::default();
            save(paths, workspace, stored)?;
            Ok(Renewal::Refused)
        }
        Err(OAuthError::Other(message)) => Ok(Renewal::Unavailable(message)),
    }
}

/// Registers (again when the redirect changed), then builds the authorization URL and saves the
/// PKCE verifier and state for the redirect.
fn start_authorization(
    paths: &GxserverPaths,
    workspace: &str,
    stored: &mut Stored,
    challenge: &BearerChallenge,
    redirect_uri: &str,
) -> Result<String, String> {
    let discovery = ensure_discovery(paths, workspace, stored, Some(challenge))?;
    let scope = oauth::resolve_scope(challenge.scope.as_deref(), &discovery);
    let resource = oauth::select_resource(MCP_URL, &discovery)?;
    let client = match &stored.client {
        Some(client)
            if client.redirect_uris.is_empty()
                || client.redirect_uris.iter().any(|uri| uri == redirect_uri) =>
        {
            client.clone()
        }
        _ => {
            let client = oauth::register_client(&discovery, redirect_uri, scope.as_deref())?;
            stored.client = Some(client.clone());
            client
        }
    };
    let state = oauth::random_token();
    let (url, verifier) = oauth::authorization_url(
        &discovery,
        &client,
        redirect_uri,
        scope.as_deref(),
        &state,
        resource.as_deref(),
    )?;
    stored.verifier = Some(verifier);
    stored.state = Some(state);
    stored.state_issued_at = Some(oauth::now_ms());
    stored.redirect_uri = Some(redirect_uri.to_string());
    save(paths, workspace, stored)?;
    Ok(url)
}

/// Connects with the workspace's saved sign-in (Open Dot's `connect`). A token that ran out is
/// renewed first; `failed_token` is an access token a request was just refused with, renewed
/// unless another thread already did. With a `redirect_uri` a missing or refused sign-in returns
/// the URL to sign in at; without one it is an error.
pub(super) fn connect(
    paths: &GxserverPaths,
    workspace: &str,
    redirect_uri: Option<&str>,
    failed_token: Option<&str>,
) -> Result<Connected, String> {
    let _guard = lock(&AUTH_LOCK);
    let mut stored = load(paths, workspace);
    let had_tokens = stored.tokens.is_some();
    if !had_tokens && redirect_uri.is_none() {
        return Err(NOT_CONNECTED.to_string());
    }
    let renewed_elsewhere = failed_token.is_some_and(|failed| {
        stored
            .tokens
            .as_ref()
            .map(|tokens| tokens.access_token.as_str())
            != Some(failed)
    });
    if (failed_token.is_none() || renewed_elsewhere)
        && with_live(paths, workspace, |live| live.session.is_some())
    {
        return Ok(Connected::Yes);
    }
    let mut renewed_once = false;
    let mut unavailable: Option<String> = None;
    let token_is_stale = stored.tokens.as_ref().is_some_and(|tokens| {
        tokens.expired(oauth::now_ms()) || Some(tokens.access_token.as_str()) == failed_token
    });
    if token_is_stale {
        renewed_once = true;
        if let Renewal::Unavailable(message) = renew(paths, workspace, &mut stored, None)? {
            unavailable = Some(message);
        }
    }
    loop {
        let token = stored
            .tokens
            .as_ref()
            .map(|tokens| tokens.access_token.clone());
        match mcp_client::initialize("Composio", MCP_URL, token.as_deref(), oauth::CLIENT_NAME) {
            Ok(session) => {
                with_live(paths, workspace, |live| live.session = Some(session));
                return Ok(Connected::Yes);
            }
            Err(McpError::Unauthorized(challenge)) => {
                if !renewed_once {
                    renewed_once = true;
                    match renew(paths, workspace, &mut stored, Some(&challenge))? {
                        Renewal::Renewed => continue,
                        Renewal::Refused => {}
                        Renewal::Unavailable(message) => unavailable = Some(message),
                    }
                }
                let Some(redirect_uri) = redirect_uri else {
                    if let Some(message) = unavailable {
                        return Err(format!("Couldn't renew the Composio sign-in: {message}"));
                    }
                    return Err(if had_tokens { EXPIRED } else { NOT_CONNECTED }.to_string());
                };
                return start_authorization(
                    paths,
                    workspace,
                    &mut stored,
                    &challenge,
                    redirect_uri,
                )
                .map(Connected::SignInAt);
            }
            Err(error) => return Err(error.message()),
        }
    }
}

/// The OAuth redirect landed with `code` (Open Dot's `finishSignIn`), checked against the
/// `state` the workspace's sign-in sent.
pub(super) fn finish(
    paths: &GxserverPaths,
    workspace: &str,
    code: &str,
    state: &str,
) -> Result<(), String> {
    {
        let _guard = lock(&AUTH_LOCK);
        let mut stored = load(paths, workspace);
        let (Some(verifier), Some(client), Some(redirect_uri)) = (
            stored.verifier.clone(),
            stored.client.clone(),
            stored.redirect_uri.clone(),
        ) else {
            return Err(NO_SIGN_IN.to_string());
        };
        if !state_matches(&stored, state) {
            return Err(STALE_LINK.to_string());
        }
        let discovery = ensure_discovery(paths, workspace, &mut stored, None)?;
        let resource = oauth::select_resource(MCP_URL, &discovery)?;
        let tokens = oauth::exchange_code(
            &discovery,
            &client,
            code,
            &verifier,
            &redirect_uri,
            resource.as_deref(),
        )
        .map_err(|error| format!("Composio didn't accept the sign-in: {}", error.message()))?;
        stored.tokens = Some(tokens);
        stored.verifier = None;
        stored.state = None;
        stored.state_issued_at = None;
        save(paths, workspace, &stored)?;
        drop_session(paths, workspace);
    }
    match connect(paths, workspace, None, None) {
        Ok(_) => Ok(()),
        Err(error) if error == EXPIRED || error == NOT_CONNECTED => Err(DIDNT_COMPLETE.to_string()),
        Err(error) => Err(error),
    }
}

/// The redirect brought back the `state` of the sign-in in progress, issued less than 10 minutes
/// ago (constant-time compare).
///
/// CDXC:Connectors 2026-10-15 WHY:
/// The redirect route needs no gxserver token, so any website can send the user's browser to it. Only a request carrying the live, unexpired state may change anything or tell clients to refetch; a bogus one must not cancel the real sign-in either.
fn state_matches(stored: &Stored, state: &str) -> bool {
    let expected = stored.state.as_deref().unwrap_or_default();
    let fresh = stored
        .state_issued_at
        .is_some_and(|issued| (oauth::now_ms() - issued).abs() <= SIGN_IN_TTL_MS);
    fresh && !expected.is_empty() && bool::from(expected.as_bytes().ct_eq(state.as_bytes()))
}

/// The workspace whose sign-in in progress sent `state`, among `workspaces`.
pub(super) fn workspace_for_state(
    paths: &GxserverPaths,
    workspaces: &[String],
    state: &str,
) -> Option<String> {
    workspaces
        .iter()
        .find(|workspace| state_matches(&load(paths, workspace), state))
        .cloned()
}

/// The user declined at Composio: forget the workspace's sign-in in progress when `state` is its
/// own.
pub(super) fn cancel_pending(paths: &GxserverPaths, workspace: &str, state: &str) {
    let _guard = lock(&AUTH_LOCK);
    let mut stored = load(paths, workspace);
    if state_matches(&stored, state) {
        stored.verifier = None;
        stored.state = None;
        stored.state_issued_at = None;
        let _ = save(paths, workspace, &stored);
    }
}

/// Forgets the workspace's sign-in and everything learned with it (Open Dot's `signOut`).
pub(super) fn sign_out(paths: &GxserverPaths, workspace: &str) {
    let _guard = lock(&AUTH_LOCK);
    with_live(paths, workspace, |live| {
        *live = Live::default();
        live.stored = Some(Stored::default());
    });
    store::remove_secret(paths, PROVIDER, workspace);
}

/// A Composio request that failed. `NotRun` stopped before the request (no sign-in, a refused
/// renewal, a failed connect); `Unreached` is the request itself failing to connect (DNS, connect,
/// TLS handshake), so a retry can't repeat a side effect; `Failed` may have run.
pub(super) enum RpcFailure {
    NotRun(String),
    Unreached(String),
    Failed(String),
}

impl RpcFailure {
    pub(super) fn message(&self) -> &str {
        match self {
            RpcFailure::NotRun(message)
            | RpcFailure::Unreached(message)
            | RpcFailure::Failed(message) => message,
        }
    }
}

/// One JSON-RPC request in the workspace's live session, connecting first, renewing the token on
/// a 401 and starting a new session when Composio dropped the old one.
pub(super) fn rpc(
    paths: &GxserverPaths,
    workspace: &str,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, RpcFailure> {
    let mut failed_token: Option<String> = None;
    for _ in 0..3 {
        connect(paths, workspace, None, failed_token.as_deref()).map_err(RpcFailure::NotRun)?;
        let Some(session) = with_live(paths, workspace, |live| live.session.clone()) else {
            continue;
        };
        let token = access_token(paths, workspace);
        match mcp_client::request(
            "Composio",
            MCP_URL,
            token.as_deref(),
            &session,
            method,
            params.clone(),
            timeout,
        ) {
            Ok(result) => return Ok(result),
            Err(McpError::Unauthorized(_)) => {
                drop_session(paths, workspace);
                failed_token = Some(token.unwrap_or_default());
            }
            Err(McpError::SessionGone) => {
                drop_session(paths, workspace);
                failed_token = None;
            }
            Err(error @ McpError::Unreached(_)) => {
                drop_session(paths, workspace);
                return Err(RpcFailure::Unreached(error.message()));
            }
            Err(error) => return Err(RpcFailure::Failed(error.message())),
        }
    }
    Err(RpcFailure::NotRun(EXPIRED.to_string()))
}
