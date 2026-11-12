//! Substitutions the interpreter tries after the rules, written in Rust because they are decisions over the whole
//! integrand rather than patterns:
//!
//! - **rational functions of the six trigonometric functions of one linear argument** `θ = e + f x`: Bioche's
//!   rules choose `t = cos θ`, `t = sin θ` or `t = tan θ` when the differential `u dθ` is invariant under
//!   `θ → -θ`, `θ → π - θ` or `θ → π + θ`; otherwise the half-angle substitution `t = tan(θ/2)` (Weierstrass).
//!   The rational function of `t` goes to the caller's rational integrator. The same for the six hyperbolic
//!   functions (`t = cosh θ`, `sinh θ`, `tanh θ`, else `tanh(θ/2)`). (Bioche 1902; Zwillinger, *Handbook of
//!   Integration*, 1992, section 2.4.)
//! - **one linear radical** `(a + b x)^(p_i/q_i)` (with `x` itself as the case `a = 0, b = 1`): `a + b x = t^q`,
//!   `q` the least common multiple of the denominators, which leaves a rational function of `t` (Euler's first
//!   substitution; Bronstein, *Symbolic Integration I*, exercise 2).
use crate::run::Run;
use crate::util::{call_of, free_of, replace, small_rat};
use mc_api::{Store, View, T};
use mc_eval::Res;

const TRIG: [&str; 6] = ["sin", "cos", "tan", "cot", "sec", "csc"];
const HYPER: [&str; 6] = ["sinh", "cosh", "tanh", "coth", "sech", "csch"];

impl<'h> Run<'_, '_, '_, '_, 'h> {
    /// A rational function of trigonometric functions of one linear argument, by Bioche's rules or the half-angle
    /// substitution.
    pub(crate) fn trig_rational(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        if let Some(r) = self.circular_rational(u, false)? {
            return Ok(Some(r));
        }
        self.circular_rational(u, true)
    }

    /// [`Run::trig_rational`] for the trigonometric functions (`hyper` false) or the hyperbolic ones (`hyper`
    /// true: `t = cosh θ`, `sinh θ`, `tanh θ`, else `tanh(θ/2)`, with the signs of `cosh^2 - sinh^2 = 1`).
    fn circular_rational(&mut self, u: T<'h>, hyper: bool) -> Res<Option<T<'h>>> {
        let names: [&str; 6] = if hyper { HYPER } else { TRIG };
        let h = self.heap();
        let x = self.x();
        // the one argument
        let mut theta: Option<T<'h>> = None;
        let mut ok = true;
        walk(h, u, &mut |t| {
            if let Some((s, args)) = call_of(h, t) {
                if names.contains(&h.sym_name(s).as_str()) && args.len() == 1 && !free_of(h, args[0], x) {
                    match theta {
                        None => theta = Some(args[0]),
                        Some(th) if th == args[0] => {}
                        Some(_) => ok = false,
                    }
                    return false;
                }
            }
            true
        });
        let Some(theta) = theta else { return Ok(None) };
        if !ok {
            return Ok(None);
        }
        let ar = &mc_arith::Arith::new(self.heap());
        let e = replace(ar, theta, x, h.int(0));
        let f = ar.sub(replace(ar, theta, x, h.int(1)), e);
        if !free_of(h, f, x) || ar.add(&[e, ar.mul(&[f, x])]) != theta {
            return Ok(None);
        }
        // the integrand in S = sin θ and C = cos θ
        let s = h.sym_term(h.sym("rules$s"));
        let c = h.sym_term(h.sym("rules$c"));
        let one = h.int(1);
        let m1 = h.int(-1);
        let v = map_calls(ar, u, &mut |name, args| {
            if args.len() != 1 || args[0] != theta {
                return None;
            }
            let k = names.iter().position(|n| *n == name)?;
            Some(match k {
                0 => s,
                1 => c,
                2 => ar.mul(&[s, ar.pow(c, m1)]),
                3 => ar.mul(&[c, ar.pow(s, m1)]),
                4 => ar.pow(c, m1),
                _ => ar.pow(s, m1),
            })
        });
        if !free_of(h, v, x) || !rational_in(h, v, &[s, c]) {
            return Ok(None);
        }
        let neg = |t: T<'h>| ar.mul(&[m1, t]);
        let v_neg_s = self.eval(replace(ar, v, s, neg(s)))?;
        let v_neg_c = self.eval(replace(ar, v, c, neg(c)))?;
        let v_neg_sc = self.eval(replace(ar, replace(ar, v, s, neg(s)), c, neg(c)))?;
        let v = self.eval(v)?;
        let half = ar.rational(one, h.int(2));
        let t = x;
        let tt = ar.pow(t, h.int(2));
        let mut tries: Vec<(T<'h>, T<'h>, T<'h>, T<'h>)> = vec![]; // (S, C, dθ/dt, t as a function of θ)
        let sqrt = |w: T<'h>| ar.pow(w, half);
        let one_m = ar.sub(one, tt);
        let one_p = ar.add(&[one, tt]);
        let call = |k: usize, a: T<'h>| h.call(h.sym(names[k]), &[a]);
        if self.zero(ar.add(&[v_neg_s, v]))? {
            // u dθ invariant under θ → -θ: t = cos θ, dθ = -dt / sin θ (t = cosh θ, dθ = dt / sinh θ)
            let sv = if hyper { sqrt(ar.sub(tt, one)) } else { sqrt(one_m) };
            let d = if hyper { ar.pow(sv, m1) } else { ar.mul(&[m1, ar.pow(sv, m1)]) };
            tries.push((sv, t, d, call(1, theta)));
        }
        if self.zero(ar.add(&[v_neg_c, v]))? {
            // θ → π - θ: t = sin θ, dθ = dt / cos θ (t = sinh θ, cosh θ = sqrt(1 + t^2))
            let cv = if hyper { sqrt(one_p) } else { sqrt(one_m) };
            tries.push((t, cv, ar.pow(cv, m1), call(0, theta)));
        }
        if self.zero(ar.sub(v_neg_sc, v))? {
            // θ → π + θ: t = tan θ (t = tanh θ)
            let w = if hyper { one_m } else { one_p };
            let r = ar.pow(sqrt(w), m1);
            tries.push((ar.mul(&[t, r]), r, ar.pow(w, m1), call(2, theta)));
        }
        // the half-angle substitution, always applicable
        let (num, den) = if hyper { (one_p, one_m) } else { (one_m, one_p) };
        let ip = ar.pow(den, m1);
        tries.push((
            ar.mul(&[h.int(2), t, ip]),
            ar.mul(&[num, ip]),
            ar.mul(&[h.int(2), ip]),
            call(2, ar.mul(&[half, theta])),
        ));
        let finv = ar.pow(f, m1);
        for (sv, cv, dth, back) in tries {
            let w = replace(ar, replace(ar, v, s, sv), c, cv);
            let w = ar.mul(&[finv, dth, w]);
            let w = self.eval(h.call(h.sym("together"), &[w]))?;
            if let Some(r) = self.int(w)? {
                let r = replace(ar, r, x, back);
                return Ok(Some(self.eval(r)?));
            }
        }
        Ok(None)
    }

    /// Powers of one linear form `a + b x` with rational exponents (and otherwise a rational function of `x`):
    /// `a + b x = t^q`.
    pub(crate) fn linear_radical(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        let h = self.heap();
        let x = self.x();
        let pow = h.sym(mc_api::names::POW);
        let mut base: Option<T<'h>> = None;
        let mut q: i128 = 1;
        let mut ok = true;
        walk(h, u, &mut |t| {
            if let Some((s, [b, e])) = call_of(h, t) {
                if s == pow && !free_of(h, *b, x) {
                    if let Some((_, d)) = small_rat(h, *e) {
                        if d != 1 {
                            match base {
                                None => base = Some(*b),
                                Some(b0) if b0 == *b => {}
                                Some(_) => ok = false,
                            }
                            q = lcm(q, d);
                            return false;
                        }
                    } else {
                        ok = false;
                    }
                }
            }
            true
        });
        let Some(base) = base else { return Ok(None) };
        if !ok || q > 12 {
            return Ok(None);
        }
        let ar = &mc_arith::Arith::new(self.heap());
        let a = replace(ar, base, x, h.int(0));
        let b = ar.sub(replace(ar, base, x, h.int(1)), a);
        if !free_of(h, b, x) || ar.add(&[a, ar.mul(&[b, x])]) != base || crate::cond::is_zero(ar, b) {
            return Ok(None);
        }
        let tsym = h.sym_term(h.sym("rules$t"));
        let qt = h.int(q as i64);
        // base^r -> T^(r q)
        let v = map_pows(ar, u, base, &mut |r| ar.pow(tsym, ar.mul(&[r, qt])));
        // x -> (T^q - a)/b
        let xv = ar.mul(&[ar.sub(ar.pow(tsym, qt), a), ar.pow(b, h.int(-1))]);
        let v = replace(ar, v, x, xv);
        if !free_of(h, v, x) {
            return Ok(None);
        }
        let dx = ar.mul(&[qt, ar.pow(tsym, h.int(q as i64 - 1)), ar.pow(b, h.int(-1))]);
        let w = replace(ar, ar.mul(&[v, dx]), tsym, x);
        let w = self.eval(h.call(h.sym("together"), &[w]))?;
        let Some(r) = self.int(w)? else {
            return Ok(None);
        };
        let back = ar.pow(base, ar.rational(h.int(1), qt));
        let r = replace(ar, r, x, back);
        Ok(Some(self.eval(r)?))
    }

    /// `tan`, `cot`, `sec`, `csc` (and their hyperbolic forms) in terms of sines and cosines; `None` when there are
    /// none.
    pub(crate) fn sines_and_cosines(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        let h = self.heap();
        let ar = &mc_arith::Arith::new(h);
        let m1 = h.int(-1);
        let x = self.x();
        let mut changed = false;
        let v = map_calls(ar, u, &mut |name, args| {
            let [a] = args else { return None };
            if free_of(h, *a, x) {
                return None;
            }
            let (num, den) = match name {
                "tan" => ("sin", "cos"),
                "cot" => ("cos", "sin"),
                "sec" => ("", "cos"),
                "csc" => ("", "sin"),
                "tanh" => ("sinh", "cosh"),
                "coth" => ("cosh", "sinh"),
                "sech" => ("", "cosh"),
                "csch" => ("", "sinh"),
                _ => return None,
            };
            changed = true;
            let d = ar.pow(h.call(h.sym(den), &[*a]), m1);
            Some(if num.is_empty() { d } else { ar.mul(&[h.call(h.sym(num), &[*a]), d]) })
        });
        if !changed {
            return Ok(None);
        }
        Ok(Some(self.eval(v)?))
    }

    /// The product logarithm of one linear argument `a + b x`: `w = product_log(a + b x)`, so
    /// `x = (w e^w - a)/b` and `dx = (1 + w) e^w dw / b`; the exponential integrand that remains goes to the
    /// engine (Corless, Gonnet, Hare, Jeffrey and Knuth 1996, section 3).
    pub(crate) fn product_log(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        let h = self.heap();
        let x = self.x();
        let mut arg: Option<T<'h>> = None;
        let mut ok = true;
        walk(h, u, &mut |t| {
            if let Some((s, args)) = call_of(h, t) {
                if h.sym_name(s) == "product_log" && args.len() == 1 && !free_of(h, args[0], x) {
                    match arg {
                        None => arg = Some(args[0]),
                        Some(a) if a == args[0] => {}
                        Some(_) => ok = false,
                    }
                    return false;
                }
            }
            true
        });
        let Some(w) = arg else { return Ok(None) };
        if !ok {
            return Ok(None);
        }
        let ar = &mc_arith::Arith::new(h);
        let a = replace(ar, w, x, h.int(0));
        let b = ar.sub(replace(ar, w, x, h.int(1)), a);
        if !free_of(h, b, x) || ar.add(&[a, ar.mul(&[b, x])]) != w || crate::cond::is_zero(ar, b) {
            return Ok(None);
        }
        let t = h.sym_term(h.sym("rules$w"));
        let et = ar.pow(h.sym_term(h.sym("e")), t);
        let plog = h.sym("product_log");
        let v = map_calls(ar, u, &mut |name, args| (name == "product_log" && args == [w]).then_some(t));
        let _ = plog;
        let xv = ar.mul(&[ar.sub(ar.mul(&[t, et]), a), ar.pow(b, h.int(-1))]);
        let v = replace(ar, v, x, xv);
        if !free_of(h, v, x) {
            return Ok(None);
        }
        let dx = ar.mul(&[ar.add(&[h.int(1), t]), et, ar.pow(b, h.int(-1))]);
        let g = replace(ar, ar.mul(&[v, dx]), t, x);
        let g = self.eval(h.call(h.sym("expand"), &[g]))?;
        let Some(r) = self.int(g)? else { return Ok(None) };
        let back = h.call(h.sym("product_log"), &[w]);
        let r = replace(ar, r, x, back);
        Ok(Some(self.eval(r)?))
    }

    /// A product with a sum among its factors (or a positive integer power of a sum), or a polynomial times
    /// positive powers of sines and cosines: expanded (the sines and cosines into multiple angles first), then
    /// integrated term by term.
    pub(crate) fn expanded(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        let h = self.heap();
        let x = self.x();
        let Some((s, args)) = call_of(h, u) else { return Ok(None) };
        if h.sym_name(s) != mc_api::names::MUL {
            return Ok(None);
        }
        let is_sum = |t: T<'h>| call_of(h, t).is_some_and(|(s, _)| h.sym_name(s) == mc_api::names::ADD);
        let mut trig = false;
        let mut sum = false;
        for &a in args {
            let (b, e) = match call_of(h, a) {
                Some((s, [b, e])) if h.sym_name(s) == mc_api::names::POW => (*b, Some(*e)),
                _ => (a, None),
            };
            let pos_int = e.is_none_or(|e| matches!(h.view(e), View::Int(k) if k > 0));
            if is_sum(b) && pos_int && !free_of(h, b, x) {
                sum = true;
            }
            if pos_int && call_of(h, b).is_some_and(|(s, _)| matches!(h.sym_name(s).as_str(), "sin" | "cos")) {
                trig = true;
            }
        }
        if !sum && !trig {
            return Ok(None);
        }
        let mut v = u;
        if trig {
            v = self.eval(h.call(h.sym("trig_reduce"), &[v]))?;
        }
        let v = self.eval(h.call(h.sym("expand"), &[v]))?;
        if v == u || !is_sum(v) {
            return Ok(None);
        }
        self.int(v)
    }

    fn zero(&mut self, t: T<'h>) -> Res<bool> {
        let ar = &mc_arith::Arith::new(self.heap());
        if crate::cond::is_zero(ar, t) {
            return Ok(true);
        }
        let h = self.heap();
        let r = self.eval(h.call(h.sym("together"), &[t]))?;
        Ok(crate::cond::is_zero(&mc_arith::Arith::new(h), r))
    }
}

fn lcm(a: i128, b: i128) -> i128 {
    fn gcd(a: i128, b: i128) -> i128 {
        if b == 0 {
            a.abs()
        } else {
            gcd(b, a % b)
        }
    }
    a / gcd(a, b) * b
}

/// Visit the subterms of `t` (pre-order); `f` returning false skips the subterm's arguments.
fn walk<'h>(h: &'h mc_core::Heap, t: T<'h>, f: &mut dyn FnMut(T<'h>) -> bool) {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !f(u) {
            continue;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
}

/// `t` with calls `name(args)` replaced by `f(name, args)` where it gives a term.
pub(crate) fn map_calls<'h>(
    ar: &mc_arith::Arith<'h, mc_core::Heap>,
    t: T<'h>,
    f: &mut dyn FnMut(&str, &[T<'h>]) -> Option<T<'h>>,
) -> T<'h> {
    let h = ar.store();
    let View::App { head, args } = h.view(t) else {
        return t;
    };
    if let Some((s, _)) = call_of(h, t) {
        if let Some(r) = f(&h.sym_name(s), args) {
            return r;
        }
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| map_calls(ar, a, f)).collect();
    if new.iter().zip(args).all(|(a, b)| a == b) {
        return t;
    }
    crate::util::rebuild(ar, head, &new)
}

/// `t` with powers `base^r` (and `base` itself, r = 1) replaced by `f(r)`.
fn map_pows<'h>(
    ar: &mc_arith::Arith<'h, mc_core::Heap>,
    t: T<'h>,
    base: T<'h>,
    f: &mut dyn FnMut(T<'h>) -> T<'h>,
) -> T<'h> {
    let h = ar.store();
    if t == base {
        return f(h.int(1));
    }
    let View::App { head, args } = h.view(t) else {
        return t;
    };
    if let Some((s, [b, e])) = call_of(h, t) {
        if h.sym_name(s) == mc_api::names::POW && *b == base {
            return f(*e);
        }
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| map_pows(ar, a, base, f)).collect();
    if new.iter().zip(args).all(|(a, b)| a == b) {
        return t;
    }
    crate::util::rebuild(ar, head, &new)
}

/// Is `t` a rational function of the symbols `vars` (sums, products, integer powers; anything free of them)?
fn rational_in<'h>(h: &'h mc_core::Heap, t: T<'h>, vars: &[T<'h>]) -> bool {
    if vars.contains(&t) {
        return true;
    }
    if vars.iter().all(|&v| free_of(h, t, v)) {
        return true;
    }
    let Some((s, args)) = call_of(h, t) else {
        return false;
    };
    let name = h.sym_name(s);
    match name.as_str() {
        mc_api::names::ADD | mc_api::names::MUL => args.iter().all(|&a| rational_in(h, a, vars)),
        mc_api::names::POW => matches!(h.view(args[1]), View::Int(_)) && rational_in(h, args[0], vars),
        _ => false,
    }
}

/// A fractional (or symbolic) power of a product, `(c g1^k1 g2^k2 ...)^n` with `c` free of `x`, among the factors
/// of `u`: `u = F q` with `q` the product of the `gi^(ki n)` and the other factors, and `F = (c g1^k1 ...)^n /
/// (g1^(k1 n) ...)`, which is constant on every interval where it is defined (its logarithmic derivative is 0), so
/// `∫ u = F ∫ q`. The same normalisation as the rule set's `FracPart` rules; `None` when no factor has this form.
pub(crate) fn unnest<'h>(ar: &mc_arith::Arith<'h, mc_core::Heap>, u: T<'h>, x: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let h = ar.store();
    let factors: Vec<T<'h>> = match call_of(h, u) {
        Some((s, args)) if h.sym_name(s) == mc_api::names::MUL => args.to_vec(),
        _ => vec![u],
    };
    let mut fs = vec![];
    let mut qs = vec![];
    let mut changed = false;
    for g in factors {
        if let Some((s, [b, n])) = call_of(h, g) {
            let integer = matches!(h.view(*n), View::Int(_) | View::BigInt { .. });
            if h.sym_name(s) == mc_api::names::POW && !integer && !free_of(h, *b, x) {
                if let Some(inner) = powers_of_product(ar, *b, x) {
                    // inner: (constant part, [(g, k)]) with at least two factors or a power inside
                    let q = ar.mul(&inner.iter().map(|&(gi, ki)| ar.pow(gi, ar.mul(&[ki, *n]))).collect::<Vec<_>>());
                    fs.push(ar.mul(&[g, ar.pow(q, h.int(-1))]));
                    qs.push(q);
                    changed = true;
                    continue;
                }
            }
        }
        qs.push(g);
    }
    if !changed {
        return None;
    }
    Some((ar.mul(&fs), ar.mul(&qs)))
}

/// `b = c g1^k1 g2^k2 ...` with `c` free of `x` and the `gi` not products or powers: the `(gi, ki)` when the form is
/// not already a single `g` (that is, a constant factor, several factors or a power appear).
fn powers_of_product<'h>(ar: &mc_arith::Arith<'h, mc_core::Heap>, b: T<'h>, x: T<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    let h = ar.store();
    let one = h.int(1);
    let parts: Vec<T<'h>> = match call_of(h, b) {
        Some((s, args)) if h.sym_name(s) == mc_api::names::MUL => args.to_vec(),
        _ => vec![b],
    };
    let mut out = vec![];
    let mut nontrivial = parts.len() > 1;
    for p in parts {
        if free_of(h, p, x) {
            continue;
        }
        match call_of(h, p) {
            Some((s, [g, k])) if h.sym_name(s) == mc_api::names::POW => {
                if matches!(call_of(h, *g), Some((t, _)) if h.sym_name(t) == mc_api::names::MUL) {
                    return None;
                }
                out.push((*g, *k));
                nontrivial = true;
            }
            _ => out.push((p, one)),
        }
    }
    (nontrivial && !out.is_empty()).then_some(out)
}
