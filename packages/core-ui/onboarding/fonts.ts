import dmSans400 from './assets/fonts/DMSans-400.ttf';
import dmSans500 from './assets/fonts/DMSans-500.ttf';
import plexMono400 from './assets/fonts/IBMPlexMono-400.ttf';
import plexMono500 from './assets/fonts/IBMPlexMono-500.ttf';
import manrope500 from './assets/fonts/Manrope-500.ttf';
import manrope600 from './assets/fonts/Manrope-600.ttf';
import manrope700 from './assets/fonts/Manrope-700.ttf';

const FONT_STYLE_ID = 'gx-onboarding-fonts';

const FONT_FACES: readonly { family: string; weight: number; url: string }[] = [
  { family: 'Manrope', weight: 500, url: manrope500 },
  { family: 'Manrope', weight: 600, url: manrope600 },
  { family: 'Manrope', weight: 700, url: manrope700 },
  { family: 'DM Sans', weight: 400, url: dmSans400 },
  { family: 'DM Sans', weight: 500, url: dmSans500 },
  { family: 'IBM Plex Mono', weight: 400, url: plexMono400 },
  { family: 'IBM Plex Mono', weight: 500, url: plexMono500 },
];

/** Registers the onboarding @font-face rules once per document (see the `*.ttf` note in asset-modules.d.ts). */
export function ensureOnboardingFonts(): void {
  if (typeof document === 'undefined' || document.getElementById(FONT_STYLE_ID)) return;
  const style = document.createElement('style');
  style.id = FONT_STYLE_ID;
  style.textContent = FONT_FACES.map(
    (face) =>
      `@font-face{font-family:'${face.family}';src:url('${face.url}') format('truetype');font-weight:${face.weight};font-display:block}`
  ).join('\n');
  document.head.appendChild(style);
}
