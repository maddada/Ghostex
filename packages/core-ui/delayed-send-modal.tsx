import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { Button } from "@/packages/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/packages/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/packages/components/ui/field";
import { Input } from "@/packages/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/packages/components/ui/select";
import { Switch } from "@/packages/components/ui/switch";
import { AGENT_LOGOS } from "./agent-logos";
import type { SidebarAgentIcon } from "../shared/sidebar-agents";

const MAX_DELAY_MS = 2_147_483_647;
const SECOND_MS = 1_000;
const MINUTE_MS = 60 * SECOND_MS;
const HOUR_MS = 60 * MINUTE_MS;

type DelayedSendTrigger = "afterDelay" | "agentStops" | "allAgentsStop";

export type DelayedSendModalProps = {
  agentIcon?: SidebarAgentIcon;
  closeAfterDoneActive?: boolean;
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  isOpen: boolean;
  onCancel: () => void;
  onCancelTimer?: () => void;
  /**
   * CDXC:DelayedSend 2026-08-19:
   * Delayed Send carries exactly one trigger. The duration inputs keep their
   * last value while a status trigger is selected, so `delayMs` is reported as
   * `undefined` for `agentStops`/`allAgentsStop`; sending a stale duration
   * alongside a status trigger reads as two triggers and gxserver rejects it.
   */
  onConfirm: (
    delayMs: number | undefined,
    sendWhenAgentStops: boolean,
    sendWhenAllProjectSessionsStop: boolean
  ) => void;
  onToggleCloseAfterDone: () => void;
  sendWhenAllProjectSessionsStopActive?: boolean;
  sendWhenAgentStopsActive?: boolean;
  sessionTitle?: string;
  supportsSendWhenAgentStops?: boolean;
  supportsSendWhenAllProjectSessionsStop?: boolean;
};

/**
 * CDXC:DelayedSend 2026-05-11-11:56
 * Terminal pins need a clock action that lets the user stage command text now
 * and submit it later. Keep the modal duration-only: the terminal already owns
 * the prompt text, and native will press Enter when the timer expires.
 *
 * CDXC:DelayedSend 2026-05-17-03:14
 * Reopening Delayed Send for an active timer must show the current remaining
 * countdown, prefill the duration controls from that remaining time, and allow
 * cancellation so users can verify or change the pending Enter keypress.
 *
 * CDXC:DelayedSend 2026-06-16-17:57:
 * Users should configure delayed-send timers only in whole hours and minutes.
 * Round active remaining deadlines up to the next whole minute when prefilling
 * so editing an existing timer cannot silently shorten a sub-minute remainder
 * and seconds never reappear as an input.
 *
 * CDXC:DelayedSend 2026-06-17-17:01:
 * The minutes field is the primary edit target now that seconds are gone.
 * Focus and select it through the dialog's open focus path and the native
 * WebView frame settle pass so opening the timer is immediately type-to-replace.
 *
 * CDXC:DelayedSend 2026-06-18-11:08:
 * The native child window can become key after React's first focus pass, so
 * retry minutes focus over the first few animation frames/timeouts. Pressing
 * Enter while editing the duration must schedule the timer immediately.
 *
 * CDXC:SessionAutomations 2026-08-02:
 * Delayed Send and Close After Done share one automation editor. The send
 * trigger is a mutually-exclusive Select, both automations expose their real
 * enabled state through switches, and disabling an active send cancels it when
 * the user saves.
 */
export function DelayedSendModal({
  agentIcon,
  closeAfterDoneActive = false,
  delayedSendDeadlineAt,
  delayedSendRemainingLabel,
  isOpen,
  onCancel,
  onCancelTimer,
  onConfirm,
  onToggleCloseAfterDone,
  sendWhenAllProjectSessionsStopActive = false,
  sendWhenAgentStopsActive = false,
  sessionTitle,
  supportsSendWhenAgentStops = false,
  supportsSendWhenAllProjectSessionsStop = false,
}: DelayedSendModalProps) {
  const [hours, setHours] = useState("0");
  const [minutes, setMinutes] = useState("5");
  const [sendEnterEnabled, setSendEnterEnabled] = useState(true);
  const [trigger, setTrigger] = useState<DelayedSendTrigger>("afterDelay");
  const [closeAfterDoneEnabled, setCloseAfterDoneEnabled] = useState(closeAfterDoneActive);
  const hoursInputId = useId();
  const minutesInputId = useId();
  const sendEnterEnabledId = useId();
  const sendTriggerId = useId();
  const closeAfterDoneEnabledId = useId();
  const minutesInputRef = useRef<HTMLInputElement>(null);
  const focusRetryTimeoutIdsRef = useRef<number[]>([]);
  const focusRetryAnimationFrameIdsRef = useRef<number[]>([]);
  const focusMinutesInput = useCallback(() => {
    const input = minutesInputRef.current;
    if (!input) {
      return;
    }
    input.focus({ preventScroll: true });
    input.select();
  }, []);
  const clearScheduledMinutesFocus = useCallback(() => {
    for (const timeoutId of focusRetryTimeoutIdsRef.current) {
      window.clearTimeout(timeoutId);
    }
    for (const animationFrameId of focusRetryAnimationFrameIdsRef.current) {
      window.cancelAnimationFrame(animationFrameId);
    }
    focusRetryTimeoutIdsRef.current = [];
    focusRetryAnimationFrameIdsRef.current = [];
  }, []);
  const scheduleMinutesFocus = useCallback(() => {
    clearScheduledMinutesFocus();
    focusMinutesInput();
    const firstAnimationFrameId = window.requestAnimationFrame(() => {
      focusMinutesInput();
      const secondAnimationFrameId = window.requestAnimationFrame(focusMinutesInput);
      focusRetryAnimationFrameIdsRef.current.push(secondAnimationFrameId);
    });
    focusRetryAnimationFrameIdsRef.current.push(firstAnimationFrameId);
    for (const delayMs of [25, 75, 150, 300]) {
      const timeoutId = window.setTimeout(focusMinutesInput, delayMs);
      focusRetryTimeoutIdsRef.current.push(timeoutId);
    }
  }, [clearScheduledMinutesFocus, focusMinutesInput]);
  const handleOpenAutoFocus = useCallback(
    (event: { preventDefault: () => void }) => {
      event.preventDefault();
      if (sendEnterEnabled && trigger === "afterDelay") {
        scheduleMinutesFocus();
      }
    },
    [scheduleMinutesFocus, sendEnterEnabled, trigger]
  );

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const remainingMs = getRemainingMs(delayedSendDeadlineAt);
    const duration = remainingMs > 0 ? durationPartsFromMs(remainingMs) : undefined;
    setHours(String(duration?.hours ?? 0));
    setMinutes(String(duration?.minutes ?? 5));
    const shouldSendWhenAllProjectSessionsStop =
      supportsSendWhenAllProjectSessionsStop && sendWhenAllProjectSessionsStopActive;
    const shouldSendWhenAgentStops =
      !shouldSendWhenAllProjectSessionsStop &&
      supportsSendWhenAgentStops &&
      sendWhenAgentStopsActive;
    const initialTrigger: DelayedSendTrigger = shouldSendWhenAllProjectSessionsStop
      ? "allAgentsStop"
      : shouldSendWhenAgentStops
        ? "agentStops"
        : "afterDelay";
    setSendEnterEnabled(true);
    setTrigger(initialTrigger);
    setCloseAfterDoneEnabled(closeAfterDoneActive);
    /*
     * CDXC:DelayedSend 2026-05-21-12:21:
     * Opening or editing Delayed Send should select the minutes field, not
     * merely place a caret there, so typing immediately replaces the common
     * duration value without requiring Cmd+A or manual deletion.
     */
    if (initialTrigger !== "afterDelay") {
      clearScheduledMinutesFocus();
    } else {
      scheduleMinutesFocus();
    }
    return () => {
      clearScheduledMinutesFocus();
    };
  }, [
    clearScheduledMinutesFocus,
    closeAfterDoneActive,
    delayedSendDeadlineAt,
    isOpen,
    scheduleMinutesFocus,
    sendWhenAllProjectSessionsStopActive,
    sendWhenAgentStopsActive,
    supportsSendWhenAllProjectSessionsStop,
    supportsSendWhenAgentStops,
  ]);

  if (!isOpen) {
    return null;
  }

  const delayMs = getDelayMs(hours, minutes);
  const isValidDelay = delayMs >= MINUTE_MS && delayMs <= MAX_DELAY_MS;
  const hasStatusTrigger = trigger !== "afterDelay";
  const isValidSchedule = hasStatusTrigger || isValidDelay;
  const hasActiveSend = Boolean(
    delayedSendRemainingLabel || sendWhenAgentStopsActive || sendWhenAllProjectSessionsStopActive
  );
  const closeAfterDoneChanged = closeAfterDoneEnabled !== closeAfterDoneActive;
  const canDisableActiveSend = !hasActiveSend || Boolean(onCancelTimer);
  const canSave = sendEnterEnabled
    ? isValidSchedule
    : canDisableActiveSend && (hasActiveSend || closeAfterDoneChanged);
  const sendWhenAgentStops = trigger === "agentStops";
  const sendWhenAllProjectSessionsStop = trigger === "allAgentsStop";
  const trimmedSessionTitle = sessionTitle?.trim();
  const sessionTargetLabel = trimmedSessionTitle || "Current agent session";
  const agentIconStyle = agentIcon
    ? ({ "--delayed-send-agent-logo": `url("${AGENT_LOGOS[agentIcon]}")` } as CSSProperties)
    : undefined;
  const triggerOptions: { label: string; value: DelayedSendTrigger }[] = [
    { label: "After a delay", value: "afterDelay" },
    ...(supportsSendWhenAgentStops
      ? [{ label: "When this agent finishes", value: "agentStops" as const }]
      : []),
    ...(supportsSendWhenAllProjectSessionsStop
      ? [{ label: "When all agents finish", value: "allAgentsStop" as const }]
      : []),
  ];
  const sendAutomationDescription = !sendEnterEnabled
    ? "No Enter keypress will be scheduled."
    : sendWhenAllProjectSessionsStopActive
      ? "Active when all agents finish working."
      : sendWhenAgentStopsActive
        ? "Active when this agent finishes working."
        : delayedSendRemainingLabel
          ? `Active. Enter sends in ${delayedSendRemainingLabel}.`
          : "Press Enter later using the selected trigger.";
  /*
   * CDXC:CodexModalRestyle 2026-08-24:
   * The Codex-style language reserves the accent color for live status text, so
   * the Send Enter summary only takes the accent class while an automation is
   * actually armed.
   */
  const sendAutomationIsActive = sendEnterEnabled && hasActiveSend;

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSave) {
      return;
    }
    if (closeAfterDoneChanged) {
      onToggleCloseAfterDone();
    }
    if (sendEnterEnabled) {
      onConfirm(
        hasStatusTrigger ? undefined : delayMs,
        sendWhenAgentStops,
        sendWhenAllProjectSessionsStop
      );
    } else if (hasActiveSend) {
      onCancelTimer?.();
    }
  };
  const submitFromDurationInput = (event: KeyboardEvent<HTMLInputElement>) => {
    if (
      event.key !== "Enter" ||
      event.nativeEvent.isComposing ||
      !sendEnterEnabled ||
      hasStatusTrigger ||
      !isValidDelay
    ) {
      return;
    }
    event.preventDefault();
    event.currentTarget.form?.requestSubmit();
  };

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className="command-config-modal-shadcn delayed-send-modal-shadcn font-sans"
        onOpenAutoFocus={handleOpenAutoFocus}
        showCloseButton={false}
      >
        <form className="delayed-send-form" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>Session Automations</DialogTitle>
            <DialogDescription className="delayed-send-dialog-description">
              <span>Configure automations for this agent session.</span>
              <span className="delayed-send-session-target">
                {agentIconStyle ? (
                  <span
                    aria-hidden="true"
                    className="delayed-send-session-agent-icon"
                    style={agentIconStyle}
                  />
                ) : null}
                <span className="delayed-send-session-title">{sessionTargetLabel}</span>
              </span>
            </DialogDescription>
          </DialogHeader>
          <div className="delayed-send-automation-stack">
            <Card className="delayed-send-automation-card" size="sm">
              <CardHeader>
                <CardTitle>Send Enter</CardTitle>
                <CardDescription
                  className={
                    sendAutomationIsActive ? "delayed-send-automation-status-active" : undefined
                  }
                >
                  {sendAutomationDescription}
                </CardDescription>
                <CardAction>
                  <Switch
                    aria-label="Send Enter automation"
                    checked={sendEnterEnabled}
                    id={sendEnterEnabledId}
                    onCheckedChange={(checked) => {
                      setSendEnterEnabled(checked);
                      if (checked && trigger === "afterDelay") {
                        window.requestAnimationFrame(scheduleMinutesFocus);
                      } else {
                        clearScheduledMinutesFocus();
                      }
                    }}
                  />
                </CardAction>
              </CardHeader>
              {sendEnterEnabled ? (
                <CardContent>
                  <FieldGroup className="delayed-send-field-group">
                    <Field>
                      <FieldLabel htmlFor={sendTriggerId}>Trigger</FieldLabel>
                      <Select
                        disabled={triggerOptions.length === 1}
                        items={triggerOptions}
                        onValueChange={(value) => {
                          const nextTrigger = value as DelayedSendTrigger;
                          setTrigger(nextTrigger);
                          if (nextTrigger === "afterDelay") {
                            window.requestAnimationFrame(scheduleMinutesFocus);
                          } else {
                            clearScheduledMinutesFocus();
                          }
                        }}
                        value={trigger}
                      >
                        <SelectTrigger className="w-full" id={sendTriggerId}>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent
                          alignItemWithTrigger={false}
                          className="delayed-send-select-content"
                        >
                          <SelectGroup>
                            {triggerOptions.map((option) => (
                              <SelectItem key={option.value} value={option.value}>
                                {option.label}
                              </SelectItem>
                            ))}
                          </SelectGroup>
                        </SelectContent>
                      </Select>
                    </Field>
                    <div className="delayed-send-trigger-detail-slot">
                      {trigger === "afterDelay" ? (
                        <div className="delayed-send-duration-grid">
                          <Field>
                            <FieldLabel htmlFor={hoursInputId}>Hours</FieldLabel>
                            <Input
                              aria-label="Hours"
                              id={hoursInputId}
                              min={0}
                              onChange={(event) => setHours(event.currentTarget.value)}
                              onKeyDown={submitFromDurationInput}
                              step={1}
                              type="number"
                              value={hours}
                            />
                          </Field>
                          <Field>
                            <FieldLabel htmlFor={minutesInputId}>Minutes</FieldLabel>
                            <Input
                              aria-label="Minutes"
                              autoFocus
                              id={minutesInputId}
                              min={0}
                              onChange={(event) => setMinutes(event.currentTarget.value)}
                              onFocus={(event) => event.currentTarget.select()}
                              onKeyDown={submitFromDurationInput}
                              ref={minutesInputRef}
                              step={1}
                              type="number"
                              value={minutes}
                            />
                          </Field>
                        </div>
                      ) : (
                        <p className="delayed-send-trigger-description">
                          {trigger === "agentStops"
                            ? "Ghostex will send Enter automatically after this agent finishes working and remains idle for 10 seconds."
                            : "Ghostex will send Enter automatically after every agent in this project finishes working and remains idle for 10 seconds."}
                        </p>
                      )}
                    </div>
                  </FieldGroup>
                </CardContent>
              ) : null}
            </Card>
            <Card
              className="delayed-send-automation-card delayed-send-close-card"
              size="sm"
            >
              <CardHeader>
                <CardTitle>Close session after Done</CardTitle>
                <CardDescription>Closes this terminal 3 minutes after Done.</CardDescription>
                <CardAction>
                  <Switch
                    aria-label="Close session after Done"
                    checked={closeAfterDoneEnabled}
                    id={closeAfterDoneEnabledId}
                    onCheckedChange={setCloseAfterDoneEnabled}
                  />
                </CardAction>
              </CardHeader>
            </Card>
          </div>
          <DialogFooter className="delayed-send-footer">
            <Button
              className="delayed-send-action-button"
              onClick={onCancel}
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button className="delayed-send-action-button" disabled={!canSave} type="submit">
              Save changes
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function getDelayMs(hours: string, minutes: string): number {
  return parseDurationPart(hours) * HOUR_MS + parseDurationPart(minutes) * MINUTE_MS;
}

function parseDurationPart(value: string): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0 || !Number.isInteger(parsed)) {
    return Number.NaN;
  }
  return parsed;
}

function getRemainingMs(deadlineAt: string | undefined): number {
  if (!deadlineAt) {
    return 0;
  }
  const deadlineMs = Date.parse(deadlineAt);
  if (!Number.isFinite(deadlineMs)) {
    return 0;
  }
  return Math.max(0, deadlineMs - Date.now());
}

function durationPartsFromMs(delayMs: number): { hours: number; minutes: number } {
  const totalMinutes = Math.max(1, Math.ceil(delayMs / MINUTE_MS));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return { hours, minutes };
}
