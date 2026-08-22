import { describe, expect, test } from "vitest";
import {
  appendBrowsePathSegment,
  canNavigateUp,
  ensureBrowseDirectoryPath,
  getBrowseDirectoryPath,
  getBrowseLeafPathSegment,
  getBrowseParentPath,
  inferProjectTitleFromPath,
  isFilesystemBrowseQuery,
  resolveProjectPathForDispatch,
} from "./remote-project-paths";

describe("remote project picker path helpers", () => {
  test("keeps browse path navigation semantics", () => {
    expect(isFilesystemBrowseQuery("~/projects")).toBe(true);
    expect(isFilesystemBrowseQuery("./local")).toBe(true);
    expect(isFilesystemBrowseQuery("plain search")).toBe(false);
    expect(ensureBrowseDirectoryPath("~/projects")).toBe("~/projects/");
    expect(getBrowseDirectoryPath("~/projects/ghost")).toBe("~/projects/");
    expect(getBrowseLeafPathSegment("~/projects/ghost")).toBe("ghost");
    expect(appendBrowsePathSegment("~/projects/g", "ghostex")).toBe("~/projects/ghostex/");
    expect(getBrowseParentPath("~/projects/ghostex/")).toBe("~/projects/");
    expect(canNavigateUp("~/projects/ghostex/")).toBe(true);
    expect(canNavigateUp("/")).toBe(false);
  });

  test("resolves project paths for dispatch", () => {
    expect(resolveProjectPathForDispatch("~/projects/ghostex/")).toBe("~/projects/ghostex");
    expect(resolveProjectPathForDispatch("./app", "/Users/madda/dev")).toBe("/Users/madda/dev/app");
    expect(resolveProjectPathForDispatch("../app", "/Users/madda/dev/ghostex")).toBe("/Users/madda/dev/app");
    expect(inferProjectTitleFromPath("~/projects/ghostex")).toBe("ghostex");
  });
});
