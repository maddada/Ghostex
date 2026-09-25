//! The chat view's runtime: the Rust chat host in `src/app/gx_chat/`, in both apps.
//!
//! `apps/gpui-web` links this file and the whole host folder, so the desktop and the web build run
//! the same chat host; the view keeps these two names for it.

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: "can we please disable quick js and delete it". The desktop chat always runs on `packages/gx-chat-core` through `src/app/gx_chat/`: the `chatBrain` setting, the QuickJS chat runtime, the shadow comparison and the QuickJS chat recorder are deleted, and an old saved `chatBrain` value is ignored. Supersedes the 2026-09-24 decision that made Rust the default and kept QuickJS selectable.
pub(crate) use crate::app::gx_chat::{
    ChatHostHandle as ChatRuntimeWorker, ChatHostOutput as ChatRuntimeOutput,
};
