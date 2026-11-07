//! `mc-arith` plugged into the evaluator: natives for the arithmetic heads, the comparisons and the logical
//! connectives, and `mc-arith`'s canonical order for associative and commutative heads.
//!
//! | head | attributes | result |
//! |---|---|---|
//! | `add`, `mul` | AC, listable, strict | the canonical sum or product (semantics 3); over lists element-wise |
//! | `pow` | listable, strict | the canonical power; over lists element-wise |
//! | `complex` | strict | a complex number when both parts are real numbers, else the term |
//! | `ball_literal(text, digits)` | strict | the ball of a `1.25~40` literal (decision C.6) |
//! | `equal`, `unequal` | strict | decided for numbers (three-valued, 2.5), identical terms and strings |
//! | `less`, `less_equal`, `greater`, `greater_equal` | strict | decided for real numbers (three-valued) |
//! | `not_same` | strict | `true` unless the arguments are identical |
//! | `and`, `or` | held | evaluated left to right, stopping at the first deciding value |
//! | `not` | strict | `true` / `false` negated |
//!
//! The symbol `im` gets the own value `complex(0, 1)`.
//!
//! A comparison that is not decided (overlapping balls, symbolic arguments) declines, so the call stays unevaluated
//! unless a rule applies. The registrations replace `mc-eval`'s machine-integer comparisons (decision F.13), which
//! stay as `iadd`, `imul`, ... for its own tests. The natives keep only symbols, so they need no tracing.
use mc_api::names;
use mc_arith::{Arith, Cmp3, Order};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::order::TermOrder;
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The flag `mc-arith`'s long numeric loops poll; the engine raises it with the evaluator's interrupt.
type Stop = Arc<AtomicBool>;

fn arith<'h>(h: &'h Heap, stop: &Stop) -> Arith<'h, Heap> {
    Arith::new(h).with_stop(stop.clone())
}

/// Fuel for an arithmetic call: one unit per 16 arguments (the call itself is charged by the evaluator).
fn charge(cx: &mut NativeCx<'_, '_, '_>, args: &[T<'_>]) -> Res<()> {
    cx.tick(args.len() as u64 / 16)
}

/// The fold of machine integers that does not overflow (most sums and products in loops), without setting up an
/// [`Arith`].
fn small_fold(h: &Heap, args: &[T<'_>], op: fn(i64, i64) -> Option<i64>, unit: i64) -> Option<i64> {
    args.iter().try_fold(unit, |acc, &a| match h.view(a) {
        View::Int(v) => op(acc, v),
        _ => None,
    })
}

fn two_ints(h: &Heap, a: T<'_>, b: T<'_>) -> Option<(i64, i64)> {
    match (h.view(a), h.view(b)) {
        (View::Int(x), View::Int(y)) => Some((x, y)),
        _ => None,
    }
}

/// A machine integer, or a rational with machine-integer parts, as (numerator, positive denominator).
fn small_ratio(h: &Heap, t: T<'_>) -> Option<(i64, i64)> {
    match h.view(t) {
        View::Int(v) => Some((v, 1)),
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(n), View::Int(d)) if d > 0 => Some((n, d)),
            _ => None,
        },
        _ => None,
    }
}

/// The sum or product of finite machine reals, machine integers and rationals of machine-integer parts with at
/// least one machine real, folded as `mc-arith` folds numbers (decision B.5): the exact numbers exactly first, then
/// the machine reals in canonical order (for finite reals the IEEE total order), so the result is bit for bit what
/// [`Arith`] gives, without setting it up. An exact rational part is converted as the arithmetic converts it, to
/// the nearest machine real (one correctly rounded division of parts that are machine reals exactly; decision
/// sp-core.6: `x/3` in loops). `None` for any other arguments (or an exact part that overflows).
fn machine_fold(h: &Heap, args: &[T<'_>], product: bool) -> Option<f64> {
    let mut exact: i128 = i128::from(product);
    // the denominator of the exact part (positive; 1 for an integer)
    let mut den: i128 = 1;
    let mut has_exact = false;
    let mut reals: [f64; 8] = [0.0; 8];
    let mut n = 0;
    for &a in args {
        match h.view(a) {
            View::Int(v) => {
                exact = if product { exact.checked_mul(v as i128)? } else { exact.checked_add(den.checked_mul(v as i128)?)? };
                has_exact = true;
            }
            View::Rational { .. } => {
                let (p, q) = small_ratio(h, a)?;
                let (p, q) = (p as i128, q as i128);
                if product {
                    exact = exact.checked_mul(p)?;
                    den = den.checked_mul(q)?;
                } else {
                    exact = exact.checked_mul(q)?.checked_add(p.checked_mul(den)?)?;
                    den = den.checked_mul(q)?;
                }
                let g = gcd128(exact.unsigned_abs(), den.unsigned_abs()) as i128;
                if g > 1 {
                    exact /= g;
                    den /= g;
                }
                has_exact = true;
            }
            View::Real(x) if x.is_finite() && n < reals.len() => {
                reals[n] = x;
                n += 1;
            }
            _ => return None,
        }
    }
    if n == 0 {
        return None;
    }
    let reals = &mut reals[..n];
    reals.sort_by(f64::total_cmp);
    let op = |a: f64, b: f64| if product { a * b } else { a + b };
    let start = if den == 1 {
        exact as f64
    } else if exact.unsigned_abs() <= 1 << 53 && den <= 1 << 53 {
        exact as f64 / den as f64
    } else {
        return None;
    };
    let (mut acc, rest) = if has_exact { (start, &reals[..]) } else { (reals[0], &reals[1..]) };
    for &x in rest {
        acc = op(acc, x);
    }
    Some(acc)
}

/// The greatest common divisor (Euclid's).
fn gcd128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// The product of machine integers and small rationals when it is a machine integer (`c/2` for even `c`): the
/// common case of exact division in loops, without setting up an [`Arith`].
fn small_product(h: &Heap, args: &[T<'_>]) -> Option<i64> {
    let (mut n, mut d) = (1i128, 1i128);
    for &a in args {
        let (p, q) = small_ratio(h, a)?;
        n = n.checked_mul(p as i128)?;
        d = d.checked_mul(q as i128)?;
    }
    if n % d != 0 {
        return None;
    }
    i64::try_from(n / d).ok()
}

/// Numeric constants (semantics 15): symbols that stand for a real number.
const CONSTANTS: [&str; 5] = ["pi", "e", "euler_gamma", "catalan", "golden_ratio"];

/// The symbols of [`CONSTANTS`] in the heap a native was registered with, looked up once (comparing symbols
/// instead of their names keeps the contagion scan free of allocations).
#[derive(Default)]
struct Consts(std::sync::OnceLock<[Sym; 5]>);

impl Consts {
    /// The index of `s` in [`CONSTANTS`], if it is one.
    fn index(&self, h: &Heap, s: Sym) -> Option<usize> {
        self.0.get_or_init(|| CONSTANTS.map(|c| h.sym(c))).iter().position(|&c| c == s)
    }
}

/// Is `t` a closed numeric expression that is not itself a number: built only from numbers and numeric constants
/// (`pi`, `2^(1/2)`, `cos(1)/pi`)? Heads are not inspected; `n` decides whether it really has a value.
fn closed_numeric(h: &Heap, c: &Consts, t: T<'_>) -> bool {
    fn leaves_numeric(h: &Heap, c: &Consts, t: T<'_>) -> bool {
        match h.view(t) {
            View::Sym(s) => c.index(h, s).is_some(),
            View::App { args, .. } => !args.is_empty() && args.iter().all(|&a| leaves_numeric(h, c, a)),
            View::Str(_) => false,
            _ => true,
        }
    }
    match h.view(t) {
        View::Sym(_) => leaves_numeric(h, c, t),
        View::App { head, args } => {
            head != h.sym_term(h.sym(names::LIST)) && h.packed(t).is_none() && !args.is_empty() && leaves_numeric(h, c, t)
        }
        _ => false,
    }
}

/// Contagion for constants (semantics 2.2 extended to section 15): when a machine real is among the arguments,
/// every closed numeric argument (`pi`, `2^(1/2)`) is replaced by its machine value, so `0.5*pi` is
/// `1.5707963267948966` and not `0.5*pi`; with a ball and no machine real, by a ball at the ball's precision
/// (`1.5~30 * pi` is a 30-digit ball). `None` when nothing changes (the common case costs one scan).
fn machine_constants<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &Consts, args: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let h = cx.heap();
    // a machine real, or a complex number with machine parts (queue G8: `complex(3.0, 1.5)*sqrt(2)`)
    let machine_complex = |a: T<'h>| match h.view(a) {
        View::App { head, args: [re, _] } => {
            head == h.sym_term(h.sym(names::COMPLEX)) && matches!(h.view(*re), View::Real(_))
        }
        _ => false,
    };
    let machine = args.iter().any(|&a| matches!(h.view(a), View::Real(_)) || machine_complex(a));
    let ball = if machine { None } else { args.iter().copied().find(|&a| matches!(h.view(a), View::Ball { .. })) };
    if !(machine || ball.is_some()) || !args.iter().any(|&a| closed_numeric(h, c, a)) {
        return Ok(None);
    }
    // the digits of the ball (its precision, rounded up)
    let digits = match ball {
        Some(b) => match h.view(cx.eval(h.call(h.sym("precision"), &[b]))?) {
            View::Real(p) if p.is_finite() && p >= 1.0 => Some(h.int(p.ceil() as i64)),
            View::Int(p) if p >= 1 => Some(h.int(p)),
            _ => return Ok(None),
        },
        None => None,
    };
    let mut out = Vec::with_capacity(args.len());
    let mut changed = false;
    for &a in args {
        if digits.is_none() {
            if let Some(v) = cached_constant(cx, c, a)? {
                out.push(v);
                changed = true;
                continue;
            }
        }
        if closed_numeric(h, c, a) {
            let v = match digits {
                Some(d) => cx.eval(h.call(h.sym("n"), &[a, d]))?,
                None => cx.eval(h.call(h.sym("n"), &[a]))?,
            };
            if matches!(h.view(v), View::Real(_) | View::Ball { .. }) {
                out.push(v);
                changed = true;
                continue;
            }
        }
        out.push(a);
    }
    Ok(changed.then_some(out))
}

/// The machine values of the [`CONSTANTS`], filled on first use by `n` itself (so they are `n`'s values bit for bit);
/// a NaN bit pattern marks an empty slot. Symbols are immediates and never memoised, so without this every
/// `0.5*pi` in a loop evaluated `n(pi)` again (about 7 us a call).
static CONSTANT_VALUES: [std::sync::atomic::AtomicU64; 5] = [const { std::sync::atomic::AtomicU64::new(u64::MAX) }; 5];

/// The machine value of `t` when it is a bare numeric constant, from [`CONSTANT_VALUES`].
fn cached_constant<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &Consts, t: T<'h>) -> Res<Option<T<'h>>> {
    use std::sync::atomic::Ordering::Relaxed;
    let h = cx.heap();
    let View::Sym(s) = h.view(t) else { return Ok(None) };
    let Some(i) = c.index(h, s) else { return Ok(None) };
    let bits = CONSTANT_VALUES[i].load(Relaxed);
    if bits != u64::MAX {
        return Ok(Some(h.real(f64::from_bits(bits))));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    match h.view(v) {
        View::Real(x) => {
            CONSTANT_VALUES[i].store(x.to_bits(), Relaxed);
            Ok(Some(v))
        }
        _ => Ok(None),
    }
}

/// Is an argument an uncertain number of `mc_numeric::uncertain` (`around`, `interval`, `centered_interval`)? A
/// cheap test on the heads before the arithmetic (decision P6N.10).
fn uncertain(h: &Heap, args: &[T<'_>]) -> bool {
    args.iter().any(|&a| matches!(h.view(a), View::App { .. })) && mc_numeric::uncertain::any(h, args)
}

/// `add(args...)`.
struct Add(Stop, Consts);

impl Native for Add {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        charge(cx, args)?;
        if uncertain(cx.heap(), args) {
            if let Some(v) = mc_numeric::uncertain::arith(cx, mc_numeric::uncertain::Op::Add, args)? {
                return Ok(Reply::Done(v));
            }
        }
        // a truncated series: the series of the result (mc-poly, decision IOSYM.1)
        if mc_poly::any_series(cx.heap(), args) {
            if let Some(v) = mc_poly::series_arith(cx, &self.0, "add", args)? {
                return Ok(Reply::Done(v));
            }
        }
        if let Some(v) = machine_constants(cx, &self.1, args)? {
            return self.call(cx, _t, &v);
        }
        let h = cx.heap();
        if let Some(v) = small_fold(h, args, i64::checked_add, 0) {
            return Ok(Reply::Done(h.int(v)));
        }
        Ok(Reply::Done(match machine_fold(h, args, false) {
            Some(v) => h.real(v),
            None => arith(h, &self.0).add(args),
        }))
    }
}

/// `mul(args...)`.
struct Mul(Stop, Consts);

impl Native for Mul {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        charge(cx, args)?;
        if uncertain(cx.heap(), args) {
            if let Some(v) = mc_numeric::uncertain::arith(cx, mc_numeric::uncertain::Op::Mul, args)? {
                return Ok(Reply::Done(v));
            }
        }
        // a truncated series: the series of the result (mc-poly, decision IOSYM.1)
        if mc_poly::any_series(cx.heap(), args) {
            if let Some(v) = mc_poly::series_arith(cx, &self.0, "mul", args)? {
                return Ok(Reply::Done(v));
            }
        }
        if let Some(v) = machine_constants(cx, &self.1, args)? {
            return self.call(cx, _t, &v);
        }
        let h = cx.heap();
        let v = small_fold(h, args, i64::checked_mul, 1).or_else(|| small_product(h, args));
        if let Some(v) = v {
            return Ok(Reply::Done(h.int(v)));
        }
        Ok(Reply::Done(match machine_fold(h, args, true) {
            Some(v) => h.real(v),
            None => arith(h, &self.0).mul(args),
        }))
    }
}

/// `pow(base, exp)`.
struct Pow(Stop, Consts);

impl Native for Pow {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [b, e] = args else { return Ok(Reply::Decline) };
        if uncertain(cx.heap(), args) {
            if let Some(v) = mc_numeric::uncertain::arith(cx, mc_numeric::uncertain::Op::Pow, args)? {
                return Ok(Reply::Done(v));
            }
        }
        // a truncated series: the series of the result (mc-poly, decision IOSYM.1)
        if mc_poly::any_series(cx.heap(), args) {
            if let Some(v) = mc_poly::series_arith(cx, &self.0, "pow", args)? {
                return Ok(Reply::Done(v));
            }
        }
        if let Some(v) = machine_constants(cx, &self.1, args)? {
            return self.call(cx, _t, &v);
        }
        let ar = arith(cx.heap(), &self.0);
        let r = ar.pow(*b, *e);
        // ball powers mc-arith leaves unevaluated (its B.13): principal values from mc-numeric's complex balls
        if let (None, Some(x), Some(y)) = (ar.num(r), ar.num(*b), ar.num(*e)) {
            if x.kind() == mc_arith::Kind::Ball || y.kind() == mc_arith::Kind::Ball {
                let _scope = mc_arith::num::StopScope::enter(Some(&self.0));
                if let Some(v) = mc_numeric::numeval::ball_power(&x, &y) {
                    return Ok(Reply::Done(ar.number(&v)));
                }
            }
        }
        Ok(Reply::Done(r))
    }
}

/// `complex(re, im)`.
struct Complex(Stop);

impl Native for Complex {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [re, im] = args else { return Ok(Reply::Decline) };
        Ok(Reply::Done(arith(cx.heap(), &self.0).complex(*re, *im)))
    }
}

/// `ball_literal(text, digits)`: a literal that does not convert (an exponent beyond the size limit) stays.
struct BallLiteral(Stop);

impl Native for BallLiteral {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [s, d] = args else { return Ok(Reply::Decline) };
        let (View::Str(text), View::Int(digits @ 1..)) = (h.view(*s), h.view(*d)) else { return Ok(Reply::Decline) };
        Ok(match arith(h, &self.0).ball_from_decimal(text, digits as u64) {
            Some(b) => Reply::Done(b),
            None => Reply::Decline,
        })
    }
}

/// The order relations.
#[derive(Clone, Copy)]
enum Rel {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl Rel {
    fn holds(self, o: Ordering) -> bool {
        match self {
            Rel::Less => o.is_lt(),
            Rel::LessEqual => o.is_le(),
            Rel::Greater => o.is_gt(),
            Rel::GreaterEqual => o.is_ge(),
        }
    }
}

/// `less(a, b)` and the other order relations on real numbers.
struct Compare(Rel, Stop);

impl Native for Compare {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [a, b] = args else { return Ok(Reply::Decline) };
        if let Some((x, y)) = two_ints(h, *a, *b) {
            return Ok(Reply::Done(cx.bool(self.0.holds(x.cmp(&y)))));
        }
        Ok(match arith(h, &self.1).compare(*a, *b) {
            Some(Cmp3::Less) => Reply::Done(cx.bool(self.0.holds(Ordering::Less))),
            Some(Cmp3::Equal) => Reply::Done(cx.bool(self.0.holds(Ordering::Equal))),
            Some(Cmp3::Greater) => Reply::Done(cx.bool(self.0.holds(Ordering::Greater))),
            // NaN: every order relation is false (2.5)
            Some(Cmp3::Unordered) => Reply::Done(cx.bool(false)),
            Some(Cmp3::Undecided) | None => Reply::Decline,
        })
    }
}

/// `equal(a, b)` (`negate`: `unequal`). Numbers compare by value (three-valued); otherwise identical terms are equal
/// and distinct strings unequal; anything else is not decided.
struct Equality {
    negate: bool,
    stop: Stop,
}

impl Native for Equality {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [a, b] = args else {
            // `equal(x)`, `unequal(x)` are true; `equal(a, b, c, ...)`: every neighbour equal; `unequal(a, b, c,
            // ...)`: no two equal (decision io-core.10)
            if args.len() == 1 {
                return Ok(Reply::Done(cx.bool(true)));
            }
            if args.is_empty() || args.len() > 64 {
                return Ok(Reply::Decline);
            }
            let mut open = false;
            if self.negate {
                for i in 0..args.len() {
                    for j in i + 1..args.len() {
                        match self.decide(cx, args[i], args[j], 0) {
                            Some(true) => return Ok(Reply::Done(cx.bool(false))),
                            Some(false) => {}
                            None => open = true,
                        }
                    }
                }
            } else {
                for w in args.windows(2) {
                    match self.decide(cx, w[0], w[1], 0) {
                        Some(false) => return Ok(Reply::Done(cx.bool(false))),
                        Some(true) => {}
                        None => open = true,
                    }
                }
            }
            return Ok(if open { Reply::Decline } else { Reply::Done(cx.bool(true)) });
        };
        let decided = self.decide(cx, *a, *b, 0);
        Ok(match decided {
            Some(e) => Reply::Done(cx.bool(e != self.negate)),
            None => Reply::Decline,
        })
    }
}

impl Equality {
    /// `a == b` decided: numbers by value, identical terms, strings; lists element by element (lists of different
    /// lengths are unequal, decision p6core.17). `None` when it is not decided.
    fn decide<'h>(&self, cx: &NativeCx<'_, '_, 'h>, a: T<'h>, b: T<'h>, depth: u32) -> Option<bool> {
        let h = cx.heap();
        if let Some((x, y)) = two_ints(h, a, b) {
            return Some(x == y);
        }
        match arith(h, &self.stop).equal(a, b) {
            Some(d) => return d,
            // numbers first: a NaN is not equal to itself
            None if a == b => return Some(true),
            None => {}
        }
        if matches!((h.view(a), h.view(b)), (View::Str(_), View::Str(_))) {
            return Some(false);
        }
        let (xs, ys) = (cx.as_list(a)?, cx.as_list(b)?);
        if xs.len() != ys.len() {
            return Some(false);
        }
        if depth > 64 {
            return None;
        }
        let mut all = true;
        for (&x, &y) in xs.iter().zip(ys) {
            match self.decide(cx, x, y, depth + 1) {
                Some(false) => return Some(false),
                Some(true) => {}
                None => all = false,
            }
        }
        all.then_some(true)
    }
}

/// `not_same(a, b)`.
struct NotSame;

impl Native for NotSame {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // `not_same(a, b, c, ...)`: no two identical
        let mut seen = std::collections::HashSet::with_capacity(args.len());
        Ok(Reply::Done(cx.bool(args.iter().all(|a| seen.insert(*a)))))
    }
}

/// `and(...)` (`is_and`) and `or(...)`, arguments held: evaluated left to right; the first deciding value (`false`
/// for `and`, `true` for `or`) or failure is the result; neutral values are dropped; what remains stays as the call
/// (or is the result, when one argument remains).
struct Junction {
    is_and: bool,
    failure: Sym,
}

impl Native for Junction {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (decide, neutral) = (cx.bool(!self.is_and), cx.bool(self.is_and));
        let mut rest = vec![];
        for &a in args {
            cx.tick(1)?;
            let v = cx.eval(a)?;
            let failed = matches!(h.view(v), View::App { head, .. } if head == h.sym_term(self.failure));
            if v == decide || failed {
                return Ok(Reply::Done(v));
            }
            if v != neutral {
                rest.push(v);
            }
        }
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        Ok(Reply::Done(match rest.as_slice() {
            [] => neutral,
            [v] => *v,
            _ => h.app(head, &rest),
        }))
    }
}

/// `not(x)`.
struct Not;

impl Native for Not {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        Ok(if cx.is_true(*x) {
            Reply::Done(cx.bool(false))
        } else if cx.is_false(*x) {
            Reply::Done(cx.bool(true))
        } else if let Some(u) = negated(cx.heap(), *x) {
            // `not not a` is `a`; a negated two-sided comparison is the opposite comparison (p6core.1)
            Reply::Again(u)
        } else {
            Reply::Decline
        })
    }
}

/// The negation of `x` without `not`: `a` for `not(a)`, the opposite relation for a comparison of two terms
/// (`not x > y` is `x <= y`, `not a == b` is `a != b`).
fn negated<'h>(h: &'h Heap, x: T<'h>) -> Option<T<'h>> {
    let View::App { head, args } = h.view(x) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    let name = h.sym_name(s);
    if name == "not" {
        return match args {
            [a] => Some(*a),
            _ => None,
        };
    }
    let opposite = match name.as_str() {
        "less" => "greater_equal",
        "less_equal" => "greater",
        "greater" => "less_equal",
        "greater_equal" => "less",
        "equal" => "unequal",
        "unequal" => "equal",
        _ => return None,
    };
    match args {
        [a, b] => Some(h.app(h.sym_term(h.sym(opposite)), &[*a, *b])),
        _ => None,
    }
}

/// `mc-arith`'s canonical order (semantics 8), replacing `mc-eval`'s structural stand-in (decision F.9).
pub struct ArithOrder;

impl TermOrder for ArithOrder {
    fn cmp<'h>(&self, h: &'h Heap, a: T<'h>, b: T<'h>) -> Ordering {
        if let Some((x, y)) = two_ints(h, a, b) {
            return x.cmp(&y);
        }
        Order::new(h).cmp(a, b)
    }
    fn sort<'h>(&self, h: &'h Heap, v: &mut [T<'h>]) {
        // machine integers and small rationals (most sums and products in loops) are ordered by value, with no
        // order to set up: distinct exact numbers never have equal values
        if v.len() < 2 {
            return;
        }
        if v.iter().all(|&t| small_ratio(h, t).is_some()) {
            // invariant: every element was checked by the `all` above
            let key = |t: T<'h>| small_ratio(h, t).expect("checked");
            v.sort_unstable_by(|&a, &b| {
                let ((p, q), (r, s)) = (key(a), key(b));
                (p as i128 * s as i128).cmp(&(r as i128 * q as i128))
            });
            return;
        }
        // one order (and its cache of symbol names) per sort
        let o = Order::new(h);
        v.sort_by(|a, b| o.cmp(*a, *b));
    }
}

/// Register the natives and the order with `k`. `stop` is the flag `mc-arith` polls.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Stop) {
    let strict = Attributes::default();
    // `add`, `mul` and `pow` thread over lists (semantics 7.2): `a - b` and `a / b` are sums and products of
    // powers, so subtraction and division thread too; two-operand `add`/`mul` on packed arrays work on their
    // payloads (N4a.7)
    let ac = Attributes { ac: true, listable: true, ..strict };
    let listable = Attributes { listable: true, ..strict };
    let held = Attributes { hold: Hold::All, ..strict };
    let mut reg = |name: &str, a: Attributes, n: Box<dyn Native>| k.register(h.sym(name), a, n);
    reg(names::ADD, ac, Box::new(Add(stop.clone(), Consts::default())));
    reg(names::MUL, ac, Box::new(Mul(stop.clone(), Consts::default())));
    reg(names::POW, listable, Box::new(Pow(stop.clone(), Consts::default())));
    reg(names::COMPLEX, strict, Box::new(Complex(stop.clone())));
    reg("ball_literal", strict, Box::new(BallLiteral(stop.clone())));
    reg("equal", strict, Box::new(Equality { negate: false, stop: stop.clone() }));
    reg("unequal", strict, Box::new(Equality { negate: true, stop: stop.clone() }));
    reg("less", strict, Box::new(Compare(Rel::Less, stop.clone())));
    reg("less_equal", strict, Box::new(Compare(Rel::LessEqual, stop.clone())));
    reg("greater", strict, Box::new(Compare(Rel::Greater, stop.clone())));
    reg("greater_equal", strict, Box::new(Compare(Rel::GreaterEqual, stop.clone())));
    reg("not_same", strict, Box::new(NotSame));
    let failure = h.sym(names::FAILURE);
    reg("and", held, Box::new(Junction { is_and: true, failure }));
    reg("or", held, Box::new(Junction { is_and: false, failure }));
    reg("not", strict, Box::new(Not));
    k.set_order(Box::new(ArithOrder));
    // the imaginary unit `im` (04-syntax section 4), so that `3 - im` is a number like `3 - 1im`
    let i = arith(h, stop).complex(h.int(0), h.int(1));
    k.set_own(h.sym("im"), i);
}

/// One canonical form for the exponential function (decision p6core.5): `exp(x)` is `e^x` whenever `exp`'s own
/// native leaves it unevaluated, so `exp(a)*e^b` is `e^(a + b)`; and `e^x` gets `exp`'s exact values
/// (`e^log(2)` is 2, `e^(im*pi)` is -1). Call after the library defining `exp` is installed; does nothing when
/// `exp` or `pow` has no native. The wrapped natives keep no terms (`exp`'s and `pow`'s keep only symbols).
pub fn canonical_exp(k: &mut Kernel, h: &Heap) {
    let (exp, pow, e) = (h.sym("exp"), h.sym(names::POW), h.sym("e"));
    let mut inner: Option<Arc<dyn Native>> = None;
    k.wrap_native(exp, |old| {
        let old: Arc<dyn Native> = Arc::from(old);
        inner = Some(old.clone());
        Box::new(ExpToPower { inner: old, exp, pow, e })
    });
    if let Some(exp_native) = inner {
        k.wrap_native(pow, |old| Box::new(PowerOfE { inner: old, exp_native, exp, e }));
    }
}

/// `exp(x)`: `exp`'s native, then `e^x` for what it leaves.
struct ExpToPower {
    inner: Arc<dyn Native>,
    exp: Sym,
    pow: Sym,
    e: Sym,
}

impl Native for ExpToPower {
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [x] = args else { return self.inner.call(cx, t, args) };
        let unevaluated = |v: T<'h>| matches!(h.view(v), View::App { head, args: [y] } if head == h.sym_term(self.exp) && *y == *x);
        let power = || Reply::Again(h.app(h.sym_term(self.pow), &[h.sym_term(self.e), *x]));
        Ok(match self.inner.call(cx, t, args)? {
            Reply::Decline => power(),
            Reply::Done(v) | Reply::Again(v) if unevaluated(v) => power(),
            r => r,
        })
    }
}

/// `pow(b, x)`: the arithmetic's power, then for `e^x` with a symbolic `x` the exact values of `exp`.
struct PowerOfE {
    inner: Box<dyn Native>,
    exp_native: Arc<dyn Native>,
    exp: Sym,
    e: Sym,
}

impl Native for PowerOfE {
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // `e^x` for an inexact x is `exp(x)`'s value, not a power of the rounded `e` (one rounding, not two)
        if let [b, x] = args {
            if *b == h.sym_term(self.e) && matches!(h.view(*x), View::Real(_) | View::Ball { .. }) {
                let call = h.app(h.sym_term(self.exp), &[*x]);
                if let Reply::Done(u) = self.exp_native.call(cx, call, &[*x])? {
                    if u != call {
                        return Ok(Reply::Done(u));
                    }
                }
            }
        }
        let r = self.inner.call(cx, t, args)?;
        let Reply::Done(v) = r else { return Ok(r) };
        let e = h.sym_term(self.e);
        let View::App { args: [b, x], .. } = h.view(v) else { return Ok(r) };
        if *b != e || !matches!(h.view(*x), View::App { .. } | View::Sym(_)) {
            return Ok(r);
        }
        let call = h.app(h.sym_term(self.exp), &[*x]);
        let same = |u: T<'h>| u == call || u == v;
        Ok(match self.exp_native.call(cx, call, &[*x])? {
            Reply::Done(u) if !same(u) => Reply::Done(u),
            Reply::Again(u) if !same(u) && !matches!(h.view(u), View::App { head, .. } if head == h.sym_term(self.exp)) => {
                Reply::Again(u)
            }
            _ => r,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Machine integers and small rationals, made by `mc-arith` (so rationals are canonical).
    fn numbers(h: &Heap) -> Vec<T<'_>> {
        let a = Arith::new(h);
        let mut v = vec![];
        for p in [-7i64, -2, -1, 0, 1, 3, 5, 12, i64::MAX, i64::MIN + 1] {
            v.push(h.int(p));
            for q in [2i64, 3, 7] {
                v.push(a.mul(&[h.int(p), a.pow(h.int(q), h.int(-1))]));
            }
        }
        v
    }

    #[test]
    fn the_fast_paths_agree_with_the_general_order_and_product() {
        let h = Heap::new(mc_core::Config::default());
        let v = numbers(&h);
        let mut fast = v.clone();
        ArithOrder.sort(&h, &mut fast);
        let mut general = v.clone();
        let o = Order::new(&h);
        general.sort_by(|a, b| o.cmp(*a, *b));
        assert_eq!(fast, general);
        let a = Arith::new(&h);
        for &x in &v {
            for &y in &v {
                assert_eq!(ArithOrder.cmp(&h, x, y), o.cmp(x, y));
                if let Some(p) = small_product(&h, &[x, y]) {
                    assert_eq!(h.int(p), a.mul(&[x, y]));
                }
            }
        }
    }
    #[test]
    fn the_machine_fold_is_bitwise_the_general_sum_and_product() {
        let h = Heap::new(mc_core::Config::default());
        let a = Arith::new(&h);
        let reals = [0.1, -0.0, 0.0, 1.0e308, -1.0e308, 3.0, -2.5, 1.0e-300, 7.0 / 3.0, -1.0e16, 1.0];
        let ints = [0i64, 1, -1, 3, i64::MAX, i64::MIN + 1];
        let mut terms: Vec<T<'_>> = reals.iter().map(|&x| h.real(x)).collect();
        terms.extend(ints.iter().map(|&v| h.int(v)));
        // rationals (sp-core.6), small and with parts beyond 2^53
        for (p, q) in [(1i64, 3i64), (-2, 7), (5, 2), (1, 10), ((1 << 60) + 1, 3), (7, (1 << 55) + 1)] {
            terms.push(a.mul(&[h.int(p), a.pow(h.int(q), h.int(-1))]));
        }
        // every sequence of 2 to 4 operands from a fixed pseudo-random walk over the terms
        let mut state = 12345u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as usize
        };
        let mut checked = 0;
        for _ in 0..5000 {
            let k = 2 + next() % 3;
            let args: Vec<T<'_>> = (0..k).map(|_| terms[next() % terms.len()]).collect();
            for product in [false, true] {
                if let Some(v) = machine_fold(&h, &args, product) {
                    let want = if product { a.mul(&args) } else { a.add(&args) };
                    assert_eq!(h.real(v), want, "{args:?} product {product}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 5000);
        // not taken: no machine real, a non-finite real, a symbol
        assert!(machine_fold(&h, &[h.int(1), h.int(2)], false).is_none());
        assert!(machine_fold(&h, &[h.real(f64::INFINITY), h.real(1.0)], false).is_none());
        assert!(machine_fold(&h, &[h.real(1.0), h.sym_term(h.sym("x"))], true).is_none());
    }
}
