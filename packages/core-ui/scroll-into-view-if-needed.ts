type ScrollViewportElement = Pick<HTMLElement, "getBoundingClientRect">;
type ScrollTargetElement = Pick<HTMLElement, "getBoundingClientRect" | "scrollIntoView">;

export function isElementOutsideScrollViewport(
  element: ScrollTargetElement,
  scrollViewport: ScrollViewportElement,
): boolean {
  const elementBounds = element.getBoundingClientRect();
  const scrollViewportBounds = scrollViewport.getBoundingClientRect();

  return (
    elementBounds.top < scrollViewportBounds.top ||
    elementBounds.bottom > scrollViewportBounds.bottom
  );
}

export function scrollElementIntoViewIfNeeded(
  element: ScrollTargetElement,
  scrollViewport: ScrollViewportElement,
): boolean {
  if (!isElementOutsideScrollViewport(element, scrollViewport)) {
    return false;
  }

  element.scrollIntoView({
    behavior: "smooth",
    block: "nearest",
    inline: "nearest",
  });
  return true;
}
