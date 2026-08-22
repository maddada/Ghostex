import type { Meta, StoryObj } from "@storybook/react-vite";
import type { SidebarStoryArgs } from "./sidebar-story-fixtures";
import { CurrentProjectsSidebarStory } from "./sidebar-current-projects-story";
import {
  DEFAULT_SIDEBAR_STORY_ARGS,
  SIDEBAR_STORY_ARG_TYPES,
  SIDEBAR_STORY_DECORATORS,
  renderCombinedSidebarStory,
  renderSidebarStory,
} from "./sidebar-story-meta";

const meta = {
  title: "Sidebar/App",
  args: DEFAULT_SIDEBAR_STORY_ARGS,
  argTypes: SIDEBAR_STORY_ARG_TYPES,
  decorators: SIDEBAR_STORY_DECORATORS,
  render: renderSidebarStory,
} satisfies Meta<SidebarStoryArgs>;

export default meta;

type Story = StoryObj<typeof meta>;

export const CommandIndicatorActive: Story = {
  args: {
    fixture: "command-indicator-active",
    theme: "dark-blue",
  },
};

export const Default: Story = {};

export const AgentIconRender: Story = {
  args: {
    fixture: "agent-icon-render",
    highlightedVisibleCount: 3,
    showCloseButtonOnSessionCards: true,
    theme: "dark-blue",
    viewMode: "grid",
    visibleCount: 3,
  },
};

export const ActiveSortToggle: Story = {
  args: {
    fixture: "sort-toggle-demo",
    highlightedVisibleCount: 2,
    showCloseButtonOnSessionCards: true,
    theme: "dark-blue",
    viewMode: "grid",
    visibleCount: 2,
  },
};

export const SelectorStates: Story = {
  args: {
    fixture: "selector-states",
    highlightedVisibleCount: 4,
    isFocusModeActive: true,
    showCloseButtonOnSessionCards: true,
    theme: "dark-green",
    viewMode: "vertical",
    visibleCount: 1,
  },
};

export const OverflowStress: Story = {
  args: {
    fixture: "overflow-stress",
    highlightedVisibleCount: 6,
    showCloseButtonOnSessionCards: true,
    theme: "light-orange",
    viewMode: "grid",
    visibleCount: 6,
  },
};

export const ScrollEndRetention: Story = {
  args: {
    fixture: "scroll-end-retention",
    highlightedVisibleCount: 1,
    theme: "plain-dark",
    viewMode: "grid",
    visibleCount: 1,
  },
};

export const CurrentProjectsScrollRegression: Story = {
  args: {
    fixture: "combined-header-alignment",
    highlightedVisibleCount: 1,
    showCloseButtonOnSessionCards: true,
    theme: "plain-dark",
    viewMode: "grid",
    visibleCount: 1,
  },
  render: (args) => (
    <div className="native-sidebar-shell" data-sidebar-mode="combined">
      <main className="native-sidebar-main">
        <CurrentProjectsSidebarStory args={args} />
      </main>
    </div>
  ),
};

export const EmptyGroups: Story = {
  args: {
    fixture: "empty-groups",
    highlightedVisibleCount: 1,
    showCloseButtonOnSessionCards: false,
    theme: "dark-blue",
    viewMode: "horizontal",
    visibleCount: 1,
  },
};

export const CombinedHeaderAlignment: Story = {
  args: {
    fixture: "combined-header-alignment",
    highlightedVisibleCount: 1,
    showCloseButtonOnSessionCards: false,
    theme: "plain-dark",
    viewMode: "grid",
    visibleCount: 1,
  },
  render: renderCombinedSidebarStory,
};

export const CombinedRecentProjects: Story = {
  args: {
    fixture: "combined-recent-projects",
    highlightedVisibleCount: 1,
    showCloseButtonOnSessionCards: false,
    theme: "plain-dark",
    viewMode: "grid",
    visibleCount: 1,
  },
  render: renderCombinedSidebarStory,
};

export const CombinedSparseReference: Story = {
  args: {
    fixture: "combined-sparse-reference",
    highlightedVisibleCount: 1,
    showCloseButtonOnSessionCards: false,
    theme: "plain-dark",
    viewMode: "grid",
    visibleCount: 1,
  },
  render: renderCombinedSidebarStory,
};
