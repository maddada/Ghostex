/**
 * Settings -> Theme -> Transparency -> More options, with Live picked: which animated style the
 * glass draws in each mode, or the user's own video. The window draws the animations in the current
 * theme's colours; the posters are rendered from the same shaders
 * (tooling/glass-live/render-posters.swift) in a sample theme.
 */
import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { cn } from '@/packages/components/utils';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { WINDOW_GLASS_LIVE_STYLE_OPTIONS, type WindowGlassLiveStyle } from '../../shared/ghostex-settings';
import auroraDark from '../assets/glass-live/aurora-dark.jpg';
import auroraLight from '../assets/glass-live/aurora-light.jpg';
import bokehDark from '../assets/glass-live/bokeh-dark.jpg';
import bokehLight from '../assets/glass-live/bokeh-light.jpg';
import driftDark from '../assets/glass-live/drift-dark.jpg';
import driftLight from '../assets/glass-live/drift-light.jpg';
import inkDark from '../assets/glass-live/ink-dark.jpg';
import inkLight from '../assets/glass-live/ink-light.jpg';
import meshDark from '../assets/glass-live/mesh-dark.jpg';
import meshLight from '../assets/glass-live/mesh-light.jpg';
import nebulaDark from '../assets/glass-live/nebula-dark.jpg';
import nebulaLight from '../assets/glass-live/nebula-light.jpg';
import silkDark from '../assets/glass-live/silk-dark.jpg';
import silkLight from '../assets/glass-live/silk-light.jpg';
import wavesDark from '../assets/glass-live/waves-dark.jpg';
import wavesLight from '../assets/glass-live/waves-light.jpg';

type Appearance = 'dark' | 'light';

const POSTERS: Readonly<Record<Exclude<WindowGlassLiveStyle, 'video'>, Record<Appearance, string>>> = {
  aurora: { dark: auroraDark, light: auroraLight },
  bokeh: { dark: bokehDark, light: bokehLight },
  drift: { dark: driftDark, light: driftLight },
  ink: { dark: inkDark, light: inkLight },
  mesh: { dark: meshDark, light: meshLight },
  nebula: { dark: nebulaDark, light: nebulaLight },
  silk: { dark: silkDark, light: silkLight },
  waves: { dark: wavesDark, light: wavesLight },
};

function styleLabel(style: WindowGlassLiveStyle): string {
  if (style === 'video') {
    return 'Your video';
  }
  return WINDOW_GLASS_LIVE_STYLE_OPTIONS.find((option) => option.value === style)?.label ?? style;
}

function fileName(path: string): string {
  return path.split('/').pop() || path;
}

export type GlassLiveGalleryProps = {
  darkStyle: WindowGlassLiveStyle;
  lightStyle: WindowGlassLiveStyle;
  /** The user's own video for each mode (an absolute path, empty when none is chosen). */
  darkVideo: string;
  lightVideo: string;
  /** Use transparency is Dark only: there is no light-mode style to pick. */
  darkOnly: boolean;
  /** The host can open a file dialog (the desktop app); elsewhere a video cannot be chosen here. */
  canChooseVideo: boolean;
  onPick: (appearance: Appearance, style: WindowGlassLiveStyle) => void;
  onChooseVideo: (appearance: Appearance) => void;
  onClearVideo: (appearance: Appearance) => void;
  /** Why the last picked video file was refused. */
  videoError?: { appearance: Appearance; message: string };
};

/**
 * CDXC:Theming 2026-09-26 DECISION:
 * User: "let's hide videos and merge videos with live / make videos just take from custom video user picks or the
 * animations we did". Live lists the eight animations and then "Your video", a file the user picks for each mode;
 * picking the card with no file yet opens the file dialog. Supersedes the separate Video choice with its macOS aerials
 * and the Ghostex video library, which are no longer offered.
 */
export function GlassLiveGallery({
  darkStyle,
  lightStyle,
  darkVideo,
  lightVideo,
  darkOnly,
  canChooseVideo,
  onPick,
  onChooseVideo,
  onClearVideo,
  videoError,
}: GlassLiveGalleryProps) {
  const [editing, setEditing] = useState<Appearance>('dark');
  const appearance: Appearance = darkOnly ? 'dark' : editing;
  const selected = appearance === 'dark' ? darkStyle : lightStyle;
  const video = appearance === 'dark' ? darkVideo : lightVideo;
  const error = videoError?.appearance === appearance ? videoError.message : undefined;
  const pickVideo = () => {
    onPick(appearance, 'video');
    if (!video && canChooseVideo) {
      onChooseVideo(appearance);
    }
  };
  return (
    <div className='glass-live-gallery'>
      <div className='glass-live-head'>
        {darkOnly ? (
          <span className='glass-live-summary'>{`Dark mode: ${styleLabel(darkStyle)}`}</span>
        ) : (
          <>
            <SegmentedControl
              aria-label='Live background for'
              onValueChange={(value) => {
                if (value === 'dark' || value === 'light') {
                  setEditing(value);
                }
              }}
              value={editing}
            >
              <SegmentedControlItem value='dark'>{`Dark mode · ${styleLabel(darkStyle)}`}</SegmentedControlItem>
              <SegmentedControlItem value='light'>{`Light mode · ${styleLabel(lightStyle)}`}</SegmentedControlItem>
            </SegmentedControl>
          </>
        )}
        <span className='glass-live-note'>Animations are drawn live in your theme colours.</span>
      </div>
      <div className='glass-live-grid' role='radiogroup' aria-label={`Live background for ${appearance} mode`}>
        {WINDOW_GLASS_LIVE_STYLE_OPTIONS.map((option) => (
          <button
            aria-checked={selected === option.value}
            className={cn(
              'glass-live-card',
              `glass-live-${option.value}-card`,
              selected === option.value && 'is-selected'
            )}
            key={option.value}
            onClick={() => onPick(appearance, option.value)}
            role='radio'
            type='button'
          >
            <img alt='' className='glass-live-poster' draggable={false} src={POSTERS[option.value][appearance]} />
            <span className='glass-live-name'>{option.label}</span>
          </button>
        ))}
        <button
          aria-checked={selected === 'video'}
          className={cn('glass-live-card', 'glass-live-video-card', selected === 'video' && 'is-selected')}
          onClick={pickVideo}
          role='radio'
          type='button'
        >
          <span aria-hidden='true' className='glass-live-poster glass-live-video-poster'>
            <span className='glass-live-video-glyph' />
          </span>
          <span className='glass-live-name'>Your video</span>
        </button>
      </div>
      {selected === 'video' ? (
        <div className='glass-live-video-slot'>
          <span className='glass-live-video-file' title={video || undefined}>
            {video ? fileName(video) : 'No video chosen yet, so the glass shows the live blur.'}
          </span>
          {canChooseVideo ? (
            <span className='glass-live-video-actions'>
              <Button onClick={() => onChooseVideo(appearance)} size='sm' type='button' variant='secondary'>
                {video ? 'Change…' : 'Choose file…'}
              </Button>
              {video ? (
                <Button onClick={() => onClearVideo(appearance)} size='sm' type='button' variant='ghost'>
                  Clear
                </Button>
              ) : null}
            </span>
          ) : (
            <span className='glass-live-video-note'>Videos are chosen in the Ghostex desktop app.</span>
          )}
          {error ? (
            <span className='glass-live-video-error' role='alert'>
              {error}
            </span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
