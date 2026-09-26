/**
 * The glass video library as Settings sees it: the videos the app lists (bundled, downloaded or
 * available to download), their download progress, and the helpers the gallery uses.
 * The app side is apps/desktop/src/app/helpers/glass_video_library.rs.
 */

/** A library video saved in `windowGlassVideoDark` / `windowGlassVideoLight`, next to `aerial:<id>` and file paths. */
export const GLASS_VIDEO_LIBRARY_PREFIX = 'library:';

/** How long a newly added video keeps its "New" badge. */
export const GLASS_VIDEO_NEW_DAYS = 7;

export type GlassVideoLibraryState = 'bundled' | 'downloaded' | 'available';

export type GlassVideoLibraryEntry = {
  id: string;
  name: string;
  tags: string[];
  /** Which appearance it suits: a hint on the card, never a filter. */
  tone?: 'dark' | 'light' | 'any';
  added?: string;
  durationSeconds?: number;
  sizeBytes?: number;
  state: GlassVideoLibraryState;
  /** Data URL or hosted URL; absent while offline for a video not on this computer. */
  poster?: string;
  preview?: string;
};

export type GlassVideoLibraryListing = {
  online: boolean;
  storageBytes: number;
  videos: GlassVideoLibraryEntry[];
};

export type GlassVideoDownload = { received: number; total?: number; error?: string };

export function glassVideoLibraryReference(id: string): string {
  return `${GLASS_VIDEO_LIBRARY_PREFIX}${id}`;
}

export function glassVideoLibraryId(value: string): string | undefined {
  return value.startsWith(GLASS_VIDEO_LIBRARY_PREFIX) ? value.slice(GLASS_VIDEO_LIBRARY_PREFIX.length) : undefined;
}

/** Whether a video added on `added` (YYYY-MM-DD) still gets the "New" badge on `now`. */
export function isGlassVideoNew(added: string | undefined, now: Date = new Date()): boolean {
  if (!added || !/^\d{4}-\d{2}-\d{2}$/.test(added)) {
    return false;
  }
  const addedAt = Date.parse(`${added}T00:00:00Z`);
  if (!Number.isFinite(addedAt)) {
    return false;
  }
  const age = now.getTime() - addedAt;
  return age >= 0 && age < GLASS_VIDEO_NEW_DAYS * 24 * 60 * 60 * 1000;
}

export function formatGlassVideoSize(bytes: number | undefined): string {
  if (bytes === undefined || !Number.isFinite(bytes)) {
    return '';
  }
  if (bytes < 1024 * 1024) {
    return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatGlassVideoLength(seconds: number | undefined): string {
  if (seconds === undefined || !Number.isFinite(seconds)) {
    return '';
  }
  const whole = Math.round(seconds);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

function text(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function number(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

/** Reads the app's `glassVideoLibraryListed` message, dropping anything malformed. */
export function parseGlassVideoLibraryListing(message: unknown): GlassVideoLibraryListing | undefined {
  if (!message || typeof message !== 'object') {
    return undefined;
  }
  const record = message as Record<string, unknown>;
  if (record.type !== 'glassVideoLibraryListed' || !Array.isArray(record.videos)) {
    return undefined;
  }
  const videos: GlassVideoLibraryEntry[] = [];
  for (const raw of record.videos) {
    if (!raw || typeof raw !== 'object') {
      continue;
    }
    const video = raw as Record<string, unknown>;
    const id = text(video.id);
    const name = text(video.name);
    const state = video.state;
    if (!id || !name || (state !== 'bundled' && state !== 'downloaded' && state !== 'available')) {
      continue;
    }
    const tone = video.tone === 'dark' || video.tone === 'light' || video.tone === 'any' ? video.tone : undefined;
    videos.push({
      id,
      name,
      tags: Array.isArray(video.tags) ? video.tags.filter((tag): tag is string => typeof tag === 'string') : [],
      tone,
      added: text(video.added),
      durationSeconds: number(video.durationSeconds),
      sizeBytes: number(video.sizeBytes),
      state,
      poster: text(video.poster),
      preview: text(video.preview),
    });
  }
  return {
    online: record.online === true,
    storageBytes: number(record.storageBytes) ?? 0,
    videos,
  };
}
