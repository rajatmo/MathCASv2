//! The natives of constrained and global optimisation: `n_minimize`, `n_maximize` (global), `find_minimum` and
//! `find_maximum` with constraints or without starting values (local), and the value and argument forms
//! `n_min_value`, `n_arg_min`, `find_min_value`, `find_arg_min` and their maximum twins.
//!
//! | form | meaning |
//! |---|---|
//! | `n_minimize(f, x)`, `n_minimize(f, [x, y])` | the global minimum `[f_min, [x -> ..., y -> ...]]` |
//! | `n_minimize([f, cons...], vars)` | subject to constraints: `<=`, `>=`, `<`, `>`, `==`, `and`, `or`, `element` |
//! | `n_minimize(f, element([x, y], disk()))` | the variables and a region at once |
//! | `vars` items `element(x, integers)`, `[x, a, b]` | integer variables; a search range |
//! | a vector variable `v` with `dot(a, v)` | its length found from the expressions; `[v -> [...]]` |
//! | `vector_greater_equal([a, b])` | a ≥ b componentwise (also `vector_less_equal`, and the `"NormCone"`) |
//!
//! The method follows the problem (decision R.20): an affine objective with affine constraints is a linear
//! program (the simplex method, with branch and bound for integers), and its answer is the exact vertex; anything
//! else runs the augmented Lagrangian with the active-set Newton polish (`optim`), from a Halton sample of the
//! search box for the global forms. Infeasible and unbounded problems give `[inf, [x -> indeterminate]]` and
//! `[-inf, ...]` with a message, as do results whose constraints could not be met.
// index loops over several parallel arrays read more plainly than zipped iterators here
#![allow(clippy::needless_range_loop)]
use crate::lp::{self, Lp, Rel, Row};
use crate::natives::{auto, bad_argument, derivative, is_failure, mentions, numeric, split_options};
use crate::optim::{self, Kind, Point, Problem, Settings};
use mc_api::names;
use mc_arith::num::StopScope;
use mc_arith::Arith;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_numfn::{NumFn, Policy, TermFn};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// What a call returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Out {
    /// `[value, rules]`
    Full,
    /// the value
    Value,
    /// the position
    Arg,
}

/// `n_minimize` and its relatives.
pub(crate) struct Optimize {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
    /// maximise
    pub maximum: bool,
    /// the global search (else local from the starting values)
    pub global: bool,
    /// the form of the result
    pub out: Out,
    /// the name in failures and messages
    pub name: &'static str,
}

/// A variable as written: a symbol, maybe a vector of `len` components, with a start or search range.
#[derive(Clone, Debug)]
struct VarSpec {
    sym: Sym,
    /// `Some(n)` for a vector variable of n components
    len: Option<usize>,
    /// the start (local) or the search range (global) per component
    start: Option<f64>,
    range: Option<(f64, f64)>,
    integer: bool,
}

/// An atomic constraint.
#[derive(Clone, Debug)]
enum Atom<'h> {
    /// e ≥ 0
    Ge(T<'h>),
    /// e = 0
    Eq(T<'h>),
}

/// The heads the constraint reader knows.
struct Heads<'h> {
    list: T<'h>,
    and: T<'h>,
    or: T<'h>,
    le: T<'h>,
    lt: T<'h>,
    ge: T<'h>,
    gt: T<'h>,
    eq: T<'h>,
    element: T<'h>,
    alternatives: T<'h>,
}

impl<'h> Heads<'h> {
    fn new(h: &'h Heap) -> Self {
        let s = |n: &str| h.sym_term(h.sym(n));
        Heads {
            list: s(names::LIST),
            and: s("and"),
            or: s("or"),
            le: s("less_equal"),
            lt: s("less"),
            ge: s("greater_equal"),
            gt: s("greater"),
            eq: s("equal"),
            element: s("element"),
            alternatives: s("alternatives"),
        }
    }
}

/// Why a constraint was refused.
type Refused = String;

/// The most branches of a disjunction of constraints.
const MAX_BRANCHES: usize = 32;

/// The constraints in disjunctive normal form: a list of branches, each a list of atoms, and the integer
/// symbols. `Err` with a description of what is not a constraint.
struct Reader<'h> {
    hd: Heads<'h>,
    ints: HashSet<Sym>,
    /// (symbol, lower bound) for the non-negative and non-positive integer domains: x ≥ 0 or x ≤ 0
    sign: Vec<(T<'h>, bool)>,
}

impl<'h> Reader<'h> {
    /// The branches of `t`.
    fn read(&mut self, cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>) -> Res<Result<Vec<Vec<Atom<'h>>>, Refused>> {
        cx.tick(1)?;
        let h = cx.heap();
        if cx.is_true(t) {
            return Ok(Ok(vec![vec![]]));
        }
        if cx.is_false(t) {
            return Ok(Ok(vec![]));
        }
        let View::App { head, args } = h.view(t) else { return Ok(Err(format!("{} is not a constraint", printed(h, t)))) };
        if head == self.hd.list || head == self.hd.and {
            let mut acc: Vec<Vec<Atom<'h>>> = vec![vec![]];
            for &a in args {
                let bs = match self.read(cx, ar, a)? {
                    Ok(b) => b,
                    Err(e) => return Ok(Err(e)),
                };
                let mut next = Vec::new();
                for x in &acc {
                    for y in &bs {
                        let mut z = x.clone();
                        z.extend(y.iter().cloned());
                        next.push(z);
                    }
                }
                if next.len() > MAX_BRANCHES {
                    return Ok(Err("too many alternatives in the constraints".into()));
                }
                acc = next;
            }
            return Ok(Ok(acc));
        }
        if head == self.hd.or {
            let mut acc = Vec::new();
            for &a in args {
                match self.read(cx, ar, a)? {
                    Ok(b) => acc.extend(b),
                    Err(e) => return Ok(Err(e)),
                }
            }
            if acc.len() > MAX_BRANCHES {
                return Ok(Err("too many alternatives in the constraints".into()));
            }
            return Ok(Ok(acc));
        }
        let rel = [(self.hd.le, 0), (self.hd.lt, 0), (self.hd.ge, 1), (self.hd.gt, 1), (self.hd.eq, 2)]
            .into_iter()
            .find(|&(hh, _)| hh == head)
            .map(|(_, k)| k);
        if let Some(k) = rel {
            if args.len() < 2 {
                return Ok(Err(format!("{} is not a constraint", printed(h, t))));
            }
            let mut atoms = Vec::new();
            for w in args.windows(2) {
                let (a, b) = (w[0], w[1]);
                // list sides compare componentwise
                match (cx.as_list(a), cx.as_list(b)) {
                    (Some(xa), Some(xb)) if xa.len() == xb.len() => {
                        for (&p, &q) in xa.iter().zip(xb) {
                            atoms.push(self.rel(cx, ar, k, p, q)?);
                        }
                    }
                    (Some(xa), None) => {
                        for &p in xa {
                            atoms.push(self.rel(cx, ar, k, p, b)?);
                        }
                    }
                    (None, Some(xb)) => {
                        for &q in xb {
                            atoms.push(self.rel(cx, ar, k, a, q)?);
                        }
                    }
                    (None, None) => atoms.push(self.rel(cx, ar, k, a, b)?),
                    _ => return Ok(Err(format!("{} compares lists of different lengths", printed(h, t)))),
                }
            }
            return Ok(Ok(vec![atoms]));
        }
        if head == self.hd.element && args.len() == 2 {
            return self.element(cx, ar, args[0], args[1]);
        }
        let hname = match h.view(head) {
            View::Sym(s) => h.sym_name(s),
            _ => String::new(),
        };
        if let Some((ge, strict)) = match hname.as_str() {
            "vector_greater_equal" => Some((true, false)),
            "vector_greater" => Some((true, true)),
            "vector_less_equal" => Some((false, false)),
            "vector_less" => Some((false, true)),
            _ => None,
        } {
            let _ = strict;
            return self.vector(cx, ar, ge, args, t);
        }
        Ok(Err(format!("{} is not a constraint", printed(h, t))))
    }

    /// a (rel) b as an atom: 0 a ≤ b, 1 a ≥ b, 2 a = b.
    fn rel(&self, cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, k: u8, a: T<'h>, b: T<'h>) -> Res<Atom<'h>> {
        Ok(match k {
            0 => Atom::Ge(cx.eval(ar.sub(b, a))?),
            1 => Atom::Ge(cx.eval(ar.sub(a, b))?),
            _ => Atom::Eq(cx.eval(ar.sub(a, b))?),
        })
    }

    /// `vector_greater_equal([a, b])` and its relatives, with an optional cone.
    fn vector(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        ge: bool,
        args: &[T<'h>],
        t: T<'h>,
    ) -> Res<Result<Vec<Vec<Atom<'h>>>, Refused>> {
        let h = cx.heap();
        let bad = || Ok(Err(format!("{} is not a constraint", printed(h, t))));
        let Some(&[a, b]) = args.first().and_then(|&p| cx.as_list(p)) else { return bad() };
        // the cone: none or "NonNegativeCone" componentwise; "NormCone": |first ones| <= last
        let cone = match args.get(1).map(|&c| h.view(c)) {
            None => None,
            Some(View::Str(s)) => Some(s.to_string()),
            Some(_) => match args.get(1).and_then(|&c| cx.as_list(c)) {
                Some(&[c, _]) => match h.view(c) {
                    View::Str(s) => Some(s.to_string()),
                    _ => return bad(),
                },
                _ => return bad(),
            },
        };
        let d = cx.eval(if ge { ar.sub(a, b) } else { ar.sub(b, a) })?;
        let items: Vec<T<'h>> = match cx.as_list(d) {
            Some(xs) => xs.to_vec(),
            None => vec![d],
        };
        match cone.as_deref() {
            None | Some("NonNegativeCone") => Ok(Ok(vec![items.into_iter().map(Atom::Ge).collect()])),
            Some("NormCone") if items.len() >= 2 => {
                // last - sqrt(sum of squares of the others) >= 0, written as last^2 - sum >= 0 and last >= 0
                let (last, rest) = items.split_last().expect("two items");
                let mut sum = h.int(0);
                for &r in rest {
                    sum = ar.add(&[sum, ar.mul(&[r, r])]);
                }
                let q = cx.eval(ar.sub(ar.mul(&[*last, *last]), sum))?;
                Ok(Ok(vec![vec![Atom::Ge(*last), Atom::Ge(q)]]))
            }
            _ => bad(),
        }
    }

    /// `element(x, domain)`.
    fn element(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        x: T<'h>,
        dom: T<'h>,
    ) -> Res<Result<Vec<Vec<Atom<'h>>>, Refused>> {
        let h = cx.heap();
        // several points: every one in the region
        if let View::App { head, args } = h.view(x) {
            if head == self.hd.alternatives {
                let mut all = Vec::new();
                for &p in args {
                    match self.element(cx, ar, p, dom)? {
                        Ok(b) if b.len() == 1 => all.extend(b.into_iter().next().expect("one")),
                        Ok(_) => return Ok(Err("a region with alternatives".into())),
                        Err(e) => return Ok(Err(e)),
                    }
                }
                return Ok(Ok(vec![all]));
            }
        }
        let (dname, dargs) = match h.view(dom) {
            View::Sym(s) => (h.sym_name(s), &[][..]),
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => (h.sym_name(s), args),
                _ => return Ok(Err(format!("{} is not a domain or region", printed(h, dom)))),
            },
            _ => return Ok(Err(format!("{} is not a domain or region", printed(h, dom)))),
        };
        let comps: Vec<T<'h>> = match cx.as_list(x) {
            Some(xs) => xs.to_vec(),
            None => vec![x],
        };
        // domains of numbers
        let ints = |r: &mut Self, sign: Option<bool>| {
            for &c in &comps {
                if let View::Sym(s) = h.view(c) {
                    r.ints.insert(s);
                }
                if let Some(g) = sign {
                    r.sign.push((c, g));
                }
            }
        };
        match dname.as_str() {
            "integers" => {
                ints(self, None);
                return Ok(Ok(vec![vec![]]));
            }
            "nonnegative_integers" | "non_negative_integers" | "positive_integers" => {
                ints(self, Some(true));
                return Ok(Ok(vec![vec![]]));
            }
            "nonpositive_integers" | "non_positive_integers" | "negative_integers" => {
                ints(self, Some(false));
                return Ok(Ok(vec![vec![]]));
            }
            "reals" => return Ok(Ok(vec![vec![]])),
            "nonnegative_reals" | "non_negative_reals" | "positive_reals" => {
                return Ok(Ok(vec![comps.iter().map(|&c| Atom::Ge(c)).collect()]));
            }
            "nonpositive_reals" | "non_positive_reals" | "negative_reals" => {
                let mut v = Vec::new();
                for &c in &comps {
                    v.push(Atom::Ge(cx.eval(ar.neg(c))?));
                }
                return Ok(Ok(vec![v]));
            }
            "vectors" | "matrices" => {
                // vectors(n, dom): the domain of each component
                return match dargs.get(1) {
                    Some(&d) => self.element(cx, ar, x, d),
                    None => Ok(Ok(vec![vec![]])),
                };
            }
            _ => {}
        }
        let n = comps.len();
        let num = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<Vec<f64>>> {
            let v = cx.eval(h.call(h.sym("n"), &[t]))?;
            Ok(match cx.as_list(v) {
                Some(xs) => xs.iter().map(|&e| ar.num(e).and_then(|n| n.as_real().map(|r| r.to_f64()))).collect(),
                None => ar.num(v).and_then(|n| n.as_real().map(|r| r.to_f64())).map(|r| vec![r]),
            })
        };
        let point_list = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<Vec<Vec<f64>>>> {
            let Some(rows) = cx.as_list(t) else { return Ok(None) };
            let mut out = Vec::new();
            for &r in rows {
                match num(cx, r)? {
                    Some(p) if p.len() == n => out.push(p),
                    _ => return Ok(None),
                }
            }
            Ok(Some(out))
        };
        let bad = || Ok(Err(format!("{} is not a region this function knows", printed(h, dom))));
        match dname.as_str() {
            "disk" | "ball" | "circle" | "sphere" => {
                // (x - c)·(x - c) compared with r^2; c and r numbers or expressions (variables of the problem)
                let c: Vec<T<'h>> = match dargs.first() {
                    Some(&c) => match cx.as_list(c) {
                        Some(cs) if cs.len() == n => cs.to_vec(),
                        _ => return bad(),
                    },
                    None => vec![h.int(0); n],
                };
                let r = dargs.get(1).copied().unwrap_or(h.int(1));
                let e = match cx.as_list(r) {
                    // semi-axes: 1 - sum ((x_k - c_k) / r_k)^2
                    Some(rs) if rs.len() == n => {
                        let mut terms = vec![h.int(1)];
                        for k in 0..n {
                            let d = ar.div(ar.sub(comps[k], c[k]), rs[k]);
                            terms.push(ar.neg(ar.mul(&[d, d])));
                        }
                        ar.add(&terms)
                    }
                    Some(_) => return bad(),
                    None => {
                        let mut terms = vec![ar.mul(&[r, r])];
                        for k in 0..n {
                            let d = ar.sub(comps[k], c[k]);
                            terms.push(ar.neg(ar.mul(&[d, d])));
                        }
                        ar.add(&terms)
                    }
                };
                let e = cx.eval(e)?;
                let mut atoms = vec![if matches!(dname.as_str(), "circle" | "sphere") { Atom::Eq(e) } else { Atom::Ge(e) }];
                if cx.as_list(r).is_none() && ar.num(r).is_none() {
                    atoms.push(Atom::Ge(r));
                }
                Ok(Ok(vec![atoms]))
            }
            "rectangle" | "cuboid" => {
                let lo = match dargs.first() {
                    Some(&p) => num(cx, p)?,
                    None => Some(vec![0.0; n]),
                };
                let hi = match dargs.get(1) {
                    Some(&p) => num(cx, p)?,
                    None => lo.as_ref().map(|l| l.iter().map(|v| v + 1.0).collect()),
                };
                let (Some(lo), Some(hi)) = (lo, hi) else { return bad() };
                if lo.len() != n || hi.len() != n {
                    return bad();
                }
                let mut v = Vec::new();
                for k in 0..n {
                    v.push(Atom::Ge(cx.eval(ar.sub(comps[k], h.real(lo[k])))?));
                    v.push(Atom::Ge(cx.eval(ar.sub(h.real(hi[k]), comps[k]))?));
                }
                Ok(Ok(vec![v]))
            }
            "cylinder" if n >= 2 => {
                // cylinder([p, q], r) (default the unit cylinder along the z axis from -1 to 1): with a = q - p and
                // d = x - p, 0 <= d.a <= a.a and the distance to the axis at most r, by Lagrange's identity
                // |d|^2 |a|^2 - (d.a)^2 <= r^2 |a|^2 (round 37)
                let (p, q) = match dargs.first() {
                    Some(&e) => match point_list(cx, e)? {
                        Some(pq) if pq.len() == 2 && pq[0].len() == n && pq[1].len() == n => (pq[0].clone(), pq[1].clone()),
                        _ => return bad(),
                    },
                    None if n == 3 => (vec![0.0, 0.0, -1.0], vec![0.0, 0.0, 1.0]),
                    None => return bad(),
                };
                let r = match dargs.get(1) {
                    Some(&r) => match num(cx, r)? {
                        Some(v) if v.len() == 1 && v[0] > 0.0 => v[0],
                        _ => return bad(),
                    },
                    None => 1.0,
                };
                let a: Vec<f64> = (0..n).map(|k| q[k] - p[k]).collect();
                let aa: f64 = a.iter().map(|v| v * v).sum();
                if aa == 0.0 {
                    return bad();
                }
                let d: Vec<T<'h>> = (0..n).map(|k| ar.sub(comps[k], h.real(p[k]))).collect();
                let da = ar.add(&(0..n).map(|k| ar.mul(&[h.real(a[k]), d[k]])).collect::<Vec<_>>());
                let dd = ar.add(&(0..n).map(|k| ar.mul(&[d[k], d[k]])).collect::<Vec<_>>());
                let side = ar.sub(h.real(r * r * aa), ar.sub(ar.mul(&[dd, h.real(aa)]), ar.mul(&[da, da])));
                Ok(Ok(vec![vec![
                    Atom::Ge(cx.eval(da)?),
                    Atom::Ge(cx.eval(ar.sub(h.real(aa), da))?),
                    Atom::Ge(cx.eval(side)?),
                ]]))
            }
            "interval" if n == 1 => {
                let Some(&[a, b]) = dargs.first().and_then(|&p| cx.as_list(p)) else { return bad() };
                let x0 = comps[0];
                Ok(Ok(vec![vec![Atom::Ge(cx.eval(ar.sub(x0, a))?), Atom::Ge(cx.eval(ar.sub(b, x0))?)]]))
            }
            "triangle" | "polygon" if n == 2 => {
                let pts = match dargs.first() {
                    Some(&p) => point_list(cx, p)?,
                    None if dname == "triangle" => Some(vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]]),
                    None => None,
                };
                let Some(pts) = pts else { return bad() };
                if pts.len() < 3 {
                    return bad();
                }
                // a convex polygon: on the inner side of every edge
                let area: f64 = (0..pts.len())
                    .map(|i| {
                        let (p, q) = (&pts[i], &pts[(i + 1) % pts.len()]);
                        p[0] * q[1] - q[0] * p[1]
                    })
                    .sum();
                let sgn = if area >= 0.0 { 1.0 } else { -1.0 };
                let mut v = Vec::new();
                for i in 0..pts.len() {
                    let (p, q) = (&pts[i], &pts[(i + 1) % pts.len()]);
                    // cross((q - p), (x - p)) * sgn >= 0
                    let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
                    let e = ar.sub(
                        ar.mul(&[h.real(ex * sgn), ar.sub(comps[1], h.real(p[1]))]),
                        ar.mul(&[h.real(ey * sgn), ar.sub(comps[0], h.real(p[0]))]),
                    );
                    v.push(Atom::Ge(cx.eval(e)?));
                }
                Ok(Ok(vec![v]))
            }
            _ => bad(),
        }
    }
}

/// The printed form of a term, for messages.
fn printed(h: &Heap, t: T<'_>) -> String {
    let s = match h.view(t) {
        View::Sym(x) => h.sym_name(x),
        View::App { head, .. } => match h.view(head) {
            View::Sym(x) => format!("{}(...)", h.sym_name(x)),
            _ => "a term".to_string(),
        },
        _ => "a number or string".to_string(),
    };
    if s.chars().count() > 80 {
        let cut: String = s.chars().take(77).collect();
        format!("{cut}...")
    } else {
        s
    }
}

/// The problem over machine reals: values and Jacobian of `[f, c...]`.
struct TermProblem<'r, 'h> {
    vals: mc_numfn::Auto<'h, mc_numfn::Machine<'r, 'h>>,
    jac: Option<mc_numfn::Auto<'h, mc_numfn::Machine<'r, 'h>>>,
    kinds: Vec<Kind>,
    n: usize,
}

impl<'h> Problem<'h> for TermProblem<'_, 'h> {
    fn n(&self) -> usize {
        self.n
    }
    fn kinds(&self) -> &[Kind] {
        &self.kinds
    }
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool> {
        Ok(self.vals.eval(cx, x, out)? && out.iter().all(|v| v.is_finite()))
    }
    fn eval_many(&mut self, cx: &mut NativeCx<'_, '_, 'h>, xs: &[f64], out: &mut [f64]) -> Res<bool> {
        Ok(self.vals.eval_flat(cx, xs, out)? && out.iter().all(|v| v.is_finite()))
    }
    fn jac(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool> {
        match self.jac.as_mut() {
            Some(j) => {
                if j.eval(cx, x, out)? && out.iter().all(|v| v.is_finite()) {
                    return Ok(true);
                }
                // a symbolic derivative not numeric here (abs at 0): differences
                optim::fd_jac(cx, self, x, out)
            }
            None => optim::fd_jac(cx, self, x, out),
        }
    }
}

/// The options this family accepts beyond the root finder's (they are read and ignored where a fixed method is
/// used: the method names of other systems, monitors, random seeds).
const IGNORED: &[&str] = &[
    "gradient",
    "evaluation_monitor",
    "step_monitor",
    "random_seed",
    "seed",
    "search_points",
    "initial_points",
];

/// The arguments without the options this family ignores, and whether `method` names something other than the
/// root finder's methods (it is then dropped: the method follows the problem).
pub(crate) fn drop_ignored<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], keep_method: bool) -> Vec<T<'h>> {
    let h = cx.heap();
    let mut out = args.to_vec();
    let Some(&last) = args.last() else { return out };
    let View::App { head, args: rules } = h.view(last) else { return out };
    if head != h.sym_term(h.sym("options")) {
        return out;
    }
    let keep: Vec<T<'h>> = rules
        .iter()
        .copied()
        .filter(|&r| match h.view(r) {
            View::App { args: &[k, _], .. } => match h.view(k) {
                View::Sym(s) => {
                    let n = h.sym_name(s);
                    !IGNORED.contains(&n.as_str()) && (n != "method" || keep_method)
                }
                _ => true,
            },
            _ => true,
        })
        .map(|r| {
            // a method with suboptions: its name; a method of another system: auto (working_precision: is read by
            // the option parser, as digits:)
            if let View::App { head, args: &[k, v] } = h.view(r) {
                if let View::Sym(s) = h.view(k) {
                    if h.sym_name(s) == "method" {
                        let m = match h.view(v) {
                            View::App { args: ms, .. } if !ms.is_empty() && cx.as_list(v).is_some() => ms[0],
                            _ => v,
                        };
                        let known = match h.view(m) {
                            View::Str(x) => KNOWN_METHODS.contains(&x.to_ascii_lowercase().replace(['_', ' '], "").as_str()),
                            View::Sym(x) => KNOWN_METHODS.contains(&h.sym_name(x).to_ascii_lowercase().replace('_', "").as_str()),
                            _ => false,
                        };
                        let m = if known { m } else { h.sym_term(h.sym("auto")) };
                        return h.app(head, &[k, m]);
                    }
                }
            }
            r
        })
        .collect();
    out.pop();
    out.push(h.app(head, &keep));
    out
}

/// The method names of the root finder and minimiser (others name methods of other systems and become `auto`).
const KNOWN_METHODS: &[&str] = &["newton", "secant", "brent", "bisection", "ridders", "regulafalsi", "falseposition", "illinois", "broyden", "quasinewton", "bfgs", "auto", "automatic"];

impl Native for Optimize {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let name = self.name;
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        if !self.global && self.out != Out::Full {
            // the value or position of find_minimum's result
            let base = if self.maximum { "find_maximum" } else { "find_minimum" };
            let r = cx.eval(h.call(h.sym(base), args))?;
            let Some(&[v, rs]) = cx.as_list(r) else { return Ok(Reply::Done(r)) };
            if self.out == Out::Value {
                return Ok(Reply::Done(v));
            }
            let Some(rs) = cx.as_list(rs) else { return Ok(Reply::Done(r)) };
            let vals: Vec<T<'h>> = rs.iter().map(|&r| match h.view(r) {
                View::App { args: &[_, x], .. } => x,
                _ => r,
            }).collect();
            // the position is always a list of the variables' values
            return Ok(Reply::Done(cx.list(&vals)));
        }
        // the global forms hold their arguments (a vector variable is substituted before the expressions are
        // evaluated, so dot(m, v) + [a, b] does not thread over the list as if dot(m, v) were a number); the
        // options are evaluated here
        let mut args = args.to_vec();
        if let Some(&last) = args.last() {
            if matches!(h.view(last), View::App { head, .. } if head == h.sym_term(h.sym("options"))) {
                let v = cx.eval(last)?;
                *args.last_mut().expect("nonempty") = v;
            }
        }
        let args = drop_ignored(cx, &args, false);
        let (args, opts) = match split_options(h, &ar, name, &args) {
            Ok(v) => v,
            Err(r) => return Ok(r),
        };
        let &[f, s] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        solve(self, cx, &ar, f, s, opts.compile, opts.digits).map(Reply::Done)
    }
}

/// The variables of argument 2 and the constraints it adds; `None` if malformed.
fn variables<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    s: T<'h>,
    hd: &Heads<'h>,
) -> Res<Option<(Vec<VarSpec>, Vec<T<'h>>)>> {
    let h = cx.heap();
    let mut vars = Vec::new();
    let mut extra = Vec::new();
    let real = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<f64>> {
        let v = match ar.num(t) {
            Some(v) => v,
            None => match ar.num(cx.eval(h.call(h.sym("n"), &[t]))?) {
                Some(v) => v,
                None => return Ok(None),
            },
        };
        Ok(v.as_real().map(|r| r.to_f64()).filter(|x| x.is_finite()))
    };
    let item = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, vars: &mut Vec<VarSpec>, extra: &mut Vec<T<'h>>| -> Res<bool> {
        let new = |sym| VarSpec { sym, len: None, start: None, range: None, integer: false };
        match h.view(t) {
            View::Sym(sym) => {
                vars.push(new(sym));
                Ok(true)
            }
            View::App { head, args: &[x, _] } if head == hd.element => {
                let xs: Vec<T<'h>> = match cx.as_list(x) {
                    Some(xs) => xs.to_vec(),
                    None => vec![x],
                };
                for e in xs {
                    let View::Sym(sym) = h.view(e) else { return Ok(false) };
                    vars.push(new(sym));
                }
                extra.push(t);
                Ok(true)
            }
            _ => match cx.as_list(t) {
                Some(&[x, a]) | Some(&[x, a, _, _]) => {
                    let View::Sym(sym) = h.view(x) else { return Ok(false) };
                    let mut v = new(sym);
                    v.start = real(cx, a)?;
                    if v.start.is_none() {
                        return Ok(false);
                    }
                    if let Some(&[_, _, lo, hi]) = cx.as_list(t) {
                        let (Some(lo), Some(hi)) = (real(cx, lo)?, real(cx, hi)?) else { return Ok(false) };
                        extra.push(cx.eval(h.call(h.sym("less_equal"), &[h.real(lo), x, h.real(hi)]))?);
                    }
                    vars.push(v);
                    Ok(true)
                }
                Some(&[x, a, b]) => {
                    let View::Sym(sym) = h.view(x) else { return Ok(false) };
                    let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else { return Ok(false) };
                    let mut v = new(sym);
                    v.start = Some(0.5 * (a + b));
                    v.range = Some((a.min(b), a.max(b)));
                    vars.push(v);
                    Ok(true)
                }
                _ => Ok(false),
            },
        }
    };
    let ok = match (h.view(s), cx.as_list(s)) {
        (_, Some(items)) if !items.is_empty() => {
            // [x, x0] alone is one variable with a start; [x, y] two variables
            // ([x, element(y, integers)] is two variables)
            let is_element = |a: T<'h>| matches!(h.view(a), View::App { head, .. } if head == hd.element);
            let single = matches!(items, [x, a] if matches!(h.view(*x), View::Sym(_)) && !matches!(h.view(*a), View::Sym(_)) && cx.as_list(*a).is_none() && !is_element(*a))
                || matches!(items, [x, _, _] | [x, _, _, _] if matches!(h.view(*x), View::Sym(_)) && items[1..].iter().all(|&e| !matches!(h.view(e), View::Sym(_)) && cx.as_list(e).is_none()));
            if single {
                item(cx, s, &mut vars, &mut extra)?
            } else {
                let mut ok = true;
                for &i in items {
                    ok &= item(cx, i, &mut vars, &mut extra)?;
                }
                ok
            }
        }
        _ => item(cx, s, &mut vars, &mut extra)?,
    };
    let distinct: HashSet<Sym> = vars.iter().map(|v| v.sym).collect();
    Ok((ok && !vars.is_empty() && distinct.len() == vars.len()).then_some((vars, extra)))
}

/// The constraints as plain conditions: a constraint named by a symbol is its value, nested lists of constraints
/// are flattened, an inactive head `inactive(g)` is `g` (the held arguments are substituted before they are
/// evaluated, so the sum of a vector term and a list cannot thread too early), and an equation `a == c` for a
/// symbol `a` that is not a variable and a numeric value `c` (a number, vector or matrix) fixes `a`: `a` is
/// replaced by `c` everywhere. `None` when a parameter is given two different values.
fn prepare_constraints<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    obj: T<'h>,
    cons: Vec<T<'h>>,
    vars: &[VarSpec],
) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let h = cx.heap();
    let inactive = h.sym_term(h.sym("inactive"));
    let activate = |t: T<'h>| {
        mc_numfn::replace(h, t, &mut |u| match h.view(u) {
            View::App { head, args: &[g] } if head == inactive && matches!(h.view(g), View::Sym(_)) => Some(g),
            _ => None,
        })
    };
    let mut flat = Vec::new();
    let mut todo: Vec<T<'h>> = cons.into_iter().rev().collect();
    while let Some(c) = todo.pop() {
        cx.tick(1)?;
        let c = match h.view(c) {
            View::Sym(_) => cx.eval(c)?,
            _ => c,
        };
        match cx.as_list(c) {
            Some(items) => todo.extend(items.iter().rev().copied()),
            None => flat.push(activate(c)),
        }
    }
    let equal = h.sym_term(h.sym("equal"));
    let is_var = |s: Sym| vars.iter().any(|v| v.sym == s);
    // a number, or a (nested) list of numbers
    let numeric = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<bool> {
        let items = cx.eval(h.call(h.sym("flatten"), &[cx.list(&[t])]))?;
        let ar = Arith::new(h);
        Ok(cx.as_list(items).is_some_and(|xs| !xs.is_empty() && xs.iter().all(|&x| ar.num(x).is_some())))
    };
    let mut params: Vec<(T<'h>, T<'h>)> = Vec::new();
    let mut rest = Vec::new();
    for c in flat {
        if let View::App { head, args: &[a, b] } = h.view(c) {
            if head == equal && matches!(h.view(a), View::Sym(s) if !is_var(s)) {
                let b = cx.eval(b)?;
                if numeric(cx, b)? {
                    match params.iter().find(|(p, _)| *p == a) {
                        Some(&(_, w)) if w != b => return Ok(None),
                        Some(_) => {}
                        None => params.push((a, b)),
                    }
                    continue;
                }
            }
        }
        rest.push(c);
    }
    let fix = |t: T<'h>| {
        if params.is_empty() {
            return t;
        }
        mc_numfn::replace(h, t, &mut |u| params.iter().find(|(p, _)| *p == u).map(|&(_, v)| v))
    };
    let obj = fix(activate(obj));
    Ok(Some((obj, rest.into_iter().map(fix).collect())))
}

/// The heads that show a variable is a vector.
const VECTOR_HEADS: &[&str] = &["dot", "total", "norm", "indexed", "part", "vector_greater_equal", "vector_less_equal", "vector_greater", "vector_less"];

/// The most components of a vector variable whose length is found by trying.
const MAX_VECTOR: usize = 16;

/// Solve one problem.
fn solve<'h>(
    op: &Optimize,
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    s: T<'h>,
    pol: Policy,
    digits: Option<u64>,
) -> Res<T<'h>> {
    let h = cx.heap();
    let name = op.name;
    let hd = Heads::new(h);
    let bad = |k: T<'h>| match bad_argument(h, name, k) {
        Reply::Done(t) => t,
        _ => unreachable!(),
    };
    // a held first argument that is not written as a list: its value
    let f = if cx.as_list(f).is_some() { f } else { cx.eval(f)? };
    let (obj, mut cons): (T<'h>, Vec<T<'h>>) = match cx.as_list(f) {
        Some([o, rest @ ..]) if cx.as_list(*o).is_none() => (*o, rest.to_vec()),
        Some(_) => return Ok(bad(h.int(1))),
        None => (f, vec![]),
    };
    let Some((mut vars, extra)) = variables(cx, ar, s, &hd)? else { return Ok(bad(h.int(2))) };
    cons.extend(extra);
    let (obj, cons) = match prepare_constraints(cx, obj, cons, &vars)? {
        Some(v) => v,
        None => return Ok(bad(h.int(1))),
    };
    // vector variables: a length given by vectors(n, ...) or found by trying
    let vsyms: Vec<Sym> = VECTOR_HEADS.iter().map(|n| h.sym(n)).collect();
    // after the substitution these must be gone (the vector comparisons stay as constraints)
    let left: Vec<Sym> = vsyms[..5].to_vec();
    let mut all = vec![obj];
    all.extend(cons.iter().copied());
    let whole = cx.list(&all);
    for k in 0..vars.len() {
        let v = vars[k].sym;
        let given = vector_length(cx, &cons, v)?;
        let needs = given.is_some() || {
            // is the symbol an argument of a vector head?
            find_in_heads(h, whole, v, &vsyms)
        };
        if !needs {
            continue;
        }
        let lens: Vec<usize> = match given {
            Some(n) => vec![n],
            None => (1..=MAX_VECTOR).collect(),
        };
        let mut chosen = None;
        for n in lens {
            cx.tick(1)?;
            let comps: Vec<T<'h>> = (1..=n).map(|i| h.sym_term(h.sym(&format!("{}·{i}", h.sym_name(v))))).collect();
            let vec_t = cx.list(&comps);
            let comps_v = comps.clone();
            let sub = mc_numfn::replace(h, whole, &mut |u| {
                if u == h.sym_term(v) {
                    return Some(vec_t);
                }
                indexed_component(h, u, h.sym_term(v), &comps_v)
            });
            let ev = cx.eval(sub)?;
            if is_failure(h, ev) || has_failure(h, ev) || mentions(cx, ev, &left)? {
                continue;
            }
            // the objective a scalar
            if cx.as_list(ev).and_then(|l| l.first()).is_some_and(|&o| cx.as_list(o).is_some()) {
                continue;
            }
            chosen = Some(n);
            break;
        }
        match chosen {
            Some(n) => vars[k].len = Some(n),
            None => return Ok(bad(h.int(2))),
        }
    }
    // the scalar slots and the substitution of vector variables
    let mut slots: Vec<Sym> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    let mut subst: Vec<(T<'h>, T<'h>)> = Vec::new();
    for (k, v) in vars.iter().enumerate() {
        match v.len {
            None => {
                slots.push(v.sym);
                owner.push(k);
            }
            Some(n) => {
                let comps: Vec<Sym> = (1..=n).map(|i| h.sym(&format!("{}·{i}", h.sym_name(v.sym)))).collect();
                let lt = cx.list(&comps.iter().map(|&c| h.sym_term(c)).collect::<Vec<_>>());
                subst.push((h.sym_term(v.sym), lt));
                for c in comps {
                    slots.push(c);
                    owner.push(k);
                }
            }
        }
    }
    let cx_lists: Vec<(T<'h>, Vec<T<'h>>)> =
        subst.iter().map(|&(a, b)| (a, cx.as_list(b).map(|l| l.to_vec()).unwrap_or_default())).collect();
    let sub = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<T<'h>> {
        if subst.is_empty() {
            return cx.eval(t);
        }
        let r = mc_numfn::replace(h, t, &mut |u| {
            for &(a, b) in &subst {
                if a == u {
                    return Some(b);
                }
                if let Some(cs) = cx_lists.iter().find(|(x, _)| *x == a).map(|(_, c)| c) {
                    if let Some(c) = indexed_component(h, u, a, cs) {
                        return Some(c);
                    }
                }
            }
            None
        });
        cx.eval(r)
    };
    let obj = sub(cx, obj)?;
    let obj = if op.maximum { cx.eval(ar.neg(obj))? } else { obj };
    if cx.as_list(obj).is_some() {
        return Ok(bad(h.int(1)));
    }
    let mut reader = Reader { hd: Heads::new(h), ints: HashSet::new(), sign: Vec::new() };
    let mut ctree = Vec::new();
    for &c in &cons {
        ctree.push(sub(cx, c)?);
    }
    let ctree = cx.list(&ctree);
    // a symbol without a value (a parameter `a` in x^2 - a*x): the optimum depends on it, and a number for the
    // unknown is no answer (the objective was once taken as constant and the point of a "no feasible point" message)
    {
        let mut stack = vec![obj, ctree];
        let mut seen = HashSet::new();
        while let Some(u) = stack.pop() {
            if !seen.insert(u) {
                continue;
            }
            match h.view(u) {
                View::App { head, args } => {
                    // a domain (element(x, integers), vectors(2, nonpositive_reals)) is no parameter
                    if matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "element" | "vectors" | "matrices")) {
                        continue;
                    }
                    if matches!(h.view(head), View::App { .. }) {
                        stack.push(head);
                    }
                    stack.extend_from_slice(args);
                }
                View::Sym(s) if !slots.contains(&s) && !vars.iter().any(|v| v.sym == s) => {
                    // the head of a call is not a parameter; a constant (pi, e) has a numeric value
                    let nv = cx.eval(h.call(h.sym("n"), &[u]))?;
                    if ar.num(nv).is_none() && !matches!(h.sym_name(s).as_str(), "inf" | "indeterminate" | "true" | "false") {
                        message(
                            cx,
                            name,
                            &format!("the objective or a constraint contains the symbol {} without a value: n_minimize and n_maximize need numerical parameters", h.sym_name(s)),
                        )?;
                        return Ok(h.call(h.sym(names::FAILURE), &[h.sym_term(h.sym("not_numeric")), f]));
                    }
                }
                _ => {}
            }
        }
    }
    let branches = match reader.read(cx, ar, ctree)? {
        Ok(b) => b,
        Err(m) => {
            message(cx, name, &m)?;
            return Ok(bad(h.int(1)));
        }
    };
    let mut int = vec![false; slots.len()];
    for (k, &sl) in slots.iter().enumerate() {
        int[k] = reader.ints.contains(&sl) || reader.ints.contains(&vars[owner[k]].sym) || vars[owner[k]].integer;
    }
    let mut sign_atoms = Vec::new();
    for &(t, nonneg) in &reader.sign {
        let e = sub(cx, t)?;
        sign_atoms.push(Atom::Ge(if nonneg { e } else { cx.eval(ar.neg(e))? }));
    }
    // integer vector components
    for (k, v) in vars.iter().enumerate() {
        if reader.ints.contains(&v.sym) {
            for (j, &o) in owner.iter().enumerate() {
                if o == k {
                    int[j] = true;
                }
            }
        }
    }
    let n = slots.len();
    let mut best: Option<(Point, bool)> = None; // (point, proven)
    let mut status = Status::Infeasible;
    let fobj = numeric(cx, obj, None)?;
    if branches.is_empty() {
        status = Status::Infeasible;
    }
    for branch in &branches {
        cx.tick(1)?;
        let mut atoms = branch.clone();
        atoms.extend(sign_atoms.iter().cloned());
        let kinds: Vec<Kind> = atoms.iter().map(|a| if matches!(a, Atom::Eq(_)) { Kind::Eq } else { Kind::Ge }).collect();
        let mut items = vec![fobj];
        for a in &atoms {
            let (Atom::Ge(e) | Atom::Eq(e)) = a;
            items.push(numeric(cx, *e, None)?);
        }
        let body = cx.list(&items);
        let jac_t = derivative(cx, body, &slots, true)?;
        let slot_syms: Vec<Sym> = slots.clone();
        let linear = match jac_t {
            Some(j) => !mentions(cx, j, &slot_syms)?,
            None => false,
        };
        let mut fm = TermFn::new(h, &slots, body);
        if fm.outputs() != items.len() {
            return Ok(bad(h.int(1)));
        }
        let mut jm = match jac_t {
            Some(j) => Some(TermFn::new(h, &slots, numeric(cx, j, None)?)),
            None => None,
        }
        .filter(|j| j.outputs() == items.len() * n);
        let vals = auto(cx, &mut fm, ar, pol);
        let jac = jm.as_mut().map(|j| auto(cx, j, ar, pol));
        let mut prob = TermProblem { vals, jac, kinds: kinds.clone(), n };
        let r = if linear { linear_branch(cx, &mut prob, &int)? } else { nonlinear_branch(op, cx, &mut prob, &vars, &owner, &int)? };
        match r {
            BranchResult::Point(p, proven) => {
                if best.as_ref().is_none_or(|(b, _)| p.better(b)) {
                    best = Some((p, proven));
                }
                status = Status::Found;
            }
            BranchResult::Unbounded => {
                status = Status::Unbounded;
                best = None;
                break;
            }
            BranchResult::Infeasible => {}
        }
    }
    let sgn = if op.maximum { -1.0 } else { 1.0 };
    let indet = |h: &'h Heap| h.sym_term(h.sym("indeterminate"));
    // working_precision: d (digits: d): the machine optimum refined with balls by the local minimiser, when it is
    // an interior point of scalar real variables (no equations, no integer variables, the inequalities strictly
    // satisfied)
    if let (Some(d), true, Some((p, _))) = (digits, matches!(status, Status::Found), best.as_ref()) {
        if subst.is_empty() && !int.iter().any(|&b| b) && interior(cx, ar, &branches, &sign_atoms, &slots, &p.x)? {
            let single = vars.len() == 1 && cx.as_list(s).is_none();
            if let Some(r) = refine_optimum(op, cx, ar, obj, &slots, &p.x, d, single)? {
                return Ok(r);
            }
        }
    }
    // an optimum on the boundary of the feasible set: the Karush-Kuhn-Tucker system refined at d digits
    if let (Some(d), true, Some((p, _))) = (digits, matches!(status, Status::Found), best.as_ref()) {
        if subst.is_empty() && !int.iter().any(|&b| b) && !interior(cx, ar, &branches, &sign_atoms, &slots, &p.x)? {
            let single = vars.len() == 1 && cx.as_list(s).is_none();
            if let Some(r) = refine_kkt(op, cx, ar, obj, &branches, &sign_atoms, &slots, &p.x, d, single)? {
                return Ok(r);
            }
        }
    }
    let (fval, xs): (T<'h>, Option<Vec<f64>>) = match (status, best) {
        (Status::Unbounded, _) => {
            message(cx, name, "the objective is unbounded on the feasible set")?;
            (if op.maximum { inf(cx, ar, true)? } else { inf(cx, ar, false)? }, None)
        }
        (_, Some((p, proven))) => {
            if p.viol > optim::feas_tol(p.x.iter().fold(0.0f64, |a, e| a.max(e.abs()))) {
                // an infeasible point is no minimum: no value and no position, as for an infeasible problem
                message(cx, name, &format!("the constraints could not be met: the best point found violates them by {:e} (relative to the size of each constraint), so no optimum is given", p.viol))?;
                (indet(h), None)
            } else {
                if !proven {
                    message(cx, name, "the result is not proven optimal: the first-order conditions were not met to machine precision at it")?;
                }
                (h.real(sgn * p.f), Some(p.x))
            }
        }
        _ => {
            message(cx, name, "no point satisfies the constraints")?;
            (if op.maximum { inf(cx, ar, false)? } else { inf(cx, ar, true)? }, None)
        }
    };
    // the values of the variables (integers as integers)
    let mut values: Vec<T<'h>> = Vec::new();
    for (k, v) in vars.iter().enumerate() {
        let comps: Vec<T<'h>> = owner
            .iter()
            .enumerate()
            .filter(|&(_, &o)| o == k)
            .map(|(j, _)| match &xs {
                None => indet(h),
                Some(x) if int[j] => h.int(x[j].round() as i64),
                Some(x) => h.real(clean(x[j], x)),
            })
            .collect();
        values.push(match v.len {
            None => comps[0],
            Some(_) if xs.is_none() => indet(h),
            Some(_) => cx.list(&comps),
        });
    }
    let rule = h.sym(names::RULE);
    let rs: Vec<T<'h>> = vars.iter().zip(&values).map(|(v, &x)| h.call(rule, &[h.sym_term(v.sym), x])).collect();
    Ok(match op.out {
        Out::Full => cx.list(&[fval, cx.list(&rs)]),
        Out::Value => fval,
        Out::Arg => {
            // one variable written alone: its value; else the list
            if vars.len() == 1 && cx.as_list(s).is_none() {
                values[0]
            } else {
                cx.list(&values)
            }
        }
    })
}

/// Is the point strictly inside every branch's inequalities (and does no branch carry an equation)? Only then is
/// the optimum a stationary point of the objective, which the local minimiser refines.
fn interior<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    branches: &[Vec<Atom<'h>>],
    sign_atoms: &[Atom<'h>],
    slots: &[Sym],
    x: &[f64],
) -> Res<bool> {
    let h = cx.heap();
    let atoms: Vec<&Atom<'h>> = sign_atoms.iter().chain(branches.iter().flatten()).collect();
    if atoms.is_empty() {
        return Ok(true);
    }
    // one branch must hold strictly at the point (a single branch: all of its atoms)
    let rule = h.sym(names::RULE);
    let rules: Vec<T<'h>> = slots.iter().zip(x).map(|(&s, &v)| h.call(rule, &[h.sym_term(s), h.real(v)])).collect();
    let rs = cx.list(&rules);
    let holds = |cx: &mut NativeCx<'_, '_, 'h>, a: &Atom<'h>| -> Res<bool> {
        let Atom::Ge(e) = a else { return Ok(false) };
        let v = cx.eval(h.call(h.sym("rewrite"), &[*e, rs]))?;
        Ok(ar.num(v).is_some_and(|n| matches!(n, mc_arith::Num::Real(r) if r.to_f64() > 1e-6)))
    };
    for a in sign_atoms {
        if !holds(cx, a)? {
            return Ok(false);
        }
    }
    for b in branches {
        let mut all = true;
        for a in b {
            if !holds(cx, a)? {
                all = false;
                break;
            }
        }
        if all {
            return Ok(true);
        }
    }
    Ok(branches.is_empty())
}

/// The optimum at `d` digits: `find_minimum(obj, [[x1, x1*], ...], digits: d)` from the machine point, the value
/// negated back for a maximum; `None` when the refinement fails (the machine result stands).
#[allow(clippy::too_many_arguments)]
fn refine_optimum<'h>(
    op: &Optimize,
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    obj: T<'h>,
    slots: &[Sym],
    x: &[f64],
    d: u64,
    single: bool,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let starts: Vec<T<'h>> = slots.iter().zip(x).map(|(&s, &v)| cx.list(&[h.sym_term(s), h.real(v)])).collect();
    let spec = cx.list(&starts);
    let opts = h.call(h.sym("options"), &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("digits")), h.int(d as i64)])]);
    let r = cx.eval(h.call(h.sym("find_minimum"), &[obj, spec, opts]))?;
    let Some(&[v, rs]) = cx.as_list(r) else { return Ok(None) };
    if ar.num(v).is_none() {
        return Ok(None);
    }
    let v = if op.maximum { cx.eval(ar.neg(v))? } else { v };
    let Some(rs) = cx.as_list(rs) else { return Ok(None) };
    let vals: Vec<T<'h>> = rs.iter().map(|&r| match h.view(r) {
        View::App { args: &[_, x], .. } => x,
        _ => r,
    }).collect();
    Ok(Some(match op.out {
        Out::Full => r_with(cx, v, rs),
        Out::Value => v,
        Out::Arg => {
            if single {
                vals[0]
            } else {
                cx.list(&vals)
            }
        }
    }))
}

/// A symbolic expression at the machine point `x` as a machine real.
fn value_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>, slots: &[Sym], x: &[f64]) -> Res<Option<f64>> {
    let h = cx.heap();
    let rules: Vec<T<'h>> = slots.iter().zip(x).map(|(&s, &v)| h.call(h.sym(names::RULE), &[h.sym_term(s), h.real(v)])).collect();
    let rs = cx.list(&rules);
    let v = cx.eval(h.call(h.sym("rewrite"), &[e, rs]))?;
    let v = cx.eval(h.call(h.sym("n"), &[v]))?;
    Ok(match h.view(v) {
        View::Real(r) if r.is_finite() => Some(r),
        View::Int(i) => Some(i as f64),
        _ => None,
    })
}

/// The optimum on a boundary at `d` digits (Karush-Kuhn-Tucker): with the active constraints `e_i >= 0` (or `= 0`)
/// at the machine optimum `x`, the multipliers `lambda` of `grad F = sum lambda_i grad e_i` (`F` the objective, negated
/// for a maximum) come from a least-squares solve at `x`, then `find_root` solves `grad F - sum lambda_i grad e_i = 0,
/// e_i = 0` at `d` digits from `(x, lambda)`. Refused (`None`, the machine result stands) when a multiplier of an
/// inequality is negative (the point is not a KKT point), no constraint is active, or the system does not converge.
/// Reference: Nocedal and Wright, Numerical Optimization, ch. 12.
#[allow(clippy::too_many_arguments)]
fn refine_kkt<'h>(
    op: &Optimize,
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    obj: T<'h>,
    branches: &[Vec<Atom<'h>>],
    sign_atoms: &[Atom<'h>],
    slots: &[Sym],
    x: &[f64],
    d: u64,
    single: bool,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let n = slots.len();
    // the branch the point lies in: all its atoms hold within the tolerance
    let scale = x.iter().fold(1.0f64, |a, v| a.max(v.abs()));
    let tol = 1e-7 * scale;
    let mut chosen: Option<Vec<&Atom<'h>>> = None;
    let none: Vec<Atom<'h>> = vec![];
    let all_branches: Vec<&Vec<Atom<'h>>> = if branches.is_empty() { vec![&none] } else { branches.iter().collect() };
    for b in all_branches {
        let atoms: Vec<&Atom<'h>> = sign_atoms.iter().chain(b.iter()).collect();
        let mut ok = true;
        for a in &atoms {
            let (Atom::Ge(e) | Atom::Eq(e)) = a;
            let Some(v) = value_at(cx, *e, slots, x)? else {
                ok = false;
                break;
            };
            if matches!(a, Atom::Eq(_)) && v.abs() > tol || matches!(a, Atom::Ge(_)) && v < -tol {
                ok = false;
                break;
            }
        }
        if ok {
            chosen = Some(atoms);
            break;
        }
    }
    let Some(atoms) = chosen else { return Ok(None) };
    // the active constraints
    let mut active: Vec<(T<'h>, bool)> = vec![];
    for a in &atoms {
        let (e, eq) = match a {
            Atom::Ge(e) => (*e, false),
            Atom::Eq(e) => (*e, true),
        };
        let v = value_at(cx, e, slots, x)?.unwrap_or(f64::INFINITY);
        if eq || v.abs() <= tol {
            active.push((e, eq));
        }
    }
    let m = active.len();
    if m == 0 || m > n {
        return Ok(None);
    }
    // (the objective of a maximum arrives negated: the minimum of obj is sought either way)
    let fobj = obj;
    // gradients: of F, and of each active e_i, symbolic and numeric at x
    let grad = |cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>| -> Res<Option<(Vec<T<'h>>, Vec<f64>)>> {
        let mut ts = Vec::with_capacity(n);
        let mut vs = Vec::with_capacity(n);
        for &sl in slots {
            let dt = cx.eval(h.call(h.sym("d"), &[e, h.sym_term(sl)]))?;
            let Some(v) = value_at(cx, dt, slots, x)? else { return Ok(None) };
            ts.push(dt);
            vs.push(v);
        }
        Ok(Some((ts, vs)))
    };
    let Some((gf_t, gf)) = grad(cx, fobj)? else { return Ok(None) };
    let mut ge_t: Vec<Vec<T<'h>>> = vec![];
    let mut ge: Vec<Vec<f64>> = vec![];
    for &(e, _) in &active {
        let Some((t, v)) = grad(cx, e)? else { return Ok(None) };
        ge_t.push(t);
        ge.push(v);
    }
    // least squares: (G G^T) lambda = G gf
    let mut a = vec![vec![0.0; m + 1]; m];
    for i in 0..m {
        for j in 0..m {
            a[i][j] = (0..n).map(|k| ge[i][k] * ge[j][k]).sum();
        }
        a[i][m] = (0..n).map(|k| ge[i][k] * gf[k]).sum();
    }
    for c in 0..m {
        let piv = (c..m).max_by(|&p, &q| a[p][c].abs().total_cmp(&a[q][c].abs())).unwrap_or(c);
        if a[piv][c].abs() < 1e-12 {
            return Ok(None);
        }
        a.swap(c, piv);
        for r in 0..m {
            if r != c {
                let f = a[r][c] / a[c][c];
                for k in c..=m {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    let lam: Vec<f64> = (0..m).map(|i| a[i][m] / a[i][i]).collect();
    // the multiplier of an inequality must not be negative (up to the rounding of the machine point)
    let gscale = 1.0 + gf.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    for (i, &(_, eq)) in active.iter().enumerate() {
        if !eq && lam[i] < -1e-7 * gscale {
            return Ok(None);
        }
    }
    // the system
    let lam_syms: Vec<T<'h>> = (0..m).map(|i| h.sym_term(h.sym(&format!("nopt$lambda{i}")))).collect();
    let mut eqs = Vec::with_capacity(n + m);
    for k in 0..n {
        let mut terms = vec![gf_t[k]];
        for i in 0..m {
            let li = cx.eval(h.call(h.sym(names::MUL), &[h.int(-1), lam_syms[i], ge_t[i][k]]))?;
            terms.push(li);
        }
        let sum = cx.eval(h.call(h.sym(names::ADD), &terms))?;
        eqs.push(h.call(h.sym("equal"), &[sum, h.int(0)]));
    }
    for &(e, _) in &active {
        eqs.push(h.call(h.sym("equal"), &[e, h.int(0)]));
    }
    let mut starts: Vec<T<'h>> = slots.iter().zip(x).map(|(&s, &v)| cx.list(&[h.sym_term(s), h.real(v)])).collect();
    for (l, &v) in lam_syms.iter().zip(&lam) {
        starts.push(cx.list(&[*l, h.real(v)]));
    }
    let eqs = cx.list(&eqs);
    let spec = cx.list(&starts);
    let opts = h.call(h.sym("options"), &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("working_precision")), h.int(d as i64)])]);
    let r = cx.eval(h.call(h.sym("find_root"), &[eqs, spec, opts]))?;
    let Some(rs) = cx.as_list(r) else { return Ok(None) };
    if rs.len() != n + m || has_failure(h, r) {
        return Ok(None);
    }
    let vals: Vec<T<'h>> = rs[..n]
        .iter()
        .map(|&r| match h.view(r) {
            View::App { args: &[_, x], .. } => x,
            _ => r,
        })
        .collect();
    if vals.iter().any(|&v| ar.num(v).is_none()) {
        return Ok(None);
    }
    // the objective at the refined point, at d digits
    let at: Vec<T<'h>> = slots.iter().zip(&vals).map(|(&s, &v)| h.call(h.sym(names::RULE), &[h.sym_term(s), v])).collect();
    let rules = cx.list(&at);
    let v = cx.eval(h.call(h.sym("rewrite"), &[obj, rules]))?;
    if ar.num(v).is_none() {
        return Ok(None);
    }
    let v = if op.maximum { cx.eval(ar.neg(v))? } else { v };
    Ok(Some(match op.out {
        Out::Full => r_with(cx, v, &at),
        Out::Value => v,
        Out::Arg => {
            if single {
                vals[0]
            } else {
                cx.list(&vals)
            }
        }
    }))
}

fn r_with<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>, rs: &[T<'h>]) -> T<'h> {
    let l = cx.list(rs);
    cx.list(&[v, l])
}

/// A coordinate below the rounding level of the point (a few units in the last place of its largest
/// coordinate) is zero to the solver's accuracy, and is printed as 0.0 (as is -0.0).
fn clean(x: f64, all: &[f64]) -> f64 {
    let scale = all.iter().fold(0.0f64, |a, e| a.max(e.abs()));
    if x == 0.0 || x.abs() <= 4.0 * f64::EPSILON * scale {
        0.0
    } else {
        x
    }
}

/// `inf` or `-inf` as the evaluator writes them.
fn inf<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, pos: bool) -> Res<T<'h>> {
    let h = cx.heap();
    let i = h.sym_term(h.sym("inf"));
    if pos {
        Ok(i)
    } else {
        cx.eval(ar.neg(i))
    }
}

/// Print a message of the function `name`.
fn message<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, text: &str) -> Res<()> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("message"), &[h.sym_term(h.sym(name)), h.string(text)]))?;
    Ok(())
}

/// Does `t` contain a failure?
fn has_failure(h: &Heap, t: T<'_>) -> bool {
    let fl = h.sym_term(h.sym(names::FAILURE));
    let mut stack = vec![t];
    let mut seen = HashSet::new();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if head == fl {
                return true;
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Is `v` an argument of a call of one of the heads `heads` in `t`?
fn find_in_heads(h: &Heap, t: T<'_>, v: Sym, heads: &[Sym]) -> bool {
    let vt = h.sym_term(v);
    let hs: Vec<T<'_>> = heads.iter().map(|&s| h.sym_term(s)).collect();
    let mut stack = vec![t];
    let mut seen = HashSet::new();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if let Some(k) = hs.iter().position(|&x| x == head) {
                // a direct argument of dot, total, norm, indexed, part; a side of a vector comparison
                let direct = if k < 5 {
                    args.contains(&vt)
                } else {
                    args.first().is_some_and(|&p| match h.view(p) {
                        View::App { args: sides, .. } => sides.contains(&vt),
                        _ => false,
                    })
                };
                if direct {
                    return true;
                }
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}


/// The length of `v` given by `element(v, vectors(n, ...))` in the constraints.
fn vector_length<'h>(cx: &mut NativeCx<'_, '_, 'h>, cons: &[T<'h>], v: Sym) -> Res<Option<usize>> {
    let h = cx.heap();
    let (el, vs) = (h.sym_term(h.sym("element")), h.sym_term(h.sym("vectors")));
    let mut stack: Vec<T<'h>> = cons.to_vec();
    while let Some(u) = stack.pop() {
        cx.tick(1)?;
        if let View::App { head, args } = h.view(u) {
            if head == el && args.len() == 2 && args[0] == h.sym_term(v) {
                if let View::App { head: dh, args: da } = h.view(args[1]) {
                    if dh == vs {
                        if let Some(View::Int(n @ 1..=4096)) = da.first().map(|&d| h.view(d)) {
                            return Ok(Some(n as usize));
                        }
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    Ok(None)
}

/// Where the search ended.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Found,
    Infeasible,
    Unbounded,
}

/// The outcome of one branch of the constraints.
enum BranchResult {
    /// the best point and whether it is proven optimal (locally for the nonlinear solvers)
    Point(Point, bool),
    Infeasible,
    Unbounded,
}

/// An affine problem: the linear program from the values and the Jacobian at 0.
fn linear_branch<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, int: &[bool]) -> Res<BranchResult> {
    let n = p.n();
    let kinds = p.kinds().to_vec();
    let w = kinds.len() + 1;
    let zero = vec![0.0; n];
    let mut v0 = vec![0.0; w];
    let mut j = vec![0.0; w * n];
    if !p.eval(cx, &zero, &mut v0)? || !p.jac(cx, &zero, &mut j)? {
        return Ok(BranchResult::Infeasible);
    }
    let c = j[..n].to_vec();
    let rows: Vec<Row> = kinds
        .iter()
        .enumerate()
        .map(|(i, k)| Row {
            a: j[(1 + i) * n..(2 + i) * n].to_vec(),
            rel: if *k == Kind::Eq { Rel::Eq } else { Rel::Ge },
            b: -v0[1 + i],
        })
        .collect();
    let r = if int.iter().any(|&b| b) {
        let mut ticks = 0u64;
        let mut ok = true;
        let mut tick = || {
            ticks += 1;
            ok
        };
        let r = lp::branch_and_bound(&c, &rows, int, &mut tick);
        let _ = &mut ok;
        match r {
            Some(r) => r,
            None => {
                return Ok(BranchResult::Infeasible);
            }
        }
    } else {
        lp::solve(&c, &rows)
    };
    cx.tick(1)?;
    Ok(match r {
        Lp::Optimal(x, _) => {
            let mut v = vec![0.0; w];
            if !p.eval(cx, &x, &mut v)? {
                return Ok(BranchResult::Infeasible);
            }
            let viol = optim::violation(&v, &kinds);
            BranchResult::Point(Point { f: v[0], viol, x, polished: true }, true)
        }
        Lp::Infeasible => BranchResult::Infeasible,
        Lp::Unbounded => BranchResult::Unbounded,
    })
}

/// The search box of the global method: the given ranges, else bounds read from the constraints, else
/// [-10, 10] around the start.
fn search_box<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, vars: &[VarSpec], owner: &[usize]) -> Res<(Vec<f64>, Vec<f64>)> {
    let n = p.n();
    let kinds = p.kinds().to_vec();
    let w = kinds.len() + 1;
    let mut lo = vec![f64::NEG_INFINITY; n];
    let mut hi = vec![f64::INFINITY; n];
    // single-variable affine inequalities: rows of the Jacobian equal at two points with one nonzero entry
    let (pa, pb): (Vec<f64>, Vec<f64>) = ((0..n).map(|k| 0.3 + 0.1 * k as f64).collect(), (0..n).map(|k| -0.7 + 0.05 * k as f64).collect());
    let (mut ja, mut jb, mut va) = (vec![0.0; w * n], vec![0.0; w * n], vec![0.0; w]);
    if p.jac(cx, &pa, &mut ja)? && p.jac(cx, &pb, &mut jb)? && p.eval(cx, &pa, &mut va)? {
        for (i, k) in kinds.iter().enumerate() {
            let ra = &ja[(1 + i) * n..(2 + i) * n];
            let rb = &jb[(1 + i) * n..(2 + i) * n];
            let nz: Vec<usize> = (0..n).filter(|&q| ra[q] != 0.0).collect();
            if nz.len() == 1 && ra.iter().zip(rb).all(|(a, b)| (a - b).abs() <= 1e-12 * (1.0 + a.abs())) {
                let q = nz[0];
                // c(x) = a (x_q - pa_q) + c(pa) >= 0  ->  x_q >= pa_q - c(pa)/a  (a > 0)
                let bound = pa[q] - va[1 + i] / ra[q];
                let ge = *k == Kind::Eq || ra[q] > 0.0;
                let le = *k == Kind::Eq || ra[q] < 0.0;
                if ge {
                    lo[q] = lo[q].max(bound);
                }
                if le {
                    hi[q] = hi[q].min(bound);
                }
            }
        }
    }
    for k in 0..n {
        let v = &vars[owner[k]];
        if let Some((a, b)) = v.range {
            lo[k] = lo[k].max(a);
            hi[k] = hi[k].min(b);
        }
        let c = v.start.unwrap_or(0.0);
        match (lo[k].is_finite(), hi[k].is_finite()) {
            (true, true) => {}
            (true, false) => hi[k] = lo[k] + 20.0,
            (false, true) => lo[k] = hi[k] - 20.0,
            (false, false) => {
                // c +- 10; a start so large that this is no interval (1e150) gives c +- |c|, which holds the origin
                let r = if c.abs() > 1e7 { c.abs() } else { 10.0 };
                lo[k] = c - r;
                hi[k] = c + r;
            }
        }
        if lo[k] > hi[k] {
            std::mem::swap(&mut lo[k], &mut hi[k]);
        }
    }
    Ok((lo, hi))
}

/// A nonlinear problem: the global search or the local solver, with branch and bound for integers.
fn nonlinear_branch<'h>(
    op: &Optimize,
    cx: &mut NativeCx<'_, '_, 'h>,
    p: &mut dyn Problem<'h>,
    vars: &[VarSpec],
    owner: &[usize],
    int: &[bool],
) -> Res<BranchResult> {
    let n = p.n();
    let set = Settings::default();
    let mut start: Vec<f64> = (0..n).map(|k| vars[owner[k]].start.unwrap_or(1.0)).collect();
    // a start at which the problem is not numeric (x^2 at 1e300 overflows): dropped, the search goes on from the
    // default start (a point moved towards the origin only as far as the overflow ends is as bad: the local solver
    // does not come down from 1e113)
    let mut vars_adj = vars.to_vec();
    {
        let mut v = vec![0.0; p.kinds().len() + 1];
        if !p.eval(cx, &start, &mut v)? {
            start = vec![1.0; n];
            for spec in vars_adj.iter_mut() {
                spec.start = None;
            }
        }
    }
    let vars = &vars_adj[..];
    let (lo, hi) = if op.global { search_box(cx, p, vars, owner)? } else { (vec![], vec![]) };
    // the usual search; with few variables a second, dense one (oscillating objectives can
    // have hundreds of local optima in the unit box: 10^5 Halton points, 64 starts) whose result replaces the
    // first only when it is clearly better (ties keep the first answer)
    let (samples, starts) = ((4000 * n).clamp(4000, 20_000), (8 + 4 * n).min(32));
    let dense = n <= 3;
    let global = op.global;
    let default_start = vars.iter().all(|v| v.start.is_none());
    let solve = |cx: &mut NativeCx<'_, '_, 'h>, q: &mut dyn Problem<'h>| -> Res<Option<Point>> {
        if global {
            let extra: Vec<Vec<f64>> = if vars.iter().any(|v| v.start.is_some() && v.range.is_none()) { vec![start.clone()] } else { vec![] };
            let first = optim::global(cx, q, &lo, &hi, samples, starts, &extra, set)?;
            if !dense {
                return Ok(first);
            }
            let second = optim::global(cx, q, &lo, &hi, 100_000, 64, &extra, set)?;
            Ok(match (first, second) {
                (Some(a), Some(b)) if b.better(&a) && (a.f - b.f).abs() > 1e-9 * (1.0 + a.f.abs()) => Some(b),
                (None, b) => b,
                (a, _) => a,
            })        } else {
            let best = optim::local(cx, q, &start, set)?;
            // no starting value given and constraints present: the default start (1, ..., 1) may sit where the
            // objective has a saddle or a shallow basin (sin(x) sin(2y) on a disk): the other sign patterns of the
            // default start are tried too, and the lowest feasible point wins
            if default_start && !q.kinds().is_empty() && n <= 4 {
                let mut best = best;
                for mask in 1..(1usize << n) {
                    let alt: Vec<f64> = (0..n).map(|k| if mask >> k & 1 == 1 { -1.0 } else { 1.0 }).collect();
                    if let Some(c) = optim::local(cx, q, &alt, set)? {
                        let ok = |pt: &Point| pt.viol <= optim::feas_tol(pt.x.iter().fold(0.0f64, |a, v| a.max(v.abs())));
                        let better = match &best {
                            None => true,
                            Some(b) => (ok(&c) && !ok(b)) || (ok(&c) == ok(b) && c.f < b.f - 1e-9 * (1.0 + b.f.abs())),
                        };
                        if better {
                            best = Some(c);
                        }
                    }
                }
                Ok(best)
            } else {
                Ok(best)
            }
        }
    };
    let r = if int.iter().any(|&b| b) {
        let mut first: Option<Point> = None;
        let mut sub = |cx: &mut NativeCx<'_, '_, 'h>, q: &mut dyn Problem<'h>| -> Res<Option<Point>> {
            // the root relaxation by the full solver, the nodes locally from its point
            match &first {
                None => {
                    let r = solve(cx, q)?;
                    first = r.clone();
                    Ok(r)
                }
                Some(f0) => optim::local(cx, q, &f0.x, set),
            }
        };
        optim::integer(cx, p, int, &mut sub)?
    } else {
        solve(cx, p)?
    };
    let no_ints = !int.iter().any(|&b| b);
    // the constraints judged against their own sizes (see `rescale`)
    let r = match r {
        Some(pt) if no_ints && !p.kinds().is_empty() => Some(rescale(cx, p, pt, &solve)?),
        other => other,
    };
    Ok(match r {
        Some(pt) => {
            // an unconstrained objective that keeps falling along a scaled ray from the optimum found has no minimum
            // (1/x towards 0-, x^3 - 3x outwards): the search box only hid it
            if p.kinds().is_empty() && no_ints && keeps_falling(cx, p, &pt)? {
                return Ok(BranchResult::Unbounded);
            }
            // a constrained one: a point that is not a verified optimum from which the local solver, started again
            // further out, finds feasible points ever lower and ever further away (x*y on x + y = 2)
            if !p.kinds().is_empty() && no_ints && !pt.polished && runs_away(cx, p, &pt, set)? {
                return Ok(BranchResult::Unbounded);
            }
            let proven = pt.polished;
            BranchResult::Point(pt, proven)
        }
        None => BranchResult::Infeasible,
    })
}

/// Does a constrained objective run away from `pt`? The local solver is started again from `pt` scaled by 10 and
/// by 100; two feasible results, each lower than the last by more than a millionth and each at least five times
/// further from the origin, show a descent with no end (a bounded problem returns to its optimum).
fn runs_away<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, pt: &Point, set: Settings) -> Res<bool> {
    let size = |x: &[f64]| x.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    if !pt.f.is_finite() || size(&pt.x) == 0.0 {
        return Ok(false);
    }
    let (mut prev_f, mut prev_x) = (pt.f, size(&pt.x));
    for s in [10.0f64, 100.0] {
        let x0: Vec<f64> = pt.x.iter().map(|v| v * s).collect();
        let Some(q) = optim::local(cx, p, &x0, set)? else { return Ok(false) };
        let feasible = q.viol <= optim::feas_tol(size(&q.x));
        if !(feasible && q.f < prev_f - 1e-6 * prev_f.abs().max(1.0) && size(&q.x) >= 5.0 * prev_x) {
            return Ok(false);
        }
        prev_f = q.f;
        prev_x = size(&q.x);
    }
    Ok(true)
}

/// The largest violation of a constraint at `x`, each judged against its own scale (`scales`: see
/// [`optim::constraint_scales`]); `None` when a value is not a finite real.
fn scaled_violation<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, scales: &[f64], x: &[f64]) -> Res<Option<(f64, f64)>> {
    let mut v = vec![0.0; p.kinds().len() + 1];
    if !p.eval(cx, x, &mut v)? {
        return Ok(None);
    }
    let mut worst = 0.0f64;
    for (i, k) in p.kinds().iter().enumerate() {
        let viol = match k {
            Kind::Eq => v[1 + i].abs(),
            Kind::Ge => (-v[1 + i]).max(0.0),
        };
        worst = worst.max(viol * scales[i]);
    }
    Ok(Some((v[0], worst)))
}

/// A point of a constrained problem checked against the size of each constraint: a feasible set smaller than the
/// absolute tolerance (`x^2 + y^2 <= 1e-10`) made the solver stop 4e-10 outside it, inside the tolerance. When some
/// constraint is small at the origin the problem is solved again with the constraints scaled to size one, and the
/// better point is kept, its violation the scaled one (so that one still outside is reported as such).
fn rescale<'h, F>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, mut pt: optim::Point, solve: &F) -> Res<optim::Point>
where
    F: Fn(&mut NativeCx<'_, '_, 'h>, &mut dyn Problem<'h>) -> Res<Option<optim::Point>>,
{
    let scales = optim::constraint_scales(cx, p)?;
    if scales.iter().all(|&s| s == 1.0) {
        return Ok(pt);
    }
    let Some((_, before)) = scaled_violation(cx, p, &scales, &pt.x)? else { return Ok(pt) };
    if before > optim::feas_tol(1.0) {
        let mut sp = optim::Scaled::new(p, scales.clone());
        if let Some(q) = solve(cx, &mut sp)? {
            if let Some((f, after)) = scaled_violation(cx, p, &scales, &q.x)? {
                if after < before {
                    pt = optim::Point { x: q.x, f, viol: after, polished: q.polished };
                    return Ok(pt);
                }
            }
        }
    }
    pt.viol = pt.viol.max(before);
    Ok(pt)
}

/// Does the objective, from the point `pt` found by the search, fall again and again along the rays `x s^k` (`s` =
/// 0.1 or 10, `k` = 1 to 4)? A local minimum does not (the points near it are higher, and the global search covered
/// the box), so a chain of strictly lower values, each by more than a millionth of the last, shows that the
/// infimum is not attained: the objective is unbounded below (or its infimum lies at a pole or at infinity).
fn keeps_falling<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, pt: &optim::Point) -> Res<bool> {
    if !pt.f.is_finite() || pt.x.iter().all(|&v| v == 0.0) {
        return Ok(false);
    }
    for s in [0.1f64, 10.0] {
        let mut prev = pt.f;
        let mut x = pt.x.clone();
        let mut falling = 0;
        for _ in 0..4 {
            for v in x.iter_mut() {
                *v *= s;
            }
            let mut out = [0.0];
            if !p.eval(cx, &x, &mut out)? || !out[0].is_finite() {
                // the next point overflows although the objective is already beyond 1e150 below zero: it falls on
                if prev < -1e150 {
                    return Ok(true);
                }
                break;
            }
            if out[0] < prev - 1e-6 * prev.abs().max(1.0) {
                falling += 1;
                prev = out[0];
            } else {
                break;
            }
        }
        if falling == 4 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `vector_greater_equal([a, b])` and its relatives: componentwise comparisons, `true` or `false` when every
/// component is numeric, unevaluated otherwise (a constraint for the optimisers).
pub(crate) struct VectorCompare {
    /// ≥ (`true`) or ≤
    pub ge: bool,
    /// strict
    pub strict: bool,
}

impl Native for VectorCompare {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let ar = Arith::new(h);
        let Some(&first) = args.first() else { return Ok(Reply::Decline) };
        if args.len() > 2 {
            return Ok(Reply::Decline);
        }
        // only the componentwise cone is decided here
        if let Some(&c) = args.get(1) {
            if !matches!(h.view(c), View::Str(s) if s == "NonNegativeCone") {
                return Ok(Reply::Decline);
            }
        }
        let Some(&[a, b]) = cx.as_list(first) else { return Ok(Reply::Decline) };
        let d = cx.eval(if self.ge { ar.sub(a, b) } else { ar.sub(b, a) })?;
        let items: Vec<T<'h>> = match cx.as_list(d) {
            Some(xs) => xs.to_vec(),
            None => vec![d],
        };
        let mut all = true;
        for &e in &items {
            let Some(v) = ar.num(e).and_then(|n| n.as_real().map(|r| r.to_f64())) else { return Ok(Reply::Decline) };
            all &= if self.strict { v > 0.0 } else { v >= 0.0 };
        }
        Ok(Reply::Done(cx.bool(all)))
    }
}

/// Is `t` a call of `options`, or a list (the constraint form)? Used by `find_minimum` to hand a call over.
pub(crate) fn wants_optimizer<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, s: T<'h>) -> bool {
    let h = cx.heap();
    if cx.as_list(f).is_some() {
        return true;
    }
    match h.view(s) {
        View::Sym(_) => true,
        View::App { head, .. } if head == h.sym_term(h.sym("element")) => true,
        _ => match cx.as_list(s) {
            // [x, y]: bare symbols; [[x, 1], y]: a mix
            Some(items) => {
                let syms = items.iter().filter(|&&i| matches!(h.view(i), View::Sym(_))).count();
                let first_sym = items.first().is_some_and(|&i| matches!(h.view(i), View::Sym(_)));
                let lists = items.iter().filter(|&&i| cx.as_list(i).is_some()).count();
                (syms >= 2 && first_sym) || (lists > 0 && syms > 0) || items.iter().any(|&i| matches!(h.view(i), View::App { head, .. } if head == h.sym_term(h.sym("element"))))
            }
            None => false,
        },
    }
}

/// `indexed(v, k)` or `indexed(v, [k])` of the vector variable `v` (components `comps`): component k.
fn indexed_component<'h>(h: &'h Heap, u: T<'h>, v: T<'h>, comps: &[T<'h>]) -> Option<T<'h>> {
    let View::App { head, args: &[x, k] } = h.view(u) else { return None };
    if head != h.sym_term(h.sym("indexed")) || x != v {
        return None;
    }
    let k = match h.view(k) {
        View::Int(k) => k,
        View::App { head: lh, args: &[k] } if lh == h.sym_term(h.sym(names::LIST)) => match h.view(k) {
            View::Int(k) => k,
            _ => return None,
        },
        _ => return None,
    };
    (k >= 1 && k as usize <= comps.len()).then(|| comps[k as usize - 1])
}
