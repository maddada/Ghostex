//! What each open chat is told about the computer its session runs on (`Event::ComputerChanged`):
//! its name, whether it is offline, and when this app last reached it, for the calm card above
//! the chat box and the messages this computer holds for it
//! (packages/gx-chat-core/src/session/computer.rs).
//!
//! The app publishes one entry per remote machine on every connect transition and after each
//! check a chat asked for (`remote_conn/chat_computers.rs`); the chat host hands every chat whose
//! copy is older the entry of its own machine (`world.rs`). A chat on this computer is told it is
//! local, so it keeps the "Couldn't reach Ghostex" words for this computer's own Ghostex. The web
//! build has no remote machines and publishes nothing.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ghostex_gx_chat_core::session::computer::SessionComputer;

#[derive(Default)]
struct Published {
    revision: u64,
    computers: Arc<BTreeMap<String, SessionComputer>>,
}

static PUBLISHED: Mutex<Option<Published>> = Mutex::new(None);

/// Replaces the published machines when anything about them changed.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub(crate) fn publish(computers: BTreeMap<String, SessionComputer>) {
    let mut published = PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let published = published.get_or_insert_with(Default::default);
    if *published.computers == computers {
        return;
    }
    published.revision += 1;
    published.computers = Arc::new(computers);
    #[cfg(not(target_family = "wasm"))]
    super::worker::nudge_computers();
}

/// The published revision (0 before anything was published) and the machines.
pub(crate) fn current() -> (u64, Arc<BTreeMap<String, SessionComputer>>) {
    PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|published| (published.revision, published.computers.clone()))
        .unwrap_or_default()
}

/// What a chat on `machine_id` (`None` for this computer) is told.
pub(super) fn for_machine(
    computers: &BTreeMap<String, SessionComputer>,
    machine_id: Option<&str>,
) -> SessionComputer {
    machine_id
        .and_then(|machine_id| computers.get(machine_id))
        .cloned()
        .unwrap_or_default()
}
