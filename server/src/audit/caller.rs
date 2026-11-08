//! Who made the HTTP request being served, kept for the length of the request so `Actor::current`
//! can name it from any handler.

use std::future::Future;

use axum::http::HeaderMap;

use super::types::Actor;
use crate::auth::{bearer_token, hash_device_api_token, is_expected_gxserver_auth_token};

/// The session the `ghostex` CLI says it runs in (`GHOSTEX_SESSION_ID` and friends). Nothing
/// proves it until sessions have their own tokens, so the log shows it as "says session …".
pub const CALLER_SESSION_HEADER: &str = "x-ghostex-caller-session";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    SharedToken {
        claimed_session: Option<String>,
    },
    PairedDevice {
        token_hash: String,
    },
    ConnectedComputer {
        server_id: String,
        name: String,
    },
    CollabGuest {
        share_id: String,
    },
    /// A session's own token or the desktop's owner session (crate::security, GitHub #495).
    Principal(crate::security::Principal),
    /// No token this gxserver knows (the request is refused, or it is an open route).
    Unknown,
}

tokio::task_local! {
    static CALLER: Caller;
    /// The signed approval the approvals gate spent on this request, if any: an entry the
    /// handler records for it carries it and reads as approved.
    static APPROVAL: std::cell::RefCell<Option<super::ApprovalRef>>;
}

/// Runs `future` with `caller` as the request's caller.
pub(crate) async fn with_caller<F: Future>(caller: Caller, future: F) -> F::Output {
    CALLER
        .scope(
            caller,
            APPROVAL.scope(std::cell::RefCell::new(None), future),
        )
        .await
}

/// The approvals gate spent `approval` on the request being served (security_gates.rs).
pub(crate) fn note_approval(approval: super::ApprovalRef) {
    let _ = APPROVAL.try_with(|slot| *slot.borrow_mut() = Some(approval));
}

/// The approval spent on the request being served, if any.
pub(super) fn request_approval() -> Option<super::ApprovalRef> {
    APPROVAL
        .try_with(|slot| slot.borrow().clone())
        .ok()
        .flatten()
}

impl Caller {
    /// Classifies the request's bearer token the same way the auth gate does.
    pub(crate) fn from_headers(
        headers: &HeaderMap,
        auth_token: &str,
        device_tokens: &crate::auth::DeviceApiTokens,
        session_tokens: &crate::security::SessionTokens,
    ) -> Self {
        let Some(token) = bearer_token(headers) else {
            return Self::Unknown;
        };
        if is_expected_gxserver_auth_token(token, auth_token) {
            let claimed_session = headers
                .get(CALLER_SESSION_HEADER)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.len() <= 200)
                .map(str::to_string);
            return Self::SharedToken { claimed_session };
        }
        if let Some(principal) = session_tokens.resolve(token) {
            return Self::Principal(principal);
        }
        if device_tokens.accepts(token) {
            return Self::PairedDevice {
                token_hash: hash_device_api_token(token),
            };
        }
        if let Some(crate::connected_computers::PeerAuthorization::Allowed { server_id, name }) =
            crate::connected_computers::authorize_peer_token(token)
        {
            return Self::ConnectedComputer { server_id, name };
        }
        if let Some(share) = crate::collab::shares::authorize_token(token) {
            return Self::CollabGuest { share_id: share.id };
        }
        Self::Unknown
    }
}

pub(super) fn current_actor() -> Actor {
    let Ok(caller) = CALLER.try_with(Clone::clone) else {
        return Actor::Ghostex;
    };
    match caller {
        Caller::SharedToken { claimed_session } => Actor::SharedToken { claimed_session },
        Caller::PairedDevice { token_hash } => paired_device_actor(&token_hash),
        Caller::ConnectedComputer { server_id, name } => {
            Actor::ConnectedComputer { server_id, name }
        }
        Caller::CollabGuest { share_id } => Actor::CollabGuest { name: share_id },
        Caller::Principal(principal) => Actor::from(&principal),
        Caller::Unknown => Actor::Ghostex,
    }
}

/// The paired phone a device token belongs to, by the hash stored with its row.
fn paired_device_actor(token_hash: &str) -> Actor {
    let paths = crate::paths::get_gxserver_paths(None);
    let device = crate::storage::open_gxserver_database(&paths)
        .ok()
        .and_then(|db| {
            crate::remote_access::repository::RemotePairingRepository::new(&db)
                .list_devices()
                .ok()
        })
        .and_then(|devices| {
            devices
                .into_iter()
                .find(|device| device.api_token_hash.as_deref() == Some(token_hash))
        });
    match device {
        Some(device) => Actor::PairedDevice {
            id: Some(device.id),
            name: device.name,
        },
        None => Actor::PairedDevice {
            id: None,
            name: "a paired phone".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(token: &str, session: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        if let Some(session) = session {
            headers.insert(CALLER_SESSION_HEADER, session.parse().unwrap());
        }
        headers
    }

    #[tokio::test]
    async fn the_request_caller_names_the_actor() {
        let main = "m".repeat(43);
        let devices = crate::auth::DeviceApiTokens::default();
        let caller = Caller::from_headers(
            &headers(&main, Some("S90:P1:G2")),
            &main,
            &devices,
            &Default::default(),
        );
        assert_eq!(
            caller,
            Caller::SharedToken {
                claimed_session: Some("S90:P1:G2".into())
            }
        );
        let actor = with_caller(caller, async { Actor::current() }).await;
        assert_eq!(
            actor,
            Actor::SharedToken {
                claimed_session: Some("S90:P1:G2".into())
            }
        );
        assert_eq!(Actor::current(), Actor::Ghostex);
    }

    #[test]
    fn a_device_token_is_told_apart_from_the_main_token() {
        let main = "m".repeat(43);
        let device = crate::auth::mint_device_api_token();
        let devices = crate::auth::DeviceApiTokens::new([hash_device_api_token(&device)]);
        assert_eq!(
            Caller::from_headers(
                &headers(&device, None),
                &main,
                &devices,
                &Default::default()
            ),
            Caller::PairedDevice {
                token_hash: hash_device_api_token(&device)
            }
        );
        assert_eq!(
            Caller::from_headers(&headers("nope", None), &main, &devices, &Default::default()),
            Caller::Unknown
        );
    }
}
