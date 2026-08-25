import { IconPuzzle } from '@tabler/icons-react';
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from '@/packages/components/ui/empty';
import type { GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';
import { InstalledExtensionCard } from './extension-card';

export function InstalledTab({
  extensions,
  iconUrlFor,
  onDetails,
  onRemove,
  onSetEnabled,
  pendingIds,
}: {
  extensions: readonly GhostexInstalledExtension[];
  iconUrlFor: (extension: GhostexInstalledExtension) => string | undefined;
  onDetails: (extension: GhostexInstalledExtension) => void;
  onRemove: (extension: GhostexInstalledExtension) => void;
  onSetEnabled: (extension: GhostexInstalledExtension, enabled: boolean) => void;
  pendingIds?: ReadonlySet<string>;
}) {
  if (!extensions.length) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyMedia variant='icon'>
            <IconPuzzle />
          </EmptyMedia>
          <EmptyTitle>No extensions installed</EmptyTitle>
          <EmptyDescription>Browse the Store to add audited extensions to Ghostex.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  return (
    <div className='grid grid-cols-1 gap-4 min-[700px]:grid-cols-2'>
      {extensions.map((extension) => (
        <InstalledExtensionCard
          extension={extension}
          iconUrl={iconUrlFor(extension)}
          key={extension.id}
          onDetails={() => onDetails(extension)}
          onRemove={() => onRemove(extension)}
          onSetEnabled={(enabled) => onSetEnabled(extension, enabled)}
          pending={pendingIds?.has(extension.id)}
        />
      ))}
    </div>
  );
}
