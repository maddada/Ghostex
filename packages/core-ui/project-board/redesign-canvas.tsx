/**
 * CDXC:ProjectBoardRedesign 2026-08-23:
 * Shared Storybook canvas for the Codex-style Project Board redesign
 * (Automate, Kanban, Docs). Carries the #0e0e0e page theme tokens plus the
 * Storybook-only shims: the preview loads the full sidebar CSS bundle whose
 * unlayered square-theme rules beat Tailwind's utilities layer, while the real
 * kanban/manage pages only load shadcn.generated.css.
 */
import type { CSSProperties, ReactNode } from 'react';

export const PAGE_THEME: CSSProperties & Record<string, string> = {
  '--background': '#0e0e0e',
  '--foreground': 'oklch(0.985 0 0)',
  '--card': '#161616',
  '--card-foreground': 'oklch(0.985 0 0)',
  '--popover': '#161616',
  '--popover-foreground': 'oklch(0.985 0 0)',
  '--primary': 'oklch(0.922 0 0)',
  '--primary-foreground': 'oklch(0.205 0 0)',
  '--secondary': '#1f1f1f',
  '--secondary-foreground': 'oklch(0.985 0 0)',
  '--muted': '#1f1f1f',
  '--muted-foreground': 'oklch(0.708 0 0)',
  '--accent': '#1f1f1f',
  '--accent-foreground': 'oklch(0.985 0 0)',
  '--destructive': 'oklch(0.704 0.191 22.216)',
  '--border': 'rgba(255, 255, 255, 0.08)',
  '--input': 'rgba(255, 255, 255, 0.15)',
  '--ring': 'oklch(0.556 0 0)',
  '--radius': '8px',
  /*
   * CDXC:AccentColor 2026-08-24:
   * Mirror the shipped default accent so redesign stories keep rendering the
   * accent text that now reads from --ghostex-accent.
   */
  '--ghostex-accent': '#38bdf8',
};

export const PAGE_SCOPED_CSS = `
  /* Storybook loads the full sidebar CSS bundle (theme.css shrinks the rem
     base and borders bare buttons); the real kanban page loads only
     shadcn.generated.css. Neutralize so the canvas matches the real page. */
  html {
    font-size: 16px;
  }
  .pb-redesign,
  .pb-redesign * {
    font-family: Inter Variable, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  /* CDXC:UnifiedToggle 2026-08-24: one app-wide toggle shape (6px track, 4px thumb). */
  .pb-redesign [data-slot="switch"] {
    border-radius: 6px;
  }
  .pb-redesign [data-slot="switch"] [data-slot="switch-thumb"] {
    border-radius: 4px;
  }
  /* The app's square theme zeroes radii and borders bare buttons from
     unlayered sheets that beat Tailwind's utilities layer. Re-assert the
     rounded Codex language inside the redesign canvas only. */
  .pb-redesign .rounded-md {
    border-radius: 6px;
  }
  .pb-redesign .rounded-lg {
    border-radius: 8px;
  }
  .pb-redesign .rounded-xl {
    border-radius: 12px;
  }
  .pb-redesign .rounded-full {
    border-radius: 999px;
  }
  .pb-redesign button:not([data-slot]) {
    border: 0;
    background: none;
  }
  .pb-redesign nav button[aria-current="page"] {
    background: rgba(255, 255, 255, 0.06);
  }
  .pb-redesign [data-slot="button"],
  .pb-redesign [data-slot="select-trigger"],
  .pb-redesign [data-slot="input"],
  .pb-redesign [data-slot="card"] {
    border-radius: 8px;
  }
  .pb-redesign [data-slot="button"] {
    font-weight: 400;
  }
  /* Select and menu popups portal to document.body, outside the canvas
     scope, so the square theme and dark popover tokens must be re-asserted
     globally while a redesign story is mounted. */
  /* styles.css pins these to --app-dropdown-background with !important, so
     the canvas has to shout back to get the redesign's panel color. */
  [data-slot="select-content"],
  [data-slot="dropdown-menu-content"],
  [data-slot="popover-content"] {
    border-radius: 8px;
    background: #161616 !important;
    border: 1px solid rgba(255, 255, 255, 0.08) !important;
  }
  [data-slot="select-item"],
  [data-slot="dropdown-menu-item"],
  [data-slot="dropdown-menu-checkbox-item"] {
    border-radius: 6px;
    font-weight: 400;
  }
`;

export function RedesignCanvas({ children }: { children: ReactNode }) {
  return (
    <div className='pb-redesign flex h-screen flex-col bg-[#0e0e0e] text-foreground' style={PAGE_THEME}>
      <style>{PAGE_SCOPED_CSS}</style>
      {children}
    </div>
  );
}
