//! The host's half of the chat socket: which chat follows which conversation, what a frame is for,
//! and the two pushes gxserver's stream and client storage make to every chat at once.
//!
//! CDXC:SessionChat 2026-09-25 DECISION:
//! User: "i think web should be already using rust core (gpui-web ghostex i mean)" and "i want this
//! whole project done". The chat's connection to gxserver is Rust, shared by the desktop app and
//! the GPUI web build: `packages/gx-chat-client` owns the socket, this file connects it to the
//! retained chats, and both apps compile this folder. The QuickJS broker, its retained store and
//! its socket (`apps/desktop/sidebar/session-chat-runtime/`) and the web build's TypeScript chat in
//! an iframe are deleted.
//!
//! A conversation is followed for as long as its chat is RETAINED, not while a view shows it:
//! `store.ts`'s rule, which the user approved on 2026-09-12 ("retaining chat data and live
//! subscriptions independently of mounted views for fast session switching"). So a view closing
//! changes nothing here, and the store's prune (`worker.rs`'s `purge`) is what unfollows.
//!
//! SEE-ALSO: packages/gx-chat-client (the socket), apps/gpui-web/src/app/gx_chat/ (the web runner
//! and storage over the same files).

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_chat_client::{ChatStreams, Endpoint, Inbound};
use ghostex_gx_chat_core::{Event, StorageKey};
use serde_json::Value;

use super::identity::{ChatIdentity, LOCAL_MACHINE_ID};
use super::storage;
use super::world::{World, drive};

/// The model catalog's cache record, which every chat's boot read carries.
const MODEL_CATALOG_STORE: &str = "modelCatalog";

/// The three context-detail preference records and the agent each belongs to.
const CONTEXT_STORES: [(&str, &str); 3] = [
    ("claudeContext", "claude"),
    ("codexContext", "codex"),
    ("cursorContext", "cursor"),
];

/// The socket and what the host knows about it.
#[derive(Default)]
pub(super) struct Transport {
    /// Installed by the runner, which is the one that knows how an inbound value reaches this
    /// thread.
    streams: Option<ChatStreams>,
    /// The endpoint each machine was last given, so an unchanged one is not sent again.
    endpoints: BTreeMap<String, Endpoint>,
    /// Retention keys of the chats that follow their conversation.
    followed: BTreeSet<String>,
    /// The catalog gxserver last pushed that this host adopted, the one a subscribing chat is given.
    catalog: Option<Value>,
}

impl Transport {
    pub(super) fn install(&mut self, streams: ChatStreams) {
        self.streams = Some(streams);
    }

    /// Where a machine's gxserver is. The same value again does nothing.
    pub(super) fn set_endpoint(&mut self, machine_id: &str, endpoint: Endpoint) {
        if self.endpoints.get(machine_id) == Some(&endpoint) {
            return;
        }
        self.endpoints
            .insert(machine_id.to_string(), endpoint.clone());
        if let Some(streams) = self.streams.as_mut() {
            streams.set_endpoint(machine_id, endpoint);
        }
    }

    /// `Effect::Subscribe`: follows the chat's conversation.
    pub(super) fn follow(&mut self, key: &str, identity: &ChatIdentity, limit: u32) {
        let Some(streams) = self.streams.as_mut() else {
            return;
        };
        self.followed.insert(key.to_string());
        streams.follow(
            machine(identity),
            &identity.project_id,
            &identity.session_id,
            limit,
        );
    }

    /// `Effect::Reconnect`: asks gxserver for a fresh authoritative snapshot of the conversation.
    pub(super) fn refresh(&mut self, identity: &ChatIdentity) {
        if let Some(streams) = self.streams.as_mut() {
            streams.refresh(machine(identity), &identity.project_id, &identity.session_id);
        }
    }

    /// `Effect::Unsubscribe`, and the store letting a chat go.
    pub(super) fn unfollow(&mut self, key: &str, identity: &ChatIdentity) {
        if !self.followed.remove(key) {
            return;
        }
        if let Some(streams) = self.streams.as_mut() {
            streams.unfollow(machine(identity), &identity.project_id, &identity.session_id);
        }
    }

    /// The pushed catalog, when this host adopted one, for a chat that just subscribed.
    pub(super) fn catalog(&self) -> Option<&Value> {
        self.catalog.as_ref()
    }
}

/// The machine id the socket is keyed by: `"local"` on this computer.
fn machine(identity: &ChatIdentity) -> &str {
    identity.machine_id.as_deref().unwrap_or(LOCAL_MACHINE_ID)
}

/// The endpoint a view's config names, which only a remote chat carries: a local chat's endpoint
/// is the app's own daemon, which the app sets once for every chat (`set_endpoint`).
pub(super) fn config_endpoint(config: &Value) -> Option<Endpoint> {
    let endpoint = config.get("endpoint")?;
    let endpoint = Endpoint::new(
        endpoint.get("baseUrl")?.as_str()?,
        endpoint.get("authToken")?.as_str()?,
    );
    endpoint.usable().then_some(endpoint)
}

/// What the socket read, delivered to the chat it belongs to.
pub(super) fn receive(world: &mut World, inbound: Inbound) {
    match inbound {
        Inbound::Frame {
            machine_id,
            project_id,
            session_id,
            frame,
        } => {
            let key = Value::Array(vec![
                Value::String(machine_id),
                Value::String(project_id),
                Value::String(session_id),
            ])
            .to_string();
            if world.store.get(&key).is_none() {
                return;
            }
            if let Some(event) = super::events::frame(frame) {
                drive(world, &key, vec![event]);
            }
        }
        Inbound::ModelCatalog { catalog, .. } => adopt_catalog(world, catalog),
    }
}

/// `adoptPublishedAgentModelCatalog`: a pushed catalog replaces the lineup only when it is at least
/// as new as the one in effect, is cached for the next boot read, and reaches every chat.
///
/// The one in effect is the last push this host adopted, else the cached record (which the
/// TypeScript's `current` started from). A server whose last fetch predates this computer's never
/// rolls it back, and the same lineup again redraws nothing.
fn adopt_catalog(world: &mut World, catalog: Value) {
    if ghostex_gx_chat_core::menus::catalog::parse_agent_model_catalog(&catalog).is_none() {
        return;
    }
    let now_ms = super::platform::now_millis();
    let record = StorageKey {
        store: MODEL_CATALOG_STORE.to_string(),
        suffix: String::new(),
    };
    let current = world.transport.catalog.clone().or_else(|| {
        storage::read(&record, now_ms)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    });
    if let Some(current) = &current {
        let updated_at = |catalog: &Value| {
            catalog
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        if *current == catalog || updated_at(&catalog) < updated_at(current) {
            return;
        }
    }
    let _ = storage::write(&record, Some(&catalog.to_string()), now_ms);
    world.transport.catalog = Some(catalog.clone());
    for key in world.store.keys() {
        drive(
            world,
            &key,
            vec![Event::ModelCatalogChanged {
                catalog: catalog.clone(),
            }],
        );
    }
}

/// The event a context-detail preference write is for every OTHER chat, or `None` for any other
/// write.
///
/// `subscribeSessionChatContextDetailsPreferences` pushed a changed preference to every chat the
/// broker served, so a row starred in one chat's context panel is starred in the next one opened.
/// All chats share this host, so the write is the signal; the chat that wrote it already has it.
pub(super) fn context_preference_event(store: &str, value: Option<&str>) -> Option<Event> {
    let (_, provider) = CONTEXT_STORES.iter().find(|(id, _)| *id == store)?;
    let preferences = value
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or(Value::Null);
    Some(Event::ContextPreferencesChanged {
        provider: (*provider).to_string(),
        preferences,
    })
}
