// Composer footer session-option pills (upstream chat spec §1.2-§1.4 port).
// Ghost controls showing the current values only: Model and Effort are menu
// triggers, including Claude's permission mode selector backed by Shift+Tab.
// The category names live in tooltips / accessible labels. Controls
// that type into the TUI are disabled while the agent is working or a dispatch
// is in flight.
//
// Values are local (see session-chat-session-options.ts): a dispatch marks the
// value "dispatched", never "confirmed".

import { IconChevronDown } from '@tabler/icons-react';
import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatSendKey } from '../../shared/session-chat';
import { Button } from '../../components/ui/button';
import { cn } from '@/packages/components/utils';
import { getDefaultSidebarAgentByIcon } from '../../shared/sidebar-agents';
import { ProjectAgentLauncherIcon } from '../project-agent-launcher-icon';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu';
import {
  applySessionChatDetectedOptions,
  readStoredSessionChatOptions,
  reconcileSessionChatOptionsFromCommand,
  seedSessionChatOptionState,
  sessionChatBoundedKeySteps,
  sessionChatCyclicKeySteps,
  sessionChatOptionsPillLabel,
  sessionChatOptionTracksValue,
  sessionChatOptionValueLabel,
  sessionChatSessionOptionCatalog,
  SESSION_CHAT_DETECTED_HINT,
  SESSION_CHAT_DISPATCHED_HINT,
  SESSION_CHAT_TRANSCRIPT_HINT,
  setSessionChatOptionValue,
  writeStoredSessionChatOptions,
  type SessionChatDetectedOptionInput,
  type SessionChatOptionDescriptor,
  type SessionChatOptionState,
  type SessionChatSessionOptionCatalog,
} from './session-chat-session-options';

type PillSkeleton = 'model' | 'options' | 'combined' | 'mode';

function pillLoadingText(skeleton: PillSkeleton): string {
  if (skeleton === 'options') {
    return 'Reading options…';
  }
  if (skeleton === 'mode') {
    return 'Reading mode…';
  }
  return 'Reading model…';
}

export interface SessionChatSessionOptionsController {
  catalog: SessionChatSessionOptionCatalog | null;
  state: SessionChatOptionState;
  /** Descriptors of the Options pill for the currently selected model. */
  optionDescriptors: readonly SessionChatOptionDescriptor[];
  recordDispatched: (descriptorId: string, value: string) => void;
  /** A command the user typed themselves reconciles the pills (§1.4). */
  reconcileTypedCommand: (text: string) => void;
  /** What gxserver confirmed from the agent transcript or terminal. */
  applyDetected: (detected: SessionChatDetectedOptionInput | null | undefined) => void;
}

/**
 * Owns the local option truth for one session. Lives in the view (not the
 * pills) so the composer's send path can reconcile a hand-typed `/model`.
 */
export function useSessionChatSessionOptions({
  agent,
  sessionKey,
}: {
  agent: string | null | undefined;
  sessionKey?: string;
}): SessionChatSessionOptionsController {
  const catalog = useMemo(() => sessionChatSessionOptionCatalog(agent), [agent]);
  const [state, setState] = useState<SessionChatOptionState>({});

  // Reseed whenever the session or the agent changes: the stored values are
  // per (session, agent), and a different agent has a different catalog.
  useEffect(() => {
    setState(catalog ? seedSessionChatOptionState(catalog, readStoredSessionChatOptions(sessionKey)) : {});
  }, [catalog, sessionKey]);

  const optionDescriptors = useMemo(() => {
    if (!catalog) {
      return [];
    }
    const modelValue = state[catalog.model.id]?.value ?? catalog.model.defaultValue ?? '';
    return catalog.optionsForModel(modelValue);
  }, [catalog, state]);

  const persist = useCallback(
    (next: SessionChatOptionState) => {
      writeStoredSessionChatOptions(sessionKey, next);
      return next;
    },
    [sessionKey]
  );

  const recordDispatched = useCallback(
    (descriptorId: string, value: string) => {
      setState((current) => persist(setSessionChatOptionValue(current, descriptorId, value, 'dispatched')));
    },
    [persist]
  );

  const reconcileTypedCommand = useCallback(
    (text: string) => {
      if (!catalog) {
        return;
      }
      setState((current) => {
        const next = reconcileSessionChatOptionsFromCommand(catalog, current, text);
        return next === current ? current : persist(next);
      });
    },
    [catalog, persist]
  );

  const applyDetected = useCallback(
    (detected: SessionChatDetectedOptionInput | null | undefined) => {
      if (!catalog || !detected) {
        return;
      }
      setState((current) => {
        const next = applySessionChatDetectedOptions(catalog, current, detected);
        return next === current ? current : persist(next);
      });
    },
    [catalog, persist]
  );

  return {
    applyDetected,
    catalog,
    optionDescriptors,
    recordDispatched,
    reconcileTypedCommand,
    state,
  };
}

export interface SessionChatSessionOptionPillsProps {
  controller: SessionChatSessionOptionsController;
  /** True while the agent is working: every pill is disabled (§1.2). */
  isWorking: boolean;
  /** False when input is held elsewhere. */
  canSend: boolean;
  /** True when the transport can inject raw keystrokes for agent TUI controls. */
  canSendKey: boolean;
  /*
  CDXC:SessionChatScreenProbed 2026-08-22: true once gxserver has actually read
  this session's screen. Until then a pill with no value has not been LOOKED
  for yet and shows a skeleton; once it is true, a pill with no value means the
  agent's screen names none, and the category word is the honest label.
  */
  screenProbed: boolean;
  onDispatchCommand: (command: string) => Promise<void>;
  onDispatchKey: (key: SessionChatSendKey, marker: string) => Promise<void>;
  /** Holds the whole composer disabled while a cyclic TUI switch is running. */
  onSwitchingChange?: (switching: boolean) => void;
  /** Agent-picker options flip the pane to the terminal after typing. */
  onSwitchToTerminal?: () => void;
}

/*
CDXC:SessionChatScreenProbed 2026-08-22:
`skeleton` names which placeholder width to use while gxserver has not read the
session's screen yet. The bar replaces only the LABEL — same button, same
chevron, same padding — so resolving a value swaps text in without moving the
composer row. The trigger is disabled while it shows, because the menu would be
offering choices against an unknown current value.
*/
function PillTrigger({
  ariaLabel,
  className,
  disabled,
  icon,
  iconOnly = false,
  label,
  skeleton,
  title,
}: {
  ariaLabel: string;
  className?: string;
  disabled: boolean;
  icon?: ReactNode;
  iconOnly?: boolean;
  label: string;
  skeleton?: PillSkeleton;
  title: string;
}) {
  // A skeleton has no value to name, so the tooltip and the accessible name
  // say what is happening instead of reading out the category word.
  const loadingText = skeleton ? pillLoadingText(skeleton) : '';
  const resolvedIconOnly = iconOnly && skeleton === undefined;
  return (
    <AppTooltip content={skeleton ? loadingText : title}>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={skeleton ? loadingText : ariaLabel}
            className={cn('ghostex-chat-footer-control max-w-40 rounded-full text-muted-foreground', className)}
            disabled={disabled || skeleton !== undefined}
            size={resolvedIconOnly ? 'icon-xs' : 'xs'}
            variant='ghost'
          />
        }
      >
        {icon}
        {skeleton ? (
          <span aria-hidden='true' className='ghostex-chat-pill-skeleton' data-pill={skeleton} />
        ) : resolvedIconOnly ? null : (
          <span className='truncate'>{label}</span>
        )}
        {resolvedIconOnly ? null : <IconChevronDown aria-hidden='true' className='size-3 shrink-0' stroke={2} />}
      </DropdownMenuTrigger>
    </AppTooltip>
  );
}

/*
The read-only twin of PillTrigger for `terminal-handoff` options: same chip, no
chevron, because there is no menu behind it — the click hands the user to the
terminal. It stays enabled while the agent is working: switching panes types
nothing at the TUI, which is the only reason the dispatching pills go dead.
*/
function PillButton({
  ariaLabel,
  className,
  disabled,
  icon,
  label,
  onClick,
  skeleton,
  title,
}: {
  ariaLabel: string;
  className?: string;
  disabled: boolean;
  icon?: ReactNode;
  label: string;
  onClick: () => void;
  skeleton?: PillSkeleton;
  title: ReactNode;
}) {
  const loadingText = skeleton ? pillLoadingText(skeleton) : '';
  return (
    <AppTooltip content={skeleton ? loadingText : title}>
      <Button
        aria-label={skeleton ? loadingText : ariaLabel}
        className={cn('ghostex-chat-footer-control max-w-40 rounded-full text-muted-foreground', className)}
        disabled={disabled || skeleton !== undefined}
        onClick={onClick}
        size='xs'
        variant='ghost'
      >
        {icon}
        {skeleton ? (
          <span aria-hidden='true' className='ghostex-chat-pill-skeleton' data-pill={skeleton} />
        ) : label ? (
          <span className='truncate'>{label}</span>
        ) : null}
      </Button>
    </AppTooltip>
  );
}

const CLAUDE_PERMISSION_MODE_ICON_KIND: Readonly<Record<string, 'advance' | 'pause'>> = {
  'accept-edits': 'advance',
  auto: 'advance',
  bypass: 'advance',
  manual: 'pause',
  plan: 'pause',
};

function ClaudePermissionModeIcon({ mode }: { mode: string }) {
  const kind = CLAUDE_PERMISSION_MODE_ICON_KIND[mode];
  if (!kind) {
    return null;
  }

  return (
    <svg
      aria-hidden='true'
      className='ghostex-chat-mode-icon'
      data-icon='inline-start'
      data-mode={mode}
      viewBox='0 0 16 14'
    >
      {kind === 'advance' ? (
        <path d='M1 2.1 6.9 7 1 11.9V2.1Zm7.1 0L14 7l-5.9 4.9V2.1Z' fill='currentColor' />
      ) : (
        <>
          <rect fill='currentColor' height='9.8' rx='0.7' width='3.2' x='2.1' y='2.1' />
          <rect fill='currentColor' height='9.8' rx='0.7' width='3.2' x='9.2' y='2.1' />
        </>
      )}
    </svg>
  );
}

export function SessionChatSessionOptionPills({
  canSend,
  canSendKey,
  controller,
  isWorking,
  onDispatchCommand,
  onDispatchKey,
  onSwitchingChange,
  onSwitchToTerminal,
  screenProbed,
}: SessionChatSessionOptionPillsProps) {
  const [dispatchingId, setDispatchingId] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const { catalog, optionDescriptors, recordDispatched, state } = controller;
  const visibleOptions = useMemo(
    () =>
      optionDescriptors.filter(
        (descriptor) =>
          (descriptor.dispatch.kind !== 'key' &&
            descriptor.dispatch.kind !== 'bounded-key-steps' &&
            descriptor.dispatch.kind !== 'cyclic-key-steps') ||
          canSendKey
      ),
    [canSendKey, optionDescriptors]
  );

  const dispatch = useCallback(
    (descriptor: SessionChatOptionDescriptor, value?: string): void => {
      setDispatchingId(descriptor.id);
      setFailure(null);
      const run = async (): Promise<void> => {
        const { dispatch: delivery } = descriptor;
        if (delivery.kind === 'command') {
          await onDispatchCommand(delivery.build(value ?? ''));
          if (value !== undefined) {
            recordDispatched(descriptor.id, value);
          }
          return;
        }
        if (delivery.kind === 'toggle-command') {
          await onDispatchCommand(delivery.command);
          return;
        }
        if (delivery.kind === 'agent-picker') {
          await onDispatchCommand(delivery.command);
          onSwitchToTerminal?.();
          return;
        }
        if (delivery.kind === 'terminal-handoff') {
          // Nothing is typed: the agent's own picker owns the change.
          onSwitchToTerminal?.();
          return;
        }
        if (delivery.kind === 'bounded-key-steps') {
          const keys = sessionChatBoundedKeySteps(
            descriptor.choices ?? [],
            state[descriptor.id]?.value,
            value ?? '',
            delivery.decreaseKey,
            delivery.increaseKey
          );
          for (const key of keys) {
            await onDispatchKey(key, '');
          }
          if (value !== undefined) {
            recordDispatched(descriptor.id, value);
          }
          return;
        }
        if (delivery.kind === 'cyclic-key-steps') {
          const keys = sessionChatCyclicKeySteps(
            descriptor.choices ?? [],
            state[descriptor.id]?.value,
            value ?? '',
            delivery.key
          );
          if (value === undefined || keys.length === 0) {
            return;
          }
          onSwitchingChange?.(true);
          recordDispatched(descriptor.id, value);
          const minimumSwitchTime = new Promise<void>((resolve) => window.setTimeout(resolve, 160));
          try {
            for (const key of keys) {
              await onDispatchKey(key, '');
            }
            await minimumSwitchTime;
          } finally {
            onSwitchingChange?.(false);
          }
          return;
        }
        await onDispatchKey(delivery.key, delivery.marker);
      };
      void run()
        .catch(() => {
          if (mountedRef.current) {
            setFailure('Could not update option');
          }
        })
        .finally(() => {
          if (mountedRef.current) {
            setDispatchingId(null);
          }
        });
    },
    [onDispatchCommand, onDispatchKey, onSwitchToTerminal, onSwitchingChange, recordDispatched, state]
  );

  if (!catalog) {
    return null;
  }

  const disabled = isWorking || !canSend || dispatchingId !== null;

  const menuRows = (descriptor: SessionChatOptionDescriptor): ReactNode => {
    const current = state[descriptor.id];
    if (sessionChatOptionTracksValue(descriptor)) {
      return (
        <DropdownMenuRadioGroup
          onValueChange={(value) => {
            if (typeof value === 'string' && value !== current?.value) {
              dispatch(descriptor, value);
            }
          }}
          value={current?.value ?? ''}
        >
          {(descriptor.choices ?? []).map((choice) => (
            <DropdownMenuRadioItem className='rounded-md' key={choice.value} value={choice.value}>
              <span className='grid min-w-0 gap-0.5'>
                <span className='truncate'>{choice.label}</span>
                {choice.description ? (
                  <span className='text-xs font-normal text-muted-foreground'>{choice.description}</span>
                ) : null}
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      );
    }
    return (
      <DropdownMenuItem className='rounded-md whitespace-nowrap' onClick={() => dispatch(descriptor)}>
        {descriptor.actionLabel ?? descriptor.label}
      </DropdownMenuItem>
    );
  };

  const modelAgent = getDefaultSidebarAgentByIcon(catalog.modelIcon);
  const modelIcon = (
    <span className='contents' data-icon='inline-start'>
      <ProjectAgentLauncherIcon agent={modelAgent ? { ...modelAgent, isDefault: true } : undefined} colorMode='brand' />
    </span>
  );
  const modelLabel = sessionChatOptionValueLabel(catalog.model, state);
  const modeButton = visibleOptions.find(
    (descriptor) =>
      descriptor.category === 'mode' &&
      descriptor.dispatch.kind === 'cyclic-key-steps' &&
      descriptor.dispatch.key === 'shift-tab'
  );
  const menuOptions = modeButton ? visibleOptions.filter((descriptor) => descriptor !== modeButton) : visibleOptions;
  const modeLabel = modeButton ? sessionChatOptionValueLabel(modeButton, state) : null;
  const modeValue = modeButton ? state[modeButton.id]?.value : undefined;
  const modeIcon = modeValue ? <ClaudePermissionModeIcon mode={modeValue} /> : null;
  const optionsLabel = sessionChatOptionsPillLabel(menuOptions, state);
  const combinedPickerEffort = menuOptions.find(
    (descriptor) => descriptor.id === 'effort' && descriptor.dispatch.kind === 'agent-picker'
  );
  const usesCombinedAgentPicker = catalog.model.dispatch.kind === 'agent-picker' && combinedPickerEffort !== undefined;
  const modelTitle = modelLabel ? `${catalog.model.label} ${modelLabel}` : catalog.model.label;
  const optionsTitle = optionsLabel ? `Options ${optionsLabel}` : 'Options';
  const modeTitle = modeLabel ? `Mode ${modeLabel}` : 'Mode';
  /*
  An unconfirmed dispatch is the weaker claim, so it wins the tooltip while any
  shown value is still only "sent". Once every shown value has agent-owned
  evidence, the hint names that evidence source.
  */
  const hintFor = (descriptors: readonly SessionChatOptionDescriptor[]): string | null => {
    const sources = descriptors.map((descriptor) => state[descriptor.id]?.source);
    if (sources.includes('dispatched')) {
      return SESSION_CHAT_DISPATCHED_HINT;
    }
    const detectedValues = descriptors
      .map((descriptor) => state[descriptor.id])
      .filter((value) => value?.source === 'detected');
    if (detectedValues.some((value) => value?.detectedSource === 'terminal')) {
      return SESSION_CHAT_DETECTED_HINT;
    }
    if (detectedValues.some((value) => value?.detectedSource === 'transcript')) {
      return SESSION_CHAT_TRANSCRIPT_HINT;
    }
    return detectedValues.length > 0 ? SESSION_CHAT_DETECTED_HINT : null;
  };
  const tooltipText = (title: string, hint: string | null): string => hint ?? title;
  const modelHint = hintFor([catalog.model]);
  const optionsHint = hintFor(menuOptions);
  /*
  CDXC:SessionChatScreenProbed 2026-08-22: a pill is "still loading" only while
  it has NO value AND gxserver has not read the screen yet. Once the screen has
  been read, no value is an answer — this agent's screen names none — and the
  category word is the honest label rather than a spinner that never ends. A
  locally dispatched value also counts as a value, so a pill the user just set
  never falls back to a skeleton while the agent repaints.
  */
  const skeletonFor = (pill: PillSkeleton, value: string | null | undefined): PillSkeleton | undefined =>
    !screenProbed && !value ? pill : undefined;

  /*
  Read-only pills (grok): both values come from the statusline gxserver reads,
  and either pill hands the user to the terminal — where the host also raises
  the "set it in the CLI, then come back" toast — instead of opening a menu
  this side cannot honour.
  */
  if (catalog.model.dispatch.kind === 'terminal-handoff') {
    const handoffTitle = (category: string, value: string | null): string =>
      value ? `${category} ${value} — change it in the CLI` : `${category} — set it in the CLI`;
    const modelHandoffTitle = handoffTitle(catalog.model.label, modelLabel);
    const optionsHandoffTitle = handoffTitle(
      visibleOptions.length === 1 ? (visibleOptions[0]?.label ?? 'Options') : 'Options',
      optionsLabel
    );
    return (
      <>
        <PillButton
          ariaLabel={modelHandoffTitle}
          className='ghostex-chat-model-pill'
          disabled={onSwitchToTerminal === undefined}
          icon={modelIcon}
          label={modelLabel ?? catalog.model.label}
          onClick={() => onSwitchToTerminal?.()}
          skeleton={skeletonFor('model', modelLabel)}
          title={modelHandoffTitle}
        />
        {visibleOptions.length > 0 ? (
          <PillButton
            ariaLabel={optionsHandoffTitle}
            className='ghostex-chat-options-pill'
            disabled={onSwitchToTerminal === undefined}
            label={optionsLabel ?? 'Options'}
            onClick={() => onSwitchToTerminal?.()}
            skeleton={skeletonFor('options', optionsLabel)}
            title={optionsHandoffTitle}
          />
        ) : null}
      </>
    );
  }

  if (usesCombinedAgentPicker) {
    const effortLabel = sessionChatOptionValueLabel(combinedPickerEffort, state);
    const selectedLabel = [modelLabel, effortLabel].filter(Boolean).join(' · ');
    const combinedLabel = selectedLabel || 'Model & Effort';
    const combinedTitle = selectedLabel ? `Model & Effort ${selectedLabel}` : 'Model & Effort';
    const combinedHint = hintFor([catalog.model, combinedPickerEffort]);

    return (
      <>
        {failure ? (
          <AppTooltip content={failure}>
            <span className='max-w-32 truncate text-[11px] text-destructive/80' role='status'>
              {failure}
            </span>
          </AppTooltip>
        ) : null}
        <DropdownMenu>
          <PillTrigger
            ariaLabel={combinedTitle}
            className='ghostex-chat-model-pill'
            disabled={disabled}
            icon={modelIcon}
            label={combinedLabel}
            skeleton={skeletonFor('combined', selectedLabel)}
            title={tooltipText(combinedTitle, combinedHint)}
          />
          <DropdownMenuContent align='start' className='ghostex-session-chat-popup w-60 rounded-xl [--radius:0.625rem]'>
            <DropdownMenuItem className='rounded-md' onClick={() => dispatch(catalog.model)}>
              Select Model &amp; Effort in CLI
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </>
    );
  }

  return (
    <>
      {failure ? (
        <AppTooltip content={failure}>
          <span className='max-w-32 truncate text-[11px] text-destructive/80' role='status'>
            {failure}
          </span>
        </AppTooltip>
      ) : null}
      <DropdownMenu>
        <PillTrigger
          ariaLabel={modelTitle}
          className='ghostex-chat-model-pill'
          disabled={disabled}
          icon={modelIcon}
          label={modelLabel ?? catalog.model.label}
          skeleton={skeletonFor('model', modelLabel)}
          title={tooltipText(modelTitle, modelHint)}
        />
        <DropdownMenuContent align='end' className='ghostex-session-chat-popup w-64 rounded-xl [--radius:0.625rem]'>
          {/* Base UI's GroupLabel throws outside a Menu.Group context. */}
          <DropdownMenuGroup>
            <DropdownMenuLabel>{catalog.model.label}</DropdownMenuLabel>
            {catalog.model.description ? (
              <DropdownMenuLabel className='whitespace-normal pt-0'>{catalog.model.description}</DropdownMenuLabel>
            ) : null}
            {menuRows(catalog.model)}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      {menuOptions.length > 0 ? (
        <DropdownMenu>
          <PillTrigger
            ariaLabel={optionsTitle}
            className='ghostex-chat-options-pill'
            disabled={disabled}
            label={optionsLabel ?? 'Options'}
            skeleton={skeletonFor('options', optionsLabel)}
            title={tooltipText(optionsTitle, optionsHint)}
          />
          <DropdownMenuContent align='end' className='ghostex-session-chat-popup w-60 rounded-xl [--radius:0.625rem]'>
            {menuOptions.map((descriptor, index) => (
              <Fragment key={descriptor.id}>
                {index > 0 ? <DropdownMenuSeparator /> : null}
                {/* Base UI's GroupLabel throws outside a Menu.Group context. */}
                <DropdownMenuGroup>
                  <DropdownMenuLabel>{descriptor.label}</DropdownMenuLabel>
                  {descriptor.description ? (
                    <DropdownMenuLabel className='whitespace-normal pt-0'>{descriptor.description}</DropdownMenuLabel>
                  ) : null}
                  {menuRows(descriptor)}
                </DropdownMenuGroup>
              </Fragment>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
      {modeButton ? (
        <DropdownMenu>
          <PillTrigger
            ariaLabel={modeTitle}
            className='ghostex-chat-mode-pill ghostex-chat-mode-pill-icon-only'
            disabled={disabled || modeValue === undefined}
            icon={modeIcon}
            iconOnly
            label={modeLabel ?? modeButton.label}
            skeleton={skeletonFor('mode', modeLabel)}
            title={tooltipText(modeTitle, hintFor([modeButton]))}
          />
          <DropdownMenuContent align='end' className='ghostex-session-chat-popup w-60 rounded-xl [--radius:0.625rem]'>
            <DropdownMenuGroup>
              <DropdownMenuLabel>{modeButton.label}</DropdownMenuLabel>
              {modeButton.description ? (
                <DropdownMenuLabel className='whitespace-normal pt-0'>{modeButton.description}</DropdownMenuLabel>
              ) : null}
              {menuRows(modeButton)}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </>
  );
}
