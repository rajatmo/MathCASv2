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

/// Is `t` a closed numeric expression that is not itself a number: built only from numbers and numeric constants
/// (`pi`, `2^(1/2)`, `cos(1)/pi`)? Heads are not inspected; `n` decides whether it really has a value.
fn closed_numeric(h: &Heap, t: T<'_>) -> bool {
    fn leaves_numeric(h: &Heap, t: T<'_>) -> bool {
        match h.view(t) {
            View::Sym(s) => CONSTANTS.contains(&h.sym_name(s).as_str()),
            View::App { args, .. } => !args.is_empty() && args.iter().all(|&a| leaves_numeric(h, a)),
            View::Str(_) => false,
            _ => true,
        }
    }
    match h.view(t) {
        View::Sym(_) => leaves_numeric(h, t),
        View::App { head, args } => {
            head != h.sym_term(h.sym(names::LIST)) && h.packed(t).is_none() && !args.is_empty() && leaves_numeric(h, t)
        }
        _ => false,
    }
}

/// Contagion for constants (semantics 2.2 extended to section 15): when a machine real is among the arguments,
/// every closed numeric argument (`pi`, `2^(1/2)`) is replaced by its machine value, so `0.5*pi` is
/// `1.5707963267948966` and not `0.5*pi`; with a ball and no machine real, by a ball at the ball's precision
/// (`1.5~30 * pi` is a 30-digit ball). `None` when nothing changes (the common case costs one scan).
fn machine_constants<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let h = cx.heap();
    let machine = args.iter().any(|&a| matches!(h.view(a), View::Real(_)));
    let ball = if machine { None } else { args.iter().copied().find(|&a| matches!(h.view(a), View::Ball { .. })) };
    if !(machine || ball.is_some()) || !args.iter().any(|&a| closed_numeric(h, a)) {
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
        if closed_numeric(h, a) {
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

/// `add(args...)`.
struct Add(Stop);

impl Native for Add {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        charge(cx, args)?;
        if let Some(v) = machine_constants(cx, args)? {
            return self.call(cx, _t, &v);
        }
        let h = cx.heap();
        Ok(Reply::Done(match small_fold(h, args, i64::checked_add, 0) {
            Some(v) => h.int(v),
            None => arith(h, &self.0).add(args),
        }))
    }
}

/// `mul(args...)`.
struct Mul(Stop);

impl Native for Mul {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        charge(cx, args)?;
        if let Some(v) = machine_constants(cx, args)? {
            return self.call(cx, _t, &v);
        }
        let h = cx.heap();
        let v = small_fold(h, args, i64::checked_mul, 1).or_else(|| small_product(h, args));
        Ok(Reply::Done(match v {
            Some(v) => h.int(v),
            None => arith(h, &self.0).mul(args),
        }))
    }
}

/// `pow(base, exp)`.
struct Pow(Stop);

impl Native for Pow {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [b, e] = args else { return Ok(Reply::Decline) };
        if let Some(v) = machine_constants(cx, args)? {
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
        let h = cx.heap();
        let [a, b] = args else { return Ok(Reply::Decline) };
        let decided = if let Some((x, y)) = two_ints(h, *a, *b) {
            Some(x == y)
        } else {
            match arith(h, &self.stop).equal(*a, *b) {
                Some(d) => d,
                // numbers first: a NaN is not equal to itself
                None if a == b => Some(true),
                None => matches!((h.view(*a), h.view(*b)), (View::Str(_), View::Str(_))).then_some(false),
            }
        };
        Ok(match decided {
            Some(e) => Reply::Done(cx.bool(e != self.negate)),
            None => Reply::Decline,
        })
    }
}

/// `not_same(a, b)`.
struct NotSame;

impl Native for NotSame {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [a, b] = args else { return Ok(Reply::Decline) };
        Ok(Reply::Done(cx.bool(a != b)))
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
        } else {
            Reply::Decline
        })
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
    reg(names::ADD, ac, Box::new(Add(stop.clone())));
    reg(names::MUL, ac, Box::new(Mul(stop.clone())));
    reg(names::POW, listable, Box::new(Pow(stop.clone())));
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
}
