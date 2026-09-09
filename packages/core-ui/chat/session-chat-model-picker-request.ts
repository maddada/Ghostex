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
  const models = agent.models
    // CDXC:SessionChat 2026-09-09 DECISION: User: keep only the selected models in the quick picker, exclude Cursor Composer too, and retain every other model under Legacy in the normal picker.
    .filter((model) => !model.group)
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
