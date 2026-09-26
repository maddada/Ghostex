import { useState } from 'react';
import { IconFolder, IconSelector, IconStack2, IconX } from '@tabler/icons-react';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/packages/components/ui/command';
import { Popover, PopoverTrigger } from '@/packages/components/ui/popover';
import { SearchableDropdownContent } from '@/packages/components/ui/searchable-dropdown';

export type ScopeOption = {
  /** `space:<viewScopeSpaceKey>` or `project:<projectId>`, unique across both kinds. */
  id: string;
  kind: 'project' | 'space';
  label: string;
  /** Muted text after the label: a project's folder, or why an entry is unavailable. */
  hint?: string;
};

function ScopeOptionIcon({ kind }: { kind: ScopeOption['kind'] }) {
  return kind === 'space' ? (
    <IconStack2 aria-hidden='true' className='scope-option-icon' />
  ) : (
    <IconFolder aria-hidden='true' className='scope-option-icon' />
  );
}

/**
 * CDXC:Extensions 2026-09-24 DECISION:
 * User: "I really hate seeing all the toggles like this, please make it just a dropdown with multi select."
 * The view scope editor picks projects and spaces from one searchable multi-select: the picks show as chips
 * with a remove button, and the dropdown lists spaces first, then projects, with a check on each pick. It
 * replaces the 2026-09-18 toggle cards (three to a row, one switch per project and per space).
 */
export function ScopeMultiSelect({
  ariaLabel,
  onClear,
  onToggle,
  options,
  placeholder,
  searchPlaceholder,
  selected,
}: {
  ariaLabel: string;
  onClear: () => void;
  onToggle: (id: string) => void;
  options: readonly ScopeOption[];
  placeholder: string;
  searchPlaceholder: string;
  selected: readonly string[];
}) {
  const [open, setOpen] = useState(false);
  const optionsById = new Map(options.map((option) => [option.id, option]));
  const chips = selected.flatMap((id) => {
    const option = optionsById.get(id);
    return option ? [option] : [];
  });
  const groups = [
    { label: 'Spaces', options: options.filter((option) => option.kind === 'space') },
    { label: 'Projects', options: options.filter((option) => option.kind === 'project') },
  ].filter((group) => group.options.length);

  return (
    <div className='scope-multiselect' data-open={open ? 'true' : undefined}>
      {chips.map((option) => (
        <span className='scope-chip' data-scope-kind={option.kind} key={option.id} title={option.hint}>
          <ScopeOptionIcon kind={option.kind} />
          <span className='scope-chip-label'>{option.label}</span>
          <button
            aria-label={`Remove ${option.label}`}
            className='scope-chip-remove'
            onClick={() => onToggle(option.id)}
            type='button'
          >
            <IconX aria-hidden='true' />
          </button>
        </span>
      ))}
      <Popover onOpenChange={setOpen} open={open}>
        <PopoverTrigger
          render={
            <button aria-label={ariaLabel} className='scope-multiselect-trigger' type='button'>
              <span className='scope-multiselect-placeholder'>{chips.length ? 'Add…' : placeholder}</span>
              <IconSelector aria-hidden='true' className='scope-multiselect-chevron' />
            </button>
          }
        />
        <SearchableDropdownContent
          align='start'
          className='scope-multiselect-menu'
          onKeyDown={(event) => event.stopPropagation()}
          sideOffset={6}
        >
          <Command
            filter={(_value, search, keywords) =>
              (keywords ?? []).join(' ').toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()) ? 1 : 0
            }
          >
            <CommandInput
              aria-label={searchPlaceholder}
              autoFocus
              clearOnEscape={false}
              placeholder={searchPlaceholder}
            />
            <CommandList aria-multiselectable>
              <CommandEmpty>No projects or spaces match.</CommandEmpty>
              {groups.map((group) => (
                <CommandGroup heading={group.label} key={group.label}>
                  {group.options.map((option) => {
                    const checked = selected.includes(option.id);
                    return (
                      <CommandItem
                        aria-selected={checked}
                        data-checked={checked}
                        key={option.id}
                        keywords={[option.label, option.hint ?? '']}
                        onSelect={() => onToggle(option.id)}
                        value={option.id}
                      >
                        <ScopeOptionIcon kind={option.kind} />
                        <span className='scope-option-label'>{option.label}</span>
                        {option.hint ? <span className='scope-option-hint'>{option.hint}</span> : null}
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              ))}
            </CommandList>
            <div className='scope-multiselect-menu-foot'>
              <span>{selected.length ? `${selected.length} selected` : 'Nothing selected'}</span>
              {selected.length ? (
                <button className='scope-multiselect-clear' onClick={onClear} type='button'>
                  Clear
                </button>
              ) : null}
            </div>
          </Command>
        </SearchableDropdownContent>
      </Popover>
    </div>
  );
}
