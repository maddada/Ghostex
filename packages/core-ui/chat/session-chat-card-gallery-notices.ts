import type { SessionChatTerminalNotice } from '@/packages/shared/session-chat';

// Snapshot of server/src/session_chat_notice.rs and the session_chat_*_blocking.rs
// detector copy. Terminal excerpts are illustrative; the production renderers are used unchanged.
export const DETECTED_NOTICE_EXAMPLES = [
  {
    group: 'Codex notices',
    notice: {
      kind: 'loginExpired',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex reported a sign-in error',
      detail:
        'Codex could not authenticate a previous request. Open the terminal and run /login to sign in again, or retry if you have already fixed it. Automatic queued delivery is paused while this error applies.',
      screenTail: 'Your access token could not be refreshed',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex notices',
    notice: {
      kind: 'loginExpired',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for sign-in',
      detail: 'Complete or cancel the sign-in dialog in the terminal before sending a message.',
      screenTail: 'Sign in with ChatGPT to use Codex',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex notices',
    notice: {
      kind: 'trustPrompt',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for directory trust',
      detail:
        'Codex asks whether to trust this folder before it will run anything here. Nothing you send reaches the agent until it is answered.',
      screenTail: 'Codex is waiting for directory trust',
      actions: [
        {
          id: 'trustDirectory',
          label: 'Trust and continue',
          kind: 'sendKeys',
          send: '\u001b[A\r',
        },
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex notices',
    notice: {
      kind: 'agentExited',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is no longer running in this terminal',
      detail:
        'The codex process appears to have exited in this terminal. Messages sent from chat cannot reach it until it is started again.',
      screenTail: "thread '",
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex notices',
    notice: {
      kind: 'usageLimit',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex reported a usage limit',
      detail:
        'Codex reported a usage, spending, or credit limit on a previous attempt. Check the limit details in the terminal. You can retry after addressing it; automatic queued delivery is paused while this warning applies.',
      screenTail: 'hit your usage limit',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex notices',
    notice: {
      kind: 'streamError',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex hit a network or server error',
      detail: 'Codex reported a transport failure on screen. The turn may need to be retried.',
      screenTail: 'Reconnecting... waiting for network',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex notices',
    notice: {
      kind: 'updatePrompt',
      severity: 'info',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is showing an update prompt',
      detail: 'An update dialog is on screen. It blocks the composer until it is answered.',
      screenTail: 'Update available!',
      actions: [
        {
          id: 'skipUpdate',
          label: 'Skip for now',
          kind: 'sendKeys',
          send: '2',
        },
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'loginExpired',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code reported a sign-in error',
      detail:
        'Claude Code could not authenticate a previous request. Open the terminal and run /login, or correct the credentials for your configured provider. You can retry if you have already fixed it; automatic queued delivery is paused while this error applies.',
      screenTail: 'Not logged in',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'loginExpired',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is waiting for sign-in',
      detail:
        'Complete or cancel the sign-in flow in the terminal before sending a message. If macOS asks you to unlock the keychain, finish that step there.',
      screenTail: 'Select login method:',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'trustPrompt',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is waiting for folder trust',
      detail:
        'Claude Code is asking whether to trust this workspace. Nothing you send reaches the agent until it is answered.',
      screenTail: 'Accessing workspace:',
      actions: [
        {
          id: 'trustDirectory',
          label: 'Trust and continue',
          kind: 'sendKeys',
        },
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'trustPrompt',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is waiting for folder trust',
      detail:
        'Claude Code is showing its workspace-trust dialog and accepts nothing until it is answered. Which option is focused differs between versions, so answer it in the terminal rather than blind-pressing Enter.',
      screenTail: 'Do you trust the files in this folder?',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'permissionsWarning',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is waiting on a permissions dialog',
      detail:
        'Claude Code is showing a settings/permissions dialog that blocks its composer. Answer it in the terminal.',
      screenTail: 'Managed settings require approval',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'streamError',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code hit a temporary service error',
      detail:
        'The request failed because of a connection or server error. Automatic continuation will retry when enabled.',
      screenTail: 'API Error: 500',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'agentExited',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code stopped with an error',
      detail:
        'Claude Code reported an error and this terminal is back at a shell prompt. Restart or resume Claude Code in the terminal before sending a message.',
      screenTail: 'Sorry, Claude',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'agentError',
      severity: 'error',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code reported an error',
      detail:
        'Claude Code reported an error on a previous attempt. Check the terminal details below and retry when ready. Automatic queued delivery is paused while this error applies.',
      screenTail: 'Sorry, Claude',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'usageLimit',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is waiting to continue',
      detail: 'The usage limit has reset and Claude Code is waiting for a keypress before it resumes.',
      screenTail: 'Usage limit has reset',
      actions: [
        {
          id: 'continueNow',
          label: 'Continue now',
          kind: 'sendKeys',
          send: '\r',
        },
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'usageLimit',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is waiting on a usage limit',
      detail:
        'Claude Code hit a usage limit and will continue on its own when the limit resets. Sending a message now cancels that wait and sends it on the current account.',
      screenTail: 'Usage limit reached',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'usageLimit',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code reported a usage limit',
      detail:
        'Claude Code reported a usage limit on a previous attempt. You can send again or change models; automatic queued delivery is paused while this warning applies.',
      screenTail: "You've hit your",
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Claude notices',
    notice: {
      kind: 'onboarding',
      severity: 'info',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Claude Code is in first-run setup',
      detail:
        'Claude Code is showing a first-run setup screen, which blocks its composer until it is finished in the terminal.',
      screenTail: 'Claude Code is in first-run setup',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for an MCP response',
      detail: 'An MCP server form is waiting for required fields or an approval decision in the terminal.',
      screenTail: 'Codex is waiting for an MCP response',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting to continue',
      detail: 'A Codex setup or migration screen is waiting for confirmation in the terminal.',
      screenTail: 'Codex is waiting to continue',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for input in a terminal dialog',
      detail:
        'A Codex picker or text prompt has replaced the chat composer. Complete or close it in the terminal before sending another message.',
      screenTail: 'Codex is waiting for input in a terminal dialog',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for a choice',
      detail:
        'A Codex menu has replaced the input box. Make or cancel the selection in the terminal before sending another message.',
      screenTail: 'Codex is waiting for a choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex stopped this chat as a precaution',
      detail:
        'Codex will not accept another message in this chat. Choose New chat or Resume another chat in the terminal.',
      screenTail: 'Codex stopped this chat as a precaution',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting to unarchive this conversation',
      detail: 'Choose whether to unarchive the conversation or cancel before sending another message.',
      screenTail: 'Codex is waiting to unarchive this conversation',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for approval',
      detail:
        'A command, edit, permission, terminal-input, network, or MCP action is waiting for your decision in the terminal.',
      screenTail: 'Codex is waiting for approval',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for an app action',
      detail:
        'Finish the requested app setup, sign-in, or external action, then choose how to continue in the terminal.',
      screenTail: 'Codex is waiting for an app action',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex setup is waiting for input',
      detail: 'Finish or continue the Codex sign-in and first-run setup in the terminal before sending a message.',
      screenTail: 'Codex setup is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for hook review',
      detail: 'Review, trust, or skip the changed hooks in the terminal before Codex starts the session.',
      screenTail: 'Codex is waiting for hook review',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for Windows sandbox setup',
      detail: 'Choose a sandbox setup or safety option in the terminal before Codex can accept input.',
      screenTail: 'Codex is waiting for Windows sandbox setup',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for Amazon Bedrock setup',
      detail: 'Choose the AWS authentication method or profile in the terminal before continuing.',
      screenTail: 'Codex is waiting for Amazon Bedrock setup',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Codex terminal blockers',
    notice: {
      kind: 'codexInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Codex is waiting for a migration choice',
      detail: 'Finish the model or external-agent configuration migration choice in the terminal before continuing.',
      screenTail: 'Codex is waiting for a migration choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Cursor terminal blockers',
    notice: {
      kind: 'cursorInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Cursor is waiting for a model selection',
      detail:
        "Cursor's model picker owns the input field. Select a model or close the picker in the terminal before sending another message.",
      screenTail: 'Cursor is waiting for a model selection',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Cursor terminal blockers',
    notice: {
      kind: 'cursorInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Cursor is waiting for model parameters',
      detail:
        "Cursor's context, reasoning effort, and Fast settings own the input field. Finish or close the parameter editor in the terminal before sending another message.",
      screenTail: 'Cursor is waiting for model parameters',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build has an active terminal modal',
      detail: 'Finish or close the focused Grok Build modal or viewer in the terminal before sending a message.',
      screenTail: 'Grok Build has an active terminal modal',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting at its start menu',
      detail: 'Start or resume a Grok Build session in the terminal before sending a message.',
      screenTail: 'Grok Build is waiting at its start menu',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for sign-in',
      detail: 'Choose the account action and finish signing in from the terminal before sending a message.',
      screenTail: 'Grok Build is waiting for sign-in',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for account consent',
      detail: 'Read and accept the account notice, or quit, before sending a message.',
      screenTail: 'Grok Build is waiting for account consent',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for authentication',
      detail: 'Finish the browser or pasted-token authentication step before sending a message.',
      screenTail: 'Grok Build is waiting for authentication',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for terminal interaction',
      detail:
        'Submit, confirm, cancel, or dismiss the focused Grok Build editor, picker, or fullscreen surface before sending a message.',
      screenTail: 'Grok Build is waiting for terminal interaction',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build cannot show its account consent notice',
      detail: 'Enlarge the terminal to read and accept the account notice, or quit, before sending a message.',
      screenTail: 'Grok Build cannot show its account consent notice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for folder trust',
      detail: 'Accept or decline the workspace trust question in the terminal before sending a message.',
      screenTail: 'Grok Build is waiting for folder trust',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for authentication',
      detail: 'Finish the browser, device-code, or pasted-token authentication step before sending a message.',
      screenTail: 'Grok Build is waiting for authentication',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for a worktree name',
      detail: 'Create or cancel the new worktree dialog in the terminal before sending a message.',
      screenTail: 'Grok Build is waiting for a worktree name',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting on running subagents',
      detail: 'Choose whether Grok Build should stop or keep its subagents running before sending a message.',
      screenTail: 'Grok Build is waiting on running subagents',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for a turn choice',
      detail: 'Complete or dismiss the rewind or jump chooser in the terminal before sending a message.',
      screenTail: 'Grok Build is waiting for a turn choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting to rewind',
      detail: 'Choose whether to cancel the running turn before rewinding, or dismiss the prompt.',
      screenTail: 'Grok Build is waiting to rewind',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for MCP input',
      detail: 'Complete, accept, decline, or cancel the MCP request in the terminal before sending a message.',
      screenTail: 'Grok Build is waiting for MCP input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for plan approval',
      detail: 'Approve the plan or send revision feedback from the terminal before sending a normal message.',
      screenTail: 'Grok Build is waiting for plan approval',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for permission',
      detail: 'Approve, reject, edit, or cancel the pending tool permission in the terminal before sending a message.',
      screenTail: 'Grok Build is waiting for permission',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is waiting for an answer',
      detail: "Answer or dismiss Grok Build's question card in the terminal before sending a message.",
      screenTail: 'Grok Build is waiting for an answer',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is in shell-command mode',
      detail: 'Exit or finish the shell-command editor in the terminal before sending a normal message.',
      screenTail: 'Grok Build is in shell-command mode',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is editing a memory note',
      detail: 'Save or cancel the memory-note editor in the terminal before sending a normal message.',
      screenTail: 'Grok Build is editing a memory note',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Grok terminal blockers',
    notice: {
      kind: 'grokInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Grok Build is editing text outside the composer',
      detail: 'Save or cancel the queued prompt or comment editor in the terminal before sending a normal message.',
      screenTail: 'Grok Build is editing text outside the composer',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for a protected value',
      detail: 'Enter or skip the requested password or secret in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for a protected value',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for approval',
      detail: 'Approve, deny, or change the command or tool decision in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for approval',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for authentication',
      detail: 'Complete the API-key, browser, or device-code sign-in flow in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for authentication',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes setup is waiting for input',
      detail: 'Finish or exit Hermes first-run and provider setup in the terminal before sending a message.',
      screenTail: 'Hermes setup is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes needs your input',
      detail: "Answer or cancel Hermes's question in the terminal before sending another message.",
      screenTail: 'Hermes needs your input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for confirmation',
      detail: 'Confirm or cancel the pending Hermes action in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for confirmation',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes billing is waiting for input',
      detail: 'Finish the billing, subscription, or browser-approval step in the terminal before sending a message.',
      screenTail: 'Hermes billing is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes billing is waiting for input',
      detail: 'Finish the billing or subscription menu in the terminal before sending a message.',
      screenTail: 'Hermes billing is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for a model choice',
      detail:
        'Finish or close the provider, model, credential, or disconnect picker in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for a model choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for the model picker',
      detail: 'Wait for the model picker, then finish or close it in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for the model picker',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting in the command palette',
      detail: 'Insert a command or close the palette in the terminal before sending a message.',
      screenTail: 'Hermes is waiting in the command palette',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for a session choice',
      detail: 'Select, create, close, resume, or dismiss the Hermes session picker in the terminal.',
      screenTail: 'Hermes is waiting for a session choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for the session picker',
      detail: 'Wait for the session picker, then finish or close it in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for the session picker',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for a pet choice',
      detail: 'Choose a pet or close the picker in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for a pet choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for the pet picker',
      detail: 'Wait for the pet picker, then choose a pet or close it in the terminal.',
      screenTail: 'Hermes is waiting for the pet picker',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting in the Skills Hub',
      detail: 'Finish or close the skill browser in the terminal before sending a message.',
      screenTail: 'Hermes is waiting in the Skills Hub',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for the Skills Hub',
      detail: 'Wait for the skill browser, then finish or close it in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for the Skills Hub',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting in the Plugins Hub',
      detail: 'Finish or close the plugin browser in the terminal before sending a message.',
      screenTail: 'Hermes is waiting in the Plugins Hub',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting for the Plugins Hub',
      detail: 'Wait for the plugin browser, then finish or close it in the terminal before sending a message.',
      screenTail: 'Hermes is waiting for the Plugins Hub',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is showing the agents view',
      detail: 'Close the full-screen agents view in the terminal before sending a message.',
      screenTail: 'Hermes is showing the agents view',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is showing the Journey view',
      detail: 'Close the full-screen Journey view in the terminal before sending a message.',
      screenTail: 'Hermes is showing the Journey view',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is waiting in a picker',
      detail: 'Close the failed or empty Hermes picker in the terminal before sending a message.',
      screenTail: 'Hermes is waiting in a picker',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Hermes terminal blockers',
    notice: {
      kind: 'hermesInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Hermes is showing a modal view',
      detail: 'Read, act on, or close the Hermes pager or modal in the terminal before sending a message.',
      screenTail: 'Hermes is showing a modal view',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for tool approval',
      detail: 'Approve or deny the pending tool call in the terminal before sending a message.',
      screenTail: 'OMP is waiting for tool approval',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for authentication',
      detail:
        'Finish the provider or account choice, browser sign-in, authorization-code, or credential step in the terminal.',
      screenTail: 'OMP is waiting for authentication',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for an answer',
      detail: "Answer or cancel OMP's question dialog in the terminal before sending a message.",
      screenTail: 'OMP is waiting for an answer',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for a protected value',
      detail:
        'Enter or cancel the requested secret, token, API key, or authorization code in the terminal before sending a message.',
      screenTail: 'OMP is waiting for a protected value',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for text input',
      detail:
        "A text field or editor has replaced OMP's ordinary composer. Submit or cancel it in the terminal before sending a message.",
      screenTail: 'OMP is waiting for text input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for a choice',
      detail:
        'A selector or confirmation dialog owns terminal input. Make or cancel the choice before sending a message.',
      screenTail: 'OMP is waiting for a choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP has an active terminal panel',
      detail: 'Finish, cancel, resume, or dismiss the focused OMP panel in the terminal before sending a message.',
      screenTail: 'OMP has an active terminal panel',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for terminal interaction',
      detail: "Close or finish OMP's focused menu, picker, configuration screen, or modal before sending a message.",
      screenTail: 'OMP is waiting for terminal interaction',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP live mode owns terminal input',
      detail: 'End the realtime voice session in the terminal before sending a normal text message.',
      screenTail: 'OMP live mode owns terminal input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP has a cancellable operation open',
      detail: 'Wait for or cancel the focused terminal operation before sending a message.',
      screenTail: 'OMP has a cancellable operation open',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP setup is waiting for input',
      detail: 'Complete, skip, or exit the current OMP setup step in the terminal before sending a message.',
      screenTail: 'OMP setup is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is paused',
      detail: 'Resume the paused OMP session in the terminal before sending a message.',
      screenTail: 'OMP is paused',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting to move the session',
      detail: 'Accept or decline the missing-directory re-root prompt in the terminal before sending a message.',
      screenTail: 'OMP is waiting to move the session',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for an authentication choice',
      detail: 'Choose the provider or account in the terminal before sending a message.',
      screenTail: 'OMP is waiting for an authentication choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Omp terminal blockers',
    notice: {
      kind: 'ompInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'OMP is waiting for an authorization code',
      detail: 'Paste or cancel the provider authorization response in the terminal before sending a message.',
      screenTail: 'OMP is waiting for an authorization code',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for project trust',
      detail:
        'Choose how Pi may use this project and whether to remember that decision in the terminal before sending a message.',
      screenTail: 'Pi is waiting for project trust',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi setup is waiting for input',
      detail: "Finish or skip Pi's first-run theme and analytics setup in the terminal before sending a message.",
      screenTail: 'Pi setup is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for authentication',
      detail:
        'Complete the provider choice, browser or device-code sign-in, credential prompt, or logout flow in the terminal.',
      screenTail: 'Pi is waiting for authentication',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for confirmation',
      detail: 'Confirm or cancel the session deletion in the terminal before sending a message.',
      screenTail: 'Pi is waiting for confirmation',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for a session choice',
      detail: 'Select, rename, delete, or cancel the Pi session picker in the terminal before sending a message.',
      screenTail: 'Pi is waiting for a session choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi configuration is waiting for input',
      detail: "Finish or close Pi's resource configuration screen in the terminal before sending a message.",
      screenTail: 'Pi configuration is waiting for input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for a setup dialog to close',
      detail: 'Review and close the provider setup information in the terminal before sending a message.',
      screenTail: 'Pi is waiting for a setup dialog to close',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for text input',
      detail:
        'A Pi text prompt or editor has replaced the ordinary composer. Submit or cancel it in the terminal before sending a message.',
      screenTail: 'Pi is waiting for text input',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
  {
    group: 'Pi terminal blockers',
    notice: {
      kind: 'piInputBlocked',
      severity: 'warning',
      source: 'screen',
      detectedAt: '2026-09-06T12:00:00.000Z',
      title: 'Pi is waiting for a choice',
      detail:
        'A Pi menu has replaced the prompt editor. Make or cancel the selection in the terminal before sending a message.',
      screenTail: 'Pi is waiting for a choice',
      actions: [
        {
          id: 'switchToTerminal',
          label: 'Open terminal',
          kind: 'switchToTerminal',
        },
      ],
    },
  },
] satisfies { group: string; notice: SessionChatTerminalNotice }[];
