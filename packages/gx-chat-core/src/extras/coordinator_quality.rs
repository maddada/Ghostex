//! The note pill a coordinator's chat shows while its model or effort may give a degraded
//! experience ("You might get a degraded experience."), from the model and effort the chat's
//! model pill reads (gxserver's `selectedOptions`), so it follows a model or effort change at once.
//! Its menu hides it for this chat; the hide is kept per session in the `notices` store, under its
//! own key, so it survives a restart.
//!
//! CDXC:Coordinators 2026-11-13 WHY:
//! The pill waits until the chat knows the model (a value the agent reported, the launch flags, or the user's pick) or gxserver has read the agent's screen and found none, and until the hide has been read, so a coordinator on a good model never flashes the warning while its chat starts.
//! SEE-ALSO: packages/gx-protocol/src/coordinator.rs (the user's decision, the rule and its words), apps/desktop/src/app/native_chat/chat_pills.rs (desktop and web), apps/mobile/app/src/chat/native/cards/ChatPills.tsx (the phone), packages/gx-chat-core/src/menus/picker/projection.rs (the model picker's note).

use serde_json::json;

use crate::document::ChatPill;
use crate::effect::Effect;
use crate::event::StorageKey;
use crate::menus::option_catalog::session_option_catalog;
use crate::menus::option_values::{option_value_label, OptionSource};
use crate::protocol::{coordinator_model_may_degrade, COORDINATOR_DEGRADED_TEXT};
use crate::questions::drafts::NOTICES_STORE;
use crate::state::ChatState;

/// The pill's id in `chatPills`.
pub const QUALITY_PILL: &str = "coordinatorQuality";
/// Its menu row's command.
pub const DISMISS_COMMAND: &str = "dismissCoordinatorQuality";
const KEY_PREFIX: &str = "coordinatorQuality:";
const HIDE_LABEL: &str = "Hide for this chat";
const ADVICE: &str = "From the model and effort this coordinator runs on.";

/// Whether this chat hid the pill: `None` until the stored answer is read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoordinatorQualityState {
    pub hidden: Option<bool>,
    /// The session key the read was asked for.
    pub read_for: Option<String>,
}

/// Where this session's hide is kept: the `notices` store, apart from its dismissed notice.
pub fn hidden_key(session_key: &str) -> StorageKey {
    StorageKey {
        store: NOTICES_STORE.to_string(),
        suffix: format!("{KEY_PREFIX}{session_key}"),
    }
}

/// True for this module's record, which the notices family must leave alone.
pub fn is_hidden_key(key: &StorageKey) -> bool {
    key.store == NOTICES_STORE && key.suffix.starts_with(KEY_PREFIX)
}

/// Asks for this coordinator chat's stored hide once per session.
pub fn read_hidden(state: &mut ChatState) -> Option<Effect> {
    let session_key = state.identity.session_key.clone();
    if session_key.is_empty()
        || state.session.coordinator_threads.is_none()
        || state.extras.coordinator_quality.read_for.as_deref() == Some(session_key.as_str())
    {
        return None;
    }
    state.extras.coordinator_quality = CoordinatorQualityState {
        hidden: None,
        read_for: Some(session_key.clone()),
    };
    Some(Effect::ReadStorage {
        key: hidden_key(&session_key),
    })
}

/// Adopts the stored hide for this session.
pub fn hidden_loaded(state: &mut ChatState, key: &StorageKey, value: Option<&str>) {
    if *key != hidden_key(&state.identity.session_key) {
        return;
    }
    state.extras.coordinator_quality.hidden = Some(value.is_some_and(|value| !value.is_empty()));
    state.core.request_publish();
}

/// `dismissCoordinatorQuality`: hides the pill in this chat for good.
pub fn dismiss(state: &mut ChatState) -> Vec<Effect> {
    state.extras.coordinator_quality.hidden = Some(true);
    vec![Effect::WriteStorage {
        key: hidden_key(&state.identity.session_key),
        value: Some("1".to_string()),
        durable: true,
    }]
}

/// The pill, on a coordinator whose model or effort may give a degraded experience
/// ([`coordinator_model_may_degrade`]), until this chat hides it.
pub fn pill(state: &ChatState) -> Option<ChatPill> {
    if state.session.coordinator_threads.is_none()
        || state.extras.coordinator_quality.hidden != Some(false)
    {
        return None;
    }
    let catalog =
        session_option_catalog(&state.menus.model_catalog, state.session.agent.as_deref());
    let values = state.menus.options.state();
    let known = |id: &str| {
        values
            .get(id)
            .filter(|entry| entry.source != OptionSource::Default && !entry.value.is_empty())
    };
    let model_id = catalog
        .as_ref()
        .map(|catalog| catalog.model.id.clone())
        .unwrap_or_else(|| "model".to_string());
    let model = known(&model_id);
    // Nothing known yet: wait for the agent to report, or for gxserver to say it found nothing.
    if model.is_none() && !state.session.screen_probed {
        return None;
    }
    let label = model.and_then(|_| {
        catalog.as_ref().and_then(|catalog| {
            option_value_label(&state.menus.model_catalog, &catalog.model, values)
        })
    });
    if !coordinator_model_may_degrade(
        model.map(|entry| entry.value.as_str()),
        label
            .as_deref()
            .or(model.and_then(|entry| entry.label.as_deref())),
        known("effort").map(|entry| entry.value.as_str()),
    ) {
        return None;
    }
    let text = COORDINATOR_DEGRADED_TEXT.to_string();
    Some(ChatPill {
        id: QUALITY_PILL.to_string(),
        label: text.clone(),
        aria_label: format!("{text}. {ADVICE}"),
        meta: ADVICE.to_string(),
        menu: vec![json!({
            "label": HIDE_LABEL,
            "iconPath": "titlebar/eye-off.svg",
            "command": { "type": DISMISS_COMMAND },
        })],
        ..ChatPill::default()
    })
}
