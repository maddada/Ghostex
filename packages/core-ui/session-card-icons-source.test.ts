import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const sessionCardsCssSource = readFileSync(new URL('./styles/session-cards.css', import.meta.url), 'utf8');

describe('session card icon source', () => {
  test('positions the floating pin button from the leading icon anchor', () => {
    /*
     * CDXC:PinnedSessions 2026-06-30-00:34:
     * The pin affordance is a separate IconPin button, not a replacement for
     * the agent/tag slot. It is anchored one icon-width to the left and remains
     * hidden on unpinned rows unless the pointer is over that exact hitbox.
     */
    expect(sessionCardsCssSource).toContain('anchor-scope: --session-leading-icon;');
    expect(sessionCardsCssSource).toContain('anchor-name: --session-leading-icon;');
    expect(sessionCardsCssSource).toContain('.session-pinned-floating-button');
    expect(sessionCardsCssSource).toContain(
      'anchor(left) - var(--session-leading-icon-size) - var(--session-pinned-icon-left-shift)'
    );
    expect(sessionCardsCssSource).toContain('inline-size: var(--session-leading-icon-size);');
    expect(sessionCardsCssSource).toContain('--session-pinned-icon-left-shift: 3px;');
    expect(sessionCardsCssSource).toContain('transform: scaleX(-1);');
    expect(sessionCardsCssSource).toContain('appearance: none;');
    expect(sessionCardsCssSource).not.toContain('right: anchor(left);');
    expect(sessionCardsCssSource).not.toContain('anchor-size(');
    expect(sessionCardsCssSource).toContain(
      ".session-pinned-floating-button[data-pinned='false']:not(:hover):not(:focus-visible)"
    );
    expect(sessionCardsCssSource).toContain('row hover alone must not show it');
    expect(sessionCardsCssSource).not.toContain('session-pinned-agent-icon');
  });

  test('keeps tagged session leading icons mutually exclusive', () => {
    /*
     * CDXC:SidebarSessionAgentIcons 2026-06-30-00:12:
     * Tagged rows render both the tag glyph and the hidden agent glyph so hover
     * can swap identities without layout churn. CSS must keep the tag colored
     * and hide the underlying agent at rest, then hide the tag on pointer hover.
     */
    expect(sessionCardsCssSource).toContain(
      ".session-tag-colored-icon[data-session-tag='favorite'],\n.session-tag-agent-icon[data-session-tag='favorite']"
    );
    expect(sessionCardsCssSource).toContain(".session-frame[data-tagged='true']:not(:hover):not(:has(.session:hover))");
    expect(sessionCardsCssSource).toContain('.session-floating-agent-icon,');
    expect(sessionCardsCssSource).toContain('.session-floating-agent-tabler-icon[data-agent-icon],');
    expect(sessionCardsCssSource).toContain(".session-persistence-provider-badge[data-slot='floating']");
    expect(sessionCardsCssSource).toContain(
      'Tagged session rows have one leading slot. At rest, including when the row'
    );
    expect(sessionCardsCssSource).toContain(".session-frame[data-tagged='true']:is(:hover, :has(.session:hover))");
    expect(sessionCardsCssSource).toContain('.session-tag-agent-icon');
    expect(sessionCardsCssSource).toContain('opacity: 0 !important;');
  });

  test('keeps active delayed send icons visible during hover and focus', () => {
    /*
     * CDXC:DelayedSend 2026-05-21-13:04:
     * An active timer owns the leading slot continuously. Pinned-row hover and
     * focus must not hide the clock or substitute the ordinary agent icon.
     */
    expect(sessionCardsCssSource).toContain('.session-floating-agent-tabler-icon.session-delayed-send-agent-icon {');
    expect(sessionCardsCssSource).toContain('Delayed Send timers are not part of this swap');
    expect(sessionCardsCssSource).not.toMatch(
      /\.session-frame\[data-pinned="true"\][^{}]*\s+\.session-delayed-send-agent-icon\s*\{/
    );
  });
});
