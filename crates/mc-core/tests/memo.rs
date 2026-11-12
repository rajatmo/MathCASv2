//! The memo (A.1, A.8): dense per region, moved with its keys at minor and major collections, weak in its values,
//! the side memo for old keys with young values, stamps preserved, and concurrent use.
use mc_core::{Config, Heap, Raw, Roots, Session, Store, T};
use rayon::prelude::*;

fn session() -> Session<Roots> {
    Session::new(Config::default(), 4, Roots::new())
}

#[test]
fn young_entries_move_with_their_keys() {
    let mut s = session();
    let f = s.heap.sym("f");
    s.step(|h: &Heap, keep: &mut Roots| {
        let (k, v) = (h.call(f, &[h.int(1)]), h.call(f, &[h.int(2)]));
        h.memo_put(k, v, 7, 0xAB);
        let hit = h.memo_get(k).unwrap();
        assert_eq!((hit.value, hit.epoch, hit.deps), (v, 7, 0xAB));
        keep.push(k);
        keep.push(v);
        // a young key with an immediate value, and one whose value dies
        let k2 = h.call(f, &[h.int(3)]);
        h.memo_put(k2, h.int(9), 1, 0);
        keep.push(k2);
        let k3 = h.call(f, &[h.int(4)]);
        h.memo_put(k3, h.call(f, &[h.int(5)]), 1, 0);
        keep.push(k3);
    });
    s.collect_full();
    s.read(|h, keep| {
        let hit = h.memo_get(keep.get(h, 0)).expect("entry promoted with its key");
        assert_eq!((hit.value, hit.epoch, hit.deps), (keep.get(h, 1), 7, 0xAB));
        assert_eq!(h.memo_get(keep.get(h, 2)).unwrap().value, h.int(9));
        assert!(h.memo_get(keep.get(h, 3)).is_none(), "the memo is weak in its values");
        assert!(h.memo_get(h.int(3)).is_none(), "immediates are not memoised");
    });
}

#[test]
fn old_keys_with_young_values_survive_in_the_side_memo() {
    let mut s = session();
    let f = s.heap.sym("f");
    s.step(|h: &Heap, keep: &mut Roots| {
        keep.push(h.call(f, &[h.int(1)]));
        keep.push(h.call(f, &[h.int(10)]));
    });
    s.collect_full();
    s.step(|h: &Heap, keep: &mut Roots| {
        let (k, other) = (keep.get(h, 0), keep.get(h, 1));
        // an old key with an old value, then with a young value: the young one wins
        h.memo_put(k, other, 1, 0);
        let v = h.call(f, &[h.int(2)]);
        h.memo_put(k, v, 2, 5);
        assert_eq!(h.memo_get(k).unwrap().value, v);
        keep.push(v);
        // a young value that dies
        h.memo_put(other, h.call(f, &[h.int(3)]), 2, 0);
    });
    s.collect_full();
    s.read(|h, keep| {
        let hit = h.memo_get(keep.get(h, 0)).expect("side entry moved to the old memo");
        assert_eq!((hit.value, hit.epoch, hit.deps), (keep.get(h, 2), 2, 5));
        assert!(h.memo_get(keep.get(h, 1)).is_none());
    });
    // the keys die: after a major collection nothing is left
    s.state.clear();
    s.collect_full();
    assert_eq!(s.heap.old_len(), 0);
}

#[test]
fn old_memo_slides_with_the_nodes() {
    let mut s = session();
    let f = s.heap.sym("f");
    s.step(|h: &Heap, keep: &mut Roots| {
        for i in 0..1000 {
            keep.push(h.call(f, &[h.int(i)]));
        }
    });
    s.collect_full();
    s.step(|h: &Heap, keep: &mut Roots| {
        for i in 0..999 {
            h.memo_put(keep.get(h, i), keep.get(h, i + 1), i as u32, 0);
        }
    });
    // drop every fourth root: the major slide renumbers the rest and must carry their entries
    let idx: Vec<usize> = (0..1000).filter(|i| i % 4 != 1).collect();
    let mut keep: Vec<Raw> = s.read(|h, k| idx.iter().map(|&i| Raw::new(k.get(h, i))).collect());
    s.heap.collect_full(&mut [&mut keep]);
    s.heap.verify(&mut [&mut keep]);
    let h = &s.heap;
    for (p, &i) in idx.iter().enumerate() {
        let hit = h.memo_get(keep[p].get(h));
        if i == 999 {
            assert!(hit.is_none());
        } else if (i + 1) % 4 == 1 {
            assert!(hit.is_none(), "entry {i} has a dead value");
        } else {
            let hit = hit.unwrap_or_else(|| panic!("entry {i} lost in the slide"));
            let want: T = keep[p + 1].get(h);
            assert_eq!((hit.value, hit.epoch), (want, i as u32));
        }
    }
}

#[test]
fn concurrent_memo_never_returns_a_wrong_pair() {
    let mut s = session();
    let (f, g) = (s.heap.sym("f"), s.heap.sym("g"));
    s.step(|h: &Heap, keep: &mut Roots| {
        for i in 0..512 {
            keep.push(h.call(f, &[h.int(i)]));
        }
    });
    s.collect_full();
    for round in 0..3 {
        s.step(|h: &Heap, keep: &mut Roots| {
            (0..200_000u32).into_par_iter().for_each(|x| {
                let i = (x.wrapping_mul(2_654_435_761) >> 7) as usize % 512;
                let k = keep.get(h, i);
                // young values for half the keys (side memo), old ones for the rest
                let v = if i.is_multiple_of(2) { h.call(g, &[h.int(i as i64)]) } else { keep.get(h, 511 - i) };
                if x % 3 == 0 {
                    h.memo_put(k, v, round, i as u64);
                } else if let Some(hit) = h.memo_get(k) {
                    assert_eq!(hit.value, v, "a memo hit is the value written for its key");
                    assert_eq!(hit.deps, i as u64);
                }
            });
        });
    }
}
