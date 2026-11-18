//! The special functions report their work: an evaluation that runs for milliseconds is charged fuel in proportion
//! (the budgets of the methods that call them count fuel, never time). Each input is measured as `[fuel, value]`;
//! one that takes longer than 2 ms must be charged at least a floor of fuel per millisecond, far below the rate of
//! the evaluator's own steps (above 1,000 units per ms here), so only work that is not reported at all fails it:
//! before the machine Mathieu, Heun and zeta kernels reported theirs, `zeta(1/2 + 10^5 i)` ran 150 ms on 16 units
//! and `mathieu_characteristic_a(30, 500.0)` 5 ms on 5.
//!
//! `MC_SPECIAL_WORK_SURVEY=1` prints every input's time, fuel and rate.
use mc_api::{Engine, StopFlag};
use mc_core::{Heap, Store, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
use std::time::Instant;

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

/// Machine-precision and high-precision values of the special functions whose kernels run long loops: series,
/// recurrences, quadratures, continued fractions, asymptotic expansions with many terms.
const INPUTS: &[&str] = &[
    "weber_e(3, 50.0)",
    "anger_j(3, 50.0)",
    "anger_j(1/3, 25.5)",
    "struve_h(2, 30.0)",
    "struve_l(1, 12.5)",
    "bessel_j(10, 100.0)",
    "bessel_k(2.5, 0.01)",
    "hankel_h1(3, 20.0 + 5.0*im)",
    "mathieu_c(2, 1.0, 0.5)",
    "mathieu_characteristic_a(4, 7.5)",
    "heun_g(2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5)",
    "lommel_s1(0.5, 1.5, 10.0)",
    "hypergeometric_pfq([1, 2, 3], [4, 5], 0.99)",
    "hypergeometric_1f1(3.5, 2.5, 40.0)",
    "hypergeometric_u(2.5, 1.5, 0.3)",
    "polylog(3, 0.95)",
    "zeta(0.5 + 100.0*im)",
    "lerch_phi(0.9, 2.0, 0.3)",
    "coulomb_f(2, 1.5, 30.0)",
    "elliptic_k(0.999999)",
    "jacobi_sn(3.0, 0.99)",
    "gamma_regularized(50.0, 45.0)",
    "beta_regularized(0.4, 30.0, 40.0)",
    "erfc(25.0)",
    "fresnel_c(40.0)",
    "exp_integral_e(3.5, 20.0)",
    "airy_ai(-30.0)",
    "spheroidal_ps(2, 1, 1.5, 0.4)",
    "n(weber_e(3, 50), 30)",
    "n(anger_j(3, 50), 30)",
    "n(struve_h(2, 30), 30)",
    "n(bessel_j(10, 100), 40)",
    "n(mathieu_c(2, 1, 1/2), 30)",
    "n(hypergeometric_pfq([1, 2, 3], [4, 5], 99/100), 30)",
    "n(polylog(3, 95/100), 40)",
    "n(zeta(1/2 + 100*im), 30)",
    "n(lerch_phi(9/10, 2, 3/10), 30)",
    "n(coulomb_f(2, 3/2, 30), 30)",
    "n(heun_g(2, 1, 1, 1, 1, 1, 1/2), 30)",
    "n(gamma_regularized(50, 45), 40)",
    "n(airy_ai(-30), 40)",
    "n(exp_integral_e(7/2, 20), 40)",
    "mathieu_c(20, 400.0, 2.0)",
    "mathieu_characteristic_a(30, 500.0)",
    "mathieu_s(7, 150.0, 1.2)",
    "heun_g(-1.5, 0.3, 2.5, 1.5, 3.0, 1.0, 0.95)",
    "heun_c(4.0, -0.6, -0.7, -0.18, 0.3, 0.9)",
    "hypergeometric_1f1(-200.5, 300.5, 500.0)",
    "hypergeometric_2f1(20.5, -30.25, 10.5, 0.999)",
    "zeta(0.5 + 100000.0*im)",
    "coulomb_f(50, -30.0, 200.0)",
    "coulomb_g(3, 20.0, 0.5)",
    "lommel_s1(20.5, 30.5, 500.0)",
    "beta_regularized(0.5, 10000.0, 10000.0)",
    "gamma_regularized(100000.0, 100000.0)",
    "exp_integral_e(100.5, 0.5)",
    "struve_h(50, 2000.0)",
    "anger_j(40.5, 300.0)",
    "weber_e(40.5, 300.0)",
    "polylog(2.5, 0.9999)",
    "lerch_phi(0.999, 1.5, 0.5)",
    "bessel_j(200.5, 150.0)",
    "spheroidal_s1(5, 2, 20.0, 0.7)",
    "elliptic_theta(3, 0.5, 0.999)",
    "kelvin_ber(10, 300.0)",
    "parabolic_cylinder_d(30.5, 15.0)",
    "hypergeometric_pfq([1.5, 2.5, 3.5], [4.5, 0.5], -300.0)",
    "marcum_q(3, 20.0, 25.0)",
    "inverse_beta_regularized(0.3, 500.0, 700.0)",
];

/// The lowest rate of fuel per millisecond allowed for an evaluation of more than 2 ms.
const FLOOR: f64 = 5.0;

/// The time in ms and the fuel of one evaluation of `src`, in a fresh session (values are remembered) whose start
/// is not measured.
fn measure(src: &str) -> (f64, f64) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.install(install);
    let _ = e.eval("work_of(sin(1.5))", &StopFlag::new());
    let t0 = Instant::now();
    let out = e.eval(&format!("work_of({src})"), &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| err.to_string());
    let ms = t0.elapsed().as_secs_f64() * 1e3;
    let fuel: f64 = out.trim_start_matches('[').split(',').next().and_then(|f| f.trim().parse().ok()).unwrap_or(-1.0);
    (ms, fuel)
}

#[test]
fn long_special_function_evaluations_report_their_work() {
    let survey = std::env::var_os("MC_SPECIAL_WORK_SURVEY").is_some();
    let mut bad = vec![];
    for src in INPUTS {
        // the fuel is the same on every run; the time of one run can be stretched by the load of the machine, so an
        // input under the floor is measured twice more and judged by its shortest time
        let mut best = (f64::INFINITY, 0.0);
        for _ in 0..3 {
            let (ms, fuel) = measure(src);
            if ms < best.0 {
                best = (ms, fuel);
            }
            if !(best.0 > 2.0 && best.1 / best.0 < FLOOR) {
                break;
            }
        }
        let (ms, fuel) = best;
        let rate = fuel / ms.max(1e-9);
        if survey {
            eprintln!("{ms:9.2} ms {fuel:10} units {rate:9.1}/ms  {src}");
        }
        if ms > 2.0 && rate < FLOOR {
            bad.push(format!("{src}: {ms:.1} ms for {fuel} units of fuel"));
        }
    }
    assert!(bad.is_empty(), "work not reported:\n{}", bad.join("\n"));
}
