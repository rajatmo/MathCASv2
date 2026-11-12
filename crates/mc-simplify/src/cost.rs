//! The complexity measure `simplify` minimises (decisions s-cost.1 to s-cost.3).
//!
//! - **Size** ([`Cost::size`]): a leaf count in which integers weigh by their length. Zero counts 1; a nonzero
//!   integer counts its number of decimal digits, plus 1 when it is negative; a rational counts 1 plus its
//!   numerator and denominator; a complex number counts 1 plus its two parts; every other atom (symbol, real,
//!   ball, string) counts 1; a compound term counts its head plus all its arguments. So `simplify` prefers fewer
//!   and shorter numbers as well as fewer nodes: `(3 + 1234567 x)^15` beats its 16-term expansion by far more than
//!   its node count says. The digits of an integer beyond 64 bits are read off its bit length, not by formatting
//!   (the lower bound `floor((bits - 1) log10 2) + 1`, exact or one short), so the measure is O(limbs) at worst
//!   and O(1) for machine integers.
//! - **Tie-breaks** ([`Score`]): among forms of equal size, fewer powers with a negative exponent, then fewer
//!   distinct function heads (`add`, `mul`, `pow`, `list` and `complex` are not counted). Ties beyond these keep
//!   the order in which the forms were found, the input first.
//! - **Caching**: terms are hash-consed, so one term is one handle and its size is stored in a map that lives for
//!   one `simplify` call. Every candidate of the search shares most of its subterms with the input and with the
//!   other candidates, so pricing a candidate walks only the nodes not priced before (the cost is incremental).
//!   Handles are stable during a native call (a collection needs the heap exclusively), so the map stays valid.
//! - **A user measure** ([`Measure::User`]): the `complexity_function:` option, a function called on every whole
//!   candidate (results cached per candidate); its value must be a real number, anything else counts as
//!   infinitely complex, so such a candidate never replaces the input. The size above still bounds the search
//!   (the growth cap of `simplify`).
use mc_api::{Store, Sym, View, T};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// The parts of the measure of one subterm that add up over its children.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Part {
    /// the digit-weighted leaf count
    size: u64,
    /// powers with a negative numeric exponent
    neg_pows: u32,
    /// a 64-bit set of the function heads in the term (bit from a hash of the name, stable across sessions)
    heads: u64,
}

/// The score of a candidate: smaller is simpler. Compared by `main`, then the tie-breaks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    /// the size (default measure) or the user function's value
    pub main: f64,
    /// powers with a negative exponent (0 under a user measure)
    pub neg_pows: u32,
    /// distinct function heads, up to collisions of a 64-bit set (0 under a user measure)
    pub funcs: u32,
}

impl Score {
    /// The total order of scores (`main` by `f64::total_cmp`).
    pub fn order(&self, o: &Score) -> Ordering {
        self.main.total_cmp(&o.main).then(self.neg_pows.cmp(&o.neg_pows)).then(self.funcs.cmp(&o.funcs))
    }
    /// Is `self` strictly simpler than `o`?
    pub fn simpler_than(&self, o: &Score) -> bool {
        self.order(o) == Ordering::Less
    }
}

/// Which measure scores candidates.
#[derive(Clone, Copy, Debug)]
pub enum Measure<'h> {
    /// the digit-weighted leaf count with its tie-breaks
    Default,
    /// `complexity_function: f`: `f(candidate)`, a real number
    User(T<'h>),
}

/// The measure of one `simplify` call with its caches.
pub struct Cost<'h> {
    h: &'h Heap,
    measure: Measure<'h>,
    memo: FxHashMap<T<'h>, Part>,
    user: FxHashMap<T<'h>, f64>,
    /// the heads that are not counted as functions, and the bit of every other head seen
    arith: [Sym; 5],
    bits: FxHashMap<Sym, u64>,
    /// nodes priced (for the measurements of NOTES.md)
    pub walked: u64,
}

impl<'h> Cost<'h> {
    /// The default measure on `h`.
    pub fn new(h: &'h Heap) -> Cost<'h> {
        Cost::with(h, Measure::Default)
    }

    /// The measure this cost uses.
    pub fn measure(&self) -> Measure<'h> {
        self.measure
    }

    /// The measure `m` on `h`.
    pub fn with(h: &'h Heap, measure: Measure<'h>) -> Cost<'h> {
        let arith =
            [mc_api::names::ADD, mc_api::names::MUL, mc_api::names::POW, mc_api::names::LIST, mc_api::names::COMPLEX]
                .map(|n| h.sym(n));
        Cost {
            h,
            measure,
            memo: FxHashMap::default(),
            user: FxHashMap::default(),
            arith,
            bits: FxHashMap::default(),
            walked: 0,
        }
    }

    /// The digit-weighted leaf count of `t`.
    pub fn size(&mut self, t: T<'h>) -> u64 {
        self.part(t).size
    }

    /// The score of the candidate `t` under the measure of the call.
    pub fn score(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Score> {
        match self.measure {
            Measure::Default => {
                let p = self.part(t);
                Ok(Score { main: p.size as f64, neg_pows: p.neg_pows, funcs: p.heads.count_ones() })
            }
            Measure::User(f) => {
                if let Some(&v) = self.user.get(&t) {
                    return Ok(Score { main: v, neg_pows: 0, funcs: 0 });
                }
                let v = user_value(cx, f, t)?;
                self.user.insert(t, v);
                Ok(Score { main: v, neg_pows: 0, funcs: 0 })
            }
        }
    }

    /// The bit of a function head (0 for the arithmetic heads).
    fn head_bit(&mut self, s: Sym) -> u64 {
        if self.arith.contains(&s) {
            return 0;
        }
        if let Some(&b) = self.bits.get(&s) {
            return b;
        }
        // FNV-1a of the name: the same bit in every session, whatever the order symbols were made in
        let mut x: u64 = 0xcbf29ce484222325;
        for byte in self.h.sym_name(s).bytes() {
            x = (x ^ byte as u64).wrapping_mul(0x100000001b3);
        }
        let b = 1u64 << (x % 64);
        self.bits.insert(s, b);
        b
    }

    /// The additive parts of `t`, walking only the nodes not priced yet.
    fn part(&mut self, t: T<'h>) -> Part {
        if let Some(&p) = self.memo.get(&t) {
            return p;
        }
        let h = self.h;
        let mut stack = vec![(t, false)];
        while let Some((u, ready)) = stack.pop() {
            if !ready && self.memo.contains_key(&u) {
                continue;
            }
            let p = match h.view(u) {
                View::Int(n) => atom(int_size(n)),
                View::BigInt { neg, mag } => atom(big_size(neg, mag)),
                View::Rational { num, den } => atom(1 + self.atom_size(num) + self.atom_size(den)),
                View::App { head, args } => {
                    if !ready {
                        stack.push((u, true));
                        stack.push((head, false));
                        stack.extend(args.iter().filter(|a| !self.memo.contains_key(*a)).map(|&a| (a, false)));
                        continue;
                    }
                    let mut p = self.memo[&head];
                    for a in args {
                        let q = self.memo[a];
                        p.size += q.size;
                        p.neg_pows += q.neg_pows;
                        p.heads |= q.heads;
                    }
                    match h.view(head) {
                        // a complex number counts 1 plus its parts, like a rational
                        View::Sym(s) if s == self.arith[4] && args.len() == 2 => {}
                        View::Sym(s) => {
                            p.heads |= self.head_bit(s);
                            if s == self.arith[2] && args.len() == 2 && negative_number(h, args[1]) {
                                p.neg_pows += 1;
                            }
                        }
                        _ => {}
                    }
                    p
                }
                _ => atom(1),
            };
            self.walked += 1;
            self.memo.insert(u, p);
        }
        self.memo[&t]
    }

    /// The size of an integer term (the parts of a rational).
    fn atom_size(&self, t: T<'h>) -> u64 {
        match self.h.view(t) {
            View::Int(n) => int_size(n),
            View::BigInt { neg, mag } => big_size(neg, mag),
            _ => 1,
        }
    }
}

fn atom(size: u64) -> Part {
    Part { size, ..Part::default() }
}

/// Is `t` a negative integer or rational (an exponent that makes a power a reciprocal)?
fn negative_number(h: &Heap, t: T<'_>) -> bool {
    match h.view(t) {
        View::Int(n) => n < 0,
        View::BigInt { neg, .. } => neg,
        View::Rational { num, .. } => negative_number(h, num),
        View::Real(v) => v < 0.0,
        _ => false,
    }
}

/// The size of a machine integer: 1 for 0, else its decimal digits, plus 1 when negative.
pub fn int_size(n: i64) -> u64 {
    if n == 0 {
        return 1;
    }
    n.unsigned_abs().ilog10() as u64 + 1 + u64::from(n < 0)
}

/// The size of a big integer from its limbs, plus 1 when negative. Up to 128 bits the digit count is exact (one
/// comparison with a power of ten); beyond, it is `floor((bits - 1) log10 2) + 1`, the exact count or one less, read
/// off the bit length without any big arithmetic.
pub fn big_size(neg: bool, mag: &[u64]) -> u64 {
    let top = mag.iter().rposition(|&w| w != 0);
    let Some(top) = top else { return 1 };
    let bits = 64 * top as u64 + (64 - mag[top].leading_zeros() as u64);
    // log10(2) = 0.30102999566398...; a 64-bit fixed-point multiple keeps it exact far beyond any limb count
    let mut digits = (((bits - 1) as u128 * 0x4D10_4D42_7DE7_FBCC_u128) >> 64) as u64 + 1;
    if top <= 1 {
        let v = mag[0] as u128 | (mag.get(1).copied().unwrap_or(0) as u128) << 64;
        if digits <= 38 && v >= 10u128.pow(digits as u32) {
            digits += 1;
        }
    }
    digits + u64::from(neg)
}

/// `f(t)` as a real number, `+inf` when it is none.
fn user_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, t: T<'h>) -> Res<f64> {
    let h = cx.heap();
    let mut r = cx.eval(h.app(f, &[t]))?;
    let ar = mc_arith::Arith::new(h);
    if ar.num(r).is_none() {
        r = cx.eval(h.call(h.sym("n"), &[r]))?;
    }
    Ok(match ar.num(r) {
        Some(mc_arith::number::Num::Real(x)) => {
            let v = x.to_f64();
            if v.is_nan() {
                f64::INFINITY
            } else {
                v
            }
        }
        _ => f64::INFINITY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_sizes() {
        assert_eq!(int_size(0), 1);
        assert_eq!(int_size(7), 1);
        assert_eq!(int_size(-7), 2);
        assert_eq!(int_size(10), 2);
        assert_eq!(int_size(99), 2);
        assert_eq!(int_size(100), 3);
        assert_eq!(int_size(-1000), 5);
        assert_eq!(int_size(i64::MAX), 19);
        assert_eq!(int_size(i64::MIN), 20);
    }

    #[test]
    fn big_integer_sizes_are_exact_or_one_short() {
        // 2^64 = 18446744073709551616 (20 digits), 2^64 - 1 has 20 digits, 2^127 has 39 digits, 2^128 - 1 has 39
        assert_eq!(big_size(false, &[0, 1]), 20);
        assert_eq!(big_size(false, &[u64::MAX]), 20);
        assert_eq!(big_size(true, &[0, 1]), 21);
        assert_eq!(big_size(false, &[0, 1 << 63]), 39);
        assert_eq!(big_size(false, &[u64::MAX, u64::MAX]), 39);
        // exact up to 128 bits, also at the powers of ten: 10^20 - 1 and 10^20, 10^38 - 1 and 10^38
        for e in [20u32, 25, 38] {
            let p = 10u128.pow(e);
            let limbs = |v: u128| [v as u64, (v >> 64) as u64];
            assert_eq!(big_size(false, &limbs(p - 1)), e as u64);
            assert_eq!(big_size(false, &limbs(p)), e as u64 + 1);
        }
        // 2^200 (61 digits) and 2^200 - 1: exact or one short
        let mut m = [u64::MAX; 4];
        m[3] = (1 << 8) - 1;
        let low = big_size(false, &m);
        assert!(low == 60 || low == 61, "{low}");
        // 10^19 < 2^64: 10^19 * 2 = 20000000000000000000 (20 digits, 65 bits)
        let v: u128 = 20_000_000_000_000_000_000;
        assert_eq!(big_size(false, &[v as u64, (v >> 64) as u64]), 20);
        // against the exact count for powers of two up to 2^1000 and their predecessors
        for k in 64..1000u64 {
            let exact = (k as f64 * std::f64::consts::LOG10_2).floor() as u64 + 1;
            let mut mag = vec![0u64; (k / 64 + 1) as usize];
            mag[(k / 64) as usize] = 1 << (k % 64);
            let got = big_size(false, &mag);
            assert!(got == exact || got + 1 == exact, "2^{k}: {got} vs {exact}");
        }
        assert_eq!(big_size(false, &[0, 0]), 1);
    }

    fn sizes(f: impl for<'h> FnOnce(&'h Heap, &mut Cost<'h>)) {
        let h = Heap::new(mc_core::Config::default());
        let mut c = Cost::new(&h);
        f(&h, &mut c);
    }

    #[test]
    fn atoms_numbers_and_calls() {
        sizes(|h, c| {
            let x = h.sym_term(h.sym("x"));
            let int = |v| h.int(v);
            assert_eq!(c.size(x), 1);
            assert_eq!(c.size(h.string("long text")), 1);
            assert_eq!(c.size(h.real(-2.5)), 1);
            assert_eq!(c.size(int(0)), 1);
            assert_eq!(c.size(int(-1)), 2);
            assert_eq!(c.size(int(1234567)), 7);
            // rationals and complex numbers: 1 plus their parts
            assert_eq!(c.size(h.rational(int(3), int(8))), 3);
            assert_eq!(c.size(h.rational(int(-1), int(2))), 4);
            assert_eq!(c.size(h.rational(int(-22), int(7))), 5);
            let cpx = h.call(h.sym(mc_api::names::COMPLEX), &[int(0), int(1)]);
            assert_eq!(c.size(cpx), 3);
            let cpx = h.call(h.sym(mc_api::names::COMPLEX), &[h.rational(int(1), int(2)), int(-10)]);
            assert_eq!(c.size(cpx), 1 + 3 + 3);
            // a big integer
            assert_eq!(c.size(h.big_int(true, &[0, 1])), 21);
            // calls: the head and every argument, shared subterms once per occurrence
            let g = h.call(h.sym("g"), &[x]);
            let f = h.call(h.sym("f"), &[g, g, int(-12)]);
            assert_eq!(c.size(f), 1 + 2 + 2 + 3);
            // x - y = add(x, mul(-1, y)): the sign counts
            let y = h.sym_term(h.sym("y"));
            let d = h.call(h.sym(mc_api::names::ADD), &[x, h.call(h.sym(mc_api::names::MUL), &[int(-1), y])]);
            assert_eq!(c.size(d), 1 + 1 + (1 + 2 + 1));
        });
    }

    #[test]
    fn candidates_are_priced_incrementally() {
        sizes(|h, c| {
            let x = h.sym_term(h.sym("x"));
            let add = h.sym(mc_api::names::ADD);
            let pow = h.sym(mc_api::names::POW);
            let big: Vec<T> = (1..200).map(|k| h.call(pow, &[x, h.int(k)])).collect();
            let s = h.call(add, &big);
            let n0 = c.size(s);
            let walked = c.walked;
            // a candidate sharing all the terms of `s` but one costs a walk of the new nodes only
            let mut other = big.clone();
            other[0] = h.call(h.sym("sin"), &[x]);
            let s2 = h.call(add, &other);
            let n1 = c.size(s2);
            assert_eq!(n1, n0 - 3 + 2);
            assert!(c.walked - walked <= 3, "{}", c.walked - walked);
            // pricing a term again walks nothing
            let w = c.walked;
            assert_eq!(c.size(s), n0);
            assert_eq!(c.walked, w);
        });
    }

    fn p<'h>(c: &mut Cost<'h>, t: T<'h>) -> (u64, u32, u32) {
        let Part { size, neg_pows, heads } = c.part(t);
        (size, neg_pows, heads.count_ones())
    }

    fn sc<'h>(c: &mut Cost<'h>, t: T<'h>) -> Score {
        let (size, neg_pows, funcs) = p(c, t);
        Score { main: size as f64, neg_pows, funcs }
    }

    #[test]
    fn tie_breaks() {
        sizes(|h, c| {
            let x = h.sym_term(h.sym("x"));
            let y = h.sym_term(h.sym("y"));
            let pow = h.sym(mc_api::names::POW);
            let mul = h.sym(mc_api::names::MUL);
            // x^-1 y has a negative power, x y^2 none (sizes 6 and 5)
            let a = h.call(mul, &[h.call(pow, &[x, h.int(-1)]), y]);
            assert_eq!(p(c, a), (6, 1, 0));
            let b = h.call(mul, &[x, h.call(pow, &[y, h.int(2)])]);
            assert_eq!(p(c, b), (5, 0, 0));
            // distinct functions: sin(x) sin(y) has one, sin(x) cos(y) two
            let sin = h.sym("sin");
            let cos = h.sym("cos");
            let s1 = h.call(mul, &[h.call(sin, &[x]), h.call(sin, &[y])]);
            let s2 = h.call(mul, &[h.call(sin, &[x]), h.call(cos, &[y])]);
            assert_eq!(p(c, s1).2, 1);
            let two = p(c, s2).2;
            // the 64-bit set may merge two names; sin and cos are distinct bits
            assert_eq!(two, 2);
            assert!(sc(c, s1).simpler_than(&sc(c, s2)));
            assert!(!sc(c, s2).simpler_than(&sc(c, s1)));
            assert!(!sc(c, s1).simpler_than(&sc(c, s1)));
        });
    }
}
