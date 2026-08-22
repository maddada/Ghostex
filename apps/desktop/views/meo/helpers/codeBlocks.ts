// @ts-nocheck
import { StateField, EditorState } from '@codemirror/state';
import { Decoration, EditorView, WidgetType } from '@codemirror/view';
import { syntaxTree, StreamLanguage } from '@codemirror/language';
import { javascript } from '@codemirror/lang-javascript';
import { python } from '@codemirror/lang-python';
import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { json } from '@codemirror/lang-json';
import { cpp } from '@codemirror/lang-cpp';
import { rust } from '@codemirror/lang-rust';
import { go } from '@codemirror/lang-go';
import { java } from '@codemirror/lang-java';
import { sql } from '@codemirror/lang-sql';
import { markdownLanguage } from '@codemirror/lang-markdown';
import { MermaidDiagramWidget, getFencedCodeContent } from './mermaidDiagram';
import { getMermaidColonBlocks } from './mermaidColonBlocks';

const shellLanguage = StreamLanguage.define({
  name: 'shell',
  startState: () => ({}),
  token: (stream: any) => {
    if (stream.match(/^#.*/)) return 'comment';
    if (stream.match(/^"[^$"]*"/)) return 'string';
    if (stream.match(/^'[^']*'/)) return 'string';
    if (stream.match(/^\$\{[^}]+\}/)) return 'variableName';
    if (stream.match(/^\$[a-zA-Z_][a-zA-Z0-9_]*/)) return 'variableName';
    if (stream.match(/^(if|then|else|elif|fi|for|do|done|while|case|esac|in|function|return|exit|echo|export|source|alias|unalias|cd|pwd|ls|grep|sed|awk|cat|printf|read|eval|local|declare|typeset|readonly|unset|shift|exec)\b/)) return 'keyword';
    if (stream.match(/^(true|false)\b/)) return 'bool';
    if (stream.match(/^\d+/)) return 'number';
    if (stream.match(/^[a-zA-Z_][a-zA-Z0-9_]*/)) return 'variableName';
    if (stream.match(/^[^"'#\s$`{|]+/)) return 'operator';
    stream.next();
    return null;
  }
});

const powerQueryKeywords = /^(let|in|each|if|then|else|try|otherwise|error|and|or|not|as|is|type|meta|section|shared)\b/i;
const powerQueryHashKeywords = /^#(date|time|datetime|datetimezone|duration|table|binary|sections|shared)\b/i;

function consumePowerQueryQuotedTail(stream: any): void {
  while (!stream.eol()) {
    if (!stream.skipTo('"')) {
      stream.skipToEnd();
      break;
    }
    stream.next();
    if (stream.peek() === '"') {
      stream.next();
      continue;
    }
    break;
  }
}

function consumePowerQueryQuoted(stream: any): boolean {
  if (stream.next() !== '"') {
    return false;
  }

  consumePowerQueryQuotedTail(stream);
  return true;
}

interface PowerQueryState {
  inBlockComment: boolean;
}

function consumePowerQueryBlockComment(stream: any, state: PowerQueryState): void {
  state.inBlockComment = true;
  while (!stream.eol()) {
    if (stream.match('*/')) {
      state.inBlockComment = false;
      break;
    }
    stream.next();
  }
}

const powerQueryLanguage = StreamLanguage.define({
  name: 'powerquery',
  startState: () => ({ inBlockComment: false }),
  token: (stream: any, state: PowerQueryState) => {
    if (stream.eatSpace()) {
      return null;
    }

    if (state.inBlockComment) {
      consumePowerQueryBlockComment(stream, state);
      return 'comment';
    }

    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }

    if (stream.match('/*')) {
      consumePowerQueryBlockComment(stream, state);
      return 'comment';
    }

    if (stream.match(/^\[[^\]\r\n]+\]/)) {
      return 'propertyName';
    }

    if (stream.match(/^@[a-z_][a-z0-9_]*/i)) {
      return 'variableName';
    }

    if (stream.match(powerQueryHashKeywords)) {
      return 'keyword';
    }

    if (stream.match(/^#"/)) {
      consumePowerQueryQuotedTail(stream);
      return 'string';
    }

    if (stream.peek() === '"') {
      consumePowerQueryQuoted(stream);
      return 'string';
    }

    if (stream.match(/^(true|false)\b/i)) {
      return 'bool';
    }

    if (stream.match(/^null\b/i)) {
      return 'atom';
    }

    if (stream.match(powerQueryKeywords)) {
      return 'keyword';
    }

    if (stream.match(/^\d+(?:\.\d+)?(?:e[+-]?\d+)?/i)) {
      return 'number';
    }

    if (stream.match(/^[a-z_][a-z0-9_.]*/i)) {
      return 'variableName';
    }

    if (stream.match(/^(?:=>|<=|>=|<>|=|<|>|\+|-|\*|\/|&|\?|!|,|;|:|\(|\)|\{|\}|\[|\])+/)) {
      return 'operator';
    }

    stream.next();
    return null;
  }
});

const csharpKeywords = new Set([
  "abstract", "as", "base", "break", "case", "catch", "checked", "class",
  "const", "continue", "default", "delegate", "do", "else", "enum", "event",
  "explicit", "extern", "finally", "fixed", "for", "foreach", "goto", "if",
  "implicit", "in", "interface", "internal", "is", "lock", "namespace", "new",
  "operator", "out", "override", "params", "private", "protected", "public",
  "readonly", "ref", "return", "sealed", "sizeof", "stackalloc", "static",
  "struct", "switch", "this", "throw", "try", "typeof", "unchecked", "unsafe",
  "using", "virtual", "void", "volatile", "while",
  "async", "await", "var", "dynamic", "yield", "when", "record", "init",
  "required", "file", "scoped", "partial", "where", "select", "group", "into",
  "let", "orderby", "join", "on", "equals", "by", "ascending", "descending",
  "from", "global", "not", "and", "or", "with", "nameof"
]);

const csharpBuiltinTypes = new Set([
  "bool", "byte", "char", "decimal", "double", "float", "int", "long",
  "object", "sbyte", "short", "string", "uint", "ulong", "ushort",
  "nint", "nuint"
]);

const csharpTypeKeywords = new Set([
  "class", "struct", "interface", "enum", "record", "new", "as", "is",
  "typeof", "sizeof", "nameof", "delegate", "event", "where"
]);

const csharpNamespaceKeywords = new Set(["namespace", "using"]);

type CSharpState = {
  inBlockComment: boolean;
  inVerbatimString: boolean;
  inRawString: boolean;
  rawQuoteCount: number;
  expectTypeName: boolean;
  expectNamespace: boolean;
  afterDot: boolean;
  inAttribute: boolean;
};

const csharpLanguage = StreamLanguage.define({
  name: "csharp",

  startState: (): CSharpState => ({
    inBlockComment: false,
    inVerbatimString: false,
    inRawString: false,
    rawQuoteCount: 0,
    expectTypeName: false,
    expectNamespace: false,
    afterDot: false,
    inAttribute: false
  }),

  token: (stream: any, state: CSharpState) => {
    // Handle multiline block comment
    if (state.inBlockComment) {
      while (!stream.eol()) {
        if (stream.match("*/")) {
          state.inBlockComment = false;
          break;
        }
        stream.next();
      }
      return "comment";
    }

    // Handle multiline verbatim string: @"..."
    if (state.inVerbatimString) {
      while (!stream.eol()) {
        if (stream.match('""')) continue; // escaped quote in verbatim string
        if (stream.match('"')) {
          state.inVerbatimString = false;
          break;
        }
        stream.next();
      }
      return "string";
    }

    // Handle multiline raw string: """ ... """
    if (state.inRawString) {
      const end = '"'.repeat(state.rawQuoteCount);
      while (!stream.eol()) {
        if (stream.match(end)) {
          state.inRawString = false;
          break;
        }
        stream.next();
      }
      return "string";
    }

    if (stream.eatSpace()) return null;

    // Preprocessor directives
    if (stream.sol() && stream.match(/^#\s*[A-Za-z_]\w*/)) {
      stream.skipToEnd();
      return "meta";
    }

    // Comments
    if (stream.match("//")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match("/*")) {
      state.inBlockComment = true;
      return "comment";
    }

    // Raw strings: """ ... """ or more quotes
    if (stream.match(/^"{3,}/)) {
      state.inRawString = true;
      state.rawQuoteCount = stream.current().length;
      return "string";
    }

    // Verbatim interpolated strings: $@"..." or @$"..."
    if (stream.match(/^\$@"/) || stream.match(/^@\$"/)) {
      state.inVerbatimString = true;
      return "string";
    }

    // Verbatim strings: @"..."
    if (stream.match(/^@"/)) {
      state.inVerbatimString = true;
      return "string";
    }

    // Interpolated regular string
    if (stream.match(/^\$"(?:[^"\\]|\\.)*"/)) return "string";

    // Regular string
    if (stream.match(/^"(?:[^"\\\r\n]|\\.)*"/)) return "string";

    // Char literal: one char or one escape
    if (stream.match(/^'(?:[^'\\\r\n]|\\.)'/)) return "string";

    // Hex before decimal
    if (stream.match(/^0[xX][0-9a-fA-F](?:[0-9a-fA-F_]*[0-9a-fA-F])?[uUlL]*/)) {
      return "number";
    }

    // Binary
    if (stream.match(/^0[bB][01](?:[01_]*[01])?[uUlL]*/)) {
      return "number";
    }

    // Decimal / float
    if (
      stream.match(
        /^(?:\d(?:[\d_]*\d)?)(?:\.(?:\d(?:[\d_]*\d)?)?)?(?:[eE][+-]?\d(?:[\d_]*\d)?)?[fFdDmM]?/
      )
    ) {
      return "number";
    }

    // Attribute brackets: [Serializable], [HttpGet("...")]
    if (stream.match("[")) {
      state.inAttribute = true;
      return "squareBracket";
    }
    if (stream.match("]")) {
      state.inAttribute = false;
      return "squareBracket";
    }

    // Identifiers, including escaped identifiers like @class
    if (stream.match(/^@?[A-Za-z_]\w*/)) {
      const raw = stream.current();
      const word = raw.startsWith("@") ? raw.slice(1) : raw;
      const next = stream.peek();
      const wasDot = state.afterDot;
      state.afterDot = false;

      if (csharpKeywords.has(word)) {
        state.expectTypeName = csharpTypeKeywords.has(word);
        state.expectNamespace = csharpNamespaceKeywords.has(word);
        return "keyword";
      }
      if (csharpBuiltinTypes.has(word)) {
        state.expectTypeName = false;
        state.expectNamespace = false;
        return "typeName";
      }
      if (word === "true" || word === "false") {
        state.expectTypeName = false;
        return "bool";
      }
      if (word === "null") {
        state.expectTypeName = false;
        return "atom";
      }

      // Namespace: using System.Collections.Generic
      if (state.expectNamespace) {
        return "namespace";
      }

      // Attribute name: [Serializable], [HttpGet]
      if (state.inAttribute) {
        return "attributeName";
      }

      // After a type-introducing keyword: new Foo, class Bar
      if (state.expectTypeName) {
        state.expectTypeName = false;
        // new Foo() — type followed by '(' is still a type (constructor)
        return "typeName";
      }

      const isUpperStart = word[0] >= "A" && word[0] <= "Z";

      // Member access: obj.Method() or obj.Property
      if (wasDot) {
        if (next === "(") return "variableName.function";
        if (next === "<") return "typeName";
        return "propertyName";
      }

      // PascalCase followed by '<' — generic type: List<T>
      if (isUpperStart && next === "<") {
        return "typeName";
      }

      // Function call: Method(...)
      if (next === "(") {
        return "variableName.function";
      }

      state.expectTypeName = false;
      return "variableName";
    }

    // Dot — member access
    if (stream.match(".")) {
      state.afterDot = true;
      return "punctuation";
    }

    // Colon — expect type name after ':' (inheritance, type constraints)
    if (stream.match(":")) {
      state.expectTypeName = true;
      return "punctuation";
    }

    // Angle brackets — '<' expects type arg, '>' does not
    if (stream.match("<")) {
      state.expectTypeName = true;
      return "angleBracket";
    }
    if (stream.match(">")) {
      return "angleBracket";
    }

    // Comma inside generic args or after base types keeps expecting types
    if (stream.match(",")) {
      return "punctuation";
    }

    // Parentheses and braces
    if (stream.match(/^[()]/)) return "paren";
    if (stream.match(/^[{}]/)) return "brace";

    // Semicolon
    if (stream.match(";")) {
      state.expectNamespace = false;
      return "punctuation";
    }

    // Lambda and other operators
    if (stream.match("=>")) return "operator";
    if (stream.match(/^[+\-*/%&|^!~?=<>]+/)) return "operator";

    stream.next();
    return null;
  }
});

const jsLanguage = javascript().language;
const jsxLanguage = javascript({ jsx: true }).language;
const tsLanguage = javascript({ typescript: true }).language;
const tsxLanguage = javascript({ typescript: true, jsx: true }).language;
const pythonLanguage = python().language;
const cssLanguage = css().language;
const htmlLanguage = html().language;
const jsonLanguage = json().language;
const swiftLanguage = cpp().language;
const rustLanguage = rust().language;
const goLanguage = go().language;
const javaLanguage = java().language;
const sqlLanguage = sql().language;
const markdownCodeLanguage = markdownLanguage;

const languageMap: Record<string, any> = {
  javascript: jsLanguage,
  js: jsLanguage,
  jsx: jsxLanguage,
  typescript: tsLanguage,
  ts: tsLanguage,
  tsx: tsxLanguage,
  python: pythonLanguage,
  py: pythonLanguage,
  css: cssLanguage,
  html: htmlLanguage,
  htm: htmlLanguage,
  json: jsonLanguage,
  markdown: markdownCodeLanguage,
  md: markdownCodeLanguage,
  swift: swiftLanguage,
  rust: rustLanguage,
  rs: rustLanguage,
  go: goLanguage,
  golang: goLanguage,
  java: javaLanguage,
  sql: sqlLanguage,
  csharp: csharpLanguage,
  cs: csharpLanguage,
  'c#': csharpLanguage,
  shell: shellLanguage,
  bash: shellLanguage,
  sh: shellLanguage,
  zsh: shellLanguage,
  m: powerQueryLanguage,
  powerquery: powerQueryLanguage,
  pq: powerQueryLanguage
};

export function resolveCodeLanguage(info: string | null | undefined): any {
  if (!info) {
    return null;
  }

  const normalized = info.toLowerCase().trim();

  return languageMap[normalized] ?? null;
}

export function resolveLiveCodeLanguage(info: string | null | undefined): any {
  if (!info) {
    return null;
  }

  const normalized = info.toLowerCase().trim();
  if (normalized === 'markdown' || normalized === 'md') {
    return null;
  }

  return languageMap[normalized] ?? null;
}

export function insertCodeBlock(view: EditorView, selection: { from: number; to: number; empty?: boolean }): void {
  const { state } = view;
  const line = state.doc.lineAt(selection.from);
  const lineText = state.doc.sliceString(line.from, line.to);
  const leadingWhitespace = /^(\s*)/.exec(lineText)![1];

  if (!selection.empty) {
    const selectedText = state.doc.sliceString(selection.from, selection.to);
    const insert = `\n${leadingWhitespace}\`\`\`\n${selectedText}\n${leadingWhitespace}\`\`\`\n`;
    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert },
      selection: { anchor: selection.from + leadingWhitespace.length + 4 }
    });
    return;
  }

  const contentWithoutLeadingWhitespace = lineText.slice(leadingWhitespace.length);
  const insert = `${leadingWhitespace}\`\`\`\n${contentWithoutLeadingWhitespace}\n${leadingWhitespace}\`\`\`\n`;
  const cursorPos = line.from + leadingWhitespace.length + 4;
  view.dispatch({
    changes: { from: line.from, to: line.to, insert },
    selection: { anchor: cursorPos }
  });
}

const sourceCodeBlockLine = Decoration.line({ class: 'meo-src-code-block' });
const mermaidColonFenceMarker = Decoration.mark({ class: 'meo-md-colon-fence-marker' });
const mermaidColonFenceCode = Decoration.mark({ class: 'meo-md-colon-fence-code' });

function computeSourceCodeBlockLines(state: EditorState): any {
  const ranges: any[] = [];
  syntaxTree(state).iterate({
    enter(node: any) {
      if (node.name !== 'FencedCode' && node.name !== 'CodeBlock') {
        return;
      }
      let line = state.doc.lineAt(node.from);
      const end = state.doc.lineAt(Math.max(node.to - 1, node.from)).number;
      while (line.number <= end) {
        ranges.push(sourceCodeBlockLine.range(line.from));
        if (line.number === end) {
          break;
        }
        line = state.doc.line(line.number + 1);
      }
      return false;
    }
  });

  for (const block of getMermaidColonBlocks(state)) {
    for (let lineNo = block.startLine; lineNo <= block.endLine; lineNo += 1) {
      const line = state.doc.line(lineNo);
      ranges.push(sourceCodeBlockLine.range(line.from));

      if (lineNo === block.startLine || lineNo === block.endLine) {
        ranges.push(mermaidColonFenceMarker.range(line.from, line.to));
        continue;
      }

      if (line.from < line.to) {
        ranges.push(mermaidColonFenceCode.range(line.from, line.to));
      }
    }
  }

  return Decoration.set(ranges, true);
}

export const sourceCodeBlockField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return computeSourceCodeBlockLines(state);
    } catch {
      return Decoration.none;
    }
  },
  update(lines: any, transaction: any) {
    if (!transaction.docChanged) {
      return lines;
    }
    try {
      return computeSourceCodeBlockLines(transaction.state);
    } catch {
      return lines;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field)
});

export function isFenceMarker(state: EditorState, from: number, to: number): boolean {
  const text = state.doc.sliceString(from, to);
  return /^`{3,}$/.test(text) || /^~{3,}$/.test(text);
}

export function getFencedCodeInfo(state: EditorState, node: any): string | null {
  let codeInfo: string | null = null;
  for (let child = node.node.firstChild; child; child = child.nextSibling) {
    if (child.name === 'CodeInfo') {
      codeInfo = state.doc.sliceString(child.from, child.to).trim().toLowerCase();
      break;
    }
  }
  return codeInfo;
}

class CopyCodeButtonWidget extends WidgetType {
  codeContent: string;

  constructor(codeContent: string) {
    super();
    this.codeContent = codeContent;
  }

  eq(other: WidgetType): boolean {
    return other instanceof CopyCodeButtonWidget && other.codeContent === this.codeContent;
  }

  toDOM(): HTMLElement {
    const container = document.createElement('span');
    container.className = 'meo-code-block-pill meo-copy-code-btn';
    container.setAttribute('aria-label', 'Copy code');
    container.setAttribute('role', 'button');
    container.setAttribute('tabindex', '0');
    container.textContent = 'copy';

    const updateText = (copied: boolean) => {
      container.textContent = copied ? 'copied' : 'copy';
      container.classList.toggle('copied', copied);
    };

    const copy = async (e: Event) => {
      e.preventDefault();
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(this.codeContent);
        updateText(true);
        setTimeout(() => updateText(false), 2000);
      } catch (err) {
        console.error('Failed to copy:', err);
      }
    };

    container.addEventListener('click', copy);
    container.addEventListener('keydown', async (e: KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        await copy(e);
      }
    });

    return container;
  }

  ignoreEvent(event: Event): boolean {
    return event.type !== 'pointerover' && event.type !== 'pointerout';
  }
}

class CodeLanguageLabelWidget extends WidgetType {
  labelText: string;

  constructor(labelText: string) {
    super();
    this.labelText = labelText;
  }

  eq(other: WidgetType): boolean {
    return other instanceof CodeLanguageLabelWidget && other.labelText === this.labelText;
  }

  toDOM(): HTMLElement {
    const label = document.createElement('span');
    label.className = 'meo-code-block-pill meo-code-language-label';
    label.textContent = this.labelText;
    label.setAttribute('aria-hidden', 'true');
    return label;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function addTopLineWidget(builder: any[], lineEnd: number, widget: WidgetType): void {
  builder.push(
    Decoration.widget({
      widget,
      side: 1
    }).range(lineEnd)
  );
}

export function addTopLineCopyButton(builder: any[], lineEnd: number, codeContent: string): void {
  if (!codeContent) {
    return;
  }
  addTopLineWidget(builder, lineEnd, new CopyCodeButtonWidget(codeContent));
}

export function addTopLinePillLabel(builder: any[], lineEnd: number, labelText: string | null): void {
  if (!labelText) {
    return;
  }
  addTopLineWidget(builder, lineEnd, new CodeLanguageLabelWidget(labelText));
}

const quotedFenceOpeningLineRegex = /^[ \t]{0,3}(?:>[ \t]?)*[ \t]{0,3}(?:`{3,}|~{3,})/;
const fenceLineRegex = /^[ \t]*[`~]{3,}.*$/;
const quotedFencePrefixRegex = /^[ \t]{0,3}((?:>[ \t]?)*)[ \t]{0,3}(?:`{3,}|~{3,})/;

export function addFenceOpeningLineMarker(builder: any[], state: EditorState, from: number, activeLines: Set<number>, addRange: Function, activeLineMarkerDeco: any, fenceMarkerDeco: any): void {
  const line = state.doc.lineAt(from);
  const text = state.doc.sliceString(line.from, line.to);
  // Support fenced code opening lines nested inside blockquotes/callouts, e.g. "> ```ts".
  if (!quotedFenceOpeningLineRegex.test(text)) {
    return;
  }

  if (activeLines.has(line.number)) {
    addRange(builder, line.from, line.to, activeLineMarkerDeco);
    return;
  }
  addRange(builder, line.from, line.to, fenceMarkerDeco);
}

export function addCodeLanguageLabel(builder: any[], state: EditorState, node: any, activeLines: Set<number>): void {
  if (node.name !== 'FencedCode') {
    return;
  }

  const startLine = state.doc.lineAt(node.from);
  if (activeLines.has(startLine.number)) {
    return;
  }

  const labelText = getFencedCodeInfo(state, node);
  if (!labelText) {
    return;
  }

  addTopLinePillLabel(builder, startLine.to, labelText);
}

export function addMermaidDiagram(builder: any[], state: EditorState, node: any): void {
  const startLine = state.doc.lineAt(node.from);
  const endLine = state.doc.lineAt(Math.max(node.to - 1, node.from));
  const diagramText = getFencedCodeContent(state, node);
  const fullBlockText = state.doc.sliceString(startLine.from, endLine.to);

  addMermaidDiagramBlock(builder, state, {
    startLine: startLine.number,
    endLine: endLine.number,
    diagramText,
    fullBlockText
  });
}

export function addMermaidDiagramBlock(
  builder: any[],
  state: EditorState,
  block: {
    startLine: number;
    endLine: number;
    diagramText: string;
    fullBlockText: string;
  }
): void {
  if (!block.diagramText.trim()) {
    return;
  }

  const startLine = state.doc.line(block.startLine);
  const endLine = state.doc.line(block.endLine);

  if (startLine.number >= endLine.number) {
    return;
  }

  const contentStartLine = state.doc.line(startLine.number + 1);
  const contentEndLine = state.doc.line(endLine.number - 1);

  if (contentStartLine.from >= contentEndLine.to) {
    return;
  }

  addTopLineCopyButton(builder, startLine.to, block.fullBlockText);

  const widget = new MermaidDiagramWidget(block.diagramText, startLine.number, endLine.number);
  builder.push(
    Decoration.replace({
      widget,
      block: true
    }).range(contentStartLine.from, contentEndLine.to)
  );
}

export function addCopyCodeButton(builder: any[], state: EditorState, from: number, to: number): void {
  const startLine = state.doc.lineAt(from);
  const endLine = state.doc.lineAt(Math.max(to - 1, from));
  const quoteDepth = getQuotedFenceDepth(startLine.text);

  const codeLines: string[] = [];
  for (let lineNum = startLine.number + 1; lineNum <= endLine.number; lineNum += 1) {
    const line = state.doc.line(lineNum);
    const lineText = stripLeadingQuotePrefix(line.text, quoteDepth);

    if (lineNum === endLine.number) {
      if (fenceLineRegex.test(lineText)) {
        continue;
      }
    }

    codeLines.push(lineText);
  }

  const codeContent = codeLines.join('\n');
  if (!codeContent) {
    return;
  }

  addTopLineCopyButton(builder, startLine.to, codeContent);
}

function getQuotedFenceDepth(lineText: string): number {
  const match = quotedFencePrefixRegex.exec(lineText);
  if (!match) {
    return 0;
  }
  return (match[1].match(/>/g) ?? []).length;
}

function stripLeadingQuotePrefix(lineText: string, quoteDepth: number): string {
  if (quoteDepth <= 0) {
    return lineText;
  }

  const prefixRe = new RegExp(`^[ \\t]{0,3}(?:>[ \\t]?){${quoteDepth}}`);
  const match = prefixRe.exec(lineText);
  if (!match) {
    return lineText;
  }
  return lineText.slice(match[0].length);
}
