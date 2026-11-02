//! Agents asking the user for a secret through a private card in the chat (`ghostex secret
//! request`). The value never enters the agent's transcript or the model's context: the user types
//! it into the card, gxserver writes it to an owner-only file, and the agent only learns the file's
//! path, which its own commands read (`TOKEN="$(cat <file>)" tool …`).
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! Ghostex runs every agent in a terminal, so a CLI verb the agent runs in its own shell is the one channel every agent family shares (no MCP tool or provider-specific hook needed). The answer travels over its own RPC, never `answerSessionChatPrompt`, whose send ledger and diagnostics record the submitted text.
//! SEE-ALSO: packages/gx-chat-core/src/secret_requests.rs (the card's document and its answer RPC), apps/desktop/src/app/native_chat/secret_request.rs and apps/mobile/app/src/chat/native/cards/SecretRequestCard.tsx (the two cards, which keep the typed text to themselves), skills/ghostex-secrets/SKILL.md (tells agents when and how to ask).

mod api;
mod store;

pub(crate) use api::{
    handle_secret_request_http, insert_secret_requests, secret_requests_identity,
    sweep_saved_files, SECRET_REQUEST_ENDPOINTS,
};

#[cfg(test)]
mod tests;
