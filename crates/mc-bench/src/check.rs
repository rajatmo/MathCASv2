//! Result checkers (02-benchmarks section 2). They run after timing, outside the measured region.
//!
//! The property checker needs the engine and lives in the child runner; this module holds the checks that compare
//! a printed result or fingerprint with a stored value.
use std::cmp::Ordering;
use std::fmt;

/// The outcome of a check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    Pass,
    /// The result is wrong; the text says how.
    Fail(String),
    /// Nothing to compare against (no stored value, or no fingerprint from the engine).
    Unchecked(String),
}

impl Check {
    /// The short word written in results files.
    pub fn word(&self) -> &'static str {
        match self {
            Check::Pass => "pass",
            Check::Fail(_) => "fail",
            Check::Unchecked(_) => "unchecked",
        }
    }
    /// The explanation, empty for a pass.
    pub fn detail(&self) -> &str {
        match self {
            Check::Pass => "",
            Check::Fail(d) | Check::Unchecked(d) => d,
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.detail() {
            "" => write!(f, "{}", self.word()),
            d => write!(f, "{} ({d})", self.word()),
        }
    }
}

/// `exact`: the printed result equals the stored text, ignoring surrounding white space.
pub fn exact(printed: &str, expected: &str) -> Check {
    if printed.trim() == expected.trim() {
        Check::Pass
    } else {
        Check::Fail(format!("got {}, expected {}", abbreviate(printed.trim()), abbreviate(expected.trim())))
    }
}

/// `exact hash`: the engine's fingerprint equals the stored hexadecimal hash.
pub fn hash(fingerprint: Option<u64>, expected: &str) -> Check {
    let digits = expected.trim().trim_start_matches("0x");
    let Ok(want) = u64::from_str_radix(digits, 16) else {
        return Check::Fail(format!("stored hash {expected} is not hexadecimal"));
    };
    match fingerprint {
        None => Check::Unchecked("the engine gives no fingerprint".into()),
        Some(got) if got == want => Check::Pass,
        Some(got) => Check::Fail(format!("fingerprint {got:#018x}, expected {want:#018x}")),
    }
}

/// `digits d`: the printed number agrees with the stored value to `d` significant digits, that is, they differ by
/// at most one unit in the `d`-th digit (the printed value may be rounded where the stored one is truncated).
///
/// For balls this is also the containment check: the printer shows only correct digits (04-syntax section 4), so
/// a ball whose printed digits agree with the true value contains it to that accuracy (decision D.7).
pub fn digits(printed: &str, expected: &str, d: usize) -> Check {
    let (Some(got), Some(want)) = (Decimal::parse(printed), Decimal::parse(expected)) else {
        return Check::Fail(format!("not a decimal number: {}", abbreviate(printed.trim())));
    };
    if want.digits.len() < d {
        return Check::Unchecked(format!("stored value has only {} digits", want.digits.len()));
    }
    if got.digits.len() < d && !got.is_zero() {
        return Check::Fail(format!("only {} significant digits printed, {d} needed", got.digits.len()));
    }
    // Align both on the larger exponent, keep d + 2 digits, and compare the difference with one unit in the d-th.
    let exp = got.exp.max(want.exp);
    let width = d + 2;
    let a = got.scaled(exp, width);
    let b = want.scaled(exp, width);
    let diff = if got.neg == want.neg { abs_diff(&a, &b) } else { add(&a, &b) };
    let mut tolerance = vec![0u8; width];
    tolerance[width - 3] = 1; // 100 in units of the last kept digit, plus one for truncation below
    tolerance[width - 1] = 1;
    if compare(&diff, &tolerance) == Ordering::Greater {
        Check::Fail(format!("differs within {d} digits: got {}", abbreviate(printed.trim())))
    } else {
        Check::Pass
    }
}

/// A decimal number as sign, significant digits (no leading zeros) and the decimal exponent of the point: the value
/// is `0.d1 d2 d3 ... * 10^exp`.
struct Decimal {
    neg: bool,
    digits: Vec<u8>,
    exp: i64,
}

impl Decimal {
    /// Accepts `-12.5`, `3.14e-5`, `1.25~40` (precision marker ignored), `1_000`.
    fn parse(s: &str) -> Option<Decimal> {
        let s = s.trim();
        let s = s.split('~').next()?;
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (mantissa, exponent) = match s.find(['e', 'E']) {
            Some(i) => (&s[..i], s[i + 1..].parse::<i64>().ok()?),
            None => (s, 0),
        };
        let mut digits = Vec::new();
        let mut before_point: i64 = 0;
        let mut seen_point = false;
        for c in mantissa.chars() {
            match c {
                '0'..='9' => {
                    digits.push(c as u8 - b'0');
                    if !seen_point {
                        before_point += 1;
                    }
                }
                '.' if !seen_point => seen_point = true,
                '_' => {}
                _ => return None,
            }
        }
        if digits.is_empty() {
            return None;
        }
        let leading = digits.iter().take_while(|&&d| d == 0).count();
        digits.drain(..leading);
        Some(Decimal { neg, digits, exp: before_point - leading as i64 + exponent })
    }

    fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// The digits of `|self| / 10^(exp - width)`, truncated, as exactly `width` digits (exp >= self.exp).
    fn scaled(&self, exp: i64, width: usize) -> Vec<u8> {
        let shift = usize::try_from(exp - self.exp).unwrap_or(usize::MAX).min(width);
        let mut out = vec![0u8; shift];
        out.extend(self.digits.iter().copied().take(width - shift));
        out.resize(width, 0);
        out
    }
}

fn compare(a: &[u8], b: &[u8]) -> Ordering {
    a.cmp(b) // equal lengths, most significant first
}

fn abs_diff(a: &[u8], b: &[u8]) -> Vec<u8> {
    let (hi, lo) = if compare(a, b) == Ordering::Less { (b, a) } else { (a, b) };
    let mut out = vec![0u8; hi.len()];
    let mut borrow = 0i8;
    for i in (0..hi.len()).rev() {
        let mut d = hi[i] as i8 - lo[i] as i8 - borrow;
        borrow = (d < 0) as i8;
        if d < 0 {
            d += 10;
        }
        out[i] = d as u8;
    }
    out
}

fn add(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; a.len()];
    let mut carry = 0u8;
    for i in (0..a.len()).rev() {
        let s = a[i] + b[i] + carry;
        out[i] = s % 10;
        carry = s / 10;
    }
    if carry > 0 {
        out.iter_mut().for_each(|d| *d = 9); // saturate: certainly above any tolerance
    }
    out
}

/// Keep result descriptions short in results files.
pub fn abbreviate(s: &str) -> String {
    const MAX: usize = 60;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{head}... ({} chars)", s.chars().count())
    }
}

/// FNV-1a, 64 bits: a fingerprint of printed text for engines without a structural one.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PI: &str = "3.14159265358979323846264338327950288";

    #[test]
    fn exact_and_hash() {
        assert_eq!(exact(" 6765\n", "6765"), Check::Pass);
        assert_eq!(exact("6764", "6765").word(), "fail");
        assert_eq!(hash(Some(0xab), "0x00ab"), Check::Pass);
        assert_eq!(hash(Some(0xab), "ac").word(), "fail");
        assert_eq!(hash(None, "ab").word(), "unchecked");
        assert_eq!(hash(Some(1), "xyz").word(), "fail");
    }

    #[test]
    fn digits_tolerates_rounding_of_the_last_digit() {
        assert_eq!(digits("3.1415926535897932385", PI, 20), Check::Pass);
        assert_eq!(digits("3.1415926535897932384", PI, 20), Check::Pass);
        assert_eq!(digits("3.1415926535897932395", PI, 20).word(), "fail");
        assert_eq!(digits("3.14159", PI, 20).word(), "fail");
        assert_eq!(digits("-3.14159", PI, 5).word(), "fail");
        assert_eq!(digits("3.14159~40", PI, 6), Check::Pass);
        assert_eq!(digits("31.4159e-1", PI, 6), Check::Pass);
        assert_eq!(digits("3.14", "3.1", 3).word(), "unchecked");
    }

    #[test]
    fn digits_across_a_power_of_ten() {
        assert_eq!(digits("1.0000000", "0.99999999", 7), Check::Pass);
        assert_eq!(digits("0.000123456", "1.23456e-4", 6), Check::Pass);
        assert_eq!(digits("1.0000000", "0.99990000", 7).word(), "fail");
    }

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
