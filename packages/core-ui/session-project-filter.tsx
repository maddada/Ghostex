import { IconChevronDown } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Command, CommandEmpty, CommandInput, CommandItem, CommandList } from '@/packages/components/ui/command';
import { SearchableDropdownContent } from '@/packages/components/ui/searchable-dropdown';
import { Popover, PopoverTrigger } from '@/packages/components/ui/popover';

export type SessionProjectOption = { projectId: string; name: string; path?: string };

/** CDXC:Sessions 2026-09-08 DECISION:
 * User: put a project dropdown at the far right of Quick Access Sessions' filter bar, with a search filter at the top of the dropdown.
 * User: match the rounded tags dropdown, make it wider, and keep search at the top.
 */
export function SessionProjectFilter({
  projects,
  value,
  onChange,
  open,
  onOpenChange,
}: {
  projects: readonly SessionProjectOption[];
  value: string;
  onChange: (value: string) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const nameCounts = new Map<string, number>();
  for (const project of projects) {
    const name = project.name.trim().toLowerCase();
    nameCounts.set(name, (nameCounts.get(name) ?? 0) + 1);
  }
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger
        render={<Button variant='outline' className='quick-access-session-project-filter' />}
        aria-label='Filter sessions by project'
        aria-expanded={open}
      >
        <span>{projects.find((project) => project.projectId === value)?.name ?? 'All projects'}</span>
        <IconChevronDown size={14} />
      </PopoverTrigger>
      <SearchableDropdownContent align='end' className='quick-access-session-project-menu'>
        <Command>
          <CommandInput autoFocus placeholder='Filter projects...' aria-label='Filter projects' clearOnEscape={false} />
          <CommandList>
            <CommandEmpty>No projects found.</CommandEmpty>
            <CommandItem
              value='All projects'
              data-checked={value === ''}
              onSelect={() => {
                onChange('');
                onOpenChange(false);
              }}
            >
              All projects
            </CommandItem>
            {projects.map((project) => (
              <CommandItem
                key={project.projectId}
                data-checked={value === project.projectId}
                title={project.path}
                value={project.projectId}
                keywords={[project.name, project.path ?? '']}
                onSelect={() => {
                  onChange(project.projectId);
                  onOpenChange(false);
                }}
              >
                <span className='quick-access-session-project-label'>
                  <span>{project.name}</span>
                  {(nameCounts.get(project.name.trim().toLowerCase()) ?? 0) > 1 && project.path ? (
                    <small>{project.path}</small>
                  ) : null}
                </span>
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </SearchableDropdownContent>
    </Popover>
  );
}
