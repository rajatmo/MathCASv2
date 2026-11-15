//! Conditional results and condition accumulation (item F3).
//!
//! **The term** `conditional_expression(value, condition)` is a value that holds only under a condition (v2 had no
//! such term before). It evaluates as follows:
//!
//! - `conditional_expression(v, true)` is `v`; `conditional_expression(v, false)` is `undefined`;
//! - a nested one merges: `conditional_expression(conditional_expression(v, c1), c2)` is
//!   `conditional_expression(v, c2 and c1)`;
//! - a condition that is a conjunction is simplified once ([`simplify_condition`]): conjuncts implied by the others
//!   are dropped, a contradiction makes the whole `undefined`, and a conjunction of order relations in one real
//!   variable is put in `reduce`'s interval form (`x > 0 and x < 1` is `0 < x and x < 1`).
//!
//! **Lifting**: arithmetic, comparisons, the logical connectives and the elementary and special functions take the
//! condition outward, `f(conditional_expression(a, c), b)` being `conditional_expression(f(a, b), c)` with the
//! conditions of several arguments conjoined (so `x + conditional_expression(y, c)` is a conditional sum, and two
//! contradictory conditions give `undefined`). Lists keep conditional elements. Operators on expressions
//! (`simplify`, `refine`, `expand`, `factor`, `d`, `integrate`, `limit`, `n`, ...) work on the value under the
//! condition (`simplify(conditional_expression(sqrt(x^2), x < 0))` is `conditional_expression(-x, x < 0)`),
//! `normal` gives the value, and `reduce` conjoins the condition to the equation.
//!
//! **Conditional truth values.** A statement that holds exactly when a condition `c` on its parameters holds (a
//! property of a function with parameters: `function_analytic(log(x^2 + a), x)`) has one form,
//! `conditional_expression(true, c)`; `conditional_expression(false, c)` is a statement that fails for the
//! parameters where `c` holds (the generic case). Where a truth value is read, a conditional one is its condition:
//! `conditional_expression(true, c)` is `c` and `conditional_expression(false, c)` is `false`. So the connectives
//! (`and`, `or`, `not`, `xor`, `implies`, `nand`, `nor`, `equivalent`) and the test of `if` take `c` in its place
//! (`not(conditional_expression(true, a > 0))` is `a <= 0`, not a conditional `false`); `reduce`, `resolve` and
//! `find_instance` conjoin the condition (as for every conditional formula); `simplify`, `full_simplify` and
//! `refine` decide the condition under the facts of the context (`true` or `false` when it is decided, the
//! condition simplified otherwise). Values that are not `true` or `false` keep the lifting below.
//!
//! **Accumulation** ([`Conditions`]): a method that needs a condition (`re(a) > 0` for a convergent integral)
//! records it in the per-call accumulator instead of returning it; the caller simplifies the conditions once at the
//! end, under the facts of the context, and attaches them as one conditional result ([`Conditions::finish`]).
use crate::decide::Decider;
use mc_api::{names, Store, Sym, View, T};
use mc_core::{Heap, Raw};
use mc_eval::pat;
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res, Rhs};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The head of a conditional result.
pub const CONDITIONAL: &str = "conditional_expression";
/// Most conjuncts a condition is simplified with.
const MAX_CONJUNCTS: usize = 8;

/// The heads that take conditions outward from their arguments.
pub const VALUE_HEADS: &[&str] = &[
    names::ADD, names::MUL, names::POW, "equal", "unequal", "less", "less_equal", "greater", "greater_equal", "exp", "log", "sin",
    "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "coth", "sech", "csch", "asin", "acos", "atan", "acot", "asinh", "acosh",
    "atanh", "abs", "sign", "gamma", "log_gamma", "erf", "erfc", "floor", "ceiling", "round", "re", "im_part", "conjugate", "arg", "max",
    "min", "real_abs", "real_sign", "sqrt",
];
/// The held connectives, whose evaluated forms take conditions outward (a conditional truth value is its condition).
pub const HELD_HEADS: &[&str] = &["and", "or"];
/// The connectives that read a conditional truth value as its condition (other conditional values are lifted).
pub const TRUTH_HEADS: &[&str] = &["not", "xor", "implies", "nand", "nor", "equivalent"];
/// The operators that conjoin the condition to the formula.
pub const CONJOIN_HEADS: &[&str] = &["reduce"];
/// The operators registered after this crate that conjoin the condition to the formula ([`install_late`]).
pub const LATE_CONJOIN_HEADS: &[&str] = &["resolve", "find_instance"];
/// The operators that work on the value under the condition.
pub const OPERATOR_HEADS: &[&str] = &[
    "simplify", "full_simplify", "refine", "expand", "factor", "together", "apart", "cancel", "d", "integrate", "limit", "n", "power_expand", "trig_expand",
    "trig_reduce", "trig_to_exp", "exp_to_trig", "complex_expand", "function_expand", "collect",
];

/// `(value, condition)` of a conditional result.
pub fn parts<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match h.view(t) {
        View::App { head, args: &[v, c] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == CONDITIONAL) => Some((v, c)),
        _ => None,
    }
}

/// The conjunction of `cs` as a term (`true` for none).
pub fn conjunction<'h>(h: &'h Heap, cs: &[T<'h>]) -> T<'h> {
    match cs {
        [] => h.sym_term(h.sym(names::TRUE)),
        [c] => *c,
        _ => h.call(h.sym("and"), cs),
    }
}

/// The conjuncts of `c`, `and`s flattened.
fn conjuncts<'h>(h: &'h Heap, c: T<'h>, out: &mut Vec<T<'h>>) {
    match h.view(c) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "and") => {
            for &a in args {
                conjuncts(h, a, out);
            }
        }
        _ => {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
}

/// `c` simplified as a condition, with `facts` (those of the context, or none) assumed: evaluated; conjuncts that
/// the facts and the other conjuncts imply dropped; `false` when one is refuted; a conjunction of order relations in
/// one real variable in `reduce`'s form. Bounded: at most [`MAX_CONJUNCTS`] conjuncts are examined.
pub fn simplify_condition<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, facts: &[T<'h>], c: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let c = cx.eval(c)?;
    if cx.is_true(c) || cx.is_false(c) {
        return Ok(c);
    }
    let mut cs = vec![];
    conjuncts(h, c, &mut cs);
    if cs.iter().any(|&x| cx.is_false(x)) {
        return Ok(cx.bool(false));
    }
    cs.retain(|&x| !cx.is_true(x));
    if cs.len() > MAX_CONJUNCTS {
        return Ok(conjunction(h, &cs));
    }
    // each conjunct against the facts and the conjuncts kept so far and still to come
    let mut i = 0;
    while i < cs.len() {
        cx.tick(1)?;
        let mut others: Vec<T<'h>> = facts.to_vec();
        others.extend(cs.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &x)| x));
        if !others.is_empty() {
            let mut d = Decider::new(cx, stop, &others)?;
            match d.truth(cx, cs[i])? {
                Some(true) => {
                    cs.remove(i);
                    continue;
                }
                Some(false) => return Ok(cx.bool(false)),
                None => {}
            }
        }
        i += 1;
    }
    if cs.len() >= 2 {
        if let Some(r) = by_reduce(cx, &cs)? {
            return Ok(r);
        }
    }
    Ok(conjunction(h, &cs))
}

/// A conjunction of order relations in one variable as `reduce` writes it, or `None`.
fn by_reduce<'h>(cx: &mut NativeCx<'_, '_, 'h>, cs: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut var: Option<T<'h>> = None;
    for &c in cs {
        let View::App { head, args } = h.view(c) else { return Ok(None) };
        let View::Sym(s) = h.view(head) else { return Ok(None) };
        if !matches!(h.sym_name(s).as_str(), "less" | "less_equal" | "greater" | "greater_equal") || args.len() != 2 {
            return Ok(None);
        }
        let b = crate::build::Bld::new(h, &Arc::new(AtomicBool::new(false)));
        let Some(vs) = b.symbols(c, 4) else { return Ok(None) };
        let nm = mc_numeric::numeval::Names::new(h);
        let vs: Vec<T<'h>> = vs.into_iter().filter(|&v| b.sym_of(v).is_some_and(|s| nm.constant(s).is_none())).collect();
        match (vs.as_slice(), var) {
            ([x], None) => var = Some(*x),
            ([x], Some(y)) if *x == y => {}
            _ => return Ok(None),
        }
    }
    let Some(x) = var else { return Ok(None) };
    let r = cx.eval(h.call(h.sym("reduce"), &[conjunction(h, cs), x, h.sym_term(h.sym("reals"))]))?;
    // only an answer made of relations and connectives (not a failure or the unevaluated call)
    let ok = cx.is_false(r)
        || match h.view(r) {
            View::App { head, .. } => {
                matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "less" | "less_equal" | "greater" | "greater_equal" | "equal" | "and" | "or"))
            }
            _ => false,
        };
    Ok(ok.then_some(r))
}

/// The per-call accumulator of conditions (roadmap F3; used by definite integration, 14D, and `solve`, 11C).
#[derive(Clone, Default)]
pub struct Conditions<'h> {
    conds: Vec<T<'h>>,
}

impl<'h> Conditions<'h> {
    /// No conditions.
    pub fn new() -> Conditions<'h> {
        Conditions { conds: vec![] }
    }
    /// Record that the result needs `c`.
    pub fn record(&mut self, c: T<'h>) {
        if !self.conds.contains(&c) {
            self.conds.push(c);
        }
    }
    /// The conditions recorded, in order.
    pub fn conditions(&self) -> &[T<'h>] {
        &self.conds
    }
    /// Were none recorded?
    pub fn is_empty(&self) -> bool {
        self.conds.is_empty()
    }
    /// The conditions, simplified once under the facts of the call's assumption context: `true`, `false` or a
    /// condition (the result depends on the context).
    pub fn simplified(&self, cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>) -> Res<T<'h>> {
        let h = cx.heap();
        if self.conds.is_empty() {
            return Ok(cx.bool(true));
        }
        let facts = cx.assumptions().to_vec();
        simplify_condition(cx, stop, &facts, conjunction(h, &self.conds))
    }
    /// `value` with the recorded conditions attached: `value` itself when they hold under the context's facts,
    /// `undefined` when they cannot hold, and `conditional_expression(value, condition)` otherwise.
    pub fn finish(&self, cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, value: T<'h>) -> Res<T<'h>> {
        let h = cx.heap();
        let c = self.simplified(cx, stop)?;
        Ok(if cx.is_true(c) {
            value
        } else if cx.is_false(c) {
            h.sym_term(h.sym("undefined"))
        } else {
            h.call(h.sym(CONDITIONAL), &[value, c])
        })
    }
}

/// The native of `conditional_expression`.
pub struct CondNative {
    pub(crate) stop: Arc<AtomicBool>,
}

impl Native for CondNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [v, c] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        if cx.is_true(*c) {
            return Ok(Reply::Done(*v));
        }
        if cx.is_false(*c) {
            return Ok(Reply::Done(h.sym_term(h.sym("undefined"))));
        }
        if let Some((w, c2)) = parts(h, *v) {
            let both = h.call(h.sym("and"), &[*c, c2]);
            return Ok(Reply::Again(h.call(h.sym(CONDITIONAL), &[w, both])));
        }
        let mut cs = vec![];
        conjuncts(h, *c, &mut cs);
        if cs.len() >= 2 || cs.len() == 1 && cs[0] != *c {
            let s = simplify_condition(cx, &self.stop, &[], *c)?;
            if cx.is_true(s) {
                return Ok(Reply::Done(*v));
            }
            if cx.is_false(s) {
                return Ok(Reply::Done(h.sym_term(h.sym("undefined"))));
            }
            if s != *c {
                // the parts are evaluated and the condition simplified: a normal form
                return Ok(Reply::Done(h.call(h.sym(CONDITIONAL), &[*v, s])));
            }
        }
        Ok(Reply::Decline)
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// `(value, condition)` of a conditional result with head `ce`.
fn parts_of<'h>(h: &'h Heap, ce: Sym, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match h.view(t) {
        View::App { head, args: &[v, c] } if matches!(h.view(head), View::Sym(s) if s == ce) => Some((v, c)),
        _ => None,
    }
}

/// `f(args)` with the conditions of its conditional arguments (head `ce`, symbol mask bit `bit`) taken outward, or
/// `None` when there are none. The mask test keeps the common path to one AND per argument.
fn lifted<'h>(h: &'h Heap, f: Sym, ce: Sym, bit: u64, args: &[T<'h>]) -> Option<T<'h>> {
    if !args.iter().any(|&a| h.mask(a) & bit != 0) {
        return None;
    }
    let mut vals = Vec::with_capacity(args.len());
    let mut conds = vec![];
    for &a in args {
        match parts_of(h, ce, a) {
            Some((v, c)) => {
                vals.push(v);
                conds.push(c);
            }
            None => vals.push(a),
        }
    }
    if conds.is_empty() {
        return None;
    }
    Some(h.call(ce, &[h.call(f, &vals), conjunction(h, &conds)]))
}

/// The truth value `v` (`Some(true)` or `Some(false)`) of a conditional truth value.
fn truth<'h>(h: &'h Heap, v: T<'h>) -> Option<bool> {
    match h.view(v) {
        View::Sym(s) if h.sym_name(s) == names::TRUE => Some(true),
        View::Sym(s) if h.sym_name(s) == names::FALSE => Some(false),
        _ => None,
    }
}

/// `args` (the first `n` of them) with each conditional truth value read as a truth value: its condition for
/// `true`, `false` for `false`; `None` when there is none.
fn truth_read<'h>(h: &'h Heap, ce: Sym, bit: u64, args: &[T<'h>], n: usize) -> Option<Vec<T<'h>>> {
    let n = n.min(args.len());
    if !args[..n].iter().any(|&a| h.mask(a) & bit != 0) {
        return None;
    }
    let mut any = false;
    let mut out = args.to_vec();
    for a in out.iter_mut().take(n) {
        if let Some((v, c)) = parts_of(h, ce, *a) {
            if let Some(b) = truth(h, v) {
                *a = if b { c } else { v };
                any = true;
            }
        }
    }
    any.then_some(out)
}

/// `f(args)` with conditional truth values read as truth values, or else with the conditions taken outward.
fn truth_or_lift<'h>(h: &'h Heap, f: Sym, ce: Sym, bit: u64, args: &[T<'h>]) -> Option<T<'h>> {
    if let Some(new) = truth_read(h, ce, bit, args, args.len()) {
        return Some(h.call(f, &new));
    }
    lifted(h, f, ce, bit, args)
}

/// `t` with every conditional result inside it replaced by its value.
fn strip_all<'h>(h: &'h Heap, ce: Sym, bit: u64, t: T<'h>) -> T<'h> {
    if h.mask(t) & bit == 0 {
        return t;
    }
    match h.view(t) {
        View::App { head, args } => {
            if let Some((v, _)) = parts_of(h, ce, t) {
                return strip_all(h, ce, bit, v);
            }
            let new: Vec<T<'h>> = args.iter().map(|&a| strip_all(h, ce, bit, a)).collect();
            h.app(head, &new)
        }
        _ => t,
    }
}

/// How a wrapped head treats conditional arguments.
#[derive(Clone, Copy)]
enum Mode {
    /// conditions of the arguments go outward
    Value,
    /// the evaluated form (of a held connective) reads conditional truth values, then takes conditions outward
    Held,
    /// a connective: conditional truth values are read as truth values, other conditions go outward
    Truth,
    /// `if`: a conditional truth value as the test is read as a truth value
    If,
    /// the operator works on the value under the condition
    Operator,
    /// `normal`: the value
    Normal,
    /// `reduce`: the condition conjoined
    Reduce,
}

/// A head's native wrapped so that conditional arguments are handled first.
struct Lift {
    f: Sym,
    /// `conditional_expression` and its symbol mask bit
    ce: Sym,
    bit: u64,
    mode: Mode,
    inner: Option<Box<dyn Native>>,
}

impl Lift {
    fn inner<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        match &self.inner {
            Some(n) => n.call(cx, t, args),
            None => Ok(Reply::Decline),
        }
    }
}

impl Native for Lift {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        match self.mode {
            Mode::Value => {
                if let Some(r) = lifted(h, self.f, self.ce, self.bit, args) {
                    return Ok(Reply::Again(r));
                }
                self.inner(cx, t, args)
            }
            Mode::Truth => {
                if let Some(r) = truth_or_lift(h, self.f, self.ce, self.bit, args) {
                    return Ok(Reply::Again(r));
                }
                self.inner(cx, t, args)
            }
            Mode::If => {
                if let Some(new) = truth_read(h, self.ce, self.bit, args, 1) {
                    return Ok(Reply::Again(h.call(self.f, &new)));
                }
                self.inner(cx, t, args)
            }
            Mode::Held => {
                let r = self.inner(cx, t, args)?;
                if let Reply::Done(u) = r {
                    if let View::App { head, args: ua } = h.view(u) {
                        if matches!(h.view(head), View::Sym(s) if s == self.f) {
                            if let Some(l) = truth_or_lift(h, self.f, self.ce, self.bit, ua) {
                                return Ok(Reply::Again(l));
                            }
                        }
                    }
                }
                Ok(r)
            }
            Mode::Operator | Mode::Normal | Mode::Reduce => {
                let Some((&first, rest)) = args.split_first() else { return self.inner(cx, t, args) };
                if h.mask(first) & self.bit == 0 {
                    return self.inner(cx, t, args);
                }
                let Some((v, c)) = parts_of(h, self.ce, first) else {
                    // `normal` drops the conditions nested inside lists, rules and sums as well
                    if matches!(self.mode, Mode::Normal) {
                        let stripped = strip_all(h, self.ce, self.bit, first);
                        if stripped != first {
                            let mut a = vec![stripped];
                            a.extend_from_slice(rest);
                            return Ok(Reply::Again(h.call(self.f, &a)));
                        }
                    }
                    return self.inner(cx, t, args);
                };
                // refine and simplify: a condition the facts imply drops (refine(conditional_expression(1/(1 - a),
                // |a| < 1), |a| < 1) is 1/(1 - a)); a conditional truth value is decided by its condition
                if matches!(self.mode, Mode::Operator) && matches!(h.sym_name(self.f).as_str(), "refine" | "simplify" | "full_simplify") {
                    let mut a = vec![c];
                    a.extend_from_slice(rest);
                    let rc = cx.eval(h.call(self.f, &a))?;
                    if rc == h.sym_term(h.sym(names::TRUE)) {
                        let mut a = vec![v];
                        a.extend_from_slice(rest);
                        return Ok(Reply::Again(h.call(self.f, &a)));
                    }
                    if truth(h, v).is_some() {
                        if rc == h.sym_term(h.sym(names::FALSE)) {
                            return Ok(Reply::Done(rc));
                        }
                        if rc == c {
                            return Ok(Reply::Done(first));
                        }
                        return Ok(Reply::Again(h.call(self.ce, &[v, rc])));
                    }
                }
                Ok(Reply::Again(match self.mode {
                    Mode::Normal => v,
                    Mode::Reduce => {
                        let mut a = vec![h.call(h.sym("and"), &[v, c])];
                        a.extend_from_slice(rest);
                        h.call(self.f, &a)
                    }
                    _ => {
                        let mut a = vec![v];
                        a.extend_from_slice(rest);
                        let inner = h.call(self.f, &a);
                        let under = h.call(h.sym("assuming"), &[c, inner]);
                        h.call(self.ce, &[under, c])
                    }
                }))
            }
        }
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        if let Some(n) = &mut self.inner {
            n.trace(visit);
        }
    }

    fn reads_term(&self) -> bool {
        self.inner.as_ref().is_some_and(|n| n.reads_term())
    }

    fn curried(&self) -> bool {
        self.inner.as_ref().is_some_and(|n| n.curried())
    }

    fn packed<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Option<T<'h>>> {
        match &self.inner {
            Some(n) => n.packed(cx, x),
            None => Ok(None),
        }
    }
}

/// Wrap the natives of [`LATE_CONJOIN_HEADS`] (registered by a later library) so that they conjoin the condition
/// of a conditional formula; called by that library after it registers them.
pub fn install_late(k: &mut Kernel, h: &Heap) {
    let ce = h.sym(CONDITIONAL);
    let bit = h.mask(h.sym_term(ce));
    for &name in LATE_CONJOIN_HEADS {
        let f = h.sym(name);
        k.wrap_native(f, |old| Box::new(Lift { f, ce, bit, mode: Mode::Reduce, inner: Some(old) }));
    }
}

/// Register `conditional_expression` and the lifting of conditions through the heads of [`VALUE_HEADS`],
/// [`HELD_HEADS`], [`OPERATOR_HEADS`], `normal` and `reduce`. A head without a native gets an argument-head rule on
/// `conditional_expression` instead (semantics 6.5), which leaves its other rules alone.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let ce = h.sym(CONDITIONAL);
    let bit = h.mask(h.sym_term(ce));
    k.register(ce, Attributes::default(), Box::new(CondNative { stop: stop.clone() }));
    let all = VALUE_HEADS
        .iter()
        .map(|&n| (n, Mode::Value))
        .chain(HELD_HEADS.iter().map(|&n| (n, Mode::Held)))
        .chain(TRUTH_HEADS.iter().map(|&n| (n, Mode::Truth)))
        .chain(OPERATOR_HEADS.iter().map(|&n| (n, Mode::Operator)))
        .chain(CONJOIN_HEADS.iter().map(|&n| (n, Mode::Reduce)))
        .chain([("normal", Mode::Normal), ("if", Mode::If)]);
    for (name, mode) in all {
        let f = h.sym(name);
        let wrapped = k.wrap_native(f, |old| Box::new(Lift { f, ce, bit, mode, inner: Some(old) }));
        let truth_mode = matches!(mode, Mode::Truth);
        if wrapped || !matches!(mode, Mode::Value | Mode::Truth) {
            continue;
        }
        // f(pre___, c @ conditional_expression(_, _), post___) := the lifted call (a connective reads a conditional
        // truth value as its condition)
        let lhs = pat::call(h, f, vec![pat::seq0(0), pat::call(h, ce, vec![pat::var(1), pat::var(2)]), pat::seq0(3)]);
        let rhs: Rhs = Arc::new(move |b, h| {
            let mut args: Vec<T> = b.seq(0).to_vec();
            args.push(h.call(h.sym(CONDITIONAL), &[b.one(1), b.one(2)]));
            args.extend_from_slice(b.seq(3));
            let r = if truth_mode { truth_or_lift(h, f, ce, bit, &args) } else { lifted(h, f, ce, bit, &args) };
            r.unwrap_or_else(|| h.call(f, &args))
        });
        let _ = k.define_attached(h, lhs, rhs, ce);
    }
}
