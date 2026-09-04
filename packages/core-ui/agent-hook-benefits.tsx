import { IconCircleCheck, IconHistory, IconMessageCircle, IconPencil } from '@tabler/icons-react';

const AGENT_HOOK_BENEFITS = [
  {
    icon: IconCircleCheck,
    text: 'See agent progress and get notified when you’re needed.',
    title: 'Live status and alerts',
  },
  {
    icon: IconPencil,
    text: 'Find sessions easily with names based on your first message.',
    title: 'Automatic session names',
  },
  {
    icon: IconMessageCircle,
    text: 'Read and reply in chat, with the terminal one click away.',
    title: 'Chat View',
  },
  {
    icon: IconHistory,
    text: 'Pick up the same conversation after sleep or a restart.',
    title: 'Resume your work',
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
