//! The Settings pages. Each page is an entity implementing `Render` and `SettingsPage`; the shell
//! creates it the first time its page opens (docs/2026-09-28/gpui-modals-migration/SETTINGS-ARCH.md).
pub(crate) mod about;
pub(crate) mod accounts;
pub(crate) mod actions;
pub(crate) mod agents;
pub(crate) mod cloud_boxes;
pub(crate) mod connectors;
pub(crate) mod debugging;
pub(crate) mod extensions;
pub(crate) mod general;
pub(crate) mod hotkeys;
pub(crate) mod integrations;
pub(crate) mod machine_picker;
pub(crate) mod open_forms;
pub(crate) mod open_targets;
pub(crate) mod orchestrator_playbook;
pub(crate) mod os_integration;
pub(crate) mod projects;
pub(crate) mod remote;
pub(crate) mod secrets;
pub(crate) mod sync;
pub(crate) mod theme;
pub(crate) mod updates;
pub(crate) mod voice_agent;
pub(crate) mod voice_agent_history;
pub(crate) mod voice_dictate;
pub(crate) mod windows_link_notice;
pub(crate) mod workspaces;

use super::model::SettingsTabId;
use super::store::SettingsStore;
use gpui::{AnyView, App, AppContext as _, Entity, Window};

/// Creates the view of page `tab`.
pub(crate) fn settings_tab_view(
    tab: SettingsTabId,
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    match tab {
        SettingsTabId::Chat
        | SettingsTabId::Sidebar
        | SettingsTabId::Terminal
        | SettingsTabId::Notifications
        | SettingsTabId::FilesLinks
        | SettingsTabId::PowerSleep => cx
            .new(|cx| general::GeneralTab::new(store.clone(), tab, window, cx))
            .into(),
        SettingsTabId::Updates => updates::updates_tab_view(store, cx),
        SettingsTabId::Remote => remote::remote_tab_view(store, window, cx),
        SettingsTabId::Sync => sync::sync_tab_view(store, cx),
        SettingsTabId::Projects => projects::projects_tab_view(store, window, cx),
        SettingsTabId::Agents => agents::agents_tab_view(store, window, cx),
        SettingsTabId::Actions => actions::actions_tab_view(store, window, cx),
        SettingsTabId::About => about::about_tab_view(store, cx),
        SettingsTabId::Debugging => debugging::debugging_tab_view(store, cx),
        SettingsTabId::Integrations => integrations::integrations_tab_view(store, cx),
        SettingsTabId::CloudBoxes => cloud_boxes::cloud_boxes_tab_view(store, cx),
        SettingsTabId::Hotkeys => hotkeys::hotkeys_tab_view(store, cx),
        SettingsTabId::Theme => theme::theme_tab_view(store, window, cx),
        SettingsTabId::OpenTargets => open_targets::open_targets_tab_view(store, window, cx),
        SettingsTabId::OsIntegration => os_integration::os_integration_tab_view(store, cx),
        SettingsTabId::Extensions => extensions::extensions_tab_view(store, window, cx),
        SettingsTabId::Accounts => accounts::accounts_tab_view(store, window, cx),
        SettingsTabId::Secrets => secrets::secrets_tab_view(store, window, cx),
        SettingsTabId::Workspaces => workspaces::workspaces_tab_view(store, cx),
        SettingsTabId::Connectors => connectors::connectors_tab_view(store, cx),
        SettingsTabId::VoiceAgent => voice_agent::voice_agent_tab_view(store, window, cx),
        // TAB-ARMS: one arm per page.
    }
}

/// A page that can hold input a click away must not throw away: an open add or edit form, an
/// unsaved draft, a hotkey being recorded, or a confirmation waiting for its answer.
pub(crate) trait HoldsUnsavedInput: 'static {
    fn holds_unsaved_input(&self, cx: &App) -> bool;
}

impl HoldsUnsavedInput for actions::ActionsTab {
    /// The Add or Edit action form is open.
    fn holds_unsaved_input(&self, _cx: &App) -> bool {
        self.editor.is_some()
    }
}

impl HoldsUnsavedInput for extensions::ExtensionsTab {
    /// The custom view or view scope editor is open.
    fn holds_unsaved_input(&self, _cx: &App) -> bool {
        self.view_editor.is_some() || self.scope_editor.is_some()
    }
}

/// Whether page `view` holds unsaved input (`HoldsUnsavedInput`); pages without forms never do.
pub(crate) fn settings_tab_holds_unsaved_input(view: &AnyView, cx: &App) -> bool {
    fn holds<T: HoldsUnsavedInput>(view: &AnyView, cx: &App) -> bool {
        view.clone()
            .downcast::<T>()
            .is_ok_and(|page| page.read(cx).holds_unsaved_input(cx))
    }
    holds::<accounts::AccountsTab>(view, cx)
        || holds::<actions::ActionsTab>(view, cx)
        || holds::<agents::AgentsTab>(view, cx)
        || holds::<cloud_boxes::CloudBoxesTab>(view, cx)
        || holds::<extensions::ExtensionsTab>(view, cx)
        || holds::<hotkeys::HotkeysTab>(view, cx)
        || holds::<integrations::IntegrationsTab>(view, cx)
        || holds::<open_targets::OpenTargetsTab>(view, cx)
        || holds::<projects::ProjectsTab>(view, cx)
        || holds::<remote::RemoteTab>(view, cx)
        || holds::<secrets::SecretsTab>(view, cx)
}
