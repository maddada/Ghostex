import { describe, expect, test } from "vitest";
import { canSubmitAddRepositoryClone } from "./add-repository-modal-state";

describe("canSubmitAddRepositoryClone", () => {
  test("enables when typed clone fields are valid before preview returns", () => {
    expect(
      canSubmitAddRepositoryClone({
        branchName: "",
        folderPath: "/Users/madda/dev/_references",
        isCloning: false,
        newFolderName: "",
        repositoryInput: "robzilla1738/macbox",
      }),
    ).toBe(true);
    expect(
      canSubmitAddRepositoryClone({
        branchName: "feature/macbox",
        folderPath: "/Users/madda/dev/_references",
        isCloning: false,
        newFolderName: "custom-folder",
        repositoryInput: "robzilla1738/macbox",
      }),
    ).toBe(true);
  });

  test("keeps existing-destination preview warnings blocking submit", () => {
    expect(
      canSubmitAddRepositoryClone({
        branchName: "",
        clonePreview: { destinationExists: true },
        folderPath: "/Users/madda/dev/_references",
        isCloning: false,
        newFolderName: "macbox",
        repositoryInput: "robzilla1738/macbox",
      }),
    ).toBe(false);
  });

  test("rejects incomplete or unparsable clone fields", () => {
    expect(
      canSubmitAddRepositoryClone({
        branchName: "",
        folderPath: "/Users/madda/dev/_references",
        isCloning: false,
        newFolderName: "macbox",
        repositoryInput: "macbox",
      }),
    ).toBe(false);
    expect(
      canSubmitAddRepositoryClone({
        branchName: "",
        folderPath: "",
        isCloning: false,
        newFolderName: "macbox",
        repositoryInput: "robzilla1738/macbox",
      }),
    ).toBe(false);
  });

  test("rejects invalid typed branch names", () => {
    expect(
      canSubmitAddRepositoryClone({
        branchName: "feature branch",
        folderPath: "/Users/madda/dev/_references",
        isCloning: false,
        newFolderName: "",
        repositoryInput: "robzilla1738/macbox",
      }),
    ).toBe(false);
  });
});
