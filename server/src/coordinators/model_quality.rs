//! The line a coordinator's guide gets when its model or effort may make it work worse
//! (`coordinator_quality_concern` in gx-protocol, which holds the user's decision).
//!
//! CDXC:Coordinators 2026-11-13 WHY:
//! The line is added to the guide `/api/readCoordinatorGuide` answers, never to the text the guide's version is taken from (playbook.rs `guide_for`), so a model change never reads as "the guide changed" to every coordinator; the coordinator sees it on its next read of the guide. The model and effort are the ones gxserver keeps for the session without reading its screen (`detect_session_chat_stored_options`: transcript, statusline, launch flags, the user's last pick), so a read stays cheap.
//! SEE-ALSO: packages/gx-protocol/src/coordinator.rs (the rule and its words), packages/gx-chat-core/src/session/coordinator_quality.rs (the chat's warning pill).

use ghostex_gx_chat_core::protocol::coordinator_quality_concern;

use super::records::SessionKey;
use crate::domain::DomainRepository;

/// The guide line for the coordinator `key`, or `None` when it runs on a model it works well on.
pub fn coordinator_quality_guide_line(
    repository: &DomainRepository<'_>,
    key: &SessionKey,
) -> Option<String> {
    let session = repository.get_session(&key.0, &key.1).ok().flatten()?;
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session)
        .or_else(|| crate::session_chat_follower::session_chat_display_agent_for_session(&session));
    let paths = crate::paths::get_gxserver_paths(None);
    let hook_state_directory =
        crate::session_chat_options::session_chat_hook_state_directory(&paths);
    let detected = crate::session_chat_options::detect_session_chat_stored_options(
        repository,
        &hook_state_directory,
        &key.0,
        &key.1,
        agent.as_deref(),
    );
    let selection = detected.as_ref().map(|detected| &detected.selection);
    let model = selection.and_then(|selection| selection.model.as_ref());
    let effort = selection.and_then(|selection| selection.effort.as_ref());
    coordinator_quality_concern(
        model.map(|model| model.value.as_str()),
        model.map(|model| model.label.as_str()),
        effort.map(|effort| effort.value.as_str()),
    )
    .map(|concern| concern.guide_line())
}
