//! Executor tool paths and programs, read the way Executor reads them, so the rule, the card and
//! the block on Executor's own app see the app and tool Executor will run. Anything Ghostex can't
//! read with certainty is refused rather than guessed.
//!
//! CDXC:Connectors 2026-11-27 WHY:
//! A single call's path is read as Executor's `canonical` does (packages/mcp/src/implementation/execute.ts in Executor 2: `tools`, then `.name` or `["json string"]` members joined with dots, the app being the part before the first dot). A program is read as Executor's own `programReach` does (the same file): every `tools` name not after a dot must be followed by `.app` or `["app"]`. Executor only uses that reading to load apps early and still runs a program that reaches `tools` another way (`const t = tools; t["exec" + "utor"]`), so Ghostex refuses those. The interpreter that runs programs (opencode's codemode) gives programs no other way to `tools`: no `globalThis`, `this`, `eval` or `Function`, only the `tools` binding. Ghostex's scan of the code is not acorn, so every `tools` word it didn't read as code (in a comment, string, template or regex it may have mis-split) is checked the same way, and `\u` escapes anywhere are refused.

use super::cli::MANAGEMENT_SLUG;

/// Executor's reserved roots, which are never app addresses (`app-slug.ts`).
const RESERVED_ROOTS: [&str; 4] = ["search", "constructor", "prototype", "then"];
/// Segment names Executor escapes in a tool path (`toolPath` in execute.ts).
const RESERVED_SEGMENTS: [&str; 3] = ["__proto__", "prototype", "constructor"];

pub(crate) const MANAGEMENT_REFUSED: &str = "Agents can't use Executor's own app (its apps, accounts and settings). The user manages Executor in Executor itself.";

/// What JavaScript's `String.prototype.trim` removes (WhiteSpace and LineTerminator).
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{B}' | '\u{C}' | ' ' | '\u{A0}' | '\u{FEFF}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\n'
                | '\r'
                | '\u{2028}'
                | '\u{2029}'
    )
}

/// An app address as Executor allows it: a DNS label, not a reserved root.
fn is_app_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !bytes.is_empty()
        && bytes.len() <= 63
        && edge(bytes[0])
        && edge(bytes[bytes.len() - 1])
        && bytes.iter().all(|&b| edge(b) || b == b'-')
        && !RESERVED_ROOTS.contains(&slug)
}

/// A tool path's segment as Executor names the tool (`toolName` in execute.ts).
fn tool_name(path: &str) -> String {
    path.split('.')
        .map(|segment| {
            if segment == "%00" {
                String::new()
            } else if segment
                .strip_prefix('%')
                .is_some_and(|rest| RESERVED_SEGMENTS.contains(&rest))
            {
                segment[1..].to_string()
            } else {
                segment.replace("%25", "%")
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// One member of a `tools…` expression: `.name` (`[A-Za-z_$][A-Za-z0-9_$]*`) or `["json"]`, and
/// what is left after it. `None` when neither matches (Executor's `expressionMember`).
fn expression_member(rest: &str) -> Option<Result<(String, &str), String>> {
    let ident_start = |c: char| c.is_ascii_alphabetic() || c == '_' || c == '$';
    let ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    if let Some(after) = rest.strip_prefix('.') {
        if !after.starts_with(ident_start) {
            return None;
        }
        let end = after.find(|c: char| !ident_char(c)).unwrap_or(after.len());
        return Some(Ok((after[..end].to_string(), &after[end..])));
    }
    let after = rest.strip_prefix("[\"")?;
    // `"(?:[^"\\]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"` then `]`.
    let mut chars = after.char_indices();
    let close = loop {
        let (index, c) = chars.next()?;
        match c {
            '"' => break index,
            '\\' => {
                let (_, escaped) = chars.next()?;
                match escaped {
                    '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => {}
                    'u' => {
                        for _ in 0..4 {
                            let (_, hex) = chars.next()?;
                            if !hex.is_ascii_hexdigit() {
                                return None;
                            }
                        }
                    }
                    _ => return None,
                }
            }
            _ => {}
        }
    };
    let remaining = after[close + 1..].strip_prefix(']')?;
    let quoted = &rest[1..close + 3];
    // Executor runs `JSON.parse` on it; a string JSON refuses (a control character, a lone
    // surrogate) is refused here too.
    Some(
        serde_json::from_str::<String>(quoted)
            .map(|segment| (segment, remaining))
            .map_err(|_| "Ghostex can't read that tool path's quoted part.".to_string()),
    )
}

/// The app and tool an `executor_call` path names, exactly as Executor resolves it:
/// `tools.linear.issues.list`, `tools["hacker-news"].top` and `linear.issues.list` all work.
pub(crate) fn call_path(path: &str) -> Result<(String, String), String> {
    let unreadable = || {
        format!(
            "Ghostex can't read the tool path {path:?}. Use the exact path search gave, like tools.<app>.<tool>."
        )
    };
    let trimmed = path.trim_matches(is_js_space);
    let canonical =
        if trimmed == "tools" || trimmed.starts_with("tools.") || trimmed.starts_with("tools[") {
            let mut segments: Vec<String> = Vec::new();
            let mut rest = &trimmed["tools".len()..];
            while !rest.is_empty() {
                match expression_member(rest) {
                    Some(Ok((segment, after))) => {
                        segments.push(segment);
                        rest = after;
                    }
                    Some(Err(error)) => return Err(error),
                    // Executor would look the whole text up as a path and find nothing.
                    None => return Err(unreadable()),
                }
            }
            segments.join(".")
        } else {
            trimmed.to_string()
        };
    let (slug, tool) = canonical.split_once('.').ok_or_else(unreadable)?;
    if slug == MANAGEMENT_SLUG {
        return Err(MANAGEMENT_REFUSED.to_string());
    }
    if !is_app_slug(slug) || tool.is_empty() {
        return Err(unreadable());
    }
    Ok((slug.to_string(), tool_name(tool)))
}

/// What a program reaches: each app it names, with the tool when the code calls one directly.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Reach {
    /// `(app slug, Some(tool))` for `tools.app.tool(…)`, `(app slug, None)` when the program may
    /// use any of the app's tools (it keeps `tools.app` or part of a path to use later).
    pub(crate) apps: Vec<(String, Option<String>)>,
    /// Whether it uses `search` (`tools.search…`).
    pub(crate) searches: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Punct(&'static str),
    /// A string's text between its quotes, as written.
    Str(String),
    Other,
}

/// Each token with the offset (in chars) it starts at.
type Tokens = Vec<(Token, usize)>;

const PUNCTUATORS: [&str; 52] = [
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=",
    "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "%=", "&=", "|=", "^=", "**",
    "<<", ">>", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "%", "&", "|",
    "^", "!", "~", "?",
];
const SINGLE_PUNCTUATORS: [&str; 5] = [":", "=", ".", "@", "#"];

/// Keywords after which a `/` starts a regular expression.
const REGEX_AFTER_WORDS: [&str; 15] = [
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
    "extends",
];

/// Keywords after which a `{` opens an object, not a block.
const EXPRESSION_AFTER_WORDS: [&str; 12] = [
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "yield",
    "await",
];

fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || (!c.is_ascii() && c.is_alphabetic())
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || c == '$'
        || (!c.is_ascii() && c.is_alphanumeric())
        || c == '\u{200C}'
        || c == '\u{200D}'
}

fn line_of(chars: &[char], at: usize) -> usize {
    chars[..at.min(chars.len())]
        .iter()
        .filter(|&&c| c == '\n')
        .count()
        + 1
}

/// Splits a program into tokens the way a JavaScript tokenizer would, closely enough for
/// [`program_reach`]: it fails on anything it can't read (an unknown character, an escape in a
/// name, an unclosed string, comment or regex). Also says whether it had to guess whether a `/`
/// started a regex, the one place its split can differ from JavaScript's.
fn scan(chars: &[char]) -> Result<(Tokens, bool), String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Brace {
        Block,
        Expression,
        Template,
    }
    let mut tokens: Vec<(Token, usize)> = Vec::new();
    let mut braces: Vec<Brace> = Vec::new();
    // Whether each open parenthesis is the head of `if`, `while`, `for` or `with`.
    let mut parens: Vec<bool> = Vec::new();
    // What the last `)` or `}` closed, for whether a `/` after it starts a regex.
    let mut closed_head = false;
    let mut closed_block = false;
    let mut uncertain = false;
    let mut i = 0;
    let fail =
        |at: usize, what: &str| -> String { format!("{what} (line {})", line_of(chars, at)) };
    // Whether a `/` here starts a regex, and whether that is a guess: after `}` (a block or an
    // object), `++`/`--`, or `of`/`yield` (a keyword or a name), JavaScript decides it from more
    // context than this scan keeps.
    let regex_allowed = |tokens: &[(Token, usize)], closed_head: bool, closed_block: bool| {
        let after_dot = tokens.len() >= 2
            && matches!(
                tokens[tokens.len() - 2].0,
                Token::Punct(".") | Token::Punct("?.")
            );
        match tokens.last().map(|(token, _)| token) {
            None => (true, false),
            Some(Token::Name(_)) if after_dot => (false, false),
            Some(Token::Name(word)) if matches!(word.as_str(), "of" | "yield") => (true, true),
            Some(Token::Name(word)) => (REGEX_AFTER_WORDS.contains(&word.as_str()), false),
            Some(Token::Str(_)) | Some(Token::Other) => (false, false),
            Some(Token::Punct(")")) => (closed_head, false),
            Some(Token::Punct("}")) => (closed_block, true),
            Some(Token::Punct("++")) | Some(Token::Punct("--")) => (false, true),
            Some(Token::Punct("]")) => (false, false),
            Some(Token::Punct(_)) => (true, false),
        }
    };
    // Reads a template from after its opening backtick (or a `}` that ends a `${`); returns
    // whether it stopped at `${`.
    let template = |start: usize| -> Result<(usize, bool), String> {
        let mut j = start;
        loop {
            match chars.get(j) {
                None => return Err(fail(start, "A template string isn't closed")),
                Some('\\') => j += 2,
                Some('`') => return Ok((j + 1, false)),
                Some('$') if chars.get(j + 1) == Some(&'{') => return Ok((j + 2, true)),
                Some(_) => j += 1,
            }
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if is_js_space(c) {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            let end = chars[i..]
                .iter()
                .position(|&c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
                .map_or(chars.len(), |offset| i + offset);
            i = end;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let end = (i + 2..chars.len().saturating_sub(1))
                .find(|&j| chars[j] == '*' && chars[j + 1] == '/')
                .ok_or_else(|| fail(i, "A comment isn't closed"))?;
            i = end + 2;
            continue;
        }
        let start = i;
        if c == '/' {
            let (regex, guessed) = regex_allowed(&tokens, closed_head, closed_block);
            uncertain |= guessed;
            if regex {
                let mut j = i + 1;
                let mut in_class = false;
                loop {
                    match chars.get(j) {
                        None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
                            return Err(fail(i, "A regular expression isn't closed"));
                        }
                        Some('\\') => j += 2,
                        Some('[') => {
                            in_class = true;
                            j += 1;
                        }
                        Some(']') => {
                            in_class = false;
                            j += 1;
                        }
                        Some('/') if !in_class => break,
                        Some(_) => j += 1,
                    }
                }
                j += 1;
                while chars.get(j).is_some_and(|&c| ident_char(c)) {
                    j += 1;
                }
                tokens.push((Token::Other, start));
                i = j;
            } else {
                let punct = if chars.get(i + 1) == Some(&'=') {
                    "/="
                } else {
                    "/"
                };
                tokens.push((Token::Punct(punct), start));
                i += punct.len();
            }
            continue;
        }
        if c == '"' || c == '\'' {
            let mut j = i + 1;
            let mut text = String::new();
            loop {
                match chars.get(j) {
                    None | Some('\n') | Some('\r') => {
                        return Err(fail(i, "A string isn't closed"));
                    }
                    Some('\\') => {
                        text.push('\\');
                        if let Some(&next) = chars.get(j + 1) {
                            text.push(next);
                        }
                        j += 2;
                    }
                    Some(&q) if q == c => break,
                    Some(&other) => {
                        text.push(other);
                        j += 1;
                    }
                }
            }
            tokens.push((Token::Str(text), start));
            i = j + 1;
            continue;
        }
        if c == '`' {
            let (next, opened) = template(i + 1)?;
            tokens.push((Token::Other, start));
            if opened {
                braces.push(Brace::Template);
                tokens.push((Token::Punct("${"), next - 2));
            }
            i = next;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let mut j = i + 1;
            while let Some(&d) = chars.get(j) {
                let exponent_sign = matches!(d, '+' | '-')
                    && matches!(chars[j - 1], 'e' | 'E')
                    && !chars[i..j].iter().any(|&c| matches!(c, 'x' | 'X'));
                if d.is_ascii_alphanumeric() || d == '_' || d == '.' || exponent_sign {
                    j += 1;
                } else {
                    break;
                }
            }
            tokens.push((Token::Other, start));
            i = j;
            continue;
        }
        if c == '\\' {
            return Err(fail(i, "Names written with \\u escapes can't be checked"));
        }
        if ident_start(c) {
            let mut j = i + 1;
            while chars.get(j).is_some_and(|&c| ident_char(c)) {
                j += 1;
            }
            if chars.get(j) == Some(&'\\') {
                return Err(fail(j, "Names written with \\u escapes can't be checked"));
            }
            let word: String = chars[i..j].iter().collect();
            tokens.push((Token::Name(word), start));
            i = j;
            continue;
        }
        // `?.` is optional chaining only when no digit follows (`a?.5:b` is a conditional).
        let rest: String = chars[i..(i + 4).min(chars.len())].iter().collect();
        let punct = PUNCTUATORS
            .iter()
            .chain(SINGLE_PUNCTUATORS.iter())
            .copied()
            .find(|punct| {
                rest.starts_with(punct)
                    && !(*punct == "?." && chars.get(i + 2).is_some_and(char::is_ascii_digit))
            })
            .ok_or_else(|| fail(i, &format!("Ghostex can't read the character {c:?}")))?;
        match punct {
            "(" => {
                let word =
                    |back: usize| match tokens.len().checked_sub(back).map(|at| &tokens[at].0) {
                        Some(Token::Name(word)) => word.as_str(),
                        _ => "",
                    };
                let head = matches!(word(1), "if" | "while" | "for" | "with")
                    || (word(1) == "await" && word(2) == "for");
                parens.push(head);
            }
            ")" => closed_head = parens.pop().unwrap_or(false),
            "{" => {
                let expression = match tokens.last().map(|(token, _)| token) {
                    None => false,
                    Some(Token::Name(word)) => EXPRESSION_AFTER_WORDS.contains(&word.as_str()),
                    Some(Token::Punct(p)) => !matches!(*p, ")" | "]" | "}" | ";" | "=>"),
                    _ => false,
                };
                braces.push(if expression {
                    Brace::Expression
                } else {
                    Brace::Block
                });
            }
            "}" => match braces.pop() {
                Some(Brace::Template) => {
                    let (next, opened) = template(i + 1)?;
                    tokens.push((Token::Other, start));
                    if opened {
                        braces.push(Brace::Template);
                        tokens.push((Token::Punct("${"), next - 2));
                    }
                    i = next;
                    continue;
                }
                Some(brace) => closed_block = brace == Brace::Block,
                None => closed_block = true,
            },
            _ => {}
        }
        tokens.push((Token::Punct(punct), start));
        i += punct.chars().count();
    }
    Ok((tokens, uncertain))
}

/// A key written as a quoted string, when it has no escapes Ghostex would have to decode.
fn plain_key(text: &str) -> Option<&str> {
    (!text.contains('\\')).then_some(text)
}

/// The static members after `tools` from token `at` on: their segments (a quoted key may hold
/// several, as Executor splits each at dots), and whether a call follows them.
fn member_chain(tokens: &[(Token, usize)], mut at: usize) -> Result<(Vec<String>, bool), ()> {
    let mut segments: Vec<String> = Vec::new();
    loop {
        let token = |index: usize| tokens.get(index).map(|(token, _)| token);
        let dot = matches!(
            token(at),
            Some(Token::Punct(".")) | Some(Token::Punct("?."))
        );
        if dot {
            if let Some(Token::Name(name)) = token(at + 1) {
                segments.push(name.clone());
                at += 2;
                continue;
            }
        }
        let open = if matches!(token(at), Some(Token::Punct("?."))) {
            at + 1
        } else {
            at
        };
        if let (Some(Token::Punct("[")), Some(Token::Str(text)), Some(Token::Punct("]"))) =
            (token(open), token(open + 1), token(open + 2))
        {
            let key = plain_key(text).ok_or(())?;
            segments.extend(key.split('.').map(str::to_string));
            at = open + 3;
            continue;
        }
        if segments.is_empty() {
            return Err(());
        }
        return Ok((segments, matches!(token(at), Some(Token::Punct("(")))));
    }
}

/// Checks the text after a `tools` the scan didn't read as code, as if it were code: it must
/// name an app directly. Returns the app it names.
fn raw_member(chars: &[char], mut at: usize) -> Option<String> {
    while chars.get(at).is_some_and(|&c| is_js_space(c)) {
        at += 1;
    }
    if chars.get(at) == Some(&'?') && chars.get(at + 1) == Some(&'.') {
        at += 2;
    } else if chars.get(at) == Some(&'.') {
        at += 1;
    } else if chars.get(at) != Some(&'[') {
        return None;
    }
    while chars.get(at).is_some_and(|&c| is_js_space(c)) {
        at += 1;
    }
    let key: String = if chars.get(at) == Some(&'[') {
        at += 1;
        while chars.get(at).is_some_and(|&c| is_js_space(c)) {
            at += 1;
        }
        let quote = *chars.get(at).filter(|&&q| q == '"' || q == '\'')?;
        let end = chars[at + 1..].iter().position(|&c| c == quote)? + at + 1;
        let key: String = chars[at + 1..end].iter().collect();
        if key.contains(['\\', '\n', '\r']) {
            return None;
        }
        let mut close = end + 1;
        while chars.get(close).is_some_and(|&c| is_js_space(c)) {
            close += 1;
        }
        if chars.get(close) != Some(&']') {
            return None;
        }
        key
    } else {
        let end = chars[at..]
            .iter()
            .position(|&c| !ident_char(c))
            .map_or(chars.len(), |offset| at + offset);
        if end == at || chars.get(end) == Some(&'\\') {
            return None;
        }
        chars[at..end].iter().collect()
    };
    Some(key.split('.').next().unwrap_or_default().to_string())
}

/// What a program reaches through `tools`, or why Ghostex can't tell.
pub(crate) fn program_reach(code: &str) -> Result<Reach, String> {
    let dynamic = |line: usize| {
        format!(
            "Ghostex runs a program only when it names each app directly, as tools.<app>.<tool>(…) or tools[\"<app>\"]…, and this one reaches tools another way (line {line}). Name the app where you use it."
        )
    };
    let chars: Vec<char> = code.chars().collect();
    let (tokens, uncertain) =
        scan(&chars).map_err(|error| format!("Ghostex can't read this program: {error}."))?;
    let tokens = &tokens;
    let mut reach = Reach::default();
    let add = |reach: &mut Reach, slug: &str, tool: Option<String>, line: usize| {
        if slug == MANAGEMENT_SLUG {
            return Err(MANAGEMENT_REFUSED.to_string());
        }
        if slug == "search" {
            reach.searches = true;
            return Ok(());
        }
        if !is_app_slug(slug) {
            return Err(dynamic(line));
        }
        let entry = (slug.to_string(), tool);
        if !reach.apps.contains(&entry) {
            reach.apps.push(entry);
        }
        Ok(())
    };
    let mut code_tools: Vec<usize> = Vec::new();
    for (index, (token, at)) in tokens.iter().enumerate() {
        let after_dot =
            index > 0 && matches!(tokens[index - 1].0, Token::Punct(".") | Token::Punct("?."));
        if after_dot || *token != Token::Name("tools".to_string()) {
            continue;
        }
        code_tools.push(*at);
        let line = line_of(&chars, *at);
        let (segments, called) = member_chain(tokens, index + 1).map_err(|_| dynamic(line))?;
        let tool = (called && segments.len() > 1).then(|| tool_name(&segments[1..].join(".")));
        add(&mut reach, &segments[0], tool, line)?;
    }
    if !uncertain {
        return Ok(reach);
    }
    // The scan guessed at a `/`, so it may have read code as a comment, string or regex: every
    // other `tools` word there must name an app as code would, and no `\u` escape may spell one.
    if code.contains("\\u") {
        return Err(
            "Ghostex can't check this program: it uses \\u escapes where it can't tell code from text. Write the characters themselves."
                .to_string(),
        );
    }
    let word: Vec<char> = "tools".chars().collect();
    for at in 0..chars.len().saturating_sub(word.len() - 1) {
        if chars[at..at + word.len()] != word[..]
            || (at > 0 && ident_char(chars[at - 1]))
            || chars.get(at + word.len()).is_some_and(|&c| ident_char(c))
            || code_tools.contains(&at)
        {
            continue;
        }
        // A property named `tools` (`x.tools`, never `...tools`) is no way to the binding.
        let mut before = at;
        while before > 0 && matches!(chars[before - 1], ' ' | '\t') {
            before -= 1;
        }
        let member =
            before > 0 && chars[before - 1] == '.' && (before < 2 || chars[before - 2] != '.');
        if member {
            continue;
        }
        let line = line_of(&chars, at);
        let slug = raw_member(&chars, at + word.len()).ok_or_else(|| dynamic(line))?;
        add(&mut reach, &slug, None, line)?;
    }
    Ok(reach)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_paths_read_as_executor_reads_them() {
        let ok = |path: &str| call_path(path).expect(path);
        assert_eq!(
            ok("tools.linear.issues.list"),
            ("linear".into(), "issues.list".into())
        );
        assert_eq!(
            ok(" tools[\"hacker-news\"].top "),
            ("hacker-news".into(), "top".into())
        );
        assert_eq!(
            ok("tools.linear[\"issues\"].delete"),
            ("linear".into(), "issues.delete".into())
        );
        assert_eq!(
            ok("tools[\"linear.issues\"].delete"),
            ("linear".into(), "issues.delete".into())
        );
        assert_eq!(
            ok("linear.issues.list"),
            ("linear".into(), "issues.list".into())
        );
        assert_eq!(
            ok("toolslinear.list"),
            ("toolslinear".into(), "list".into())
        );
        assert_eq!(
            ok("tools.linear[\"%constructor\"]"),
            ("linear".into(), "constructor".into())
        );
        for management in [
            "tools.executor.apps.list",
            "tools[\"executor\"].apps.list",
            "tools.executor[\"apps\"].list",
            "tools[\"executor.apps\"].list",
            "tools[\"\\u0065xecutor\"].apps.list",
            "executor.apps.list",
        ] {
            assert_eq!(
                call_path(management).unwrap_err(),
                MANAGEMENT_REFUSED,
                "{management}"
            );
        }
        for unreadable in [
            "tools",
            "tools.",
            "tools.linear['issues'].delete",
            "tools.linear[issues].delete",
            "tools .linear.list",
            "tools.linear",
            "\u{FEFF}tools.executor.apps.list",
            "Linear.list",
            "search.tools",
            "tools[\"\\ud800\"].x",
        ] {
            assert!(call_path(unreadable).is_err(), "{unreadable}");
        }
    }

    fn apps(code: &str) -> Vec<(String, Option<String>)> {
        program_reach(code).expect(code).apps
    }

    #[test]
    fn programs_name_their_apps_directly() {
        assert_eq!(
            apps("return await tools.gmail.send_email({ to: 'a' });"),
            vec![("gmail".into(), Some("send_email".into()))]
        );
        assert_eq!(
            apps("const l = tools[\"linear\"]; await l.issues.delete({});"),
            vec![("linear".into(), None)]
        );
        assert_eq!(
            apps("const x = await tools.linear?.[\"issues.list\"]();"),
            vec![("linear".into(), Some("issues.list".into()))]
        );
        assert_eq!(
            apps("// use the tools below\nreturn await tools.hn.top();"),
            vec![("hn".into(), Some("top".into()))]
        );
        assert_eq!(
            apps("const s = \"tools\"; // tools.executor\nreturn s;"),
            vec![]
        );
        assert_eq!(
            apps("for await (const x of y) /re/.test(x); return 1;"),
            vec![]
        );
        assert_eq!(apps("const a = x.tools; return a;"), vec![]);
        assert!(
            program_reach("return await tools.search({ query: 'x' })")
                .unwrap()
                .searches
        );
        let template = "return `${await tools.github.issues.list({})}`;";
        assert_eq!(
            apps(template),
            vec![("github".into(), Some("issues.list".into()))]
        );
        assert_eq!(
            apps("if (a) /re/.test(s); return tools.hn.top();"),
            vec![("hn".into(), Some("top".into()))]
        );
    }

    #[test]
    fn programs_that_reach_tools_another_way_are_refused() {
        for code in [
            "const t = tools; await t[\"exec\" + \"utor\"].apps.list();",
            "await tools[\"exec\" + \"utor\"].apps.list();",
            "await tools[name].apps.list();",
            "const { executor } = tools;",
            "f(...tools);",
            "await t\\u006fols.executor.apps.list();",
            "await tools[\"\\u0065xecutor\"].apps.list();",
            "if (a) /re//1; tools[k].x();",
            "x = {} /\"/; tools[k].x(); \"/",
            "await tools /* c */ [k]();",
            "await tools.executor.apps.list();",
            "await tools[\"executor\"].apps.list();",
            "await tools[\"executor.apps\"].list();",
            "x = {} / 2; // tools[\"exec\" + \"utor\"].apps.list()",
            "x = {} / 2; const s = \"t\\u006fols\";",
            "await tools.Executor.apps.list();",
            "/* open",
            "const s = 'open",
        ] {
            assert!(program_reach(code).is_err(), "{code}");
        }
        assert_eq!(
            program_reach("await tools.executor.apps.list()").unwrap_err(),
            MANAGEMENT_REFUSED
        );
    }
}
