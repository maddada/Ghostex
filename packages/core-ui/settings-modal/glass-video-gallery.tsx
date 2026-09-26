/**
 * Settings -> Theme -> Transparency -> More options, with Video picked: which video plays in each
 * mode, the Ghostex video library (get, use, preview, remove), the aerials this computer has, and
 * a file of your own.
 *
 * CDXC:Theming 2026-09-26 DECISION:
 * Built from the approved library mockup (docs/2026-09-25/glass-video-library). A video's dark/light tone is only a
 * hint on its card, never a filter; a video keeps a "New" badge for 7 days after it was added; nothing downloads until
 * Get is pressed; the video that ships with Ghostex cannot be removed. With Use transparency set to Dark only there is
 * only the dark-mode video to choose (the user: hide the options for the mode where glass never shows).
 */
import { useEffect, useState, type CSSProperties, type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import {
  formatGlassVideoLength,
  formatGlassVideoSize,
  glassVideoLibraryId,
  glassVideoLibraryReference,
  isGlassVideoNew,
  type GlassVideoLibraryEntry,
} from '../../shared/glass-video-library';
import { type GlassVideoLibraryControls } from './use-glass-video-library';

type Appearance = 'dark' | 'light';
type Target = Appearance | 'both';

export type GlassVideoGalleryProps = {
  darkValue: string;
  lightValue: string;
  /** Use transparency is Dark only: there is no light-mode video to pick. */
  darkOnly: boolean;
  library: GlassVideoLibraryControls;
  aerials: readonly { name: string; value: string }[];
  canChooseFile: boolean;
  fileError?: { appearance: Appearance; message: string };
  onChooseFile: (appearance: Appearance) => void;
  onAssign: (target: Target, value: string) => void;
  onClear: (appearance: Appearance) => void;
};

/** A soft stand-in picture for videos without one (aerials, files, offline library videos). */
function placeholderStyle(seed: string): CSSProperties {
  let hash = 0;
  for (const character of seed) {
    hash = (hash * 31 + character.charCodeAt(0)) % 360;
  }
  return {
    background: `linear-gradient(135deg, hsl(${hash} 55% 42%), hsl(${(hash + 70) % 360} 50% 30%) 55%, hsl(${(hash + 150) % 360} 45% 22%))`,
  };
}

function VideoPicture({
  name,
  poster,
  preview,
  seed,
  className,
}: {
  name: string;
  poster?: string;
  preview?: string;
  seed: string;
  className?: string;
}) {
  const [hovered, setHovered] = useState(false);
  const source = hovered && preview ? preview : poster;
  return (
    <span
      className={cn('glass-video-picture', className)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={source ? undefined : placeholderStyle(seed)}
    >
      {source ? <img alt='' draggable={false} src={source} /> : null}
      {preview ? <span className='glass-video-preview-hint'>{hovered ? 'Previewing' : 'Hover to preview'}</span> : null}
      <span className='sr-only'>{name}</span>
    </span>
  );
}

function ProgressRing({ fraction }: { fraction: number }) {
  const radius = 8;
  const length = 2 * Math.PI * radius;
  return (
    <svg
      aria-label={`Downloading ${Math.round(fraction * 100)}%`}
      className='glass-video-progress-ring'
      role='img'
      viewBox='0 0 22 22'
    >
      <circle className='track' cx='11' cy='11' r={radius} />
      <circle
        className='bar'
        cx='11'
        cy='11'
        r={radius}
        strokeDasharray={length}
        strokeDashoffset={length * (1 - Math.max(0, Math.min(1, fraction)))}
      />
    </svg>
  );
}

function slotName(
  value: string,
  videos: readonly GlassVideoLibraryEntry[] | undefined,
  aerials: readonly { name: string; value: string }[]
): { name: string; missing: boolean; entry?: GlassVideoLibraryEntry } {
  if (!value) {
    return { name: 'None', missing: false };
  }
  const libraryId = glassVideoLibraryId(value);
  if (libraryId) {
    const entry = videos?.find((video) => video.id === libraryId);
    if (!videos) {
      return { name: 'Library video', missing: false };
    }
    if (!entry || entry.state === 'available') {
      return { name: entry ? `${entry.name} (not downloaded)` : 'Removed from the library', missing: true, entry };
    }
    return { name: entry.name, missing: false, entry };
  }
  if (value.startsWith('aerial:')) {
    const aerial = aerials.find((candidate) => candidate.value === value);
    return aerial ? { name: aerial.name, missing: false } : { name: 'Aerial (not downloaded)', missing: true };
  }
  return { name: value.split('/').pop() ?? value, missing: false };
}

export function GlassVideoGallery({
  darkValue,
  lightValue,
  darkOnly,
  library,
  aerials,
  canChooseFile,
  fileError,
  onChooseFile,
  onAssign,
  onClear,
}: GlassVideoGalleryProps) {
  const [menuFor, setMenuFor] = useState<string>();
  const [previewId, setPreviewId] = useState<string>();
  const [manageOpen, setManageOpen] = useState(false);
  const listing = library.listing;
  const online = listing?.online ?? true;
  const videos = listing?.videos;
  const appearances: Appearance[] = darkOnly ? ['dark'] : ['dark', 'light'];
  const valueFor = (appearance: Appearance) => (appearance === 'dark' ? darkValue : lightValue);

  useEffect(() => {
    if (!menuFor) {
      return;
    }
    const close = (event: MouseEvent) => {
      if (!(event.target as HTMLElement | null)?.closest('.glass-video-menu, .glass-video-menu-button')) {
        setMenuFor(undefined);
      }
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [menuFor]);

  const assign = (target: Target, value: string) => {
    onAssign(darkOnly ? 'dark' : target, value);
    setMenuFor(undefined);
    setPreviewId(undefined);
  };

  const badges = (value: string) => {
    const modes = appearances.filter((appearance) => valueFor(appearance) === value);
    return modes.length > 0 ? (
      <span className='glass-video-badges'>
        {modes.map((mode) => (
          <span className={cn('glass-video-mode-badge', `is-${mode}`)} key={mode}>
            {mode === 'dark' ? 'Dark' : 'Light'}
          </span>
        ))}
      </span>
    ) : null;
  };

  const assignMenu = (value: string, extra?: ReactNode) => (
    <div className='glass-video-menu' role='menu'>
      {darkOnly ? (
        <button className='glass-video-menu-item' onClick={() => assign('dark', value)} role='menuitem' type='button'>
          Use this video {darkValue === value ? <span aria-hidden='true'>✓</span> : null}
        </button>
      ) : (
        <>
          <button className='glass-video-menu-item' onClick={() => assign('dark', value)} role='menuitem' type='button'>
            Use in dark mode {darkValue === value ? <span aria-hidden='true'>✓</span> : null}
          </button>
          <button
            className='glass-video-menu-item'
            onClick={() => assign('light', value)}
            role='menuitem'
            type='button'
          >
            Use in light mode {lightValue === value ? <span aria-hidden='true'>✓</span> : null}
          </button>
          <button className='glass-video-menu-item' onClick={() => assign('both', value)} role='menuitem' type='button'>
            Use in both
          </button>
        </>
      )}
      {extra}
    </div>
  );

  const libraryCard = (video: GlassVideoLibraryEntry) => {
    const value = glassVideoLibraryReference(video.id);
    const onComputer = video.state !== 'available';
    const unavailable = !online && !onComputer;
    const download = library.downloads[video.id];
    const error = library.errors[video.id];
    const length = formatGlassVideoLength(video.durationSeconds);
    const sub = unavailable
      ? 'Available when online'
      : video.state === 'bundled'
        ? `${length} · Included`
        : video.state === 'downloaded'
          ? `${length} · Ready`
          : `${length} · ${formatGlassVideoSize(video.sizeBytes)}`;
    const inUse = appearances.some((appearance) => valueFor(appearance) === value);
    return (
      <div
        className={cn('glass-video-card', unavailable && 'is-unavailable')}
        data-video-card={video.id}
        key={video.id}
      >
        <span className='glass-video-card-media'>
          <VideoPicture name={video.name} poster={video.poster} preview={video.preview} seed={video.id} />
          {badges(value)}
          {isGlassVideoNew(video.added) ? <span className='glass-video-new-badge'>New</span> : null}
        </span>
        <span className='glass-video-meta'>
          <span className='glass-video-meta-text'>
            <span className='glass-video-name'>{video.name}</span>
            <span className='glass-video-sub'>
              {sub}
              {video.tone && video.tone !== 'any' ? ` · suits ${video.tone} mode` : ''}
            </span>
          </span>
          {download ? (
            <>
              <ProgressRing fraction={download.total ? download.received / download.total : 0} />
              <Button onClick={() => library.cancel(video.id)} size='sm' type='button' variant='ghost'>
                Cancel
              </Button>
            </>
          ) : onComputer ? (
            <Button
              onClick={() =>
                darkOnly ? assign('dark', value) : setMenuFor(menuFor === video.id ? undefined : video.id)
              }
              size='sm'
              type='button'
              variant={inUse ? 'ghost' : 'secondary'}
            >
              {inUse ? 'In use' : 'Use'}
            </Button>
          ) : (
            <Button
              disabled={!online}
              onClick={() => library.download(video.id)}
              size='sm'
              type='button'
              variant='secondary'
            >
              Get
            </Button>
          )}
          <button
            aria-label={`More for ${video.name}`}
            className='glass-video-menu-button'
            onClick={() => setMenuFor(menuFor === `more:${video.id}` ? undefined : `more:${video.id}`)}
            type='button'
          >
            ⋯
          </button>
        </span>
        {error ? <span className='glass-video-error'>{error}</span> : null}
        {menuFor === video.id ? assignMenu(value) : null}
        {menuFor === `more:${video.id}` ? (
          <div className='glass-video-menu' role='menu'>
            {onComputer ? (
              <>
                {darkOnly ? (
                  <button
                    className='glass-video-menu-item'
                    onClick={() => assign('dark', value)}
                    role='menuitem'
                    type='button'
                  >
                    Use this video
                  </button>
                ) : (
                  <>
                    <button
                      className='glass-video-menu-item'
                      onClick={() => assign('dark', value)}
                      role='menuitem'
                      type='button'
                    >
                      Use in dark mode
                    </button>
                    <button
                      className='glass-video-menu-item'
                      onClick={() => assign('light', value)}
                      role='menuitem'
                      type='button'
                    >
                      Use in light mode
                    </button>
                    <button
                      className='glass-video-menu-item'
                      onClick={() => assign('both', value)}
                      role='menuitem'
                      type='button'
                    >
                      Use in both
                    </button>
                  </>
                )}
                <span className='glass-video-menu-separator' />
              </>
            ) : null}
            <button
              className='glass-video-menu-item'
              onClick={() => {
                setPreviewId(video.id);
                setMenuFor(undefined);
              }}
              role='menuitem'
              type='button'
            >
              Preview
            </button>
            {video.state === 'downloaded' ? (
              <button
                className='glass-video-menu-item is-danger'
                onClick={() => {
                  library.remove(video.id);
                  setMenuFor(undefined);
                }}
                role='menuitem'
                type='button'
              >
                Remove download · {formatGlassVideoSize(video.sizeBytes)}
              </button>
            ) : null}
            {video.state === 'bundled' ? <span className='glass-video-menu-note'>Included with Ghostex</span> : null}
          </div>
        ) : null}
      </div>
    );
  };

  const previewVideo = previewId ? videos?.find((video) => video.id === previewId) : undefined;
  const downloaded = videos?.filter((video) => video.state !== 'available') ?? [];

  return (
    <div className='glass-video-gallery'>
      {listing && !online ? (
        <div className='glass-video-offline' role='status'>
          You’re offline. Videos on this computer still play; the rest can be downloaded when you’re back online.
        </div>
      ) : null}

      <div className='glass-video-slots'>
        {appearances.map((appearance) => {
          const value = valueFor(appearance);
          const slot = slotName(value, videos, aerials);
          return (
            <div className={cn('glass-video-slot', `is-${appearance}`)} key={appearance}>
              <VideoPicture
                className='glass-video-slot-picture'
                name={slot.name}
                poster={slot.entry && !slot.missing ? slot.entry.poster : undefined}
                seed={value || appearance}
              />
              <span className='glass-video-slot-text'>
                <span className='glass-video-slot-mode'>
                  {darkOnly ? 'Video' : appearance === 'dark' ? 'Dark mode' : 'Light mode'}
                </span>
                <span className={cn('glass-video-slot-name', slot.missing && 'is-missing')}>
                  {value ? slot.name : 'None: shows the live blur'}
                </span>
              </span>
              {value ? (
                <Button onClick={() => onClear(appearance)} size='sm' type='button' variant='ghost'>
                  Clear
                </Button>
              ) : null}
            </div>
          );
        })}
      </div>

      <section className='glass-video-section'>
        <header className='glass-video-section-head'>
          <span className='glass-video-section-title'>Ghostex videos</span>
          <span className='glass-video-section-sub'>Looping videos made for the glass. Hover to preview.</span>
        </header>
        {videos ? (
          <div className='glass-video-grid'>{videos.map(libraryCard)}</div>
        ) : (
          <div className='glass-video-loading'>Loading the library…</div>
        )}
      </section>

      {aerials.length > 0 ? (
        <section className='glass-video-section'>
          <header className='glass-video-section-head'>
            <span className='glass-video-section-title'>On this computer</span>
            <span className='glass-video-section-sub'>Aerial wallpapers your computer has downloaded</span>
          </header>
          <div className='glass-video-grid'>
            {aerials.map((aerial) => (
              <div className='glass-video-card' data-video-card={aerial.value} key={aerial.value}>
                <span className='glass-video-card-media'>
                  <VideoPicture name={aerial.name} seed={aerial.value} />
                  {badges(aerial.value)}
                </span>
                <span className='glass-video-meta'>
                  <span className='glass-video-meta-text'>
                    <span className='glass-video-name'>{aerial.name}</span>
                    <span className='glass-video-sub'>Aerial wallpaper</span>
                  </span>
                  <Button
                    onClick={() =>
                      darkOnly
                        ? assign('dark', aerial.value)
                        : setMenuFor(menuFor === aerial.value ? undefined : aerial.value)
                    }
                    size='sm'
                    type='button'
                    variant='secondary'
                  >
                    Use
                  </Button>
                </span>
                {menuFor === aerial.value ? assignMenu(aerial.value) : null}
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {canChooseFile ? (
        <section className='glass-video-section'>
          <header className='glass-video-section-head'>
            <span className='glass-video-section-title'>Your files</span>
            <span className='glass-video-section-sub'>.mp4, .mov or .m4v</span>
          </header>
          <div className='glass-video-file-buttons'>
            {appearances.map((appearance) => (
              <Button
                key={appearance}
                onClick={() => onChooseFile(appearance)}
                size='sm'
                type='button'
                variant='secondary'
              >
                {darkOnly
                  ? 'Choose a file…'
                  : appearance === 'dark'
                    ? 'Choose a file for dark mode…'
                    : 'Choose a file for light mode…'}
              </Button>
            ))}
          </div>
          {fileError ? <span className='glass-video-error'>{fileError.message}</span> : null}
        </section>
      ) : null}

      {listing ? (
        <div className='glass-video-storage'>
          <span>{formatGlassVideoSize(listing.storageBytes) || '0 KB'} used by downloaded videos</span>
          <Button onClick={() => setManageOpen(!manageOpen)} size='sm' type='button' variant='ghost'>
            {manageOpen ? 'Done' : 'Manage downloads'}
          </Button>
        </div>
      ) : null}
      {listing && manageOpen ? (
        <div className='glass-video-manage'>
          {downloaded.length === 0 ? (
            <span className='glass-video-sub'>No videos downloaded yet.</span>
          ) : (
            downloaded.map((video) => (
              <div className='glass-video-manage-row' key={video.id}>
                <VideoPicture
                  className='glass-video-manage-picture'
                  name={video.name}
                  poster={video.poster}
                  seed={video.id}
                />
                <span className='glass-video-manage-name'>
                  {video.name}
                  {video.state === 'bundled' ? <span className='glass-video-sub'> · included with Ghostex</span> : null}
                </span>
                <span className='glass-video-sub'>{formatGlassVideoSize(video.sizeBytes)}</span>
                {video.state === 'downloaded' ? (
                  <Button onClick={() => library.remove(video.id)} size='sm' type='button' variant='ghost'>
                    Remove
                  </Button>
                ) : null}
              </div>
            ))
          )}
        </div>
      ) : null}

      {previewVideo ? (
        <div className='glass-video-preview-scrim' onMouseDown={() => setPreviewId(undefined)}>
          <div
            aria-label={`${previewVideo.name} preview`}
            aria-modal='true'
            className='glass-video-preview-modal'
            onMouseDown={(event) => event.stopPropagation()}
            role='dialog'
          >
            <VideoPicture
              className='glass-video-preview-picture'
              name={previewVideo.name}
              poster={previewVideo.preview ?? previewVideo.poster}
              seed={previewVideo.id}
            />
            <div className='glass-video-preview-body'>
              <div className='glass-video-preview-title'>{previewVideo.name}</div>
              <div className='glass-video-sub'>
                {formatGlassVideoLength(previewVideo.durationSeconds)} loop ·{' '}
                {formatGlassVideoSize(previewVideo.sizeBytes)} · 240p, made to sit behind the glass
              </div>
              {previewVideo.tags.length > 0 ? (
                <div className='glass-video-tags'>
                  {previewVideo.tags.map((tag) => (
                    <span className='glass-video-tag' key={tag}>
                      {tag}
                    </span>
                  ))}
                </div>
              ) : null}
              <div className='glass-video-preview-actions'>
                <Button onClick={() => setPreviewId(undefined)} size='sm' type='button' variant='ghost'>
                  Close
                </Button>
                {previewVideo.state === 'available' ? (
                  <Button
                    disabled={!online || Boolean(library.downloads[previewVideo.id])}
                    onClick={() => library.download(previewVideo.id)}
                    size='sm'
                    type='button'
                  >
                    Get · {formatGlassVideoSize(previewVideo.sizeBytes)}
                  </Button>
                ) : darkOnly ? (
                  <Button
                    onClick={() => assign('dark', glassVideoLibraryReference(previewVideo.id))}
                    size='sm'
                    type='button'
                  >
                    Use this video
                  </Button>
                ) : (
                  <>
                    <Button
                      onClick={() => assign('dark', glassVideoLibraryReference(previewVideo.id))}
                      size='sm'
                      type='button'
                      variant='secondary'
                    >
                      Use in dark mode
                    </Button>
                    <Button
                      onClick={() => assign('both', glassVideoLibraryReference(previewVideo.id))}
                      size='sm'
                      type='button'
                    >
                      Use in both
                    </Button>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
