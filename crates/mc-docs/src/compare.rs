//! How [`crate::runner::check`] compares a stored output with a fresh run: the same lines, each with the same text,
//! except that machine real numbers may differ in their last digits.
//!
//! The pages are written on one system and checked on every system the program is built for. Elementary functions
//! of machine reals come from each system's mathematics library, which may round the last bit differently (a root
//! found as `2.1972245773362196` on one system is `2.197224577336219` on another), and an error estimate or a
//! fingerprint of sampled data then differs too. So a real (a number written with a decimal point or an exponent)
//! matches a real within a relative [`REAL_TOLERANCE`], and a fingerprint (a quoted string of 16 hexadecimal digits,
//! as `interpolating_function` shows) matches any fingerprint; integers and all other text must be equal.
use mc_nbfile::OutputLine;

/// How far apart (relative to the larger magnitude) two machine reals may be and still match.
pub const REAL_TOLERANCE: f64 = 1e-9;

/// Whether a fresh run's lines `fresh` match the stored lines `stored`.
pub fn same_output(stored: &[OutputLine], fresh: &[OutputLine]) -> bool {
    stored.len() == fresh.len()
        && stored.iter().zip(fresh).all(|(a, b)| a.kind == b.kind && (a.text == b.text || same_text(&a.text, &b.text)))
}

/// A piece of an output line.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    /// a number written with a decimal point or an exponent
    Real(f64),
    /// a quoted string of 16 hexadecimal digits
    Fingerprint,
    /// anything else, compared exactly
    Text(&'a str),
}

/// Whether two lines are equal up to the last digits of their reals and their fingerprints.
fn same_text(a: &str, b: &str) -> bool {
    let (ta, tb) = (tokens(a), tokens(b));
    ta.len() == tb.len()
        && ta.iter().zip(&tb).all(|(x, y)| match (x, y) {
            (Token::Real(x), Token::Real(y)) => {
                x == y || (x - y).abs() <= REAL_TOLERANCE * x.abs().max(y.abs())
            }
            _ => x == y,
        })
}

/// The tokens of a line: quoted strings, numbers (starting at a digit that does not continue a name), other text.
fn tokens(s: &str) -> Vec<Token<'_>> {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut text_start = 0;
    let mut i = 0;
    fn flush<'a>(out: &mut Vec<Token<'a>>, s: &'a str, from: usize, to: usize) {
        if to > from {
            out.push(Token::Text(&s[from..to]));
        }
    }
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            // a string, with its escapes
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(b.len());
            let inner = &s[i + 1..j.min(b.len())];
            if inner.len() == 16 && inner.bytes().all(|c| c.is_ascii_hexdigit()) {
                flush(&mut out, s, text_start, i);
                out.push(Token::Fingerprint);
                text_start = end;
            }
            i = end;
            continue;
        }
        let continues_name = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if c.is_ascii_digit() && !continues_name {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let mut real = false;
            if j + 1 < b.len() && b[j] == b'.' && b[j + 1].is_ascii_digit() {
                real = true;
                j += 1;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if j < b.len() && b[j] == b'e' {
                let mut k = j + 1;
                if k < b.len() && (b[k] == b'-' || b[k] == b'+') {
                    k += 1;
                }
                if k < b.len() && b[k].is_ascii_digit() {
                    real = true;
                    while k < b.len() && b[k].is_ascii_digit() {
                        k += 1;
                    }
                    j = k;
                }
            }
            match s[i..j].parse::<f64>() {
                Ok(v) if real => {
                    flush(&mut out, s, text_start, i);
                    out.push(Token::Real(v));
                    text_start = j;
                }
                // integers and other digits stay text
                _ => {}
            }
            i = j;
            continue;
        }
        i += 1;
    }
    flush(&mut out, s, text_start, b.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_nbfile::OutputKind;

    fn value(t: &str) -> OutputLine {
        OutputLine { kind: OutputKind::Value, text: t.into() }
    }

    #[test]
    fn reals_match_up_to_their_last_digits() {
        assert!(same_text("[t -> 2.1972245773362196]", "[t -> 2.197224577336219]"));
        assert!(same_text(
            "failure(not_converged, n_integrate, 0.5040670573734137, 1.6766555641862712e-6)",
            "failure(not_converged, n_integrate, 0.5040670573734137, 1.6766555641821802e-6)"
        ));
        assert!(same_text("x -> -1.0e-300", "x -> -1.0000000000001e-300"));
        assert!(!same_text("[t -> 2.19]", "[t -> 2.20]"));
        assert!(!same_text("1.5e-6", "1.5e-7"));
        // integers, names with digits and the rest of the text are exact
        assert!(!same_text("x^10", "x^11"));
        assert!(!same_text("x1 + 2.0", "x2 + 2.0"));
        assert!(!same_text("[1.0, 2.0]", "[1.0, 2.0, 3.0]"));
        assert!(!same_text("1.0 + x", "1.0 + y"));
    }

    #[test]
    fn fingerprints_match_any_fingerprint() {
        let a = "[y -> interpolating_function(\"y\", [0.0, 1.0000000010762093], 501, \"7e5099b7fca175ab\")]";
        let b = "[y -> interpolating_function(\"y\", [0.0, 1.0000000010762093], 501, \"74905e1781bd53bb\")]";
        assert!(same_text(a, b));
        assert!(!same_text(&a.replace("501", "502"), b));
        // other strings are exact
        assert!(!same_text("\"abc\"", "\"abd\""));
        assert!(!same_text("\"0123456789abcdef\" + 1", "\"0123456789abcdef\" + 2"));
    }

    #[test]
    fn lines_match_by_kind_and_text() {
        assert!(same_output(&[value("1.0000000000000002")], &[value("1.0")]));
        let message = OutputLine { kind: OutputKind::Print, text: "1.0".into() };
        assert!(!same_output(&[value("1.0")], &[message]));
        assert!(!same_output(&[value("1")], &[value("1"), value("2")]));
    }
}
