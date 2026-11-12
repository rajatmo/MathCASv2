//! `NativeCx::scratch` (decisions G.1 to G.3): an evaluation in a scope leaves no terms behind, effects inside one
//! are retried without it and happen once, cells written in it keep shared values, and results are the same with
//! scopes on or off, at one and four threads, under the stress collector.
use mc_core::{Config, Heap, Store, View, T};
use mc_eval::{session, Attributes, EvalConfig, Hold, Kernel, Native, NativeCx, Reply, Res};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

/// A structural rendering of a term (plain data, so it can leave a scope).
fn render(h: &Heap, t: T<'_>) -> String {
    let mut out = String::new();
    let mut stack: Vec<Result<T<'_>, &str>> = vec![Ok(t)];
    while let Some(x) = stack.pop() {
        match x {
            Err(s) => out.push_str(s),
            Ok(t) => match h.view(t) {
                View::App { head, args } => {
                    out.push_str(&render(h, head));
                    out.push('(');
                    stack.push(Err(")"));
                    for (i, &a) in args.iter().enumerate().rev() {
                        stack.push(Ok(a));
                        if i > 0 {
                            stack.push(Err(","));
                        }
                    }
                }
                View::Sym(s) => out.push_str(&h.sym_name(s)),
                v => out.push_str(&format!("{v:?}")),
            },
        }
    }
    out
}

/// `scr(body)`: evaluates `body` in a scratch scope and returns its rendering as a string; counts its attempts.
struct Scr(Arc<AtomicUsize>);

impl Native for Scr {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [body] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        let s = cx.scratch(|cx| {
            self.0.fetch_add(1, Relaxed);
            let v = cx.eval(*body)?;
            Ok(render(h, v))
        })?;
        Ok(Reply::Done(h.string(&s)))
    }
}

fn kernel_session(cfg: Config, threads: usize) -> (mc_core::Session<Kernel>, Arc<AtomicUsize>) {
    let mut s = session(cfg, threads, EvalConfig::default());
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    s.step(move |h, k| k.register(h.sym("scr"), Attributes { hold: Hold::All, ..Attributes::default() }, Box::new(Scr(c))));
    (s, n)
}

/// `f(iadd(i, 1), g(1.5 + i, iadd(i, i)), [iadd(i, 3), 2.5])`: three new compound terms when evaluated.
fn work<'h>(h: &'h Heap, i: i64) -> T<'h> {
    let s = |n: &str| h.sym_term(h.sym(n));
    let list = s("list");
    h.app(s("f"), &[h.app(s("iadd"), &[h.int(i), h.int(1)]), h.app(s("g"), &[h.real(1.5 + i as f64), h.app(s("iadd"), &[h.int(i), h.int(i)])]), h.app(list, &[h.app(s("iadd"), &[h.int(i), h.int(3)]), h.real(2.5)])])
}

#[test]
fn a_scope_leaves_no_terms_behind() {
    let mut stored = vec![];
    for scratch in [true, false] {
        let (mut s, n) = kernel_session(Config { scratch, ..Config::default() }, 1);
        let shown = s.step(|h, k| {
            let mut last = String::new();
            for i in 0..200 {
                let e = k.eval(h, h.call(h.sym("scr"), &[work(h, i)]));
                last = match h.view(e.value) {
                    View::Str(t) => t.to_string(),
                    _ => panic!("a string"),
                };
            }
            last
        });
        assert_eq!(shown, "f(Int(200),g(Real(200.5),Int(398)),list(Int(202),Real(2.5)))");
        assert_eq!(n.load(Relaxed), 200);
        stored.push(s.heap.stored());
    }
    // the evaluations' terms (calls, reals, lists, results) stay only without scopes
    assert!(stored[0] + 400 < stored[1], "stored with scopes {}, without {}", stored[0], stored[1]);
}

#[test]
fn effects_are_retried_without_the_scope() {
    let (mut s, n) = kernel_session(Config::default(), 1);
    let out = s.step(|h, k| {
        let p = h.call(h.sym("print"), &[h.call(h.sym("g"), &[h.real(0.25)])]);
        let e = k.eval(h, h.call(h.sym("scr"), &[p]));
        assert!(matches!(h.view(e.value), View::Str(_)));
        k.output(h).iter().map(|&t| render(h, t)).collect::<Vec<_>>()
    });
    // refused in the scope, then performed once without it
    assert_eq!(n.load(Relaxed), 2);
    assert_eq!(out, vec!["g(Real(0.25))".to_string()]);
    s.collect_full();
    s.verify();
}

#[test]
fn cells_written_in_a_scope_keep_shared_values() {
    for cfg in [Config::default(), Config::stress()] {
        let (mut s, _) = kernel_session(cfg, 2);
        let v = s.step(|h, k| {
            let sy = |n: &str| h.sym_term(h.sym(n));
            let c = sy("c");
            // block(var(c, 0), scr(assign(c, g(3.75, list(7)))), c)
            let value = h.call(h.sym("g"), &[h.real(3.75), h.call(h.sym("list"), &[h.int(7)])]);
            let body = h.app(
                sy("block"),
                &[
                    h.app(sy("var"), &[c, h.int(0)]),
                    h.app(sy("scr"), &[h.app(sy("assign"), &[c, value])]),
                    c,
                ],
            );
            let e = k.eval(h, body);
            assert!(!h.is_scratch(e.value));
            render(h, e.value)
        });
        assert_eq!(v, "g(Real(3.75),list(Int(7)))");
        s.collect_full();
        s.verify();
    }
}

#[test]
fn same_results_with_and_without_scopes() {
    let run = |cfg: Config, threads: usize| {
        let (mut s, _) = kernel_session(cfg, threads);
        let r = s.step(|h, k| {
            // a map whose elements each open a scope (parallel tasks), and nested scopes
            let sy = |n: &str| h.sym_term(h.sym(n));
            let x = sy("x");
            let elems: Vec<T<'_>> = (0..64).map(|i| h.int(i)).collect();
            let inner = h.app(sy("scr"), &[h.app(sy("scr"), &[h.app(sy("g"), &[x, h.app(sy("imul"), &[x, x])])])]);
            let f = h.app(sy("fn"), &[h.app(sy("list"), &[x]), inner]);
            let e = k.eval(h, h.app(sy("map"), &[f, h.app(sy("list"), &elems)]));
            render(h, e.value)
        });
        s.collect_full();
        s.verify();
        r
    };
    let reference = run(Config { scratch: false, ..Config::default() }, 1);
    assert!(reference.contains("g(Int(63),Int(3969))"), "{reference}");
    for (cfg, threads) in [(Config::default(), 1), (Config::default(), 4), (Config::stress(), 4)] {
        assert_eq!(run(cfg, threads), reference);
    }
}
