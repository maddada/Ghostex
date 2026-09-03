import {
  IconAlertTriangle,
  IconBook2,
  IconCheck,
  IconCommand,
  IconDeviceDesktop,
  IconHistory,
  IconInfoCircle,
  IconLayoutSidebarLeftExpand,
  IconMoon,
  IconSearch,
  IconStarFilled,
  IconTool,
  IconWorld,
} from '@tabler/icons-react';
import type { ReactNode } from 'react';
import type { TitlebarNotice, TitlebarTip, TitlebarTipIcon } from './types';

export function TitlebarTipsMenu({
  notices,
  onMarkRead,
  onOpenChangelog,
  onOpenDocs,
  onOpenHighlightedFeatures,
  onOpenNoticeSettings,
  onOpenTipAction,
  onViewGhostexGuide,
  readTips,
  unreadTips,
}: {
  notices: TitlebarNotice[];
  onMarkRead: (tipId: string) => void;
  onOpenChangelog: () => void;
  onOpenDocs: () => void;
  onOpenHighlightedFeatures: () => void;
  onOpenNoticeSettings: (notice: TitlebarNotice) => void;
  onOpenTipAction: (tip: TitlebarTip) => void;
  onViewGhostexGuide: () => void;
  readTips: TitlebarTip[];
  unreadTips: TitlebarTip[];
}) {
  return (
    <div className='titlebar-tips-panel' onClick={(event) => event.stopPropagation()}>
      <div className='titlebar-tips-header'>
        <div className='titlebar-tips-title'>
          <IconInfoCircle aria-hidden='true' size={18} stroke={1.8} />
          <span>Tips</span>
        </div>
        <div className='titlebar-tips-actions'>
          <button aria-label='Open Docs' className='titlebar-tips-action-button' onClick={onOpenDocs} type='button'>
            <IconBook2 aria-hidden='true' size={14} stroke={1.9} />
            <span>Docs</span>
          </button>
          <button
            aria-label='Open Video'
            className='titlebar-tips-action-button'
            onClick={onOpenHighlightedFeatures}
            type='button'
          >
            <IconStarFilled aria-hidden='true' size={14} />
            <span>Video</span>
          </button>
          <button aria-label='Setup' className='titlebar-tips-action-button' onClick={onViewGhostexGuide} type='button'>
            <IconTool aria-hidden='true' size={14} stroke={1.9} />
            <span>Setup</span>
          </button>
          <button
            aria-label='Open Updates'
            className='titlebar-tips-action-button'
            onClick={onOpenChangelog}
            type='button'
          >
            <IconHistory aria-hidden='true' size={14} stroke={1.9} />
            <span>Updates</span>
          </button>
        </div>
      </div>
      <div className='titlebar-tips-scroll'>
        {notices.length > 0 ? (
          <TitlebarTipsSection count={notices.length} emptyText='' title='Notices'>
            {notices.map((notice) => (
              <TitlebarNoticeRow key={notice.id} notice={notice} onOpenSettings={() => onOpenNoticeSettings(notice)} />
            ))}
          </TitlebarTipsSection>
        ) : null}
        {/*
         CDXC:Onboarding 2026-06-12-10:56:
         Hide the Unread section when every tip is read so the panel does not show an empty "All caught up." block.
        */}
        {unreadTips.length > 0 ? (
          <TitlebarTipsSection count={unreadTips.length} emptyText='' title='Unread'>
            {unreadTips.map((tip) => (
              <TitlebarTipRow
                key={tip.id}
                onMarkRead={onMarkRead}
                onOpenTipAction={onOpenTipAction}
                read={false}
                tip={tip}
              />
            ))}
          </TitlebarTipsSection>
        ) : null}
        <TitlebarTipsSection count={readTips.length} emptyText='No read tips yet.' title='Read'>
          {readTips.map((tip) => (
            <TitlebarTipRow key={tip.id} onMarkRead={onMarkRead} onOpenTipAction={onOpenTipAction} read tip={tip} />
          ))}
        </TitlebarTipsSection>
      </div>
    </div>
  );
}

/**
 * CDXC:Onboarding 2026-06-12-08:20:
 * Tips & Tricks section headers must stay expanded. Collapsible Notices, Unread,
 * and Read groups hid content behind extra clicks without improving scanability.
 *
 * CDXC:Onboarding 2026-06-12-23:28:
 * The macOS Tips & Tricks panel should not show right-aligned section counts.
 * Keep the item count internal for empty-state rendering, but make section
 * headers read as labels only.
 */
export function TitlebarTipsSection({
  children,
  count,
  emptyText,
  title,
}: {
  children: ReactNode;
  count: number;
  emptyText: string;
  title: string;
}) {
  return (
    <section className='titlebar-tips-section'>
      <div className='titlebar-tips-section-heading'>
        <span>{title}</span>
      </div>
      <div className='titlebar-tips-list'>
        {count > 0 ? children : <div className='titlebar-tips-empty'>{emptyText}</div>}
      </div>
    </section>
  );
}

export function TitlebarNoticeRow({ notice, onOpenSettings }: { notice: TitlebarNotice; onOpenSettings: () => void }) {
  return (
    <button
      aria-label={`${notice.title}. Open related settings.`}
      className='titlebar-tip-row titlebar-tip-row-notice'
      data-read='false'
      onClick={onOpenSettings}
      type='button'
    >
      <div className='titlebar-tip-icon'>{getTitlebarTipIcon(notice.icon)}</div>
      <div className='titlebar-tip-copy'>
        <div className='titlebar-tip-title'>{notice.title}</div>
        <div className='titlebar-tip-body'>{notice.body}</div>
      </div>
    </button>
  );
}

export function TitlebarTipRow({
  onMarkRead,
  onOpenTipAction,
  read,
  tip,
}: {
  onMarkRead: (tipId: string) => void;
  onOpenTipAction: (tip: TitlebarTip) => void;
  read: boolean;
  tip: TitlebarTip;
}) {
  const detailContent = (
    <>
      <span className='titlebar-tip-icon'>{getTitlebarTipIcon(tip.icon)}</span>
      <span className='titlebar-tip-copy'>
        <span className='titlebar-tip-title'>{tip.title}</span>
        <span className='titlebar-tip-body'>{tip.body}</span>
      </span>
    </>
  );
  return (
    <article className='titlebar-tip-row' data-actionable={String(Boolean(tip.action))} data-read={String(read)}>
      {tip.action ? (
        <button
          aria-label={`${tip.title}. Open related details.`}
          className='titlebar-tip-detail titlebar-tip-detail-button'
          onClick={() => onOpenTipAction(tip)}
          type='button'
        >
          {detailContent}
        </button>
      ) : (
        <span className='titlebar-tip-detail'>{detailContent}</span>
      )}
      {read ? (
        <span className='titlebar-tip-read-state' aria-label='Read'>
          <IconCheck aria-hidden='true' size={15} stroke={1.9} />
        </span>
      ) : (
        <button
          aria-label={`Mark ${tip.title} as read`}
          className='titlebar-tip-read-button'
          onClick={() => onMarkRead(tip.id)}
          type='button'
        >
          <IconCheck aria-hidden='true' size={15} stroke={1.9} />
        </button>
      )}
    </article>
  );
}

export function getTitlebarTipIcon(icon: TitlebarTipIcon): ReactNode {
  switch (icon) {
    case 'browser':
      return <IconWorld aria-hidden='true' size={16} stroke={1.8} />;
    case 'command':
      return <IconCommand aria-hidden='true' size={16} stroke={1.8} />;
    case 'moon':
      return <IconMoon aria-hidden='true' size={16} stroke={1.8} />;
    case 'resources':
      return <IconDeviceDesktop aria-hidden='true' size={16} stroke={1.8} />;
    case 'search':
      return <IconSearch aria-hidden='true' size={16} stroke={1.8} />;
    case 'sidebar':
      return <IconLayoutSidebarLeftExpand aria-hidden='true' size={16} stroke={1.8} />;
    case 'warning':
      return <IconAlertTriangle aria-hidden='true' size={16} stroke={1.8} />;
  }
}
