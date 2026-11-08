//! Topic notes, `memory/<topic>.md`: detail a bot reads only when the work touches it. A topic's
//! title is its first `# ` heading and its description the first plain line under it (no
//! frontmatter). The index of up to 40 topics in 2,000 characters loads with the bot's memory;
//! the archive and the daily log are not topics.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::gbots::folder::MEMORY_DIR;

/// Topics a bot may have, and how much of the index loads.
pub(crate) const TOPICS_MAX: usize = 40;
const TOPICS_MAX_CHARS: usize = 2_000;
/// One topic file's size limit.
pub(crate) const TOPIC_MAX_BYTES: usize = 64 * 1024;
pub(crate) const ARCHIVE_STEM: &str = "archive";
const NAME_MAX_CHARS: usize = 60;

#[derive(Clone, Debug)]
pub(crate) struct Topic {
    pub stem: String,
    pub title: String,
    pub description: String,
    pub bytes: usize,
    pub path: PathBuf,
}

impl Topic {
    pub fn summary(&self) -> Value {
        json!({
            "name": self.stem,
            "title": self.title,
            "description": self.description,
            "bytes": self.bytes,
            "path": self.path,
        })
    }

    /// Its line in the index: `- Airlines: which ones to avoid (memory/airlines.md)`.
    fn index_line(&self) -> String {
        if self.description.is_empty() {
            format!("- {} (memory/{}.md)", self.title, self.stem)
        } else {
            format!(
                "- {}: {} (memory/{}.md)",
                self.title, self.description, self.stem
            )
        }
    }
}

/// The title and description of a topic's text; the title defaults to `stem`.
fn title_and_description(stem: &str, text: &str) -> (String, String) {
    let mut title = stem.to_string();
    let mut seen_title = false;
    for line in text.lines().map(str::trim) {
        if !seen_title {
            if let Some(heading) = line.strip_prefix("# ") {
                title = heading.trim().to_string();
                seen_title = true;
            }
            continue;
        }
        if !line.is_empty() && !line.starts_with('#') {
            // A dated note describes the topic by its words, not its date.
            let line = super::entries::parse(line)
                .first()
                .map(|entry| entry.text.clone())
                .unwrap_or_else(|| line.to_string());
            let description: String = line.chars().take(160).collect();
            return (title, description);
        }
    }
    (title, String::new())
}

/// Every topic of the bot in `folder`, by name.
pub(crate) fn list(folder: &Path) -> Vec<Topic> {
    let Ok(entries) = std::fs::read_dir(folder.join(MEMORY_DIR)) else {
        return Vec::new();
    };
    let mut topics: Vec<Topic> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "md")
                && path.file_stem().is_some_and(|stem| stem != ARCHIVE_STEM)
        })
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            let (title, description) = title_and_description(&stem, &text);
            Topic {
                stem,
                title,
                description,
                bytes: text.len(),
                path,
            }
        })
        .collect();
    topics.sort_by(|a, b| a.stem.cmp(&b.stem));
    topics
}

/// The index lines that load with the bot's memory.
pub(crate) fn index_lines(folder: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let mut used = 0;
    for topic in list(folder).into_iter().take(TOPICS_MAX) {
        let line = topic.index_line();
        if used + line.len() > TOPICS_MAX_CHARS {
            break;
        }
        used += line.len();
        lines.push(line);
    }
    lines
}

/// A topic name as a file stem (`Air lines.md` → `air-lines`), or why it can't be one.
pub(crate) fn stem_of(name: &str) -> Result<String, String> {
    let name = name.trim();
    let name = name.strip_suffix(".md").unwrap_or(name);
    let name = name.strip_prefix("memory/").unwrap_or(name);
    let mut stem = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            stem.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_' | ' ' | '.') && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        return Err("Name the topic with letters or numbers, e.g. --topic airlines.".to_string());
    }
    if stem.chars().count() > NAME_MAX_CHARS {
        return Err(format!(
            "Keep a topic's name under {NAME_MAX_CHARS} characters."
        ));
    }
    if stem == ARCHIVE_STEM || stem == "log" {
        return Err(format!(
            "`{stem}` is kept by Ghostex; pick another topic name."
        ));
    }
    Ok(stem)
}

/// `memory/<stem>.md` relative to the bot folder.
pub(crate) fn relative_path(stem: &str) -> String {
    format!("{MEMORY_DIR}/{stem}.md")
}

/// The text a new topic starts with.
pub(crate) fn new_topic_text(title: &str) -> String {
    format!("# {}\n\n", title.trim())
}
