//! The pictures an agent opened from disk: a tool row's thumbnails, and the strip a folded turn
//! keeps of them.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User: "we need to show image thumbnails when reading images from desk and bubble those image previews up to the transcript". From the options page (Q958 to Q962): a call that read a picture file (Claude's Read, Codex's view image, OpenCode's read, any agent's read of an image) shows the picture as a small 48px thumbnail on its row (Q959 "Small (48 px)"), and when its turn folds ("Worked for …", "Previous turn", summary mode's closed "Active work") the pictures the turn read stay visible as a strip of thumbnails (Q958 "On the row, and bubbled up to the turn"). The strip first wrapped onto more lines to show every picture (Q961 "Show all of them, wrapping onto more lines"); seeing it under "Previous turn" with no words, the user then chose "C but make them scroll sideways on vertical mouse scroll or horizontal trackpad scroll in case there's more images (like we did for sidepanel tabs basically)": the strip sits inside the folded turn's card under its header, past a hairline, the card's counts start with "N images" (`images_stat`), and it is one row that scrolls sideways when it does not fit, a mouse wheel's vertical turn or a trackpad's sideways swipe moving it, fading at an edge that hides more, superseding the wrapping. A thumbnail opens the chat's picture viewer, whose arrows step through these pictures with the rest of the transcript's (Q960), and the phone does the same (Q962). The picture is read where the agent read it: the path goes to the session machine's gxserver (`readSessionChatImage`), never the reader's own disk, and a file that has gone shows the missing-picture placeholder.
//! CDXC:SessionChat 2026-11-06 DECISION:
//! User, on a Read of the screenshot they had just pasted, whose row showed no picture: "when agent is reading a pic please show thumbnail of it next to this row in the chat transcript". Every row that read a picture shows it, the pictures the user's own messages show included: the 2026-11-02 rule that skipped a picture the user already showed (by path, then by a digest of its bytes) no longer applies to a row's own tile, since a skipped thumbnail read as a missing feature (the strips got it back the same day, below). The tile is row-sized and sits on the row's right (desktop `READ_TILE`, native_chat/read_pictures.rs).
//! CDXC:SessionChat 2026-11-06 DECISION:
//! User, on a closed "4 tool calls" fold and a closed text row whose strip showed the screenshot they had just pasted: "please don't show the image that was read here under the collapsed tool calls if it's the same one i just sent in my chat fam", and on the open row: "only in the case that it's actually inside the expanded area and we show it on the right side then we should keep showing it because in that case it literally says \"Read image.png\" and the user can't see which image it's referering to u know?". Supersedes the decision above for strips only: the strip a closed fold keeps ("N tool calls", "Worked for …", "Previous turn", a closed text row, summary mode's closed "Active work") leaves out a picture a user message of the chat already shows, matched by path first and, where the paths differ, by a digest of its bytes (`SeenPictures`, as on 2026-11-02); with every picture left out there is no strip and the card's "N images" count does not count them. The tile on the Read row itself stays, the user's own picture included: core marks such a row picture `userShown` (`USER_SHOWN`) so a renderer that builds a strip from rows' `images` (the "N tool calls" fold) leaves it out, and keeps it out of `readImages` itself.
//! SEE-ALSO: `image_paths_in_call` also feeds the `imagePaths` of an older turn the server kept folded (server/src/session_chat_history.rs). The renderers draw `images` on a tool row and `readImages` under a fold: desktop `native_chat/tool_run.rs`, `tool_card.rs`, `completed_work_row.rs` and `transcript.rs`; phone `ToolRows.tsx`, `ToolCard.tsx` and `TranscriptItemRow.tsx`.

use serde_json::Value;

use crate::document::TranscriptItem;
use crate::transcript::images::{image_source, ImageRef};
use crate::transcript::jsstr::{ascii_lower, js_trim};
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_summary::{js_string_value, key_literals};

/// The file kinds the chat's picture loaders decode. An SVG is text to an agent's Read, so it is
/// not a picture it looked at.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// Whether `path` names a picture file by its extension.
pub fn is_image_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty() && IMAGE_EXTENSIONS.contains(&ascii_lower(extension).as_str())
    })
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// `path` as the session's machine can open it: a `file://` address unwrapped, a relative path
/// put under the session's folder, and `None` when it still is not absolute.
fn session_path(path: &str, working_directory: Option<&str>) -> Option<String> {
    let path = js_trim(path);
    let path = path.strip_prefix("file://").unwrap_or(path);
    if path.is_empty() {
        return None;
    }
    if is_absolute(path) {
        return Some(path.to_string());
    }
    let folder = working_directory
        .map(js_trim)
        .filter(|folder| is_absolute(folder))?;
    let relative = path.strip_prefix("./").unwrap_or(path);
    Some(format!("{}/{relative}", folder.trim_end_matches('/')))
}

/// The tool's name with case, `_`, `-` and `.` folded away, the way `tool_label` reads it.
fn normalized_name(name: &str) -> String {
    let name = name.rsplit('.').next().unwrap_or(name);
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

/// The paths a reading tool names in its arguments.
fn named_paths(input: &Value) -> Vec<&str> {
    let mut paths: Vec<&str> = Vec::new();
    for key in [
        "file_path",
        "filePath",
        "path",
        "absolute_path",
        "target_file",
        "Path",
        "AbsolutePath",
    ] {
        if let Some(path) = input.get(key).and_then(Value::as_str) {
            paths.push(path);
            break;
        }
    }
    for key in ["paths", "file_paths", "files"] {
        if let Some(Value::Array(items)) = input.get(key) {
            paths.extend(items.iter().filter_map(|item| {
                item.as_str()
                    .or_else(|| item.get("path").and_then(Value::as_str))
                    .or_else(|| item.get("file_path").and_then(Value::as_str))
            }));
        }
    }
    paths
}

/// The pictures a Codex code-mode script opened with `tools.view_image({path: …})`.
fn script_image_paths(script: &str) -> Vec<String> {
    const CALL: &str = "tools.view_image(";
    let mut paths = Vec::new();
    let mut rest = script;
    while let Some(start) = rest.find(CALL) {
        rest = &rest[start + CALL.len()..];
        let argument = &rest[..rest.find("tools.").unwrap_or(rest.len())];
        if let Some(path) = key_literals(argument, &["path"])
            .into_iter()
            .find_map(js_string_value)
        {
            paths.push(path);
        }
    }
    paths
}

/// The picture files one tool call opened, each once, as absolute paths on the session's
/// machine. Empty for a call that read no picture.
pub fn image_paths_in_call(
    name: &str,
    input: &Value,
    working_directory: Option<&str>,
) -> Vec<String> {
    let found: Vec<String> = match input {
        Value::String(script) if script.contains("tools.view_image(") => script_image_paths(script),
        Value::Object(_) => match normalized_name(name).as_str() {
            "read" | "readfile" | "readmanyfiles" | "view" | "viewfile" | "viewimage"
            | "readimage" | "openfile" | "openimage" => {
                named_paths(input).into_iter().map(str::to_string).collect()
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let mut paths: Vec<String> = Vec::new();
    for path in found {
        if !is_image_path(&path) {
            continue;
        }
        if let Some(path) = session_path(&path, working_directory) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// One picture file as the transcript's image source (`image_source`): read on the session's
/// machine.
pub fn read_image_source(path: &str) -> Value {
    image_source(&ImageRef {
        path: Some(path.to_string()),
        url: None,
        alt: None,
    })
}

/// The pictures a tool row shows: what its call opened, unless the call failed (the agent saw no
/// picture then, and the row says why).
pub fn tool_read_images(pair: &ToolPair<'_>, working_directory: Option<&str>) -> Vec<Value> {
    if pair.result_is_error() {
        return Vec::new();
    }
    match (pair.call_name(), pair.call_input()) {
        (Some(name), Some(input)) => image_paths_in_call(name, input, working_directory)
            .iter()
            .map(|path| read_image_source(path))
            .collect(),
        _ => Vec::new(),
    }
}

/// A projected message's `readImages`: every picture its tool rows show, in row order, each once.
pub fn message_read_images(rows: &[Value]) -> Vec<Value> {
    let mut images: Vec<Value> = Vec::new();
    for image in rows
        .iter()
        .filter_map(|row| row.get("images").and_then(Value::as_array))
        .flatten()
    {
        push_once(&mut images, image.clone());
    }
    images
}

fn push_once(images: &mut Vec<Value>, image: Value) {
    let path = image.get("path");
    if !images.iter().any(|seen| seen.get("path") == path) {
        images.push(image);
    }
}

/// A turn's `readImages`, the strip under its fold: the pictures its rows read, oldest first, then
/// those of the rows the server still holds back (`imagePaths` on its `deferredWork`), each once.
pub fn turn_read_images<'a>(
    rows: impl IntoIterator<Item = &'a Value>,
    deferred_paths: &[String],
) -> Vec<Value> {
    let mut images: Vec<Value> = Vec::new();
    for image in rows
        .into_iter()
        .filter_map(|row| row.get("readImages").and_then(Value::as_array))
        .flatten()
    {
        push_once(&mut images, image.clone());
    }
    for path in deferred_paths {
        if is_absolute(path) && is_image_path(path) {
            push_once(&mut images, read_image_source(path));
        }
    }
    images
}

/// The field core sets on a row's picture that a user message of the chat already shows: the row
/// keeps its tile, a closed fold's strip leaves it out (2026-11-06 in the module doc).
pub const USER_SHOWN: &str = "userShown";

/// Whether a strip leaves `image` out: a picture the user already showed.
pub fn is_user_shown(image: &Value) -> bool {
    image.get(USER_SHOWN) == Some(&Value::Bool(true))
}

/// The pictures the user already showed in the chat: each one's path as written in a user
/// message (an attached picture, an "[Image #1](path)" reference, a Markdown picture or link), and
/// the digests of those whose bytes the chat has read.
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User: "if the agent reads the same picture i pasted in my message then we shouldn't show that thumbnail if u get me, because it's just going to be shown twice in chat transcript (in my message and in the agent side of the transcript)". A picture an agent read that is a picture a user message of the chat shows: the same path first, and, where the paths differ, the same bytes (a digest of what `readSessionChatImage` returned for each, so it is known once both have been read; until then the read counts as another picture). Since 2026-11-06 only a closed fold's strip leaves it out; the row keeps its tile.
#[derive(Debug, Default)]
pub struct SeenPictures {
    paths: std::collections::HashSet<String>,
    digests: std::collections::HashSet<u64>,
    /// The chat's digests by path, copied (one per picture read) so a pass can keep this value
    /// beside the scope it was read from (`ProjectionScope::seen_pictures`).
    known: Option<std::collections::HashMap<String, u64>>,
}

/// A path in the one spelling both sides compare in: no `file://`, and macOS's `/private` link
/// folded so `/private/var/…` and `/var/…` are one file.
fn compared_path(path: &str) -> String {
    let path = js_trim(path);
    let path = path.strip_prefix("file://").unwrap_or(path);
    for linked in ["/private/var/", "/private/tmp/", "/private/etc/"] {
        if let Some(rest) = path.strip_prefix(linked) {
            return format!("{}{rest}", &linked["/private".len()..]);
        }
    }
    path.to_string()
}

/// The link destinations a user's words carry (`[label](dest)`, `![alt](dest)`,
/// `[Image: source: dest]`) that name a picture file.
fn written_picture_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        rest = &rest[at + 2..];
        let destination = match rest.strip_prefix('<') {
            Some(angled) => angled.split('>').next().unwrap_or_default(),
            None => rest.split([')', '\n']).next().unwrap_or_default(),
        };
        let destination = destination.split(" \"").next().unwrap_or(destination);
        if is_image_path(destination) {
            paths.push(compared_path(destination));
        }
    }
    for line in text.lines() {
        if let Some(path) =
            crate::transcript::image_markers::image_source_path_from_text(js_trim(line))
        {
            if is_image_path(path) {
                paths.push(compared_path(path));
            }
        }
    }
    paths
}

impl SeenPictures {
    /// Read off `messages`' user rows and the digests the chat has (`picture_digests`).
    pub fn new(
        messages: &[ghostex_gx_protocol::ChatMessage],
        digests: &std::collections::HashMap<String, u64>,
    ) -> Self {
        use ghostex_gx_protocol::{ChatBlock, ChatRole};
        let mut paths = std::collections::HashSet::new();
        for message in messages
            .iter()
            .filter(|message| message.role == ChatRole::User)
        {
            for block in &message.blocks {
                match block {
                    ChatBlock::ImageRef { path, url, .. } => {
                        if let Some(path) = path.as_deref().or(url.as_deref()) {
                            paths.insert(compared_path(path));
                        }
                    }
                    ChatBlock::Text { text } => paths.extend(written_picture_paths(text)),
                    _ => {}
                }
            }
        }
        let shown_digests = digests
            .iter()
            .filter(|(path, _)| paths.contains(&compared_path(path)))
            .map(|(_, digest)| *digest)
            .collect();
        Self {
            paths,
            digests: shown_digests,
            known: Some(digests.clone()),
        }
    }

    /// Whether the picture at `path` is one the user already showed.
    pub fn contains(&self, path: &str) -> bool {
        self.paths.contains(&compared_path(path))
            || self
                .known
                .as_ref()
                .and_then(|known| known.get(path))
                .is_some_and(|digest| self.digests.contains(digest))
    }

    fn shows(&self, image: &Value) -> bool {
        image
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| self.contains(path))
    }
}

/// The digest `SeenPictures` compares, of a picture's base64 as `readSessionChatImage` sent it:
/// the same bytes always encode to the same text.
fn picture_digest(base64: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::hash::DefaultHasher::new();
    base64.hash(&mut hasher);
    hasher.finish()
}

/// Remembers the digest of a picture the chat read, and has the list built again when it shows
/// that two different paths hold one picture (a read of a picture the user showed).
pub fn note_picture_bytes(state: &mut crate::state::ChatState, path: &str, base64: &str) {
    let digest = picture_digest(base64);
    let view = &mut state.transcript_view;
    let twin = view
        .picture_digests
        .iter()
        .any(|(other, known)| *known == digest && other != path);
    if view.picture_digests.insert(path.to_string(), digest) == Some(digest) {
        return;
    }
    if twin {
        view.picture_digests_revision += 1;
        view.detail_revision += 1;
    }
}

/// On one projected message: marks each row picture the user already showed `userShown` (its
/// tile stays) and takes it off the message's `readImages` strip, copying the message only when
/// something changes.
fn mark_seen(message: &mut std::sync::Arc<Value>, seen: &SeenPictures) {
    let unmarked = |images: Option<&Value>| {
        images.and_then(Value::as_array).is_some_and(|images| {
            images
                .iter()
                .any(|image| seen.shows(image) != is_user_shown(image))
        })
    };
    let rows_with = |key: &str, message: &Value| {
        message
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|rows| rows.iter().any(|row| unmarked(row.get("images"))))
    };
    let card_rows = message
        .get("toolCard")
        .is_some_and(|card| rows_with("entries", card));
    let strip_shows = message
        .get("readImages")
        .and_then(Value::as_array)
        .is_some_and(|images| images.iter().any(|image| seen.shows(image)));
    if !rows_with("tools", message) && !card_rows && !strip_shows {
        return;
    }
    let message = std::sync::Arc::make_mut(message);
    let mark_rows = |rows: Option<&mut Value>| {
        if let Some(Value::Array(rows)) = rows {
            for image in rows
                .iter_mut()
                .filter_map(|row| row.get_mut("images").and_then(Value::as_array_mut))
                .flatten()
            {
                if let Value::Object(fields) = image {
                    if seen.shows(&Value::Object(fields.clone())) {
                        fields.insert(USER_SHOWN.to_string(), Value::Bool(true));
                    } else {
                        fields.shift_remove(USER_SHOWN);
                    }
                }
            }
        }
    };
    mark_rows(message.get_mut("tools"));
    mark_rows(
        message
            .get_mut("toolCard")
            .and_then(|card| card.get_mut("entries")),
    );
    if let Some(Value::Array(images)) = message.get_mut("readImages") {
        images.retain(|image| !seen.shows(image));
    }
}

/// The `{kind: "images", label}` count a folded turn's card shows beside its tool calls:
/// "1 image", "3 images".
pub fn images_stat(count: usize) -> Value {
    serde_json::json!({
        "kind": "images",
        "label": format!("{count} {}", if count == 1 { "image" } else { "images" }),
    })
}

/// Marks every row's pictures the user already showed (`SeenPictures`, `mark_seen`), then fills
/// each turn's `readImages` from the rows its fold holds (a finished turn's folded work and the
/// paths of the rows the server still holds back, a running turn's "Active work"), leaving those
/// pictures out.
///
/// A separate pass over the built rows, so the fold rules that decide which rows a turn folds stay
/// the only place that decides it.
pub fn bubble_read_images(items: &mut [TranscriptItem], seen: &SeenPictures) {
    for item in items.iter_mut() {
        match item {
            TranscriptItem::Message { message } => mark_seen(message, seen),
            TranscriptItem::Summary {
                user,
                final_message,
                earlier_replies,
                work,
                outcome,
                cards,
                read_images,
                ..
            } => {
                std::iter::once(user)
                    .chain(final_message.iter_mut())
                    .chain(earlier_replies.iter_mut())
                    .chain(work.iter_mut())
                    .chain(outcome.iter_mut())
                    .chain(cards.iter_mut())
                    .for_each(|message| mark_seen(message, seen));
                *read_images = turn_read_images(work.iter().map(|row| row.as_ref()), &[]);
            }
            TranscriptItem::CompletedWork {
                work,
                artifacts,
                final_message,
                deferred,
                read_images,
                stats,
                ..
            } => {
                work.iter_mut()
                    .chain(artifacts.iter_mut())
                    .chain(final_message.iter_mut())
                    .for_each(|message| mark_seen(message, seen));
                let held_back: Vec<String> = deferred
                    .as_ref()
                    .and_then(|deferred| deferred.get("imagePaths"))
                    .and_then(Value::as_array)
                    .map(|paths| {
                        paths
                            .iter()
                            .filter_map(Value::as_str)
                            .filter(|path| !seen.contains(path))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                *read_images = turn_read_images(work.iter().map(|row| row.as_ref()), &held_back);
                // The card's "3 images" count, first among its counts, says what the strip in the
                // card is.
                stats.retain(|stat| stat.get("kind").and_then(Value::as_str) != Some("images"));
                if !read_images.is_empty() {
                    stats.insert(0, images_stat(read_images.len()));
                }
            }
            TranscriptItem::Unknown(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostex_gx_protocol::ChatBlock;
    use serde_json::json;

    fn paths(name: &str, input: Value) -> Vec<String> {
        image_paths_in_call(name, &input, Some("/repo"))
    }

    #[test]
    fn claude_and_opencode_reads_of_a_picture() {
        assert_eq!(
            paths("Read", json!({"file_path": "/repo/shots/before.png"})),
            ["/repo/shots/before.png"]
        );
        assert_eq!(
            paths("read", json!({"filePath": "/repo/a/B.JPEG"})),
            ["/repo/a/B.JPEG"]
        );
        assert_eq!(
            paths("read_file", json!({"absolute_path": "/x/y.webp"})),
            ["/x/y.webp"]
        );
        // A relative path is under the session's folder.
        assert_eq!(
            paths("Read", json!({"file_path": "./docs/c.gif"})),
            ["/repo/docs/c.gif"]
        );
    }

    #[test]
    fn reads_of_other_files_and_writes_of_pictures_are_not_pictures_read() {
        assert!(paths("Read", json!({"file_path": "/repo/src/main.rs"})).is_empty());
        assert!(paths("Read", json!({"file_path": "/repo/icon.svg"})).is_empty());
        assert!(paths("Write", json!({"file_path": "/repo/out.png"})).is_empty());
        assert!(paths("Bash", json!({"command": "open /repo/out.png"})).is_empty());
        assert!(paths("Read", json!({"file_path": "/repo/.png"})).is_empty());
    }

    #[test]
    fn codex_view_image_as_a_call_and_inside_a_script() {
        assert_eq!(
            paths("view_image", json!({"path": "/Users/me/shot.png"})),
            ["/Users/me/shot.png"]
        );
        let script = json!(
            "const a=await tools.view_image({path:\"/s/one.png\",detail:\"high\"}); image(a.image_url);\nconst b=await tools.view_image({path:\"/s/two.jpg\"}); image(b.image_url);\nawait tools.exec_command({cmd:\"ls\", path:\"/s/not.png\"});"
        );
        assert_eq!(paths("exec", script), ["/s/one.png", "/s/two.jpg"]);
    }

    #[test]
    fn a_failed_read_shows_no_picture() {
        let call = ChatBlock::ToolCall {
            name: "Read".into(),
            input: json!({"file_path": "/repo/gone.png"}),
            call_id: Some("c1".into()),
        };
        let failed = ChatBlock::ToolResult {
            output: "File does not exist.".into(),
            is_error: Some(true),
            call_id: Some("c1".into()),
            patch: None,
        };
        let done = ChatBlock::ToolResult {
            output: "[image]".into(),
            is_error: None,
            call_id: Some("c1".into()),
            patch: None,
        };
        let pair = |result| ToolPair {
            call: Some(&call),
            result: Some(result),
        };
        assert!(tool_read_images(&pair(&failed), None).is_empty());
        let images = tool_read_images(&pair(&done), None);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0]["transport"], "read");
        assert_eq!(images[0]["path"], "/repo/gone.png");
        assert_eq!(images[0]["label"], "gone.png");
    }

    #[test]
    fn a_turn_lists_each_picture_once_then_the_held_back_ones() {
        let rows = [
            json!({"readImages": [read_image_source("/a.png"), read_image_source("/b.png")]}),
            json!({"readImages": [read_image_source("/a.png")]}),
            json!({"text": "no pictures"}),
        ];
        let images = turn_read_images(
            rows.iter(),
            &[
                "/b.png".to_string(),
                "/c.jpg".to_string(),
                "/d.rs".to_string(),
            ],
        );
        let paths: Vec<&str> = images
            .iter()
            .map(|image| image["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["/a.png", "/b.png", "/c.jpg"]);
    }
}
