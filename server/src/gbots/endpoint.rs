//! `/api/gbots {op, …}`: what clients and the `ghostex gbots` CLI ask of GBots.
//!
//! | op | params | who |
//! | --- | --- | --- |
//! | `list` | | anyone here |
//! | `show` | `bot` | anyone here |
//! | `chats` | `bot` | anyone here |
//! | `brief` | `bot?` (a bot's own chat needs none) | the bot's own chats, you, Chief of Staff |
//! | `create` | `name`, `title?`, `description?`, `soul?`, `icon?`, `color?`, `agentId?`, `model?`, `effort?`, `approval?`, `department?`, `lead?` | you, Chief of Staff |
//! | `edit` | `bot` and the fields to change; `department` (a department, or "" for none) and `lead` move it | you, Chief of Staff; a department's lead for its members' `soul` only |
//! | `departments`, `department` | `department` | anyone here |
//! | `departmentCreate`, `departmentEdit`, `departmentRemove` | departments.rs | you, Chief of Staff |
//! | `remove` | `bot` | you, Chief of Staff |
//! | `newChat` | `bot`, `task?` | you, Chief of Staff |
//! | `closeChat` | `chat` (a session ref) | you; Chief of Staff for the chats it opened |
//! | `ask` | `bot`, `task`, `consult?`, `chat?` (`new`, `standing` or a ref), `doneWhen?`; or `rework` (an assignment id) and `task` | you, Chief of Staff, GBots and other coordinators (assign.rs says who may hand work) |
//! | `assignments` | `all?` | the caller's own; `all` for you and Chief of Staff |
//! | `changes` | | anyone here |
//! | `change` | `change` | anyone here (the "GBot added" card reads it) |
//! | `undo` | `change` | you; Chief of Staff for its own changes |
//! | `routines`, `routineAdd`, `routineEdit`, `routinePause`, `routineResume`, `routineRemove`, `routineRunNow` | routines.rs | you, Chief of Staff, the GBot's own chats |
//! | `memory…`, `prefs…` | see memory/ops.rs | the bot's own chats, you, Chief of Staff |
//! | `problems` (`all?`), `problemRetry`, `problemRebrief` (`text`), `problemClose` (`handed?`) | `problem` | you, Chief of Staff (problems/ops.rs) |
//! | `flag` | `text` | a GBot's own chat |
//! | `dayWatch`; `dayWatchOn` (`briefTime?`, `timezone?`), `dayWatchOff` | day_watch/ops.rs | anyone here; you, Chief of Staff |
//! | `api`; `apiOn` (`label?`), `apiNewKey` (`label?`), `apiOff` | `bot` (api/ops.rs) | anyone here; you alone, and on and New key ask your approval |
//! | `computer`; `computerSet` (`computer`: `this`, a connected computer or a Cloud Box; `folder?`, `computerUse?`) | `bot` (computer/ops.rs) | anyone here; you alone from Ghostex on your computer, except giving it this computer back |
//! | `voice`; `voiceSet` (`voice`, or "" for the Voice agent's own) | `bot` (call.rs) | anyone here; you, Chief of Staff |
//! | `teamExport`, `teamPreview`, `teamImport`, `presets`, `preset`, `presetSave`, `presetRemove`, `presetExport` | team_file/mod.rs | team_file/mod.rs; importing is yours alone |
//!
//! `create` also takes `preset` (a preset's id or name): what it leaves out comes from the preset.
//!
//! Every op is refused while GBots are off (feature.rs). An approval level above `ask` is set from
//! your own computer or phone only.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User (Q-D A): "the user and Chief of Staff" may create and edit bots; a department lead edits only its own members' instructions (departments come later). Other agents are refused with one sentence; "you" is the owner's devices and the `ghostex` CLI in your own terminal (the token file), which also asks for your approval before adding a bot (server/src/server/security_gates.rs). Chief of Staff adds bots without asking (Q2103 A). Letting a bot do more than ask first (approval `edits`, `auto`, `full`) is yours alone, like the other things only you can set.

use std::sync::Arc;

use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::record::{
    bot_summary, delete_bot, insert_bot, list_bots, new_id, read_bot, record_change, require_bot,
    update_bot, Bot, BotFields, DEFAULT_ICON, MAX_BOTS,
};
use crate::coordinators::now_iso;
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::{rpc_error, rpc_success};
use crate::security::Principal;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

pub(crate) const GBOTS_ENDPOINT: &str = "/api/gbots";

/// Who is asking, as GBots' rules see it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Asker {
    /// The owner's computer or phone.
    OwnerDevice,
    /// The `ghostex` CLI in the user's own terminal (the token file).
    Terminal,
    /// Chief of Staff, with its head role.
    Chief,
    /// Any other agent session.
    Agent,
    /// Anyone else (a connected computer, an extension).
    Other,
}

impl Asker {
    fn of(caller: &Principal) -> Self {
        match caller {
            Principal::Owner { .. } | Principal::Device { .. } => Self::OwnerDevice,
            Principal::LocalAgent => Self::Terminal,
            caller if caller.is_head() => Self::Chief,
            Principal::Session { .. } => Self::Agent,
            _ => Self::Other,
        }
    }

    pub(super) fn is_user(self) -> bool {
        matches!(self, Self::OwnerDevice | Self::Terminal)
    }

    pub(super) fn may_manage(self) -> bool {
        self.is_user() || self == Self::Chief
    }

    /// `createdBy` and `openedBy`.
    pub(super) fn label(self) -> &'static str {
        if self == Self::Chief {
            "chief"
        } else {
            "user"
        }
    }
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

pub(crate) async fn handle_gbots_http(
    state: Arc<AppState>,
    caller: Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    // A GBot chat on this computer whose GBot lives on another one reads its brief and keeps its
    // memory there (computer/home.rs), whether GBots are on here or not.
    super::computer::remember_state(&state);
    if let Some(answer) =
        super::computer::forward_home(&state, &caller, &endpoint_path, &request_id, &params).await
    {
        return answer;
    }
    if !super::gbots_on(&state.paths) {
        return error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "turnedOff",
                message: super::gbots_off_message(&state.paths),
            },
        );
    }
    // A retry restarts a chat, which awaits gxserver's own lifecycle calls (problems/ops.rs).
    if text(&params, "op") == Some("problemRetry") {
        let may_manage = Asker::of(&caller).may_manage();
        return match super::problems::retry(state, may_manage, params).await {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => error_response(endpoint_path, request_id, error),
        };
    }
    let outcome = tokio::task::spawn_blocking(move || run(&state, &caller, &params)).await;
    match outcome {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Ok(Err(error)) => error_response(endpoint_path, request_id, error),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("GBots could not answer: {error}"),
            },
        ),
    }
}

/// GBots' own refusals carry their HTTP status (409 while GBots are off); everything else is a
/// domain error as usual.
fn error_response(
    endpoint_path: String,
    request_id: String,
    error: DomainStateError,
) -> RoutedResponse {
    if error.code != "turnedOff" {
        return domain_error_response(endpoint_path, request_id, error);
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::CONFLICT,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    text(params, key).ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

fn run(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let asker = Asker::of(caller);
    let op = text(params, "op").unwrap_or("list");
    match op {
        "list" => list(state, &db, &repository),
        "show" => show(state, &db, &repository, required(params, "bot")?),
        "chats" => {
            let bot = require_bot(&db, required(params, "bot")?)?;
            chats(state, &repository, &bot)
        }
        "brief" => brief(state, &db, caller, asker, text(params, "bot")),
        "create" => {
            if !asker.may_manage() {
                return Err(forbidden(
                    "Only you and Chief of Staff can add GBots. Ask Chief of Staff for one.",
                ));
            }
            // Fields left out come from a preset (team_file/presets.rs), its starter notes after.
            let (params, preset) = super::team_file::with_preset(&state.paths, params)?;
            let answer = create(state, &db, &repository, asker, &params, asker.label())?;
            super::team_file::after_preset_create(state, &db, &answer, preset.as_ref(), asker.label());
            Ok(answer)
        }
        "edit" => {
            let mut by = asker.label().to_string();
            if !asker.may_manage() {
                // A department's lead changes its own members' instructions (department_ops.rs).
                let own = own_bot(&db, caller)?;
                super::department_ops::check_lead_edit(&db, own.as_ref(), params)?;
                if let Some(own) = own {
                    by = format!("bot:{}", own.bot_id);
                }
            }
            edit(state, &db, &repository, asker, &by, params)
        }
        op if super::department_ops::handles(op) => super::department_ops::run(
            state,
            &db,
            &repository,
            asker.may_manage(),
            asker.label(),
            op,
            params,
        ),
        "remove" => {
            if !asker.may_manage() {
                return Err(forbidden(
                    "Only you and Chief of Staff can remove a GBot.",
                ));
            }
            remove(state, &db, &repository, asker, required(params, "bot")?)
        }
        "newChat" => {
            if !asker.may_manage() {
                return Err(forbidden(
                    "Only you and Chief of Staff can open a chat with a GBot.",
                ));
            }
            let bot = require_bot(&db, required(params, "bot")?)?;
            let (session, started) = super::session::create_bot_chat(
                state,
                &db,
                &repository,
                &bot,
                asker.label(),
                text(params, "task"),
            )?;
            Ok(json!({
                "bot": bot_summary(&bot),
                "chat": super::session::chat_summary(&state.metadata.server_id, &session),
                "providerStarted": started,
            }))
        }
        "closeChat" => close_chat(state, &db, &repository, asker, required(params, "chat")?),
        "ask" => {
            let sender = super::assign::Sender::of(
                &db,
                caller_session(caller),
                asker.is_user(),
                asker == Asker::Chief,
            )?;
            super::assign::ask(state, &db, &repository, sender, params)
        }
        "assignments" => {
            let all = params.get("all").and_then(Value::as_bool) == Some(true);
            let sender = super::assign::Sender::of(
                &db,
                caller_session(caller),
                asker.is_user(),
                asker == Asker::Chief,
            )
            .ok();
            super::assign::assignments(&db, sender.as_ref(), all && asker.may_manage())
        }
        "changes" => {
            let changes = super::changes::list_changes(&db, 50)?
                .iter()
                .map(|change| super::changes::change_summary(&db, change))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({ "changes": changes }))
        }
        "change" => {
            let change = require_change(&db, required(params, "change")?)?;
            Ok(json!({ "change": super::changes::change_summary(&db, &change)? }))
        }
        "undo" => {
            let change = require_change(&db, required(params, "change")?)?;
            let allowed = asker.is_user() || (asker == Asker::Chief && change.by == "chief");
            if !allowed {
                return Err(forbidden(if asker == Asker::Chief {
                    "Chief of Staff undoes only the changes it made; the user undoes the rest."
                } else {
                    "Only you and Chief of Staff undo a change to GBots."
                }));
            }
            if super::team_file::is_team_change(&change.kind) {
                return super::team_file::undo(&db, &change, asker.label(), &mut |bot_id| {
                    remove(state, &db, &repository, asker, bot_id)
                });
            }
            super::changes::undo(
                &db,
                &change,
                |bot_id| remove(state, &db, &repository, asker, bot_id),
                |before| {
                    let mut restore = Map::new();
                    restore.insert("bot".into(), json!(before.bot_id));
                    for (key, value) in [
                        ("name", json!(before.name)),
                        ("title", json!(before.title)),
                        ("description", json!(before.description)),
                        ("soul", json!(before.soul)),
                        ("icon", json!(before.icon)),
                        ("color", json!(before.color)),
                        ("agentId", json!(before.agent_id)),
                        ("model", json!(before.model)),
                        ("effort", json!(before.effort)),
                        ("approval", json!(before.approval)),
                    ] {
                        restore.insert(key.into(), value);
                    }
                    // Its department too, unless that department is gone since.
                    let department = before
                        .department_id
                        .as_deref()
                        .and_then(|id| super::departments::read_department(&db, id).ok().flatten());
                    restore.insert(
                        "department".into(),
                        json!(department.map(|department| department.department_id).unwrap_or_default()),
                    );
                    restore.insert("lead".into(), json!(before.is_lead));
                    edit(state, &db, &repository, asker, asker.label(), &restore)
                },
            )
        }
        op if super::memory::handles(op) => {
            let writes = !matches!(
                op,
                "memoryShow" | "memoryTopics" | "memoryTopic" | "memoryLog" | "memoryJournal" | "prefsShow"
            );
            if let Some(key) = api_chat_key(&db, &repository, caller) {
                if writes {
                    return Err(forbidden(
                        "A chat of the GBot API doesn't save memory: its messages come from outside Ghostex. Tell the user what's worth keeping instead.",
                    ));
                }
                if !key.is_some_and(|key| key.share_memory) {
                    return Err(forbidden(
                        "This GBot's memory isn't shared with apps: a chat of the GBot API can't read it. The user can turn on Share memory with apps in GBot Settings > Bot API.",
                    ));
                }
            }
            let memory_caller = super::memory::MemoryCaller {
                own_bot: own_bot(&db, caller)?,
                may_manage: asker.may_manage(),
                label: asker.label(),
            };
            super::memory::run(state, &db, &repository, memory_caller, op, params)
        }
        op if super::problems::handles(op) => {
            let own_chat = match (own_bot(&db, caller)?, caller_session(caller)) {
                (Some(bot), Some((project, session))) => {
                    Some((bot.bot_id, project.to_string(), session.to_string()))
                }
                _ => None,
            };
            super::problems::run(
                state,
                &db,
                &repository,
                asker.may_manage(),
                asker.label(),
                own_chat,
                op,
                params,
            )
        }
        // Team files and presets (team_file/).
        op if super::team_file::handles(op) => super::team_file::run(
            state,
            &db,
            &repository,
            super::team_file::TeamCaller {
                is_user: asker.is_user(),
                may_manage: asker.may_manage(),
                label: asker.label(),
            },
            op,
            params,
            &mut |fields, created_by| create(state, &db, &repository, asker, fields, created_by),
            &mut |bot_id| remove(state, &db, &repository, asker, bot_id),
        ),
        // Its voice for calls (call.rs).
        op if super::call::handles(op) => super::call::run(state, &db, asker.may_manage(), op, params),
        // The GBot API: is it on, turn it on or off, a new key (api/ops.rs).
        op if super::api::handles(op) => super::api::run(state, &db, caller, asker.is_user(), op, params),
        // Where its chats run (computer/).
        op if super::computer::handles(op) => super::computer::run(
            state,
            &db,
            &repository,
            super::computer::ComputerCaller {
                owner_device: asker == Asker::OwnerDevice,
                may_manage: asker.may_manage(),
            },
            op,
            params,
        ),
        // "Keep an eye on my day" (day_watch/).
        op if super::day_watch::handles(op) => super::day_watch::run(state, &db, &repository, asker, op, params),
        // A GBot's routines (routines.rs).
        op if op.starts_with("routine") => super::routines::run(state, &db, &repository, caller, asker, op, params),
        other => Err(DomainStateError::bad_request(format!(
            "Unknown GBots op {other}: use list, show, chats, brief, create, edit, remove, newChat, closeChat, ask, assignments, changes, change, undo, a department op (departments, departmentCreate, …), a memory op (memoryShow, memoryAdd, …), a routine op, a problem op (problems, problemRetry, problemRebrief, problemClose), flag, a team op (teamExport, teamPreview, teamImport) or a preset op (presets, preset, presetSave, presetRemove, presetExport)."
        ))),
    }
}

fn list(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    let departments = super::departments::list_departments(db)?;
    let bots = list_bots(db)?
        .iter()
        .map(|bot| {
            let mut summary = bot_summary(bot);
            summary["departmentName"] = json!(bot
                .department_id
                .as_deref()
                .and_then(|id| departments
                    .iter()
                    .find(|department| department.department_id == id))
                .map(|department| department.name.clone()));
            summary["folder"] = json!(super::folder::bot_folder(&state.paths, &bot.bot_id));
            summary["openChats"] = json!(
                super::session::bot_chats(repository, bot)
                    .map(|chats| chats.len())
                    .unwrap_or(0)
                    + super::computer::list_remote_chats(db, Some(&bot.bot_id))
                        .map(|chats| chats.len())
                        .unwrap_or(0)
            );
            summary["computerInfo"] = super::computer::computer_status(state, db, &bot.bot_id);
            summary["voice"] = json!(super::call::read_voice(db, &bot.bot_id));
            summary["voiceChoices"] = crate::voice_agent::gbot_voice_choices(&state.paths);
            summary
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "bots": bots,
        "departments": super::departments::list(db)?["departments"].take(),
    }))
}

fn show(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    reference: &str,
) -> Result<Value, DomainStateError> {
    let bot = require_bot(db, reference)?;
    let mut summary = bot_summary(&bot);
    summary["soul"] = json!(bot.soul);
    summary["folder"] = json!(super::folder::bot_folder(&state.paths, &bot.bot_id));
    if let Some(department) = bot
        .department_id
        .as_deref()
        .map(|id| super::departments::read_department(db, id))
        .transpose()?
        .flatten()
    {
        summary["department"] =
            super::departments::department_summary(&department, &list_bots(db)?);
        summary["departmentName"] = json!(department.name);
    }
    let drift = super::folder::soul_drift(&state.paths, &bot);
    summary["soulDrift"] = json!(drift);
    if let Some(drift) = drift {
        summary["soulDriftMessage"] = json!(format!(
            "SOUL.md was {drift} outside Ghostex, so {} still follows the instructions saved in Ghostex. Save new instructions with `ghostex gbots edit {} --soul-file <file>`.",
            bot.name, bot.name
        ));
    }
    summary["chats"] = chats(state, repository, &bot)?["chats"].take();
    summary["computerInfo"] = super::computer::computer_status(state, db, &bot.bot_id);
    summary["voice"] = json!(super::call::read_voice(db, &bot.bot_id));
    summary["voiceChoices"] = crate::voice_agent::gbot_voice_choices(&state.paths);
    summary["remoteChats"] = json!(super::computer::list_remote_chats(db, Some(&bot.bot_id))?
        .iter()
        .map(super::computer::remote_chat_summary)
        .collect::<Vec<_>>());
    Ok(json!({ "bot": summary }))
}

fn chats(
    state: &AppState,
    repository: &DomainRepository<'_>,
    bot: &Bot,
) -> Result<Value, DomainStateError> {
    let chats: Vec<Value> = super::session::bot_chats(repository, bot)?
        .iter()
        .map(|session| super::session::chat_summary(&state.metadata.server_id, session))
        .collect();
    Ok(json!({ "bot": bot_summary(bot), "chats": chats }))
}

/// The project and session of a caller that is a session.
fn caller_session(caller: &Principal) -> Option<(&str, &str)> {
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => Some((project_id.as_str(), session_id.as_str())),
        _ => None,
    }
}

fn require_change(
    db: &Connection,
    change_id: &str,
) -> Result<super::changes::Change, DomainStateError> {
    super::changes::read_change(db, change_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no GBots change {change_id}. `ghostex gbots changes` lists them."
        ))
    })
}

/// The bot a caller's own session is a chat of.
/// When the caller is one of the GBot API's chats (api/chat.rs): `Some` with its key (`None`
/// inside when the key is gone); `None` for any other caller.
fn api_chat_key(
    db: &Connection,
    repository: &DomainRepository<'_>,
    caller: &Principal,
) -> Option<Option<super::api::BotKey>> {
    let session = caller_session(caller)
        .and_then(|(project, session)| repository.get_session(project, session).ok().flatten())?;
    let (_, opened_by) = super::session::chat_of(&session)?;
    let key_id = opened_by.strip_prefix("api:")?;
    Some(super::api::key_by_id(db, key_id))
}

pub(super) fn own_bot(
    db: &Connection,
    caller: &Principal,
) -> Result<Option<Bot>, DomainStateError> {
    let Principal::Session { project_id, .. } = caller else {
        return Ok(None);
    };
    super::record::bot_for_project(db, project_id)
}

fn brief(
    state: &AppState,
    db: &Connection,
    caller: &Principal,
    asker: Asker,
    reference: Option<&str>,
) -> Result<Value, DomainStateError> {
    let own = own_bot(db, caller)?;
    let bot = match (reference, own) {
        (None, Some(own)) => own,
        (Some(reference), Some(own)) => {
            let named = require_bot(db, reference)?;
            if named.bot_id != own.bot_id && !asker.may_manage() {
                return Err(forbidden("A GBot reads only its own brief."));
            }
            named
        }
        (Some(reference), None) => {
            if !asker.may_manage() {
                return Err(forbidden(
                    "Only a GBot's own chats, you and Chief of Staff read its brief.",
                ));
            }
            require_bot(db, reference)?
        }
        (None, None) => {
            return Err(DomainStateError::bad_request(
                "This isn't a GBot's chat: name the GBot with --bot.",
            ))
        }
    };
    let team = super::brief::Team::load(db)?;
    let repository = DomainRepository::new(db, &state.metadata.server_id);
    // A GBot API chat reads the API brief: no memory or preferences unless shared (brief.rs).
    let brief = match api_chat_key(db, &repository, caller) {
        Some(key) => super::brief::api_brief_text(
            &state.paths,
            &bot,
            &team,
            key.is_some_and(|key| key.share_memory),
        ),
        None => super::brief::brief_text(&state.paths, &bot, &team),
    };
    Ok(json!({
        "bot": bot_summary(&bot),
        "brief": brief,
    }))
}

/// Refuses an approval level above `ask` unless the owner's own device sets it.
fn check_approval(asker: Asker, fields: &BotFields) -> Result<(), DomainStateError> {
    if fields.raises_approval() && asker != Asker::OwnerDevice {
        return Err(forbidden(
            "Only you can let a GBot do more than ask first: set its approval level from Ghostex on your computer or phone.",
        ));
    }
    Ok(())
}

pub(super) fn create(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    asker: Asker,
    params: &Map<String, Value>,
    created_by: &str,
) -> Result<Value, DomainStateError> {
    let fields = BotFields::from_params(params);
    if fields
        .name
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
    {
        return Err(DomainStateError::bad_request("Give the GBot a name."));
    }
    check_approval(asker, &fields)?;
    if asker == Asker::Chief {
        super::changes::check_chief_add_caps(db)?;
    }
    let others = list_bots(db)?;
    if others.len() >= MAX_BOTS {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "This computer has {MAX_BOTS} GBots, the most it can have. Remove one first."
            ),
        });
    }
    let rows = super::session::bot_agent_rows(repository)?;
    let agent_id = match fields.agent_id.as_deref().map(str::trim) {
        Some(agent_id) if !agent_id.is_empty() => agent_id.to_string(),
        _ => crate::coordinators::default_coordinator_launcher(&rows).ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "No agent a GBot can run on is set up here. Set up {} first.",
                crate::coordinators::coordinator_agent_families_text()
            ))
        })?,
    };
    super::session::check_agent(&rows, &agent_id)?;
    let approval = super::session::default_approval(&rows, &agent_id).to_string();
    let now = now_iso();
    let mut bot = Bot {
        bot_id: new_id("b_"),
        name: String::new(),
        title: String::new(),
        description: String::new(),
        soul: String::new(),
        icon: DEFAULT_ICON.to_string(),
        color: super::record::COLORS[0].0.to_string(),
        agent_id,
        model: None,
        effort: None,
        approval,
        department_id: None,
        is_lead: false,
        computer: "this".to_string(),
        project_id: String::new(),
        work_folder: None,
        memory_enabled: true,
        paused_at: None,
        created_by: created_by.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    fields.apply(&mut bot, &others)?;
    let placement = super::departments::Placement::from_params(db, params)?;
    if placement.lead == Some(true) && placement.department.as_ref().is_none_or(Option::is_none) {
        return Err(DomainStateError::bad_request(
            "A GBot leads the department it is in: pass --department with --lead.",
        ));
    }
    let folder =
        super::folder::create_folder(&state.paths, &bot).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not create the GBot's folder: {error}"),
        })?;
    insert_bot(db, &bot)?;
    super::presentation::refresh_presented_bots(db);
    let project = match super::project::add_bot_project(state, db, repository, &bot, &folder) {
        Ok(project) => project,
        Err(error) => {
            // Nothing of the user's is in a folder made a moment ago.
            let _ = delete_bot(db, &bot.bot_id);
            let _ = std::fs::remove_dir_all(&folder);
            super::presentation::refresh_presented_bots(db);
            return Err(error);
        }
    };
    bot.project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    update_bot(db, &bot)?;
    if placement.apply(db, &bot)? {
        bot = read_bot(db, &bot.bot_id)?.unwrap_or(bot);
    }
    let change_id = record_change(db, "botAdded", &bot.bot_id, None, Some(&bot), asker.label())?;
    super::brief::refresh_all_instruction_files(&state.paths, db);
    let mut summary = bot_summary(&bot);
    summary["folder"] = json!(folder);
    let mut answer = json!({ "bot": summary, "changeId": change_id });
    // Chief of Staff's adds are told to the user in one card in its chat (card.rs).
    if asker == Asker::Chief {
        answer["chatCard"] = super::card::gbot_added_card(&change_id, &bot);
    }
    Ok(answer)
}

fn edit(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    asker: Asker,
    by: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let before = require_bot(db, required(params, "bot")?)?;
    let fields = BotFields::from_params(params);
    let placement = super::departments::Placement::from_params(db, params)?;
    check_approval(asker, &fields)?;
    if let Some(agent_id) = fields.agent_id.as_deref() {
        super::session::check_agent(
            &super::session::bot_agent_rows(repository)?,
            agent_id.trim(),
        )?;
    }
    let others = list_bots(db)?;
    let mut bot = before.clone();
    fields.apply(&mut bot, &others)?;
    let persona_changed = bot != before;
    if persona_changed {
        bot.updated_at = now_iso();
        update_bot(db, &bot)?;
    }
    if placement.apply(db, &bot)? {
        bot = read_bot(db, &bot.bot_id)?.unwrap_or(bot);
    }
    if bot == before {
        return Ok(json!({ "bot": bot_summary(&bot), "changed": false }));
    }
    let moved = bot.department_id != before.department_id || bot.is_lead != before.is_lead;
    if bot.soul != before.soul {
        super::folder::write_soul(&state.paths, &bot).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Saved, but SOUL.md could not be written: {error}"),
        })?;
    }
    // Open chats are told to read their brief again (memory/notify.rs): its own instructions, or
    // its department's, changed.
    if bot.soul != before.soul || bot.department_id != before.department_id {
        super::memory::tell_open_chats(state, repository, &bot);
    }
    if bot.name != before.name && !bot.project_id.is_empty() {
        super::project::rename_bot_project(state, db, repository, &bot)?;
    }
    let change_id = record_change(db, "botEdited", &bot.bot_id, Some(&before), Some(&bot), by)?;
    super::presentation::refresh_presented_bots(db);
    super::brief::refresh_all_instruction_files(&state.paths, db);
    if moved {
        // Another bot may have stopped leading the department this one now leads.
        let others: Vec<String> = list_bots(db)?
            .into_iter()
            .filter(|other| {
                other.department_id.is_some()
                    && (other.department_id == bot.department_id
                        || other.department_id == before.department_id)
            })
            .map(|other| other.bot_id)
            .collect();
        super::department_ops::redraw(state, db, repository, &others);
    }
    if !bot.project_id.is_empty() {
        let _ = crate::server::schedule_presentation_project_delta(
            state,
            db,
            repository,
            &bot.project_id,
            "projectUpdated",
        );
    }
    Ok(json!({ "bot": bot_summary(&bot), "changed": true, "changeId": change_id }))
}

fn remove(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    asker: Asker,
    reference: &str,
) -> Result<Value, DomainStateError> {
    let bot = require_bot(db, reference)?;
    let chats = super::session::bot_chats(repository, &bot)?;
    for chat in &chats {
        let text = |key: &str| {
            chat.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        super::session::close_chat(state, &text("projectId"), &text("sessionId"))?;
    }
    // Its chats on another computer close there too, when it is connected.
    let closed_elsewhere = super::computer::close_remote_chats(db, &bot.bot_id);
    if let Some(project) = super::project::bot_project(repository, &bot)? {
        if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
            super::project::remove_bot_project(state, db, repository, project_id)?;
        }
    }
    delete_bot(db, &bot.bot_id)?;
    super::departments::sync_leads(db)?;
    let change_id = record_change(
        db,
        "botRemoved",
        &bot.bot_id,
        Some(&bot),
        None,
        asker.label(),
    )?;
    super::presentation::refresh_presented_bots(db);
    super::brief::refresh_all_instruction_files(&state.paths, db);
    let folder = super::folder::bot_folder(&state.paths, &bot.bot_id);
    let trashed = if folder.exists() {
        super::trash::move_to_trash(
            &state.paths.home_dir,
            &folder,
            &format!("GBot {}", bot.name),
        )
    } else {
        Ok(())
    };
    Ok(json!({
        "removed": bot_summary(&bot),
        "closedChats": chats.len() + closed_elsewhere,
        "changeId": change_id,
        "folderInTrash": trashed.is_ok() && !folder.exists(),
        "folderError": trashed.err(),
    }))
}

/// `(projectId, sessionId)` of a chat ref: `S…:P…:G…`, `P…:G…`, or a bare session id looked up
/// among the bots' projects.
pub(super) fn chat_key(
    db: &Connection,
    repository: &DomainRepository<'_>,
    reference: &str,
) -> Result<(String, String), DomainStateError> {
    let parts: Vec<&str> = reference.trim().split(':').collect();
    if parts.len() >= 2 {
        let session = parts[parts.len() - 1].to_string();
        let project = parts[parts.len() - 2].to_string();
        return Ok((project, session));
    }
    for bot in list_bots(db)? {
        if repository
            .get_session(&bot.project_id, reference)?
            .is_some()
        {
            return Ok((bot.project_id, reference.to_string()));
        }
    }
    Err(DomainStateError::not_found(format!(
        "There is no GBot chat {reference}. Run `ghostex gbots chats <bot>` to see them."
    )))
}

fn close_chat(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    asker: Asker,
    reference: &str,
) -> Result<Value, DomainStateError> {
    let (project_id, session_id) = chat_key(db, repository, reference)?;
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("There is no chat {reference}.")))?;
    let Some((bot_id, opened_by)) = super::session::chat_of(&session) else {
        return Err(DomainStateError::bad_request(
            "That chat isn't a GBot's. Close it from the sidebar or with `ghostex agents close`.",
        ));
    };
    let allowed = asker.is_user() || (asker == Asker::Chief && opened_by == "chief");
    if !allowed {
        return Err(forbidden(if asker == Asker::Chief {
            "Chief of Staff closes only the GBot chats it opened, never yours."
        } else {
            "Only you, and Chief of Staff for the chats it opened, can close a GBot's chat."
        }));
    }
    let bot = read_bot(db, bot_id)?;
    super::session::close_chat(state, &project_id, &session_id)?;
    Ok(json!({
        "closed": super::session::chat_summary(&state.metadata.server_id, &session),
        "bot": bot.as_ref().map(bot_summary),
    }))
}
