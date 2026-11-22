//! When each agent launcher was last used, for the Settings › Agents roster.

use crate::domain::{sql_error, DomainRepository, DomainResult};
use std::collections::HashMap;

impl DomainRepository<'_> {
    /// The newest activity time of every agent that ever had a session, keyed by both the
    /// session's `agentId` and the launcher it was started from (`launchAgentId`), so a custom
    /// launcher counts as used even after its session was identified as the CLI it runs.
    /// Stopped sessions count: an agent with old sessions was used before.
    pub fn agent_launcher_last_used(&self) -> DomainResult<HashMap<String, String>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT agentId,
                       json_extract(runtimeSettingsJson, '$.launchAgentId'),
                       MAX(COALESCE(lastActiveAt, updatedAt, createdAt))
                FROM sessions
                WHERE kind = 'agent'
                GROUP BY 1, 2
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(sql_error)?;
        let mut last_used: HashMap<String, String> = HashMap::new();
        for row in rows {
            let (agent_id, launch_agent_id, at) = row.map_err(sql_error)?;
            let Some(at) = at else { continue };
            for id in [agent_id, launch_agent_id].into_iter().flatten() {
                let id = id.trim();
                if id.is_empty() {
                    continue;
                }
                let newer = last_used
                    .get(id)
                    .is_none_or(|previous| previous.as_str() < at.as_str());
                if newer {
                    last_used.insert(id.to_string(), at.clone());
                }
            }
        }
        Ok(last_used)
    }
}
