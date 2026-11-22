//! fzf-style optimal alignment matcher: a Smith-Waterman variant with affine
//! gap penalties plus bonuses for word boundaries, camelCase humps, and
//! unbroken runs. Derived from zehn's `src/fuzzy.zig`; this is now the shared
//! matcher for the TUI and Find GUI, including Ghostex's stricter quality gate
//! and literal-match preference.

pub const MAX_POSITIONS: usize = 32;

#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub score: i32,
    /// Up to 32 highlighted byte positions in the haystack.
    pub positions: [u16; MAX_POSITIONS],
    pub pos_len: u8,
}

impl Match {
    fn empty(score: i32) -> Self {
        Self {
            score,
            positions: [0; MAX_POSITIONS],
            pos_len: 0,
        }
    }

    pub fn highlights(&self) -> &[u16] {
        &self.positions[..self.pos_len as usize]
    }
}

// fzf-style scoring constants.
const SCORE_MATCH: i32 = 16;
const GAP_START: i32 = -3;
const GAP_EXT: i32 = -1;
const BONUS_BOUNDARY: i32 = SCORE_MATCH / 2; // 8
const BONUS_CAMEL: i32 = BONUS_BOUNDARY - 1; // 7
const BONUS_CONSECUTIVE: i32 = -(GAP_START + GAP_EXT); // 4
const FIRST_CHAR_MULT: i32 = 2;

const NEG: i32 = i32::MIN / 2;

// CDXC:PromptSearch 2026-06-16-18:16:
// Search filtering must stop showing sessions just because the query is a loose
// subsequence. Non-empty queries need both a minimum score percentage against an
// ideal match and compact per-term spans, so unrelated prompts with scattered
// matching bytes are rejected before ranking.
const MIN_SCORE_QUALITY_PERCENT: i32 = 60;
const MIN_TERM_COMPACTNESS_PERCENT: usize = 50;
const MIN_COMPACT_TERM_LEN: usize = 3;

/// Above this much DP work, fall back to the greedy scorer to stay fast on
/// pathologically large haystacks.
const DP_BUDGET: usize = 200_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    White,
    Delim,
    NonWord,
    Lower,
    Upper,
    Number,
}

fn class_of(c: u8) -> Class {
    match c {
        b' ' | b'\t' | b'\n' | b'\r' => Class::White,
        b'/' | b'\\' | b'_' | b'-' | b'.' | b',' | b':' | b';' | b'(' | b')' | b'[' | b']'
        | b'{' | b'}' => Class::Delim,
        b'0'..=b'9' => Class::Number,
        b'a'..=b'z' => Class::Lower,
        b'A'..=b'Z' => Class::Upper,
        _ => Class::NonWord,
    }
}

#[inline]
fn lower(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}

#[inline]
fn eqi(a: u8, b: u8) -> bool {
    lower(a) == lower(b)
}

/// Bonus for a (word) haystack char given the preceding char's class.
fn bonus_at(prev: Class, cur: Class) -> i32 {
    if matches!(cur, Class::White | Class::Delim | Class::NonWord) {
        return 0;
    }
    match prev {
        Class::White | Class::Delim | Class::NonWord => BONUS_BOUNDARY,
        Class::Lower => {
            if cur == Class::Upper {
                BONUS_CAMEL
            } else {
                0
            }
        }
        Class::Number => {
            if cur != Class::Number {
                BONUS_CAMEL
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Reusable matcher; holds scratch buffers so per-query matching avoids
/// repeated allocation.
#[derive(Default)]
pub struct Matcher {
    mm: Vec<i32>,
    par: Vec<i32>,
    b: Vec<i32>,
    r_best: Vec<i32>,
    r_arg: Vec<i32>,
}

impl Matcher {
    pub fn new() -> Self {
        Self::default()
    }

    fn ensure(&mut self, cells: usize, win: usize) {
        if self.mm.len() < cells {
            self.mm.resize(cells, 0);
            self.par.resize(cells, 0);
        }
        if self.b.len() < win {
            self.b.resize(win, 0);
            self.r_best.resize(win, 0);
            self.r_arg.resize(win, 0);
        }
    }

    // CDXC:PromptSearch 2026-08-07-09:08:
    // A literal query occurrence must always remain searchable, regardless of
    // earlier characters that could form a lower-quality fuzzy subsequence.
    // Prefer the literal span whenever one exists so the visible reason for the
    // result is cohesive and deterministic. Only use fuzzy alignment when no
    // literal word or phrase occurs, and keep that path behind the quality gate.
    pub fn matches(&mut self, needle: &[u8], hay: &[u8]) -> Option<Match> {
        if let Some(m) = exact_substring_match(needle, hay) {
            return Some(m);
        }
        let raw = self.raw_match(needle, hay);
        quality_filter(needle, raw)
    }

    fn raw_match(&mut self, needle: &[u8], hay: &[u8]) -> Option<Match> {
        if needle.is_empty() {
            return Some(Match::empty(0));
        }
        if hay.is_empty() {
            return None;
        }

        // Window = first occurrence of needle[0] .. last occurrence of the final
        // needle char. Anything outside cannot be part of a match.
        let sidx = hay.iter().position(|&hc| eqi(hc, needle[0]))?;

        let last = needle[needle.len() - 1];
        let mut eidx: usize = 0;
        let mut i = hay.len();
        while i > sidx {
            if eqi(hay[i - 1], last) {
                eidx = i;
                break;
            }
            i -= 1;
        }
        if eidx <= sidx {
            return None;
        }

        let win = &hay[sidx..eidx];
        let m = needle.len();
        let n = win.len();
        if n < m {
            return None;
        }

        if m * n > DP_BUDGET {
            return greedy(needle, hay, sidx);
        }
        self.ensure(m * n, n);
        self.dp(needle, hay, win, sidx)
    }

    /// Affine-gap optimal alignment. `mm[i][j]` = best score for aligning
    /// `needle[0..=i]` with `needle[i]` matched exactly at window column `j`.
    fn dp(&mut self, needle: &[u8], hay: &[u8], win: &[u8], base: usize) -> Option<Match> {
        let m = needle.len();
        let n = win.len();

        for j in 0..n {
            let prev_char = if base + j == 0 {
                b' '
            } else {
                hay[base + j - 1]
            };
            self.b[j] = bonus_at(class_of(prev_char), class_of(win[j]));
        }

        // row 0: first needle char, with leading-skip penalty j*GAP_EXT
        for j in 0..n {
            if eqi(win[j], needle[0]) {
                self.mm[j] = SCORE_MATCH + self.b[j] * FIRST_CHAR_MULT + (j as i32) * GAP_EXT;
            } else {
                self.mm[j] = NEG;
            }
            self.par[j] = -1;
        }

        for i in 1..m {
            let row = i * n;
            let prow = (i - 1) * n;

            // prefix-max R[x] = max_{c<=x} (mm[prow+c] - GAP_EXT*c), with argmax
            let mut run_best: i32 = NEG;
            let mut run_arg: i32 = -1;
            for c in 0..n {
                let v = self.mm[prow + c];
                if v > NEG {
                    let adj = v - (c as i32) * GAP_EXT;
                    if adj > run_best {
                        run_best = adj;
                        run_arg = c as i32;
                    }
                }
                self.r_best[c] = run_best;
                self.r_arg[c] = run_arg;
            }

            for j in 0..n {
                let idx = row + j;
                if !eqi(win[j], needle[i]) {
                    self.mm[idx] = NEG;
                    self.par[idx] = -1;
                    continue;
                }
                // consecutive: predecessor matched at j-1
                let mut v_con: i32 = NEG;
                if j >= 1 && self.mm[prow + j - 1] > NEG {
                    v_con = self.mm[prow + j - 1] + SCORE_MATCH + self.b[j].max(BONUS_CONSECUTIVE);
                }
                // gap: predecessor matched at c <= j-2
                let mut v_non: i32 = NEG;
                let mut non_par: i32 = -1;
                if j >= 2 && self.r_best[j - 2] > NEG {
                    let pred = GAP_START + ((j - 2) as i32) * GAP_EXT + self.r_best[j - 2];
                    v_non = pred + SCORE_MATCH + self.b[j];
                    non_par = self.r_arg[j - 2];
                }
                if v_con >= v_non {
                    self.mm[idx] = v_con;
                    self.par[idx] = if v_con > NEG { (j - 1) as i32 } else { -1 };
                } else {
                    self.mm[idx] = v_non;
                    self.par[idx] = non_par;
                }
            }
        }

        // best end cell in last row
        let mut best: i32 = NEG;
        let mut best_j: usize = 0;
        let last_row = (m - 1) * n;
        for j in 0..n {
            if self.mm[last_row + j] > best {
                best = self.mm[last_row + j];
                best_j = j;
            }
        }
        if best <= NEG {
            return None;
        }

        let mut result = Match::empty(best);
        let mut tmp = [0u16; 64];
        let mut cnt: usize = 0;
        let mut i: i32 = (m - 1) as i32;
        let mut j: i32 = best_j as i32;
        while i >= 0 && j >= 0 {
            if cnt < tmp.len() {
                tmp[cnt] = (base + j as usize) as u16;
                cnt += 1;
            }
            let p = self.par[(i as usize) * n + (j as usize)];
            i -= 1;
            j = p;
        }
        let take = cnt.min(MAX_POSITIONS);
        for k in 0..take {
            result.positions[k] = tmp[cnt - 1 - k];
        }
        result.pos_len = take as u8;
        Some(result)
    }
}

fn exact_substring_match(needle: &[u8], hay: &[u8]) -> Option<Match> {
    if needle.is_empty() {
        return Some(Match::empty(0));
    }
    if needle.len() > hay.len() {
        return None;
    }

    let mut best: Option<Match> = None;
    let mut start: usize = 0;
    while start + needle.len() <= hay.len() {
        let matched = needle
            .iter()
            .enumerate()
            .all(|(offset, &nc)| eqi(nc, hay[start + offset]));
        if matched {
            let previous = if start == 0 {
                Class::White
            } else {
                class_of(hay[start - 1])
            };
            let boundary_bonus = bonus_at(previous, class_of(hay[start]));
            let boundary_penalty = (BONUS_BOUNDARY - boundary_bonus).max(0) * FIRST_CHAR_MULT;
            let mut result = Match::empty(ideal_score(needle.len()) - boundary_penalty);
            if start + needle.len() - 1 <= u16::MAX as usize {
                let highlighted = needle.len().min(MAX_POSITIONS);
                for offset in 0..highlighted {
                    result.positions[offset] = (start + offset) as u16;
                }
                result.pos_len = highlighted as u8;
            }
            if best.is_none_or(|current| result.score > current.score) {
                best = Some(result);
            }
        }
        start += 1;
    }
    best
}

/// Greedy fallback for oversized haystacks. Agrees with the DP on *whether* a
/// match exists; score/positions may be suboptimal.
fn greedy(needle: &[u8], hay: &[u8], start: usize) -> Option<Match> {
    let mut m = Match::empty(0);
    let mut ni: usize = 0;
    let mut consecutive: i32 = 0;
    let mut prev = Class::White;
    let mut hi = start;
    while hi < hay.len() && ni < needle.len() {
        let hc = hay[hi];
        if eqi(hc, needle[ni]) {
            let mut s = SCORE_MATCH;
            s += bonus_at(prev, class_of(hc));
            if consecutive > 0 {
                s += BONUS_CONSECUTIVE;
            }
            m.score += s;
            consecutive += 1;
            if (m.pos_len as usize) < MAX_POSITIONS {
                m.positions[m.pos_len as usize] = hi as u16;
                m.pos_len += 1;
            }
            ni += 1;
        } else {
            consecutive = 0;
        }
        prev = class_of(hc);
        hi += 1;
    }
    if ni < needle.len() {
        return None;
    }
    Some(m)
}

fn quality_filter(needle: &[u8], maybe_match: Option<Match>) -> Option<Match> {
    let m = maybe_match?;
    if needle.is_empty() {
        return Some(m);
    }
    if match_quality_percent(needle, &m) < MIN_SCORE_QUALITY_PERCENT {
        return None;
    }
    if !terms_are_compact_enough(needle, &m) {
        return None;
    }
    Some(m)
}

pub fn match_quality_percent(needle: &[u8], m: &Match) -> i32 {
    let ideal = ideal_score(needle.len());
    if ideal <= 0 {
        return 100;
    }
    if m.score <= 0 {
        return 0;
    }
    let pct = (m.score as i64) * 100 / (ideal as i64);
    pct.min(100) as i32
}

fn ideal_score(needle_len: usize) -> i32 {
    if needle_len == 0 {
        return 0;
    }
    let n = needle_len as i32;
    SCORE_MATCH * n + BONUS_BOUNDARY * FIRST_CHAR_MULT + BONUS_BOUNDARY * (n - 1)
}

fn terms_are_compact_enough(needle: &[u8], m: &Match) -> bool {
    let pos_len = m.pos_len as usize;
    if pos_len < needle.len() {
        return true;
    }

    let mut ni: usize = 0;
    let mut pi: usize = 0;
    while ni < needle.len() && pi < pos_len {
        while ni < needle.len() && is_query_separator(needle[ni]) {
            ni += 1;
            pi += 1;
        }
        if ni >= needle.len() || pi >= pos_len {
            break;
        }

        let term_pos_start = pi;
        let mut term_len: usize = 0;
        while ni < needle.len() && !is_query_separator(needle[ni]) && pi < pos_len {
            ni += 1;
            pi += 1;
            term_len += 1;
        }
        if term_len < MIN_COMPACT_TERM_LEN {
            continue;
        }

        let first = m.positions[term_pos_start];
        let last = m.positions[pi - 1];
        let span = last.saturating_sub(first) as usize + 1;
        let compactness = term_len * 100 / span;
        if compactness < MIN_TERM_COMPACTNESS_PERCENT {
            return false;
        }
    }

    true
}

fn is_query_separator(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// Convenience wrapper for one-off matching (allocates scratch each call).
pub fn match_once(needle: &[u8], hay: &[u8]) -> Option<Match> {
    Matcher::new().matches(needle, hay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needle_matches_with_score_zero() {
        assert_eq!(match_once(b"", b"anything").unwrap().score, 0);
    }

    #[test]
    fn literal_substring_survives_an_earlier_scattered_fuzzy_path() {
        let hay: &[u8] = b"please make the bg hover effect for sessions more prominent (now it's very dark so not clear i'm hovering) should be lighter color\n\nalso i want a 1 px border around the currently active session, and also if i collapse the project/group/section that this session is in then we put this border around the collapsed header that contains the active session";
        let r = match_once(b"border", hay).expect("literal match");
        let start = hay
            .windows(6)
            .position(|w| w == b"border")
            .expect("needle present");
        assert_eq!(r.score, ideal_score(6));
        assert_eq!(r.pos_len, 6);
        for offset in 0..6usize {
            assert_eq!(r.positions[offset] as usize, start + offset);
        }
    }

    #[test]
    fn scattered_subsequence_is_rejected_by_the_quality_gate() {
        // "abc" appears only as three far-apart letters — not a real match.
        let hay: &[u8] = b"a lot of words between here and the b, then much later a trailing c";
        assert!(match_once(b"abc", hay).is_none());
    }

    #[test]
    fn word_boundary_beats_mid_word() {
        let boundary = match_once(b"fix", b"please fix this").unwrap().score;
        let mid = match_once(b"fix", b"prefixing things").unwrap().score;
        assert!(
            boundary > mid,
            "boundary {boundary} should beat mid-word {mid}"
        );
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(match_once(b"REFACTOR", b"please refactor the parser").is_some());
    }
}
