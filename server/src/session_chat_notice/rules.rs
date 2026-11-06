use super::*;

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

pub(super) struct NoticeActionSpec {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) kind: SessionChatTerminalNoticeActionKind,
    pub(super) send: Option<&'static str>,
}

pub(super) const OPEN_TERMINAL: NoticeActionSpec = NoticeActionSpec {
    id: "switchToTerminal",
    label: "Terminal view",
    kind: SessionChatTerminalNoticeActionKind::SwitchToTerminal,
    send: None,
};

const RESTART_CODEX: NoticeActionSpec = NoticeActionSpec {
    id: "restartAgent",
    label: "Restart Codex",
    kind: SessionChatTerminalNoticeActionKind::RestartAgent,
    send: None,
};

const RESTART_ZCODE: NoticeActionSpec = NoticeActionSpec {
    id: "restartAgent",
    label: "Restart ZCode",
    kind: SessionChatTerminalNoticeActionKind::RestartAgent,
    send: None,
};

/// The detail of Claude's "You've hit your limit" card. When the matched line names the reset ("resets 4pm"), classify.rs
/// puts it in the sentence (`with_reset_time`).
pub(super) const CLAUDE_USAGE_LIMIT_DETAIL: &str = "Claude Code hit its usage limit. Switch account or change model to keep going; queued messages wait until then.";

pub(super) struct NoticeRule {
    pub(super) kind: &'static str,
    pub(super) severity: SessionChatTerminalNoticeSeverity,
    pub(super) title: &'static str,
    pub(super) detail: &'static str,
    /*
    CDXC:SessionChat 2026-08-21:
    Severity and "blocks input" are DIFFERENT axes and must not be collapsed.
    Severity says how alarming the card looks; this says whether a message
    delivered while the state is up actually reaches the model. A trust dialog
    is only a `Warning` — the user is one keypress from continuing — yet
    anything typed into it is eaten as the ANSWER to it, which is worse for an
    automated sender than a loud `Error` banner the composer still works
    behind.

    True ⇒ a message sent now does not reach the model: it is consumed by an
    on-screen dialog, swallowed by a CLI that cannot talk to its provider, or
    typed at a shell where the agent used to be. Anything that only makes a
    turn *fail loudly* (a transient stream error the CLI retries) stays false:
    holding there would stall the queue on a state that heals itself.
    */
    pub(super) blocks_input: bool,
    /// Ordered newest-wording-first; the first match wins.
    pub(super) signatures: &'static [NoticeSignature],
    pub(super) actions: &'static [NoticeActionSpec],
    /// Keep the matched line as the card's `details` (behind its Details toggle): limits carry their
    /// reset time and stream errors carry the server's own words.
    pub(super) quote_evidence: bool,
}

// --- codex ------------------------------------------------------------------

const CODEX_RULES: &[NoticeRule] = &[
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Codex is signed out",
        // Codex has no /login: /logout ends it, and a signed-out Codex asks to sign in when it starts.
        detail: "Send /logout, then start Codex again and sign in. Messages you send wait until then.",
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Your access token could not be refreshed"),
                    NoticePart::Gap(160),
                    NoticePart::Text("sign in again"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Login expired"),
                    NoticePart::Gap(40),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Finish signing in",
        detail: "Codex is waiting for you to sign in. Finish or cancel it in the terminal, then send your message.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Sign in with ChatGPT to use Codex")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Finish signing in via your browser")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Provide your own API key")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_TRUST_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Trust this folder?",
        detail: "Codex won't run anything here until you trust this folder.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Dialog,
            parts: &[NoticePart::Text(
                "Do you trust the contents of this directory?",
            )],
            corroborators: &["Yes, continue", "No, quit"],
        }],
        // Select Trust explicitly, then confirm: Codex's onboarding shortcut
        // now only highlights Yes and requires Enter to grant trust.
        actions: &[
            NoticeActionSpec {
                id: "trustDirectory",
                label: "Trust folder",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("\x1b[A\r"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Codex isn't running",
        detail: "Codex closed. Restart it to keep chatting.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[NoticePart::Text("Error: cannot launch detached daemon;")],
                corroborators: &[],
            },
            // "Update now" on Codex's update prompt installs the new version and exits to the shell.
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[NoticePart::Text("Please restart Codex.")],
                corroborators: &[],
            },
            // Ghostex's own restore script, when `codex resume` could not start the conversation.
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[NoticePart::Text("Unable to restore Codex session")],
                corroborators: &[],
            },
            // Codex 0.15x prints "To continue this session, run:" and the command on its own line.
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[
                    NoticePart::Text("To continue this session, run"),
                    NoticePart::Gap(3),
                    NoticePart::Text("codex resume"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[
                    NoticePart::Text("thread '"),
                    NoticePart::Gap(80),
                    NoticePart::Text("panicked at"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[NoticePart::Text(
                    "internal error; agent loop died unexpectedly",
                )],
                corroborators: &[],
            },
        ],
        actions: &[RESTART_CODEX, OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Usage limit reached",
        detail: "Codex hit a usage or credit limit. Switch account or wait for it to reset; queued messages wait until then.",
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("hit your usage limit")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("hit your spend cap")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Your workspace is out of credits")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Quota exceeded. Check your plan")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_STREAM_ERROR,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Codex lost its connection",
        detail: "A network or server error stopped the last request. Send again to retry.",
        // The composer still accepts input and
        // codex retries the transport itself, so a message sent now DOES reach
        // the model once the connection comes back. Holding here would stall a
        // queue on a state that heals without the user, which is the failure
        // mode of over-widening this predicate.
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Reconnecting... waiting for network")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Reconnecting..."),
                    NoticePart::Gap(1),
                    NoticePart::Digit,
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Error while reading the server response")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("exceeded retry limit, last status")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("unexpected status "),
                    NoticePart::Digit,
                    NoticePart::Digit,
                    NoticePart::Digit,
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Connection failed")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("We're currently experiencing high demand")],
                corroborators: &[],
            },
            // codex-rs/protocol/src/error.rs spells this one with a typographic apostrophe.
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("We\u{2019}re currently experiencing high demand")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Selected model is at capacity")],
                corroborators: &[],
            },
            // CDXC:AgentScreenDetection 2026-09-08 WHY:
            // Codex's protocol/src/error.rs classifies these transport failures as retryable, but they can remain on screen after its own retries finish.
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("stream disconnected before completion:")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("rate limit exceeded:")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("request timed out")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_UPDATE_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Info,
        title: "Codex has an update",
        detail: "Skip it for now or update in the terminal to keep chatting.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                // "Update available! 0.1…" and, since Codex 0.156, "Update available · 0.156.1 → …".
                parts: &[
                    NoticePart::Text("Update available"),
                    NoticePart::Gap(3),
                    NoticePart::Digit,
                ],
                // Only the blocking MODAL warrants a notice; the harmless
                // in-history box carries no skip choice.
                corroborators: &["Skip until next version"],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("This version will no longer be supported")],
                corroborators: &[],
            },
        ],
        actions: &[
            NoticeActionSpec {
                id: "skipUpdate",
                label: "Skip for now",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("2"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
];

// --- claude / openclaude ----------------------------------------------------

const CLAUDE_RULES: &[NoticeRule] = &[
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Claude Code is signed out",
        detail: "Its login expired. Sign in to continue; queued messages wait until then.",
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Not logged in"),
                    NoticePart::Gap(24),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Login expired"),
                    NoticePart::Gap(24),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("OAuth token revoked"),
                    NoticePart::Gap(24),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 401")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    // An account whose organization turned off Claude Code for subscriptions answers every
    // prompt with this line; only another account or an API key gets past it. It stays a
    // `loginExpired` kind so every chat keeps its Switch account button; the title is what the
    // account recovery pass recognises (`SessionChatTerminalNotice::account_disabled`).
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: CLAUDE_ACCOUNT_DISABLED_TITLE,
        detail: CLAUDE_ACCOUNT_DISABLED_DETAIL,
        blocks_input: false,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[NoticePart::Text(
                "Your organization has disabled Claude subscription access",
            )],
            corroborators: &[],
        }],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Finish signing in",
        detail: "Claude Code is waiting for you to sign in. Finish or cancel it in the terminal, then send your message.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Select login method:")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Claude account with subscription")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Paste code here if prompted")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text(
                    "Run in another terminal: security unlock-keychain",
                )],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_TRUST_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Trust this folder?",
        detail: "Claude Code won't do anything here until you trust this folder. Answer it in the terminal.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Do you trust the files in this folder?")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[
                    NoticePart::Text("hasn't been trusted yet"),
                    NoticePart::Gap(200),
                    NoticePart::Text("Trusting allows Claude to read and execute files"),
                ],
                corroborators: &[],
            },
        ],
        // Deliberately no sendKeys: the mid-session variant focuses CANCEL, so
        // a blind Enter would decline.
        actions: &[OPEN_TERMINAL],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_PERMISSIONS_WARNING,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Claude Code needs an answer",
        detail: "A settings dialog is open in the terminal. Answer it there to continue.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Managed settings require approval")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text(
                    "WARNING: Claude Code running in Bypass Permissions mode",
                )],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_STREAM_ERROR,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Claude's service had a problem",
        detail: "A connection or server error stopped the last request. It retries by itself when automatic continuation is on.",
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 500")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 502")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 503")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 504")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 529")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: Server error mid-response")],
                corroborators: &[],
            },
            // CDXC:AgentProviders 2026-10-15 WHY: Claude Code 2.1.296's own wordings for a dropped network; without them the screen showed no notice and the recovery pass never retried (accounts/api_errors.rs reads the transcript first now).
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("API Error: Can"),
                    NoticePart::Gap(3),
                    NoticePart::Text("t reach the API server"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: Connection refused")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: Connection lost mid-response")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: Connection closed mid-response")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: Response stalled")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: The response stopped arriving")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Unable to connect to API")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Request timed out")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Connection error.")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Claude Code stopped",
        detail: "It quit with an error. Start it again in the terminal before you send.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Exit,
            parts: &[
                NoticePart::Text("Sorry, Claude"),
                NoticePart::Gap(6),
                NoticePart::Text("encountered an error"),
            ],
            corroborators: &[],
        }],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    // Ghostex's own restore script, when neither the exact resume nor its fallback started Claude.
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Couldn't reopen this chat",
        detail: "Claude Code quit before the chat loaded. Ghostex tries again by itself, or try now.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Exit,
            parts: &[NoticePart::Text(
                "Unable to find restorable Claude session id",
            )],
            corroborators: &[],
        }],
        actions: &[
            NoticeActionSpec {
                id: "restartAgent",
                label: "Try again",
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_ERROR,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Claude Code hit an error",
        detail: "The last request failed. Check the details, then send again; queued messages wait until then.",
        blocks_input: false,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[
                NoticePart::Text("Sorry, Claude"),
                NoticePart::Gap(6),
                NoticePart::Text("encountered an error"),
            ],
            corroborators: &[],
        }],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Your limit has reset",
        detail: "Continue to pick up where Claude Code stopped.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[
                NoticePart::Text("Usage limit has reset"),
                NoticePart::Gap(24),
                NoticePart::Text("press enter to continue"),
            ],
            corroborators: &[],
        }],
        actions: &[
            NoticeActionSpec {
                id: "continueNow",
                label: "Continue",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("\r"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
    // CDXC:AgentProviders 2026-09-11 DECISION:
    // User: the "Usage limit reached · continuing automatically" screen must not block sending from chat. The CLI's own line says "esc or type to cancel", the input box is on screen, and there is nothing in the chat UI to "clear", so the old "Clear it in the terminal before sending" refusal was wrong.
    // The notice stays a warning card; a send cancels the CLI's wait and goes to the current account, and the wording says so. Queued prompts keep pausing on every usage-limit kind (`blocks_queued_delivery`).
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Usage limit reached",
        detail: "Claude Code continues by itself when the limit resets. Sending a message now cancels that wait.",
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Usage limit reached"),
                    NoticePart::Gap(60),
                    NoticePart::Text("continuing automatically"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Automatic continue "),
                    NoticePart::Gap(4),
                    NoticePart::Text("turned off"),
                ],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Usage limit reached",
        detail: CLAUDE_USAGE_LIMIT_DETAIL,
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("You've hit your"),
                    NoticePart::Gap(40),
                    NoticePart::Text("limit"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("You've reached your"),
                    NoticePart::Gap(40),
                    NoticePart::Text("limit"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("You're out of usage credits")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
];

/// CDXC:AgentScreenDetection 2026-09-07 DECISION:
/// User: show Cursor's workspace trust notice in chat so it can be accepted without switching to the terminal.
const CURSOR_RULES: &[NoticeRule] = &[
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_TRUST_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Trust this folder?",
        detail: "Cursor won't run anything here until you trust this folder.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Dialog,
            parts: &[NoticePart::Text("Workspace Trust Required")],
            corroborators: &[
                "Do you trust the contents of this directory?",
                "[a] Trust this workspace",
                "[q] Quit",
            ],
        }],
        actions: &[
            NoticeActionSpec {
                id: "trustDirectory",
                label: "Trust this workspace",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("a"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
    // A cancelled or failed sign-in exits to the shell (session_chat_cursor_login.rs reads the live sign-in screens).
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
        severity: SessionChatTerminalNoticeSeverity::Info,
        title: "Sign in to Cursor",
        detail: "Cursor isn't signed in on this computer. Sign in to start chatting; your browser opens Cursor's sign-in page.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Exit,
            parts: &[NoticePart::Text(
                "Authentication required to use Cursor Agent",
            )],
            corroborators: &[],
        }],
        actions: &[
            NoticeActionSpec {
                id: "restartAgent",
                label: "Sign in",
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
];

// --- zcode ------------------------------------------------------------------

const ZCODE_RULES: &[NoticeRule] = &[NoticeRule {
    kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
    severity: SessionChatTerminalNoticeSeverity::Error,
    title: "ZCode isn't running",
    detail: "ZCode closed. Restart it to keep chatting.",
    blocks_input: true,
    signatures: &[
        // The two shell-prompt forms, verified against a SIGTERM'd session:
        // "Error: ZCode runtime exited with status 143. Diagnostics: …" and
        // the resume hint Codex's exit rule also keys on.
        NoticeSignature {
            scope: NoticeScope::Exit,
            parts: &[NoticePart::Text("ZCode runtime exited with status")],
            corroborators: &[],
        },
        NoticeSignature {
            scope: NoticeScope::Exit,
            parts: &[
                NoticePart::Text("To continue this session, run"),
                NoticePart::Gap(3),
                NoticePart::Text("zcode --resume"),
            ],
            corroborators: &[],
        },
        // CDXC:AgentScreenDetection 2026-10-06 WHY: the Banner arm covers the
        // other death form, where ZCode's exit screen still owns the pane
        // ("The session has terminated. Press Enter to exit.") and no shell
        // prompt is on screen yet, so the Exit scope's prompt requirement can
        // never match there.
        NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[
                NoticePart::Text("The session has terminated"),
                NoticePart::Gap(40),
                NoticePart::Text("Press Enter to exit"),
            ],
            corroborators: &[],
        },
        // CDXC:AgentScreenDetection 2026-10-07 WHY: ZCode draws inline (no
        // alternate screen) and PowerShell does not clear below its prompt,
        // so on Windows the dead TUI's composer rule and status bar stay under
        // "PS C:\…>" and the Exit scope's last-line prompt check never holds
        // (seen with a killed runtime: "Error: ZCode runtime exited with
        // status 4294967295. Diagnostics: …"). The launcher line followed by
        // a PowerShell prompt is the same "the shell is back" evidence.
        NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[
                NoticePart::Text("ZCode runtime exited with status"),
                NoticePart::Gap(400),
                NoticePart::Text(" PS "),
                NoticePart::Gap(2),
                NoticePart::Text("\\"),
            ],
            corroborators: &[],
        },
    ],
    actions: &[RESTART_ZCODE, OPEN_TERMINAL],
    quote_evidence: true,
}];

pub(super) fn notice_rules(agent: SessionChatOptionAgent) -> &'static [NoticeRule] {
    match agent {
        SessionChatOptionAgent::Claude => CLAUDE_RULES,
        SessionChatOptionAgent::Codex => CODEX_RULES,
        SessionChatOptionAgent::Cursor => CURSOR_RULES,
        SessionChatOptionAgent::Zcode => ZCODE_RULES,
        // Grok, Hermes, Omp, Pi and Empryo have no phrase-catalog rules here. Hermes
        // and Pi have source-derived focused-component detectors after this
        // catalog; the other agents rely on measured composer readiness.
        SessionChatOptionAgent::Antigravity
        | SessionChatOptionAgent::Copilot
        | SessionChatOptionAgent::Dsh
        | SessionChatOptionAgent::Empryo
        | SessionChatOptionAgent::Grok
        | SessionChatOptionAgent::Hermes
        | SessionChatOptionAgent::Omp
        | SessionChatOptionAgent::Pi => &[],
    }
}

/// Every catalog, for the kind-level queries below. Adding an agent's rules
/// here is the only step needed to teach the predicate about it.
const ALL_NOTICE_RULES: &[&[NoticeRule]] = &[CODEX_RULES, CLAUDE_RULES, CURSOR_RULES, ZCODE_RULES];

/*
CDXC:SessionChat 2026-08-21:
Whether a message delivered right now would actually reach the model, DERIVED
from the catalog above rather than restated as a second list of kind strings
that would drift the first time a rule is added. Automated senders — the chat
prompt queue's scheduler is the first — must gate on this, never on severity:
a `Warning` trust dialog eats what it is sent, while an `Error` stream banner
does not.

This is the default for notices constructed without a catalog match.
Catalog matches carry their own rule's input policy instead: the same kind can
describe an advisory error or a screen waiting for sign-in or a keypress.
Automatic queue delivery uses `blocks_queued_delivery`, which also holds on
unresolved quota, authentication, and agent errors.

The two watchdog-only kinds have no catalog rule and are answered here:
  - `deliveryFailed` — the watchdog could not prove the LAST message arrived (or
    proved that something else was submitted in its place), so the terminal has
    already demonstrated it is not accepting sends.
  - `queuedInput` — the opposite: the CLI accepted the message and is holding
    it client-side. Nothing is lost, so failing a row for it would be a false
    alarm. The scheduler's own idle gate is what keeps it from piling on.
An unknown kind is not blocking: it can only come from a newer peer, and this
predicate runs on notices this daemon classified itself.
*/
pub fn session_chat_notice_kind_blocks_input(kind: &str) -> bool {
    match kind {
        SESSION_CHAT_NOTICE_DELIVERY_FAILED => true,
        SESSION_CHAT_NOTICE_QUEUED_INPUT => false,
        // The refusal proves the terminal DID deliver the message — the model
        // declined it. A follow-up prompt goes through fine.
        SESSION_CHAT_NOTICE_API_REFUSAL => false,
        SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED | SESSION_CHAT_NOTICE_CLAUDE_INPUT_BLOCKED => true,
        SESSION_CHAT_NOTICE_CURSOR_INPUT_BLOCKED => true,
        SESSION_CHAT_NOTICE_GROK_INPUT_BLOCKED => true,
        SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED => true,
        SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED => true,
        SESSION_CHAT_NOTICE_PI_INPUT_BLOCKED => true,
        SESSION_CHAT_NOTICE_EMPRYO_INPUT_BLOCKED => true,
        /*
        CDXC:SessionChat 2026-08-21: the resume-usage picker owns
        the input line, and unlike the dialogs in the catalog it does not merely
        swallow a message — its trailing Enter CONFIRMS a row. A send delivered
        into it silently compacts the conversation the user was continuing, so
        it blocks harder than anything else here. It has no catalog rule because
        its rows are read off the screen rather than declared.
        */
        SESSION_CHAT_NOTICE_RESUME_PROMPT => true,
        /*
        CDXC:SessionChat 2026-08-29: same shape as the resume
        picker — a numbered chooser owning the input line, where a digit both
        selects and commits — so a send delivered into it answers the model/
        effort switch instead of reaching the model.
        */
        SESSION_CHAT_NOTICE_SWITCH_CONFIRM_PROMPT => true,
        // CDXC:AgentScreenDetection 2026-08-29: same again — the paused
        // chooser owns the input line until a row is picked.
        SESSION_CHAT_NOTICE_SESSION_PAUSED_PROMPT => true,
        // CDXC:AgentScreenDetection 2026-09-04: the permission prompt owns
        // the input line too; a message typed into it is read as dialog keys
        // and its Enter confirms the highlighted row.
        SESSION_CHAT_NOTICE_PERMISSION_PROMPT => true,
        _ => ALL_NOTICE_RULES
            .iter()
            .flat_map(|rules| rules.iter())
            .any(|rule| rule.kind == kind && rule.blocks_input),
    }
}
