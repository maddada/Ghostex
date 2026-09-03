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
// Agents without a catalog (unknown ids) get no pills at all — with one
// exception owned by the pills, not by this module: a DRAFT session still
// renders its agent pill, because the composer's "Agents" switcher lives in
// that pill's menu (CDXC:Drafts 2026-08-28).

import {
  agentModelCatalogEffortLabel,
  type AgentModelCatalog,
  type AgentModelCatalogAgent,
  type AgentModelCatalogModel,
} from '../../shared/agent-model-catalog';
import { currentAgentModelCatalog } from '../../shared/agent-model-catalog-store';
import type { SessionChatDetectedChoice, SessionChatSendKey } from '../../shared/session-chat';
import type { SidebarAgentIcon } from '../../shared/sidebar-agents';

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
  /** The CLI's own row text when it differs from `label` (see AgentModelCatalogModel). */
  pickerLabel?: string;
  description?: string;
}

export type SessionChatOptionDispatch =
  /** Types `build(value)` into the TUI; the chosen value becomes the local truth. */
  | { kind: 'command'; build: (value: string) => string }
  /** Types a filtered picker command, then confirms its sole matching row. */
  | { kind: 'command-confirm-picker'; build: (value: string) => string }
  /** Types a fixed command that FLIPS an unknown baseline (no value tracked). */
  | { kind: 'toggle-command'; command: string }
  /** Types a command that opens the agent's own picker, then shows the terminal. */
  | { kind: 'agent-picker'; command: string }
  /**
   * CDXC:AgentScreenDetection 2026-09-03 WHY:
   * Codex's model and effort are set by driving its own `/model` picker on the
   * daemon (`/api/selectSessionChatModel`), because `/model <name>` is not a
   * command there. The model rows and the effort rows share this kind: a model
   * row keeps the current effort when the new model offers it, an effort row
   * keeps the current model. Hosts without the endpoint fall back to the
   * `agent-picker` behaviour (type `/model`, show the terminal).
   */
  | { kind: 'model-picker' }
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
  /** Cycles forward through a fixed ordered setting using one repeated key. */
  | { kind: 'cyclic-key-steps'; key: SessionChatSendKey }
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
  /**
   * Tooltip for a `terminal-handoff` pill, replacing the generic
   * "<category> <value>. Change it in the CLI." Set it when the CLI has a
   * named command for the change, so the handoff can say which one to type.
   */
  handoffHint?: string;
  /** Muted line under the menu heading. */
  description?: string;
}

export interface SessionChatSessionOptionCatalog {
  /** The model pill's descriptor (category "model"). */
  model: SessionChatOptionDescriptor;
  /** Provider artwork shown beside the current model name. */
  modelIcon: SidebarAgentIcon;
  /** Everything else, in category order, for the current model. */
  optionsForModel: (modelValue: string) => readonly SessionChatOptionDescriptor[];
  /**
   * For `model-picker` catalogs: the effort a model change should keep or
   * fall to — `currentEffort` when `modelValue` offers it, else the model's
   * catalog default.
   */
  pickerEffortFor?: (modelValue: string, currentEffort: string | undefined) => string | undefined;
}

// ---------------------------------------------------------------------------
// Catalog-driven descriptors
//
// CDXC:AgentProviders 2026-09-02: the model lineup, the effort levels and
// the fast-mode support of every agent come from the published agent model
// catalog (packages/shared/agent-model-catalog.ts), bundled as a snapshot and
// refreshed from the repo's main branch at runtime. This module only decides
// HOW each option is delivered to the TUI; it never names a model itself.
// ---------------------------------------------------------------------------

function effortChoices(catalog: AgentModelCatalog, efforts: readonly string[]): readonly SessionChatOptionChoice[] {
  return efforts.map((value) => ({ value, label: agentModelCatalogEffortLabel(catalog, value) }));
}

function modelChoices(agent: AgentModelCatalogAgent): readonly SessionChatOptionChoice[] {
  return agent.models.map((model) => {
    const choice: SessionChatOptionChoice = { value: model.value, label: model.label };
    if (model.pickerLabel !== undefined) {
      choice.pickerLabel = model.pickerLabel;
    }
    if (model.description !== undefined) {
      choice.description = model.description;
    }
    return choice;
  });
}

function catalogModel(agent: AgentModelCatalogAgent, modelValue: string): AgentModelCatalogModel | undefined {
  return agent.models.find((model) => model.value === modelValue);
}

/**
 * Effort levels offered under a model: the model's own list, or every level
 * the agent knows while the model is unknown or not in the catalog.
 */
function effortsForModel(agent: AgentModelCatalogAgent, modelValue: string): readonly string[] {
  return catalogModel(agent, modelValue)?.efforts ?? agent.efforts;
}

/** Same descriptor object for the same choice list, so callers can compare identity. */
function memoByChoices(
  build: (choices: readonly SessionChatOptionChoice[]) => SessionChatOptionDescriptor
): (choices: readonly SessionChatOptionChoice[]) => SessionChatOptionDescriptor {
  const cache = new Map<string, SessionChatOptionDescriptor>();
  return (choices) => {
    const key = choices.map((choice) => choice.value).join('\u0000');
    let descriptor = cache.get(key);
    if (descriptor === undefined) {
      descriptor = build(choices);
      cache.set(key, descriptor);
    }
    return descriptor;
  };
}

// ---------------------------------------------------------------------------
// Claude / OpenClaude
// ---------------------------------------------------------------------------

const CLAUDE_MODES: readonly SessionChatOptionChoice[] = [
  { value: 'bypass', label: 'Bypass permissions' },
  { value: 'auto', label: 'Auto' },
  { value: 'manual', label: 'Manual' },
  { value: 'accept-edits', label: 'Accept edits' },
  { value: 'plan', label: 'Plan' },
];

/*
Permission mode is Shift+Tab in Claude Code's TUI. The terminal footer supplies
the current value, so selecting a target sends the exact forward distance in
Claude's cyclic mode order.
*/
const CLAUDE_MODE: SessionChatOptionDescriptor = {
  id: 'mode',
  label: 'Mode',
  category: 'mode',
  choices: CLAUDE_MODES,
  description: "Select Claude Code's permission mode.",
  dispatch: { kind: 'cyclic-key-steps', key: 'shift-tab' },
};

function buildClaudeCatalog(
  catalog: AgentModelCatalog,
  agent: AgentModelCatalogAgent
): SessionChatSessionOptionCatalog {
  const model: SessionChatOptionDescriptor = {
    id: 'model',
    label: 'Model',
    category: 'model',
    choices: modelChoices(agent),
    dispatch: { kind: 'command', build: (value) => `/model ${value}` },
  };
  const effortFor = memoByChoices((choices) => ({
    id: 'effort',
    label: 'Effort',
    category: 'thought_level',
    choices,
    dispatch: { kind: 'command', build: (value) => `/effort ${value}` },
  }));
  const fastMode: SessionChatOptionDescriptor = {
    id: 'fastMode',
    label: 'Fast mode',
    category: 'model_config',
    actionLabel: 'Toggle Fast mode',
    dispatch: { kind: 'toggle-command', command: agent.fastMode.command ?? '/fast' },
  };
  return {
    model,
    modelIcon: 'claude',
    optionsForModel: (modelValue) => {
      // Until gxserver confirms the model, do not offer effort controls that
      // may not exist for the actual model (the catalog gives Haiku none).
      const current = catalogModel(agent, modelValue);
      const efforts = current?.efforts ?? [];
      return sortDescriptors([
        ...(efforts.length > 0 ? [effortFor(effortChoices(catalog, efforts))] : []),
        ...(agent.fastMode.available && current?.fastMode === true ? [fastMode] : []),
        CLAUDE_MODE,
      ]);
    },
  };
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/** Effort levels the catalog gives a Codex model (every level while unknown). */
export function codexEffortChoices(modelValue: string): readonly SessionChatOptionChoice[] {
  const catalog = currentAgentModelCatalog();
  const agent = catalog.agents.codex;
  return agent ? effortChoices(catalog, effortsForModel(agent, modelValue)) : [];
}

const CODEX_MODE: SessionChatOptionDescriptor = {
  id: 'mode',
  label: 'Mode',
  category: 'mode',
  actionLabel: 'Switch to Plan mode',
  dispatch: { kind: 'command', build: () => '/plan' },
};

function buildCodexCatalog(catalog: AgentModelCatalog, agent: AgentModelCatalogAgent): SessionChatSessionOptionCatalog {
  /*
  CDXC:AgentScreenDetection 2026-09-03 DECISION:
  User: Codex must list the available models and efforts in the chat box, and
  whether fast mode is on or off, like Claude does; fast is toggled by sending
  `/fast`. Model and effort go through the daemon-driven picker
  (`model-picker`), the earlier Shift+Up/Down effort steps were relative to a
  value the screen may not have shown yet.
  */
  const model: SessionChatOptionDescriptor = {
    id: 'model',
    label: 'Model',
    category: 'model',
    choices: modelChoices(agent),
    actionLabel: "Open the CLI's model picker",
    dispatch: { kind: 'model-picker' },
  };
  const effortFor = memoByChoices((choices) => ({
    id: 'effort',
    label: 'Reasoning effort',
    category: 'thought_level',
    choices,
    dispatch: { kind: 'model-picker' },
  }));
  const fastMode: SessionChatOptionDescriptor = {
    id: 'fastMode',
    label: 'Fast mode',
    category: 'model_config',
    actionLabel: 'Fast mode',
    dispatch: { kind: 'toggle-command', command: agent.fastMode.command ?? '/fast' },
  };
  return {
    model,
    modelIcon: 'codex',
    optionsForModel: (modelValue) => {
      const current = catalogModel(agent, modelValue);
      return sortDescriptors([
        effortFor(effortChoices(catalog, effortsForModel(agent, modelValue))),
        ...(agent.fastMode.available && (current?.fastMode ?? true) ? [fastMode] : []),
        CODEX_MODE,
      ]);
    },
    pickerEffortFor: (modelValue, currentEffort) => {
      const efforts = effortsForModel(agent, modelValue);
      if (currentEffort !== undefined && efforts.includes(currentEffort)) {
        return currentEffort;
      }
      return catalogModel(agent, modelValue)?.defaultEffort ?? efforts[0];
    },
  };
}

// ---------------------------------------------------------------------------
// Cursor Agent
// ---------------------------------------------------------------------------

function buildCursorCatalog(
  catalog: AgentModelCatalog,
  agent: AgentModelCatalogAgent
): SessionChatSessionOptionCatalog {
  const choices = modelChoices(agent);
  /* The picker filter needs the row's literal text ("Cursor Grok 4.6"). */
  const pickerFilter = (value: string): string => {
    const choice = choices.find((entry) => entry.value === value);
    return choice?.pickerLabel ?? choice?.label ?? value;
  };
  const model: SessionChatOptionDescriptor = {
    id: 'model',
    label: 'Model',
    category: 'model',
    choices,
    dispatch: {
      kind: 'command-confirm-picker',
      build: (value) => `/model ${pickerFilter(value)}`,
    },
  };
  /*
  Cursor exposes reasoning effort inside the model picker's Tab-to-edit panel.
  The footer is authoritative, but navigating that nested picker blind would
  also risk changing the context window or Fast toggle. Show the detected value
  and hand the user to the agent-owned picker to change it.
  */
  const effortFor = memoByChoices((effortRows) => ({
    id: 'effort',
    label: 'Reasoning effort',
    category: 'thought_level',
    choices: effortRows,
    actionLabel: 'Change it in the CLI',
    dispatch: { kind: 'agent-picker', command: '/model' },
  }));
  return {
    model,
    modelIcon: 'cursor-cli',
    optionsForModel: (modelValue) => {
      const efforts = effortsForModel(agent, modelValue);
      return efforts.length > 0 ? [effortFor(effortChoices(catalog, efforts))] : [];
    },
  };
}

// ---------------------------------------------------------------------------
// Grok
// ---------------------------------------------------------------------------

/*
Grok prints `Grok 4.6 (medium)` in its composer footer and changes both values
through one interactive `/model` picker, which also owns the effort list per
model. Blind keystrokes into that picker would be guesswork against a menu this
side cannot see, so both pills are read-only mirrors of the statusline gxserver
already reads, and either one hands the user to the terminal to make the change.
*/
function buildGrokCatalog(catalog: AgentModelCatalog, agent: AgentModelCatalogAgent): SessionChatSessionOptionCatalog {
  /** No `choices`: the model is never typed from here, only mirrored. */
  const model: SessionChatOptionDescriptor = {
    id: 'model',
    label: 'Model',
    category: 'model',
    actionLabel: 'Change it in the CLI',
    dispatch: { kind: 'terminal-handoff' },
  };
  const effortFor = memoByChoices((choices) => ({
    id: 'effort',
    label: 'Reasoning effort',
    category: 'thought_level',
    choices,
    actionLabel: 'Change it in the CLI',
    dispatch: { kind: 'terminal-handoff' },
  }));
  return {
    model,
    modelIcon: 'grok-build',
    optionsForModel: (modelValue) => [effortFor(effortChoices(catalog, effortsForModel(agent, modelValue)))],
  };
}

// ---------------------------------------------------------------------------
// Antigravity CLI
// ---------------------------------------------------------------------------

/*
Antigravity's `/model` takes one flattened id per model and effort
(`gemini-3.8-flash-high`; see `agy models`), and rejects the bare model id
when the model has efforts. The catalog keys rows by the model part, so the
model pill appends the model's default effort and the effort pill re-sends
the current model with the chosen effort. Models without efforts (the Claude
and GPT-OSS rows) are typed as-is.
*/
function antigravityModelCommand(agent: AgentModelCatalogAgent, modelValue: string, effort?: string): string {
  const model = catalogModel(agent, modelValue);
  const suffix = effort ?? model?.defaultEffort ?? model?.efforts[0];
  return model !== undefined && model.efforts.length > 0 && suffix !== undefined
    ? `/model ${modelValue}-${suffix}`
    : `/model ${modelValue}`;
}

function buildAntigravityCatalog(
  catalog: AgentModelCatalog,
  agent: AgentModelCatalogAgent
): SessionChatSessionOptionCatalog {
  const model: SessionChatOptionDescriptor = {
    id: 'model',
    label: 'Model',
    category: 'model',
    choices: modelChoices(agent),
    dispatch: { kind: 'command', build: (value) => antigravityModelCommand(agent, value) },
  };
  const effortByModel = new Map<string, SessionChatOptionDescriptor>();
  const effortFor = (modelValue: string, efforts: readonly string[]): SessionChatOptionDescriptor => {
    let descriptor = effortByModel.get(modelValue);
    if (descriptor === undefined) {
      descriptor = {
        id: 'effort',
        label: 'Effort',
        category: 'thought_level',
        choices: effortChoices(catalog, efforts),
        dispatch: { kind: 'command', build: (effort) => antigravityModelCommand(agent, modelValue, effort) },
      };
      effortByModel.set(modelValue, descriptor);
    }
    return descriptor;
  };
  return {
    model,
    modelIcon: 'antigravity-cli',
    optionsForModel: (modelValue) => {
      // The effort is typed together with the model, so it is only offered
      // once gxserver has confirmed which catalog model is running.
      const current = catalogModel(agent, modelValue);
      return current !== undefined && current.efforts.length > 0 ? [effortFor(current.value, current.efforts)] : [];
    },
  };
}

// ---------------------------------------------------------------------------
// Pi
// ---------------------------------------------------------------------------

/*
Pi reports both values in its terminal statusline. Its model list is
provider-dependent, and model/effort changes belong to the CLI, so these are
read-only mirrors with the same terminal handoff used for Grok.
*/
const PI_MODEL: SessionChatOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  actionLabel: 'Change it in the CLI',
  dispatch: { kind: 'terminal-handoff' },
};

const PI_EFFORT: SessionChatOptionDescriptor = {
  id: 'effort',
  label: 'Reasoning effort',
  category: 'thought_level',
  actionLabel: 'Change it in the CLI',
  dispatch: { kind: 'terminal-handoff' },
};

/*
Hermes names its model in the leading segment of its terminal statusline and
owns model selection in its interactive /model picker, so the pill is the same
read-only terminal mirror used for Grok and Pi. Its statusline never names a
reasoning effort, so there is no effort pill to mirror.
*/
const HERMES_MODEL: SessionChatOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  actionLabel: 'Change it in the CLI',
  handoffHint: 'Change the model in the CLI by using /model',
  dispatch: { kind: 'terminal-handoff' },
};

const OMP_MODEL: SessionChatOptionDescriptor = {
  id: 'model',
  label: 'Model',
  category: 'model',
  actionLabel: 'Change it in the CLI',
  dispatch: { kind: 'terminal-handoff' },
};

const OMP_EFFORT: SessionChatOptionDescriptor = {
  id: 'effort',
  label: 'Reasoning effort',
  category: 'thought_level',
  actionLabel: 'Change it in the CLI',
  dispatch: { kind: 'terminal-handoff' },
};

// ---------------------------------------------------------------------------
// Catalog resolution
// ---------------------------------------------------------------------------

function sortDescriptors(descriptors: readonly SessionChatOptionDescriptor[]): readonly SessionChatOptionDescriptor[] {
  return [...descriptors].sort((left, right) => CATEGORY_ORDER[left.category] - CATEGORY_ORDER[right.category]);
}

const HERMES_CATALOG: SessionChatSessionOptionCatalog = {
  model: HERMES_MODEL,
  modelIcon: 'hermes-agent',
  optionsForModel: () => [],
};

const PI_CATALOG: SessionChatSessionOptionCatalog = {
  model: PI_MODEL,
  modelIcon: 'pi',
  optionsForModel: () => [PI_EFFORT],
};

const OMP_CATALOG: SessionChatSessionOptionCatalog = {
  model: OMP_MODEL,
  modelIcon: 'omp',
  optionsForModel: () => [OMP_EFFORT],
};

const CATALOG_BUILDERS: Record<
  string,
  (catalog: AgentModelCatalog, agent: AgentModelCatalogAgent) => SessionChatSessionOptionCatalog
> = {
  claude: buildClaudeCatalog,
  codex: buildCodexCatalog,
  cursor: buildCursorCatalog,
  grok: buildGrokCatalog,
  antigravity: buildAntigravityCatalog,
};

/** Built once per catalog document, so a refresh swaps every agent at once. */
const catalogsByDocument = new WeakMap<AgentModelCatalog, Record<string, SessionChatSessionOptionCatalog>>();

function catalogsFor(catalog: AgentModelCatalog): Record<string, SessionChatSessionOptionCatalog> {
  let byAgent = catalogsByDocument.get(catalog);
  if (byAgent !== undefined) {
    return byAgent;
  }
  byAgent = {};
  for (const [agentId, build] of Object.entries(CATALOG_BUILDERS)) {
    const agent = catalog.agents[agentId];
    if (agent !== undefined) {
      byAgent[agentId] = build(catalog, agent);
    }
  }
  if (byAgent.claude !== undefined) {
    byAgent.openclaude = byAgent.claude;
  }
  // Both the transcript family id (read state) and the sidebar agent id reach
  // this lookup, so the catalog answers to either spelling.
  if (byAgent.antigravity !== undefined) {
    byAgent['antigravity-cli'] = byAgent.antigravity;
  }
  byAgent.hermes = HERMES_CATALOG;
  byAgent['hermes-agent'] = HERMES_CATALOG;
  byAgent.omp = OMP_CATALOG;
  byAgent.pi = PI_CATALOG;
  catalogsByDocument.set(catalog, byAgent);
  return byAgent;
}

/**
 * The option catalog for an agent, built from the agent model catalog in
 * effect right now. React callers pair this with `useAgentModelCatalog` so a
 * remote refresh re-renders them (session-chat-option-pills.tsx).
 */
export function sessionChatSessionOptionCatalog(
  agent: string | null | undefined
): SessionChatSessionOptionCatalog | null {
  if (agent === null || agent === undefined) {
    return null;
  }
  return catalogsFor(currentAgentModelCatalog())[agent] ?? null;
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
      dispatch.kind === 'command' || dispatch.kind === 'command-confirm-picker'
        ? dispatch.build(descriptor.choices?.[0]?.value ?? '')
        : dispatch.kind === 'toggle-command' || dispatch.kind === 'agent-picker'
          ? dispatch.command
          : dispatch.kind === 'model-picker'
            ? '/model'
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
  detectedSource?: SessionChatDetectedChoice['source'];
  /** ISO time gxserver read the value (source "detected"). */
  detectedAt?: string;
}

/** Descriptor id → local value. Only value-carrying options appear. */
export type SessionChatOptionState = Readonly<Record<string, SessionChatOptionValue>>;

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
    (descriptor.dispatch.kind === 'command' ||
      descriptor.dispatch.kind === 'command-confirm-picker' ||
      descriptor.dispatch.kind === 'model-picker' ||
      descriptor.dispatch.kind === 'bounded-key-steps' ||
      descriptor.dispatch.kind === 'cyclic-key-steps') &&
    descriptor.choices !== undefined &&
    descriptor.choices.length > 0
  );
}

/** Exact forward-only key sequence for a cyclic ordered setting. */
export function sessionChatCyclicKeySteps(
  choices: readonly SessionChatOptionChoice[],
  currentValue: string | undefined,
  targetValue: string,
  key: SessionChatSendKey
): SessionChatSendKey[] {
  const currentIndex = choices.findIndex((choice) => choice.value === currentValue);
  const targetIndex = choices.findIndex((choice) => choice.value === targetValue);
  if (currentIndex < 0 || targetIndex < 0 || choices.length < 2) {
    return [];
  }
  const count = (targetIndex - currentIndex + choices.length) % choices.length;
  return Array.from({ length: count }, () => key);
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
CDXC:AgentScreenDetection 2026-08-01:
gxserver reads the agent's structured transcript and terminal statusline and
reports what it is REALLY running
(`selectedOptions` on read results and snapshot/replaced/state frames). That
outranks this surface's local truth, with one exception: a value the user just
dispatched keeps the pill for a short grace window, because the TUI may not have
repainted yet. A detection that AGREES with a pending dispatch confirms it.
Nothing detected ⇒ nothing here runs and no current value is claimed.
*/
export interface SessionChatDetectedOptionInput {
  model?: { value: string; label: string; source?: SessionChatDetectedChoice['source'] };
  effort?: { value: string; label: string; source?: SessionChatDetectedChoice['source'] };
  mode?: { value: string; label: string; source?: SessionChatDetectedChoice['source'] };
  contextWindow?: string;
  terminalStatusLine?: string;
  fast?: boolean;
  detectedAt: string;
}

function applyDetectedChoice(
  state: SessionChatOptionState,
  descriptorId: string,
  detected: {
    value: string;
    label: string;
    source?: SessionChatDetectedChoice['source'];
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
  if (detected.mode) {
    const modelValue = next[catalog.model.id]?.value ?? catalog.model.defaultValue ?? '';
    const hasMode = catalog.optionsForModel(modelValue).some((descriptor) => descriptor.id === 'mode');
    if (hasMode) {
      next = applyDetectedChoice(next, 'mode', detected.mode, detected.detectedAt);
    }
  }
  return next;
}

/** Composer chips: first letter up, the rest left as the agent reported it. */
function sessionChatSentenceCaseLabel(label: string): string {
  const first = label.charAt(0);
  if (first === '') {
    return label;
  }
  return first.toUpperCase() + label.slice(1);
}

/** The detected text names this choice: its id, its label, or the CLI's own row text. */
function choiceNamesLabel(choice: SessionChatOptionChoice, detectedLabel: string): boolean {
  const needle = detectedLabel.toLowerCase();
  return [choice.value, choice.label, choice.pickerLabel ?? ''].some((name) => name.toLowerCase() === needle);
}

/**
 * Pill label: the value's label, or null when nothing is known.
 *
 * When the detected text names a catalog choice (its id, its label, or the
 * CLI's own row text such as `Claude Opus 5` / `gpt-5.6-sol`), the catalog's
 * display label wins, so the published catalog decides how a model is shown.
 * Anything else the agent reports (`Opus 4.5` on a `--model` override, an
 * unknown id) still renders verbatim; lowercase statusline tokens
 * (`grok-4.6`, `medium`) are shown in sentence case.
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
    if (choice && choiceNamesLabel(choice, detectedLabel)) {
      return choice.label;
    }
    return sessionChatSentenceCaseLabel(detectedLabel);
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

/*
The key after this prefix is composed by `useSessionChatSessionOptions`
(session-chat-option-pills.tsx), which appends the draft's concrete agent id to
the session key while the session is a draft. Read/write here take whatever key
they are handed; the scheme itself — and why a non-draft session keeps the bare
session key — is documented at that call site.
*/
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
    if (detectedSource === 'terminal' || detectedSource === 'transcript' || detectedSource === 'statusline') {
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
