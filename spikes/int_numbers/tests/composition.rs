//! Composition invariants: every workload gives the same value through every store configuration as on the bare
//! numbers, under stress collection (a minor collection at every safe point, a major one every fourth, the store's
//! invariants verified after each: tables, creation order, contiguous arena payloads, boxed byte accounting,
//! canonical numbers).
use int_numbers::store::{Config, Gc, Hashing, Heap, Placement};
use int_numbers::work::{self, Big, Churn, Keep};

fn configs() -> Vec<Config> {
    let gen = Gc::Gen { nursery: 1 << 20 };
    let mk = |name, place, hash, gc| Config { name, place, hash, gc, stress: true, count_boxed: true };
    vec![
        mk("arena", Placement::Arena, Hashing::Eager, gen),
        mk("boxed", Placement::Boxed, Hashing::Eager, gen),
        mk("hybrid", Placement::Hybrid(8), Hashing::Eager, gen),
        mk("sampled", Placement::Hybrid(8), Hashing::Sampled, gen),
        mk("lazy", Placement::Hybrid(8), Hashing::Lazy(8), gen),
        mk("nogc", Placement::Hybrid(8), Hashing::Eager, Gc::None),
    ]
}

fn agree(name: &str, bare: u64, store: impl Fn(&mut Heap) -> work::Out) {
    let mut fp2 = None;
    for cfg in configs() {
        let mut h = Heap::new(cfg);
        let o = store(&mut h);
        assert_eq!(o.check, bare, "{name}: {} differs from the bare numbers", cfg.name);
        if let Some(n) = o.notes.iter().find(|n| n.0 == "fp2") {
            let prev = fp2.get_or_insert_with(|| n.1.clone());
            assert_eq!(*prev, n.1, "{name}: {} terms differ", cfg.name);
        }
        h.verify();
    }
}

#[test]
fn exact_workloads_agree() {
    agree("N5 tree", work::n5_tree_bare(600).check, |h| work::n5_tree_store(h, 600));
    agree("N5 seq", work::n5_seq_bare(400).check, |h| work::n5_seq_store(h, 400));
    agree("N10 split", work::n10_split_bare(300).check, |h| work::n10_split_store(h, 300));
    agree("N10 seq", work::n10_seq_bare(300).check, |h| work::n10_seq_store(h, 300));
    agree("EXP plain", work::expand_bare(30).check, |h| work::expand_plain_store(h, 30));
    agree("EXP packed", work::expand_bare(30).check, |h| work::expand_packed_store(h, 30));
}

#[test]
fn ball_workloads_agree() {
    agree("ZETA", work::zeta_bare(200, 400).check, |h| work::zeta_store(h, 200, 400));
    agree("PI/E", work::consts_bare(300).check, |h| work::consts_store(h, 300));
}

#[test]
fn sessions_agree() {
    let c = Churn { evals: 20, lo_digits: 1e2, hi_digits: 3e3 };
    agree("LS churn", work::churn_bare(c).check, |h| work::churn_store(h, c));
    let k = Keep { evals: 30, big_digits: 2e3, terms: 100 };
    agree("LS keep", work::keep_bare(k).check, |h| work::keep_store(h, k));
    let b = Big { evals: 30, limbs: 300, ring: 4, terms: 20 };
    agree("LS big", work::big_bare(b).check, |h| work::big_store(h, b));
    agree("HASH", work::hash_hazard_bare(40, 100).check, |h| work::hash_hazard_store(h, 40, 100));
}
