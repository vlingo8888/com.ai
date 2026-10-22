use crate::scan::cursor;
use crate::scan::bracket_stack::BracketStack;
use crate::scan::machine::{Machine, MachineState};
use crate::scan::string_machine::StringMachine;

/// Extracts arbitrary values including the brackets.
///
/// E.g.:
///
/// ```text
/// bg-[#0088cc]
///    ^^^^^^^^^
///
/// bg-red-500/[20%]
///            ^^^^^
/// ```
#[derive(Debug, Default)]
pub struct ArbitraryValueMachine {
    /// Track brackets to ensure they are balanced
    bracket_stack: BracketStack,

    /// Only used by the upstream loop kept as a test reference (`simulate`
    /// drives a `StringMachine` directly).
    #[cfg_attr(not(test), allow(dead_code))]
    string_machine: StringMachine,
}

/// Depth of [`BracketStack`].
const MAX_DEPTH: usize = 32;
const NONE: usize = usize::MAX;

/// Not upstream: everything an enclosing arbitrary value needs to know about a
/// nested `[…]` to skip over it without re-reading it.
///
/// The machine is a pure function of `(input, start)`, but every `[` of a value
/// is also the start of the values the outer machines retry from there, and
/// `extract_sub_candidates` re-runs them from every `[` it skipped. On long
/// lines of nested brackets (minified bundles, JSON blobs) that is quadratic.
/// With summaries memoized per line, each byte is read about once, and the
/// result is exactly the one of the upstream loop (see `matches_reference`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Summary {
    done: bool,
    /// Cursor position when the machine returned (the closing `]` when done).
    end: usize,
    /// First `)` / `}` at the value's own level (read as a regular character
    /// here, but a bracket mismatch for an enclosing value).
    stray: usize,
    /// `reach[k]`: position of the first push that made the stack `k` deep.
    reach: [usize; MAX_DEPTH + 1],
}

pub(crate) mod memo {
    use super::Summary;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Memo {
        ptr: usize,
        len: usize,
        summaries: HashMap<usize, Summary>,
        /// Outcome of a scan that is at `pos` with an empty bracket stack
        /// (past its first character): `0` = unknown, else `(end + 1) << 1 | done`.
        outcomes: Vec<u64>,
    }

    thread_local! {
        static MEMO: RefCell<Option<Memo>> = const { RefCell::new(None) };
    }

    /// Memoize summaries for `input` until the returned guard is dropped.
    pub(crate) fn activate(input: &[u8]) -> Guard {
        let prev = MEMO.with(|m| {
            m.borrow_mut().replace(Memo {
                ptr: input.as_ptr() as usize,
                len: input.len(),
                summaries: HashMap::new(),
                outcomes: Vec::new(),
            })
        });
        Guard(prev)
    }

    pub(crate) struct Guard(Option<Memo>);

    impl Drop for Guard {
        fn drop(&mut self) {
            let prev = self.0.take();
            MEMO.with(|m| *m.borrow_mut() = prev);
        }
    }

    fn matches(m: &Memo, input: &[u8]) -> bool {
        m.ptr == input.as_ptr() as usize && m.len == input.len()
    }

    #[inline]
    pub(super) fn is_active(input: &[u8]) -> bool {
        MEMO.with(|m| m.borrow().as_ref().is_some_and(|m| matches(m, input)))
    }

    #[inline]
    pub(super) fn get(input: &[u8], start: usize) -> Option<Summary> {
        MEMO.with(|m| m.borrow().as_ref().filter(|m| matches(m, input))?.summaries.get(&start).copied())
    }

    #[inline]
    pub(super) fn outcome_at(input: &[u8], pos: usize) -> Option<(bool, usize)> {
        MEMO.with(|m| {
            let m = m.borrow();
            let v = *m.as_ref().filter(|m| matches(m, input))?.outcomes.get(pos)?;
            (v != 0).then(|| (v & 1 == 1, (v >> 1) as usize - 1))
        })
    }

    pub(super) fn put_outcomes(input: &[u8], positions: &[usize], (done, end): (bool, usize)) {
        if positions.is_empty() {
            return;
        }
        MEMO.with(|m| {
            if let Some(m) = m.borrow_mut().as_mut().filter(|m| matches(m, input)) {
                if m.outcomes.is_empty() {
                    m.outcomes = vec![0; m.len];
                }
                let v = ((end as u64 + 1) << 1) | done as u64;
                for &pos in positions {
                    m.outcomes[pos] = v;
                }
            }
        })
    }

    #[inline]
    pub(super) fn put(input: &[u8], start: usize, summary: Summary) {
        MEMO.with(|m| {
            if let Some(m) = m.borrow_mut().as_mut().filter(|m| matches(m, input)) {
                m.summaries.insert(start, summary);
            }
        })
    }
}

/// How many nested summaries may be computed recursively before falling back
/// to reading brackets one by one (bounds the call stack on adversarial input).
const RECURSION_BUDGET: u8 = 64;

fn summarize(input: &[u8], start: usize, budget: u8) -> Summary {
    if let Some(summary) = memo::get(input, start) {
        return summary;
    }
    let summary = simulate(input, start, budget);
    memo::put(input, start, summary);
    summary
}

/// The upstream loop (see `scan_reference`), which also records the
/// [`Summary`] fields and, with a budget, skips nested `[…]` via their summary.
fn simulate(input: &[u8], start: usize, budget: u8) -> Summary {
    let len = input.len();
    let mut stack = [0u8; MAX_DEPTH];
    let mut n = 0;
    let mut reach = [NONE; MAX_DEPTH + 1];
    let mut stray = NONE;
    let mut pos = start + 1;

    macro_rules! fail {
        ($pos:expr) => {
            return Summary { done: false, end: $pos, stray, reach }
        };
    }

    while pos < len {
        let byte = input[pos];
        match Class::from(byte) {
            Class::Escape => {
                let next = if pos + 1 < len { input[pos + 1] } else { 0x00 };
                pos += 2;
                // An escaped whitespace character is not allowed
                if Class::from(next) == Class::Whitespace {
                    fail!(pos);
                }
            }

            Class::OpenParen | Class::OpenBracket | Class::OpenCurly => {
                if n >= MAX_DEPTH {
                    fail!(pos);
                }

                // Skip a nested `[…]` (but not `[]`, whose `]` the nested value
                // reads as a regular character).
                if byte == b'[' && budget > 0 && pos + 1 < len && input[pos + 1] != b']' && memo::is_active(input) {
                    let nested = summarize(input, pos, budget - 1);
                    let base = n + 1;
                    if reach[base] == NONE {
                        reach[base] = pos;
                    }

                    // Here, a stray `)`/`}` is a mismatch and pushing past
                    // `MAX_DEPTH` fails, both before the nested value ends.
                    let overflow = nested.reach[MAX_DEPTH + 1 - base];
                    let first = nested.stray.min(overflow);
                    // Depths reached before that still count for the values
                    // enclosing this one (they overflow sooner).
                    for k in 1..=MAX_DEPTH - base {
                        if reach[base + k] == NONE && nested.reach[k] < first {
                            reach[base + k] = nested.reach[k];
                        }
                    }
                    if first < nested.end {
                        fail!(first);
                    }
                    if !nested.done {
                        fail!(nested.end);
                    }

                    // The nested `]` pops our `[`.
                    pos = nested.end + 1;
                    continue;
                }

                stack[n] = match byte {
                    b'(' => b')',
                    b'[' => b']',
                    _ => b'}',
                };
                n += 1;
                if reach[n] == NONE {
                    reach[n] = pos;
                }
                pos += 1;
            }

            Class::CloseParen | Class::CloseBracket | Class::CloseCurly if n > 0 => {
                n -= 1;
                if stack[n] != byte {
                    fail!(pos);
                }
                pos += 1;
            }

            // End of an arbitrary value
            Class::CloseBracket if start + 1 != pos => {
                return Summary { done: true, end: pos, stray, reach };
            }

            // Start of a string
            Class::Quote => {
                let mut cursor = cursor::Cursor { input, pos };
                match StringMachine.next(&mut cursor) {
                    MachineState::Idle => fail!(cursor.pos),
                    MachineState::Done(_) => pos = cursor.pos + 1,
                }
            }

            // Any kind of whitespace is not allowed
            Class::Whitespace => fail!(pos),

            // String interpolation-like syntax is not allowed. E.g.: `[${x}]`
            Class::Dollar if pos + 1 < len && input[pos + 1] == b'{' => fail!(pos),

            // Everything else is valid
            _ => {
                if n == 0 && stray == NONE && matches!(byte, b')' | b'}') {
                    stray = pos;
                }
                pos += 1;
            }
        }
    }

    fail!(pos)
}

/// `(done, end)` of the value starting at `start`, without the [`Summary`]
/// details (what a top-level call needs).
///
/// Once the scan is past its first character with an empty bracket stack, its
/// future depends on the position only. Scans from different starts (e.g. the
/// many `[]` of a minified bundle, whose `]` is read as a regular character so
/// the scan runs on to the next whitespace) converge on such positions, so the
/// outcome is memoized there and later scans stop as soon as they reach one.
fn outcome(input: &[u8], start: usize) -> (bool, usize) {
    if let Some(summary) = memo::get(input, start) {
        return (summary.done, summary.end);
    }

    let len = input.len();
    let mut stack = [0u8; MAX_DEPTH];
    let mut n = 0;
    let mut pos = start + 1;
    let mut visited = Vec::new();

    let result = 'scan: {
        while pos < len {
            if n == 0 && pos > start + 1 {
                if let Some(known) = memo::outcome_at(input, pos) {
                    break 'scan known;
                }
                visited.push(pos);
            }

            let byte = input[pos];
            match Class::from(byte) {
                Class::Escape => {
                    let next = if pos + 1 < len { input[pos + 1] } else { 0x00 };
                    pos += 2;
                    if Class::from(next) == Class::Whitespace {
                        break 'scan (false, pos);
                    }
                }

                Class::OpenParen | Class::OpenBracket | Class::OpenCurly => {
                    if n >= MAX_DEPTH {
                        break 'scan (false, pos);
                    }

                    if byte == b'[' && pos + 1 < len && input[pos + 1] != b']' {
                        let nested = summarize(input, pos, RECURSION_BUDGET);
                        let overflow = nested.reach[MAX_DEPTH - n];
                        let first = nested.stray.min(overflow);
                        if first < nested.end {
                            break 'scan (false, first);
                        }
                        if !nested.done {
                            break 'scan (false, nested.end);
                        }
                        pos = nested.end + 1;
                        continue;
                    }

                    stack[n] = match byte {
                        b'(' => b')',
                        b'[' => b']',
                        _ => b'}',
                    };
                    n += 1;
                    pos += 1;
                }

                Class::CloseParen | Class::CloseBracket | Class::CloseCurly if n > 0 => {
                    n -= 1;
                    if stack[n] != byte {
                        break 'scan (false, pos);
                    }
                    pos += 1;
                }

                Class::CloseBracket if start + 1 != pos => break 'scan (true, pos),

                Class::Quote => {
                    let mut cursor = cursor::Cursor { input, pos };
                    match StringMachine.next(&mut cursor) {
                        MachineState::Idle => break 'scan (false, cursor.pos),
                        MachineState::Done(_) => pos = cursor.pos + 1,
                    }
                }

                Class::Whitespace => break 'scan (false, pos),

                Class::Dollar if pos + 1 < len && input[pos + 1] == b'{' => break 'scan (false, pos),

                _ => pos += 1,
            }
        }

        (false, pos)
    };

    memo::put_outcomes(input, &visited, result);
    result
}

impl Machine for ArbitraryValueMachine {
    #[inline(always)]
    fn reset(&mut self) {
        self.bracket_stack.reset();
    }

    #[inline]
    fn next(&mut self, cursor: &mut cursor::Cursor<'_>) -> MachineState {
        // An arbitrary value must start with an open bracket
        if Class::OpenBracket != cursor.curr().into() {
            return MachineState::Idle;
        }

        let start_pos = cursor.pos;
        let (done, end) = if memo::is_active(cursor.input) {
            outcome(cursor.input, start_pos)
        } else {
            let summary = simulate(cursor.input, start_pos, 0);
            (summary.done, summary.end)
        };

        cursor.pos = end;
        if done {
            self.done(start_pos, cursor)
        } else {
            self.restart()
        }
    }
}

#[cfg(test)]
impl ArbitraryValueMachine {
    /// The upstream implementation of `next`, kept to check [`simulate`].
    fn scan_reference(&mut self, cursor: &mut cursor::Cursor<'_>) -> MachineState {
        if Class::OpenBracket != cursor.curr().into() {
            return MachineState::Idle;
        }

        let start_pos = cursor.pos;
        cursor.advance();

        let len = cursor.input.len();

        while cursor.pos < len {
            match cursor.curr().into() {
                Class::Escape => match cursor.next().into() {
                    Class::Whitespace => {
                        cursor.advance_twice();
                        return self.restart();
                    }
                    _ => cursor.advance_twice(),
                },

                Class::OpenParen | Class::OpenBracket | Class::OpenCurly => {
                    if !self.bracket_stack.push(cursor.curr()) {
                        return self.restart();
                    }
                    cursor.advance();
                }

                Class::CloseParen | Class::CloseBracket | Class::CloseCurly
                    if !self.bracket_stack.is_empty() =>
                {
                    if !self.bracket_stack.pop(cursor.curr()) {
                        return self.restart();
                    }
                    cursor.advance();
                }

                Class::CloseBracket
                    if start_pos + 1 != cursor.pos && self.bracket_stack.is_empty() =>
                {
                    return self.done(start_pos, cursor);
                }

                Class::Quote => match self.string_machine.next(cursor) {
                    MachineState::Idle => return self.restart(),
                    MachineState::Done(_) => cursor.advance(),
                },

                Class::Whitespace => return self.restart(),

                Class::Dollar if matches!(cursor.next().into(), Class::OpenCurly) => {
                    return self.restart()
                }

                _ => cursor.advance(),
            };
        }

        self.restart()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Escape,

    OpenParen,

    CloseParen,

    OpenBracket,

    CloseBracket,

    OpenCurly,

    CloseCurly,

    Quote,

    Whitespace,

    Dollar,

    Other,
}
impl Class {
    pub const TABLE: [Class; 256] = {
        let mut t = [Class::Other; 256];
        t[b'\\' as usize] = Class::Escape;
        t[b'(' as usize] = Class::OpenParen;
        t[b')' as usize] = Class::CloseParen;
        t[b'[' as usize] = Class::OpenBracket;
        t[b']' as usize] = Class::CloseBracket;
        t[b'{' as usize] = Class::OpenCurly;
        t[b'}' as usize] = Class::CloseCurly;
        t[b'"' as usize] = Class::Quote;
        t[b'\'' as usize] = Class::Quote;
        t[b'`' as usize] = Class::Quote;
        t[b' ' as usize] = Class::Whitespace;
        t[b'\t' as usize] = Class::Whitespace;
        t[b'\n' as usize] = Class::Whitespace;
        t[b'\r' as usize] = Class::Whitespace;
        t[b'\x0C' as usize] = Class::Whitespace;
        t[b'$' as usize] = Class::Dollar;
        t
    };
}

impl From<u8> for Class {
    #[inline(always)]
    fn from(byte: u8) -> Self { Class::TABLE[byte as usize] }
}

impl From<&u8> for Class {
    #[inline(always)]
    fn from(byte: &u8) -> Self { Class::TABLE[*byte as usize] }
}

#[cfg(test)]
mod tests {
    use super::ArbitraryValueMachine;
    use crate::scan::machine::{Machine, MachineState};

    /// Every `[` of `input`, through the memoized machine (one memo for the
    /// whole input, like `Extractor::extract`) and through the upstream loop.
    fn assert_matches_reference(input: &[u8]) {
        let _memo = super::memo::activate(input);
        for start in (0..input.len()).filter(|&i| input[i] == b'[') {
            let mut a = crate::scan::cursor::Cursor::new(input);
            a.pos = start;
            let mut b = a;
            let sa = ArbitraryValueMachine::default().next(&mut a);
            let sb = ArbitraryValueMachine::default().scan_reference(&mut b);
            let span = |s: MachineState| match s {
                MachineState::Done(span) => Some((span.start, span.end)),
                MachineState::Idle => None,
            };
            assert_eq!(
                (span(sa), a.pos),
                (span(sb), b.pos),
                "start {start} of {:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn matches_reference() {
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        const ALPHABET: &[u8] = b"[[[[]]]]((){}}a-:_'\"`\\ $#";
        for _ in 0..20_000 {
            let len = (rnd() % 60) as usize;
            let input: Vec<u8> = (0..len).map(|_| ALPHABET[(rnd() % ALPHABET.len() as u64) as usize]).collect();
            assert_matches_reference(&input);
        }
        // Long, bracket-heavy inputs that nest around the stack depth
        const DEEP: &[u8] = b"[[[[[[[[[[((((({{a]]]]]]]))))}}'\\";
        for _ in 0..3_000 {
            let len = 50 + (rnd() % 400) as usize;
            let input: Vec<u8> = (0..len).map(|_| DEEP[(rnd() % DEEP.len() as u64) as usize]).collect();
            assert_matches_reference(&input);
        }
        // Nesting around and past the bracket stack depth
        for depth in [30, 31, 32, 33, 34, 40, 80, 200] {
            for inner in ["a", ")", "}", "(", "[]", " ", "'x'", "\\ ", "${"] {
                let input = format!("{}{inner}{}", "[".repeat(depth), "]".repeat(depth));
                assert_matches_reference(input.as_bytes());
                let input = format!("{}{inner}{}", "[a(".repeat(depth), ")]".repeat(depth));
                assert_matches_reference(input.as_bytes());
            }
        }
        // JSON-like nesting
        let json = r#"{"tree":[{"a":[{"b":[1,2,[3,[4]]],"c":"x]y"},{"d":[]}]},[[[["e"]]]]],"f":["(",")","{"]}"#;
        assert_matches_reference(json.as_bytes());
        assert_matches_reference(json.repeat(50).as_bytes());
    }

    #[test]
    #[ignore]
    fn test_arbitrary_value_machine_performance() {
        let input = r#"<div class="[color:red] [[data-foo]] [url('https://tailwindcss.com')] [url(https://tailwindcss.com)]"></div>"#.repeat(100);

        ArbitraryValueMachine::test_throughput(100_000, &input);
        ArbitraryValueMachine::test_duration_once(&input);

        todo!()
    }

    #[test]
    fn test_arbitrary_value_machine_extraction() {
        for (input, expected) in [
            // Simple variable
            ("[#0088cc]", vec!["[#0088cc]"]),
            // With parentheses
            (
                "[url(https://tailwindcss.com)]",
                vec!["[url(https://tailwindcss.com)]"],
            ),
            // With strings, where bracket balancing doesn't matter
            ("['[({])}']", vec!["['[({])}']"]),
            // With strings later in the input
            (
                "[url('https://tailwindcss.com?[{]}')]",
                vec!["[url('https://tailwindcss.com?[{]}')]"],
            ),
            // With nested brackets
            ("[[data-foo]]", vec!["[[data-foo]]"]),
            (
                "[&>[data-slot=icon]:last-child]",
                vec!["[&>[data-slot=icon]:last-child]"],
            ),
            // With data types
            ("[length:32rem]", vec!["[length:32rem]"]),
            // Spaces are not allowed
            ("[ #0088cc ]", vec![]),
            // Unbalanced brackets are not allowed
            ("[foo[bar]", vec![]),
            // Empty brackets are not allowed
            ("[]", vec![]),
        ] {
            assert_eq!(ArbitraryValueMachine::test_extract_all(input), expected);
        }
    }

    #[test]
    fn test_exceptions() {
        for (input, expected) in [
            // JS string interpolation
            ("[${x}]", vec![]),
            ("[url(${x})]", vec![]),
            // Allowed in strings
            ("[url('${x}')]", vec!["[url('${x}')]"]),
        ] {
            assert_eq!(ArbitraryValueMachine::test_extract_all(input), expected);
        }
    }
}
