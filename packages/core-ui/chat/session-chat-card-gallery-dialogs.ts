import type { SessionChatTerminalDialog, SessionChatTerminalNotice } from '@/packages/shared/session-chat';

export function dialogNotice(dialog: SessionChatTerminalDialog): SessionChatTerminalNotice {
  return {
    kind: 'codexInputBlocked',
    severity: 'info',
    source: 'screen',
    detectedAt: '2026-09-06T12:00:00.000Z',
    title: dialog.title,
    detail: dialog.body,
    dialog,
    choices: dialog.rows.map((row, index) => ({
      index,
      label: [row.label, row.description].filter(Boolean).join(' '),
      selected: row.selected,
    })),
    screenTail: [
      dialog.title,
      dialog.body,
      ...dialog.rows.map((row) => `${row.number}. ${row.label}`),
      dialog.footer,
    ].join('\n'),
  };
}

function form(
  title: string,
  body: string,
  input: SessionChatTerminalDialog['input'],
  footer: string,
  actions: string[],
  inputValue = ''
) {
  return dialogNotice({ id: `gallery-${title}`, title, body, input, footer, actions, inputValue, rows: [] });
}

function picker(title: string, labels: string[], body = 'Choose an option to continue.') {
  return dialogNotice({
    id: `gallery-${title}`,
    title,
    body,
    input: null,
    inputValue: '',
    footer: 'enter to select · esc to cancel',
    actions: ['confirm', 'cancel'],
    rows: labels.map((label, index) => ({ number: index + 1, label, description: null, selected: index === 0 })),
  });
}

// Representative terminal contents exercise each rendering branch in the shared
// dialog card, including its title-specific submit labels and textarea variants.
export const DIALOG_EXAMPLES = [
  dialogNotice({
    id: 'gallery-usage-limit',
    title: 'Usage-limit continuation',
    body: '',
    input: null,
    inputValue: '',
    footer: 'Enter to confirm · Esc to cancel',
    actions: ['up', 'down', 'confirm', 'cancel'],
    rows: [
      'Stop and wait for limit to reset',
      'Wait here, then continue automatically shortly',
      'Switch to usage credits',
    ].map((label, index) => ({ number: index + 1, label, description: null, selected: index === 0 })),
  }),
  picker(
    'Implement this plan?',
    ['Yes, implement this plan', 'No, stay in Plan mode'],
    'Update the shared card styles, then review the gallery.'
  ),
  picker(
    'Trust folder "example"?',
    ['Trust and continue', 'Quit Codex'],
    '/workspace/example\n\nOnly continue if you trust this folder’s contents.'
  ),
  picker(
    'Update Codex to 0.154.0?',
    ['Update now', 'Skip for now', 'Skip until next version'],
    'This session runs Codex 0.153.4. Update now installs 0.154.0 with the official Codex installer. Codex quits to install it, so start it again in this session afterwards.'
  ),
  picker('Select model', ['Default model (recommended)', 'Fast model', 'Reasoning model']),
  picker('Select reasoning effort', ['Low', 'Medium (recommended)', 'High', 'Extra high']),
  picker('Choose an import source', ['Local file', 'Previous conversation']),
  picker('Choose what to import', ['Messages', 'Messages and settings']),
  ...[
    'Select Syntax Theme',
    'Select Pet',
    'Keymap',
    'Select a base branch',
    'Select a commit to review',
    'Auto-review Denials',
    'Apps',
    'Plugins',
    'Resume a previous session',
    'Fork a previous session',
  ].map((title) =>
    form(
      title,
      'Type to search\n\n› Current selection\n  Another available selection',
      'search',
      '↑/↓ to navigate · enter to select · esc to close',
      ['up', 'down', 'confirm', 'cancel']
    )
  ),
  ...['Name thread', 'Rename thread', 'Edit goal', 'Save conversation', 'Export filename', 'Save transcript'].map(
    (title) =>
      form(
        title,
        'Enter a value below.',
        'text',
        'enter to save · esc to cancel',
        ['confirm', 'cancel'],
        'Review the card gallery'
      )
  ),
  form('Add marketplace', 'Enter a repository or marketplace URL.', 'text', 'Enter to add · esc to cancel', [
    'confirm',
    'cancel',
  ]),
  form(
    'Add directory',
    'Enter the directory path.',
    'text',
    'Enter to add · esc to cancel',
    ['confirm', 'cancel'],
    '/workspace/example'
  ),
  form('Ready to code?', 'Review the plan or request changes below.', 'text', 'enter to submit · esc to cancel', [
    'confirm',
    'cancel',
  ]),
  form('Custom review instructions', 'What should the review focus on?', 'text', 'enter to submit · esc to cancel', [
    'confirm',
    'cancel',
  ]),
  form('Tell us more (bug)', 'Describe what happened.', 'text', 'enter to submit · esc to cancel', [
    'confirm',
    'cancel',
  ]),
  form(
    'Submit feedback / bug report',
    'Describe the issue and how to reproduce it.',
    'text',
    'enter to submit · esc to cancel',
    ['confirm', 'cancel']
  ),
  form('Remap Shortcut', 'Choose a replacement shortcut.', 'key', 'esc to cancel', ['cancel']),
  form(
    'Experimental features',
    '[x] Feature enabled\n[ ] Optional feature',
    null,
    '↑/↓ to move · space to toggle · enter to save · esc to close',
    ['up', 'down', 'toggle', 'confirm', 'cancel']
  ),
  form(
    'Status line',
    '[x] Model\n[x] Context remaining\n[ ] Project directory',
    null,
    '←/→ to move · tab to change field · space to select · enter to save · esc to close',
    ['up', 'down', 'left', 'right', 'tab', 'toggle', 'confirm', 'cancel']
  ),
  form(
    'Git diff',
    'diff --git a/example.ts b/example.ts\n-const size = 12;\n+const size = 14;',
    null,
    'pgup/pgdn to page · q to quit',
    ['up', 'down', 'pageUp', 'pageDown', 'home', 'end', 'cancel']
  ),
  form('Setup complete', 'Codex is ready to use.', null, 'Enter to continue · esc to close', ['confirm', 'cancel']),
  form('Default model', 'Use this model for new conversations.', null, 'enter to set as default · esc to close', [
    'confirm',
    'cancel',
  ]),
];

export const PICKER_EXAMPLES: SessionChatTerminalNotice[] = [
  [
    'resumePrompt',
    'Claude Code is asking how to resume this session',
    ['Resume from summary (recommended)', 'Resume full session as-is', 'Don’t ask me again'],
  ],
  [
    'switchConfirmPrompt',
    'Claude Code is asking to confirm the model switch',
    ['Yes, switch model', 'No, keep current model'],
  ],
  [
    'switchConfirmPrompt',
    'Claude Code is asking to confirm the effort switch',
    ['Yes, switch effort', 'No, keep current effort'],
  ],
  [
    'sessionPausedPrompt',
    'Claude Code paused this session on a safeguards flag',
    ['Switch model', 'Edit last message'],
  ],
  ['permissionPrompt', 'Claude Code is asking for permission to proceed', ['Yes', 'Yes, allow for this session', 'No']],
  [
    'usageLimit',
    'What do you want to do?',
    ['Stop and wait for limit to reset', 'Wait here, then continue automatically at 8:10am'],
  ],
  [
    'usageLimit',
    'Usage limit resets shortly',
    ['Stop and wait for limit to reset', 'Wait here, then continue automatically shortly'],
  ],
].map(([kind, title, labels]) => ({
  kind: kind as string,
  title: title as string,
  severity: 'warning',
  source: 'screen',
  detectedAt: '2026-09-06T12:00:00.000Z',
  detail: 'Claude Code accepts no input until this is answered. Pick an option to answer it here.',
  choices: (labels as string[]).map((label, index) => ({ index, label, selected: index === 0 })),
  screenTail: [title, ...(labels as string[])].join('\n'),
  actions: [{ id: 'switchToTerminal', label: 'Open terminal', kind: 'switchToTerminal' }],
}));
