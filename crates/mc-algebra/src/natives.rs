//! The natives: `minimal_polynomial`, `to_number_field`, `root_sum`, `to_radicals`, and the `extension:` option of
//! `factor` (a wrapper around `mc-poly`'s native).
//!
//! An interrupt or fuel exhaustion returns the evaluator's abort; a result beyond the memory limit is
//! `failure(limit, memory)`; a malformed call is `failure(argument, name, message)`.
use crate::algnum::{self, adjoin, AlgNum};
use crate::convert::{self, poly_term, to_alg, Memo};
use crate::cx::{Env, Out, Tick};
use crate::embed;
use crate::field::{Elem, NumberField};
use crate::kpoly::{self, KPoly};
use crate::qpoly::{self, q};
use crate::rootsum;
use mc_api::{names, Store, View, T};
use mc_arith::num::{Int, Nat, Rat, StopScope};
use mc_arith::Num;
use mc_core::{Heap, Raw};
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_poly::work::{Halt, Meter, WorkResult};

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The functions of this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlgFn {
    MinimalPolynomial,
    ToNumberField,
    RootReduce,
    AlgebraicNumberNorm,
    AlgebraicNumberTrace,
    RootSum,
    ToRadicals,
    CountRoots,
}

/// A function of this crate as a native.
pub(crate) struct AlgNative {
    pub f: AlgFn,
    pub name: &'static str,
    pub stop: Arc<AtomicBool>,
}

/// Run `f` with an environment and a metered budget, and turn its outcome into a reply.
fn run<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    name: &'static str,
    f: impl FnOnce(&Env<'h>, &mut Meter) -> WorkResult<Out<'h>>,
) -> Res<Reply<'h>> {
    let h: &'h Heap = cx.heap();
    let env = Env::new(h, stop.clone(), name);
    let _scope = StopScope::enter(Some(stop));
    let mut tick = Tick { cx, stop, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        f(&env, &mut m)
    };
    match r {
        Ok(Out::Done(t)) => Ok(Reply::Done(t)),
        Ok(Out::Again(t)) => Ok(Reply::Again(t)),
        Ok(Out::Decline) => Ok(Reply::Decline),
        Ok(Out::Numeric(t, _)) => Ok(Reply::Again(t)),
        Err(Halt::Stopped) => match tick.abort {
            Some(a) => Err(a),
            None => Ok(Reply::Done(env.call(names::FAILURE, &[env.sym(names::failure::INTERRUPTED)]))),
        },
        Err(Halt::Memory) => {
            let tags = [env.sym(names::failure::LIMIT), env.sym(names::failure::MEMORY)];
            Ok(Reply::Done(env.call(names::FAILURE, &tags)))
        }
    }
}

impl Native for AlgNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if self.f == AlgFn::ToRadicals {
            return to_radicals(cx, &self.stop, args);
        }
        let f = self.f;
        if f == AlgFn::RootSum {
            // a numeric sum is evaluated here, so that a vanishing imaginary part can be dropped
            let mut numeric = None;
            let r = run(cx, &self.stop, self.name, |env, m| match rootsum::root_sum(env, args, m)? {
                Out::Numeric(t, real) => {
                    numeric = Some((t, real));
                    Ok(Out::Decline)
                }
                o => Ok(o),
            })?;
            let Some((t, real)) = numeric else { return Ok(r) };
            let v = cx.eval(t)?;
            if real {
                let env = Env::new(cx.heap(), self.stop.clone(), self.name);
                if let Some(re) = rootsum::real_part_if_zero_imaginary(&env, v) {
                    return Ok(Reply::Done(re));
                }
            }
            return Ok(Reply::Done(v));
        }
        run(cx, &self.stop, self.name, |env, m| match f {
            AlgFn::MinimalPolynomial => minimal_polynomial(env, args, m),
            AlgFn::ToNumberField => to_number_field(env, args, m),
            AlgFn::RootReduce => root_reduce(env, args, m),
            AlgFn::AlgebraicNumberNorm => norm_or_trace(env, args, true, m),
            AlgFn::AlgebraicNumberTrace => norm_or_trace(env, args, false, m),
            AlgFn::RootSum => rootsum::root_sum(env, args, m),
            AlgFn::ToRadicals => Ok(Out::Decline),
            AlgFn::CountRoots => crate::countroots::count_roots(env, args, m),
        })
    }
    fn reads_term(&self) -> bool {
        false
    }
}

// ------------------------------------------------------------------------------------------- minimal_polynomial

/// `minimal_polynomial(a, x)`, `minimal_polynomial(a)` (a function).
///
/// `minimal_polynomial(a, x, extension: b)`: the characteristic polynomial of `a` as an element of `K = Q(b)`,
/// which is the minimal polynomial `p` of `a` over Q raised to `[K : Q(a)]` (Cohen, *A Course in Computational
/// Algebraic Number Theory*, proposition 4.3.2), expanded.
fn minimal_polynomial<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if let Some((&[a, x], b)) = extension_option(env.h(), args) {
        if !matches!(env.h().view(x), View::Sym(_)) {
            return Ok(Out::Decline);
        }
        let mut memo = Memo::default();
        let (Some(xa), Some(xb)) = (to_alg(env, a, &mut memo, m)?, to_alg(env, b, &mut memo, m)?) else {
            return Ok(Out::Decline);
        };
        let (Some(sa), Some(sb)) = (algnum::simple(&xa, m)?, algnum::simple(&xb, m)?) else { return Ok(Out::Decline) };
        let Some(t) = adjoin(&sb.k, &sa.k, m)? else { return Ok(Out::Decline) };
        let mp = xa.minpoly(m)?;
        let (n, dk) = (mp.len() - 1, sb.k.degree());
        if t.field.degree() != dk || dk % n != 0 {
            return Ok(env.failure("not_in_field", "the number is not in the extension"));
        }
        let mut cp = qpoly::of_i64(&[1]);
        for _ in 0..dk / n {
            cp = qpoly::mul(&cp, &mp, m)?;
        }
        return Ok(Out::Done(poly_term(env, &qpoly::from_ints(&qpoly::primitive(&cp)), x)));
    }
    let (a, x) = match args {
        [a, x] => (*a, Some(*x)),
        [a] => (*a, None),
        _ => return Ok(env.arg_failure("minimal_polynomial takes an algebraic number and a variable")),
    };
    if let Some(x) = x {
        if !matches!(env.h().view(x), View::Sym(_)) {
            return Ok(Out::Decline);
        }
    }
    // composed sums and products first (item F2), the number field of the expression otherwise
    let (mp, z) = match crate::minpoly::fast_minpoly(env, a, m)? {
        Some(r) => r,
        None => {
            let mut memo = Memo::default();
            let Some(alg) = to_alg(env, a, &mut memo, m)? else { return Ok(Out::Decline) };
            let mp = alg.minpoly(m)?;
            let Some(z) = alg.enclose(256, m)? else { return Ok(Out::Decline) };
            (mp, z)
        }
    };
    // the check: the polynomial vanishes on an enclosure of the number
    if !embed::eval_ball(&mp, &z, 300).contains_zero() {
        return Ok(env.failure("internal", "the minimal polynomial does not vanish at the number"));
    }
    // the function form uses a slot: &(p(&1))
    let xv = x.unwrap_or_else(|| env.sym("&1"));
    let body = poly_term(env, &qpoly::from_ints(&qpoly::primitive(&mp)), xv);
    Ok(Out::Done(if x.is_some() { body } else { env.function(xv, body) }))
}

/// The coefficients `[c0, c1, ...]` with `a = c0 + c1 theta + ...` (the degree of theta many); `Err` with the reply
/// when a number is not algebraic or `a` does not lie in the field of `theta`.
fn field_coefficients<'h>(
    env: &Env<'h>,
    a: T<'h>,
    theta: T<'h>,
    memo: &mut Memo<'h>,
    m: &mut Meter,
) -> WorkResult<Result<T<'h>, Out<'h>>> {
    let (Some(x), Some(th)) = (to_alg(env, a, memo, m)?, to_alg(env, theta, memo, m)?) else {
        return Ok(Err(Out::Decline));
    };
    let (Some(x), Some(th)) = (algnum::simple(&x, m)?, algnum::simple(&th, m)?) else { return Ok(Err(Out::Decline)) };
    let Some(tw) = adjoin(&th.k, &x.k, m)? else { return Ok(Err(Out::Decline)) };
    if tw.shift.is_some() {
        return Ok(Err(env.failure("not_in_field", "the number does not lie in the field of the generator")));
    }
    // x itself (x.a in its field), not its field's generator, written in the generator's field
    let d = th.k.degree();
    let e = tw.field.compose(&x.a, &tw.beta);
    Ok(Ok(coefficient_list(env, &e, d)))
}

/// `add`, `mul` and `pow` of algebraic numbers `algebraic_number(theta, coefficients)` of one field: the result in
/// the same field (the sum, product or integer power computed over the field of the generator).
pub(crate) struct AlgebraicArithmetic {
    pub inner: Box<dyn Native>,
    pub stop: Arc<AtomicBool>,
}

impl Native for AlgebraicArithmetic {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h: &'h Heap = cx.heap();
        let an = h.sym_term(h.sym("algebraic_number"));
        let gens: Vec<T<'h>> = args
            .iter()
            .filter_map(|&a| match h.view(a) {
                View::App { head, args: &[theta, _] } if head == an => Some(theta),
                _ => None,
            })
            .collect();
        let Some(&theta) = gens.first() else { return self.inner.call(cx, t, args) };
        if gens.iter().any(|&g| g != theta) {
            return self.inner.call(cx, t, args);
        }
        // an integer power only
        if let View::App { head, args: &[_, e] } = h.view(t) {
            if head == h.sym_term(h.sym("pow")) && !matches!(h.view(e), View::Int(_)) {
                return self.inner.call(cx, t, args);
            }
        }
        // the generator evaluated (algebraic_number keeps sqrt(2) as written), and the sum, product or power over it
        let theta_ev = cx.eval(theta)?;
        let fixed: Vec<T<'h>> = args
            .iter()
            .map(|&a| match h.view(a) {
                View::App { head, args: &[_, cs] } if head == an => h.app(an, &[theta_ev, cs]),
                _ => a,
            })
            .collect();
        let View::App { head: op, .. } = h.view(t) else { return self.inner.call(cx, t, args) };
        let t2 = h.app(op, &fixed);
        let theta = theta_ev;
        let r = run(cx, &self.stop, "algebraic_number", |env, m| {
            let mut memo = Memo::default();
            match field_coefficients(env, t2, theta, &mut memo, m)? {
                Ok(cs) => Ok(Out::Again(env.call("algebraic_number", &[theta, cs]))),
                Err(_) => Ok(Out::Decline),
            }
        })?;
        match r {
            Reply::Decline => self.inner.call(cx, t, args),
            r => Ok(r),
        }
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

// ------------------------------------------------------------------------------------------- to_number_field

/// `to_number_field(a, theta)` and `to_number_field([a, b, ...])`.
fn to_number_field<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let mut memo = Memo::default();
    match args {
        [a, theta] if env.as_list(*a).is_none() => match field_coefficients(env, *a, *theta, &mut memo, m)? {
            // the number as an algebraic number of the field of theta (a rational stays a rational)
            Ok(cs) => Ok(Out::Again(env.call("algebraic_number", &[args[1], cs]))),
            Err(o) => Ok(o),
        },
        // one number: the field it generates, the number its generator
        [a] if env.as_list(*a).is_none() => Ok(Out::Again(env.call("to_number_field", &[*a, *a]))),
        [list] | [list, _] => {
            let Some(items) = env.as_list(*list) else { return Ok(Out::Decline) };
            let mut k = algnum::rationals();
            let mut gamma = env.int(0);
            let mut elems: Vec<Elem> = vec![];
            for &it in items {
                let Some(x) = to_alg(env, it, &mut memo, m)? else { return Ok(Out::Decline) };
                let Some(x) = algnum::simple(&x, m)? else { return Ok(Out::Decline) };
                let Some(tw) = adjoin(&k, &x.k, m)? else { return Ok(Out::Decline) };
                elems = elems.iter().map(|e| tw.field.compose(e, &tw.theta)).collect();
                elems.push(tw.field.compose(&x.a, &tw.beta));
                match tw.shift {
                    Some(s) => gamma = env.ar.add(&[it, env.ar.mul(&[env.int(s), gamma])]),
                    // the first irrational number generates the field alone
                    None if k.degree() == 1 && tw.field.degree() > 1 => gamma = it,
                    None => {}
                }
                k = tw.field;
            }
            let d = k.degree();
            // each number as an algebraic number of the common field Q(gamma)
            let nums: Vec<T<'h>> =
                elems.iter().map(|e| env.call("algebraic_number", &[gamma, coefficient_list(env, e, d)])).collect();
            Ok(Out::Again(env.list(&nums)))
        }
        _ => Ok(env.arg_failure("to_number_field takes a number (and a generator), or a list of numbers")),
    }
}

/// `[c0, ..., c_(d-1)]`.
fn coefficient_list<'h>(env: &Env<'h>, e: &Elem, d: usize) -> T<'h> {
    let items: Vec<T<'h>> = (0..d).map(|i| rat_term(env, &e.get(i).cloned().unwrap_or_else(|| q(0)))).collect();
    env.list(&items)
}

fn rat_term<'h>(env: &Env<'h>, c: &Rat) -> T<'h> {
    env.ar.number(&Num::rat(c.clone()))
}

// ------------------------------------------------------------------------------------------- to_radicals

/// `to_radicals(e)`: every `root_sum` over a cubic or quartic in `e` as a sum over the radical roots `solve` gives.
fn to_radicals<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h: &'h Heap = cx.heap();
    let &[e] = args else { return Ok(Reply::Decline) };
    // the root sums of e (subterms), found without evaluation
    let env = Env::new(h, stop.clone(), "to_radicals");
    let mut found: Vec<(T<'h>, T<'h>, T<'h>, T<'h>)> = vec![];
    let mut stack = vec![e];
    let mut seen = rustc_hash::FxHashSet::default();
    let mut unlimited = mc_poly::work::Unlimited;
    let mut m = Meter::new(&mut unlimited);
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let Ok(Some((x, body, g))) = rootsum::radical_candidate(&env, u, &mut m) {
            found.push((u, x, body, g));
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    if found.is_empty() {
        // `root(p, k)` objects of degree 2 to 4 are written with radicals by the library (`calc_to_radicals`)
        let root = h.sym_term(h.sym("root"));
        let mut stack = vec![e];
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if head == root {
                    return Ok(Reply::Again(h.call(h.sym("calc_to_radicals"), &[e])));
                }
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        return Ok(Reply::Done(e));
    }
    let mut out = e;
    for (rs, x, body, g) in found {
        let eq = h.call(h.sym("equal"), &[body, env.int(0)]);
        let sols = cx.eval(h.call(h.sym("solve"), &[eq, x]))?;
        let Some(rows) = env.as_list(sols) else { continue };
        let mut roots = vec![];
        for &row in rows {
            let Some(&[rule]) = env.as_list(row) else { continue };
            let View::App { args: &[_, v], .. } = h.view(rule) else { continue };
            roots.push(v);
        }
        let deg = rootsum_degree(&env, x, body, &mut m);
        if Some(roots.len()) != deg {
            continue;
        }
        let Ok(sum) = rootsum::radical_sum(&env, g, &roots, &mut m) else { continue };
        let Ok(r) = env.subst(out, rs, sum, &mut m) else { continue };
        out = r;
    }
    Ok(Reply::Again(out))
}

fn rootsum_degree<'h>(env: &Env<'h>, x: T<'h>, body: T<'h>, m: &mut Meter) -> Option<usize> {
    let p = convert::rational_coefficients(env, x, body, m).ok()??;
    // distinct roots: solve lists each root once
    let sf = qpoly::square_free(&p, m).ok()?;
    if sf.len() != p.len() {
        return None;
    }
    qpoly::deg(&p)
}

// ------------------------------------------------------------------------------------------- the extension: option

/// What a function does over the extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtOp {
    /// `factor(p, extension: a)`
    Factor,
    /// `factor_square_free(p, extension: a)`
    SquareFree,
    /// `polynomial_gcd(p, q, ..., extension: a)`
    Gcd,
}

/// `factor`, `factor_square_free` and `polynomial_gcd` with the `extension:` option handled here, every other call
/// passed to `mc-poly`'s native.
pub(crate) struct ExtensionOption {
    pub inner: Box<dyn Native>,
    pub stop: Arc<AtomicBool>,
    pub op: ExtOp,
    pub name: &'static str,
}

impl Native for ExtensionOption {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h: &'h Heap = cx.heap();
        if let Some(call) = gaussian_option(h, self.name, args) {
            return Ok(Reply::Again(call));
        }
        let Some((polys, ext)) = extension_option(h, args) else {
            // an exact non-real coefficient (im) puts the polynomials over the Gaussian rationals
            if !args.is_empty() && !args.iter().any(|&a| is_options(h, a)) && { let env = Env::new(h, self.stop.clone(), self.name); args.iter().any(|&a| has_algebraic_constant(&env, a)) } {
                let auto = h.sym_term(h.sym("automatic"));
                let polys = args;
                let op = self.op;
                let attempt = run(cx, &self.stop, self.name, |env, m| over_extension(env, op, polys, auto, m))?;
                if !matches!(attempt, Reply::Decline) && !is_failure_reply(h, &attempt) {
                    return Ok(attempt);
                }
            }
            return self.inner.call(cx, t, args);
        };
        // a list: each element
        let list = h.sym_term(h.sym(names::LIST));
        if let (ExtOp::Factor | ExtOp::SquareFree, [p]) = (self.op, polys) {
            if let View::App { head, args: items } = h.view(*p) {
                if head == list {
                    let opts = args[args.len() - 1];
                    let calls: Vec<T<'h>> = items.iter().map(|&it| h.call(h.sym(self.name), &[it, opts])).collect();
                    return Ok(Reply::Again(h.app(list, &calls)));
                }
            }
        }
        let op = self.op;
        run(cx, &self.stop, self.name, |env, m| over_extension(env, op, polys, ext, m))
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

fn is_options<'h>(h: &'h Heap, t: T<'h>) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym("options")))
}

/// Is the reply a failure term?
fn is_failure_reply<'h>(h: &'h Heap, r: &Reply<'h>) -> bool {
    match r {
        Reply::Done(t) | Reply::Again(t) => matches!(h.view(*t), View::App { head, .. } if head == h.sym_term(h.sym(names::FAILURE))),
        _ => false,
    }
}

/// Does the term hold an exact non-real number (`im`)? Polynomials with such coefficients are taken over the
/// Gaussian rationals; roots of rationals need `extension: automatic` asked for.
fn has_algebraic_constant<'h>(env: &Env<'h>, t: T<'h>) -> bool {
    if let Some(n) = env.ar.num(t) {
        return matches!(n, Num::Complex(..)) && n.is_exact();
    }
    match env.h().view(t) {
        View::App { args, .. } => args.iter().any(|&a| has_algebraic_constant(env, a)),
        _ => false,
    }
}

/// `(polys, a)` for `f(polys..., options(extension -> a))`.
fn extension_option<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> Option<(&'a [T<'h>], T<'h>)> {
    let (&opts, polys) = args.split_last()?;
    if polys.is_empty() {
        return None;
    }
    let View::App { head, args: rules } = h.view(opts) else { return None };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let key = h.sym_term(h.sym("extension"));
    match rules {
        [r] => match h.view(*r) {
            View::App { args: &[k, v], .. } if k == key => Some((polys, v)),
            _ => None,
        },
        _ => None,
    }
}

/// `f(polys..., gaussian_integers: true)` as `f(polys..., extension: im)` (the Gaussian rationals are `Q(i)`);
/// `gaussian_integers: false` is the call without the option.
fn gaussian_option<'h>(h: &'h Heap, name: &str, args: &[T<'h>]) -> Option<T<'h>> {
    let (&opts, polys) = args.split_last()?;
    let View::App { head, args: &[r] } = h.view(opts) else { return None };
    if head != h.sym_term(h.sym("options")) || polys.is_empty() {
        return None;
    }
    let View::App { args: &[k, v], .. } = h.view(r) else { return None };
    if k != h.sym_term(h.sym("gaussian_integers")) {
        return None;
    }
    let mut a = polys.to_vec();
    if v == h.sym_term(h.sym(names::TRUE)) {
        let rule = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("extension")), h.sym_term(h.sym("im"))]);
        a.push(h.call(h.sym("options"), &[rule]));
    } else if v != h.sym_term(h.sym(names::FALSE)) {
        return None;
    }
    Some(h.call(h.sym(name), &a))
}

/// Polynomials in one variable read over one number field, with the printer of its elements.
pub(crate) struct OverField<'h> {
    pub(crate) k: NumberField,
    pr: Printer<'h>,
    pub(crate) x: T<'h>,
    pub(crate) polys: Vec<KPoly>,
}

/// The polynomials over `Q(a, b, ..., their algebraic coefficients)`; `Err` with the reply when they are not
/// univariate polynomials with algebraic coefficients.
pub(crate) fn read_over_field<'h>(env: &Env<'h>, ps: &[T<'h>], ext: T<'h>, m: &mut Meter) -> WorkResult<Result<OverField<'h>, Out<'h>>> {
    let mut memo = Memo::default();
    // the generators: the extension's numbers, then the algebraic kernels of the polynomials
    let mut gens: Vec<(T<'h>, AlgNum)> = vec![];
    let ext_items: Vec<T<'h>> = if ext == env.sym("automatic") {
        vec![]
    } else {
        env.as_list(ext).map_or_else(|| vec![ext], |xs| xs.to_vec())
    };
    for it in ext_items {
        match to_alg(env, it, &mut memo, m)? {
            Some(a) => gens.push((it, a)),
            None => return Ok(Err(env.arg_failure("the extension must be algebraic numbers"))),
        }
    }
    let c = env.conv();
    let (ring, shared) = c.ring(ps, &[], m)?;
    let mut var: Option<usize> = None;
    let mut kernel_alg: Vec<Option<AlgNum>> = vec![];
    for (i, &k) in ring.kernels.iter().enumerate() {
        let a = to_alg(env, k, &mut memo, m)?;
        match &a {
            Some(x) => gens.push((k, x.clone())),
            None if var.is_none() => var = Some(i),
            None => {
                return Ok(Err(env.failure("not_univariate", "over an extension the polynomials must have one variable")))
            }
        }
        kernel_alg.push(a);
    }
    let Some(v) = var else { return Ok(Err(Out::Decline)) };
    // keep each generator once, written as small as possible
    let mut simple: Vec<(T<'h>, AlgNum)> = vec![];
    for (t, a) in gens {
        let Some(s) = algnum::simple(&a, m)? else { return Ok(Err(Out::Decline)) };
        if s.as_rat().is_some() || simple.iter().any(|(_, o)| o.k == s.k) {
            continue;
        }
        simple.push((t, s));
    }
    let xs: Vec<AlgNum> = simple.iter().map(|(_, a)| a.clone()).collect();
    let Some((k, images)) = algnum::common_field(&xs, m)? else { return Ok(Err(Out::Decline)) };
    // the kernels' elements in k
    let mut kernel_elems: Vec<Option<Elem>> = vec![];
    for a in &kernel_alg {
        kernel_elems.push(match a {
            Some(a) => match embed_into(&k, &images, &simple, a, m)? {
                Some(e) => Some(e),
                None => return Ok(Err(Out::Decline)),
            },
            None => None,
        });
    }
    let mut polys = Vec::with_capacity(ps.len());
    for &p in ps {
        let poly = c.to_poly(p, &ring, &shared, m)?;
        let deg = poly.num.degree(v).unwrap_or(0) as usize;
        if deg > 2000 {
            return Ok(Err(Out::Decline));
        }
        let mut kp: KPoly = vec![vec![]; deg + 1];
        for i in 0..poly.num.len() {
            m.step(1)?;
            let e = poly.num.exps(i);
            let mut coef = k.of(&poly.coef(i));
            for (j, &ej) in e.iter().enumerate() {
                if j != v && ej > 0 {
                    // invariant: every kernel but the variable has an element
                    let base = kernel_elems[j].as_ref().expect("an algebraic kernel");
                    let Some(pw) = k.pow(base, ej as i64) else { return Ok(Err(Out::Decline)) };
                    coef = k.mul(&coef, &pw);
                }
            }
            let d = e[v] as usize;
            kp[d] = k.add(&kp[d], &coef);
        }
        polys.push(kpoly::trim(kp));
    }
    // the printer writes elements with the generators' own terms
    let gen_terms: Vec<(Elem, T<'h>, usize)> =
        simple.iter().zip(&images).map(|((t, a), img)| (img.clone(), *t, a.k.degree())).collect();
    let Some(pr) = Printer::new(env, &k, &gen_terms) else { return Ok(Err(Out::Decline)) };
    Ok(Ok(OverField { k, pr, x: ring.kernels[v], polys }))
}

/// `factor`, `factor_square_free` or `polynomial_gcd` over the extension.
fn over_extension<'h>(env: &Env<'h>, op: ExtOp, ps: &[T<'h>], ext: T<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let of = match read_over_field(env, ps, ext, m)? {
        Ok(of) => of,
        // no variable: the polynomials are numbers, which mc-poly's function handles without the option
        Err(Out::Decline) if op != ExtOp::Gcd && ps.len() == 1 => return Ok(Out::Done(ps[0])),
        Err(o) => return Ok(o),
    };
    let k = &of.k;
    match op {
        ExtOp::Gcd => {
            let mut g: KPoly = vec![];
            for p in &of.polys {
                g = kpoly::gcd(k, &g, p, m)?;
            }
            if g.is_empty() {
                return Ok(Out::Done(env.int(0)));
            }
            // the gcd is monic
            let g = kpoly::monic(k, &g);
            let mut terms = vec![];
            for (i, c) in g.iter().enumerate() {
                let cc = of.pr.coords(c).unwrap_or_default();
                let ct = of.pr.term_with(env, &cc);
                terms.push(env.ar.mul(&[ct, env.ar.pow(of.x, env.int(i as i64))]));
            }
            Ok(Out::Done(env.ar.add(&terms)))
        }
        ExtOp::Factor | ExtOp::SquareFree => {
            let p = &of.polys[0];
            if p.len() <= 1 {
                return Ok(Out::Done(ps[0]));
            }
            let fs = if op == ExtOp::Factor {
                match kpoly::factor(k, p, m)? {
                    Some(fs) => fs,
                    None => return Ok(Out::Decline),
                }
            } else {
                // invariant: a trimmed polynomial of positive degree has a leading coefficient
                let unit = p.last().expect("nonzero").clone();
                let fs = kpoly::square_free(k, p, m)?.into_iter().map(|(f, e)| (kpoly::monic(k, &f), e)).collect();
                kpoly::Factored { unit, factors: fs }
            };
            let mut unit = fs.unit.clone();
            let mut parts = vec![];
            for (f, e) in &fs.factors {
                let (t, l) = factor_term(env, &of, f);
                // unit = unit / l^e
                unit = k.scale(&unit, &l.recip().expect("a positive integer").pow_u64(*e as u64));
                parts.push(env.ar.pow(t, env.int(*e as i64)));
            }
            let Some(uc) = of.pr.coords(&unit) else { return Ok(Out::Decline) };
            parts.push(of.pr.term_with(env, &uc));
            Ok(Out::Done(env.ar.mul(&parts)))
        }
    }
}

/// The term of a polynomial over the field with its coordinates' denominators cleared, and the factor `l` it was
/// multiplied by.
fn factor_term<'h>(env: &Env<'h>, of: &OverField<'h>, f: &[Elem]) -> (T<'h>, Rat) {
    let coords: Vec<Vec<Rat>> = f.iter().map(|c| of.pr.coords(c).unwrap_or_default()).collect();
    let mut l = Nat::from_u64(1);
    for c in coords.iter().flatten() {
        let g = mc_arith::num::gcd::gcd(&l, &c.den);
        l = l.mul(&c.den.divrem(&g).0);
    }
    let lr = Rat::from_int(Int::from_nat(l));
    let mut terms = vec![];
    for (i, cc) in coords.iter().enumerate() {
        let scaled: Vec<Rat> = cc.iter().map(|c| c.mul(&lr)).collect();
        let ct = of.pr.term_with(env, &scaled);
        terms.push(env.ar.mul(&[ct, env.ar.pow(of.x, env.int(i as i64))]));
    }
    (env.ar.add(&terms), lr)
}


/// The element of `a` in `k`, given the images of the generators `simple` (whose fields `a`'s field is one of,
/// or `a` is rational).
fn embed_into(
    k: &NumberField,
    images: &[Elem],
    simple: &[(T<'_>, AlgNum)],
    a: &AlgNum,
    m: &mut Meter,
) -> WorkResult<Option<Elem>> {
    if let Some(c) = a.as_rat() {
        return Ok(Some(k.of(&c)));
    }
    let Some(s) = algnum::simple(a, m)? else { return Ok(None) };
    let Some(i) = simple.iter().position(|(_, o)| o.k == s.k) else { return Ok(None) };
    // a = s.a in s.k, whose generator is images[i]
    Ok(Some(k.compose(&s.a, &images[i])))
}

/// Writes elements of a field as combinations of products of generator terms (a monomial basis found greedily in
/// order of total degree).
struct Printer<'h> {
    /// the basis: coordinates in the power basis and the monomial term
    basis: Vec<(Vec<Rat>, T<'h>)>,
    n: usize,
    env_terms: Vec<T<'h>>,
}

impl<'h> Printer<'h> {
    fn new(env: &Env<'h>, k: &NumberField, gens: &[(Elem, T<'h>, usize)]) -> Option<Printer<'h>> {
        let n = k.degree();
        let mut exps: Vec<Vec<usize>> = vec![vec![]];
        for (_, _, d) in gens {
            let mut next = vec![];
            for e in &exps {
                for j in 0..*d {
                    let mut f = e.clone();
                    f.push(j);
                    next.push(f);
                }
            }
            if next.len() > 20000 {
                return None;
            }
            exps = next;
        }
        exps.sort_by_key(|e| (e.iter().sum::<usize>(), e.clone()));
        let mut basis: Vec<(Vec<Rat>, T<'h>)> = vec![];
        let mut echelon: Vec<(usize, Vec<Rat>)> = vec![];
        for e in exps {
            if basis.len() == n {
                break;
            }
            let mut el = k.one();
            let mut factors = vec![];
            for (j, &ej) in e.iter().enumerate() {
                if ej > 0 {
                    el = k.mul(&el, &k.pow(&gens[j].0, ej as i64)?);
                    factors.push(env.ar.pow(gens[j].1, env.int(ej as i64)));
                }
            }
            let v = dense(&el, n);
            if let Some(r) = reduce_against(&echelon, v.clone()) {
                echelon.push(r);
                basis.push((v, env.ar.mul(&factors)));
            }
        }
        if basis.len() != n {
            return None;
        }
        let env_terms = basis.iter().map(|b| b.1).collect();
        Some(Printer { basis, n, env_terms })
    }

    /// The coordinates of `a` in the basis.
    fn coords(&self, a: &Elem) -> Option<Vec<Rat>> {
        let n = self.n;
        // solve sum c_j b_j = a: Gaussian elimination on the n x n system
        let mut rows: Vec<Vec<Rat>> = (0..n)
            .map(|i| {
                let mut r: Vec<Rat> = self.basis.iter().map(|b| b.0[i].clone()).collect();
                r.push(a.get(i).cloned().unwrap_or_else(|| q(0)));
                r
            })
            .collect();
        for col in 0..n {
            let piv = (col..n).find(|&r| !rows[r][col].is_zero())?;
            rows.swap(col, piv);
            let inv = rows[col][col].recip()?;
            for v in rows[col][col..].iter_mut() {
                *v = v.mul(&inv);
            }
            let pivot_row = rows[col].clone();
            for (r, row) in rows.iter_mut().enumerate() {
                if r != col && !row[col].is_zero() {
                    let f = row[col].clone();
                    for (v, p) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                        *v = v.sub(&p.mul(&f));
                    }
                }
            }
        }
        Some(rows.into_iter().map(|r| r[n].clone()).collect())
    }

    /// The term `sum c_j b_j`.
    fn term_with(&self, env: &Env<'h>, cs: &[Rat]) -> T<'h> {
        let parts: Vec<T<'h>> = cs
            .iter()
            .zip(&self.env_terms)
            .filter(|(c, _)| !c.is_zero())
            .map(|(c, &b)| env.ar.mul(&[rat_term(env, c), b]))
            .collect();
        env.ar.add(&parts)
    }
}

/// `v` reduced against the echelon rows; `Some((pivot, row))` when it is independent of them.
fn reduce_against(echelon: &[(usize, Vec<Rat>)], mut v: Vec<Rat>) -> Option<(usize, Vec<Rat>)> {
    for (p, row) in echelon {
        if !v[*p].is_zero() {
            let f = v[*p].mul(&row[*p].recip()?);
            for (x, y) in v.iter_mut().zip(row) {
                *x = x.sub(&f.mul(y));
            }
        }
    }
    let p = v.iter().position(|c| !c.is_zero())?;
    Some((p, v))
}

/// The dense coordinates of an element (length n).
fn dense(e: &Elem, n: usize) -> Vec<Rat> {
    (0..n).map(|i| e.get(i).cloned().unwrap_or_else(|| q(0))).collect()
}


// ------------------------------------------------------------------------------------------- root_reduce, norm, trace

/// `root_reduce(a)`: a rational, `u + v sqrt(D)` for a quadratic number, else `root(x => p(x), k)`.
fn root_reduce<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let &[a] = args else { return Ok(env.arg_failure("root_reduce takes one algebraic number")) };
    if let Some(r) = root_reduce_one(env, a, m)? {
        return Ok(Out::Done(r));
    }
    // an expression that is not an algebraic number (a polynomial in x with algebraic coefficients, say): its
    // algebraic number parts are reduced, the rest is kept
    let h = env.h();
    // an atom that is not an algebraic number (true, false, a symbol): its own reduction
    let View::App { head, .. } = h.view(a) else { return Ok(Out::Done(a)) };
    if head == env.sym("root_reduce") {
        return Ok(Out::Decline);
    }
    let r = root_reduce_parts(env, a, m)?;
    if r == a {
        // nothing to reduce: the expression is its own reduction
        return Ok(Out::Done(a));
    }
    Ok(Out::Again(r))
}

/// `t` with each algebraic number part replaced by its reduced form; an algebraic number is reduced as a whole.
fn root_reduce_parts<'h>(env: &Env<'h>, t: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
    m.step(1)?;
    let h = env.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if head == env.hd.list || head == env.sym("algebraic_number") || head == env.sym("root") {
        return Ok(t);
    }
    if let Some(r) = root_reduce_one(env, t, m)? {
        return Ok(r);
    }
    let mut na = Vec::with_capacity(args.len());
    for &u in args {
        na.push(root_reduce_parts(env, u, m)?);
    }
    Ok(if na.as_slice() == args { t } else { env.rebuild(head, &na) })
}

/// The reduced form of the algebraic number `a` (a rational, a quadratic surd or a root object), or `None` when
/// `a` is not an algebraic number.
fn root_reduce_one<'h>(env: &Env<'h>, a: T<'h>, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let mut memo = Memo::default();
    let Some(x) = to_alg(env, a, &mut memo, m)? else { return Ok(None) };
    let Some(s) = algnum::simple(&x, m)? else { return Ok(None) };
    if let Some(c) = s.as_rat() {
        return Ok(Some(rat_term(env, &c)));
    }
    let f = s.k.modulus();
    // invariant: a number that is not rational has a field of degree at least 2 with an embedding
    let e = s.k.embedding().expect("an embedding");
    if f.len() == 3 {
        // t^2 + p t + c = 0: t = -p/2 + s sqrt(p^2/4 - c), s = +1 for the larger real root or positive imaginary part
        let half_p = f[1].mul(&qpoly::qq(1, 2));
        let disc = half_p.mul(&half_p).sub(&f[0]);
        let plus = if e.real { !e.re.sub(&half_p.neg()).num.neg } else { !e.im.num.neg };
        let sq = env.ar.pow(rat_term(env, &disc), rat_term(env, &qpoly::qq(1, 2)));
        let sq = if plus { sq } else { env.ar.neg(sq) };
        return Ok(Some(env.ar.add(&[rat_term(env, &half_p.neg()), sq])));
    }
    Ok(Some(convert::root_object_term(env, f, e.index)))
}

/// The norm (`true`) or trace of `a` over Q in the field `Q(a)`: from its minimal polynomial `x^n + ... + c0`,
/// `(-1)^n c0` and `-c_(n-1)`.
///
/// With `extension: b` the norm or trace is over the field `K = Q(b)`, which must contain `a`: with `r = [K : Q(a)]`
/// the norm is `N(a)^r` and the trace `r Tr(a)` (the conjugates of `a` over `K` are those over `Q(a)`, each taken
/// `r` times; Cohen, *A Course in Computational Algebraic Number Theory*, section 4.1).
fn norm_or_trace<'h>(env: &Env<'h>, args: &[T<'h>], norm: bool, m: &mut Meter) -> WorkResult<Out<'h>> {
    let (a, ext) = match args {
        [a] => (*a, None),
        [_, _] => match extension_option(env.h(), args) {
            Some((&[a], b)) => (a, Some(b)),
            _ => return Ok(env.arg_failure("expected one algebraic number")),
        },
        _ => return Ok(env.arg_failure("expected one algebraic number")),
    };
    let mut memo = Memo::default();
    let Some(x) = to_alg(env, a, &mut memo, m)? else { return Ok(Out::Decline) };
    let mp = x.minpoly(m)?;
    let n = mp.len() - 1;
    let mut r = 1usize;
    if let Some(b) = ext {
        let Some(xb) = to_alg(env, b, &mut memo, m)? else {
            return Ok(env.arg_failure("the extension must be an algebraic number"));
        };
        let (Some(sa), Some(sb)) = (algnum::simple(&x, m)?, algnum::simple(&xb, m)?) else { return Ok(Out::Decline) };
        let Some(t) = adjoin(&sb.k, &sa.k, m)? else { return Ok(Out::Decline) };
        let dk = sb.k.degree();
        if t.field.degree() != dk || dk % n != 0 {
            return Ok(env.failure("not_in_field", "the number is not in the extension"));
        }
        r = dk / n;
    }
    let v = if norm {
        let c = mp[0].clone();
        let c = if n % 2 == 1 { c.neg() } else { c };
        let mut p = qpoly::qq(1, 1);
        for _ in 0..r {
            p = p.mul(&c);
        }
        p
    } else {
        mp[n - 1].neg().mul(&qpoly::qq(r as i64, 1))
    };
    Ok(Out::Done(rat_term(env, &v)))
}

// ------------------------------------------------------------------------------------------- normal(root_sum(...))

/// `normal` with sums over roots expanded: `root_sum(x => p(x), g)` becomes `g(root(p, 1)) + ... + g(root(p, d))`
/// (decision F1.12); every other call goes to `mc-poly`'s native unchanged.
pub(crate) struct NormalRootSum {
    pub inner: Box<dyn Native>,
    pub stop: Arc<AtomicBool>,
}

impl Native for NormalRootSum {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h: &'h Heap = cx.heap();
        let Some(&e) = args.first() else { return self.inner.call(cx, t, args) };
        // the midpoint of a centered interval, the value of an `around`
        if let (1, View::App { head, args: &[mid, _] }) = (args.len(), h.view(e)) {
            if let View::Sym(s) = h.view(head) {
                if matches!(h.sym_name(s).as_str(), "centered_interval" | "around") {
                    return Ok(Reply::Done(mid));
                }
            }
        }
        let env = Env::new(h, self.stop.clone(), "normal");
        let mut unlimited = mc_poly::work::Unlimited;
        let mut m = Meter::new(&mut unlimited);
        // the root sums of e, outermost first
        let mut found = vec![];
        let mut stack = vec![e];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(u) = stack.pop() {
            if !seen.insert(u) {
                continue;
            }
            if let Some(&[f, g]) = env.args_of(u, "root_sum") {
                if let Some((x, body)) = env.function_parts(f) {
                    if let Ok(Some(p)) = convert::rational_coefficients(&env, x, body, &mut m) {
                        if p.len() > 1 && p.len() <= 1001 {
                            found.push((u, f, g, p.len() - 1));
                            continue;
                        }
                    }
                }
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        if found.is_empty() {
            return self.inner.call(cx, t, args);
        }
        let mut out = e;
        for (rs, f, g, d) in found {
            let mut parts = Vec::with_capacity(d);
            for k in 1..=d {
                let r = env.call("root", &[f, env.int(k as i64)]);
                let Ok(v) = rootsum::apply(&env, g, r, &mut m) else { return self.inner.call(cx, t, args) };
                parts.push(v);
            }
            let Ok(r) = env.subst(out, rs, env.ar.add(&parts), &mut m) else { return self.inner.call(cx, t, args) };
            out = r;
        }
        let mut new_args = args.to_vec();
        new_args[0] = out;
        Ok(Reply::Again(h.call(h.sym("normal"), &new_args)))
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
