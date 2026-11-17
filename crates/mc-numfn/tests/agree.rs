//! The compiled and interpreted backends agree (item N0's exit criterion): random expressions in scalars and a
//! vector (constant indices, list results, constants, conditionals, blocks), random closures, evaluated at random
//! points (signed zeros, extremes, values outside the real domains) on both backends give bitwise the same outputs
//! whenever the compiled code completes, and the same verdict (finite or not); declines are counted.
use mc_api::{Engine, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::CoreEngine;
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{function_parts, Arg, Compiled, Interpreted, NumFn, Prep, Var};

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
}

const POINTS: &[f64] = &[0.0, -0.0, 1.0, -1.0, 0.5, -2.5, 3.0, 0.1, 1e300, -1e-300, 7.25, -0.75, 1e-8, 40.0, 2.0, 10.650000000000002, 5e-324, -5e-324];

fn vars_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<Vec<Var>> {
    let h = cx.heap();
    let items = cx.as_list(spec)?;
    let mut out = vec![];
    for &i in items {
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

/// `numfn_check(expr, vars, seed, n)` (held): `[done, declined, mismatches, refusal]` over n random points; for
/// a closure, `vars` is the list of argument kinds (`1` a scalar, `[n]` a vector).
struct Check;

fn kinds<'h>(cx: &NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<Vec<Arg>> {
    let h = cx.heap();
    cx.as_list(spec)?
        .iter()
        .map(|&k| match h.view(k) {
            View::Int(1) => Some(Arg::Scalar),
            _ => match cx.as_list(k)? {
                [n] => match h.view(*n) {
                    View::Int(n) => Some(Arg::Vector(n as usize)),
                    _ => None,
                },
                _ => None,
            },
        })
        .collect()
}

impl Native for Check {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (expr, spec, seed, n, outputs) = match args {
            [a, b, c, d] => (a, b, c, d, None),
            [a, b, c, d, o] => (a, b, c, d, Some(*o)),
            _ => return Ok(Reply::Decline),
        };
        let (View::Int(seed), View::Int(n)) = (h.view(*seed), h.view(*n)) else { return Ok(Reply::Decline) };
        let closure = matches!(h.view(*expr), View::App { head, .. } if head == h.sym_term(h.sym("fn")))
            || (matches!(h.view(*expr), View::Sym(_)) && kinds(cx, *spec).is_some());
        let (vars, body) = if closure {
            let f = cx.eval(*expr)?;
            let Some(k) = kinds(cx, *spec) else { return Ok(Reply::Decline) };
            let Some(p) = function_parts(h, f, &k) else { return Ok(Reply::Decline) };
            p
        } else {
            let Some(v) = vars_of(cx, *spec) else { return Ok(Reply::Decline) };
            (v, *expr)
        };
        let mut interp = Interpreted::new(cx, body, &vars, Prep::Machine)?;
        if let Some(View::Int(o)) = outputs.map(|o| h.view(o)) {
            interp.f.set_outputs(o as usize);
        }
        let compiled = Compiled::new(cx, &vars, interp.body(), interp.numeric());
        let c = match compiled {
            Ok(c) => c,
            Err(e) => return Ok(Reply::Done(cx.list(&[h.int(0), h.int(0), h.int(0), h.string(&e)]))),
        };
        let (ni, no) = (NumFn::<f64>::inputs(&interp), NumFn::<f64>::outputs(&interp));
        if (c.inputs(), c.outputs()) != (ni, no) {
            // a list built inside a block: the closure's outputs are counted from its literal shape (`Auto` refuses)
            return Ok(Reply::Done(cx.list(&[h.int(0), h.int(0), h.int(0), h.string("shapes differ")])));
        }
        let mut r = Rng(seed as u64 | 1);
        let (mut done, mut declined, mut bad) = (0i64, 0i64, vec![]);
        for _ in 0..n {
            let x: Vec<f64> = (0..ni).map(|_| POINTS[r.below(POINTS.len() as u64) as usize]).collect();
            let (mut oi, mut oc) = (vec![0.0; no], vec![0.0; no]);
            let fi = interp.eval(cx, &x, &mut oi)?;
            match c.try_eval(cx, &x, &mut oc)? {
                None => declined += 1,
                Some(fc) => {
                    done += 1;
                    let same = fi == fc && (!fi || oi.iter().zip(&oc).all(|(a, b)| a.to_bits() == b.to_bits()));
                    if !same && bad.len() < 3 {
                        bad.push(format!("at {x:?}: interpreted {fi} {oi:?}, compiled {fc} {oc:?}"));
                    }
                }
            }
        }
        let msg = h.string(&bad.join("; "));
        Ok(Reply::Done(cx.list(&[h.int(done), h.int(declined), h.int(bad.len() as i64), msg, h.call(h.sym("hold"), &[interp.body()])])))
    }
}

fn engine() -> CoreEngine {
    let mut e = CoreEngine::new();
    e.install(|k: &mut Kernel, h: &Heap| {
        let held = Attributes { hold: Hold::All, ..Attributes::default() };
        k.register(h.sym("numfn_check"), held, Box::new(Check));
    });
    e
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

const ATOMS: &[&str] = &["x", "z", "y[1]", "y[2]", "y[3]", "y[-1]", "2", "0.5", "3.25", "-1.5", "pi", "e", "1/3", "length(y)"];
const UNARY: &[&str] = &["sin", "cos", "exp", "sqrt", "log", "abs", "atan", "tanh", "asin"];
const BINARY: &[&str] = &["+", "-", "*", "/", "^"];

fn expr(r: &mut Rng, d: u32) -> String {
    if d == 0 || r.below(5) == 0 {
        return r.pick(ATOMS).to_string();
    }
    match r.below(10) {
        0..=3 => {
            let op = r.pick(BINARY);
            let rhs = if op == "^" { r.pick(&["2", "3", "-1", "0.5", "x", "1/3", "-2"]).to_string() } else { expr(r, d - 1) };
            format!("({} {op} {rhs})", expr(r, d - 1))
        }
        4 | 5 => format!("{}({})", r.pick(UNARY), expr(r, d - 1)),
        6 => format!("{}({}, {})", r.pick(&["min", "max"]), expr(r, d - 1), expr(r, d - 1)),
        7 => format!("(if {} < {} then {} else {})", expr(r, d - 1), expr(r, d - 1), expr(r, d - 1), expr(r, d - 1)),
        8 => format!("{{ let u = {}; u*u - {} }}", expr(r, d - 1), expr(r, d - 1)),
        _ => format!("({} {} {} {})", expr(r, d - 1), r.pick(&["+", "*"]), expr(r, d - 1), r.pick(&["+ x", "* z", "- y[2]"])),
    }
}

fn result(r: &mut Rng) -> String {
    match r.below(3) {
        0 => expr(r, 4),
        1 => format!("[{}, {}]", expr(r, 3), expr(r, 3)),
        _ => format!("[[{}, {}], [{}, x]]", expr(r, 2), expr(r, 2), expr(r, 2)),
    }
}

fn parse(s: &str) -> [i64; 3] {
    let inner = s.trim_start_matches('[');
    let mut it = inner.split(", ").take(3).map(|v| v.parse::<i64>().unwrap_or_else(|_| panic!("{s}")));
    [it.next().unwrap(), it.next().unwrap(), it.next().unwrap()]
}

#[test]
fn random_expressions_agree_on_both_backends() {
    let mut e = engine();
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut done, mut declined, mut refused) = (0, 0, 0);
    // AGREE_N=5000 for a longer search
    let n: usize = std::env::var("AGREE_N").ok().and_then(|v| v.parse().ok()).unwrap_or(250);
    for k in 0..n {
        let src = result(&mut r);
        let out = eval(&mut e, &format!("numfn_check({src}, [x, [y, 3], z], {}, 24)", k + 1));
        let [d, dc, bad] = parse(&out);
        assert_eq!(bad, 0, "{src}: {out}");
        if d == 0 && dc == 0 {
            refused += 1;
        }
        done += d;
        declined += dc;
    }
    // most expressions compile; both outcomes of a compiled call are well represented
    assert!(refused < n / 5, "{refused} refused");
    assert!(done > 2000 && declined > 200, "{done} done, {declined} declined, {refused} refused");
}

/// Roots and their powers at signed zeros, infinities and the other points: `sqrt` of a machine real is the
/// correctly rounded root with `+0.0` at both zeros, `x^(1/2)` the machine power, and a power of a root is the
/// root raised (never folded into `x^(3/2)`).
#[test]
fn roots_powers_of_roots_and_signed_zeros_agree() {
    let mut e = engine();
    let srcs = [
        "sqrt(x)",
        "sqrt(y[2]) ^ 3",
        "sqrt(y[1]) ^ 2 - y[1]",
        "sqrt(x) ^ -1",
        "sqrt(x) ^ 0.5",
        "x ^ (1/2)",
        "x ^ (3/2) - sqrt(x) ^ 3",
        "sqrt(x) * sqrt(z)",
        "sqrt(x * z)",
        "sin(y[1]) * sqrt(y[2]) ^ 3",
        "0.0 * x",
        "-1.5 * sqrt(z)",
        "sqrt(0.5 * 3.25) + sqrt(z)",
        "1 / (1 + sqrt(abs(x)))",
        "sqrt(x - z)",
        "[sqrt(x), sqrt(-x), sqrt(y[3]) ^ 2]",
    ];
    for (k, src) in srcs.iter().enumerate() {
        let out = eval(&mut e, &format!("numfn_check({src}, [x, [y, 3], z], {}, 400)", k + 1));
        let [d, _, bad] = parse(&out);
        assert_eq!(bad, 0, "{src}: {out}");
        assert!(d > 0, "{src}: {out}");
    }
}

#[test]
fn random_closures_agree_on_both_backends() {
    let mut e = engine();
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    let (mut done, mut refused) = (0, 0);
    for k in 0..120 {
        let body = result(&mut r);
        // the same body as a closure of (x, y, z), of (x, y) with z bound by `let`, and with typed parameters
        let src = match k % 3 {
            0 => format!("(x, y, z) => {body}"),
            1 => format!("(x, y) => {{ let z = 0.25*x; {body} }}"),
            _ => format!("(x: Real, y, z: Real) => {body}"),
        };
        let kinds = if k % 3 == 1 { "[1, [3]]" } else { "[1, [3], 1]" };
        let out = eval(&mut e, &format!("numfn_check({src}, {kinds}, {}, 24)", k + 1));
        let [d, _, bad] = parse(&out);
        assert_eq!(bad, 0, "{src}: {out}");
        if d == 0 {
            refused += 1;
        }
        done += d;
    }
    assert!(refused < 30 && done > 1000, "{done} done, {refused} refused");
}

#[test]
fn named_functions_inline_into_expressions() {
    let mut e = engine();
    assert_eq!(eval(&mut e, "fn g(a: Real, b: Real) = a*b - sin(b)\nfn rhs(t: Real, u: Real, v: Real) = [v, -u + 0.1*g(t, v)]"), "null");
    for (src, vars) in [("g(x, z) + y[1]", "[x, [y, 3], z], 7, 40"), ("rhs(x, y[1], y[2])", "[x, [y, 2]], 7, 40, 2")] {
        let out = eval(&mut e, &format!("numfn_check({src}, {vars})"));
        let [d, _, bad] = parse(&out);
        assert!(bad == 0 && d > 20, "{src}: {out}");
    }
    // a function name as the function value: the body is a call the compiler inlines
    let out = eval(&mut e, "numfn_check(g, [1, 1], 3, 40)");
    assert!(out.starts_with("[40, 0, 0"), "{out}");
}

#[test]
fn refusals_leave_the_interpreter() {
    let mut e = engine();
    for (src, why) in [
        ("besselj(0, x)", "the call of `besselj`"),
        // `n` makes the index 4.0, which the interpreter refuses too
        ("y[4]", "an index that is not a constant integer"),
        ("y + 1.0", "a list where a number is expected"),
        ("x < z", "a boolean result"),
    ] {
        let out = eval(&mut e, &format!("numfn_check({src}, [x, [y, 3], z], 1, 4)"));
        assert!(out.starts_with("[0, 0, 0, \"") && out.contains(why), "{src}: {out}");
    }
}


/// `map(x => body, xs)` over a long packed vector and `array(k => body, n)` compile the body once (`mc-lists`): the result is the term the
/// interpreter's element-by-element values make (`map_indexed`, never compiled), bit for bit (the printed texts
/// show signed zeros), at signed zeros, the smallest and largest machine numbers, and for integers; a body the
/// compiler declines at some element (a complex root, a quotient that is not an integer) is left to the interpreter.
#[test]
fn compiled_maps_agree_with_the_interpreter() {
    let started = std::time::Instant::now();
    let mut e = engine();
    eval(&mut e, "xs = join(range(-3, 3, 0.001), [0.0, -0.0, 1.0e308, -1.0e308, 5.0e-324, -5.0e-324, 2.2250738585072014e-308]);");
    eval(&mut e, "ks = range(-3000, 3000);");
    for (body, over) in [
        ("x^2 - 3*x + 1", "xs"),
        ("sqrt(abs(x))", "xs"),
        ("-x", "xs"),
        ("x * 0.0", "xs"),
        ("0.0 * x - x", "xs"),
        ("sin(x) * exp(-x^2)", "xs"),
        ("atan(x) + tanh(x)", "xs"),
        ("sqrt(abs(x)) ^ 3", "xs"),
        ("x / 3", "xs"),
        ("sqrt(x)", "xs"),
        ("k^2 - 7*k", "ks"),
        ("k / 2", "ks"),
        ("k^7", "ks"),
        ("2*k + 0.5", "ks"),
    ] {
        let v = if over == "ks" { "k" } else { "x" };
        let compiled = format!("map({v} => {body}, {over})");
        let interpreted = format!("map_indexed(({v}, at) => {body}, {over})");
        assert_eq!(eval(&mut e, &format!("({compiled}) === ({interpreted})")), "true", "{compiled}");
        assert_eq!(eval(&mut e, &format!("to_string({compiled}) == to_string({interpreted})")), "true", "{compiled}");
    }
    // a list with complex values (declined at x < 0)
    assert_eq!(eval(&mut e, "length(map(x => sqrt(x), xs))"), "6008");
    // array(k => body, n) and array(k => body, n, origin) over the indices, the same way
    for body in ["k^2 - 7*k", "k / 2", "sqrt(k) - 0.5", "sin(k / 1000.0) * k"] {
        for (compiled, indices) in [("array(k => {b}, 6000)", "range(6000)"), ("array(k => {b}, 5000, -2500)", "range(-2500, 2499)")] {
            let compiled = compiled.replace("{b}", body);
            let interpreted = format!("map_indexed((k, at) => {body}, {indices})");
            assert_eq!(eval(&mut e, &format!("to_string({compiled}) == to_string({interpreted})")), "true", "{compiled}");
            assert_eq!(eval(&mut e, &format!("({compiled}) === ({interpreted})")), "true", "{compiled}");
        }
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "took {:?}", started.elapsed());
}

#[test]
fn compiled_nests_and_folds_agree_with_the_interpreter() {
    let started = std::time::Instant::now();
    let mut e = engine();
    eval(&mut e, "xs = join(range(-3, 3, 0.001), [0.0, -0.0, 1.0e308, -1.0e308, 5.0e-324, -5.0e-324, 2.2250738585072014e-308]);");
    eval(&mut e, "ks = range(-3000, 3000);");
    // nest and nest_list: the compiled lambda against the same body as a named function (never compiled)
    for (body, start, n) in [
        ("3.7*x*(1 - x)", "0.2", 5000),
        // signed zeros: -0.0 stays -0.0 under the halving, 0.0 under the negated product
        ("x/2", "-0.0", 2000),
        ("-(x*0.0)", "1.5", 2000),
        // overflow to infinity and a NaN after it
        ("x*2.0", "1.0", 1500),
        ("x*2.0 - x*2.0", "1.0e307", 1500),
        // the subnormals down to zero
        ("x/2.0", "1.0e-300", 1200),
        ("x - 1.0e-310", "1.0e-307", 1100),
        // machine integers, and an overflow (the compiled code declines, the interpreter goes on exactly)
        ("mod(31*x + 7, 1000003)", "5", 5000),
        ("2*x", "1", 1100),
        // a real start with an integer-valued body: the types differ (declined)
        ("floor(x)", "2.5", 1100),
    ] {
        eval(&mut e, &format!("clear(g); fn g(x) = {body}"));
        for f in ["nest_list", "nest"] {
            let compiled = format!("{f}(x => {body}, {start}, {n})");
            let interpreted = format!("{f}(g, {start}, {n})");
            assert_eq!(eval(&mut e, &format!("to_string({compiled}) == to_string({interpreted})")), "true", "{compiled}");
            assert_eq!(eval(&mut e, &format!("({compiled}) === ({interpreted})")), "true", "{compiled}");
        }
    }
    // fold and fold_list, with and without a start
    for (body, start, over) in [
        ("a + x^2", Some("0.0"), "xs"),
        ("a*0.999 + x", Some("-0.0"), "xs"),
        ("max(a, x) - 1.0e-3", None, "xs"),
        ("a*2.0 + x", Some("1.0"), "xs"),
        ("mod(a*31 + x, 1000003)", Some("7"), "ks"),
        ("a*x", Some("1"), "ks"),
        ("a + x", None, "ks"),
        // an integer accumulator over reals: the types differ (declined)
        ("a + x", Some("0"), "xs"),
    ] {
        eval(&mut e, &format!("clear(g); fn g(a, x) = {body}"));
        let st = start.map(|s| format!("{s}, ")).unwrap_or_default();
        for f in ["fold_list", "fold"] {
            let compiled = format!("{f}((a, x) => {body}, {st}{over})");
            let interpreted = format!("{f}(g, {st}{over})");
            assert_eq!(eval(&mut e, &format!("to_string({compiled}) == to_string({interpreted})")), "true", "{compiled}");
            assert_eq!(eval(&mut e, &format!("({compiled}) === ({interpreted})")), "true", "{compiled}");
        }
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "took {:?}", started.elapsed());
}

#[test]
fn compiled_nests_and_folds_are_fast() {
    let mut e = engine();
    let t0 = std::time::Instant::now();
    assert_eq!(eval(&mut e, "length(nest_list(x => 3.7*x*(1 - x), 0.2, 200000))"), "200001");
    assert_eq!(eval(&mut e, "is_real(fold((a, x) => a + x^2, 0., range(0, 1, 0.000005)))"), "true");
    // the interpreter took about 2 s and 1.2 s
    assert!(t0.elapsed() < std::time::Duration::from_secs(1), "took {:?}", t0.elapsed());
}
