import { AccountConnectFlow } from './connect-flow';
import { useLayoutEffect, useRef, useState } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import type { AccountHelper, AccountProvider } from '@/packages/shared/agent-accounts';
import { AccountLogo } from './controls';
import { CopyCommand } from './copy-command';

/** CDXC:Settings 2026-09-07 DECISION: Each provider has a connection-guide button. Both open the same Settings dialog with a backdrop, shared Ghostex instructions once, and two bullets for the provider-specific steps, each with a short helper and author credit. */
export function AccountConnectionGuide({
  provider,
  helpers,
  machineId,
  busy,
  onClose,
}: {
  provider?: AccountProvider;
  helpers: AccountHelper[];
  machineId: string;
  busy: boolean;
  onClose: () => void;
}) {
  const anchor = useRef<HTMLSpanElement>(null);
  const [settingsHeight, setSettingsHeight] = useState(0);
  /** CDXC:Settings 2026-09-07 DECISION: The tutorial is at most 90% of its Settings modal's height, including when Settings resizes. */
  useLayoutEffect(() => {
    if (!provider) {
      return;
    }
    const settings = anchor.current?.closest<HTMLElement>('.settings-modal-dialog');
    if (!settings) return;
    const measure = () => setSettingsHeight(settings.offsetHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(settings);
    return () => observer.disconnect();
  }, [provider]);
  return (
    <>
      <span hidden ref={anchor} />
      <Dialog
        open={Boolean(provider)}
        onOpenChange={(open) => {
          if (!open) onClose();
        }}
      >
        <DialogContent
          nested
          showCloseButton
          className='gx-accounts gx-account-guide sm:max-w-xl'
          style={{ maxHeight: settingsHeight * 0.9 }}
        >
          <DialogHeader>
            <DialogTitle>Connect your accounts</DialogTitle>
            <DialogDescription>
              Add an account for usage stats on the computer where your sessions run. This is optional, even for a single account.
            </DialogDescription>
          </DialogHeader>
          <section>
            <h3>1. Save a login</h3>
            <p>
              Enter the account email and allow shared conversations, then choose Add account. Finish signing in through your browser; Settings shows the progress automatically.
            </p>
            <ul className='gx-account-guide-providers'>
              {(['claude', 'codex'] as const).map((id) => {
                const helper = helpers.find((item) => item.provider === id);
                return (
                  <li key={id} data-selected={provider === id}>
                    <h4>
                      <AccountLogo provider={id} />
                      {id === 'claude' ? 'Claude Code' : 'Codex'}
                    </h4>
                    <p>
                      {id === 'claude'
                        ? 'Claude accounts use Claude Swap (cswap) by Onur Cetinkol (realiti4).'
                        : 'Codex accounts use Codex Swap (xswap CLI) by Mohamad Yahia (maddada).'}
                    </p>
                    <p>
                      {id === 'claude'
                        ? 'Enter the new account’s email below and choose it in the browser. Ghostex uses a separate login profile and verifies the account before saving it with cswap. To refresh an existing login, use that account’s Reconnect action.'
                        : 'Enter the new account’s email below and choose it in the browser. xswap verifies the login before saving a separate account home with shared session history. To refresh an existing login, use that account’s Reconnect action.'}
                    </p>
                    {helper && !helper.installed && <CopyCommand command={helper.installCommand} />}

                  </li>
                );
              })}
            </ul>
            {provider && helpers.find((helper) => helper.provider === provider)?.installed && <AccountConnectFlow machineId={machineId} provider={provider} />}
          </section>
          <section>
            <h3>2. Your account is ready</h3>
            <p>Ghostex verifies and adds the connected account automatically, then opens Settings &gt; Accounts with it highlighted. Star the account to show its usage in the titlebar. In chat context details, star Account limits to show usage in the status line.</p>
            <p>Give the account a name or swap its slot with another account. A small label centered over the session’s agent icon identifies the account. Use its slot number or set up to two custom letters or numbers in the account’s settings.</p>
          </section>
          <section>
            <h3>3. Start with an account</h3>
            <p>
              Open the agent dropdown in a project’s sidebar header. Choose Claude or Codex, then an account to start a
              session immediately. Custom agents keep their own settings.
            </p>
            <p>
              Quick launch uses the account chosen under Account for new sessions in Settings. Choosing another
              account from the launcher applies only to that new session. Until you add an account for a provider,
              Ghostex uses its current CLI login without an account switcher.
            </p>
          </section>
          <section>
            <h3>Let work continue</h3>
            <p>
              Under New session defaults, enable auto-continue and choose whether to wait for a usage reset or switch to
              an available account of the same provider. Automatic switching uses only connected accounts you have
              marked available, following your priority setting. Shared history lets the conversation resume.
            </p>
            <p>
              With error retries enabled, temporary failures retry after 5, 10, 20, 40, then 60 minutes between
              attempts. Login and permission problems need your attention. The computer and Ghostex server must stay
              running.
            </p>
            <p>
              Existing sessions keep their saved settings. Change one session through More actions → Switch account,
              where you can choose a login or override auto-continue.
            </p>
          </section>
        </DialogContent>
      </Dialog>
    </>
  );
}
