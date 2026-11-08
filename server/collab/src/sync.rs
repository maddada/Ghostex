//! What gxserver is told, so it can let guests read their one chat and draw the chat's Collab
//! part (guests, typing, held messages) without holding any of Collab itself:
//! - the live shares ([`HostCall::SetShares`]): sent whenever they change, and checked every few
//!   seconds so a share that runs out ends on gxserver's side too;
//! - each shared chat's `collab` part ([`HostCall::SetChatExtras`]): sent whenever something
//!   in it changes ([`chat_changed`], where gxserver used to republish the chat itself).
//!
//! Changes are queued and sent by one task, in order, off the async threads.

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use ghostex_gx_protocol::collab_component::{HostCall, ShareScope};

#[derive(Default)]
struct State {
    queue: VecDeque<(String, String)>,
    last_scopes: Option<Vec<ShareScope>>,
    /// Chats gxserver was last told are shared.
    shared_chats: BTreeSet<(String, String)>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}

fn wake() -> &'static tokio::sync::Notify {
    static WAKE: OnceLock<tokio::sync::Notify> = OnceLock::new();
    WAKE.get_or_init(tokio::sync::Notify::new)
}

/// Something in a chat's Collab part changed (a guest joined, typed, a message was held).
pub fn chat_changed(project_id: &str, session_id: &str) {
    state()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .queue
        .push_back((project_id.to_string(), session_id.to_string()));
    wake().notify_one();
}

/// Sends everything queued and the share list if it changed (blocking).
pub fn drain() {
    // One drain at a time, so an older share list never lands after a newer one.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let scopes = crate::shares::scopes();
    let (changed_scopes, mut chats) = {
        let mut state = state().lock().unwrap_or_else(PoisonError::into_inner);
        let changed = state.last_scopes.as_ref() != Some(&scopes);
        let chats: Vec<(String, String)> = state.queue.drain(..).collect();
        (changed, chats)
    };
    if changed_scopes {
        let live: BTreeSet<(String, String)> = scopes
            .iter()
            .map(|scope| (scope.project_id.clone(), scope.session_id.clone()))
            .collect();
        if crate::host::call(HostCall::SetShares {
            shares: scopes.clone(),
        })
        .is_ok()
        {
            let mut state = state().lock().unwrap_or_else(PoisonError::into_inner);
            state.last_scopes = Some(scopes);
            // Every chat that started or stopped being shared gets its part again.
            chats.extend(state.shared_chats.symmetric_difference(&live).cloned());
            chats.extend(live.iter().cloned());
        }
    }
    chats.sort();
    chats.dedup();
    for (project_id, session_id) in chats {
        let extras = crate::shares::chat_extras(&project_id, &session_id);
        let shared = extras["shared"] == true;
        if crate::host::call(HostCall::SetChatExtras {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            extras,
        })
        .is_ok()
        {
            let mut state = state().lock().unwrap_or_else(PoisonError::into_inner);
            if shared {
                state.shared_chats.insert((project_id, session_id));
            } else {
                state.shared_chats.remove(&(project_id, session_id));
            }
        }
    }
}

/// Forgets what gxserver was told, so the next [`drain`] sends everything again.
pub fn resync() {
    let mut state = state().lock().unwrap_or_else(PoisonError::into_inner);
    state.last_scopes = None;
    wake().notify_one();
}

/// The sending task, for the life of the sidecar.
pub async fn run() {
    loop {
        let _ = tokio::task::spawn_blocking(drain).await;
        tokio::select! {
            _ = wake().notified() => {}
            _ = crate::changed() => {}
            _ = tokio::time::sleep(Duration::from_secs(10)) => {}
        }
        // Coalesce a burst (a guest typing sends many).
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}
