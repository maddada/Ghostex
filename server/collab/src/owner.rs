//! The owner's calls (`/api/collab`, from Settings › Collab, the chat's Share popover and the
//! phone), forwarded by gxserver:
//! - `op: "read"` (default): the choices, the relay, the guest page check (`guestPage`) and every
//!   share with its mode, expiry and viewers. Never a link or a code.
//! - `op: "set"`: `patch` holds the choices to change (`expiry` hour/fourHours/day, `mode`
//!   approve/view, `ownerName`). `enabled`, `relayUrl`, `requireCode: false` and a Send default
//!   are refused (Collab is on while installed).
//! - `op: "share"`: `projectId`, `sessionId`; the chat's share, made with the choices if it has
//!   none, with its `link` and `code`. Needs the owner's signed approval every time (it is also
//!   how the owner sees an existing share's link again), and a relay serving the pinned page.
//! - `op: "stop"` (`shareId`), `op: "stopAll"`, `op: "removeViewer"` (`shareId`, `viewerId`):
//!   always allowed. `op: "setMode"` (`shareId`, `mode`): to Send, or from View only to Ask me,
//!   needs the owner's signed approval; narrowing doesn't.
//! - `op: "resolve"`: `projectId`, `sessionId`, `heldId`, `action` (`send`, `edit` or `decline`) and
//!   for `send` an optional edited `text`: the owner's answer to a guest message held for approval.
//!   `send` needs the owner's signed approval.
//! - `op: "audit"`: the newest log entries.
//! - `op: "typing"`: the owner typing in a shared chat (`projectId`, `sessionId`, `typing`).
//!
//! A change that needs approval answers [`OwnerReply::needs_presence`] with the card's words;
//! gxserver asks the owner (GitHub #495, `ai/security-tokens-and-approvals.md`) and repeats the
//! same call with the grant once they approved. The component never makes such a change without
//! a grant for exactly that action and target.

use ghostex_gx_protocol::approval::SensitiveAction;
use ghostex_gx_protocol::collab_component::{
    HostCall, OwnerCall, OwnerReply, Performed, PresenceNeed,
};
use serde_json::{json, Map, Value};

use crate::config::{self, GuestMode};
use crate::host::{self, share_title};
use crate::shares::{self, StoredShare};

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn quoted(title: &str) -> String {
    format!("\u{201c}{title}\u{201d}")
}

fn ends_in_words(ms: i64) -> String {
    match ms / 3_600_000 {
        0 | 1 => "an hour".to_string(),
        24 => "a day".to_string(),
        hours => format!("{hours} hours"),
    }
}

/// Why a call stopped early: refused, or waiting for the owner's approval.
enum Stop {
    Refused(&'static str, String),
    Presence(PresenceNeed),
}

impl From<(&'static str, String)> for Stop {
    fn from((code, message): (&'static str, String)) -> Self {
        Self::Refused(code, message)
    }
}

/// One call's run: the grant gxserver repeated it with, and the approved changes it made.
struct Run<'a> {
    call: &'a OwnerCall,
    performed: Vec<Performed>,
}

impl Run<'_> {
    /// The approval id when gxserver repeated this call with a grant for exactly `action` and
    /// `target`; otherwise the call stops and asks for one.
    fn gate(
        &self,
        action: SensitiveAction,
        target: &str,
        summary: String,
        detail: String,
    ) -> Result<String, Stop> {
        match &self.call.grant {
            Some(grant) if grant.action == action && grant.target == target => {
                Ok(grant.approval_id.clone())
            }
            _ => Err(Stop::Presence(PresenceNeed {
                action,
                target: target.to_string(),
                summary,
                detail,
            })),
        }
    }

    fn performed(&mut self, approval_id: &str, ok: bool) {
        self.performed.push(Performed {
            approval_id: approval_id.to_string(),
            ok,
        });
    }
}

/// The answer to one forwarded owner call.
pub async fn handle(call: OwnerCall) -> OwnerReply {
    let mut run = Run {
        call: &call,
        performed: Vec::new(),
    };
    let answer = handle_op(&mut run, &call.params).await;
    let mut reply = match answer {
        Ok(result) => OwnerReply::ok(result),
        Err(Stop::Refused(code, message)) => OwnerReply::refused(code, message),
        Err(Stop::Presence(need)) => OwnerReply::needs_presence(need),
    };
    reply.performed = run.performed;
    reply
}

fn chat_changed(share: &StoredShare) {
    crate::sync::chat_changed(&share.project_id, &share.session_id);
}

pub fn read_all() -> Value {
    let config = config::read_config();
    let relay = config::relay_url();
    let list: Vec<Value> = shares::list()
        .iter()
        .map(|share| shares::owner_view(share, &relay, Some(share_title(share))))
        .collect();
    json!({
        "installed": true,
        "config": {
            // Collab is on while it is installed (the clients read this).
            "enabled": true,
            "expiry": config.expiry.as_str(),
            "requireCode": true,
            "mode": config.mode.as_str(),
            "ownerName": config.owner_name,
            "ownerDisplayName": config::owner_display_name(&config),
        },
        "relay": relay,
        "relayConnected": crate::connector::relay_connected(),
        "guestPage": crate::page_pin::status_json(),
        "shares": list,
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn share_of(params: &Map<String, Value>) -> Result<StoredShare, Stop> {
    if let Some(id) = text(params, "shareId") {
        return shares::get(&id)
            .ok_or_else(|| ("notFound", "This chat is no longer shared.".to_string()).into());
    }
    match (text(params, "projectId"), text(params, "sessionId")) {
        (Some(project_id), Some(session_id)) => shares::for_session(&project_id, &session_id)
            .ok_or_else(|| ("notFound", "This chat is no longer shared.".to_string()).into()),
        _ => Err((
            "badRequest",
            "shareId, or projectId and sessionId, is required.".to_string(),
        )
            .into()),
    }
}

/// Sends a guest's (labelled) words to the shared chat's agent through gxserver.
pub async fn deliver(
    share: &StoredShare,
    text: &str,
    image_paths: &[String],
) -> Result<(), String> {
    host::call_async(HostCall::Deliver {
        project_id: share.project_id.clone(),
        session_id: share.session_id.clone(),
        text: text.to_string(),
        image_paths: image_paths.to_vec(),
    })
    .await
    .map(|_| ())
}

async fn handle_op(run: &mut Run<'_>, params: &Map<String, Value>) -> Result<Value, Stop> {
    let op = text(params, "op").unwrap_or_else(|| "read".to_string());
    match op.as_str() {
        "read" => {}
        "set" => {
            let patch = params.get("patch").cloned().unwrap_or(Value::Null);
            let mut next = config::read_config();
            config::apply_patch(&mut next, &patch).map_err(|message| ("badRequest", message))?;
            config::update_config(|config| {
                let _ = config::apply_patch(config, &patch);
            })
            .map_err(|error| {
                Stop::Refused(
                    "internalError",
                    format!("Ghostex couldn't save Ghostex Collab's settings: {error}"),
                )
            })?;
            for share in shares::list() {
                chat_changed(&share);
            }
            crate::notify_changed();
        }
        "share" => {
            let config = config::read_config();
            let (Some(project_id), Some(session_id)) =
                (text(params, "projectId"), text(params, "sessionId"))
            else {
                return Err((
                    "badRequest",
                    "projectId and sessionId are required.".to_string(),
                )
                    .into());
            };
            let info = host::chat_info(&project_id, &session_id);
            if info.exists != Some(true) {
                return Err(("notFound", "That chat no longer exists.".to_string()).into());
            }
            // Checked before the approval is asked for, so a paused relay costs no touch.
            let relay = config::relay_url();
            if !crate::page_pin::ensure_verified(&relay).await {
                let message = crate::page_pin::status_json()["message"]
                    .as_str()
                    .unwrap_or("The share relay's guest page couldn't be checked.")
                    .to_string();
                return Err(("guestPageUnverified", message).into());
            }
            let title = info.title.unwrap_or_else(|| "A chat".to_string());
            let existing = shares::for_session(&project_id, &session_id);
            // CDXC:Collab 2026-11-08 DECISION:
            // User (#495): reading a share's link and code goes through the gate too, so it is the same approval as sharing: no read returns them, only the answer to an approved `share`.
            let (summary, detail) = match &existing {
                Some(share) => (
                    format!("Show the link and code for {}", quoted(&title)),
                    match share.expires_at_ms {
                        Some(at) => format!(
                            "Whoever gets them can join the shared chat until {}.",
                            chrono::DateTime::from_timestamp_millis(at)
                                .map(|at| at
                                    .with_timezone(&chrono::Local)
                                    .format("%H:%M")
                                    .to_string())
                                .unwrap_or_else(|| "it ends".to_string())
                        ),
                        None => {
                            "Whoever gets them can join the shared chat until you stop sharing."
                                .to_string()
                        }
                    },
                ),
                None => (
                    format!("Share {} through a link", quoted(&title)),
                    format!(
                        "Anyone with the link and its code can follow this chat {}{}.",
                        match config.expiry.millis() {
                            Some(ms) => format!("for {}", ends_in_words(ms)),
                            // CDXC:Collab 2026-11-08 DECISION: User (Q1785) "until i stop it": the approval card says it plainly.
                            None => "until you stop sharing: the link doesn't run out".to_string(),
                        },
                        if config.mode == GuestMode::View {
                            ", without writing"
                        } else {
                            " and write messages that wait for your approval"
                        }
                    ),
                ),
            };
            let approval = run.gate(
                SensitiveAction::CollabShareChat,
                &session_id,
                summary,
                detail,
            )?;
            let share = shares::share_session(&config, &project_id, &session_id);
            run.performed(&approval, true);
            chat_changed(&share);
            crate::notify_changed();
            let mut view = shares::owner_view(&share, &relay, Some(title));
            if let (Some(view), Value::Object(secrets)) =
                (view.as_object_mut(), shares::owner_secrets(&share, &relay))
            {
                view.extend(secrets);
            }
            return Ok(json!({ "share": view }));
        }
        "stop" => {
            let share = share_of(params)?;
            shares::stop(&share.id);
            chat_changed(&share);
            crate::notify_changed();
        }
        "stopAll" => {
            for share in shares::stop_all() {
                chat_changed(&share);
            }
            crate::notify_changed();
        }
        "setMode" => {
            let share = share_of(params)?;
            let mode = text(params, "mode")
                .as_deref()
                .and_then(GuestMode::parse)
                .ok_or(("badRequest", "mode is send, approve or view.".to_string()))?;
            let title = share_title(&share);
            // Narrowing (to Ask me, or View only) is always allowed; letting guests write more
            // needs the approval, Send most of all.
            let approval = match (share.mode, mode) {
                (from, to) if from == to => None,
                (_, GuestMode::Send) => Some(run.gate(
                    SensitiveAction::CollabEnableSendMode,
                    &share.session_id,
                    format!(
                        "Let guests of {} send messages straight to its agent",
                        quoted(&title)
                    ),
                    "Their messages reach the agent without your approval, and the agent may act on them with your tools and logins.".to_string(),
                )?),
                (GuestMode::View, GuestMode::Approve) => Some(run.gate(
                    SensitiveAction::CollabChangeShare,
                    &share.session_id,
                    format!("Let guests of {} write messages", quoted(&title)),
                    "Each message waits for your approval before it reaches the agent."
                        .to_string(),
                )?),
                _ => None,
            };
            shares::set_mode(&share.id, mode);
            if let Some(approval) = &approval {
                run.performed(approval, true);
            }
            chat_changed(&share);
        }
        "removeViewer" => {
            let share = share_of(params)?;
            let viewer = text(params, "viewerId").unwrap_or_default();
            // CDXC:Collab 2026-11-07 DECISION: the coordinator, for the user: "a block list so a removed guest can't rejoin": Remove keeps that guest's browser out of the share for good (`block: false` lets them back).
            let block = params.get("block").and_then(Value::as_bool).unwrap_or(true);
            if let Some(removed) = shares::remove_viewer(&share.id, &viewer, block) {
                crate::audit::record(
                    &share,
                    &share_title(&share),
                    "removed",
                    &removed.name,
                    None,
                    0,
                    None,
                );
            }
            chat_changed(&share);
            crate::notify_changed();
        }
        "audit" => {
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(100)
                .min(1_000) as usize;
            return Ok(json!({ "entries": crate::audit::read_recent(limit) }));
        }
        "typing" => {
            if let (Some(project_id), Some(session_id)) =
                (text(params, "projectId"), text(params, "sessionId"))
            {
                let typing = params
                    .get("typing")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                crate::guest::owner_typing(&project_id, &session_id, typing);
            }
            return Ok(json!({}));
        }
        "resolve" => {
            let share = share_of(params)?;
            let held_id = text(params, "heldId").unwrap_or_default();
            let action = text(params, "action").unwrap_or_default();
            let outcome = match action.as_str() {
                "send" => "sent",
                "edit" => "edited",
                _ => "declined",
            };
            let title = share_title(&share);
            // CDXC:Collab 2026-11-08 DECISION:
            // User (#495): approving a guest's message is a sensitive action ("approve guest messages" needs the biometric/PIN approval), so an agent holding a token can't wave a stranger's words through. Edit and Decline never send anything and stay free.
            let approval = if action == "send" {
                let held = shares::peek_held(&share.id, &held_id)
                    .ok_or(("notFound", "That message was already answered.".to_string()))?;
                let words = text(params, "text").unwrap_or_else(|| held.text.clone());
                let (shown, referenced) = crate::split_image_references(&words);
                let shown: String = shown.chars().take(600).collect();
                let pictures = held.image_paths.len() + referenced.len();
                Some(run.gate(
                    SensitiveAction::CollabApproveGuestMessage,
                    &held_id,
                    format!("Send {}\u{2019}s message to {}", held.name, quoted(&title)),
                    format!(
                        "{}{}{}",
                        if words == held.text {
                            ""
                        } else {
                            "Edited before sending. "
                        },
                        quoted(&shown),
                        match pictures {
                            0 => String::new(),
                            1 => " With 1 picture.".to_string(),
                            count => format!(" With {count} pictures."),
                        }
                    ),
                )?)
            } else {
                None
            };
            let held = shares::take_held(&share.id, &held_id, outcome)
                .ok_or(("notFound", "That message was already answered.".to_string()))?;
            chat_changed(&share);
            let audit = |outcome: &str| {
                crate::audit::record(
                    &share,
                    &title,
                    "message",
                    &held.name,
                    Some(&held.text),
                    held.image_paths.len(),
                    Some(outcome),
                );
            };
            return match action.as_str() {
                "send" => {
                    let words = text(params, "text").unwrap_or_else(|| held.text.clone());
                    let sent = deliver(
                        &share,
                        &crate::labelled_guest_text(&held.name, &words),
                        &held.image_paths,
                    )
                    .await;
                    if let Some(approval) = &approval {
                        run.performed(approval, sent.is_ok());
                    }
                    sent.map_err(|message| Stop::Refused("sendFailed", message))?;
                    audit("approved");
                    Ok(json!({ "sent": true }))
                }
                "edit" => {
                    audit("edited");
                    Ok(
                        json!({ "composerText": crate::labelled_guest_text(&held.name, &held.text) }),
                    )
                }
                _ => {
                    audit("declined");
                    Ok(json!({ "declined": true }))
                }
            };
        }
        other => return Err(("badRequest", format!("Unknown op {other}.")).into()),
    }
    Ok(read_all())
}

/// A deleted chat's share ends with it: its link is dead and its guests are dropped.
pub fn end_share_of_deleted_chat(project_id: &str, session_id: &str) {
    if let Some(share) = shares::stop_for_session(project_id, session_id) {
        crate::audit::record(&share, "A deleted chat", "ended", "", None, 0, None);
        chat_changed(&share);
        crate::notify_changed();
    }
}

/// Ends the shares whose chat is gone from gxserver (deleted while the component was stopped,
/// or by a path that didn't end its share). `true` when one ended.
pub fn end_shares_of_missing_chats() -> bool {
    let mut ended = false;
    for share in shares::list() {
        if host::chat_info(&share.project_id, &share.session_id).exists == Some(false) {
            end_share_of_deleted_chat(&share.project_id, &share.session_id);
            ended = true;
        }
    }
    ended
}
