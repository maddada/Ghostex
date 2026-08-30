import { IconCheck, IconSelector, IconStarFilled } from '@tabler/icons-react';
import type { CSSProperties } from 'react';
import { Button } from '../components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover';
import type { GxserverStashedPromptTag } from '../shared/gxserver-protocol';
import { GXSERVER_FAVORITE_PROMPT_TAG_ID } from '../shared/gxserver-protocol';

type StashedPromptEditorTagSelectProps = {
  isFavorite: boolean;
  onFavoriteChange: (isFavorite: boolean) => void;
  onTagChange: (tagId: string | undefined) => void;
  selectedTagId: string | undefined;
  tags: readonly GxserverStashedPromptTag[];
};

export function StashedPromptEditorTagSelect({
  isFavorite,
  onFavoriteChange,
  onTagChange,
  selectedTagId,
  tags,
}: StashedPromptEditorTagSelectProps) {
  const favoriteTag = tags.find((tag) => tag.tagId === GXSERVER_FAVORITE_PROMPT_TAG_ID);
  const selectedTag = tags.find((tag) => tag.tagId === selectedTagId);
  const hasSelection = isFavorite || selectedTag !== undefined;

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            aria-label='Tags for saved prompt'
            className='ghostex-stashed-prompt-editor-tag-trigger'
            size='sm'
            type='button'
            variant='outline'
          >
            <span className='ghostex-stashed-prompt-editor-tag-trigger-label'>
              {isFavorite ? (
                <span className='ghostex-stashed-prompt-editor-tag-value'>
                  <IconStarFilled
                    aria-hidden='true'
                    className='ghostex-stashed-prompt-editor-favorite-icon'
                    size={12}
                    style={{ color: favoriteTag?.color }}
                  />
                  <span>{favoriteTag?.name ?? 'Favorite'}</span>
                </span>
              ) : null}
              {selectedTag ? (
                <span className='ghostex-stashed-prompt-editor-tag-value'>
                  <span
                    aria-hidden='true'
                    className='ghostex-stashed-prompt-tag-dot'
                    style={{ '--ghostex-tag-color': selectedTag.color } as CSSProperties}
                  />
                  <span>{selectedTag.name}</span>
                </span>
              ) : null}
              {!hasSelection ? (
                <span className='ghostex-stashed-prompt-editor-tag-value'>
                  <span aria-hidden='true' className='ghostex-stashed-prompt-select-tag-dot' data-tone='none' />
                  <span>No tag</span>
                </span>
              ) : null}
            </span>
            <IconSelector aria-hidden='true' className='ghostex-stashed-prompt-editor-tag-selector' size={14} />
          </Button>
        }
      />
      <PopoverContent
        align='start'
        className='ghostex-stashed-prompt-tag-popover'
        onKeyDown={(event) => event.stopPropagation()}
        sideOffset={4}
      >
        <div className='ghostex-stashed-prompt-tag-popover-title'>Tags</div>
        <div className='ghostex-stashed-prompt-tag-menu-list'>
          <button
            className='ghostex-stashed-prompt-tag-menu-item'
            data-active={String(isFavorite)}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onFavoriteChange(!isFavorite);
            }}
            type='button'
          >
            <IconStarFilled
              aria-hidden='true'
              className='ghostex-stashed-prompt-editor-favorite-icon'
              size={13}
              style={{ color: favoriteTag?.color }}
            />
            <span className='ghostex-stashed-prompt-tag-menu-name'>{favoriteTag?.name ?? 'Favorite'}</span>
            <IconCheck aria-hidden='true' className='ghostex-stashed-prompt-tag-menu-check' size={13} stroke={2.4} />
          </button>
          {tags
            .filter((tag) => tag.tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID)
            .map((tag) => (
              <button
                className='ghostex-stashed-prompt-tag-menu-item'
                data-active={String(selectedTagId === tag.tagId)}
                key={tag.tagId}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  onTagChange(selectedTagId === tag.tagId ? undefined : tag.tagId);
                }}
                style={{ '--ghostex-tag-color': tag.color } as CSSProperties}
                type='button'
              >
                <span aria-hidden='true' className='ghostex-stashed-prompt-tag-dot' />
                <span className='ghostex-stashed-prompt-tag-menu-name'>{tag.name}</span>
                <IconCheck
                  aria-hidden='true'
                  className='ghostex-stashed-prompt-tag-menu-check'
                  size={13}
                  stroke={2.4}
                />
              </button>
            ))}
          <button
            className='ghostex-stashed-prompt-tag-menu-item'
            data-active={String(!hasSelection)}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onFavoriteChange(false);
              onTagChange(undefined);
            }}
            type='button'
          >
            <span aria-hidden='true' className='ghostex-stashed-prompt-select-tag-dot' data-tone='none' />
            <span className='ghostex-stashed-prompt-tag-menu-name'>No tag</span>
            <IconCheck aria-hidden='true' className='ghostex-stashed-prompt-tag-menu-check' size={13} stroke={2.4} />
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
