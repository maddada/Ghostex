import { readLooseString } from "./primitives";

export const DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES: readonly string[] = [];

type TerminalDevServerPortRule = {
  lowerBound: number;
  upperBound: number;
};

export function normalizeTerminalDevServerIgnoredPortRuleInput(
  value: string,
): string | undefined {
  return parseTerminalDevServerPortRule(value)?.canonicalString;
}

export function normalizeTerminalDevServerIgnoredPortRules(candidate: unknown): readonly string[] {
  if (!Array.isArray(candidate)) {
    return DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES;
  }
  const mergedRules = mergeTerminalDevServerPortRules(
    candidate.map(readLooseString).flatMap((value) => {
      const rule = parseTerminalDevServerPortRule(value);
      return rule ? [rule] : [];
    }),
  ).map((rule) => rule.canonicalString);

  return mergedRules.length === 0 ? DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES : mergedRules;
}

function parseTerminalDevServerPortRule(value: string): (TerminalDevServerPortRule & {
  canonicalString: string;
}) | undefined {
  const trimmedValue = value.trim();
  if (!trimmedValue) {
    return undefined;
  }
  const rangeMatch = trimmedValue.match(/^(\d+)(?:\s*-\s*(\d+))?$/u);
  if (!rangeMatch) {
    return undefined;
  }
  const lowerBound = Number(rangeMatch[1]);
  const upperBound = rangeMatch[2] === undefined ? lowerBound : Number(rangeMatch[2]);
  if (
    !Number.isInteger(lowerBound) ||
    !Number.isInteger(upperBound) ||
    lowerBound < 1 ||
    upperBound > 65535 ||
    lowerBound > upperBound
  ) {
    return undefined;
  }
  return {
    lowerBound,
    upperBound,
    canonicalString:
      lowerBound === upperBound ? String(lowerBound) : `${lowerBound}-${upperBound}`,
  };
}

function mergeTerminalDevServerPortRules(
  rules: ReadonlyArray<TerminalDevServerPortRule>,
): Array<TerminalDevServerPortRule & { canonicalString: string }> {
  const mergedRules: TerminalDevServerPortRule[] = [];
  for (const rule of [...rules].sort((left, right) =>
    left.lowerBound === right.lowerBound
      ? left.upperBound - right.upperBound
      : left.lowerBound - right.lowerBound,
  )) {
    const previousRule = mergedRules.at(-1);
    if (!previousRule || rule.lowerBound > previousRule.upperBound + 1) {
      mergedRules.push({ lowerBound: rule.lowerBound, upperBound: rule.upperBound });
      continue;
    }
    previousRule.upperBound = Math.max(previousRule.upperBound, rule.upperBound);
  }

  return mergedRules.map((rule) => ({
    ...rule,
    canonicalString:
      rule.lowerBound === rule.upperBound
        ? String(rule.lowerBound)
        : `${rule.lowerBound}-${rule.upperBound}`,
  }));
}
