import { IconSearch } from '@tabler/icons-react';
import { useMemo, useState } from 'react';
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from '@/packages/components/ui/empty';
import { Field, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { Input } from '@/packages/components/ui/input';
import { ToggleGroup, ToggleGroupItem } from '@/packages/components/ui/toggle-group';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionPlacement,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { StoreExtensionCard } from './extension-card';

type TypeFilter = 'all' | GhostexExtensionPlacement | 'terminal-pane';

function entrySupportsType(entry: GhostexExtensionCatalogEntry, type: TypeFilter): boolean {
  if (type === 'all') return true;
  return entry.kind === 'terminal-pane'
    ? type === 'terminal-pane'
    : entry.placements.includes(type as GhostexExtensionPlacement);
}

function singleFilterValue(values: unknown, fallback: string): string {
  return Array.isArray(values) && typeof values[0] === 'string' ? values[0] : fallback;
}

export function StoreTab({
  catalog,
  iconUrlFor,
  installed,
  onDetails,
}: {
  catalog: readonly GhostexExtensionCatalogEntry[];
  iconUrlFor: (entry: GhostexExtensionCatalogEntry) => string | undefined;
  installed: readonly GhostexInstalledExtension[];
  onDetails: (entry: GhostexExtensionCatalogEntry) => void;
}) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('all');
  const [type, setType] = useState<TypeFilter>('all');
  const categories = useMemo(
    () =>
      Array.from(new Set(catalog.flatMap((entry) => entry.categories))).sort((left, right) =>
        left.localeCompare(right)
      ),
    [catalog]
  );
  const installedById = useMemo(() => new Map(installed.map((extension) => [extension.id, extension])), [installed]);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filtered = catalog.filter((entry) => {
    const matchesQuery =
      !normalizedQuery ||
      `${entry.title} ${entry.description} ${entry.author} ${entry.categories.join(' ')}`
        .toLocaleLowerCase()
        .includes(normalizedQuery);
    return (
      matchesQuery && (category === 'all' || entry.categories.includes(category)) && entrySupportsType(entry, type)
    );
  });

  return (
    <div className='flex flex-col gap-5'>
      <FieldGroup className='gap-3'>
        <Field>
          <FieldLabel className='sr-only' htmlFor='extensions-store-search'>
            Search extensions
          </FieldLabel>
          <Input
            id='extensions-store-search'
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder='Search extensions'
            value={query}
          />
        </Field>
      </FieldGroup>
      <div className='flex flex-col gap-3'>
        <div className='overflow-x-auto pb-1'>
          <ToggleGroup
            aria-label='Filter extensions by type'
            onValueChange={(values) => setType(singleFilterValue(values, 'all') as TypeFilter)}
            spacing={2}
            value={[type]}
            variant='outline'
          >
            {(['all', 'view', 'chat-bar', 'popup', 'modal', 'terminal-pane'] as const).map((value) => (
              <ToggleGroupItem key={value} size='sm' value={value}>
                {value === 'all'
                  ? 'All types'
                  : value === 'chat-bar'
                    ? 'Chat bar'
                    : value === 'terminal-pane'
                      ? 'Terminal pane'
                      : value[0].toUpperCase() + value.slice(1)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
        {categories.length ? (
          <div className='overflow-x-auto pb-1'>
            <ToggleGroup
              aria-label='Filter extensions by category'
              onValueChange={(values) => setCategory(singleFilterValue(values, 'all'))}
              spacing={2}
              value={[category]}
              variant='outline'
            >
              <ToggleGroupItem size='sm' value='all'>
                All categories
              </ToggleGroupItem>
              {categories.map((value) => (
                <ToggleGroupItem key={value} size='sm' value={value}>
                  {value}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </div>
        ) : null}
      </div>
      {filtered.length ? (
        <div className='grid grid-cols-1 gap-4 min-[700px]:grid-cols-2'>
          {filtered.map((entry) => (
            <StoreExtensionCard
              entry={entry}
              iconUrl={iconUrlFor(entry)}
              installedVersion={installedById.get(entry.name)?.state.version}
              key={entry.name}
              onDetails={() => onDetails(entry)}
            />
          ))}
        </div>
      ) : (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant='icon'>
              <IconSearch />
            </EmptyMedia>
            <EmptyTitle>No matching extensions</EmptyTitle>
            <EmptyDescription>Try a different search or clear one of the filters.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </div>
  );
}
