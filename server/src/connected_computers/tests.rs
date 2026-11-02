use serde_json::json;

use super::registry::{self, Registration};
use super::*;

/// The registry is one per process, so the tests that write to it run one at a time.
pub(crate) fn reset() {
    registry::reset_for_tests();
}

pub(crate) fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn outbound(server_id: &str, agent_access: bool) -> Registration {
    Registration::from_params(
        &json!({
            "serverId": server_id,
            "name": "Omarchy",
            "baseUrl": "http://127.0.0.1:58800/",
            "token": "remote-token-value-0123456789abcdefghij",
            "agentAccess": agent_access,
        }),
        "S9l",
    )
    .expect("valid registration")
}

#[test]
fn a_registration_only_points_at_this_computers_loopback() {
    for base_url in [
        "http://10.0.0.4:58744",
        "https://127.0.0.1:58744",
        "http://127.0.0.1",
        "http://127.0.0.1:0",
        "http://example.com:80",
    ] {
        let params = json!({"serverId": "S4k", "baseUrl": base_url, "token": "t"});
        assert!(
            Registration::from_params(&params, "S9l").is_err(),
            "{base_url} must be refused"
        );
    }
    let params = json!({"serverId": "S4k", "baseUrl": "http://localhost:58800", "token": "t"});
    let registration = Registration::from_params(&params, "S9l").unwrap();
    assert_eq!(registration.base_url, "http://127.0.0.1:58800");
    assert!(registration.agent_access, "agent access is on by default");
}

#[test]
fn a_computer_cannot_register_itself_and_errors_never_quote_the_token() {
    let params = json!({"serverId": "S9l", "baseUrl": "http://127.0.0.1:1", "token": "secret-x"});
    let error = Registration::from_params(&params, "S9l").err().unwrap();
    assert!(!error.contains("secret-x"));
    let params =
        json!({"serverId": "S4k", "baseUrl": "http://127.0.0.1:1", "token": "has space secret-y"});
    let error = Registration::from_params(&params, "S9l").err().unwrap();
    assert!(!error.contains("secret-y"));
}

#[test]
fn a_renewal_keeps_the_token_the_other_computer_holds() {
    let _serial = serial();
    registry::reset_for_tests();
    let first =
        registry::register(outbound("S4k", true)).expect("an outbound computer gets a token");
    let second = registry::register(outbound("S4k", true)).expect("renewal");
    assert_eq!(first, second);
    assert!(first.starts_with("gxpeer_"));
    assert_eq!(
        authorize_peer_token(&first),
        Some(PeerAuthorization::Allowed {
            server_id: "S4k".into(),
            name: "Omarchy".into()
        })
    );
    assert_eq!(authorize_peer_token("gxpeer_wrong"), None);
    registry::reset_for_tests();
}

#[test]
fn turning_agent_access_off_refuses_the_other_computer_and_the_relay() {
    let _serial = serial();
    registry::reset_for_tests();
    let token = registry::register(outbound("S4k", true)).unwrap();
    registry::register(outbound("S4k", false));
    assert_eq!(
        authorize_peer_token(&token),
        Some(PeerAuthorization::AccessOff {
            name: "Omarchy".into()
        })
    );
    assert!(!find("S4k").unwrap().agent_access);
    assert!(registry::unregister("S4k"));
    assert_eq!(authorize_peer_token(&token), None);
    assert!(find("S4k").is_none());
    registry::reset_for_tests();
}

#[test]
fn the_listing_never_shows_a_token_or_address() {
    let _serial = serial();
    registry::reset_for_tests();
    registry::register(outbound("S4k", true));
    let listed = list();
    let text = listed[0].public_view().to_string();
    assert!(!text.contains("remote-token-value"));
    assert!(!text.contains("58800"));
    assert!(text.contains("Omarchy"));
    registry::reset_for_tests();
}

#[test]
fn relay_paths_name_a_computer_and_an_api_path() {
    assert_eq!(
        parse_relay_path("/api/computers/S4k/relay/api/readSessionChat"),
        Some(("S4k", "/api/readSessionChat"))
    );
    assert_eq!(parse_relay_path("/api/computers/S4k/relay/ext/x"), None);
    assert_eq!(parse_relay_path("/api/computers/../relay/api/x"), None);
    assert_eq!(
        parse_relay_path("/api/computers/S4k/relay/api/../control/stop"),
        None
    );
    assert_eq!(
        relay_base_url("http://127.0.0.1:58744", "S4k"),
        "http://127.0.0.1:58744/api/computers/S4k/relay"
    );
}
