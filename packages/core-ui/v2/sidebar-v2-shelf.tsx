import { IconChevronDown } from '@tabler/icons-react';
import type { ReactNode } from 'react';
import { useSidebarCollapsiblePresence } from '../sidebar-collapse-animation';

/*
 * CDXC:SidebarV2 2026-07-29:
 * Settled and Snoozed are shelves, not filters: they park rows out of the
 * inbox without deleting them, including two important rules:
 *
 * - The count shows ONLY while collapsed. Expanded, the visible rows are the
 *   count, and repeating it just adds noise to the header.
 * - Collapsing unmounts the rows after the shared disclosure animation. A
 *   shelf can hold hundreds of settled sessions, so hidden rows must not keep
 *   paying their layout cost once the transition is complete.
 */

export type SidebarV2ShelfTone = 'browser' | 'parked' | 'settled' | 'snoozed';

export type SidebarV2ShelfProps = {
  children: ReactNode;
  count: number;
  isExpanded: boolean;
  label: string;
  onToggle: () => void;
  tone: SidebarV2ShelfTone;
};

export function SidebarV2Shelf({ children, count, isExpanded, label, onToggle, tone }: SidebarV2ShelfProps) {
  const { isPresent, isVisuallyCollapsed, setCollapsibleElement } = useSidebarCollapsiblePresence(!isExpanded);
  if (count === 0) {
    return null;
  }
  return (
    <>
      <li className='sidebar-v2-shelf-header-item'>
        <button
          aria-expanded={isExpanded}
          className='sidebar-v2-shelf-header'
          data-tone={tone}
          onClick={onToggle}
          type='button'
        >
          <span className='sidebar-v2-shelf-label'>{isExpanded ? label : `${label} (${count})`}</span>
          <span aria-hidden='true' className='sidebar-v2-shelf-rule' />
          <IconChevronDown
            aria-hidden='true'
            className='sidebar-v2-shelf-chevron'
            data-expanded={String(isExpanded)}
            size={12}
            stroke={2}
          />
        </button>
      </li>
      {isPresent ? (
        <li
          aria-hidden={isVisuallyCollapsed}
          className='sidebar-v2-shelf-body sidebar-animated-collapse-body'
          data-collapsed={String(isVisuallyCollapsed)}
          inert={isVisuallyCollapsed ? true : undefined}
          ref={setCollapsibleElement}
        >
          <ul className='sidebar-v2-shelf-body-list'>{children}</ul>
        </li>
      ) : null}
    </>
  );
}
