//! The natives of the crate and the wrappers that give the arithmetic, comparisons and polynomial functions of other
//! libraries their meaning on finite field elements.
//!
//! Conventions:
//!
//! - Calls with arguments of the wrong kinds decline and stay unevaluated (a field of a non-prime characteristic,
//!   an embedding between fields of different characteristics, a power with a fractional exponent).
//! - Elements of one field combine with each other and with exact numbers whose denominators are prime to p;
//!   elements of different fields stay apart (`a + b` is a sum of two elements).
//! - The wrapped natives (`add`, `mul`, `pow`, `equal`, `unequal`, `element`, `minimal_polynomial`, `factor`,
//!   `factor_square_free`, `expand`, `polynomial_gcd`, `polynomial_lcm`, `polynomial_quotient`,
//!   `polynomial_remainder`) pass every call without an element (or without a finite field in `extension:`) to the
//!   native they wrap, unchanged.
use crate::conway;
use crate::embed::{self, Embedding};
use crate::field::{El, Field, Repr};
use crate::fp::{self, Fp};
use crate::fqpoly::{self, QPoly};
use crate::terms::*;
use mc_api::names;
use mc_arith::num::Nat;
use mc_core::{Heap, Raw, Store, View, T};
use mc_eval::{Abort, Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numtheory::budget::Budget;
use std::sync::Arc;

/// The body of a native.
type Body = for<'a, 'k, 'h> fn(&mut NativeCx<'a, 'k, 'h>, &Syms, T<'h>, &[T<'h>]) -> Res<Reply<'h>>;

/// A native of this crate.
struct Fun {
    body: Body,
    syms: Syms,
    curried: bool,
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        (self.body)(cx, &self.syms, t, args)
    }
    fn curried(&self) -> bool {
        self.curried
    }
}

/// The natives: name, body, whether it takes curried calls.
pub(crate) const TABLE: &[(&str, Body, bool)] = &[
    ("finite_field", finite_field, true),
    ("finite_field_element", finite_field_element, true),
    ("finite_field_embedding", finite_field_embedding, true),
    ("finite_field_index", finite_field_index, false),
    ("finite_field_element_norm", element_norm, false),
    ("finite_field_element_trace", element_trace, false),
    ("finite_field_element_minimal_polynomial", element_minimal_polynomial, false),
    ("is_finite_field_element", is_element, false),
    ("is_finite_field_element_primitive", is_element_primitive, false),
    ("finite_field_primitive_element", primitive_element, false),
    ("frobenius_automorphism", frobenius_automorphism, false),
    ("finite_field_log", finite_field_log, false),
    ("finite_field_root", finite_field_root, false),
    ("conway_polynomial", conway_polynomial, false),
    ("is_irreducible_polynomial", is_irreducible_polynomial, false),
    ("is_primitive_polynomial", is_primitive_polynomial, false),
];

/// The functions of other libraries given a meaning on elements.
const WRAPPED: &[(&str, Op)] = &[
    (names::ADD, Op::Add),
    (names::MUL, Op::Mul),
    (names::POW, Op::Pow),
    ("equal", Op::Equal(false)),
    ("unequal", Op::Equal(true)),
    ("element", Op::Element),
    ("minimal_polynomial", Op::MinimalPolynomial),
    ("multiplicative_order", Op::Order),
    ("factor", Op::Factor(false)),
    ("factor_square_free", Op::Factor(true)),
    ("expand", Op::Expand),
    ("polynomial_gcd", Op::Gcd(false)),
    ("polynomial_lcm", Op::Gcd(true)),
    ("polynomial_quotient", Op::Div(false)),
    ("polynomial_remainder", Op::Div(true)),
    ("cancel", Op::Cancel),
    ("together", Op::Cancel),
    ("dot", Op::Reeval),
    ("det", Op::Reeval),
    ("inverse", Op::Reeval),
    ("total", Op::Reeval),
];

/// Register the natives and the wrappers.
pub fn install(k: &mut Kernel, h: &Heap) {
    let syms = Syms::new(h);
    for &(name, body, curried) in TABLE {
        k.register(h.sym(name), Attributes::default(), Box::new(Fun { body, syms, curried }));
    }
    for &(name, op) in WRAPPED {
        let s = h.sym(name);
        let wrapped = k.wrap_native(s, |inner| Box::new(Wrap { inner, op, syms, name: s }));
        if !wrapped {
            // no native to wrap (a build without that library): the element cases alone
            k.register(s, Attributes::default(), Box::new(Wrap { inner: Box::new(Declines), op, syms, name: s }));
        }
    }
}

/// A native that always declines.
struct Declines;

impl Native for Declines {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(Reply::Decline)
    }
}

fn done(t: T<'_>) -> Res<Reply<'_>> {
    Ok(Reply::Done(t))
}

fn again(t: T<'_>) -> Res<Reply<'_>> {
    Ok(Reply::Again(t))
}

fn decline<'h>() -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

fn sym<'h>(h: &'h Heap, name: &str) -> T<'h> {
    h.sym_term(h.sym(name))
}

/// `failure(argument, f, message)`
fn arg_failure<'h>(h: &'h Heap, f: &str, message: &str) -> T<'h> {
    h.call(h.sym(names::FAILURE), &[sym(h, "argument"), sym(h, f), h.string(message)])
}

/// The element term of `a` in the field of `e` (reusing its field term).
fn elem_like<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, e: &Elem<'h>, a: &El) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    elem_term(h, s, &e.field, e.fterm, a, cx)
}

// ------------------------------------------------------------------------------------------- finite_field

/// `finite_field(p, n)`, `finite_field(q)`, `finite_field(p, n, repr)`, `finite_field(p, f, repr)`, and the calls
/// `F(k)`, `F([c0, ...])`, `F("property")` of a field.
fn finite_field<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let View::App { head, .. } = h.view(t) else { return decline() };
    if !matches!(h.view(head), View::Sym(_)) {
        // F(data)
        return field_call(cx, s, head, args);
    }
    let (spec, repr) = match args {
        [a] => (vec![*a], Repr::Polynomial),
        [a, b] => match h.view(*b) {
            View::Str(r) => match repr_of(r) {
                Some(r) => (vec![*a], r),
                None => return decline(),
            },
            _ => (vec![*a, *b], Repr::Polynomial),
        },
        [a, b, r] => match h.view(*r) {
            View::Str(r) => match repr_of(r) {
                Some(r) => (vec![*a, *b], r),
                None => return decline(),
            },
            _ => return decline(),
        },
        _ => return decline(),
    };
    // the characteristic and the polynomial
    let field = match spec[..] {
        [q] => {
            let Some(q) = nat_of(h, q) else { return decline() };
            if q < Nat::from_u64(2) {
                return decline();
            }
            let (base, e) = mc_numtheory::power::perfect_power(&q, cx)?;
            let Some(p) = base.to_u128().filter(|&v| v < 1 << 62).map(|v| v as u64) else { return decline() };
            if !mc_numtheory::word::is_prime_u64(p) {
                return decline();
            }
            if e > 4096 {
                // the same degree cap as finite_field(p, n); no Conway search for degree 100000 (B26-10)
                return decline();
            }
            standard_field(cx, p, e as usize, repr)?
        }
        [p, n] => {
            let Some(p) = small_prime(h, p) else { return decline() };
            if let View::Int(n) = h.view(n) {
                if !(1..=4096).contains(&n) {
                    return decline();
                }
                standard_field(cx, p, n as usize, repr)?
            } else if let Some((x, body)) = function_parts(h, s, n) {
                let Some(poly) = fp_poly_of(h, s, body, x, Fp::new(p)) else { return decline() };
                checked_field(p, &poly, repr, cx)?
            } else {
                return decline();
            }
        }
        _ => None,
    };
    let Some(field) = field else { return decline() };
    let canon = field_term(h, s, &field);
    done(canon)
}

fn repr_of(r: &str) -> Option<Repr> {
    match r {
        "Polynomial" => Some(Repr::Polynomial),
        "Exponential" => Some(Repr::Exponential),
        _ => None,
    }
}

fn small_prime(h: &Heap, t: T<'_>) -> Option<u64> {
    match h.view(t) {
        View::Int(p) if (2..1 << 62).contains(&p) && mc_numtheory::word::is_prime_u64(p as u64) => Some(p as u64),
        _ => None,
    }
}

/// The field F_(p^n) with its standard polynomial.
fn standard_field(cx: &mut NativeCx<'_, '_, '_>, p: u64, n: usize, repr: Repr) -> Res<Option<Arc<Field>>> {
    let Some((poly, _)) = conway::defining(p, n, cx)? else { return Ok(None) };
    checked_field_with(p, &poly, repr, true, cx)
}

/// `F(data)` for a field term F.
fn field_call<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, ft: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [data] = args else { return decline() };
    let Some(f) = field_of(h, s, ft, cx)? else { return decline() };
    if let Some(r) = properties(cx, s, &f, ft, None, *data)? {
        return done(r);
    }
    let Some(el) = data_elem(h, s, &f, *data) else { return decline() };
    match elem_term(h, s, &f, ft, &el, cx)? {
        Some(t) => done(t),
        None => decline(),
    }
}

/// The value of a property (or a list of them) of a field or an element; None when `data` names none.
fn properties<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, f: &Arc<Field>, ft: T<'h>, el: Option<&El>, data: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if let Some(items) = call_of(h, data, s.list) {
        if items.is_empty() || !items.iter().all(|&i| matches!(h.view(i), View::Str(_))) {
            return Ok(None);
        }
        let mut out = vec![];
        for &i in items {
            match properties(cx, s, f, ft, el, i)? {
                Some(v) => out.push(v),
                None => return Ok(None),
            }
        }
        return Ok(Some(h.call(s.list, &out)));
    }
    let View::Str(name) = h.view(data) else { return Ok(None) };
    let v = match (name, el) {
        ("Characteristic", _) => h.int(f.p() as i64),
        ("ExtensionDegree", _) => h.int(f.n as i64),
        ("FieldSize", _) => nat_term(h, &f.q),
        ("FieldIrreducible", _) => defining_function(h, s, f),
        ("ElementRepresentation", _) => h.string(f.repr.name()),
        ("Field", Some(_)) => ft,
        ("Coefficients", Some(a)) => coeff_list(h, s, a),
        ("Index", Some(a)) => match index_of(f, a, cx)? {
            Some(k) => nat_term(h, &k),
            None => return Ok(None),
        },
        ("Trace", Some(a)) => h.int(f.trace(a) as i64),
        ("Norm", Some(a)) => h.int(f.norm(a) as i64),
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// The index of an element in its field's representation.
fn index_of(f: &Field, a: &El, b: &mut dyn Budget) -> Res<Option<Nat>> {
    match f.repr {
        Repr::Polynomial => Ok(Some(f.poly_index(a))),
        Repr::Exponential => f.exp_index(a, b),
    }
}

// ------------------------------------------------------------------------------------------- elements

/// `finite_field_element(F, data)` and the property calls `a("Index")` of an element.
fn finite_field_element<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let View::App { head, .. } = h.view(t) else { return decline() };
    if !matches!(h.view(head), View::Sym(_)) {
        let [data] = args else { return decline() };
        let Some(e) = elem_of(h, s, head, cx)? else { return decline() };
        return match properties(cx, s, &e.field, e.fterm, Some(&e.el), *data)? {
            Some(v) => done(v),
            None => decline(),
        };
    }
    let [ft, data] = args else { return decline() };
    let Some(f) = field_of(h, s, *ft, cx)? else { return decline() };
    let Some(el) = data_elem(h, s, &f, *data) else { return decline() };
    // the field term canonical too (an element written with a field in another form)
    let canon_ft = if field_term(h, s, &f) == *ft { *ft } else { field_term(h, s, &f) };
    match elem_term(h, s, &f, canon_ft, &el, cx)? {
        Some(c) => done(c),
        None => decline(),
    }
}

fn one_elem<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, args: &[T<'h>]) -> Res<Option<Elem<'h>>> {
    let h = cx.heap();
    let [a] = args else { return Ok(None) };
    elem_of(h, s, *a, cx)
}

fn finite_field_index<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some(e) = one_elem(cx, s, args)? else { return decline() };
    match index_of(&e.field, &e.el, cx)? {
        Some(k) => done(nat_term(cx.heap(), &k)),
        None => decline(),
    }
}

fn element_norm<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some(e) = one_elem(cx, s, args)? else { return decline() };
    done(cx.heap().int(e.field.norm(&e.el) as i64))
}

fn element_trace<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some(e) = one_elem(cx, s, args)? else { return decline() };
    done(cx.heap().int(e.field.trace(&e.el) as i64))
}

/// The minimal polynomial of a over F_(p^k) (k dividing the degree of the field; k = 1: integer coefficients).
fn minimal_polynomial_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, e: &Elem<'h>, x: T<'h>, k: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let f = &e.field;
    if k == 0 || !f.n.is_multiple_of(k) {
        return Ok(None);
    }
    if k == 1 {
        return Ok(Some(fp_poly_term(h, s, &f.minimal_polynomial(&e.el), x)));
    }
    // conjugates under the Frobenius of F_(p^k)
    let mut cs = vec![e.el.clone()];
    loop {
        // invariant: cs is nonempty
        let next = f.frobenius(cs.last().unwrap(), k);
        if next == e.el || cs.len() * k >= f.n {
            break;
        }
        cs.push(next);
    }
    let mut prod: QPoly = vec![f.one()];
    for c in &cs {
        prod = fqpoly::mul(f, &prod, &[f.neg(c), f.one()]);
    }
    q_poly_term(h, s, f, e.fterm, &prod, x, cx)
}

fn element_minimal_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, x, k) = match args {
        [a, x] => (*a, *x, 1),
        [a, x, k] => match h.view(*k) {
            View::Int(k) if k >= 1 => (*a, *x, k as usize),
            _ => return decline(),
        },
        _ => return decline(),
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return decline();
    }
    let Some(e) = elem_of(h, s, a, cx)? else { return decline() };
    match minimal_polynomial_term(cx, s, &e, x, k)? {
        Some(v) => again(v),
        None => decline(),
    }
}

fn is_element<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [a] = args else { return decline() };
    let h = cx.heap();
    let ok = call_of(h, *a, s.ffe).is_some() && elem_of(h, s, *a, cx)?.is_some();
    done(cx.bool(ok))
}

fn is_element_primitive<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some(e) = one_elem(cx, s, args)? else { return decline() };
    match e.field.is_primitive(&e.el, cx)? {
        Some(v) => done(cx.bool(v)),
        None => decline(),
    }
}

/// The primitive element of least polynomial index.
fn primitive_element<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [ft] = args else { return decline() };
    let Some(f) = field_of(h, s, *ft, cx)? else { return decline() };
    // an element of the prime field has order dividing p - 1: for n >= 2 the search starts at the index p (the
    // element x), the first one outside it (B26-9: 10^6 constants used up the whole iteration cap for p > 10^6)
    let mut k = if f.n >= 2 { Nat::from_u64(f.p()) } else { Nat::from_u64(1) };
    for _ in 0..1_000_000 {
        if k >= f.q {
            break;
        }
        let a = f.from_index(&k);
        cx.charge((f.n * f.n) as u64)?;
        match f.is_primitive(&a, cx)? {
            None => return decline(),
            Some(true) => {
                return match elem_term(h, s, &f, *ft, &a, cx)? {
                    Some(t) => done(t),
                    None => decline(),
                }
            }
            Some(false) => {}
        }
        k = k.add_u64(1);
    }
    decline()
}

fn frobenius_automorphism<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, kt) = match args {
        [a] => (*a, None),
        [a, k] => (*a, Some(*k)),
        _ => return decline(),
    };
    let Some(e) = elem_of(h, s, a, cx)? else { return decline() };
    let k = match kt {
        None => 1,
        Some(kt) => {
            let Some(ki) = int_of(h, kt) else { return decline() };
            let n = Nat::from_u64(e.field.n as u64);
            ki.divrem_floor(&n).1.to_u128().unwrap_or(0) as usize
        }
    };
    let v = e.field.frobenius(&e.el, k);
    match elem_like(cx, s, &e, &v)? {
        Some(t) => done(t),
        None => decline(),
    }
}

/// `finite_field_log(a)` (to the base x, which must be primitive) and `finite_field_log(a, g)`.
fn finite_field_log<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, g) = match args {
        [a] => (*a, None),
        [a, g] => (*a, Some(*g)),
        _ => return decline(),
    };
    let Some(e) = elem_of(h, s, a, cx)? else { return decline() };
    let base = match g {
        None => e.field.gen(),
        Some(g) => match elem_of(h, s, g, cx)? {
            Some(ge) if ge.field.same_elements(&e.field) => ge.el,
            _ => return decline(),
        },
    };
    match e.field.log(&e.el, &base, cx)? {
        Some(k) => done(nat_term(h, &k)),
        None => decline(),
    }
}

/// `finite_field_root(a, n)`: the n-th roots of a, by increasing index.
fn finite_field_root<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, n] = args else { return decline() };
    let n = match h.view(*n) {
        View::Int(n) if (1..=1 << 20).contains(&n) => n as usize,
        _ => return decline(),
    };
    let Some(e) = elem_of(h, s, *a, cx)? else { return decline() };
    let f = &e.field;
    // roots of X^n - a
    let mut poly: QPoly = vec![vec![]; n + 1];
    poly[0] = f.neg(&e.el);
    poly[n] = f.one();
    let rs = fqpoly::roots(f, &poly, cx)?;
    let mut out = vec![];
    for r in &rs {
        match elem_like(cx, s, &e, r)? {
            Some(t) => out.push(t),
            None => return decline(),
        }
    }
    done(cx.list(&out))
}

/// `conway_polynomial(p, n, x)`.
fn conway_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [p, n, x] = args else { return decline() };
    let Some(p) = small_prime(h, *p) else { return decline() };
    let n = match h.view(*n) {
        View::Int(n) if (1..=4096).contains(&n) => n as usize,
        _ => return decline(),
    };
    if !matches!(h.view(*x), View::Sym(_)) {
        return decline();
    }
    match conway::conway(p, n, conway::RUNTIME_STEPS, cx)? {
        Some(c) => again(fp_poly_term(h, s, &c, *x)),
        None => done(arg_failure(h, "conway_polynomial", "the Conway polynomial is not tabulated and too costly to compute")),
    }
}

/// The variable of a polynomial in one variable, if it has exactly one.
fn single_variable<'h>(h: &'h Heap, s: &Syms, ts: &[T<'h>]) -> Option<T<'h>> {
    let mut vs = vec![];
    for &t in ts {
        variables(h, s, t, &mut vs);
    }
    let names = ["true", "false", "null"];
    vs.retain(|&v| match h.view(v) {
        View::Sym(x) => !names.contains(&h.sym_name(x).as_str()),
        _ => false,
    });
    match vs[..] {
        [x] => Some(x),
        _ => None,
    }
}

/// `is_irreducible_polynomial(f)`, with `modulus: p` or `extension: F`.
fn is_irreducible_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let plain = plain_args(h, s, args);
    // a list of polynomials: each one, with the same options
    if let Some(items) = plain.first().and_then(|&f| call_of(h, f, s.list)) {
        let calls: Vec<T<'h>> = items
            .iter()
            .map(|&p| {
                let mut a = args.to_vec();
                a[0] = p;
                h.call(h.sym("is_irreducible_polynomial"), &a)
            })
            .collect();
        return Ok(Reply::Again(h.call(s.list, &calls)));
    }
    // is_irreducible_polynomial(f) or (f, x): the variable may be named (D26-21)
    let (f, named) = match plain {
        [f] => (f, None),
        [f, x] if matches!(h.view(*x), View::Sym(_)) => (f, Some(*x)),
        _ => return decline(),
    };
    // an algebraic-number extension (not a finite field): factor over it
    let gaussian = option(h, s, args, "gaussian_integers").is_some_and(|g| cx.is_true(g));
    let ext = option(h, s, args, "extension");
    let finite_ext = match ext {
        Some(e) => field_of(h, s, e, cx)?,
        None => None,
    };
    if gaussian || (ext.is_some() && finite_ext.is_none()) {
        let im = h.call(h.sym(mc_api::names::COMPLEX), &[h.int(0), h.int(1)]);
        let e = if gaussian { im } else { ext.unwrap_or(im) };
        return rational_irreducible(cx, *f, Some(e));
    }
    let field = if let Some(m) = option(h, s, args, "modulus") {
        match small_prime(h, m) {
            Some(p) => {
                let Some((poly, _)) = conway::defining(p, 1, cx)? else { return decline() };
                checked_field(p, &poly, Repr::Polynomial, cx)?
            }
            None => return decline(),
        }
    } else if ext.is_some() {
        finite_ext
    } else {
        find_elem(h, s, &[*f], cx)?.map(|e| e.field)
    };
    let Some(k) = field else {
        // over the rationals: one irreducible factor of degree >= 1
        return rational_irreducible(cx, *f, None);
    };
    let Some(x) = named.or_else(|| single_variable(h, s, &[*f])) else {
        return match q_constant(h, s, &k, *f, cx)? {
            Some(_) => done(cx.bool(false)),
            None => decline(),
        };
    };
    let Some(poly) = q_poly_of(h, s, &k, *f, x, cx)? else { return decline() };
    if poly.len() < 2 {
        return done(cx.bool(false));
    }
    let (_, fs) = factor_any(&k, &poly, cx)?;
    done(cx.bool(fs.len() == 1 && fs[0].1 == 1))
}

fn q_constant(h: &Heap, s: &Syms, k: &Field, t: T<'_>, b: &mut dyn Budget) -> Res<Option<El>> {
    let x = sym(h, "x");
    Ok(q_poly_of(h, s, k, t, x, b)?.filter(|p| p.len() <= 1).map(|p| p.into_iter().next().unwrap_or_default()))
}

/// Irreducibility over the rationals (or over the number field `ext`, given as `factor`'s `extension:`) by
/// `factor_list`-free means: `factor(f)` must leave a single irreducible factor (a constant times it), checked on
/// the result's structure.
fn rational_irreducible<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, ext: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut vs = vec![];
    let s = Syms::new(h);
    variables(h, &s, f, &mut vs);
    if vs.is_empty() || !polynomial_shape(h, &s, f) {
        return decline();
    }
    // of degree 1 in a variable y, f = a y + b is irreducible over every field when gcd(a, b) is a nonzero constant
    // (a factor of positive degree in y is f up to a unit; one free of y divides a and b), and that gcd does not
    // depend on the field (Euclid's algorithm runs over the field of the coefficients): this needs no factorisation
    let call = |name: &str, args: &[T<'h>]| h.call(h.sym(name), args);
    for &y in &vs {
        if cx.eval(call("exponent", &[f, y]))? != h.int(1) {
            continue;
        }
        let a = cx.eval(call("coefficient", &[f, y, h.int(1)]))?;
        let b = cx.eval(call("coefficient", &[f, y, h.int(0)]))?;
        let g = cx.eval(call("polynomial_gcd", &[a, b]))?;
        let mut ws = vec![];
        variables(h, &s, g, &mut ws);
        if ws.is_empty() && is_rational(h, g) && g != h.int(0) {
            return done(cx.bool(true));
        }
    }
    let g = match ext {
        Some(e) => {
            // named arguments travel as options(name -> value)
            let opt = h.call(h.sym(mc_api::names::RULE), &[h.sym_term(h.sym("extension")), e]);
            cx.eval(h.call(h.sym("factor"), &[f, h.call(h.sym("options"), &[opt])]))?
        }
        None => cx.eval(h.call(h.sym("factor"), &[f]))?,
    };
    // no factorisation (a failure, or the call left as it is): no answer
    let head_is = |t: T<'h>, name: &str| matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(name)));
    if head_is(g, mc_api::names::FAILURE) || head_is(g, "factor") {
        return decline();
    }
    // a product of a constant and one non-constant factor, or that factor alone
    let constant = |a: T<'h>| {
        let mut ws = vec![];
        variables(h, &s, a, &mut ws);
        ws.is_empty()
    };
    let factors: Vec<T<'h>> = match call_of(h, g, s.mul) {
        Some(args) => args.iter().copied().filter(|&a| !is_rational(h, a) && !constant(a)).collect(),
        None => vec![g],
    };
    let ok = factors.len() == 1 && call_of(h, factors[0], s.pow).is_none();
    done(cx.bool(ok))
}

/// Whether a term is built from symbols and exact numbers by sums, products and non-negative integer powers. The
/// exact numbers include complex rationals and algebraic constants (`2^(1/2)`, `(1 + 3^(1/2))/2`): coefficients
/// that `factor` takes as they are.
fn polynomial_shape(h: &Heap, s: &Syms, t: T<'_>) -> bool {
    match h.view(t) {
        View::Sym(_) | View::Int(_) | View::BigInt { .. } | View::Rational { .. } => true,
        View::App { head, args } => match h.view(head) {
            View::Sym(x) if x == s.add || x == s.mul => args.iter().all(|&a| polynomial_shape(h, s, a)),
            View::Sym(x) if x == s.pow => match args {
                [b, e] if matches!(h.view(*e), View::Int(k) if k >= 0) => polynomial_shape(h, s, *b),
                [b, e] => exact_constant(h, s, *b) && exact_constant(h, s, *e),
                _ => false,
            },
            View::Sym(x) if h.sym_name(x) == mc_api::names::COMPLEX => args.iter().all(|&a| exact_constant(h, s, a)),
            _ => false,
        },
        _ => false,
    }
}

/// A term without symbols built from exact rationals by sums, products, powers and `complex`.
fn exact_constant(h: &Heap, s: &Syms, t: T<'_>) -> bool {
    match h.view(t) {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } => true,
        View::App { head, args } => match h.view(head) {
            View::Sym(x) if x == s.add || x == s.mul || x == s.pow || h.sym_name(x) == mc_api::names::COMPLEX => {
                args.iter().all(|&a| exact_constant(h, s, a))
            }
            _ => false,
        },
        _ => false,
    }
}

/// `is_primitive_polynomial(f, p)`.
fn is_primitive_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, p] = args else { return decline() };
    let Some(p) = small_prime(h, *p) else { return decline() };
    let Some(x) = single_variable(h, s, &[*f]) else { return decline() };
    let fp = Fp::new(p);
    let Some(poly) = fp_poly_of(h, s, *f, x, fp) else { return decline() };
    if poly.len() < 2 {
        return done(cx.bool(false));
    }
    // a primitive polynomial is monic (the minimal polynomial of a generator of F_p^n*): 4 x^2 + 2 x + 6 and
    // x^2/2 + ... over F_7 are not, though they are multiples of one
    if poly.last() != Some(&1) {
        return done(cx.bool(false));
    }
    let n = poly.len() - 1;
    let big_n = Nat::pow(p, n as u64).sub_u64(1);
    let Some(fs) = crate::field::factor_capped(&big_n, cx)? else { return decline() };
    let primes: Vec<Nat> = fs.into_iter().map(|x| x.0).collect();
    let m = fp::monic(fp, &poly);
    // x itself is primitive only in F_2 = F_2[x]/(x + 1)... and x | f is never primitive for n >= 1 unless f = x + c
    if m[0] == 0 {
        return done(cx.bool(false));
    }
    {
        let r = conway::is_primitive_poly(fp, &m, &big_n, &primes, cx)?;
        done(cx.bool(r))
    }
}

// ------------------------------------------------------------------------------------------- embeddings

/// The data of an embedding term `finite_field_embedding(gK -> b)`.
struct Emb<'h> {
    small: Arc<Field>,
    big: Arc<Field>,
    small_t: T<'h>,
    big_t: T<'h>,
    e: Embedding,
}

fn embedding_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, t: T<'h>) -> Res<Option<Emb<'h>>> {
    let h = cx.heap();
    let Some([r]) = call_of(h, t, s.emb) else { return Ok(None) };
    let Some([a, b]) = call_of(h, *r, s.rule) else { return Ok(None) };
    let Some(ea) = elem_of(h, s, *a, cx)? else { return Ok(None) };
    let Some(eb) = elem_of(h, s, *b, cx)? else { return Ok(None) };
    let Some(e) = embed::from_rule(&ea.field, &eb.field, &ea.el, &eb.el) else { return Ok(None) };
    Ok(Some(Emb { small: ea.field, big: eb.field, small_t: ea.fterm, big_t: eb.fterm, e }))
}

/// `finite_field_embedding(K, F)`, `finite_field_embedding(a -> b)`, and the calls `E(a)`, `E("Projection")(b)`.
fn finite_field_embedding<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let View::App { head, .. } = h.view(t) else { return decline() };
    if !matches!(h.view(head), View::Sym(_)) {
        return embedding_call(cx, s, head, args);
    }
    let (small, big, image, small_t, big_t) = match args {
        [k, f] if call_of(h, *k, s.ff).is_some() => {
            let Some(small) = field_of(h, s, *k, cx)? else { return decline() };
            let Some(big) = field_of(h, s, *f, cx)? else { return decline() };
            let Some(img) = embed::canonical(&small, &big, cx)? else { return decline() };
            (small, big, img, *k, *f)
        }
        [r] => {
            let Some(e) = embedding_of(cx, s, t)? else {
                // a rule a -> b: canonical when a is the generator
                let Some([a, b]) = call_of(h, *r, s.rule) else { return decline() };
                let Some(ea) = elem_of(h, s, *a, cx)? else { return decline() };
                let Some(eb) = elem_of(h, s, *b, cx)? else { return decline() };
                let Some(e) = embed::from_rule(&ea.field, &eb.field, &ea.el, &eb.el) else {
                    // elements of two fields, the second not holding a root of the first one's minimal polynomial
                    return done(arg_failure(h, "finite_field_embedding", "the image is not a root of the minimal polynomial of the generator"));
                };
                return embedding_term(cx, s, &ea.field, &eb.field, ea.fterm, eb.fterm, &e.image, Some(t));
            };
            (e.small, e.big, e.e.image, e.small_t, e.big_t)
        }
        _ => return decline(),
    };
    embedding_term(cx, s, &small, &big, small_t, big_t, &image, Some(t))
}

#[allow(clippy::too_many_arguments)]
fn embedding_term<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &Syms,
    small: &Field,
    big: &Field,
    small_t: T<'h>,
    big_t: T<'h>,
    image: &El,
    t: Option<T<'h>>,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(g) = elem_term(h, s, small, small_t, &small.gen(), cx)? else { return decline() };
    let Some(im) = elem_term(h, s, big, big_t, image, cx)? else { return decline() };
    let canon = h.call(s.emb, &[h.call(s.rule, &[g, im])]);
    // the canonical term: the call itself when it already is
    done(t.filter(|&t| t == canon).unwrap_or(canon))
}

/// `E(a)` and `E("Projection")(b)`.
fn embedding_call<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, head: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return decline() };
    if call_of(h, head, s.emb).is_some() {
        if let View::Str(_) = h.view(*x) {
            // E("Projection") stays as it is, to be called
            return decline();
        }
        let Some(e) = embedding_of(cx, s, head)? else { return decline() };
        let el = if let Some(c) = residue(h, *x, e.small.fp) {
            e.small.constant(c)
        } else {
            match elem_of(h, s, *x, cx)? {
                Some(a) if a.field.same(&e.small) => a.el,
                _ => return decline(),
            }
        };
        let v = e.e.apply(&e.small, &e.big, &el);
        return match elem_term(h, s, &e.big, e.big_t, &v, cx)? {
            Some(t) => done(t),
            None => decline(),
        };
    }
    // E("Projection")(b)
    let View::App { head: inner, args: pa } = h.view(head) else { return decline() };
    if !matches!(pa, [p] if matches!(h.view(*p), View::Str("Projection"))) {
        return decline();
    }
    let Some(e) = embedding_of(cx, s, inner)? else { return decline() };
    let el = if let Some(c) = residue(h, *x, e.big.fp) {
        e.big.constant(c)
    } else {
        match elem_of(h, s, *x, cx)? {
            Some(a) if a.field.same(&e.big) => a.el,
            _ => return decline(),
        }
    };
    let Some(v) = e.e.project(&e.small, &e.big, &el) else { return decline() };
    match elem_term(h, s, &e.small, e.small_t, &v, cx)? {
        Some(t) => done(t),
        None => decline(),
    }
}

// ------------------------------------------------------------------------------------------- the wrappers

/// What a wrapper does on elements.
#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Mul,
    Pow,
    /// `equal` (or `unequal` when true)
    Equal(bool),
    Element,
    MinimalPolynomial,
    /// `multiplicative_order(a)` of an element (D26-09)
    Order,
    /// `factor` (`factor_square_free` when true)
    Factor(bool),
    Expand,
    /// `polynomial_gcd` (`polynomial_lcm` when true)
    Gcd(bool),
    /// `polynomial_quotient` (`polynomial_remainder` when true)
    Div(bool),
    /// `cancel` and `together`: a quotient of coprime polynomials
    Cancel,
    /// a function computing with the arithmetic directly (`dot`, `det`, ...): its value evaluated again, so that
    /// the sums and products of elements in it combine
    Reeval,
}

/// A native of another library with the element cases in front.
struct Wrap {
    inner: Box<dyn Native>,
    op: Op,
    syms: Syms,
    /// the wrapped symbol
    name: mc_core::Sym,
}

impl Native for Wrap {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let s = &self.syms;
        let shallow = args.iter().any(|&a| call_of(h, a, s.ffe).is_some());
        let r = match self.op {
            Op::Add | Op::Mul if shallow => arith(cx, s, &*self.inner, t, args, matches!(self.op, Op::Mul))?,
            Op::Pow if shallow => power(cx, s, args)?,
            Op::Equal(neg) if shallow => equality(cx, s, args, neg)?,
            Op::Equal(neg) if args.len() == 2 && deep_elem(h, s, args) => deep_equality(cx, s, args, neg)?,
            Op::Element if args.len() == 2 && call_of(h, args[1], s.ff).is_some() => element(cx, s, args)?,
            Op::MinimalPolynomial if shallow => minpoly(cx, s, args)?,
            Op::Order if shallow && args.len() == 1 => order(cx, s, args[0])?,
            Op::Factor(_) if prime_power_modulus(h, s, args) => as_extension(cx, s, self.name, args)?,
            Op::Factor(_) | Op::Expand | Op::Gcd(_) | Op::Div(_) | Op::Cancel if deep_elem(h, s, args) => polynomial(cx, s, self.op, args)?,
            Op::Reeval if deep_elem(h, s, args) => {
                return match self.inner.call(cx, t, args)? {
                    Reply::Done(v) if call_of(h, v, self.name).is_none() => Ok(Reply::Again(v)),
                    r => Ok(r),
                };
            }
            _ => None,
        };
        match r {
            Some(r) => Ok(r),
            None => self.inner.call(cx, t, args),
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

/// Whether an element occurs in the arguments (or a field is the `extension:`).
fn deep_elem(h: &Heap, s: &Syms, args: &[T<'_>]) -> bool {
    if let Some(e) = option(h, s, args, "extension") {
        if call_of(h, e, s.ff).is_some() {
            return true;
        }
    }
    let mut stack: Vec<T<'_>> = plain_args(h, s, args).to_vec();
    let mut n = 0;
    while let Some(t) = stack.pop() {
        n += 1;
        if n > 50_000 {
            return false;
        }
        if let View::App { head, args } = h.view(t) {
            if matches!(h.view(head), View::Sym(x) if x == s.ffe) {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Sums and products: the elements of each field combined (exact numbers joining when there is one field).
fn arith<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, inner: &dyn Native, t: T<'h>, args: &[T<'h>], is_mul: bool) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let mut groups: Vec<(Elem<'h>, usize)> = vec![];
    let mut numbers: Vec<T<'h>> = vec![];
    let mut rest: Vec<T<'h>> = vec![];
    for &a in args {
        if call_of(h, a, s.ffe).is_some() {
            if let Some(e) = elem_of(h, s, a, cx)? {
                if let Some(g) = groups.iter_mut().find(|g| g.0.field.same(&e.field)) {
                    g.0.el = if is_mul { g.0.field.mul(&g.0.el, &e.el) } else { g.0.field.add(&g.0.el, &e.el) };
                    g.1 += 1;
                } else {
                    groups.push((e, 1));
                }
                continue;
            }
        }
        if is_rational(h, a) {
            numbers.push(a);
        } else {
            rest.push(a);
        }
    }
    if groups.is_empty() {
        return Ok(None);
    }
    let mut changed = groups.iter().any(|g| g.1 > 1);
    if groups.len() == 1 {
        let g = &mut groups[0];
        let mut keep = vec![];
        for &c in &numbers {
            match residue(h, c, g.0.field.fp) {
                Some(r) => {
                    let v = g.0.field.constant(r);
                    g.0.el = if is_mul { g.0.field.mul(&g.0.el, &v) } else { g.0.field.add(&g.0.el, &v) };
                    changed = true;
                }
                None => keep.push(c),
            }
        }
        numbers = keep;
    }
    rest.extend(numbers);
    let mut out = rest.clone();
    for (g, _) in &groups {
        // 0 + x is x; 0 x is 0
        if g.el.is_empty() && !is_mul && !rest.is_empty() {
            changed = true;
            continue;
        }
        if g.el.is_empty() && is_mul && groups.len() == 1 {
            return Ok(elem_like(cx, s, g, &g.el)?.map(Reply::Done));
        }
        let Some(term) = elem_like(cx, s, g, &g.el)? else { return Ok(None) };
        out.push(term);
    }
    if out.len() == 1 {
        return Ok(Some(Reply::Done(out[0])));
    }
    if !changed {
        return Ok(None);
    }
    inner.call(cx, t, &out).map(Some)
}

/// An element to an integer power.
fn power<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, args: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let [b, e] = args else { return Ok(None) };
    let Some(k) = int_of(h, *e) else { return Ok(None) };
    let Some(be) = elem_of(h, s, *b, cx)? else { return Ok(None) };
    let Some(v) = be.field.pow_int(&be.el, &k) else {
        return Ok(Some(Reply::Done(h.call(h.sym(names::FAILURE), &[sym(h, names::failure::DIVISION_BY_ZERO)]))));
    };
    Ok(elem_like(cx, s, &be, &v)?.map(Reply::Done))
}

/// The element of a term in the field of `like` (an element of it or an exact number).
fn coerce(h: &Heap, s: &Syms, like: &Field, t: T<'_>, b: &mut dyn Budget) -> Res<Option<El>> {
    if let Some(r) = residue(h, t, like.fp) {
        return Ok(Some(like.constant(r)));
    }
    match elem_of(h, s, t, b)? {
        Some(e) if e.field.same(like) => Ok(Some(e.el)),
        _ => Ok(None),
    }
}

fn equality<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, args: &[T<'h>], neg: bool) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let [a, b] = args else { return Ok(None) };
    let first = if call_of(h, *a, s.ffe).is_some() { *a } else { *b };
    let Some(e) = elem_of(h, s, first, cx)? else { return Ok(None) };
    let Some(x) = coerce(h, s, &e.field, *a, cx)? else { return Ok(None) };
    let Some(y) = coerce(h, s, &e.field, *b, cx)? else { return Ok(None) };
    Ok(Some(Reply::Done(cx.bool((x == y) != neg))))
}

/// `element(x, F)` for a field F.
fn element<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, args: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let [x, ft] = args else { return Ok(None) };
    let Some(f) = field_of(h, s, *ft, cx)? else { return Ok(None) };
    if is_rational(h, *x) {
        return Ok(Some(Reply::Done(cx.bool(residue(h, *x, f.fp).is_some()))));
    }
    if call_of(h, *x, s.ffe).is_some() {
        let e = elem_of(h, s, *x, cx)?;
        return Ok(Some(Reply::Done(cx.bool(e.is_some_and(|e| e.field.same(&f))))));
    }
    Ok(None)
}

/// `minimal_polynomial(a, x)` and `minimal_polynomial(a, x, k)` for an element a.
fn minpoly<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, args: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let (a, x, k) = match args {
        [a, x] => (*a, *x, 1usize),
        [a, x, k] => match h.view(*k) {
            View::Int(k) if k >= 1 => (*a, *x, k as usize),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    let Some(e) = elem_of(h, s, a, cx)? else { return Ok(None) };
    Ok(minimal_polynomial_term(cx, s, &e, x, k)?.map(Reply::Again))
}

/// The factorisation over F_q: over the prime field by `mc-poly`'s mod-p code, else by `fqpoly`.
fn factor_any(k: &Field, a: &[El], b: &mut dyn Budget) -> Res<(El, Vec<(QPoly, u32)>)> {
    if k.n == 1 {
        let up: Vec<u64> = a.iter().map(|c| c.first().copied().unwrap_or(0)).collect();
        let mut w = BudgetWork { b, stop: None };
        let r = {
            let mut m = mc_poly::work::Meter::new(&mut w);
            mc_poly::factor::factor_mod_p(mc_poly::modp::Fp::new(k.p()), &up, &mut m)
        };
        match r {
            Ok((lc, fs)) => {
                let mut out: Vec<(QPoly, u32)> = fs
                    .into_iter()
                    .map(|(f, e)| (f.into_iter().map(|c| k.constant(c)).collect(), e))
                    .collect();
                fqpoly::sort_factors(k, &mut out);
                return Ok((k.constant(lc), out));
            }
            Err(_) => return Err(w.stop.unwrap_or(Abort::Limit)),
        }
    }
    fqpoly::factor(k, a, b)
}

/// The evaluator's budget as `mc-poly`'s work budget.
struct BudgetWork<'a> {
    b: &'a mut dyn Budget,
    stop: Option<Abort>,
}

impl mc_poly::work::Work for BudgetWork<'_> {
    fn charge(&mut self, units: u64) -> mc_poly::work::WorkResult<()> {
        self.b.charge(units).map_err(|e| {
            self.stop = Some(e);
            mc_poly::work::Halt::Stopped
        })
    }
}

/// The polynomial functions over a finite field.
fn polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, op: Op, args: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let plain = plain_args(h, s, args);
    // the field: the extension, else the elements' field
    let (k, fterm) = match option(h, s, args, "extension") {
        Some(ext) if call_of(h, ext, s.ff).is_some() => match field_of(h, s, ext, cx)? {
            Some(f) => (f, ext),
            None => return Ok(None),
        },
        Some(_) => return Ok(None),
        None => match find_elem(h, s, plain, cx)? {
            Some(e) => (e.field, e.fterm),
            None => return Ok(None),
        },
    };
    let (polys, x) = match op {
        Op::Div(_) => match plain {
            [a, b, x] if matches!(h.view(*x), View::Sym(_)) => (vec![*a, *b], *x),
            _ => return Ok(None),
        },
        Op::Gcd(_) => {
            let Some(x) = single_variable(h, s, plain) else { return Ok(None) };
            (plain.to_vec(), x)
        }
        _ => {
            let [a] = plain else { return Ok(None) };
            match single_variable(h, s, &[*a]) {
                Some(x) => (vec![*a], x),
                None => {
                    // a constant: itself, as an element
                    let Some(c) = q_constant(h, s, &k, *a, cx)? else { return Ok(None) };
                    return Ok(elem_term(h, s, &k, fterm, &c, cx)?.map(Reply::Done));
                }
            }
        }
    };
    if let Op::Cancel = op {
        let Some((n, d)) = q_rat_of(h, s, &k, polys[0], x, cx)? else { return Ok(None) };
        if d.is_empty() {
            return Ok(None);
        }
        if n.is_empty() {
            return Ok(Some(Reply::Done(h.int(0))));
        }
        // num/den coprime, the numerator monic; a constant denominator joins the leading coefficient
        let g = fqpoly::gcd(&k, &n, &d);
        let n = fqpoly::divrem(&k, &n, &g).0;
        let d = fqpoly::divrem(&k, &d, &g).0;
        // invariant: n and d are nonzero
        let ln = n.last().unwrap().clone();
        let inv_ln = k.inv(&ln).expect("a unit");
        let n1 = fqpoly::scale(&k, &n, &inv_ln);
        let d1 = fqpoly::scale(&k, &d, &inv_ln);
        let Some(nt) = q_poly_term(h, s, &k, fterm, &n1, x, cx)? else { return Ok(None) };
        let out = if d1.len() == 1 {
            // invariant: d1 is a nonzero constant
            let c = k.inv(&d1[0]).expect("a unit");
            let Some(ct) = elem_term(h, s, &k, fterm, &c, cx)? else { return Ok(None) };
            h.call(s.mul, &[ct, nt])
        } else {
            let Some(dt) = q_poly_term(h, s, &k, fterm, &d1, x, cx)? else { return Ok(None) };
            h.call(s.mul, &[nt, h.call(s.pow, &[dt, h.int(-1)])])
        };
        return Ok(Some(Reply::Again(out)));
    }
    let mut ps = vec![];
    for &p in &polys {
        match q_poly_of(h, s, &k, p, x, cx)? {
            Some(v) => ps.push(v),
            None => return Ok(None),
        }
    }
    let term = |cx: &mut NativeCx<'_, '_, 'h>, p: &[El]| -> Res<Option<T<'h>>> { q_poly_term(h, s, &k, fterm, p, x, cx) };
    let out = match op {
        Op::Expand => term(cx, &ps[0])?,
        Op::Div(rem) => {
            if ps[1].is_empty() {
                return Ok(None);
            }
            let (q, r) = fqpoly::divrem(&k, &ps[0], &ps[1]);
            term(cx, if rem { &r } else { &q })?
        }
        Op::Gcd(lcm) => {
            let mut acc = if lcm { vec![k.one()] } else { vec![] };
            for p in &ps {
                if lcm {
                    if p.is_empty() {
                        acc = vec![];
                        break;
                    }
                    let g = fqpoly::gcd(&k, &acc, p);
                    acc = fqpoly::monic(&k, &fqpoly::divrem(&k, &fqpoly::mul(&k, &acc, p), &g).0);
                } else {
                    acc = fqpoly::gcd(&k, &acc, p);
                }
            }
            term(cx, &acc)?
        }
        Op::Factor(square_free) => {
            let a = &ps[0];
            if a.len() < 2 {
                let c = a.first().cloned().unwrap_or_default();
                return Ok(elem_term(h, s, &k, fterm, &c, cx)?.map(Reply::Done));
            }
            let (lc, fs) = if square_free {
                let lc = a.last().cloned().unwrap_or_default();
                (lc, fqpoly::square_free(&k, a, cx)?)
            } else {
                factor_any(&k, a, cx)?
            };
            let mut parts = vec![];
            if lc != k.one() {
                let Some(c) = elem_term(h, s, &k, fterm, &lc, cx)? else { return Ok(None) };
                parts.push(c);
            }
            for (f, e) in &fs {
                let Some(ft) = q_poly_term(h, s, &k, fterm, f, x, cx)? else { return Ok(None) };
                parts.push(if *e == 1 { ft } else { h.call(s.pow, &[ft, h.int(*e as i64)]) });
            }
            Some(if parts.len() == 1 { parts[0] } else { h.call(s.mul, &parts) })
        }
        _ => None,
    };
    Ok(out.map(Reply::Again))
}

/// `multiplicative_order(a)` of a nonzero element: the least k with a^k = 1 (unevaluated when q - 1 cannot be
/// factored within the cap, and for 0, which has no order).
fn order<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, a: T<'h>) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let Some(e) = elem_of(h, s, a, cx)? else { return Ok(None) };
    Ok(e.field.element_order(&e.el, cx)?.map(|k| Reply::Done(nat_term(h, &k))))
}

/// `p == q` for polynomials in one variable with elements of a field inside (and integers, which are numbers of its
/// prime field): equal when they are the same polynomial over the field (D26-07: `expand(factor(p, extension: F))`
/// against `p`). Unevaluated when a side is no such polynomial.
fn deep_equality<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, args: &[T<'h>], neg: bool) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let [a, b] = args else { return Ok(None) };
    let Some(e) = find_elem(h, s, args, cx)? else { return Ok(None) };
    let Some(x) = single_variable(h, s, args) else { return Ok(None) };
    let Some(pa) = q_poly_of(h, s, &e.field, *a, x, cx)? else { return Ok(None) };
    let Some(pb) = q_poly_of(h, s, &e.field, *b, x, cx)? else { return Ok(None) };
    Ok(Some(Reply::Done(cx.bool((pa == pb) != neg))))
}

/// Whether the call has `modulus: q` for an integer q that is not prime.
fn prime_power_modulus(h: &Heap, s: &Syms, args: &[T<'_>]) -> bool {
    option(h, s, args, "modulus").is_some_and(|m| nat_of(h, m).is_some_and(|q| q > Nat::from_u64(3) && !is_probable_prime(&q)))
}

fn is_probable_prime(q: &Nat) -> bool {
    q.to_u128().is_some_and(|v| v < 1 << 62 && mc_numtheory::word::is_prime_u64(v as u64))
}

/// `f(p, modulus: q)` with q = r^e (e > 1) as `f(p, extension: F_q)`: the polynomial functions over the field of q
/// elements (D26-08). Unevaluated for a q that is no prime power.
fn as_extension<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: &Syms, name: mc_core::Sym, args: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let Some(m) = option(h, s, args, "modulus") else { return Ok(None) };
    let Some(q) = nat_of(h, m) else { return Ok(None) };
    let (base, e) = mc_numtheory::power::perfect_power(&q, cx)?;
    let Some(p) = base.to_u128().filter(|&v| v < 1 << 62).map(|v| v as u64) else { return Ok(None) };
    if !(2..=4096).contains(&e) || !mc_numtheory::word::is_prime_u64(p) {
        return Ok(None);
    }
    let Some(field) = standard_field(cx, p, e as usize, Repr::Polynomial)? else { return Ok(None) };
    let ft = field_term(h, s, &field);
    let (Some((&opts, rest)), true) = (args.split_last(), true) else { return Ok(None) };
    let Some(rules) = call_of(h, opts, s.options) else { return Ok(None) };
    let mut new_rules: Vec<T<'h>> = rules
        .iter()
        .copied()
        .filter(|&r| !matches!(call_of(h, r, s.rule), Some(&[k, _]) if k == h.sym_term(h.sym("modulus"))))
        .collect();
    new_rules.push(h.call(s.rule, &[h.sym_term(h.sym("extension")), ft]));
    let mut out = rest.to_vec();
    out.push(h.call(s.options, &new_rules));
    Ok(Some(Reply::Again(h.call(name, &out))))
}
