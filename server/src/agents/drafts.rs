/*
CDXC:Drafts 2026-08-28:
A DRAFT is a real, durable gxserver session row whose agent CLI is running in
the background but which has never received a user prompt. Every new agent
session created from the sidebar starts this way, so trust / login / upgrade
screens surface while the user is still typing, and so the row is visible on
every client instead of only on the machine that created it.

The whole feature hangs off ONE marker: `runtimeSettings.draftStatus = "draft"`.
It is written at creation (`GxserverCreateSessionParams.draft`) and REMOVED —
never set to a second value — the moment the first user prompt actually reaches
the agent. A promoted draft is byte-for-byte an ordinary session; there is no
"was a draft" state to reason about anywhere downstream.

This module owns the marker, the promotion choke point, the draft-derived
display title, the agent list the composer's "Agents" section renders, and the
agent switch itself. Everything else in the daemon asks here rather than
testing the key.

CDXC:Drafts 2026-08-29 (drafts are durable):
A draft is never thrown away on its own. It survives navigating to another
session, sleeping, and daemon restarts, whether or not anything has been typed
into it, and leaves the sidebar by exactly two routes: the user deletes it, or
it is promoted. There is no navigate-away discard and no boot-time sweep of
empty drafts any more — an empty draft is a session the user made on purpose.
Since 2026-10-04 one more route exists, off by default since 2026-10-09: with the
advanced `closeEmptySessionsOnNew` setting on and a Chat agent, the user's next new
session in the same project closes it while it is still fully empty (CDXC:Sessions
2026-10-09 in empty_session_cleanup.rs).
*/

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::zmx::{dispatch_zmx_lifecycle_endpoint, ZmxServerContext};

pub(crate) const DRAFT_STATUS_KEY: &str = "draftStatus";
pub(crate) const DRAFT_STATUS_DRAFT: &str = "draft";

/// Sidebar rows are one line; anything past this is noise the tooltip already
/// carries. Measured in CHARACTERS, not bytes, so a prompt written in any
/// script clamps to the same visual length.
const DRAFT_DISPLAY_TITLE_MAX_CHARS: usize = 64;

pub(crate) fn runtime_settings_are_draft(runtime_settings: &Map<String, Value>) -> bool {
    read_text_from_map(runtime_settings, DRAFT_STATUS_KEY).as_deref() == Some(DRAFT_STATUS_DRAFT)
}

pub(crate) fn session_is_draft(session: &Value) -> bool {
    runtime_settings_are_draft(&object_field(session, "runtimeSettings"))
}

/// Arms the marker at creation. The marker is server-owned: a client that puts
/// `draftStatus` in `runtimeSettings` by hand does not get a draft, and a
/// create call without `draft: true` always strips it.
pub(crate) fn apply_draft_session_create_param(
    params: &Map<String, Value>,
    runtime_settings: &mut Map<String, Value>,
) {
    if params.get("draft").and_then(Value::as_bool) == Some(true) {
        runtime_settings.insert(DRAFT_STATUS_KEY.to_string(), json!(DRAFT_STATUS_DRAFT));
    } else {
        runtime_settings.remove(DRAFT_STATUS_KEY);
    }
}

pub(crate) fn clear_draft_status(runtime_settings: &mut Map<String, Value>) -> bool {
    runtime_settings.remove(DRAFT_STATUS_KEY).is_some()
}

/*
The TERMINAL-DIRECT half of the promotion choke point: a user who ignores chat
and types straight into the agent's pane. It has two signals, in order of how
much they actually prove.

1. `promote_draft_on_prompt_evidence` — an agent hook that says, in so many
   words, that the user submitted a prompt. This is the signal we want, and for
   every agent whose hooks Ghostex has installed it is the one that fires.

2. `promote_draft_on_first_activity` — the first working/attention transition,
   for agents with no hooks, where a terminal title is the only evidence there
   is. A title cannot distinguish "the user prompted me" from "I am booting":
   Codex's startup spinner paints exactly the same working title (see
   `default_activity`). The daemon already has a policy for that — layer 1 of
   CDXC:AgentScreenDetection — and drafts lean on it rather than inventing
   a second one: `arm_draft_launch_activity_suppression` re-arms the launch
   window on EVERY draft provider start, so a boot spinner is folded back to
   idle and never reaches this function with a timestamp.

Both are deliberately keyed on "this row had NO `lastActiveAt` and is about to
get one" rather than on the presence of a timestamp in the update:
`compute_activity_update` carries the session's existing `lastActiveAt` forward
on every idle transition, so the timestamp alone says nothing about whether the
agent was ever prompted.
*/
pub(crate) fn promote_draft_on_first_activity(
    session: &Value,
    runtime_settings: &mut Map<String, Value>,
    next_last_active_at: Option<&str>,
) -> bool {
    // CDXC:Drafts 2026-09-14 WHY:
    // Native Windows Codex can paint a working title before creating its first conversation, after the startup suppression window. Promoting that empty draft removes Chat eligibility and sends the return-to-chat button to hook Settings.
    // Its explicit prompt hook and Chat send already promote on real input; passive activity must not consume the draft.
    #[cfg(windows)]
    if crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
        == Some("codex")
    {
        return false;
    }
    if read_text_value(session, "lastActiveAt").is_some() {
        return false;
    }
    if next_last_active_at
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return false;
    }
    clear_draft_status(runtime_settings)
}

/// Whether an agent-hook event is positive evidence that the USER submitted a
/// prompt: Claude's `UserPromptSubmit`, or any hook carrying the prompt text
/// itself. Nothing a CLI does while starting up produces either.
pub(crate) fn is_draft_prompt_evidence(params: &Map<String, Value>) -> bool {
    is_explicit_user_prompt_submit_event(params) || read_text(params, "firstUserMessage").is_some()
}

pub(crate) fn promote_draft_on_prompt_evidence(
    params: &Map<String, Value>,
    runtime_settings: &mut Map<String, Value>,
) -> bool {
    if !is_draft_prompt_evidence(params) {
        return false;
    }
    clear_draft_status(runtime_settings)
}

/*
Re-arms layer 1 of CDXC:AgentScreenDetection on a draft — the same reset a
launch, resume or wake performs — so passive title signals are folded back to
idle for the window instead of stamping `lastActiveAt`.

Drafts need this at moments an ordinary session does not, because for a draft
that stamp is not merely a wrong "Last Active" label: it is the promotion that
takes the row out of the draft state permanently. Called on every draft provider
start (a cold desktop attach and the restart inside an agent switch both go
through `/api/startSessionProvider`, and neither is a wake), and after Ghostex
itself types an option command into a draft's terminal.

Scoped to drafts on purpose: no non-draft session's activity changes because of
this.
*/
pub(crate) fn arm_draft_launch_activity_suppression(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<Value, DomainStateError> {
    if !session_is_draft(session) {
        return Ok(session.clone());
    }
    let (Some(project_id), Some(session_id)) = (
        read_text_value(session, "projectId"),
        read_text_value(session, "sessionId"),
    ) else {
        return Ok(session.clone());
    };
    let update =
        crate::session_status::compute_activity_update(session, &Map::new(), Some("launch"));
    let mut runtime_settings = object_field(session, "runtimeSettings");
    if runtime_settings.get("agentActivity") == Some(&update.activity) {
        return Ok(session.clone());
    }
    runtime_settings.insert("agentActivity".to_string(), update.activity);
    let mut session_update = Map::new();
    session_update.insert("projectId".to_string(), json!(project_id));
    session_update.insert("sessionId".to_string(), json!(session_id));
    session_update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&session_update)
}

/*
Kills a draft's background agent CLI, including the macOS launchd job
`kill_zmx_session` cleans up. Runs when a draft row is DELETED (the user
deleting it, through `/api/removeSession`), because that row is the last thing
pointing at that daemon: miss it and the CLI runs forever with nothing in the
sidebar to stop it.

Deliberately unconditional rather than probe-then-kill. The row is being deleted
either way, so the `unknown` provider state a kill of an already-dead daemon
writes never reaches a client, while a probe costs a second subprocess and opens
a window where a provider that was still starting reads as `missing` and is
orphaned anyway.
*/
pub(crate) fn kill_draft_session_provider(session: &Value) {
    if !session_is_draft(session) {
        return;
    }
    let Some(zmx_name) = read_text_value(session, "zmxName") else {
        return;
    };
    let Ok(zmx) = crate::toolchain::require_bundled_zmx() else {
        return;
    };
    crate::zmx::kill_zmx_session(&zmx_name, &zmx.executable_path);
}

/*
The CHAT half of the promotion choke point: one durable write, called after a
chat/queue send has actually handed its bytes to the agent. Returns whether the
row changed, so the caller can skip a presentation delta for the overwhelmingly
common case of a send into a session that was never a draft.
*/
pub(crate) fn promote_draft_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(false);
    };
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    if !clear_draft_status(&mut runtime_settings) {
        return Ok(false);
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)?;
    Ok(true)
}

/*
The sidebar row's title while a draft waits: the first non-blank line of the
text the user has typed, so a draft reads as what it is about instead of as
"Claude Session". Projection-level only — the durable `title` stays the agent
default, which is what a promoted draft goes back to showing.
*/
pub(crate) fn draft_display_title(content: &str) -> Option<String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.chars().count() <= DRAFT_DISPLAY_TITLE_MAX_CHARS {
        return Some(normalized);
    }
    let mut clamped = normalized
        .chars()
        .take(DRAFT_DISPLAY_TITLE_MAX_CHARS - 1)
        .collect::<String>()
        .trim_end()
        .to_string();
    clamped.push('\u{2026}');
    Some(clamped)
}

/*
CDXC:Drafts 2026-08-28 (agent switching):
Only chat-supported agents can be switched to, because the composer that offers
the switch can only read a transcript it has a decoder for. `openclaude` and
`grok-build` are alternate ids for the Claude and Grok families, so they map
onto them rather than adding families of their own; `omp` is its own family
despite sharing Pi's transcript format, because it paints different chrome.
*/
/// Which agents count as chat-supported comes from the one chat agent table
/// (`SESSION_CHAT_AGENTS` in gx-protocol); this only names the agent id each family is switched
/// to by.
pub(crate) fn chat_supported_base_agent_id(value: Option<&str>) -> Option<&'static str> {
    let label = value?.trim().to_ascii_lowercase();
    match ghostex_gx_chat_core::protocol::session_chat_transcript_family(&label)? {
        "hermes" => Some("hermes-agent"),
        "pi" if label == "omp" => Some("omp"),
        family => Some(family),
    }
}

#[derive(Clone)]
struct AvailableDraftAgent {
    agent_id: String,
    base_agent_id: String,
    icon: String,
    name: String,
}

impl AvailableDraftAgent {
    fn to_value(&self) -> Value {
        json!({
            "agentId": self.agent_id,
            "baseAgentId": self.base_agent_id,
            "icon": self.icon,
            "name": self.name,
        })
    }
}

/*
The "Agents" section of the chat composer's model dropdown, resolved by the
daemon that owns the project so every client (desktop, web, phone over SSH)
offers the same set without shipping its own copy of the project's agent
configuration.

The list starts with the sidebar launcher's normalized rows, then drops agents
whose transcript family chat cannot read. That keeps custom names, hidden
built-ins, custom agents, and display order identical between both menus.
*/
/// CDXC:Drafts 2026-09-04 DECISION:
/// User: Switch Agent CLI must match the agent order in the sidebar Select Agent dropdown, so this projection preserves the sidebar rows' order before filtering out chat-unsupported families.
pub(crate) fn available_draft_agents(project: &Value) -> Value {
    let agents =
        crate::sidebar_hud::sidebar_agent_buttons_from_projects(std::slice::from_ref(project));
    let agents = agents
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|agent| {
            let agent_id = read_text_from_map(agent, "agentId")?;
            let icon = read_text_from_map(agent, "icon")?;
            let base_agent_id = chat_supported_base_agent_id(Some(&agent_id))
                .or_else(|| chat_supported_base_agent_id(Some(&icon)))?;
            Some(AvailableDraftAgent {
                agent_id,
                base_agent_id: base_agent_id.to_string(),
                icon,
                name: read_text_from_map(agent, "name")?,
            })
        })
        .collect::<Vec<_>>();
    Value::Array(agents.iter().map(AvailableDraftAgent::to_value).collect())
}

/*
CDXC:Drafts 2026-08-28 (agent switching):
`/api/switchDraftAgent`. A draft holds no conversation, so this needs no
confirmation and destroys nothing: forget the agent identity the old CLI
published, rebuild the launch plan through the SAME resolution
`/api/createAgentSession` uses (so a switched draft is indistinguishable from
one created with the new agent), and get the new CLI running.

CDXC:Drafts 2026-08-28 (live-pane switching):
"Get the new CLI running" has two shapes, and which one runs is decided by the
provider probe — never by a failure.

  * The pane is ALIVE (the common case: the user is looking at the draft they
    just opened). The switch then happens INSIDE that pane and the provider is
    never touched. Zsh launches the agent from a one-shot startup hook and
    keeps its initialized login shell; other shells use `<setup>; <agent CLI>;
    exec $SHELL -l`. Interrupting the CLI returns the same pty to its login
    shell, where the new agent's launch line can simply be typed.
  * The pane is MISSING (a slept or never-opened draft). There is nothing to
    reuse and nothing to interrupt, so the rewritten row is started the normal
    way. This is the wake path, not a fallback.

Killing and restarting the provider — what this endpoint did until 2026-08-28 —
is what made all four reported switch defects: the desktop's attach client
exits with the pane, and the exit poll closes the tab, which drops chat-mode
membership and DESTROYS the CEF chat page. That page is where the user's unsent
composer text lives (its 2s draft-sync debounce never gets to flush), and its
teardown is the visible chat → raw terminal → chat flash. Do not reintroduce a
kill here.

The refusal on a promoted session is the point of the whole endpoint being
draft-only: once a prompt has reached the agent, its transcript, resume plan,
and stored conversation id all belong to that agent, and rewriting the row
would strand every one of them.
*/
pub(crate) fn switch_draft_agent(
    repository: &DomainRepository<'_>,
    db: &Connection,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let lifecycle = read_lifecycle(params)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let session = require_session(repository, &lifecycle)?;
    crate::agentbox::refuse_for_agentbox_session(&session, "Switching the agent")?;
    if !session_is_draft(&session) {
        return Err(DomainStateError {
            code: "invalidState",
            message:
                "This session has already been prompted, so its agent can no longer be changed."
                    .to_string(),
        }
        .into());
    }
    let agent_id = read_required_text(params.get("agentId"), "agentId")?;
    let previous_agent_id = read_text_value(&session, "agentId");
    if previous_agent_id
        .as_deref()
        .is_some_and(|previous| previous.eq_ignore_ascii_case(&agent_id))
    {
        return Ok(AgentEndpointOutput {
            presentation_session: None,
            result: json!({ "agentId": agent_id, "session": session }),
        });
    }
    /*
    Resolve the new agent BEFORE the row is touched. The rebuild below refuses a
    commandless agent id too, but by then the old CLI has already been
    interrupted and a typo would leave the user staring at a bare login shell.
    */
    let agent_config = resolve_project_agent_config(&project, &agent_id, None);
    if read_text_from_map(&agent_config, "command").is_none()
        && default_agent_command(&agent_id).is_none()
    {
        return Err(DomainStateError::bad_request(format!(
            "{agent_id} is not an agent this project can launch."
        ))
        .into());
    }
    let settings = read_agent_settings(db)?;

    /*
    The probe picks the switch's shape, and nothing else: `exists` means the
    pane below is reused, anything else means the rewritten row is started. It
    is also the reason neither branch kills: an unconditional kill would stamp
    the row `unknown` for a draft whose CLI was never started (nobody has opened
    it yet), which the sidebar reads as a broken session.
    */
    let probe = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/probeSessionProvider",
        params,
        context,
        &settings,
    )?;
    let provider_exists = probe
        .result
        .get("providerState")
        .and_then(|state| state.get("lifecycleState"))
        .and_then(Value::as_str)
        == Some("exists");
    // The probe writes the freshly observed `providerState` back to the row, so
    // re-read before the rewrite below builds its update out of it.
    let session = require_session(repository, &lifecycle)?;

    let switch_guard = DraftAgentSwitch::begin(&lifecycle.project_id, &lifecycle.session_id)?;

    /*
    Everything the OLD agent owns has to go before the new plan is built, or
    `create_agent_session_params_for_project` would resolve the new agent id
    against the previous agent's stored command and icon and quietly launch the
    wrong CLI. The identity keys go with them: the conversation the old CLI
    published at startup does not exist for the new one.

    `agentActivity` is dropped for two reasons at once. It carries the OLD
    agent's `agentName`, which would keep naming a CLI that is no longer running
    until the new one's first event; and dropping it makes the rebuild below
    mint a fresh `default_activity`, which arms layer 1 of
    CDXC:AgentScreenDetection — so the new CLI's startup spinner is folded
    back to idle instead of stamping `lastActiveAt` and promoting the very draft
    the user just switched.
    */
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    for key in [
        "accountId",
        "accountName",
        "accountColor",
        "accountSlot",
        "accountProvider",
        "accountBaseCommand",
        "accountCommand",
        "accountSwitch",
        "accountRecovery",
        "accountRecoverySuppressed",
        "accountPolicyDefault",
        "accountPolicyOverride",
        "agentActivity",
        "agentSessionId",
        "agentSessionPath",
        "agentCommand",
        "launchAgentId",
        "agentName",
        crate::session_chat_model_selection_alert::MODEL_SELECTION_FAILURE_KEY,
    ] {
        runtime_settings.remove(key);
    }
    let previous_launch = object_field(&session, "launchSettings");
    let previous_config = previous_agent_id
        .as_deref()
        .map(|id| resolve_project_agent_config(&project, id, Some(&previous_launch)))
        .unwrap_or_default();
    let previous_family =
        resume_agent_family_id(previous_agent_id, &previous_config, &previous_launch);
    let next_family = resume_agent_family_id(Some(agent_id.clone()), &agent_config, &Map::new());
    if previous_family != next_family {
        crate::session_approval::clear_approval_wait(&mut runtime_settings);
    }
    let mut launch_settings = object_field(&session, "launchSettings");
    for key in [
        "acceptAllMode",
        "agentCommand",
        "agentLaunchPlan",
        "agentResumePlan",
        "icon",
        "runtimeRelevant",
    ] {
        launch_settings.remove(key);
    }

    let mut create_params = Map::new();
    create_params.insert("projectId".to_string(), json!(lifecycle.project_id.clone()));
    create_params.insert("agentId".to_string(), json!(agent_id.clone()));
    // Same guard remote agent starts use: refuse an unknown agent id instead of
    // leaving the draft pointing at a CLI that will never launch.
    create_params.insert("requireLaunchCommand".to_string(), json!(true));
    // The row is STILL a draft after the switch — only its agent changed. The
    // create-time resolution strips the marker unless it is asked for, which is
    // what keeps a client from minting a draft by hand.
    create_params.insert("draft".to_string(), json!(true));
    create_params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    create_params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    // CDXC:SessionChat 2026-09-22 DECISION:
    // User: picking another agent's model in the chat picker "switches the CLI and also sets the right effort and model on the CLI that we switched to". A draft rebuilt without them started the new CLI on its own saved default, so the pick rides the rebuilt launch line the way `createAgentSession` carries `agentModel` and `agentEffort`.
    for key in ["agentModel", "agentEffort"] {
        if let Some(value) = params.get(key).filter(|value| !value.is_null()) {
            create_params.insert(key.to_string(), value.clone());
        }
    }
    // CDXC:AgentProviders 2026-09-16 DECISION:
    // User: switching Codex to Claude or Claude to Codex must choose the same account as launching the target agent from the sidebar button.
    // A draft whose Run on row picked a box keeps that box when the new agent can run in one; its
    // pane stays at the login shell until the first message starts the box (draft_run_location.rs).
    let keep_box = crate::agentbox::pending_session_agentbox(&session)
        .filter(|_| {
            resume_agent_family_id(Some(agent_id.clone()), &agent_config, &Map::new())
                .is_some_and(|family| crate::agentbox::AGENTBOX_AGENTS.contains(&family.as_str()))
        })
        .map(|agentbox| agentbox.provider);
    if let Some(provider) = keep_box.as_deref() {
        create_params.insert(
            "runLocation".to_string(),
            json!(format!("agentbox:{provider}")),
        );
    }
    let mut resolved = create_agent_session_params_for_project(db, &project, &create_params)?;
    if keep_box.is_some() {
        hold_box_launch(&mut resolved);
    }
    /*
    The line the live pane will be typed, read out BEFORE the row is written so
    a plan that somehow carries no command leaves the draft exactly as it was
    instead of claiming an agent nobody can launch.
    */
    let reuse_command = (provider_exists && keep_box.is_none())
        .then(|| draft_switch_reuse_command(&resolved))
        .transpose()?;
    if provider_exists && keep_box.is_none() {
        if let Some(launch_settings) =
            crate::zmx::launch_settings_with_consumed_agent_launch_startup_text(&Value::Object(
                resolved.clone(),
            ))
        {
            resolved.insert("launchSettings".to_string(), Value::Object(launch_settings));
        }
    }

    let mut update = lifecycle_update(&lifecycle);
    update.insert("agentId".to_string(), json!(agent_id.clone()));
    update.insert("kind".to_string(), json!("agent"));
    if let Some(value) = resolved.get("launchSettings").cloned() {
        update.insert("launchSettings".to_string(), value);
    }
    /*
    The rebuild's own default starts this row at "working" whenever the
    runtime settings it is rebuilt from still carry a `firstUserMessage`. That
    is right for a session being created to run a first prompt and wrong for a
    draft, which is being relaunched with nothing to do — and it would hand the
    new CLI's startup spinner an already-expired suppression stamp to promote
    the draft through. Seed the idle default instead: it names the NEW agent and
    arms layer 1 of CDXC:AgentScreenDetection from this instant, so the
    window is already open before the CLI below is typed or started.
    */
    let mut next_runtime_settings = resolved
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    next_runtime_settings.insert(
        "agentActivity".to_string(),
        default_activity(Some(&agent_id), None),
    );
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(next_runtime_settings),
    );
    /*
    The row's title follows the agent only while it is still the PREVIOUS
    agent's default. A title the user typed, an auto-generated one, or a
    terminal-observed one is theirs and survives the switch untouched. The
    draft-text display title is projection-level and is unaffected either way.
    */
    if let Some(title) = next_draft_agent_title(
        &project,
        &session,
        resolved.get("title").and_then(Value::as_str),
    ) {
        update.insert("title".to_string(), json!(title));
    }
    let updated = repository.update_session(&update)?;
    crate::session_chat_model_selection::discard_for_agent_switch(
        db,
        &lifecycle.project_id,
        &lifecycle.session_id,
    );

    if let Some(command) = reuse_command {
        /*
        The new CLI boots inside a provider that never restarted, so the re-arm
        `start_session_provider` performs for every draft start never runs for
        it. Re-arm here instead, or the new CLI's boot spinner is free to stamp
        `lastActiveAt` and promote the draft the user is still typing into.
        */
        let updated = arm_draft_launch_activity_suppression(repository, &updated)?;
        /*
        Cancel first so a send the user queued against the OLD agent cannot land
        between the interrupts and the new CLI, exactly as `interruptSessionChat`
        does; then enqueue the whole reuse as ONE job on the per-session worker.
        One job because CDXC:SessionChat: separate jobs may be
        separated by somebody else's, and every byte below has to reach this pty
        in order.
        */
        crate::session_chat_send::cancel_session_chat_sends(
            &lifecycle.project_id,
            &lifecycle.session_id,
        );
        let completion = crate::session_chat_send::enqueue_session_write_sequence_with_completion(
            &updated,
            &lifecycle.project_id,
            &lifecycle.session_id,
            DRAFT_AGENT_SWITCH_SEND_SOURCE,
            build_draft_agent_switch_steps(&command),
            None,
        )?;
        switch_guard.finish_after(completion);
        return Ok(AgentEndpointOutput {
            presentation_session: Some((
                lifecycle.project_id.clone(),
                lifecycle.session_id.clone(),
            )),
            result: json!({
                "agentId": agent_id,
                "session": updated,
            }),
        });
    }

    let provider = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        params,
        context,
        &settings,
    )?;
    let session = provider.result.get("session").cloned().unwrap_or(updated);
    Ok(AgentEndpointOutput {
        presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
        result: json!({
            "agentId": agent_id,
            "provider": provider.result,
            "session": session,
        }),
    })
}

/// Attribution for every byte an agent switch types into a live draft pane, so
/// diagnostic input logs separate them from a user's own keystrokes and from
/// chat sends.
const DRAFT_AGENT_SWITCH_SEND_SOURCE: &str = "draft-agent-switch";
/// How long the old CLI gets to start (when the switch came in its first seconds) and exit.
const DRAFT_SWITCH_EXIT_TIMEOUT_MS: u64 = 30_000;
/// How long the new CLI's start command gets to run once typed.
const DRAFT_SWITCH_LAUNCH_TIMEOUT_MS: u64 = 10_000;

/// The new agent's launch line, taken from the rebuilt plan's `startupText` —
/// already leading-space prefixed so the login shell keeps it out of atuin
/// history — with the plan's own trailing Enter stripped. That Enter is a
/// separate step in the sequence below, and sending both would submit twice:
/// the second one would land in the agent CLI that the first one started.
pub(crate) fn draft_switch_reuse_command(
    resolved: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    let command = resolved
        .get("launchSettings")
        .and_then(|settings| settings.get("agentLaunchPlan"))
        .and_then(|plan| plan.get("startupText"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n']);
    if command.trim().is_empty() {
        return Err(DomainStateError {
            code: "invalidState",
            message: "The selected agent resolved to an empty launch command, so the draft's agent was left unchanged."
                .to_string(),
        });
    }
    Ok(crate::accounts::default_login::route_around_signed_out_default(command))
}

/*
The keystrokes an agent switch types into a draft's LIVE pane, in the order a
person sitting at that pane would type them, as one indivisible job: exit the
old CLI, then type the new one's start command and make sure it ran.
*/
pub(crate) fn build_draft_agent_switch_steps(
    command: &str,
) -> Vec<crate::session_chat_send::SessionChatSendStep> {
    use crate::session_chat_send::SessionChatSendStep;
    let mut steps = build_draft_agent_exit_steps();
    steps.push(SessionChatSendStep::RunShellCommand {
        command: command.to_string(),
        home_dir: crate::paths::get_gxserver_paths(None).home_dir,
        timeout_ms: DRAFT_SWITCH_LAUNCH_TIMEOUT_MS,
    });
    steps
}

/// The first half of [`build_draft_agent_switch_steps`]: exit the draft's CLI and wait for its
/// login shell. On its own it stops a draft whose Run on row picked a box, which starts nothing
/// until the first message (agents/draft_run_location.rs).
///
/// CDXC:Drafts 2026-10-24 WHY:
/// One burst of three Ctrl+C presses followed by a wait for the CLI to exit failed with "The agent did not exit back to the shell" whenever the burst did not end the CLI (GitHub #290): Claude asks again when background tasks run, and a switch made in a new chat's first seconds reached a shell still loading its startup files, before the old CLI had even started. The draft switch now uses the account switch's exit: it presses Escape and Ctrl+C until the CLI's process is gone, waits for a CLI that has not started yet, and types nothing until the shell is at its prompt (accounts/exit.rs).
pub(crate) fn build_draft_agent_exit_steps() -> Vec<crate::session_chat_send::SessionChatSendStep> {
    vec![
        crate::session_chat_send::SessionChatSendStep::InterruptAgentForAccountSwitch {
            home_dir: crate::paths::get_gxserver_paths(None).home_dir,
            timeout_ms: DRAFT_SWITCH_EXIT_TIMEOUT_MS,
        },
    ]
}

fn next_draft_agent_title(
    project: &Value,
    session: &Value,
    next_title: Option<&str>,
) -> Option<String> {
    let next_title = next_title?;
    let current_title = read_text_value(session, "title")?;
    let previous_default = project_agent_session_default_title(project, session);
    let is_placeholder = session
        .pointer("/runtimeSettings/titleSource")
        .and_then(Value::as_str)
        == Some("placeholder");
    ((is_placeholder || current_title == previous_default) && current_title != next_title)
        .then(|| next_title.to_string())
}
