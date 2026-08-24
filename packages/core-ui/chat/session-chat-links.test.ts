import { describe, expect, it } from 'vitest';
import { classifySessionChatLinkHref } from './session-chat-links';

describe('classifySessionChatLinkHref', () => {
  it('routes web URLs to the host browser', () => {
    expect(classifySessionChatLinkHref('https://example.com/x?y=1#z')).toEqual({
      kind: 'url',
      url: 'https://example.com/x?y=1#z',
    });
    expect(classifySessionChatLinkHref('http://localhost:5173')).toEqual({
      kind: 'url',
      url: 'http://localhost:5173',
    });
  });

  it('routes absolute and relative machine paths to the host editor', () => {
    expect(classifySessionChatLinkHref('/Users/me/repo/src/app.ts')).toEqual({
      kind: 'file',
      path: '/Users/me/repo/src/app.ts',
    });
    expect(classifySessionChatLinkHref('docs/specs/terminal-screen.md')).toEqual({
      kind: 'file',
      path: 'docs/specs/terminal-screen.md',
    });
    expect(classifySessionChatLinkHref('./sidebar/chat/view.tsx')).toEqual({
      kind: 'file',
      path: './sidebar/chat/view.tsx',
    });
  });

  it('unwraps file:// URLs and percent-encoded path segments', () => {
    expect(classifySessionChatLinkHref('file:///Users/me/My%20Notes.md')).toEqual({
      kind: 'file',
      path: '/Users/me/My Notes.md',
    });
    expect(classifySessionChatLinkHref('/Users/me/My%20Notes.md')).toEqual({
      kind: 'file',
      path: '/Users/me/My Notes.md',
    });
  });

  it('drops the editor coordinates agents quote paths with', () => {
    expect(classifySessionChatLinkHref('gpui/src/main.rs:28210')).toEqual({
      kind: 'file',
      path: 'gpui/src/main.rs',
    });
    expect(classifySessionChatLinkHref('gpui/src/main.rs:28210:14')).toEqual({
      kind: 'file',
      path: 'gpui/src/main.rs',
    });
  });

  it('keeps a Windows drive path a path, not a one-letter scheme', () => {
    expect(classifySessionChatLinkHref('C:\\repo\\app.ts')).toEqual({
      kind: 'file',
      path: 'C:\\repo\\app.ts',
    });
  });

  it('leaves anchors and schemes the chat cannot show inert', () => {
    for (const href of ['', '   ', '#section', 'mailto:me@example.com', 'vscode://file/x']) {
      expect(classifySessionChatLinkHref(href)).toEqual({ kind: 'inert' });
    }
  });
});
