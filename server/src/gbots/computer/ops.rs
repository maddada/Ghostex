//! The `/api/gbots` computer ops: `computer` (where a GBot's chats run, and what it could be
//! given) and `computerSet` (give it a computer, a Cloud Box or this computer back, and computer
//! use).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::store::{self, BotComputer, Place};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

pub(crate) fn handles(op: &str) -> bool {
    matches!(op, "computer" | "computerSet")
}

/// Who is asking, as the computer ops see it.
#[derive(Clone, Copy)]
pub(crate) struct ComputerCaller {
    /// The owner's own computer or phone (not the token file, which agents share).
    pub owner_device: bool,
    /// You (any way) or Chief of Staff.
    pub may_manage: bool,
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

/// The folder a GBot's chats work in on another computer when none was picked.
pub(crate) fn default_folder(bot_name: &str) -> String {
    let name: String = bot_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim().trim_matches('.').to_string();
    format!("~/GBots/{}", if name.is_empty() { "GBot" } else { &name })
}

pub(crate) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    caller: ComputerCaller,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let bot = super::super::record::require_bot(
        db,
        text(params, "bot").ok_or_else(|| DomainStateError::bad_request("Pass bot."))?,
    )?;
    match op {
        "computerSet" => set(state, db, repository, caller, &bot, params),
        _ => show(state, db, &bot),
    }
}

/// A connected computer as the Computer tab lists it.
fn computer_row(computer: &crate::connected_computers::ConnectedComputer) -> Value {
    json!({
        "serverId": computer.server_id,
        "name": computer.name,
        "agentAccess": computer.agent_access,
        "computer": Place::Computer(computer.server_id.clone()).wire(),
    })
}

/// A Cloud Box as the Computer tab lists it, with its project's name.
fn box_row(
    repository: &DomainRepository<'_>,
    row: &ghostex_gx_protocol::cloud_box::CloudBox,
) -> Value {
    let project_name = row
        .home_project_id
        .as_deref()
        .and_then(|id| repository.get_project(id).ok().flatten())
        .and_then(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    json!({
        "boxId": row.box_id,
        "name": row.name,
        "state": row.state,
        "provider": row.provider.label(),
        "projectId": row.home_project_id,
        "projectName": project_name,
        "monthUsd": row.cost_month_usd,
        "usdPerHour": row.usd_per_hour,
        "computer": Place::CloudBox(row.box_id.clone()).wire(),
    })
}

/// Where `bot`'s chats run now and whether that computer is there, as `/api/gbots` answers it.
pub(crate) fn computer_status(state: &AppState, db: &Connection, bot_id: &str) -> Value {
    let computer = store::read(db, bot_id).unwrap_or_else(|_| BotComputer::this());
    let mut summary = computer.summary();
    match &computer.place {
        Place::This => {
            summary["available"] = json!(true);
        }
        Place::Computer(server_id) => {
            let live = crate::connected_computers::find(server_id);
            summary["connected"] = json!(live.is_some());
            summary["agentAccess"] =
                json!(live.as_ref().is_some_and(|computer| computer.agent_access));
            summary["available"] =
                json!(live.as_ref().is_some_and(|computer| computer.agent_access));
            if let Some(live) = live {
                summary["name"] = json!(live.name);
            }
        }
        Place::CloudBox(box_id) => {
            let row = crate::cloud_boxes::for_gbots::boxes(state)
                .ok()
                .and_then(|boxes| boxes.into_iter().find(|row| row.box_id == *box_id));
            summary["available"] = json!(row.is_some());
            if let Some(row) = row {
                let db_repository = DomainRepository::new(db, &state.metadata.server_id);
                summary["box"] = box_row(&db_repository, &row);
            }
        }
    }
    summary
}

fn show(
    state: &AppState,
    db: &Connection,
    bot: &super::super::record::Bot,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, &state.metadata.server_id);
    let computers: Vec<Value> = crate::connected_computers::list()
        .iter()
        .map(computer_row)
        .collect();
    let boxes: Vec<Value> = crate::cloud_boxes::for_gbots::boxes(state)
        .unwrap_or_default()
        .iter()
        .filter(|row| row.home_project_id.is_some())
        .map(|row| box_row(&repository, row))
        .collect();
    let spend = crate::cloud_boxes::for_gbots::month_spend(state);
    let chats: Vec<Value> = store::list_remote_chats(db, Some(&bot.bot_id))?
        .iter()
        .map(super::remote_chat::remote_chat_summary)
        .collect();
    Ok(json!({
        "bot": super::super::record::bot_summary(bot),
        "computer": computer_status(state, db, &bot.bot_id),
        "thisComputer": crate::connected_computers::this_computer_name(),
        "choices": { "computers": computers, "cloudBoxes": boxes },
        "cloudSpend": spend,
        "remoteChats": chats,
    }))
}

/*
CDXC:GBots 2026-11-09 DECISION:
Coordinator brief for M12: "Only the user can give a GBot a computer (and starting a Cloud Box or letting it use agent logins already asks for approval: keep those gates)." So giving a GBot another computer, a Cloud Box or computer use is done from the owner's own devices only (GBot Settings > Computer in Ghostex on the computer), like raising its approval level; the `ghostex` CLI in a terminal (and so the phone, which reaches the computer through it over SSH) and Chief of Staff may only give it this computer back and turn computer use off, since turning things off never asks. Since 2026-11-10 your own devices aren't asked for the widening itself (Q2105), and starting a chat on its Cloud Box no longer asks anyone (Q2104), both in security/policy.rs. Before that, starting a chat on its Cloud Box asked for your approval when anyone but you started it (security_gates.rs `gbots_cloud_box_action`).
*/
fn set(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    caller: ComputerCaller,
    bot: &super::super::record::Bot,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if !caller.may_manage {
        return Err(forbidden(
            "Only you can give a GBot a computer: pick it in GBot Settings > Computer in Ghostex on your computer.",
        ));
    }
    let (before, next) = plan(state, db, repository, bot, params)?;
    let widens = widens(&before, &next);
    if widens && !caller.owner_device {
        return Err(forbidden(
            "Only you can give a GBot another computer, a Cloud Box or computer use: pick it in GBot Settings > Computer in Ghostex on your computer.",
        ));
    }
    store::write(db, &bot.bot_id, &next)?;
    super::super::presentation::refresh_presented_bots(db);
    let _ = crate::server::schedule_presentation_project_delta(
        state,
        db,
        repository,
        &bot.project_id,
        "projectUpdated",
    );
    // What loads into its next chats says where it works now.
    super::super::brief::refresh_instruction_files(&state.paths, db, bot);
    let mut answer = show(state, db, bot)?;
    answer["note"] = json!(if next.place == before.place {
        "Saved.".to_string()
    } else {
        format!(
            "{}'s new chats run on {}; chats already open stay where they are.",
            bot.name,
            match next.place {
                Place::This => "this computer".to_string(),
                _ => next.label(),
            }
        )
    });
    Ok(answer)
}

/// The GBot's computer now and as `params` would leave it (refused when it names nothing usable).
fn plan(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &super::super::record::Bot,
    params: &Map<String, Value>,
) -> Result<(BotComputer, BotComputer), DomainStateError> {
    let before = store::read(db, &bot.bot_id)?;
    let before_wire = before.place.wire();
    let wanted = text(params, "computer").unwrap_or(&before_wire);
    let computer_use = params
        .get("computerUse")
        .and_then(Value::as_bool)
        .unwrap_or(before.computer_use);
    let mut next = match wanted {
        "this" | "This computer" | "here" => BotComputer {
            place: Place::This,
            name: None,
            folder: None,
            computer_use,
        },
        wanted => pick(state, repository, &before, bot, wanted, params)?,
    };
    next.computer_use = computer_use;
    if next.computer_use && matches!(next.place, Place::CloudBox(_)) {
        return Err(DomainStateError::bad_request(
            "Computer use needs a computer with a screen: this one or a connected computer, not a Cloud Box.",
        ));
    }
    Ok((before, next))
}

/// Whether going from `before` to `next` lets the GBot do more: another computer or Cloud Box, a
/// new folder there, or computer use.
fn widens(before: &BotComputer, next: &BotComputer) -> bool {
    next.place != Place::This && next.place != before.place
        || (next.computer_use && !before.computer_use)
        || (next.place != Place::This && next.folder != before.folder)
}

/*
CDXC:GBots 2026-11-09 WHY:
The security review of M12 (finding 8): an owner device's click alone gave a GBot another computer, a Cloud Box or computer use, but agents can drive the GUI ("a plain Allow button is never enough", ai/security-tokens-and-approvals.md), and an agent with computer use could click it. So a change that lets a GBot do more asks for the owner's signed approval (Touch ID on the Mac, the phone's lock) even from the owner's own window (security_gates.rs `gbots_computer_action`); giving it this computer back or turning computer use off never asks.
*/
/// The approval a `computerSet` needs: its summary and the GBot's id, or `None` when it lets the
/// GBot do no more than now (or would be refused anyway, which the handler says).
pub(crate) fn widening_approval(
    state: &AppState,
    params: &Map<String, Value>,
) -> Option<(String, String)> {
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let bot = super::super::record::find_bot(&db, text(params, "bot")?).ok()??;
    let (before, next) = plan(state, &db, &repository, &bot, params).ok()?;
    if !widens(&before, &next) {
        return None;
    }
    let mut parts = Vec::new();
    if next.place != Place::This && (next.place != before.place || next.folder != before.folder) {
        parts.push(match (&next.place, &next.folder) {
            (Place::CloudBox(_), _) => format!("run its chats on the Cloud Box {}", next.label()),
            (_, Some(folder)) => format!("run its chats on {} in {folder}", next.label()),
            _ => format!("run its chats on {}", next.label()),
        });
    }
    if next.computer_use && !before.computer_use {
        parts.push("use the screen and apps of the computer it works on".to_string());
    }
    Some((
        format!("Let {} {}", bot.name, parts.join(" and ")),
        bot.bot_id,
    ))
}

/// The computer or Cloud Box `wanted` names (`computer:<id>`, `cloudbox:<id>`, or a name).
fn pick(
    state: &AppState,
    repository: &DomainRepository<'_>,
    before: &BotComputer,
    bot: &super::super::record::Bot,
    wanted: &str,
    params: &Map<String, Value>,
) -> Result<BotComputer, DomainStateError> {
    let lower = wanted.to_lowercase();
    let folder_given = text(params, "folder").map(str::to_string);
    let wanted_place = Place::parse(wanted);
    if !matches!(wanted_place, Place::CloudBox(_)) {
        let id = match &wanted_place {
            Place::Computer(id) => id.clone(),
            _ => wanted.to_string(),
        };
        if let Some(computer) = crate::connected_computers::list()
            .into_iter()
            .find(|computer| computer.server_id == id || computer.name.to_lowercase() == lower)
        {
            if !computer.agent_access {
                return Err(DomainStateError::bad_request(
                    crate::connected_computers::access_off_message(&computer),
                ));
            }
            let place = Place::Computer(computer.server_id.clone());
            let folder = folder_given
                .or_else(|| {
                    (before.place == place)
                        .then(|| before.folder.clone())
                        .flatten()
                })
                .unwrap_or_else(|| default_folder(&bot.name));
            if !(folder.starts_with('/') || folder.starts_with("~/")) {
                return Err(DomainStateError::bad_request(
                    "The folder on that computer is a full path or starts with ~/.",
                ));
            }
            return Ok(BotComputer {
                place,
                name: Some(computer.name.clone()),
                folder: Some(folder),
                computer_use: before.computer_use,
            });
        }
        if matches!(wanted_place, Place::Computer(_)) {
            return Err(DomainStateError::not_found(
                "That computer isn't connected to this one. Connect it in Settings > Remote > Remote machines first.",
            ));
        }
    }
    let box_id = match &wanted_place {
        Place::CloudBox(id) => id.clone(),
        _ => wanted.to_string(),
    };
    let boxes = crate::cloud_boxes::for_gbots::boxes(state)?;
    let Some(row) = boxes
        .iter()
        .find(|row| row.box_id == box_id || row.name.to_lowercase() == lower)
    else {
        let names: Vec<String> = crate::connected_computers::list()
            .iter()
            .map(|computer| computer.name.clone())
            .chain(boxes.iter().map(|row| row.name.clone()))
            .collect();
        return Err(DomainStateError::not_found(if names.is_empty() {
            format!("There's no connected computer or Cloud Box called {wanted}. Connect a computer in Settings > Remote, or start a Cloud Box first.")
        } else {
            format!(
                "There's no connected computer or Cloud Box called {wanted}. Pick one of: {}.",
                names.join(", ")
            )
        }));
    };
    let Some(project_id) = row.home_project_id.as_deref() else {
        return Err(DomainStateError::bad_request(format!(
            "{} belongs to no project here, so a GBot can't work on it.",
            row.name
        )));
    };
    let project_name = repository
        .get_project(project_id)?
        .and_then(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| project_id.to_string());
    Ok(BotComputer {
        place: Place::CloudBox(row.box_id.clone()),
        name: Some(format!("{} ({project_name})", row.name)),
        folder: None,
        computer_use: before.computer_use,
    })
}
