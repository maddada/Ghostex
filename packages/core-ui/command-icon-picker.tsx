import { IconChevronDown } from "@tabler/icons-react";
import { useEffect, useId, useState } from "react";
import { flushSync } from "react-dom";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/packages/components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/packages/components/ui/popover";
import {
  DEFAULT_SIDEBAR_COMMAND_ICON,
  getSidebarCommandIconLabel,
  type SidebarCommandIcon,
} from "../shared/sidebar-command-icons";
import { SIDEBAR_COMMAND_ICON_OPTIONS, SidebarCommandIconGlyph } from "./sidebar-command-icon";

export type CommandIconPickerProps = {
  icon?: SidebarCommandIcon;
  onIconChange: (icon: SidebarCommandIcon) => void;
};

export function CommandIconPicker({
  icon,
  onIconChange,
}: CommandIconPickerProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [iconListElement, setIconListElement] = useState<HTMLDivElement | null>(null);
  const labelId = useId();
  const selectedIcon = icon ?? DEFAULT_SIDEBAR_COMMAND_ICON;

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    if (!iconListElement) {
      return;
    }

    const handleWheel = (event: WheelEvent) => {
      const maxScrollTop = iconListElement.scrollHeight - iconListElement.clientHeight;
      const nextScrollTop = Math.max(
        0,
        Math.min(maxScrollTop, iconListElement.scrollTop + event.deltaY),
      );

      if (nextScrollTop !== iconListElement.scrollTop) {
        event.preventDefault();
        event.stopPropagation();
        iconListElement.scrollTop = nextScrollTop;
      }
    };

    iconListElement.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      iconListElement.removeEventListener("wheel", handleWheel);
    };
  }, [iconListElement, isOpen]);

  return (
    <div className="command-icon-picker-fields">
      <div className="command-config-field command-icon-picker-field">
        <span className="command-config-label" id={labelId}>
          Icon
        </span>
        <Popover open={isOpen} onOpenChange={setIsOpen}>
          <PopoverTrigger
            render={
              <button
                aria-expanded={isOpen}
                aria-labelledby={labelId}
                className="group-title-input command-config-input command-icon-picker-trigger"
                type="button"
              >
                <span className="command-icon-picker-trigger-value">
                  <span aria-hidden="true" className="command-button-icon-shell">
                    <SidebarCommandIconGlyph
                      className="command-button-leading-icon"
                      icon={selectedIcon}
                      size={16}
                    />
                  </span>
                  <span>{getSidebarCommandIconLabel(selectedIcon)}</span>
                </span>
                <IconChevronDown
                  aria-hidden="true"
                  className="command-icon-picker-trigger-chevron"
                  size={16}
                />
              </button>
            }
          />
          <PopoverContent
            align="start"
            className="command-icon-picker-menu"
            onOpenAutoFocus={(event) => event.preventDefault()}
          >
            <Command>
              {/*
               * CDXC:SidebarActions 2026-05-15-14:24:
               * The action icon dropdown needs a searchable shadcn Command
               * input at the top while every option keeps a left-side glyph.
               * Use Popover for open/close behavior instead of custom document
               * listeners so keyboard and outside-click handling stay with the
               * component primitive.
               *
               * CDXC:SidebarActions 2026-05-15-14:46:
               * The picker appears inside a modal settings dialog, so wheel
               * input on the portaled Popover can be consumed by dialog scroll
               * locking before the browser performs default list scrolling.
               * Attach a non-passive wheel listener to the Command list so long
               * icon sets remain browseable while the modal background stays
               * locked.
               *
               * CDXC:CommandIcons 2026-06-09-09:32:
               * CommandInput sits inside InputGroup without an inline-start addon,
               * so add pl-3 here to match other Settings fields; InputGroup only
               * applies horizontal inset when start/end addons are present.
               *
               * CDXC:CommandIcons 2026-06-16-07:48:
               * Action icons inherit the surrounding titlebar/settings glyph
               * color. Do not expose or apply per-action icon colors because
               * action glyphs should match the titlebar icons beside them.
               */}
              <CommandInput
                aria-label="Search icons"
                className="command-icon-picker-search pl-3"
                clearLabel="Clear icon search"
                placeholder="Search icons"
                spellCheck={false}
              />
              <CommandList
                className="command-icon-picker-options scroll-mask-y"
                ref={setIconListElement}
              >
                <CommandEmpty className="command-icon-picker-empty-state">
                  No matching icons
                </CommandEmpty>
                <CommandGroup>
                  {SIDEBAR_COMMAND_ICON_OPTIONS.map((option) => (
                    <CommandItem
                      className="command-icon-picker-option"
                      data-checked={selectedIcon === option.icon}
                      key={option.icon}
                      onSelect={() => {
                        /*
                         * CDXC:SidebarActions 2026-06-19-19:52:
                         * The action icon picker is a portaled Popover inside
                         * Settings. Close it before handing the selected icon
                         * to the parent editor so parent re-renders cannot leave
                         * the picker popup owning focus.
                         */
                        flushSync(() => {
                          setIsOpen(false);
                        });
                        onIconChange(option.icon);
                      }}
                      value={option.label}
                    >
                      <span aria-hidden="true" className="command-button-icon-shell">
                        <SidebarCommandIconGlyph
                          className="command-button-leading-icon"
                          icon={option.icon}
                          size={16}
                        />
                      </span>
                      <span className="command-icon-picker-option-copy">{option.label}</span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              </CommandList>
            </Command>
          </PopoverContent>
        </Popover>
      </div>
    </div>
  );
}
