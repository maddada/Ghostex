import { IconRefresh, IconSearch } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Field, FieldLabel } from '@/packages/components/ui/field';
import { InputGroup, InputGroupAddon, InputGroupInput } from '@/packages/components/ui/input-group';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { APP_MODAL_SELECT_CONTENT_CLASS } from '@/packages/core-ui/app-modal-shell';
import {
  EXTENSION_TYPE_FILTERS,
  extensionSourceLabel,
  extensionTypeLabel,
  type ExtensionFilter,
  type ExtensionSourceFilter,
  type ExtensionTypeFilter,
} from './extension-filter';

export function ExtensionsFilterBar({
  categories,
  filter,
  loading,
  onChange,
  onRefresh,
  shownCount,
  sources,
  totalCount,
}: {
  categories: readonly string[];
  filter: ExtensionFilter;
  loading?: boolean;
  onChange: (filter: ExtensionFilter) => void;
  onRefresh?: () => void;
  shownCount: number;
  /** The sources present on this page; the source filter is hidden when there is only one. */
  sources: readonly Exclude<ExtensionSourceFilter, 'all'>[];
  totalCount: number;
}) {
  return (
    <div className='extensions-filter-bar'>
      <Field className='extensions-filter-search gap-0'>
        <FieldLabel className='sr-only' htmlFor='extensions-filter-search'>
          Search extensions
        </FieldLabel>
        <InputGroup className='h-8'>
          <InputGroupAddon>
            <IconSearch aria-hidden='true' />
          </InputGroupAddon>
          <InputGroupInput
            /*
             * CDXC:Extensions 2026-08-30:
             * This list is embedded in the Settings Extensions page, so the
             * query field must never autofocus: opening Settings on this page
             * would steal focus from the global settings search field.
             */
            className='h-8 font-normal'
            id='extensions-filter-search'
            onChange={(event) => onChange({ ...filter, query: event.currentTarget.value })}
            placeholder='Search extensions'
            value={filter.query}
          />
        </InputGroup>
      </Field>
      {sources.length > 1 ? (
        <Select
          onValueChange={(value) => onChange({ ...filter, source: value as ExtensionSourceFilter })}
          value={filter.source}
        >
          <SelectTrigger aria-label='Filter extensions by source' className='w-32 font-normal'>
            <SelectValue>{extensionSourceLabel(filter.source)}</SelectValue>
          </SelectTrigger>
          <SelectContent align='end' className={APP_MODAL_SELECT_CONTENT_CLASS}>
            <SelectGroup>
              {(['all', ...sources] as const).map((value) => (
                <SelectItem key={value} value={value}>
                  {extensionSourceLabel(value)}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      ) : null}
      <Select
        onValueChange={(value) => onChange({ ...filter, type: value as ExtensionTypeFilter })}
        value={filter.type}
      >
        <SelectTrigger aria-label='Filter extensions by type' className='w-32 font-normal'>
          <SelectValue>{extensionTypeLabel(filter.type)}</SelectValue>
        </SelectTrigger>
        <SelectContent align='end' className={APP_MODAL_SELECT_CONTENT_CLASS}>
          <SelectGroup>
            {EXTENSION_TYPE_FILTERS.map((value) => (
              <SelectItem key={value} value={value}>
                {extensionTypeLabel(value)}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      {categories.length ? (
        <Select onValueChange={(value) => onChange({ ...filter, category: value as string })} value={filter.category}>
          <SelectTrigger aria-label='Filter extensions by category' className='w-40 font-normal'>
            <SelectValue>{filter.category === 'all' ? 'All categories' : filter.category}</SelectValue>
          </SelectTrigger>
          <SelectContent align='end' className={APP_MODAL_SELECT_CONTENT_CLASS}>
            <SelectGroup>
              <SelectItem value='all'>All categories</SelectItem>
              {categories.map((value) => (
                <SelectItem key={value} value={value}>
                  {value}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      ) : null}
      <span className='extensions-filter-count'>
        {shownCount === totalCount ? `${totalCount} shown` : `${shownCount} of ${totalCount} shown`}
      </span>
      {onRefresh ? (
        <Button
          aria-label='Refresh extensions'
          disabled={loading}
          onClick={onRefresh}
          size='icon-sm'
          type='button'
          variant='ghost'
        >
          <IconRefresh />
        </Button>
      ) : null}
    </div>
  );
}
