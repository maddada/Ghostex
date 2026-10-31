use super::*;

macro_rules! rebuild_sessions_with_session_tag {
    ($version:literal) => {
        concat!(
            r#"
      UPDATE sessions
      SET sessionTag = NULL
      WHERE sessionTag IS NOT NULL
        AND sessionTag NOT IN (
          'favorite',
          'high-priority',
          'research',
          'todo',
          'in-progress',
          'testing',
          'blocked',
          'low-priority',
          'on-hold',
          'done',
          'bug',
          'feature',
          'design'
        );

      CREATE TABLE sessions_next (
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('terminal', 'agent', 't3')),
        title TEXT NOT NULL,
        lifecycleState TEXT NOT NULL CHECK (lifecycleState IN ('running', 'sleeping', 'stopped', 'missing', 'unknown')),
        providerStateJson TEXT NOT NULL,
        zmxName TEXT NOT NULL,
        cwd TEXT,
        agentId TEXT,
        commandId TEXT,
        isPinned INTEGER NOT NULL DEFAULT 0 CHECK (isPinned IN (0, 1)),
        isFavorite INTEGER NOT NULL DEFAULT 0 CHECK (isFavorite IN (0, 1)),
        restoredFromSessionId TEXT,
        restoredFromHistoryId TEXT,
        launchSettingsJson TEXT NOT NULL DEFAULT '{}',
        runtimeSettingsJson TEXT NOT NULL DEFAULT '{}',
        completionRulesJson TEXT NOT NULL DEFAULT '{}',
        attentionRulesJson TEXT NOT NULL DEFAULT '{}',
        notificationRulesJson TEXT NOT NULL DEFAULT '{}',
        worktreeJson TEXT NOT NULL DEFAULT '{}',
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        lastActiveAt TEXT,
        sidebarOrder REAL,
        sessionTag TEXT CHECK (
          sessionTag IS NULL OR sessionTag IN (
            'favorite',
            'high-priority',
            'research',
            'todo',
            'in-progress',
            'testing',
            'blocked',
            'low-priority',
            'on-hold',
            'done',
            'bug',
            'feature',
            'design'
          )
        ),
        PRIMARY KEY (projectId, sessionId),
        FOREIGN KEY (projectId) REFERENCES projects(projectId) ON DELETE CASCADE
      );

      INSERT INTO sessions_next (
        projectId,
        sessionId,
        kind,
        title,
        lifecycleState,
        providerStateJson,
        zmxName,
        cwd,
        agentId,
        commandId,
        isPinned,
        isFavorite,
        restoredFromSessionId,
        restoredFromHistoryId,
        launchSettingsJson,
        runtimeSettingsJson,
        completionRulesJson,
        attentionRulesJson,
        notificationRulesJson,
        worktreeJson,
        createdAt,
        updatedAt,
        lastActiveAt,
        sidebarOrder,
        sessionTag
      )
      SELECT
        projectId,
        sessionId,
        kind,
        title,
        lifecycleState,
        providerStateJson,
        zmxName,
        cwd,
        agentId,
        commandId,
        isPinned,
        isFavorite,
        restoredFromSessionId,
        restoredFromHistoryId,
        launchSettingsJson,
        runtimeSettingsJson,
        completionRulesJson,
        attentionRulesJson,
        notificationRulesJson,
        worktreeJson,
        createdAt,
        updatedAt,
        lastActiveAt,
        sidebarOrder,
        sessionTag
      FROM sessions;

      DROP TABLE sessions;
      ALTER TABLE sessions_next RENAME TO sessions;

      CREATE INDEX IF NOT EXISTS idx_sessions_project_updated
        ON sessions(projectId, updatedAt);

      CREATE INDEX IF NOT EXISTS idx_sessions_project_sidebar_order
        ON sessions(projectId, sidebarOrder);

      PRAGMA user_version = "#,
            $version,
            r#";
    "#
        )
    };
}

pub const GXSERVER_STORAGE_MIGRATIONS: &[Migration] = &[
    Migration {
        id: "0001_foundation",
        sql: r#"
      CREATE TABLE IF NOT EXISTS metadata (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS id_allocations (
        allocationId INTEGER PRIMARY KEY,
        id TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('server', 'project', 'session')),
        parentId TEXT NOT NULL DEFAULT '',
        createdAt TEXT NOT NULL,
        UNIQUE(kind, parentId, id)
      );

      CREATE INDEX IF NOT EXISTS idx_id_allocations_kind_parent
        ON id_allocations(kind, parentId);

      PRAGMA user_version = 1;
    "#,
    },
    Migration {
        id: "0002_domain_state",
        sql: r#"
      CREATE TABLE IF NOT EXISTS projects (
        projectId TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT,
        identityIconJson TEXT NOT NULL DEFAULT '{}',
        isPinned INTEGER NOT NULL DEFAULT 0 CHECK (isPinned IN (0, 1)),
        isFavorite INTEGER NOT NULL DEFAULT 0 CHECK (isFavorite IN (0, 1)),
        defaultCommand TEXT,
        worktreeJson TEXT NOT NULL DEFAULT '{}',
        customAgentsJson TEXT NOT NULL DEFAULT '[]',
        customAgentOrderJson TEXT NOT NULL DEFAULT '[]',
        customCommandsJson TEXT NOT NULL DEFAULT '[]',
        customCommandOrderJson TEXT NOT NULL DEFAULT '[]',
        deletedDefaultCommandIdsJson TEXT NOT NULL DEFAULT '[]',
        launchSettingsJson TEXT NOT NULL DEFAULT '{}',
        runtimeSettingsJson TEXT NOT NULL DEFAULT '{}',
        completionRulesJson TEXT NOT NULL DEFAULT '{}',
        attentionRulesJson TEXT NOT NULL DEFAULT '{}',
        notificationRulesJson TEXT NOT NULL DEFAULT '{}',
        gitConfigJson TEXT NOT NULL DEFAULT '{}',
        projectBoardConfigJson TEXT NOT NULL DEFAULT '{}',
        previousSessionHistoryJson TEXT NOT NULL DEFAULT '[]',
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS sessions (
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('terminal', 'agent')),
        title TEXT NOT NULL,
        lifecycleState TEXT NOT NULL CHECK (lifecycleState IN ('running', 'sleeping', 'stopped', 'missing', 'unknown')),
        providerStateJson TEXT NOT NULL,
        zmxName TEXT NOT NULL,
        cwd TEXT,
        agentId TEXT,
        commandId TEXT,
        isPinned INTEGER NOT NULL DEFAULT 0 CHECK (isPinned IN (0, 1)),
        isFavorite INTEGER NOT NULL DEFAULT 0 CHECK (isFavorite IN (0, 1)),
        restoredFromSessionId TEXT,
        restoredFromHistoryId TEXT,
        launchSettingsJson TEXT NOT NULL DEFAULT '{}',
        runtimeSettingsJson TEXT NOT NULL DEFAULT '{}',
        completionRulesJson TEXT NOT NULL DEFAULT '{}',
        attentionRulesJson TEXT NOT NULL DEFAULT '{}',
        notificationRulesJson TEXT NOT NULL DEFAULT '{}',
        worktreeJson TEXT NOT NULL DEFAULT '{}',
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        lastActiveAt TEXT,
        PRIMARY KEY (projectId, sessionId),
        FOREIGN KEY (projectId) REFERENCES projects(projectId) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_sessions_project_updated
        ON sessions(projectId, updatedAt);

      PRAGMA user_version = 2;
    "#,
    },
    Migration {
        id: "0003_session_sidebar_order",
        sql: r#"
      ALTER TABLE sessions ADD COLUMN sidebarOrder REAL;

      CREATE INDEX IF NOT EXISTS idx_sessions_project_sidebar_order
        ON sessions(projectId, sidebarOrder);

      PRAGMA user_version = 3;
    "#,
    },
    Migration {
        id: "0004_previous_session_history_quality",
        sql: r#"
      DELETE FROM sessions
      WHERE lifecycleState NOT IN ('running', 'sleeping')
        AND isPinned = 0
        AND isFavorite = 0
        AND lastActiveAt IS NULL
        AND (
          lifecycleState <> 'stopped'
          OR lower(trim(title)) IN (
            'terminal session',
            'amp cli session',
            'amp session',
            'antigravity cli session',
            'antigravity session',
            'claude session',
            'claude code session',
            'codebuddy session',
            'code buddy session',
            'codex session',
            'codex cli session',
            'copilot session',
            'cursor agent session',
            'cursor cli session',
            'cursor session',
            'droid session',
            'factory droid session',
            'gemini session',
            'grok session',
            'grok build session',
            'hermes session',
            'hermes agent session',
            'kiro session',
            'kiro cli session',
            'omp session',
            'opencode session',
            'open code session',
            'openai codex session',
            'pi session',
            'qoder session',
            'qodercli session',
            'rovo session',
            'rovo dev session',
            'rovodev session',
            'search by text'
          )
          OR trim(title) GLOB 'Session [0-9]*'
          OR trim(title) GLOB '👻*'
        );

      UPDATE sessions
      SET lastActiveAt = updatedAt
      WHERE lifecycleState NOT IN ('running', 'sleeping')
        AND lastActiveAt IS NULL;

      PRAGMA user_version = 4;
    "#,
    },
    Migration {
        id: "0005_session_tags",
        sql: r#"
      ALTER TABLE sessions ADD COLUMN sessionTag TEXT CHECK (
        sessionTag IS NULL OR sessionTag IN (
          'favorite',
          'high-priority',
          'research',
          'todo',
          'in-progress',
          'testing',
          'blocked',
          'low-priority',
          'on-hold',
          'done',
          'bug',
          'feature',
          'design'
        )
      );

      UPDATE sessions
      SET sessionTag = 'favorite'
      WHERE isFavorite = 1
        AND sessionTag IS NULL;

      PRAGMA user_version = 5;
    "#,
    },
    Migration {
        id: "0006_expand_session_tags",
        sql: rebuild_sessions_with_session_tag!("6"),
    },
    Migration {
        id: "0007_expand_session_tags_in_progress_and_type",
        sql: rebuild_sessions_with_session_tag!("7"),
    },
    Migration {
        id: "0008_remove_retired_session_type_tags",
        sql: rebuild_sessions_with_session_tag!("8"),
    },
    Migration {
        id: "0009_remove_legacy_zmux_chat_projects",
        sql: r#"
      DELETE FROM sessions
      WHERE projectId IN (
        SELECT projectId
        FROM projects
        WHERE path LIKE '%/zmux/chats/%'
          AND (
            name GLOB 'Chat [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] *'
            OR name IN ('Browser', 'Plugins')
          )
      );

      DELETE FROM projects
      WHERE path LIKE '%/zmux/chats/%'
        AND (
          name GLOB 'Chat [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] *'
          OR name IN ('Browser', 'Plugins')
        );

      PRAGMA user_version = 9;
    "#,
    },
    Migration {
        id: "0010_portless_persistence_model",
        sql: r#"
      CREATE TABLE IF NOT EXISTS portless_domain_identities (
        identityId INTEGER PRIMARY KEY,
        identityScope TEXT NOT NULL CHECK (identityScope IN ('project', 'worktree')),
        projectId TEXT NOT NULL,
        worktreeKey TEXT,
        projectSlug TEXT,
        worktreeSlug TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        CHECK (
          (
            identityScope = 'project'
            AND worktreeKey IS NULL
            AND projectSlug IS NOT NULL
            AND worktreeSlug IS NULL
          )
          OR (
            identityScope = 'worktree'
            AND worktreeKey IS NOT NULL
            AND projectSlug IS NULL
            AND worktreeSlug IS NOT NULL
          )
        ),
        FOREIGN KEY (projectId) REFERENCES projects(projectId) ON DELETE CASCADE
      );

      CREATE UNIQUE INDEX IF NOT EXISTS idx_portless_domain_project_identity
        ON portless_domain_identities(projectId)
        WHERE identityScope = 'project';

      CREATE UNIQUE INDEX IF NOT EXISTS idx_portless_domain_worktree_identity
        ON portless_domain_identities(projectId, worktreeKey)
        WHERE identityScope = 'worktree';

      CREATE UNIQUE INDEX IF NOT EXISTS idx_portless_domain_project_slug
        ON portless_domain_identities(projectSlug)
        WHERE identityScope = 'project';

      CREATE UNIQUE INDEX IF NOT EXISTS idx_portless_domain_worktree_slug
        ON portless_domain_identities(projectId, worktreeSlug)
        WHERE identityScope = 'worktree';

      CREATE TABLE IF NOT EXISTS portless_state (
        stateId TEXT PRIMARY KEY CHECK (stateId = 'global'),
        enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
        protocol TEXT NOT NULL CHECK (protocol IN ('https', 'http')),
        setupOwnership TEXT NOT NULL CHECK (setupOwnership IN ('unknown', 'missing', 'ghostex', 'standalone')),
        setupStatus TEXT NOT NULL CHECK (setupStatus IN ('unknown', 'needed', 'active', 'failed', 'disabled', 'postponed')),
        runtimeStatus TEXT NOT NULL CHECK (runtimeStatus IN ('unknown', 'inactive', 'active', 'failed')),
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      PRAGMA user_version = 10;
    "#,
    },
    Migration {
        id: "0011_t3_session_kind",
        /*
        CDXC:AgentProviders 2026-06-23-06:19:
        Embedded T3 panes now have gxserver-owned session identity. Rebuild the
        sessions table so existing state.db files accept kind=t3 rows without
        weakening the rest of the TypeScript-compatible session constraints.
        */
        sql: rebuild_sessions_with_session_tag!("11"),
    },
    Migration {
        id: "0012_recent_projects",
        /*
        CDXC:Projects 2026-06-24-12:27:
        Recent Projects is a first-class gxserver project-domain state. Store
        explicit parked state and closed time on the project row so GPUI can
        hydrate a real path-bearing recent list without deriving rows from
        labels, inactive sessions, shell titles, command text, or filesystem
        guesses.
        */
        sql: r#"
      ALTER TABLE projects ADD COLUMN isRecentProject INTEGER NOT NULL DEFAULT 0 CHECK (isRecentProject IN (0, 1));
      ALTER TABLE projects ADD COLUMN recentClosedAt TEXT;

      CREATE INDEX IF NOT EXISTS idx_projects_recent_closed
        ON projects(isRecentProject, recentClosedAt, updatedAt);

      PRAGMA user_version = 12;
    "#,
    },
    Migration {
        id: "0013_app_user_data",
        /*
        CDXC:ServerDaemon 2026-06-24-13:30:
        Scratch Pad and Pinned Prompts need a global gxserver-owned source of
        truth for reused React app-modal surfaces. Keep their user-authored
        bodies out of project/session metadata, presentation deltas, and logs by
        storing only the explicit app-user-data rows read by the product-data
        RPCs.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS app_user_data (
        itemKind TEXT NOT NULL CHECK (itemKind IN ('scratchPad', 'pinnedPrompt')),
        itemId TEXT NOT NULL,
        content TEXT NOT NULL,
        title TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        PRIMARY KEY (itemKind, itemId),
        CHECK (itemKind <> 'scratchPad' OR itemId = 'global'),
        CHECK (itemKind <> 'pinnedPrompt' OR content <> '')
      );

      CREATE INDEX IF NOT EXISTS idx_app_user_data_kind_updated
        ON app_user_data(itemKind, updatedAt, itemId);

      PRAGMA user_version = 13;
    "#,
    },
    Migration {
        id: "0014_automations",
        /*
        CDXC:Automations 2026-06-29-15:55:
        Project automations are daemon-owned instead of native-sidebar project-cache fields. Store definitions and run history in dedicated tables so macOS, CLI, GPUI, and remote clients control the same scheduler without renderer-local timers or duplicated CLI bridge state.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS automations (
        automationId TEXT PRIMARY KEY,
        projectId TEXT NOT NULL,
        agentId TEXT NOT NULL,
        name TEXT NOT NULL,
        prompt TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
        scheduleJson TEXT NOT NULL,
        executionModeJson TEXT NOT NULL,
        nextRunAt TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        FOREIGN KEY (projectId) REFERENCES projects(projectId) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_automations_project_updated
        ON automations(projectId, updatedAt);

      CREATE INDEX IF NOT EXISTS idx_automations_due
        ON automations(enabled, nextRunAt);

      CREATE TABLE IF NOT EXISTS automation_runs (
        runId TEXT PRIMARY KEY,
        automationId TEXT NOT NULL,
        projectId TEXT NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'findings', 'no_findings', 'failed', 'needs_attention', 'cancelled', 'skipped')),
        sessionId TEXT,
        worktreeJson TEXT NOT NULL DEFAULT '{}',
        errorMessage TEXT,
        findingsSummary TEXT,
        isArchived INTEGER NOT NULL DEFAULT 0 CHECK (isArchived IN (0, 1)),
        isUnread INTEGER NOT NULL DEFAULT 0 CHECK (isUnread IN (0, 1)),
        createdAt TEXT NOT NULL,
        completedAt TEXT,
        updatedAt TEXT NOT NULL,
        FOREIGN KEY (automationId) REFERENCES automations(automationId) ON DELETE CASCADE,
        FOREIGN KEY (projectId) REFERENCES projects(projectId) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_automation_runs_project_created
        ON automation_runs(projectId, createdAt);

      CREATE INDEX IF NOT EXISTS idx_automation_runs_active
        ON automation_runs(automationId, status);

      PRAGMA user_version = 14;
    "#,
    },
    Migration {
        id: "0015_project_visibility",
        /*
        CDXC:Projects 2026-06-30-21:23:
        Project visibility and system roles are gxserver domain state, not macOS sidebar-only filtering. Store hidden/system markers on project rows so mobile, CLI, GPUI, and macOS all omit Remote Attach carrier projects and other non-active project containers from shared inventory without client-specific project-name filters.
        */
        sql: r#"
      ALTER TABLE projects ADD COLUMN visibility TEXT NOT NULL DEFAULT 'visible' CHECK (visibility IN ('visible', 'hidden'));
      ALTER TABLE projects ADD COLUMN systemKind TEXT CHECK (systemKind IS NULL OR systemKind IN ('remoteAttachCarrier'));

      UPDATE projects
      SET visibility = 'hidden',
          systemKind = 'remoteAttachCarrier',
          isRecentProject = 0,
          recentClosedAt = NULL
      WHERE systemKind IS NULL
        AND trim(name) = 'Remote Attach'
        AND (
          trim(COALESCE(path, '')) LIKE '%/.ghostex/remote-attach-carriers'
          OR trim(COALESCE(path, '')) LIKE '%/.ghostex-dev/remote-attach-carriers'
        );

      CREATE INDEX IF NOT EXISTS idx_projects_visibility
        ON projects(visibility, systemKind, updatedAt);

      PRAGMA user_version = 15;
    "#,
    },
    Migration {
        id: "0016_session_settle_snooze_lifecycle",
        /*
        CDXC:StateSync 2026-07-29-00:00:
        Sidebar V2 settle/snooze is server-owned session state, so every client
        (GPUI, web, mobile, CLI, remote machines) reads one durable answer
        instead of deriving a private inbox. `settledOverrideAt` is the
        server-internal stamp for the current override: real activity newer than
        the stamp resets the override, which is how the event-driven
        "activity un-settles" rule is expressed against gxserver's activity
        clock. It is deliberately not published in presentation.
        Old state.db files simply get NULL columns, which is the "no lifecycle
        state" default the client predicates already expect.
        */
        sql: r#"
      ALTER TABLE sessions ADD COLUMN settledAt TEXT;
      ALTER TABLE sessions ADD COLUMN settledOverride TEXT CHECK (
        settledOverride IS NULL OR settledOverride IN ('settled', 'active')
      );
      ALTER TABLE sessions ADD COLUMN settledOverrideAt TEXT;
      ALTER TABLE sessions ADD COLUMN snoozedAt TEXT;
      ALTER TABLE sessions ADD COLUMN snoozedUntil TEXT;

      PRAGMA user_version = 16;
    "#,
    },
    Migration {
        id: "0017_stashed_prompts",
        /*
        CDXC:SavedPrompts 2026-07-29-00:00:
        Prompt stash entries are captured server-side when a prompt-editor
        save-and-close completes, so every client reads one durable queue.
        projectId/sessionId are soft references (no FK): a stash must outlive
        the project or session it was written from, because restoring an old
        prompt into a new project is the point of the feature. cwd records the
        worktree/checkout the prompt was composed in for scope display only.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS stashed_prompts (
        promptId TEXT PRIMARY KEY,
        content TEXT NOT NULL CHECK (content <> ''),
        projectId TEXT,
        sessionId TEXT,
        cwd TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_stashed_prompts_updated
        ON stashed_prompts(updatedAt);

      PRAGMA user_version = 17;
    "#,
    },
    Migration {
        id: "0018_global_sidebar_commands",
        /*
        CDXC:AgentLauncher 2026-08-01-16:00:
        Global Actions show the same action on every project, so they cannot
        live in projects.customCommandsJson the way Project Actions do. Store
        them daemon-side in their own table so mobile, web, and every desktop
        build read one list instead of mirroring a per-project column. The
        definition body is the same normalized stored-command shape Project
        Actions use; only ownership and ordering differ. Defaults
        (dev/build/test/setup) stay project-scoped, so there is no
        deletedDefaultCommandIds equivalent here and every row is user-created.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS global_sidebar_commands (
        commandId TEXT PRIMARY KEY,
        definitionJson TEXT NOT NULL,
        sortOrder REAL NOT NULL,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_global_sidebar_commands_order
        ON global_sidebar_commands(sortOrder, commandId);

      PRAGMA user_version = 18;
    "#,
    },
    Migration {
        id: "0019_remove_unsupported_session_kinds",
        sql: r#"
      DELETE FROM sessions
      WHERE kind NOT IN ('terminal', 'agent');

      CREATE TABLE sessions_next (
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('terminal', 'agent')),
        title TEXT NOT NULL,
        lifecycleState TEXT NOT NULL CHECK (lifecycleState IN ('running', 'sleeping', 'stopped', 'missing', 'unknown')),
        providerStateJson TEXT NOT NULL,
        zmxName TEXT NOT NULL,
        cwd TEXT,
        agentId TEXT,
        commandId TEXT,
        isPinned INTEGER NOT NULL DEFAULT 0 CHECK (isPinned IN (0, 1)),
        isFavorite INTEGER NOT NULL DEFAULT 0 CHECK (isFavorite IN (0, 1)),
        restoredFromSessionId TEXT,
        restoredFromHistoryId TEXT,
        launchSettingsJson TEXT NOT NULL DEFAULT '{}',
        runtimeSettingsJson TEXT NOT NULL DEFAULT '{}',
        completionRulesJson TEXT NOT NULL DEFAULT '{}',
        attentionRulesJson TEXT NOT NULL DEFAULT '{}',
        notificationRulesJson TEXT NOT NULL DEFAULT '{}',
        worktreeJson TEXT NOT NULL DEFAULT '{}',
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        lastActiveAt TEXT,
        sidebarOrder REAL,
        sessionTag TEXT CHECK (
          sessionTag IS NULL OR sessionTag IN (
            'favorite',
            'high-priority',
            'research',
            'todo',
            'in-progress',
            'testing',
            'blocked',
            'low-priority',
            'on-hold',
            'done',
            'bug',
            'feature',
            'design'
          )
        ),
        settledAt TEXT,
        settledOverride TEXT CHECK (
          settledOverride IS NULL OR settledOverride IN ('settled', 'active')
        ),
        settledOverrideAt TEXT,
        snoozedAt TEXT,
        snoozedUntil TEXT,
        PRIMARY KEY (projectId, sessionId),
        FOREIGN KEY (projectId) REFERENCES projects(projectId) ON DELETE CASCADE
      );

      INSERT INTO sessions_next (
        projectId,
        sessionId,
        kind,
        title,
        lifecycleState,
        providerStateJson,
        zmxName,
        cwd,
        agentId,
        commandId,
        isPinned,
        isFavorite,
        restoredFromSessionId,
        restoredFromHistoryId,
        launchSettingsJson,
        runtimeSettingsJson,
        completionRulesJson,
        attentionRulesJson,
        notificationRulesJson,
        worktreeJson,
        createdAt,
        updatedAt,
        lastActiveAt,
        sidebarOrder,
        sessionTag,
        settledAt,
        settledOverride,
        settledOverrideAt,
        snoozedAt,
        snoozedUntil
      )
      SELECT
        projectId,
        sessionId,
        kind,
        title,
        lifecycleState,
        providerStateJson,
        zmxName,
        cwd,
        agentId,
        commandId,
        isPinned,
        isFavorite,
        restoredFromSessionId,
        restoredFromHistoryId,
        launchSettingsJson,
        runtimeSettingsJson,
        completionRulesJson,
        attentionRulesJson,
        notificationRulesJson,
        worktreeJson,
        createdAt,
        updatedAt,
        lastActiveAt,
        sidebarOrder,
        sessionTag,
        settledAt,
        settledOverride,
        settledOverrideAt,
        snoozedAt,
        snoozedUntil
      FROM sessions;

      DROP TABLE sessions;
      ALTER TABLE sessions_next RENAME TO sessions;

      CREATE INDEX IF NOT EXISTS idx_sessions_project_updated
        ON sessions(projectId, updatedAt);

      CREATE INDEX IF NOT EXISTS idx_sessions_project_sidebar_order
        ON sessions(projectId, sidebarOrder);

      PRAGMA user_version = 19;
    "#,
    },
    Migration {
        id: "0020_delayed_sends",
        /*
        CDXC:DelayedSend 2026-08-17:
        Delayed Send is session automation, not renderer state. Keep one
        durable row beside the gxserver session it targets so the hosting
        daemon can re-arm it after either the desktop app or gxserver restarts.
        The row stores only canonical ids, trigger/deadline lifecycle, and an
        optional bounded failure reason; terminal input and content never enter
        this table.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS delayed_sends (
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        trigger TEXT NOT NULL CHECK (trigger IN ('timer', 'agentStops', 'allAgentsStop')),
        deadlineAt TEXT,
        nonWorkingSinceAt TEXT,
        state TEXT NOT NULL CHECK (state IN ('armed', 'firing', 'completed', 'failed', 'expired')),
        errorMessage TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        PRIMARY KEY (projectId, sessionId),
        FOREIGN KEY (projectId, sessionId) REFERENCES sessions(projectId, sessionId) ON DELETE CASCADE,
        CHECK (
          (trigger = 'timer' AND deadlineAt IS NOT NULL)
          OR (trigger IN ('agentStops', 'allAgentsStop') AND deadlineAt IS NULL)
        )
      );

      CREATE INDEX IF NOT EXISTS idx_delayed_sends_due
        ON delayed_sends(state, trigger, deadlineAt);

      PRAGMA user_version = 20;
    "#,
    },
    Migration {
        id: "0021_session_chat_queue",
        /*
        CDXC:SessionChat 2026-08-21:
        The Ghostex-owned chat prompt queue and the synced composer draft. Both
        are daemon-owned so the queue drains with every client closed and the
        same session opened on another device shows what was already typed.
        Deliberately no foreign key onto `sessions`: a queued prompt and a draft
        are text the USER typed, and losing it because a session row was pruned
        or re-created is exactly the failure this feature exists to prevent —
        the session is validated per request instead. `position` is dense and
        rewritten on reorder; `state` mirrors the wire contract in
        packages/shared/session-chat-queue.ts.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS session_chat_queued_prompts (
        promptId     TEXT PRIMARY KEY,
        projectId    TEXT NOT NULL,
        sessionId    TEXT NOT NULL,
        position     INTEGER NOT NULL,
        text         TEXT NOT NULL,
        state        TEXT NOT NULL DEFAULT 'queued' CHECK (
          state IN ('queued', 'sending', 'failed')
        ),
        errorMessage TEXT,
        createdAt    TEXT NOT NULL,
        updatedAt    TEXT NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_session_chat_queued_prompts_session
        ON session_chat_queued_prompts(projectId, sessionId, position);

      CREATE TABLE IF NOT EXISTS session_chat_drafts (
        projectId      TEXT NOT NULL,
        sessionId      TEXT NOT NULL,
        content        TEXT NOT NULL,
        originClientId TEXT NOT NULL,
        updatedAt      TEXT NOT NULL,
        PRIMARY KEY (projectId, sessionId)
      );

      PRAGMA user_version = 21;
    "#,
    },
    Migration {
        id: "0022_stashed_prompt_tags",
        /*
        CDXC:SavedPrompts 2026-08-23:
        Saved Prompts get user-defined tags, filtered from a pill rail above the
        list. Favorites is not a separate column: it is a seeded builtin tag row
        so the star, the Favorites pill, and a user tag all read and write the
        same link table instead of two parallel truths that can disagree.
        Deleting a tag only unfiles prompts (link cascade); the prompts survive.
        The link rows cascade off `stashed_prompts` too, so the 200-row recency
        cap in `save_stashed_prompt` cannot leave orphaned tag assignments.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS stashed_prompt_tags (
        tagId     TEXT PRIMARY KEY,
        name      TEXT NOT NULL CHECK (name <> ''),
        color     TEXT NOT NULL,
        isBuiltin INTEGER NOT NULL DEFAULT 0 CHECK (isBuiltin IN (0, 1)),
        sortOrder REAL NOT NULL,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS stashed_prompt_tag_links (
        promptId  TEXT NOT NULL REFERENCES stashed_prompts(promptId) ON DELETE CASCADE,
        tagId     TEXT NOT NULL REFERENCES stashed_prompt_tags(tagId) ON DELETE CASCADE,
        createdAt TEXT NOT NULL,
        PRIMARY KEY (promptId, tagId)
      );

      CREATE INDEX IF NOT EXISTS idx_stashed_prompt_tag_links_tag
        ON stashed_prompt_tag_links(tagId);

      INSERT OR IGNORE INTO stashed_prompt_tags (
        tagId, name, color, isBuiltin, sortOrder, createdAt, updatedAt
      ) VALUES (
        'favorite',
        'Favorites',
        '#e3b341',
        1,
        0,
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      );

      PRAGMA user_version = 22;
    "#,
    },
    Migration {
        id: "0023_session_parking",
        sql: r#"
      ALTER TABLE sessions ADD COLUMN isParked INTEGER NOT NULL DEFAULT 0 CHECK (isParked IN (0, 1));

      PRAGMA user_version = 23;
    "#,
    },
    Migration {
        id: "0024_stashed_prompt_tag",
        /*
        CDXC:SavedPrompts 2026-08-24:
        Stash actions file prompts under a durable builtin Stashed tag. Seed
        the catalogue and backfill existing stash rows so old and new Saved
        Prompts have the same filing behavior.
        */
        sql: r#"
      INSERT OR IGNORE INTO stashed_prompt_tags (
        tagId, name, color, isBuiltin, sortOrder, createdAt, updatedAt
      ) VALUES (
        'stashed',
        'Stashed',
        '#3b82f6',
        1,
        1,
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      );

      INSERT OR IGNORE INTO stashed_prompt_tag_links (promptId, tagId, createdAt)
      SELECT promptId, 'stashed', strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      FROM stashed_prompts;

      PRAGMA user_version = 24;
    "#,
    },
    Migration {
        id: "0025_session_agent_notes",
        /*
        CDXC:SessionNotes 2026-08-24:
        A session note is keyed by the AGENT session id (the provider resume
        id), not by the ghostex session id, so closing a session and resuming
        the same conversation later brings the note back with it. That is also
        why there is no FK onto `sessions`: the note must outlive the ghostex
        row it was written from, exactly like `stashed_prompts` above.
        agent/projectId/sessionId are soft debug references to the LAST writer
        and are never used as lookup keys.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS session_agent_notes (
        agentSessionId TEXT PRIMARY KEY CHECK (agentSessionId <> ''),
        note TEXT NOT NULL CHECK (note <> ''),
        agent TEXT,
        projectId TEXT,
        sessionId TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      PRAGMA user_version = 25;
    "#,
    },
    Migration {
        id: "0026_stashed_prompt_agent_session",
        /*
        CDXC:SavedPrompts 2026-08-24:
        A stashed prompt belongs to the agent CONVERSATION it was stashed from,
        not merely to the ghostex session row that happened to be open: Claude
        and Codex mint a new conversation id on compaction/resume, and the
        successor re-key in `apply_session_state_update` moves this column with
        it, exactly like `session_agent_notes`. Nullable and no FK, because a
        stash must outlive both the conversation and the session row.
        Deliberately no backfill: filling this from the sessions registry would
        need JSON1 to read `runtimeSettingsJson`, which a migration cannot
        assume is compiled in, so legacy rows stay NULL and
        `list_stashed_prompts` resolves them from the live registry at read
        time.
        */
        sql: r#"
      ALTER TABLE stashed_prompts ADD COLUMN agentSessionId TEXT;

      CREATE INDEX IF NOT EXISTS idx_stashed_prompts_agent_session
        ON stashed_prompts(agentSessionId);

      PRAGMA user_version = 26;
    "#,
    },
    Migration {
        id: "0027_tailcat_state",
        /*
        CDXC:RemotePairing 2026-09-01:
        Only the user's intent is durable: enabled, the served ports, and the
        client-key allow-list. The address blob is deliberately absent, because
        it is derived from the on-disk server key at runtime and a persisted
        copy would outlive a deleted key.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS tailcat_state (
        stateId TEXT PRIMARY KEY CHECK (stateId <> ''),
        enabled INTEGER NOT NULL,
        portsCsv TEXT NOT NULL,
        allowedClientKeysCsv TEXT NOT NULL,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );

      PRAGMA user_version = 27;
    "#,
    },
    Migration {
        id: "0028_remote_pairing",
        /*
        CDXC:RemotePairing 2026-09-03:
        One live pairing secret, stored only as a hash with its expiry, and
        the devices that registered through it. The device row keeps the SSH
        key fingerprint (to find and delete its `authorized_keys` line) and
        the optional Easy Connect client key (to undo the allow-list entry);
        `lastSeenAt` is bumped by the phone on each connect.
        */
        sql: r#"
      CREATE TABLE IF NOT EXISTS remote_pairing_secret (
        stateId TEXT PRIMARY KEY CHECK (stateId <> ''),
        secretHash TEXT NOT NULL,
        expiresAt TEXT NOT NULL,
        createdAt TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS remote_paired_devices (
        id TEXT PRIMARY KEY CHECK (id <> ''),
        name TEXT NOT NULL,
        platform TEXT NOT NULL,
        sshKeyFingerprint TEXT NOT NULL,
        tailcatClientKey TEXT,
        pairedAt TEXT NOT NULL,
        lastSeenAt TEXT
      );

      PRAGMA user_version = 28;
    "#,
    },
    Migration {
        id: "0029_session_chat_model_selections",
        sql: r#"
      CREATE TABLE IF NOT EXISTS session_chat_model_selections (
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        selectionId TEXT NOT NULL,
        model TEXT NOT NULL,
        effort TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'queued',
        errorMessage TEXT,
        retryAt INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (projectId, sessionId)
      );
      PRAGMA user_version = 29;
    "#,
    },
    Migration {
        id: "0030_session_chat_draft_versions",
        sql: r#"
      ALTER TABLE session_chat_drafts ADD COLUMN draftId TEXT;
      ALTER TABLE session_chat_drafts ADD COLUMN revision INTEGER;
      CREATE TABLE session_chat_draft_versions (
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        draftId TEXT NOT NULL,
        revision INTEGER NOT NULL,
        content TEXT NOT NULL,
        originClientId TEXT NOT NULL,
        updatedAt TEXT NOT NULL,
        consumed INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (projectId, sessionId, draftId)
      );
      PRAGMA user_version = 30;
    "#,
    },
    Migration {
        id: "0031_session_chat_selection_options",
        sql: r#"
      ALTER TABLE session_chat_model_selections ADD COLUMN options TEXT NOT NULL DEFAULT '{}';
      PRAGMA user_version = 31;
    "#,
    },
    Migration {
        id: "0032_session_chat_delivered_drafts",
        sql: r#"
      CREATE TABLE session_chat_delivered_drafts (
        id TEXT PRIMARY KEY,
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        content TEXT NOT NULL,
        deliveredAt TEXT NOT NULL
      );
      PRAGMA user_version = 32;
    "#,
    },
    Migration {
        id: "0033_session_chat_draft_recovery",
        sql: r#"
      ALTER TABLE session_chat_drafts ADD COLUMN parked INTEGER NOT NULL DEFAULT 0;
      CREATE TABLE session_chat_draft_recovery (
        projectId TEXT NOT NULL, sessionId TEXT NOT NULL, draftId TEXT NOT NULL,
        revision INTEGER NOT NULL, content TEXT NOT NULL, updatedAt TEXT NOT NULL,
        checkpoint INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (projectId,sessionId,draftId,revision)
      );
      INSERT INTO session_chat_draft_recovery(projectId,sessionId,draftId,revision,content,updatedAt)
        SELECT projectId,sessionId,draftId,revision,content,updatedAt FROM session_chat_draft_versions WHERE content<>'' AND revision>consumed;
      CREATE TABLE session_chat_draft_handoffs (
        id TEXT PRIMARY KEY, projectId TEXT NOT NULL, sessionId TEXT NOT NULL,
        content TEXT NOT NULL, draftId TEXT NOT NULL, revision INTEGER NOT NULL,
        direction TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending', updatedAt TEXT NOT NULL
      );
      CREATE INDEX session_chat_draft_handoffs_session ON session_chat_draft_handoffs(projectId,sessionId,updatedAt);
      PRAGMA user_version = 33;
    "#,
    },
    Migration {
        id: "0034_session_chat_startup_sends",
        sql: r#"
      ALTER TABLE session_chat_queued_prompts ADD COLUMN startupSend INTEGER NOT NULL DEFAULT 0;
      PRAGMA user_version = 34;
    "#,
    },
    Migration {
        id: "0035_notification_feed",
        sql: r#"
      CREATE TABLE notification_feed (
        id TEXT PRIMARY KEY,
        projectId TEXT NOT NULL,
        sessionId TEXT NOT NULL,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        subtitle TEXT NOT NULL,
        body TEXT NOT NULL,
        agentName TEXT,
        attentionEventId TEXT,
        createdAt TEXT NOT NULL,
        readAt TEXT,
        deferredAt TEXT
      );
      CREATE INDEX idx_notification_feed_session ON notification_feed(sessionId, createdAt);
      CREATE INDEX idx_notification_feed_read ON notification_feed(readAt, createdAt);
      PRAGMA user_version = 35;
    "#,
    },
    Migration {
        id: "0036_custom_session_tags",
        /*
        CDXC:Sessions 2026-09-11 WHY:
        The sessionTag CHECK listed the built-in tags by value, so a custom
        `custom-…` id could never be persisted. SQLite cannot alter a CHECK in
        place, and the usual table rebuild is off the table now: dropping
        `sessions` with foreign keys on cascade-deletes every `delayed_sends`
        row. Renaming the column, re-adding it with the wider CHECK, copying
        the values across, and dropping the old column keeps the table, its
        foreign keys, and its indexes intact; the only visible change is that
        sessionTag is now the last column, which nothing reads positionally.
        The GLOB only pins the prefix and character class; the exact id shape
        is enforced by normalize_optional_session_tag.
        */
        sql: r#"
      ALTER TABLE sessions RENAME COLUMN sessionTag TO sessionTagLegacy;
      ALTER TABLE sessions ADD COLUMN sessionTag TEXT CHECK (
        sessionTag IS NULL OR sessionTag IN (
          'favorite',
          'high-priority',
          'research',
          'todo',
          'in-progress',
          'testing',
          'blocked',
          'low-priority',
          'on-hold',
          'done',
          'bug',
          'feature',
          'design'
        ) OR sessionTag GLOB 'custom-[a-z0-9]*'
      );
      UPDATE sessions SET sessionTag = sessionTagLegacy;
      ALTER TABLE sessions DROP COLUMN sessionTagLegacy;
      PRAGMA user_version = 36;
    "#,
    },
    Migration {
        id: "0037_session_fork_revision",
        sql: include_str!("migrations/0037_session_fork_revision.sql"),
    },
    Migration {
        id: "0038_delayed_send_watched_session",
        sql: include_str!("migrations/0038_delayed_send_watched_session.sql"),
    },
    Migration {
        id: "0039_session_chat_selection_scope",
        sql: include_str!("migrations/0039_session_chat_selection_scope.sql"),
    },
    Migration {
        id: "0040_prune_consumed_draft_recovery",
        sql: include_str!("migrations/0040_prune_consumed_draft_recovery.sql"),
    },
    Migration {
        id: "0041_coordinators",
        sql: include_str!("migrations/0041_coordinators.sql"),
    },
    Migration {
        id: "0042_coordinator_pending_messages",
        sql: include_str!("migrations/0042_coordinator_pending_messages.sql"),
    },
    Migration {
        id: "0043_session_chat_send_requests",
        sql: include_str!("migrations/0043_session_chat_send_requests.sql"),
    },
    Migration {
        id: "0044_session_chat_queued_images",
        sql: include_str!("migrations/0044_session_chat_queued_images.sql"),
    },
    Migration {
        id: "0045_agent_message_deliveries",
        sql: include_str!("migrations/0045_agent_message_deliveries.sql"),
    },
    Migration {
        id: "0046_session_lifecycle_index",
        sql: include_str!("migrations/0046_session_lifecycle_index.sql"),
    },
    Migration {
        id: "0047_session_presentation_kept_index",
        sql: include_str!("migrations/0047_session_presentation_kept_index.sql"),
    },
    Migration {
        id: "0048_session_chat_resources",
        sql: include_str!("migrations/0048_session_chat_resources.sql"),
    },
];
