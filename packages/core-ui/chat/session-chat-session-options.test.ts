import { afterEach, describe, expect, it, vi } from 'vitest';
import { createSessionChatOptionState } from './session-chat-option-state';
import {
  applySessionChatDetectedOptions,
  codexEffortChoices,
  reconcileSessionChatOptionsFromCommand,
  seedSessionChatOptionState,
  sessionChatBoundedKeySteps,
  sessionChatOptionCommandNames,
  sessionChatOptionsPillLabel,
  sessionChatOptionTracksValue,
  sessionChatOptionValueLabel,
  sessionChatSessionOptionCatalog,
  SESSION_CHAT_DISPATCH_GRACE_MS,
  setSessionChatOptionValue,
} from './session-chat-session-options';
import { classifySessionChatSend } from './session-chat-send-classification';
import { sessionChatSlashCommandsForAgent } from './session-chat-slash-commands';

function catalogFor(agent: string) {
  const catalog = sessionChatSessionOptionCatalog(agent);
  if (!catalog) {
    throw new Error(`expected a catalog for ${agent}`);
  }
  return catalog;
}

describe('pending session chat options', () => {
  afterEach(() => vi.useRealTimers());

  it('shows the selected Claude model before delivery completes, then rolls back a failed request', () => {
    const catalog = catalogFor('claude');
    const store = createSessionChatOptionState(catalog, undefined, vi.fn());
    store.applyDetected({ model: { value: 'sonnet', label: 'Sonnet 5' }, detectedAt: new Date().toISOString() });
    const changed = vi.fn();
    const unsubscribe = store.subscribe(changed);
    const request = store.beginDispatch({ model: 'opus' });
    expect(changed).toHaveBeenCalledOnce();
    expect(sessionChatOptionValueLabel(catalog.model, store.getSnapshot())).toBe('Opus 5');
    expect(store.getSnapshot().model?.source).toBe('dispatched');
    request.rollback();
    expect(sessionChatOptionValueLabel(catalog.model, store.getSnapshot())).toBe('Sonnet 5');
    unsubscribe();
  });

  it('keeps a newer selection when an older request fails, then confirms fresh evidence', () => {
    vi.useFakeTimers();
    const store = createSessionChatOptionState(catalogFor('codex'), undefined, vi.fn());
    const oldChange = store.beginDispatch({ effort: 'high' });
    const newChange = store.beginDispatch({ effort: 'ultra' });
    oldChange.rollback();
    expect(store.getSnapshot().effort?.value).toBe('ultra');
    newChange.complete();
    store.applyDetected({
      model: { value: 'gpt-6-astra', label: 'Astra', source: 'terminal' },
      effort: { value: 'ultra', label: 'Ultra', source: 'terminal' },
      detectedAt: new Date().toISOString(),
    });
    expect(store.getSnapshot().effort?.source).toBe('detected');
  });

  it('restores the detected mode after failed delivery without changing another session', () => {
    const first = createSessionChatOptionState(catalogFor('claude'), undefined, vi.fn());
    const second = createSessionChatOptionState(catalogFor('claude'), undefined, vi.fn());
    first.applyDetected({ mode: { value: 'manual', label: 'Manual' }, detectedAt: new Date().toISOString() });
    const change = first.beginDispatch({ mode: 'plan' });
    expect(first.getSnapshot().mode?.value).toBe('plan');
    expect(second.getSnapshot().mode).toBeUndefined();
    change.rollback();
    expect(first.getSnapshot().mode?.value).toBe('manual');
  });

  it('undoes an unconfirmed Fast toggle using the fresh unchanged footer', () => {
    vi.useFakeTimers();
    const unconfirmed = vi.fn();
    const store = createSessionChatOptionState(catalogFor('codex'), undefined, unconfirmed);
    const unsubscribe = store.subscribe(() => {});
    const change = store.beginDispatch({ fastMode: 'on' });
    change.complete();
    vi.advanceTimersByTime(150);
    store.applyDetected({
      model: { value: 'gpt-6-astra', label: 'Astra', source: 'terminal' },
      detectedAt: new Date().toISOString(),
    });
    expect(store.getSnapshot().fastMode?.value).toBe('on');
    vi.advanceTimersByTime(SESSION_CHAT_DISPATCH_GRACE_MS);
    expect(store.getSnapshot().fastMode?.value).toBe('off');
    expect(unconfirmed).toHaveBeenCalledOnce();
    unsubscribe();
  });

  it('clears Codex Plan on an ordinary footer and keeps the pill label to effort only', () => {
    const catalog = catalogFor('codex');
    const store = createSessionChatOptionState(catalog, undefined, vi.fn());
    store.applyDetected({
      model: { value: 'gpt-6-astra', label: 'Astra', source: 'terminal' },
      effort: { value: 'high', label: 'High' },
      mode: { value: 'plan', label: 'Plan' },
      detectedAt: new Date().toISOString(),
    });
    expect(store.getSnapshot().mode?.value).toBe('plan');
    expect(sessionChatOptionsPillLabel(catalog.optionsForModel('gpt-6-astra'), store.getSnapshot())).toBe('High');
    store.applyDetected({
      model: { value: 'gpt-6-astra', label: 'Astra', source: 'terminal' },
      detectedAt: new Date().toISOString(),
    });
    expect(store.getSnapshot().mode?.value).toBe('default');
  });
});

describe('session chat session-option catalogs', () => {
  it('gives Pi a terminal handoff and unknown agents no pills at all', () => {
    expect(catalogFor('pi').model.dispatch).toEqual({ kind: 'terminal-handoff' });
    expect(sessionChatSessionOptionCatalog(null)).toBeNull();
    expect(sessionChatSessionOptionCatalog('unknown-agent')).toBeNull();
    expect(sessionChatOptionCommandNames('grok')).toEqual([]);
  });

  it('shares one catalog between claude and openclaude', () => {
    expect(sessionChatSessionOptionCatalog('openclaude')).toBe(sessionChatSessionOptionCatalog('claude'));
  });

  it('shows the claude model lineup from the published agent model catalog', () => {
    expect(catalogFor('claude').model.choices?.map(({ value, label }) => ({ value, label }))).toEqual([
      { value: 'fable', label: 'Fable 5.1' },
      { value: 'opus[1m]', label: 'Opus 5 (1M)' },
      { value: 'opus', label: 'Opus 5' },
      { value: 'sonnet', label: 'Sonnet 5' },
      { value: 'haiku', label: 'Haiku 4.5' },
    ]);
  });

  it('does not claim a claude model or effort before agent-owned evidence', () => {
    const catalog = catalogFor('claude');
    const state = seedSessionChatOptionState(catalog);
    expect(state).toEqual({});
    expect(sessionChatOptionValueLabel(catalog.model, state)).toBeNull();
    expect(catalog.optionsForModel('').map((descriptor) => descriptor.id)).toEqual(['mode']);
  });

  it("varies claude's options by model", () => {
    const catalog = catalogFor('claude');
    const ids = (model: string) => catalog.optionsForModel(model).map((descriptor) => descriptor.id);
    // Haiku has no effort tiers; only Opus offers fast mode; mode is always
    // available and sorts last.
    expect(ids('sonnet')).toEqual(['effort', 'mode']);
    expect(ids('haiku')).toEqual(['mode']);
    expect(ids('opus')).toEqual(['effort', 'fastMode', 'mode']);
  });

  it('delivers claude values as slash commands', () => {
    const catalog = catalogFor('claude');
    const model = catalog.model;
    if (model.dispatch.kind !== 'command') {
      throw new Error('claude model must dispatch a command');
    }
    expect(model.dispatch.build('opus')).toBe('/model opus');
    const effort = catalog.optionsForModel('sonnet').find((descriptor) => descriptor.id === 'effort');
    if (effort?.dispatch.kind !== 'command') {
      throw new Error('claude effort must dispatch a command');
    }
    expect(effort.dispatch.build('xhigh')).toBe('/effort xhigh');
    expect(effort.choices?.map((choice) => choice.value)).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
      'ultracode',
    ]);
    const fast = catalog.optionsForModel('opus').find((descriptor) => descriptor.id === 'fastMode');
    expect(fast?.dispatch).toEqual({ kind: 'toggle-command', command: '/fast' });
  });

  it("cycles claude's mode with a real Shift+Tab keystroke", () => {
    const mode = catalogFor('claude')
      .optionsForModel('sonnet')
      .find((descriptor) => descriptor.id === 'mode');
    expect(mode?.dispatch).toEqual({
      kind: 'cyclic-key-steps',
      key: 'shift-tab',
    });
    expect(mode && sessionChatOptionTracksValue(mode)).toBe(true);
  });

  it("drives codex's model picker for both the model and the effort", () => {
    const catalog = catalogFor('codex');
    expect(catalog.model.actionLabel).toBe("Open the CLI's model picker");
    expect(catalog.model.description).toBeUndefined();
    expect(catalog.model.dispatch).toEqual({ kind: 'model-picker' });
    expect(catalog.pickerEffortFor?.('gpt-5.6-sol', 'xhigh')).toBe('xhigh');
    expect(catalog.pickerEffortFor?.('gpt-5.5', 'ultra')).toBe('medium');
    const effort = catalog.optionsForModel('gpt-5.6-sol').find((descriptor) => descriptor.id === 'effort');
    expect(effort?.dispatch).toEqual({
      kind: 'model-picker',
    });
    expect(seedSessionChatOptionState(catalog)).toEqual({});
    expect(sessionChatOptionValueLabel(catalog.model, {})).toBeNull();
  });

  it('offers each codex model the effort levels the catalog gives it', () => {
    expect(codexEffortChoices('gpt-5.6-sol').map((choice) => choice.value)).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
      'ultra',
    ]);
    expect(codexEffortChoices('gpt-5.6-luna').map((choice) => choice.value)).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
    ]);
    expect(codexEffortChoices('gpt-5.5').map((choice) => choice.value)).toEqual(['low', 'medium', 'high', 'xhigh']);
    // An unknown model gets every level the agent knows.
    expect(codexEffortChoices('gpt-9.1-nova').map((choice) => choice.value)).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
      'ultra',
    ]);
  });

  it('shows Cursor models without the brand prefix and offers every effort level', () => {
    const catalog = catalogFor('cursor');
    expect(catalog.model.choices?.find((choice) => choice.value === 'cursor-grok-4.6')?.label).toBe('Grok 4.6');
    if (catalog.model.dispatch.kind !== 'command-confirm-picker') {
      throw new Error('cursor model must dispatch a filtered picker command');
    }
    expect(catalog.model.dispatch.build('cursor-grok-4.6')).toBe('/model Cursor Grok 4.6');
    expect(catalog.optionsForModel('cursor-grok-4.6')[0]?.choices?.map((choice) => choice.label)).toEqual([
      'Low',
      'Medium',
      'High',
      'Extra high',
    ]);
    expect(catalog.model.choices?.find((choice) => choice.value === 'claude-opus-5')?.label).toBe('Opus 5');
    expect(catalog.model.dispatch.build('claude-opus-5')).toBe('/model Claude Opus 5');
    // A model without effort tiers gets no effort pill at all.
    expect(catalog.optionsForModel('auto')).toEqual([]);
  });

  it('uses the effort delta when known and a deterministic boundary when unknown', () => {
    const choices = codexEffortChoices('gpt-5.5');
    expect(sessionChatBoundedKeySteps(choices, 'medium', 'xhigh', 'shift-down', 'shift-up')).toEqual([
      'shift-up',
      'shift-up',
    ]);
    expect(sessionChatBoundedKeySteps(choices, undefined, 'low', 'shift-down', 'shift-up')).toEqual([
      'shift-down',
      'shift-down',
      'shift-down',
    ]);
    expect(sessionChatBoundedKeySteps(choices, undefined, 'high', 'shift-down', 'shift-up')).toEqual([
      'shift-up',
      'shift-up',
      'shift-up',
      'shift-down',
    ]);
  });

  it('gives codex a plan-mode entry that types /plan', () => {
    const mode = catalogFor('codex')
      .optionsForModel('gpt-5.6-sol')
      .find((descriptor) => descriptor.id === 'mode');
    if (mode?.dispatch.kind !== 'toggle-command') {
      throw new Error('codex mode must dispatch a toggle command');
    }
    expect(mode.dispatch.command).toBe('/plan');
    expect(mode.actionLabel).toBe('Plan mode');
    // …and the picker offers it too, so both routes classify identically.
    expect(sessionChatSlashCommandsForAgent('codex').map((command) => command.name)).toEqual(
      expect.arrayContaining(['plan', 'permissions'])
    );
  });

  it('classifies every dispatched pill command as a command marker', () => {
    for (const agent of ['claude', 'codex']) {
      const catalog = catalogFor(agent);
      const names = [
        ...sessionChatSlashCommandsForAgent(agent).map((command) => command.name),
        ...sessionChatOptionCommandNames(agent),
      ];
      const commands: string[] = [];
      if (catalog.model.dispatch.kind === 'command') {
        commands.push(catalog.model.dispatch.build('opus'));
      }
      for (const choice of catalog.model.choices ?? []) {
        for (const descriptor of catalog.optionsForModel(choice.value)) {
          if (descriptor.dispatch.kind === 'command') {
            commands.push(descriptor.dispatch.build(descriptor.choices?.[0]?.value ?? ''));
          } else if (descriptor.dispatch.kind === 'toggle-command' || descriptor.dispatch.kind === 'agent-picker') {
            commands.push(descriptor.dispatch.command);
          }
        }
      }
      expect(commands.length).toBeGreaterThan(0);
      for (const command of commands) {
        expect([command, classifySessionChatSend(command, names)]).toEqual([command, 'command']);
      }
    }
  });

  it('reconciles a hand-typed command into the pills', () => {
    const catalog = catalogFor('claude');
    const seeded = seedSessionChatOptionState(catalog);
    const afterModel = reconcileSessionChatOptionsFromCommand(catalog, seeded, '/model opus');
    expect(afterModel.model).toMatchObject({ value: 'opus', source: 'dispatched' });
    // Dispatches are stamped so a later detection can tell a just-sent value
    // from one the agent has been running for a while.
    expect(Number.isFinite(Date.parse(afterModel.model?.dispatchedAt ?? ''))).toBe(true);
    const afterEffort = reconcileSessionChatOptionsFromCommand(catalog, afterModel, '  /effort   max  ');
    expect(afterEffort.effort).toMatchObject({ value: 'max', source: 'dispatched' });
    // Prose and unknown arguments leave the state untouched (identity).
    expect(reconcileSessionChatOptionsFromCommand(catalog, afterEffort, 'hello')).toBe(afterEffort);
    expect(reconcileSessionChatOptionsFromCommand(catalog, afterEffort, '/model nonsense')).toBe(afterEffort);
  });

  it('labels the options pill with known values joined by a middle dot', () => {
    const catalog = catalogFor('claude');
    let state = seedSessionChatOptionState(catalog);
    state = setSessionChatOptionValue(state, 'model', 'opus', 'dispatched');
    const descriptors = catalog.optionsForModel('opus');
    expect(sessionChatOptionsPillLabel(descriptors, state)).toBeNull();
    state = setSessionChatOptionValue(state, 'effort', 'xhigh', 'dispatched');
    expect(sessionChatOptionsPillLabel(descriptors, state)).toBe('Extra high');
    // Nothing known → the pill falls back to its own name.
    expect(sessionChatOptionsPillLabel(descriptors, {})).toBeNull();
  });

  it('keeps a stored value only when the catalog still offers it', () => {
    const catalog = catalogFor('claude');
    const dispatchedAt = new Date().toISOString();
    const kept = seedSessionChatOptionState(catalog, {
      model: { value: 'fable', source: 'dispatched', dispatchedAt },
    });
    expect(kept.model).toEqual({ value: 'fable', source: 'dispatched', dispatchedAt });
    const dropped = seedSessionChatOptionState(catalog, {
      model: { value: 'retired-model', source: 'dispatched', dispatchedAt },
    });
    expect(dropped.model).toBeUndefined();
    expect(
      seedSessionChatOptionState(catalog, {
        model: { value: 'fable', source: 'dispatched', dispatchedAt: '2020-01-01T00:00:00Z' },
      }).model
    ).toBeUndefined();
  });
});

/*
Detected options: gxserver reads structured transcript metadata and the agent's
own statusline. These lock the precedence rules — detected beats stale
dispatches, a pending dispatch survives a disagreeing probe for the grace
window, an agreeing probe CONFIRMS it, and raw labels render verbatim.
*/
describe('session chat detected options', () => {
  const at = (offsetMs: number): string => new Date(1_800_000_000_000 + offsetMs).toISOString();

  it("fills an unknown value and keeps the agent's raw label", () => {
    const catalog = catalogFor('claude');
    const seeded = seedSessionChatOptionState(catalog);
    const next = applySessionChatDetectedOptions(catalog, seeded, {
      model: { value: 'fable', label: 'Fable 5', source: 'transcript' },
      effort: { value: 'high', label: 'high', source: 'terminal' },
      detectedAt: at(0),
    });
    expect(next.model).toEqual({
      value: 'fable',
      source: 'detected',
      label: 'Fable 5',
      detectedSource: 'transcript',
      detectedAt: at(0),
    });
    // The real rendered model text beats the catalog's.
    expect(sessionChatOptionValueLabel(catalog.model, next)).toBe('Fable 5');
    // The terminal only echoed the effort's own token, so the catalog's
    // prettier label is kept.
    const [effort] = catalog.optionsForModel('fable');
    expect(sessionChatOptionValueLabel(effort!, next)).toBe('High');
  });

  it('shows a model version the catalog does not know verbatim', () => {
    const catalog = catalogFor('claude');
    const next = applySessionChatDetectedOptions(catalog, seedSessionChatOptionState(catalog), {
      model: { value: 'opus', label: 'Opus 4.5' },
      detectedAt: at(0),
    });
    expect(sessionChatOptionValueLabel(catalog.model, next)).toBe('Opus 4.5');
  });

  it("gives codex's picker-driven model pill a real value", () => {
    const catalog = catalogFor('codex');
    const next = applySessionChatDetectedOptions(catalog, seedSessionChatOptionState(catalog), {
      model: { value: 'gpt-5.6-sol', label: 'gpt-5.6-sol' },
      effort: { value: 'xhigh', label: 'xhigh' },
      detectedAt: at(0),
    });
    // A catalog id echoed verbatim renders with the catalog's label…
    expect(sessionChatOptionValueLabel(catalog.model, next)).toBe('GPT 5.6 Sol');
    const [effort] = catalog.optionsForModel('gpt-5.6-sol');
    expect(sessionChatOptionValueLabel(effort!, next)).toBe('Extra high');
    // …and an id it has never heard of still uses the terminal's spelling,
    // sentence-cased for the composer chips.
    const unknown = applySessionChatDetectedOptions(catalog, next, {
      model: { value: 'gpt-9.1-nova', label: 'gpt-9.1-nova' },
      detectedAt: at(1),
    });
    expect(sessionChatOptionValueLabel(catalog.model, unknown)).toBe('Gpt-9.1-nova');
  });

  it('confirms a pending dispatch it agrees with', () => {
    const catalog = catalogFor('claude');
    const dispatched = setSessionChatOptionValue(
      seedSessionChatOptionState(catalog),
      'model',
      'fable',
      'dispatched',
      () => 1_800_000_000_000
    );
    expect(dispatched.model?.source).toBe('dispatched');
    const confirmed = applySessionChatDetectedOptions(catalog, dispatched, {
      model: { value: 'fable', label: 'Fable 5' },
      detectedAt: at(2_000),
    });
    expect(confirmed.model).toEqual({
      value: 'fable',
      source: 'detected',
      label: 'Fable 5',
      detectedAt: at(2_000),
    });
  });

  it('keeps a just-dispatched value while a disagreeing probe is in the grace window', () => {
    const catalog = catalogFor('claude');
    const dispatched = setSessionChatOptionValue(
      seedSessionChatOptionState(catalog),
      'model',
      'fable',
      'dispatched',
      () => 1_800_000_000_000
    );
    // A cached read taken BEFORE the dispatch must not stomp it…
    expect(
      applySessionChatDetectedOptions(catalog, dispatched, {
        model: { value: 'sonnet', label: 'Sonnet 5' },
        detectedAt: at(-1_000),
      })
    ).toBe(dispatched);
    // …nor may a probe that caught the pre-repaint screen.
    expect(
      applySessionChatDetectedOptions(catalog, dispatched, {
        model: { value: 'sonnet', label: 'Sonnet 5' },
        detectedAt: at(SESSION_CHAT_DISPATCH_GRACE_MS - 1),
      })
    ).toBe(dispatched);
    // Past the window the terminal is believed: the agent did something else.
    const later = applySessionChatDetectedOptions(catalog, dispatched, {
      model: { value: 'sonnet', label: 'Sonnet 5' },
      detectedAt: at(SESSION_CHAT_DISPATCH_GRACE_MS + 1),
    });
    expect(later.model).toMatchObject({ value: 'sonnet', source: 'detected' });
  });

  it('is identity when the detection repeats and a no-op when nothing was detected', () => {
    const catalog = catalogFor('claude');
    const seeded = seedSessionChatOptionState(catalog);
    const detected = applySessionChatDetectedOptions(catalog, seeded, {
      model: { value: 'fable', label: 'Fable 5' },
      detectedAt: at(0),
    });
    expect(
      applySessionChatDetectedOptions(catalog, detected, {
        model: { value: 'fable', label: 'Fable 5' },
        detectedAt: at(0),
      })
    ).toBe(detected);
    expect(applySessionChatDetectedOptions(catalog, detected, null)).toBe(detected);
    expect(applySessionChatDetectedOptions(catalog, detected, undefined)).toBe(detected);
  });

  it('ignores an effort detection for a model that has no effort tiers', () => {
    const catalog = catalogFor('claude');
    const haiku = applySessionChatDetectedOptions(catalog, seedSessionChatOptionState(catalog), {
      model: { value: 'haiku', label: 'Haiku' },
      effort: { value: 'high', label: 'high' },
      detectedAt: at(0),
    });
    expect(haiku.model).toMatchObject({ value: 'haiku', source: 'detected' });
    // Haiku offers no effort option, so no unsupported value is retained.
    expect(haiku.effort).toBeUndefined();
  });

  it('does not reuse a persisted detection before gxserver reconfirms it', () => {
    const codex = catalogFor('codex');
    const reseeded = seedSessionChatOptionState(codex, {
      model: {
        value: 'gpt-9.1-nova',
        source: 'detected',
        label: 'gpt-9.1-nova',
        detectedAt: at(0),
      },
    });
    expect(reseeded).toEqual({});
  });
});
