/*
 * Inline SVG glyphs for the fake macOS desktop. Kept dependency-free (no icon
 * package, no Tailwind) so the sandbox desktop styles stay self-contained.
 */
import type { SVGProps } from "react";

type GlyphProps = SVGProps<SVGSVGElement> & { size?: number };

function Glyph({ children, size = 16, ...rest }: GlyphProps) {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
      {...rest}
    >
      {children}
    </svg>
  );
}

export function AppleGlyph({ size = 15 }: { size?: number }) {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M17.05 12.54c-.02-2.2 1.8-3.26 1.88-3.31-1.02-1.5-2.62-1.7-3.19-1.73-1.36-.14-2.65.8-3.34.8-.69 0-1.75-.78-2.87-.76-1.48.02-2.84.86-3.6 2.18-1.53 2.66-.39 6.6 1.1 8.76.73 1.06 1.6 2.25 2.74 2.2 1.1-.04 1.52-.71 2.85-.71 1.33 0 1.7.71 2.87.69 1.18-.02 1.93-1.08 2.65-2.14.83-1.22 1.18-2.4 1.2-2.46-.03-.01-2.3-.88-2.32-3.52zM14.9 5.9c.6-.74 1.01-1.76.9-2.78-.87.04-1.93.58-2.56 1.31-.56.65-1.06 1.7-.93 2.7.97.08 1.97-.49 2.59-1.23z"
        fill="currentColor"
      />
    </svg>
  );
}

export function InfoCircleGlyph({ size = 17 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <circle cx="12" cy="12" fill="none" r="9" stroke="currentColor" strokeWidth="1.6" />
      <path
        d="M12 11v5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.6"
      />
      <circle cx="12" cy="8" fill="currentColor" r="1" />
    </Glyph>
  );
}

export function WifiGlyph({ size = 15 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M3.5 9.2a13 13 0 0 1 17 0M6.4 12.5a9 9 0 0 1 11.2 0M9.3 15.8a5 5 0 0 1 5.4 0"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.7"
      />
      <circle cx="12" cy="19" fill="currentColor" r="1.2" />
    </Glyph>
  );
}

export function BatteryGlyph({ size = 17 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <rect
        fill="none"
        height="9"
        rx="2.4"
        stroke="currentColor"
        strokeWidth="1.4"
        width="16"
        x="2"
        y="7.5"
      />
      <rect fill="currentColor" height="5.4" rx="1.2" width="11" x="3.5" y="9.3" />
      <path
        d="M20 10.5v3a1.9 1.9 0 0 0 0-3z"
        fill="currentColor"
        opacity="0.7"
      />
    </Glyph>
  );
}

export function ControlCenterGlyph({ size = 15 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <rect
        fill="none"
        height="7"
        rx="3.5"
        stroke="currentColor"
        strokeWidth="1.5"
        width="18"
        x="3"
        y="3"
      />
      <circle cx="8" cy="6.5" fill="currentColor" r="1.6" />
      <rect
        fill="none"
        height="7"
        rx="3.5"
        stroke="currentColor"
        strokeWidth="1.5"
        width="18"
        x="3"
        y="14"
      />
      <circle cx="16" cy="17.5" fill="currentColor" r="1.6" />
    </Glyph>
  );
}

export function SearchGlyph({ size = 14 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <circle cx="11" cy="11" fill="none" r="6.5" stroke="currentColor" strokeWidth="1.7" />
      <path
        d="m16 16 4.5 4.5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.7"
      />
    </Glyph>
  );
}

export function WarningGlyph({ size = 14 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M12 4.5 21 19.5H3z"
        fill="none"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.6"
      />
      <path d="M12 10v4" fill="none" stroke="currentColor" strokeLinecap="round" strokeWidth="1.6" />
      <circle cx="12" cy="17" fill="currentColor" r="0.95" />
    </Glyph>
  );
}

export function CloseGlyph({ size = 12 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="m6 6 12 12M18 6 6 18"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="2"
      />
    </Glyph>
  );
}

export function PlusGlyph({ size = 12 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M12 5v14M5 12h14"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="2"
      />
    </Glyph>
  );
}

export function BookGlyph({ size = 13 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M4 5.5A1.5 1.5 0 0 1 5.5 4H10a2 2 0 0 1 2 2v13a2 2 0 0 0-2-2H5.5A1.5 1.5 0 0 1 4 15.5zM20 5.5A1.5 1.5 0 0 0 18.5 4H14a2 2 0 0 0-2 2v13a2 2 0 0 1 2-2h4.5a1.5 1.5 0 0 0 1.5-1.5z"
        fill="none"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.5"
      />
    </Glyph>
  );
}

export function StarGlyph({ size = 13 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="m12 4 2.5 5.2 5.5.8-4 3.9 1 5.6-5-2.7-5 2.7 1-5.6-4-3.9 5.5-.8z"
        fill="currentColor"
      />
    </Glyph>
  );
}

export function ToolGlyph({ size = 13 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M14.7 6.3a3.8 3.8 0 0 0 4.9 4.9L21 9.6a5.6 5.6 0 0 1-7.6 6.3l-5 5a2 2 0 0 1-2.8-2.8l5-5A5.6 5.6 0 0 1 16.9 5.6z"
        fill="none"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.5"
      />
    </Glyph>
  );
}

export function HistoryGlyph({ size = 13 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1M3.5 5v4.5H8"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.6"
      />
      <path
        d="M12 7.5V12l3 1.8"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.6"
      />
    </Glyph>
  );
}

export function TerminalGlyph({ size = 20 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="m6 8 4 4-4 4M13 16h5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
      />
    </Glyph>
  );
}

export function FinderGlyph({ size = 26 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M8 8.5v2M16 8.5v2"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.8"
      />
      <path
        d="M7.5 14.5c1.6 1.8 3.2 2.7 4.5 2.7s2.9-.9 4.5-2.7"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.8"
      />
    </Glyph>
  );
}

export function CompassGlyph({ size = 26 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <circle cx="12" cy="12" fill="none" r="8.5" stroke="currentColor" strokeWidth="1.4" />
      <path d="m15.5 8.5-1.9 5.1-5.1 1.9 1.9-5.1z" fill="currentColor" />
    </Glyph>
  );
}

export function MailGlyph({ size = 26 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <rect
        fill="none"
        height="12"
        rx="2.4"
        stroke="currentColor"
        strokeWidth="1.6"
        width="17"
        x="3.5"
        y="6"
      />
      <path
        d="m4.5 8 7.5 5.4L19.5 8"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.6"
      />
    </Glyph>
  );
}

export function NotesGlyph({ size = 26 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M7.5 8.5h9M7.5 12h9M7.5 15.5h5.5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.7"
      />
    </Glyph>
  );
}

export function MusicGlyph({ size = 26 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M10 17V7.2l7-1.7v9.6"
        fill="none"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.7"
      />
      <circle cx="8" cy="17.2" fill="currentColor" r="2.3" />
      <circle cx="15" cy="15.2" fill="currentColor" r="2.3" />
    </Glyph>
  );
}

export function TrashGlyph({ size = 26 }: { size?: number }) {
  return (
    <Glyph size={size}>
      <path
        d="M5.5 7.5h13M9.5 7.5V5.8a1.3 1.3 0 0 1 1.3-1.3h2.4a1.3 1.3 0 0 1 1.3 1.3v1.7M7.2 7.5l.8 10.6a1.6 1.6 0 0 0 1.6 1.4h4.8a1.6 1.6 0 0 0 1.6-1.4l.8-10.6"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.5"
      />
    </Glyph>
  );
}
