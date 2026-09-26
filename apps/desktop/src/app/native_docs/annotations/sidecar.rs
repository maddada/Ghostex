//! The annotations sidecar, `.ghostex/manage-annotations.json`: every file's notes keyed by its
//! docs-relative path. Parsing drops bad paths and invalid notes and keeps the file's key order;
//! serializing writes the same pretty-printed bytes `JSON.stringify(store, null, 2)` writes.
//! Rename, move and delete remap or drop keys, subtrees included.
//!
//! CDXC:Docs 2026-09-24 SEE-ALSO: apps/desktop/views/manage/annotation-store.tsx
//! (`parseManageAnnotationStore`, `serializeManageAnnotationStore`), manage-app.tsx (load and
//! 550 ms save) and file-tree-utils.ts (`remapManageAnnotationPathsForMove`,
//! `removeManageAnnotationPathsForDeletedEntry`). The React page reads and writes the same file,
//! so the format and key order must stay identical until that page is deleted.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use super::model::{
    DOCS_ANNOTATION_ID_PREFIX, DOCS_ANNOTATION_IMAGE_ID_PREFIX, DOCS_ANNOTATION_IMAGE_MAX_BYTES,
    DOCS_ANNOTATION_MAX_IMAGES, DOCS_ANNOTATION_NOTE_MAX_LEN, DocsAnnotation, DocsAnnotationImage,
    DocsAnnotationScope, DocsAnnotationType, DocsQuickLabelId, annotation_review_counts,
    is_valid_annotation, iso_timestamp_from_ms, js_date_parse_ms, js_trim, mark_annotations_sent,
    normalize_annotation_quote, normalize_attachment_name, truncate_utf16,
};

/// Project-relative sidecar path; readable and writable even though `.ghostex` is otherwise hidden.
pub(crate) const DOCS_ANNOTATIONS_SIDECAR_PATH: &str = ".ghostex/manage-annotations.json";
pub(crate) const DOCS_ANNOTATION_SCHEMA_VERSION: u32 = 1;
/// A change is written this long after the last edit, and only when the stable key changed.
pub(crate) const DOCS_ANNOTATIONS_SAVE_DELAY_MS: u64 = 550;

/// Notes by docs-relative path, in the order a JavaScript object enumerates its keys (array-index
/// keys such as `"12"` first in numeric order, then the rest in insertion order), so every
/// iteration, the stable key and the saved file match the React page.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocsAnnotationsByPath {
    entries: Vec<(String, Vec<DocsAnnotation>)>,
}

impl DocsAnnotationsByPath {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of paths with notes.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &[DocsAnnotation])> {
        self.entries
            .iter()
            .map(|(path, annotations)| (path.as_str(), annotations.as_slice()))
    }

    pub(crate) fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(path, _)| path.as_str())
    }

    pub(crate) fn get(&self, path: &str) -> Option<&[DocsAnnotation]> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, annotations)| annotations.as_slice())
    }

    pub(crate) fn get_mut(&mut self, path: &str) -> Option<&mut Vec<DocsAnnotation>> {
        self.entries
            .iter_mut()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, annotations)| annotations)
    }

    /// Number of notes on one path (the files-tree badge).
    pub(crate) fn count(&self, path: &str) -> usize {
        self.get(path).map_or(0, <[DocsAnnotation]>::len)
    }

    /// Stores a file's notes; an empty list removes the path, as `updateAnnotationsForSelectedFile`
    /// does. An existing path keeps its position.
    pub(crate) fn set(&mut self, path: &str, annotations: Vec<DocsAnnotation>) {
        if annotations.is_empty() {
            self.remove(path);
        } else {
            js_object_insert(&mut self.entries, path.to_string(), annotations);
        }
    }

    pub(crate) fn remove(&mut self, path: &str) -> Option<Vec<DocsAnnotation>> {
        let index = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == path)?;
        Some(self.entries.remove(index).1)
    }

    /// A rename or move of `source_path` (a file or a folder) to `destination_path`: every key at
    /// or under the source moves, and a key that lands on an existing one appends to its list.
    /// Returns whether anything moved.
    pub(crate) fn remap_for_move(&mut self, source_path: &str, destination_path: &str) -> bool {
        let mut changed = false;
        let mut next: Vec<(String, Vec<DocsAnnotation>)> = Vec::with_capacity(self.entries.len());
        for (path, annotations) in std::mem::take(&mut self.entries) {
            let key = match remap_docs_path_by_move(&path, source_path, destination_path) {
                Some(moved) => {
                    changed = true;
                    moved
                }
                None => path,
            };
            match next.iter_mut().find(|(candidate, _)| *candidate == key) {
                Some((_, existing)) => existing.extend(annotations),
                None => js_object_insert(&mut next, key, annotations),
            }
        }
        self.entries = next;
        changed
    }

    /// Drops the notes of a deleted file or folder, subtree included. Returns whether any went.
    pub(crate) fn remove_for_deleted_entry(&mut self, deleted_path: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(path, _)| {
            path != deleted_path && !is_docs_descendant_path(path, deleted_path)
        });
        self.entries.len() != before
    }

    /// Stamps `sentAt` on the ids a send covered, per path. Returns whether any note changed.
    pub(crate) fn mark_sent(
        &mut self,
        ids_by_path: &[(String, Vec<String>)],
        sent_at: &str,
    ) -> bool {
        let mut changed = false;
        for (path, ids) in ids_by_path {
            if let Some(annotations) = self.get_mut(path) {
                changed |= mark_annotations_sent(annotations, ids, sent_at);
            }
        }
        changed
    }

    /// Paths holding something to send for `send_all` ("Send new across all files" and its
    /// `all` twin): at least one note, or at least one pending note, in `Array.prototype.sort`
    /// order (UTF-16 code units).
    pub(crate) fn paths_with_notes(&self, pending_only: bool) -> Vec<String> {
        let mut paths: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, annotations)| {
                if pending_only {
                    annotation_review_counts(annotations).pending > 0
                } else {
                    !annotations.is_empty()
                }
            })
            .map(|(path, _)| path.clone())
            .collect();
        paths.sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
        paths
    }

    /// `JSON.stringify(annotationsByPath)`: compare against the last saved key to decide whether
    /// a save is needed.
    pub(crate) fn stable_key(&self) -> String {
        serde_json::to_string(self).expect("annotations serialize to JSON")
    }
}

impl Serialize for DocsAnnotationsByPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (path, annotations) in &self.entries {
            map.serialize_entry(path, annotations)?;
        }
        map.end()
    }
}

/// Reads the sidecar. An empty, missing-shape or unparsable file counts as empty; bad paths and
/// invalid notes are dropped; a path left with no notes is left out. `now_ms` stands in for
/// `Date.now()` where a stored note lacks `createdAt` or an id.
pub(crate) fn parse_docs_annotations_sidecar(content: &str, now_ms: i64) -> DocsAnnotationsByPath {
    let mut store = DocsAnnotationsByPath::new();
    if js_trim(content).is_empty() {
        return store;
    }
    let content = drop_lone_surrogate_escapes(content);
    let Ok(raw) = serde_json::from_str::<RawSidecar>(&content) else {
        return store;
    };
    let Some(raw_entries) = raw.annotations_by_path.and_then(|object| object.0) else {
        return store;
    };
    for (path, annotations) in raw_entries {
        let Some(path) = normalize_stored_annotation_path(&path) else {
            continue;
        };
        let Value::Array(annotations) = annotations else {
            continue;
        };
        let annotations: Vec<DocsAnnotation> = annotations
            .iter()
            .filter_map(|annotation| normalize_stored_annotation(annotation, now_ms))
            .collect();
        if !annotations.is_empty() {
            js_object_insert(&mut store.entries, path, annotations);
        }
    }
    store
}

/// The sidecar file content: `{annotationsByPath, updatedAt, version: 1}` with 2-space
/// indentation and a trailing newline. `updated_at` is the save time as an ISO string.
pub(crate) fn serialize_docs_annotations_sidecar(
    annotations_by_path: &DocsAnnotationsByPath,
    updated_at: &str,
) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Sidecar<'a> {
        annotations_by_path: &'a DocsAnnotationsByPath,
        updated_at: &'a str,
        version: u32,
    }
    let mut text = serde_json::to_string_pretty(&Sidecar {
        annotations_by_path,
        updated_at,
        version: DOCS_ANNOTATION_SCHEMA_VERSION,
    })
    .expect("annotations serialize to JSON");
    text.push('\n');
    text
}

/// Same as `serialize_docs_annotations_sidecar`, stamped with `now_ms`.
pub(crate) fn serialize_docs_annotations_sidecar_at(
    annotations_by_path: &DocsAnnotationsByPath,
    now_ms: i64,
) -> String {
    serialize_docs_annotations_sidecar(annotations_by_path, &iso_timestamp_from_ms(now_ms))
}

/// A stored key as a clean relative path: no leading `/`, no NUL, no `.` or `..` segment, empty
/// segments dropped. `None` rejects the key.
pub(crate) fn normalize_stored_annotation_path(path: &str) -> Option<String> {
    let trimmed = js_trim(path);
    if trimmed.is_empty() || trimmed.starts_with('/') || trimmed.contains('\0') {
        return None;
    }
    let components: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
    if components.iter().any(|part| *part == "." || *part == "..") {
        return None;
    }
    Some(components.join("/"))
}

/// One stored note, cleaned the way `normalizeStoredAnnotation` cleans it, or `None` when it is
/// not a valid note.
pub(crate) fn normalize_stored_annotation(value: &Value, now_ms: i64) -> Option<DocsAnnotation> {
    let object = value.as_object()?;
    let kind = DocsAnnotationType::parse(object.get("type")?.as_str()?)?;
    let quote = object
        .get("quote")
        .and_then(Value::as_str)
        .map(normalize_annotation_quote)
        .unwrap_or_default();
    let note = object
        .get("note")
        .and_then(Value::as_str)
        .map(|note| truncate_utf16(note, DOCS_ANNOTATION_NOTE_MAX_LEN).to_string())
        .unwrap_or_default();
    let attachments: Vec<DocsAnnotationImage> = match object.get("attachments") {
        Some(Value::Array(attachments)) => attachments
            .iter()
            .filter_map(|attachment| normalize_stored_attachment(attachment, now_ms))
            .take(DOCS_ANNOTATION_MAX_IMAGES)
            .collect(),
        _ => Vec::new(),
    };
    if !is_valid_annotation(kind, &quote, &note, attachments.len()) {
        return None;
    }
    let created_at = match object.get("createdAt") {
        Some(Value::String(created_at)) => created_at.clone(),
        _ => iso_timestamp_from_ms(now_ms),
    };
    let id = match object.get("id") {
        Some(Value::String(id)) if !js_trim(id).is_empty() => id.clone(),
        _ => format!("{DOCS_ANNOTATION_ID_PREFIX}{now_ms}"),
    };
    Some(DocsAnnotation {
        attachments,
        created_at,
        id,
        label_id: object
            .get("labelId")
            .and_then(Value::as_str)
            .and_then(DocsQuickLabelId::parse),
        note,
        scope: DocsAnnotationScope::for_quote(&quote),
        quote,
        kind,
        updated_at: stored_timestamp(object.get("updatedAt")),
        sent_at: stored_timestamp(object.get("sentAt")),
    })
}

/// One stored image attachment (`normalizeStoredAttachment`): a `data:image/` URL, an `image/`
/// type, at most 512 KiB.
pub(crate) fn normalize_stored_attachment(
    value: &Value,
    now_ms: i64,
) -> Option<DocsAnnotationImage> {
    let object = value.as_object()?;
    let data_url = object.get("dataUrl").and_then(Value::as_str).unwrap_or("");
    let mime_type = object.get("mimeType").and_then(Value::as_str).unwrap_or("");
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(normalize_attachment_name)
        .unwrap_or_else(|| "image".to_string());
    let size = object
        .get("size")
        .and_then(Value::as_f64)
        .filter(|size| size.is_finite())
        .map_or(0.0, |size| size.max(0.0));
    if !data_url.starts_with("data:image/")
        || !mime_type.starts_with("image/")
        || size > DOCS_ANNOTATION_IMAGE_MAX_BYTES
    {
        return None;
    }
    let id = match object.get("id") {
        Some(Value::String(id)) if !js_trim(id).is_empty() => id.clone(),
        _ => format!("{DOCS_ANNOTATION_IMAGE_ID_PREFIX}{now_ms}"),
    };
    Some(DocsAnnotationImage {
        data_url: data_url.to_string(),
        id,
        mime_type: mime_type.to_string(),
        name,
        size,
    })
}

/// Where `path` lands when `source_path` moves to `destination_path`; `None` when unaffected.
pub(crate) fn remap_docs_path_by_move(
    path: &str,
    source_path: &str,
    destination_path: &str,
) -> Option<String> {
    if path == source_path {
        return Some(destination_path.to_string());
    }
    if is_docs_descendant_path(path, source_path) {
        return Some(format!("{destination_path}{}", &path[source_path.len()..]));
    }
    None
}

/// `path` sits inside the folder `ancestor_path`.
pub(crate) fn is_docs_descendant_path(path: &str, ancestor_path: &str) -> bool {
    path.len() > ancestor_path.len()
        && path.starts_with(ancestor_path)
        && path.as_bytes()[ancestor_path.len()] == b'/'
}

fn stored_timestamp(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?;
    js_date_parse_ms(value).map(|_| value.to_string())
}

/// Sets `key` the way assigning to a JavaScript object property does: an existing key keeps its
/// place, a new array-index key goes among the leading index keys in numeric order, any other new
/// key goes last.
fn js_object_insert<T>(entries: &mut Vec<(String, T)>, key: String, value: T) {
    if let Some((_, existing)) = entries.iter_mut().find(|(candidate, _)| *candidate == key) {
        *existing = value;
        return;
    }
    match js_array_index(&key) {
        Some(index) => {
            let position = entries
                .iter()
                .position(|(candidate, _)| {
                    js_array_index(candidate).is_none_or(|other| other > index)
                })
                .unwrap_or(entries.len());
            entries.insert(position, (key, value));
        }
        None => entries.push((key, value)),
    }
}

/// A canonical array index (`"0"` to `"4294967294"`), which JavaScript enumerates before other keys.
fn js_array_index(key: &str) -> Option<u32> {
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    key.parse::<u32>().ok().filter(|index| *index != u32::MAX)
}

/// `JSON.stringify` writes a lone UTF-16 surrogate (a note or quote the page cut mid-emoji) as
/// `\udXXX`, which serde_json refuses; one such note would otherwise empty the whole store on
/// load and the next save would erase every note. The half character is dropped instead.
fn drop_lone_surrogate_escapes(content: &str) -> Cow<'_, str> {
    if !content.contains("\\u") {
        return Cow::Borrowed(content);
    }
    let bytes = content.as_bytes();
    let surrogate_at = |index: usize| -> Option<u16> {
        if bytes.get(index) != Some(&b'\\') || bytes.get(index + 1) != Some(&b'u') {
            return None;
        }
        let hex = content.get(index + 2..index + 6)?;
        let unit = u16::from_str_radix(hex, 16).ok()?;
        (0xD800..=0xDFFF).contains(&unit).then_some(unit)
    };
    let mut output = String::with_capacity(content.len());
    let mut changed = false;
    let mut index = 0;
    let mut copied_from = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            index += 1;
            continue;
        }
        match surrogate_at(index) {
            Some(0xD800..=0xDBFF)
                if surrogate_at(index + 6).is_some_and(|low| (0xDC00..=0xDFFF).contains(&low)) =>
            {
                index += 12;
            }
            Some(_) => {
                output.push_str(&content[copied_from..index]);
                index += 6;
                copied_from = index;
                changed = true;
            }
            None => index += 2,
        }
    }
    if !changed {
        return Cow::Borrowed(content);
    }
    output.push_str(&content[copied_from..]);
    Cow::Owned(output)
}

/// The top level of the sidecar: only `annotationsByPath` matters, the last one wins like
/// `JSON.parse`.
struct RawSidecar {
    annotations_by_path: Option<OrderedObject>,
}

impl<'de> Deserialize<'de> for RawSidecar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RawSidecarVisitor;
        impl<'de> Visitor<'de> for RawSidecarVisitor {
            type Value = RawSidecar;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("the annotations sidecar object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawSidecar, A::Error> {
                let mut annotations_by_path = None;
                while let Some(key) = map.next_key::<Cow<'de, str>>()? {
                    if key == "annotationsByPath" {
                        annotations_by_path = Some(map.next_value::<OrderedObject>()?);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(RawSidecar {
                    annotations_by_path,
                })
            }
        }
        deserializer.deserialize_map(RawSidecarVisitor)
    }
}

/// A JSON object's entries in JavaScript enumeration order, or `None` when the value is not an
/// object. serde_json's own `Map` sorts keys unless `preserve_order` is on, so it cannot be used.
struct OrderedObject(Option<Vec<(String, Value)>>);

impl<'de> Deserialize<'de> for OrderedObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedObjectVisitor;
        impl<'de> Visitor<'de> for OrderedObjectVisitor {
            type Value = OrderedObject;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any JSON value")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<OrderedObject, A::Error> {
                let mut entries = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    js_object_insert(&mut entries, key, value);
                }
                Ok(OrderedObject(Some(entries)))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<OrderedObject, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(OrderedObject(None))
            }

            fn visit_bool<E: de::Error>(self, _: bool) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }

            fn visit_i64<E: de::Error>(self, _: i64) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }

            fn visit_u64<E: de::Error>(self, _: u64) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }

            fn visit_f64<E: de::Error>(self, _: f64) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }

            fn visit_str<E: de::Error>(self, _: &str) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }

            fn visit_unit<E: de::Error>(self) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }

            fn visit_none<E: de::Error>(self) -> Result<OrderedObject, E> {
                Ok(OrderedObject(None))
            }
        }
        deserializer.deserialize_any(OrderedObjectVisitor)
    }
}
