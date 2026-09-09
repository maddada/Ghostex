import { getAccountsConnections, showAccountFlowToast } from './transport';
import { openAppModal } from '../app-modal-host-bridge';

export function accountSetupOwner(): string {
  const bridge = (window as unknown as { ghostexGpui?: { gxserverBootstrap?: { clientId?: string } } }).ghostexGpui;
  if (bridge?.gxserverBootstrap) return bridge.gxserverBootstrap.clientId ?? 'desktop';
  let id = localStorage.getItem('ghostex.accountSetupOwner');
  if (!id) { id = crypto.randomUUID(); localStorage.setItem('ghostex.accountSetupOwner', id); }
  return id;
}
/** CDXC:AgentProviders 2026-09-08 DECISION: Finishing login reopens Settings at Accounts, even if the user left Settings while the browser was open. The account is already registered before this completion is announced. */
export function monitorAccountSetup(): () => void {
  let stopped = false; let pending = false;
  const poll = async () => {
    if (pending || stopped) return;
    pending = true;
    try {
      const results = await Promise.allSettled(getAccountsConnections().map(async (connection) => ({connection, data: await connection.request({ operation: 'setupStatus', owner: accountSetupOwner() })})));
      for (const result of results) {
        if (result.status !== 'fulfilled') continue;
        const {connection, data} = result.value;
        for (const job of data.setupJobs ?? []) {
          if (stopped || job.acknowledged || job.status !== 'complete') continue;
          await connection.request({ operation: 'setupAcknowledge', owner: accountSetupOwner(), jobId: job.id });
          openAppModal({ type: 'open', modal: 'settings', initialTab: 'accounts' });
          showAccountFlowToast('Account connected', 'Your account is ready and highlighted in Settings.');
        }
      }
    } catch { /* A disconnected computer will be checked again on the next connection poll. */ }
    finally { pending = false; }
  };
  const timer = setInterval(() => void poll(), 2500);
  return () => { stopped = true; clearInterval(timer); };
}
