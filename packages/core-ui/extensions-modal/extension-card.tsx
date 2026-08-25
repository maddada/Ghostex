import { IconArrowUpRight } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/packages/components/ui/card';
import { Switch } from '@/packages/components/ui/switch';
import type { GhostexExtensionCatalogEntry, GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';

export function ExtensionIcon({ src, title }: { src?: string; title: string }) {
  return <img alt={`${title} icon`} className='size-12 shrink-0 object-contain' src={src} />;
}

export function InstalledExtensionCard({
  extension,
  iconUrl,
  onDetails,
  onRemove,
  onSetEnabled,
  pending,
}: {
  extension: GhostexInstalledExtension;
  iconUrl?: string;
  onDetails: () => void;
  onRemove: () => void;
  onSetEnabled: (enabled: boolean) => void;
  pending?: boolean;
}) {
  return (
    <Card className='min-h-52 bg-card/45 py-5 ring-border/70' data-extension-id={extension.id}>
      <CardHeader className='grid grid-cols-[auto_minmax(0,1fr)] gap-4 px-5'>
        <ExtensionIcon src={iconUrl} title={extension.manifest.title} />
        <div className='min-w-0'>
          <CardTitle className='truncate text-[15px] font-normal'>{extension.manifest.title}</CardTitle>
          <CardDescription className='mt-1 line-clamp-3 leading-5'>{extension.manifest.description}</CardDescription>
        </div>
      </CardHeader>
      <CardContent className='mt-auto px-5 text-xs text-muted-foreground'>
        Version {extension.state.version}
      </CardContent>
      <CardFooter className='justify-between gap-4 px-5'>
        <div className='flex items-center gap-2'>
          <Button disabled={pending} onClick={onDetails} size='sm' type='button' variant='outline'>
            Details
          </Button>
          <Button disabled={pending} onClick={onRemove} size='sm' type='button' variant='outline'>
            Remove
          </Button>
        </div>
        <Switch
          aria-label={`${extension.state.enabled ? 'Disable' : 'Enable'} ${extension.manifest.title}`}
          checked={extension.state.enabled}
          disabled={pending}
          onCheckedChange={onSetEnabled}
        />
      </CardFooter>
    </Card>
  );
}

export function StoreExtensionCard({
  entry,
  iconUrl,
  installedVersion,
  onDetails,
}: {
  entry: GhostexExtensionCatalogEntry;
  iconUrl?: string;
  installedVersion?: string;
  onDetails: () => void;
}) {
  return (
    <Card className='min-h-52 bg-card/45 py-5 ring-border/70' data-extension-id={entry.name}>
      <CardHeader className='grid grid-cols-[auto_minmax(0,1fr)] gap-4 px-5'>
        <ExtensionIcon src={iconUrl} title={entry.title} />
        <div className='min-w-0'>
          <CardTitle className='truncate text-[15px] font-normal'>{entry.title}</CardTitle>
          <CardDescription className='mt-1 line-clamp-3 leading-5'>{entry.description}</CardDescription>
        </div>
      </CardHeader>
      <CardContent className='mt-auto flex flex-wrap gap-1.5 px-5 text-xs text-muted-foreground'>
        {entry.categories.slice(0, 3).map((category) => (
          <span className='border border-border/70 px-2 py-0.5' key={category}>
            {category}
          </span>
        ))}
      </CardContent>
      <CardFooter className='justify-between gap-4 px-5'>
        <span className='text-xs text-muted-foreground'>
          {installedVersion ? `Installed ${installedVersion}` : `Version ${entry.version}`}
        </span>
        <Button onClick={onDetails} size='sm' type='button' variant='outline'>
          Details
          <IconArrowUpRight data-icon='inline-end' />
        </Button>
      </CardFooter>
    </Card>
  );
}
