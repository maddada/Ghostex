import { agentModelCatalogEffortLabel, type AgentModelCatalog } from '@/packages/shared/agent-model-catalog';
import type { ModelPickerRequest, ModelPickerProvider } from './session-chat-model-picker';

const SHORT_MODEL_LABELS: Record<string, string> = {
  'gpt-6-astra': 'Astra',
  'gpt-5.6-sol': 'Sol',
  'gpt-5.6-terra': 'Terra',
  'gpt-5.6-luna': 'Luna',
  fable: 'Fable',
  'opus[1m]': 'Opus (1m)',
  opus: 'Opus',
  sonnet: 'Sonnet',
  haiku: 'Haiku',
};

/** CDXC:SessionChat 2026-09-11 DECISION: User chose this exact top-to-bottom Cursor overlay order, keeping related models together. */
const CURSOR_MODEL_ORDER = [
  'auto',
  'cursor-grok-4.6',
  'gemini-3.8-flash',
  'claude-fable-5-1',
  'claude-opus-5',
  'claude-opus-4-8',
  'claude-sonnet-5',
  'gpt-5.6-sol',
  'gpt-5.6-terra',
  'gpt-5.6-luna',
];
const CURSOR_MODEL_RANK = new Map(CURSOR_MODEL_ORDER.map((value, index) => [value, index]));

/** CDXC:SessionChat 2026-09-11 DECISION: User chose this exact top-to-bottom Antigravity overlay order, with Gemini together, then Opus, Sonnet and GPT-OSS. */
const ANTIGRAVITY_MODEL_ORDER = [
  'gemini-3.8-flash',
  'gemini-3.1-pro',
  'claude-opus-4-6-thinking',
  'claude-sonnet-4-6',
  'gpt-oss-120b-medium',
];
const ANTIGRAVITY_MODEL_RANK = new Map(ANTIGRAVITY_MODEL_ORDER.map((value, index) => [value, index]));
const MODEL_RANKS: Partial<Record<ModelPickerProvider, Map<string, number>>> = {
  cursor: CURSOR_MODEL_RANK,
  antigravity: ANTIGRAVITY_MODEL_RANK,
};

/** CDXC:SessionChat 2026-09-09 DECISION: User: Cursor, Grok Build and Antigravity get the quick picker with white accents and one standard icon for every model. */
export function modelPickerProvider(icon?: string): ModelPickerProvider | undefined {
  if (icon === 'claude' || icon === 'codex') return icon;
  if (icon === 'cursor-cli' || icon === 'cursor') return 'cursor';
  if (icon === 'grok-build' || icon === 'grok') return 'grok';
  if (icon === 'antigravity-cli' || icon === 'antigravity') return 'antigravity';
}

/** Shared by the in-pane chat picker and the terminal's native modal host. */
export function createModelPickerRequest(
  catalog: AgentModelCatalog,
  provider: ModelPickerProvider,
  selectedModel?: string,
  selectedEffort?: string
): ModelPickerRequest | undefined {
  const agent = catalog.agents[provider];
  if (!agent) return;
  const modelRank = MODEL_RANKS[provider];
  const models = agent.models
    // CDXC:SessionChat 2026-09-09 DECISION: User: keep only the selected models in the quick picker, exclude Cursor Composer too, and retain every other model under Legacy in the normal picker.
    .filter((model) => !model.group)
    .sort((a, b) =>
      modelRank ? (modelRank.get(a.value) ?? modelRank.size) - (modelRank.get(b.value) ?? modelRank.size) : 0
    )
    .map((model) => ({
      value: model.value,
      label:
        provider === 'claude' || provider === 'codex' ? (SHORT_MODEL_LABELS[model.value] ?? model.label) : model.label,
      version: provider === 'codex' ? model.label.replace(/\s+(Astra|Sol|Terra|Luna)$/, '') : undefined,
      efforts: model.efforts.map((value) => ({ value, label: agentModelCatalogEffortLabel(catalog, value) })),
      defaultEffort: model.defaultEffort ?? agent.defaultEffort,
    }));
  // Detection may not have arrived yet. The catalog default is a starting cursor, not a claim about the running agent.
  const model =
    models.find((entry) => entry.value === selectedModel) ??
    models.find((entry) => entry.value === agent.models.find((model) => model.default)?.value) ??
    models[0];
  if (!model) return;
  const effort =
    model.efforts.find((entry) => entry.value === selectedEffort)?.value ??
    model.efforts.find((entry) => entry.value === model.defaultEffort)?.value ??
    model.efforts[0]?.value ??
    '';
  const efforts = agent.efforts.map((value) => ({ value, label: agentModelCatalogEffortLabel(catalog, value) }));
  return {
    requestId: crypto.randomUUID(),
    provider,
    models,
    efforts,
    model: model.value,
    effort,
  };
}
