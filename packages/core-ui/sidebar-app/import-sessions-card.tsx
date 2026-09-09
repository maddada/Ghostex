import { useState } from 'react';
import { IconHistoryToggle } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import './import-sessions-card.css';

const IMPORT_SESSIONS_INTRO_SEEN_KEY = 'ghostex.sidebar.import-sessions-intro-seen.v1';

export function useImportSessionsIntro(openExternalSessions: () => void) {
  const [isVisible, setIsVisible] = useState(
    () => window.localStorage.getItem(IMPORT_SESSIONS_INTRO_SEEN_KEY) !== 'true'
  );
  const openImportSessions = () => {
    openExternalSessions();
    window.localStorage.setItem(IMPORT_SESSIONS_INTRO_SEEN_KEY, 'true');
    setIsVisible(false);
  };
  return { isVisible, openImportSessions };
}

/** CDXC:Sessions 2026-09-09 DECISION:
 * User: introduce older sessions at the bottom of the sidebar on first launch, with a dark neutral charcoal, rounded, padded card, 7px side margins, and a whitish outline button.
 * User approved a compact layout with the title using the full card width, 14px padding, 10px gaps, and a smaller button under the title. The history icon belongs inside the button so it does not narrow the title.
 * User approved the single title Continue Claude & Codex sessions and the button label Previous Sessions List.
 * Both Previous Sessions List and Import Sessions immediately below Sessions in the hamburger menu open the External list directly.
 */
export function ImportSessionsCard({ onImport }: { onImport: () => void }) {
  return (
    <section className='sidebar-import-sessions-card' aria-label='Continue older sessions'>
      <h3>Continue Claude &amp; Codex sessions</h3>
      <Button variant='outline' className='sidebar-import-sessions-button' onClick={onImport}>
        <IconHistoryToggle aria-hidden='true' stroke={1.6} />
        Previous Sessions List
      </Button>
    </section>
  );
}
