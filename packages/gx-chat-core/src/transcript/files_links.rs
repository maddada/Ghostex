//! Files and links: every picture, attached file, file path and web link a session's messages
//! mention, gathered once per session so the chat's Files and links list can show them.
//!
//! The rules read the messages with the transcript's own detection, never a second parser: a
//! message's text goes through [`native_markdown`] and [`markdown_references`] exactly as its
//! bubble does (so a path is listed exactly when the transcript draws it as a pill, and a URL when
//! it draws a link), its picture blocks and Markdown images are the transcript's pictures, and the
//! files an agent wrote come from the file change cards' own [`split_file_changes`].
//!
//! - **Attachments**: every picture (a pasted or attached `[Image #N](path)`, a picture block, a
//!   Markdown image, a picture path) and every file the user attached with the composer
//!   (`[File #N]`, `[PDF #N]`, `[Audio #N]`, `[Video #N]`, `[Folder #N]` references).
//! - **Files**: every other file or folder reference in a user or agent message, and every file
//!   the agent created, edited or deleted.
//! - **Links**: every http(s) address, written as a Markdown link or bare, with its link text.
//!
//! Only the user's and the agent's own messages count: reasoning, tool output and the harness's
//! injected turns (gxserver drops those before calling [`collect`]) are left out, so a file a tool
//! merely printed is not listed. Each item is listed once, keyed by its path or address, with how
//! often and when it was first and last mentioned; newest mention first.
//!
//! SEE-ALSO: server/src/session_files_links.rs (reads every session's transcript and calls
//! [`collect`]), composer/files_links.rs (the button, the card and its filter).

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use markdown::mdast::Node;
use markdown::ParseOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::transcript::file_changes::split_file_changes;
use crate::transcript::image_markers::image_source_path_from_text;
use crate::transcript::images::{image_label, image_source, ImageRef};
use crate::transcript::jsstr::last_path_segment;
use crate::transcript::line_breaks::user_line_breaks;
use crate::transcript::markdown_links::markdown_references;
use crate::transcript::native_markdown::{native_markdown, native_markdown_with_mentions};

/// Which part of the list an item is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilesLinksSection {
    Attachments,
    Files,
    Links,
}

/// One message that mentioned an item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksMention {
    pub message_id: String,
    /// Epoch ms, when the transcript recorded one.
    #[serde(default)]
    pub timestamp: Option<i64>,
}

/// One listed attachment, file or link.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksItem {
    pub section: FilesLinksSection,
    /// The path or address it is listed under, so two mentions of it are one row.
    pub key: String,
    /// What the row reads: the file's name, "Pasted image", or the link's text.
    pub label: String,
    /// The whole path or address.
    pub detail: String,
    /// `image`, `file`, `folder` or `url`, the reference pill's own kinds.
    pub kind: String,
    /// The local path a file or picture opens from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The address a link or a web picture opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The picture source the image viewer opens (`images.rs`'s `image_source`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Value>,
    /// What the agent last did to the file: `new`, `edited` or `deleted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// The user attached it with the composer.
    #[serde(default)]
    pub attached: bool,
    #[serde(default)]
    pub by_user: bool,
    #[serde(default)]
    pub by_agent: bool,
    pub mentions: u32,
    pub first: FilesLinksMention,
    pub last: FilesLinksMention,
}

impl FilesLinksItem {
    /// Points a file or picture at another path (gxserver makes a relative path absolute against
    /// the session's own folder); the key follows, so [`merge`] can join it with the same file
    /// written out in full.
    pub fn rebase(&mut self, path: &str) {
        if self.path.is_none() {
            return;
        }
        self.key = path_key(path);
        self.detail = path.to_string();
        self.path = Some(path.to_string());
        if let Some(image) = self.image.as_mut().and_then(Value::as_object_mut) {
            if image.get("transport").and_then(Value::as_str) == Some("read") {
                image.insert("path".to_string(), path.into());
                image.insert("copyPath".to_string(), path.into());
            }
        }
    }
}

/// The key a path is listed under: one spelling for `/` and `\`, and a Windows drive path in one
/// case, which the file system ignores.
pub fn path_key(path: &str) -> String {
    let key = path.replace('\\', "/");
    let bytes = key.as_bytes();
    if bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        key.to_ascii_lowercase()
    } else {
        key
    }
}

/// A reference label the composer writes for a file the user attached (`native_path_reference`).
fn is_attachment_label(label: &str) -> bool {
    [
        "File #", "PDF #", "Audio #", "Video #", "Folder #", "Image #",
    ]
    .iter()
    .any(|prefix| {
        label.trim().strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
        })
    })
}

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Items in the order they were first met, keyed for the merge.
#[derive(Default)]
struct Collector {
    items: Vec<FilesLinksItem>,
    index: std::collections::HashMap<String, usize>,
}

impl Collector {
    fn add(&mut self, mut item: FilesLinksItem) {
        if item.key.is_empty() {
            return;
        }
        if let Some(&at) = self.index.get(&item.key) {
            let known = &mut self.items[at];
            // One message mentioning it twice (`[text](url)` is also scanned as a bare address)
            // is still one mention.
            if known.last.message_id == item.last.message_id {
                item.mentions = 0;
            }
            join(known, item);
            return;
        }
        item.mentions = item.mentions.max(1);
        self.index.insert(item.key.clone(), self.items.len());
        self.items.push(item);
    }
}

/// Folds a later mention of the same item into the one already listed.
fn join(known: &mut FilesLinksItem, later: FilesLinksItem) {
    // A picture or an attachment stays one wherever else it is mentioned.
    if later.section == FilesLinksSection::Attachments {
        known.section = FilesLinksSection::Attachments;
        known.kind = later.kind.clone();
        if known.image.is_none() {
            known.image = later.image.clone();
        }
    }
    known.attached |= later.attached;
    known.by_user |= later.by_user;
    known.by_agent |= later.by_agent;
    if later.change.is_some() {
        known.change = later.change.clone();
    }
    // A link keeps the first text anyone gave it.
    if known.section == FilesLinksSection::Links
        && known.url.as_deref() == Some(known.label.as_str())
        && later.url.as_deref() != Some(later.label.as_str())
    {
        known.label = later.label.clone();
    }
    known.mentions += later.mentions;
    if earlier(&later.first, &known.first) {
        known.first = later.first;
    }
    if !earlier(&later.last, &known.last) {
        known.last = later.last;
    }
}

/// `a` came before `b`; a mention with no time counts as the older one.
fn earlier(a: &FilesLinksMention, b: &FilesLinksMention) -> bool {
    a.timestamp.unwrap_or(i64::MIN) < b.timestamp.unwrap_or(i64::MIN)
}

/// Every item the messages mention, newest mention first.
pub fn collect(messages: &[ChatMessage]) -> Vec<FilesLinksItem> {
    let mut collector = Collector::default();
    for message in messages {
        let user = match message.role {
            ChatRole::User => true,
            ChatRole::Assistant => false,
            _ => continue,
        };
        let mention = FilesLinksMention {
            message_id: message.id.clone(),
            timestamp: message.timestamp,
        };
        let base =
            |section, key: String, label: String, detail: String, kind: &str| FilesLinksItem {
                section,
                key,
                label,
                detail,
                kind: kind.to_string(),
                path: None,
                url: None,
                image: None,
                change: None,
                attached: false,
                by_user: user,
                by_agent: !user,
                mentions: 1,
                first: mention.clone(),
                last: mention.clone(),
            };
        let picture = |reference: ImageRef| -> Option<FilesLinksItem> {
            let source = image_source(&reference);
            let transport = text_of(&source, "transport");
            let location = match transport {
                "read" => reference.path.clone()?,
                "url" => reference.url.clone()?,
                // Bytes inline or nothing to show: no place to list.
                _ => return None,
            };
            let mut item = base(
                FilesLinksSection::Attachments,
                if transport == "read" {
                    path_key(&location)
                } else {
                    location.clone()
                },
                image_label(&reference),
                location.clone(),
                "image",
            );
            if transport == "read" {
                item.path = Some(location);
            } else {
                item.url = Some(location);
            }
            item.image = Some(source);
            Some(item)
        };
        for block in &message.blocks {
            match block {
                ChatBlock::Text { text } => {
                    if let Some(path) = image_source_path_from_text(text) {
                        if let Some(item) = picture(ImageRef {
                            path: Some(path.to_string()),
                            ..ImageRef::default()
                        }) {
                            collector.add(item);
                        }
                        continue;
                    }
                    let mentions = message.mention_paths.as_deref().unwrap_or_default();
                    for reference in text_references(text, user, mentions) {
                        let kind = text_of(&reference, "kind");
                        let source_label = text_of(&reference, "sourceLabel");
                        match kind {
                            "url" => {
                                let url = text_of(&reference, "title").to_string();
                                let label = match source_label.trim() {
                                    "" => url.clone(),
                                    label => label.to_string(),
                                };
                                let mut item = base(
                                    FilesLinksSection::Links,
                                    url.clone(),
                                    label,
                                    url.clone(),
                                    "url",
                                );
                                item.url = Some(url);
                                collector.add(item);
                            }
                            "image" => {
                                if let Some(item) = picture(ImageRef {
                                    path: Some(text_of(&reference, "path").to_string()),
                                    ..ImageRef::default()
                                }) {
                                    let attached = user && is_attachment_label(source_label);
                                    collector.add(FilesLinksItem { attached, ..item });
                                }
                            }
                            "file" | "folder" => {
                                let path = text_of(&reference, "path").to_string();
                                if path.is_empty() {
                                    continue;
                                }
                                let attached = user && is_attachment_label(source_label);
                                let mut item = base(
                                    if attached {
                                        FilesLinksSection::Attachments
                                    } else {
                                        FilesLinksSection::Files
                                    },
                                    path_key(&path),
                                    file_label(&path),
                                    path.clone(),
                                    kind,
                                );
                                item.attached = attached;
                                item.path = Some(path);
                                collector.add(item);
                            }
                            // Skill pills and the Side chat pill are not files.
                            _ => {}
                        }
                    }
                    for reference in markdown_images(text) {
                        if let Some(item) = picture(reference) {
                            collector.add(item);
                        }
                    }
                }
                ChatBlock::ImageRef { path, url, alt } => {
                    if let Some(item) = picture(ImageRef {
                        path: path.clone(),
                        url: url.clone(),
                        alt: alt.clone(),
                    }) {
                        collector.add(item);
                    }
                }
                _ => {}
            }
        }
        if user {
            continue;
        }
        let (_, changes) = split_file_changes(message.blocks.iter());
        for change in changes {
            if change.result_is_error() || change.path.trim().is_empty() {
                continue;
            }
            let path = change.path.clone();
            let picture = crate::composer::reference_pills::reference_kind(&path, &path)
                == crate::composer::reference_pills::ReferenceKind::Image;
            let mut item = base(
                if picture {
                    FilesLinksSection::Attachments
                } else {
                    FilesLinksSection::Files
                },
                path_key(&path),
                file_label(&path),
                path.clone(),
                if picture { "image" } else { "file" },
            );
            if picture {
                item.image = Some(image_source(&ImageRef {
                    path: Some(path.clone()),
                    ..ImageRef::default()
                }));
            }
            item.path = Some(path);
            item.change = Some(change.status.to_string());
            collector.add(item);
        }
    }
    merge(collector.items)
}

/// The same over the wire rows gxserver reads; a row this build cannot read is skipped.
pub fn collect_values(messages: &[Value]) -> Vec<FilesLinksItem> {
    let messages: Vec<ChatMessage> = messages
        .iter()
        .filter_map(|message| serde_json::from_value(message.clone()).ok())
        .collect();
    collect(&messages)
}

/// Joins items that share a key (after [`FilesLinksItem::rebase`]) and orders them newest
/// mention first.
pub fn merge(items: Vec<FilesLinksItem>) -> Vec<FilesLinksItem> {
    let mut merged: Vec<FilesLinksItem> = Vec::with_capacity(items.len());
    let mut index: std::collections::HashMap<String, usize> = Default::default();
    for item in items {
        match index.get(&item.key) {
            Some(&at) => join(&mut merged[at], item),
            None => {
                index.insert(item.key.clone(), merged.len());
                merged.push(item);
            }
        }
    }
    merged.sort_by(|a, b| {
        b.last
            .timestamp
            .unwrap_or(i64::MIN)
            .cmp(&a.last.timestamp.unwrap_or(i64::MIN))
    });
    merged
}

/// A file's row label: its name, or the folder's own name.
fn file_label(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match last_path_segment(trimmed) {
        "" => path.to_string(),
        name => name.to_string(),
    }
}

/// The references the message's bubble draws: its Markdown as the transcript marks it, then the
/// same reference scan the pills come from (`presentation.rs`). A link to a picture
/// (`[Image #1](path)`) is drawn as the picture itself, so the marked text no longer holds it; the
/// same scan over the text as written finds those.
fn text_references(text: &str, user: bool, mentions: &[String]) -> Vec<Value> {
    let native = if user {
        native_markdown_with_mentions(&user_line_breaks(text), true, mentions)
    } else {
        native_markdown(text, false)
    };
    let mut references = markdown_references(&native);
    references.extend(
        markdown_references(text)
            .into_iter()
            .filter(|reference| text_of(reference, "kind") == "image"),
    );
    references
}

/// The pictures a message writes as Markdown images (`![alt](src)`), which the transcript draws
/// in place rather than as pills.
fn markdown_images(text: &str) -> Vec<ImageRef> {
    if !text.contains("![") {
        return Vec::new();
    }
    let Ok(tree) = markdown::to_mdast(text, &ParseOptions::default()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    fn walk(node: &Node, found: &mut Vec<ImageRef>) {
        if let Node::Image(image) = node {
            let url = image.url.trim();
            let web = url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("data:");
            found.push(ImageRef {
                path: (!web && !url.is_empty())
                    .then(|| url.strip_prefix("file://").unwrap_or(url).to_string()),
                url: web.then(|| url.to_string()),
                alt: (!image.alt.is_empty()).then(|| image.alt.clone()),
            });
        }
        for child in node.children().into_iter().flatten() {
            walk(child, found);
        }
    }
    walk(&tree, &mut found);
    found
}
