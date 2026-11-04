//! The string functions on the assembled engine: normal cases, Unicode (characters, not bytes), lists mapping,
//! failures, `to_expression`, large inputs and limits.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut e = engine();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = eval(&mut e, src);
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn splitting() {
    check(&[
        (r#"string_split("a b  c")"#, r#"["a", "b", "c"]"#),
        (r#"string_split("  tab\tand\nnew line ")"#, r#"["tab", "and", "new", "line"]"#),
        (r#"string_split("")"#, "[]"),
        (r#"string_split("a,,b", ",")"#, r#"["a", "", "b"]"#),
        (r#"string_split(",a,", ",")"#, r#"["", "a", ""]"#),
        (r#"string_split("a, b, c", ", ")"#, r#"["a", "b", "c"]"#),
        (r#"string_split("a,b;c", [",", ";"])"#, r#"["a", "b", "c"]"#),
        (r#"string_split("a,b,c", ",", 2)"#, r#"["a", "b,c"]"#),
        (r#"string_split("a,b,c", ",", 1)"#, r#"["a,b,c"]"#),
        (r#"string_split("αβγβδ", "β")"#, r#"["α", "γ", "δ"]"#),
        (r#"string_split("x", "")"#, "failure(argument, string_split, 2)"),
        (r#"string_split("a,b", ",", 0)"#, "failure(argument, string_split, 3)"),
        (r#"string_split("a,b", 3)"#, "failure(argument, string_split, 2)"),
        (r#"string_split("a,b", [",", x])"#, "failure(argument, string_split, 2)"),
        (r#"string_split(s, ",")"#, r#"string_split(s, ",")"#),
        (r#"string_split(["a b", "c"])"#, r#"[["a", "b"], ["c"]]"#),
    ]);
}

#[test]
fn replacing() {
    check(&[
        (r#"string_replace("abcabc", "b" -> "X")"#, r#""aXcaXc""#),
        (r#"string_replace("abc", ["a" -> "b", "b" -> "a"])"#, r#""bac""#),
        (r#"string_replace("aaa", "aa" -> "b")"#, r#""ba""#),
        (r#"string_replace("abab", ["ab" -> "1", "a" -> "2"])"#, r#""11""#),
        (r#"string_replace("abab", ["a" -> "2", "ab" -> "1"])"#, r#""2b2b""#),
        (r#"string_replace("aaa", "a" -> "b", 2)"#, r#""bba""#),
        (r#"string_replace("aaa", "a" -> "b", 0)"#, r#""aaa""#),
        (r#"string_replace("héllo wörld", "ö", "o")"#, r#""héllo world""#),
        (r#"string_replace("abc", "z" -> "y")"#, r#""abc""#),
        (r#"string_replace("abc", "b" -> "")"#, r#""ac""#),
        (r#"string_replace("abc", "b" :> "B")"#, r#""aBc""#),
        (r#"string_replace(["ab", "bb"], "b" -> "c")"#, r#"["ac", "cc"]"#),
        (r#"string_replace("abc", "" -> "x")"#, "failure(argument, string_replace, 2)"),
        (r#"string_replace("abc", "a" -> 1)"#, "failure(argument, string_replace, 2)"),
        (r#"string_replace("abc", "a" -> "b", -1)"#, "failure(argument, string_replace, 3)"),
        (r#"string_replace("abc", r)"#, r#"string_replace("abc", r)"#),
    ]);
}

#[test]
fn taking_and_dropping() {
    check(&[
        (r#"string_take("héllo", 2)"#, r#""hé""#),
        (r#"string_take("héllo", -2)"#, r#""lo""#),
        (r#"string_take("héllo", 0)"#, r#""""#),
        (r#"string_take("héllo", 5)"#, r#""héllo""#),
        (r#"string_take("héllo", [2, 3])"#, r#""él""#),
        (r#"string_take("héllo", [-3, -1])"#, r#""llo""#),
        (r#"string_take("héllo", [2])"#, r#""é""#),
        (r#"string_take("héllo", [-1])"#, r#""o""#),
        (r#"string_take("héllo", [2, 1])"#, r#""""#),
        (r#"string_take("😀a😀", [2, 3])"#, r#""a😀""#),
        (r#"string_take("abc", 5)"#, "failure(index, 5, 3)"),
        (r#"string_take("abc", -4)"#, "failure(index, -4, 3)"),
        (r#"string_take("abc", [0, 1])"#, "failure(index, 0, 3)"),
        (r#"string_take("abc", [2, 5])"#, "failure(index, 5, 3)"),
        (r#"string_take("abc", [3, 1])"#, "failure(index, 1, 3)"),
        (r#"string_take("abc", [1, 2, 3])"#, "failure(argument, string_take, 2)"),
        (r#"string_take("abc", 1.5)"#, "failure(argument, string_take, 2)"),
        (r#"string_take("abc", k)"#, r#"string_take("abc", k)"#),
        (r#"string_take(["abc", "de"], 1)"#, r#"["a", "d"]"#),
        (r#"string_drop("héllo", 2)"#, r#""llo""#),
        (r#"string_drop("héllo", -1)"#, r#""héll""#),
        (r#"string_drop("héllo", [2, 3])"#, r#""hlo""#),
        (r#"string_drop("héllo", [5])"#, r#""héll""#),
        (r#"string_drop("abc", 4)"#, "failure(index, 4, 3)"),
    ]);
}

#[test]
fn searching() {
    check(&[
        (r#"string_contains("hello", "ell")"#, "true"),
        (r#"string_contains("hello", "elo")"#, "false"),
        (r#"string_contains("hello", "")"#, "true"),
        (r#"string_contains("hello", ["x", "ll"])"#, "true"),
        (r#"string_contains(["ab", "cd"], "c")"#, "[false, true]"),
        (r#"string_contains("naïve", "ï")"#, "true"),
        (r#"string_starts("hello", "he")"#, "true"),
        (r#"string_starts("hello", "lo")"#, "false"),
        (r#"string_ends("hello", ["x", "lo"])"#, "true"),
        (r#"string_ends("hello", "he")"#, "false"),
        (r#"string_contains("hello", 3)"#, "failure(argument, string_contains, 2)"),
        (r#"string_starts(3, "a")"#, "failure(argument, string_starts, 1)"),
        (r#"string_ends(s, "a")"#, r#"string_ends(s, "a")"#),
    ]);
}

#[test]
fn case_characters_reversal() {
    check(&[
        (r#"to_upper("straße")"#, r#""STRASSE""#),
        (r#"to_upper("abc xyz 1")"#, r#""ABC XYZ 1""#),
        (r#"to_lower("ΣΑΣ")"#, r#""σας""#),
        (r#"to_lower("ÀÉÎ")"#, r#""àéî""#),
        (r#"to_upper(["a", "b"])"#, r#"["A", "B"]"#),
        (r#"to_upper(3)"#, "failure(argument, to_upper, 1)"),
        (r#"characters("añ😀")"#, r#"["a", "ñ", "😀"]"#),
        (r#"characters("")"#, "[]"),
        (r#"from_characters(["a", "ñ", "😀"])"#, r#""añ😀""#),
        (r#"from_characters([])"#, r#""""#),
        (r#"from_characters(characters("añ😀 b"))"#, r#""añ😀 b""#),
        (r#"from_characters(["ab"])"#, "failure(argument, from_characters, 1)"),
        (r#"from_characters(["a", 1])"#, "failure(argument, from_characters, 1)"),
        (r#"from_characters("a")"#, "failure(argument, from_characters, 1)"),
        (r#"from_characters(cs)"#, "from_characters(cs)"),
        (r#"string_reverse("añ😀")"#, r#""😀ña""#),
        (r#"string_reverse("")"#, r#""""#),
        (r#"string_length(string_reverse("añ😀"))"#, "3"),
    ]);
}

#[test]
fn padding_and_trimming() {
    check(&[
        (r#"string_pad("ab", 5)"#, r#""ab   ""#),
        (r#"string_pad("ab", 5, "*", side: left)"#, r#""***ab""#),
        (r#"string_pad("ab", 7, "-=", side: center)"#, r#""-=ab-=-""#),
        (r#"string_pad("ab", 5, side: right)"#, r#""ab   ""#),
        (r#"string_pad("ñ", 3, "😀")"#, r#""ñ😀😀""#),
        (r#"string_pad("abc", 2)"#, r#""abc""#),
        (r#"string_pad(["a", "bb"], 3, ".")"#, r#"["a..", "bb."]"#),
        (r#"string_pad("ab", -1)"#, "failure(argument, string_pad, 2)"),
        (r#"string_pad("ab", 5, "")"#, "failure(argument, string_pad, 3)"),
        (r#"string_pad("ab", 5, side: up)"#, "failure(argument, string_pad, 3)"),
        (r#"string_pad("ab", 5, width: 3)"#, "failure(unknown_option, width)"),
        (r#"string_pad("ab", 10^12)"#, "failure(limit, memory, string_pad(\"ab\", 1000000000000))"),
        (r#"string_trim("  hi \n")"#, r#""hi""#),
        (r#"string_trim("hi")"#, r#""hi""#),
        (r#"string_trim("\u{3000}hi\u{2003}")"#, r#""hi""#),
        (r#"string_trim("xxhixx", "x")"#, r#""hi""#),
        (r#"string_trim("ababcab", "ab")"#, r#""c""#),
        (r#"string_trim("abc", "")"#, "failure(argument, string_trim, 2)"),
        (r#"string_trim(["  a", "b  "])"#, r#"["a", "b"]"#),
    ]);
}

#[test]
fn reading_source_text() {
    check(&[
        (r#"to_expression("1 + 2")"#, "3"),
        (r#"to_expression("x^2 + x^2")"#, "2*x^2"),
        (r#"to_expression("[1, 2/4, \"s\"]")"#, r#"[1, 1/2, "s"]"#),
        (r#"to_expression("let a = 2; a*3")"#, "6"),
        (r#"to_expression("sin(pi/6)")"#, "1/2"),
        (r#"to_expression("")"#, "null"),
        (r#"to_expression("1 +")"#, r#"failure(syntax, "1:4: expected an expression, found end of input")"#),
        (r#"to_expression(to_string(x^2 + 1/3))"#, "1/3 + x^2"),
        (r#"to_expression(["1", "2 + 2"])"#, "[1, 4]"),
        (r#"to_expression(3)"#, "failure(argument, to_expression, 1)"),
        // a definition read at run time takes effect, and its names are the bare symbols
        (r#"to_expression("fn sq(x) = x*x")"#, "null"),
        ("sq(7)", "49"),
    ]);
}

/// Long strings: every scan charges fuel by its bytes and finishes; a result too large is a failure, not an
/// allocation; the engine stays usable.
#[test]
fn long_strings() {
    let mut e = engine();
    eval(&mut e, r#"s = string_join(map(k => "ab,", range(100000)))"#);
    assert_eq!(eval(&mut e, "string_length(s)"), "300000");
    assert_eq!(eval(&mut e, r#"length(string_split(s, ","))"#), "100001");
    assert_eq!(eval(&mut e, r#"length(string_split(s, [";", ","]))"#), "100001");
    assert_eq!(eval(&mut e, r#"string_length(string_replace(s, "," -> "😀"))"#), "300000");
    assert_eq!(eval(&mut e, r#"string_length(string_replace(s, ["x" -> "", "," -> ""]))"#), "200000");
    assert_eq!(eval(&mut e, "length(characters(s))"), "300000");
    assert_eq!(eval(&mut e, "string_take(string_reverse(s), 4)"), r#"",ba,""#);
    assert_eq!(eval(&mut e, r#"string_ends(to_upper(s), "AB,")"#), "true");
    assert_eq!(eval(&mut e, "string_length(string_pad(s, 400000))"), "400000");
    assert_eq!(
        eval(&mut e, r#"string_pad("ab", 2^40, "😀")"#),
        "failure(limit, memory, string_pad(\"ab\", 1099511627776, \"😀\"))"
    );
    assert_eq!(eval(&mut e, "1 + 1"), "2");
}

/// Every function's examples with 1 and 4 threads and under collection stress give the same results.
#[test]
fn the_same_at_1_and_4_threads_under_stress() {
    let cases = [
        r#"string_split("a b c d e f", " ")"#,
        r#"map(k => string_take("abcdef", k), range(6))"#,
        r#"map(k => string_pad(to_string(k), 4, "0", side: left), range(20))"#,
        r#"string_replace(string_join(map(to_string, range(200))), ["1" -> "one", "2" -> "two"])"#,
        r#"map(c => to_upper(c), characters("añ😀ß"))"#,
        r#"to_expression("map(k => k^2, range(10))")"#,
    ];
    let run = |threads: usize, heap: mc_core::Config| -> Vec<String> {
        let mut e = CoreEngine::with_config(EngineConfig { threads, heap, ..EngineConfig::default() });
        cases.iter().map(|c| eval(&mut e, c)).collect()
    };
    let reference = run(1, mc_core::Config::default());
    assert_eq!(run(4, mc_core::Config::stress()), reference);
    assert_eq!(run(1, mc_core::Config::stress()), reference);
}
