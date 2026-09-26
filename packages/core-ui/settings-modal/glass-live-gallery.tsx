/**
 * Settings -> Theme -> Transparency -> More options, with Live picked: which animated style the
 * glass draws in each mode. The window draws it in the current theme's colours; the posters are
 * rendered from the same shaders (tooling/glass-live/render-posters.swift) in a sample theme.
 */
import { useState } from 'react';
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

const POSTERS: Readonly<Record<WindowGlassLiveStyle, Record<Appearance, string>>> = {
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
  return WINDOW_GLASS_LIVE_STYLE_OPTIONS.find((option) => option.value === style)?.label ?? style;
}

export type GlassLiveGalleryProps = {
  darkStyle: WindowGlassLiveStyle;
  lightStyle: WindowGlassLiveStyle;
  /** Use transparency is Dark only: there is no light-mode style to pick. */
  darkOnly: boolean;
  onPick: (appearance: Appearance, style: WindowGlassLiveStyle) => void;
};

export function GlassLiveGallery({ darkStyle, lightStyle, darkOnly, onPick }: GlassLiveGalleryProps) {
  const [editing, setEditing] = useState<Appearance>('dark');
  const appearance: Appearance = darkOnly ? 'dark' : editing;
  const selected = appearance === 'dark' ? darkStyle : lightStyle;
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
        <span className='glass-live-note'>Drawn live in your theme colours.</span>
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
      </div>
    </div>
  );
}
