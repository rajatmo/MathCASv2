//! Terms of the language and the crate's data: reading and writing fields, elements and polynomials.
//!
//! - A field is the term `finite_field(p, &(c0 + c1*&1 + ... + &1^n), "Polynomial")` (or `"Exponential"`): the
//!   characteristic, the defining polynomial as a pure function and the element representation. Its canonical form
//!   has the polynomial's terms by increasing degree, a unit coefficient left out, so that it reads back to itself.
//! - An element is `finite_field_element(F, [c0, c1, ...])` (coefficients of its residue, no trailing zeros) in a
//!   field with the polynomial representation, and `finite_field_element(F, k)` (0 is zero, k >= 1 is the
//!   generator to the power k - 1) in one with the exponential representation.
//! - Polynomials over a field are ordinary sums of products of elements and powers of a variable.
use crate::field::{El, Field, Repr};
use crate::fp::{self, Fp, Poly};
use crate::fqpoly::QPoly;
use mc_api::names;
use mc_arith::num::{Int, Nat};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::Res;
use mc_numtheory::budget::Budget;
use std::sync::Arc;

/// The symbols the crate reads and writes.
#[derive(Clone, Copy)]
pub struct Syms {
    /// `finite_field`
    pub ff: Sym,
    /// `finite_field_element`
    pub ffe: Sym,
    /// `finite_field_embedding`
    pub emb: Sym,
    /// `fn`
    pub fun: Sym,
    /// `list`
    pub list: Sym,
    /// `add`
    pub add: Sym,
    /// `mul`
    pub mul: Sym,
    /// `pow`
    pub pow: Sym,
    /// `rule`
    pub rule: Sym,
    /// `options`
    pub options: Sym,
    /// `&1`
    pub slot1: Sym,
}

impl Syms {
    /// The symbols of the heap.
    pub fn new(h: &Heap) -> Syms {
        Syms {
            ff: h.sym("finite_field"),
            ffe: h.sym("finite_field_element"),
            emb: h.sym("finite_field_embedding"),
            fun: h.sym("fn"),
            list: h.sym(names::LIST),
            add: h.sym(names::ADD),
            mul: h.sym(names::MUL),
            pow: h.sym(names::POW),
            rule: h.sym(names::RULE),
            options: h.sym("options"),
            slot1: h.sym("&1"),
        }
    }
}

/// The arguments of `t` when it is a call with the symbol head `s`.
pub fn call_of<'h>(h: &'h Heap, t: T<'h>, s: Sym) -> Option<&'h [T<'h>]> {
    match h.view(t) {
        View::App { head, args } if matches!(h.view(head), View::Sym(x) if x == s) => Some(args),
        _ => None,
    }
}

/// An exact integer.
pub fn int_of(h: &Heap, t: T<'_>) -> Option<Int> {
    match h.view(t) {
        View::Int(v) => Some(Int::from_i64(v)),
        View::BigInt { neg, mag } => Some(Int::new(neg, Nat::from_limbs(mag.to_vec()))),
        _ => None,
    }
}

/// A non-negative exact integer.
pub fn nat_of(h: &Heap, t: T<'_>) -> Option<Nat> {
    int_of(h, t).filter(|i| !i.neg).map(|i| i.mag)
}

/// The term of a natural number.
pub fn nat_term<'h>(h: &'h Heap, n: &Nat) -> T<'h> {
    h.big_int(false, &n.limbs)
}

/// The residue modulo p of an integer or a rational whose denominator is prime to p.
pub fn residue(h: &Heap, t: T<'_>, f: Fp) -> Option<u64> {
    match h.view(t) {
        View::Int(v) => Some(f.of_i64(v)),
        View::BigInt { neg, mag } => {
            let r = f.of_nat(&Nat::from_limbs(mag.to_vec()));
            Some(if neg { f.neg(r) } else { r })
        }
        View::Rational { num, den } => {
            let n = residue(h, num, f)?;
            let d = residue(h, den, f)?;
            Some(f.mul(n, f.inv(d)?))
        }
        _ => None,
    }
}

/// Whether `t` is an exact integer or rational.
pub fn is_rational(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
}

/// The polynomial over F_p of a term in the variable `x` built from exact numbers with `add`, `mul` and integer
/// `pow` (the body of a defining polynomial).
pub fn fp_poly_of(h: &Heap, s: &Syms, t: T<'_>, x: T<'_>, f: Fp) -> Option<Poly> {
    if t == x {
        return Some(vec![0, 1]);
    }
    if let Some(c) = residue(h, t, f) {
        let mut v = vec![c];
        fp::trim(&mut v);
        return Some(v);
    }
    let View::App { head, args } = h.view(t) else { return None };
    let View::Sym(hs) = h.view(head) else { return None };
    if hs == s.add {
        let mut acc = vec![];
        for &a in args {
            acc = fp::add(f, &acc, &fp_poly_of(h, s, a, x, f)?);
        }
        Some(acc)
    } else if hs == s.mul {
        let mut acc = vec![1 % f.p];
        for &a in args {
            acc = fp::mul(f, &acc, &fp_poly_of(h, s, a, x, f)?);
        }
        Some(acc)
    } else if hs == s.pow && args.len() == 2 {
        let e = match h.view(args[1]) {
            View::Int(e) if (0..=1 << 16).contains(&e) => e as u64,
            _ => return None,
        };
        let b = fp_poly_of(h, s, args[0], x, f)?;
        let mut acc = vec![1 % f.p];
        for _ in 0..e {
            acc = fp::mul(f, &acc, &b);
        }
        Some(acc)
    } else {
        None
    }
}

/// The parameter and body of a pure function of one argument (`&(...)`, `x => ...`).
pub fn function_parts<'h>(h: &'h Heap, s: &Syms, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let args = call_of(h, t, s.fun)?;
    let [params, body] = args else { return None };
    let ps = call_of(h, *params, s.list)?;
    let [x] = ps else { return None };
    matches!(h.view(*x), View::Sym(_)).then_some((*x, *body))
}

/// The pieces of a field term: prime, defining polynomial and representation (no validity check).
pub fn field_parts(h: &Heap, s: &Syms, t: T<'_>) -> Option<(u64, Poly, Repr)> {
    let args = call_of(h, t, s.ff)?;
    let [p, f, r] = args else { return None };
    let p = match h.view(*p) {
        View::Int(v) if (2..1 << 62).contains(&v) => v as u64,
        _ => return None,
    };
    let repr = match h.view(*r) {
        View::Str("Polynomial") => Repr::Polynomial,
        View::Str("Exponential") => Repr::Exponential,
        _ => return None,
    };
    let (x, body) = function_parts(h, s, *f)?;
    let poly = fp_poly_of(h, s, body, x, Fp::new(p))?;
    Some((p, poly, repr))
}

/// The field of a field term (validated on first use: p prime, the polynomial irreducible, primitive for the
/// exponential representation).
pub fn field_of(h: &Heap, s: &Syms, t: T<'_>, b: &mut dyn Budget) -> Res<Option<Arc<Field>>> {
    let Some((p, poly, repr)) = field_parts(h, s, t) else { return Ok(None) };
    checked_field(p, &poly, repr, b)
}

/// The field F_p[x]/(poly) after the checks of [`field_of`].
pub fn checked_field(p: u64, poly: &[u64], repr: Repr, b: &mut dyn Budget) -> Res<Option<Arc<Field>>> {
    checked_field_with(p, poly, repr, false, b)
}

/// [`checked_field`] for a polynomial the caller knows to be irreducible (one found by the Conway search): the test of
/// irreducibility, which takes seconds at degree 300 and cannot be interrupted, is skipped.
pub fn checked_field_with(p: u64, poly: &[u64], repr: Repr, irreducible: bool, b: &mut dyn Budget) -> Res<Option<Arc<Field>>> {
    if let Some(f) = Field::lookup(p, poly, repr) {
        return Ok(Some(f));
    }
    if !mc_numtheory::word::is_prime_u64(p) || p >= 1 << 62 {
        return Ok(None);
    }
    let fp = Fp::new(p);
    let mut poly = poly.to_vec();
    fp::trim(&mut poly);
    if poly.len() < 2 || poly.len() > 4097 || (!irreducible && !fp::is_irreducible(fp, &poly)) {
        return Ok(None);
    }
    let f = Field::get(p, &poly, repr);
    if repr == Repr::Exponential && f.is_primitive(&f.gen(), b)? != Some(true) {
        return Ok(None);
    }
    Field::mark_checked(&f);
    Ok(Some(f))
}

/// The canonical term of a field.
pub fn field_term<'h>(h: &'h Heap, s: &Syms, f: &Field) -> T<'h> {
    let x = h.sym_term(s.slot1);
    let mut parts = vec![];
    for (i, &c) in f.given.iter().enumerate() {
        if c == 0 {
            continue;
        }
        let xi = match i {
            0 => None,
            1 => Some(x),
            _ => Some(h.call(s.pow, &[x, h.int(i as i64)])),
        };
        parts.push(match (xi, c) {
            (None, c) => h.int(c as i64),
            (Some(xi), 1) => xi,
            (Some(xi), c) => h.call(s.mul, &[h.int(c as i64), xi]),
        });
    }
    let body = if parts.len() == 1 { parts[0] } else { h.call(s.add, &parts) };
    let fun = h.call(s.fun, &[h.call(s.list, &[x]), body]);
    h.call(s.ff, &[h.int(f.p() as i64), fun, h.string(f.repr.name())])
}

/// An element read from a term: its field, the field's term and the element.
pub struct Elem<'h> {
    /// the field
    pub field: Arc<Field>,
    /// the field's term (canonical when the element is)
    pub fterm: T<'h>,
    /// the element
    pub el: El,
}

/// The element a canonical element term stands for.
pub fn elem_of<'h>(h: &'h Heap, s: &Syms, t: T<'h>, b: &mut dyn Budget) -> Res<Option<Elem<'h>>> {
    let Some(args) = call_of(h, t, s.ffe) else { return Ok(None) };
    let [ft, data] = args else { return Ok(None) };
    let Some(field) = field_of(h, s, *ft, b)? else { return Ok(None) };
    let Some(el) = data_elem(h, s, &field, *data) else { return Ok(None) };
    Ok(Some(Elem { field, fterm: *ft, el }))
}

/// The element of `f` given by `data`: an index (by the field's representation) or a list of coefficients.
pub fn data_elem(h: &Heap, s: &Syms, f: &Field, data: T<'_>) -> Option<El> {
    if let Some(cs) = call_of(h, data, s.list) {
        let v: Option<Vec<u64>> = cs.iter().map(|&c| residue(h, c, f.fp)).collect();
        return Some(f.reduce(&v?));
    }
    // a negative integer or a rational is a number of the prime field (-1 is p - 1, 1/2 the inverse of 2; D26-06, 21)
    if nat_of(h, data).is_none() {
        let c = residue(h, data, f.fp)?;
        return Some(f.constant(c));
    }
    // an index beyond q: its digits reduced modulo f (an exponential index modulo q - 1)
    let k = nat_of(h, data)?;
    if k.bits() > 1 << 16 {
        return None;
    }
    Some(match f.repr {
        Repr::Polynomial => f.from_index(&k),
        Repr::Exponential => f.from_exp_index(&k),
    })
}

/// The canonical element term (None when an exponential index could not be found).
pub fn elem_term<'h>(h: &'h Heap, s: &Syms, f: &Field, fterm: T<'h>, a: &El, b: &mut dyn Budget) -> Res<Option<T<'h>>> {
    let data = match f.repr {
        Repr::Polynomial => coeff_list(h, s, a),
        Repr::Exponential => match f.exp_index(a, b)? {
            Some(k) => nat_term(h, &k),
            None => return Ok(None),
        },
    };
    Ok(Some(h.call(s.ffe, &[fterm, data])))
}

/// The list of coefficients of an element.
pub fn coeff_list<'h>(h: &'h Heap, s: &Syms, a: &El) -> T<'h> {
    let items: Vec<T<'h>> = a.iter().map(|&c| h.int(c as i64)).collect();
    h.call(s.list, &items)
}

/// The defining polynomial of a field as a pure function (the term the field shows).
pub fn defining_function<'h>(h: &'h Heap, s: &Syms, f: &Field) -> T<'h> {
    let ft = field_term(h, s, f);
    // invariant: field_term builds finite_field(p, fn, repr)
    call_of(h, ft, s.ff).expect("a field term")[1]
}

/// The polynomial with these F_p coefficients in the variable x.
pub fn fp_poly_term<'h>(h: &'h Heap, s: &Syms, a: &[u64], x: T<'h>) -> T<'h> {
    let mut parts = vec![];
    for (i, &c) in a.iter().enumerate() {
        if c == 0 {
            continue;
        }
        let xi = match i {
            0 => None,
            1 => Some(x),
            _ => Some(h.call(s.pow, &[x, h.int(i as i64)])),
        };
        parts.push(match (xi, c) {
            (None, c) => h.int(c as i64),
            (Some(xi), 1) => xi,
            (Some(xi), c) => h.call(s.mul, &[h.int(c as i64), xi]),
        });
    }
    match parts.len() {
        0 => h.int(0),
        1 => parts[0],
        _ => h.call(s.add, &parts),
    }
}

/// The polynomial over F_q in the variable x, its coefficients written as elements (zero coefficients left out; the
/// zero polynomial is 0).
pub fn q_poly_term<'h>(h: &'h Heap, s: &Syms, k: &Field, fterm: T<'h>, a: &[El], x: T<'h>, b: &mut dyn Budget) -> Res<Option<T<'h>>> {
    let mut parts = vec![];
    for (i, c) in a.iter().enumerate() {
        if c.is_empty() {
            continue;
        }
        let Some(ct) = elem_term(h, s, k, fterm, c, b)? else { return Ok(None) };
        parts.push(match i {
            0 => ct,
            1 => h.call(s.mul, &[ct, x]),
            _ => h.call(s.mul, &[ct, h.call(s.pow, &[x, h.int(i as i64)])]),
        });
    }
    Ok(Some(match parts.len() {
        0 => h.int(0),
        1 => parts[0],
        _ => h.call(s.add, &parts),
    }))
}

/// The first element found in the terms (depth first), with its field.
pub fn find_elem<'h>(h: &'h Heap, s: &Syms, ts: &[T<'h>], b: &mut dyn Budget) -> Res<Option<Elem<'h>>> {
    let mut stack: Vec<T<'h>> = ts.iter().rev().copied().collect();
    let mut seen = 0usize;
    while let Some(t) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            return Ok(None);
        }
        if call_of(h, t, s.ffe).is_some() {
            if let Some(e) = elem_of(h, s, t, b)? {
                return Ok(Some(e));
            }
            continue;
        }
        if let View::App { head, args } = h.view(t) {
            if call_of(h, t, s.ff).is_some() {
                continue;
            }
            stack.extend(args.iter().rev());
            stack.push(head);
        }
    }
    Ok(None)
}

/// The polynomial over the field `k` of a term in x: sums, products and non-negative integer powers of x, exact
/// numbers and elements of `k`. None when anything else occurs.
pub fn q_poly_of(h: &Heap, s: &Syms, k: &Field, t: T<'_>, x: T<'_>, b: &mut dyn Budget) -> Res<Option<QPoly>> {
    if t == x {
        return Ok(Some(vec![vec![], k.one()]));
    }
    if let Some(c) = residue(h, t, k.fp) {
        return Ok(Some(if c == 0 { vec![] } else { vec![k.constant(c)] }));
    }
    if is_rational(h, t) {
        return Ok(None);
    }
    let View::App { head, args } = h.view(t) else { return Ok(None) };
    let View::Sym(hs) = h.view(head) else { return Ok(None) };
    if hs == s.ffe {
        let Some(e) = elem_of(h, s, t, b)? else { return Ok(None) };
        if !e.field.same(k) {
            return Ok(None);
        }
        return Ok(Some(if e.el.is_empty() { vec![] } else { vec![e.el] }));
    }
    if hs == s.add {
        let mut acc = vec![];
        for &a in args {
            let Some(v) = q_poly_of(h, s, k, a, x, b)? else { return Ok(None) };
            acc = crate::fqpoly::add(k, &acc, &v);
        }
        return Ok(Some(acc));
    }
    if hs == s.mul {
        let mut acc = vec![k.one()];
        for &a in args {
            let Some(v) = q_poly_of(h, s, k, a, x, b)? else { return Ok(None) };
            acc = crate::fqpoly::mul(k, &acc, &v);
        }
        return Ok(Some(acc));
    }
    if hs == s.pow && args.len() == 2 {
        let e = match h.view(args[1]) {
            View::Int(e) if (0..=1 << 16).contains(&e) => e as u64,
            _ => return Ok(None),
        };
        let Some(base) = q_poly_of(h, s, k, args[0], x, b)? else { return Ok(None) };
        let mut acc = vec![k.one()];
        for _ in 0..e {
            acc = crate::fqpoly::mul(k, &acc, &base);
            b.charge(1)?;
        }
        return Ok(Some(acc));
    }
    Ok(None)
}

/// The variables of a polynomial term: symbols other than the heads, outside elements and fields.
pub fn variables<'h>(h: &'h Heap, s: &Syms, t: T<'h>, out: &mut Vec<T<'h>>) {
    match h.view(t) {
        View::Sym(_) => {
            if !out.contains(&t) {
                out.push(t);
            }
        }
        View::App { args, .. } => {
            if call_of(h, t, s.ffe).is_some() || call_of(h, t, s.ff).is_some() {
                return;
            }
            for &a in args {
                variables(h, s, a, out);
            }
        }
        _ => {}
    }
}

/// The value of an `options(...)` argument's rule `name -> v`.
pub fn option<'h>(h: &'h Heap, s: &Syms, args: &[T<'h>], name: &str) -> Option<T<'h>> {
    let last = *args.last()?;
    let rules = call_of(h, last, s.options)?;
    let key = h.sym_term(h.sym(name));
    rules.iter().find_map(|&r| match call_of(h, r, s.rule) {
        Some(&[k, v]) if k == key => Some(v),
        _ => None,
    })
}

/// The arguments without a trailing `options(...)`.
pub fn plain_args<'a, 'h>(h: &'h Heap, s: &Syms, args: &'a [T<'h>]) -> &'a [T<'h>] {
    match args.split_last() {
        Some((&last, rest)) if call_of(h, last, s.options).is_some() => rest,
        _ => args,
    }
}

/// A rational function over `k` in x, as (numerator, denominator), of a term built from sums, products, integer
/// powers, x, exact numbers and elements of `k`. None when anything else occurs or a denominator vanishes.
pub fn q_rat_of(h: &Heap, s: &Syms, k: &Field, t: T<'_>, x: T<'_>, b: &mut dyn Budget) -> Res<Option<(QPoly, QPoly)>> {
    use crate::fqpoly::{add, mul};
    let one = vec![k.one()];
    if let Some(p) = q_poly_of(h, s, k, t, x, b)? {
        return Ok(Some((p, one)));
    }
    let View::App { head, args } = h.view(t) else { return Ok(None) };
    let View::Sym(hs) = h.view(head) else { return Ok(None) };
    if hs == s.add || hs == s.mul {
        let (mut n, mut d) = (if hs == s.add { vec![] } else { one.clone() }, one.clone());
        for &a in args {
            let Some((an, ad)) = q_rat_of(h, s, k, a, x, b)? else { return Ok(None) };
            if hs == s.add {
                n = add(k, &mul(k, &n, &ad), &mul(k, &an, &d));
            } else {
                n = mul(k, &n, &an);
            }
            d = mul(k, &d, &ad);
            b.charge(1)?;
        }
        return Ok(Some((n, d)));
    }
    if hs == s.pow && args.len() == 2 {
        let e = match h.view(args[1]) {
            View::Int(e) if (-(1 << 16)..=1 << 16).contains(&e) => e,
            _ => return Ok(None),
        };
        let Some((bn, bd)) = q_rat_of(h, s, k, args[0], x, b)? else { return Ok(None) };
        let (bn, bd) = if e < 0 { (bd, bn) } else { (bn, bd) };
        if bd.is_empty() {
            return Ok(None);
        }
        let (mut n, mut d) = (one.clone(), one);
        for _ in 0..e.unsigned_abs() {
            n = mul(k, &n, &bn);
            d = mul(k, &d, &bd);
            b.charge(1)?;
        }
        return Ok(Some((n, d)));
    }
    Ok(None)
}
