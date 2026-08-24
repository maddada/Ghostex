use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::repository::project::MAX_ID_GENERATION_ATTEMPTS;
use crate::domain::{
    now_iso, optional_trimmed_string_param, required_string_param, sql_error, DomainRepository,
    DomainResult, DomainStateError,
};

impl<'a> DomainRepository<'a> {
    pub fn read_app_user_data(&self) -> DomainResult<Value> {
        /*
        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Scratch Pad and Pinned Prompts hydrate through one gxserver-owned
        product-data snapshot. Return only the exact shared React fields and do
        not derive values from GPUI product-state files, presentation labels,
        terminal text, project paths, command text, URLs, or logs.
        */
        read_app_user_data_state(self.db)
    }

    pub fn save_scratch_pad(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Scratch Pad autosave stores freeform user text in gxserver state and
        returns the refreshed app-user-data snapshot. The daemon must not log or
        echo note content outside the authenticated RPC response.
        */
        let content = required_string_param(params, "content")?;
        let timestamp = now_iso();
        let created_at = self
            .db
            .query_row(
                "SELECT createdAt FROM app_user_data WHERE itemKind = 'scratchPad' AND itemId = 'global'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .unwrap_or_else(|| timestamp.clone());
        self.db
            .execute(
                r#"
                INSERT INTO app_user_data (
                  itemKind, itemId, content, title, createdAt, updatedAt
                ) VALUES (
                  'scratchPad', 'global', ?1, NULL, ?2, ?3
                )
                ON CONFLICT(itemKind, itemId) DO UPDATE SET
                  content = excluded.content,
                  updatedAt = excluded.updatedAt
                "#,
                params![content, created_at, timestamp],
            )
            .map_err(sql_error)?;
        read_app_user_data_state(self.db)
    }

    pub fn save_pinned_prompt(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Pinned Prompt saves mirror the existing SidebarPinnedPrompt behavior:
        create or update by promptId, preserve createdAt, stamp updatedAt on
        save, normalize empty titles from content, and treat an empty content
        save as removing that prompt instead of storing an unusable row.
        */
        let content = required_string_param(params, "content")?;
        let title = required_string_param(params, "title")?;
        let supplied_prompt_id = optional_trimmed_string_param(params, "promptId")?;
        if content.is_empty() {
            if let Some(prompt_id) = supplied_prompt_id {
                self.db
                    .execute(
                        "DELETE FROM app_user_data WHERE itemKind = 'pinnedPrompt' AND itemId = ?1",
                        [prompt_id],
                    )
                    .map_err(sql_error)?;
            }
            return read_app_user_data_state(self.db);
        }

        let prompt_id = match supplied_prompt_id {
            Some(prompt_id) => prompt_id,
            None => create_unique_app_pinned_prompt_id(self.db)?,
        };
        let timestamp = now_iso();
        let created_at = self
            .db
            .query_row(
                "SELECT createdAt FROM app_user_data WHERE itemKind = 'pinnedPrompt' AND itemId = ?1",
                [&prompt_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .unwrap_or_else(|| timestamp.clone());
        let normalized_title = normalize_app_pinned_prompt_title(title, content);
        self.db
            .execute(
                r#"
                INSERT INTO app_user_data (
                  itemKind, itemId, content, title, createdAt, updatedAt
                ) VALUES (
                  'pinnedPrompt', ?1, ?2, ?3, ?4, ?5
                )
                ON CONFLICT(itemKind, itemId) DO UPDATE SET
                  content = excluded.content,
                  title = excluded.title,
                  updatedAt = excluded.updatedAt
                "#,
                params![prompt_id, content, normalized_title, created_at, timestamp],
            )
            .map_err(sql_error)?;
        read_app_user_data_state(self.db)
    }

    pub fn save_stashed_prompt(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:StashedPrompts 2026-07-29-00:00:
        Stash saves are fired best-effort by the prompt-editor CLI after every
        save-and-close, so the same text can arrive repeatedly. Re-saving
        content that already exists for the same project bumps that row's
        updatedAt instead of inserting a duplicate, and the queue is capped by
        recency. Prompt bodies must never be logged or echoed outside the
        authenticated RPC response.
        */
        let content = required_string_param(params, "content")?;
        if content.trim().is_empty() {
            return Err(DomainStateError::bad_request("content must not be empty."));
        }
        if content.chars().count() > MAX_STASHED_PROMPT_CONTENT_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "content must be at most {MAX_STASHED_PROMPT_CONTENT_CHARS} characters."
            )));
        }
        let project_id = optional_trimmed_string_param(params, "projectId")?;
        let requested_prompt_id = optional_trimmed_string_param(params, "promptId")?;
        let session_id = optional_trimmed_string_param(params, "sessionId")?;
        let cwd = optional_trimmed_string_param(params, "cwd")?;
        let timestamp = now_iso();
        if let Some(prompt_id) = requested_prompt_id {
            let updated = self
                .db
                .execute(
                    r#"
                    UPDATE stashed_prompts
                    SET content = ?2,
                        updatedAt = ?3
                    WHERE promptId = ?1
                    "#,
                    params![prompt_id, content, timestamp],
                )
                .map_err(sql_error)?;
            if updated == 0 {
                return Err(DomainStateError::not_found("Saved prompt does not exist."));
            }
            let prompt = read_stashed_prompt_row(self.db, &prompt_id)?.ok_or_else(|| {
                DomainStateError::corrupt_state("Saved prompt vanished during update.")
            })?;
            return Ok(json!({ "created": false, "prompt": prompt }));
        }
        let existing_prompt_id: Option<String> = self
            .db
            .query_row(
                r#"
                SELECT promptId FROM stashed_prompts
                WHERE content = ?1 AND COALESCE(projectId, '') = COALESCE(?2, '')
                ORDER BY updatedAt DESC
                LIMIT 1
                "#,
                params![content, project_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let (prompt_id, created) = match existing_prompt_id {
            Some(prompt_id) => {
                self.db
                    .execute(
                        r#"
                        UPDATE stashed_prompts
                        SET sessionId = COALESCE(?2, sessionId),
                            cwd = COALESCE(?3, cwd),
                            updatedAt = ?4
                        WHERE promptId = ?1
                        "#,
                        params![prompt_id, session_id, cwd, timestamp],
                    )
                    .map_err(sql_error)?;
                (prompt_id, false)
            }
            None => {
                let prompt_id = create_unique_stashed_prompt_id(self.db)?;
                self.db
                    .execute(
                        r#"
                        INSERT INTO stashed_prompts (
                          promptId, content, projectId, sessionId, cwd, createdAt, updatedAt
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                        "#,
                        params![prompt_id, content, project_id, session_id, cwd, timestamp],
                    )
                    .map_err(sql_error)?;
                self.db
                    .execute(
                        r#"
                        DELETE FROM stashed_prompts
                        WHERE promptId NOT IN (
                          SELECT promptId FROM stashed_prompts
                          ORDER BY updatedAt DESC, promptId DESC
                          LIMIT ?1
                        )
                        "#,
                        params![MAX_STASHED_PROMPTS],
                    )
                    .map_err(sql_error)?;
                (prompt_id, true)
            }
        };
        let prompt = read_stashed_prompt_row(self.db, &prompt_id)?.ok_or_else(|| {
            DomainStateError::corrupt_state("Stashed prompt vanished during save.")
        })?;
        Ok(json!({ "created": created, "prompt": prompt }))
    }

    pub fn list_stashed_prompts(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:StashedPrompts 2026-07-29-00:00:
        The default modal scope is "this project and its worktrees". Current
        worktree sessions already carry the parent projectId, and legacy
        worktree checkouts registered as their own project carry
        worktree.parentProjectId, so the family of a projectId is: its root
        (itself, or its parent for a legacy worktree project) plus every
        project whose worktree.parentProjectId is that root.
        */
        let scope_project_id = optional_trimmed_string_param(params, "projectId")?;
        let family = match scope_project_id {
            Some(project_id) => Some(stashed_prompt_project_family(self.db, &project_id)?),
            None => None,
        };
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT s.promptId, s.content, s.projectId, s.sessionId, s.cwd,
                       s.createdAt, s.updatedAt, p.name, p.identityIconJson,
                       p.path, p.worktreeJson
                FROM stashed_prompts s
                LEFT JOIN projects p ON p.projectId = s.projectId
                ORDER BY s.updatedAt DESC, s.promptId DESC
                "#,
            )
            .map_err(sql_error)?;
        let prompts = statement
            .query_map([], stashed_prompt_json_from_row)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?
            .into_iter()
            .filter(|prompt| match &family {
                None => true,
                Some(family) => prompt
                    .get("projectId")
                    .and_then(Value::as_str)
                    .is_some_and(|project_id| family.contains(project_id)),
            })
            .collect::<Vec<_>>();
        /*
        CDXC:StashedPromptTags 2026-08-23:
        The modal needs the tag catalogue and every row's assignments in the
        same paint as the prompts, otherwise the pill rail and the row chips
        render a frame apart and the counts visibly jump. One list call answers
        all three.
        */
        let tag_ids_by_prompt = read_stashed_prompt_tag_ids(self.db)?;
        let prompts = prompts
            .into_iter()
            .map(|mut prompt| {
                let prompt_id = prompt
                    .get("promptId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if let Some(object) = prompt.as_object_mut() {
                    object.insert(
                        "tagIds".to_string(),
                        json!(tag_ids_by_prompt
                            .get(&prompt_id)
                            .cloned()
                            .unwrap_or_default()),
                    );
                }
                prompt
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "prompts": prompts,
            "tags": read_stashed_prompt_tags(self.db)?,
        }))
    }

    pub fn list_stashed_prompt_tags(&self) -> DomainResult<Value> {
        Ok(json!({ "tags": read_stashed_prompt_tags(self.db)? }))
    }

    /*
    CDXC:StashedPromptTags 2026-08-23:
    Create or rename a tag. Names are compared case-insensitively so a second
    "Release" cannot shadow the first in the rail; a colliding create returns
    the tag that already exists instead of erroring, because the user's intent
    ("I want a Release tag") is already satisfied.
    */
    pub fn save_stashed_prompt_tag(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let name = normalize_stashed_prompt_tag_name(required_string_param(params, "name")?)?;
        let color = normalize_stashed_prompt_tag_color(
            optional_trimmed_string_param(params, "color")?.as_deref(),
        )?;
        let requested_tag_id = optional_trimmed_string_param(params, "tagId")?;
        let timestamp = now_iso();

        let conflicting_tag_id: Option<String> = self
            .db
            .query_row(
                r#"
                SELECT tagId FROM stashed_prompt_tags
                WHERE lower(name) = lower(?1) AND tagId <> COALESCE(?2, '')
                LIMIT 1
                "#,
                params![name, requested_tag_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;

        let tag_id = match (requested_tag_id, conflicting_tag_id) {
            (Some(tag_id), _) => {
                let updated = self
                    .db
                    .execute(
                        r#"
                        UPDATE stashed_prompt_tags
                        SET name = ?2, color = ?3, updatedAt = ?4
                        WHERE tagId = ?1
                        "#,
                        params![tag_id, name, color, timestamp],
                    )
                    .map_err(sql_error)?;
                if updated == 0 {
                    return Err(DomainStateError::not_found("Tag does not exist."));
                }
                tag_id
            }
            (None, Some(existing_tag_id)) => existing_tag_id,
            (None, None) => {
                let tag_id = create_unique_stashed_prompt_tag_id(self.db)?;
                self.db
                    .execute(
                        r#"
                        INSERT INTO stashed_prompt_tags (
                          tagId, name, color, isBuiltin, sortOrder, createdAt, updatedAt
                        ) VALUES (
                          ?1, ?2, ?3, 0,
                          (SELECT COALESCE(MAX(sortOrder), 0) + 1 FROM stashed_prompt_tags),
                          ?4, ?4
                        )
                        "#,
                        params![tag_id, name, color, timestamp],
                    )
                    .map_err(sql_error)?;
                tag_id
            }
        };

        let tag = read_stashed_prompt_tag_row(self.db, &tag_id)?
            .ok_or_else(|| DomainStateError::corrupt_state("Tag vanished during save."))?;
        Ok(json!({
            "tag": tag,
            "tags": read_stashed_prompt_tags(self.db)?,
        }))
    }

    /*
    CDXC:StashedPromptTags 2026-08-23:
    Deleting a tag unfiles every prompt that carried it (the link rows cascade)
    and never deletes a prompt. Favorites is builtin and stays, because the star
    control on every row has nowhere to write once it is gone.
    */
    pub fn delete_stashed_prompt_tag(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let tag_id = required_string_param(params, "tagId")?;
        let is_builtin: Option<bool> = self
            .db
            .query_row(
                "SELECT isBuiltin FROM stashed_prompt_tags WHERE tagId = ?1",
                [tag_id],
                |row| row.get::<_, i64>(0).map(|value| value != 0),
            )
            .optional()
            .map_err(sql_error)?;
        if is_builtin == Some(true) {
            return Err(DomainStateError::bad_request(
                "Favorites is a built-in tag and cannot be deleted.",
            ));
        }
        let deleted = self
            .db
            .execute("DELETE FROM stashed_prompt_tags WHERE tagId = ?1", [tag_id])
            .map_err(sql_error)?;
        Ok(json!({
            "deleted": deleted > 0,
            "tags": read_stashed_prompt_tags(self.db)?,
        }))
    }

    /*
    CDXC:StashedPromptTags 2026-08-23:
    One prompt's whole tag set is replaced in a single call rather than
    toggled one link at a time: the modal already knows the set it wants, and a
    replace cannot drift out of sync the way a stream of toggles can when two
    clients star the same prompt at once.
    */
    pub fn set_stashed_prompt_tags(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let prompt_id = required_string_param(params, "promptId")?.to_string();
        let requested_tag_ids = match params.get("tagIds") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|tag_id| !tag_id.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>(),
            None | Some(Value::Null) => Vec::new(),
            Some(_) => {
                return Err(DomainStateError::bad_request(
                    "tagIds must be an array of strings.",
                ))
            }
        };
        let prompt_exists: bool = self
            .db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM stashed_prompts WHERE promptId = ?1)",
                [&prompt_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !prompt_exists {
            return Err(DomainStateError::not_found("Saved prompt does not exist."));
        }

        let known_tag_ids = read_stashed_prompt_tags(self.db)?
            .iter()
            .filter_map(|tag| tag.get("tagId").and_then(Value::as_str).map(str::to_string))
            .collect::<HashSet<_>>();
        let timestamp = now_iso();

        /*
        Replacing a set is a delete plus inserts, so it takes the writer
        reservation first: a failure partway would otherwise leave the prompt
        with the old links stripped and the new ones missing — an untagged
        prompt the user never asked for.
        */
        self.db
            .execute_batch("BEGIN IMMEDIATE TRANSACTION")
            .map_err(sql_error)?;
        let result = (|| -> DomainResult<()> {
            self.db
                .execute(
                    "DELETE FROM stashed_prompt_tag_links WHERE promptId = ?1",
                    [&prompt_id],
                )
                .map_err(sql_error)?;
            let mut applied: HashSet<&str> = HashSet::new();
            for tag_id in &requested_tag_ids {
                if !known_tag_ids.contains(tag_id) || !applied.insert(tag_id.as_str()) {
                    continue;
                }
                self.db
                    .execute(
                        r#"
                        INSERT INTO stashed_prompt_tag_links (promptId, tagId, createdAt)
                        VALUES (?1, ?2, ?3)
                        "#,
                        params![prompt_id, tag_id, timestamp],
                    )
                    .map_err(sql_error)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(error) = self.db.execute_batch("COMMIT") {
                    let _ = self.db.execute_batch("ROLLBACK");
                    return Err(sql_error(error));
                }
            }
            Err(error) => {
                let _ = self.db.execute_batch("ROLLBACK");
                return Err(error);
            }
        }

        let prompt = read_stashed_prompt_row(self.db, &prompt_id)?.ok_or_else(|| {
            DomainStateError::corrupt_state("Saved prompt vanished while tagging.")
        })?;
        Ok(json!({ "prompt": prompt }))
    }

    pub fn delete_stashed_prompt(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let prompt_id = required_string_param(params, "promptId")?;
        let deleted = self
            .db
            .execute(
                "DELETE FROM stashed_prompts WHERE promptId = ?1",
                [prompt_id],
            )
            .map_err(sql_error)?;
        Ok(json!({ "deleted": deleted > 0 }))
    }

    /*
    CDXC:GlobalActions 2026-08-01-16:00:
    Global Actions are daemon-owned rather than project-owned, so every client
    reads one list instead of a per-project column. Rows come back in sortOrder
    with commandId as the tiebreak, so two actions saved in the same tick still
    order deterministically across reads.
    */
    pub fn list_global_sidebar_commands(&self) -> DomainResult<Vec<Value>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT definitionJson FROM global_sidebar_commands
                ORDER BY sortOrder ASC, commandId ASC
                "#,
            )
            .map_err(sql_error)?;
        let stored_definitions = statement
            .query_map([], |row| row.get::<_, String>("definitionJson"))
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        Ok(stored_definitions
            .into_iter()
            .filter_map(|definition| serde_json::from_str::<Value>(&definition).ok())
            .collect())
    }

    pub fn save_global_sidebar_command(
        &self,
        command_id: &str,
        definition: &Value,
    ) -> DomainResult<()> {
        let timestamp = now_iso();
        let definition_json = serde_json::to_string(definition).map_err(|error| {
            DomainStateError::corrupt_state(format!(
                "Global action definition could not be serialized: {error}"
            ))
        })?;
        /*
        A new action lands after everything currently stored; an edit keeps the
        position it already had. COALESCE over MAX gives the first row
        sortOrder 1 without a separate empty-table branch.
        */
        self.db
            .execute(
                r#"
                INSERT INTO global_sidebar_commands (
                  commandId, definitionJson, sortOrder, createdAt, updatedAt
                )
                VALUES (
                  ?1,
                  ?2,
                  COALESCE(
                    (SELECT sortOrder FROM global_sidebar_commands WHERE commandId = ?1),
                    (SELECT COALESCE(MAX(sortOrder), 0) + 1 FROM global_sidebar_commands)
                  ),
                  ?3,
                  ?3
                )
                ON CONFLICT(commandId) DO UPDATE SET
                  definitionJson = excluded.definitionJson,
                  updatedAt = excluded.updatedAt
                "#,
                params![command_id, definition_json, timestamp],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn delete_global_sidebar_command(&self, command_id: &str) -> DomainResult<()> {
        self.db
            .execute(
                "DELETE FROM global_sidebar_commands WHERE commandId = ?1",
                [command_id],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    /*
    Reorder assigns positions from the ids the client sent, in order. Ids the
    client did not mention keep their relative order after the listed ones
    rather than being dropped, so a client on an older list cannot silently
    delete an action it had not loaded yet.
    */
    pub fn order_global_sidebar_commands(&self, command_ids: &[String]) -> DomainResult<()> {
        let timestamp = now_iso();
        /*
        One reorder is many row writes, so it takes the writer reservation before
        reading the stored ids and commits or rolls back as a unit, mirroring
        `update_session_order`. Without the transaction a failure partway leaves
        some rows on the new sortOrder and some on the old — an interleaved list
        that the server would then echo back as the confirmed order. Taking
        BEGIN IMMEDIATE before the SELECT also closes the read-then-write race
        against a concurrent save or delete, which would otherwise keep a stale
        sortOrder that this reorder never accounted for.
        */
        self.db
            .execute_batch("BEGIN IMMEDIATE TRANSACTION")
            .map_err(sql_error)?;
        let result = (|| -> DomainResult<()> {
            let stored_ids = {
                let mut statement = self
                    .db
                    .prepare(
                        r#"
                        SELECT commandId FROM global_sidebar_commands
                        ORDER BY sortOrder ASC, commandId ASC
                        "#,
                    )
                    .map_err(sql_error)?;
                let stored_ids = statement
                    .query_map([], |row| row.get::<_, String>("commandId"))
                    .map_err(sql_error)?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(sql_error)?;
                stored_ids
            };
            /*
            A repeated id would otherwise consume two index positions — the
            append guard below skips the duplicate, but the write loop would
            still assign it twice — so requested ids are deduplicated first.
            */
            let mut next_order: Vec<String> = Vec::with_capacity(stored_ids.len());
            for command_id in command_ids {
                if stored_ids.contains(command_id) && !next_order.contains(command_id) {
                    next_order.push(command_id.clone());
                }
            }
            for command_id in stored_ids {
                if !next_order.contains(&command_id) {
                    next_order.push(command_id);
                }
            }
            for (index, command_id) in next_order.iter().enumerate() {
                self.db
                    .execute(
                        r#"
                        UPDATE global_sidebar_commands
                        SET sortOrder = ?2, updatedAt = ?3
                        WHERE commandId = ?1
                        "#,
                        params![command_id, (index + 1) as f64, timestamp],
                    )
                    .map_err(sql_error)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(error) = self.db.execute_batch("COMMIT") {
                    let _ = self.db.execute_batch("ROLLBACK");
                    return Err(sql_error(error));
                }
                Ok(())
            }
            Err(error) => {
                let _ = self.db.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }
}

fn read_app_user_data_state(db: &Connection) -> DomainResult<Value> {
    let scratch_pad_content = db
        .query_row(
            "SELECT content FROM app_user_data WHERE itemKind = 'scratchPad' AND itemId = 'global'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .unwrap_or_default();
    let mut statement = db
        .prepare(
            r#"
            SELECT itemId, content, title, createdAt, updatedAt
            FROM app_user_data
            WHERE itemKind = 'pinnedPrompt'
            ORDER BY updatedAt DESC, itemId ASC
            "#,
        )
        .map_err(sql_error)?;
    let pinned_prompts = statement
        .query_map([], |row| {
            let prompt_id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let created_at: String = row.get(3)?;
            let updated_at: String = row.get(4)?;
            let normalized_title =
                normalize_app_pinned_prompt_title(title.as_deref().unwrap_or(""), &content);
            Ok(json!({
                "content": content,
                "createdAt": created_at,
                "promptId": prompt_id,
                "title": normalized_title,
                "updatedAt": updated_at,
            }))
        })
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(json!({
        "pinnedPrompts": pinned_prompts,
        "scratchPadContent": scratch_pad_content,
    }))
}

fn create_unique_app_pinned_prompt_id(db: &Connection) -> DomainResult<String> {
    let millis = chrono::Utc::now().timestamp_millis();
    for attempt in 0..MAX_ID_GENERATION_ATTEMPTS {
        let candidate = if attempt == 0 {
            format!("gxserver-prompt-{millis}")
        } else {
            format!("gxserver-prompt-{millis}-{attempt}")
        };
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM app_user_data WHERE itemKind = 'pinnedPrompt' AND itemId = ?1)",
                [&candidate],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(DomainStateError::corrupt_state(
        "Could not allocate a unique pinned prompt id.",
    ))
}

fn normalize_app_pinned_prompt_title(title_candidate: &str, content: &str) -> String {
    let trimmed_title = title_candidate.trim();
    if !trimmed_title.is_empty() {
        return trimmed_title.to_string();
    }
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(80).collect::<String>())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Untitled Prompt".to_string())
}

const MAX_STASHED_PROMPTS: i64 = 200;

const MAX_STASHED_PROMPT_CONTENT_CHARS: usize = 200_000;

fn create_unique_stashed_prompt_id(db: &Connection) -> DomainResult<String> {
    let millis = chrono::Utc::now().timestamp_millis();
    for attempt in 0..MAX_ID_GENERATION_ATTEMPTS {
        let candidate = if attempt == 0 {
            format!("gxserver-stash-{millis}")
        } else {
            format!("gxserver-stash-{millis}-{attempt}")
        };
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM stashed_prompts WHERE promptId = ?1)",
                [&candidate],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(DomainStateError::corrupt_state(
        "Could not allocate a unique stashed prompt id.",
    ))
}

fn stashed_prompt_json_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let prompt_id: String = row.get(0)?;
    let content: String = row.get(1)?;
    let project_id: Option<String> = row.get(2)?;
    let session_id: Option<String> = row.get(3)?;
    let cwd: Option<String> = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    let project_name: Option<String> = row.get(7)?;
    let identity_icon_json: Option<String> = row.get(8)?;
    let project_path: Option<String> = row.get(9)?;
    let worktree_json: Option<String> = row.get(10)?;
    /*
    CDXC:StashedPrompts 2026-07-29:
    Stash rows label their origin project with the same icon priority as the
    sidebar. Publish the user-selected identity fields plus the cached icon
    discovered from the repository; the client ranks those fields and falls
    back to a folder glyph.
    */
    let identity_icon = identity_icon_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    let project_icon = identity_icon
        .as_ref()
        .and_then(|icon| icon.get("icon").cloned());
    let project_icon_data_url = identity_icon
        .as_ref()
        .and_then(|icon| icon.get("iconDataUrl").and_then(Value::as_str))
        .map(str::to_string);
    let project = json!({
        "path": project_path,
        "worktree": worktree_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok()),
    });
    let project_discovered_icon_data_url = crate::project_icon::project_icon_key(&project)
        .as_deref()
        .and_then(crate::project_icon::published_project_icon_data_url);
    Ok(json!({
        "content": content,
        "createdAt": created_at,
        "cwd": cwd,
        "projectIcon": project_icon,
        "projectIconDataUrl": project_icon_data_url,
        "projectDiscoveredIconDataUrl": project_discovered_icon_data_url,
        "projectId": project_id,
        "projectName": project_name,
        "promptId": prompt_id,
        "sessionId": session_id,
        "updatedAt": updated_at,
    }))
}

fn read_stashed_prompt_row(db: &Connection, prompt_id: &str) -> DomainResult<Option<Value>> {
    let prompt = db
        .query_row(
            r#"
        SELECT s.promptId, s.content, s.projectId, s.sessionId, s.cwd,
               s.createdAt, s.updatedAt, p.name, p.identityIconJson,
               p.path, p.worktreeJson
        FROM stashed_prompts s
        LEFT JOIN projects p ON p.projectId = s.projectId
        WHERE s.promptId = ?1
        "#,
            [prompt_id],
            stashed_prompt_json_from_row,
        )
        .optional()
        .map_err(sql_error)?;
    /*
    CDXC:StashedPromptTags 2026-08-23:
    Every single-row read carries tagIds for the same reason the list does: the
    modal merges this row straight into its state, and a row that arrived
    without its assignments would blank that prompt's chips on save.
    */
    let Some(mut prompt) = prompt else {
        return Ok(None);
    };
    let mut statement = db
        .prepare(
            r#"
            SELECT l.tagId
            FROM stashed_prompt_tag_links l
            JOIN stashed_prompt_tags t ON t.tagId = l.tagId
            WHERE l.promptId = ?1
            ORDER BY t.isBuiltin DESC, t.sortOrder ASC, t.tagId ASC
            "#,
        )
        .map_err(sql_error)?;
    let tag_ids = statement
        .query_map([prompt_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    if let Some(object) = prompt.as_object_mut() {
        object.insert("tagIds".to_string(), json!(tag_ids));
    }
    Ok(Some(prompt))
}

const STASHED_PROMPT_TAG_FALLBACK_COLOR: &str = "#7f9cf5";

const MAX_STASHED_PROMPT_TAG_NAME_CHARS: usize = 40;

fn normalize_stashed_prompt_tag_name(name: &str) -> DomainResult<String> {
    let trimmed = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        return Err(DomainStateError::bad_request("name must not be empty."));
    }
    if trimmed.chars().count() > MAX_STASHED_PROMPT_TAG_NAME_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "name must be at most {MAX_STASHED_PROMPT_TAG_NAME_CHARS} characters."
        )));
    }
    Ok(trimmed)
}

/*
CDXC:StashedPromptTags 2026-08-23:
Tag colors are interpolated straight into CSS by every client, so the daemon
stores only a literal `#rrggbb` and rejects anything else rather than letting a
crafted value become a style expression downstream.
*/
fn normalize_stashed_prompt_tag_color(color: Option<&str>) -> DomainResult<String> {
    let Some(color) = color else {
        return Ok(STASHED_PROMPT_TAG_FALLBACK_COLOR.to_string());
    };
    let normalized = color.trim().to_ascii_lowercase();
    let is_hex = normalized.len() == 7
        && normalized.starts_with('#')
        && normalized[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if !is_hex {
        return Err(DomainStateError::bad_request(
            "color must be a #rrggbb hex string.",
        ));
    }
    Ok(normalized)
}

fn create_unique_stashed_prompt_tag_id(db: &Connection) -> DomainResult<String> {
    let millis = chrono::Utc::now().timestamp_millis();
    for attempt in 0..MAX_ID_GENERATION_ATTEMPTS {
        let candidate = if attempt == 0 {
            format!("gxserver-prompt-tag-{millis}")
        } else {
            format!("gxserver-prompt-tag-{millis}-{attempt}")
        };
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM stashed_prompt_tags WHERE tagId = ?1)",
                [&candidate],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(DomainStateError::corrupt_state(
        "Could not allocate a unique saved prompt tag id.",
    ))
}

fn stashed_prompt_tag_json_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let tag_id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let color: String = row.get(2)?;
    let is_builtin: i64 = row.get(3)?;
    let created_at: String = row.get(4)?;
    let updated_at: String = row.get(5)?;
    Ok(json!({
        "color": color,
        "createdAt": created_at,
        "isBuiltin": is_builtin != 0,
        "name": name,
        "tagId": tag_id,
        "updatedAt": updated_at,
    }))
}

/*
Builtin tags lead the rail so Favorites keeps the first slot no matter how many
tags the user adds later; user tags follow in creation order.
*/
fn read_stashed_prompt_tags(db: &Connection) -> DomainResult<Vec<Value>> {
    let mut statement = db
        .prepare(
            r#"
            SELECT tagId, name, color, isBuiltin, createdAt, updatedAt
            FROM stashed_prompt_tags
            ORDER BY isBuiltin DESC, sortOrder ASC, tagId ASC
            "#,
        )
        .map_err(sql_error)?;
    let tags = statement
        .query_map([], stashed_prompt_tag_json_from_row)
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(tags)
}

fn read_stashed_prompt_tag_row(db: &Connection, tag_id: &str) -> DomainResult<Option<Value>> {
    db.query_row(
        r#"
        SELECT tagId, name, color, isBuiltin, createdAt, updatedAt
        FROM stashed_prompt_tags
        WHERE tagId = ?1
        "#,
        [tag_id],
        stashed_prompt_tag_json_from_row,
    )
    .optional()
    .map_err(sql_error)
}

/*
Assignments come back keyed by prompt, in the same rail order as the catalogue,
so a row's chips and the pill rail never disagree about which tag comes first.
*/
fn read_stashed_prompt_tag_ids(db: &Connection) -> DomainResult<HashMap<String, Vec<String>>> {
    let mut statement = db
        .prepare(
            r#"
            SELECT l.promptId, l.tagId
            FROM stashed_prompt_tag_links l
            JOIN stashed_prompt_tags t ON t.tagId = l.tagId
            ORDER BY t.isBuiltin DESC, t.sortOrder ASC, t.tagId ASC
            "#,
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            let prompt_id: String = row.get(0)?;
            let tag_id: String = row.get(1)?;
            Ok((prompt_id, tag_id))
        })
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let mut tag_ids_by_prompt: HashMap<String, Vec<String>> = HashMap::new();
    for (prompt_id, tag_id) in rows {
        tag_ids_by_prompt.entry(prompt_id).or_default().push(tag_id);
    }
    Ok(tag_ids_by_prompt)
}

fn stashed_prompt_project_family(
    db: &Connection,
    project_id: &str,
) -> DomainResult<HashSet<String>> {
    let mut statement = db
        .prepare("SELECT projectId, worktreeJson FROM projects")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            let project_id: String = row.get(0)?;
            let worktree_json: Option<String> = row.get(1)?;
            Ok((project_id, worktree_json))
        })
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let parent_by_project: HashMap<String, String> = rows
        .iter()
        .filter_map(|(id, worktree_json)| {
            let parent = serde_json::from_str::<Value>(worktree_json.as_deref()?)
                .ok()?
                .get("parentProjectId")?
                .as_str()
                .filter(|parent| !parent.is_empty())?
                .to_string();
            Some((id.clone(), parent))
        })
        .collect();
    let root = parent_by_project
        .get(project_id)
        .cloned()
        .unwrap_or_else(|| project_id.to_string());
    let mut family: HashSet<String> = HashSet::new();
    family.insert(project_id.to_string());
    family.insert(root.clone());
    for (id, parent) in &parent_by_project {
        if parent == &root {
            family.insert(id.clone());
        }
    }
    Ok(family)
}
