import {
  IconArrowsSort,
  IconBox,
  IconChevronDown,
  IconCircleDot,
  IconFilter,
  IconFolder,
  IconGitPullRequest,
  IconLayoutList,
  IconLayoutSidebar,
} from "@tabler/icons-react";
import { useState, type ReactNode } from "react";
import { Avatar, Dropdown, MenuItem, cx } from "./components";
import { STATUS_FILTERS } from "./format";
import { GROUP_BY_OPTIONS, type WorkGroupBy } from "./grouping";
import type { WorkProject, WorkTracker } from "./types";
import type { WorkFilters } from "./work-list";

/** What the type segment shows: both kinds, only the tracker's issues, or only PRs. */
export type WorkTypeChoice = "all" | "issues" | "pullRequests";

export function typeChoiceOf(filters: WorkFilters): WorkTypeChoice {
  if (!filters.pullRequests) return "issues";
  if (!filters.linearIssues && !filters.githubIssues) return "pullRequests";
  return "all";
}

function typeChoicePatch(choice: WorkTypeChoice): Partial<WorkFilters> {
  const issues = choice !== "pullRequests";
  return {
    linearIssues: issues,
    githubIssues: issues,
    pullRequests: choice !== "issues",
  };
}

/** The Filter menu's filters set away from their defaults (state, repo, project, in my sidebar). */
export function menuFilterCount(filters: WorkFilters): number {
  let count = 0;
  if (filters.status !== "open") count += 1;
  if (filters.projectId) count += 1;
  if (filters.trackerProject) count += 1;
  if (filters.inSidebar) count += 1;
  return count;
}

/** The list's only order today; the View menu's Sort section lists it. */
const SORT_LABEL = "Recently updated";

type FilterSection = "status" | "repo" | "project";

/**
 * CDXC:WorkMode 2026-10-10 DECISION:
 * User: "Let's try c for filter bar". Under the search row sits one row that never wraps: on the left a single-select type segment (All | the tracker's issues | PRs) and the "Assigned to me" toggle ("Mine" on a narrow panel); on the right a Filter menu (funnel and a count: State, Repo, Project, "Only in my sidebar") and a View menu (the grouping's name: Group by and Sort). A narrow panel shrinks labels to icons instead of wrapping; the filters, their defaults and the remembered grouping behave as before.
 */
export function WorkFilterBar({
  filters,
  onFiltersChange,
  tracker,
  projects,
  trackerProjects,
  groupBy,
  onGroupByChange,
}: {
  filters: WorkFilters;
  onFiltersChange: (filters: WorkFilters) => void;
  tracker: WorkTracker;
  projects: WorkProject[];
  /** The Linear projects' or GitHub Projects' names in the list, sorted. */
  trackerProjects: string[];
  groupBy: WorkGroupBy;
  onGroupByChange: (groupBy: WorkGroupBy) => void;
}) {
  const set = (patch: Partial<WorkFilters>) =>
    onFiltersChange({ ...filters, ...patch });
  const type = typeChoiceOf(filters);
  const issuesLabel = tracker === "github" ? "GitHub issues" : "Linear issues";
  const count = menuFilterCount(filters);
  const groupByLabel =
    GROUP_BY_OPTIONS.find((option) => option.value === groupBy)?.label ??
    "None";
  const typeButton = (
    choice: WorkTypeChoice,
    className: string,
    label: string,
    content: ReactNode,
  ) => (
    <button
      type="button"
      role="radio"
      aria-checked={type === choice}
      aria-label={label}
      title={label}
      className={cx("w-seg-item", className, type === choice && "is-on")}
      onClick={() => set(typeChoicePatch(choice))}
    >
      {content}
    </button>
  );

  return (
    <div className="w-filterbar work-filters">
      <div
        className="w-seg work-type-segment"
        role="radiogroup"
        aria-label="Show"
      >
        {typeButton("all", "type-all", "Issues and PRs", "All")}
        {typeButton(
          "issues",
          tracker === "github" ? "type-gh-issues" : "type-linear",
          issuesLabel,
          <>
            <IconCircleDot
              size={13}
              className={tracker === "github" ? "c-open" : "c-linear"}
            />
            <span className="w-wide-only">{issuesLabel}</span>
          </>,
        )}
        {typeButton(
          "pullRequests",
          "type-prs",
          "Pull requests",
          <>
            <IconGitPullRequest size={13} className="c-open" />
            <span className="w-wide-only">PRs</span>
          </>,
        )}
      </div>
      <button
        type="button"
        aria-pressed={filters.assignedToMe}
        title="Assigned to me"
        className={cx(
          "w-toggle",
          "filter-assigned-to-me",
          filters.assignedToMe && "is-on",
        )}
        onClick={() => set({ assignedToMe: !filters.assignedToMe })}
      >
        <Avatar name="Me" size={15} />
        <span className="w-wide-only">Assigned to me</span>
        <span className="w-narrow-only">Mine</span>
      </button>
      <span className="w-spacer" />
      <Dropdown
        className="work-filter-menu"
        align="end"
        trigger={(open, toggle) => (
          <button
            type="button"
            aria-haspopup="menu"
            aria-expanded={open}
            title={count ? `Filters (${count} set)` : "Filters"}
            className={cx(
              "w-toggle",
              "work-filter-button",
              count > 0 && "is-on",
              open && "is-focus",
            )}
            onClick={toggle}
          >
            <IconFilter size={13} />
            <span className="w-wide-only">Filter</span>
            {count > 0 ? <span className="w-count">{count}</span> : null}
            <IconChevronDown size={12} />
          </button>
        )}
      >
        {() => (
          <FilterMenu
            filters={filters}
            set={set}
            tracker={tracker}
            projects={projects}
            trackerProjects={trackerProjects}
          />
        )}
      </Dropdown>
      <Dropdown
        className="work-view-menu"
        align="end"
        trigger={(open, toggle) => (
          <button
            type="button"
            aria-haspopup="menu"
            aria-expanded={open}
            title={
              groupBy === "none"
                ? `View: not grouped · ${SORT_LABEL}`
                : `View: grouped by ${groupByLabel} · ${SORT_LABEL}`
            }
            className={cx(
              "w-toggle",
              "work-view-button",
              groupBy !== "none" && "is-on",
              open && "is-focus",
            )}
            onClick={toggle}
          >
            <IconLayoutList size={13} />
            <span className="w-wide-only">
              {groupBy === "none" ? "View" : groupByLabel}
            </span>
            <IconChevronDown size={12} />
          </button>
        )}
      >
        {(close) => (
          <>
            <div className="w-menu-title">Group by</div>
            {GROUP_BY_OPTIONS.map((option) => (
              <MenuItem
                key={option.value}
                checked={groupBy === option.value}
                onSelect={() => {
                  onGroupByChange(option.value);
                  close();
                }}
              >
                {option.label}
              </MenuItem>
            ))}
            <div className="w-menu-sep" />
            <div className="w-menu-title">Sort</div>
            <MenuItem checked onSelect={close}>
              <IconArrowsSort size={13} />
              {SORT_LABEL}
            </MenuItem>
          </>
        )}
      </Dropdown>
    </div>
  );
}

/** State, Repo and Project open in place one at a time, so a choice keeps the menu open. */
function FilterMenu({
  filters,
  set,
  tracker,
  projects,
  trackerProjects,
}: {
  filters: WorkFilters;
  set: (patch: Partial<WorkFilters>) => void;
  tracker: WorkTracker;
  projects: WorkProject[];
  trackerProjects: string[];
}) {
  const [expanded, setExpanded] = useState<FilterSection | null>(null);
  const toggle = (section: FilterSection) =>
    setExpanded((current) => (current === section ? null : section));
  const statusLabel =
    STATUS_FILTERS.find((option) => option.value === filters.status)?.label ??
    "Open";
  const repoLabel =
    projects.find((project) => project.projectId === filters.projectId)?.name ??
    "All";
  const pick = (patch: Partial<WorkFilters>) => {
    set(patch);
    setExpanded(null);
  };

  return (
    <div className="w-filter-menu">
      <FilterRow
        className="filter-status"
        label="State"
        value={statusLabel}
        expanded={expanded === "status"}
        onToggle={() => toggle("status")}
      >
        {STATUS_FILTERS.map((option) => (
          <MenuItem
            key={option.value}
            checked={filters.status === option.value}
            onSelect={() => pick({ status: option.value })}
          >
            {option.label}
          </MenuItem>
        ))}
      </FilterRow>
      <FilterRow
        className="filter-project"
        icon={<IconFolder size={13} />}
        label="Repo"
        value={repoLabel}
        expanded={expanded === "repo"}
        onToggle={() => toggle("repo")}
      >
        <MenuItem
          checked={!filters.projectId}
          onSelect={() => pick({ projectId: "" })}
        >
          All repos
        </MenuItem>
        {projects.map((project) => (
          <MenuItem
            key={project.projectId}
            checked={filters.projectId === project.projectId}
            onSelect={() => pick({ projectId: project.projectId })}
          >
            {project.name}
            {project.repo ? (
              <span className="w-menu-hint">{project.repo}</span>
            ) : null}
          </MenuItem>
        ))}
      </FilterRow>
      <FilterRow
        className="filter-tracker-project"
        icon={
          <IconBox
            size={13}
            className={tracker === "linear" ? "c-linear" : undefined}
          />
        }
        label={tracker === "github" ? "GitHub Project" : "Linear project"}
        value={filters.trackerProject || "All"}
        expanded={expanded === "project"}
        onToggle={() => toggle("project")}
      >
        <MenuItem
          checked={!filters.trackerProject}
          onSelect={() => pick({ trackerProject: "" })}
        >
          All projects
        </MenuItem>
        {trackerProjects.map((name) => (
          <MenuItem
            key={name}
            checked={filters.trackerProject === name}
            onSelect={() => pick({ trackerProject: name })}
          >
            {name}
          </MenuItem>
        ))}
      </FilterRow>
      <div className="w-menu-sep" />
      <button
        type="button"
        role="menuitemcheckbox"
        aria-checked={filters.inSidebar}
        className="w-menu-item w-filter-row filter-in-sidebar"
        onClick={() => set({ inSidebar: !filters.inSidebar })}
      >
        <IconLayoutSidebar size={13} />
        Only in my sidebar
        <span
          className={cx("w-switch", filters.inSidebar && "is-on")}
          aria-hidden
        />
      </button>
    </div>
  );
}

function FilterRow({
  className,
  icon,
  label,
  value,
  expanded,
  onToggle,
  children,
}: {
  className: string;
  icon?: ReactNode;
  label: string;
  value: string;
  expanded: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div className={cx("w-filter-section", className)}>
      <button
        type="button"
        aria-expanded={expanded}
        className="w-menu-item w-filter-row"
        onClick={onToggle}
      >
        {icon}
        {label}
        <span className="w-filter-value" title={value}>
          <span className="w-filter-value-text">{value}</span>
          <IconChevronDown size={12} />
        </span>
      </button>
      {expanded ? <div className="w-filter-options">{children}</div> : null}
    </div>
  );
}
