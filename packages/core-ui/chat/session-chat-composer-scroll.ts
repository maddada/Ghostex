/**
 * CDXC:SessionChat 2026-09-09 DECISION:
 * User: keep the composer's scroll fade, allow manual scrolling to put text in it, and move the active line clear as soon as the user types, like the supplied ChatGPT app examples.
 * Both composer editors reveal only their own scroll area after edits; scroll events never request a caret reveal.
 */
export function revealSessionChatComposerCaret(root: HTMLElement, caretRect?: DOMRect): DOMRect | undefined {
  if (document.activeElement !== root) return caretRect;
  let rect = caretRect;
  if (!rect) {
    const selection = document.getSelection();
    if (!selection?.focusNode || !root.contains(selection.focusNode)) return;
    const range = document.createRange();
    range.setStart(selection.focusNode, selection.focusOffset);
    range.collapse(true);
    rect = [...range.getClientRects()].find((value) => value.height > 0);
    // An empty contenteditable line is represented by a BR, not a text range.
    if (!rect && selection.focusNode instanceof Element) {
      const line = selection.focusNode.childNodes[selection.focusOffset];
      if (line instanceof HTMLBRElement) rect = line.getBoundingClientRect();
    }
  }
  if (!rect || rect.height === 0) return rect;
  const style = getComputedStyle(root);
  const lineHeight = Math.max(rect.height, parseFloat(style.lineHeight));
  const leading = (lineHeight - rect.height) / 2;
  const availableInset = Math.max(0, (root.clientHeight - lineHeight) / 2);
  const box = root.getBoundingClientRect();
  const top = box.top + root.clientTop + Math.min(parseFloat(style.scrollPaddingTop), availableInset);
  const bottom =
    box.top + root.clientTop + root.clientHeight - Math.min(parseFloat(style.scrollPaddingBottom), availableInset);
  const left = box.left + root.clientLeft;
  const right = left + root.clientWidth;
  const previousTop = root.scrollTop;
  const previousLeft = root.scrollLeft;
  root.scrollTop += rect.top - leading < top ? rect.top - leading - top : Math.max(0, rect.bottom + leading - bottom);
  root.scrollLeft += rect.left < left ? rect.left - left : Math.max(0, rect.right + 2 - right);
  return new DOMRect(
    rect.left - (root.scrollLeft - previousLeft),
    rect.top - (root.scrollTop - previousTop),
    rect.width,
    rect.height
  );
}
