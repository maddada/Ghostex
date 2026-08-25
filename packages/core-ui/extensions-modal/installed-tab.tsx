import { IconPuzzle } from '@tabler/icons-react';
import type { GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';
import { InstalledExtensionCard } from './extension-card';
import { ExtensionEmptyState, ExtensionGroup } from './extension-surface';

export function InstalledTab({
  extensions,
  iconUrlFor,
  onDetails,
  onRemove,
  onSetChatBarAutoOpen,
  onSetEnabled,
  pendingIds,
}: {
  extensions: readonly GhostexInstalledExtension[];
  iconUrlFor: (extension: GhostexInstalledExtension) => string | undefined;
  onDetails: (extension: GhostexInstalledExtension) => void;
  onRemove: (extension: GhostexInstalledExtension) => void;
  onSetChatBarAutoOpen: (extension: GhostexInstalledExtension, autoOpen: boolean) => void;
  onSetEnabled: (extension: GhostexInstalledExtension, enabled: boolean) => void;
  pendingIds?: ReadonlySet<string>;
}) {
  if (!extensions.length) {
    return (
      <div className='flex h-full min-h-0'>
        <ExtensionEmptyState
          description='Browse the Store to add audited extensions to Ghostex.'
          icon={IconPuzzle}
          title='No extensions installed'
        />
      </div>
    );
  }
  return (
    <div className='vertical-scroll-fade-mask h-full min-h-0 overflow-y-auto p-3 [--edge-fade-distance:16px]'>
      <ExtensionGroup>
        {extensions.map((extension) => (
          <InstalledExtensionCard
            extension={extension}
            iconUrl={iconUrlFor(extension)}
            key={extension.id}
            onDetails={() => onDetails(extension)}
            onRemove={() => onRemove(extension)}
            onSetChatBarAutoOpen={(autoOpen) => onSetChatBarAutoOpen(extension, autoOpen)}
            onSetEnabled={(enabled) => onSetEnabled(extension, enabled)}
            pending={pendingIds?.has(extension.id)}
          />
        ))}
      </ExtensionGroup>
    </div>
  );
}
