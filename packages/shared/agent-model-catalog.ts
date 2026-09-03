/*
CDXC:AgentProviders 2026-09-02:
The list of models, effort levels and fast-mode support each agent CLI offers
is published OUTSIDE the app, at `agent-model-catalog.json` on the repo's main
branch, so a CLI shipping a new model only needs that file edited and pushed.
Every Ghostex client (desktop CEF chat view, web app, mobile embedded chat)
renders its model dropdown and effort selector from this document; nothing
about the lineup is hard-coded in a component any more.

This module is the schema plus the pure helpers. The bundled snapshot, the
remote refresh and the React hook live in `agent-model-catalog-store.ts`.

How the values were collected, and how to re-collect them, is written up in
`docs/2026-09-02/agent-model-catalog/REPORT.md`.
*/

export const AGENT_MODEL_CATALOG_URL =
  'https://raw.githubusercontent.com/maddada/Ghostex/main/agent-model-catalog.json';

export const AGENT_MODEL_CATALOG_SCHEMA_VERSION = 1;

/** One row of an agent's model picker. */
export interface AgentModelCatalogModel {
  /** Stable id the client dispatches and persists; never shown to the user. */
  value: string;
  /** Display label, already shortened for the pills. */
  label: string;
  /**
   * The exact row text the CLI's own picker shows, when it differs from
   * `label`. Cursor's `/model <filter>` needs the literal "Cursor Grok 4.6"
   * even though the pill says "Grok 4.6".
   */
  pickerLabel?: string;
  description?: string;
  /** Effort levels this model accepts; empty when the model has none. */
  efforts: readonly string[];
  defaultEffort?: string;
  fastMode: boolean;
  default?: boolean;
}

export interface AgentModelCatalogFastMode {
  available: boolean;
  /** Slash command that toggles it, when the CLI has one. */
  command?: string | null;
  /** "model" when only some models offer it, "session" when it is global. */
  scope?: string | null;
}

export interface AgentModelCatalogAgent {
  name: string;
  /** Every effort level the agent knows, in rank order (lowest first). */
  efforts: readonly string[];
  defaultEffort?: string;
  fastMode: AgentModelCatalogFastMode;
  models: readonly AgentModelCatalogModel[];
}

export interface AgentModelCatalog {
  schemaVersion: number;
  /** ISO date; the newer of two catalogs wins (bundled vs cached vs remote). */
  updatedAt: string;
  /** Effort id → label shown in the selector. */
  effortLabels: Readonly<Record<string, string>>;
  agents: Readonly<Record<string, AgentModelCatalogAgent>>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() !== '' ? value : undefined;
}

function stringList(value: unknown): string[] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  const list: string[] = [];
  for (const entry of value) {
    if (typeof entry !== 'string' || entry.trim() === '') {
      return null;
    }
    list.push(entry);
  }
  return list;
}

function parseModel(input: unknown, agentEfforts: readonly string[]): AgentModelCatalogModel | null {
  if (!isRecord(input)) {
    return null;
  }
  const value = optionalString(input.value);
  const label = optionalString(input.label);
  if (!value || !label) {
    return null;
  }
  const efforts = input.efforts === undefined ? [...agentEfforts] : stringList(input.efforts);
  if (efforts === null) {
    return null;
  }
  const model: AgentModelCatalogModel = {
    value,
    label,
    efforts,
    fastMode: input.fastMode === true,
  };
  const pickerLabel = optionalString(input.pickerLabel);
  if (pickerLabel !== undefined && pickerLabel !== label) {
    model.pickerLabel = pickerLabel;
  }
  const description = optionalString(input.description);
  if (description !== undefined) {
    model.description = description;
  }
  const defaultEffort = optionalString(input.defaultEffort);
  if (defaultEffort !== undefined) {
    model.defaultEffort = defaultEffort;
  }
  if (input.default === true) {
    model.default = true;
  }
  return model;
}

function parseAgent(input: unknown): AgentModelCatalogAgent | null {
  if (!isRecord(input)) {
    return null;
  }
  const name = optionalString(input.name);
  const efforts = stringList(input.efforts);
  if (!name || efforts === null || !Array.isArray(input.models)) {
    return null;
  }
  const models: AgentModelCatalogModel[] = [];
  for (const entry of input.models) {
    const model = parseModel(entry, efforts);
    if (model === null) {
      return null;
    }
    models.push(model);
  }
  const fastModeInput = isRecord(input.fastMode) ? input.fastMode : {};
  const agent: AgentModelCatalogAgent = {
    name,
    efforts,
    fastMode: {
      available: fastModeInput.available === true,
      command: optionalString(fastModeInput.command) ?? null,
      scope: optionalString(fastModeInput.scope) ?? null,
    },
    models,
  };
  const defaultEffort = optionalString(input.defaultEffort);
  if (defaultEffort !== undefined) {
    agent.defaultEffort = defaultEffort;
  }
  return agent;
}

/**
 * Validates a catalog document. Returns null for anything that is not a
 * complete, well-formed catalog of the supported schema version, so a partial
 * or hand-edited file can never replace a good one.
 */
export function parseAgentModelCatalog(input: unknown): AgentModelCatalog | null {
  if (!isRecord(input) || input.schemaVersion !== AGENT_MODEL_CATALOG_SCHEMA_VERSION) {
    return null;
  }
  const updatedAt = optionalString(input.updatedAt);
  if (!updatedAt || !isRecord(input.agents) || !isRecord(input.effortLabels)) {
    return null;
  }
  const effortLabels: Record<string, string> = {};
  for (const [effort, label] of Object.entries(input.effortLabels)) {
    if (typeof label !== 'string' || label.trim() === '') {
      return null;
    }
    effortLabels[effort] = label;
  }
  const agents: Record<string, AgentModelCatalogAgent> = {};
  for (const [agentId, agentInput] of Object.entries(input.agents)) {
    const agent = parseAgent(agentInput);
    if (agent === null) {
      return null;
    }
    agents[agentId] = agent;
  }
  return { schemaVersion: AGENT_MODEL_CATALOG_SCHEMA_VERSION, updatedAt, effortLabels, agents };
}

/** The newer catalog by `updatedAt`; ties go to `candidate`. */
export function newerAgentModelCatalog(current: AgentModelCatalog, candidate: AgentModelCatalog): AgentModelCatalog {
  return candidate.updatedAt >= current.updatedAt ? candidate : current;
}

function sentenceCase(text: string): string {
  const first = text.charAt(0);
  return first === '' ? text : first.toUpperCase() + text.slice(1);
}

export function agentModelCatalogEffortLabel(catalog: AgentModelCatalog, effort: string): string {
  return catalog.effortLabels[effort] ?? sentenceCase(effort);
}

/**
 * Longest model label a footer pill shows before it is cut. Cursor's rows
 * are the long ones ("Gemini 3.7 Flash", "Kimi K2.7 Code"); Codex's
 * "GPT 5.3 Codex Spark" is the longest label that still fits whole.
 */
export const AGENT_MODEL_LABEL_MAX_CHARS = 20;

/**
 * Cuts a long model label for a pill, keeping whole words where it can and
 * closing with an ellipsis. The full label belongs in the pill's tooltip.
 */
export function truncateAgentModelLabel(label: string, maxChars: number = AGENT_MODEL_LABEL_MAX_CHARS): string {
  const trimmed = label.trim();
  if (trimmed.length <= maxChars) {
    return trimmed;
  }
  const budget = Math.max(1, maxChars - 1);
  const head = trimmed.slice(0, budget);
  const lastSpace = head.lastIndexOf(' ');
  // Keep the word boundary when it leaves at least half the budget visible;
  // otherwise a single long token is cut mid-word.
  const cut = lastSpace >= budget / 2 ? head.slice(0, lastSpace) : head;
  return `${cut.trimEnd()}…`;
}
