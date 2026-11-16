//! The natives: the wrappers of `sum`, `product`, `r_solve`, `r_solve_value`, and this crate's own functions.
use crate::cx::{Budget, Method, Sx, Verdict};
use crate::{numsum, prod, rsolve, sumfn};
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_core::Raw;
use mc_eval::{NativeCx, Reply, Res};
use mc_poly::work::Meter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// `sum` or `product` with this crate's methods after the inner native's.
pub struct SumWrap {
    /// the list library's native
    pub inner: Box<dyn mc_eval::Native>,
    /// the stop flag
    pub stop: Arc<AtomicBool>,
    /// `product` (else `sum`)
    pub product: bool,
}

/// The options of a call: `(arguments without them, the option rules)`.
fn split_options<'h>(h: &'h mc_core::Heap, args: &[T<'h>]) -> (Vec<T<'h>>, Vec<T<'h>>) {
    if let Some((&last, rest)) = args.split_last() {
        if let View::App { head, args: rules } = h.view(last) {
            if head == h.sym_term(h.sym("options")) {
                return (rest.to_vec(), rules.to_vec());
            }
        }
    }
    (args.to_vec(), vec![])
}

/// Print a message of the function `name` (the engine's `message`).
fn message<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, text: &str) -> Res<()> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("message"), &[h.sym_term(h.sym(name)), h.string(text)]))?;
    Ok(())
}

/// The message for a numeric sum or product that gave no value, if `verdict` has a reason.
fn explain<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, verdict: Verdict) -> Res<()> {
    let what = if name == "n_product" || name == "product" { "product" } else { "sum" };
    match verdict {
        Verdict::Diverges => message(cx, name, &format!("the {what} does not converge (its terms do not fall fast enough), so it has no numerical value")),
        Verdict::Slow => message(cx, name, &format!("the {what} converges too slowly (or diverges): the terms fall about like 1/k and no estimate can be given")),
        Verdict::NotNumeric => message(cx, name, &format!("the {what}'s terms are not numbers (a symbol without a value): n_sum and n_product need numerical terms")),
        Verdict::Open => Ok(()),
    }
}

/// The options of `n_sum` and `n_product`: unknown names and values print a message (the call still runs).
fn read_options<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, opts: &[T<'h>]) -> Res<Method> {
    let h = cx.heap();
    let mut method = Method::Auto;
    for &r in opts {
        let View::App { args: &[k, v], .. } = h.view(r) else { continue };
        let View::Sym(key) = h.view(k) else { continue };
        let key = h.sym_name(key);
        match key.as_str() {
            "working_precision" | "digits" => {}
            "method" => {
                let text = match h.view(v) {
                    View::Sym(s) => h.sym_name(s),
                    _ => String::new(),
                };
                let norm: String = text.chars().filter(|c| *c != '_' && *c != '-').collect::<String>().to_lowercase();
                method = match norm.as_str() {
                    "" | "auto" | "automatic" => Method::Auto,
                    "eulermaclaurin" | "abelplana" | "integral" | "integraltest" | "tail" => Method::Tail,
                    "alternatingsigns" | "alternating" | "cohenvillegaszagier" => Method::Alternating,
                    "direct" | "terms" | "sequence" => Method::Direct,
                    _ => {
                        message(cx, name, &format!("unknown method {text}: use automatic, euler_maclaurin, alternating_signs or direct; the method is chosen automatically"))?;
                        Method::Auto
                    }
                };
            }
            "rigorous" => {
                if v == h.sym_term(h.sym("true")) {
                    message(cx, name, "rigorous: true is not available for numeric sums and products: the value is an estimate, not an enclosure")?;
                }
            }
            other => {
                message(cx, name, &format!("unknown option {other}: the options are working_precision, digits, method and rigorous"))?;
            }
        }
    }
    Ok(method)
}

/// The option `name` among `rules`.
fn option<'h>(h: &'h mc_core::Heap, rules: &[T<'h>], name: &str) -> Option<T<'h>> {
    let key = h.sym_term(h.sym(name));
    rules.iter().find_map(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } if k == key => Some(v),
        _ => None,
    })
}

/// Does `t` hold the subterm `x` (a head or an argument)?
fn contains_head<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The base q of the infinite q-Pochhammer symbols `q_pochhammer(a, q)` in `t`, when they all share one.
fn q_base<'h>(h: &'h mc_core::Heap, t: T<'h>) -> Option<T<'h>> {
    let mut found: Option<T<'h>> = None;
    let mut stack = vec![t];
    let qp = h.sym_term(h.sym("q_pochhammer"));
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == qp && args.len() == 2 {
                match found {
                    Some(q) if q != args[1] => return None,
                    _ => found = Some(args[1]),
                }
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    found
}

impl mc_eval::Native for SumWrap {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.product { "product" } else { "sum" };
        let (plain, opts) = split_options(h, args);
        if !opts.is_empty() {
            if let Some(r) = mc_poly::opts::assumptions_option(cx, name, args)? {
                return Ok(Reply::Done(r));
            }
            // generate_conditions: `false` gives the value alone, `true` (the default, see `conditioned`) the value
            // with the condition under which an infinite sum or product with parameters converges
            if opts.len() == 1 && option(h, &opts, "generate_conditions").is_some() {
                let wants = option(h, &opts, "generate_conditions") == Some(h.sym_term(h.sym("true")));
                let call = h.call(h.sym(name), &plain);
                if !wants {
                    let old = PLAIN.with(|p| p.replace(true));
                    let r = match self.compute(cx, call, &plain) {
                        Ok(Reply::Again(x)) => cx.eval(x).map(Reply::Done),
                        other => other,
                    };
                    PLAIN.with(|p| p.set(old));
                    return r;
                }
                return Ok(Reply::Again(call));
            }
            return self.inner.call(cx, t, args);
        }
        // the outermost sum or product of an evaluation: the value and its condition (nested calls, made by the
        // methods and by the condition's own tests, give plain values)
        if PLAIN.with(|p| p.get()) {
            return self.compute(cx, t, args);
        }
        PLAIN.with(|p| p.set(true));
        let r = self.compute(cx, t, args);
        let r = match r {
            Ok(Reply::Done(v)) => conditioned(cx, name, self.product, args, v),
            // a rewritten call (zeta(p) of the list library's rule): its value, then the condition
            Ok(Reply::Again(x)) => match cx.eval(x) {
                Ok(v) => conditioned(cx, name, self.product, args, v),
                Err(e) => Err(e),
            },
            other => other,
        };
        PLAIN.with(|p| p.set(false));
        r
    }
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.inner.trace(visit);
    }
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }
    fn curried(&self) -> bool {
        self.inner.curried()
    }
}

thread_local! {
    /// Set while a sum or product is computed: the sums and products evaluated inside (by the methods, or to test a
    /// condition) give their plain values, without conditions.
    static PLAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with the sums and products it evaluates giving plain values (no conditions): for the methods of other
/// functions that sum a series formally (generating functions).
pub(crate) fn plain<R>(f: impl FnOnce() -> R) -> R {
    let old = PLAIN.with(|p| p.replace(true));
    let r = f();
    PLAIN.with(|p| p.set(old));
    r
}

/// Is `t` free of every symbol but `k` and the constants (a sum or product without parameters)?
fn no_parameters<'h>(h: &'h mc_core::Heap, t: T<'h>, k: T<'h>) -> bool {
    let mut stack = vec![t];
    let mut seen = std::collections::HashSet::new();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(s) => {
                if u != k && !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio" | "im" | "inf" | "glaisher" | "khinchin") {
                    return false;
                }
            }
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
    true
}

/// The condition under which `sum(f, [k, lo, inf])` converges, when it depends on the parameters: that of the
/// convergence tests of `sum_convergence` (sum(1/n^a) needs re(a) > 1), else that of the ratio test, r < 1 for
/// r = lim |f(k + 1)/f(k)| when r depends on the parameters (sum(a^k) needs |a| < 1). `Ok(None)`: none found (the
/// sum converges, or nothing can be said); `Err(())` inside: the sum is proved divergent.
fn convergence<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>) -> Res<Result<Option<T<'h>>, ()>> {
    let h = cx.heap();
    let inf = h.sym_term(h.sym("inf"));
    let conv_name = h.sym_term(h.sym("sum_convergence"));
    let conv = cx.eval(h.call(h.sym("sum_convergence"), &[f, h.call(h.sym(names::LIST), &[k, lo, inf])]))?;
    if cx.is_true(conv) {
        return Ok(Ok(None));
    }
    if conv == h.sym_term(h.sym("false")) {
        return Ok(Err(()));
    }
    if !contains_head(h, conv, conv_name) && !contains_head(h, conv, k) {
        return Ok(Ok(Some(conv)));
    }
    let next = h.call(h.sym("add"), &[k, h.int(1)]);
    let shifted = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[k, next])]))?;
    let ratio = h.call(h.sym("simplify"), &[h.call(h.sym("abs"), &[h.call(h.sym("mul"), &[shifted, h.call(h.sym("pow"), &[f, h.int(-1)])])])]);
    let r = cx.eval(h.call(h.sym("limit"), &[ratio, h.call(h.sym(names::RULE), &[k, inf])]))?;
    let number = matches!(h.view(r), View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. });
    let found = !contains_head(h, r, h.sym_term(h.sym("limit"))) && !contains_head(h, r, k) && !number && r != inf;
    Ok(Ok(found.then(|| h.call(h.sym("less"), &[r, h.int(1)]))))
}

/// The value `v` of an infinite sum or product with parameters (`args` the arguments of the call) with the
/// condition under which it converges, as `integrate` gives its conditions: `conditional_expression(v, c)`; the
/// value alone when there is no condition or the assumptions in force imply it. A bilateral sum needs the
/// conditions of both halves. A product converges with the sum of its factors less 1; a product in q-Pochhammer
/// symbols for |q| < 1. A sum or product proved divergent stays unevaluated, with a message.
fn conditioned<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, product: bool, args: &[T<'h>], v: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let inf = h.sym_term(h.sym("inf"));
    let [f, spec] = args else { return Ok(Reply::Done(v)) };
    let (k, lo, hi) = match cx.as_list(*spec) {
        Some(&[k, lo, hi]) => (k, lo, hi),
        Some(&[k, hi]) => (k, h.int(1), hi),
        _ => return Ok(Reply::Done(v)),
    };
    let neg_inf = matches!(h.view(lo), View::App { head, args: &[c, i] } if head == h.sym_term(h.sym("mul")) && i == inf && c == h.int(-1));
    // the parameters are read from the value (the summand may name variables that have values, as a matrix `a`);
    // a list value (a sum of vectors or matrices) gets no condition
    if !matches!(h.view(k), View::Sym(_)) || (hi != inf && !neg_inf) || no_parameters(h, v, k) || cx.as_list(v).is_some() {
        return Ok(Reply::Done(v));
    }
    if contains_head(h, v, h.sym_term(h.sym(name))) || contains_head(h, v, h.sym_term(h.sym(names::FAILURE))) {
        return Ok(Reply::Done(v));
    }
    let mut conds = vec![];
    if product {
        if let Some(q) = q_base(h, v) {
            conds.push(h.call(h.sym("less"), &[h.call(h.sym("abs"), &[q]), h.int(1)]));
        }
    }
    if conds.is_empty() {
        let term = if product { cx.eval(h.call(h.sym("add"), &[*f, h.int(-1)]))? } else { *f };
        // the halves: k from lo to inf; for a lower end -inf, f(-k) from -hi (or 1 when the other half runs from 0)
        let mut halves = vec![];
        let mk = h.call(h.sym("mul"), &[h.int(-1), k]);
        let neg = cx.eval(h.call(h.sym("rewrite"), &[term, h.call(h.sym(names::RULE), &[k, mk])]))?;
        match (neg_inf, hi == inf) {
            (false, _) => halves.push((term, lo)),
            (true, true) => {
                halves.push((term, h.int(0)));
                halves.push((neg, h.int(1)));
            }
            (true, false) => halves.push((neg, cx.eval(h.call(h.sym("mul"), &[h.int(-1), hi]))?)),
        }
        for (g, start) in halves {
            match convergence(cx, g, k, start)? {
                Ok(Some(c)) => conds.push(c),
                Ok(None) => {}
                Err(()) => {
                    let what = if product { "product" } else { "sum" };
                    message(cx, name, &format!("the {what} does not converge"))?;
                    return Ok(Reply::Done(h.call(h.sym(name), args)));
                }
            }
        }
    }
    // a test that did not give a condition on numbers (a failure, a list inside) says nothing
    conds.retain(|&c| !contains_head(h, c, h.sym_term(h.sym(names::FAILURE))) && !contains_head(h, c, h.sym_term(h.sym(names::LIST))));
    if conds.is_empty() {
        return Ok(Reply::Done(v));
    }
    let cond = if conds.len() == 1 { conds[0] } else { h.call(h.sym("and"), &conds) };
    // a condition the assumptions in force imply is dropped; one that cannot hold is a divergent sum
    let implied = cx.eval(h.call(h.sym("refine"), &[cond]))?;
    if cx.is_true(implied) {
        return Ok(Reply::Done(v));
    }
    if implied == h.sym_term(h.sym("false")) {
        let what = if product { "product" } else { "sum" };
        message(cx, name, &format!("the {what} does not converge for any value of the parameters"))?;
        return Ok(Reply::Done(h.call(h.sym(name), args)));
    }
    Ok(Reply::Again(h.call(h.sym("conditional_expression"), &[v, cond])))
}

impl SumWrap {
    /// The value of `sum(args)` or `product(args)` without conditions.
    fn compute<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.product { "product" } else { "sum" };
        let [f, spec] = args else { return self.inner.call(cx, t, args) };
        let (f, spec) = (*f, *spec);
        let _scope = StopScope::enter(Some(&self.stop));
        // the indefinite sum of a non-polynomial: Gosper
        if !self.product && matches!(h.view(spec), View::Sym(_)) && spec != h.sym_term(h.sym("inf")) {
            let mut b = Budget::new(&self.stop, sumfn::WORK);
            let mut m = Meter::new(&mut b);
            let mut sx = Sx::new(cx, &self.stop);
            if let Some(v) = sumfn::indefinite(&mut sx, f, spec, &mut m)? {
                return Ok(Reply::Done(v));
            }
            return self.inner.call(cx, t, args);
        }
        // from -inf: this crate's methods (the list library's geometric sum read f(-inf) and gave x^-inf forms)
        let from_neg_inf = matches!(cx.as_list(spec), Some(&[_, lo, _]) if matches!(h.view(lo),
            View::App { head, args: &[c, i] } if head == h.sym_term(h.sym("mul")) && c == h.int(-1) && i == h.sym_term(h.sym("inf"))));
        let r = if from_neg_inf && !self.product { Reply::Decline } else { self.inner.call(cx, t, args)? };
        if !matches!(r, Reply::Decline) {
            if self.product {
                if let Reply::Done(v) = r {
                    // factorials raised to symbolic powers as gamma functions
                    let mut sx = Sx::new(cx, &self.stop);
                    return Ok(Reply::Done(factorial_to_gamma(&mut sx, v, false)?));
                }
            }
            return Ok(r);
        }
        // a telescoping indefinite product g(k + 1) / g(k)
        if self.product && matches!(h.view(spec), View::Sym(_)) {
            let mut sx = Sx::new(cx, &self.stop);
            if let Some(v) = crate::sumfn::telescoping(&mut sx, f, spec, true)? {
                return Ok(Reply::Done(v));
            }
            return Ok(Reply::Decline);
        }
        let Some(&[k, lo, hi]) = cx.as_list(spec).map(|s| match s {
            [k, b] => [*k, h.int(1), *b],
            [k, a, b] => [*k, *a, *b],
            _ => [h.int(0), h.int(0), h.int(0)],
        }).as_ref() else {
            return Ok(Reply::Decline);
        };
        if !matches!(h.view(k), View::Sym(_)) {
            return Ok(Reply::Decline);
        }
        let mut b = Budget::new(&self.stop, sumfn::WORK);
        let mut m = Meter::new(&mut b);
        let mut sx = Sx::new(cx, &self.stop);
        // an exact non-integer bound (3/2): no closed form of an integer-bounded sum applies (D23-18)
        if numsum::non_integer_bound(&mut sx, lo)? || numsum::non_integer_bound(&mut sx, hi)? {
            return Ok(Reply::Decline);
        }
        if numsum::inexact(&sx, f) || numsum::inexact(&sx, lo) || numsum::inexact(&sx, hi) {
            // balls of d digits (and no machine real): the sum at d digits
            if let Some(d) = numsum::ball_digits(&sx, &[f, lo, hi]) {
                if let Some(v) = numsum::numeric_digits(&mut sx, f, k, lo, hi, self.product, d)? {
                    return Ok(Reply::Done(v));
                }
            }
            if let Some(v) = numsum::numeric(&mut sx, f, k, lo, hi, self.product)? {
                return Ok(Reply::Done(v));
            }
            let verdict = sx.verdict;
            explain(sx.cx, name, verdict)?;
            return Ok(Reply::Decline);
        }
        let v = if self.product {
            let inf = sx.sym("inf");
            let hi = if hi == inf { None } else { Some(hi) };
            prod::product(&mut sx, f, k, lo, hi, &mut m)?
        } else {
            match sumfn::definite(&mut sx, f, k, lo, hi, &mut m)? {
                Some(v) => Some(v),
                // a sum from a small integer a: the sum from 0 or 1 (which has a closed form) less the first terms
                None => from_base(&mut sx, f, k, lo, hi)?,
            }
        };
        Ok(match v {
            Some(v) => Reply::Done(v),
            None => Reply::Decline,
        })
    }
}

/// `sum_{k=a}^{b} f` as `sum_{k=c}^{b} f` (c = 0 or 1, below a, the sum having a closed form) minus the terms
/// `f(c) .. f(a - 1)`, when those are finite.
fn from_base<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>) -> Res<Option<T<'h>>> {
    let Some(a) = sx.as_int(lo).filter(|a| (1..=40).contains(a)) else { return Ok(None) };
    if numsum::inexact(sx, f) || numsum::inexact(sx, hi) || sx.free(f, k) {
        return Ok(None);
    }
    for base in [1i64, 0] {
        if base >= a {
            continue;
        }
        let spec = sx.list(&[k, sx.int(base), hi]);
        let call = sx.call("sum", &[f, spec]);
        let r = sx.ev("time_constrained", &[call, sx.int(3)])?;
        if sx.contains_any(r, &["sum", "failure", "time_constrained", "indeterminate", "complex_infinity", "undefined"]) {
            continue;
        }
        let mut head = vec![];
        let mut ok = true;
        for j in base..a {
            let t = sx.subst(f, k, sx.int(j))?;
            if sx.bad(t) || sx.contains_any(t, &["undefined"]) {
                ok = false;
                break;
            }
            head.push(t);
        }
        if !ok {
            continue;
        }
        let h = sx.add(&head)?;
        let v = sx.sub(r, h)?;
        return Ok(Some(v));
    }
    Ok(None)
}

/// `t` with every `factorial(u)` raised to a symbolic power written `gamma(u + 1)`.
fn factorial_to_gamma<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, under: bool) -> Res<T<'h>> {
    let h = sx.h();
    match h.view(t) {
        View::App { head, args } => {
            let symbolic_power = head == sx.sym(names::POW) && args.len() == 2 && !matches!(h.view(args[1]), View::Int(_));
            let mut new = Vec::with_capacity(args.len());
            for (i, &a) in args.iter().enumerate() {
                new.push(factorial_to_gamma(sx, a, symbolic_power && i == 0)?);
            }
            if under && head == sx.sym("factorial") && new.len() == 1 {
                let u1 = sx.plus(new[0], 1)?;
                return sx.ev("gamma", &[u1]);
            }
            if new.as_slice() == args {
                return Ok(t);
            }
            sx.eval(h.app(head, &new))
        }
        _ => Ok(t),
    }
}

/// `r_solve` / `r_solve_value` with this crate's solver first.
pub struct RSolveWrap {
    /// the calculus crate's native
    pub inner: Box<dyn mc_eval::Native>,
    /// the stop flag
    pub stop: Arc<AtomicBool>,
    /// `r_solve_value`
    pub value: bool,
}

impl mc_eval::Native for RSolveWrap {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (plain, opts) = split_options(h, args);
        if option(h, &opts, "rsolve$inner").is_some() {
            return self.inner.call(cx, t, &plain);
        }
        let gen = option(h, &opts, "generated_parameters");
        let _scope = StopScope::enter(Some(&self.stop));
        let mut b = Budget::new(&self.stop, sumfn::WORK);
        let mut m = Meter::new(&mut b);
        let mut sx = Sx::new(cx, &self.stop);
        if let [eqs, target, n] = plain.as_slice() {
            if let Some(v) = rsolve::r_solve(&mut sx, *eqs, *target, *n, self.value, gen, &mut m)? {
                return Ok(Reply::Done(v));
            }
        }
        if !opts.is_empty() {
            return Ok(Reply::Decline);
        }
        self.inner.call(cx, t, args)
    }
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.inner.trace(visit);
    }
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }
    fn curried(&self) -> bool {
        self.inner.curried()
    }
}

/// This crate's own functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fun {
    /// `n_sum`
    NSum,
    /// `n_product`
    NProduct,
    /// `difference_delta`
    Delta,
    /// `discrete_shift`
    Shift,
    /// `discrete_ratio`
    Ratio,
    /// `generating_function`
    Gf,
    /// `sum_convergence`
    Conv,
    /// `recurrence_table`
    Table,
}

/// A native of this crate.
pub struct Native {
    /// which function
    pub f: Fun,
    /// the stop flag
    pub stop: Arc<AtomicBool>,
}

/// The head of a watched summand, [`Watch`].
pub(crate) const WATCH: &str = "numsum$watch";

/// The running monitored `n_sum` and `n_product` calls: number, evaluation monitor, and the failure it gave. The
/// monitor is kept here, not in the summand, which the methods substitute into (a closure `n => ...` with the
/// index's name would lose its parameter).
static WATCHED: std::sync::Mutex<Vec<(u64, Raw, Option<Raw>)>> = std::sync::Mutex::new(Vec::new());

/// The number of the next monitored call.
static WATCH_NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// `numsum$watch(id, k, e)`: the summand `e` of the monitored call number `id` at the index value `k`. With a
/// numeric `k` the call's evaluation monitor is called with it first (every evaluation of a term, by whichever
/// method `n_sum` uses: exact first terms, acceleration, tails); a failure it gives is recorded for the call and is
/// the value. With a symbolic `k` (the closed-form and convergence tests) the term is `e` itself.
pub(crate) struct Watch;

impl mc_eval::Native for Watch {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[id, k, e] = args else { return Ok(Reply::Decline) };
        let (View::Int(id), true) = (h.view(id), mc_arith::Arith::new(h).num(k).is_some()) else { return Ok(Reply::Done(e)) };
        let m = WATCHED.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|w| w.0 == id as u64).map(|w| w.1);
        let Some(m) = m else { return Ok(Reply::Done(e)) };
        let v = cx.eval(h.app(m.get(h), &[k]))?;
        let failed = h.packed(v).is_none() && matches!(h.view(v), View::App { head, .. } if head == h.sym_term(h.sym(names::FAILURE)));
        if failed {
            if let Some(w) = WATCHED.lock().unwrap_or_else(|p| p.into_inner()).iter_mut().find(|w| w.0 == id as u64) {
                w.2.get_or_insert(Raw::new(v));
            }
            return Ok(Reply::Done(v));
        }
        Ok(Reply::Done(e))
    }
}

/// The failure the evaluation monitor of call `id` gave; the call leaves the record.
fn watch_end<'h>(h: &'h mc_core::Heap, id: u64) -> Option<T<'h>> {
    let mut all = WATCHED.lock().unwrap_or_else(|p| p.into_inner());
    let i = all.iter().position(|w| w.0 == id)?;
    let (_, _, f) = all.remove(i);
    f.map(|r| r.get(h))
}

impl mc_eval::Native for Native {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if !matches!(self.f, Fun::NSum | Fun::NProduct) {
            return self.run(cx, t, args, None);
        }
        // `evaluation_monitor: f` (`mc_numfn::monitor`): called with the index of every term evaluated
        let (args, mons) = match mc_numfn::take_monitors(cx, args, &["evaluation_monitor"]) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let h = cx.heap();
        let Some(m) = mons.evaluation_fn(h) else { return self.run(cx, t, &args, None) };
        let id = WATCH_NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        WATCHED.lock().unwrap_or_else(|p| p.into_inner()).push((id, Raw::new(m), None));
        let r = self.run(cx, t, &args, Some(id));
        let failed = watch_end(h, id);
        let r = r?;
        Ok(match failed {
            Some(f) => Reply::Done(f),
            None => r,
        })
    }
}

impl Native {
    /// The call; `watch`: the number of a monitored call (the summand is watched).
    fn run<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>], watch: Option<u64>) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, opts) = split_options(h, args);
        let _scope = StopScope::enter(Some(&self.stop));
        let mut sx = Sx::new(cx, &self.stop);
        let v = match self.f {
            Fun::NSum | Fun::NProduct => {
                let name = if self.f == Fun::NSum { "n_sum" } else { "n_product" };
                // several iterators: the first outermost; the inner sum stays unevaluated until the outer index has a value
                // (evaluated with the index still symbolic it would complain that its terms are not numbers)
                let args: Vec<T<'h>> = if args.len() > 2 {
                    let mut inner = vec![args[0]];
                    inner.extend_from_slice(&args[2..]);
                    vec![h.call(h.sym(name), &inner), args[1]]
                } else {
                    args
                };
                let [f, spec] = args.as_slice() else { return Ok(Reply::Decline) };
                let Some(s) = sx.as_list(*spec) else { return Ok(Reply::Decline) };
                sx.method = read_options(sx.cx, name, &opts)?;
                let (k, lo, hi) = match *s {
                    [k, b] => (k, h.int(1), b),
                    [k, a, b] => (k, a, b),
                    [k, a, b, st] => {
                        // k = a + st j, j = 0, 1, ...
                        let j = sx.sym("numsum$j");
                        let sj = sx.mul(&[st, j])?;
                        let kj = sx.add(&[a, sj])?;
                        let g = sx.subst(*f, k, kj)?;
                        let top = if b == sx.sym("inf") {
                            b
                        } else {
                            let d = sx.sub(b, a)?;
                            let q = sx.div(d, st)?;
                            sx.ev("floor", &[q])?
                        };
                        let sp = sx.list(&[j, sx.int(0), top]);
                        return Ok(Reply::Again(h.call(h.sym(name), &[g, sp])));
                    }
                    _ => return Ok(Reply::Decline),
                };
                // a monitored summand: every evaluation of a term at a numeric index calls the monitor (`Watch`)
                let watched;
                let f = match watch {
                    Some(id) => {
                        watched = h.call(h.sym(WATCH), &[h.int(id as i64), k, *f]);
                        &watched
                    }
                    None => f,
                };
                // working_precision: d (or digits: d): the sum at d digits
                let digits = option(h, &opts, "working_precision").or_else(|| option(h, &opts, "digits")).and_then(|v| match h.view(v) {
                    View::Int(d @ 1..=100_000) => Some(d as u64),
                    View::Real(d) if d > 0.0 && d <= 1e5 => Some(d.ceil() as u64),
                    _ => None,
                });
                if let Some(d) = digits {
                    // an infinite product with a closed form (Weierstrass, gamma and sine products): the closed form at d digits
                    if self.f == Fun::NProduct && hi == sx.sym("inf") {
                        let spec = sx.list(&[k, lo, hi]);
                        let exact = sx.ev("product", &[*f, spec])?;
                        // only the Weierstrass products (closed form with Euler's constant): a closed form through complex
                        // gamma values of a real product would give a complex enclosure, and the numeric path is as fast
                        let ok = sx.free(exact, sx.sym("product")) && !sx.free(exact, sx.sym("euler_gamma"));
                        if ok {
                            return Ok(Reply::Done(sx.ev("n", &[exact, sx.int(d as i64)])?));
                        }
                    }
                    if let Some(v) = numsum::numeric_digits(&mut sx, *f, k, lo, hi, self.f == Fun::NProduct, d)? {
                        return Ok(Reply::Done(v));
                    }
                }
                let lo = sx.ev("n", &[lo])?;
                let hi = if hi == sx.sym("inf") { hi } else { sx.ev("n", &[hi])? };
                let fl = *f;
                let r = numsum::numeric(&mut sx, fl, k, lo, hi, self.f == Fun::NProduct)?;
                if r.is_none() {
                    let verdict = sx.verdict;
                    explain(sx.cx, name, verdict)?;
                }
                // more than machine precision was asked and only a machine value could be had: none is given (a value with
                // 16 correct digits printed as if it had the 30 asked for is a wrong answer, B23-24)
                if let (Some(d), Some(_)) = (digits, r) {
                    if d > 16 {
                        let what = if self.f == Fun::NSum { "sum" } else { "product" };
                        message(sx.cx, name, &format!("the {what} could not be evaluated to {d} digits (only to machine precision), so no value is given"))?;
                        return Ok(Reply::Decline);
                    }
                }
                r
            }
            Fun::Gf => {
                let [f, n, z] = args.as_slice() else { return Ok(Reply::Decline) };
                let (f, n, z) = (*f, *n, *z);
                // lists, equations and rules thread
                for head in [names::LIST, "equal", names::RULE] {
                    if let Some(xs) = sx.args_of(f, head) {
                        let parts: Vec<T<'h>> = xs.iter().map(|&x| h.call(h.sym("generating_function"), &[x, n, z])).collect();
                        return Ok(Reply::Again(h.call(h.sym(head), &parts)));
                    }
                }
                // several variables: the first innermost
                if let (Some(ns), Some(zs)) = (sx.as_list(n), sx.as_list(z)) {
                    if ns.len() != zs.len() || ns.is_empty() {
                        return Ok(Reply::Decline);
                    }
                    let mut g = f;
                    for (&ni, &zi) in ns.iter().zip(zs) {
                        g = h.call(h.sym("generating_function"), &[g, ni, zi]);
                    }
                    return Ok(Reply::Again(g));
                }
                if !matches!(h.view(n), View::Sym(_)) {
                    return Ok(Reply::Decline);
                }
                let v = crate::gf::gf(&mut sx, f, n, z)?;
                // an unevaluated generating function of the same arguments is no answer
                match v {
                    Some(v) if v != h.call(h.sym("generating_function"), &[f, n, z]) => {
                        // generate_conditions: true: the series converges for |z| < 1/r, r the limit of |a(n+1)/a(n)|
                        // (the ratio test); no condition when r is 0 (an entire function) or the limit is not found
                        let wants = option(h, &opts, "generate_conditions") == Some(h.sym_term(h.sym("true")));
                        if wants {
                            let next = sx.call("add", &[n, sx.int(1)]);
                            let shifted = sx.ev("rewrite", &[f, sx.rule(n, next)])?;
                            let ratio = sx.call("mul", &[shifted, sx.call("pow", &[f, sx.int(-1)])]);
                            let q = sx.ev("abs", &[ratio])?;
                            let r = sx.ev("limit", &[q, sx.rule(n, sx.sym("inf"))])?;
                            let found = sx.free(r, sx.sym("limit")) && sx.free(r, n) && r != sx.int(0) && r != sx.sym("inf");
                            if found {
                                let bound = sx.call("pow", &[r, sx.int(-1)]);
                                let cond = sx.call("less", &[sx.call("abs", &[z]), bound]);
                                Some(sx.ev("conditional_expression", &[v, cond])?)
                            } else {
                                Some(v)
                            }
                        } else {
                            Some(v)
                        }
                    }
                    _ => None,
                }
            }
            Fun::Conv => {
                let [f, spec] = args.as_slice() else { return Ok(Reply::Decline) };
                let (n, lo) = match sx.as_list(*spec) {
                    Some(&[n, a, b]) if b == sx.sym("inf") => (n, sx.as_int(a)),
                    Some(&[n]) => (n, None),
                    None => (*spec, None),
                    _ => return Ok(Reply::Decline),
                };
                if !matches!(h.view(n), View::Sym(_)) {
                    return Ok(Reply::Decline);
                }
                let mut b = Budget::new(&self.stop, sumfn::WORK);
                let mut m = Meter::new(&mut b);
                crate::conv::convergence(&mut sx, *f, n, lo, &mut m)?
            }
            Fun::Table => {
                let [eqs, a, spec] = args.as_slice() else { return Ok(Reply::Decline) };
                let deps = option(h, &opts, "dependent_variables");
                let exact = match rsolve::table(&mut sx, *eqs, *a, *spec)? {
                    // a system of recurrences in several sequences
                    None if deps.is_some() || sx.as_list(*a).is_some() => rsolve::table_system(&mut sx, *eqs, *a, *spec, deps)?,
                    v => v,
                };
                // working_precision: d (an integer, or machine_precision): the values, computed exactly, rounded to d digits
                match (exact, option(h, &opts, "working_precision")) {
                    (Some(v), Some(wp)) => match h.view(wp) {
                        View::Int(d @ 1..=100_000) => Some(sx.ev("n", &[v, sx.int(d)])?),
                        View::Sym(_) | View::Real(_) => Some(sx.ev("n", &[v])?),
                        _ => Some(v),
                    },
                    (v, _) => v,
                }
            }
            Fun::Delta | Fun::Shift | Fun::Ratio => {
                let Some((&f, specs)) = args.split_first() else { return Ok(Reply::Decline) };
                if specs.is_empty() {
                    return Ok(Reply::Decline);
                }
                let mut g = f;
                for &s in specs {
                    let Some(v) = difference(&mut sx, self.f, g, s)? else { return Ok(Reply::Decline) };
                    g = v;
                }
                Some(g)
            }
        };
        Ok(match v {
            Some(v) => Reply::Done(v),
            None => Reply::Decline,
        })
    }
}

/// One step of `difference_delta`, `discrete_shift` or `discrete_ratio` for the iterator `spec`.
fn difference<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: Fun, g: T<'h>, spec: T<'h>) -> Res<Option<T<'h>>> {
    let h = sx.h();
    let (k, times, step) = match h.view(spec) {
        View::Sym(_) => (spec, h.int(1), h.int(1)),
        _ => match sx.as_list(spec) {
            Some(&[k]) => (k, h.int(1), h.int(1)),
            Some(&[k, m]) => (k, m, h.int(1)),
            Some(&[k, m, s]) => (k, m, s),
            _ => return Ok(None),
        },
    };
    if !matches!(h.view(k), View::Sym(_)) {
        return Ok(None);
    }
    if f == Fun::Shift {
        let ms = sx.mul(&[times, step])?;
        return Ok(Some(shifted(sx, g, k, ms)?));
    }
    // a constant ratio: f (r^h - 1)^m or f r^(h m), for any m
    if let Some(r) = crate::hyper::ratio(sx, g, k)? {
        let r = sx.ev("simplify", &[r])?;
        if sx.free(r, k) && !sx.free(g, k) {
            let rh = sx.pow(r, step)?;
            let v = if f == Fun::Delta {
                let d = sx.plus(rh, -1)?;
                let dm = sx.pow(d, times)?;
                sx.mul(&[g, dm])?
            } else if times == sx.int(1) {
                rh
            } else if sx.as_int(times).is_some_and(|t| t > 1) {
                // the iterated ratio of a geometric term is 1 after the first
                sx.int(1)
            } else {
                return Ok(None);
            };
            return Ok(Some(sumfn::tidy(sx, v)?));
        }
    }
    let Some(times) = sx.as_int(times).filter(|&t| (0..=64).contains(&t)) else { return Ok(None) };
    let mut g = g;
    for _ in 0..times {
        g = if f == Fun::Delta { delta(sx, g, k, step)? } else { ratio(sx, g, k, step)? };
    }
    Ok(Some(g))
}

/// `g` with `k` replaced by `k + s`, shifted values of known functions reduced; sums, products and limits over
/// bounds that move with `k` shift their summand instead.
fn shifted<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>, k: T<'h>, s: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    if let View::App { head, args } = h.view(g) {
        let name = match h.view(head) {
            View::Sym(x) => h.sym_name(x),
            _ => String::new(),
        };
        if (name == "sum" || name == "product") && args.len() == 2 {
            let (body, spec) = (args[0], args[1]);
            let var = match h.view(spec) {
                View::Sym(_) => Some(spec),
                _ => sx.as_list(spec).and_then(|l| l.first().copied()),
            };
            if let Some(j) = var {
                // k bound by a definite sum: the sum does not depend on it (Q19-12); an indefinite sum in k moves
                if j == k && spec != k {
                    return Ok(g);
                }
                // bounds that do not all move by s with k: replaced in the bounds (product(f(i), [i, m, n]) shifted in
                // n is product(f(i), [i, m, 1 + n]))
                if !sx.free(spec, k) && j != k {
                    let bounds: Vec<T<'h>> = sx.as_list(spec).map(|l| l[1..].to_vec()).unwrap_or_default();
                    let ks = sx.add(&[k, s])?;
                    let mut uniform = !bounds.is_empty();
                    for &b in &bounds {
                        let moved = sx.subst(b, k, ks)?;
                        let d = sx.sub(moved, b)?;
                        let ds = sx.sub(d, s)?;
                        let d = sx.ev("simplify", &[ds])?;
                        if d != sx.int(0) {
                            uniform = false;
                        }
                    }
                    if !uniform {
                        let nb = sx.subst(body, k, ks)?;
                        let ns = sx.subst(spec, k, ks)?;
                        return sx.eval(h.app(head, &[nb, ns]));
                    }
                }
                if !sx.free(spec, k) || j == k {
                    // the summation variable moves with k (the body shifted, the bounds kept)
                    let js = sx.add(&[j, s])?;
                    let nb = sx.subst(body, j, js)?;
                    let nb = if j == k {
                        nb
                    } else {
                        let ks = sx.add(&[k, s])?;
                        sx.subst(nb, k, ks)?
                    };
                    return sx.eval(h.app(head, &[nb, spec]));
                }
            }
        }
        // an integral in k: indefinite, the antiderivative at k + s (the integrand shifted, a constant apart);
        // definite with bounds free of k, a constant
        if name == "integrate" && args.len() == 2 {
            if args[1] == k {
                let ks = sx.add(&[k, s])?;
                let nb = sx.subst(args[0], k, ks)?;
                return sx.eval(h.app(head, &[nb, k]));
            }
            if let Some(l) = sx.as_list(args[1]) {
                if l.first() == Some(&k) && l[1..].iter().all(|&b| sx.free(b, k)) {
                    return Ok(g);
                }
            }
        }
        if name == "limit" && args.len() == 2 {
            // k is the limit's own variable: bound, so the limit does not depend on it
            if let Some(&[x, p]) = sx.args_of(args[1], names::RULE) {
                if x == k && sx.free(p, k) {
                    return Ok(g);
                }
            }
            // a point p(k) that moves with k: the limit of F(k + s, x) at p(k + s); when p(k + s) = p(k) + s, written
            // at the same point as the limit of F(k + s, x + s)
            if let Some(&[x, p]) = sx.args_of(args[1], names::RULE) {
                if !sx.free(p, k) && x != k {
                    let ks = sx.add(&[k, s])?;
                    let moved = sx.subst(p, k, ks)?;
                    let d = sx.sub(moved, p)?;
                    let d = sx.sub(d, s)?;
                    let d = sx.ev("simplify", &[d])?;
                    let body = sx.subst(args[0], k, ks)?;
                    if d == sx.int(0) {
                        let xs = sx.add(&[x, s])?;
                        let nb = sx.subst(args[0], x, xs)?;
                        let nb = sx.subst(nb, k, ks)?;
                        return sx.eval(h.app(head, &[nb, args[1]]));
                    }
                    let to = sx.rule(x, moved);
                    return sx.eval(h.app(head, &[body, to]));
                }
            }
        }
    }
    let ks = sx.add(&[k, s])?;
    let g1 = sx.subst(g, k, ks)?;
    let r = crate::shift::reduce(sx, g1, k)?;
    if r == g1 {
        return Ok(g1);
    }
    poly_part(sx, r, k)
}

/// A product with its rational factors in `k` collected: expanded when they form a polynomial, factored otherwise.
fn poly_part<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if head != sx.sym(names::MUL) {
        return Ok(t);
    }
    let mut rational = vec![];
    let mut other = vec![];
    for &a in args {
        if crate::hyper::rational_in(sx, a, k)? {
            rational.push(a);
        } else {
            other.push(a);
        }
    }
    if rational.is_empty() || other.is_empty() {
        return Ok(t);
    }
    let r = sx.mul(&rational)?;
    let r = if sumfn::is_polynomial(sx, r, k)? { sx.ev("expand", &[r])? } else { sx.ev("factor", &[r])? };
    let o = sx.mul(&other)?;
    sx.mul(&[r, o])
}

/// `t` with powers `b^(e + c)` (c an integer constant) written `b^c b^e`, so that common factors show.
fn split_offsets<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(split_offsets(sx, a)?);
    }
    if head == sx.sym(names::POW) && new.len() == 2 && sx.as_int(new[1]).is_none() {
        let (e, c) = crate::shift::offset(sx, new[1])?;
        if c != 0 {
            let a = sx.pow(new[0], e)?;
            let b = sx.pow(new[0], sx.int(c))?;
            return sx.mul(&[a, b]);
        }
    }
    if new.as_slice() == args {
        return Ok(t);
    }
    sx.eval(h.app(head, &new))
}

/// One forward difference `g(k + h) - g(k)`.
fn delta<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>, k: T<'h>, step: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    // the difference of an indefinite sum is its summand; of a falling factorial n factorial_power(k, n - 1)
    if step == sx.int(1) {
        if let Some(&[body, spec]) = sx.args_of(g, "sum") {
            if spec == k {
                return Ok(body);
            }
        }
        if let Some(&[x, n]) = sx.args_of(g, "factorial_power") {
            if x == k {
                let n1 = sx.plus(n, -1)?;
                let fp = sx.ev("factorial_power", &[k, n1])?;
                return sx.mul(&[n, fp]);
            }
        }
        if let View::App { head, args } = h.view(g) {
            if head == sx.sym(names::MUL) {
                // c factorial_power(k, n)
                let fp: Vec<T<'h>> = args.iter().copied().filter(|&a| sx.args_of(a, "factorial_power").is_some_and(|x| x[0] == k)).collect();
                let rest: Vec<T<'h>> = args.iter().copied().filter(|a| !fp.contains(a)).collect();
                if fp.len() == 1 && rest.iter().all(|&r| sx.free(r, k)) {
                    let d = delta(sx, fp[0], k, step)?;
                    let mut fs = rest.clone();
                    fs.push(d);
                    return sx.mul(&fs);
                }
            }
        }
    }
    let ks = sx.add(&[k, step])?;
    let g1 = sx.subst(g, k, ks)?;
    let g1 = crate::shift::reduce(sx, g1, k)?;
    let d = sx.sub(g1, g)?;
    let s = split_offsets(sx, d)?;
    let t = sx.ev("together", &[s])?;
    let fa = sx.ev("factor", &[t])?;
    let e = sx.ev("expand", &[d])?;
    let fe = sx.ev("factor", &[e])?;
    let mut cands = vec![fa, fe, e];
    cands.retain(|&c| !sx.bad(c));
    if cands.is_empty() {
        return Ok(d);
    }
    Ok(sx.smallest(&cands))
}

/// The ratio `g(k + h) / g(k)`: the term ratio of a hypergeometric `g` (h a positive integer), else the shifted
/// values reduced.
fn ratio<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>, k: T<'h>, step: T<'h>) -> Res<T<'h>> {
    if let Some(hs) = sx.as_int(step).filter(|&s| (1..=16).contains(&s)) {
        if let Some(r) = crate::hyper::ratio(sx, g, k)? {
            let mut fs = vec![];
            for j in 0..hs {
                let kj = sx.plus(k, j)?;
                fs.push(sx.subst(r, k, kj)?);
            }
            let p = sx.mul(&fs)?;
            let c1 = sx.ev("factor", &[p])?;
            let c2 = sx.ev("simplify", &[p])?;
            let mut cands = vec![c1, c2];
            cands.retain(|&c| !sx.bad(c));
            if !cands.is_empty() {
                return Ok(sx.smallest(&cands));
            }
        }
    }
    let ks = sx.add(&[k, step])?;
    let g1 = sx.subst(g, k, ks)?;
    let g1 = crate::shift::reduce(sx, g1, k)?;
    let q = sx.div(g1, g)?;
    sumfn::tidy(sx, q)
}
