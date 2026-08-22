import type { ReactNode } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/packages/components/ui/tooltip";
import { cn } from "@/packages/components/utils";

type DisabledSettingControlTooltipProps = {
  children: ReactNode;
  className?: string;
  disabled: boolean;
  reason?: string;
};

/**
 * Disabled form controls cannot be tooltip triggers themselves because the
 * shared button skin removes their pointer events. Keep the visible control
 * disabled and let a small, keyboard-focusable wrapper explain why.
 */
export function DisabledSettingControlTooltip({
  children,
  className,
  disabled,
  reason,
}: DisabledSettingControlTooltipProps) {
  if (!disabled || !reason) {
    return children;
  }

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            aria-label={`Unavailable: ${reason}`}
            className={cn("inline-flex max-w-full cursor-not-allowed", className)}
            tabIndex={0}
          >
            {children}
          </span>
        }
      />
      <TooltipContent
        className="max-w-72 text-center"
        side="top"
        sideOffset={6}
      >
        {reason}
      </TooltipContent>
    </Tooltip>
  );
}
