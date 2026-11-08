//! The component's owner and guest rules against a fake gxserver (`host::fake`): the gate is asked
//! for and never skipped, a grant for another action or chat doesn't count, no read hands out a
//! link or a code, guests write only as their share's mode allows.

use ghostex_gx_protocol::approval::SensitiveAction;
use ghostex_gx_protocol::collab_component::{GrantInfo, HostCall, OwnerCall, OwnerReply};
use serde_json::{json, Map, Value};

use crate::config::GuestMode;
use crate::host::fake::{friendly, FakeHost};
use crate::shares;

/// The component keeps one share list per process, so these run one at a time.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var("GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE", "1");
    shares::stop_all();
    guard
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap()
}

async fn owner(value: Value, grant: Option<GrantInfo>) -> OwnerReply {
    crate::owner::handle(OwnerCall {
        params: params(value),
        grant,
    })
    .await
}

fn grant(action: SensitiveAction, target: &str) -> Option<GrantInfo> {
    Some(GrantInfo {
        approval_id: "appr_1".into(),
        action,
        target: target.into(),
    })
}

async fn shared() -> shares::StoredShare {
    let reply = owner(
        json!({"op": "share", "projectId": "p", "sessionId": "s"}),
        grant(SensitiveAction::CollabShareChat, "s"),
    )
    .await;
    assert!(reply.result.is_some(), "{reply:?}");
    shares::for_session("p", "s").expect("shared")
}

#[tokio::test]
async fn sharing_asks_for_approval_and_only_its_own_grant_counts() {
    let _serial = serial();
    FakeHost::install(friendly);
    let call = json!({"op": "share", "projectId": "p", "sessionId": "s"});
    let reply = owner(call.clone(), None).await;
    let need = reply.needs_presence.expect("asks");
    assert_eq!(need.action, SensitiveAction::CollabShareChat);
    assert_eq!(need.target, "s");
    assert!(need.summary.contains("Fix login"), "{}", need.summary);
    assert!(
        shares::list().is_empty(),
        "nothing shared before the approval"
    );

    // A grant for another action, or another chat, is not this approval.
    for wrong in [
        grant(SensitiveAction::CollabEnableSendMode, "s"),
        grant(SensitiveAction::CollabShareChat, "other"),
    ] {
        let reply = owner(call.clone(), wrong).await;
        assert!(reply.needs_presence.is_some());
        assert!(shares::list().is_empty());
    }

    let reply = owner(call, grant(SensitiveAction::CollabShareChat, "s")).await;
    let share = &reply.result.as_ref().expect("shared")["share"];
    assert!(share["link"].as_str().unwrap().contains("/s/"));
    assert_eq!(share["code"].as_str().unwrap().len(), shares::CODE_DIGITS);
    assert_eq!(reply.performed.len(), 1);
    assert!(reply.performed[0].ok);

    // No read hands out the link or the code.
    let read = owner(json!({"op": "read"}), None).await.result.unwrap();
    assert_eq!(read["installed"], true);
    assert_eq!(read["config"]["enabled"], true);
    let text = read.to_string();
    assert!(!text.contains(share["code"].as_str().unwrap()));
    assert!(!text.contains("\"link\""));
}

#[tokio::test]
async fn a_deleted_chat_cannot_be_shared() {
    let _serial = serial();
    FakeHost::install(|call| match call {
        HostCall::ChatInfo { .. } => Ok(json!({"exists": false})),
        _ => Ok(json!({})),
    });
    let reply = owner(
        json!({"op": "share", "projectId": "p", "sessionId": "gone"}),
        None,
    )
    .await;
    assert_eq!(reply.refused.unwrap().code, "notFound");
}

#[tokio::test]
async fn send_mode_needs_its_own_approval_and_narrowing_never_asks() {
    let _serial = serial();
    FakeHost::install(friendly);
    let share = shared().await;
    let to_send = json!({"op": "setMode", "shareId": share.id, "mode": "send"});
    let reply = owner(to_send.clone(), None).await;
    assert_eq!(
        reply.needs_presence.unwrap().action,
        SensitiveAction::CollabEnableSendMode
    );
    assert_eq!(shares::get(&share.id).unwrap().mode, GuestMode::Approve);
    owner(to_send, grant(SensitiveAction::CollabEnableSendMode, "s")).await;
    assert_eq!(shares::get(&share.id).unwrap().mode, GuestMode::Send);
    let reply = owner(
        json!({"op": "setMode", "shareId": share.id, "mode": "view"}),
        None,
    )
    .await;
    assert!(reply.result.is_some());
    assert_eq!(shares::get(&share.id).unwrap().mode, GuestMode::View);
    // From View only back to Ask me widens again.
    let reply = owner(
        json!({"op": "setMode", "shareId": share.id, "mode": "approve"}),
        None,
    )
    .await;
    assert_eq!(
        reply.needs_presence.unwrap().action,
        SensitiveAction::CollabChangeShare
    );
}

#[tokio::test]
async fn collab_has_no_switch_and_patches_cannot_weaken_it() {
    let _serial = serial();
    FakeHost::install(friendly);
    for patch in [
        json!({"enabled": false}),
        json!({"relayUrl": "https://evil.example"}),
        json!({"requireCode": false}),
        json!({"mode": "send"}),
        json!({"expiry": "never"}),
    ] {
        let reply = owner(json!({"op": "set", "patch": patch}), None).await;
        assert_eq!(reply.refused.unwrap().code, "badRequest");
    }
    let reply = owner(json!({"op": "set", "patch": {"expiry": "day"}}), None).await;
    assert_eq!(reply.result.unwrap()["config"]["expiry"], "day");
    let _ = crate::config::update_config(|config| config.expiry = crate::config::Expiry::Hour);
}

#[tokio::test]
async fn guests_write_as_their_share_allows() {
    let _serial = serial();
    let host = FakeHost::install(friendly);
    let share = shared().await;
    let viewer = shares::join(&share.id, "Sam", "dev1").ok().unwrap();
    let message =
        |text: &str| json!({"params": {"projectId": "p", "sessionId": "s", "text": text}});

    // Ask me: held, nothing delivered.
    let (status, answer) = crate::guest::answer(
        &share,
        &viewer,
        "/api/sendSessionChatMessage",
        &message("hi"),
    )
    .await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["result"]["collab"]["held"], true);
    assert!(!host
        .calls()
        .iter()
        .any(|call| matches!(call, HostCall::Deliver { .. })));
    let held_id = answer["result"]["collab"]["heldId"]
        .as_str()
        .unwrap()
        .to_string();

    // Another chat's ids, or a picture this guest didn't upload: refused.
    let other = json!({"params": {"projectId": "p", "sessionId": "other", "text": "x"}});
    assert_eq!(
        crate::guest::answer(&share, &viewer, "/api/sendSessionChatMessage", &other)
            .await
            .0,
        403
    );
    let picture = json!({"params": {"projectId": "p", "sessionId": "s", "text": "see [Image #1](/Users/me/secret.png)"}});
    assert_eq!(
        crate::guest::answer(&share, &viewer, "/api/sendSessionChatMessage", &picture)
            .await
            .0,
        403
    );

    // Sending the held message needs the owner's approval, then goes to the agent labelled.
    let resolve =
        json!({"op": "resolve", "shareId": share.id, "heldId": held_id, "action": "send"});
    let reply = owner(resolve.clone(), None).await;
    assert_eq!(
        reply.needs_presence.unwrap().action,
        SensitiveAction::CollabApproveGuestMessage
    );
    let reply = owner(
        resolve,
        grant(SensitiveAction::CollabApproveGuestMessage, &held_id),
    )
    .await;
    assert_eq!(reply.result.unwrap()["sent"], true);
    let delivered: Vec<String> = host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::Deliver {
                text, session_id, ..
            } if session_id == "s" => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        delivered,
        vec!["Sam (a guest in Ghostex Collab) says:\nhi".to_string()]
    );

    // View only: nothing written.
    shares::set_mode(&share.id, GuestMode::View);
    let (status, answer) = crate::guest::answer(
        &share,
        &viewer,
        "/api/sendSessionChatMessage",
        &message("again"),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(answer["error"], "collabViewOnly");
}

#[tokio::test]
async fn gxserver_learns_the_shares_and_each_chats_part() {
    let _serial = serial();
    let host = FakeHost::install(friendly);
    crate::sync::resync();
    crate::sync::drain();
    let share = shared().await;
    crate::sync::drain();
    let calls = host.calls();
    let scopes = calls.iter().rev().find_map(|call| match call {
        HostCall::SetShares { shares } => Some(shares.clone()),
        _ => None,
    });
    let scopes = scopes.expect("shares sent");
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].token, share.token);
    assert!(calls.iter().any(|call| matches!(call,
        HostCall::SetChatExtras { session_id, extras, .. } if session_id == "s" && extras["shared"] == true)));
    // Uninstalling ends every share, and gxserver is told.
    crate::serve::handle_event(ghostex_gx_protocol::collab_component::SidecarEvent::EndAll);
    let calls = host.calls();
    assert!(
        matches!(calls.iter().rev().find(|call| matches!(call, HostCall::SetShares { .. })),
        Some(HostCall::SetShares { shares }) if shares.is_empty())
    );
    assert!(
        matches!(calls.last(), Some(HostCall::SetChatExtras { extras, .. }) if extras["shared"] == false)
    );
}

#[tokio::test]
async fn a_link_that_lasts_until_stopped_says_so_and_still_needs_approval() {
    let _serial = serial();
    FakeHost::install(friendly);
    let reply = owner(
        json!({"op": "set", "patch": {"expiry": "untilStopped"}}),
        None,
    )
    .await;
    assert_eq!(reply.result.unwrap()["config"]["expiry"], "untilStopped");
    let call = json!({"op": "share", "projectId": "p", "sessionId": "s"});
    let need = owner(call.clone(), None)
        .await
        .needs_presence
        .expect("asks");
    assert_eq!(need.action, SensitiveAction::CollabShareChat);
    assert!(
        need.detail.contains("until you stop sharing"),
        "{}",
        need.detail
    );
    assert!(shares::list().is_empty());
    let reply = owner(call.clone(), grant(SensitiveAction::CollabShareChat, "s")).await;
    let share = &reply.result.unwrap()["share"];
    assert!(share["expiresAtMs"].is_null(), "{share}");
    // Showing the link again later says it lasts until stopped, too.
    let need = owner(call, None).await.needs_presence.expect("asks again");
    assert!(
        need.detail.contains("until you stop sharing"),
        "{}",
        need.detail
    );
    let _ = crate::config::update_config(|config| config.expiry = crate::config::Expiry::Hour);
    shares::stop_all();
}
