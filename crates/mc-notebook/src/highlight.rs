//! Syntax colouring of input cells (design 3.11): the classes of `mc_syntax::colour` in the roles of design 8.2,
//! plus the runs of aliases being typed (the marker and the name after it, design 3.9).
use crate::theme::Palette;
use eframe::egui::Color32;
use mc_syntax::TokenClass;
use std::ops::Range;

/// The look of a class: colour, italic, bold (drawn as the bold face), background.
pub fn style(class: TokenClass, p: &Palette) -> (Color32, bool, bool, Option<Color32>) {
    use TokenClass::*;
    match class {
        Operator | Global => (p.text, false, false, None),
        Undefined => (p.blue, false, false, None),
        Local => (p.cyan, false, false, None),
        PatternVar => (p.cyan, true, false, None),
        Shadowing => (p.dark_red, false, false, None),
        Keyword => (p.purple, false, false, None),
        Constant | Number => (p.orange, false, false, None),
        String => (p.green, false, false, None),
        Comment => (p.weak, true, false, None),
        Placeholder => (p.faint, false, false, None),
        SyntaxError => (p.red, false, false, Some(p.over_bg(p.red, 0.12))),
        UnmatchedBracket => (p.dark_red, false, true, None),
    }
}

/// The runs of `src` that are aliases being typed: the marker and the name after it.
pub fn alias_runs(src: &str) -> Vec<Range<usize>> {
    let mark = mc_nbmodel::ALIAS_MARK;
    let mut out = vec![];
    for (i, _) in src.match_indices(mark) {
        let len: usize =
            src[i + mark.len_utf8()..].chars().take_while(|c| !c.is_whitespace()).map(char::len_utf8).sum();
        out.push(i..i + mark.len_utf8() + len);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases() {
        assert_eq!(alias_runs("x ⋮al y ⋮"), [2..7, 10..13]);
        assert!(alias_runs("x → α²").is_empty());
    }

    #[test]
    fn every_class_has_a_look() {
        let p = Palette::light();
        assert_eq!(style(TokenClass::Global, &p).0, p.text);
        assert!(style(TokenClass::Comment, &p).1);
        assert!(style(TokenClass::SyntaxError, &p).3.is_some());
    }
}
