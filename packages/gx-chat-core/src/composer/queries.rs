//! The five pure helpers the renderer asks for one gesture.
//!
//! They hold no state and change none. The phone reaches them through [`crate::query`]. Each
//! answer keeps the key order the TypeScript wrote, which the replay recording that checked the
//! port fingerprinted (`docs/2026-09-21/rust-chat/REPLAY.md`).
//!
//! `nativeChat` wired them at the end of
//! `packages/shared/session-chat-controller/native-host.ts`; this file is the same five functions
//! with the same argument shapes.

use serde::{Deserialize, Serialize};

use crate::composer::json::OrderedMap;
use crate::composer::keys::{ComposerKeyEvent, ComposerKeyIntent, KeyPlatform};
use crate::composer::policy::send_blocked_toast_request;
use crate::composer::reference_menu::{reference_menu_rows, ReferenceMenuRow};
use crate::composer::reference_pills::reference_pill_text;
use crate::composer::reference_pills::{composer_references as scan_references, ReferenceKind};
use crate::composer::transcript_menu::transcript_menu_rows;

/// One reference as `composerReferences` answers it: the scan plus the pill's measured text.
///
/// CDXC:SessionChat 2026-09-18 DECISION:
/// User: the GPUI composer shows `[Image #1](/path)` as the same clickable reference pill the React
/// composer shows. Both read this one projection so a pill's kind, label, and width cannot drift.
/// Offsets count UTF-16 code units, which is what a JS string index is; the host converts them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerReferenceRow {
    pub start: usize,
    pub end: usize,
    pub kind: ReferenceKind,
    pub label: String,
    pub path: String,
    pub pill: String,
}

/// The reference pills in a draft.
pub fn composer_references(text: &str) -> Vec<ComposerReferenceRow> {
    scan_references(text)
        .into_iter()
        .map(|reference| ComposerReferenceRow {
            start: reference.start,
            end: reference.end,
            kind: reference.kind,
            pill: reference_pill_text(&reference.label, reference.kind),
            label: reference.label,
            path: reference.path,
        })
        .collect()
}

/// What a background keystroke means for the composer.
pub fn composer_key_intent(
    event: &ComposerKeyEvent,
    platform: Option<&str>,
) -> Option<ComposerKeyIntent> {
    crate::composer::keys::composer_key_intent(
        event,
        platform.map(KeyPlatform::from_wire).unwrap_or_default(),
    )
}

/// The right-click menu for a reference.
pub fn reference_menu(href: &str) -> Vec<ReferenceMenuRow> {
    reference_menu_rows(href)
}

/// The transcript's whole right-click menu.
pub fn transcript_menu(
    href: Option<&str>,
    selection: &str,
    question_active: bool,
) -> Vec<ReferenceMenuRow> {
    transcript_menu_rows(href, selection, question_active)
}

/// The toast a blocked Send raises.
pub fn send_blocked_toast(reason: &str) -> OrderedMap {
    send_blocked_toast_request(reason)
}
