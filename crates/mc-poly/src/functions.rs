//! The user-level functions: argument shapes, threading over lists, and the reading and writing of terms around
//! the kernels.
use crate::convert::{Conv, Ring, Terms};
use crate::natives::{Env, Out};
use crate::poly::Poly;
use crate::qpoly::{rat_of, rat_parts, QPoly};
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use crate::{factor, ratfun, resultant};
use mc_api::{names, Store, View, T};
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
    match args {
        [e, o] if env.args_of(*o, "options").is_some() => return expand_mod(env, *e, *o, m),
        // expand(e, patt): the library's rule (only the parts that are not free of patt are expanded)
        [_, _] => return Ok(Out::Decline),
        _ => {}
    }
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

/// `expand(e, modulus: n)` (n > 1, prime or not): the expansion over `Z/n`, that is over the integers with every
/// coefficient then reduced to `0..n` (reduction mod n is a ring map). A rational coefficient needs a denominator
/// prime to n (it is multiplied by the inverse); otherwise, or with numeric kernels (`sqrt(2)`), the call stays.
/// `modulus: 0` is the plain expansion; another option is an argument failure. Lists thread.
fn expand_mod<'h>(env: &Env<'h>, e: T<'h>, o: T<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let h = env.h();
    let rules = env.args_of(o, "options").unwrap_or(&[]);
    let n = match rules {
        [r] => match h.view(*r) {
            View::App { args: &[k, v], .. } if k == env.sym("modulus") => env.int(v),
            _ => None,
        },
        _ => None,
    };
    // trig: true is the library's rule (the trigonometric functions expanded first)
    if let [r] = rules {
        if let View::App { args: &[k, _], .. } = h.view(*r) {
            if k == env.sym("trig") {
                return Ok(Out::Decline);
            }
        }
    }
    let n = match n {
        Some(0) => return expand(env, &[e], m),
        Some(n) if n > 1 => n as u64,
        _ => return Ok(env.arg_failure("expand(e, modulus: n) takes the option modulus (an integer > 1, or 0)")),
    };
    thread(env, e, m, &mut |x, m| {
        let c = env.conv();
        let (ring, shared) = c.ring(&[x], &[], m)?;
        if ring.kernels.iter().any(|&k| env.ar.num(k).is_some()) {
            return Ok(Out::Decline);
        }
        let p = c.to_poly(x, &ring, &shared, m)?;
        let Some(dinv) = inverse_mod(p.den.rem_u64(n), n) else { return Ok(Out::Decline) };
        let terms = p
            .num
            .terms()
            .map(|(ex, k)| (ex.to_vec(), Z::S(((k.rem_u64(n) as u128 * dinv as u128) % n as u128) as i64)))
            .collect();
        let r = QPoly::from_poly(Poly::from_terms(ring.nvars(), terms));
        Ok(Out::Done(c.to_term(&r, &ring, m)?))
    })
}

/// The inverse of `a` modulo `n` (`n > 1`), if `gcd(a, n) = 1`.
fn inverse_mod(a: u64, n: u64) -> Option<u64> {
    let (mut r0, mut r1) = (n as i128, (a % n) as i128);
    let (mut s0, mut s1) = (0i128, 1i128);
    while r1 != 0 {
        let q = r0 / r1;
        (r0, r1) = (r1, r0 - q * r1);
        (s0, s1) = (s1, s0 - q * s1);
    }
    (r0 == 1).then(|| s0.rem_euclid(n as i128) as u64)
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

/// A trailing `options(modulus -> q)` with q a nonzero integer: `(p, q, the arguments between p and the options)`.
fn modulus_option<'h, 'a>(env: &Env<'h>, args: &'a [T<'h>]) -> Option<(T<'h>, T<'h>, &'a [T<'h>])> {
    let (&last, front) = args.split_last()?;
    let rs = env.args_of(last, "options")?;
    let (&p, rest) = front.split_first()?;
    let h = env.h();
    let [r] = rs else { return None };
    let View::App { args: &[k, v], .. } = h.view(*r) else { return None };
    (k == env.sym("modulus") && matches!(h.view(v), View::Int(q) if q != 0)).then_some((p, v, rest))
}

/// `coefficient(p, x, k)` (k = 1 when omitted): the coefficient of `x^k` in the expansion of p; a list of variables
/// gives the list of coefficients, a monomial `x^a y^b` the coefficient of the term in each variable in turn, and
/// `modulus: q` the coefficients reduced modulo q.
pub(crate) fn coefficient<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let h = env.h();
    // `modulus: q`: the coefficient of p reduced modulo q (polynomial_mod first: the same coefficients reduced)
    if let Some((p, q, rest)) = modulus_option(env, args) {
        let mut a = vec![h.call(h.sym("polynomial_mod"), &[p, q])];
        a.extend_from_slice(rest);
        return Ok(Out::Again(h.call(h.sym("coefficient"), &a)));
    }
    if let [p, x, more @ ..] = args {
        // a list of variables: the coefficient of each
        if let Some(xs) = env.as_list(*x) {
            let items: Vec<T<'h>> = xs
                .iter()
                .map(|&xi| {
                    let mut a = vec![*p, xi];
                    a.extend_from_slice(more);
                    h.call(h.sym("coefficient"), &a)
                })
                .collect();
            return Ok(Out::Again(env.list(&items)));
        }
        // a monomial x1^k1 ... xn^kn (power 1): the coefficients in each variable in turn
        if more.is_empty() {
            if let View::App { head, args: fs } = h.view(*x) {
                if head == env.hd.mul && fs.iter().all(|&f| env.ar.num(f).is_none() && variable(env, f).is_some()) {
                    let inner = fs.iter().fold(*p, |acc, &f| h.call(h.sym("coefficient"), &[acc, f]));
                    return Ok(Out::Again(inner));
                }
            }
        }
    }
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
        let p = split_symbolic_powers(env, p, x);
        let (ring, ts) = env.read_terms(p, &[x], m)?;
        let dg = Degrees::new(env, &ring, x);
        Ok(Out::Done(env.conv().to_term(&part_of_degree(&ts, &dg, k), &ring, m)?))
    })
}

/// For a form `x = b^e` with a symbolic exponent: `p` with each power `b^(e k + c)` (k a positive integer, c a
/// nonzero number) written as the unevaluated product `(b^e)^k b^c`, so that `x^(1 + s)` counts as `x^s` times `x`
/// (`coefficient(x^(1 + s), x^s)` is `x`). Other terms are unchanged.
fn split_symbolic_powers<'h>(env: &Env<'h>, p: T<'h>, x: T<'h>) -> T<'h> {
    let h = env.h();
    let View::App { head, args: &[base, e] } = h.view(x) else { return p };
    if head != env.hd.pow || env.ar.num(e).is_some() {
        return p;
    }
    /// The symbols of `t`.
    fn syms<'h>(h: &'h mc_core::Heap, t: T<'h>, out: &mut Vec<T<'h>>) {
        match h.view(t) {
            View::Sym(_) => out.push(t),
            View::App { head, args } => {
                syms(h, head, out);
                for &a in args {
                    syms(h, a, out);
                }
            }
            _ => {}
        }
    }
    let mut es = Vec::new();
    syms(env.h(), e, &mut es);
    fn walk<'h>(env: &Env<'h>, t: T<'h>, x: T<'h>, base: T<'h>, e: T<'h>, es: &[T<'h>]) -> T<'h> {
        let h = env.h();
        let View::App { head, args } = h.view(t) else { return t };
        if head == env.hd.pow && args.len() == 2 && args[0] == base && t != x {
            let a = args[1];
            for k in 1..=8i64 {
                let c = env.ar.sub(a, env.ar.mul(&[h.int(k), e]));
                // a numeric remainder, or a symbolic one free of the exponent's symbols (x^(s + t) = x^s x^t)
                let free = || {
                    let mut cs = Vec::new();
                    syms(h, c, &mut cs);
                    cs.iter().all(|s| !es.contains(s))
                };
                if env.ar.num(c).is_some() || free() {
                    if c == h.int(0) {
                        return h.app(env.hd.pow, &[x, h.int(k)]);
                    }
                    let xk = if k == 1 { x } else { h.app(env.hd.pow, &[x, h.int(k)]) };
                    return h.app(env.hd.mul, &[xk, h.app(env.hd.pow, &[base, c])]);
                }
            }
            return t;
        }
        if head == env.hd.add || head == env.hd.mul || head == env.hd.list {
            let xs: Vec<T<'h>> = args.iter().map(|&a| walk(env, a, x, base, e, es)).collect();
            return h.app(head, &xs);
        }
        t
    }
    walk(env, p, x, base, e, &es)
}

/// `coefficient_list(p, x)`: the coefficients of `x^0, x^1, ..., x^degree`; `coefficient_list(p, [x, y, ...])`
/// nests one list level per variable.
pub(crate) fn coefficient_list<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if let Some((p, q, rest)) = modulus_option(env, args) {
        let h = env.h();
        let mut a = vec![h.call(h.sym("polynomial_mod"), &[p, q])];
        a.extend_from_slice(rest);
        return Ok(Out::Again(h.call(h.sym("coefficient_list"), &a)));
    }
    let [p, xs] = args else {
        return Ok(env.arg_failure("coefficient_list(p, x) takes two arguments"));
    };
    // a truncated series: the coefficients of its known terms (its polynomial part)
    if env.args_of(*p, "series_data").is_some() {
        let h = env.h();
        return Ok(Out::Again(h.call(h.sym("coefficient_list"), &[h.call(h.sym("normal"), &[*p]), *xs])));
    }
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
    // the nested lists hold the product of (degree + 1) entries: beyond the library's 2^26 cells, a limit
    let cells = shape.iter().try_fold(1u64, |acc, &d| acc.checked_mul(d as u64 + 1).filter(|&c| c <= 1 << 26));
    if cells.is_none() {
        return Err(crate::work::Halt::Memory);
    }
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
    // `kernels: true`: the kernels of the expansion as they are (powers and exponentials included)
    let mut modulus = None;
    let (args, raw) = match args.split_last() {
        Some((&last, rest)) if env.args_of(last, "options").is_some() => {
            let h = env.h();
            let rs = env.args_of(last, "options").unwrap_or(&[]);
            let on = rs.iter().any(|&r| matches!(h.view(r), mc_api::View::App { args: &[k, v], .. } if k == env.sym("kernels") && v == env.sym("true")));
            // `modulus: m`: the variables of p with its coefficients reduced modulo m (x + 2 y modulo 2 is x)
            for &r in rs {
                if let mc_api::View::App { args: &[k, v], .. } = h.view(r) {
                    if k == env.sym("modulus") && !matches!(h.view(v), mc_api::View::Int(0)) {
                        modulus = Some(v);
                    }
                }
            }
            (rest, on)
        }
        _ => (args, false),
    };
    let [p] = args else {
        return Ok(env.arg_failure("variables(p) takes one argument"));
    };
    if let Some(q) = modulus {
        let h = env.h();
        let reduced = h.call(h.sym("polynomial_mod"), &[*p, q]);
        return Ok(Out::Again(h.call(h.sym("variables"), &[reduced])));
    }
    if raw {
        let (ring, ts) = env.read_terms(*p, &[], m)?;
        let vs: Vec<T<'h>> = (0..ring.nvars())
            .filter(|&v| ts.items.iter().any(|(e, _, _)| e[v] > 0) && !ring.is_numeric(&env.ar, v))
            .map(|v| ring.kernels[v])
            .collect();
        return Ok(Out::Done(env.list(&vs)));
    }
    let (vs, sorted) = variable_list(env, *p, m)?;
    let mut vs = vs;
    if !sorted {
        vs.sort_by(|&a, &b| env.ar.cmp(a, b));
        vs.dedup();
    }
    Ok(Out::Done(env.list(&vs)))
}

/// The variables of `p` and whether they are already in canonical order.
fn variable_list<'h>(env: &Env<'h>, p: T<'h>, m: &mut Meter) -> WorkResult<(Vec<T<'h>>, bool)> {
    // a list (also nested), an equation or an inequality: the variables of its parts
    let h = env.h();
    let container = |t: T<'h>| -> Option<&'h [T<'h>]> {
        if let Some(xs) = env.as_list(t) {
            return Some(xs);
        }
        match h.view(t) {
            mc_api::View::App { head, args } => match h.view(head) {
                mc_api::View::Sym(s)
                    if matches!(h.sym_name(s).as_str(), "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal" | "and" | "or" | "not") =>
                {
                    Some(args)
                }
                _ => None,
            },
            _ => None,
        }
    };
    if let Some(parts) = container(p) {
        let mut out: Vec<T<'h>> = vec![];
        for &q in parts {
            for v in variable_list(env, q, m)?.0 {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
        return Ok((out, false));
    }
    let (ring, ts) = env.read_terms(p, &[], m)?;
    let mut vs: Vec<T<'h>> = vec![];
    // a kernel u^r with r a number (a root, a reciprocal) stands for the variables of u; exp(u) for none
    let mut changed = false;
    for v in 0..ring.nvars() {
        if !ts.items.iter().any(|(e, _, _)| e[v] > 0) || ring.is_numeric(&env.ar, v) {
            continue;
        }
        let k = ring.kernels[v];
        if let Some(&[base, ex]) = env.args_of(k, "pow") {
            // e^u has no variables (the base is the constant e)
            if base == env.sym("e") && env.ar.num(ex).is_none() {
                changed = true;
                continue;
            }
            if env.ar.num(ex).is_some() && env.ar.num(base).is_none() {
                changed = true;
                for u in variable_list(env, base, m)?.0 {
                    if !vs.contains(&u) {
                        vs.push(u);
                    }
                }
                continue;
            }
        }
        vs.push(k);
    }
    Ok((vs, !changed))
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
    if let Some(out) = rational_gcd(env, args, m, lcm)? {
        return Ok(out);
    }
    let (ring, ps) = env.read(args, &[], m)?;
    let acc = fold_gcd(&ps, lcm, ring.nvars(), m)?;
    Ok(Out::Done(env.conv().to_term(&acc, &ring, m)?))
}

/// The gcd (or the lcm, positive leading coefficient) of the polynomials `ps` (at least one).
fn fold_gcd(ps: &[QPoly], lcm: bool, nv: usize, m: &mut Meter) -> WorkResult<QPoly> {
    let mut acc = ps[0].clone();
    for p in &ps[1..] {
        acc = if !lcm {
            qgcd(&acc, p, m)?
        } else if acc.is_zero() || p.is_zero() {
            QPoly::zero(nv)
        } else {
            // lcm = a b / gcd(a, b)
            let g = qgcd(&acc, p, m)?;
            let prod = acc.mul(p, m)?;
            let (gc, gp) = g.primitive();
            let q = crate::work::exact(prod.num.div_exact(&gp, m)?, "the gcd divides the product")?;
            // invariant: the gcd of two polynomials one of which is nonzero is nonzero
            QPoly::new(q, prod.den).scale(&gc.recip().expect("a nonzero gcd"))
        };
    }
    if lcm && acc.num.lc().is_neg() {
        acc = acc.neg();
    }
    Ok(acc)
}

/// Rational functions with denominators: the gcd of `a_i / b_i` is `gcd(a_i) / lcm(b_i)` and the lcm is
/// `lcm(a_i) / gcd(b_i)` (numerators and denominators in lowest terms). `None` when every argument is a polynomial.
fn rational_gcd<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, lcm: bool) -> WorkResult<Option<Out<'h>>> {
    let c = Conv::rational(&env.ar, &env.hd);
    let (ring, _) = c.ring(args, &[], m)?;
    let mut fs = Vec::with_capacity(args.len());
    for &a in args {
        match ratfun::read(&c, a, &ring, m)? {
            Ok(f) => fs.push(f),
            Err(_) => return Ok(None),
        }
    }
    if fs.iter().all(|f| f.den.is_constant()) {
        return Ok(None);
    }
    let nv = ring.nvars();
    let nums: Vec<QPoly> = fs.iter().map(|f| QPoly::from_poly(f.num.clone())).collect();
    let dens: Vec<QPoly> = fs.iter().map(|f| QPoly::from_poly(f.den.clone())).collect();
    let (n, d) = (fold_gcd(&nums, lcm, nv, m)?, fold_gcd(&dens, !lcm, nv, m)?);
    let f = ratfun::RatFun::reduced(n.num, d.num, m)?;
    Ok(Some(Out::Done(ratfun::to_term(&c, &f, &ring, m)?)))
}

/// `polynomial_gcd(p, q, ..., modulus: pr)` and `polynomial_lcm(...)` over `F_pr` for polynomials in one variable:
/// the monic gcd (Euclid's algorithm in `F_pr[x]`) or the monic lcm `a b / gcd(a, b)`; several variables stay
/// unevaluated.
pub(crate) fn polynomial_gcd_mod<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, lcm: bool, pr: u64) -> WorkResult<Out<'h>> {
    use crate::modp::{self, Fp};
    if args.is_empty() {
        return Ok(env.arg_failure("at least one polynomial is needed"));
    }
    let (ring, ps) = env.read(args, &[], m)?;
    if ring.nvars() > 1 || ring.kernels.iter().any(|&k| env.ar.num(k).is_some()) {
        return Ok(Out::Decline);
    }
    let f = Fp::new(pr);
    let mut dense = Vec::with_capacity(ps.len());
    for p in &ps {
        let Some(r) = crate::ringfns::reduce_mod(p, pr) else { return Ok(Out::Decline) };
        let mut d: Vec<u64> = if ring.nvars() == 0 {
            r.terms().map(|(_, c)| c.rem_u64(pr)).collect()
        } else {
            r.check_dense()?;
            r.to_dense(0).iter().map(|c| c.rem_u64(pr)).collect()
        };
        modp::trim(&mut d);
        dense.push(d);
    }
    let mut acc = modp::monic(f, &dense[0]);
    for d in &dense[1..] {
        acc = if !lcm {
            modp::gcd(f, &acc, d, m)?
        } else if acc.is_empty() || d.is_empty() {
            vec![]
        } else {
            let g = modp::gcd(f, &acc, d, m)?;
            let (q, _) = modp::divrem(f, &acc, &g);
            modp::mul(f, &q, d)
        };
        acc = modp::monic(f, &acc);
    }
    let nv = ring.nvars();
    let coeffs: Vec<Z> = acc.iter().map(|&c| Z::S(c as i64)).collect();
    let p = if nv == 0 { Poly::constant(0, coeffs.first().cloned().unwrap_or(Z::ZERO)) } else { Poly::from_dense(nv, 0, &coeffs) };
    Ok(Out::Done(env.conv().to_term(&QPoly::from_poly(p), &ring, m)?))
}

/// `gcd(a, b, ...)` and `lcm(a, b, ...)` of polynomials (decision K.5). Calls whose arguments are all exact numbers
/// never get here: the native sends them to mc-numtheory's half-gcd first (decision 9.1).
pub(crate) fn gcd<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, lcm: bool) -> WorkResult<Out<'h>> {
    // exact Gaussian rationals: the library's rule (the Euclidean algorithm in Z[i])
    if !args.is_empty() && args.iter().all(|&a| env.ar.num(a).is_some_and(|x| x.is_exact())) && args.iter().any(|&a| env.ar.num(a).is_some_and(|x| x.as_rat().is_none())) {
        return Ok(Out::Decline);
    }
    if args.iter().any(|&a| env.ar.num(a).is_some_and(|x| x.as_rat().is_none())) {
        let name = if lcm { "lcm" } else { "gcd" };
        return Ok(env.arg_failure(&format!("{name} takes exact numbers or polynomials")));
    }
    // symbols may stand for integers: when the arguments share no polynomial factor (gcd(a, m), gcd(x, x + 1)),
    // the integer gcd and lcm are unknown and the call stays; polynomial_gcd gives the polynomial answer 1
    // nested calls of the same function flatten: lcm(lcm(l, m), n) is lcm(l, m, n)
    let name = if lcm { "lcm" } else { "gcd" };
    if args.iter().any(|&a| env.args_of(a, name).is_some()) {
        let mut flat = vec![];
        for &a in args {
            match env.args_of(a, name) {
                Some(xs) => flat.extend_from_slice(xs),
                None => flat.push(a),
            }
        }
        return Ok(Out::Again(env.h().call(env.h().sym(name), &flat)));
    }
    if args.len() >= 2 && args.iter().any(|&a| env.ar.num(a).is_none()) {
        let (ring, ps) = env.read(args, &[], m)?;
        let mut g = ps[0].clone();
        for p in &ps[1..] {
            g = qgcd(&g, p, m)?;
        }
        let constant = (0..ring.nvars()).all(|v| g.num.degree(v).unwrap_or(0) == 0);
        if constant {
            return Ok(Out::Decline);
        }
    }
    polynomial_gcd(env, args, m, lcm)
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
    let (args, modulus) = match crate::ringfns::modulus_option(env, args) {
        Ok(x) => x,
        Err(f) => return Ok(f),
    };
    if let (Some(pr), [p, q, x]) = (modulus, args) {
        return division_mod(env, (*p, *q, *x), m, quotient, pr);
    }
    let [p, q, x] = args else {
        return Ok(env.arg_failure("polynomial_quotient(p, q, x) takes three arguments"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let (ring, ps) = env.read(&[*p, *q], &[*x], m)?;
    // invariant: `read` puts the given variables in the ring
    let v = ring.var(*x).expect("x is in the ring");
    let (a, b) = (&ps[0], &ps[1]);
    if b.is_zero() {
        return Ok(division_by_zero(env));
    }
    let db = b.num.degree(v).unwrap_or(0);
    let lead: Vec<(&[u32], &Z)> = b.num.terms().filter(|(e, _)| e[v] == db).collect();
    if lead.len() != 1 || lead[0].0.iter().enumerate().any(|(i, &e)| i != v && e != 0) {
        return symbolic_division(env, (&ring, a, b), (v, *x), m, quotient);
    }
    // invariant: `b` is nonzero (division by zero returned above), so its leading coefficient is
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

/// `polynomial_quotient(p, q, x, modulus: pr)` and `polynomial_remainder(...)`: division in `F_pr[x]` (the
/// coefficients reduced modulo the prime, the divisor's leading coefficient inverted), quotient and remainder with
/// coefficients in `0..pr`. Polynomials in other variables too stay unevaluated; a divisor that vanishes modulo pr
/// is a division by zero.
fn division_mod<'h>(env: &Env<'h>, (p, q, x): (T<'h>, T<'h>, T<'h>), m: &mut Meter, quotient: bool, pr: u64) -> WorkResult<Out<'h>> {
    use crate::modp::{self, Fp};
    if !env.is_variable(x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let (ring, ps) = env.read(&[p, q], &[x], m)?;
    if ring.nvars() != 1 || ring.kernels.iter().any(|&k| env.ar.num(k).is_some()) {
        return Ok(Out::Decline);
    }
    let mut dense = Vec::with_capacity(2);
    for p in &ps {
        let Some(r) = crate::ringfns::reduce_mod(p, pr) else { return Ok(Out::Decline) };
        r.check_dense()?;
        let mut d: Vec<u64> = r.to_dense(0).iter().map(|c| c.rem_u64(pr)).collect();
        modp::trim(&mut d);
        dense.push(d);
    }
    if dense[1].is_empty() {
        return Ok(division_by_zero(env));
    }
    m.step((dense[0].len() * dense[1].len()) as u64 + 1)?;
    let (qt, r) = modp::divrem(Fp::new(pr), &dense[0], &dense[1]);
    let out = if quotient { qt } else { r };
    let coeffs: Vec<Z> = out.iter().map(|&c| Z::S(c as i64)).collect();
    let poly = if coeffs.is_empty() { Poly::zero(1) } else { Poly::from_dense(1, 0, &coeffs) };
    Ok(Out::Done(env.conv().to_term(&QPoly::from_poly(poly), &ring, m)?))
}

/// Division in x when the leading coefficient of the divisor is not a number: pseudo-division (Knuth 4.6.1,
/// algorithm R) by the leading coefficient L, then the quotient and remainder coefficients divided by `L^k` and
/// reduced to lowest terms, so the result is the division over the field of rational functions in the other
/// variables.
fn symbolic_division<'h>(
    env: &Env<'h>,
    (ring, a, b): (&Ring<'h>, &QPoly, &QPoly),
    (v, x): (usize, T<'h>),
    m: &mut Meter,
    quotient: bool,
) -> WorkResult<Out<'h>> {
    let nv = ring.nvars();
    let db = b.num.degree(v).unwrap_or(0);
    let lc = b.num.coeffs_in(v).into_iter().find(|(d, _)| *d == db).map(|(_, c)| c).expect("a leading coefficient");
    let mut r = a.num.clone();
    let mut qt = Poly::zero(nv);
    let mut k = 0u32;
    while let Some(dr) = r.degree(v).filter(|&d| d >= db) {
        m.step((r.len() + b.num.len()) as u64)?;
        let rc = r.coeffs_in(v).into_iter().find(|(d, _)| *d == dr).map(|(_, c)| c).expect("a leading coefficient");
        let mut e = vec![0u32; nv];
        e[v] = dr - db;
        let shifted = rc.mul_term(&e, &Z::ONE);
        r = lc.mul(&r, m)?.sub(&shifted.mul(&b.num, m)?);
        qt = lc.mul(&qt, m)?.add(&shifted);
        k += 1;
    }
    let den = lc.pow(k as u64, m)?;
    // the quotient of a by b scales by b.den / a.den; the remainder by 1 / a.den
    let (part, scale) = if quotient {
        (qt, rat_of(&b.den, &a.den))
    } else {
        (r, rat_of(&Z::ONE, &a.den))
    };
    let (sn, sd) = rat_parts(&scale);
    let mut terms = Vec::new();
    for (d, c) in part.coeffs_in(v) {
        let f = ratfun::RatFun::reduced(c.scale(&sn), den.scale(&sd), m)?;
        let t = ratfun::to_term(&env.conv(), &f, ring, m)?;
        terms.push(if d == 0 { t } else { env.ar.mul(&[t, env.ar.pow(x, env.h().int(d as i64))]) });
    }
    Ok(Out::Done(env.ar.add(&terms)))
}

/// `factor(p)` and `factor_square_free(p)`: the factorisation over the integers as a product of powers; a
/// rational function has its numerator and denominator factored.
pub(crate) fn factor<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, square_free: bool) -> WorkResult<Out<'h>> {
    let (args, modulus) = match crate::ringfns::modulus_option(env, args) {
        Ok(x) => x,
        Err(f) => return Ok(f),
    };
    if let Some(pr) = modulus {
        return factor_over_fp(env, args, m, square_free, pr);
    }
    let [e] = args else {
        return Ok(env.arg_failure("factor(p) takes one argument"));
    };
    thread(env, *e, m, &mut |e, m| {
        let c = Conv::rational(&env.ar, &env.hd).splitting();
        let (ring, _) = c.ring(&[e], &[], m)?;
        let Ok(f) = ratfun::read(&c, e, &ring, m)? else {
            return Ok(division_by_zero(env));
        };
        let (ring, f) = match merge_root_kernels(env, &ring, &f) {
            Some((r2, f2)) => (r2, f2),
            None => (ring, f),
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

/// `factor(p, modulus: pr)` and `factor_square_free(p, modulus: pr)` for a polynomial in one variable: the leading
/// coefficient times the monic irreducible (square-free) factors over `F_pr` with coefficients in `0..pr`.
fn factor_over_fp<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, square_free: bool, pr: u64) -> WorkResult<Out<'h>> {
    use crate::modp::{self, Fp};
    let [e] = args else {
        return Ok(env.arg_failure("factor(p, modulus: pr) takes one polynomial"));
    };
    let (ring, ps) = env.read(&[*e], &[], m)?;
    if ring.nvars() != 1 || ring.kernels.iter().any(|&k| env.ar.num(k).is_some()) {
        return Ok(Out::Decline);
    }
    let Some(r) = crate::ringfns::reduce_mod(&ps[0], pr) else { return Ok(Out::Decline) };
    r.check_dense()?;
    let mut d: Vec<u64> = r.to_dense(0).iter().map(|c| c.rem_u64(pr)).collect();
    modp::trim(&mut d);
    let f = Fp::new(pr);
    let (lc, facs) = factor::factor_mod_p(f, &d, m)?;
    let term_of = |g: &[u64], m: &mut Meter| -> WorkResult<T<'h>> {
        let coeffs: Vec<Z> = g.iter().map(|&c| Z::S(c as i64)).collect();
        let p = Poly::from_dense(1, 0, &coeffs);
        env.conv().to_term(&QPoly::from_poly(p), &ring, m)
    };
    let mut parts = vec![env.h().int(lc as i64)];
    if square_free {
        // the product of the factors of equal multiplicity
        let mut by_mult: std::collections::BTreeMap<u32, Vec<Vec<u64>>> = std::collections::BTreeMap::new();
        for (g, mult) in facs {
            by_mult.entry(mult).or_default().push(g);
        }
        for (mult, gs) in by_mult {
            let mut acc: Vec<u64> = vec![1];
            for g in gs {
                acc = modp::mul(f, &acc, &g);
            }
            let t = term_of(&acc, m)?;
            parts.push(if mult == 1 { t } else { env.ar.pow(t, env.h().int(mult as i64)) });
        }
    } else {
        for (g, mult) in facs {
            let t = term_of(&g, m)?;
            parts.push(if mult == 1 { t } else { env.ar.pow(t, env.h().int(mult as i64)) });
        }
    }
    Ok(Out::Done(env.ar.mul(&parts)))
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


/// The kernels `x` and `x^(p/q)` of one base as powers of the single root `x^(1/L)` (L the least common multiple of
/// the denominators): `x + 2 sqrt(x) + 1` is a polynomial in `u = x^(1/2)`, where it factors as `(1 + u)^2`. The
/// polynomials are rewritten in a ring whose kernel `u` replaces the group; `None` when no base has two kernels with
/// a fractional exponent among them, or when an exponent is negative or too large.
fn merge_root_kernels<'h>(env: &Env<'h>, ring: &Ring<'h>, f: &ratfun::RatFun) -> Option<(Ring<'h>, ratfun::RatFun)> {
    use rustc_hash::FxHashMap;
    let h = env.h();
    // base -> [(variable, p, q)]
    let mut groups: FxHashMap<T<'h>, Vec<(usize, i64, i64)>> = FxHashMap::default();
    for (i, &k) in ring.kernels.iter().enumerate() {
        let (b, p, q) = match h.view(k) {
            View::App { head, args: [b, e] } if head == env.hd.pow => match env.ar.num(*e).and_then(|x| x.as_rat().cloned()) {
                Some(r) => {
                    let (n, d) = rat_parts(&r);
                    (*b, n.as_i64()?, d.as_i64()?)
                }
                None => (k, 1, 1),
            },
            _ => (k, 1, 1),
        };
        if p <= 0 || env.ar.num(b).is_some() {
            continue;
        }
        groups.entry(b).or_default().push((i, p, q));
    }
    let gcd = |mut a: i64, mut b: i64| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    // (variable -> (new kernel, multiplier))
    let mut map: FxHashMap<usize, (T<'h>, u32)> = FxHashMap::default();
    for (b, g) in &groups {
        if g.len() < 2 || g.iter().all(|&(_, _, q)| q == 1) {
            continue;
        }
        let l = g.iter().try_fold(1i64, |acc, &(_, _, q)| acc.checked_mul(q / gcd(acc, q)))?;
        if l > 64 {
            return None;
        }
        let u = env.ar.pow(*b, env.conv().rat_term(&rat_of(&Z::ONE, &Z::S(l))));
        for &(i, p, q) in g {
            map.insert(i, (u, u32::try_from(p * (l / q)).ok()?));
        }
    }
    if map.is_empty() {
        return None;
    }
    let mut kernels: Vec<T<'h>> = ring.kernels.iter().enumerate().filter(|(i, _)| !map.contains_key(i)).map(|(_, &k)| k).collect();
    for &(u, _) in map.values() {
        if !kernels.contains(&u) {
            kernels.push(u);
        }
    }
    kernels.sort_by(|&a, &b| env.ar.cmp(a, b));
    let new = Ring::from_kernels(kernels, true);
    let target: Vec<(usize, u32)> = ring
        .kernels
        .iter()
        .enumerate()
        .map(|(i, &k)| match map.get(&i) {
            Some(&(u, mult)) => new.var(u).map(|v| (v, mult)),
            None => new.var(k).map(|v| (v, 1)),
        })
        .collect::<Option<_>>()?;
    let nv = new.nvars();
    let convert = |p: &Poly| -> Option<Poly> {
        let mut terms = Vec::with_capacity(p.len());
        for (e, c) in p.terms() {
            let mut ne = vec![0u32; nv];
            for (i, &x) in e.iter().enumerate() {
                let (v, mult) = target[i];
                ne[v] = ne[v].checked_add(x.checked_mul(mult)?)?;
            }
            terms.push((ne, c.clone()));
        }
        Some(Poly::from_terms(nv, terms))
    };
    Some((new, ratfun::RatFun { num: convert(&f.num)?, den: convert(&f.den)? }))
}
/// `resultant(p, q, x)`.
pub(crate) fn resultant<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (args, modulus) = match crate::ringfns::modulus_option(env, args) {
        Ok(x) => x,
        Err(f) => return Ok(f),
    };
    let [p, q, x] = args else {
        return Ok(env.arg_failure("resultant(p, q, x) takes three arguments"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    if modulus.is_none() {
        if let Some(r) = crate::ringfns::product_resultant(env, *p, *q, *x, m)? {
            return Ok(Out::Done(r));
        }
        if x_in_denominator(env, *p, *x) || x_in_denominator(env, *q, *x) {
            // rational functions of x: the resultant extended multiplicatively, res(A/B, C/D) = res(A, C) res(B, D)
            // / (res(A, D) res(B, C)), as res(f g, h) = res(f, h) res(g, h) for polynomials
            let h = env.h();
            let c = |f: &str, a: &[T<'h>]| h.call(h.sym(f), a);
            let (pt, qt) = (c("together", &[*p]), c("together", &[*q]));
            let (a1, b1) = (c("numerator", &[pt]), c("denominator", &[pt]));
            let (a2, b2) = (c("numerator", &[qt]), c("denominator", &[qt]));
            let res = |u: T<'h>, w: T<'h>| c("resultant", &[u, w, *x]);
            let num = c(names::MUL, &[res(a1, a2), res(b1, b2)]);
            let den = c(names::MUL, &[res(a1, b2), res(b1, a2)]);
            return Ok(Out::Again(c(names::MUL, &[num, c(names::POW, &[den, h.int(-1)])])));
        }
    }
    let (ring, ps) = env.read(&[*p, *q], &[*x], m)?;
    // invariant: `read` puts the given variables in the ring
    let v = ring.var(*x).expect("x is in the ring");
    let (a, b) = (&ps[0], &ps[1]);
    if let Some(pr) = modulus {
        // reduced first, so the degrees are those over F_p (decision F2.3)
        let (Some(ra), Some(rb)) = (crate::ringfns::reduce_mod(a, pr), crate::ringfns::reduce_mod(b, pr)) else {
            return Ok(Out::Decline);
        };
        let r = crate::ringfns::mod_poly(&resultant::resultant(&ra, &rb, v, m)?, pr);
        return Ok(Out::Done(env.conv().to_term(&QPoly::from_poly(r), &ring, m)?));
    }
    let r = resultant::resultant(&a.num, &b.num, v, m)?;
    // res(a / d1, b / d2) = res(a, b) / (d1^deg b d2^deg a)
    let (na, nb) = (a.num.degree(v).unwrap_or(0) as u64, b.num.degree(v).unwrap_or(0) as u64);
    let den = a.den.pow(nb).mul(&b.den.pow(na));
    let t = env.conv().to_term(&QPoly::new(r, den), &ring, m)?;
    // finite-field elements are kernels here: their products and powers are evaluated by expanding
    let h = env.h();
    let ff = h.sym_term(h.sym("finite_field_element"));
    let has_ff = |u: T<'h>| {
        let mut stack = vec![u];
        while let Some(w) = stack.pop() {
            if let mc_api::View::App { head, args } = h.view(w) {
                if head == ff {
                    return true;
                }
                stack.extend_from_slice(args);
            }
        }
        false
    };
    if has_ff(*p) || has_ff(*q) {
        return Ok(Out::Again(h.call(h.sym("expand"), &[t])));
    }
    Ok(Out::Done(t))
}

/// Does `t` have a power `u^e` with `e` a negative number and `u` containing `x` (a denominator in `x`)?
fn x_in_denominator<'h>(env: &Env<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = env.h();
    let contains = |t: T<'h>| {
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if u == x {
                return true;
            }
            if let mc_api::View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        false
    };
    let pow = h.sym_term(h.sym(names::POW));
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let mc_api::View::App { head, args } = h.view(u) {
            if head == pow && args.len() == 2 {
                let neg = match h.view(args[1]) {
                    mc_api::View::Int(n) => n < 0,
                    mc_api::View::BigInt { neg, .. } => neg,
                    mc_api::View::Rational { num, .. } => matches!(h.view(num), mc_api::View::Int(n) if n < 0),
                    _ => false,
                };
                if neg && contains(args[0]) {
                    return true;
                }
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// `discriminant(p, x)`.
pub(crate) fn discriminant<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (args, modulus) = match crate::ringfns::modulus_option(env, args) {
        Ok(x) => x,
        Err(f) => return Ok(f),
    };
    let [p, x] = args else {
        return Ok(env.arg_failure("discriminant(p, x) takes two arguments"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    if modulus.is_none() {
        if let Some(r) = crate::ringfns::product_discriminant(env, *p, *x, m)? {
            return Ok(Out::Done(r));
        }
    }
    let (ring, ps) = env.read(&[*p], &[*x], m)?;
    // invariant: `read` puts the given variables in the ring
    let v = ring.var(*x).expect("x is in the ring");
    let a = &ps[0];
    if let Some(pr) = modulus {
        let Some(ra) = crate::ringfns::reduce_mod(a, pr) else { return Ok(Out::Decline) };
        let d = crate::ringfns::mod_poly(&resultant::discriminant(&ra, v, m)?, pr);
        return Ok(Out::Done(env.conv().to_term(&QPoly::from_poly(d), &ring, m)?));
    }
    let d = resultant::discriminant(&a.num, v, m)?;
    // disc(a / d) = disc(a) / d^(2n - 2)
    let n = a.num.degree(v).unwrap_or(0) as u64;
    let den = a.den.pow((2 * n).saturating_sub(2));
    Ok(Out::Done(env.conv().to_term(&QPoly::new(d, den), &ring, m)?))
}

/// `together(e)`: one fraction in lowest terms; `cancel(e)`: each term of a sum in lowest terms.
pub(crate) fn together<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter, cancel: bool) -> WorkResult<Out<'h>> {
    // the options modulus: m and trig: true are the library's rules
    if let [_, o] = args {
        if let Some(rs) = env.args_of(*o, "options") {
            let known = rs.iter().all(|&r| match env.h().view(r) {
                View::App { args: &[k, _], .. } => k == env.sym("modulus") || k == env.sym("trig"),
                _ => false,
            });
            if known {
                return Ok(Out::Decline);
            }
        }
    }
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
