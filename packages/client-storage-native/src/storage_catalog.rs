//! Which catalog store owns a client-storage key, as `definitionForKey` in
//! `packages/client-storage/catalog.ts` answers it: the FIRST definition in catalog order whose key
//! matches (a prefix for a collection, the whole key otherwise). Only the columns the start-up
//! migrations need are copied: the id, the key, whether it is a collection, and the backend.
//!
//! CDXC:Settings 2026-09-25 WHY:
//! The start-up migrations moved out of the QuickJS runtime (`initializeClientStorage`) into Rust so
//! a fresh install gets its tables and an upgraded one keeps its data with the runtime off. They
//! decide where a browser-era value belongs by asking the catalog, so this copy must list every
//! definition in the same order: a local key listed before an indexeddb prefix that also matches it
//! must keep winning. `bun tooling/client-storage/catalog-parity.ts` compares this table with the
//! TypeScript one and exits non-zero on any difference; run it whenever `catalog.ts` changes.
//!
//! SEE-ALSO: packages/client-storage/catalog.ts, apps/desktop/src/app/gx_chat/storage.rs (the chat's
//! own copy of its rows' bounds).

/// Where a catalog store keeps its rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogBackend {
    /// The `preferences` table: one raw string per key.
    Local,
    /// Per-process only, never written to disk.
    Session,
    /// The `records` table: a row with bounds and metadata.
    Records,
}

/// One catalog definition, as far as the start-up migrations read it.
#[derive(Clone, Copy, Debug)]
pub struct CatalogStore {
    pub id: &'static str,
    pub key: &'static str,
    pub collection: bool,
    pub backend: CatalogBackend,
    /// The catalog `version`. Every definition is at 1 and none has an `upgrade` today.
    pub version: i64,
}

const fn store(
    id: &'static str,
    key: &'static str,
    collection: bool,
    backend: CatalogBackend,
) -> CatalogStore {
    CatalogStore {
        id,
        key,
        collection,
        backend,
        version: 1,
    }
}

use CatalogBackend::{Local, Records, Session};

/// Every definition of `storageCatalog`, in its order.
pub const CATALOG: &[CatalogStore] = &[
    store(
        "migrationReceipts",
        "ghostex.storage.imported.",
        true,
        Records,
    ),
    store(
        "addRepository",
        "ghostex.addRepository.lastLocation",
        false,
        Local,
    ),
    store(
        "exportOptions",
        "ghostex.exportTranscript.includeOptions",
        false,
        Local,
    ),
    store("exportMode", "ghostex.exportTranscript.mode", false, Local),
    store(
        "gitDiff",
        "ghostex.gitCommitModal.diffPreferences.v1",
        false,
        Local,
    ),
    store(
        "hiddenItems",
        "ghostex.sidebar.hidden-items.v1",
        false,
        Local,
    ),
    store(
        "collections",
        "ghostex.sidebar.projectCollections.v1",
        false,
        Local,
    ),
    store(
        "launcher",
        "ghostex-sidebar-project-terminal-launcher",
        false,
        Local,
    ),
    store("accountOwner", "ghostex.accountSetupOwner", false, Local),
    store(
        "machineTab",
        "ghostex-sidebar-selected-machine-tab",
        true,
        Local,
    ),
    store("collapse", "ghostex-sidebar-ui-collapse-state", true, Local),
    store("codeWrap", "ghostex.sessionChat.codeWrap", false, Local),
    store("verbose", "ghostex.sessionChat.verbose.", true, Records),
    store("summary", "ghostex.sessionChat.summary.", true, Records),
    store(
        "tasksCollapsed",
        "ghostex.chat.agentTasks.collapsed",
        false,
        Local,
    ),
    store(
        "terminalExpanded",
        "ghostex.sessionChat.terminalToolExpanded",
        false,
        Local,
    ),
    store(
        "claudeContext",
        "ghostex.chat.context-details.v1",
        false,
        Local,
    ),
    store(
        "codexContext",
        "ghostex.chat.context-details.codex.v1",
        false,
        Local,
    ),
    store(
        "cursorContext",
        "ghostex.chat.context-details.cursor.v1",
        false,
        Local,
    ),
    store(
        "notices",
        "ghostex.sessionChat.noticeDismissed.",
        true,
        Records,
    ),
    store(
        "sessionOptions",
        "ghostex.sessionChat.options.",
        true,
        Records,
    ),
    store("modelFavorites", "ghostex.model-favorites", false, Local),
    store(
        "modelOutbox",
        "ghostex.model-selection-outbox.",
        true,
        Records,
    ),
    store(
        "retiredQuestions",
        "ghostex:async-questions:",
        true,
        Records,
    ),
    store(
        "questionDrafts",
        "ghostex.sessionChat.questionDraft.",
        true,
        Records,
    ),
    store(
        "interactions",
        "ghostex.session-chat.interactions.v1:",
        true,
        Records,
    ),
    store(
        "composerSelection",
        "ghostex.sessionChat.composerSelection.",
        true,
        Records,
    ),
    store("chatClient", "ghostex.sessionChat.clientId", false, Local),
    store(
        "returnedPrompts",
        "ghostex.sessionChat.returnedPrompts.applied",
        false,
        Local,
    ),
    store("sentHistory", "ghostex.sessionChat.sent.", true, Records),
    store(
        "deliveryReceipts",
        "ghostex.sessionChat.delivered.",
        true,
        Records,
    ),
    store("drafts", "ghostex.sessionChat.draft.", true, Records),
    store("recovery", "ghostex.sessionChat.recovery.", true, Records),
    store(
        "recoveryDismissed",
        "ghostex.sessionChat.recoveryDismissed.",
        true,
        Records,
    ),
    store("draftOutbox", "ghostex.sessionChat.outbox.", true, Records),
    store(
        "chatSnapshots",
        "ghostex.sessionChat.snapshot.",
        true,
        Records,
    ),
    store(
        "workspaceGroups",
        "ghostex-gpui-workspace-session-groups",
        false,
        Local,
    ),
    store(
        "projectLastSession",
        "ghostex.gpui.project-last-session.v1:",
        true,
        Records,
    ),
    store(
        "closeAfterDone",
        "ghostex-gpui-close-after-done-session-ids",
        false,
        Local,
    ),
    store(
        "remoteOrder",
        "ghostex-gpui-remote-group-order",
        false,
        Local,
    ),
    store(
        "remoteRecents",
        "ghostex-gpui-remote-recent-projects",
        false,
        Local,
    ),
    store(
        "remotePresentations",
        "ghostex-gpui-remote-last-seen-presentations",
        true,
        Records,
    ),
    store("nativeSettings", "ghostex-native-settings", false, Local),
    store("boardView", "ghostex-project-board-view", false, Local),
    store("boardCards", "ghostexProjectBoardCardView.v1", false, Local),
    store("docsSide", "ghostex.manage.sidebarSide", false, Local),
    store("docsPinned", "ghostex.manage.sidebarPinned", false, Local),
    store(
        "docsFormatting",
        "ghostex.manage.formattingBarCollapsed",
        false,
        Local,
    ),
    store("docsIndex", "ghostex-docs-index-v1:", true, Session),
    store("docsOpenFiles", "ghostex.manage.openFiles.", true, Records),
    store("docsDrafts", "ghostex.manage.drafts.", true, Records),
    store(
        "docsActiveFile",
        "ghostex.manage.activeFile.",
        true,
        Records,
    ),
    store("commitAgent", "ghostex.promptAgent.gitCommit", false, Local),
    store(
        "renameAgent",
        "ghostex.promptAgent.renameSession",
        false,
        Local,
    ),
    store(
        "openTarget",
        "ghostex.titlebar.lastOpenTargetId",
        false,
        Local,
    ),
    store(
        "lastAction",
        "ghostex.titlebar.lastActionCommandByProject:",
        true,
        Records,
    ),
    store(
        "keepAwake",
        "ghostex.titlebar.keepAwakeRuntime",
        false,
        Local,
    ),
    store(
        "keepAwakeSync",
        "ghostex.titlebar.keepAwakeRuntimeSync",
        false,
        Local,
    ),
    store(
        "lidSleep",
        "ghostex.titlebar.lidSleepPrevention",
        false,
        Local,
    ),
    store("titlebarGit", "ghostex.titlebar.gitState.", true, Records),
    store("tipsRead", "ghostex.titlebar.tips.readIds", false, Local),
    store(
        "modelCatalog",
        "ghostex.agentModelCatalog.v1",
        false,
        Records,
    ),
    store("agentationToolbar", "feedback-toolbar-", true, Local),
    store(
        "agentationAnnotations",
        "feedback-annotations-",
        true,
        Local,
    ),
    store(
        "agentationHidden",
        "agentation-session-toolbar-hidden",
        false,
        Session,
    ),
    store("agentationModes", "agentation-", true, Local),
];

/// `definitionForKey`: the first definition that owns `key`.
pub fn definition_for_key(key: &str) -> Option<&'static CatalogStore> {
    CATALOG.iter().find(|definition| {
        if definition.collection {
            key.starts_with(definition.key)
        } else {
            key == definition.key
        }
    })
}

/// A definition by id.
pub fn catalog_store(id: &str) -> Option<&'static CatalogStore> {
    CATALOG.iter().find(|definition| definition.id == id)
}
