//! Parallel runs of natives (`NativeCx::par_until`, `par_map`, `par_first`; roadmap item 16A, semantics 18.1, 18.4):
//! the same values, effects, failures and fuel for any number of threads and split rule; later elements cancelled
//! once an earlier one stops the run; definitions made by an earlier element seen by every later one.
use mc_core::{Config, Heap, Session, Store, View, T};
use mc_eval::{session, Attributes, EvalConfig, Hold, Kernel, Native, NativeCx, Reply, Res, Split, Status};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn show(h: &Heap, t: T<'_>) -> String {
    match h.view(t) {
        View::Int(v) => v.to_string(),
        View::Sym(s) => h.sym_name(s),
        View::App { head, args } => format!("{}({})", show(h, head), args.iter().map(|&a| show(h, a)).collect::<Vec<_>>().join(", ")),
        v => format!("{v:?}"),
    }
}

/// `spin(n)`: `n` units of fuel, then `n`.
struct Spin;

impl Native for Spin {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [n] = args else { return Ok(Reply::Decline) };
        let View::Int(k) = cx.heap().view(*n) else { return Ok(Reply::Decline) };
        let mut left = k.max(0) as u64;
        while left > 0 {
            let s = left.min(100);
            cx.tick(s)?;
            left -= s;
        }
        Ok(Reply::Done(*n))
    }
}

/// `pmap(items...)`, `pfirst(items...)` (held): the items evaluated by `par_map` (a list of the values) or by
/// `par_first` (the first value other than `null`, with its position, or `null`). Counts the elements started.
struct Run {
    first: bool,
    started: Arc<AtomicUsize>,
}

impl Native for Run {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let items: Vec<T<'h>> = args.to_vec();
        let started = &self.started;
        if self.first {
            let null = cx.null();
            let r = cx.par_first(items.len(), &|cx, i| {
                started.fetch_add(1, Relaxed);
                let v = cx.eval(items[i])?;
                Ok((v != null).then_some(v))
            })?;
            Ok(Reply::Done(match r {
                Some((i, v)) => cx.list(&[h.int(i as i64), v]),
                None => null,
            }))
        } else {
            let vs = cx.par_map(items.len(), &|cx, i| {
                started.fetch_add(1, Relaxed);
                cx.eval(items[i])
            })?;
            Ok(Reply::Done(cx.list(&vs)))
        }
    }
}

/// The sequential reference and parallel configurations.
fn configs() -> Vec<(usize, EvalConfig, Config)> {
    let mut v = vec![(1, EvalConfig::reference(), Config::no_collect())];
    for (t, split) in [(1, Split::Lazy), (1, Split::Always), (2, Split::Always), (4, Split::Lazy), (8, Split::Lazy)] {
        v.push((t, EvalConfig { split, ..EvalConfig::default() }, Config::stress()));
    }
    v
}

fn make(threads: usize, cfg: EvalConfig, heap: Config) -> (Session<Kernel>, Arc<AtomicUsize>) {
    let mut s = session(heap, threads, cfg);
    let started = Arc::new(AtomicUsize::new(0));
    let (a, b) = (started.clone(), started.clone());
    s.step(move |h, k| {
        let held = Attributes { hold: Hold::All, ..Attributes::default() };
        k.register(h.sym("spin"), Attributes::default(), Box::new(Spin));
        k.register(h.sym("pmap"), held, Box::new(Run { first: false, started: a }));
        k.register(h.sym("pfirst"), held, Box::new(Run { first: true, started: b }));
    });
    (s, started)
}

/// `body` in every configuration: the same (value, status, fuel) everywhere.
fn same_everywhere(body: impl for<'h> Fn(&'h Heap, &mut Kernel) -> (String, Status, u64) + Sync) -> (String, Status, u64) {
    let mut first = None;
    for (threads, cfg, heap) in configs() {
        let (mut s, _) = make(threads, cfg, heap);
        let r = s.step(|h, k| body(h, k));
        match &first {
            None => first = Some(r),
            Some(f) => assert_eq!(&r, f, "{threads} threads, {cfg:?}"),
        }
    }
    first.expect("a configuration")
}

fn spin<'h>(h: &'h Heap, n: i64) -> T<'h> {
    h.call(h.sym("spin"), &[h.int(n)])
}

#[test]
fn maps_are_the_sequential_ones() {
    let _s = serial();
    let r = same_everywhere(|h, k| {
        let items: Vec<T> = (0..40).map(|i| h.call(h.sym("iadd"), &[spin(h, 2000 + 37 * i), h.int(i)])).collect();
        let e = k.eval(h, h.call(h.sym("pmap"), &items));
        (show(h, e.value), e.status, e.stats.fuel)
    });
    assert!(r.0.starts_with("list(2000, 2038, 2076"), "{}", r.0);
}

#[test]
fn the_first_answer_in_order_is_kept() {
    let _s = serial();
    // element 3 answers late (much work), element 5 early: element 3 must win
    let r = same_everywhere(|h, k| {
        let null = h.sym_term(h.sym("null"));
        let mut items: Vec<T> = (0..8).map(|i| h.call(h.sym("if"), &[h.call(h.sym("same"), &[spin(h, 500 * i), h.int(-1)]), h.int(0), null])).collect();
        items[3] = h.call(h.sym("iadd"), &[spin(h, 400_000), h.int(3)]);
        items[5] = h.int(5);
        let e = k.eval(h, h.call(h.sym("pfirst"), &items));
        (show(h, e.value), e.status, e.stats.fuel)
    });
    assert_eq!(r.0, "list(3, 400003)");
}

#[test]
fn later_elements_are_cancelled() {
    let _s = serial();
    for (threads, cfg, heap) in configs() {
        let (mut s, started) = make(threads, cfg, heap);
        let t0 = std::time::Instant::now();
        let (v, fuel) = s.step(|h, k| {
            // the first element answers after a little work; the others would spin for minutes
            let mut items = vec![h.call(h.sym("iadd"), &[spin(h, 20_000), h.int(1)])];
            items.extend((0..7).map(|_| spin(h, 1 << 40)));
            let e = k.eval(h, h.call(h.sym("pfirst"), &items));
            (show(h, e.value), e.stats.fuel)
        });
        assert_eq!(v, "list(0, 20001)", "{threads} threads {cfg:?}");
        assert!(fuel < 100_000, "fuel {fuel}: cancelled work is not charged");
        assert!(t0.elapsed().as_secs() < 30, "{threads} threads: cancelled tasks stop");
        assert!(started.load(Relaxed) >= 1);
    }
}

#[test]
fn definitions_by_earlier_elements_are_seen() {
    let _s = serial();
    let r = same_everywhere(|h, k| {
        let c = h.sym("c");
        k.set_own(c, h.int(0));
        let (define, iadd) = (h.sym("define"), h.sym("iadd"));
        // element 0 defines c after some work; elements 1.. read it (their speculative values are redone, and the
        // memo of the discarded work can make the redo cheaper: fuel is not compared here)
        let mut items = vec![h.call(define, &[h.sym_term(c), h.call(iadd, &[spin(h, 30_000), h.int(7)])])];
        items.extend((0..6).map(|i| h.call(iadd, &[h.sym_term(c), h.call(iadd, &[spin(h, 1000 + i), h.int(i)])])));
        let e = k.eval(h, h.call(h.sym("pmap"), &items));
        (show(h, e.value), e.status, 0)
    });
    assert_eq!(r.0, "list(null, 31007, 31009, 31011, 31013, 31015, 31017)");
}

#[test]
fn a_fuel_limit_fails_the_same_way_everywhere() {
    let _s = serial();
    let r = same_everywhere(|h, k| {
        let items: Vec<T> = (0..16).map(|i| spin(h, 10_000 + i)).collect();
        let e = k.eval_with(h, h.call(h.sym("pmap"), &items), 100_000);
        (show(h, e.value), e.status, 0)
    });
    assert_eq!((r.0.as_str(), r.1), ("failure(limit, fuel)", Status::Fuel));
    // and under a budget that suffices, the same fuel everywhere
    let r = same_everywhere(|h, k| {
        let items: Vec<T> = (0..16).map(|i| spin(h, 10_000 + i)).collect();
        let e = k.eval_with(h, h.call(h.sym("pmap"), &items), 1_000_000);
        (show(h, e.value), e.status, e.stats.fuel)
    });
    assert_eq!(r.1, Status::Running);
}

#[test]
fn nested_runs_cancel_with_their_parent() {
    let _s = serial();
    let r = same_everywhere(|h, k| {
        let inner: Vec<T> = (0..4).map(|_| spin(h, 1 << 40)).collect();
        let items = vec![h.int(1), h.call(h.sym("pmap"), &inner), h.call(h.sym("pmap"), &inner)];
        let e = k.eval(h, h.call(h.sym("pfirst"), &items));
        (show(h, e.value), e.status, e.stats.fuel)
    });
    assert_eq!(r.0, "list(0, 1)");
}
