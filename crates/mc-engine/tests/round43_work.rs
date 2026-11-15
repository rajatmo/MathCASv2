//! Round 43: budgets of work. The methods that give up after a budget (integration, limits, `simplify`, `d_solve`,
//! sums, the decision service) count fuel, not seconds: the same inputs evaluated with every block of fuel delayed
//! (a slower machine) give the same printed results and spend the same fuel, and four threads give the results of
//! one.
use mc_api::{Engine, StopFlag};
use mc_core::{Heap, Store, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};

/// `work_of(e)`: `[fuel, value]`, the fuel the evaluation of `e` spent and its value.
struct WorkOf;

impl Native for WorkOf {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [e] = args else { return Ok(Reply::Decline) };
        let f0 = cx.fuel_spent();
        let v = cx.eval(*e)?;
        let fuel = (cx.fuel_spent() - f0) as i64;
        Ok(Reply::Done(h.call(h.sym("list"), &[h.int(fuel), v])))
    }
}

fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("work_of"), Attributes { hold: Hold::All, effect: true, ..Attributes::default() }, Box::new(WorkOf));
}

/// Inputs whose methods run into their budgets of work or come near them (an integral without an elementary
/// antiderivative, searches of `simplify` over rational functions of several variables whose polynomial moves have
/// a budget of their own, a two-sided limit, an equation and a sum), and some that finish well within them.
const INPUTS: &[&str] = &[
    "integrate(sqrt(1 + x^3), x)",
    "integrate(x^2*sin(x), x)",
    "simplify(sum(1/(x^k + y^(k + 1) + k), [k, 1, 6]))",
    "full_simplify((x^12 - y^12)/(x^4 - y^4) - (x^8 + x^4*y^4 + y^8))",
    "simplify(sin(x)^2 + cos(x)^2)",
    "limit((1 + 1/x)^x, x -> inf)",
    "d_solve(y''(x) + x*y(x) == 0, y(x), x)",
    "sum(1/k^2, [k, 1, inf])",
];

/// Each input in a fresh session with `threads` threads: its printed `[fuel, value]`.
fn run(threads: usize, inputs: &[&str]) -> Vec<String> {
    inputs
        .iter()
        .map(|src| {
            let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
            e.install(install);
            match e.eval(&format!("work_of({src})"), &StopFlag::new()) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            }
        })
        .collect()
}

/// The value part of a printed `[fuel, value]`.
fn value(printed: &str) -> &str {
    printed.split_once(", ").map_or(printed, |(_, v)| v)
}

/// One test, the runs in order: the libraries keep caches for the life of the process, so a first run fills them
/// and the runs compared come after it.
#[test]
fn budgets_of_work_do_not_depend_on_speed_or_threads() {
    let _warm = run(1, INPUTS);
    let plain = run(1, INPUTS);
    // every block of fuel delayed by a millisecond: the evaluation is several times slower
    mc_eval::set_test_slowdown(1000);
    let slow = run(1, INPUTS);
    mc_eval::set_test_slowdown(0);
    for ((src, a), b) in INPUTS.iter().zip(&plain).zip(&slow) {
        assert_eq!(a, b, "{src}: the same value and the same fuel on a slower machine");
    }
    let four = run(4, INPUTS);
    for ((src, a), b) in INPUTS.iter().zip(&plain).zip(&four) {
        assert_eq!(value(a), value(b), "{src}: four threads give the value of one");
    }
}
