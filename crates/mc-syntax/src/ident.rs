//! Identifier, keyword and string-escape rules, shared by the lexer and the printer so that what the printer writes
//! is exactly what the lexer reads back (04-syntax.md section 2).

/// The reserved words of the language.
pub const KEYWORDS: &[&str] = &[
    "let", "var", "fn", "rule", "sym", "module", "pub", "import", "if", "then", "else", "while", "for", "in", "match",
    "when", "held", "and", "or", "not", "true", "false", "null",
];

/// Whether `name` is a reserved word.
pub fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

/// A character that may start an identifier (Unicode letters and `_`).
pub fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// A character that may continue an identifier.
pub fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Unicode aliases (04 section 4): a whole identifier run equal to the alias reads as the target name.
pub fn alias(run: &str) -> Option<&'static str> {
    match run {
        "π" => Some("pi"),
        "ⅇ" => Some("e"),
        "ⅈ" => Some("im"),
        _ => None,
    }
}

/// Unicode characters that read exactly as an ASCII operator or keyword (04 section 2): each character on the left
/// is one token, the same token as the text on the right. `√` is the prefix square root (`√x` is `sqrt(x)`); the
/// letters `π`, `ⅇ`, `ⅈ` and the symbol `∞` are names (see [`alias`] and [`symbol_alias`]).
pub const UNICODE_OPERATORS: &[(char, &str)] = &[
    ('→', "->"),
    ('⧴', ":>"),
    ('≤', "<="),
    ('≥', ">="),
    ('≠', "!="),
    ('⩵', "=="),
    ('≡', "==="),
    ('≢', "!=="),
    ('×', "*"),
    ('·', "*"),
    ('⋅', "*"),
    ('÷', "/"),
    ('−', "-"),
    ('∧', "and"),
    ('∨', "or"),
    ('¬', "not"),
    ('∈', "in"),
    ('√', "sqrt"),
];

/// The ASCII spelling of a Unicode operator character ([`UNICODE_OPERATORS`]).
pub fn unicode_operator(c: char) -> Option<&'static str> {
    if c.is_ascii() {
        return None;
    }
    UNICODE_OPERATORS.iter().find(|e| e.0 == c).map(|e| e.1)
}

/// The placeholder of a template slot (`□`, U+25A1). It is not valid syntax: input containing one reads as an error
/// at the first placeholder (08-notebook.md section 4.3).
pub const PLACEHOLDER: char = '□';

/// Characters that are not identifier characters but read as a name on their own.
pub fn symbol_alias(c: char) -> Option<&'static str> {
    match c {
        '∞' => Some("inf"),
        _ => None,
    }
}

/// Whether `seg` reads back as one plain identifier segment: identifier characters, not starting with a digit, not
/// ending in `_` (that would be a pattern), not a keyword and not an alias.
fn is_plain_segment(seg: &str) -> bool {
    let mut chars = seg.chars();
    match chars.next() {
        Some(c) if is_ident_start(c) => {}
        _ => return false,
    }
    seg.chars().all(is_ident_continue) && !seg.ends_with('_') && !is_keyword(seg) && alias(seg).is_none()
}

/// Whether `name` is written as a plain identifier: one segment, or several joined by `.` (a qualified name `M.f`).
pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_plain_segment)
}

/// Whether `name` can be written in front of `_`, `__` or `___` as a named pattern (one unqualified segment).
pub fn is_pattern_name(name: &str) -> bool {
    !name.contains('.') && is_plain_segment(name)
}

/// The token text that reads back as an identifier with exactly this name: the name itself when it is plain, or the
/// name in backquotes (decision C.4) otherwise.
pub fn ident_token(name: &str, out: &mut String) {
    if is_plain_name(name) {
        out.push_str(name);
    } else {
        out.push('`');
        for c in name.chars() {
            match c {
                '`' => out.push_str("\\`"),
                '\\' => out.push_str("\\\\"),
                _ => push_escaped_char(c, out),
            }
        }
        out.push('`');
    }
}

/// Appends `s` as a string literal with escapes.
pub fn string_literal(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => push_escaped_char(c, out),
        }
    }
    out.push('"');
}

/// Appends `s` as the literal the printer writes: raw (`r"..."`, with as many `#` as its quotes need) when it is TeX
/// (it holds a `$` and a backslash) and has no control character, so `$\alpha$` prints as `r"$\alpha$"` rather than
/// with a doubled backslash; otherwise [`string_literal`] with escapes. Other strings with backslashes (file paths)
/// keep their escaped form, which programs reading printed values (`image("...")`) expect.
pub fn string_token(s: &str, out: &mut String) {
    if !s.contains('\\') || !s.contains('$') || s.chars().any(char::is_control) {
        return string_literal(s, out);
    }
    // the fewest `#` whose closing `"#..#` does not occur in the text
    let mut close = String::from("\"");
    while s.contains(&close) {
        close.push('#');
    }
    let hashes = &close[1..];
    out.push('r');
    out.push_str(hashes);
    out.push('"');
    out.push_str(s);
    out.push_str(&close);
}

/// Control characters are written as escapes; everything else as itself.
fn push_escaped_char(c: char, out: &mut String) {
    match c {
        '\n' => out.push_str("\\n"),
        '\t' => out.push_str("\\t"),
        c if c.is_control() => {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        }
        c => out.push(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names() {
        assert!(is_plain_name("x"));
        assert!(is_plain_name("θ1"));
        assert!(is_plain_name("_x"));
        assert!(is_plain_name("Geo.area"));
        assert!(!is_plain_name("x_"));
        assert!(!is_plain_name("1x"));
        assert!(!is_plain_name("if"));
        assert!(!is_plain_name("π"));
        assert!(!is_plain_name("ⅇ"));
        assert!(!is_plain_name("a b"));
        assert!(!is_plain_name("M."));
        assert!(!is_plain_name(""));
    }

    #[test]
    fn quoting() {
        let mut s = String::new();
        ident_token("if", &mut s);
        ident_token("a`b", &mut s);
        string_literal("a\"\n\u{1}", &mut s);
        assert_eq!(s, "`if``a\\`b`\"a\\\"\\n\\u{1}\"");
    }

    #[test]
    fn tex_strings_print_raw() {
        let token = |s: &str| {
            let mut o = String::new();
            string_token(s, &mut o);
            o
        };
        assert_eq!(token("plain \"q\""), r#""plain \"q\"""#);
        assert_eq!(token(r"$\alpha_1$"), r#"r"$\alpha_1$""#);
        assert_eq!(token(r#"$\a "b""#), r##"r#"$\a "b""#"##);
        assert_eq!(token(r##"$\ "#"##), r###"r##"$\ "#"##"###);
        // a control character, or no `$` (a path), keeps the escaped form
        assert_eq!(token("$\\\n"), r#""$\\\n""#);
        assert_eq!(token(r"C:\temp\a.png"), r#""C:\\temp\\a.png""#);
    }
}
