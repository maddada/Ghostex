//! `ghostex visual guide`: the whole guide to Ghostex visuals, Ghostex forms and Live pages, built
//! into the CLI, and `ghostex visual boosts`: the Boosts' prompts (`/api/boosts`).
//!
//! CDXC:Boosts 2026-11-20 DECISION:
//! User (Q4217 on the Boosts page): agents learn how to make these from the CLI help, not skills ("if we say use ghostex cli to read instructions the agent always gets latest info and he always can read this info, unlike with skills cuz they need to be invoked with /skillname always"), and the help must be "complete enough that a Boost's prompt can just point to it". So the guide is the ghostex-visuals skill's own text, embedded when the CLI is built, split by topic so an agent reads only what its Boost needs; the skill stays for people who invoke it by name, and the two can never differ.
//! CDXC:AgentSkills 2026-11-20 DECISION:
//! User (Q4262 "B"): Ghostex's own skills are retired for CLI help, superseding "the skill stays for people who invoke it by name" above: the ghostex-visuals skill is gone, its text lives on as visual_guide.md beside this file, and `ghostex visual guide` is the only way agents read it (the remaining `ghostex` skill sends them here).
//! SEE-ALSO: server/src/ghostex_cli/visual_guide.md (the text), packages/gx-chat-core/src/extras/boosts.rs (the prompts that point here), server/src/ghostex_cli/agent_guide.rs.

use serde_json::{json, Value};

use super::args::Flags;
use super::rpc::{call_gxserver_rpc, CliError, CliResult};

const SKILL: &str = include_str!("visual_guide.md");
const EXAMPLE: &str = include_str!("visual_guide_example.html");

const GUIDE_HELP: &str = "Usage: ghostex visual guide <topic>

The full guide to showing things in the Ghostex chat. Read the topic you need
before you make one, once per conversation:

  ghostex visual guide visuals   Ghostex visuals: charts, stat tiles, tables
                                 and text layouts in a ```visual block
  ghostex visual guide forms     Ghostex forms: questions in a ```questions
                                 block, and one-click answers
  ghostex visual guide pages     Live pages: self-contained HTML published
                                 with `ghostex show` (mockups, comparisons,
                                 small tools, triage pages, questions with
                                 pictures)
  ghostex visual guide pages export  Exporting a Live page
  ghostex visual guide pages picks   Lists with picks on rows
  ghostex visual guide example   A complete Live page to copy the shape of
  ghostex visual guide all       Everything above but the example";

/// The skill's text after its front matter and comments: the opening (names and when to use
/// which) and one part per `## ` heading.
fn parts() -> (String, Vec<(String, String)>) {
    let body = SKILL
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(SKILL, |(_, body)| body);
    let lines: Vec<&str> = body
        .lines()
        .filter(|line| !line.starts_with("# CDXC:"))
        .collect();
    let mut opening = String::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut in_fence = false;
    for line in lines {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && line.starts_with("## ") {
            sections.push((line[3..].to_string(), String::new()));
        }
        let into = match sections.last_mut() {
            Some((_, text)) => text,
            None => &mut opening,
        };
        into.push_str(line);
        into.push('\n');
    }
    (
        opening.replace("# ghostex-visuals", "# Ghostex visuals guide"),
        sections,
    )
}

/// The words of the skill that name itself, as the CLI says them.
fn as_cli(text: &str) -> String {
    text.replace(
        "`examples/plan-review.html` beside this skill",
        "the example `ghostex visual guide example` prints",
    )
    .replace(
        "older copy of this skill: load the skill again",
        "older guide: run `ghostex visual guide pages` again",
    )
    .replace("this skill", "this guide")
}

/// The part whose heading starts with `heading`.
fn section(heading: &str) -> Option<String> {
    parts()
        .1
        .into_iter()
        .find(|(title, _)| title.starts_with(heading))
        .map(|(_, text)| text)
}

fn guide_text(topic: &str) -> Option<String> {
    let (opening, sections) = parts();
    let text = match topic {
        "visuals" | "visual" | "charts" => format!("{opening}\n{}", section("Ghostex visuals")?),
        "forms" | "form" | "questions" => format!("{opening}\n{}", section("Ghostex forms")?),
        "pages" | "page" | "live-pages" => {
            let pages = section("Live pages")?;
            let (before_export, rest) = pages.split_once("### Exported pages")?;
            let (_, after_export) = rest.split_once("### Live pages with questions")?;
            let (questions, _) =
                after_export.split_once("### Lists you ask about: picks on rows")?;
            format!("{opening}\n{before_export}\n### Live pages with questions{questions}\nFor exports: `ghostex visual guide pages export`. For row picks: `ghostex visual guide pages picks`.\n")
        }
        "pages-export" => {
            let pages = section("Live pages")?;
            let (_, rest) = pages.split_once("### Exported pages")?;
            let (export, _) = rest.split_once("### Live pages with questions")?;
            format!("### Exported pages{export}")
        }
        "pages-picks" => {
            let pages = section("Live pages")?;
            let (_, picks) = pages.split_once("### Lists you ask about: picks on rows")?;
            format!("### Lists you ask about: picks on rows{picks}")
        }
        "example" => return Some(EXAMPLE.to_string()),
        "all" => {
            let mut all = opening;
            for (_, text) in sections {
                all.push('\n');
                all.push_str(&text);
            }
            all
        }
        _ => return None,
    };
    Some(as_cli(&text))
}

pub(crate) fn guide_command(args: &[String]) -> CliResult<()> {
    let topic = args.first().map(String::as_str).unwrap_or("help");
    if matches!(topic, "help" | "-h" | "--help") {
        println!("{GUIDE_HELP}");
        return Ok(());
    }
    let topic = if matches!(topic, "pages" | "page" | "live-pages") {
        match args.get(1).map(String::as_str) {
            Some("export") => "pages-export",
            Some("picks") => "pages-picks",
            Some(_) => {
                return Err(CliError::Other(format!(
                    "Unknown pages topic.\n\n{GUIDE_HELP}"
                )))
            }
            None => topic,
        }
    } else {
        topic
    };
    match guide_text(topic) {
        Some(text) => {
            println!("{}", text.trim_end());
            Ok(())
        }
        None => Err(CliError::Other(format!(
            "Unknown guide topic: {topic}\n\n{GUIDE_HELP}"
        ))),
    }
}

const BOOSTS_HELP: &str =
    "Usage: ghostex visual boosts [list|show <id>|set <id> <prompt|->|reset <id>|on|off] [--json]

Boosts are the pills a person adds to a chat message from the Boost pill in
the chat box, or by typing * (Live page, Chart it, Computer use, Plan with
workers and the rest, plus their own). Ghostex adds each Boost's prompt under
that message, in a <ghostex-system> block, for that one message. Prompts,
the user's own Boosts and the on/off switch are kept for every chat on this
computer; Settings > Boosts edits them all.

  ghostex visual boosts              Every Boost, and whether its prompt was edited
  ghostex visual boosts show <id>    One Boost's prompt
  ghostex visual boosts set <id> <prompt>
                                     Edit a Boost's prompt (- reads it from stdin)
  ghostex visual boosts reset <id>   Go back to the prompt Ghostex ships
  ghostex visual boosts on | off     Show or hide Boosts in every chat";

fn call(params: Value) -> CliResult<Value> {
    call_gxserver_rpc(crate::boosts::BOOSTS_ENDPOINT, &params, &Flags::default())
}

fn rows(answer: &Value) -> Vec<Value> {
    answer["boosts"].as_array().cloned().unwrap_or_default()
}

pub(crate) fn boosts_command(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{BOOSTS_HELP}");
        return Ok(());
    }
    let json_out = args.iter().any(|arg| arg == "--json");
    let words: Vec<&String> = args.iter().filter(|arg| *arg != "--json").collect();
    let id = || {
        words
            .get(1)
            .map(|id| id.to_string())
            .ok_or_else(|| CliError::Other(format!("Say which Boost.\n\n{BOOSTS_HELP}")))
    };
    let answer = match words.first().map(|word| word.as_str()).unwrap_or("list") {
        "list" => call(json!({ "op": "list" }))?,
        "show" => {
            let id = id()?;
            let answer = call(json!({ "op": "list" }))?;
            let Some(row) = rows(&answer)
                .into_iter()
                .chain(answer["custom"].as_array().cloned().unwrap_or_default())
                .find(|row| row["id"] == id)
            else {
                return Err(CliError::Other(format!("No Boost is called \"{id}\".")));
            };
            if json_out {
                println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
            } else {
                println!("{}", row["prompt"].as_str().unwrap_or_default());
            }
            return Ok(());
        }
        "set" => {
            let id = id()?;
            let prompt = match words.get(2).map(|word| word.as_str()) {
                Some("-") | None => {
                    let mut text = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                        .map_err(|error| CliError::Other(error.to_string()))?;
                    text
                }
                Some(_) => words[2..]
                    .iter()
                    .map(|word| word.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            call(json!({ "op": "set", "id": id, "prompt": prompt }))?
        }
        "reset" => call(json!({ "op": "reset", "id": id()? }))?,
        "on" | "off" => call(json!({ "op": "setOn", "on": words[0] == "on" }))?,
        other => {
            return Err(CliError::Other(format!(
                "Unknown boosts command: {other}\n\n{BOOSTS_HELP}"
            )))
        }
    };
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&answer).unwrap_or_default()
        );
        return Ok(());
    }
    if answer["on"] == false {
        println!("Boosts are off (ghostex visual boosts on shows them again).");
    }
    for row in rows(&answer) {
        println!(
            "{:<15} {:<20} {}",
            row["id"].as_str().unwrap_or_default(),
            row["label"].as_str().unwrap_or_default(),
            if row["edited"] == true {
                "edited prompt"
            } else {
                "Ghostex's prompt"
            }
        );
    }
    for row in answer["custom"].as_array().into_iter().flatten() {
        println!(
            "{:<15} {:<20} your Boost",
            row["id"].as_str().unwrap_or_default(),
            row["label"].as_str().unwrap_or_default(),
        );
    }
    Ok(())
}
