use serde_json::{json, Map, Value};

use super::*;

// ---------------------------------------------------------------------------
// toCliSession
// ---------------------------------------------------------------------------

pub(super) fn to_cli_session(
    session: &Value,
    project: Option<&Value>,
    index: usize,
    presentation_session: Option<&Value>,
    presentation_order: Option<usize>,
) -> Value {
    let p = |key: &str| presentation_session.and_then(|value| value.get(key));
    let s = |key: &str| session.get(key);
    let lifecycle_state = js_string(s("lifecycleState"));
    let provider_state =
        js_string(s("providerState").and_then(|value| value.get("lifecycleState")));
    let activity = normalize_cli_session_activity(p("activity"));
    let status: Value = if lifecycle_state == "sleeping" {
        json!("sleep")
    } else if lifecycle_state == "stopped" {
        json!("stopped")
    } else if lifecycle_state == "running" {
        json!("running")
    } else if !provider_state.is_empty() {
        json!(provider_state)
    } else if !lifecycle_state.is_empty() {
        json!(lifecycle_state)
    } else {
        json!("unknown")
    };
    let provider_session_name = js_coalesce(&[
        s("zmxName"),
        s("providerState").and_then(|value| value.get("zmxName")),
    ]);
    let title = js_coalesce(&[p("title"), s("title")]);
    /*
     * CDXC:StateSync 2026-06-22-00:47:
     * Prefer presentation identity for the agent provider session id because
     * it is already the UI contract; fall back to listSessions runtime
     * metadata when a snapshot row is unavailable.
     */
    let agent_session_id = string_flag(js_coalesce(&[
        p("agentSessionId"),
        s("runtimeSettings").and_then(|value| value.get("agentSessionId")),
    ]));
    let agent_session_path = string_flag(js_coalesce(&[
        p("agentSessionPath"),
        s("runtimeSettings").and_then(|value| value.get("agentSessionPath")),
    ]));
    /*
     * CDXC:SessionTitles 2026-06-07-09:33:
     * Expose gxserver's rendered display title separately from the raw durable
     * title so clients can show unsynced/placeholder chrome without leaking
     * display glyphs into rename or restore payloads.
     */
    let display_title = js_coalesce(&[p("displayTitle"), title]);
    let is_live = lifecycle_state == "running" || provider_state == "exists";
    let mut map = Map::new();
    insert_js(&mut map, "actions", &[p("actions")]);
    insert_js(&mut map, "agent", &[s("agentId")]);
    insert_js(&mut map, "agentId", &[s("agentId")]);
    insert_js(&mut map, "agentIcon", &[p("agentIcon"), s("agentId")]);
    insert_js(&mut map, "agentName", &[p("agentName")]);
    map.insert("agentSessionId".to_string(), agent_session_id);
    map.insert("agentSessionPath".to_string(), agent_session_path);
    map.insert("alias".to_string(), json!(index as i64 + 1));
    insert_js(&mut map, "attention", &[p("attention")]);
    insert_js(&mut map, "programStatus", &[p("programStatus")]);
    insert_js(&mut map, "createdAt", &[p("createdAt"), s("createdAt")]);
    insert_js(&mut map, "isDraft", &[p("isDraft")]);
    insert_js(&mut map, "globalRef", &[s("globalRef")]);
    insert_js(&mut map, "groupId", &[p("groupId")]);
    insert_js(&mut map, "displayTitle", &[display_title]);
    insert_js(
        &mut map,
        "displayTitleTooltip",
        &[p("displayTitleTooltip"), display_title],
    );
    map.insert("isFocused".to_string(), json!(false));
    insert_js(&mut map, "isFavorite", &[p("isFavorite"), s("isFavorite")]);
    map.insert("isLocalOnly".to_string(), json!(false));
    insert_js(&mut map, "isParked", &[p("isParked"), s("isParked")]);
    insert_js(&mut map, "isPinned", &[p("isPinned"), s("isPinned")]);
    insert_js(&mut map, "sessionTag", &[p("sessionTag"), s("sessionTag")]);
    /*
     * CDXC:StateSync 2026-07-29-00:00:
     * Settle/snooze is server-owned inbox state, so the CLI inventory carries
     * it alongside pins and tags. Absent keys mean "never settled / never
     * snoozed", which is also what an older daemon and a pre-migration
     * state.db produce.
     */
    insert_js(&mut map, "settledAt", &[p("settledAt"), s("settledAt")]);
    insert_js(
        &mut map,
        "settledOverride",
        &[p("settledOverride"), s("settledOverride")],
    );
    insert_js(&mut map, "snoozedAt", &[p("snoozedAt"), s("snoozedAt")]);
    insert_js(
        &mut map,
        "snoozedUntil",
        &[p("snoozedUntil"), s("snoozedUntil")],
    );
    /*
     * CDXC:Git 2026-07-29-00:00:
     * Branch / +n −n / PR badge is resolved once per session cwd by the daemon,
     * so the CLI inventory forwards it verbatim rather than shelling out to git
     * per row. Presentation is the only source: a session row in state.db has no
     * git state of its own, and an older daemon simply omits the key.
     */
    insert_js(&mut map, "gitStatus", &[p("gitStatus")]);
    // CDXC:WorkMode 2026-10-09 SEE-ALSO: mobile_summary.rs forwards `work` too; the phone's Copy submenu (apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts) copies the PR, Linear and issue links from it. Present only for a session of a work-mode project.
    insert_non_null(&mut map, "work", p("work"));
    // CDXC:SessionLinks 2026-10-24 SEE-ALSO: mobile_summary.rs forwards `links` too; the phone's chat strip draws the session's PR, issue and Linear chips from it.
    insert_non_null(&mut map, "links", p("links"));
    // CDXC:ClaudeCloud 2026-10-23 SEE-ALSO: mobile_summary.rs forwards it too; the phone marks a Claude cloud session's row and opens its claude.ai page from it (apps/mobile/app/src/sessions/claudeCloud.ts).
    insert_non_null(&mut map, "claudeCloud", p("claudeCloud"));
    // Host-timer chrome for the mobile session menus; absent when the
    // presentation snapshot does not carry resolved timer projections.
    insert_js(&mut map, "closeAfterDone", &[p("closeAfterDone")]);
    insert_js(
        &mut map,
        "delayedSendRemainingLabel",
        &[p("delayedSendRemainingLabel")],
    );
    /*
     * CDXC:DelayedSend 2026-09-03:
     * The remaining label above is a snapshot from the moment of the poll. A
     * client that only polls every few seconds needs the absolute deadline to
     * tick the countdown from its own clock between polls, exactly like the
     * desktop sidebar does from the presentation delta. Absent for the
     * "waiting for agent(s)" triggers, whose countdown starts only once the
     * ten-second stability window opens.
     */
    insert_js(
        &mut map,
        "delayedSendDeadlineAt",
        &[p("delayedSendDeadlineAt")],
    );
    /*
     * CDXC:SessionStatus 2026-09-25 WHY:
     * The phone's session row draws the same status as the desktop sidebar row, which reads these three off the presentation session: the pink question dot (`pendingQuestionCount`), the grey background-work dot (`backgroundWorkDetectedAt`) and the Close After Done countdown (`closeAfterDoneDeadlineAt`). Presentation is the only source; an older daemon omits them. `to_mobile_session_summary` below forwards them again, and the phone reads them in apps/mobile/app/src/components/sessions/sessionStatus.ts.
     */
    insert_js(
        &mut map,
        "pendingQuestionCount",
        &[p("pendingQuestionCount")],
    );
    insert_js(
        &mut map,
        "backgroundWorkDetectedAt",
        &[p("backgroundWorkDetectedAt")],
    );
    // The red waiting-for-approval dot (CDXC:SessionStatus 2026-10-27 in server/src/session_approval.rs); absent when false.
    insert_js(&mut map, "awaitingApproval", &[p("awaitingApproval")]);
    // CDXC:Sidebar 2026-10-30 SEE-ALSO: the phone's Unified list (apps/mobile/app/src/contract/unified/) reads what gx-core's Unified sidebar reads: when the working stint began (its "came back" moment and Working timer) and a failed model change (its Failed status).
    insert_js(&mut map, "workingStartedAt", &[p("workingStartedAt")]);
    insert_js(
        &mut map,
        "modelSelectionFailure",
        &[p("modelSelectionFailure")],
    );
    insert_js(
        &mut map,
        "closeAfterDoneDeadlineAt",
        &[p("closeAfterDoneDeadlineAt")],
    );
    /*
     * CDXC:SessionChat 2026-08-21-b:
     * The phone's session-list badge reads these two off the mobile summary, so
     * the inventory has to forward them from the presentation snapshot the same
     * way it forwards the Delayed Send countdown above. Without this the badge
     * is wired end to end on the mobile side and can never light up. Absent
     * means "no queue" / "nothing failed", which is also what a daemon that
     * predates the queue publishes.
     */
    insert_js(&mut map, "queuedPromptCount", &[p("queuedPromptCount")]);
    insert_js(
        &mut map,
        "queuedPromptFailedCount",
        &[p("queuedPromptFailedCount")],
    );
    // CDXC:Drafts 2026-09-04: the composer-draft dot's input, forwarded for the
    // same reason as the queue counts above. Absent means no draft.
    insert_js(&mut map, "hasComposerDraft", &[p("hasComposerDraft")]);
    /*
     * CDXC:SessionNotes 2026-08-24:
     * The phone's session row renders the note dot and the note text from this
     * field, so the inventory forwards the presentation value verbatim.
     * Presentation is the only source: a session row in state.db has no note of
     * its own (notes are keyed by agent session id), and an older daemon simply
     * omits the key.
     */
    insert_js(&mut map, "sessionNote", &[p("sessionNote")]);
    // CDXC:Coordinators 2026-10-01 SEE-ALSO: mobile_summary.rs forwards it too; the phone's session row draws the coordinator crown from it (apps/mobile/app/src/components/sessions/SessionRow.tsx) and nests threads under it from the three fields after it (apps/mobile/app/src/contract/coordinatorTree.ts).
    insert_js(&mut map, "coordinatorRole", &[p("coordinatorRole")]);
    insert_js(
        &mut map,
        "coordinatorProjectId",
        &[p("coordinatorProjectId")],
    );
    insert_js(
        &mut map,
        "coordinatorSessionId",
        &[p("coordinatorSessionId")],
    );
    insert_js(
        &mut map,
        "coordinatorThreadState",
        &[p("coordinatorThreadState")],
    );
    insert_js(
        &mut map,
        "sendWhenAllProjectSessionsStopActive",
        &[p("sendWhenAllProjectSessionsStopActive")],
    );
    insert_js(
        &mut map,
        "sendWhenAgentStopsActive",
        &[p("sendWhenAgentStopsActive")],
    );
    map.insert("isLive".to_string(), json!(is_live));
    insert_js(
        &mut map,
        "isPrimaryTitleTerminalTitle",
        &[p("isPrimaryTitleTerminalTitle")],
    );
    map.insert(
        "isSleeping".to_string(),
        json!(lifecycle_state == "sleeping"),
    );
    insert_js(&mut map, "isTemporaryTitle", &[p("isTemporaryTitle")]);
    insert_js(&mut map, "kind", &[p("kind"), s("kind")]);
    insert_js(
        &mut map,
        "lastActiveAt",
        &[p("lastActiveAt"), s("lastActiveAt")],
    );
    insert_js(
        &mut map,
        "lastInteractionAt",
        &[
            p("meaningfulActivityAt"),
            p("lastActiveAt"),
            s("lastActiveAt"),
            s("updatedAt"),
        ],
    );
    map.insert("lifecycleState".to_string(), json!(lifecycle_state));
    map.insert("ownership".to_string(), json!("gxserver"));
    insert_js(&mut map, "primaryTitle", &[p("primaryTitle")]);
    insert_js(&mut map, "projectId", &[s("projectId")]);
    insert_js(
        &mut map,
        "projectName",
        &[project.and_then(|value| value.get("name")), s("projectId")],
    );
    match js_coalesce(&[
        p("cwd"),
        s("cwd"),
        project.and_then(|value| value.get("path")),
    ]) {
        Some(value) if !value.is_null() => {
            map.insert("projectPath".to_string(), value.clone());
        }
        _ => {
            map.insert("projectPath".to_string(), json!(""));
        }
    }
    map.insert("provider".to_string(), json!("zmx"));
    insert_js(&mut map, "providerSessionName", &[provider_session_name]);
    // The phone shakes the title and prefills Rename from these (CDXC:SessionTitles 2026-10-12).
    insert_js(&mut map, "renameFailedAt", &[p("renameFailedAt")]);
    insert_js(&mut map, "renameFailedTitle", &[p("renameFailedTitle")]);
    if !provider_state.is_empty() {
        map.insert("providerSessionState".to_string(), json!(provider_state));
    }
    insert_js(&mut map, "sessionId", &[s("sessionId")]);
    insert_js(&mut map, "sessionPersistenceName", &[provider_session_name]);
    map.insert("sessionPersistenceProvider".to_string(), json!("zmx"));
    insert_js(&mut map, "sidebarOrder", &[p("sidebarOrder")]);
    insert_js(&mut map, "sortKey", &[p("sortKey")]);
    if let Some(order) = presentation_order {
        map.insert("sortOrder".to_string(), json!(order as i64));
    }
    map.insert("status".to_string(), status);
    map.insert("activity".to_string(), json!(activity));
    insert_js(&mut map, "surface", &[p("surface")]);
    insert_js(&mut map, "terminalTitle", &[p("terminalTitle")]);
    insert_js(&mut map, "title", &[title]);
    insert_js(&mut map, "titleSource", &[p("titleSource")]);
    insert_js(&mut map, "trustedResumeTitle", &[p("trustedResumeTitle")]);
    insert_js(&mut map, "updatedAt", &[p("updatedAt"), s("updatedAt")]);
    insert_js(
        &mut map,
        "visibleInSidebarByDefault",
        &[p("visibleInSidebarByDefault")],
    );
    insert_js(&mut map, "zmxName", &[p("zmxName"), s("zmxName")]);
    Value::Object(map)
}

/// JS stringFlag(): non-strings coerce (null/undefined -> null); strings trim
/// and empty trims become null.
pub(super) fn string_flag(value: Option<&Value>) -> Value {
    match value {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Value::Null
            } else {
                Value::String(trimmed.to_string())
            }
        }
        Some(other) => Value::String(js_display(other)),
    }
}

pub(super) fn normalize_cli_session_activity(value: Option<&Value>) -> String {
    let normalized = js_string(value)
        .trim()
        .to_lowercase()
        .replace(['_', ' '], "-");
    if normalized == "attention"
        || normalized == "needs-attention"
        || normalized == "attention-required"
    {
        return "attention".to_string();
    }
    if normalized == "working"
        || normalized == "active"
        || normalized == "busy"
        || normalized == "processing"
    {
        return "working".to_string();
    }
    "idle".to_string()
}
