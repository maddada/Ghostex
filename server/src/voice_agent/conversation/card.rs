//! The confirm card a stop the voice agent was unsure about puts on screen (`control.rs`): taking
//! it down, and the user's Stop or Cancel on it.

use std::time::Instant;

use serde_json::json;

use super::super::{control, tools};
use super::{Conversation, Ended, Input};

impl Conversation {
    /// Takes the confirm card off screen.
    pub(super) fn close_confirm(&mut self, outcome: &str) {
        if let Some(open) = self.confirm.take() {
            self.to_client(control::confirm_done_frame(&open.id, outcome));
        }
    }

    /// A button on the confirm card: Stop runs the stop it asked about (its result goes to the
    /// model, which says it), Cancel tells the model nothing was stopped.
    pub(super) async fn answer_confirm(
        &mut self,
        id: String,
        session: Option<String>,
    ) -> Result<(), Ended> {
        let Some(open) = self.confirm.clone().filter(|open| open.id == id) else {
            return Ok(());
        };
        let Some(reference) = session else {
            self.close_confirm("cancelled");
            return self.add_user_item(control::card_cancel_message()).await;
        };
        let Some(title) = open.title_of(&reference).map(str::to_string) else {
            self.to_client(json!({"type": "error", "code": "badRequest", "message": "That session is not on the card.", "fatal": false}));
            return Ok(());
        };
        self.close_confirm("confirmed");
        self.last_talk = Instant::now();
        let call_id = format!("card-{id}");
        let label = format!("Stopping {title}");
        self.to_client(json!({"type": "tool", "phase": "started", "callId": call_id, "name": "stop_session", "label": label}));
        let (state, tx, memory) = (self.state.clone(), self.tx.clone(), self.memory.clone());
        tokio::spawn(async move {
            let outcome = tools::run(
                &state,
                &memory,
                "stop_session",
                &json!({"session": reference}),
            )
            .await;
            let _ = tx.send(Input::CardStopDone {
                call_id,
                title,
                label,
                outcome,
            });
        });
        Ok(())
    }

    /// Adds a message in the user's turn that the model reads but no caption shows.
    pub(super) async fn add_user_item(&mut self, text: String) -> Result<(), Ended> {
        self.provider_send(json!({
            "type": "conversation.item.create",
            "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]},
        }))
        .await
    }
}
