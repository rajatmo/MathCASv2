//! Extend selection (08-notebook.md section 3.4): the design's vectors on `f(aa + b*c, d)` and more.
use mc_syntax::extend_selection;

/// The selected text after one press from a caret at the `|` of `marked`.
fn first(marked: &str) -> Option<String> {
    let at = marked.find('|').expect("a caret mark");
    let src = marked.replacen('|', "", 1);
    extend_selection(&src, at, at).map(|r| src[r].to_string())
}

/// The selections after successive presses from `start..end`, up to the whole content.
fn chain(src: &str, start: usize, end: usize) -> Vec<String> {
    let mut out = Vec::new();
    let (mut s, mut e) = (start, end);
    while let Some(r) = extend_selection(src, s, e) {
        out.push(src[r.clone()].to_string());
        (s, e) = (r.start, r.end);
        assert!(out.len() < 100, "no progress: {out:?}");
    }
    out
}

#[test]
fn design_vectors() {
    let cases = [
        ("|f(aa + b*c, d)", "f"),
        ("f|(aa + b*c, d)", "f(aa + b*c, d)"),
        ("f(a|a + b*c, d)", "aa"),
        ("f(aa| + b*c, d)", "aa"),
        ("f(aa +| b*c, d)", "aa + b*c"),
        ("f(aa + b|*c, d)", "b*c"),
        ("f(aa + b*c|, d)", "b*c"),
        ("f(aa + b*c,| d)", "aa + b*c, d"),
    ];
    let bad: Vec<String> = cases
        .iter()
        .filter_map(|&(m, want)| {
            let got = first(m);
            (got.as_deref() != Some(want)).then(|| format!("{m:?}: want {want:?}, got {got:?}"))
        })
        .collect();
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));

    let src = "f(aa + b*c, d)";
    assert_eq!(chain(src, 7, 10), ["aa + b*c", "aa + b*c, d", "f(aa + b*c, d)"]);
}

#[test]
fn units() {
    let src = "x = {\n  let y = (a + b)*c  # note\n  y^2\n}";
    let a = src.find("a +").unwrap_or(0);
    assert_eq!(
        chain(src, a, a + 1),
        ["a + b", "(a + b)", "(a + b)*c", "let y = (a + b)*c", "let y = (a + b)*c  # note\n  y^2", "{\n  let y = (a + b)*c  # note\n  y^2\n}", src]
    );
    // a statement that does not parse still has its tokens and brackets
    let src = "g(u, [v, w] +";
    let v = src.find('v').unwrap_or(0);
    assert_eq!(chain(src, v, v), ["v", "v, w", "[v, w]", "u, [v, w] +", "g(u, [v, w] +"]);
    // empty text, caret in white space
    assert_eq!(extend_selection("", 0, 0), None);
    assert_eq!(extend_selection("a +  b", 4, 4), Some(0..6));
}
