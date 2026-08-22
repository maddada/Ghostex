"use client";

import * as React from "react";
import { IconCheck, IconSearch, IconX } from "@tabler/icons-react";
import { Command as CommandPrimitive } from "cmdk";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "./dialog";
import { InputGroup, InputGroupAddon } from "./input-group";
import { cn } from "../utils";

function Command({ className, ...props }: React.ComponentProps<typeof CommandPrimitive>) {
  return (
    <CommandPrimitive
      data-slot="command"
      className={cn(
        "flex size-full flex-col overflow-hidden rounded-none bg-popover p-1 text-popover-foreground",
        className,
      )}
      {...props}
    />
  );
}

function CommandDialog({
  title = "Command Palette",
  description = "Search for a command to run...",
  children,
  className,
  showCloseButton = false,
  ...props
}: Omit<React.ComponentProps<typeof Dialog>, "children"> & {
  title?: string;
  description?: string;
  className?: string;
  showCloseButton?: boolean;
  children: React.ReactNode;
}) {
  return (
    <Dialog {...props}>
      <DialogHeader className="sr-only">
        <DialogTitle>{title}</DialogTitle>
        <DialogDescription>{description}</DialogDescription>
      </DialogHeader>
      <DialogContent
        className={cn("top-1/3 translate-y-0 overflow-hidden rounded-none! p-0", className)}
        showCloseButton={showCloseButton}
      >
        {children}
      </DialogContent>
    </Dialog>
  );
}

function CommandInput({
  className,
  clearOnEscape = true,
  clearLabel = "Clear search",
  onKeyDown,
  onValueChange,
  value,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Input> & {
  clearOnEscape?: boolean;
  clearLabel?: string;
}) {
  const inputRef = React.useRef<HTMLInputElement>(null);
  const [uncontrolledValue, setUncontrolledValue] = React.useState("");
  const currentValue = typeof value === "string" ? value : uncontrolledValue;
  const hasQuery = currentValue.length > 0;
  const handleValueChange = (nextValue: string) => {
    setUncontrolledValue(nextValue);
    onValueChange?.(nextValue);
  };

  return (
    <div data-slot="command-input-wrapper" className="p-1 pb-0">
      <InputGroup className="h-9 bg-input/30">
        {/*
         * CDXC:SearchInputs 2026-06-04-02:59:
         * Command-backed search boxes, including Settings icon search, should match the sidebar search affordance by keeping the search icon on the right while empty and replacing it with a focused clear button after typing.
         *
         * CDXC:SearchInputs 2026-06-04-03:11:
         * Escape on a focused non-empty command search clears the query by default the same way as the X button instead of closing the surrounding popover first.
         *
         * CDXC:CommandPalette 2026-06-15-16:21:
         * The app command palette must own Escape as a modal-close command, even when the query is non-empty. Keep clear-on-Escape opt-out at this shared input boundary so other command searches preserve their clear affordance without the palette clearing text before it closes.
         */}
        <CommandPrimitive.Input
          data-slot="command-input"
          className={cn(
            "w-full text-sm outline-hidden disabled:cursor-not-allowed disabled:opacity-50",
            className,
          )}
          onKeyDown={(event) => {
            if (clearOnEscape && event.key === "Escape" && currentValue.length > 0) {
              event.preventDefault();
              event.stopPropagation();
              handleValueChange("");
              inputRef.current?.focus();
              return;
            }
            onKeyDown?.(event);
          }}
          onValueChange={handleValueChange}
          ref={inputRef}
          value={currentValue}
          {...props}
        />
        <InputGroupAddon align="inline-end">
          {hasQuery ? (
            <button
              aria-label={clearLabel}
              data-slot="command-input-clear"
              className="flex size-6 appearance-none items-center justify-center rounded-none border-0 bg-transparent p-0 text-muted-foreground shadow-none hover:text-foreground focus-visible:text-foreground focus-visible:outline-none"
              onClick={() => {
                handleValueChange("");
                inputRef.current?.focus();
              }}
              type="button"
            >
              <IconX aria-hidden="true" className="size-4 shrink-0" />
            </button>
          ) : (
            <IconSearch aria-hidden="true" className="size-4 shrink-0 opacity-50" />
          )}
        </InputGroupAddon>
      </InputGroup>
    </div>
  );
}

function CommandList({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.List>) {
  return (
    <CommandPrimitive.List
      data-slot="command-list"
      className={cn(
        /*
         * CDXC:ScrollFades 2026-06-19-14:16:
         * Command palettes and command-backed pickers should use the same
         * Codex-style scroll-container fade as the macOS sidebar instead of
         * bespoke gray shadows or unfaded clipped edges.
         */
        "vertical-scroll-fade-mask no-scrollbar max-h-72 scroll-py-1 overflow-x-hidden overflow-y-auto outline-none",
        className,
      )}
      {...props}
    />
  );
}

function CommandEmpty({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Empty>) {
  return (
    <CommandPrimitive.Empty
      data-slot="command-empty"
      className={cn("py-6 text-center text-sm", className)}
      {...props}
    />
  );
}

function CommandGroup({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Group>) {
  return (
    <CommandPrimitive.Group
      data-slot="command-group"
      className={cn(
        "overflow-hidden p-1 text-foreground **:[[cmdk-group-heading]]:px-3 **:[[cmdk-group-heading]]:py-2 **:[[cmdk-group-heading]]:text-xs **:[[cmdk-group-heading]]:font-medium **:[[cmdk-group-heading]]:text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

function CommandSeparator({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Separator>) {
  return (
    <CommandPrimitive.Separator
      data-slot="command-separator"
      className={cn("my-1 h-px bg-border/50", className)}
      {...props}
    />
  );
}

function CommandItem({
  className,
  children,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Item>) {
  return (
    <CommandPrimitive.Item
      data-slot="command-item"
      className={cn(
        "group/command-item relative flex cursor-default items-center gap-2 rounded-none px-3 py-2 text-sm outline-hidden select-none in-data-[slot=dialog-content]:rounded-none data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 data-selected:bg-muted data-selected:text-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 data-selected:*:[svg]:text-foreground",
        className,
      )}
      {...props}
    >
      {children}
      <IconCheck className="ml-auto opacity-0 group-has-data-[slot=command-shortcut]/command-item:hidden group-data-[checked=true]/command-item:opacity-100" />
    </CommandPrimitive.Item>
  );
}

function CommandShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="command-shortcut"
      className={cn(
        "ml-auto text-xs tracking-widest text-muted-foreground group-data-selected/command-item:text-foreground",
        className,
      )}
      {...props}
    />
  );
}

export {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
};
