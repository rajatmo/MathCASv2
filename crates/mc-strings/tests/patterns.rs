//! String patterns (round 30): blanks and runs, classes, repetition, alternatives, `longest`/`shortest`, bindings
//! and conditions, anchors, regular expressions, and the functions that search with them.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

fn check(cases: &[(&str, &str)]) {
    let mut e = engine();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn greedy_and_lazy() {
    check(&[
        (r#"string_cases("abcdefg", string_expression(x__, __) -> x)"#, r#"["abcdef"]"#),
        (r#"string_cases("abcdefg", string_expression(shortest(x__), __) -> x)"#, r#"["a"]"#),
        (r#"string_cases("abcdefg", string_expression(x__, longest(__)) :> x)"#, r#"["abcdef"]"#),
        // an exception, as written for except(c): one character not matching c (c held, then evaluated)
        (r#"string_replace("the cat in the hat", (t_ when not match_q(t, characters("aeiou"))) -> "")"#, r#""eaiea""#),
        (r#"string_match_q(["ain't", "test", "A&M", "anybody"], string_expression(___, (t_ when not match_q(t, word_character)), ___))"#, "[true, false, true, false]"),
        (r#"string_replace("1a2b3c4", repeated(t @ letter_character when not match_q(t, "a")) -> "")"#, r#""1a234""#),
        (r#"string_cases("0111011110", longest(repeated("1")))"#, r#"["111", "1111"]"#),
        (r#"string_cases("0111011110", shortest(repeated("1")))"#, r#"["1", "1", "1", "1", "1", "1", "1"]"#),
        (r#"string_replace("011011110", longest(repeated("1")) :> "x")"#, r#""0x0x0""#),
        (r#"string_replace("011110A010", shortest(string_expression("0", __, "0")) :> "x")"#, r#""xAx""#),
        (r#"string_cases("hello", repeated(_, [2]))"#, r#"["he", "ll"]"#),
    ]);
}

#[test]
fn blanks_classes_and_bindings() {
    check(&[
        (r#"string_replace("abc abcb abdc", string_expression("ab", _) -> "X")"#, r#""X Xb Xc""#),
        (r#"string_replace(["ab", "abc", "abcd"], string_expression("b", __) -> "X")"#, r#"["ab", "aX", "aX"]"#),
        (r#"string_replace(["ab", "abc", "abcd"], string_expression("b", ___) -> "X")"#, r#"["aX", "aX", "aX"]"#),
        (r#"string_cases("abbcbccaabbabccaa", string_expression(x_, x_))"#, r#"["bb", "cc", "aa", "bb", "cc", "aa"]"#),
        (r#"string_cases("a10b6a77a3a#", string_expression("a", repeated(digit_character)))"#, r#"["a10", "a77", "a3"]"#),
        (r#"string_cases("a1b22c333", x @ repeated(digit_character) :> string_length(x))"#, "[1, 2, 3]"),
        (r#"string_cases("abc", x_ when x != "b")"#, r#"["a", "c"]"#),
        (r#"string_match_q("abc", x__ when string_length(x) == 3)"#, "true"),
        (r#"string_cases("the cat 12 sat -3.5 on", number_string)"#, r#"["12", "-3.5"]"#),
        (r#"string_cases("one two  three", whitespace)"#, r#"[" ", "  "]"#),
        (r#"string_cases("abcabc", alternatives("bc", "a"))"#, r#"["a", "bc", "a", "bc"]"#),
        (r#"string_cases("The Cat", "the", ignore_case: true)"#, r#"["The"]"#),
        (r#"string_cases(["ab1", "c22"], repeated(digit_character))"#, r#"[["1"], ["22"]]"#),
    ]);
}

#[test]
fn anchors_and_regular_expressions() {
    check(&[
        (r#"string_replace("aa abcd", string_expression(word_boundary, "aa", word_boundary) -> "XX")"#, r#""XX abcd""#),
        ("string_cases(\"line1\\nline2\", string_expression(start_of_line, letter_character))", r#"["l", "l"]"#),
        (r#"string_replace("hello world", regular_expression("(\\w+) (\\w+)") -> "$2 $1")"#, r#""world hello""#),
        (r#"string_cases("x1 y22 z333", regular_expression("[a-z]\\d+"))"#, r#"["x1", "y22", "z333"]"#),
        (r#"string_cases("x1 y22 z333", regular_expression("[a-z]\\d+?"))"#, r#"["x1", "y2", "z3"]"#),
        (r#"string_cases("aXbXXc", regular_expression("[^X]+"))"#, r#"["a", "b", "c"]"#),
        (r#"string_match_q("2024-11-05", regular_expression("\\d{4}-\\d{2}-\\d{2}"))"#, "true"),
        (r#"string_split("a, b,c", regular_expression(",\\s*"))"#, r#"["a", "b", "c"]"#),
        // look-around is not supported: the call stays unevaluated
        (r#"string_cases("ab", regular_expression("a(?=b)"))"#, r#"string_cases("ab", regular_expression("a(?=b)"))"#),
    ]);
}

#[test]
fn searching_functions() {
    check(&[
        (r#"string_match_q("apppbb", string_expression("a", ___, "b"))"#, "true"),
        (r#"string_match_q("apple", "a*e")"#, "true"),
        (r#"string_match_q("Apple", "a*", ignore_case: true)"#, "true"),
        (r#"string_match_q("apple", "a@")"#, "true"),
        (r#"string_free_q("bcde", string_expression("c", __, "t"))"#, "true"),
        (r#"string_free_q("abc", "b")"#, "false"),
        (r#"string_count("abcadcadcbaac", string_expression("a", _, "c"))"#, "4"),
        (r#"string_count("aaaa", "aa")"#, "2"),
        (r#"string_position("abxxabxxabxxabxxab", string_expression(_, "ab", _))"#, "[[4, 7], [8, 11], [12, 15]]"),
        (r#"string_position("aaaa", "aa")"#, "[[1, 2], [2, 3], [3, 4]]"),
        (r#"string_position("aaaa", "aa", overlaps: false)"#, "[[1, 2], [3, 4]]"),
        (r#"string_cases("aaaa", "aa", overlaps: true)"#, r#"["aa", "aa", "aa"]"#),
        (r#"string_contains("hello", repeated(digit_character))"#, "false"),
        (r#"string_starts("hello", letter_character)"#, "true"),
        (r#"string_ends("hello5", digit_character)"#, "true"),
        (r#"string_ends("hello", digit_character)"#, "false"),
        (r#"string_delete("a1b2c3", digit_character)"#, r#""abc""#),
        (r#"string_replace("abc", [letter_character -> "L"], 2)"#, r#""LLc""#),
        (r#"string_replace("a1b2", digit_character :> f(1))"#, r#"string_expression("a", f(1), "b", f(1))"#),
        (r#"string_split("a1b22c333d", repeated(digit_character))"#, r#"["a", "b", "c", "d"]"#),
        (r#"string_split("a1b2", digit_character -> "D")"#, r#"["a", "D", "b", "D", ""]"#),
        (r#"string_cases(s, "a")"#, r#"string_cases(s, "a")"#),
        (r#"string_cases("abc", undefined_thing(1))"#, r#"string_cases("abc", undefined_thing(1))"#),
    ]);
}

#[test]
fn long_texts_stay_bounded() {
    // a repetition of one character does not nest; a long run of a compound repetition is a failure, not a crash
    check(&[
        (r#"string_length(first(string_cases(string_pad("", 200000, "x"), repeated("x"))))"#, "200000"),
        (r#"string_cases(string_pad("", 20000, "x"), repeated("xx"))"#, "failure(limit, string_pattern)"),
        (r#"string_count(string_pad("", 100000, "x"), "x")"#, "100000"),
    ]);
}
