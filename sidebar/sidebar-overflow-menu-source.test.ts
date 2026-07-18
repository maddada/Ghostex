import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const sidebarAppSource = readFileSync(new URL("./sidebar-app.tsx", import.meta.url), "utf8");
const groupPanelsCssSource = readFileSync(
  new URL("./styles/group-panels.css", import.meta.url),
  "utf8",
);

describe("sidebar recent projects source", () => {
  test("renders plain machine-scoped sections without the legacy drawer", () => {
    expect(sidebarAppSource).toContain("function RecentProjectsSection(");
    expect(sidebarAppSource).toContain("recentProjectsByMachine.remoteByMachineId.get(machine.id)");
    expect(sidebarAppSource).not.toContain("recent-projects-drawer");
    expect(sidebarAppSource).not.toContain("Search recent projects");
    expect(groupPanelsCssSource).toContain(".recent-projects-section");
    expect(groupPanelsCssSource).not.toContain(".recent-projects-drawer");
    expect(sidebarAppSource).not.toContain("function SidebarReferenceSettingsButton(");
  });
});
