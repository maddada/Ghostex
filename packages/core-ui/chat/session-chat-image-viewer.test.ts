import { describe, expect, test } from 'vitest';
import { isSessionChatImageHref, sessionChatImageTargetForHref } from './session-chat-image-viewer';

describe('image viewer href classification', () => {
  test('recognizes image extensions with query/hash tolerance', () => {
    expect(isSessionChatImageHref('/Users/me/.ghostex/i/1755.png')).toBe(true);
    expect(isSessionChatImageHref('https://example.com/shot.JPEG?w=100')).toBe(true);
    expect(isSessionChatImageHref('/tmp/diagram.webp#zoom')).toBe(true);
    expect(isSessionChatImageHref('/tmp/report.pdf')).toBe(false);
    expect(isSessionChatImageHref('https://example.com/page')).toBe(false);
  });

  test('http(s)/data hrefs stay URLs; everything else is a machine path', () => {
    expect(sessionChatImageTargetForHref('https://example.com/a.png')).toEqual({
      url: 'https://example.com/a.png',
    });
    expect(sessionChatImageTargetForHref('data:image/png;base64,AAAA')).toEqual({
      url: 'data:image/png;base64,AAAA',
    });
    expect(sessionChatImageTargetForHref('/Users/me/.ghostex/i/a.png')).toEqual({
      path: '/Users/me/.ghostex/i/a.png',
    });
  });

  test('percent-encoded machine paths decode back to literal characters', () => {
    expect(sessionChatImageTargetForHref('/Users/me/My%20Shots/a.png')).toEqual({
      path: '/Users/me/My Shots/a.png',
    });
    // Malformed escapes fall back to the raw href instead of throwing.
    expect(sessionChatImageTargetForHref('/tmp/bad%zz.png')).toEqual({
      path: '/tmp/bad%zz.png',
    });
  });
});
