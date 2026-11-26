//! The login: every request needs voxl's password, except the scripts and assets the page
//! loads before it can present one and the health check. A browser signs in at its own
//! prompt (HTTP Basic, with any user name unless --basic-auth-user names one) or opens a
//! link with the password in `?token=`, which the page passes on to its socket and its
//! file requests. Programs send `Authorization: Bearer <password>` or Basic credentials
//! (`curl -u :<password>`).

use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use std::collections::HashMap;
use std::sync::Arc;

/// What the page fetches before it has the password, and the health check.
const PUBLIC: &[&str] = &[
    "/client.js",
    "/worker.js",
    "/audio.js",
    "/mic.js",
    "/favicon.ico",
    "/inter.woff2",
    "/healthz",
];

pub struct Login {
    /// The user name Basic credentials have to carry; any name when `None`.
    user: Option<String>,
    password: String,
}

impl Login {
    pub fn new(user: Option<String>, password: String) -> Self {
        Login { user, password }
    }

    /// From --enable-basic-auth (on unless false), --basic-auth-user and the password:
    /// --basic-auth-password, else VOXL_PASSWORD or SELKIES_BASIC_AUTH_PASSWORD, else a
    /// random one. `None` with the login off; the flag says whether the password was made
    /// up here, so the only way to learn it is the link voxl prints.
    pub fn from_args(a: &HashMap<String, String>) -> Option<(Self, bool)> {
        if !crate::truthy(a.get("enable-basic-auth"), true) {
            return None;
        }
        let given = a
            .get("basic-auth-password")
            .cloned()
            .or_else(|| std::env::var("VOXL_PASSWORD").ok())
            .or_else(|| std::env::var("SELKIES_BASIC_AUTH_PASSWORD").ok())
            .filter(|p| !p.is_empty());
        let generated = given.is_none();
        let password = given.unwrap_or_else(random_password);
        let user = a.get("basic-auth-user").filter(|u| !u.is_empty()).cloned();
        Some((Login::new(user, password), generated))
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    /// Whether a request to `uri` with these headers may go through.
    fn admits(&self, uri: &Uri, headers: &HeaderMap) -> bool {
        if PUBLIC.contains(&uri.path()) {
            return true;
        }
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().split_once(' '));
        if let Some((scheme, credentials)) = authorization {
            let credentials = credentials.trim();
            if scheme.eq_ignore_ascii_case("bearer") && same(credentials, &self.password) {
                return true;
            }
            if scheme.eq_ignore_ascii_case("basic") && self.basic(credentials) {
                return true;
            }
        }
        Query::<HashMap<String, String>>::try_from_uri(uri)
            .ok()
            .and_then(|q| q.0.get("token").map(|t| same(t, &self.password)))
            .unwrap_or(false)
    }

    /// Basic credentials (`user:password` in base64) with the password and, if one is
    /// set, the user name.
    fn basic(&self, encoded: &str) -> bool {
        let Some(decoded) = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
        else {
            return false;
        };
        let Some((user, password)) = decoded.split_once(':') else {
            return false;
        };
        let user_ok = self.user.as_deref().is_none_or(|u| same(user, u));
        same(password, &self.password) && user_ok
    }
}

/// Compares in time that depends only on the lengths, not on where the bytes differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |diff, (x, y)| diff | (x ^ y))
            == 0
}

/// 128 random bits as 32 hex digits.
fn random_password() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("random password");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Middleware for every route: answers 401 with a Basic challenge, so a browser asks
/// for the password, instead of running the handler for a request without it.
pub async fn check(State(login): State<Arc<Login>>, req: Request, next: Next) -> Response {
    if login.admits(req.uri(), req.headers()) {
        return next.run(req).await;
    }
    log::debug!("no password: {} {}", req.method(), req.uri().path());
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            r#"Basic realm="voxl", charset="UTF-8""#,
        )],
        "voxl needs its password: open the link voxl printed when it started, sign in when \
         the browser asks, or send Authorization: Bearer <password>",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn admitted(login: &Login, uri: &str, authorization: Option<&str>) -> bool {
        let mut headers = HeaderMap::new();
        if let Some(a) = authorization {
            headers.insert(header::AUTHORIZATION, HeaderValue::from_str(a).unwrap());
        }
        login.admits(&uri.parse().unwrap(), &headers)
    }

    fn basic(user_password: &str) -> String {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(user_password)
        )
    }

    /// A login with a password made up at runtime, as voxl makes one, and the password.
    fn with_random_password(user: Option<&str>) -> (Login, String) {
        let password = random_password();
        (
            Login::new(user.map(str::to_string), password.clone()),
            password,
        )
    }

    #[test]
    fn requests_without_the_password_are_refused() {
        let (login, pw) = with_random_password(None);
        let wrong = &pw[1..];
        for uri in [
            "/",
            "/index.html",
            "/ws",
            "/screenshot",
            "/files/list?path=",
            "/wait?idle=500",
            "/stats",
        ] {
            assert!(!admitted(&login, uri, None), "{uri}");
            assert!(
                !admitted(&login, uri, Some(&format!("Bearer {wrong}"))),
                "{uri}"
            );
            assert!(
                !admitted(&login, &format!("{uri}?token={wrong}"), None),
                "{uri}"
            );
        }
        // What the page loads before it can present the password, and health checks.
        for uri in [
            "/client.js",
            "/worker.js?token=x",
            "/inter.woff2",
            "/healthz",
        ] {
            assert!(admitted(&login, uri, None), "{uri}");
        }
    }

    #[test]
    fn each_way_of_sending_the_password_is_accepted() {
        let (login, pw) = with_random_password(None);
        assert!(admitted(
            &login,
            "/screenshot",
            Some(&format!("Bearer {pw}"))
        ));
        assert!(admitted(
            &login,
            "/screenshot",
            Some(&format!("bearer {pw}"))
        ));
        // Basic, with any user name (curl -u :password sends an empty one).
        assert!(admitted(&login, "/", Some(&basic(&format!(":{pw}")))));
        assert!(admitted(&login, "/", Some(&basic(&format!("me:{pw}")))));
        assert!(!admitted(
            &login,
            "/",
            Some(&basic(&format!("me:{}", &pw[1..])))
        ));
        assert!(!admitted(&login, "/", Some("Basic not-base64!")));
        // The link's query, decoded (here every byte is percent-encoded), beside the
        // page's other parameters.
        let encoded: String = pw.bytes().map(|b| format!("%{b:02X}")).collect();
        assert!(admitted(&login, &format!("/?stats&token={encoded}"), None));
        assert!(admitted(&login, &format!("/ws?token={pw}"), None));
        assert!(!admitted(&login, &format!("/ws?token={}", &pw[1..]), None));
    }

    #[test]
    fn basic_auth_user_has_to_match_when_set() {
        let (login, pw) = with_random_password(Some("ubuntu"));
        assert!(admitted(&login, "/", Some(&basic(&format!("ubuntu:{pw}")))));
        assert!(!admitted(&login, "/", Some(&basic(&format!("root:{pw}")))));
        // The user name only applies to Basic credentials.
        assert!(admitted(&login, "/", Some(&format!("Bearer {pw}"))));
        assert!(admitted(&login, &format!("/?token={pw}"), None));
    }

    #[test]
    fn password_comes_from_the_flag_or_is_generated() {
        let args = |v: &[(&str, &str)]| -> HashMap<String, String> {
            v.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert!(Login::from_args(&args(&[("enable-basic-auth", "false")])).is_none());
        let given = random_password();
        let (login, generated) =
            Login::from_args(&args(&[("basic-auth-password", &given)])).unwrap();
        assert_eq!((login.password(), generated), (given.as_str(), false));
        // Without a password from the environment, an empty one counts as none and each
        // start makes up a different one.
        if std::env::var_os("VOXL_PASSWORD").is_none()
            && std::env::var_os("SELKIES_BASIC_AUTH_PASSWORD").is_none()
        {
            let (a, generated) = Login::from_args(&args(&[("basic-auth-password", "")])).unwrap();
            let (b, _) = Login::from_args(&args(&[])).unwrap();
            assert!(generated);
            assert_eq!(a.password().len(), 32);
            assert_ne!(a.password(), b.password());
        }
    }
}
