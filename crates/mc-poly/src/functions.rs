//! The user-level functions: argument shapes, threading over lists, and the reading and writing of terms around
//! the kernels.
use crate::convert::{Conv, Ring, Terms};
use crate::natives::{Env, Out};
use crate::poly::Poly;
use crate::qpoly::{rat_of, rat_parts, QPoly};
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use crate::{factor, ratfun, resultant};
use mc_api::{Store, View, T};
use mc_arith::num::Rat;

/// Apply `f` to each element when `t` is a list (the functions that thread over lists, like `expand`).
fn thread<'h>(
    env: &Env<'h>,
    t: T<'h>,
    m: &mut Meter,
    f: &mut dyn FnMut(T<'h>, &mut Meter) -> WorkResult<Out<'h>>,
) -> WorkResult<Out<'h>> {
    match env.as_list(t) {
        Some(xs) => {
            let mut out = Vec::with_capacity(xs.len());
            let mut again = false;
            for &x in xs {
                match f(x, m)? {
                    Out::Done(v) => out.push(v),
                    Out::Again(v) => {
                        again = true;
                        out.push(v)
                    }
                    Out::Decline => out.push(x),
                }
            }
            let l = env.list(&out);
            Ok(if again { Out::Again(l) } else { Out::Done(l) })
        }
        None => f(t, m),
    }
}

/// `expand(e)`: sums, products and positive integer powers multiplied out (in the kernels of e).
pub(crate) fn expand<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [e] = args else {
        return Ok(env.arg_failure("expand(e) takes one argument"));
    };
    thread(env, *e, m, &mut |x, m| {
        let c = env.conv();
        let (ring, shared) = c.ring(&[x], &[], m)?;
        // a canonical sum of monomials in independent kernels is its own expansion
        if !ring.interacting() && c.monomials(x, &ring, m)?.is_some() {
            return Ok(Out::Done(x));
        }
        let p = c.to_poly(x, &ring, &shared, m)?;
        Ok(Out::Done(c.to_term(&p, &ring, m)?))
    })
}

/// The variable of a polynomial argument: a kernel `x`, or a power `x^n` (n > 0), which stands for `x` with
/// exponents scaled by n.
fn variable<'h>(env: &Env<'h>, x: T<'h>) -> Option<(T<'h>, u32)> {
    if !env.is_variable(x) {
        return None;
    }
    if let View::App { head, args } = env.h().view(x) {
        if head == env.hd.pow && args.len() == 2 {
            if let Some(n @ 1..=0xFFFF) = env.int(args[1]) {
                return Some((args[0], n as u32));
            }
        }
    }
    Some((x, 1))
}

/// The exponent of `x` in every term (`x^-1` kernels counted negatively), for the variable `x` of `ring`.
struct Degrees {
    pos: Option<usize>,
    neg: Option<usize>,
}

impl Degrees {
    fn new<'h>(env: &Env<'h>, ring: &Ring<'h>, x: T<'h>) -> Degrees {
        let inv = env.ar.pow(x, env.h().int(-1));
        Degrees { pos: ring.var(x), neg: ring.var(inv) }
    }
    fn of(&self, e: &[u32]) -> i64 {
        self.pos.map_or(0, |v| e[v] as i64) - self.neg.map_or(0, |v| e[v] as i64)
    }
    /// The term with `x` removed.
    fn strip(&self, e: &[u32]) -> Vec<u32> {
        let mut f = e.to_vec();
        for v in [self.pos, self.neg].into_iter().flatten() {
            f[v] = 0;
        }
        f
    }
}

/// The part of `p` of degree `k` in `x`, with `x` removed.
fn part_of_degree(p: &Terms, dg: &Degrees, k: i64) -> QPoly {
    p.select(|e| dg.of(e) == k, |e| dg.strip(e))
}

/// `coefficient(p, x, k)` (k = 1 when omitted): the coefficient of `x^k` in the expansion of p.
pub(crate) fn coefficient<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (p, x, k) = match args {
        [p, x] => (*p, *x, 1),
        [p, x, k] => match env.int(*k) {
            Some(k) => (*p, *x, k),
            None => return Ok(env.arg_failure("the power k must be an integer")),
        },
        _ => return Ok(env.arg_failure("coefficient(p, x, k) takes two or three arguments")),
    };
    let Some((x, scale)) = variable(env, x) else {
        return Ok(env.arg_failure("the variable must not be a number, sum or product"));
    };
    let Some(k) = k.checked_mul(scale as i64) else {
        return Ok(env.arg_failure("the power is too large"));
    };
    thread(env, p, m, &mut |p, m| {
        let (ring, ts) = env.read_terms(p, &[x], m)?;
        let dg = Degrees::new(env, &ring, x);
        Ok(Out::Done(env.conv().to_term(&part_of_degree(&ts, &dg, k), &ring, m)?))
    })
}

/// `coefficient_list(p, x)`: the coefficients of `x^0, x^1, ..., x^degree`; `coefficient_list(p, [x, y, ...])`
/// nests one list level per variable.
pub(crate) fn coefficient_list<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [p, xs] = args else {
        return Ok(env.arg_failure("coefficient_list(p, x) takes two arguments"));
    };
    let vars: Vec<T<'h>> = env.as_list(*xs).map_or_else(|| vec![*xs], |v| v.to_vec());
    if vars.is_empty() || vars.len() > 32 || !vars.iter().all(|&v| env.is_variable(v)) {
        return Ok(env.arg_failure("the variables must be kernels (at most 32)"));
    }
    let (ring, ts) = env.read_terms(*p, &vars, m)?;
    let dgs: Vec<Degrees> = vars.iter().map(|&v| Degrees::new(env, &ring, v)).collect();
    if ts.items.iter().any(|(e, _, _)| dgs.iter().any(|d| d.of(e) < 0)) {
        return Ok(env.arg_failure("the polynomial has negative powers of a variable"));
    }
    // the dense shape: the degree in each variable over the whole polynomial (so the nested lists are rectangular)
    let shape: Vec<i64> = dgs.iter().map(|d| ts.items.iter().map(|(e, _, _)| d.of(e)).max().unwrap_or(0)).collect();
    // nested lists by an explicit stack over the variables (depth at most 32)
    fn build<'h>(
        env: &Env<'h>,
        ring: &Ring<'h>,
        p: &Terms,
        dgs: &[Degrees],
        shape: &[i64],
        m: &mut Meter,
    ) -> WorkResult<T<'h>> {
        let Some((d, rest)) = dgs.split_first() else {
            return env.conv().to_term(&p.select(|_| true, |e| e.to_vec()), ring, m);
        };
        let mut items = Vec::with_capacity(shape[0] as usize + 1);
        for k in 0..=shape[0] {
            m.step(p.items.len() as u64)?;
            let part = Terms::of_qpoly(&part_of_degree(p, d, k));
            items.push(build(env, ring, &part, rest, &shape[1..], m)?);
        }
        Ok(env.list(&items))
    }
    Ok(Out::Done(build(env, &ring, &ts, &dgs, &shape, m)?))
}

/// `degree(p, x)`: the largest power of `x` in the expansion of p (`-inf` for 0).
pub(crate) fn degree<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [p, x] = args else {
        return Ok(env.arg_failure("degree(p, x) takes two arguments"));
    };
    let Some((x, scale)) = variable(env, *x) else {
        return Ok(env.arg_failure("the variable must not be a number, sum or product"));
    };
    thread(env, *p, m, &mut |p, m| {
        let (ring, ts) = env.read_terms(p, &[x], m)?;
        let dg = Degrees::new(env, &ring, x);
        Ok(Out::Done(match ts.items.iter().map(|(e, _, _)| dg.of(e)).max() {
            None => env.ar.neg(env.sym("inf")),
            // the degree in x^n is the degree in x over n
            Some(d) => env.ar.div(env.h().int(d), env.h().int(scale as i64)),
        }))
    })
}

/// `variables(p)`: the kernels of the expansion of p that are not numbers, in canonical order.
pub(crate) fn variables<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [p] = args else {
        return Ok(env.arg_failure("variables(p) takes one argument"));
    };
    let (ring, ts) = env.read_terms(*p, &[], m)?;
    let vs: Vec<T<'h>> = (0..ring.nvars())
        .filter(|&v| ts.items.iter().any(|(e, _, _)| e[v] > 0) && !ring.is_numeric(&env.ar, v))
        .map(|v| ring.kernels[v])
        .collect();
    Ok(Out::Done(env.list(&vs)))
}

/// The gcd of rational polynomials: the gcd of the integer primitive parts times the gcd of the contents (the
/// gcd of the numerators over the lcm of the denominators).
fn qgcd(a: &QPoly, b: &QPoly, m: &mut Meter) -> WorkResult<QPoly> {
    if a.is_zero() {
        return Ok(b.clone());
    }
    if b.is_zero() {
        return Ok(a.clone());
    }
    let (ca, pa) = a.primitive();
    let (cb, pb) = b.primitive();
    let g = crate::gcd::gcd(&pa, &pb, m)?;
    let ((na, da), (nb, db)) = (rat_parts(&ca), rat_parts(&cb));
    let num = na.gcd(&nb);
    let den = da.mul(&db).div_exact(&da.gcd(&db));
    Ok(QPoly::new(g.scale(&num), den))
}

/// `polynomial_gcd(p, q, ...)` and `polynomial_lcm(p, q, ...)`.
pub(crate) fn polynomial_gcd<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, lcm: bool) -> WorkResult<Out<'h>> {
    if args.is_empty() {
        return Ok(env.arg_failure("at least one polynomial is needed"));
    }
    let (ring, ps) = env.read(args, &[], m)?;
    let mut acc = ps[0].clone();
    for p in &ps[1..] {
        acc = if !lcm {
            qgcd(&acc, p, m)?
        } else if acc.is_zero() || p.is_zero() {
            QPoly::zero(ring.nvars())
        } else {
            // lcm = a b / gcd(a, b)
            let g = qgcd(&acc, p, m)?;
            let prod = acc.mul(p, m)?;
            let (gc, gp) = g.primitive();
            let q = prod.num.div_exact(&gp, m)?.expect("the gcd divides the product");
            QPoly::new(q, prod.den).scale(&gc.recip().expect("a nonzero gcd"))
        };
    }
    if lcm && acc.num.lc().is_neg() {
        acc = acc.neg();
    }
    Ok(Out::Done(env.conv().to_term(&acc, &ring, m)?))
}

/// `gcd(a, b, ...)`: of exact numbers (integers; for rationals the gcd of the numerators over the lcm of the
/// denominators), otherwise of polynomials (decision K.5).
pub(crate) fn gcd<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let nums: Option<Vec<Rat>> = args.iter().map(|&a| env.ar.num(a).and_then(|x| x.as_rat().cloned())).collect();
    if let Some(qs) = nums {
        let (mut n, mut d) = (Z::ZERO, Z::ONE);
        for q in &qs {
            m.step(1)?;
            let (a, b) = rat_parts(q);
            n = n.gcd(&a);
            d = d.mul(&b).div_exact(&d.gcd(&b));
        }
        return Ok(Out::Done(env.conv().rat_term(&rat_of(&n, &d))));
    }
    if args.iter().any(|&a| env.ar.num(a).is_some_and(|x| x.as_rat().is_none())) {
        return Ok(env.arg_failure("gcd takes exact numbers or polynomials"));
    }
    polynomial_gcd(env, args, m, false)
}

/// `failure(division_by_zero)`.
fn division_by_zero<'h>(env: &Env<'h>) -> Out<'h> {
    let h = env.h();
    Out::Done(h.call(h.sym(mc_api::names::FAILURE), &[env.sym(mc_api::names::failure::DIVISION_BY_ZERO)]))
}

/// `polynomial_quotient(p, q, x)` and `polynomial_remainder(p, q, x)`: division as polynomials in x; the leading
/// coefficient of q in x must be a number (otherwise the call stays unevaluated).
pub(crate) fn polynomial_division<'h>(
    env: &Env<'h>,
    args: &[T<'h>],
    m: &mut Meter,
    quotient: bool,
) -> WorkResult<Out<'h>> {
    let [p, q, x] = args else {
        return Ok(env.arg_failure("polynomial_quotient(p, q, x) takes three arguments"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let (ring, ps) = env.read(&[*p, *q], &[*x], m)?;
    let v = ring.var(*x).expect("x is in the ring");
    let (a, b) = (&ps[0], &ps[1]);
    if b.is_zero() {
        return Ok(division_by_zero(env));
    }
    let db = b.num.degree(v).unwrap_or(0);
    let lead: Vec<(&[u32], &Z)> = b.num.terms().filter(|(e, _)| e[v] == db).collect();
    if lead.len() != 1 || lead[0].0.iter().enumerate().any(|(i, &e)| i != v && e != 0) {
        return Ok(Out::Decline);
    }
    let lci = rat_of(lead[0].1, &b.den).recip().expect("a nonzero leading coefficient");
    let mut r = a.clone();
    let mut qt = QPoly::zero(ring.nvars());
    while let Some(dr) = r.num.degree(v).filter(|&d| d >= db) {
        m.step((r.num.len() + b.num.len()) as u64)?;
        // the part of r of degree dr in x, divided by lc x^db
        let terms = r
            .num
            .terms()
            .filter(|(e, _)| e[v] == dr)
            .map(|(e, c)| {
                let mut f = e.to_vec();
                f[v] -= db;
                (f, c.clone())
            })
            .collect();
        let t = QPoly::new(Poly::from_terms(ring.nvars(), terms), r.den.clone()).scale(&lci);
        r = r.sub(&t.mul(b, m)?);
        qt = qt.add(&t);
    }
    let out = if quotient { qt } else { r };
    Ok(Out::Done(env.conv().to_term(&out, &ring, m)?))
}

/// `factor(p)` and `factor_square_free(p)`: the factorisation over the integers as a product of powers; a
/// rational function has its numerator and denominator factored.
pub(crate) fn factor<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, square_free: bool) -> WorkResult<Out<'h>> {
    let [e] = args else {
        return Ok(env.arg_failure("factor(p) takes one argument"));
    };
    thread(env, *e, m, &mut |e, m| {
        let c = Conv::rational(&env.ar, &env.hd);
        let (ring, _) = c.ring(&[e], &[], m)?;
        let Ok(f) = ratfun::read(&c, e, &ring, m)? else {
            return Ok(division_by_zero(env));
        };
        let num = factored_term(env, &ring, &factor::factor(&f.num, square_free, m)?, m)?;
        if f.den.as_constant() == Some(Z::ONE) {
            return Ok(Out::Done(num));
        }
        let den = factored_term(env, &ring, &factor::factor(&f.den, square_free, m)?, m)?;
        let inv = env.ar.pow(den, env.h().int(-1));
        Ok(Out::Done(env.ar.mul(&[num, inv])))
    })
}

/// `unit * prod f_i^e_i` as a canonical product.
fn factored_term<'h>(env: &Env<'h>, ring: &Ring<'h>, f: &factor::Factored, m: &mut Meter) -> WorkResult<T<'h>> {
    let h = env.h();
    let mut parts = vec![env.conv().rat_term(&Rat::from_int(f.unit.to_int()))];
    for (g, e) in &f.factors {
        let t = env.conv().int_poly_term(g, ring, m)?;
        parts.push(if *e == 1 { t } else { env.ar.pow(t, h.int(*e as i64)) });
    }
    Ok(env.ar.mul(&parts))
}

/// `resultant(p, q, x)`.
pub(crate) fn resultant<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [p, q, x] = args else {
        return Ok(env.arg_failure("resultant(p, q, x) takes three arguments"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let (ring, ps) = env.read(&[*p, *q], &[*x], m)?;
    let v = ring.var(*x).expect("x is in the ring");
    let (a, b) = (&ps[0], &ps[1]);
    let r = resultant::resultant(&a.num, &b.num, v, m)?;
    // res(a / d1, b / d2) = res(a, b) / (d1^deg b d2^deg a)
    let (na, nb) = (a.num.degree(v).unwrap_or(0) as u64, b.num.degree(v).unwrap_or(0) as u64);
    let den = a.den.pow(nb).mul(&b.den.pow(na));
    Ok(Out::Done(env.conv().to_term(&QPoly::new(r, den), &ring, m)?))
}

/// `discriminant(p, x)`.
pub(crate) fn discriminant<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [p, x] = args else {
        return Ok(env.arg_failure("discriminant(p, x) takes two arguments"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let (ring, ps) = env.read(&[*p], &[*x], m)?;
    let v = ring.var(*x).expect("x is in the ring");
    let a = &ps[0];
    let d = resultant::discriminant(&a.num, v, m)?;
    // disc(a / d) = disc(a) / d^(2n - 2)
    let n = a.num.degree(v).unwrap_or(0) as u64;
    let den = a.den.pow((2 * n).saturating_sub(2));
    Ok(Out::Done(env.conv().to_term(&QPoly::new(d, den), &ring, m)?))
}

/// `together(e)`: one fraction in lowest terms; `cancel(e)`: each term of a sum in lowest terms.
pub(crate) fn together<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, cancel: bool) -> WorkResult<Out<'h>> {
    let [e] = args else {
        return Ok(env.arg_failure("together(e) takes one argument"));
    };
    thread(env, *e, m, &mut |e, m| {
        let parts: Vec<T<'h>> = match env.h().view(e) {
            View::App { head, args } if cancel && head == env.hd.add => args.to_vec(),
            _ => vec![e],
        };
        let c = Conv::rational(&env.ar, &env.hd);
        let mut out = Vec::with_capacity(parts.len());
        for t in parts {
            let (ring, _) = c.ring(&[t], &[], m)?;
            let Ok(f) = ratfun::read(&c, t, &ring, m)? else {
                return Ok(division_by_zero(env));
            };
            out.push(ratfun::to_term(&c, &f, &ring, m)?);
        }
        Ok(Out::Done(if out.len() == 1 { out[0] } else { env.ar.add(&out) }))
    })
}
