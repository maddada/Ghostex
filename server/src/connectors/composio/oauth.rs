//! The MCP authorization flow the `@modelcontextprotocol/sdk` client runs (`client/auth.js`), as
//! plain blocking steps: protected-resource and authorization-server discovery, dynamic client
//! registration, a PKCE S256 authorization URL, the code exchange and refresh-token renewal.
//! Storage and the order of the steps live in `account.rs`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::http::{agent, read_json, send, HttpError};

/// The MCP protocol version the SDK sends on discovery requests (`LATEST_PROTOCOL_VERSION`).
pub(crate) const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
pub(crate) const CLIENT_NAME: &str = "Ghostex";

/// What `discoverOAuthServerInfo` found, saved so a sign-in survives a restart.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Discovery {
    pub(crate) authorization_server_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resource_metadata_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resource_metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authorization_server_metadata: Option<Value>,
}

impl Discovery {
    fn metadata_str(&self, key: &str) -> Option<&str> {
        self.authorization_server_metadata
            .as_ref()?
            .get(key)?
            .as_str()
    }

    fn metadata_list(&self, key: &str) -> Option<Vec<String>> {
        let list = self
            .authorization_server_metadata
            .as_ref()?
            .get(key)?
            .as_array()?;
        Some(
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        )
    }

    fn endpoint(&self, key: &str, fallback_path: &str) -> Result<String, String> {
        if let Some(endpoint) = self.metadata_str(key) {
            return Ok(endpoint.to_string());
        }
        url::Url::parse(&self.authorization_server_url)
            .and_then(|base| base.join(fallback_path))
            .map(|url| url.to_string())
            .map_err(|_| "Composio's sign-in server address is not valid.".to_string())
    }
}

/// The registered client (RFC 7591 response).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ClientInfo {
    pub(crate) client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    pub(crate) redirect_uris: Vec<String>,
}

/// OAuth tokens plus when they were saved, so an expired access token is renewed before use.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Tokens {
    pub(crate) access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) token_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) scope: Option<String>,
    #[serde(default, rename = "savedAt", skip_serializing_if = "Option::is_none")]
    pub(crate) saved_at: Option<i64>,
}

impl Tokens {
    /// True when the server said how long the token lives and that time (less a minute) is over.
    pub(crate) fn expired(&self, now_ms: i64) -> bool {
        match (self.expires_in, self.saved_at) {
            (Some(expires_in), Some(saved_at)) => {
                now_ms >= saved_at + (expires_in as i64).saturating_mul(1_000) - 60_000
            }
            _ => false,
        }
    }
}

/// A failed OAuth call, classified the way the SDK's `auth()` recovers from it.
#[derive(Debug)]
pub(crate) enum OAuthError {
    /// `invalid_client` / `unauthorized_client`: forget everything and register again.
    InvalidClient(String),
    /// `invalid_grant`: the refresh token or code is no longer good.
    InvalidGrant(String),
    /// Any other answer, or no answer.
    Other(String),
}

impl OAuthError {
    pub(crate) fn message(&self) -> &str {
        match self {
            OAuthError::InvalidClient(message)
            | OAuthError::InvalidGrant(message)
            | OAuthError::Other(message) => message,
        }
    }

    fn from_http(error: HttpError) -> Self {
        if let HttpError::Status { body, .. } = &error {
            let code = serde_json::from_str::<Value>(body).ok().and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            match code.as_deref() {
                Some("invalid_client" | "unauthorized_client") => {
                    return OAuthError::InvalidClient(error.message())
                }
                Some("invalid_grant") => return OAuthError::InvalidGrant(error.message()),
                _ => {}
            }
        }
        OAuthError::Other(error.message())
    }
}

/// `resource_metadata`, `scope` and `error` from a `WWW-Authenticate: Bearer …` header
/// (`extractWWWAuthenticateParams`).
#[derive(Clone, Debug, Default)]
pub(crate) struct BearerChallenge {
    pub(crate) resource_metadata_url: Option<String>,
    pub(crate) scope: Option<String>,
}

pub(crate) fn parse_bearer_challenge(header: Option<&str>) -> BearerChallenge {
    let Some(header) = header else {
        return BearerChallenge::default();
    };
    let mut parts = header.splitn(2, ' ');
    let scheme = parts.next().unwrap_or_default();
    let Some(rest) = parts.next() else {
        return BearerChallenge::default();
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return BearerChallenge::default();
    }
    BearerChallenge {
        resource_metadata_url: challenge_field(rest, "resource_metadata")
            .filter(|value| url::Url::parse(value).is_ok()),
        scope: challenge_field(rest, "scope"),
    }
}

/// `field="value"` or `field=value` (the SDK's regex `${field}=(?:"([^"]+)"|([^\s,]+))`).
fn challenge_field(text: &str, field: &str) -> Option<String> {
    let needle = format!("{field}=");
    let mut search_from = 0;
    while let Some(found) = text[search_from..].find(&needle) {
        let start = search_from + found;
        let boundary_ok = start == 0
            || !text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let value_start = start + needle.len();
        if boundary_ok {
            let rest = &text[value_start..];
            let value = if let Some(quoted) = rest.strip_prefix('"') {
                quoted.split('"').next().unwrap_or_default()
            } else {
                rest.split(|c: char| c.is_whitespace() || c == ',')
                    .next()
                    .unwrap_or_default()
            };
            return (!value.is_empty()).then(|| value.to_string());
        }
        search_from = value_start;
    }
    None
}

fn discovery_get(url: &str) -> Result<ureq::http::Response<ureq::Body>, HttpError> {
    send(
        agent()
            .get(url)
            .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
            .header("Accept", "application/json")
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .call(),
    )
}

/// RFC 9728 protected-resource metadata: the `resource_metadata` URL from the 401 when given,
/// else the path-aware well-known URL, then the root one (`discoverMetadataWithFallback`).
fn protected_resource_metadata(
    server_url: &str,
    resource_metadata_url: Option<&str>,
) -> Result<Value, String> {
    let server = url::Url::parse(server_url).map_err(|error| error.to_string())?;
    let response = if let Some(explicit) = resource_metadata_url {
        discovery_get(explicit)
    } else {
        let path = server.path().trim_end_matches('/');
        let mut path_aware = server.clone();
        path_aware.set_path(&format!("/.well-known/oauth-protected-resource{path}"));
        match discovery_get(path_aware.as_str()) {
            Err(error)
                if !path.is_empty()
                    && error
                        .status()
                        .is_some_and(|status| (400..500).contains(&status)) =>
            {
                let mut root = server.clone();
                root.set_path("/.well-known/oauth-protected-resource");
                root.set_query(None);
                discovery_get(root.as_str())
            }
            other => other,
        }
    };
    let response = response.map_err(|error| error.message())?;
    read_json(response).map_err(|error| error.message())
}

/// RFC 8414 / OpenID discovery URLs in the SDK's order (`buildDiscoveryUrls`).
fn authorization_server_metadata_urls(authorization_server_url: &str) -> Vec<String> {
    let Ok(url) = url::Url::parse(authorization_server_url) else {
        return Vec::new();
    };
    let origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/');
    if path.is_empty() {
        return vec![
            format!("{origin}/.well-known/oauth-authorization-server"),
            format!("{origin}/.well-known/openid-configuration"),
        ];
    }
    vec![
        format!("{origin}/.well-known/oauth-authorization-server{path}"),
        format!("{origin}/.well-known/openid-configuration{path}"),
        format!("{origin}{path}/.well-known/openid-configuration"),
    ]
}

fn authorization_server_metadata(authorization_server_url: &str) -> Result<Option<Value>, String> {
    for url in authorization_server_metadata_urls(authorization_server_url) {
        match discovery_get(&url) {
            Ok(response) => {
                return read_json(response)
                    .map(Some)
                    .map_err(|error| error.message())
            }
            Err(error)
                if error
                    .status()
                    .is_some_and(|status| (400..500).contains(&status)) =>
            {
                continue
            }
            Err(error) => {
                return Err(format!(
                    "Couldn't read Composio's sign-in settings: {}",
                    error.message()
                ))
            }
        }
    }
    Ok(None)
}

/// `discoverOAuthServerInfo`: the authorization server named by the protected-resource metadata
/// (the MCP server's origin when there is none) and its metadata.
pub(crate) fn discover(
    server_url: &str,
    resource_metadata_url: Option<&str>,
) -> Result<Discovery, String> {
    let resource_metadata = protected_resource_metadata(server_url, resource_metadata_url).ok();
    let authorization_server_url = resource_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("authorization_servers"))
        .and_then(Value::as_array)
        .and_then(|servers| servers.first())
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            url::Url::parse(server_url)
                .ok()
                .and_then(|url| url.join("/").ok())
                .map(|url| url.to_string())
        })
        .ok_or_else(|| "Composio's sign-in server address is not valid.".to_string())?;
    let authorization_server_metadata = authorization_server_metadata(&authorization_server_url)?;
    Ok(Discovery {
        authorization_server_url,
        resource_metadata_url: resource_metadata_url.map(str::to_string),
        resource_metadata,
        authorization_server_metadata,
    })
}

/// `selectResourceURL`: the `resource` parameter, sent only when the protected-resource metadata
/// exists and covers the server URL.
pub(crate) fn select_resource(
    server_url: &str,
    discovery: &Discovery,
) -> Result<Option<String>, String> {
    let Some(resource) = discovery
        .resource_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("resource"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let requested = url::Url::parse(server_url).map_err(|error| error.to_string())?;
    let configured = url::Url::parse(resource).map_err(|error| error.to_string())?;
    let with_slash = |path: &str| {
        if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        }
    };
    let allowed = requested.origin() == configured.origin()
        && requested.path().len() >= configured.path().len()
        && with_slash(requested.path()).starts_with(&with_slash(configured.path()));
    if !allowed {
        return Err(format!(
            "Composio's sign-in is for {resource}, not {server_url}."
        ));
    }
    Ok(Some(configured.to_string()))
}

/// SEP-835 scope selection: the 401's `scope`, else the resource's `scopes_supported`.
pub(crate) fn resolve_scope(
    challenge_scope: Option<&str>,
    discovery: &Discovery,
) -> Option<String> {
    if let Some(scope) = challenge_scope.filter(|scope| !scope.is_empty()) {
        return Some(scope.to_string());
    }
    let scopes = discovery
        .resource_metadata
        .as_ref()?
        .get("scopes_supported")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    (!scopes.is_empty()).then(|| scopes.join(" "))
}

/// RFC 7591 dynamic client registration as a public client (`token_endpoint_auth_method: none`).
pub(crate) fn register_client(
    discovery: &Discovery,
    redirect_uri: &str,
    scope: Option<&str>,
) -> Result<ClientInfo, String> {
    if discovery.authorization_server_metadata.is_some()
        && discovery.metadata_str("registration_endpoint").is_none()
    {
        return Err("Composio's sign-in server doesn't accept new apps right now.".to_string());
    }
    let endpoint = discovery.endpoint("registration_endpoint", "/register")?;
    let mut body = json!({
        "client_name": CLIENT_NAME,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    if let Some(scope) = scope {
        body["scope"] = Value::String(scope.to_string());
    }
    let response = send(
        agent()
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .send(body.to_string()),
    )
    .map_err(|error| {
        format!(
            "Couldn't register Ghostex with Composio: {}",
            error.message()
        )
    })?;
    let value = read_json(response).map_err(|error| error.message())?;
    serde_json::from_value::<ClientInfo>(value)
        .ok()
        .filter(|client| !client.client_id.is_empty())
        .ok_or_else(|| "Composio didn't return an app id for Ghostex.".to_string())
}

/// 32 random bytes, base64url without padding (43 chars, all PKCE "unreserved" characters).
pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `startAuthorization`: the URL to open and the PKCE verifier to keep for the code exchange.
pub(crate) fn authorization_url(
    discovery: &Discovery,
    client: &ClientInfo,
    redirect_uri: &str,
    scope: Option<&str>,
    state: &str,
    resource: Option<&str>,
) -> Result<(String, String), String> {
    if discovery.authorization_server_metadata.is_some() {
        let response_types = discovery
            .metadata_list("response_types_supported")
            .unwrap_or_default();
        if !response_types.iter().any(|kind| kind == "code") {
            return Err(
                "Composio's sign-in server doesn't support this kind of sign-in.".to_string(),
            );
        }
        if let Some(methods) = discovery.metadata_list("code_challenge_methods_supported") {
            if !methods.iter().any(|method| method == "S256") {
                return Err(
                    "Composio's sign-in server doesn't support a secure sign-in.".to_string(),
                );
            }
        }
    }
    let endpoint = discovery.endpoint("authorization_endpoint", "/authorize")?;
    let mut url = url::Url::parse(&endpoint).map_err(|error| error.to_string())?;
    let verifier = random_token();
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &client.client_id)
            .append_pair("code_challenge", &code_challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state);
        if let Some(scope) = scope {
            query.append_pair("scope", scope);
            if scope.contains("offline_access") {
                query.append_pair("prompt", "consent");
            }
        }
        if let Some(resource) = resource {
            query.append_pair("resource", resource);
        }
    }
    Ok((url.to_string(), verifier))
}

/// `selectClientAuthMethod` + `applyClientAuthentication` for the token request.
fn apply_client_auth(
    discovery: &Discovery,
    client: &ClientInfo,
    params: &mut Vec<(String, String)>,
) -> Option<String> {
    let supported = discovery
        .metadata_list("token_endpoint_auth_methods_supported")
        .unwrap_or_default();
    let has_secret = client.client_secret.is_some();
    let known = |method: &str| {
        matches!(
            method,
            "client_secret_basic" | "client_secret_post" | "none"
        )
    };
    let method = match client.token_endpoint_auth_method.as_deref() {
        Some(method)
            if known(method) && (supported.is_empty() || supported.iter().any(|m| m == method)) =>
        {
            method.to_string()
        }
        _ if supported.is_empty() => if has_secret {
            "client_secret_basic"
        } else {
            "none"
        }
        .to_string(),
        _ if has_secret && supported.iter().any(|m| m == "client_secret_basic") => {
            "client_secret_basic".to_string()
        }
        _ if has_secret && supported.iter().any(|m| m == "client_secret_post") => {
            "client_secret_post".to_string()
        }
        _ if supported.iter().any(|m| m == "none") => "none".to_string(),
        _ => if has_secret {
            "client_secret_post"
        } else {
            "none"
        }
        .to_string(),
    };
    match method.as_str() {
        "client_secret_basic" => {
            let secret = client.client_secret.clone().unwrap_or_default();
            Some(format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", client.client_id, secret))
            ))
        }
        "client_secret_post" => {
            params.push(("client_id".to_string(), client.client_id.clone()));
            if let Some(secret) = &client.client_secret {
                params.push(("client_secret".to_string(), secret.clone()));
            }
            None
        }
        _ => {
            params.push(("client_id".to_string(), client.client_id.clone()));
            None
        }
    }
}

fn token_request(
    discovery: &Discovery,
    client: &ClientInfo,
    mut params: Vec<(String, String)>,
    resource: Option<&str>,
) -> Result<Tokens, OAuthError> {
    let endpoint = discovery
        .endpoint("token_endpoint", "/token")
        .map_err(OAuthError::Other)?;
    if let Some(resource) = resource {
        params.push(("resource".to_string(), resource.to_string()));
    }
    let authorization = apply_client_auth(discovery, client, &mut params);
    let mut request = agent()
        .post(&endpoint)
        .header("Accept", "application/json")
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build();
    if let Some(authorization) = authorization {
        request = request.header("Authorization", authorization);
    }
    let pairs = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let response = send(request.send_form(pairs)).map_err(OAuthError::from_http)?;
    let value = read_json(response).map_err(|error| OAuthError::Other(error.message()))?;
    let mut tokens = serde_json::from_value::<Tokens>(value)
        .ok()
        .filter(|tokens| !tokens.access_token.is_empty())
        .ok_or_else(|| OAuthError::Other("Composio didn't return a sign-in token.".to_string()))?;
    tokens.saved_at = Some(now_ms());
    Ok(tokens)
}

/// `exchangeAuthorization` / `fetchToken` with the authorization code.
pub(crate) fn exchange_code(
    discovery: &Discovery,
    client: &ClientInfo,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    resource: Option<&str>,
) -> Result<Tokens, OAuthError> {
    token_request(
        discovery,
        client,
        vec![
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), code.to_string()),
            ("code_verifier".to_string(), verifier.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
        ],
        resource,
    )
}

/// `refreshAuthorization`: keeps the old refresh token when the server doesn't send a new one.
pub(crate) fn refresh_tokens(
    discovery: &Discovery,
    client: &ClientInfo,
    refresh_token: &str,
    resource: Option<&str>,
) -> Result<Tokens, OAuthError> {
    let mut tokens = token_request(
        discovery,
        client,
        vec![
            ("grant_type".to_string(), "refresh_token".to_string()),
            ("refresh_token".to_string(), refresh_token.to_string()),
        ],
        resource,
    )?;
    if tokens.refresh_token.is_none() {
        tokens.refresh_token = Some(refresh_token.to_string());
    }
    Ok(tokens)
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
