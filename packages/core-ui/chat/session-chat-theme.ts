import { useSyncExternalStore } from 'react';
import type { SessionChatTheme, SessionChatThemeSetting } from '@/packages/shared/session-chat';

function systemChatTheme(): SessionChatTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function subscribeSystemChatTheme(listener: () => void): () => void {
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  media.addEventListener('change', listener);
  return () => media.removeEventListener('change', listener);
}

export function resolveSessionChatTheme(setting: SessionChatThemeSetting): SessionChatTheme {
  return setting === 'system' ? systemChatTheme() : setting;
}

export function useSessionChatTheme(setting: SessionChatThemeSetting): SessionChatTheme {
  const systemTheme = useSyncExternalStore(subscribeSystemChatTheme, systemChatTheme, () => 'light' as const);
  return setting === 'system' ? systemTheme : setting;
}
