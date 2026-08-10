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
  /** True when the transport can inject raw keystrokes (Claude's mode cycle). */
  canSendKey: boolean;
  onDispatchCommand: (command: string) => Promise<void>;
  onDispatchKey: (key: SessionChatSendKey, marker: string) => Promise<void>;
  /** Agent-picker options flip the pane to the terminal after typing. */
  onSwitchToTerminal?: () => void;
}

function PillTrigger({
  ariaLabel,
  disabled,
  label,
  title,
}: {
  ariaLabel: string;
  disabled: boolean;
  label: string;
  title: string;
}) {
  return (
    <AppTooltip content={title}>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={ariaLabel}
            className="ghostex-chat-footer-control max-w-40 rounded-full text-muted-foreground"
            disabled={disabled}
            size="xs"
            variant="ghost"
          />
        }
      >
        <span className="truncate">{label}</span>
        <IconChevronDown aria-hidden="true" className="size-3 shrink-0" stroke={2} />
      </DropdownMenuTrigger>
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
        (descriptor) => descriptor.dispatch.kind !== "key" || canSendKey,
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
          onSwitchToTerminal?.();
          await onDispatchCommand(delivery.command);
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
    [onDispatchCommand, onDispatchKey, onSwitchToTerminal, recordDispatched],
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
      <>
        <DropdownMenuItem className="rounded-md" onClick={() => dispatch(descriptor)}>
          {descriptor.actionLabel ?? descriptor.label}
        </DropdownMenuItem>
        {descriptor.dispatch.kind === "agent-picker" && descriptor.choices?.length ? (
          <DropdownMenuLabel className="whitespace-normal">
            {descriptor.choices.map((choice) => choice.label).join(" · ")}
          </DropdownMenuLabel>
        ) : null}
      </>
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
            disabled={disabled}
            label={combinedLabel}
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
          disabled={disabled}
          label={modelLabel ?? catalog.model.label}
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
            disabled={disabled}
            label={optionsLabel ?? "Options"}
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
