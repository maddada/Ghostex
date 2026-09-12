import { Command, CommandInput, CommandList, CommandEmpty, CommandItem } from '../components/ui/command';
import { SearchableDropdownContent } from '../components/ui/searchable-dropdown';
import { IconSelector, IconStarFilled } from '@tabler/icons-react';
import type { CSSProperties } from 'react';
import { Button } from '../components/ui/button';
import { Popover, PopoverTrigger } from '../components/ui/popover';
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
      <SearchableDropdownContent
        align='start'
        className='ghostex-stashed-prompt-tag-popover'
        onKeyDown={(event) => event.stopPropagation()}
        sideOffset={4}
      >
        <Command>
          <CommandInput autoFocus placeholder='Filter tags...' aria-label='Filter tags' clearOnEscape={false} />
          <CommandList aria-multiselectable>
            <CommandEmpty>No tags found.</CommandEmpty>
            <CommandItem
              className='ghostex-stashed-prompt-tag-menu-item'
              data-checked={isFavorite}
              aria-selected={isFavorite}
              value='favorite'
              keywords={[favoriteTag?.name ?? 'Favorite']}
              onSelect={() => {
                onFavoriteChange(!isFavorite);
              }}
            >
              <span className='ghostex-stashed-prompt-tag-menu-marker'>
                <IconStarFilled
                  aria-hidden='true'
                  className='ghostex-stashed-prompt-editor-favorite-icon'
                  size={13}
                  style={{ color: favoriteTag?.color }}
                />
              </span>
              <span className='ghostex-stashed-prompt-tag-menu-name'>{favoriteTag?.name ?? 'Favorite'}</span>
            </CommandItem>
            {tags
              .filter((tag) => tag.tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID)
              .map((tag) => (
                <CommandItem
                  className='ghostex-stashed-prompt-tag-menu-item'
                  data-checked={selectedTagId === tag.tagId}
                  aria-selected={selectedTagId === tag.tagId}
                  value={tag.tagId}
                  keywords={[tag.name]}
                  key={tag.tagId}
                  onSelect={() => {
                    onTagChange(selectedTagId === tag.tagId ? undefined : tag.tagId);
                  }}
                  style={{ '--ghostex-tag-color': tag.color } as CSSProperties}
                >
                  <span className='ghostex-stashed-prompt-tag-menu-marker'>
                    <span aria-hidden='true' className='ghostex-stashed-prompt-tag-dot' />
                  </span>
                  <span className='ghostex-stashed-prompt-tag-menu-name'>{tag.name}</span>
                </CommandItem>
              ))}
            <CommandItem
              className='ghostex-stashed-prompt-tag-menu-item'
              data-checked={!hasSelection}
              aria-selected={!hasSelection}
              value='No tag'
              onSelect={() => {
                onFavoriteChange(false);
                onTagChange(undefined);
              }}
            >
              <span className='ghostex-stashed-prompt-tag-menu-marker'>
                <span aria-hidden='true' className='ghostex-stashed-prompt-select-tag-dot' data-tone='none' />
              </span>
              <span className='ghostex-stashed-prompt-tag-menu-name'>No tag</span>
            </CommandItem>
          </CommandList>
        </Command>
      </SearchableDropdownContent>
    </Popover>
  );
}
