//! `ghostex gbots voice <bot> [<voice>|--default]`: the voice a GBot answers calls in
//! (server/src/gbots/call.rs).

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The voice verb's lines in `ghostex gbots --help`.
pub(crate) const VOICE_HELP: &str = "  ghostex gbots voice <bot> [<voice>|--default] [--json]
                                                 The voice it answers calls in (one of the Voice
                                                 agent's voices; --default uses the Voice agent's
                                                 own); without a voice, shows it and the choices
";

fn names(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `ghostex gbots voice …`.
pub(crate) fn run_voice(rest: &[String], flags: &Flags) -> CliResult<()> {
    let bot = rest.get(1).cloned().ok_or_else(|| {
        CliError::Other("Name the GBot: ghostex gbots voice <bot> [<voice>|--default].".to_string())
    })?;
    let mut params = json!({ "op": "voice", "bot": bot });
    if flags.contains("default") {
        params = json!({ "op": "voiceSet", "bot": bot, "voice": "" });
    } else if let Some(voice) = rest.get(2) {
        params = json!({ "op": "voiceSet", "bot": bot, "voice": voice });
    }
    let answer = call_gxserver_rpc("/api/gbots", &params, flags)?;
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    let name = answer["bot"]["name"].as_str().unwrap_or_default();
    let provider = answer["provider"].as_str().unwrap_or("xai");
    let label = answer["providerLabel"].as_str().unwrap_or(provider);
    let default = answer["defaultVoice"].as_str().unwrap_or_default();
    match answer["voice"].as_str() {
        Some(voice) => {
            let offered = answer["voiceOptions"][provider]
                .as_array()
                .is_some_and(|voices| voices.iter().any(|v| v.as_str() == Some(voice)));
            if offered {
                println!("{name} answers calls in the {label} voice {voice}.");
            } else {
                println!("{name}'s voice is {voice}, which {label} (the Voice agent's provider now) doesn't have, so calls use {default}.");
            }
        }
        None => println!("{name} answers calls in the Voice agent's own voice ({label} {default})."),
    }
    if params["op"] == "voice" {
        println!("{label} voices: {}", names(&answer["voiceOptions"][provider]));
    }
    Ok(())
}
