//! Chat drafts and attachments: attachment picks, image saves, stashes, the draft handoff to the terminal, composer inserts and launch drafts.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// CDXC:Drafts 2026-09-10 DECISION:
/// User: view switches must preserve unsent text with durable identities, acknowledged transfers, and independent recovery history.
/// The composer saves its exact revision before vacating; the server retains recovery after terminal placement, superseding the temporary-stash-only handoff.
#[derive(Clone, Debug)]
pub(crate) struct GpuiSessionChatDraftHandoff {
    /// Exact composer text the terminal must receive.
    pub(crate) content: String,
    pub(crate) handoff_id: String,
    pub(crate) draft_version: Option<serde_json::Value>,
    /// The Saved Prompts row holding the durable copy, when this handoff
    /// created it. `None` means the save matched a prompt the user had already
    /// saved by hand, which must stay in Saved Prompts.
    pub(crate) stashed_prompt_id: Option<String>,
}

/// Next free path in Downloads for `<session>-1.png` without overwriting.
fn session_chat_image_downloads_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("session");
    let extension = path.extension().and_then(|ext| ext.to_str());
    let (base, mut next) = match stem.rsplit_once('-') {
        Some((base, suffix))
            if !base.is_empty()
                && !suffix.is_empty()
                && suffix.chars().all(|ch| ch.is_ascii_digit()) =>
        {
            (
                base.to_string(),
                suffix.parse::<u32>().unwrap_or(1).saturating_add(1),
            )
        }
        _ => (stem.to_string(), 2),
    };
    loop {
        let numbered = match extension {
            Some(ext) => format!("{base}-{next}.{ext}"),
            None => format!("{base}-{next}"),
        };
        let path = directory.join(&numbered);
        if !path.exists() {
            return path;
        }
        if next == u32::MAX {
            return directory.join(format!("{base}-{next}-{}", next));
        }
        next += 1;
    }
}

impl GhostexGpuiApp {
    pub(crate) fn request_session_chat_attachment_picks(
        &mut self,
        session_id: TerminalSessionId,
        request_id: String,
        directories_only: Option<bool>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(generation) = self.begin_session_chat_native_request(session_id) else {
            return;
        };
        let receiver =
            cx.prompt_for_paths(Self::attachment_path_prompt_options(true, directories_only));
        cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                _ => Vec::new(),
            };
            let _ = this.update(cx, |this, cx| {
                let payload = serde_json::json!({ "requestId": request_id, "paths": picked.iter().map(|path| path.to_string_lossy().to_string()).collect::<Vec<_>>() });
                this.dispatch_session_chat_generation_response(generation, "onSessionChatAttachmentsPicked", &payload, true, cx);
            });
        })
        .detach();
    }

    pub(crate) fn request_session_chat_image_save(
        &mut self,
        session_id: TerminalSessionId,
        request_id: String,
        suggested_name: String,
        base64_data: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(generation) = self.begin_session_chat_native_request(session_id) else {
            return;
        };
        let bytes = match BASE64_STANDARD.decode(base64_data.as_bytes()) {
            Ok(bytes) => bytes,
            Err(error) => {
                support_logs::append(
                    support_logs::GpuiSupportLog::AppModal,
                    "gpui.sessionChat.imageSaveFailed",
                    serde_json::json!({ "error": error.to_string(), "stage": "decode" }),
                );
                self.dispatch_session_chat_generation_response(
                    generation,
                    "onSessionChatImageSaved",
                    &serde_json::json!({ "requestId": request_id, "error": "The image bytes could not be read." }),
                    true,
                    cx,
                );
                return;
            }
        };
        // A file name, never a path: the page supplies `<session>-1.png`.
        let file_name = Path::new(&suggested_name)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "session-1.png".to_string());
        let downloads = home_dir().join("Downloads");
        cx.spawn(async move |this, cx| {
            let write_result = (|| -> Result<PathBuf, String> {
                fs::create_dir_all(&downloads).map_err(|error| error.to_string())?;
                let destination = session_chat_image_downloads_path(&downloads, &file_name);
                fs::write(&destination, &bytes).map_err(|error| error.to_string())?;
                Ok(destination)
            })();
            let _ = this.update(cx, |this, cx| match write_result {
                Ok(destination) => {
                    let saved_name = destination
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| file_name.clone());
                    this.dispatch_gpui_workspace_action_toast(
                        "success",
                        "Saved to Downloads",
                        &saved_name,
                        cx,
                    );
                    this.dispatch_session_chat_generation_response(generation, "onSessionChatImageSaved", &serde_json::json!({"requestId": request_id}), true, cx);
                }
                Err(error) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::AppModal,
                        "gpui.sessionChat.imageSaveFailed",
                        serde_json::json!({
                            "error": error,
                            "stage": "write",
                        }),
                    );
                    this.dispatch_session_chat_generation_response(generation, "onSessionChatImageSaved", &serde_json::json!({"requestId": request_id, "error": "The image could not be written."}), true, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn request_session_chat_stash_prompt(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(view) = self.native_chat_views.get(&session_id).cloned() {
            view.update(cx, |view, cx| {
                view.invoke(serde_json::json!({"type":"stash","text":view.draft}), cx)
            });
        }
    }

    pub(crate) fn request_session_chat_handoff_to_terminal(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        // This is the Chat View button's directional command, not the shared
        // toggle hotkey. Its host message crosses an async hop, so it
        // may arrive after a newer native Chat View click has already switched
        // the session back from terminal. Ignore that stale request instead of
        // toggling from whichever state happens to be current when it lands.
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return;
        }
        if let Some(view) = self.native_chat_views.get(&session_id).cloned()
            && !view.read(cx).draft.is_empty()
        {
            self.pending_session_chat_draft_handoffs.insert(session_id);
            view.update(cx, |view, cx| view.invoke(serde_json::json!({"type":"handoff","text":view.draft,"draftVersion":{"draftId":view.draft_id,"revision":view.draft_revision}}), cx));
        }
        self.toggle_agents_session_chat_mode(session_id, cx);
    }

    /// Release a legacy temporary stash after placement; the independent draft journal remains until submission or explicit discard.
    pub(crate) fn release_session_chat_draft_handoff_stash(
        &self,
        handoff: GpuiSessionChatDraftHandoff,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(prompt_id) = handoff.stashed_prompt_id else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = gpui_gxserver_rpc_result(
                    "/api/deleteStashedPrompt",
                    &serde_json::json!({ "promptId": prompt_id }),
                    std::time::Duration::from_secs(5),
                );
            })
            .detach();
    }

    /*
    CDXC:Drafts 2026-08-24:
    Delivery of a handed-off draft, decoupled from the focus-handoff drains:
    those run once per remount and always before the chat page's async
    save-then-clear answers, so a record parked after the drain used to wait,
    invisible, for a second view switch. This follows the pane's CURRENT
    owner: a session already back in chat mode gets the draft returned to the
    composer instead of a paste into a terminal the user is no longer looking
    at. Returns true when nothing is pending any more (delivered, or moved
    back to chat); false means the record stayed parked and a retry may help.
    The remove-after-success shape is load-bearing — the record points at the
    draft's durable Saved Prompts row, and only a confirmed terminal paste may
    delete that row.
    */
    pub(crate) fn deliver_pending_session_terminal_composer_insert(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(handoff) = self
            .pending_session_terminal_composer_insert
            .get(&session_id)
            .cloned()
        else {
            return true;
        };
        if self.agents_chat_mode_sessions.contains(&session_id) {
            // The draft goes back where the user is. The Saved Prompts row
            // stays: reaching the composer is not a confirmed terminal paste.
            self.pending_session_terminal_composer_insert
                .remove(&session_id);
            self.pending_session_chat_draft_handoffs.remove(&session_id);
            self.pending_session_chat_received_drafts.insert(session_id, serde_json::json!({"content":handoff.content,"draftVersion":handoff.draft_version,"handoffId":handoff.handoff_id}));
            self.deliver_pending_session_chat_received_draft(session_id, cx);
            self.schedule_session_chat_received_draft_delivery(session_id, cx);
            return true;
        }
        self.dispatch_session_chat_terminal_draft_handoff(session_id, handoff, cx)
    }

    /*
    A terminal surface can still be remounting when the draft handoff answer
    arrives; surface reconciliation is event-driven, so poll briefly instead
    of waiting for the next unrelated event. Running out of attempts leaves
    the record parked (and the draft in Saved Prompts) for the next focus
    handoff or return to chat — never dropped.
    */
    pub(crate) fn schedule_pending_session_terminal_composer_insert_delivery(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            for _ in 0..PENDING_TERMINAL_COMPOSER_INSERT_MAX_ATTEMPTS {
                cx.background_executor()
                    .timer(PENDING_TERMINAL_COMPOSER_INSERT_RETRY_INTERVAL)
                    .await;
                match this.update(cx, |this, cx| {
                    this.deliver_pending_session_terminal_composer_insert(session_id, cx)
                }) {
                    Ok(false) => {}
                    // Delivered, moved back to chat, or the app is gone.
                    _ => return,
                }
            }
        })
        .detach();
    }

    pub(crate) fn insert_prompt_into_session_chat(
        &mut self,
        session_id: TerminalSessionId,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        if !view.read(cx).composer_ready {
            self.hold_for_session_chat_composer(session_id, content);
            return true;
        }
        view.update(cx, |view, cx| view.insert_prompt(content, cx));
        true
    }

    /// Adds `content` after what is in the session's chat box; before the chat box is ready it
    /// joins whatever is already waiting to go in.
    ///
    /// CDXC:Drafts 2026-10-25 WHY:
    /// The view's own `composer_ready` decides, not `session_chat_composer_ready_sessions`: a
    /// prewarmed view reports ready before its session is in Chat, so that set could miss a
    /// composer the user was typing in, and every comment was held until the next click.
    pub(crate) fn append_to_session_chat_composer(
        &mut self,
        session_id: TerminalSessionId,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = self
            .native_chat_views
            .get(&session_id)
            .filter(|view| view.read(cx).composer_ready)
            .cloned();
        match view {
            Some(view) => view.update(cx, |view, cx| view.append_prompt(content, cx)),
            None => self.hold_for_session_chat_composer(session_id, content),
        }
    }

    /// Puts text held for the session's chat box (`hold_for_session_chat_composer`) after what the
    /// chat box holds, once it is ready; until then the text stays held.
    ///
    /// CDXC:Drafts 2026-10-25 WHY:
    /// Held text is appended, never swapped in for the draft: replacing it wiped every comment
    /// already in the chat box and the text the user had typed.
    pub(crate) fn deliver_held_session_chat_composer_text(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .pending_session_chat_composer_insert
            .contains_key(&session_id)
        {
            return;
        }
        let Some(view) = self
            .native_chat_views
            .get(&session_id)
            .filter(|view| view.read(cx).composer_ready)
            .cloned()
        else {
            return;
        };
        if let Some(content) = self
            .pending_session_chat_composer_insert
            .remove(&session_id)
        {
            view.update(cx, |view, cx| view.append_prompt(&content, cx));
        }
    }

    /// Keeps `content` for a chat box that cannot take it yet, after whatever already waits there.
    pub(crate) fn hold_for_session_chat_composer(
        &mut self,
        session_id: TerminalSessionId,
        content: &str,
    ) {
        hold_composer_text(
            &mut self.pending_session_chat_composer_insert,
            session_id,
            content,
        );
    }
}

/// `content` joined after the text already held under `key`.
fn hold_composer_text<K: std::hash::Hash + Eq>(
    held: &mut HashMap<K, String>,
    key: K,
    content: &str,
) {
    let waiting = held.entry(key).or_default();
    *waiting = ghostex_gx_chat_core::composer::review_comments::append_to_draft(waiting, content);
}

#[cfg(test)]
mod held_composer_text_tests {
    use super::hold_composer_text;
    use std::collections::HashMap;

    #[test]
    fn every_held_text_joins_the_one_before() {
        let mut held = HashMap::new();
        hold_composer_text(&mut held, 1, "[README.md L7](ghostex-comment:a)");
        hold_composer_text(&mut held, 1, "[greet.py L18](ghostex-comment:b)");
        hold_composer_text(&mut held, 2, "other chat");
        hold_composer_text(&mut held, 1, "a note");
        assert_eq!(
            held[&1],
            "[README.md L7](ghostex-comment:a) [greet.py L18](ghostex-comment:b)\n\na note"
        );
        assert_eq!(held[&2], "other chat");
    }
}

/*
CDXC:Drafts 2026-09-02:
A session that opens straight in Chat asks the daemon for the first-input
draft it was created with (Handoff / Export stages the transcript mention this
way) BEFORE that draft is typed into the terminal Chat parks behind it. The
daemon flips the marker so the terminal typing then does nothing, and the text
lands in the chat composer through the same queued insert the terminal → chat
transfer uses, so it survives a composer that is not mounted yet. A daemon
predating the endpoint answers not-found and nothing happens, which is exactly
the previous behaviour.
*/
impl GhostexGpuiApp {
    pub(crate) fn request_session_chat_launch_draft(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let request = if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            let params = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            cx.background_executor().spawn(async move {
                gpui_gxserver_rpc_result(
                    "/api/claimSessionChatLaunchDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        } else {
            let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
                return;
            };
            let Some(target) = self.gpui_remote_gxserver_request_target(&key.remote_machine_id)
            else {
                return;
            };
            let params = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            cx.background_executor().spawn(async move {
                gpui_remote_gxserver_rpc_result(
                    &target,
                    "/api/claimSessionChatLaunchDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        };
        cx.spawn(async move |this, cx| {
            let Ok(result) = request.await else {
                return;
            };
            let content = result
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if content.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.deliver_session_chat_composer_insert(session_id, content, cx);
            });
        })
        .detach();
    }
}
