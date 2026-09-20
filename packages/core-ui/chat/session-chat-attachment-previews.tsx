import { IconLoader2, IconX } from '@tabler/icons-react';
import { useMemo } from 'react';
import { AppTooltip } from '../app-tooltip';
import type { PastedImagePreview } from './session-chat-image-attachments';
import { useSessionChatImageViewer } from './session-chat-image-viewer';
import { sessionChatComposerReferences } from './session-chat-reference-pills';
import './session-chat-composer-images.css';

/**
 * CDXC:SessionChat 2026-09-20 DECISION:
 * User: a thumbnail's tooltip names the reference it stands for (`Image #1`) instead of repeating
 * the whole file path, which the reference pill's own tooltip already shows.
 */
export function SessionChatAttachmentPreviews({
  images,
  text,
  pending = 0,
  activeImagePath,
  onRemove,
  disabled = false,
}: {
  images: readonly PastedImagePreview[];
  /** The draft the thumbnails belong to, which is where their `Image #N` labels come from. */
  text: string;
  pending?: number;
  activeImagePath?: string | null;
  onRemove: (image: PastedImagePreview) => void;
  disabled?: boolean;
}) {
  const imageViewer = useSessionChatImageViewer();
  const labels = useMemo(() => {
    const found = new Map<string, string>();
    for (const reference of sessionChatComposerReferences(text)) {
      if (reference.kind === 'image' && !found.has(reference.path)) found.set(reference.path, reference.label);
    }
    return found;
  }, [text]);
  return images.length > 0 || pending > 0 ? (
    <div className='flex flex-wrap items-center gap-2 pb-2'>
      {images.map((image) => (
        <div className='ghostex-chat-composer-image relative' key={image.id}>
          <AppTooltip content={labels.get(image.path) ?? 'Pasted image'}>
            <button
              aria-label='View pasted image'
              className='block rounded-lg'
              disabled={!imageViewer}
              onClick={() =>
                imageViewer?.open({
                  alt: 'Pasted image',
                  url: image.dataUrl,
                })
              }
              type='button'
            >
              <img
                alt='Pasted image'
                className='h-12 w-12 rounded-lg border border-input object-cover'
                data-reference-active={activeImagePath === image.path ? 'true' : undefined}
                src={image.dataUrl}
              />
            </button>
          </AppTooltip>
          <button
            aria-label='Remove image'
            disabled={disabled}
            className='ghostex-chat-composer-image-remove absolute -right-1.5 -top-1.5 flex size-4 items-center justify-center rounded-full border border-input bg-card text-muted-foreground hover:text-foreground'
            onClick={() => onRemove(image)}
            type='button'
          >
            <IconX aria-hidden='true' size={10} stroke={2.4} />
          </button>
        </div>
      ))}
      {pending > 0 ? (
        <div
          aria-label='Saving attachment'
          className='flex h-12 w-12 items-center justify-center rounded-lg border border-dashed border-input text-muted-foreground'
        >
          <IconLoader2 aria-hidden='true' className='animate-spin' size={16} stroke={2} />
        </div>
      ) : null}
    </div>
  ) : null;
}
