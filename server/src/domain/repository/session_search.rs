use serde_json::Value;

use crate::domain::{
    session_from_row, session_row_from_sql, sql_error, DomainRepository, DomainResult,
};

impl DomainRepository<'_> {
    /// CDXC:Sessions 2026-09-11 WHY:
    /// Search and Previous Sessions need the whole registry for fork supersession, but hydrating unrelated runtime payloads and rules on every keystroke dominates large histories.
    /// Project only the launch and runtime keys their filters, titles, results and fork families consume, and retain the canonical row normalizer for tags, surface, IDs and provider lifecycle.
    /// Unused objects are still checked for valid JSON object shape so malformed JSON and non-object columns continue to fail canonical hydration.
    /// SEE-ALSO: presentation/search.rs, presentation/session_attributes.rs, presentation/fork_family.rs.
    pub fn list_session_search_rows(&self) -> DomainResult<Vec<Value>> {
        let mut statement = self.db.prepare_cached(
            r#"
                SELECT projectId, sessionId, kind, title, lifecycleState, zmxName, cwd, agentId, commandId, isPinned, isParked, isFavorite, restoredFromSessionId, restoredFromHistoryId, createdAt, updatedAt, lastActiveAt, sidebarOrder, sessionTag, settledAt, settledOverride, settledOverrideAt, snoozedAt, snoozedUntil,
                       providerStateJson,
                       CASE WHEN NOT json_valid(launchSettingsJson) THEN launchSettingsJson
                            WHEN json_type(launchSettingsJson) <> 'object' THEN launchSettingsJson
                            ELSE (SELECT json_group_object(key, json(launchSettingsJson -> fullkey))
                                FROM json_each(launchSettingsJson)
                                WHERE key IN ('surface', 'icon', 'forkedFromSessionId', 'agentCommand'))
                       END AS launchSettingsJson,
                       CASE WHEN NOT json_valid(runtimeSettingsJson) THEN runtimeSettingsJson
                            WHEN json_type(runtimeSettingsJson) <> 'object' THEN runtimeSettingsJson
                            ELSE (SELECT json_group_object(key, json(runtimeSettingsJson -> fullkey))
                                FROM json_each(runtimeSettingsJson)
                                WHERE key IN (
                                    'agentName', 'agentSessionId', 'agentSessionPath',
                                    'externalSession', 'surface', 'titleSource', 'restoreTitleSource',
                                    'forkFirstPromptAutoTitlePending', 'forkedFromSessionId',
                                    'previousAgentSessionIds', 'sessionPersistenceProvider',
                                    'sessionPersistenceName'))
                       END AS runtimeSettingsJson,
                       CASE WHEN NOT json_valid(completionRulesJson) THEN completionRulesJson
                            WHEN json_type(completionRulesJson) = 'object' THEN '{}'
                            ELSE completionRulesJson END AS completionRulesJson,
                       CASE WHEN NOT json_valid(attentionRulesJson) THEN attentionRulesJson
                            WHEN json_type(attentionRulesJson) = 'object' THEN '{}'
                            ELSE attentionRulesJson END AS attentionRulesJson,
                       CASE WHEN NOT json_valid(notificationRulesJson) THEN notificationRulesJson
                            WHEN json_type(notificationRulesJson) = 'object' THEN '{}'
                            ELSE notificationRulesJson END AS notificationRulesJson,
                       CASE WHEN NOT json_valid(worktreeJson) THEN worktreeJson
                            WHEN json_type(worktreeJson) = 'object' THEN '{}'
                            ELSE worktreeJson END AS worktreeJson
                FROM sessions
                ORDER BY updatedAt DESC, projectId ASC, sessionId ASC
            "#,
        ).map_err(sql_error)?;
        let rows = statement
            .query_map([], session_row_from_sql)
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| session_from_row(&self.server_id, row))
            .collect()
    }
}
