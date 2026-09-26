export interface SessionChatFilePosition {
  line: number;
  endLine?: number;
  column?: number;
}

/** Trailing editor coordinates: `:913`, `:913-940`, or `:42:8`. */
const POSITION_SUFFIX = /:(\d{1,7})(?:-(\d{1,7})|(?::(\d{1,7})))?$/;

/** Splits a path from a valid line, line-range, or line-and-column suffix. */
export function splitSessionChatFilePosition(value: string): {
  path: string;
  position?: SessionChatFilePosition;
} {
  const match = POSITION_SUFFIX.exec(value);
  const line = Number(match?.[1]);
  const endLine = Number(match?.[2]);
  const column = Number(match?.[3]);
  if (
    !match ||
    !Number.isSafeInteger(line) ||
    line < 1 ||
    (match[2] !== undefined && (!Number.isSafeInteger(endLine) || endLine < line)) ||
    (match[3] !== undefined && (!Number.isSafeInteger(column) || column < 1))
  ) {
    return { path: value };
  }
  return {
    path: value.slice(0, value.length - match[0].length),
    position: {
      line,
      ...(match[2] === undefined ? {} : { endLine }),
      ...(match[3] === undefined ? {} : { column }),
    },
  };
}

/** The exact coordinate suffix shown beside a file name and in its tooltip. */
export function sessionChatFilePositionSuffix(position?: SessionChatFilePosition): string {
  if (!position) return '';
  const lines = position.endLine === undefined ? `${position.line}` : `${position.line}-${position.endLine}`;
  return `:${lines}${position.column === undefined ? '' : `:${position.column}`}`;
}
