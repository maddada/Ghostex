//! The stored option state: `composer('optionWrite', {optionKey, optionState})`.
//!
//! Port of `nativeOptionPersistence` in
//! `packages/shared/session-chat-controller/native-options.ts` and of
//! `readStoredSessionChatOptions` / `writeStoredSessionChatOptions` in
//! `packages/core-ui/chat/session-chat-session-options.ts`.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! The core had no writer at all for the option pills: `OptionStore::take_dirty` existed with no
//! caller, so a detected model or effort was never persisted and the core made three fewer host
//! round trips than the TypeScript at boot. The replay that checked the port paired storage
//! answers by ORDER, so those three missing writes drifted every answer after them.
//!
//! The record is keyed by the SCOPED option key (`<sessionKey>` or `<sessionKey>#<agentId>`), not
//! by the session key, which is what `storedSessionChatOptionKeys` scans for and what the host
//! hands back in the boot read's `optionStates` map.

use serde_json::{Map, Value};

use crate::event::StorageKey;
use crate::menus::option_values::{option_state_from_value, OptionState};

/// The store id, as `StorageKey::store`. The host owns the
/// `ghostex.sessionChat.options.` prefix and the scoped suffix.
pub const SESSION_OPTIONS_STORE: &str = "sessionOptions";

/// The record one scoped option key lives in.
pub fn session_options_key(option_key: &str) -> StorageKey {
    StorageKey {
        store: SESSION_OPTIONS_STORE.to_string(),
        suffix: option_key.to_string(),
    }
}

/// `persistence.read(key)`: `seed.optionStates[key] ?? {}`.
///
/// An absent key, an absent map or a malformed entry all read as "nothing stored", which is what
/// `readStoredSessionChatOptions` answers for an unparseable record.
pub fn stored_option_state(seed: &Value, option_key: Option<&str>) -> OptionState {
    let Some(option_key) = option_key else {
        return OptionState::new();
    };
    let Some(entry) = seed.get(option_key) else {
        return OptionState::new();
    };
    option_state_from_value(entry)
}

/// `seed.optionStates[key] = state`, so a store rebuilt later seeds from what was last written
/// rather than from what the boot read happened to carry.
pub fn remember_option_state(seed: &mut Value, option_key: &str, state: &OptionState) {
    let value = option_state_to_value(state);
    match seed.as_object_mut() {
        Some(object) => {
            object.insert(option_key.to_string(), value);
        }
        None => {
            let mut object = Map::new();
            object.insert(option_key.to_string(), value);
            *seed = Value::Object(object);
        }
    }
}

/// `JSON.stringify(state)`: the option values in the field order the TypeScript wrote them.
pub fn option_state_to_value(state: &OptionState) -> Value {
    serde_json::to_value(state).unwrap_or(Value::Null)
}
