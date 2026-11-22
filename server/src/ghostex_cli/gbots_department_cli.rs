//! `ghostex gbots department …`: GBots' departments (server/src/gbots/departments.rs), and the
//! `--department` / `--lead` flags of `ghostex gbots create` and `edit`.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::{FlagValue, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The department verbs' lines in `ghostex gbots --help`.
pub(crate) const DEPARTMENT_HELP: &str =
    "  ghostex gbots department list [--json]          Departments, their leads and members
  ghostex gbots department show <department> [--json]
  ghostex gbots department create --name <name> [--lead <bot>]
                       [--instructions <text> | --instructions-file <file>] [--icon <icon>]
                                                 Group GBots under a name; every member reads
                                                 its shared instructions, and its lead hands
                                                 the members work
  ghostex gbots department edit <department> [--name <name>] [--lead <bot> | --no-lead]
                       [--instructions <text> | --instructions-file <file>] [--icon <icon>]
                       [--order <n>]
  ghostex gbots department set-lead <department> <bot>
  ghostex gbots department remove <department>   Its GBots stay, in no department
  ghostex gbots edit <bot> --department <department>|none [--lead | --no-lead]
                                                 Move a GBot to a department (or out of one)
";

fn gbots(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/gbots", &params, flags)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `--lead` / `--no-lead` of a bot's create or edit; `None` when neither is passed.
fn lead_flag(flags: &Flags) -> CliResult<Option<bool>> {
    if flags.contains("noLead") {
        return Ok(Some(false));
    }
    match flags.0.get("lead") {
        None => Ok(None),
        Some(FlagValue::Bool(value)) => Ok(Some(*value)),
        Some(FlagValue::Text(value)) => match value.trim().to_lowercase().as_str() {
            "true" | "yes" => Ok(Some(true)),
            "false" | "no" => Ok(Some(false)),
            other => Err(CliError::Other(format!(
                "--lead takes no value (it makes the GBot lead its department); got `{other}`. Put --lead last, or use --lead=true."
            ))),
        },
    }
}

/// Adds `department` and `lead` to a bot's create or edit.
pub(crate) fn placement_flags(flags: &Flags, params: &mut Map<String, Value>) -> CliResult<()> {
    if let Some(department) = flags.string_value("department") {
        params.insert("department".to_string(), json!(department));
    } else if flags.contains("department") {
        return Err(CliError::Other(
            "Name the department: --department <department>, or --department none.".to_string(),
        ));
    }
    if let Some(lead) = lead_flag(flags)? {
        params.insert("lead".to_string(), json!(lead));
    }
    Ok(())
}

fn department_fields(flags: &Flags, params: &mut Map<String, Value>) -> CliResult<()> {
    for (flag, key) in [
        ("name", "name"),
        ("icon", "icon"),
        ("instructions", "instructions"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    if let Some(path) = flags.string_value("instructionsFile") {
        let instructions = std::fs::read_to_string(path)
            .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?;
        params.insert("instructions".to_string(), json!(instructions));
    }
    if flags.contains("noLead") {
        params.insert("lead".to_string(), json!(""));
    } else if let Some(lead) = flags.string_value("lead") {
        params.insert("lead".to_string(), json!(lead));
    }
    if let Some(order) = flags.string_value("order") {
        let order: i64 = order
            .trim()
            .parse()
            .map_err(|_| CliError::Other("--order takes a whole number.".to_string()))?;
        params.insert("sortOrder".to_string(), json!(order));
    }
    Ok(())
}

fn department_arg(rest: &[String], verb: &str) -> CliResult<String> {
    rest.get(2).cloned().ok_or_else(|| {
        CliError::Other(format!(
            "Name the department: ghostex gbots department {verb} <department>. `ghostex gbots department list` lists them."
        ))
    })
}

/// `ghostex gbots department <verb> …`; `rest[0]` is `department`.
pub(crate) fn run_department(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("list");
    let answer = match verb {
        "list" => gbots(json!({ "op": "departments" }), flags)?,
        "show" => gbots(
            json!({ "op": "department", "department": department_arg(rest, verb)? }),
            flags,
        )?,
        "create" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("departmentCreate"));
            department_fields(flags, &mut params)?;
            if let Some(name) = rest.get(2).filter(|_| !params.contains_key("name")) {
                params.insert("name".into(), json!(name));
            }
            gbots(Value::Object(params), flags)?
        }
        "edit" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("departmentEdit"));
            params.insert("department".into(), json!(department_arg(rest, verb)?));
            department_fields(flags, &mut params)?;
            gbots(Value::Object(params), flags)?
        }
        "set-lead" => {
            let department = department_arg(rest, verb)?;
            let bot = rest.get(3).cloned().ok_or_else(|| {
                CliError::Other(
                    "Name the GBot: ghostex gbots department set-lead <department> <bot>."
                        .to_string(),
                )
            })?;
            gbots(
                json!({ "op": "departmentEdit", "department": department, "lead": bot }),
                flags,
            )?
        }
        "remove" => gbots(
            json!({ "op": "departmentRemove", "department": department_arg(rest, verb)? }),
            flags,
        )?,
        other => {
            return Err(CliError::Other(format!(
                "unknown department command `{other}`: use list, show, create, edit, set-lead or remove.\n\n{DEPARTMENT_HELP}"
            )))
        }
    };
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    match verb {
        "list" => print_list(&answer),
        "show" => print_department(&answer["department"]),
        "create" => {
            let department = &answer["department"];
            println!(
                "Added the department {} ({}).",
                text(department, "name"),
                text(department, "departmentId")
            );
            print_department(department);
        }
        "edit" | "set-lead" => {
            let department = &answer["department"];
            if answer["changed"].as_bool() == Some(true) {
                println!(
                    "Saved {}. Its GBots' new chats start with it; open chats pick it up when they run `ghostex gbots brief`.",
                    text(department, "name")
                );
            } else {
                println!("Nothing changed for {}.", text(department, "name"));
            }
            print_department(department);
        }
        "remove" => {
            let members: Vec<&str> = answer["members"]
                .as_array()
                .map(|members| members.iter().map(|bot| text(bot, "name")).collect())
                .unwrap_or_default();
            let left = if members.is_empty() {
                String::new()
            } else {
                let verb = if members.len() == 1 { "stays" } else { "stay" };
                format!(" {} {verb}, in no department.", members.join(", "))
            };
            println!(
                "Removed the department {}.{left}",
                text(&answer["removed"], "name")
            );
        }
        _ => print_json(&answer),
    }
    Ok(())
}

fn member_names(department: &Value) -> String {
    let members: Vec<String> = department["members"]
        .as_array()
        .map(|members| {
            members
                .iter()
                .map(|bot| {
                    if bot["isLead"].as_bool() == Some(true) {
                        format!("{} (lead)", text(bot, "name"))
                    } else {
                        text(bot, "name").to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if members.is_empty() {
        "no GBots yet".to_string()
    } else {
        members.join(", ")
    }
}

fn print_list(answer: &Value) {
    let departments = answer["departments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if departments.is_empty() {
        println!(
            "No departments yet. Add one with `ghostex gbots department create --name <name>`."
        );
    }
    for department in &departments {
        println!(
            "{}\t{}\t{}",
            text(department, "name"),
            text(department, "departmentId"),
            member_names(department)
        );
    }
    let unassigned: Vec<&str> = answer["unassigned"]
        .as_array()
        .map(|bots| bots.iter().map(|bot| text(bot, "name")).collect())
        .unwrap_or_default();
    if !unassigned.is_empty() {
        println!("In no department: {}", unassigned.join(", "));
    }
}

fn print_department(department: &Value) {
    println!(
        "{} ({})",
        text(department, "name"),
        text(department, "departmentId")
    );
    let lead = text(&department["lead"], "name");
    println!("Lead: {}", if lead.is_empty() { "none" } else { lead });
    println!("Members: {}", member_names(department));
    let instructions = text(department, "instructions");
    if !instructions.trim().is_empty() {
        println!("\nShared instructions:\n{instructions}");
    }
}
