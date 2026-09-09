'use client';

import * as React from 'react';
import { Select as SelectPrimitive } from '@base-ui/react/select';
import { useRender } from '@base-ui/react/use-render';
import { Separator } from '@base-ui/react/separator';
import { Combobox } from '@base-ui/react/combobox';
import { InputGroup, InputGroupAddon } from './input-group';
import './searchable-dropdown.css';

import { cn } from '../utils';
import { IconSelector, IconChevronUp, IconChevronDown, IconSearch, IconX } from '@tabler/icons-react';
import { OverlayLayerContext, overlayTooltipBorderStyle } from './overlay-surface';

type SelectProps = Omit<SelectPrimitive.Root.Props<string>, 'onValueChange' | 'onOpenChange'> & {
  onValueChange?: (value: string) => void;
  onOpenChange?: (
    open: boolean,
    details: SelectPrimitive.Root.ChangeEventDetails | Combobox.Root.ChangeEventDetails
  ) => void;
  /** Automatic for eight or more options; enable explicitly for growing lists. */
  searchable?: boolean;
  searchPlaceholder?: string;
  emptyMessage?: string;
};

type SelectItemDefinition = {
  label: React.ReactNode;
  value: string;
  searchText: string;
};

type SelectChildProps = {
  children?: React.ReactNode;
  value?: unknown;
  label?: string;
  name?: string;
};

function collectSelectItems(children: React.ReactNode): SelectItemDefinition[] {
  const items: SelectItemDefinition[] = [];

  function visit(node: React.ReactNode) {
    React.Children.forEach(node, (child) => {
      if (!React.isValidElement<SelectChildProps>(child)) {
        return;
      }

      if (child.type === SelectItem && typeof child.props.value === 'string') {
        items.push({
          label: child.props.children,
          value: child.props.value,
          searchText: child.props.label ?? selectText(child.props.children),
        });
        return;
      }

      visit(child.props.children);
    });
  }

  visit(children);
  return items;
}

function selectText(node: React.ReactNode): string {
  return React.Children.toArray(node)
    .map((child) => {
      if (typeof child === 'string' || typeof child === 'number') return String(child);
      if (!React.isValidElement<SelectChildProps>(child)) return '';
      return child.props.label ?? child.props.name ?? selectText(child.props.children);
    })
    .join(' ');
}

const SearchableSelectContext = React.createContext<{
  value: string | null;
  readOnly: boolean;
  visibleValues: Set<string>;
  labels: Map<string, React.ReactNode>;
  query: string;
  setQuery: (query: string) => void;
  searchPlaceholder: string;
  emptyMessage: string;
} | null>(null);

/** CDXC:DesignSystem 2026-09-08 DECISION:
 * User: replace dropdowns that usually have lots of items with the wider, rounded Sessions dropdown with search at the top.
 */
function Select({
  children,
  items,
  onValueChange,
  onOpenChange,
  searchable,
  searchPlaceholder = 'Search...',
  emptyMessage = 'No matches found.',
  ...props
}: SelectProps) {
  const inferredItems = collectSelectItems(children);
  const [query, setQuery] = React.useState('');
  const [uncontrolledValue, setUncontrolledValue] = React.useState(props.defaultValue ?? null);
  const value = props.value === undefined ? uncontrolledValue : props.value;
  const changeValue = (nextValue: string | null) => {
    setUncontrolledValue(nextValue);
    if (nextValue !== null) onValueChange?.(nextValue);
  };
  const labels = new Map(inferredItems.map((item) => [item.value, item.label]));
  if (items) {
    if (Array.isArray(items)) {
      for (const item of items) {
        if ('value' in item) labels.set(item.value, item.label);
        else if ('items' in item) for (const entry of item.items) labels.set(entry.value, entry.label);
      }
    } else {
      for (const [key, label] of Object.entries(items)) labels.set(key, label);
    }
  }
  if (!(searchable ?? inferredItems.length >= 8)) {
    return (
      <SearchableSelectContext.Provider value={null}>
        <SelectPrimitive.Root
          {...props}
          value={value}
          items={items ?? inferredItems}
          onValueChange={changeValue}
          onOpenChange={onOpenChange}
        >
          {children}
        </SelectPrimitive.Root>
      </SearchableSelectContext.Provider>
    );
  }
  const terms = query.trim().toLocaleLowerCase().split(/\s+/);
  const values = inferredItems.map((item) => item.value);
  const filteredValues = inferredItems
    .filter((item) => {
      const text = `${item.searchText} ${selectText(labels.get(item.value))}`.toLocaleLowerCase();
      return terms.every((term) => text.includes(term));
    })
    .map((item) => item.value);
  return (
    <SearchableSelectContext.Provider
      value={{
        value,
        readOnly: props.readOnly ?? false,
        visibleValues: new Set(filteredValues),
        labels,
        query,
        setQuery,
        searchPlaceholder,
        emptyMessage,
      }}
    >
      <Combobox.Root
        {...props}
        value={value}
        items={values}
        filteredItems={filteredValues}
        filter={null}
        autoHighlight
        inputValue={query}
        onInputValueChange={setQuery}
        onValueChange={changeValue}
        onOpenChange={(open, details) => {
          setQuery('');
          onOpenChange?.(open, details);
        }}
      >
        {children}
      </Combobox.Root>
    </SearchableSelectContext.Provider>
  );
}

function SelectGroup({ className, ...props }: SelectPrimitive.Group.Props) {
  const search = React.useContext(SearchableSelectContext);
  if (search) {
    if (!collectSelectItems(props.children).some((item) => search.visibleValues.has(item.value))) return null;
    return <Combobox.Group data-slot='select-group' className={cn('p-1', className)} {...props} />;
  }
  return <SelectPrimitive.Group data-slot='select-group' className={cn('scroll-my-1.5 p-1', className)} {...props} />;
}

function SelectValue({ className, ...props }: SelectPrimitive.Value.Props) {
  const search = React.useContext(SearchableSelectContext);
  if (search) return <SearchableSelectValue className={className} {...props} />;
  return (
    <SelectPrimitive.Value data-slot='select-value' className={cn('flex flex-1 text-left', className)} {...props} />
  );
}

function SearchableSelectValue({
  className,
  children,
  placeholder,
  render,
  style,
  ref,
  ...props
}: SelectPrimitive.Value.Props) {
  const search = React.useContext(SearchableSelectContext)!;
  const state = { value: search.value, placeholder: search.value === null };
  return useRender({
    defaultTagName: 'span',
    render,
    ref,
    state,
    props: {
      ...props,
      'data-slot': 'select-value',
      className: cn('flex flex-1 text-left', typeof className === 'function' ? className(state) : className),
      style: typeof style === 'function' ? style(state) : style,
      children:
        typeof children === 'function'
          ? children(search.value)
          : (children ?? (search.value === null ? undefined : search.labels.get(search.value)) ?? placeholder),
    },
  });
}

function SelectTrigger({
  className,
  size = 'default',
  children,
  ...props
}: SelectPrimitive.Trigger.Props & {
  size?: 'sm' | 'default';
}) {
  const search = React.useContext(SearchableSelectContext);
  const Trigger = search ? SearchableSelectTrigger : SelectPrimitive.Trigger;
  return (
    <Trigger
      data-slot='select-trigger'
      data-size={size}
      className={cn(
        "flex w-fit items-center justify-between gap-1.5 rounded-none border border-transparent bg-input/50 px-3 py-2 text-sm whitespace-nowrap transition-[color,box-shadow] duration-200 outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 data-placeholder:text-muted-foreground data-[size=default]:h-8 data-[size=sm]:h-7 *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-1.5 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    >
      {children}
      {search ? (
        <IconChevronDown className='pointer-events-none size-4 text-muted-foreground' />
      ) : (
        <SelectPrimitive.Icon render={<IconSelector className='pointer-events-none size-4 text-muted-foreground' />} />
      )}
    </Trigger>
  );
}

function SearchableSelectTrigger({ render, style, className, ...props }: SelectPrimitive.Trigger.Props) {
  const search = React.useContext(SearchableSelectContext)!;
  const selectState = (state: Combobox.Trigger.State): SelectPrimitive.Trigger.State => ({
    ...state,
    value: search.value,
    readOnly: search.readOnly,
  });
  return (
    <Combobox.Trigger
      {...props}
      render={typeof render === 'function' ? (elementProps, state) => render(elementProps, selectState(state)) : render}
      style={typeof style === 'function' ? (state) => style(selectState(state)) : style}
      className={typeof className === 'function' ? (state) => className(selectState(state)) : className}
    />
  );
}

function SelectContent({
  className,
  children,
  header,
  showScrollButtons = true,
  side = 'bottom',
  sideOffset = 4,
  align = 'center',
  alignOffset = 0,
  alignItemWithTrigger = true,
  style,
  ...props
}: SelectPrimitive.Popup.Props &
  Pick<SelectPrimitive.Positioner.Props, 'align' | 'alignOffset' | 'side' | 'sideOffset' | 'alignItemWithTrigger'> & {
    header?: React.ReactNode;
    showScrollButtons?: boolean;
  }) {
  const search = React.useContext(SearchableSelectContext);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const layer = Math.max(React.useContext(OverlayLayerContext) + 1, search ? 1302 : 50);
  if (search)
    return (
      <OverlayLayerContext.Provider value={layer}>
        <Combobox.Portal>
          <Combobox.Positioner
            side={side}
            sideOffset={sideOffset}
            align={align}
            alignOffset={alignOffset}
            className='isolate z-50'
            style={{ zIndex: layer }}
          >
            <Combobox.Popup
              data-slot='select-content'
              data-searchable-dropdown=''
              className={cn('searchable-dropdown bg-popover text-popover-foreground', className)}
              style={{ ...overlayTooltipBorderStyle, ...style }}
              initialFocus={inputRef}
              {...props}
            >
              <div data-slot='command-input-wrapper'>
                <InputGroup className='bg-input/30'>
                  <Combobox.Input
                    ref={inputRef}
                    data-slot='command-input'
                    aria-label={search.searchPlaceholder}
                    placeholder={search.searchPlaceholder}
                    className='w-full outline-hidden'
                  />
                  <InputGroupAddon align='inline-end'>
                    {search.query ? (
                      <button
                        type='button'
                        aria-label='Clear search'
                        className='flex size-6 items-center justify-center bg-transparent text-muted-foreground'
                        onClick={() => {
                          search.setQuery('');
                          inputRef.current?.focus();
                        }}
                      >
                        <IconX size={16} />
                      </button>
                    ) : (
                      <IconSearch size={16} className='opacity-50' />
                    )}
                  </InputGroupAddon>
                </InputGroup>
              </div>
              {header ? <div data-slot='select-content-header'>{header}</div> : null}
              <Combobox.Empty className='searchable-dropdown-empty'>{search.emptyMessage}</Combobox.Empty>
              <Combobox.List
                data-slot='command-list'
                className='vertical-scroll-fade-mask no-scrollbar overflow-y-auto'
              >
                {children}
              </Combobox.List>
            </Combobox.Popup>
          </Combobox.Positioner>
        </Combobox.Portal>
      </OverlayLayerContext.Provider>
    );
  return (
    <OverlayLayerContext.Provider value={layer}>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Positioner
          side={side}
          sideOffset={sideOffset}
          align={align}
          alignOffset={alignOffset}
          alignItemWithTrigger={alignItemWithTrigger}
          className='isolate z-50'
          style={{ zIndex: layer }}
        >
          <SelectPrimitive.Popup
            data-slot='select-content'
            data-align-trigger={alignItemWithTrigger}
            className={cn(
              /*
               * CDXC:DesignSystem 2026-06-19-14:16:
               * Select popups can scroll in settings, filters, and project-board
               * forms. Keep their overflow cue consistent with the sidebar's
               * Codex-style scroll-container fade.
               */
              'vertical-scroll-fade-mask relative isolate z-50 max-h-(--available-height) w-(--anchor-width) origin-(--transform-origin) overflow-x-hidden overflow-y-auto rounded-lg bg-popover text-popover-foreground shadow-lg duration-100 data-[align-trigger=true]:animate-none data-[side=bottom]:slide-in-from-top-2 data-[side=inline-end]:slide-in-from-left-2 data-[side=inline-start]:slide-in-from-right-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95',
              className
            )}
            style={{ ...overlayTooltipBorderStyle, ...style }}
            {...props}
          >
            {header ? <div data-slot='select-content-header'>{header}</div> : null}
            {showScrollButtons ? <SelectScrollUpButton /> : null}
            <SelectPrimitive.List>{children}</SelectPrimitive.List>
            {showScrollButtons ? <SelectScrollDownButton /> : null}
          </SelectPrimitive.Popup>
        </SelectPrimitive.Positioner>
      </SelectPrimitive.Portal>
    </OverlayLayerContext.Provider>
  );
}

function SelectLabel({ className, ...props }: SelectPrimitive.GroupLabel.Props) {
  const search = React.useContext(SearchableSelectContext);
  if (search)
    return (
      <Combobox.GroupLabel
        data-slot='select-label'
        className={cn('px-2 py-1 text-xs text-muted-foreground', className)}
        {...props}
      />
    );
  return (
    <SelectPrimitive.GroupLabel
      data-slot='select-label'
      className={cn('px-2 py-1 text-xs text-muted-foreground', className)}
      {...props}
    />
  );
}

function SelectItem({ className, children, ...props }: SelectPrimitive.Item.Props) {
  const search = React.useContext(SearchableSelectContext);
  if (search) {
    if (!search.visibleValues.has(props.value)) return null;
    const { label, ...itemProps } = props;
    return (
      <Combobox.Item
        data-slot='select-item'
        className={cn(
          'relative flex w-full cursor-default items-center gap-2 outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50',
          className
        )}
        {...itemProps}
      >
        <span className='searchable-dropdown-item-label'>{children}</span>
      </Combobox.Item>
    );
  }
  return (
    <SelectPrimitive.Item
      data-slot='select-item'
      className={cn(
        /*
         * CDXC:DesignSystem 2026-09-09 DECISION:
         * User: select rows carry no check mark. The selected row is marked by a different background instead, so the popup can match the trigger width exactly.
         */
        "relative flex min-h-7 w-full cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-hidden select-none focus:bg-accent focus:text-accent-foreground not-data-[variant=destructive]:focus:**:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 *:[span]:last:flex *:[span]:last:items-center *:[span]:last:gap-2",
        className
      )}
      {...props}
    >
      <SelectPrimitive.ItemText className='flex min-w-0 flex-1 gap-2 overflow-hidden text-ellipsis whitespace-nowrap'>
        {children}
      </SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
}

function SelectSeparator({ className, ...props }: SelectPrimitive.Separator.Props) {
  const search = React.useContext(SearchableSelectContext);
  if (search)
    return <Separator data-slot='select-separator' className={cn('my-1 h-px bg-border', className)} {...props} />;
  return (
    <SelectPrimitive.Separator
      data-slot='select-separator'
      className={cn('pointer-events-none -mx-1 my-1 h-px bg-border', className)}
      {...props}
    />
  );
}

function SelectScrollUpButton({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.ScrollUpArrow>) {
  return (
    <SelectPrimitive.ScrollUpArrow
      data-slot='select-scroll-up-button'
      className={cn(
        "top-0 z-10 flex w-full cursor-default items-center justify-center bg-popover py-1 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    >
      <IconChevronUp />
    </SelectPrimitive.ScrollUpArrow>
  );
}

function SelectScrollDownButton({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.ScrollDownArrow>) {
  return (
    <SelectPrimitive.ScrollDownArrow
      data-slot='select-scroll-down-button'
      className={cn(
        "bottom-0 z-10 flex w-full cursor-default items-center justify-center bg-popover py-1 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    >
      <IconChevronDown />
    </SelectPrimitive.ScrollDownArrow>
  );
}

export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
};
