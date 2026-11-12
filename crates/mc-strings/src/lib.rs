//! MathCAS v2 string library (roadmap item 1C): natives of the evaluator for splitting, replacing, slicing,
//! searching, changing case, taking strings apart into characters and back, padding, trimming, and reading a
//! string as source text.
//!
//! [`install`] registers every function of [`FUNCTIONS`] with a kernel. `string_join`, `string_length`,
//! `to_string` and `split` belong to `mc-lists` and stay there.
//!
//! - **Characters (decision S.1).** Lengths and positions count Unicode scalar values, never bytes: `string_take`,
//!   `string_reverse`, `characters` and `string_pad` never split a character. They do not segment grapheme
//!   clusters (a letter with a combining accent is two characters).
//! - **Arguments (decision S.2).** A symbol or compound term where a string is expected leaves the call
//!   unevaluated; any other atom gives `failure(argument, f, k)` (argument k of f), and a position outside the
//!   string `failure(index, i, n)` (as `mc-lists` does for lists). A list where a function's first argument, a
//!   string, is expected maps the function over it (`to_upper(["a", "b"])` is `["A", "B"]`); `from_characters`
//!   takes its list whole.
//! - **Search strings (decision S.3)** are literal; several are tried in the order given at each position, the
//!   leftmost occurrence wins and occurrences do not overlap.
//! - **Reading (decision S.4):** `to_expression` reads with `mc-syntax` and leaves the evaluation to the caller;
//!   names are not resolved through modules (see the `read` module).
//!
//! Every native charges fuel for the bytes it scans or builds (a scan polls the interrupt every 64 KB); results
//! larger than 2^30 bytes are `failure(limit, memory, call)`.
#![warn(missing_docs)]
mod ctx;
mod pattern;
mod read;
mod regex;
mod scan;
mod search;
mod strpat;
mod shape;

use ctx::Lib;
use mc_core::{Heap, Store, Sym, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use std::sync::Arc;

/// The implementation of one function.
type Body = for<'a, 'b, 'c, 'h> fn(&Lib, &mut NativeCx<'a, 'b, 'h>, T<'h>, &'c [T<'h>]) -> Res<Reply<'h>>;

/// A native of the library: its body, its arity range and the shared state (symbols only, so no tracing).
struct Fun {
    lib: Arc<Lib>,
    body: Body,
    sym: Sym,
    min: usize,
    max: usize,
    /// a list first argument maps the function over its elements
    maps: bool,
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if args.len() < self.min || args.len() > self.max {
            return Ok(Reply::Decline);
        }
        let h = cx.heap();
        if self.maps {
            if let Some(xs) = self.lib.as_list(h, args[0]) {
                cx.tick(xs.len() as u64 + 1)?;
                let mut parts = args.to_vec();
                let items: Vec<T<'h>> = xs
                    .iter()
                    .map(|&x| {
                        parts[0] = x;
                        h.call(self.sym, &parts)
                    })
                    .collect();
                return Ok(Reply::Again(h.call(self.lib.s.list, &items)));
            }
        }
        (self.body)(&self.lib, cx, t, args)
    }
}

macro_rules! functions {
    ($($name:literal => $body:path, $min:literal, $max:literal, $maps:literal, $desc:literal;)*) => {
        /// Every function of the library, with a one-line description.
        pub const FUNCTIONS: &[(&str, &str)] = &[$(($name, $desc),)*];
        const BODIES: &[(&str, Body, usize, usize, bool)] = &[$(($name, $body, $min, $max, $maps),)*];
    };
}

functions! {
    "string_split" => search::string_split, 1, 4, true,
        "string_split(s): the words of s; string_split(s, sep[, n]): the pieces between seps (strings or a string pattern); string_split(s, p -> v): v between the pieces";
    "string_replace" => search::string_replace, 2, 4, true,
        "string_replace(s, p -> r), (s, [rules][, n]), (s, a, b): replacement of strings or string patterns left to right, at most n times";
    "string_cases" => scan::string_cases, 2, 4, true,
        "string_cases(s, p): the substrings of s matching the string pattern p; (s, p -> r): r for each match; (s, spec, n): at most n; overlaps:, ignore_case:";
    "string_position" => scan::string_position, 2, 4, true,
        "string_position(s, p): the [start, end] positions of the matches of p in s (overlapping by default); (s, p, n): at most n";
    "string_count" => scan::string_count, 2, 3, true,
        "string_count(s, p): the number of matches of the string pattern p in s, without overlaps";
    "string_match_q" => scan::string_match_q, 2, 3, true,
        "string_match_q(s, p): whether the whole of s matches the string pattern p (in a literal string, * and @ are wildcards)";
    "string_free_q" => scan::string_free_q, 2, 3, true,
        "string_free_q(s, p): whether no substring of s matches the string pattern p";
    "string_delete" => string_delete, 2, 4, true,
        "string_delete(s, p): s without the substrings matching the string pattern p";
    "string_take" => shape::string_take, 2, 2, true,
        "string_take(s, k): the first k characters (last -k); string_take(s, [i, j]): characters i to j; [i]: one";
    "string_drop" => shape::string_drop, 2, 2, true,
        "string_drop(s, k), string_drop(s, [i, j]): s without the characters string_take selects";
    "string_contains" => search::string_contains, 2, 3, true,
        "string_contains(s, sub): whether s contains sub (one of a list of strings, or a match of a string pattern)";
    "string_starts" => search::string_starts, 2, 3, true,
        "string_starts(s, prefix): whether s starts with prefix (one of a list of strings, or a match of a string pattern)";
    "string_ends" => search::string_ends, 2, 3, true,
        "string_ends(s, suffix): whether s ends with suffix (one of a list of strings, or a match of a string pattern)";
    "to_upper" => shape::to_upper, 1, 1, true,
        "to_upper(s): s in upper case (Unicode case mapping)";
    "to_lower" => shape::to_lower, 1, 1, true,
        "to_lower(s): s in lower case (Unicode case mapping)";
    "characters" => shape::characters, 1, 1, true,
        "characters(s): the list of the characters of s, as one-character strings";
    "from_characters" => shape::from_characters, 1, 2, false,
        "from_characters([c, ...]): the string of a list of one-character strings or code points (a code point alone: its character; lists of lists: a list of strings); from_characters(bytes, \"UTF8\"): the string of UTF-8 bytes";
    "string_reverse" => shape::string_reverse, 1, 1, true,
        "string_reverse(s): the characters of s in reverse order";
    "string_pad" => shape::string_pad, 2, 4, true,
        "string_pad(s, n[, pad], side: right|left|center): s padded to n characters with pad (default a space)";
    "string_trim" => shape::string_trim, 1, 2, true,
        "string_trim(s): s without surrounding white space; string_trim(s, sub): without repeated sub at both ends";
    "string_expression" => pattern::string_expression, 0, 100000, false,
        "string_expression(p1, p2, ...): the strings and patterns joined in sequence; literal strings give one string (\"a\" ~~ \"b\" is \"ab\"), a pattern keeps it symbolic";
    "longest" => pattern::marker, 1, 2, false,
        "longest(p): the pattern p with its runs and repetitions taking as much as possible first";
    "shortest" => pattern::marker, 1, 2, false,
        "shortest(p): the pattern p with its runs and repetitions taking as little as possible first";
    "regular_expression" => pattern::marker, 1, 1, false,
        "regular_expression(\"re\"): a string pattern written as a regular expression (groups $1, $2, ... in a replacement string)";
    "digit_character" => pattern::marker, 0, 0, false,
        "digit_character: a string pattern for one digit 0-9";
    "letter_character" => pattern::marker, 0, 0, false,
        "letter_character: a string pattern for one letter";
    "whitespace_character" => pattern::marker, 0, 0, false,
        "whitespace_character: a string pattern for one white-space character";
    "word_character" => pattern::marker, 0, 0, false,
        "word_character: a string pattern for one letter or digit";
    "punctuation_character" => pattern::marker, 0, 0, false,
        "punctuation_character: a string pattern for one punctuation character";
    "hexadecimal_character" => pattern::marker, 0, 0, false,
        "hexadecimal_character: a string pattern for one hexadecimal digit";
    "whitespace" => pattern::marker, 0, 0, false,
        "whitespace: a string pattern for a run of white space";
    "number_string" => pattern::marker, 0, 0, false,
        "number_string: a string pattern for the text of a decimal number (-12, 3.25, .5)";
    "start_of_string" => pattern::marker, 0, 0, false,
        "start_of_string: a string pattern for the start of the string";
    "end_of_string" => pattern::marker, 0, 0, false,
        "end_of_string: a string pattern for the end of the string";
    "start_of_line" => pattern::marker, 0, 0, false,
        "start_of_line: a string pattern for the start of a line";
    "end_of_line" => pattern::marker, 0, 0, false,
        "end_of_line: a string pattern for the end of a line";
    "word_boundary" => pattern::marker, 0, 0, false,
        "word_boundary: a string pattern for a boundary between a word character and another";
    "to_expression" => read::to_expression, 1, 3, true,
        "to_expression(s): the string read as MathCAS source and evaluated (names not resolved through modules); to_expression(s, input_form, f): the term read wrapped in f before it is evaluated";
}

/// `string_delete(s, p)`: every match of `p` removed.
fn string_delete<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    scan::replace(lib, cx, t, args, true)
}

/// Register every function of [`FUNCTIONS`] with `k`, all strict (decision S.2).
pub fn install(k: &mut Kernel, h: &Heap) {
    let lib = Arc::new(Lib::new(h));
    for &(name, body, min, max, maps) in BODIES {
        let sym = h.sym(name);
        k.register(sym, Attributes::default(), Box::new(Fun { lib: lib.clone(), body, sym, min, max, maps }));
    }
}
