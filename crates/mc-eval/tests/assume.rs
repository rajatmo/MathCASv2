//! Assumption contexts (semantics 13, `mc_eval::assume`): the context a native reads inside `assuming` (flattened,
//! sorted, without duplicates, nested contexts merged, restored afterwards), the memo never reusing a result under
//! another context (also through a rule and through memo hits) while sharing context-free results, and speculative
//! tasks inheriting the context (1 and 4 threads, lazy and always-split, under stress collection).
use mc_core::{Config, Heap, Store, T};
use mc_eval::pat::{call, var};
use mc_eval::{session, Attributes, Binds, EvalConfig, Kernel, Native, NativeCx, Reply, Res, Rhs, Split};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

/// `probe(x)`: `list(facts..., x)`, counting its calls; declares `uses_assumptions`.
struct Probe(Arc<AtomicUsize>);

impl Native for Probe {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        self.0.fetch_add(1, Relaxed);
        let mut items = cx.assumptions().to_vec();
        items.push(*x);
        Ok(Reply::Done(cx.list(&items)))
    }
}

/// `plain(x)`: `x`, counting its calls; reads no context.
struct Plain(Arc<AtomicUsize>);

impl Native for Plain {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        self.0.fetch_add(1, Relaxed);
        Ok(Reply::Done(*x))
    }
}

struct Counters {
    probe: Arc<AtomicUsize>,
    plain: Arc<AtomicUsize>,
}

/// Install `assuming`, `probe`, `plain` and the rule `g(y_) := probe(y)`.
fn install(h: &Heap, k: &mut Kernel) -> Counters {
    let c = Counters { probe: Arc::new(AtomicUsize::new(0)), plain: Arc::new(AtomicUsize::new(0)) };
    mc_eval::assume::install(k, h);
    let uses = Attributes { uses_assumptions: true, ..Attributes::default() };
    k.register(h.sym("probe"), uses, Box::new(Probe(c.probe.clone())));
    k.register(h.sym("plain"), Attributes::default(), Box::new(Plain(c.plain.clone())));
    let probe = h.sym("probe");
    let rhs: Rhs = Arc::new(move |b: &Binds<'_>, h: &Heap| h.call(probe, &[b.one(0)]));
    k.define(h, call(h, h.sym("g"), vec![var(0)]), rhs).expect("a rule");
    c
}

fn s<'h>(h: &'h Heap, name: &str) -> T<'h> {
    h.sym_term(h.sym(name))
}

fn list<'h>(h: &'h Heap, items: &[T<'h>]) -> T<'h> {
    h.call(h.sym("list"), items)
}

fn assuming<'h>(h: &'h Heap, facts: T<'h>, body: T<'h>) -> T<'h> {
    h.call(h.sym("assuming"), &[facts, body])
}

fn f<'h>(h: &'h Heap, head: &str, x: T<'h>) -> T<'h> {
    h.call(h.sym(head), &[x])
}

#[test]
fn natives_read_the_context_of_the_evaluation() {
    let mut ses = session(Config::default(), 1, EvalConfig::default());
    ses.step(|h, k| {
        install(h, k);
    });
    ses.step(|h, k| {
        let (p, q, r) = (s(h, "p"), s(h, "q"), s(h, "r"));
        let and = |a, b| h.call(h.sym("and"), &[a, b]);
        let probe0 = f(h, "probe", h.int(0));
        let cases = [
            // outside every context: no facts
            (probe0, list(h, &[h.int(0)])),
            // flattened (`and`, lists), sorted, without duplicates, `true` dropped
            (assuming(h, list(h, &[q, and(p, q), s(h, "true")]), probe0), list(h, &[p, q, h.int(0)])),
            // nested contexts merge; the inner one ends with its body
            (
                assuming(h, r, list(h, &[assuming(h, list(h, &[p, r]), probe0), probe0])),
                list(h, &[list(h, &[p, r, h.int(0)]), list(h, &[r, h.int(0)])]),
            ),
            // the body is held: evaluated only inside
            (assuming(h, p, f(h, "g", h.int(1))), list(h, &[p, h.int(1)])),
            // the wrong number of arguments leaves the call alone
            (h.call(h.sym("assuming"), &[p]), h.call(h.sym("assuming"), &[p])),
        ];
        for (t, want) in cases {
            assert_eq!(k.eval(h, t).value, want);
        }
    });
}

#[test]
fn a_result_is_never_reused_under_another_context() {
    let mut ses = session(Config::default(), 1, EvalConfig::default());
    let c = ses.step(install);
    let n = |c: &Arc<AtomicUsize>| c.load(Relaxed);
    ses.step(|h, k| {
        let (p, q) = (s(h, "p"), s(h, "q"));
        let probe1 = f(h, "probe", h.int(1));
        let mut ev = |t| k.eval(h, t).value;
        // the same context: the second evaluation is a memo hit
        assert_eq!(ev(assuming(h, p, probe1)), list(h, &[p, h.int(1)]));
        assert_eq!(ev(assuming(h, p, probe1)), list(h, &[p, h.int(1)]));
        assert_eq!(n(&c.probe), 1);
        // another context, then none: evaluated again each time, with its own facts
        assert_eq!(ev(assuming(h, q, probe1)), list(h, &[q, h.int(1)]));
        assert_eq!(ev(probe1), list(h, &[h.int(1)]));
        assert_eq!(ev(probe1), list(h, &[h.int(1)]));
        assert_eq!(n(&c.probe), 3);
        // probe(1) inside another term under p: the entry keyed by the context p answers
        assert_eq!(ev(assuming(h, p, list(h, &[probe1, h.int(9)]))), list(h, &[list(h, &[p, h.int(1)]), h.int(9)]));
        assert_eq!(n(&c.probe), 3);
        // through a rule: g(2) depends on the context because probe does
        let g2 = f(h, "g", h.int(2));
        assert_eq!(ev(g2), list(h, &[h.int(2)]));
        assert_eq!(ev(assuming(h, p, g2)), list(h, &[p, h.int(2)]));
        assert_eq!(ev(assuming(h, q, g2)), list(h, &[q, h.int(2)]));
        assert_eq!(ev(g2), list(h, &[h.int(2)]));
        assert_eq!(n(&c.probe), 6);
        // a context-free result is shared across contexts
        let plain5 = f(h, "plain", h.int(5));
        assert_eq!(ev(plain5), h.int(5));
        assert_eq!(ev(assuming(h, p, list(h, &[plain5]))), list(h, &[h.int(5)]));
        assert_eq!(ev(assuming(h, q, list(h, &[plain5]))), list(h, &[h.int(5)]));
        assert_eq!(n(&c.plain), 1);
    });
}

/// `assuming(p, map(probe, list(1, ..., n)))` and the same through `g`: every element sees the context.
fn mapped(h: &Heap, k: &mut Kernel, n: i64) {
    let (p, list_s) = (s(h, "p"), h.sym("list"));
    let xs: Vec<T<'_>> = (1..=n).map(|i| h.int(i)).collect();
    for head in ["probe", "g"] {
        let body = h.call(h.sym("map"), &[s(h, head), h.call(list_s, &xs)]);
        let v = k.eval(h, assuming(h, p, body)).value;
        let want: Vec<T<'_>> = xs.iter().map(|&x| list(h, &[p, x])).collect();
        assert_eq!(v, h.call(list_s, &want), "{head}");
    }
}

#[test]
fn speculative_tasks_inherit_the_context() {
    let n = 300;
    for (threads, split, cfg) in [
        (1, Split::Lazy, Config::default()),
        (4, Split::Lazy, Config::default()),
        (4, Split::Always, Config::default()),
        (4, Split::Always, Config::stress()),
    ] {
        let mut ses = session(cfg, threads, EvalConfig { split, ..EvalConfig::default() });
        ses.step(|h, k| {
            install(h, k);
        });
        ses.step(|h, k| mapped(h, k, n));
    }
}

