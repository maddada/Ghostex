import { useState } from 'react';
import { IconBrandAndroid, IconBrandApple, IconChevronDown, IconExternalLink } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { cn } from '@/packages/components/utils';
import { AndroidInstallPopover } from './android-install-popover';
import { REMOTE_SETUP_DISCORD_URL } from './constants';

const ANDROID_POPOVER_ID = 'remote-setup-android-install-popover';

export function GetAppSection({ onOpenExternalUrl }: { onOpenExternalUrl: (url: string) => void }) {
  const [isAndroidOpen, setIsAndroidOpen] = useState(false);

  return (
    <section className='remote-setup-section remote-setup-get-app'>
      <header className='remote-setup-section-head'>
        <span className='remote-setup-section-number'>1</span>
        <h3 className='remote-setup-section-title'>Get the Ghostex app</h3>
      </header>
      <div className='remote-setup-rows'>
        <div className='remote-setup-row remote-setup-android-row'>
          <div className='remote-setup-row-main'>
            <span className='remote-setup-row-label'>
              <IconBrandAndroid aria-hidden='true' size={16} stroke={1.8} />
              Android
            </span>
            <span className='remote-setup-row-detail'>Install the APK from the latest GitHub release.</span>
          </div>
          <Button
            aria-controls={ANDROID_POPOVER_ID}
            aria-expanded={isAndroidOpen}
            className='remote-setup-android-install-button'
            onClick={() => setIsAndroidOpen((open) => !open)}
            size='xs'
            type='button'
            variant='outline'
          >
            How to install
            <IconChevronDown
              aria-hidden='true'
              className={cn('remote-setup-chevron', isAndroidOpen && 'remote-setup-chevron-open')}
            />
          </Button>
        </div>
        {isAndroidOpen ? <AndroidInstallPopover id={ANDROID_POPOVER_ID} /> : null}
        <div className='remote-setup-row remote-setup-iphone-row'>
          <div className='remote-setup-row-main'>
            <span className='remote-setup-row-label'>
              <IconBrandApple aria-hidden='true' size={16} stroke={1.8} />
              iPhone
            </span>
            <span className='remote-setup-row-detail'>
              TestFlight only for now. Join the Discord and ask for TestFlight access.
            </span>
          </div>
          <Button
            className='remote-setup-join-discord-button'
            onClick={() => onOpenExternalUrl(REMOTE_SETUP_DISCORD_URL)}
            size='xs'
            type='button'
            variant='outline'
          >
            Join Discord
            <IconExternalLink aria-hidden='true' />
          </Button>
        </div>
      </div>
    </section>
  );
}
