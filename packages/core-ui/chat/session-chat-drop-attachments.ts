/*
Browser drops expose file bytes but deliberately hide absolute local paths.
Directory entries therefore have to be walked while the drop payload is live,
then recreated by the session machine's attachment transport. Chromium (and so
CEF) never exposes `File.path` to a page, so a local GPUI session gets the
drag's absolute paths from the host shell, which captures them natively at
drag-enter; the composer uses those only for a session on this machine.
*/

export interface SessionChatDroppedDirectory {
  directories: string[];
  files: SessionChatDroppedFile[];
  name: string;
}

export interface SessionChatDroppedFile {
  file: File;
  relativePath: string;
}

export interface SessionChatDroppedAttachments {
  directories: SessionChatDroppedDirectory[];
  files: File[];
}

interface BrowserFileSystemEntry {
  isDirectory: boolean;
  isFile: boolean;
  name: string;
}

interface BrowserFileSystemFileEntry extends BrowserFileSystemEntry {
  file: (success: (file: File) => void, failure?: (error: DOMException) => void) => void;
}

interface BrowserFileSystemDirectoryReader {
  readEntries: (success: (entries: BrowserFileSystemEntry[]) => void, failure?: (error: DOMException) => void) => void;
}

interface BrowserFileSystemDirectoryEntry extends BrowserFileSystemEntry {
  createReader: () => BrowserFileSystemDirectoryReader;
}

type DropItemWithEntry = DataTransferItem & {
  webkitGetAsEntry?: () => BrowserFileSystemEntry | null;
};

export function sessionChatDataTransferHasFiles(dataTransfer: DataTransfer): boolean {
  return Array.from(dataTransfer.types).includes('Files');
}

/** Cleans the host-captured absolute paths of the drag currently over the page. */
export function sessionChatNativeDropPaths(hostDragPaths: readonly string[] | undefined): string[] {
  const paths = (hostDragPaths ?? []).map((path) => path.trim()).filter((path) => path !== '');
  return [...new Set(paths)];
}

function createSessionChatDropUploadId(): string {
  return typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function readEntryFile(entry: BrowserFileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}

function readDirectoryEntries(entry: BrowserFileSystemDirectoryEntry): Promise<BrowserFileSystemEntry[]> {
  const reader = entry.createReader();
  const entries: BrowserFileSystemEntry[] = [];
  return new Promise((resolve, reject) => {
    const readBatch = (): void => {
      reader.readEntries((batch) => {
        if (batch.length === 0) {
          resolve(entries);
          return;
        }
        entries.push(...batch);
        readBatch();
      }, reject);
    };
    readBatch();
  });
}

async function walkDirectory(
  entry: BrowserFileSystemDirectoryEntry,
  relativeDirectory: string,
  result: SessionChatDroppedDirectory
): Promise<void> {
  if (relativeDirectory !== '') {
    result.directories.push(relativeDirectory);
  }
  for (const child of await readDirectoryEntries(entry)) {
    const relativePath = relativeDirectory === '' ? child.name : `${relativeDirectory}/${child.name}`;
    if (child.isDirectory) {
      await walkDirectory(child as BrowserFileSystemDirectoryEntry, relativePath, result);
    } else if (child.isFile) {
      result.files.push({ file: await readEntryFile(child as BrowserFileSystemFileEntry), relativePath });
    }
  }
}

/**
 * Snapshot and resolve a browser drop before the browser releases its data.
 * Must be called synchronously from the drop event handler: the DataTransfer's
 * items are neutered once the handler returns.
 */
export async function readSessionChatDroppedAttachments(
  dataTransfer: DataTransfer
): Promise<SessionChatDroppedAttachments> {
  const items = Array.from(dataTransfer.items);
  const fallbackFiles = Array.from(dataTransfer.files);
  const result: SessionChatDroppedAttachments = { directories: [], files: [] };
  let usedEntries = false;

  for (const item of items) {
    if (item.kind !== 'file') {
      continue;
    }
    const entry = (item as DropItemWithEntry).webkitGetAsEntry?.() as unknown as BrowserFileSystemEntry | null;
    if (entry?.isDirectory) {
      usedEntries = true;
      const directory: SessionChatDroppedDirectory = {
        directories: [],
        files: [],
        name: entry.name,
      };
      await walkDirectory(entry as BrowserFileSystemDirectoryEntry, '', directory);
      result.directories.push(directory);
      continue;
    }
    // Top-level files read through the DataTransfer's own File object. Their
    // FileSystem-entry mirror cannot be read on file:// pages (Chromium
    // rejects the isolated-filesystem URL with an EncodingError), which is
    // exactly where the CEF chat surface runs; only directories, which have
    // no File object, go through the entry API.
    const file = item.getAsFile();
    if (file) {
      result.files.push(file);
    }
  }

  if (!usedEntries && result.files.length === 0) {
    result.files = fallbackFiles;
  }
  return result;
}

export interface SessionChatDropAttachmentUpload {
  base64Data: string;
  directory?: boolean;
  relativePath?: string;
  suggestedName?: string;
  uploadId?: string;
}

function readDroppedFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('Dropped file could not be read.'));
    reader.onload = () => {
      const dataUrl = typeof reader.result === 'string' ? reader.result : '';
      resolve(dataUrl.split(',', 2)[1] ?? '');
    };
    reader.readAsDataURL(file);
  });
}

async function uploadDroppedDirectory(
  directory: SessionChatDroppedDirectory,
  saveAttachment: (payload: SessionChatDropAttachmentUpload) => Promise<string>
): Promise<string> {
  const uploadId = createSessionChatDropUploadId();
  const rootPath = await saveAttachment({
    base64Data: '',
    directory: true,
    suggestedName: directory.name,
    uploadId,
  });
  for (const relativePath of directory.directories) {
    await saveAttachment({
      base64Data: '',
      directory: true,
      relativePath,
      suggestedName: directory.name,
      uploadId,
    });
  }
  for (const droppedFile of directory.files) {
    await saveAttachment({
      base64Data: await readDroppedFileAsBase64(droppedFile.file),
      relativePath: droppedFile.relativePath,
      suggestedName: directory.name,
      uploadId,
    });
  }
  return rootPath;
}

export async function uploadSessionChatDroppedAttachments(
  { directories, files }: SessionChatDroppedAttachments,
  saveAttachment: (payload: SessionChatDropAttachmentUpload) => Promise<string>
): Promise<string[]> {
  const paths: string[] = [];
  for (const file of files) {
    paths.push(
      await saveAttachment({
        base64Data: await readDroppedFileAsBase64(file),
        ...(file.name ? { suggestedName: file.name } : {}),
      })
    );
  }
  for (const directory of directories) {
    paths.push(await uploadDroppedDirectory(directory, saveAttachment));
  }
  return paths;
}
