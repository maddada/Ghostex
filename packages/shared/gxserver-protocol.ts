/*
CDXC:ServerApi 2026-05-30-14:04:
The gxserver protocol is the shared contract for the daemon, future gx/ghostex CLI clients, macOS clients, and remote clients. JSON fields and endpoint path tokens stay camelCase; protocol mismatch is a hard failure that asks the user to update instead of falling back to compatibility behavior.

CDXC:ServerApi 2026-06-04-03:20:
`zmxName` is the canonical provider identity and must carry the full server-project-session id. Clients should treat shorter project/session or compact g-* names as legacy display/state data, not as the reconnect target for gxserver zmx sessions.

CDXC:CefRuntime 2026-06-13-02:24:
CLI commands that still require visible macOS UI, AppKit, CEF, or sidebar-local workspace state must enter through a typed gxserver command contract. gxserver owns auth, protocol checks, dispatch, timeouts, and the supported action list; the macOS app is only the renderer-side executor for behavior that cannot live in the daemon yet.

CDXC:ServerApi 2026-06-22-16:17:
Local starts now rely only on server and no longer keep the deleted gxserver/ TypeScript source tree. Keep the TypeScript protocol contract in packages/shared/ so native web builds and Rust daemon packaging consume an app-owned contract without reaching into gxserver/.
*/
export * from './gxserver-protocol-core';
export * from './gxserver-protocol-health';
export * from './gxserver-protocol-agents';
export * from './gxserver-protocol-prompts';
export * from './gxserver-protocol-projects';
export * from './gxserver-protocol-domain';
export * from './gxserver-protocol-sessions';
export * from './gxserver-protocol-presentation';
export * from './gxserver-protocol-session-runtime';
export * from './gxserver-protocol-events';
export * from './gxserver-protocol-cloud-boxes';

// Session Chat wire types live in ./session-chat (canonical) and are
// re-exported here so protocol consumers keep a single import surface.
export type {
  GxserverAnswerSessionChatPromptParams,
  GxserverAnswerSessionChatPromptResult,
  GxserverHandoffSessionChatDraftParams,
  GxserverHandoffSessionChatDraftResult,
  GxserverReplaceSessionChatDraftParams,
  GxserverReplaceSessionChatDraftResult,
  GxserverInterruptSessionChatParams,
  GxserverInterruptSessionChatResult,
  GxserverQueueSessionChatPromptParams,
  GxserverQueueSessionChatPromptResult,
  GxserverReadSessionChatParams,
  GxserverReadSessionChatQueueParams,
  GxserverReadSessionChatQueueResult,
  GxserverReadSessionChatResult,
  GxserverRemoveSessionChatQueuedPromptParams,
  GxserverReorderSessionChatQueueParams,
  GxserverSendSessionChatMessageParams,
  GxserverSendSessionChatMessageResult,
  GxserverSendSessionChatQueuedPromptParams,
  GxserverSendSessionChatQueuedPromptResult,
  GxserverSessionChatAppendedEvent,
  GxserverSessionChatEvent,
  GxserverSessionChatQueueResult,
  GxserverSessionChatRemoveQueuedPromptResult,
  GxserverSessionChatReplacedEvent,
  GxserverSessionChatSnapshotEvent,
  GxserverSessionChatStateEvent,
  GxserverSetSessionChatDraftParams,
  GxserverSetSessionChatDraftResult,
  GxserverSubscribeSessionChatMessage,
  GxserverUnsubscribeSessionChatMessage,
  GxserverUpdateSessionChatQueuedPromptParams,
  SessionChatDraft,
  SessionChatQueuedPrompt,
  SessionChatQueuedPromptState,
} from './session-chat';
