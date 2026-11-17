//! [`B`]: the term builder of one call (canonical sums, products and powers through `mc-arith`, plain calls for
//! everything else), and [`rewrite`], the bottom-up walk every transform of this crate is built on.
use mc_api::{names, Store, Sym, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The symbols this crate looks at, interned once per call.
pub struct Heads {
    /// `add`
    pub add: Sym,
    /// `mul`
    pub mul: Sym,
    /// `pow`
    pub pow: Sym,
    /// `list`
    pub list: Sym,
    /// `complex`, the head of a complex number
    pub complex: Sym,
    /// the constant `e`
    pub e: Sym,
    /// `exp`
    pub exp: Sym,
    /// `log`
    pub log: Sym,
    /// `sin`
    pub sin: Sym,
    /// `cos`
    pub cos: Sym,
    /// `tan`
    pub tan: Sym,
    /// `cot`
    pub cot: Sym,
    /// `sec`
    pub sec: Sym,
    /// `csc`
    pub csc: Sym,
    /// `sinh`
    pub sinh: Sym,
    /// `cosh`
    pub cosh: Sym,
    /// `tanh`
    pub tanh: Sym,
}

/// The builder: the heap, its canonical arithmetic and the interned heads.
pub struct B<'h> {
    /// the canonical arithmetic
    pub ar: Arith<'h, Heap>,
    /// the heads
    pub hd: Heads,
    h: &'h Heap,
    stop: Arc<AtomicBool>,
}

impl<'h> B<'h> {
    /// A builder on `h`; big exact numbers poll `stop`.
    pub fn new(h: &'h Heap, stop: &Arc<AtomicBool>) -> B<'h> {
        let hd = Heads {
            add: h.sym(names::ADD),
            mul: h.sym(names::MUL),
            pow: h.sym(names::POW),
            list: h.sym(names::LIST),
            complex: h.sym(names::COMPLEX),
            e: h.sym("e"),
            exp: h.sym("exp"),
            log: h.sym("log"),
            sin: h.sym("sin"),
            cos: h.sym("cos"),
            tan: h.sym("tan"),
            cot: h.sym("cot"),
            sec: h.sym("sec"),
            csc: h.sym("csc"),
            sinh: h.sym("sinh"),
            cosh: h.sym("cosh"),
            tanh: h.sym("tanh"),
        };
        B { ar: Arith::new(h).with_stop(stop.clone()), hd, h, stop: stop.clone() }
    }

    /// The stop flag the builder's arithmetic polls (parallel tasks make their own builders with it).
    pub fn stop(&self) -> &Arc<AtomicBool> {
        &self.stop
    }

    /// The heap.
    pub fn heap(&self) -> &'h Heap {
        self.h
    }
    /// A machine integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    /// The rational n/d (canonical).
    pub fn rat(&self, n: i64, d: i64) -> T<'h> {
        self.ar.rational(self.h.int(n), self.h.int(d))
    }
    /// The complex number `re + im*i` (parts canonical real numbers, `im` not zero).
    pub fn complex(&self, re: T<'h>, im: T<'h>) -> T<'h> {
        self.h.call(self.hd.complex, &[re, im])
    }
    /// The imaginary unit.
    pub fn i(&self) -> T<'h> {
        self.complex(self.int(0), self.int(1))
    }
    /// A canonical sum.
    pub fn add(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.int(0),
            [a] => *a,
            _ => self.ar.add(args),
        }
    }
    /// A canonical product.
    pub fn mul(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.int(1),
            [a] => *a,
            _ => self.ar.mul(args),
        }
    }
    /// A canonical power.
    pub fn pow(&self, b: T<'h>, e: T<'h>) -> T<'h> {
        self.ar.pow(b, e)
    }
    /// `b^k` for a machine integer k.
    pub fn powi(&self, b: T<'h>, k: i64) -> T<'h> {
        self.ar.pow(b, self.h.int(k))
    }
    /// `-a`
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.ar.neg(a)
    }
    /// `a - b`
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.sub(a, b)
    }
    /// `a / b`
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.div(a, b)
    }
    /// The call `s(args...)`, evaluated later.
    pub fn call(&self, s: Sym, args: &[T<'h>]) -> T<'h> {
        self.h.call(s, args)
    }
    /// The call `name(args...)`, evaluated later.
    pub fn call_named(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }
    /// `head(args...)` with canonical construction when the head is `add`, `mul` or `pow`.
    pub fn app(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        match (self.sym_of(head), args) {
            (Some(s), _) if s == self.hd.add => self.add(args),
            (Some(s), _) if s == self.hd.mul => self.mul(args),
            (Some(s), [b, e]) if s == self.hd.pow => self.pow(*b, *e),
            _ => self.h.app(head, args),
        }
    }
    /// The symbol of a symbol term.
    pub fn sym_of(&self, t: T<'h>) -> Option<Sym> {
        match self.h.view(t) {
            View::Sym(s) => Some(s),
            _ => None,
        }
    }
    /// `(s, args)` when `t` is a call with symbol head `s`.
    pub fn call_of(&self, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
        match self.h.view(t) {
            View::App { head, args } => self.sym_of(head).map(|s| (s, args)),
            _ => None,
        }
    }
    /// The argument of a one-argument call of `s`.
    pub fn unary(&self, t: T<'h>, s: Sym) -> Option<T<'h>> {
        match self.call_of(t) {
            Some((g, [a])) if g == s => Some(*a),
            _ => None,
        }
    }
    /// The terms of a sum (`t` itself when it is not one).
    pub fn terms(&self, t: T<'h>) -> Vec<T<'h>> {
        match self.call_of(t) {
            Some((s, args)) if s == self.hd.add => args.to_vec(),
            _ => vec![t],
        }
    }
    /// The factors of a product (`t` itself when it is not one).
    pub fn factors(&self, t: T<'h>) -> Vec<T<'h>> {
        match self.call_of(t) {
            Some((s, args)) if s == self.hd.mul => args.to_vec(),
            _ => vec![t],
        }
    }
    /// `(base, k)` when `t` is `base^k` with a machine integer `k`; `(t, 1)` otherwise.
    pub fn int_power(&self, t: T<'h>) -> (T<'h>, i64) {
        match self.call_of(t) {
            Some((s, [b, e])) if s == self.hd.pow => match self.h.view(*e) {
                View::Int(k) => (*b, k),
                _ => (t, 1),
            },
            _ => (t, 1),
        }
    }
    /// Is `t` an exact or inexact number (complex numbers included)?
    pub fn is_number(&self, t: T<'h>) -> bool {
        self.ar.num(t).is_some()
    }
    /// The parts `(re, im)` of a complex number term.
    pub fn complex_parts(&self, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
        match self.call_of(t) {
            Some((s, [re, im])) if s == self.hd.complex => Some((*re, *im)),
            _ => None,
        }
    }
    /// `exp(u)` as a call (also for the base `e`).
    pub fn exp(&self, u: T<'h>) -> T<'h> {
        self.call(self.hd.exp, &[u])
    }
    /// The exponent `u` when `t` is `exp(u)` or `e^u`.
    pub fn exp_arg(&self, t: T<'h>) -> Option<T<'h>> {
        if let Some(u) = self.unary(t, self.hd.exp) {
            return Some(u);
        }
        match self.call_of(t) {
            Some((s, [b, u])) if s == self.hd.pow && self.sym_of(*b) == Some(self.hd.e) => Some(*u),
            _ => None,
        }
    }
}

/// A rule of [`rewrite`]: the replacement of one (already rebuilt) node, or `None` to keep it.
pub type Rule<'r, 'h> = dyn FnMut(&mut NativeCx<'_, '_, 'h>, &B<'h>, T<'h>) -> Res<Option<T<'h>>> + 'r;

/// `t` rewritten bottom up: every compound node is rebuilt from its rewritten arguments (canonically for sums,
/// products and powers) and then offered to `rule`, whose replacement is kept as it is (not walked again). Each
/// node costs one unit of fuel. Iterative, with a memo over the DAG, so deep and shared terms are cheap.
pub fn rewrite<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, rule: &mut Rule<'_, 'h>) -> Res<T<'h>> {
    let h = b.heap();
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    let mut args_buf = Vec::new();
    while let Some((u, ready)) = stack.pop() {
        if done.contains_key(&u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else {
            done.insert(u, u);
            continue;
        };
        // numbers (complex numbers are calls) are left alone
        if b.complex_parts(u).is_some() {
            done.insert(u, u);
            continue;
        }
        if !ready {
            stack.push((u, true));
            stack.extend(args.iter().filter(|a| !done.contains_key(*a)).map(|&a| (a, false)));
            continue;
        }
        cx.tick(1)?;
        args_buf.clear();
        args_buf.extend(args.iter().map(|a| done[a]));
        let node = if args_buf.as_slice() == args { u } else { b.app(head, &args_buf) };
        let out = rule(cx, b, node)?.unwrap_or(node);
        done.insert(u, out);
    }
    Ok(done[&t])
}

/// `t` rewritten with `rule` and evaluated, repeatedly, until it no longer changes (at most `max` rounds).
pub fn fixed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, mut t: T<'h>, max: usize, rule: &mut Rule<'_, 'h>) -> Res<T<'h>> {
    for _ in 0..max {
        let r = rewrite(cx, b, t, rule)?;
        if r == t {
            break;
        }
        let r = cx.eval(r)?;
        if r == t {
            break;
        }
        t = r;
    }
    Ok(t)
}

/// `name(t)` evaluated (a library function of another crate: `together`, `factor`, `expand`, ...).
pub fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, name: &str, t: T<'h>) -> Res<T<'h>> {
    cx.eval(b.call_named(name, &[t]))
}

/// The rational coefficient `(n, d)` of a term (`(1, 1)` without a numerical factor), `None` for an inexact, complex
/// or very large one.
fn rational_coefficient<'h>(b: &B<'h>, t: T<'h>) -> Option<(i128, i128)> {
    let c = match b.call_of(t) {
        Some((s, [c, ..])) if s == b.hd.mul && b.is_number(*c) => *c,
        _ if b.is_number(t) => t,
        _ => return Some((1, 1)),
    };
    match b.heap().view(c) {
        View::Int(n) => Some((i128::from(n), 1)),
        View::Rational { num, den } => match (b.heap().view(num), b.heap().view(den)) {
            (View::Int(n), View::Int(d)) => Some((i128::from(n), i128::from(d))),
            _ => None,
        },
        _ => None,
    }
}

fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    (a, b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Is `t` a polynomial sum: built from rational numbers and symbols by sums, products and powers with positive
/// integer exponents, with at least one symbol that is not a numerical constant (`2 + 2 n`, not `2 + pi/3`)?
pub fn polynomial_sum<'h>(b: &B<'h>, t: T<'h>) -> bool {
    const CONSTANTS: &[&str] = &["pi", "e", "euler_gamma", "catalan", "golden_ratio", "degree", "glaisher", "khinchin"];
    let Some((s, args)) = b.call_of(t) else { return false };
    if s != b.hd.add {
        return false;
    }
    let h = b.heap();
    let mut variable = false;
    let mut seen = std::collections::HashSet::new();
    let mut stack: Vec<T<'h>> = args.to_vec();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Int(_) | View::Rational { .. } => {}
            View::Sym(s) => variable |= !CONSTANTS.contains(&h.sym_name(s).as_str()),
            _ => match b.call_of(u) {
                Some((p, [base, e])) if p == b.hd.pow => match h.view(*e) {
                    View::Int(k) if k > 0 => stack.push(*base),
                    _ => return false,
                },
                Some((p, xs)) if p == b.hd.add || p == b.hd.mul => stack.extend_from_slice(xs),
                _ => return false,
            },
        }
    }
    variable
}

/// The sum `t` with the positive rational content of its terms' coefficients written outside, the sum left with
/// integer coefficients without a common factor: `3/2 - 2 cos(2 x) + (1/2) cos(4 x)` is
/// `(1/2) (3 - 4 cos(2 x) + cos(4 x))`, `2 cos(3 x) + 6 cos(x)` is `2 (cos(3 x) + 3 cos(x))`. Lists, relations and
/// logical connectives map over their parts; anything else, and a sum with an inexact, complex or very large
/// coefficient, stays as it is.
pub fn content_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    // shared subterms are done once (a list can be a tree exponentially larger than itself)
    content_form_memo(cx, b, t, &mut FxHashMap::default())
}

fn content_form_memo<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> Res<T<'h>> {
    if let Some(&r) = memo.get(&t) {
        return Ok(r);
    }
    cx.tick(1)?;
    let r = content_form_node(cx, b, t, memo)?;
    memo.insert(t, r);
    Ok(r)
}

fn content_form_node<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> Res<T<'h>> {
    let h = b.heap();
    let Some((s, args)) = b.call_of(t) else { return Ok(t) };
    let name = h.sym_name(s);
    if s == b.hd.list
        || matches!(name.as_str(), "and" | "or" | "not" | "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal")
    {
        let mut out = Vec::with_capacity(args.len());
        for &a in args {
            out.push(content_form_memo(cx, b, a, memo)?);
        }
        if out.as_slice() == args {
            return Ok(t);
        }
        return cx.eval(b.call(s, &out));
    }
    if s != b.hd.add {
        return Ok(t);
    }
    let (mut g, mut l) = (0i128, 1i128);
    for &u in args {
        let Some((n, d)) = rational_coefficient(b, u) else { return Ok(t) };
        g = gcd_i128(g, n);
        let Some(nl) = (l / gcd_i128(l, d)).checked_mul(d) else { return Ok(t) };
        l = nl;
        if l > i128::from(i64::MAX) || g > i128::from(i64::MAX) {
            return Ok(t);
        }
    }
    if g == 0 || g == l {
        return Ok(t);
    }
    let (g, l) = (g as i64, l as i64);
    let inv = b.rat(l, g);
    let terms: Vec<T<'h>> = args.iter().map(|&u| b.mul(&[inv, u])).collect();
    let sum = b.add(&terms);
    cx.eval(b.mul(&[b.rat(g, l), sum]))
}

/// A plain full form of `t` (debugging).
#[doc(hidden)]
pub fn dbg<'h>(h: &'h Heap, t: T<'h>) -> String {
    match h.view(t) {
        View::Int(k) => k.to_string(),
        View::Sym(s) => h.sym_name(s),
        View::App { head, args } => format!("{}({})", dbg(h, head), args.iter().map(|&a| dbg(h, a)).collect::<Vec<_>>().join(", ")),
        View::Rational { num, den } => format!("{}/{}", dbg(h, num), dbg(h, den)),
        View::Real(v) => v.to_string(),
        _ => "?".into(),
    }
}
