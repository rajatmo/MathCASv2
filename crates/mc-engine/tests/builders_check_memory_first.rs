//! The big builders count the memory of their result against what the evaluation may still allocate (the session's
//! budget, or the cap of an enclosing `memory_constrained`) before allocating, so that a result the memory cannot
//! hold is a failure of the input and the session goes on. Each input below would allocate gigabytes to terabytes at
//! once (an allocation the process cannot survive); under a cap of 10^8 bytes each must give `failure(limit, memory,
//! ...)` at once, and the session must still evaluate afterwards.
//!
//! Covered here: `range` (integer and inexact steps), `table` (a count and an iterator), `array`, `constant_array`,
//! `join`, `catenate`, `flatten`, `transpose` of deep arrays, and the arrays of `random_real`, `random_integer`.
//! Also `dot` and `kronecker_product` (machine and symbolic entries), `outer`, and the strings that `string_pad` and
//! `string_replace` build (below their limit of 2^30 bytes).
//! Covered elsewhere by size limits decided before computing: exact powers and factorials (2^28 bits), strings
//! (2^30 bytes, the printer a quarter of the memory left), `tuples`, `subsets`, `permutations` (2^26 slots), the
//! matrix builders of the prelude (`memory_available`) and the raster size of pictures.
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

/// Products and strings whose inputs fit the cap but whose result does not: the result is refused before it is
/// built, and the same calls with a result that fits still evaluate.
#[test]
fn products_and_strings_refuse_results_beyond_the_memory_left() {
    let mut e = CoreEngine::new();
    ev(&mut e, "col = table(1.0, 20000, 2); row = table(1.0, 2, 20000); sc = table(x, 3000, 2); sr = table(y, 2, 3000)");
    ev(&mut e, "km = table(x, 80, 80); r = range(4000); sa = string_pad(\"a\", 10^5, \"a\")");
    let inputs = [
        "length(dot(col, row))",
        "length(dot(sc, sr))",
        "length(kronecker_product(km, km))",
        "length(outer(f, r, r))",
        "string_length(string_pad(\"a\", 9*10^8))",
        "string_length(string_replace(sa, \"a\" -> string_pad(\"b\", 5000, \"b\")))",
    ];
    for src in inputs {
        let t0 = Instant::now();
        let out = ev(&mut e, &format!("memory_constrained({src}, 10^8)"));
        let ms = t0.elapsed().as_millis();
        assert!(out.starts_with("failure(limit, memory"), "{src}: {out}");
        assert!(ms < 20_000, "{src} took {ms} ms");
    }
    assert_eq!(ev(&mut e, "memory_constrained(length(dot(table(1.0, 100, 2), table(1.0, 2, 100))), 10^8)"), "100");
    assert_eq!(ev(&mut e, "memory_constrained(length(kronecker_product(table(x, 10, 10), table(y, 10, 10))), 10^8)"), "100");
    assert_eq!(ev(&mut e, "memory_constrained(length(outer(f, range(100), range(100))), 10^8)"), "100");
    assert_eq!(ev(&mut e, "memory_constrained(string_length(string_pad(\"a\", 10^6)), 10^8)"), "1000000");
    assert_eq!(ev(&mut e, "string_length(sa)"), "100000");
}
