//! Where each `Effect` goes: this thread, or the renderer's own dispatch.
//!
//! The core returns `Vec<Effect>` from every `handle`. Half of them are I/O this host performs
//! itself (storage, timers, and the chat socket in `transport.rs`); the other half are things only
//! the view can do, because it owns the composer field, the clipboard, the window and the app
//! shell. Those ride back in the frame's `requests` array in the exact wire form
//! `apps/desktop/src/app/native_chat/state.rs` already dispatches, so no drawing or dispatch code
//! changes when the brain does.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! `Effect::SendRpc` is forwarded as a `rpc` request rather than called from this thread: the view
//! already performs every chat RPC (`native_chat/rpc.rs`, with the remote machine's target and the
//! web build's `fetch`), and an answer is `resolve`d back by the id the core allocated.

use ghostex_gx_chat_core::{Effect, HostRequest, OpenTarget, RequestKind, UserAction};
use serde_json::{Map, Value};

/// Who performs one effect.
pub(super) enum Routed {
    /// This thread: storage, the boot read, the timer.
    Host(Effect),
    /// The view, through the frame's `requests` array.
    Renderer(Box<HostRequest>),
    /// The core itself, as a gesture it asked to have replayed at it.
    SelfAction(Box<UserAction>),
    /// Nothing, on purpose, because the QuickJS brain performed nothing either. Named so the
    /// deliberate no-op is visible in the counters rather than looking like a lost effect.
    Swallowed(&'static str),
}

/// The host actions this host deliberately performs nothing for, because the QuickJS brain did
/// not either.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// The spec for the parity window was what the app did then, not what it ought to do, and the
/// QuickJS brain SWALLOWED `suggestionSend`. It is the slash picker saying "the draft is already
/// the whole command, send it instead of completing it", and `native-host.ts`'s `suggestionKey` arm
/// kept only a completion that carried `content`, so `{send: true}` fell out of the switch and
/// nothing at all was pushed. The real send happens one step EARLIER, in the view: `keyboard.rs`
/// reads `suggestions.sendOnEnter` off the snapshot and calls `send` itself rather than dispatching
/// `suggestionKey`, and that flag is the same rule the core's inner test is
/// (`composer/suggestions.rs`), so the effect is reached only when the view's snapshot is a turn
/// stale. This host therefore swallows it too, explicitly: forwarding it to the app shell (which is
/// what this file once did) is a request the QuickJS brain never sent, and performing the send
/// here is impossible anyway, because a send needs the composer field, the draft id and the draft
/// revision, all of which are the view's. The name is a code constant, never a user's data.
pub(super) const SWALLOWED_HOST_ACTIONS: &[&str] = &["suggestionSend"];

/// Sorts one effect into its performer.
///
/// Exhaustive on purpose. An effect a later family adds and nobody routes would otherwise vanish
/// without a trace, which is the failure mode `SEAM.md` section 5 records for `actionComplete`.
pub(super) fn route(effect: Effect) -> Routed {
    match effect {
        Effect::ReadStorage { .. }
        | Effect::ReadStorageBatch { .. }
        | Effect::WriteStorage { .. }
        | Effect::WriteStorageBatch { .. }
        | Effect::ReadComposerBoot { .. }
        | Effect::FlushStorage { .. }
        | Effect::ReadRetainedSnapshot
        | Effect::WriteRetainedSnapshot { .. }
        | Effect::SetTimer { .. }
        | Effect::Subscribe { .. }
        | Effect::Unsubscribe
        | Effect::Reconnect => Routed::Host(effect),
        // `{kind: 'broker', method: 'presentation', params: {state}}`, which is what
        // `native-host.ts:676` pushed and what each app's chat binding reads
        // (`message["method"] == "presentation"` takes `params.state`). It goes to the renderer
        // rather than being performed here because the cache it feeds is the APP's (the next view
        // of the session opens from it); this host has no door onto it.
        Effect::UpdatePresentation { state } => {
            let mut params = Map::new();
            params.insert("state".into(), *state);
            Routed::Renderer(Box::new(broker("presentation", params)))
        }
        Effect::SendRpc {
            request_id,
            method,
            params,
        } => Routed::Renderer(Box::new(HostRequest {
            id: Some(request_id),
            kind: RequestKind::Rpc,
            method: method.as_str().to_string(),
            params: object(*params),
        })),
        Effect::SetComposerText {
            content,
            caret,
            from_history,
        } => {
            let mut params = Map::new();
            params.insert("content".into(), Value::String(content));
            // `caret` is a UTF-16 offset, which is what a JavaScript string index is. The view
            // converts it against its own field (`ensure_input`), so it crosses unchanged.
            params.insert(
                "caret".into(),
                caret.map(Value::from).unwrap_or(Value::Null),
            );
            Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::Composer,
                // The view reads `request["method"] == "history"` to decide whether the replacement
                // is undoable, so the spelling is load bearing.
                method: if from_history { "history" } else { "insert" }.to_string(),
                params,
            }))
        }
        Effect::ClearComposerIfUnchanged { text } => {
            let mut params = Map::new();
            params.insert("text".into(), Value::String(text));
            Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::ComposerClearExpected,
                method: String::new(),
                params,
            }))
        }
        Effect::Open(OpenTarget::Url { url }) => {
            let mut params = Map::new();
            params.insert("url".into(), Value::String(url));
            Routed::Renderer(Box::new(dispatched("openLink", params)))
        }
        Effect::Open(OpenTarget::File { path, line, column }) => {
            let mut params = Map::new();
            params.insert("path".into(), Value::String(path));
            if let Some(line) = line {
                params.insert("line".into(), Value::from(line));
            }
            if let Some(column) = column {
                params.insert("column".into(), Value::from(column));
            }
            Routed::Renderer(Box::new(dispatched("openFile", params)))
        }
        // The QuickJS brain never pushed either kind: a clipboard write only ever reached the view
        // inside `markdownSaved`, and a toast is a raw `{type: "toast"}` app message rather than a
        // `sessionChatHostAction`. Both go out as their own dispatch arm rather than as a `host`
        // method the app shell would silently drop, so the first family to emit one is drawn
        // instead of counted.
        Effect::Copy { text } => {
            let mut params = Map::new();
            params.insert("text".into(), Value::String(text));
            Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::Other("copy".to_string()),
                method: String::new(),
                params,
            }))
        }
        Effect::Toast { level, message } => {
            let mut params = Map::new();
            params.insert("level".into(), Value::String(level));
            params.insert("message".into(), Value::String(message));
            Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::Other("toast".to_string()),
                method: String::new(),
                params,
            }))
        }
        // `{kind: 'returnedPrompt', method: 'restore', params: {text}}`, which is what
        // `native-host.ts:1179` pushed for `restoreReturned`. The view compares it against its own
        // composer text before it applies it, so the text crosses and the decision stays the
        // view's. Before this arm the effect fell through to the wildcard and was counted as
        // `effectsUnrouted`: a prompt the agent handed back reached no composer.
        Effect::RestoreReturnedPrompt { text } => {
            let mut params = Map::new();
            params.insert("text".into(), Value::String(text));
            Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::ReturnedPrompt,
                method: "restore".to_string(),
                params,
            }))
        }
        Effect::MarkdownSaved { path } => {
            let mut params = Map::new();
            params.insert("path".into(), Value::String(path));
            Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::MarkdownSaved,
                method: String::new(),
                params,
            }))
        }
        Effect::HostAction { action, params } => host_action(action, *params),
        // `Effect` is `#[non_exhaustive]`: a core newer than this host is a missing arm, not a
        // crash. Every variant this build knows is spelled out above, so an arm can only be missing
        // when the crate grows one, and `gxChat.host.summary` counts it as `effectsUnrouted`.
        _ => Routed::Renderer(Box::new(HostRequest {
            id: None,
            kind: RequestKind::Other(UNROUTED.to_string()),
            method: String::new(),
            params: Map::new(),
        })),
    }
}

/// The dispatch kind an effect this build does not know rides under.
///
/// The view has no arm for it, so it draws nothing; the counter is how it is noticed.
pub(super) const UNROUTED: &str = "unrouted";

/// Where one [`Effect::HostAction`] goes: nowhere, back into the core, or to the app shell.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `switchToTerminal` is forwarded from here unchanged, and the app shell drops it. That is
/// deliberate: `native-host.ts` pushed the identical `{kind: 'host', method: 'switchToTerminal'}`
/// and `receive_session_chat_host_action` has no arm for that spelling (it knows `terminalView`), so
/// the QuickJS brain did exactly the same nothing. The missing arm is an app-shell gap that predates
/// this port; fixing it belongs in `session_chat.rs`, not in host routing. Forwarding means that on
/// the day the shell grows the arm, the chat starts switching with no change here.
fn host_action(action: String, params: Value) -> Routed {
    if let Some(name) = SWALLOWED_HOST_ACTIONS
        .iter()
        .find(|known| **known == action)
    {
        return Routed::Swallowed(name);
    }
    // `selectOption` is a gesture the core asked to have replayed at itself: its params are already
    // a `UserAction`, `"type"` and all, because `native-host.ts` called its own `action` switch here
    // rather than pushing a request. Feeding it back is what makes a model menu's option pick land;
    // forwarding it to the app shell would drop it.
    if action == "selectOption" {
        return match serde_json::from_value::<UserAction>(params) {
            Ok(action) => Routed::SelfAction(Box::new(action)),
            Err(_) => Routed::Renderer(Box::new(HostRequest {
                id: None,
                kind: RequestKind::Other(UNROUTED.to_string()),
                method: String::new(),
                params: Map::new(),
            })),
        };
    }
    Routed::Renderer(Box::new(dispatched(&action, object(params))))
}

/// `{kind: "broker", method, params}`: what the view hands its app, which is the presentation cache.
fn broker(method: &str, params: Map<String, Value>) -> HostRequest {
    HostRequest {
        id: None,
        kind: RequestKind::Broker,
        method: method.to_string(),
        params,
    }
}

/// The request one [`Effect::HostAction`] rides in.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// Five of the core's host actions are NOT `sessionChatHostAction`s. The QuickJS brain pushed each
/// under a dispatch kind of its own, because the view has to read its own composer field, its own
/// selection, its own draft revision or its own image cache before it can perform them
/// (`native-host.ts` lines 1130, 1216, 1247, 1296, 1498). Routed through `host` they would reach
/// `receive_session_chat_host_action`, which has no arm for any of them and drops them: a send
/// would clear no field, a failed send would restore no text, and a draft arriving from another
/// client would reach no composer. The method and the parameter shape are the view's, not the
/// core's, so both are built here.
fn dispatched(action: &str, mut params: Map<String, Value>) -> HostRequest {
    let method = |params: &Map<String, Value>, fallback: &str| {
        params
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    };
    match action {
        // `insertAttachments` reads `paths` and the view supplies the selection.
        "attachmentReferences" => HostRequest {
            id: None,
            kind: RequestKind::AttachmentReferences,
            method: "insert".to_string(),
            params,
        },
        // `receive_chat_image` reads `method == "loaded"` and then `base64Data` and `mediaType`
        // from `params` itself, where the core nests the whole read answer under `image`
        // (`transcript/actions.rs`, re-checked 2026-09-22). The nesting is the core's deliberate
        // shape and the flattening is the view's wire form, so the translation lives here: a read
        // that failed carries `path` and `error` and no `image`, which is exactly the `failed`
        // method the view's early return wants.
        "chatImage" => {
            let loaded = params.remove("image").filter(Value::is_object);
            let found = loaded.is_some();
            if let Some(Value::Object(image)) = loaded {
                for (key, value) in image {
                    params.entry(key).or_insert(value);
                }
            }
            HostRequest {
                id: None,
                kind: RequestKind::ChatImage,
                method: if found { "loaded" } else { "failed" }.to_string(),
                params,
            }
        }
        // The view dispatches these three on the request's `method`, which is the gesture that
        // produced them: `send`, `queue`, `compact` or `handoff`.
        "draftSubmitted" => HostRequest {
            id: None,
            kind: RequestKind::DraftSubmitted,
            method: method(&params, "send"),
            params,
        },
        "submissionFailed" => HostRequest {
            id: None,
            kind: RequestKind::SubmissionFailed,
            method: method(&params, "send"),
            params,
        },
        "draftReceived" => HostRequest {
            id: None,
            kind: RequestKind::DraftReceived,
            method: "handoff".to_string(),
            params,
        },
        // Everything else is `{kind: "host", method: <action>, params}`, which the view turns into
        // `{type: "sessionChatHostAction", action, ...params}`.
        _ => HostRequest {
            id: None,
            kind: RequestKind::Host,
            method: action.to_string(),
            params,
        },
    }
}

/// A params object, or an empty one for a payload that is not an object.
fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
