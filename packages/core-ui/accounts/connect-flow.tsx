import { useEffect, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Checkbox } from '@/packages/components/ui/checkbox';
import { SettingsInput } from '../settings-modal/fields';
import type { AccountProvider, AccountSetupJob, AgentAccount } from '@/packages/shared/agent-accounts';
import { getAccountsConnections } from './transport';
import { accountSetupOwner } from './setup-monitor';
import { AccountText, useAccountText, useHideAccountEmails } from './account-text';
import { AppTooltip } from '../app-tooltip';

export function AccountConnectFlow({ machineId, provider, account, initialJob, onComplete }: {
  machineId: string; provider: AccountProvider; account?: AgentAccount; initialJob?: AccountSetupJob; onComplete?: () => void;
}) {
  const accountText = useAccountText();
  const hideEmails = useHideAccountEmails();
  const [email, setEmail] = useState(account?.email ?? '');
  const [consent, setConsent] = useState(account?.registered === true);
  const [job, setJob] = useState<AccountSetupJob | undefined>(initialJob);
  const [error, setError] = useState('');
  const [starting, setStarting] = useState(false);
  const [terminal, setTerminal] = useState(false);
  const [code, setCode] = useState('');
  const request = async (operation: Parameters<NonNullable<ReturnType<typeof getAccountsConnections>[number]['request']>>[0]) => {
    const connection = getAccountsConnections().find((c) => c.id === machineId);
    if (!connection) throw new Error('The computer connection is unavailable.');
    return connection.request(operation);
  };
  useEffect(() => {
    if (!job || job.status === 'complete' || job.status === 'failed') return;
    let stopped = false;
    const timer = setInterval(() => {
      void request({ operation: 'setupStatus', owner: accountSetupOwner() }).then((data) => {
        if (stopped) return;
        const next = data.setupJobs?.find((next) => next.id === job.id);
        if (next) { setJob(next); if (next.status === 'complete') onComplete?.(); }
      }).catch((cause) => { if (!stopped) setError(String(cause)); });
    }, 1000);
    return () => { stopped = true; clearInterval(timer); };
  }, [job?.id, job?.status]);
  const start = async () => {
    setStarting(true); setError('');
    try {
      const data = await request({ operation: 'setupStart', owner: accountSetupOwner(), provider, email, shareHistory: true, accountId: account?.registered ? account.id : undefined, selector: account?.selector });
      setJob(data.setupJobs?.filter((job) => !job.acknowledged && job.provider === provider).at(-1));
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Could not start login.'); }
    finally { setStarting(false); }
  };
  const quote = (value: string) => "'" + value.replaceAll("'", "'\\''") + "'";
  const loginCommand = provider === 'claude'
    ? `ghostex account-login claude --email ${quote(email)} --json${account ? ` --account ${quote(account.selector)}` : ''}`
    : account ? `xswap login ${quote(account.selector)}` : `xswap add --login --share-history --email ${quote(email)} --json`;
  const active = job && !['complete', 'failed'].includes(job.status);
  return <div className='gx-account-connect-flow'>
    {job?.status === 'complete' ? <p role='status'>Account connected. <AccountText text={job.email} /></p> : active ? <>
      <p role='status'>{job.status === 'saving' ? 'Verifying and adding your account…' : 'Finish signing in through your browser. We’ll finish adding the account automatically.'}</p>
      <div className='gx-account-row-actions'>
        {job.url && <a href={job.url} target='_blank' rel='noreferrer' className='gx-account-signin-link'>Open sign-in page</a>}
        <Button variant='ghost' size='sm' onClick={() => setTerminal(!terminal)}>{terminal ? 'Hide terminal' : 'Show terminal'}</Button>
        <Button variant='ghost' size='sm' onClick={() => void request({ operation: 'setupCancel', owner: accountSetupOwner(), jobId: job.id }).catch((cause) => setError(String(cause)))}>Cancel</Button>
      </div>
      <form className='gx-account-code-input' onSubmit={(event) => {
        event.preventDefault();
        void request({ operation: 'setupInput', owner: accountSetupOwner(), jobId: job.id, input: code }).then(() => setCode('')).catch((cause) => setError(String(cause)));
      }}>
        <SettingsInput aria-label='Sign-in code' placeholder='Paste a sign-in code here if prompted' value={code} onChange={(event) => setCode(event.target.value)} autoComplete='off' />
        <Button type='submit' variant='outline' size='sm' disabled={!code.trim()}>Send code</Button>
      </form>
    </> : <>
      <label className='gx-account-field'>Email<SettingsInput type={hideEmails ? 'password' : 'email'} disabled={Boolean(account)} value={email} onChange={(event) => setEmail(event.target.value)} autoComplete={hideEmails ? 'off' : 'email'} /></label>
      {!account?.registered && <label className='gx-account-consent'><Checkbox checked={consent} onCheckedChange={setConsent} /><span>Share conversations between my {provider === 'claude' ? 'Claude' : 'Codex'} accounts.</span></label>}
      <AppTooltip content={<code>{accountText(loginCommand)}</code>}><Button variant='secondary' disabled={starting || !consent || !email.includes('@')} onClick={() => void start()}>{starting ? 'Starting sign-in…' : job?.status === 'failed' ? 'Try again' : account ? 'Reconnect account' : 'Add account'}</Button></AppTooltip>
    </>}
    {(error || job?.error) && <p role='alert'><AccountText text={error || job?.error || ''} /></p>}
    {(terminal || job?.status === 'failed') && job?.output && <pre className='gx-account-terminal-output'><AccountText text={job.output} /></pre>}
  </div>;
}
