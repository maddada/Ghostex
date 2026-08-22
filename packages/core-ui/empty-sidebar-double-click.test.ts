import { describe, expect, test, vi } from "vitest";
import {
  isEmptySidebarDoubleClick,
  SIDEBAR_EMPTY_SPACE_BLOCKER_SELECTOR,
} from "./empty-sidebar-double-click";

class FakeNode {}

class FakeElement extends FakeNode {
  public readonly children: FakeElement[] = [];
  public dataset: Record<string, string> = {};
  public parentElement: FakeElement | undefined;

  public constructor(public readonly tagName: string) {
    super();
  }

  public append(...children: FakeElement[]): void {
    for (const child of children) {
      child.parentElement = this;
      this.children.push(child);
    }
  }

  public contains(target: FakeElement): boolean {
    if (target === this) {
      return true;
    }

    return this.children.some((child) => child.contains(target));
  }

  public closest(selector: string): FakeElement | null {
    let current: FakeElement | undefined = this;
    while (current) {
      if (matchesSidebarBlockerSelector(current, selector)) {
        return current;
      }
      current = current.parentElement;
    }
    return null;
  }
}

type FakeDocument = {
  body: FakeElement;
  createElement: (tagName: string) => FakeElement;
  elementsFromPoint?: (x: number, y: number) => FakeElement[];
};

const originalDocument = globalThis.document;
const originalElement = globalThis.Element;
const originalNode = globalThis.Node;

function installFakeDom(): FakeDocument {
  const fakeDocument: FakeDocument = {
    body: new FakeElement("body"),
    createElement: (tagName: string) => new FakeElement(tagName),
  };
  Object.defineProperty(globalThis, "Node", {
    configurable: true,
    value: FakeNode,
  });
  Object.defineProperty(globalThis, "Element", {
    configurable: true,
    value: FakeElement,
  });
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: fakeDocument,
  });
  return fakeDocument;
}

function restoreFakeDom(): void {
  Object.defineProperty(globalThis, "Node", {
    configurable: true,
    value: originalNode,
  });
  Object.defineProperty(globalThis, "Element", {
    configurable: true,
    value: originalElement,
  });
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: originalDocument,
  });
}

function matchesSidebarBlockerSelector(element: FakeElement, selector: string): boolean {
  if (selector !== SIDEBAR_EMPTY_SPACE_BLOCKER_SELECTOR) {
    return false;
  }

  return (
    ["button", "input", "select", "textarea", "a"].includes(element.tagName) ||
    element.dataset.emptySpaceBlocking !== undefined
  );
}

describe("isEmptySidebarDoubleClick", () => {
  test("should allow double clicks that land in unoccupied sidebar gutter space", () => {
    const fakeDocument = installFakeDom();
    const currentTarget = document.createElement("div");
    const content = document.createElement("div");

    currentTarget.append(content);
    fakeDocument.body.append(currentTarget);

    const elementsFromPointMock = vi.fn(() => [content, currentTarget, document.body]);
    Object.defineProperty(document, "elementsFromPoint", {
      configurable: true,
      value: elementsFromPointMock,
    });

    expect(
      isEmptySidebarDoubleClick({
        clientX: 120,
        clientY: 48,
        currentTarget,
        target: content,
      }),
    ).toBe(true);

    expect(elementsFromPointMock).toHaveBeenCalledWith(120, 48);
    restoreFakeDom();
  });

  test("should block double clicks when a non-empty sidebar surface is under the pointer", () => {
    const fakeDocument = installFakeDom();
    const currentTarget = document.createElement("div");
    const groupSurface = document.createElement("section");
    const emptyArea = document.createElement("div");

    groupSurface.dataset.emptySpaceBlocking = "true";
    currentTarget.append(groupSurface, emptyArea);
    fakeDocument.body.append(currentTarget);

    Object.defineProperty(document, "elementsFromPoint", {
      configurable: true,
      value: vi.fn(() => [groupSurface, currentTarget, document.body]),
    });

    expect(
      isEmptySidebarDoubleClick({
        clientX: 80,
        clientY: 24,
        currentTarget,
        target: emptyArea,
      }),
    ).toBe(false);
    restoreFakeDom();
  });

  test("should fall back to target ancestry when point-based hit testing is unavailable", () => {
    const fakeDocument = installFakeDom();
    const currentTarget = document.createElement("div");
    const emptyArea = document.createElement("div");
    const button = document.createElement("button");

    currentTarget.append(emptyArea, button);
    fakeDocument.body.append(currentTarget);

    Object.defineProperty(document, "elementsFromPoint", {
      configurable: true,
      value: undefined,
    });

    expect(
      isEmptySidebarDoubleClick({
        clientX: 0,
        clientY: 0,
        currentTarget,
        target: emptyArea,
      }),
    ).toBe(true);
    expect(button.closest(SIDEBAR_EMPTY_SPACE_BLOCKER_SELECTOR)).toBe(button);
    expect(
      isEmptySidebarDoubleClick({
        clientX: 0,
        clientY: 0,
        currentTarget,
        target: button,
      }),
    ).toBe(false);
    restoreFakeDom();
  });
});
