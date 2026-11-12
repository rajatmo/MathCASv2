//! I1's and I3's stress invariants: every shape, collecting at every safe point (a major collection every fourth)
//! and verifying the heap before and after each collection, must give the check of the non-collecting,
//! single-threaded reference, at 1, 2 and 4 threads, with serial and parallel copying.
use mc_core::shapes::{run, Scale, Shape};
use mc_core::Config;

fn reference(shape: Shape) -> u64 {
    run(shape, Config::no_collect(), 1, Scale::Quick).check
}

fn stress_shape(shape: Shape) {
    let want = reference(shape);
    for par_gc in [true, false] {
        for threads in [1, 2, 4] {
            let cfg = Config { par_gc, ..Config::stress() };
            let out = run(shape, cfg, threads, Scale::Quick);
            assert_eq!(out.check, want, "{}: {threads} threads, parallel copy {par_gc}", shape.name());
            assert!(out.gc.minors > 0 && out.gc.majors > 0, "{}: stress mode collected", shape.name());
        }
    }
    // a nursery of one byte: a minor collection at every safe point, majors by the normal trigger, no verification
    let cfg = Config { nursery: 1, ..Config::default() };
    assert_eq!(run(shape, cfg, 4, Scale::Quick).check, want, "{}: tiny nursery", shape.name());
}

#[test]
fn stress_s3() {
    stress_shape(Shape::S3);
}

#[test]
fn stress_s8() {
    stress_shape(Shape::S8);
}

#[test]
fn stress_long_session() {
    stress_shape(Shape::Ls);
}

#[test]
fn stress_m8() {
    stress_shape(Shape::M8);
}

#[test]
fn stress_numbers() {
    stress_shape(Shape::Num);
}
