import { IconHistory } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Popover, PopoverContent, PopoverTitle, PopoverTrigger } from '@/packages/components/ui/popover';
import type { GxserverStashedPrompt } from '@/packages/shared/gxserver-protocol';
import { formatRelativeTimeLabel } from '@/packages/core-ui/relative-time';

export function SessionChatRecoveredHistory({
  versions,
  onSelect,
}: {
  versions: GxserverStashedPrompt[];
  onSelect: (prompt: GxserverStashedPrompt) => void;
}) {
  return (
    <Popover>
      <PopoverTrigger
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
        render={
          <button
            type='button'
            className='inline-flex shrink-0 items-center gap-1 rounded px-1 text-xs text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring'
          />
        }
      >
        <IconHistory aria-hidden='true' size={13} />
        {versions.length} earlier {versions.length === 1 ? 'version' : 'versions'}
      </PopoverTrigger>
      <PopoverContent
        side='top'
        align='start'
        sideOffset={8}
        className='w-[min(30rem,calc(100vw-2rem))] gap-3 rounded-xl p-3.5'
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <PopoverTitle className='text-sm font-medium'>Earlier versions</PopoverTitle>
        <div className='max-h-[min(28rem,55vh)] space-y-3 overflow-y-auto overscroll-contain'>
          {versions.map((version) => (
            <div className='rounded-lg border border-border bg-muted/30 p-3' key={version.promptId}>
              <div className='mb-2 flex items-center justify-between gap-2'>
                <span className='text-xs text-muted-foreground'>{formatRelativeTimeLabel(version.updatedAt)}</span>
                <div className='flex gap-1'>
                  <Button size='sm' variant='ghost' onClick={() => void navigator.clipboard.writeText(version.content)}>
                    Copy
                  </Button>
                  <Button size='sm' variant='secondary' onClick={() => onSelect(version)}>
                    Insert
                  </Button>
                </div>
              </div>
              <div className='max-h-48 overflow-y-auto whitespace-pre-wrap break-words text-sm leading-relaxed select-text'>
                {version.content}
              </div>
            </div>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
