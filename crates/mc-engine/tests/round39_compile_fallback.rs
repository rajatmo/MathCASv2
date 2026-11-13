//! Round 39: numeric functions when the code generator fails. With compilation switched to a panic inside the code
//! generator (what a host it does not support gives) every numeric routine must give the answer it gives with
//! compiled code, through the interpreters. The switch is process-wide, so these tests have a binary of their own.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use mc_jit::jit::{set_failure, Failure};

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn num(src: &str) -> f64 {
    let s = ev(src);
    s.trim().parse::<f64>().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

fn failing<R>(mode: Failure, f: impl FnOnce() -> R) -> R {
    set_failure(mode);
    f()
}

#[test]
fn numeric_routines_without_compiled_code() {
    for mode in [Failure::Panic, Failure::Refuse] {
        failing(mode, || {
            // integrals against closed forms
            assert!((num("n_integrate(1/(1 + x^2), [x, 0, inf])") - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
            assert!((num("n_integrate(exp(-x^2), [x, -inf, inf])") - std::f64::consts::PI.sqrt()).abs() < 1e-10);
            assert!((num("n_integrate(sin(x)^2, [x, 0, pi])") - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
            // an exact integral that checks itself numerically
            assert_eq!(ev("integrate(1/(1 + x^2), [x, 0, inf])"), "(1/2)*pi");
            // roots, a minimum, a differential equation, a sum
            assert!((num("x /. find_root(cos(x) - x, [x, 1])[1]") - 0.7390851332151607).abs() < 1e-14);
            assert!((num("find_minimum((x - 2)^2 + 1, [x, 0])[1]") - 1.0).abs() < 1e-12);
            let y = num("n_dsolve_value([y'(t) == -y(t), y(0) == 1], y(2), [t, 0, 2])");
            assert!((y - (-2f64).exp()).abs() < 1e-6, "{y}");
            assert!((num("n_sum(1/k^2, [k, 1, inf])") - std::f64::consts::PI.powi(2) / 6.0).abs() < 1e-10);
            // plots draw
            assert!(ev("plot(sin(x), [x, 0, 2 pi])").starts_with("image("), "plot");
            // a compiled function falls back to the interpreter of the compiled form, and says why
            assert_eq!(ev("fn sq1(k: Int) = k^2 + 1; compile(sq1); sq1(12)"), "145");
            let info = ev("fn sq2(x: Real) = x^2 + 1.0; compile(sq2); compiled_info(sq2)");
            assert!(info.contains("interpreter"), "{info}");
            let why = if mode == Failure::Panic { "the code generator failed" } else { "switched off" };
            assert!(info.contains(why), "{info}");
        });
    }
    set_failure(Failure::None);
}
