// Composer footer session-option pills (upstream chat spec §1.2-§1.4 port).
// Two ghost pills — Model and Options — showing the VALUE only; the category
// name lives in the tooltip/aria label ("Effort High"). Both are disabled
// while the agent is working or while a dispatch is in flight, because every
// dispatch types into the same TUI input line the composer uses.
//
// Values are local (see session-chat-session-options.ts): a dispatch marks the
// value "dispatched", never "confirmed".

import { IconChevronDown } from "@tabler/icons-react";
import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AppTooltip } from "../app-tooltip";
import type { SessionChatSendKey } from "../../shared/session-chat";
import { Button } from "../../components/ui/button";
import { cn } from "@/packages/components/utils";
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
} from "../../components/ui/dropdown-menu";
import {
  applySessionChatDetectedOptions,
  readStoredSessionChatOptions,
  reconcileSessionChatOptionsFromCommand,
  seedSessionChatOptionState,
  sessionChatBoundedKeySteps,
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
} from "./session-chat-session-options";

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
    setState(
      catalog ? seedSessionChatOptionState(catalog, readStoredSessionChatOptions(sessionKey)) : {},
    );
  }, [catalog, sessionKey]);

  const optionDescriptors = useMemo(() => {
    if (!catalog) {
      return [];
    }
    const modelValue = state[catalog.model.id]?.value ?? catalog.model.defaultValue ?? "";
    return catalog.optionsForModel(modelValue);
  }, [catalog, state]);

  const persist = useCallback(
    (next: SessionChatOptionState) => {
      writeStoredSessionChatOptions(sessionKey, next);
      return next;
    },
    [sessionKey],
  );

  const recordDispatched = useCallback(
    (descriptorId: string, value: string) => {
      setState((current) =>
        persist(setSessionChatOptionValue(current, descriptorId, value, "dispatched")),
      );
    },
    [persist],
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
    [catalog, persist],
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
    [catalog, persist],
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
  label,
  skeleton,
  title,
}: {
  ariaLabel: string;
  className?: string;
  disabled: boolean;
  label: string;
  skeleton?: "model" | "options" | "combined";
  title: string;
}) {
  // A skeleton has no value to name, so the tooltip and the accessible name
  // say what is happening instead of reading out the category word.
  const loadingText = skeleton === "options" ? "Reading options…" : "Reading model…";
  return (
    <AppTooltip content={skeleton ? loadingText : title}>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={skeleton ? loadingText : ariaLabel}
            className={cn(
              "ghostex-chat-footer-control max-w-40 rounded-full text-muted-foreground",
              className,
            )}
            disabled={disabled || skeleton !== undefined}
            size="xs"
            variant="ghost"
          />
        }
      >
        {skeleton ? (
          <span
            aria-hidden="true"
            className="ghostex-chat-pill-skeleton"
            data-pill={skeleton}
          />
        ) : (
          <span className="truncate">{label}</span>
        )}
        <IconChevronDown aria-hidden="true" className="size-3 shrink-0" stroke={2} />
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
  label,
  onClick,
  skeleton,
  title,
}: {
  ariaLabel: string;
  className?: string;
  disabled: boolean;
  label: string;
  onClick: () => void;
  skeleton?: "model" | "options" | "combined";
  title: string;
}) {
  const loadingText = skeleton === "options" ? "Reading options…" : "Reading model…";
  return (
    <AppTooltip content={skeleton ? loadingText : title}>
      <Button
        aria-label={skeleton ? loadingText : ariaLabel}
        className={cn(
          "ghostex-chat-footer-control max-w-40 rounded-full text-muted-foreground",
          className,
        )}
        disabled={disabled || skeleton !== undefined}
        onClick={onClick}
        size="xs"
        variant="ghost"
      >
        {skeleton ? (
          <span
            aria-hidden="true"
            className="ghostex-chat-pill-skeleton"
            data-pill={skeleton}
          />
        ) : (
          <span className="truncate">{label}</span>
        )}
      </Button>
    </AppTooltip>
  );
}

export function SessionChatSessionOptionPills({
  canSend,
  canSendKey,
  controller,
  isWorking,
  onDispatchCommand,
  onDispatchKey,
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
          (descriptor.dispatch.kind !== "key" &&
            descriptor.dispatch.kind !== "bounded-key-steps") ||
          canSendKey,
      ),
    [canSendKey, optionDescriptors],
  );

  const dispatch = useCallback(
    (descriptor: SessionChatOptionDescriptor, value?: string): void => {
      setDispatchingId(descriptor.id);
      setFailure(null);
      const run = async (): Promise<void> => {
        const { dispatch: delivery } = descriptor;
        if (delivery.kind === "command") {
          await onDispatchCommand(delivery.build(value ?? ""));
          if (value !== undefined) {
            recordDispatched(descriptor.id, value);
          }
          return;
        }
        if (delivery.kind === "toggle-command") {
          await onDispatchCommand(delivery.command);
          return;
        }
        if (delivery.kind === "agent-picker") {
          await onDispatchCommand(delivery.command);
          onSwitchToTerminal?.();
          return;
        }
        if (delivery.kind === "terminal-handoff") {
          // Nothing is typed: the agent's own picker owns the change.
          onSwitchToTerminal?.();
          return;
        }
        if (delivery.kind === "bounded-key-steps") {
          const keys = sessionChatBoundedKeySteps(
            descriptor.choices ?? [],
            state[descriptor.id]?.value,
            value ?? "",
            delivery.decreaseKey,
            delivery.increaseKey,
          );
          for (const key of keys) {
            await onDispatchKey(key, "");
          }
          if (value !== undefined) {
            recordDispatched(descriptor.id, value);
          }
          return;
        }
        await onDispatchKey(delivery.key, delivery.marker);
      };
      void run()
        .catch(() => {
          if (mountedRef.current) {
            setFailure("Could not update option");
          }
        })
        .finally(() => {
          if (mountedRef.current) {
            setDispatchingId(null);
          }
        });
    },
    [onDispatchCommand, onDispatchKey, onSwitchToTerminal, recordDispatched, state],
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
            if (typeof value === "string" && value !== current?.value) {
              dispatch(descriptor, value);
            }
          }}
          value={current?.value ?? ""}
        >
          {(descriptor.choices ?? []).map((choice) => (
            <DropdownMenuRadioItem
              className="rounded-md"
              key={choice.value}
              value={choice.value}
            >
              <span className="grid min-w-0 gap-0.5">
                <span className="truncate">{choice.label}</span>
                {choice.description ? (
                  <span className="text-xs font-normal text-muted-foreground">
                    {choice.description}
                  </span>
                ) : null}
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      );
    }
    return (
      <DropdownMenuItem
        className="rounded-md whitespace-nowrap"
        onClick={() => dispatch(descriptor)}
      >
        {descriptor.actionLabel ?? descriptor.label}
      </DropdownMenuItem>
    );
  };

  const modelLabel = sessionChatOptionValueLabel(catalog.model, state);
  const optionsLabel = sessionChatOptionsPillLabel(visibleOptions, state);
  const combinedPickerEffort = visibleOptions.find(
    (descriptor) =>
      descriptor.id === "effort" && descriptor.dispatch.kind === "agent-picker",
  );
  const usesCombinedAgentPicker =
    catalog.model.dispatch.kind === "agent-picker" && combinedPickerEffort !== undefined;
  const modelTitle = modelLabel ? `${catalog.model.label} ${modelLabel}` : catalog.model.label;
  const optionsTitle = optionsLabel ? `Options ${optionsLabel}` : "Options";
  /*
  An unconfirmed dispatch is the weaker claim, so it wins the tooltip while any
  shown value is still only "sent". Once every shown value has agent-owned
  evidence, the hint names that evidence source.
  */
  const hintFor = (descriptors: readonly SessionChatOptionDescriptor[]): string | null => {
    const sources = descriptors.map((descriptor) => state[descriptor.id]?.source);
    if (sources.includes("dispatched")) {
      return SESSION_CHAT_DISPATCHED_HINT;
    }
    const detectedValues = descriptors
      .map((descriptor) => state[descriptor.id])
      .filter((value) => value?.source === "detected");
    if (detectedValues.some((value) => value?.detectedSource === "terminal")) {
      return SESSION_CHAT_DETECTED_HINT;
    }
    if (detectedValues.some((value) => value?.detectedSource === "transcript")) {
      return SESSION_CHAT_TRANSCRIPT_HINT;
    }
    return detectedValues.length > 0 ? SESSION_CHAT_DETECTED_HINT : null;
  };
  const tooltipText = (title: string, hint: string | null): string => hint ?? title;
  const modelHint = hintFor([catalog.model]);
  const optionsHint = hintFor(visibleOptions);
  /*
  CDXC:SessionChatScreenProbed 2026-08-22: a pill is "still loading" only while
  it has NO value AND gxserver has not read the screen yet. Once the screen has
  been read, no value is an answer — this agent's screen names none — and the
  category word is the honest label rather than a spinner that never ends. A
  locally dispatched value also counts as a value, so a pill the user just set
  never falls back to a skeleton while the agent repaints.
  */
  const skeletonFor = (
    pill: "model" | "options" | "combined",
    value: string | null | undefined,
  ): "model" | "options" | "combined" | undefined =>
    !screenProbed && !value ? pill : undefined;

  /*
  Read-only pills (grok): both values come from the statusline gxserver reads,
  and either pill hands the user to the terminal — where the host also raises
  the "set it in the CLI, then come back" toast — instead of opening a menu
  this side cannot honour.
  */
  if (catalog.model.dispatch.kind === "terminal-handoff") {
    const handoffTitle = (category: string, value: string | null): string =>
      value ? `${category} ${value} — change it in the CLI` : `${category} — set it in the CLI`;
    const modelHandoffTitle = handoffTitle(catalog.model.label, modelLabel);
    const optionsHandoffTitle = handoffTitle(
      visibleOptions.length === 1 ? (visibleOptions[0]?.label ?? "Options") : "Options",
      optionsLabel,
    );
    return (
      <>
        <PillButton
          ariaLabel={modelHandoffTitle}
          className="ghostex-chat-model-pill"
          disabled={onSwitchToTerminal === undefined}
          label={modelLabel ?? catalog.model.label}
          onClick={() => onSwitchToTerminal?.()}
          skeleton={skeletonFor("model", modelLabel)}
          title={modelHandoffTitle}
        />
        {visibleOptions.length > 0 ? (
          <PillButton
            ariaLabel={optionsHandoffTitle}
            className="ghostex-chat-options-pill"
            disabled={onSwitchToTerminal === undefined}
            label={optionsLabel ?? "Options"}
            onClick={() => onSwitchToTerminal?.()}
            skeleton={skeletonFor("options", optionsLabel)}
            title={optionsHandoffTitle}
          />
        ) : null}
      </>
    );
  }

  if (usesCombinedAgentPicker) {
    const effortLabel = sessionChatOptionValueLabel(combinedPickerEffort, state);
    const selectedLabel = [modelLabel, effortLabel].filter(Boolean).join(" · ");
    const combinedLabel = selectedLabel || "Model & Effort";
    const combinedTitle = selectedLabel
      ? `Model & Effort ${selectedLabel}`
      : "Model & Effort";
    const combinedHint = hintFor([catalog.model, combinedPickerEffort]);

    return (
      <>
        {failure ? (
          <AppTooltip content={failure}>
            <span className="max-w-32 truncate text-[11px] text-destructive/80" role="status">
              {failure}
            </span>
          </AppTooltip>
        ) : null}
        <DropdownMenu>
          <PillTrigger
            ariaLabel={combinedTitle}
            className="ghostex-chat-model-pill"
            disabled={disabled}
            label={combinedLabel}
            skeleton={skeletonFor("combined", selectedLabel)}
            title={tooltipText(combinedTitle, combinedHint)}
          />
          <DropdownMenuContent
            align="start"
            className="ghostex-session-chat-popup w-60 rounded-xl [--radius:0.625rem]"
          >
            <DropdownMenuItem
              className="rounded-md"
              onClick={() => dispatch(catalog.model)}
            >
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
          <span className="max-w-32 truncate text-[11px] text-destructive/80" role="status">
            {failure}
          </span>
        </AppTooltip>
      ) : null}
      <DropdownMenu>
        <PillTrigger
          ariaLabel={modelTitle}
          className="ghostex-chat-model-pill"
          disabled={disabled}
          label={modelLabel ?? catalog.model.label}
          skeleton={skeletonFor("model", modelLabel)}
          title={tooltipText(modelTitle, modelHint)}
        />
        <DropdownMenuContent
          align="end"
          className="ghostex-session-chat-popup w-64 rounded-xl [--radius:0.625rem]"
>
          {/* Base UI's GroupLabel throws outside a Menu.Group context. */}
          <DropdownMenuGroup>
            <DropdownMenuLabel>{catalog.model.label}</DropdownMenuLabel>
            {catalog.model.description ? (
              <DropdownMenuLabel className="whitespace-normal pt-0">
                {catalog.model.description}
              </DropdownMenuLabel>
            ) : null}
            {menuRows(catalog.model)}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      {visibleOptions.length > 0 ? (
        <DropdownMenu>
          <PillTrigger
            ariaLabel={optionsTitle}
            className="ghostex-chat-options-pill"
            disabled={disabled}
            label={optionsLabel ?? "Options"}
            skeleton={skeletonFor("options", optionsLabel)}
            title={tooltipText(optionsTitle, optionsHint)}
          />
          <DropdownMenuContent
            align="end"
            className="ghostex-session-chat-popup w-60 rounded-xl [--radius:0.625rem]"
>
            {visibleOptions.map((descriptor, index) => (
              <Fragment key={descriptor.id}>
                {index > 0 ? <DropdownMenuSeparator /> : null}
                {/* Base UI's GroupLabel throws outside a Menu.Group context. */}
                <DropdownMenuGroup>
                  <DropdownMenuLabel>{descriptor.label}</DropdownMenuLabel>
                  {descriptor.description ? (
                    <DropdownMenuLabel className="whitespace-normal pt-0">
                      {descriptor.description}
                    </DropdownMenuLabel>
                  ) : null}
                  {menuRows(descriptor)}
                </DropdownMenuGroup>
              </Fragment>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </>
  );
}
