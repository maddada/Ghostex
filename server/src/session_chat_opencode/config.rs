use super::error;
use crate::domain::DomainStateError;
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use serde_json::Value;
use std::{fs, io::Write, path::Path};

fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(v) => CstInputValue::Bool(*v),
        Value::Number(v) => CstInputValue::Number(v.to_string()),
        Value::String(v) => CstInputValue::String(v.clone()),
        Value::Array(v) => CstInputValue::Array(v.iter().map(input).collect()),
        Value::Object(v) => {
            CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), input(v))).collect())
        }
    }
}

pub(crate) fn read_config(text: &str) -> Result<Value, DomainStateError> {
    jsonc_parser::parse_to_serde_value(
        if text.trim().is_empty() { "{}" } else { text },
        &Default::default(),
    )
    .map_err(|e| error(format!("Invalid OpenCode configuration: {e}")))
}

/// OpenCode accepts JSONC. Edit only the owned property so comments and other settings survive.
pub(crate) fn patch_config(path: &Path, key: &str, value: &Value) -> Result<(), DomainStateError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}\n".into(),
        Err(e) => return Err(error(format!("Could not read OpenCode configuration: {e}"))),
    };
    let root = CstRootNode::parse(&text, &Default::default()).map_err(|e| error(e.to_string()))?;
    let object = root
        .object_value()
        .ok_or_else(|| error("OpenCode configuration must contain an object."))?;
    match object.get(key) {
        Some(property) => property.set_value(input(value)),
        None => {
            object.append(key, input(value));
        }
    }
    let directory = path
        .parent()
        .ok_or_else(|| error("Invalid OpenCode configuration path."))?;
    fs::create_dir_all(directory).map_err(|e| error(e.to_string()))?;
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|e| error(e.to_string()))?;
    temp.write_all(root.to_string().as_bytes())
        .map_err(|e| error(e.to_string()))?;
    temp.persist(path).map_err(|e| error(e.error.to_string()))?;
    Ok(())
}

pub(crate) fn save_default_model(model: Value) -> Result<(), DomainStateError> {
    let directory = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::resume_lookup::home_dir().join(".config"))
        .join("opencode");
    let path = if directory.join("opencode.jsonc").is_file() {
        directory.join("opencode.jsonc")
    } else {
        directory.join("opencode.json")
    };
    patch_config(&path, "model", &model)
}
