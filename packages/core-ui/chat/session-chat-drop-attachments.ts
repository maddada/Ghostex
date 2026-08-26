/*
Browser drops expose file bytes but deliberately hide absolute local paths.
Directory entries therefore have to be walked while the drop payload is live,
then recreated by the session machine's attachment transport. CEF may expose
an absolute `File.path`; the composer uses that only for a local GPUI session.
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

export function sessionChatNativeDropPaths(dataTransfer: DataTransfer): string[] {
  const transferredFiles = Array.from(dataTransfer.files);
  const files =
    transferredFiles.length > 0
      ? transferredFiles
      : Array.from(dataTransfer.items)
          .filter((item) => item.kind === 'file')
          .map((item) => item.getAsFile())
          .filter((file): file is File => file !== null);
  const paths = files.map((file) => (file as File & { path?: string }).path?.trim() ?? '');
  return paths.length > 0 && paths.every((path) => path !== '') ? [...new Set(paths)] : [];
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

/** Snapshot and resolve a browser drop before the browser releases its data. */
async function readSessionChatDroppedAttachments(dataTransfer: DataTransfer): Promise<SessionChatDroppedAttachments> {
  const items = Array.from(dataTransfer.items);
  const fallbackFiles = Array.from(dataTransfer.files);
  const result: SessionChatDroppedAttachments = { directories: [], files: [] };
  let usedEntries = false;

  for (const item of items) {
    if (item.kind !== 'file') {
      continue;
    }
    const entry = (item as DropItemWithEntry).webkitGetAsEntry?.() as unknown as BrowserFileSystemEntry | null;
    if (!entry) {
      const file = item.getAsFile();
      if (file) {
        result.files.push(file);
      }
      continue;
    }
    usedEntries = true;
    if (entry.isDirectory) {
      const directory: SessionChatDroppedDirectory = {
        directories: [],
        files: [],
        name: entry.name,
      };
      await walkDirectory(entry as BrowserFileSystemDirectoryEntry, '', directory);
      result.directories.push(directory);
    } else if (entry.isFile) {
      result.files.push(await readEntryFile(entry as BrowserFileSystemFileEntry));
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
  dataTransfer: DataTransfer,
  saveAttachment: (payload: SessionChatDropAttachmentUpload) => Promise<string>
): Promise<string[]> {
  const { directories, files } = await readSessionChatDroppedAttachments(dataTransfer);
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
