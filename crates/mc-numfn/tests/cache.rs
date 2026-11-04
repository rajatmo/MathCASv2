//! The process-wide code cache (package N1): bodies differing in their machine-real constants share one
//! compilation; cached code with lifted constants gives bitwise the results of the instance's own compiled code
//! and of the interpreter; the family policy compiles after the family's evaluations and runs compiled from the
//! first call on a hit; scoped entries follow redefinitions of inlined functions; kernels do not share code that
//! inlined their own functions.
//!
//! The cache is process-wide and the tests of this file run in parallel, so every test uses expression shapes of
//! its own.
use mc_bench::{Engine, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::CoreEngine;
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{auto_expr, Backend, Compiled, Interpreted, NumFn, Options, Policy, Prep, Var};

fn vars_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<Vec<Var>> {
    let h = cx.heap();
    let mut out = vec![];
    for &i in cx.as_list(spec)? {
        match h.view(i) {
            View::Sym(s) => out.push(Var::scalar(s)),
            _ => {
                let [y, n] = cx.as_list(i)? else { return None };
                let (View::Sym(s), View::Int(n)) = (h.view(*y), h.view(*n)) else { return None };
                out.push(Var::vector(s, n as usize));
            }
        }
    }
    Some(out)
}

fn reals<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<f64>> {
    let h = cx.heap();
    cx.as_list(t)?
        .iter()
        .map(|&v| match h.view(v) {
            View::Real(x) => Some(x),
            View::Int(k) => Some(k as f64),
            _ => None,
        })
        .collect()
}

fn policy(s: &str) -> Option<Policy> {
    Some(match s {
        "compiled" => Policy::Compiled,
        "interpreted" => Policy::Interpreted,
        s if s.starts_with("auto:") => Policy::Auto { after: s[5..].parse().ok()? },
        s if s.starts_with("family:") => Policy::Family { after: s[7..].parse().ok()? },
        _ => return None,
    })
}

/// `numfn_family(expr, vars, points, "policy")` (held; the preparation computes the constants):
/// `[ok, backend, compiled, declined, interpreted, hits, misses, lifted]` after one `eval_flat` over the points.
struct Family;

impl Native for Family {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [expr, spec, points, pol] = args else { return Ok(Reply::Decline) };
        let (Some(vars), Some(x), View::Str(p)) = (vars_of(cx, *spec), reals(cx, *points), h.view(*pol)) else {
            return Ok(Reply::Decline);
        };
        let Some(policy) = policy(p) else { return Ok(Reply::Decline) };
        let mut f = auto_expr(cx, *expr, &vars, &Options { policy, ..Options::default() })?;
        let mut out = vec![0.0; f.outputs() * (x.len() / f.inputs().max(1))];
        let ok = f.eval_flat(cx, &x, &mut out)?;
        let s = f.stats();
        let lifted = f.compiled().map_or(-1, |c| c.lifted().len() as i64);
        let backend = h.string(if f.backend() == Backend::Compiled { "compiled" } else { "interpreted" });
        let n = |k: u64| h.int(k as i64);
        Ok(Reply::Done(cx.list(&[
            cx.bool(ok),
            backend,
            n(s.compiled),
            n(s.declined),
            n(s.interpreted),
            n(s.cache_hits),
            n(s.cache_misses),
            h.int(lifted),
        ])))
    }
}

/// `numfn_agree(expr, vars, points)` (held): the interpreter, the instance's own compiled code and the cached
/// code with lifted constants at every point: `[done, declined, mismatches, lifted, message]`; `[-1, ...]` when
/// the cached code is refused although the instance's own compiles (or the reverse).
struct Agree;

impl Native for Agree {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [expr, spec, points] = args else { return Ok(Reply::Decline) };
        let points = cx.eval(*points)?;
        let (Some(vars), Some(x)) = (vars_of(cx, *spec), reals(cx, points)) else { return Ok(Reply::Decline) };
        let mut interp = Interpreted::new(cx, *expr, &vars, Prep::Machine)?;
        let (body, numeric) = (interp.body(), interp.numeric());
        let own = Compiled::new(cx, &vars, body, numeric);
        let cached = Compiled::cached(cx, &vars, body, numeric);
        let (own, cached) = match (own, cached) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(_), Err(e)) => return Ok(Reply::Done(cx.list(&[h.int(0), h.int(0), h.int(0), h.int(0), h.string(&e)]))),
            (a, b) => {
                let m = format!("own {:?}, cached {:?}", a.err(), b.err());
                return Ok(Reply::Done(cx.list(&[h.int(-1), h.int(0), h.int(0), h.int(0), h.string(&m)])));
            }
        };
        let (ni, no) = (NumFn::<f64>::inputs(&interp), NumFn::<f64>::outputs(&interp));
        let (mut done, mut declined, mut bad) = (0i64, 0i64, vec![]);
        for p in x.chunks(ni.max(1)) {
            let (mut oi, mut oo, mut oc) = (vec![0.0; no], vec![0.0; no], vec![0.0; no]);
            let fi = interp.eval(cx, p, &mut oi)?;
            let fo = own.try_eval(cx, p, &mut oo)?;
            let fc = cached.try_eval(cx, p, &mut oc)?;
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            match (fo, fc) {
                (None, None) => declined += 1,
                (Some(a), Some(b)) => {
                    done += 1;
                    let same = a == b && fi == a && (!a || (bits(&oo) == bits(&oc) && bits(&oi) == bits(&oc)));
                    if !same && bad.len() < 3 {
                        bad.push(format!("at {p:?}: interpreted {fi} {oi:?}, own {a} {oo:?}, cached {b} {oc:?}"));
                    }
                }
                // declines must coincide too: the lifted code is the same function of its parameters
                (a, b) => bad.push(format!("at {p:?}: own {a:?}, cached {b:?}")),
            }
        }
        let n = |k: i64| h.int(k);
        let lifted = cached.lifted().len() as i64;
        Ok(Reply::Done(cx.list(&[n(done), n(declined), n(bad.len() as i64), n(lifted), h.string(&bad.join("; "))])))
    }
}

fn engine() -> CoreEngine {
    let mut e = CoreEngine::new();
    e.install(|k: &mut Kernel, h: &Heap| {
        let held = Attributes { hold: Hold::All, ..Attributes::default() };
        k.register(h.sym("numfn_family"), held, Box::new(Family));
        k.register(h.sym("numfn_agree"), held, Box::new(Agree));
    });
    e
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The first `k` integers of a printed list.
fn ints(s: &str, k: usize) -> Vec<i64> {
    s.trim_start_matches('[').split(", ").filter_map(|v| v.parse().ok()).take(k).collect()
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
    fn constant(&mut self) -> String {
        let c = self.pick(&["0.5", "-1.5", "3.25", "0.1", "-0.0", "1e-300", "2.0", "-3.0", "1e300", "0.3"]);
        c.to_string()
    }
}

/// A random expression in `x`, `z` and a vector `y` of 3, with machine-real constants in every position the
/// compiler reads: sums, products, powers (real and integer exponents), elementary functions, comparisons,
/// `min`/`max`, blocks.
fn expr(r: &mut Rng, d: u32) -> String {
    if d == 0 || r.below(4) == 0 {
        return match r.below(3) {
            0 => r.constant(),
            _ => r.pick(&["x", "z", "y[1]", "y[2]", "y[-1]", "pi", "1/3"]).to_string(),
        };
    }
    match r.below(10) {
        0..=3 => {
            let op = r.pick(&["+", "-", "*", "/", "^"]);
            let rhs = if op == "^" { r.pick(&["2", "3", "-1", "0.5", "2.0", "1.5", "-2.0", "x"]).to_string() } else { expr(r, d - 1) };
            format!("({} {op} {rhs})", expr(r, d - 1))
        }
        4 | 5 => format!("{}({})", r.pick(&["sin", "cos", "exp", "sqrt", "log", "abs", "atan", "tanh"]), expr(r, d - 1)),
        6 => format!("{}({}, {})", r.pick(&["min", "max"]), expr(r, d - 1), r.constant()),
        7 => format!("(if {} < {} then {} else {})", expr(r, d - 1), r.constant(), expr(r, d - 1), expr(r, d - 1)),
        8 => format!("{{ let u = {}; u*{} - {} }}", expr(r, d - 1), r.constant(), expr(r, d - 1)),
        _ => format!("({} + {}*x + {})", expr(r, d - 1), r.constant(), r.constant()),
    }
}

const POINTS: &[f64] = &[0.0, -0.0, 1.0, -1.0, 0.5, 2.5, -3.75, 1e300, -1e300, 1e-300, 7.0, 0.1];

#[test]
fn lifted_constants_give_bitwise_the_uncached_results() {
    let mut e = engine();
    let mut r = Rng(0x5851_f42d_4c95_7f2d);
    let (mut done, mut declined, mut refused, mut lifted) = (0, 0, 0, 0);
    let n: usize = std::env::var("AGREE_N").ok().and_then(|v| v.parse().ok()).unwrap_or(250);
    for _ in 0..n {
        let src = match r.below(3) {
            0 => expr(&mut r, 4),
            1 => format!("[{}, {}]", expr(&mut r, 3), expr(&mut r, 3)),
            _ => format!("[[{}, {}], [{}, x]]", expr(&mut r, 2), expr(&mut r, 2), expr(&mut r, 2)),
        };
        let pts: Vec<String> = (0..24 * 5).map(|_| format!("{:?}", POINTS[r.below(POINTS.len() as u64) as usize])).collect();
        let out = eval(&mut e, &format!("numfn_agree({src}, [x, [y, 3], z], [{}])", pts.join(", ")));
        let v = ints(&out, 4);
        assert!(v.len() == 4 && v[0] >= 0 && v[2] == 0, "{src}: {out}");
        if v[0] == 0 && v[1] == 0 {
            refused += 1;
        }
        done += v[0];
        declined += v[1];
        lifted += v[3];
    }
    assert!(refused < n / 5, "{refused} refused");
    assert!(done > 2000 && declined > 100 && lifted > n as i64, "{done} done, {declined} declined, {lifted} lifted");
}

#[test]
fn a_family_compiles_once() {
    let mut e = engine();
    // ten instances of one family (constants differ), five evaluations each (one batch): the family passes 32
    // evaluations in the seventh instance, which compiles; the later ones hit the cache and run compiled from the
    // start
    let mut seen = vec![];
    for k in 1..=10 {
        let src = format!("numfn_family(x^2 - {k}.5*cosh(x)/7.0 + 0.{k}, [x], [0.1, 0.2, 0.3, 0.4, 0.5], \"family:32\")");
        seen.push(eval(&mut e, &src));
    }
    for (k, s) in seen.iter().enumerate() {
        let v: Vec<&str> = s.trim_matches(|c| c == '[' || c == ']').split(", ").collect();
        assert_eq!(v[0], "true", "{s}");
        if k < 6 {
            assert_eq!(&v[1..], ["\"interpreted\"", "0", "0", "5", "0", "1", "-1"], "instance {k}: {s}");
        } else if k == 6 {
            // its batch takes the family past 32 evaluations: compiled before the batch runs
            assert_eq!(&v[1..], ["\"compiled\"", "5", "0", "0", "0", "1", "2"], "instance {k}: {s}");
        } else {
            assert_eq!(&v[1..], ["\"compiled\"", "5", "0", "0", "1", "0", "2"], "instance {k}: {s}");
        }
    }
    // the same body in other variable names is the same family; `auto:N` counts its own evaluations only and then
    // takes the family's code
    let s = eval(&mut e, "numfn_family(w^2 - 3.5*cosh(w)/7.0 + 0.25, [w], [0.1, 0.2], \"family:32\")");
    assert_eq!(s, "[true, \"compiled\", 2, 0, 0, 1, 0, 2]");
    let s = eval(&mut e, "numfn_family(w^2 - 3.5*cosh(w)/7.0 + 0.25, [w], [0.1, 0.2], \"auto:3\")");
    assert_eq!(s, "[true, \"interpreted\", 0, 0, 2, 1, 0, -1]");
    let s = eval(&mut e, "numfn_family(w^2 - 3.5*cosh(w)/7.0 + 0.25, [w], [0.1, 0.2, 0.3, 0.4], \"auto:3\")");
    assert_eq!(s, "[true, \"compiled\", 4, 0, 0, 1, 0, 2]");
    // a different shape (a constant became an integer exponent) is another family
    let s = eval(&mut e, "numfn_family(w^3 - 3.5*cosh(w)/7.0 + 0.25, [w], [0.1], \"family:32\")");
    assert_eq!(s, "[true, \"interpreted\", 0, 0, 1, 0, 1, -1]");
    // the interpreted policy does not look at the cache
    let s = eval(&mut e, "numfn_family(w^2 - 3.5*cosh(w)/7.0 + 0.25, [w], [0.1], \"interpreted\")");
    assert_eq!(s, "[true, \"interpreted\", 0, 0, 1, 0, 0, -1]");
}

#[test]
fn compiled_code_serves_every_kernel_unless_it_inlined_their_functions() {
    let (mut a, mut b) = (engine(), engine());
    let s = eval(&mut a, "numfn_family(tanh(x)*1.25 - x/3.0, [x], [0.5], \"compiled\")");
    assert_eq!(s, "[true, \"compiled\", 1, 0, 0, 0, 1, 2]");
    // the code inlines nothing: another kernel takes it
    let s = eval(&mut b, "numfn_family(tanh(x)*2.25 - x/5.0, [x], [0.5], \"compiled\")");
    assert_eq!(s, "[true, \"compiled\", 1, 0, 0, 1, 0, 2]");
    // the same body calling a function each kernel defines its own way: compiled per kernel, each its own result
    assert_eq!(eval(&mut a, "fn kq(u: Real) = u*u"), "null");
    assert_eq!(eval(&mut b, "fn kq(u: Real) = u + 100.0"), "null");
    let sa = eval(&mut a, "numfn_family(kq(x) + 0.5, [x], [3.0], \"compiled\")");
    let sb = eval(&mut b, "numfn_family(kq(x) + 1.5, [x], [3.0], \"compiled\")");
    assert_eq!(sa, "[true, \"compiled\", 1, 0, 0, 0, 1, 1]");
    assert_eq!(sb, "[true, \"compiled\", 1, 0, 0, 0, 1, 1]");
    let sa = eval(&mut a, "numfn_family(kq(x) + 2.5, [x], [3.0], \"compiled\")");
    assert_eq!(sa, "[true, \"compiled\", 1, 0, 0, 1, 0, 1]");
    // redefining the inlined function invalidates the kernel's entry: the next instance compiles afresh
    assert_eq!(eval(&mut a, "fn kq(u: Real) = u*u*u"), "null");
    let sa = eval(&mut a, "numfn_family(kq(x) + 3.5, [x], [3.0], \"compiled\")");
    assert_eq!(sa, "[true, \"compiled\", 1, 0, 0, 0, 1, 1]");
}

#[test]
fn values_follow_the_instance_and_redefinitions() {
    let mut e = engine();
    // the values of cached code are the instance's constants, not the first instance's
    assert_eq!(eval(&mut e, "fn kv(u: Real) = 2.0*u"), "null");
    let r1 = eval(&mut e, "numfn_agree(kv(x) - 0.75*y[1] + z, [x, [y, 1], z], [1.0, 2.0, 3.0])");
    let r2 = eval(&mut e, "numfn_agree(kv(x) - 1.75*y[1] + z, [x, [y, 1], z], [1.0, 2.0, 3.0])");
    assert!(r1.starts_with("[1, 0, 0, 1") && r2.starts_with("[1, 0, 0, 1"), "{r1} {r2}");
    // after a redefinition the cached code is recompiled, and agrees with the new definition
    assert_eq!(eval(&mut e, "fn kv(u: Real) = 3.0*u - 1.0"), "null");
    let r3 = eval(&mut e, "numfn_agree(kv(x) - 2.75*y[1] + z, [x, [y, 1], z], [1.0, 2.0, 3.0])");
    assert!(r3.starts_with("[1, 0, 0, 1"), "{r3}");
    // refusals are cached too, and retried once a symbol of the body is defined
    let s = eval(&mut e, "numfn_family(kw(x) + 0.5, [x], [1.0], \"compiled\")");
    assert_eq!(s, "[false, \"interpreted\", 0, 0, 1, 0, 1, -1]");
    let s = eval(&mut e, "numfn_family(kw(x) + 0.75, [x], [1.0], \"compiled\")");
    assert_eq!(s, "[false, \"interpreted\", 0, 0, 1, 1, 0, -1]");
    assert_eq!(eval(&mut e, "fn kw(u: Real) = u - 1.0"), "null");
    let s = eval(&mut e, "numfn_family(kw(x) + 0.625, [x], [1.0], \"compiled\")");
    assert_eq!(s, "[true, \"compiled\", 1, 0, 0, 0, 1, 1]");
}
