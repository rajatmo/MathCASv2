//! The measurement behind the packing threshold (decision N4a.3): for lists of `n` machine reals, the cost of
//! building `list(...)` from element terms (packed from `PACK_MIN` leaves on) and viewing it once (which unpacks a
//! packed list), against the same work on `g(...)`, which is never packed; and the bytes each form keeps.
//!
//! `cargo run --release -p mc-core --example pack_threshold`
use mc_core::{Config, Heap, Roots, Session, Store, View, PACK_MIN};
use std::time::Instant;

fn main() {
    let sizes = [8usize, 16, 32, 48, 64, 96, 128, 256, 1024, 8192];
    println!("PACK_MIN = {PACK_MIN}");
    println!("{:>6} {:>12} {:>12} {:>8} {:>12} {:>12}", "n", "list ns/el", "g ns/el", "ratio", "list B/el", "g B/el");
    for (k, &n) in sizes.iter().enumerate() {
        let reps = (2_000_000 / n).max(4);
        let mut times = [0f64; 2];
        let mut bytes = [0f64; 2];
        for (j, head) in ["list", "g"].iter().enumerate() {
            let mut s = Session::new(Config::default(), 1, Roots::new());
            let t0 = Instant::now();
            s.step(|h: &Heap, keep: &mut Roots| {
                let f = h.sym(head);
                for r in 0..reps {
                    let xs: Vec<_> = (0..n).map(|i| h.real((r * n + i) as f64 * 0.5)).collect();
                    let t = h.call(f, &xs);
                    // one consumer that reads the elements through the view
                    if let View::App { args, .. } = h.view(t) {
                        assert_eq!(args.len(), n);
                    }
                    if r % 16 == 0 {
                        keep.push(t);
                    }
                }
            });
            times[j] = t0.elapsed().as_nanos() as f64 / (reps * n) as f64;
            s.collect_full();
            let kept = reps.div_ceil(16) * n;
            bytes[j] = s.heap.reserved_bytes() as f64 / kept as f64;
        }
        println!(
            "{n:>6} {:>12.1} {:>12.1} {:>8.2} {:>12.1} {:>12.1}   [{}/{} {}%]",
            times[0],
            times[1],
            times[0] / times[1],
            bytes[0],
            bytes[1],
            k + 1,
            sizes.len(),
            (k + 1) * 100 / sizes.len()
        );
    }
}
