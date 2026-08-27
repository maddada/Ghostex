import { IconCircleCheck, IconHistory, IconMessageCircle, IconPencil } from '@tabler/icons-react';

const AGENT_HOOK_BENEFITS = [
  {
    icon: IconCircleCheck,
    text: 'See when agents are working, waiting, or done, and get notified when they finish or need you.',
    title: 'Live status and alerts',
  },
  {
    icon: IconPencil,
    text: 'Sessions name themselves automatically from your first message, so you see "Fix login bug", not "zsh\u00a0(3)".',
    title: 'Names, not noise',
  },
  {
    icon: IconMessageCircle,
    text: 'Read and reply in a clean conversation view, then switch back to the full terminal with one click.',
    title: 'Chat View',
  },
  {
    icon: IconHistory,
    text: 'Resume the exact agent conversation after sleep, reload, or app restart instead of starting over.',
    title: 'Resume Agents',
  },
] as const;

export function AgentHookBenefits() {
  return (
    <div className='agent-hook-benefits-grid'>
      {AGENT_HOOK_BENEFITS.map((benefit) => {
        const BenefitIcon = benefit.icon;
        return (
          <article className='agent-hook-benefit-card' key={benefit.title}>
            <h3>
              <BenefitIcon aria-hidden='true' size={15} />
              {benefit.title}
            </h3>
            <p>{benefit.text}</p>
          </article>
        );
      })}
    </div>
  );
}
