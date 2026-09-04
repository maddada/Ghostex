use super::*;

/*
CDXC:Drafts 2026-08-28 (agent switching):
Only chat-supported agents can be switched to, because the composer that offers
the switch can only read a transcript it has a decoder for. `openclaude` and
`grok-build` are alternate ids for the Claude and Grok families, so they map
onto them rather than adding families of their own; `omp` is its own family
despite sharing Pi's transcript format, because it paints different chrome.
*/
pub(crate) fn chat_supported_base_agent_id(value: Option<&str>) -> Option<&'static str> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "antigravity-cli" => Some("antigravity"),
        "claude" | "openclaude" => Some("claude"),
        "codex" => Some("codex"),
        "cursor" | "cursor-cli" => Some("cursor"),
        "grok" | "grok-build" => Some("grok"),
        "hermes" | "hermes-agent" => Some("hermes-agent"),
        "pi" => Some("pi"),
        "omp" => Some("omp"),
        _ => None,
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
    never touched. The provider script is `<setup>; <agent CLI>;
    exec $SHELL -l`, so interrupting the CLI drops the same pty into its login
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
        "agentActivity",
        "agentSessionId",
        "agentSessionPath",
        "agentCommand",
        "launchAgentId",
        "agentName",
    ] {
        runtime_settings.remove(key);
    }
    let mut launch_settings = object_field(&session, "launchSettings");
    for key in [
        "acceptAllMode",
        "agentCommand",
        "agentLaunchPlan",
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
    let resolved = create_agent_session_params_for_project(db, &project, &create_params)?;
    /*
    The line the live pane will be typed, read out BEFORE the row is written so
    a plan that somehow carries no command leaves the draft exactly as it was
    instead of claiming an agent nobody can launch.
    */
    let reuse_command = provider_exists
        .then(|| draft_switch_reuse_command(&resolved))
        .transpose()?;

    let mut update = lifecycle_update(&lifecycle);
    update.insert("agentId".to_string(), json!(agent_id.clone()));
    update.insert("kind".to_string(), json!("agent"));
    if let Some(value) = resolved.get("launchSettings").cloned() {
        update.insert("launchSettings".to_string(), value);
    }
    /*
    The rebuild's own default would start this row at "working", because a
    create that carries launch startup text treats the launch as work. That is
    right for a session being created to run a first prompt and wrong for a
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
        previous_agent_id.as_deref(),
        resolved.get("title").and_then(Value::as_str),
    ) {
        update.insert("title".to_string(), json!(title));
    }
    let updated = repository.update_session(&update)?;

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
        crate::session_chat_send::enqueue_session_write_sequence(
            &updated,
            &lifecycle.project_id,
            &lifecycle.session_id,
            DRAFT_AGENT_SWITCH_SEND_SOURCE,
            build_draft_agent_switch_steps(&command),
        )?;
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
/// Ctrl+C. Three of these inside one second is the exit gesture the chat-
/// supported CLIs measure (a single one only cancels the current turn).
const DRAFT_SWITCH_INTERRUPT: &str = "\u{3}";
/// Spacing between the interrupts. Wide enough that each is its own stdin
/// chunk, narrow enough that all three land inside the CLIs' one-second window.
const DRAFT_SWITCH_INTERRUPT_SPACING_MS: u64 = 150;
/// Time the interrupted pane gets to become a login shell before the new
/// agent's line is typed into it. This settles a SHELL start (`exec $SHELL -l`
/// is the last line of the provider script), not a TUI repaint, which is why it
/// is several times the chat path's settles.
const DRAFT_SWITCH_SHELL_SETTLE_MS: u64 = 800;

/// The new agent's launch line, taken from the rebuilt plan's `startupText` —
/// already leading-space prefixed so the login shell keeps it out of atuin
/// history — with the plan's own trailing Enter stripped. That Enter is a
/// separate step in the sequence below, and sending both would submit twice:
/// the second one would land in the agent CLI that the first one started.
fn draft_switch_reuse_command(resolved: &Map<String, Value>) -> Result<String, DomainStateError> {
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
    Ok(command.to_string())
}

/*
The keystrokes an agent switch types into a draft's LIVE pane, in the order a
person sitting at that pane would type them, as one indivisible job.

Three interrupts, not one: every chat-supported CLI reads a single Ctrl+C as
"cancel this turn" and only a repeat inside its own short window as "exit", so
one would be swallowed. The burst spans 300ms — comfortably inside the second
those CLIs measure — and the third interrupt's spacing simply folds into the
shell settle that follows it.

The command and its Enter are separate writes for the reason every server-side
writer of an agent pty keeps them separate (see SESSION_CHAT_CLEAR_INPUT_SETTLE_MS
in `session_chat_send`): a submit that coalesces into the body's stdin chunk is
inserted as literal text instead of running it.
*/
fn build_draft_agent_switch_steps(
    command: &str,
) -> Vec<crate::session_chat_send::SessionChatSendStep> {
    use crate::session_chat_send::SessionChatSendStep;
    let mut steps = Vec::new();
    for _ in 0..3 {
        steps.push(SessionChatSendStep::Write(
            DRAFT_SWITCH_INTERRUPT.to_string(),
        ));
        steps.push(SessionChatSendStep::SleepMs(
            DRAFT_SWITCH_INTERRUPT_SPACING_MS,
        ));
    }
    steps.push(SessionChatSendStep::SleepMs(DRAFT_SWITCH_SHELL_SETTLE_MS));
    steps.push(SessionChatSendStep::Write(command.to_string()));
    steps.push(SessionChatSendStep::SleepMs(
        crate::session_chat_send::SESSION_CHAT_SUBMIT_DELAY_MS,
    ));
    steps.push(SessionChatSendStep::Write(
        crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
    ));
    steps
}

fn next_draft_agent_title(
    project: &Value,
    session: &Value,
    previous_agent_id: Option<&str>,
    next_title: Option<&str>,
) -> Option<String> {
    let next_title = next_title?;
    let current_title = read_text_value(session, "title")?;
    let previous_default = create_agent_session_default_title(
        read_text_from_map(
            &resolve_project_agent_config(project, previous_agent_id.unwrap_or_default(), None),
            "name",
        )
        .as_deref(),
        previous_agent_id,
    );
    (current_title == previous_default && current_title != next_title)
        .then(|| next_title.to_string())
}
