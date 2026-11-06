//! The panic guard (package robust): a native that panics gives `failure(internal, "message (location)", f)` as the
//! statement's value and the session goes on, with its definitions, history and line counter intact.
use mc_bench::{Engine, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::{CoreEngine, EachOptions, EngineConfig, Origin, Report};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// `boom(x)` panics with a formatted message.
struct Boom;
/// `out_of_bounds(x)` indexes past the end of a vector.
struct OutOfBounds;
/// `boom_inside(x)` evaluates `boom(x)`: the innermost native is the one named.
struct BoomInside;
/// `boom_scratch(x)` panics inside a scratch scope, after evaluating in it.
struct BoomScratch;

impl Native for Boom {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let n = match args.first().map(|&a| cx.heap().view(a)) {
            Some(View::Int(n)) => n,
            _ => -1,
        };
        panic!("boom number {n}");
    }
}

impl Native for OutOfBounds {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let v = [1u8; 2];
        let i = args.len() + 5;
        Ok(Reply::Done(cx.heap().int(v[i] as i64)))
    }
}

impl Native for BoomInside {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let v = cx.eval(h.call(h.sym("boom"), args))?;
        Ok(Reply::Done(v))
    }
}

impl Native for BoomScratch {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let x = args[0];
        cx.scratch(|cx| {
            let y = cx.eval(h.call(h.sym("add"), &[x, h.int(1)]))?;
            let _ = cx.eval(h.call(h.sym("mul"), &[y, y]))?;
            panic!("inside a scratch scope");
        })
    }
}

fn install(k: &mut Kernel, h: &Heap) {
    let strict = Attributes::default();
    k.register(h.sym("boom"), strict, Box::new(Boom));
    k.register(h.sym("out_of_bounds"), strict, Box::new(OutOfBounds));
    k.register(h.sym("boom_inside"), strict, Box::new(BoomInside));
    k.register(h.sym("boom_scratch"), strict, Box::new(BoomScratch));
}

fn engine(threads: usize) -> CoreEngine {
    let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
    e.install(install);
    e
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// `failure(internal, "boom number 7 (crates/.../guard.rs:L:C)", boom)` has this file as its location.
fn assert_internal(printed: &str, message: &str, native: Option<&str>) {
    assert!(printed.starts_with(&format!("failure(internal, \"{message} (")), "{printed}");
    assert!(printed.contains("guard.rs:"), "the location is kept: {printed}");
    match native {
        Some(f) => assert!(printed.ends_with(&format!("\", {f})")), "{printed}"),
        None => assert!(printed.ends_with("\")"), "{printed}"),
    }
}

#[test]
fn a_panicking_native_gives_an_internal_failure_and_the_session_goes_on() {
    let mut e = engine(2);
    assert_eq!(eval(&mut e, "a = 41"), "null");
    assert_internal(&eval(&mut e, "boom(7)"), "boom number 7", Some("boom"));
    assert_eq!(eval(&mut e, "a + 1"), "42");
    let printed = eval(&mut e, "out_of_bounds(1)");
    assert!(printed.starts_with("failure(internal, \"index out of bounds"), "{printed}");
    assert!(printed.ends_with(", out_of_bounds)"), "{printed}");
    // the innermost native is named
    assert_internal(&eval(&mut e, "boom_inside(3)"), "boom number 3", Some("boom"));
    // a failure value like any other: it propagates through a strict call
    assert!(eval(&mut e, "f(x_) := x + 1\nf(boom(1))").starts_with("failure(internal"));
    assert_eq!(eval(&mut e, "f(2)"), "3");
    // definitions made earlier in the same input stay
    assert!(eval(&mut e, "b = 5\nboom(2)").starts_with("failure(internal"));
    assert_eq!(eval(&mut e, "b"), "5");
    // bind_last sees the failure as the last result
    assert!(eval(&mut e, "boom(3)").starts_with("failure(internal"));
    e.bind_last("last").unwrap();
    assert!(eval(&mut e, "last").starts_with("failure(internal"));
}

#[test]
fn panics_in_parallel_tasks_and_in_scratch_scopes_are_caught() {
    for threads in [1, 4] {
        let mut e = engine(threads);
        // many elements: the group is split into speculative tasks on several threads
        let src = "map(k => if k == 900 then boom(k) else sum(range(k)), range(1000))";
        assert_internal(&eval(&mut e, src), "boom number 900", Some("boom"));
        assert_eq!(eval(&mut e, "map(k => k^2, range(4))"), "[1, 4, 9, 16]");
        assert_internal(&eval(&mut e, "boom_scratch(4)"), "inside a scratch scope", Some("boom_scratch"));
        // the scratch slot was released: scopes keep working
        for _ in 0..40 {
            assert!(eval(&mut e, "boom_scratch(1)").starts_with("failure(internal"));
        }
        assert_eq!(eval(&mut e, "n_integrate(x^2, [x, 0, 3])"), "9.0");
    }
}

#[test]
fn statements_after_a_panic_in_the_same_input_run() {
    let mut e = engine(2);
    let mut reports = vec![];
    let r = e.eval_each("1 + 1\nboom(5)\nc = 2\nc * 3", &Origin::input(), &StopFlag::new(), EachOptions { numbered: true }, &mut |r| {
        reports.push(r)
    });
    assert_eq!(r.ok(), Some(true), "a statement failed");
    assert_eq!(reports.len(), 3, "{reports:?}");
    assert_eq!(reports[0], Report::Value { n: Some(1), printed: "2".into(), image: None });
    match &reports[1] {
        Report::Failure { n: Some(2), printed } => assert_internal(printed, "boom number 5", Some("boom")),
        r => panic!("{r:?}"),
    }
    assert_eq!(reports[2], Report::Value { n: Some(4), printed: "6".into(), image: None });
    assert_eq!(e.line(), 5);
    // the history keeps the earlier values
    assert_eq!(eval(&mut e, "out(1) + out(4)"), "8");
}

#[test]
fn many_panics_in_a_row_leave_a_working_session() {
    let mut e = engine(4);
    for i in 0..200 {
        assert!(eval(&mut e, &format!("boom({i})")).contains(&format!("boom number {i} (")));
    }
    assert_eq!(eval(&mut e, "expand((x + 1)^3)"), "1 + 3*x + 3*x^2 + x^3");
}
