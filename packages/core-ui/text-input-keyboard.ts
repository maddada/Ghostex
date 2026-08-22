export type TextInputEditState = {
  selectionEnd?: number | null;
  selectionStart?: number | null;
  value: string;
};

export type TextInputEditResult = {
  selectionEnd: number;
  selectionStart: number;
  value: string;
};

export function isTextEditingKey(
  event: Pick<KeyboardEvent, "altKey" | "ctrlKey" | "isComposing" | "key" | "metaKey">,
): boolean {
  if (event.altKey || event.isComposing) {
    return false;
  }

  if ((event.ctrlKey || event.metaKey) && event.key === "Backspace") {
    return true;
  }

  if (event.ctrlKey || event.metaKey) {
    return false;
  }

  return event.key === "Backspace" || event.key === "Delete" || event.key.length === 1;
}

export function isEditableKeyboardTarget(target: EventTarget | null): boolean {
  if (typeof HTMLElement === "undefined" || !(target instanceof HTMLElement)) {
    return false;
  }

  if (target.isContentEditable) {
    return true;
  }

  return (
    (typeof HTMLInputElement !== "undefined" && target instanceof HTMLInputElement) ||
    (typeof HTMLTextAreaElement !== "undefined" && target instanceof HTMLTextAreaElement) ||
    (typeof HTMLSelectElement !== "undefined" && target instanceof HTMLSelectElement)
  );
}

export function applyTextEditingKey(
  state: TextInputEditState,
  key: string,
  modifiers?: Pick<KeyboardEvent, "ctrlKey" | "metaKey">,
): TextInputEditResult | undefined {
  const { selectionEnd, selectionStart, value } = normalizeSelection(state);

  if (key === "Backspace") {
    if (selectionStart === selectionEnd && (modifiers?.ctrlKey || modifiers?.metaKey)) {
      if (selectionStart === 0) {
        return {
          selectionEnd,
          selectionStart,
          value,
        };
      }

      const nextSelectionStart = findPreviousWordBoundary(value, selectionStart);
      return {
        selectionEnd: nextSelectionStart,
        selectionStart: nextSelectionStart,
        value: `${value.slice(0, nextSelectionStart)}${value.slice(selectionEnd)}`,
      };
    }

    if (selectionStart === selectionEnd) {
      if (selectionStart === 0) {
        return {
          selectionEnd,
          selectionStart,
          value,
        };
      }

      const nextSelectionStart = selectionStart - 1;
      return {
        selectionEnd: nextSelectionStart,
        selectionStart: nextSelectionStart,
        value: `${value.slice(0, nextSelectionStart)}${value.slice(selectionEnd)}`,
      };
    }

    return {
      selectionEnd: selectionStart,
      selectionStart,
      value: `${value.slice(0, selectionStart)}${value.slice(selectionEnd)}`,
    };
  }

  if (key === "Delete") {
    if (selectionStart === selectionEnd) {
      if (selectionStart >= value.length) {
        return {
          selectionEnd,
          selectionStart,
          value,
        };
      }

      return {
        selectionEnd: selectionStart,
        selectionStart,
        value: `${value.slice(0, selectionStart)}${value.slice(selectionStart + 1)}`,
      };
    }

    return {
      selectionEnd: selectionStart,
      selectionStart,
      value: `${value.slice(0, selectionStart)}${value.slice(selectionEnd)}`,
    };
  }

  if (key.length !== 1) {
    return undefined;
  }

  const nextSelectionStart = selectionStart + key.length;
  return {
    selectionEnd: nextSelectionStart,
    selectionStart: nextSelectionStart,
    value: `${value.slice(0, selectionStart)}${key}${value.slice(selectionEnd)}`,
  };
}

function normalizeSelection({
  selectionEnd,
  selectionStart,
  value,
}: TextInputEditState): TextInputEditResult {
  const valueLength = value.length;
  const normalizedSelectionStart = clampSelectionIndex(selectionStart ?? valueLength, valueLength);
  const normalizedSelectionEnd = clampSelectionIndex(selectionEnd ?? valueLength, valueLength);

  if (normalizedSelectionStart <= normalizedSelectionEnd) {
    return {
      selectionEnd: normalizedSelectionEnd,
      selectionStart: normalizedSelectionStart,
      value,
    };
  }

  return {
    selectionEnd: normalizedSelectionStart,
    selectionStart: normalizedSelectionEnd,
    value,
  };
}

function clampSelectionIndex(index: number, valueLength: number): number {
  return Math.min(Math.max(index, 0), valueLength);
}

function findPreviousWordBoundary(value: string, selectionStart: number): number {
  let index = selectionStart;

  while (index > 0 && /\s/u.test(value[index - 1] ?? "")) {
    index -= 1;
  }

  while (index > 0 && !/\s/u.test(value[index - 1] ?? "")) {
    index -= 1;
  }

  return index;
}
