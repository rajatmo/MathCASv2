//! The big builders count the memory of their result against what the evaluation may still allocate (the session's
//! budget, or the cap of an enclosing `memory_constrained`) before allocating, so that a result the memory cannot
//! hold is a failure of the input and the session goes on. Each input below would allocate gigabytes to terabytes at
//! once (an allocation the process cannot survive); under a cap of 10^8 bytes each must give `failure(limit, memory,
//! ...)` at once, and the session must still evaluate afterwards.
//!
//! Covered here: `range` (integer and inexact steps), `table` (a count and an iterator), `array`, `constant_array`,
//! `join`, `catenate`, `flatten`, `transpose` of deep arrays, and the arrays of `random_real`, `random_integer`.
//! Covered elsewhere by size limits decided before computing: exact powers and factorials (2^28 bits), strings
//! (2^30 bytes, the printer a quarter of the memory left), `tuples`, `subsets`, `permutations`, `outer` (2^26
//! slots), the matrix builders of the prelude (`memory_available`) and the raster size of pictures.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::Instant;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn builders_refuse_results_beyond_the_memory_left() {
    let mut e = CoreEngine::new();
    ev(&mut e, "big = range(10^4)");
    let inputs = [
        "length(range(10^10))",
        "length(range(0, 10^10, 1/2))",
        "length(range(0., 10.^10))",
        "length(table(i, [i, 10^10]))",
        "length(table(0, 10^10))",
        "length(array(f, [10^5, 10^5]))",
        "length(constant_array(0, [10^5, 10^5]))",
        "length(join(table(big, 10^4)))",
        "length(apply(join, table(big, 10^4)))",
        "length(catenate(table(big, 10^4)))",
        "length(flatten(table(big, 10^6)))",
        "dimensions(random_real(1, [30000, 30000]))",
        "dimensions(random_integer(9, [30000, 30000]))",
    ];
    for src in inputs {
        let t0 = Instant::now();
        let out = ev(&mut e, &format!("memory_constrained({src}, 10^8)"));
        let ms = t0.elapsed().as_millis();
        assert!(out.starts_with("failure(limit, memory"), "{src}: {out}");
        assert!(ms < 20_000, "{src} took {ms} ms");
    }
    assert_eq!(ev(&mut e, "length(big)"), "10000");
}
