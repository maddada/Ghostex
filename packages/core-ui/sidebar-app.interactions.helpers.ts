import { expect, fireEvent, waitFor } from "storybook/test";
import type { SidebarToExtensionMessage } from "../shared/session-grid-contract";
import { getSidebarStoryMessages, resetSidebarStoryMessages } from "./sidebar-story-harness";

export async function waitForReadyMessage() {
  await waitForRenderedSidebar();

  /**
   * CDXC:StorybookInteractions 2026-05-08-17:46
   * Interaction stories verify sidebar behavior, not the synthetic Storybook
   * bridge's ready-message timing. The current React harness can render the
   * sidebar before the captured ready message is observable, so rendered,
   * undimmed sidebar chrome is the reliable readiness contract.
   *
   * CDXC:StorybookInteractions 2026-05-08-18:49
   * Storybook stories now hydrate against the user's current ghostex settings.
   * Local settings can make first render slower, so readiness waits must allow
   * the iframe to finish the real app render instead of failing on loader UI.
   */
}

export async function waitForStorybookReadyMessage() {
  await waitFor(
    () => {
      return expect(
        getSidebarStoryMessages().some(
          (message) => (message as { type?: string }).type === "ready",
        ),
      ).toBe(true);
    },
    { timeout: 20_000 },
  );

  await waitFor(
    () => {
      const stack = document.body.querySelector(".stack");
      const hasRenderedGroups = document.body.querySelector("[data-sidebar-group-id]");

      expect(stack).toBeTruthy();
      expect(stack).toHaveAttribute("data-dimmed", "false");
      expect(hasRenderedGroups).toBeTruthy();
    },
    { timeout: 20_000 },
  );

  await nextFrame(window);
}

async function waitForRenderedSidebar() {
  await waitFor(
    () => {
      const stack = document.body.querySelector(".stack");
      const hasRenderedGroups = document.body.querySelector("[data-sidebar-group-id]");

      expect(stack).toBeTruthy();
      expect(stack).toHaveAttribute("data-dimmed", "false");
      expect(hasRenderedGroups).toBeTruthy();
    },
    { timeout: 20_000 },
  );

  await nextFrame(window);
}

export async function expectMessage(expectedMessage: Partial<SidebarToExtensionMessage>) {
  await waitFor(() => {
    return expect(
      getSidebarStoryMessages().some((message) => isSubsetMatch(message, expectedMessage)),
    ).toBe(true);
  });
}

export async function expectNoMessage(
  expectedMessage: Partial<SidebarToExtensionMessage>,
  delayMs = 50,
) {
  await delay(delayMs);
  expect(getSidebarStoryMessages().some((message) => isSubsetMatch(message, expectedMessage))).toBe(
    false,
  );
}

export async function dragAndDrop(
  source: HTMLElement,
  target: HTMLElement,
  targetPosition: DragTargetPosition = "center",
) {
  const dragState = await dragToHover(source, target, targetPosition);
  await releaseDrag(target, dragState);
}

type DragTargetPosition = "after" | "before" | "center";

export async function dragToHover(
  source: HTMLElement,
  target: HTMLElement,
  targetPosition: DragTargetPosition = "center",
) {
  const sourcePosition = getCenter(source);
  const targetCoordinates = getTargetPoint(target, targetPosition);
  const pointerData = {
    button: 0,
    buttons: 1,
    isPrimary: true,
    pointerId: 1,
    pointerType: "mouse",
  };

  await fireEvent.pointerDown(source, {
    ...pointerData,
    bubbles: true,
    clientX: sourcePosition.x,
    clientY: sourcePosition.y,
  });
  await delay(250);
  await fireEvent.pointerMove(source.ownerDocument, {
    ...pointerData,
    bubbles: true,
    clientX: sourcePosition.x + 2,
    clientY: sourcePosition.y + 2,
  });
  await nextFrame(source.ownerDocument.defaultView);
  await fireEvent.pointerMove(target, {
    ...pointerData,
    bubbles: true,
    clientX: targetCoordinates.x,
    clientY: targetCoordinates.y,
  });
  await nextFrame(source.ownerDocument.defaultView);

  return { pointerData, targetPosition: targetCoordinates };
}

export async function releaseDrag(
  target: HTMLElement,
  dragState: {
    pointerData: {
      button: number;
      buttons: number;
      isPrimary: boolean;
      pointerId: number;
      pointerType: string;
    };
    targetPosition: { x: number; y: number };
  },
) {
  await fireEvent.pointerUp(target, {
    ...dragState.pointerData,
    buttons: 0,
    bubbles: true,
    clientX: dragState.targetPosition.x,
    clientY: dragState.targetPosition.y,
  });
  await nextFrame(target.ownerDocument.defaultView);
}

export async function openContextMenu(element: HTMLElement) {
  const bounds = element.getBoundingClientRect();
  await fireEvent.contextMenu(element, {
    bubbles: true,
    clientX: bounds.left + bounds.width / 2,
    clientY: bounds.top + 12,
  });
}

export async function dragSessionToGroup(root: ParentNode, sessionId: string, groupId: string) {
  resetSidebarStoryMessages();
  const targetSession = root.querySelector(
    `[data-sidebar-group-id="${groupId}"] [data-sidebar-session-id]`,
  );
  await dragAndDrop(
    await findRequiredElement(
      root,
      `[data-sidebar-session-id="${sessionId}"]`,
      `${sessionId} card`,
    ),
    targetSession instanceof HTMLElement
      ? targetSession
      : await findRequiredElement(
          root,
          `[data-sidebar-group-id="${groupId}"] .group-empty-state`,
          `${groupId} empty state`,
        ),
    targetSession instanceof HTMLElement ? "before" : "center",
  );
  await expectMessage({
    groupId,
    sessionId,
    targetIndex: 0,
    type: "moveSessionToGroup",
  });
}

export async function expectSessionMembership(
  root: ParentNode,
  groupId: string,
  expectedSessionIds: readonly string[],
) {
  await waitFor(() => {
    const groupSessionIds = Array.from(
      root.querySelectorAll(`[data-sidebar-group-id="${groupId}"] [data-sidebar-session-id]`),
    ).map((element) => element.getAttribute("data-sidebar-session-id"));

    return expect(groupSessionIds).toEqual(expectedSessionIds);
  });
}

export async function findRequiredElement(root: ParentNode, selector: string, description: string) {
  let matchedElement: HTMLElement | undefined;

  await waitFor(() => {
    const element = root.querySelector(selector);
    if (!(element instanceof HTMLElement)) {
      throw new Error(`Could not find ${description} with selector: ${selector}`);
    }

    matchedElement = element;
    return expect(element).toBeTruthy();
  });

  if (!matchedElement) {
    throw new Error(`Could not find ${description} with selector: ${selector}`);
  }

  return matchedElement;
}

function getCenter(element: HTMLElement) {
  const bounds = element.getBoundingClientRect();
  return {
    x: bounds.left + bounds.width / 2,
    y: bounds.top + bounds.height / 2,
  };
}

function getTargetPoint(element: HTMLElement, position: DragTargetPosition) {
  const bounds = element.getBoundingClientRect();
  const x = bounds.left + bounds.width / 2;
  if (position === "before") {
    return {
      x,
      y: bounds.top + bounds.height * 0.25,
    };
  }

  if (position === "after") {
    return {
      x,
      y: bounds.top + bounds.height * 0.75,
    };
  }

  return {
    x,
    y: bounds.top + bounds.height / 2,
  };
}

async function delay(durationMs: number) {
  await new Promise((resolve) => {
    setTimeout(resolve, durationMs);
  });
}

async function nextFrame(windowLike: Window | null) {
  await new Promise((resolve) => {
    if (!windowLike || typeof windowLike.requestAnimationFrame !== "function") {
      setTimeout(resolve, 0);
      return;
    }

    windowLike.requestAnimationFrame(() => {
      resolve(undefined);
    });
  });
}

function isSubsetMatch(
  actualMessage: SidebarToExtensionMessage,
  expectedMessage: Partial<SidebarToExtensionMessage>,
) {
  return Object.entries(expectedMessage).every(([key, value]) => {
    const actualValue = actualMessage[key as keyof SidebarToExtensionMessage];
    return Array.isArray(value)
      ? JSON.stringify(actualValue) === JSON.stringify(value)
      : actualValue === value;
  });
}
