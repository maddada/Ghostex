//! Connecting a provider (Q998 "browser then key"): Instabox's loopback + PKCE sign-in that mints
//! a scoped key, Boat's claim page with a 6-digit code, or a pasted key for either. Keys are sealed
//! with the vault in `cloud_providers`; nothing here logs a key, a code or a token.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_cloud::provider::ProviderAccount;
use ghostex_gx_cloud::{boat, instabox, pkce};
use ghostex_gx_protocol::cloud_box::{
    CloudConnectMethod, CloudProvider, CloudProviderConnectionState, CloudProviderId,
    ConnectParams, ConnectResult,
};

use super::http::call;
use super::store::{self, StoredProvider};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::server::AppState;

/// How long a browser sign-in waits for the user.
const SIGN_IN_WAIT: Duration = Duration::from_secs(600);
/// `key_id` of a Boat key the claim minted (revoked on disconnect); pasted keys have none.
const BOAT_CLAIMED_KEY: &str = "claim";

fn pending() -> &'static Mutex<HashMap<String, ConnectResult>> {
    static PENDING: OnceLock<Mutex<HashMap<String, ConnectResult>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_pending(connect_id: &str, change: impl FnOnce(&mut ConnectResult)) {
    if let Some(result) = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_mut(connect_id)
    {
        change(result);
    }
}

fn random_text(bytes: usize) -> String {
    let mut entropy = vec![0u8; bytes];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
    ghostex_gx_cloud::ticket::base64url_encode(&entropy)
}

fn iso_in(seconds: u64) -> String {
    super::time::iso_in(seconds as i64)
}

/// The saved key of a connected provider.
pub(super) fn provider_key(
    paths: &GxserverPaths,
    provider: &CloudProviderId,
) -> Result<String, DomainStateError> {
    let stored = store::read_provider(paths, provider)?
        .filter(|stored| stored.row.state == CloudProviderConnectionState::Connected)
        .ok_or_else(|| {
            super::params::not_connected(format!(
                "{} isn't connected. Connect it in Settings › Cloud Boxes.",
                provider.label()
            ))
        })?;
    let sealed = stored.key_sealed.ok_or_else(|| {
        super::params::not_connected(format!("{} has no saved key.", provider.label()))
    })?;
    store::unseal(paths, &sealed)
}

/// The row a connected provider shows.
pub(super) fn connected_row(
    provider: &CloudProviderId,
    account: &ProviderAccount,
    connected_at: Option<String>,
) -> CloudProvider {
    CloudProvider {
        label: provider.label().to_string(),
        id: provider.clone(),
        state: CloudProviderConnectionState::Connected,
        connect_methods: vec![CloudConnectMethod::Browser, CloudConnectMethod::Key],
        available: true,
        unavailable_reason: None,
        account_label: account.account_label.clone(),
        account_email: account.account_email.clone(),
        org_id: account.org_id.clone(),
        plan: account.plan.clone(),
        limits: account.limits.clone(),
        connected_at,
        error: None,
        debug_only: false,
    }
}

/// Reads who a key belongs to (and checks it works). Blocking.
pub(super) fn read_account(
    provider: &CloudProviderId,
    key: &str,
) -> Result<ProviderAccount, String> {
    match provider {
        CloudProviderId::Instabox => {
            call(instabox::usage_request(key), instabox::parse_usage).map_err(|error| error.message)
        }
        CloudProviderId::Boat => {
            let me = super::http::send(&boat::me_request(key)).map_err(|error| error.message)?;
            let limits = super::http::send(&boat::limits_request(key)).ok();
            boat::parse_account(&me, limits.as_ref()).map_err(|error| error.message)
        }
        other => Err(format!("{} has no key to check.", other.label())),
    }
}

fn save_connected(
    state: &AppState,
    provider: &CloudProviderId,
    key: &str,
    refresh: Option<&str>,
    key_id: Option<String>,
    mut account: ProviderAccount,
    email: Option<String>,
) -> Result<CloudProvider, DomainStateError> {
    if account.account_email.is_none() {
        account.account_email = email;
    }
    let row = connected_row(provider, &account, Some(crate::domain::params::now_iso()));
    store::save_provider(
        &state.paths,
        &StoredProvider {
            row: row.clone(),
            key_sealed: Some(store::seal(&state.paths, key)?),
            refresh_sealed: refresh
                .map(|refresh| store::seal(&state.paths, refresh))
                .transpose()?,
            key_id,
        },
    )?;
    super::events::broadcast_scope(state, &["providers", "spend"]);
    Ok(row)
}

fn connected_result(provider: &CloudProviderId, row: CloudProvider) -> ConnectResult {
    ConnectResult {
        provider: provider.clone(),
        state: CloudProviderConnectionState::Connected,
        connect_id: None,
        auth_url: None,
        user_code: None,
        browser_opened: false,
        expires_at: None,
        provider_row: Some(row),
        error: None,
    }
}

/// Opens a URL in this computer's browser.
fn open_browser(url: &str) -> bool {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// `connect`. Blocking parts run on a worker; the browser flows return at once with `connecting`.
pub(super) async fn start(
    state: Arc<AppState>,
    request: ConnectParams,
) -> Result<ConnectResult, DomainStateError> {
    let provider = request.provider.clone();
    if !matches!(provider, CloudProviderId::Instabox | CloudProviderId::Boat) {
        return Err(DomainStateError::bad_request(format!(
            "{} has nothing to connect.",
            provider.label()
        )));
    }
    if let Some(key) = request
        .key
        .clone()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
    {
        let expected = if provider == CloudProviderId::Instabox {
            "ibk_"
        } else {
            "boat_"
        };
        if !key.starts_with(expected) {
            return Err(DomainStateError::bad_request(format!(
                "That isn't a valid {} API key (they start with {expected}).",
                provider.label()
            )));
        }
        let task_state = state.clone();
        let task_provider = provider.clone();
        let row = super::params::blocking(move || -> Result<CloudProvider, DomainStateError> {
            let account =
                read_account(&task_provider, &key).map_err(super::params::not_connected)?;
            save_connected(&task_state, &task_provider, &key, None, None, account, None)
        })
        .await??;
        return Ok(connected_result(&provider, row));
    }
    let open = request.open_browser != Some(false);
    match provider {
        CloudProviderId::Instabox => start_instabox(state, open),
        _ => {
            let email = request
                .email
                .clone()
                .map(|email| email.trim().to_string())
                .filter(|email| email.contains('@'))
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Boat signs you in by the email of your Boat account. Enter it, or paste an API key instead.",
                    )
                })?;
            start_boat(state, email, open).await
        }
    }
}

/// `connectStatus`.
pub(super) fn status(connect_id: &str) -> Result<ConnectResult, DomainStateError> {
    pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(connect_id)
        .cloned()
        .ok_or_else(|| {
            DomainStateError::not_found("That sign-in is no longer waiting. Start it again.")
        })
}

fn start_instabox(state: Arc<AppState>, open: bool) -> Result<ConnectResult, DomainStateError> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| {
        super::params::internal(format!("Couldn't wait for the sign-in: {error}"))
    })?;
    let port = listener
        .local_addr()
        .map_err(|error| super::params::internal(error.to_string()))?
        .port();
    let redirect = instabox::redirect_uri(port);
    let verifier = pkce::verifier_from_entropy(&{
        let mut entropy = [0u8; 48];
        rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
        entropy
    });
    let csrf = random_text(24);
    let auth_url = instabox::sign_in_url(&redirect, &csrf, &pkce::s256_challenge(&verifier));
    let connect_id = random_text(12);
    let browser_opened = open && open_browser(&auth_url);
    let result = ConnectResult {
        provider: CloudProviderId::Instabox,
        state: CloudProviderConnectionState::Connecting,
        connect_id: Some(connect_id.clone()),
        auth_url: Some(auth_url),
        user_code: None,
        browser_opened,
        expires_at: Some(iso_in(SIGN_IN_WAIT.as_secs())),
        provider_row: None,
        error: None,
    };
    pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(connect_id.clone(), result.clone());
    std::thread::spawn(move || {
        let outcome = wait_for_code(&listener, &csrf).and_then(|code| {
            let tokens = call(
                instabox::cli_token_request(&code, &verifier, &redirect),
                instabox::parse_tokens,
            )
            .map_err(|error| error.message)?;
            let host = hostname();
            let (key, key_id) = call(
                instabox::create_key_request(&tokens.access_token, &format!("Ghostex on {host}")),
                instabox::parse_created_key,
            )
            .map_err(|error| error.message)?;
            let account = read_account(&CloudProviderId::Instabox, &key)?;
            let email = super::http::send(&instabox::me_request(&tokens.access_token))
                .ok()
                .and_then(|response| instabox::parse_me_email(&response));
            save_connected(
                &state,
                &CloudProviderId::Instabox,
                &key,
                tokens.refresh_token.as_deref(),
                key_id,
                account,
                email,
            )
            .map_err(|error| error.message)
        });
        finish(&connect_id, outcome);
        super::events::broadcast_scope(&state, &["providers"]);
    });
    Ok(result)
}

fn finish(connect_id: &str, outcome: Result<CloudProvider, String>) {
    set_pending(connect_id, |result| match outcome {
        Ok(row) => {
            result.state = CloudProviderConnectionState::Connected;
            result.provider_row = Some(row);
            result.user_code = None;
        }
        Err(message) => {
            result.state = CloudProviderConnectionState::Error;
            result.error = Some(message);
        }
    });
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "this computer".to_string())
}

/// Waits for the one request Instabox's redirect makes to `/callback`, answers the browser with a
/// small page, and returns the code after checking `state`.
fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        if started.elapsed() > SIGN_IN_WAIT {
            return Err(
                "The Instabox sign-in wasn't finished in time. Start it again.".to_string(),
            );
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = read_callback(stream, expected_state) {
                    return result;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// `None` for a request that isn't the callback (a favicon), so the wait goes on.
fn read_callback(mut stream: TcpStream, expected_state: &str) -> Option<Result<String, String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 8192];
    let read = stream.read(&mut buffer).ok()?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let target = request
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .to_string();
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    if path != "/callback" {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return None;
    }
    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let result = if params.get("state").map(String::as_str) != Some(expected_state) {
        Err("The sign-in answer didn't match this request. Start it again.".to_string())
    } else if let Some(error) = params.get("error") {
        Err(match error.as_str() {
            "access_denied" => "The Instabox sign-in was cancelled.".to_string(),
            "invite_required" | "invite_invalid" => {
                "Instabox needs an invite code for this account first.".to_string()
            }
            other => params
                .get("error_description")
                .cloned()
                .unwrap_or_else(|| format!("Instabox refused the sign-in ({other}).")),
        })
    } else {
        params
            .get("code")
            .cloned()
            .ok_or_else(|| "Instabox sent no sign-in code.".to_string())
    };
    let page = match &result {
        Ok(_) => "<h3>Instabox is connected to Ghostex.</h3><p>You can close this tab.</p>",
        Err(_) => {
            "<h3>Ghostex couldn't connect Instabox.</h3><p>Go back to Ghostex to try again.</p>"
        }
    };
    let body = format!("<!doctype html><meta charset=utf-8><title>Ghostex</title><body style=\"font-family:system-ui;padding:40px\">{page}</body>");
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    Some(result)
}

async fn start_boat(
    state: Arc<AppState>,
    email: String,
    open: bool,
) -> Result<ConnectResult, DomainStateError> {
    let identity_email = email.clone();
    let claim = super::params::blocking(move || {
        call(
            boat::identity_request(&identity_email),
            boat::parse_identity,
        )
    })
    .await?
    .map_err(|error| super::params::provider_error(error.message))?;
    let connect_id = random_text(12);
    let browser_opened = open && open_browser(&claim.verification_uri);
    let result = ConnectResult {
        provider: CloudProviderId::Boat,
        state: CloudProviderConnectionState::Connecting,
        connect_id: Some(connect_id.clone()),
        auth_url: Some(claim.verification_uri.clone()),
        user_code: Some(claim.user_code.clone()),
        browser_opened,
        expires_at: Some(iso_in(claim.expires_in)),
        provider_row: None,
        error: None,
    };
    pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(connect_id.clone(), result.clone());
    std::thread::spawn(move || {
        let deadline =
            Instant::now() + Duration::from_secs(claim.expires_in.max(60)) + SIGN_IN_WAIT;
        let mut interval = claim.interval;
        let outcome = loop {
            std::thread::sleep(Duration::from_secs(interval));
            if Instant::now() > deadline {
                break Err("The Boat sign-in wasn't finished in time. Start it again.".to_string());
            }
            match call(
                boat::claim_token_request(&claim.claim_token),
                boat::parse_claim_poll,
            ) {
                Ok(boat::ClaimPoll::Key(key)) => {
                    break read_account(&CloudProviderId::Boat, &key).and_then(|account| {
                        save_connected(
                            &state,
                            &CloudProviderId::Boat,
                            &key,
                            None,
                            Some(BOAT_CLAIMED_KEY.to_string()),
                            account,
                            Some(email.clone()),
                        )
                        .map_err(|error| error.message)
                    })
                }
                Ok(boat::ClaimPoll::Pending) => {}
                Ok(boat::ClaimPoll::SlowDown) => interval += 5,
                Ok(boat::ClaimPoll::Expired) => {
                    break Err("The Boat code expired. Start the sign-in again.".to_string())
                }
                // A dropped connection is retried on the next tick.
                Err(error) if error.retryable => {}
                Err(error) => break Err(error.message),
            }
        };
        finish(&connect_id, outcome);
        super::events::broadcast_scope(&state, &["providers"]);
    });
    Ok(result)
}

/// `disconnect`: revokes a key Ghostex minted (best effort) and forgets the provider.
pub(super) fn disconnect(
    state: &AppState,
    provider: &CloudProviderId,
) -> Result<(), DomainStateError> {
    let Some(stored) = store::read_provider(&state.paths, provider)? else {
        return Ok(());
    };
    let key = stored
        .key_sealed
        .as_deref()
        .and_then(|sealed| store::unseal(&state.paths, sealed).ok());
    match provider {
        CloudProviderId::Instabox => {
            let refresh = stored
                .refresh_sealed
                .as_deref()
                .and_then(|sealed| store::unseal(&state.paths, sealed).ok());
            if let (Some(refresh), Some(key_id)) = (refresh, stored.key_id.as_deref()) {
                if let Ok(tokens) =
                    call(instabox::refresh_request(&refresh), instabox::parse_tokens)
                {
                    let _ = super::http::send(&instabox::revoke_key_request(
                        &tokens.access_token,
                        key_id,
                    ));
                    let _ = super::http::send(&instabox::logout_request(&tokens.access_token));
                }
            }
        }
        CloudProviderId::Boat => {
            if let (Some(key), Some(BOAT_CLAIMED_KEY)) = (key.as_deref(), stored.key_id.as_deref())
            {
                let _ = super::http::send(&boat::revoke_request(key));
            }
        }
        _ => {}
    }
    store::delete_provider(&state.paths, provider)?;
    super::events::broadcast_scope(state, &["providers", "spend"]);
    Ok(())
}

/// Marks a provider whose key was refused, so Settings asks to connect again.
pub(super) fn mark_expired(state: &AppState, provider: &CloudProviderId, message: &str) {
    if let Ok(Some(mut stored)) = store::read_provider(&state.paths, provider) {
        stored.row.state = CloudProviderConnectionState::Expired;
        stored.row.available = false;
        stored.row.error = Some(message.to_string());
        let _ = store::update_provider_row(&state.paths, &stored.row);
        super::events::broadcast_scope(state, &["providers"]);
    }
}
