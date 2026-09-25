//! Pin, unpin, park, unpark and tag: one call, one shared planner, and an optimistic value that
//! is applied only after the daemon has taken it.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! Four sidebar commands are the same call with different fields (`setSessionPinned`,
//! `setSessionParked`, `setSessionTag`, `setSessionFavorite`), so they are one planner rather than
//! four: `/api/updateSession` with whichever flags the payload names. Two details are easy to lose
//! and both are ported here rather than rediscovered:
//!
//! - Setting a TAG also sets `isFavorite`, derived from whether the tag is `favorite`. The
//!   TypeScript writes both in one call and a port that sent only the tag would leave the star
//!   and the tag disagreeing until the daemon's own row arrived.
//! - `/api/updateSession` clears a tag with an explicit `null`, and a presentation row models "no
//!   tag" as an ABSENT field. The optimistic patch therefore writes absence, not null, so it
//!   predicts the shape the daemon will send back.
//!
//! **No tag catalog is read here, by either side.** The three catalogs M4d separated (this
//! computer's for which filters exist and for pruning, every machine's merged for a label, the
//! row's own machine's for its submenu) are all inputs to BUILDING the menu, which M4c already
//! owns. The action carries the tag id the chosen row was built with and passes it through
//! untouched, so there is no catalog choice left to get wrong at this point.
//!
//! **Nothing local happens before the call.** Checked rather than assumed: the four commands above
//! and `setSessionParked` are every caller, and the only local-first session writes in the
//! codebase (`setSessionSleepingLocally`, `hideSessionLocally`, `hideSessionsLocally`) are the
//! React sidebar's own and are never reached from the native path. A call that fails leaves the
//! row exactly as the daemon has it, and shows nothing.
//!
//! Ported from `updateSessionFlags` and `setSessionParked` in the deleted
//! `gxserver-runtime/sessions-and-focus.ts` (see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_flags.rs.

use serde_json::{json, Map, Value};

use crate::keys::SessionKey;
use crate::overlay::SessionPatch;
use crate::selectors::QUICK_AUTOMATIONS_PROJECT_ID;

use super::lifecycle::LIFECYCLE_PATCH_TTL_MS;
use super::resolve::text_field;

/// The flags one payload names. A field left `None` is one the call does not mention, which the
/// daemon then leaves alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFlags {
    pub is_pinned: Option<bool>,
    pub is_parked: Option<bool>,
    pub is_favorite: Option<bool>,
    /// `Some(None)` is the explicit clear.
    pub session_tag: Option<Option<String>>,
}

impl SessionFlags {
    /// The call's body, in the order the TypeScript spreads it: the flags, then the ids. The same
    /// body on either machine: a remote leg spreads the same flags over that machine's raw ids.
    pub(super) fn to_params(&self, session: &SessionKey) -> Value {
        let mut params = Map::new();
        if let Some(is_favorite) = self.is_favorite {
            params.insert("isFavorite".to_string(), Value::Bool(is_favorite));
        }
        if let Some(is_parked) = self.is_parked {
            params.insert("isParked".to_string(), Value::Bool(is_parked));
        }
        if let Some(is_pinned) = self.is_pinned {
            params.insert("isPinned".to_string(), Value::Bool(is_pinned));
        }
        if let Some(session_tag) = &self.session_tag {
            params.insert(
                "sessionTag".to_string(),
                session_tag
                    .as_ref()
                    .map_or(Value::Null, |tag| Value::String(tag.clone())),
            );
        }
        params.insert(
            "projectId".to_string(),
            Value::String(session.project_id.clone()),
        );
        params.insert(
            "sessionId".to_string(),
            Value::String(session.session_id.clone()),
        );
        Value::Object(params)
    }

    /// The optimistic patch these flags predict, once the daemon has taken them.
    fn to_patch(&self, now_ms: u64) -> SessionPatch {
        let mut patch = SessionPatch::empty(now_ms.saturating_add(LIFECYCLE_PATCH_TTL_MS));
        if let Some(is_pinned) = self.is_pinned {
            patch = patch.with_pinned(is_pinned);
        }
        if let Some(is_parked) = self.is_parked {
            patch = patch.with_parked(is_parked);
        }
        if let Some(is_favorite) = self.is_favorite {
            patch = patch.with_favorite(is_favorite);
        }
        if let Some(session_tag) = &self.session_tag {
            patch = patch.with_session_tag(session_tag.clone());
        }
        patch
    }
}

/// What the host must call, and what follows a sleep when parking is configured to sleep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagsRequest {
    pub session: SessionKey,
    pub rpc_path: &'static str,
    pub rpc_params: Value,
    pub flags: SessionFlags,
    /// Parking also sleeps when `sleepSessionWhenParking` is on. The sleep runs after the flag
    /// call has come back, through the lifecycle path that already owns it.
    pub then_sleep: bool,
}

impl FlagsRequest {
    pub fn to_json(&self) -> Value {
        json!({
            "rpc": { "path": self.rpc_path, "params": self.rpc_params },
            "thenSleep": self.then_sleep,
        })
    }
}

/// What the host does once the call has come back. Empty when it did not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlagsFollowUp {
    Patch {
        session: SessionKey,
        patch: SessionPatch,
    },
    /// Run the ordinary sleep, which has its own call, its own answer and its own guard.
    Sleep { session: SessionKey },
}

impl FlagsFollowUp {
    pub fn to_json(&self) -> Value {
        match self {
            Self::Patch { session, patch } => json!({
                "follow": "patch",
                "session": session.to_sidebar_session_id(),
                "isPinned": patch.is_pinned,
                "isParked": patch.is_parked,
                "isFavorite": patch.is_favorite,
                // Three states on the wire too: absent, a tag, or an explicit clear.
                "sessionTag": patch.session_tag.as_ref().map(|tag| {
                    tag.as_ref().map_or(Value::Null, |tag| Value::String(tag.clone()))
                }),
            }),
            Self::Sleep { session } => json!({
                "follow": "sleep",
                "session": session.to_sidebar_session_id(),
            }),
        }
    }
}

/// The four flag payloads, or `None` when this file does not own one.
///
/// `sleep_session_when_parking` is the setting the host reads; it only matters for a park.
///
/// Refused, with the reason at each refusal: a browser row is an app tab with no daemon session
/// behind it, and the Quick Automations row has no daemon session either. A REMOTE row is answered
/// by `remote.rs` from the same flags (`session_flags_of`). A row the store does not hold is NOT
/// refused, because the TypeScript does not check: it parses the id and calls, and a port that
/// checked would silently do nothing where the shipped code still asks.
pub fn plan_flags_request(
    message: &Value,
    sleep_session_when_parking: bool,
) -> Option<FlagsRequest> {
    let sidebar_session_id = text_field(message, "sessionId")?;
    if sidebar_session_id.starts_with("gpui-browser:") {
        return None;
    }
    let session = SessionKey::parse_sidebar_session_id(sidebar_session_id)?;
    if !session.machine.is_local() || session.project_id == QUICK_AUTOMATIONS_PROJECT_ID {
        return None;
    }
    let (flags, then_sleep) = session_flags_of(message, sleep_session_when_parking)?;
    Some(FlagsRequest {
        rpc_path: "/api/updateSession",
        rpc_params: flags.to_params(&session),
        flags,
        then_sleep,
        session,
    })
}

/// The flags one of the four payloads names, and whether a park also sleeps. One parse for both
/// machines, so a remote pin and a local pin cannot read `pinned` two ways.
pub(super) fn session_flags_of(
    message: &Value,
    sleep_session_when_parking: bool,
) -> Option<(SessionFlags, bool)> {
    let mut then_sleep = false;
    let flags = match text_field(message, "type")? {
        "setSessionPinned" => SessionFlags {
            is_pinned: Some(message.get("pinned")?.as_bool()?),
            ..SessionFlags::default()
        },
        "setSessionParked" => {
            let parked = message.get("parked")?.as_bool()?;
            then_sleep = parked && sleep_session_when_parking;
            SessionFlags {
                is_parked: Some(parked),
                ..SessionFlags::default()
            }
        }
        // A tag and the star are one call: `isFavorite` is whether the tag IS `favorite`.
        "setSessionTag" => {
            let tag = message.get("sessionTag").and_then(|tag| match tag {
                Value::Null => None,
                other => other.as_str().map(str::to_string),
            });
            SessionFlags {
                is_favorite: Some(tag.as_deref() == Some("favorite")),
                session_tag: Some(tag),
                ..SessionFlags::default()
            }
        }
        // The same pair from the other direction: starring sets the `favorite` tag, unstarring
        // clears whatever tag was there.
        "setSessionFavorite" => {
            let favorite = message.get("favorite")?.as_bool()?;
            SessionFlags {
                is_favorite: Some(favorite),
                session_tag: Some(favorite.then(|| "favorite".to_string())),
                ..SessionFlags::default()
            }
        }
        _ => return None,
    };
    Some((flags, then_sleep))
}

/// What to do with the answer. A call that did not come back changes nothing at all, which is what
/// `await this.updateSessionFlags(...)` does when its promise rejects: the patch below never runs
/// and no toast is shown.
pub fn apply_flags_answer(
    request: &FlagsRequest,
    accepted: bool,
    now_ms: u64,
) -> Vec<FlagsFollowUp> {
    if !accepted {
        return Vec::new();
    }
    let mut follow_ups = vec![FlagsFollowUp::Patch {
        session: request.session.clone(),
        patch: request.flags.to_patch(now_ms),
    }];
    if request.then_sleep {
        follow_ups.push(FlagsFollowUp::Sleep {
            session: request.session.clone(),
        });
    }
    follow_ups
}

/// Whether this payload is one this file answers.
pub fn owns_flags_message(message: &Value) -> bool {
    matches!(
        text_field(message, "type"),
        Some("setSessionPinned" | "setSessionParked" | "setSessionTag" | "setSessionFavorite")
    )
}

/// Every message type this file answers, for a host that wants to count a decline by name.
pub const FLAGS_MESSAGE_TYPES: [&str; 4] = [
    "setSessionPinned",
    "setSessionParked",
    "setSessionTag",
    "setSessionFavorite",
];
