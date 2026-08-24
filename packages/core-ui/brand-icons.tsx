import antigravityLogo from './assets/editor-icons/antigravity.svg' with { type: 'text' };
import cursorLogo from './assets/editor-icons/cursor_dark.svg' with { type: 'text' };
import intellijIdeaLogo from './assets/editor-icons/intellijidea.svg' with { type: 'text' };
import jetBrainsLogo from './assets/editor-icons/jetbrains.svg' with { type: 'text' };
import phpStormLogo from './assets/editor-icons/phpstorm.svg' with { type: 'text' };
import pyCharmLogo from './assets/editor-icons/pycharm.svg' with { type: 'text' };
import riderLogo from './assets/editor-icons/rider.svg' with { type: 'text' };
import rubyMineLogo from './assets/editor-icons/rubymine.svg' with { type: 'text' };
import vscodeLogo from './assets/editor-icons/vscode.svg' with { type: 'text' };
import vscodiumLogo from './assets/editor-icons/vscodium.svg' with { type: 'text' };
import webStormLogo from './assets/editor-icons/webstorm.svg' with { type: 'text' };
import zedLogo from './assets/editor-icons/zed-logo_dark.svg' with { type: 'text' };

export type EditorBrandIconId =
  | 'antigravity'
  | 'cursor'
  | 'idea'
  | 'jetbrains'
  | 'phpstorm'
  | 'pycharm'
  | 'rider'
  | 'rubymine'
  | 'vscode'
  | 'vscode-insiders'
  | 'vscodium'
  | 'webstorm'
  | 'zed';

/**
 * CDXC:SidebarBrandIcons 2026-05-16-22:34:
 * Open-target menus and Settings rows must use canonical SVGL editor logos
 * instead of Tabler placeholders or hand-maintained path copies. VS Code
 * Insiders shares the VS Code mark and adds a small green overlay so it stays
 * visually related while remaining distinguishable in dense native menus.
 *
 * CDXC:SidebarBrandIcons 2026-05-16-23:24:
 * SVGL does not publish individual logos for every JetBrains IDE in the
 * built-in catalog. Use direct SVGL product icons where present and the SVGL
 * JetBrains mark for the remaining JetBrains-family targets so Settings still
 * has stable icons without inventing product artwork.
 */
const EDITOR_BRAND_LOGOS: Record<EditorBrandIconId, string> = {
  antigravity: svgTextToDataUrl(antigravityLogo),
  cursor: svgTextToDataUrl(cursorLogo),
  idea: svgTextToDataUrl(intellijIdeaLogo),
  jetbrains: svgTextToDataUrl(jetBrainsLogo),
  phpstorm: svgTextToDataUrl(phpStormLogo),
  pycharm: svgTextToDataUrl(pyCharmLogo),
  rider: svgTextToDataUrl(riderLogo),
  rubymine: svgTextToDataUrl(rubyMineLogo),
  vscode: svgTextToDataUrl(vscodeLogo),
  'vscode-insiders': svgTextToDataUrl(vscodeLogo),
  vscodium: svgTextToDataUrl(vscodiumLogo),
  webstorm: svgTextToDataUrl(webStormLogo),
  zed: svgTextToDataUrl(zedLogo),
};

function svgTextToDataUrl(svgText: string): string {
  if (svgText.startsWith('data:image/svg+xml,')) {
    return svgText;
  }
  return `data:image/svg+xml,${encodeURIComponent(svgText)}`;
}

export function EditorBrandIcon({ className, icon }: { className?: string; icon: EditorBrandIconId }) {
  return (
    <span
      aria-hidden='true'
      className={['editor-brand-icon', className].filter(Boolean).join(' ')}
      data-editor-brand-icon={icon}
    >
      <img alt='' className='editor-brand-icon-image' draggable={false} src={EDITOR_BRAND_LOGOS[icon]} />
      {icon === 'vscode-insiders' ? <span className='editor-brand-icon-insiders-badge' /> : null}
    </span>
  );
}

export function VisualStudioCodeIcon({ className }: { className?: string }) {
  return <EditorBrandIcon className={className} icon='vscode' />;
}

export function getEditorBrandIconId(targetId: string): EditorBrandIconId | undefined {
  if (
    targetId === 'antigravity' ||
    targetId === 'cursor' ||
    targetId === 'idea' ||
    targetId === 'phpstorm' ||
    targetId === 'pycharm' ||
    targetId === 'rider' ||
    targetId === 'rubymine' ||
    targetId === 'vscode' ||
    targetId === 'vscode-insiders' ||
    targetId === 'vscodium' ||
    targetId === 'webstorm' ||
    targetId === 'zed'
  ) {
    return targetId;
  }
  if (
    targetId === 'aqua' ||
    targetId === 'clion' ||
    targetId === 'datagrip' ||
    targetId === 'dataspell' ||
    targetId === 'goland' ||
    targetId === 'rustrover'
  ) {
    return 'jetbrains';
  }
  return undefined;
}
