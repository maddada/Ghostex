import { useEffect, useState } from "react";
import { IconX } from "@tabler/icons-react";
import { Button } from "@/packages/components/ui/button";
import { cn } from "@/packages/components/utils";
import { AppTooltip } from "./app-tooltip";
import {
  ghostexHotkeyTextFromKeyboardEvent,
  isReservedghostexHotkeyText,
  normalizeHotkeyText,
} from "../shared/ghostex-hotkeys";
import { formatSidebarHotkeyLabel } from "./hotkey-label";

export type HotkeyRecorderFieldProps = {
  ariaInvalid?: boolean;
  className?: string;
  hotkey: string;
  id?: string;
  onChange: (hotkey: string) => void;
  originalHotkey: string;
};

export function HotkeyRecorderField({
  ariaInvalid = false,
  className,
  hotkey,
  id,
  onChange,
  originalHotkey,
}: HotkeyRecorderFieldProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [reservedHotkey, setReservedHotkey] = useState("");
  const normalizedHotkey = normalizeHotkeyText(hotkey);
  const normalizedOriginalHotkey = normalizeHotkeyText(originalHotkey);
  const originalHotkeyLabel = formatSidebarHotkeyLabel(normalizedOriginalHotkey) || "Unassigned";
  const isModified = normalizedHotkey !== normalizedOriginalHotkey;
  const recordingLabel = reservedHotkey
    ? `${formatSidebarHotkeyLabel(reservedHotkey)} is reserved`
    : "Press Shortcut";
  const label = isRecording ? recordingLabel : formatSidebarHotkeyLabel(normalizedHotkey);

  useEffect(() => {
    if (!isRecording) {
      setReservedHotkey("");
      return;
    }
    const recordPhysicalHotkey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.key === "Escape") {
        setIsRecording(false);
        return;
      }
      if (
        (event.key === "Backspace" || event.key === "Delete") &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.shiftKey
      ) {
        setIsRecording(false);
        onChange("");
        return;
      }
      const recordedHotkey = ghostexHotkeyTextFromKeyboardEvent(event);
      if (!recordedHotkey) {
        return;
      }
      /**
       * CDXC:Hotkeys 2026-08-22:
       * The focused terminal owns Cmd+K for Ghostty's clear-screen binding,
       * so it cannot be handed to a command. Keep recording and name the
       * chord that was refused instead of silently swallowing the press.
       */
      if (isReservedghostexHotkeyText(recordedHotkey)) {
        setReservedHotkey(recordedHotkey);
        return;
      }
      /**
       * CDXC:Hotkeys 2026-07-30:
       * Record the physical key (`KeyboardEvent.code`) rather than the
       * Option-modified character (`KeyboardEvent.key`). For example, macOS
       * reports Option+S as `ß`; GPUI dispatches the physical S key, so storing
       * the produced character made the shortcut impossible to run.
       */
      setIsRecording(false);
      onChange(recordedHotkey);
    };
    document.addEventListener("keydown", recordPhysicalHotkey, { capture: true });
    return () => document.removeEventListener("keydown", recordPhysicalHotkey, { capture: true });
  }, [isRecording, onChange]);

  return (
    <div
      data-hotkey-recorder="true"
      data-recording={isRecording ? "true" : undefined}
      className="group/hotkey-recorder relative w-full"
    >
      <Button
        aria-invalid={ariaInvalid}
        className={cn(
          "h-10 w-full justify-start overflow-hidden px-3 pr-9 font-mono text-sm",
          className,
        )}
        id={id}
        onClick={() => {
          setIsRecording((recording) => !recording);
        }}
        type="button"
        variant="outline"
      >
        <span className="truncate">{label || "Unassigned"}</span>
      </Button>
      {isModified || normalizedHotkey ? (
        <div className="pointer-events-none absolute top-1/2 right-1.5 z-10 flex -translate-y-1/2 items-center gap-1 opacity-0 transition-opacity group-focus-within/hotkey-recorder:pointer-events-auto group-focus-within/hotkey-recorder:opacity-100 group-hover/hotkey-recorder:pointer-events-auto group-hover/hotkey-recorder:opacity-100">
          {isModified ? (
            <AppTooltip content={`Reset to ${originalHotkeyLabel}`}>
              <Button
                aria-label={`Reset hotkey to ${originalHotkeyLabel}`}
                className="h-7 rounded-none border border-border bg-background/95 px-2 font-mono text-xs text-muted-foreground shadow-sm hover:bg-muted hover:text-foreground"
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  setIsRecording(false);
                  onChange(normalizedOriginalHotkey);
                }}
                type="button"
                variant="outline"
              >
                {originalHotkeyLabel}
              </Button>
            </AppTooltip>
          ) : null}
          {normalizedHotkey ? (
            <AppTooltip content="Remove hotkey">
              <Button
                aria-label="Remove hotkey"
                className="size-7 rounded-none border border-border bg-background/95 p-0 text-muted-foreground shadow-sm hover:bg-muted hover:text-foreground"
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  setIsRecording(false);
                  onChange("");
                }}
                size="icon-xs"
                type="button"
                variant="outline"
              >
                {/* CDXC:Hotkeys 2026-05-11-09:06
                    The remove affordance is a real button inside the hotkey field,
                    revealed only when that field is hovered or focused so hotkey rows
                    stay quiet until the user targets a specific binding. */}
                <IconX aria-hidden="true" className="size-4" />
              </Button>
            </AppTooltip>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
