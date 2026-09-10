//! CDXC:SessionChat 2026-09-10 WHY:
//! Subagents may override the lead's model and effort, and a resumed child may change them again. Read the latest child assistant record (Claude) or turn context (Codex), including sidechain rows, without borrowing the lead's current settings.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use serde_json::Value;

use crate::session_chat::{
    read_transcript_file_version, SessionChatTranscriptAgent, TranscriptFileVersion,
};
use crate::session_chat_fleet_transcript::scan_tail;

#[derive(Clone, Default, Serialize)]
pub(crate) struct SubagentModel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read(path: &Path, family: SessionChatTranscriptAgent) -> io::Result<SubagentModel> {
    type Cache = HashMap<PathBuf, (TranscriptFileVersion, SubagentModel)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let version = read_transcript_file_version(path)?;
    if let Some((_, info)) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).filter(|(old, _)| old == &version).cloned())
    {
        return Ok(info);
    }
    let mut info = SubagentModel::default();
    scan_tail(path, |line| {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return Ok(true);
        };
        match (family, record.get("type").and_then(Value::as_str)) {
            (SessionChatTranscriptAgent::Claude, Some("assistant")) => {
                let model = text(record.pointer("/message/model"));
                if model.as_deref().is_none_or(|model| model.starts_with('<')) {
                    return Ok(true);
                }
                info = SubagentModel {
                    model,
                    effort: text(record.get("effort")),
                };
            }
            (SessionChatTranscriptAgent::Codex, Some("turn_context")) => {
                info = SubagentModel {
                    model: text(record.pointer("/payload/model")),
                    effort: text(record.pointer("/payload/effort"))
                        .or_else(|| text(record.pointer("/payload/reasoning_effort"))),
                };
            }
            _ => return Ok(true),
        }
        Ok(false)
    })?;
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (version, info.clone()));
    }
    Ok(info)
}
