import { IconChevronRight, IconLoader2 } from '@tabler/icons-react';
import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import { Toaster, toast } from 'sonner';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { InputGroup, InputGroupAddon, InputGroupInput } from '@/packages/components/ui/input-group';
import { cn } from '@/packages/components/utils';
import type { SessionChatTheme } from '@/packages/shared/session-chat';

export type SaveSessionMessageMarkdown = (params: { content: string; path: string }) => Promise<{ path: string }>;
export type ListSessionMessageMarkdownPaths = () => Promise<readonly string[]>;

function localDateDirectory(date = new Date()): string {
  const year = String(date.getFullYear()).padStart(4, '0');
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function normalizedMarkdownStem(value: string): string {
  return value.trim().replace(/\.md$/iu, '').trim();
}

function normalizedFolderPath(value: string): string {
  return value
    .trim()
    .split('/')
    .map((segment) => segment.trim())
    .join('/');
}

function folderPathError(value: string): string | undefined {
  const path = normalizedFolderPath(value);
  if (path === '') {
    return 'Enter a folder name.';
  }
  if (path.length > 240) {
    return 'Use a folder path of 240 characters or fewer.';
  }
  for (const segment of path.split('/')) {
    if (segment === '') {
      return 'Enter a folder name between each slash.';
    }
    if (segment === '.' || segment === '..' || /[. ]$/u.test(segment)) {
      return 'Folder names cannot be a period or end with a period or space.';
    }
    if (/[\\<>:"|?*\u0000-\u001f]/u.test(segment)) {
      return 'A folder name contains a character that cannot be used in a path.';
    }
    if (/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])$/iu.test(segment)) {
      return 'A folder name is reserved by the operating system.';
    }
  }
  return undefined;
}

function sessionMarkdownBase(sessionTitle: string): string {
  const title = sessionTitle
    .trim()
    .replace(/[\\/<>:"|?*\u0000-\u001f]/gu, ' ')
    .replace(/\s+/gu, ' ')
    .replace(/[. ]+$/gu, '')
    .trim();
  return (title || 'Saved response').slice(0, 110).trimEnd();
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

function suggestedMarkdownStem(sessionTitle: string, folderPath: string, existingPaths: readonly string[]): string {
  const base = sessionMarkdownBase(sessionTitle);
  const suffixSeparator = base.includes('-') ? '-' : ' ';
  const prefix = `docs/${normalizedFolderPath(folderPath)}/`;
  const numberedName = new RegExp(`^${escapeRegExp(base)}${escapeRegExp(suffixSeparator)}(\\d+)\\.md$`, 'iu');
  let highestSuffix = 0;
  for (const path of existingPaths) {
    if (!path.toLocaleLowerCase().startsWith(prefix.toLocaleLowerCase())) {
      continue;
    }
    const name = path.slice(prefix.length);
    if (name.includes('/')) {
      continue;
    }
    const match = numberedName.exec(name);
    const suffix = match?.[1] ? Number.parseInt(match[1], 10) : 0;
    highestSuffix = Math.max(highestSuffix, suffix);
  }
  return `${base}${suffixSeparator}${highestSuffix + 1}`;
}

function markdownStemError(value: string): string | undefined {
  const stem = normalizedMarkdownStem(value);
  if (stem === '') {
    return 'Enter a file name.';
  }
  if (stem.length > 120) {
    return 'Use a file name of 120 characters or fewer.';
  }
  if (/[\\/<>:"|?*\u0000-\u001f]/u.test(stem)) {
    return 'The file name contains a character that cannot be used in a path.';
  }
  if (stem === '.' || stem === '..' || /[. ]$/u.test(stem)) {
    return 'Enter a file name without a trailing period or space.';
  }
  if (/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$/iu.test(stem)) {
    return 'That file name is reserved by the operating system.';
  }
  return undefined;
}

export function SessionChatSaveMarkdownDialog({
  listExistingPaths,
  markdown,
  onOpenChange,
  open,
  save,
  sessionTitle,
  theme,
}: {
  listExistingPaths: ListSessionMessageMarkdownPaths;
  markdown: string;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  save: SaveSessionMessageMarkdown;
  sessionTitle: string;
  theme: SessionChatTheme;
}) {
  const fileNameInputId = useId();
  const folderInputId = useId();
  const folderNameRef = useRef(localDateDirectory());
  const inputRef = useRef<HTMLInputElement>(null);
  const usesSuggestedNameRef = useRef(true);
  const toasterId = useId();
  const [existingPaths, setExistingPaths] = useState<readonly string[] | null>(null);
  const [fileName, setFileName] = useState('');
  const [fileNameError, setFileNameError] = useState<string>();
  const [folderName, setFolderName] = useState(localDateDirectory);
  const [folderNameError, setFolderNameError] = useState<string>();
  const [listingError, setListingError] = useState<string>();
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    let loadedSelectionFrame: number | undefined;
    const initialFolder = localDateDirectory();
    usesSuggestedNameRef.current = true;
    folderNameRef.current = initialFolder;
    setExistingPaths(null);
    setFolderName(initialFolder);
    setFileName(suggestedMarkdownStem(sessionTitle, initialFolder, []));
    setFileNameError(undefined);
    setFolderNameError(undefined);
    setListingError(undefined);
    setSaving(false);
    const selectionFrame = requestAnimationFrame(() => inputRef.current?.select());
    void listExistingPaths()
      .then((paths) => {
        if (!active) {
          return;
        }
        setExistingPaths(paths);
        if (usesSuggestedNameRef.current) {
          const fileNameWasFocused = document.activeElement === inputRef.current;
          setFileName(suggestedMarkdownStem(sessionTitle, folderNameRef.current, paths));
          if (fileNameWasFocused) {
            loadedSelectionFrame = requestAnimationFrame(() => inputRef.current?.select());
          }
        }
      })
      .catch((error: unknown) => {
        if (!active) {
          return;
        }
        setListingError(error instanceof Error ? error.message : 'Could not read the project Docs files.');
      });
    return () => {
      active = false;
      cancelAnimationFrame(selectionFrame);
      if (loadedSelectionFrame !== undefined) {
        cancelAnimationFrame(loadedSelectionFrame);
      }
    };
  }, [listExistingPaths, open, sessionTitle]);

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    const folderValidationError = folderPathError(folderName);
    const fileNameValidationError = markdownStemError(fileName);
    setFolderNameError(folderValidationError);
    setFileNameError(fileNameValidationError);
    if (folderValidationError || fileNameValidationError || existingPaths === null) {
      return;
    }
    const relativePath = `docs/${normalizedFolderPath(folderName)}/${normalizedMarkdownStem(fileName)}.md`;
    setSaving(true);
    setFileNameError(undefined);
    try {
      const result = await save({ content: markdown, path: relativePath });
      await navigator.clipboard.writeText(result.path);
      onOpenChange(false);
      toast.success('Saved to Markdown', {
        description: `${result.path} was copied to the clipboard.`,
        toasterId,
      });
    } catch (error) {
      setFileNameError(error instanceof Error ? error.message : 'Could not save the Markdown file.');
      setSaving(false);
    }
  };

  const displayedFolderError = folderNameError ?? listingError;

  return (
    <>
      <Dialog
        onOpenChange={(nextOpen) => {
          if (!saving || nextOpen) {
            onOpenChange(nextOpen);
          }
        }}
        open={open}
      >
        <DialogContent
          className={cn(
            'ghostex-session-chat-popup w-full max-w-md rounded-xl font-sans [--radius:0.625rem]',
            theme === 'dark' && 'dark'
          )}
        >
          <form className='flex flex-col gap-6' onSubmit={(event) => void submit(event)}>
            <DialogHeader>
              <DialogTitle>Save to Markdown</DialogTitle>
              <DialogDescription>
                Save this final response in the project Docs folder. Its full path will be copied after saving.
              </DialogDescription>
            </DialogHeader>
            <FieldGroup>
              <Field data-invalid={displayedFolderError !== undefined}>
                <FieldLabel htmlFor={folderInputId}>Folder</FieldLabel>
                <InputGroup>
                  <InputGroupAddon align='inline-start'>…/docs/</InputGroupAddon>
                  <InputGroupInput
                    aria-invalid={displayedFolderError !== undefined}
                    autoCapitalize='none'
                    autoComplete='off'
                    disabled={saving}
                    id={folderInputId}
                    onChange={(event) => {
                      const nextFolder = event.currentTarget.value;
                      folderNameRef.current = nextFolder;
                      setFolderName(nextFolder);
                      setFolderNameError(undefined);
                      if (usesSuggestedNameRef.current && existingPaths !== null) {
                        setFileName(suggestedMarkdownStem(sessionTitle, nextFolder, existingPaths));
                      }
                    }}
                    placeholder={localDateDirectory()}
                    spellCheck={false}
                    value={folderName}
                  />
                </InputGroup>
                <FieldDescription>Use / to create nested folders.</FieldDescription>
                <FieldError>{displayedFolderError}</FieldError>
              </Field>
              <Field data-invalid={fileNameError !== undefined}>
                <FieldLabel htmlFor={fileNameInputId}>File name</FieldLabel>
                <InputGroup>
                  <InputGroupInput
                    aria-invalid={fileNameError !== undefined}
                    autoCapitalize='none'
                    autoComplete='off'
                    autoFocus
                    disabled={saving}
                    id={fileNameInputId}
                    onChange={(event) => {
                      usesSuggestedNameRef.current = false;
                      setFileName(event.currentTarget.value.replace(/\.md$/iu, ''));
                      setFileNameError(undefined);
                    }}
                    onFocus={(event) => {
                      if (usesSuggestedNameRef.current) {
                        event.currentTarget.select();
                      }
                    }}
                    placeholder='response-name'
                    ref={inputRef}
                    spellCheck={false}
                    value={fileName}
                  />
                  <InputGroupAddon align='inline-end'>.md</InputGroupAddon>
                </InputGroup>
                <FieldError>{fileNameError}</FieldError>
              </Field>
            </FieldGroup>
            <DialogFooter>
              <Button disabled={saving} onClick={() => onOpenChange(false)} type='button' variant='outline'>
                Cancel
              </Button>
              <Button disabled={saving || existingPaths === null} type='submit' variant='outline'>
                {saving || existingPaths === null ? (
                  <IconLoader2 className='animate-spin' data-icon='inline-start' />
                ) : null}
                Save to md
                <IconChevronRight aria-hidden='true' data-icon='inline-end' />
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
      <Toaster
        id={toasterId}
        position='bottom-center'
        richColors
        theme={theme}
        toastOptions={{
          style: {
            background: 'var(--popover)',
            border: '1px solid var(--border)',
            color: 'var(--popover-foreground)',
          },
        }}
      />
    </>
  );
}
