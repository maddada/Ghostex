//! Which requests voxl answers. Browsers let any web page open a WebSocket to voxl or send
//! it requests, and a page whose own domain is made to resolve to voxl's address (DNS
//! rebinding) even counts as voxl's own page. So voxl only answers a `Host` that can't be
//! rebound (an IP address or localhost) or that `--allowed-hosts` names, and turns away
//! what a browser sends from another site's page (its `Origin`) unless `--allowed-origins`
//! names that site. Programs that aren't browsers send no `Origin` and get through.

use axum::extract::{Request, State};
use axum::http::{header, HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::IpAddr;
use std::sync::Arc;

pub struct Access {
    /// Host names: ".example.com" also covers its subdomains, "*" any host.
    hosts: Vec<String>,
    /// Origins (`scheme://host[:port]`), "*" any origin.
    origins: Vec<String>,
}

impl Access {
    /// From `--allowed-hosts` and `--allowed-origins`, both comma-separated.
    pub fn new(hosts: Option<&str>, origins: Option<&str>) -> Self {
        let list = |v: Option<&str>| -> Vec<String> {
            v.unwrap_or("")
                .split(',')
                .map(|s| s.trim().trim_end_matches('/').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        };
        Access {
            // A port given with a host name doesn't matter: the name is what's rebound.
            hosts: list(hosts).iter().map(|h| split_host(h).0).collect(),
            origins: list(origins).iter().map(|o| origin_key(o)).collect(),
        }
    }

    fn host_allowed(&self, name: &str) -> bool {
        name.parse::<IpAddr>().is_ok()
            || name == "localhost"
            || name.ends_with(".localhost")
            || self.hosts.iter().any(|h| {
                h == "*"
                    || h == name
                    || h.strip_prefix('.')
                        .is_some_and(|domain| name == domain || name.ends_with(h.as_str()))
            })
    }

    /// Why a request with these `Host` and `Origin` headers is turned away, or `None` to
    /// answer it.
    fn refuse(&self, host: Option<&str>, origin: Option<&str>) -> Option<String> {
        let (name, port) = host.map(split_host).unwrap_or_default();
        // Browsers always send a Host, so one without it comes from another kind of client.
        if host.is_some() && !self.host_allowed(&name) {
            return Some(format!(
                "voxl doesn't answer for the host {name:?}. To serve it under that name, start voxl with --allowed-hosts={name}"
            ));
        }
        let origin = origin?.trim();
        let lower = origin.to_ascii_lowercase();
        let key = origin_key(&lower);
        if self.origins.iter().any(|o| o == "*" || *o == key) {
            return None;
        }
        // voxl's own page: the origin the browser loaded it from is the host it asks for.
        let own = host.is_some()
            && split_origin(&lower).is_some_and(|(n, p, default)| {
                n == name && p.unwrap_or(default) == port.unwrap_or(default)
            });
        (!own).then(|| {
            format!(
                "voxl only takes requests from its own page, not from {origin}. To let that site connect, start voxl with --allowed-origins={origin}"
            )
        })
    }
}

/// Splits a `Host` value into its lowercase name (an IPv6 address without the brackets)
/// and its port. A port that isn't a number becomes 0, which matches no origin.
fn split_host(v: &str) -> (String, Option<u16>) {
    let v = v.trim();
    let (name, port) = match v.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((ip, after)) => (ip, after.strip_prefix(':')),
            None => (rest, None),
        },
        None => match v.rsplit_once(':') {
            Some((n, p)) if !n.contains(':') => (n, Some(p)),
            _ => (v, None),
        },
    };
    (
        name.trim_end_matches('.').to_ascii_lowercase(),
        port.map(|p| p.parse().unwrap_or(0)),
    )
}

/// Splits an origin into its host name, its port and its scheme's default port; `None`
/// for one that isn't http(s), such as the "null" of a sandboxed frame.
fn split_origin(v: &str) -> Option<(String, Option<u16>, u16)> {
    let (scheme, rest) = v.split_once("://")?;
    let default = match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        _ => return None,
    };
    let (name, port) = split_host(rest);
    Some((name, port, default))
}

/// An origin with its port written out, so `https://a.example` and `https://a.example:443`
/// compare equal; anything else as it is.
fn origin_key(v: &str) -> String {
    match (v.split_once("://"), split_origin(v)) {
        (Some((scheme, _)), Some((name, port, default))) => {
            format!("{scheme}://{name}:{}", port.unwrap_or(default))
        }
        _ => v.to_string(),
    }
}

fn value(req: &Request, name: HeaderName) -> Option<&str> {
    // A value that isn't text can't name an allowed host or origin.
    req.headers().get(name).map(|v| v.to_str().unwrap_or(""))
}

/// Middleware for every route: answers 403 instead of running the handler for a request
/// [`Access`] turns away.
pub async fn check(State(access): State<Arc<Access>>, req: Request, next: Next) -> Response {
    let host = value(&req, header::HOST).or_else(|| req.uri().authority().map(|a| a.as_str()));
    if let Some(why) = access.refuse(host, value(&req, header::ORIGIN)) {
        log::warn!("refused {} {}: {why}", req.method(), req.uri().path());
        return (StatusCode::FORBIDDEN, why).into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn served(a: &Access, host: &str, origin: Option<&str>) -> bool {
        a.refuse(Some(host), origin).is_none()
    }

    #[test]
    fn unlisted_host_names_are_refused() {
        let a = Access::new(None, None);
        // A host name voxl wasn't told to serve.
        assert!(!served(&a, "other.example:6901", None));
        assert!(!served(
            &a,
            "other.example:6901",
            Some("http://other.example:6901")
        ));
        // Names that can't be rebound.
        for host in [
            "127.0.0.1:6901",
            "[::1]:6901",
            "10.0.0.5",
            "localhost:6901",
            "voxl.localhost",
        ] {
            assert!(served(&a, host, None), "{host}");
        }
    }

    #[test]
    fn other_sites_pages_are_refused() {
        let a = Access::new(None, None);
        assert!(!served(
            &a,
            "127.0.0.1:6901",
            Some("https://another-site.example")
        ));
        assert!(!served(&a, "127.0.0.1:6901", Some("null")));
        // Another app on the same machine is another site.
        assert!(!served(&a, "localhost:6901", Some("http://localhost:3000")));
        // voxl's own page, and programs that send no Origin.
        assert!(served(&a, "127.0.0.1:6901", Some("http://127.0.0.1:6901")));
        assert!(served(&a, "[::1]:6901", Some("http://[::1]:6901")));
        assert!(served(&a, "127.0.0.1:6901", None));
    }

    #[test]
    fn allowed_hosts_and_origins_are_served() {
        // Behind a proxy that keeps the viewer's host name (TLS ends at the proxy).
        let a = Access::new(
            Some(".preview.example.com,pc.local:6901"),
            Some("https://app.example.com/"),
        );
        assert!(served(
            &a,
            "abc.preview.example.com",
            Some("https://abc.preview.example.com")
        ));
        assert!(served(
            &a,
            "abc.preview.example.com:443",
            Some("https://abc.preview.example.com")
        ));
        assert!(served(&a, "preview.example.com", None));
        assert!(served(&a, "pc.local:6901", Some("http://pc.local:6901")));
        assert!(!served(&a, "notpreview.example.com", None));
        // A page on the allowed site may connect; others still can't.
        assert!(served(
            &a,
            "127.0.0.1:6901",
            Some("https://app.example.com")
        ));
        assert!(!served(
            &a,
            "127.0.0.1:6901",
            Some("https://another-site.example")
        ));
        // The proxied page's origin has to match the host it asked for.
        assert!(!served(
            &a,
            "abc.preview.example.com",
            Some("https://xyz.preview.example.com")
        ));

        // An allowed origin matches with or without its scheme's default port.
        let ports = Access::new(None, Some("https://app.example.com:443,http://pc.example"));
        assert!(served(
            &ports,
            "127.0.0.1:6901",
            Some("https://app.example.com")
        ));
        assert!(served(
            &ports,
            "127.0.0.1:6901",
            Some("http://pc.example:80")
        ));
        assert!(!served(
            &ports,
            "127.0.0.1:6901",
            Some("https://app.example.com:8443")
        ));
        assert!(!served(
            &ports,
            "127.0.0.1:6901",
            Some("http://app.example.com")
        ));

        let any = Access::new(Some("*"), Some("*"));
        assert!(served(
            &any,
            "other.example",
            Some("https://another-site.example")
        ));
    }
}
