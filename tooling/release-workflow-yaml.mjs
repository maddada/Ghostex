/*
 CDXC:Release 2026-09-01-11:20:
 Release preflight used to assert on literal substrings of the Actions workflow
 YAML. The "chore: Formatting" pass in 49831862 ran prettier, which normalises
 YAML scalars to single quotes, so `GHOSTEX_REQUIRE_BEADS_SMOKE: "1"` became
 `: '1'` and the assertion stopped matching while the gate itself was fully
 intact. 8.3.0 shipped past a check that was protecting nothing, and 8.4.0 only
 caught it by hand. Structural assertions need a parsed document, and this repo
 declares no YAML dependency, so this is a deliberately small parser for the
 subset GitHub Actions workflows use.

 It is strict on purpose: every construct it does not understand throws
 WorkflowYamlError instead of being guessed at or skipped. A wrong parse is the
 same silent-staleness failure mode this module exists to remove, so a parse
 failure must be loud and must be reported as "the check is broken", never as
 "the product regressed".

 Supported subset: block mappings, block sequences, flow mappings/sequences
 (including ones prettier wrapped across lines), single/double quoted scalars,
 plain scalars with more-indented continuation lines, `|` and `>` block scalars
 with `-`/`+` chomping and explicit indentation indicators, `#` comments
 outside quotes, and YAML-core scalar typing (bool/null/int/float/string).
 Unsupported and rejected: tabs for indentation, anchors, aliases, tags,
 explicit `? key` entries, and multiple documents.
*/

export class WorkflowYamlError extends Error {
  constructor(message) {
    super(message);
    this.name = 'WorkflowYamlError';
  }
}

const BLOCK_SCALAR_HEADER = /^([|>])(?:([+-])(\d+)?|(\d+)([+-])?)?$/;

function isIgnorable(line) {
  return line === undefined || /^\s*$/.test(line) || /^\s*#/.test(line);
}

function indentOf(line) {
  const match = /^[ \t]*/.exec(line)[0];
  if (match.includes('\t')) {
    throw new WorkflowYamlError(`Tab indentation is not supported: ${JSON.stringify(line)}`);
  }
  return match.length;
}

/*
 Returns the index where a trailing `#` comment starts, or -1. A `#` only opens
 a comment at the start of the line or after whitespace, and never inside a
 quoted scalar. This is the single place quote awareness matters for comments;
 block scalar bodies never reach it because they are consumed raw.
*/
function commentIndex(line) {
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (quote === "'") {
      if (char === "'") {
        if (line[index + 1] === "'") {
          index += 1;
        } else {
          quote = null;
        }
      }
      continue;
    }
    if (quote === '"') {
      if (char === '\\') {
        index += 1;
      } else if (char === '"') {
        quote = null;
      }
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      continue;
    }
    if (char === '#' && (index === 0 || /\s/.test(line[index - 1]))) {
      return index;
    }
  }
  return -1;
}

function stripComment(line) {
  const index = commentIndex(line);
  return (index === -1 ? line : line.slice(0, index)).replace(/\s+$/, '');
}

function unquoteDouble(text) {
  let out = '';
  for (let index = 1; index < text.length - 1; index += 1) {
    const char = text[index];
    if (char !== '\\') {
      out += char;
      continue;
    }
    index += 1;
    const escaped = text[index];
    if (escaped === 'n') out += '\n';
    else if (escaped === 't') out += '\t';
    else if (escaped === 'r') out += '\r';
    else if (escaped === '0') out += '\0';
    else if (escaped === 'u') {
      out += String.fromCharCode(Number.parseInt(text.slice(index + 1, index + 5), 16));
      index += 4;
    } else out += escaped;
  }
  return out;
}

function parseScalarText(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("'")) {
    if (trimmed.length < 2 || !trimmed.endsWith("'")) {
      throw new WorkflowYamlError(`Unterminated single-quoted scalar: ${trimmed}`);
    }
    return trimmed.slice(1, -1).replaceAll("''", "'");
  }
  if (trimmed.startsWith('"')) {
    if (trimmed.length < 2 || !trimmed.endsWith('"')) {
      throw new WorkflowYamlError(`Unterminated double-quoted scalar: ${trimmed}`);
    }
    return unquoteDouble(trimmed);
  }
  return trimmed;
}

/*
 YAML-core typing, matching what a spec parser produces for these files. Version
 strings such as `1.4.0` and `24.13.1` stay strings; `90` and `150` become
 numbers. Assertions still compare with String(...) so a gate written as `1` and
 one written as `'1'` are treated identically, which is the whole point.
*/
function typeScalar(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("'") || trimmed.startsWith('"')) {
    return parseScalarText(trimmed);
  }
  if (trimmed === '' || trimmed === '~' || /^(?:null|Null|NULL)$/.test(trimmed)) {
    return null;
  }
  if (/^(?:true|True|TRUE)$/.test(trimmed)) return true;
  if (/^(?:false|False|FALSE)$/.test(trimmed)) return false;
  if (/^[-+]?\d+$/.test(trimmed)) return Number.parseInt(trimmed, 10);
  if (/^0x[0-9a-fA-F]+$/.test(trimmed)) return Number.parseInt(trimmed, 16);
  if (/^[-+]?(?:\d+\.\d*|\.\d+)(?:[eE][-+]?\d+)?$/.test(trimmed)) return Number.parseFloat(trimmed);
  if (/^[-+]?\d+(?:\.\d*)?[eE][-+]?\d+$/.test(trimmed)) return Number.parseFloat(trimmed);
  return trimmed;
}

function skipFlowWhitespace(text, start) {
  let index = start;
  while (index < text.length && /\s/.test(text[index])) index += 1;
  return index;
}

function readFlowQuoted(text, start) {
  const quote = text[start];
  let index = start + 1;
  while (index < text.length) {
    const char = text[index];
    if (quote === "'" && char === "'") {
      if (text[index + 1] === "'") {
        index += 2;
        continue;
      }
      return { end: index + 1, raw: text.slice(start, index + 1) };
    }
    if (quote === '"') {
      if (char === '\\') {
        index += 2;
        continue;
      }
      if (char === '"') {
        return { end: index + 1, raw: text.slice(start, index + 1) };
      }
    }
    index += 1;
  }
  throw new WorkflowYamlError(`Unterminated quoted scalar in flow collection: ${text.slice(start)}`);
}

function readFlowPlain(text, start) {
  let index = start;
  while (index < text.length) {
    // GitHub expressions may legally appear unquoted; skip them whole so their
    // braces are not mistaken for flow-collection punctuation.
    if (text.startsWith('${{', index)) {
      const close = text.indexOf('}}', index);
      if (close === -1) {
        throw new WorkflowYamlError(`Unterminated \${{ }} expression in flow collection: ${text.slice(start)}`);
      }
      index = close + 2;
      continue;
    }
    const char = text[index];
    if (char === ',' || char === '}' || char === ']') break;
    if (char === ':' && (index + 1 >= text.length || /[\s,}\]]/.test(text[index + 1]))) break;
    index += 1;
  }
  return { end: index, raw: text.slice(start, index).replace(/\s+$/, '') };
}

function parseFlowNode(text, start) {
  let index = skipFlowWhitespace(text, start);
  if (index >= text.length) {
    throw new WorkflowYamlError(`Unexpected end of flow collection: ${text}`);
  }
  if (text[index] === '{') return parseFlowMapping(text, index);
  if (text[index] === '[') return parseFlowSequence(text, index);
  const token = text[index] === '"' || text[index] === "'" ? readFlowQuoted(text, index) : readFlowPlain(text, index);
  if (token.raw === '') {
    throw new WorkflowYamlError(`Empty flow entry near: ${text.slice(index)}`);
  }
  return { end: token.end, value: typeScalar(token.raw) };
}

function parseFlowMapping(text, start) {
  const map = {};
  let index = skipFlowWhitespace(text, start + 1);
  if (text[index] === '}') return { end: index + 1, value: map };
  for (;;) {
    index = skipFlowWhitespace(text, index);
    const keyToken =
      text[index] === '"' || text[index] === "'" ? readFlowQuoted(text, index) : readFlowPlain(text, index);
    const key = parseScalarText(keyToken.raw);
    index = skipFlowWhitespace(text, keyToken.end);
    if (text[index] !== ':') {
      throw new WorkflowYamlError(`Expected ":" after flow mapping key ${key} in: ${text}`);
    }
    const entry = parseFlowNode(text, index + 1);
    if (Object.prototype.hasOwnProperty.call(map, key)) {
      throw new WorkflowYamlError(`Duplicate flow mapping key: ${key}`);
    }
    map[key] = entry.value;
    index = skipFlowWhitespace(text, entry.end);
    if (text[index] === ',') {
      index = skipFlowWhitespace(text, index + 1);
      if (text[index] === '}') return { end: index + 1, value: map };
      continue;
    }
    if (text[index] === '}') return { end: index + 1, value: map };
    throw new WorkflowYamlError(`Expected "," or "}" in flow mapping: ${text}`);
  }
}

function parseFlowSequence(text, start) {
  const items = [];
  let index = skipFlowWhitespace(text, start + 1);
  if (text[index] === ']') return { end: index + 1, value: items };
  for (;;) {
    const entry = parseFlowNode(text, index);
    items.push(entry.value);
    index = skipFlowWhitespace(text, entry.end);
    if (text[index] === ',') {
      index = skipFlowWhitespace(text, index + 1);
      if (text[index] === ']') return { end: index + 1, value: items };
      continue;
    }
    if (text[index] === ']') return { end: index + 1, value: items };
    throw new WorkflowYamlError(`Expected "," or "]" in flow sequence: ${text}`);
  }
}

function flowIsBalanced(text) {
  let depth = 0;
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (char === '"' || char === "'") {
      index = readFlowQuoted(text, index).end;
      continue;
    }
    if (text.startsWith('${{', index)) {
      const close = text.indexOf('}}', index);
      if (close === -1) return false;
      index = close + 2;
      continue;
    }
    if (char === '{' || char === '[') depth += 1;
    if (char === '}' || char === ']') depth -= 1;
    index += 1;
  }
  return depth === 0;
}

function mappingColonIndex(content) {
  let quote = null;
  for (let index = 0; index < content.length; index += 1) {
    const char = content[index];
    if (quote === "'") {
      if (char === "'") {
        if (content[index + 1] === "'") index += 1;
        else quote = null;
      }
      continue;
    }
    if (quote === '"') {
      if (char === '\\') index += 1;
      else if (char === '"') quote = null;
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      continue;
    }
    if (char === ':' && (index + 1 === content.length || content[index + 1] === ' ')) {
      return index;
    }
  }
  return -1;
}

function splitMappingEntry(content) {
  const colon = mappingColonIndex(content);
  if (colon === -1) {
    throw new WorkflowYamlError(`Expected a "key: value" mapping entry, got: ${content}`);
  }
  return { key: parseScalarText(content.slice(0, colon)), rest: content.slice(colon + 1).trim() };
}

function foldBlockScalar(rawLines) {
  const folded = [];
  for (let index = 0; index < rawLines.length; index += 1) {
    const line = rawLines[index];
    const previous = folded.length > 0 ? folded[folded.length - 1] : null;
    if (line === '') {
      folded.push('');
      continue;
    }
    if (previous === null || previous === '' || /^\s/.test(line) || /^\s/.test(previous)) {
      folded.push(line);
      continue;
    }
    folded[folded.length - 1] = `${previous} ${line}`;
  }
  return folded;
}

function applyChomping(text, chomp) {
  if (chomp === '+') return text;
  const stripped = text.replace(/\n+$/, '');
  if (chomp === '-') return stripped;
  return stripped === '' ? '' : `${stripped}\n`;
}

function parseBlockScalar(state, keyIndent, header) {
  const match = BLOCK_SCALAR_HEADER.exec(header);
  if (!match) {
    throw new WorkflowYamlError(`Unsupported block scalar header: ${header}`);
  }
  const style = match[1];
  const chomp = match[2] ?? match[5] ?? '';
  const explicitIndent = match[3] ?? match[4];
  const collected = [];
  while (state.index < state.lines.length) {
    const line = state.lines[state.index];
    if (/^\s*$/.test(line)) {
      collected.push('');
      state.index += 1;
      continue;
    }
    if (indentOf(line) <= keyIndent) break;
    collected.push(line.replace(/\s+$/, ''));
    state.index += 1;
  }
  while (collected.length > 0 && collected[collected.length - 1] === '') {
    collected.pop();
    state.index -= 1;
  }
  if (collected.length === 0) {
    return applyChomping('', chomp);
  }
  const firstContent = collected.find((line) => line !== '');
  const contentIndent = explicitIndent ? keyIndent + Number.parseInt(explicitIndent, 10) : indentOf(firstContent);
  const body = collected.map((line) => (line === '' ? '' : line.slice(contentIndent)));
  const lines = style === '>' ? foldBlockScalar(body) : body;
  return applyChomping(`${lines.join('\n')}\n`, chomp);
}

function parsePlainScalar(state, keyIndent, rest) {
  let text = rest;
  while (state.index < state.lines.length) {
    const line = state.lines[state.index];
    if (isIgnorable(line)) break;
    if (indentOf(line) <= keyIndent) break;
    const continued = stripComment(line).trim();
    if (continued === '') break;
    text += ` ${continued}`;
    state.index += 1;
  }
  return typeScalar(text);
}

function parseFlowValue(state, rest) {
  let text = rest;
  while (!flowIsBalanced(text)) {
    if (state.index >= state.lines.length) {
      throw new WorkflowYamlError(`Unterminated flow collection: ${text}`);
    }
    const line = state.lines[state.index];
    state.index += 1;
    if (isIgnorable(line)) continue;
    text += ` ${stripComment(line).trim()}`;
  }
  const node = parseFlowNode(text, 0);
  const tail = text.slice(node.end).trim();
  if (tail !== '') {
    throw new WorkflowYamlError(`Unexpected trailing content after flow collection: ${tail}`);
  }
  return node.value;
}

function skipIgnorable(state) {
  while (state.index < state.lines.length && isIgnorable(state.lines[state.index])) {
    state.index += 1;
  }
}

function rejectUnsupported(content) {
  if (content.startsWith('&') || content.startsWith('*') || content.startsWith('!')) {
    throw new WorkflowYamlError(`Anchors, aliases, and tags are not supported: ${content}`);
  }
  if (content === '?' || content.startsWith('? ')) {
    throw new WorkflowYamlError(`Explicit "? key" mapping entries are not supported: ${content}`);
  }
}

function parseNode(state, minIndent) {
  skipIgnorable(state);
  if (state.index >= state.lines.length) return null;
  const line = state.lines[state.index];
  const indent = indentOf(line);
  if (indent < minIndent) return null;
  const content = stripComment(line).slice(indent);
  rejectUnsupported(content);
  if (content === '-' || content.startsWith('- ')) return parseSequence(state, indent);
  if (content.startsWith('{') || content.startsWith('[')) {
    state.index += 1;
    return parseFlowValue(state, content);
  }
  if (mappingColonIndex(content) !== -1) return parseMapping(state, indent);
  /*
   A bare scalar sitting where a block node is expected, such as each entry of a
   `needs:` sequence. Continuation lines must be indented at least as far as this
   line, so a sibling sequence entry one column out ends the scalar.
  */
  state.index += 1;
  if (BLOCK_SCALAR_HEADER.test(content)) return parseBlockScalar(state, indent - 1, content);
  return parsePlainScalar(state, indent - 1, content);
}

function parseSequence(state, indent) {
  const items = [];
  for (;;) {
    skipIgnorable(state);
    if (state.index >= state.lines.length) break;
    const line = state.lines[state.index];
    if (indentOf(line) !== indent) break;
    const content = stripComment(line).slice(indent);
    if (content !== '-' && !content.startsWith('- ')) break;
    if (content === '-') {
      state.index += 1;
      items.push(parseNode(state, indent + 1));
      continue;
    }
    /*
     Rewrite `- key: value` into `  key: value` in this parser's own copy of the
     lines so the item body is parsed by the ordinary block-node code at its real
     column. The mutation never escapes this parse.
    */
    state.lines[state.index] = ' '.repeat(indent + 2) + content.slice(2);
    items.push(parseNode(state, indent + 2));
  }
  return items;
}

function parseMapping(state, indent) {
  const map = {};
  for (;;) {
    skipIgnorable(state);
    if (state.index >= state.lines.length) break;
    const line = state.lines[state.index];
    const lineIndent = indentOf(line);
    if (lineIndent < indent) break;
    if (lineIndent > indent) {
      throw new WorkflowYamlError(`Unexpected indentation at: ${JSON.stringify(line)}`);
    }
    const content = stripComment(line).slice(indent);
    if (content === '-' || content.startsWith('- ')) break;
    rejectUnsupported(content);
    const { key, rest } = splitMappingEntry(content);
    state.index += 1;
    let value;
    if (rest === '') {
      value = parseNode(state, indent + 1);
    } else if (BLOCK_SCALAR_HEADER.test(rest)) {
      value = parseBlockScalar(state, indent, rest);
    } else if (rest.startsWith('{') || rest.startsWith('[')) {
      value = parseFlowValue(state, rest);
    } else {
      value = parsePlainScalar(state, indent, rest);
    }
    if (Object.prototype.hasOwnProperty.call(map, key)) {
      throw new WorkflowYamlError(`Duplicate mapping key: ${key}`);
    }
    map[key] = value;
  }
  return map;
}

export function parseWorkflowYaml(text) {
  const state = { index: 0, lines: text.split(/\r?\n/) };
  skipIgnorable(state);
  if (state.index < state.lines.length && state.lines[state.index].trim() === '---') {
    state.index += 1;
  }
  skipIgnorable(state);
  if (state.index >= state.lines.length) return null;
  const document = parseNode(state, 0);
  skipIgnorable(state);
  if (state.index < state.lines.length) {
    throw new WorkflowYamlError(
      `Unparsed trailing content (multiple documents are not supported): ${JSON.stringify(state.lines[state.index])}`
    );
  }
  return document;
}

/* Depth-first walk over every node of a parsed document. */
export function* walkYaml(node, keyPath = []) {
  yield { node, keyPath };
  if (Array.isArray(node)) {
    for (const [index, item] of node.entries()) {
      yield* walkYaml(item, [...keyPath, index]);
    }
    return;
  }
  if (node && typeof node === 'object') {
    for (const [key, value] of Object.entries(node)) {
      yield* walkYaml(value, [...keyPath, key]);
    }
  }
}

/* Every value stored under `key`, anywhere in the document, with its path. */
export function collectByKey(document, key) {
  const found = [];
  for (const { node, keyPath } of walkYaml(document)) {
    if (node && typeof node === 'object' && !Array.isArray(node) && Object.hasOwn(node, key)) {
      found.push({ path: [...keyPath, key].join('.'), value: node[key] });
    }
  }
  return found;
}

/* Every string scalar in the document that contains `needle`, with its path. */
export function collectScalarsContaining(document, needle) {
  const found = [];
  for (const { node, keyPath } of walkYaml(document)) {
    if (typeof node === 'string' && node.includes(needle)) {
      found.push({ path: keyPath.join('.'), value: node });
    }
  }
  return found;
}
