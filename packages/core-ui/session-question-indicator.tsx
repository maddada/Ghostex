import './session-question-indicator.css';

/**
 * CDXC:SessionStatus 2026-09-12 DECISION:
 * User: show attention whenever an async question appears, even while the agent keeps working. Combine the orange working spinner with the blue attention dot until the questions are answered or skipped.
 */
export function SessionQuestionIndicator({ working }: { working: boolean }) {
  return (
    <span
      className='session-question-indicator'
      data-working={working}
      role='img'
      aria-label={working ? 'Working · answer requested' : 'Answer requested'}
      title={working ? 'Working · answer requested' : 'Answer requested'}
    />
  );
}
