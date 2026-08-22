import { IconSearch, IconX } from '@tabler/icons-react';
import type { ReactNode, RefObject } from 'react';
import { InputGroup, InputGroupAddon } from '../components/ui/input-group';

export type QuickAccessSearchInputProps = {
  ariaLabel: string;
  clearLabel: string;
  inputRef: RefObject<HTMLInputElement | null>;
  placeholder: string;
  query: string;
  setQuery: (query: string) => void;
  trailingControl?: ReactNode;
};

/**
 * The non-command Quick Access pages use the same input-group composition as
 * CommandInput so changing tabs never changes the search field's typography,
 * color, height, inset, or clear affordance.
 */
export function QuickAccessSearchInput({
  ariaLabel,
  clearLabel,
  inputRef,
  placeholder,
  query,
  setQuery,
  trailingControl,
}: QuickAccessSearchInputProps) {
  const hasQuery = query.length > 0;

  return (
    <div data-slot='command-input-wrapper'>
      <InputGroup className='h-9 bg-input/30'>
        <input
          aria-label={ariaLabel}
          className='w-full bg-transparent pl-3 text-sm text-foreground outline-hidden placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50'
          data-slot='quick-access-search-input'
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder={placeholder}
          ref={inputRef}
          type='text'
          value={query}
        />
        <InputGroupAddon align='inline-end' className={trailingControl ? 'gap-1' : undefined}>
          {hasQuery ? (
            <button
              aria-label={clearLabel}
              className='flex size-6 appearance-none items-center justify-center rounded-none border-0 bg-transparent p-0 text-muted-foreground shadow-none hover:text-foreground focus-visible:text-foreground focus-visible:outline-none'
              data-slot='command-input-clear'
              onClick={() => {
                setQuery('');
                inputRef.current?.focus();
              }}
              type='button'
            >
              <IconX aria-hidden='true' className='size-4 shrink-0' />
            </button>
          ) : (
            <IconSearch aria-hidden='true' className='size-4 shrink-0 opacity-50' />
          )}
          {trailingControl}
        </InputGroupAddon>
      </InputGroup>
    </div>
  );
}
