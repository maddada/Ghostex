import { describe, expect, test, vi } from "vitest";
import {
  isElementOutsideScrollViewport,
  scrollElementIntoViewIfNeeded,
} from "./scroll-into-view-if-needed";

function createViewport(top: number, bottom: number) {
  return {
    getBoundingClientRect: () =>
      ({
        bottom,
        top,
      }) as DOMRect,
  };
}

function createElement(top: number, bottom: number) {
  return {
    getBoundingClientRect: () =>
      ({
        bottom,
        top,
      }) as DOMRect,
    scrollIntoView: vi.fn(),
  };
}

describe("isElementOutsideScrollViewport", () => {
  test("should return false when the element is fully visible", () => {
    const viewport = createViewport(100, 300);
    const element = createElement(140, 220);

    expect(isElementOutsideScrollViewport(element, viewport)).toBe(false);
  });

  test("should return true when the element starts above the viewport", () => {
    const viewport = createViewport(100, 300);
    const element = createElement(80, 180);

    expect(isElementOutsideScrollViewport(element, viewport)).toBe(true);
  });

  test("should return true when the element ends below the viewport", () => {
    const viewport = createViewport(100, 300);
    const element = createElement(220, 320);

    expect(isElementOutsideScrollViewport(element, viewport)).toBe(true);
  });
});

describe("scrollElementIntoViewIfNeeded", () => {
  test("should scroll smoothly to the nearest edge when the element is out of view", () => {
    const viewport = createViewport(100, 300);
    const element = createElement(320, 420);

    expect(scrollElementIntoViewIfNeeded(element, viewport)).toBe(true);
    expect(element.scrollIntoView).toHaveBeenCalledWith({
      behavior: "smooth",
      block: "nearest",
      inline: "nearest",
    });
  });

  test("should not scroll when the element is already in view", () => {
    const viewport = createViewport(100, 300);
    const element = createElement(130, 240);

    expect(scrollElementIntoViewIfNeeded(element, viewport)).toBe(false);
    expect(element.scrollIntoView).not.toHaveBeenCalled();
  });
});
