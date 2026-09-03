import { type ReactNode } from 'react';
import { IconExternalLink } from '@tabler/icons-react';
import { type WebviewApi } from '../../webview-api';
import packageJson from '../../../../package.json';
import { SettingsNativeScrollArea } from '../fields';
import { SettingsTabSearch, hasVisibleSettingsSearchResult } from '../search';
import { GHOSTEX_DISCORD_URL } from '@/packages/shared/sidebar-commands';

export const GHOSTEX_GITHUB_URL = 'https://github.com/maddada/Ghostex';
export const GHOSTEX_SPONSOR_URL = 'https://github.com/sponsors/maddada';

export function AboutSettingsTab({
  search,
  searchEmptyState,
  vscode,
}: {
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const links = [
    {
      description: 'Chat with the community and get help.',
      label: 'Join Discord',
      url: GHOSTEX_DISCORD_URL,
    },
    {
      description: 'View the source, releases, and report issues.',
      label: 'View on GitHub',
      url: GHOSTEX_GITHUB_URL,
    },
    {
      description: 'Support the continued development of Ghostex.',
      label: 'Sponsor Ghostex',
      url: GHOSTEX_SPONSOR_URL,
    },
  ] as const;

  if (search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <SettingsNativeScrollArea className='settings-main-scroll' viewportClassName='settings-native-scroll-viewport'>
        <div className='settings-page-width px-5 py-5'>{searchEmptyState}</div>
      </SettingsNativeScrollArea>
    );
  }

  return (
    <SettingsNativeScrollArea className='settings-main-scroll' viewportClassName='settings-native-scroll-viewport'>
      <div className='settings-about-page settings-page-width'>
        <header className='settings-about-header'>
          <div className='settings-about-mark' aria-hidden='true'>
            G
          </div>
          <div>
            <h2 className='settings-about-title'>Ghostex</h2>
            <p className='settings-about-version'>Version {packageJson.version}</p>
          </div>
        </header>
        <p className='settings-about-description'>A workspace for building with coding agents.</p>
        <div className='settings-about-links'>
          {links.map((link) => (
            <a
              className='settings-about-link'
              href={link.url}
              key={link.label}
              onClick={(event) => {
                if (!vscode) {
                  return;
                }
                event.preventDefault();
                vscode.postMessage({ type: 'openExternalUrl', url: link.url });
              }}
              rel='noreferrer'
              target='_blank'
            >
              <span className='settings-about-link-copy'>
                <span className='settings-about-link-title'>{link.label}</span>
                <span className='settings-about-link-description'>{link.description}</span>
              </span>
              <IconExternalLink aria-hidden='true' size={16} />
            </a>
          ))}
        </div>
      </div>
    </SettingsNativeScrollArea>
  );
}
