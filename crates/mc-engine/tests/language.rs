//! The language features of `mc-eval` (package H) through the engine: closures, blocks, locals, loops, `match`,
//! `rewrite`, definitions with options, defaults, held parameters and decorators, the output channel and the memo
//! setting; the same results under collection stress at 1 and 4 threads; and the language parts of benchmark cases
//! R2, R9 and R12 with small library stubs (`mod`, `range`, `max`, `nest`, `depth`) standing in for the library
//! packages.
use mc_bench::{Engine, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::{Attributes, EvalConfig, Kernel, Native, NativeCx, Reply, Res, Split};

/// Library stand-ins on machine integers: `mod(x, m)` for `m > 0`, `max(list)`.
struct Mod;
struct Max;

struct Nest;
struct Range;
struct Depth;

fn int(h: &Heap, t: T<'_>) -> Option<i64> {
    match h.view(t) {
        View::Int(v) => Some(v),
        _ => None,
    }
}

impl Native for Mod {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[x, m] = args else { return Ok(Reply::Decline) };
        Ok(match int(h, x).zip(int(h, m)) {
            Some((x, m)) if m > 0 => Reply::Done(h.int(x.rem_euclid(m))),
            _ => Reply::Decline,
        })
    }
}

impl Native for Max {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[xs] = args else { return Ok(Reply::Decline) };
        let Some(xs) = cx.as_list(xs) else { return Ok(Reply::Decline) };
        cx.tick(xs.len() as u64)?;
        let m = xs.iter().map(|&x| int(h, x)).collect::<Option<Vec<i64>>>().and_then(|v| v.into_iter().max());
        Ok(m.map_or(Reply::Decline, |m| Reply::Done(h.int(m))))
    }
}

impl Native for Nest {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[f, x, n] = args else { return Ok(Reply::Decline) };
        let Some(n) = int(h, n) else { return Ok(Reply::Decline) };
        let mut t = x;
        for _ in 0..n {
            cx.tick(1)?;
            t = h.app(f, &[t]);
        }
        Ok(Reply::Done(t))
    }
}

impl Native for Range {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[n] = args else { return Ok(Reply::Decline) };
        let Some(n) = int(h, n) else { return Ok(Reply::Decline) };
        cx.tick(n.max(0) as u64)?;
        let xs: Vec<T> = (1..=n).map(|i| h.int(i)).collect();
        Ok(Reply::Done(cx.list(&xs)))
    }
}

impl Native for Depth {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[t] = args else { return Ok(Reply::Decline) };
        // the deepest path through arguments, iteratively
        let (mut best, mut stack) = (0i64, vec![(t, 1i64)]);
        while let Some((u, d)) = stack.pop() {
            cx.tick(1)?;
            best = best.max(d);
            if let View::App { args, .. } = h.view(u) {
                stack.extend(args.iter().map(|&a| (a, d + 1)));
            }
        }
        Ok(Reply::Done(h.int(best)))
    }
}

fn install_stubs(k: &mut Kernel, h: &Heap) {
    let strict = Attributes::default();
    k.register(h.sym("mod"), strict, Box::new(Mod));
    k.register(h.sym("max"), strict, Box::new(Max));
    k.register(h.sym("nest"), strict, Box::new(Nest));
    k.register(h.sym("range"), strict, Box::new(Range));
    k.register(h.sym("depth"), strict, Box::new(Depth));
}

fn engine(cfg: EngineConfig) -> CoreEngine {
    let mut e = CoreEngine::with_config(cfg);
    e.install(install_stubs);
    e
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) if v.output.is_empty() => v.printed,
        Ok(v) => format!("{} | {}", v.output.join(" "), v.printed),
        Err(err) => format!("error: {err}"),
    }
}

/// Inputs and their printed results (output first, after `|`), evaluated in order in one session.
const SESSION: &[(&str, &str)] = &[
    // closures and application
    ("(x => x + 1)(2)", "3"),
    ("f = (x, y) => x*y\nf(3, 4)", "12"),
    ("&(&1 + &2)(5, 6)", "11"),
    ("(() => 7)()", "7"),
    ("((x, ys...) => [x, ys])(1, 2, 3)", "[1, [2, 3]]"),
    ("((x, y = x + 1) => [x, y])(1)", "[1, 2]"),
    ("((x: Int) => x)(1/2)", "failure(no_match, (x: Int) => x, [1/2])"),
    ("((x) => x)(1, 2)", "failure(no_match, x => x, [1, 2])"),
    // a held argument is not evaluated unless the body uses it
    ("((held e, x) => x)(print(1), print(2))", "2 | 2"),
    ("((held e) => [e, e])(print(1))", "1 1 | [1, 1]"),
    ("map(x => x^2, [1, 2, 3])", "[1, 4, 9]"),
    // lexical scope and hygiene
    ("add_y = y => (x => x + y)\nadd_y(x)(1)", "1 + x"),
    ("rule mk(y_) := (x => x + y)\nmk(x)(1)", "1 + x"),
    ("{ let x = 1; let g = y => x + y; let x = 10; g(0) + x }", "11"),
    ("{ let a = y; let g = (y => a + y); g(1) }", "1 + y"),
    // blocks, mutable locals, loops
    ("{ let a = 2; let b = a + 1; a*b }", "6"),
    ("{ let k: Int = 2.5; k }", "failure(type_mismatch, k, 2.5)"),
    ("{ var i = 0; var s = 0; while i < 10 { i = i + 1; s = s + i }; s }", "55"),
    ("{ var a = 1; let g = () => a; a = 5; g() }", "5"),
    ("make_counter = () => { var c = 0; () => { c = c + 1 } }\nctr = make_counter()\nctr()\nctr()", "2"),
    ("{ var s = 0; for i in range(100) { s = s + i }; s }", "5050"),
    ("{ var s = 0; for i in range(10, 1, -3) { s = s + i }; s }", "22"),
    ("for x in [1, 2, 3] { print(x*10) }", "10 20 30 | null"),
    ("print(\"hello\"); 5", "hello | 5"),
    ("while 1 { 2 }", "failure(not_boolean, 1)"),
    ("{ 1/0; 2 }", "failure(division_by_zero)"),
    ("top = 1\nvar w = 2\nw = w + top\nw", "3"),
    ("if 1 > 2 then 3", "null"),
    // match
    ("match 3 { 0 => zero, n_ when n > 2 => big, _ => small }", "big"),
    ("match [1, 2, 3] { [h_, t___] => t }", "[2, 3]"),
    ("match q { 0 => zero }", "failure(no_match, q)"),
    // rewrite
    ("rewrite(g(a, h(a)), a -> b)", "g(b, h(b))"),
    ("rewrite(h(h(h(x))), h(y_) :> y)", "h(h(x))"),
    ("rewrite(h(h(h(x))), h(y_) :> y, strategy: fixed_point)", "x"),
    ("rewrite(d(d(d(1))), d(n_) :> n + 1, strategy: innermost)", "4"),
    ("rewrite(p(1), [p(0) -> zero, p(n_) :> n])", "1"),
    ("rewrite(e, a -> b, strategy: sideways)", "failure(bad_option, strategy, sideways)"),
    // definitions
    ("fn gd(x, n: Int = 10) = x + n\n[gd(1), gd(1, 2)]", "[11, 3]"),
    ("gd(1, \"a\")", "failure(no_match, gd, [1, \"a\"])"),
    ("fn sc(x; scale: Int = 3) = x*scale\n[sc(2), sc(2, scale: 5)]", "[6, 10]"),
    ("sc(2, bogus: 1)", "failure(unknown_option, bogus)"),
    ("fn hf(held e, x) = x\nhf(print(1), print(2))", "2 | 2"),
    ("fn fact(n: Int) = if n == 0 then 1 else n*fact(n - 1)\nfact(20)", "2432902008176640000"),
    ("fact(x)", "failure(no_match, fact, [x])"),
    ("rule open(0) = zero\nopen(1)", "open(1)"),
    ("@listable fn sq(x) = x^2\nsq([1, 2, 3])", "[1, 4, 9]"),
    ("sq([1, 2], [3])", "failure(shape_mismatch)"),
    ("@associative @commutative @identity(0) sym oplus\n[oplus(a, 0, oplus(b, 0)), oplus(a, 0), oplus(0, 0)]", "[oplus(a, b), a, 0]"),
    ("@on(square) rule area(square(a_)) := a^2\narea(square(3))", "9"),
    ("fn deep(n: Int) = if n == 0 then 0 else deep(n - 1)\ndeep(30000)", "0"),
];

#[test]
fn language_session() {
    let mut e = engine(EngineConfig::default());
    for (src, want) in SESSION {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
}

#[test]
fn language_results_are_the_same_under_collection_stress_and_at_1_and_4_threads() {
    let reference: Vec<String> = {
        let mut e = engine(EngineConfig { threads: 1, ..EngineConfig::default() });
        SESSION.iter().map(|(src, _)| eval(&mut e, src)).collect()
    };
    for (threads, split) in [(1, Split::Lazy), (4, Split::Lazy), (4, Split::Always)] {
        for memo in ["on", "off"] {
            let d = EngineConfig::default();
            let cfg = EvalConfig { split, ..d.eval };
            let mut e = engine(EngineConfig { threads, heap: mc_core::Config::stress(), eval: cfg });
            e.configure("memo", memo).expect("memo is a setting");
            let got: Vec<String> = SESSION.iter().map(|(src, _)| eval(&mut e, src)).collect();
            assert_eq!(got, reference, "{threads} threads, {split:?}, under stress, memo {memo}");
        }
    }
}

/// Cells used from many speculative tasks: reads of a frontier cell, private cells per element, a closure escaping
/// from each element.
#[test]
fn cells_under_speculation_are_deterministic() {
    let src = "fn steps(k: Int) = { var c = k; var s = 0; while c != 1 { c = if mod(c, 2) == 0 then c/2 else 3c + 1; s = s + 1 }; s }
{ var base = 100; let xs = map(k => steps(k) + base, range(300)); base = 0; [max(xs), map(k => base + k, [1, 2])] }
map(k => { var c = k; () => c }, [1, 2, 3])";
    let reference = eval(&mut engine(EngineConfig { threads: 1, ..EngineConfig::default() }), src);
    assert_eq!(reference, "[() => local_cell(1), () => local_cell(2), () => local_cell(3)]");
    // lazy splitting, and splitting every group (maximal speculation)
    for split in [Split::Lazy, Split::Always, Split::Always] {
        let d = EngineConfig::default();
        let cfg = EngineConfig { threads: 4, heap: mc_core::Config::stress(), eval: EvalConfig { split, ..d.eval } };
        let mut e = engine(cfg);
        assert_eq!(eval(&mut e, src), reference);
        assert_eq!(eval(&mut e, "{ var base = 100; let xs = map(k => steps(k) + base, range(300)); base = 0; [max(xs), map(k => base + k, [1, 2])] }"), "[227, [1, 2]]");
    }
}

/// R2 (memoising Fibonacci, with a block so the rule returns the value), R9 (deep nesting through a `let`) and R12
/// (interpreted loops) at their smallest sizes.
#[test]
fn benchmark_cases_r2_r9_r12() {
    let mut e = engine(EngineConfig::default());
    let r2 = "rule fib(0) = 0\nrule fib(1) = 1\nrule fib(k_: Int) := { let v = fib(k - 1) + fib(k - 2); fib(k) = v; v }\n\
              result = fib(m)\nresult^2 - fib(m - 1)*fib(m + 1) == (-1)^(m - 1)";
    // the smallest size, 10^3, recurses deeper than the debug build's depth limit (decision G.7)
    let m = if cfg!(debug_assertions) { 200 } else { 1000 };
    assert_eq!(eval(&mut e, &format!("m = {m}\n{r2}")), "true");
    assert_eq!(eval(&mut e, "m = 10^4\n{ let t = nest(f, x, m); depth(t) }"), "10001");
    let r12 = "fn collatz_steps(k: Int) = {\n  var c = k\n  var s = 0\n  while c != 1 {\n    c = if mod(c, 2) == 0 then c/2 else 3c + 1\n    s = s + 1\n  }\n  s\n}\n\
               m = 10^4\nrange(m) |> map(collatz_steps, _) |> max";
    assert_eq!(eval(&mut e, r12), "261");
}
