import { useEffect, useState } from 'react';
import { IconCheck, IconCopy } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { QrCode } from '@/packages/components/ui/qr-code';
import { GHOSTEX_ANDROID_INSTALL_URL, GHOSTEX_ANDROID_INSTALL_URL_LABEL } from './constants';

const ANDROID_INSTALL_STEPS: readonly string[] = [
  'On the phone, open the link and download ghostex-android.apk.',
  'Open the downloaded file. If Android asks, allow your browser to install unknown apps.',
  'Tap Install, then open Ghostex and continue with step 2 below.',
];

/** The in-modal popover under the Android row: QR, link, and the three APK install steps. */
export function AndroidInstallPopover({ id }: { id: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);

  return (
    <div className='remote-setup-android-popover' id={id} role='region' aria-label='Install on your Android phone'>
      <div className='remote-setup-android-popover-top'>
        <QrCode
          alt='QR code for the Ghostex Android download'
          className='remote-setup-android-qr'
          size={112}
          value={GHOSTEX_ANDROID_INSTALL_URL}
        />
        <div className='remote-setup-android-popover-text'>
          <div className='remote-setup-android-popover-title'>Install on your Android phone</div>
          <div className='remote-setup-android-link-row'>
            <span className='remote-setup-android-link' title={GHOSTEX_ANDROID_INSTALL_URL}>
              {GHOSTEX_ANDROID_INSTALL_URL_LABEL}
            </span>
            <Button
              aria-label='Copy the Android download link'
              onClick={() => {
                void navigator.clipboard.writeText(GHOSTEX_ANDROID_INSTALL_URL).then(() => setCopied(true));
              }}
              size='icon-xs'
              type='button'
              variant='ghost'
            >
              {copied ? <IconCheck aria-hidden='true' /> : <IconCopy aria-hidden='true' />}
            </Button>
          </div>
          <div className='remote-setup-muted'>
            Scan the code or open the link on the phone. It goes to the latest release on GitHub.
          </div>
        </div>
      </div>
      <ol className='remote-setup-steps remote-setup-android-install-steps'>
        {ANDROID_INSTALL_STEPS.map((step, index) => (
          <li className='remote-setup-step' key={step}>
            <span className='remote-setup-step-number'>{index + 1}</span>
            <span>{step}</span>
          </li>
        ))}
      </ol>
      <div className='remote-setup-muted'>
        Updates: the app checks GitHub for newer releases. Settings → Updates downloads the new APK and opens the
        installer.
      </div>
    </div>
  );
}
