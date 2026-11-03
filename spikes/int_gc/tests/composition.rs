//! Composition invariants of the generational collector under the concurrent store and the parallel evaluator.
//! Every workload, with collection at every safe point and the heap's invariants verified before and after each
//! collection, must give exactly the result of the sequential, non-collecting reference, at 1, 2 and 4 threads and
//! for both promotion orders.
use int_gc::eval::{Config as EvalConfig, Defs, Eval, Split};
use int_gc::heap::{Config, Heap, Order};
use int_gc::work::{self, Out, Session};
use int_gc::Roots;
use rayon::ThreadPoolBuilder;

type W = fn(&mut Session, EvalConfig) -> Out;

fn workloads() -> Vec<(&'static str, W)> {
    vec![
        ("M6a", |s, c| work::m6a(s, c, 100, 3)),
        ("M8", |s, _| work::m8(s, 20_000)),
        ("M10", |s, c| work::m10(s, c, 500, 2)),
        ("S3", |s, _| work::s3_steps(s, 10_000, 300)),
        ("S8", |s, _| work::s8(s, 20_000)),
        ("LS", |s, c| work::long_session_n(s, c, 200, 120)),
    ]
}

fn run(w: W, cfg: Config, split: Split, threads: usize) -> u64 {
    let pool = ThreadPoolBuilder::new().num_threads(threads).stack_size(64 << 20).build().unwrap();
    let mut s = Session::new(cfg, 1);
    pool.install(|| {
        let out = w(&mut s, EvalConfig::new(split));
        s.finish();
        out.check
    })
}

#[test]
fn stress_matches_reference_at_every_thread_count() {
    for (name, w) in workloads() {
        let reference = run(w, Config::none(), Split::Never, 1);
        for (order, par_gc) in [(Order::Topo, true), (Order::Chunk, true), (Order::Topo, false), (Order::Chunk, false)] {
            for threads in [1, 2, 4] {
                let mut cfg = Config { par_gc, ..Config::gen(order) };
                cfg.stress = true;
                cfg.instrument = true;
                for split in [Split::Lazy, Split::Always] {
                    let got = run(w, cfg, split, threads);
                    assert_eq!(got, reference, "{name}: {order:?}, parallel copy {par_gc}, {threads} threads, {split:?}");
                }
            }
        }
    }
}

#[test]
fn small_nursery_matches_reference() {
    // a nursery of one chunk: a minor collection at almost every safe point, majors by the normal trigger
    for (name, w) in workloads() {
        let reference = run(w, Config::none(), Split::Never, 1);
        for order in [Order::Topo, Order::Chunk] {
            let mut cfg = Config::gen(order);
            cfg.nursery = 1;
            assert_eq!(run(w, cfg, Split::Lazy, 4), reference, "{name}: {order:?}");
        }
    }
}

/// Arrays the lock-free young table retires when a shard grows are freed at the next safe point, and the young
/// table returns to its initial size.
#[test]
fn retired_table_arrays_are_freed_at_the_safe_point() {
    let pool = ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    let mut s = Session::new(Config::gen(Order::Topo), 1);
    let initial = s.heap.young_table_bytes();
    assert_eq!(initial.1, 0);
    let f = s.heap.sym("f");
    pool.install(|| {
        let Session { heap, keep, .. } = &mut s;
        {
            let h = &*heap;
            use rayon::prelude::*;
            let v: Vec<_> = (0..200_000i64).into_par_iter().map(|i| h.app(f.t(), &[h.int(i)])).collect();
            keep.push(h.app(f.t(), &v[..10]));
        }
        let (cur, retired) = heap.young_table_bytes();
        assert!(retired > 0 && cur > initial.0, "the table should have grown");
        heap.safe_point(&mut [keep], true);
        let freed = heap.log().last().unwrap().table_freed;
        assert!(freed >= retired, "retired arrays not freed");
        assert_eq!(heap.young_table_bytes(), initial);
    });
}

/// A memo entry whose key and value survive a minor collection survives it, renumbered (weak memo, decision 1b.5),
/// and a re-evaluation after the collection is a memo hit.
#[test]
fn memo_entries_follow_promotion() {
    for order in [Order::Topo, Order::Chunk] {
        let mut s = Session::new(Config::gen(order), 1);
        let (inc, wrap) = (s.heap.sym("inc"), s.heap.sym("wrap"));
        s.step(|h: &Heap, d: &Defs, keep: &mut Roots| {
            let mut t = h.int(1);
            for _ in 0..50 {
                t = h.app(wrap.t(), &[h.app(inc.t(), &[h.int(3)]), t]);
            }
            let v = Eval::new(h, d, EvalConfig::new(Split::Never)).run(t).0.unwrap();
            keep.push(t);
            keep.push(v);
        });
        // force the minor collection (the step's own safe point may find the nursery below its trigger)
        let Session { heap, defs, keep, .. } = &mut s;
        heap.safe_point(&mut [defs, keep], true);
        let h = &*heap;
        let (t, v) = (keep.get(h, 0), keep.get(h, 1));
        assert_eq!(h.memo_get(t).map(|e| e.0), Some(v), "{order:?}: memo entry lost by promotion");
    }
}

/// A node wider than 2^15 arguments is hashed in parallel chunks (decision 4.3); its hash, hence its id, must not
/// depend on how the chunks were combined. (Spike 4's combiner dropped the length of a combined right operand: the
/// same 200 000-wide list built 21 times in its store got 3 different ids.)
#[test]
fn wide_nodes_are_canonical_whatever_the_split() {
    let pool = ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    let mut s = Session::new(Config::gen(Order::Topo), 1);
    let list = s.heap.sym("list");
    pool.install(|| {
        s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
            let xs: Vec<_> = (0..200_000i64).map(|i| h.int(i)).collect();
            let seq = std::thread::scope(|sc| sc.spawn(|| h.app(list.t(), &xs)).join().unwrap());
            for _ in 0..20 {
                assert_eq!(h.app(list.t(), &xs), seq, "a wide node got a second id");
            }
            keep.push(seq);
        });
    });
}
