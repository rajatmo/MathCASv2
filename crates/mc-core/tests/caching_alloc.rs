//! The caching allocator as the global allocator of a test binary: blocks of every size class and beyond, freed on
//! other threads than they were made on, grown and shrunk across classes, keep their contents; sessions of the store
//! run on it.
use mc_core::alloc::{CachingAlloc, MAX_SMALL};
use mc_core::{Config, Heap, Store};

#[global_allocator]
static ALLOC: CachingAlloc = CachingAlloc;

/// A vector of `n` bytes filled with a pattern of `seed`.
fn filled(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

fn intact(v: &[u8], seed: u8) -> bool {
    v.iter().enumerate().all(|(i, &b)| b == (i as u8).wrapping_mul(31).wrapping_add(seed))
}

#[test]
fn blocks_keep_their_contents_across_threads() {
    let sizes: Vec<usize> = (1..=MAX_SMALL + 64).step_by(7).chain([16, 256, 257, 1024, 1025, 4096, 70_000]).collect();
    let (tx, rx) = std::sync::mpsc::channel::<Vec<Vec<u8>>>();
    // made on four threads, freed on this one
    let makers: Vec<_> = (0..4u8)
        .map(|t| {
            let (tx, sizes) = (tx.clone(), sizes.clone());
            std::thread::spawn(move || {
                for round in 0..40u8 {
                    let batch: Vec<Vec<u8>> = sizes.iter().map(|&n| filled(n, t ^ round)).collect();
                    for (v, &n) in batch.iter().zip(&sizes) {
                        assert_eq!(v.len(), n);
                        assert!(intact(v, t ^ round));
                    }
                    tx.send(batch).expect("sent");
                }
            })
        })
        .collect();
    drop(tx);
    let mut got = 0;
    for batch in rx {
        got += batch.len();
        drop(batch);
        // reuse what was freed at once
        let again: Vec<Vec<u8>> = sizes.iter().map(|&n| filled(n, 7)).collect();
        assert!(again.iter().all(|v| intact(v, 7)));
    }
    for m in makers {
        m.join().expect("joins");
    }
    assert_eq!(got, 4 * 40 * sizes.len());
}

#[test]
fn growing_and_shrinking_keeps_the_prefix() {
    for start in [1usize, 15, 16, 17, 200, 256, 300, 1000, 1024, 2000] {
        let mut v = filled(start, 3);
        // grow one byte at a time through the classes and past them
        while v.len() < 3000 {
            let n = v.len();
            v.push((n as u8).wrapping_mul(31).wrapping_add(3));
        }
        assert!(intact(&v, 3), "{start}");
        v.truncate(5);
        v.shrink_to_fit();
        assert!(intact(&v, 3), "{start}");
        let mut s = String::with_capacity(start);
        for k in 0..500 {
            s.push_str(&k.to_string());
        }
        assert!(s.starts_with("0123456789101112"));
    }
}

#[test]
fn zeroed_blocks_are_zero_after_reuse() {
    for _ in 0..100 {
        let dirty: Vec<Vec<u8>> = (1..=64).map(|k| vec![0xAB; k * 16]).collect();
        drop(dirty);
        let clean: Vec<Vec<u8>> = (1..=64).map(|k| vec![0; k * 16]).collect();
        assert!(clean.iter().all(|v| v.iter().all(|&b| b == 0)));
    }
}

#[test]
fn the_store_runs_on_it() {
    let h = Heap::new(Config::default());
    let f = h.sym("f");
    let mut t = h.int(0);
    for k in 0..20_000 {
        t = h.call(f, &[t, h.int(k), h.string(&format!("s{k}"))]);
    }
    assert!(h.sym_name(h.sym("f")) == "f");
}
