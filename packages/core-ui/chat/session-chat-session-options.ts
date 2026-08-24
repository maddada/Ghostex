// Per-agent session-option catalogs for the composer footer pills
// (upstream chat spec §1.2-§1.4 port).
//
// The agent is a TUI: there is no API to set a model or a reasoning effort,
// only keystrokes. So every option here is DELIVERED as a slash command (or a
// raw key) typed into the running agent. A displayed value is either pending
// local intent (`dispatched`) or agent-owned evidence (`detected`) read from
// the transcript/statusline by gxserver. No catalog value is presented as the
// current session truth without evidence.
//
// Agents without a catalog (unknown ids) get no pills at all.

import type { SessionChatSendKey } from '../../shared/session-chat';

export type SessionChatOptionCategory = 'model' | 'thought_level' | 'model_config' | 'mode';

/** Options-pill ordering (§1.2); the model category has its own pill. */
const CATEGORY_ORDER: Record<SessionChatOptionCategory, number> = {
  model: -1,
  thought_level: 0,
  model_config: 1,
  mode: 2,
};

export interface SessionChatOptionChoice {
  value: string;
  label: string;
  description?: string;
}

export type SessionChatOptionDispatch =
  /** Types `build(value)` into the TUI; the chosen value becomes the local truth. */
  | { kind: 'command'; build: (value: string) => string }
  /** Types a fixed command that FLIPS an unknown baseline (no value tracked). */
  | { kind: 'toggle-command'; command: string }
  /** Types a command that opens the agent's own picker, then shows the terminal. */
  | { kind: 'agent-picker'; command: string }
  /**
   * Nothing is typed: the pill shows the value gxserver read from the agent's
   * statusline and hands the user to the terminal to change it. For a TUI whose
   * picker cannot be driven blind from here (grok), a read-only pill plus a
   * handoff is the honest control — see GROK_CATALOG.
   */
  | { kind: 'terminal-handoff' }
  /** Steps through a bounded TUI setting using shifted arrow keys. */
  | {
      kind: 'bounded-key-steps';
      decreaseKey: SessionChatSendKey;
      increaseKey: SessionChatSendKey;
    }
  /** Writes a raw keystroke sequence (no text, no Enter). */
  | { kind: 'key'; key: SessionChatSendKey; marker: string };

export interface SessionChatOptionDescriptor {
  /** Stable per agent; also the persistence key. */
  id: string;
  /** Category name, e.g. "Effort" — shown in the tooltip, not in the pill. */
  label: string;
  category: SessionChatOptionCategory;
  dispatch: SessionChatOptionDispatch;
  /** Present for value-carrying (select) options only. */
  choices?: readonly SessionChatOptionChoice[];
  defaultValue?: string;
  /** Row label for toggle / agent-picker / key rows. */
  actionLabel?: string;
  /** Muted line under the menu heading. */
  description?: string;
}

export interface SessionChatSessionOptionCatalog {
  /** The model pill's descriptor (category "model"). */
  model: SessionChatOptionDescriptor;
  /** Everything else, in category order, for the current model. */
  optionsForModel: (modelValue: string) => readonly SessionChatOptionDescriptor[];
}

// ---------------------------------------------------------------------------
// Claude / OpenClaude
// ---------------------------------------------------------------------------

const CLAUDE_MODELS: readonly SessionChatOptionChoice[] = [
  { value: 'fable', label: 'Fable 5' },
  { value: 'opus', label: 'Opus 5' },
  { value: 'sonnet', label: 'Sonnet 5' },
  { value: 'haiku', label: 'Haiku 4.5' },
];

const CLAUDE_EFFORTS: readonly SessionChatOptionChoice[] = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'xhigh', label: 'Extra high' },
  { value: 'max', label: 'Max' },
  { value: 'auto', label: 'Auto' },
];

const CLAUDE_MODEL: SessionChatOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  choices: CLAUDE_MODELS,
  dispatch: { kind: 'command', build: (value) => `/model ${value}` },
};

const CLAUDE_EFFORT: SessionChatOptionDescriptor = {
  id: 'effort',
  label: 'Effort',
  category: 'thought_level',
  choices: CLAUDE_EFFORTS,
  dispatch: { kind: 'command', build: (value) => `/effort ${value}` },
};

const CLAUDE_FAST_MODE: SessionChatOptionDescriptor = {
  id: 'fastMode',
  label: 'Fast mode',
  category: 'model_config',
  actionLabel: 'Toggle Fast mode',
  description: 'Flips fast mode; the current state is only known to the agent.',
  dispatch: { kind: 'toggle-command', command: '/fast' },
};

/*
Permission mode is Shift+Tab in Claude Code's TUI — it has no slash command,
so it is delivered as a raw keystroke through sendSessionChatMessage's `key`
param. Cycling is blind (the TUI owns the order), which is exactly the
"toggle-command" shape: an action row, no tracked value.
*/
const CLAUDE_MODE: SessionChatOptionDescriptor = {
  id: 'mode',
  label: 'Mode',
  category: 'mode',
  actionLabel: 'Cycle mode (Shift+Tab)',
  description: "Steps through Claude Code's permission modes.",
  dispatch: { kind: 'key', key: 'shift-tab', marker: 'Sent Shift+Tab (mode cycle)' },
};

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

const CODEX_MODELS: readonly SessionChatOptionChoice[] = [
  { value: 'gpt-5.6-sol', label: 'GPT-5.6 Sol' },
  { value: 'gpt-5.6-terra', label: 'GPT-5.6 Terra' },
  { value: 'gpt-5.6-luna', label: 'GPT-5.6 Luna' },
  { value: 'gpt-5.5', label: 'GPT-5.5' },
  { value: 'gpt-5.2-codex', label: 'GPT-5.2 Codex' },
];

const CODEX_EFFORTS: readonly SessionChatOptionChoice[] = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'xhigh', label: 'Extra high' },
];

export function codexEffortChoices(_modelValue: string): readonly SessionChatOptionChoice[] {
  return CODEX_EFFORTS;
}

/* Codex owns model selection in its interactive terminal picker. */
const CODEX_MODEL: SessionChatOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  choices: CODEX_MODELS,
  actionLabel: "Open the CLI's model picker",
  dispatch: { kind: 'agent-picker', command: '/model' },
};

const CODEX_EFFORT: SessionChatOptionDescriptor = {
  id: 'effort',
  label: 'Reasoning effort',
  category: 'thought_level',
  dispatch: {
    kind: 'bounded-key-steps',
    decreaseKey: 'shift-down',
    increaseKey: 'shift-up',
  },
};

const CODEX_MODE: SessionChatOptionDescriptor = {
  id: 'mode',
  label: 'Mode',
  category: 'mode',
  actionLabel: 'Switch to Plan mode',
  dispatch: { kind: 'command', build: () => '/plan' },
};

// ---------------------------------------------------------------------------
// Grok
// ---------------------------------------------------------------------------

/*
Grok prints `Grok 4.6 (medium)` in its composer footer and changes both values
through one interactive `/model` picker, which also owns the effort list per
model. There is no command that sets either value directly, and blind keystrokes
into that picker would be guesswork against a menu this side cannot see. So both
pills are read-only mirrors of the statusline gxserver already reads, and either
one hands the user to the terminal to make the change.
*/
const GROK_EFFORTS: readonly SessionChatOptionChoice[] = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'xhigh', label: 'Extra high' },
];

/** No `choices`: grok's model list is account-dependent and never typed here. */
const GROK_MODEL: SessionChatOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  actionLabel: 'Change it in the CLI',
  dispatch: { kind: 'terminal-handoff' },
};

const GROK_EFFORT: SessionChatOptionDescriptor = {
  id: 'effort',
  label: 'Reasoning effort',
  category: 'thought_level',
  choices: GROK_EFFORTS,
  actionLabel: 'Change it in the CLI',
  dispatch: { kind: 'terminal-handoff' },
};

// ---------------------------------------------------------------------------
// Catalog resolution
// ---------------------------------------------------------------------------

const CLAUDE_CATALOG: SessionChatSessionOptionCatalog = {
  model: CLAUDE_MODEL,
  optionsForModel: (modelValue) =>
    sortDescriptors([
      // Until gxserver confirms the model, do not offer effort controls that
      // may not exist for the actual model (Haiku has none).
      ...(modelValue === '' || modelValue === 'haiku' ? [] : [CLAUDE_EFFORT]),
      ...(modelValue === 'opus' ? [CLAUDE_FAST_MODE] : []),
      CLAUDE_MODE,
    ]),
};

const CODEX_CATALOG: SessionChatSessionOptionCatalog = {
  model: CODEX_MODEL,
  optionsForModel: (modelValue) =>
    sortDescriptors([{ ...CODEX_EFFORT, choices: codexEffortChoices(modelValue) }, CODEX_MODE]),
};

function sortDescriptors(descriptors: readonly SessionChatOptionDescriptor[]): readonly SessionChatOptionDescriptor[] {
  return [...descriptors].sort((left, right) => CATEGORY_ORDER[left.category] - CATEGORY_ORDER[right.category]);
}

const GROK_CATALOG: SessionChatSessionOptionCatalog = {
  model: GROK_MODEL,
  optionsForModel: () => [GROK_EFFORT],
};

const CATALOG_BY_AGENT: Record<string, SessionChatSessionOptionCatalog> = {
  claude: CLAUDE_CATALOG,
  openclaude: CLAUDE_CATALOG,
  codex: CODEX_CATALOG,
  grok: GROK_CATALOG,
};

export function sessionChatSessionOptionCatalog(
  agent: string | null | undefined
): SessionChatSessionOptionCatalog | null {
  if (agent === null || agent === undefined) {
    return null;
  }
  return CATALOG_BY_AGENT[agent] ?? null;
}

/**
 * Command names the option pills can type, so classifySessionChatSend renders
 * a dispatched pill command as the same muted "Ran /model sonnet" row a typed
 * command gets. Names only (no slash), matching the slash-command catalog.
 */
export function sessionChatOptionCommandNames(agent: string | null | undefined): readonly string[] {
  const catalog = sessionChatSessionOptionCatalog(agent);
  if (!catalog) {
    return [];
  }
  const names = new Set<string>();
  const collect = (descriptor: SessionChatOptionDescriptor): void => {
    const { dispatch } = descriptor;
    const command =
      dispatch.kind === 'command'
        ? dispatch.build(descriptor.choices?.[0]?.value ?? '')
        : dispatch.kind === 'toggle-command' || dispatch.kind === 'agent-picker'
          ? dispatch.command
          : null;
    if (command === null) {
      return;
    }
    const name = command.trim().split(/\s+/, 1)[0]?.replace(/^\//, '') ?? '';
    if (name !== '') {
      names.add(name);
    }
  };
  collect(catalog.model);
  // Union over every model, so a name only reachable under one model (Claude's
  // /fast) still classifies as a command.
  for (const choice of catalog.model.choices ?? [{ value: '', label: '' }]) {
    for (const descriptor of catalog.optionsForModel(choice.value)) {
      collect(descriptor);
    }
  }
  return [...names];
}

// ---------------------------------------------------------------------------
// Local value state
// ---------------------------------------------------------------------------

export type SessionChatOptionSource = 'default' | 'dispatched' | 'detected';

export interface SessionChatOptionValue {
  value: string;
  source: SessionChatOptionSource;
  /**
   * The raw text the agent reported (`Fable 5`, an unknown codex id). Only set
   * by a detection; preferred over the catalog label so the pill shows the
   * real model string instead of the catalog's guess.
   */
  label?: string;
  /** ISO time this surface typed the option command (source "dispatched"). */
  dispatchedAt?: string;
  /** Agent-owned evidence used for a detected value. */
  detectedSource?: 'terminal' | 'transcript';
  /** ISO time gxserver read the value (source "detected"). */
  detectedAt?: string;
}

/** Descriptor id → local value. Only value-carrying options appear. */
export type SessionChatOptionState = Readonly<Record<string, SessionChatOptionValue>>;

export const SESSION_CHAT_DISPATCHED_HINT = 'Sent to the agent — not confirmed';
export const SESSION_CHAT_DETECTED_HINT = 'Read from the terminal';
export const SESSION_CHAT_TRANSCRIPT_HINT = 'Confirmed by the agent transcript';

/**
 * How long a just-typed option command outranks a DISAGREEING detection: the
 * TUI needs a moment to repaint, and a probe that catches the old statusline
 * must not flip the pill back. A detection that AGREES confirms immediately,
 * and after the window a disagreement wins (the agent did something else).
 */
export const SESSION_CHAT_DISPATCH_GRACE_MS = 10_000;

function isTrackedValue(descriptor: SessionChatOptionDescriptor, value: string): boolean {
  return (descriptor.choices ?? []).some((choice) => choice.value === value);
}

/** Value-carrying descriptors: a select the pills can label from. */
export function sessionChatOptionTracksValue(descriptor: SessionChatOptionDescriptor): boolean {
  return (
    (descriptor.dispatch.kind === 'command' || descriptor.dispatch.kind === 'bounded-key-steps') &&
    descriptor.choices !== undefined &&
    descriptor.choices.length > 0
  );
}

/**
 * Exact key sequence for a bounded ordered setting. With a known current
 * value, send only the delta. Without one, first saturate at the nearer edge
 * and then step inward, so the requested value is deterministic.
 */
export function sessionChatBoundedKeySteps(
  choices: readonly SessionChatOptionChoice[],
  currentValue: string | undefined,
  targetValue: string,
  decreaseKey: SessionChatSendKey,
  increaseKey: SessionChatSendKey
): SessionChatSendKey[] {
  const targetIndex = choices.findIndex((choice) => choice.value === targetValue);
  if (targetIndex < 0 || choices.length < 2) {
    return [];
  }
  const currentIndex = choices.findIndex((choice) => choice.value === currentValue);
  if (currentIndex >= 0) {
    const delta = targetIndex - currentIndex;
    return Array.from({ length: Math.abs(delta) }, () => (delta > 0 ? increaseKey : decreaseKey));
  }
  const lastIndex = choices.length - 1;
  const fromLowerEdge = lastIndex + targetIndex;
  const fromUpperEdge = lastIndex + (lastIndex - targetIndex);
  return fromLowerEdge <= fromUpperEdge
    ? [
        ...Array.from({ length: lastIndex }, () => decreaseKey),
        ...Array.from({ length: targetIndex }, () => increaseKey),
      ]
    : [
        ...Array.from({ length: lastIndex }, () => increaseKey),
        ...Array.from({ length: lastIndex - targetIndex }, () => decreaseKey),
      ];
}

export function seedSessionChatOptionState(
  catalog: SessionChatSessionOptionCatalog,
  stored: SessionChatOptionState = {}
): SessionChatOptionState {
  const next: Record<string, SessionChatOptionValue> = {};
  const seed = (descriptor: SessionChatOptionDescriptor): void => {
    if (next[descriptor.id]) {
      return;
    }
    const storedValue = stored[descriptor.id];
    if (!sessionChatOptionTracksValue(descriptor)) {
      return;
    }
    /*
    A persisted detection can be stale after the user changes the agent in the
    terminal while Chat is unmounted. gxserver will re-confirm it on the seed
    read; only still-pending local intent survives this synchronous reseed.
    */
    if (storedValue?.source === 'dispatched' && isTrackedValue(descriptor, storedValue.value)) {
      next[descriptor.id] = storedValue;
      return;
    }
    if (descriptor.defaultValue !== undefined) {
      next[descriptor.id] = { value: descriptor.defaultValue, source: 'default' };
    }
  };
  seed(catalog.model);
  const modelValue = next[catalog.model.id]?.value ?? catalog.model.defaultValue ?? '';
  for (const descriptor of catalog.optionsForModel(modelValue)) {
    seed(descriptor);
  }
  return next;
}

export function setSessionChatOptionValue(
  state: SessionChatOptionState,
  descriptorId: string,
  value: string,
  source: SessionChatOptionSource,
  now: () => number = Date.now
): SessionChatOptionState {
  const current = state[descriptorId];
  if (current?.value === value && current.source === source) {
    return state;
  }
  const next: SessionChatOptionValue = { value, source };
  if (source === 'dispatched') {
    // Stamped so a detection can tell "the user just sent this" from "the
    // agent has been running this for a while".
    next.dispatchedAt = new Date(now()).toISOString();
  }
  return { ...state, [descriptorId]: next };
}

/**
 * A command the USER typed reconciles the pills: `/model opus` makes the model
 * pill read Opus without a second dispatch. Exact match against the catalog's
 * own builders, so an unrelated `/model` argument is ignored.
 */
export function reconcileSessionChatOptionsFromCommand(
  catalog: SessionChatSessionOptionCatalog,
  state: SessionChatOptionState,
  text: string
): SessionChatOptionState {
  const normalized = text.trim().replace(/\s+/g, ' ');
  if (!normalized.startsWith('/')) {
    return state;
  }
  const modelValue = state[catalog.model.id]?.value ?? catalog.model.defaultValue ?? '';
  const descriptors = [catalog.model, ...catalog.optionsForModel(modelValue)];
  let next = state;
  for (const descriptor of descriptors) {
    if (descriptor.dispatch.kind !== 'command') {
      continue;
    }
    for (const choice of descriptor.choices ?? []) {
      if (descriptor.dispatch.build(choice.value) === normalized) {
        next = setSessionChatOptionValue(next, descriptor.id, choice.value, 'dispatched');
      }
    }
  }
  return next;
}

/*
CDXC:SessionChatDetectedOptions 2026-08-01:
gxserver reads the agent's structured transcript and terminal statusline and
reports what it is REALLY running
(`selectedOptions` on read results and snapshot/replaced/state frames). That
outranks this surface's local truth, with one exception: a value the user just
dispatched keeps the pill for a short grace window, because the TUI may not have
repainted yet. A detection that AGREES with a pending dispatch confirms it —
the tooltip names the transcript or terminal evidence that confirmed it.
Nothing detected ⇒ nothing here runs and no current value is claimed.
*/
export interface SessionChatDetectedOptionInput {
  model?: { value: string; label: string; source?: 'terminal' | 'transcript' };
  effort?: { value: string; label: string; source?: 'terminal' | 'transcript' };
  detectedAt: string;
}

function applyDetectedChoice(
  state: SessionChatOptionState,
  descriptorId: string,
  detected: {
    value: string;
    label: string;
    source?: 'terminal' | 'transcript';
  },
  detectedAt: string
): SessionChatOptionState {
  const current = state[descriptorId];
  const detectedAtMs = Date.parse(detectedAt);
  const dispatchedAtMs = current?.dispatchedAt ? Date.parse(current.dispatchedAt) : Number.NaN;
  const agrees = current?.value === detected.value;
  if (
    current?.source === 'dispatched' &&
    Number.isFinite(dispatchedAtMs) &&
    Number.isFinite(detectedAtMs) &&
    !agrees &&
    // A read taken BEFORE the dispatch is stale by construction; a read taken
    // just after it may have caught the pre-repaint screen.
    detectedAtMs < dispatchedAtMs + SESSION_CHAT_DISPATCH_GRACE_MS
  ) {
    return state;
  }
  if (
    current?.source === 'detected' &&
    agrees &&
    current.label === detected.label &&
    current.detectedSource === detected.source &&
    current.detectedAt === detectedAt
  ) {
    return state;
  }
  return {
    ...state,
    [descriptorId]: {
      value: detected.value,
      source: 'detected',
      label: detected.label,
      ...(detected.source ? { detectedSource: detected.source } : {}),
      detectedAt,
    },
  };
}

/** Folds a detection onto the local state (see the note above). */
export function applySessionChatDetectedOptions(
  catalog: SessionChatSessionOptionCatalog,
  state: SessionChatOptionState,
  detected: SessionChatDetectedOptionInput | null | undefined
): SessionChatOptionState {
  if (!detected) {
    return state;
  }
  let next = state;
  if (detected.model) {
    next = applyDetectedChoice(next, catalog.model.id, detected.model, detected.detectedAt);
  }
  if (detected.effort) {
    const modelValue = next[catalog.model.id]?.value ?? catalog.model.defaultValue ?? '';
    // Only when the current model actually has an effort option (Haiku has none).
    const hasEffort = catalog.optionsForModel(modelValue).some((descriptor) => descriptor.id === 'effort');
    if (hasEffort) {
      next = applyDetectedChoice(next, 'effort', detected.effort, detected.detectedAt);
    }
  }
  return next;
}

/**
 * Pill label: the value's label, or null when nothing is known.
 *
 * A detected label wins over the catalog's, so a real `Fable 5` / `Opus 4.5` /
 * unknown codex id renders verbatim. When the terminal only echoed the option's
 * own token (`high`), the catalog's prettier label is used instead.
 */
export function sessionChatOptionValueLabel(
  descriptor: SessionChatOptionDescriptor,
  state: SessionChatOptionState
): string | null {
  const current = state[descriptor.id];
  if (!current) {
    return null;
  }
  const choice = descriptor.choices?.find((entry) => entry.value === current.value);
  const detectedLabel = current.label?.trim();
  if (detectedLabel) {
    if (choice && detectedLabel.toLowerCase() === choice.value.toLowerCase()) {
      return choice.label;
    }
    return detectedLabel;
  }
  return choice?.label ?? null;
}

/** Options-pill label: known non-model values joined by " · " (§1.2). */
export function sessionChatOptionsPillLabel(
  descriptors: readonly SessionChatOptionDescriptor[],
  state: SessionChatOptionState
): string | null {
  const labels = descriptors
    .map((descriptor) => sessionChatOptionValueLabel(descriptor, state))
    .filter((label): label is string => label !== null);
  return labels.length > 0 ? labels.join(' · ') : null;
}

// ---------------------------------------------------------------------------
// Persistence — last dispatched values per session
// ---------------------------------------------------------------------------

const STORAGE_PREFIX = 'ghostex.sessionChat.options.';

function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    // Storage disabled by the embedder: pills still work, just per-mount.
    return null;
  }
}

export function readStoredSessionChatOptions(sessionKey: string | null | undefined): SessionChatOptionState {
  if (!sessionKey) {
    return {};
  }
  const raw = storage()?.getItem(`${STORAGE_PREFIX}${sessionKey}`);
  if (!raw) {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return {};
  }
  const next: Record<string, SessionChatOptionValue> = {};
  for (const [id, entry] of Object.entries(parsed as Record<string, unknown>)) {
    if (!entry || typeof entry !== 'object') {
      continue;
    }
    const { detectedAt, detectedSource, dispatchedAt, label, source, value } = entry as {
      detectedAt?: unknown;
      detectedSource?: unknown;
      dispatchedAt?: unknown;
      label?: unknown;
      source?: unknown;
      value?: unknown;
    };
    if (typeof value !== 'string' || (source !== 'default' && source !== 'dispatched' && source !== 'detected')) {
      continue;
    }
    const stored: SessionChatOptionValue = { value, source };
    if (typeof label === 'string' && label !== '') {
      stored.label = label;
    }
    if (typeof dispatchedAt === 'string') {
      stored.dispatchedAt = dispatchedAt;
    }
    if (typeof detectedAt === 'string') {
      stored.detectedAt = detectedAt;
    }
    if (detectedSource === 'terminal' || detectedSource === 'transcript') {
      stored.detectedSource = detectedSource;
    }
    next[id] = stored;
  }
  return next;
}

export function writeStoredSessionChatOptions(
  sessionKey: string | null | undefined,
  state: SessionChatOptionState
): void {
  if (!sessionKey) {
    return;
  }
  try {
    storage()?.setItem(`${STORAGE_PREFIX}${sessionKey}`, JSON.stringify(state));
  } catch {
    // Quota/private-mode failures must not break sending.
  }
}
