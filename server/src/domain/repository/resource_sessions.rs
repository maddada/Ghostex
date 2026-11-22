//! Resource ownership is independent of sidebar visibility and provider probe caches.

use crate::domain::{sql_error, DomainRepository, DomainResult};
use crate::ids::create_zmx_session_name;
use serde_json::{json, Value};
use std::collections::HashSet;

impl DomainRepository<'_> {
    /// CDXC:Resources 2026-09-07 DECISION:
    /// User: "we shouldn't have orphaned ones". Resolve live zmx names against durable ownership, including Recent Projects, parked sessions and command terminals that the sidebar omits.
    /// This narrow read avoids hydrating transcript and launch metadata for every historical session when Resources opens.
    pub fn resource_session_owners(&self, names: &HashSet<String>) -> DomainResult<Vec<Value>> {
        let mut statement = self.db.prepare(
            "SELECT s.projectId, s.sessionId, s.title, p.name FROM sessions s JOIN projects p ON p.projectId = s.projectId ORDER BY s.sidebarOrder, s.sessionId"
        ).map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut owners = Vec::new();
        for row in rows {
            let (project_id, session_id, title, project_title) = row.map_err(sql_error)?;
            let name = create_zmx_session_name(&self.server_id, &project_id, &session_id);
            if names.contains(&name) {
                owners.push(json!({"projectId": project_id, "sessionId": session_id, "title": title, "projectTitle": project_title, "zmxName": name}));
            }
        }
        Ok(owners)
    }
}
