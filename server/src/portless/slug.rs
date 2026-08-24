use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::repository::*;

pub(crate) const MAX_HOST_LABEL_LEN: usize = 63;
const STABLE_SUFFIX_HEX_LENGTHS: &[usize] = &[8, 10, 12, 16, 24, 32, 48];

pub(crate) fn parse_worktree_backfill_metadata(
    project_id: &str,
    worktree_json: &str,
) -> Result<Option<PortlessWorktreeBackfillMetadata>> {
    let value: Value = serde_json::from_str(worktree_json)
        .with_context(|| format!("parse Portless worktree metadata for project {project_id}"))?;
    let Some(worktree) = value.as_object() else {
        return Ok(None);
    };
    let Some(parent_project_id) = trimmed_json_string(worktree.get("parentProjectId")) else {
        return Ok(None);
    };
    validate_stable_key("parentProjectId", &parent_project_id)?;
    Ok(Some(PortlessWorktreeBackfillMetadata {
        parent_project_id,
        name: trimmed_json_string(worktree.get("name")),
        branch: trimmed_json_string(worktree.get("branch")),
    }))
}

pub(crate) fn trimmed_json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn stable_worktree_key(row: &PortlessProjectBackfillRow) -> String {
    row.project_id.clone()
}

pub(crate) fn project_base_slug(row: &PortlessProjectBackfillRow) -> String {
    hostname_safe_slug(&row.name)
        .or_else(|| {
            row.path
                .as_deref()
                .and_then(path_basename)
                .and_then(hostname_safe_slug)
        })
        .unwrap_or_else(|| deterministic_fallback_slug("project", &row.project_id))
}

pub(crate) fn worktree_base_slug(
    worktree: &PortlessWorktreeBackfillMetadata,
    worktree_key: &str,
) -> String {
    worktree
        .name
        .as_deref()
        .and_then(hostname_safe_slug)
        .or_else(|| {
            worktree
                .branch
                .as_deref()
                .and_then(branch_last_segment)
                .and_then(hostname_safe_slug)
        })
        .unwrap_or_else(|| deterministic_fallback_slug("wt", worktree_key))
}

fn deterministic_fallback_slug(prefix: &str, stable_id: &str) -> String {
    append_slug_suffix(prefix, &stable_hex_suffix("fallback", stable_id, 10))
}

pub(crate) fn allocate_slug(
    reserved_slugs: &HashSet<String>,
    base_slug: &str,
    namespace: &str,
    stable_id: &str,
) -> Result<String> {
    validate_slug("baseSlug", base_slug)?;
    if !reserved_slugs.contains(base_slug) {
        return Ok(base_slug.to_string());
    }
    for length in STABLE_SUFFIX_HEX_LENGTHS {
        let suffix = stable_hex_suffix(namespace, stable_id, *length);
        let candidate = append_slug_suffix(base_slug, &suffix);
        validate_slug("candidateSlug", &candidate)?;
        if !reserved_slugs.contains(&candidate) {
            return Ok(candidate);
        }
    }
    for attempt in 1..=1024 {
        let suffix = stable_hex_suffix(namespace, &format!("{stable_id}\0{attempt}"), 32);
        let candidate = append_slug_suffix(base_slug, &suffix);
        validate_slug("candidateSlug", &candidate)?;
        if !reserved_slugs.contains(&candidate) {
            return Ok(candidate);
        }
    }
    bail!("Unable to allocate a stable Portless slug.")
}

fn append_slug_suffix(base_slug: &str, suffix: &str) -> String {
    let max_base_len = MAX_HOST_LABEL_LEN.saturating_sub(suffix.len() + 1);
    let base = truncate_slug_label(base_slug, max_base_len);
    format!("{base}-{suffix}")
}

fn stable_hex_suffix(namespace: &str, stable_id: &str, hex_len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(stable_id.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(hex_len);
    hex
}

fn hostname_safe_slug(input: &str) -> Option<String> {
    let mut output = String::new();
    let mut last_was_hyphen = false;
    for byte in input.trim().bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => {
                output.push(byte as char);
                last_was_hyphen = false;
            }
            b'A'..=b'Z' => {
                output.push(byte.to_ascii_lowercase() as char);
                last_was_hyphen = false;
            }
            _ => {
                if !output.is_empty() && !last_was_hyphen {
                    output.push('-');
                    last_was_hyphen = true;
                }
            }
        }
    }
    let label = truncate_slug_label(&output, MAX_HOST_LABEL_LEN);
    (!label.is_empty()).then_some(label)
}

fn truncate_slug_label(input: &str, max_len: usize) -> String {
    let mut value = input.trim_matches('-').to_string();
    if value.len() > max_len {
        value.truncate(max_len);
        value = value.trim_matches('-').to_string();
    }
    value
}

fn path_basename(path: &str) -> Option<&str> {
    let trimmed = path.trim();
    let without_trailing_separator = trimmed.trim_end_matches(&['/', '\\'][..]);
    let candidate = if without_trailing_separator.is_empty() {
        trimmed
    } else {
        without_trailing_separator
    };
    candidate
        .rsplit(&['/', '\\'][..])
        .find(|segment| !segment.trim().is_empty())
}

fn branch_last_segment(branch: &str) -> Option<&str> {
    let trimmed = branch.trim().trim_matches('/');
    let without_refs = trimmed
        .strip_prefix("refs/heads/")
        .unwrap_or(trimmed)
        .trim_matches('/');
    without_refs
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
}
