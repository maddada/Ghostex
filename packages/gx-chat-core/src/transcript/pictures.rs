//! The transcript's pictures in reading order: what the picture viewer steps through.
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User: "when i see this in the chat view transcript on desktop/mobile i want to be able to go between the different images with right/left arrow and see buttons also when i hover near the right/left". A picture opened from the transcript opens on a list of every picture the transcript shows (each message's own thumbnails, then the pictures written into its text, message after message), so the viewer's arrows, keys, edge buttons and swipes move through the whole chat rather than one message. The list stops at its ends instead of wrapping, and a picture shown twice (the same file attached and linked again) is in it once.
//! SEE-ALSO: apps/desktop/src/app/native_chat/image_viewer/ and apps/mobile/app/src/chat/native/cards/ImageViewer.tsx draw the viewer from this list; `transcriptPictures` in query.rs is the phone's door to it.

use serde::Serialize;
use serde_json::Value;

use crate::document::TranscriptItem;
use crate::transcript::native_markdown::{NATIVE_IMAGE_OPEN, NATIVE_MARK};

/// The pictures a viewer steps through and the one it opens on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PictureGallery {
    pub images: Vec<Value>,
    pub index: usize,
}

/// Every picture the transcript shows, oldest first, each once.
///
/// Within a message its thumbnail row comes first, because every renderer draws it above the prose,
/// then the pictures its text carries in the order they are written. Within a turn the order is
/// the order the turn is drawn: the prompt, the work fold, then the replies.
pub fn transcript_pictures<'a>(items: impl IntoIterator<Item = &'a TranscriptItem>) -> Vec<Value> {
    let mut pictures = Vec::new();
    let mut seen = Vec::new();
    let mut add = |message: &Value| {
        for picture in message_pictures(message) {
            match picture_key(&picture) {
                Some(key) if seen.contains(&key) => {}
                Some(key) => {
                    seen.push(key);
                    pictures.push(picture);
                }
                None => pictures.push(picture),
            }
        }
    };
    for item in items {
        match item {
            TranscriptItem::Message { message } => add(message),
            TranscriptItem::Summary {
                user,
                final_message,
                earlier_replies,
                work,
                outcome,
                read_images,
                ..
            } => {
                add(user);
                work.iter().for_each(|message| add(message));
                add(&read_images_row(read_images));
                outcome.iter().for_each(|message| add(message));
                earlier_replies.iter().for_each(|message| add(message));
                if let Some(message) = final_message {
                    add(message);
                }
            }
            TranscriptItem::CompletedWork {
                work,
                artifacts,
                final_message,
                read_images,
                ..
            } => {
                work.iter().for_each(|message| add(message));
                add(&read_images_row(read_images));
                artifacts.iter().for_each(|message| add(message));
                if let Some(message) = final_message {
                    add(message);
                }
            }
            TranscriptItem::GroupedWork {
                work,
                turns,
                read_images,
                ..
            } => {
                work.iter().for_each(|message| add(message));
                add(&read_images_row(read_images));
                for turn in turns {
                    turn.artifacts.iter().for_each(|message| add(message));
                    if let Some(message) = &turn.final_message {
                        add(message);
                    }
                }
            }
            TranscriptItem::Unknown(_) => {}
        }
    }
    pictures
}

/// The transcript's pictures, opened on `picture`; just `picture` when the transcript does not
/// show it (a picture from a subagent's own transcript, or one the list has moved past).
pub fn picture_gallery<'a>(
    items: impl IntoIterator<Item = &'a TranscriptItem>,
    picture: &Value,
) -> PictureGallery {
    let images = transcript_pictures(items);
    let wanted = picture_key(picture);
    let index = images.iter().position(|image| match &wanted {
        Some(key) => picture_key(image).as_ref() == Some(key),
        None => image == picture,
    });
    match index {
        Some(index) => PictureGallery { images, index },
        None => PictureGallery {
            images: vec![picture.clone()],
            index: 0,
        },
    }
}

/// A turn's strip of pictures read from disk, as a message whose thumbnail row it is: it stands
/// in for the folded rows (or the rows the server still holds back) at the fold's place.
fn read_images_row(read_images: &[Value]) -> Value {
    serde_json::json!({ "images": read_images })
}

/// One message's pictures: its `images` row, the pictures its tool calls opened from disk
/// (`readImages`, `read_images.rs`), then the image marks in its `text`.
fn message_pictures(message: &Value) -> Vec<Value> {
    let mut pictures: Vec<Value> = ["images", "readImages"]
        .iter()
        .filter_map(|key| message.get(*key).and_then(Value::as_array))
        .flatten()
        .cloned()
        .collect();
    if let Some(text) = message.get("text").and_then(Value::as_str) {
        pictures.extend(marked_pictures(text));
    }
    pictures
}

/// The pictures marked into a body by `native_markdown`, in the order they are written.
pub fn marked_pictures(text: &str) -> Vec<Value> {
    let mut pictures = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(NATIVE_IMAGE_OPEN) {
        let payload = &rest[start + NATIVE_IMAGE_OPEN.len()..];
        let Some(end) = payload.find(NATIVE_MARK) else {
            break;
        };
        if let Ok(picture) = serde_json::from_str::<Value>(&payload[..end]) {
            pictures.push(picture);
        }
        rest = &payload[end + NATIVE_MARK.len_utf8()..];
    }
    pictures
}

/// What makes two pictures the same picture: where its bytes come from. The label differs between
/// a thumbnail and the same file written into prose, so it is not part of it. `None` for a picture
/// with no source at all, which is never merged with another.
fn picture_key(picture: &Value) -> Option<(String, String, String)> {
    let field = |key: &str| {
        picture
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (path, url) = (field("path"), field("url"));
    (!path.is_empty() || !url.is_empty()).then(|| (field("transport"), path, url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::images::{image_source, ImageRef};
    use crate::transcript::native_markdown::native_markdown;
    use serde_json::json;
    use std::sync::Arc;

    fn file(path: &str) -> Value {
        image_source(&ImageRef {
            path: Some(path.to_string()),
            url: None,
            alt: None,
        })
    }

    fn message(id: &str, images: Vec<Value>, markdown: &str) -> Arc<Value> {
        Arc::new(json!({
            "id": id,
            "images": images,
            "text": native_markdown(markdown, false),
        }))
    }

    fn paths(images: &[Value]) -> Vec<&str> {
        images
            .iter()
            .map(|image| image["path"].as_str().unwrap_or_default())
            .collect()
    }

    #[test]
    fn lists_thumbnails_then_inline_pictures_message_by_message() {
        let items = vec![
            TranscriptItem::Message {
                message: message("u1", vec![file("/a.png")], "look at this"),
            },
            TranscriptItem::Message {
                message: message(
                    "a1",
                    vec![file("/b.png")],
                    "- ![one](/c.png): first\n- ![two](/d.png): second\n\nAnd [the chart](/e.jpg).",
                ),
            },
        ];
        let pictures = transcript_pictures(&items);
        assert_eq!(
            paths(&pictures),
            ["/a.png", "/b.png", "/c.png", "/d.png", "/e.jpg"]
        );
    }

    #[test]
    fn follows_a_turn_in_drawing_order_and_skips_code() {
        let items = vec![
            TranscriptItem::Summary {
                id: "t1".into(),
                user: message("u1", vec![file("/prompt.png")], "hi"),
                final_message: Some(message("f", vec![], "![done](/final.png)")),
                earlier_replies: vec![message("e", vec![], "![early](/early.png)")],
                active: false,
                work_open: false,
                work: vec![message("w", vec![file("/work.png")], "")],
                outcome: vec![],
                latest_reply: true,
                cards: vec![],
                read_images: vec![],
            },
            TranscriptItem::Message {
                message: message("a2", vec![], "```md\n![not a picture](/code.png)\n```"),
            },
        ];
        assert_eq!(
            paths(&transcript_pictures(&items)),
            ["/prompt.png", "/work.png", "/early.png", "/final.png"]
        );
    }

    #[test]
    fn a_picture_shown_twice_is_listed_once_and_opens_where_it_first_appears() {
        let items = vec![
            TranscriptItem::Message {
                message: message("u1", vec![file("/shot.png")], ""),
            },
            TranscriptItem::Message {
                message: message("a1", vec![], "![again](/shot.png) and ![next](/next.png)"),
            },
        ];
        let inline = marked_pictures(&native_markdown("![again](/shot.png)", false))
            .pop()
            .unwrap();
        let gallery = picture_gallery(&items, &inline);
        assert_eq!(paths(&gallery.images), ["/shot.png", "/next.png"]);
        assert_eq!(gallery.index, 0);
        let next = marked_pictures(&native_markdown("![x](/next.png)", false))
            .pop()
            .unwrap();
        assert_eq!(picture_gallery(&items, &next).index, 1);
    }

    #[test]
    fn pictures_a_turn_read_follow_its_prompt_in_the_order_they_were_read() {
        let mut work = json!({"id": "w", "images": [], "text": ""});
        work["readImages"] = json!([file("/read-1.png"), file("/read-2.png")]);
        let items = vec![
            TranscriptItem::Message {
                message: message("u1", vec![file("/prompt.png")], ""),
            },
            TranscriptItem::CompletedWork {
                id: "u1".into(),
                label: "Worked for 4s".into(),
                stats: vec![],
                files_card: Value::Null,
                files: vec![],
                files_label: String::new(),
                simple_files_label: String::new(),
                expandable: true,
                deferred: None,
                work: vec![Arc::new(work)],
                questions: vec![],
                sent_messages: vec![],
                gbot_cards: vec![],
                workflows: vec![],
                artifacts: vec![],
                final_message: Some(message("f", vec![], "done")),
                // The folded work's pictures and one only the server's held-back rows name.
                read_images: vec![file("/read-1.png"), file("/read-2.png"), file("/held.png")],
                cards_in_fold: false,
                tally: Default::default(),
            },
        ];
        assert_eq!(
            paths(&transcript_pictures(&items)),
            ["/prompt.png", "/read-1.png", "/read-2.png", "/held.png"]
        );
        let gallery = picture_gallery(&items, &file("/read-2.png"));
        assert_eq!(gallery.index, 2);
    }

    #[test]
    fn a_picture_the_transcript_does_not_show_opens_alone() {
        let items = vec![TranscriptItem::Message {
            message: message("u1", vec![file("/a.png")], ""),
        }];
        let gallery = picture_gallery(&items, &file("/elsewhere.png"));
        assert_eq!(paths(&gallery.images), ["/elsewhere.png"]);
        assert_eq!(gallery.index, 0);
    }
}
