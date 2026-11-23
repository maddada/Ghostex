//! Instruction entry files: the one-line pointer to `~/.agents/main.md` and how
//! an existing file is classified.
//!
//! CDXC:AgentSync 2026-09-16 WHY:
//! Entry files are written, never symlinked. Each agent has its own filename and some
//! tools refuse or de-duplicate a linked instruction file, and Cursor needs YAML frontmatter
//! around the sentence. The pointer keeps the real text in one place (`~/.agents/main.md`).

use crate::catalog::{InstructionKind, POINTER_LINE};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstructionState {
    /// No file.
    Missing,
    /// Points at `~/.agents/main.md` (either alone or prepended above agent-specific lines),
    /// and `~/.agents/main.md` exists with content.
    Pointer,
    /// Points at `~/.agents/main.md`, but `~/.agents/main.md` is missing or empty.
    DanglingPointer,
    /// Mentions `~/.agents/main.md` with older wording; rewritten with a backup.
    LegacyPointer,
    /// Real content of its own.
    OtherContent,
    /// A symlink; left alone and reported.
    Symlink,
    /// A folder or something else unexpected at the path.
    NotAFile,
}

/// The exact file content Agent Sync writes.
pub fn pointer_content(kind: InstructionKind) -> String {
    match kind {
        InstructionKind::Plain => format!("{POINTER_LINE}\n"),
        InstructionKind::Mdc => format!(
            "---\ndescription: Shared global agent instructions\nalwaysApply: true\n---\n\n{POINTER_LINE}\n"
        ),
    }
}

/// Split leading YAML frontmatter (`---\n...\n---`) from a markdown/MDC file, handling LF and CRLF.
/// Returns `(frontmatter_with_delimiters, rest_of_file)` when present.
fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let after_open = content
        .strip_prefix("---\r\n")
        .or_else(|| content.strip_prefix("---\n"))?;
    let mut offset = content.len() - after_open.len();
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        offset += line.len();
        if trimmed == "---" {
            return Some((&content[..offset], &content[offset..]));
        }
    }
    None
}

/// Extract the instruction body suitable for seeding `~/.agents/main.md`.
/// Strips UTF-8 BOM and, for Cursor `.mdc` files, strips leading YAML frontmatter.
pub fn seed_body(content: &str, kind: InstructionKind) -> String {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let body = match kind {
        InstructionKind::Mdc => split_frontmatter(content)
            .map(|(_, rest)| rest)
            .unwrap_or(content),
        InstructionKind::Plain => content,
    };
    let trimmed_start = body.trim_start_matches(['\r', '\n']);
    if trimmed_start.trim().is_empty() {
        return String::new();
    }
    if trimmed_start.ends_with('\n') {
        trimmed_start.to_string()
    } else {
        format!("{trimmed_start}\n")
    }
}

/// Build the file content for `PointerPolicy::Prepend`, keeping the agent's own instructions
/// underneath the pointer line and preserving valid `.mdc` frontmatter.
pub fn prepend_pointer(existing: &str, kind: InstructionKind) -> String {
    let existing = existing.strip_prefix('\u{feff}').unwrap_or(existing);
    match kind {
        InstructionKind::Plain => {
            let rest = existing.trim_start_matches(['\r', '\n']);
            if rest.trim().is_empty() {
                pointer_content(InstructionKind::Plain)
            } else {
                format!("{POINTER_LINE}\n\n{rest}")
            }
        }
        InstructionKind::Mdc => {
            if let Some((frontmatter, rest)) = split_frontmatter(existing) {
                let body = rest.trim_start_matches(['\r', '\n']);
                if body.trim().is_empty() {
                    format!("{frontmatter}\n{POINTER_LINE}\n")
                } else {
                    format!("{frontmatter}\n{POINTER_LINE}\n\n{body}")
                }
            } else {
                let rest = existing.trim_start_matches(['\r', '\n']);
                if rest.trim().is_empty() {
                    pointer_content(InstructionKind::Mdc)
                } else {
                    format!("{}\n{rest}", pointer_content(InstructionKind::Mdc))
                }
            }
        }
    }
}

pub fn classify(path: &Path, kind: InstructionKind) -> InstructionState {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return InstructionState::Missing;
    };
    if metadata.file_type().is_symlink() {
        return InstructionState::Symlink;
    }
    if !metadata.is_file() {
        return InstructionState::NotAFile;
    }
    let Ok(content) = fs::read_to_string(path) else {
        return InstructionState::OtherContent;
    };
    classify_content(&content, kind)
}

pub fn classify_content(content: &str, kind: InstructionKind) -> InstructionState {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let normalized = normalize_lines(content);
    if normalized == normalize_lines(&pointer_content(kind)) {
        return InstructionState::Pointer;
    }
    match kind {
        InstructionKind::Plain => {
            if normalized.lines().next() == Some(POINTER_LINE) {
                return InstructionState::Pointer;
            }
            if normalized.lines().count() == 1 && normalized.contains("~/.agents/main.md") {
                return InstructionState::LegacyPointer;
            }
        }
        InstructionKind::Mdc => {
            if let Some((_, rest)) = split_frontmatter(content) {
                if normalize_lines(rest).lines().next() == Some(POINTER_LINE) {
                    return InstructionState::Pointer;
                }
            }
        }
    }
    if content.contains("~/.agents/main.md") && normalized.lines().count() <= 6 {
        return InstructionState::LegacyPointer;
    }
    InstructionState::OtherContent
}

fn normalize_lines(content: &str) -> String {
    content
        .replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
