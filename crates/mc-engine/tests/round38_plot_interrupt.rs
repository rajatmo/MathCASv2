//! Plots of numerically solved differential equations. Compiled code calls a solution through a callee registered
//! once per process under the solution's content key; when the session that owned the solution has ended and an
//! equal solution is solved again, the callee looks the new copy up. These tests check that such a plot draws the
//! same picture as the first, that it stops promptly when interrupted at any moment, and that many sessions plotting
//! equal solutions at once all finish.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::sync::mpsc;
use std::time::Duration;

/// y' = y cos(x + y), y(0) = 1, on [0, 30]
const SOLVE: &str = "ysol = n_dsolve_value([y'(x) == y(x) * cos(x + y(x)), y(0) == 1], y, [x, 0, 30])";
const PLOT: &str = "plot(ysol(x), [x, 0, 30], plot_range: all)";
/// the family y' = a y, y(0) = 1
const FAMILY: &str = "sol = parametric_nd_solve([y'(t) == a * y(t), y(0) == 1], y, [t, 0, 10], [a])";
const FAMILY_PLOT: &str = "plot(eval(table(rewrite(y(a)(t), sol), a, range(-1, 1, 0.1))), [t, 0, 1], plot_range: all)";

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// Runs `f` on its own thread and waits for it a generous while: a deadlock shows as a failure, not a stuck run.
fn finishes<R: Send + 'static>(what: &str, f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(300)).unwrap_or_else(|_| panic!("{what} did not finish"))
}

fn is_picture(s: &str) -> bool {
    s.starts_with("image(")
}

/// The bytes of the picture file a plot printed as `image("path")`.
fn picture(s: &str) -> Vec<u8> {
    assert!(is_picture(s), "{s}");
    let path = s.trim_start_matches("image(\"").split('"').next().unwrap_or_default();
    std::fs::read(path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

/// A number printed by the session.
fn number(s: &str) -> f64 {
    s.trim().parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

#[test]
fn a_solution_solved_again_in_a_later_session_is_plotted() {
    finishes("plotting solutions solved again", || {
        let mut e = engine();
        eval(&mut e, SOLVE);
        let a = eval(&mut e, PLOT);
        assert!(is_picture(&a), "{a}");
        let v1 = eval(&mut e, "ysol(20.0)");
        drop(e);
        // a new session solves the same equation: an equal solution under the same key
        let mut e = engine();
        eval(&mut e, SOLVE);
        let b = eval(&mut e, PLOT);
        assert!(picture(&a) == picture(&b), "both sessions draw the same picture");
        assert_eq!(eval(&mut e, "ysol(20.0)"), v1, "the solution solved again has the same values");
        // the residual of the equation at a few points, by the solution's own derivative
        for x in ["3.5", "11.0", "27.25"] {
            let r = number(&eval(&mut e, &format!("ysol'({x}) - ysol({x}) * cos({x} + ysol({x}))")));
            assert!(r.abs() < 1e-5, "residual at {x}: {r}");
        }
        // a third session: the derivative and another solution
        drop(e);
        let mut e = engine();
        eval(&mut e, SOLVE);
        let d = eval(&mut e, "plot(ysol'(x), [x, 0, 30])");
        assert!(is_picture(&d), "{d}");
        eval(&mut e, "zsol = n_dsolve_value([z'(x) == -z(x), z(0) == 2], z, [x, 0, 3])");
        assert!(is_picture(&eval(&mut e, "plot(zsol(x), [x, 0, 3])")));
        // z = 2 e^(-x)
        assert!((number(&eval(&mut e, "zsol(1.5)")) - 2.0 * (-1.5f64).exp()).abs() < 1e-6);
    });
}

#[test]
fn a_family_solved_again_in_a_later_session_is_plotted() {
    finishes("plotting a family solved again", || {
        let mut pictures = vec![];
        for _ in 0..3 {
            let mut e = engine();
            eval(&mut e, FAMILY);
            pictures.push(picture(&eval(&mut e, FAMILY_PLOT)));
            // y = e^(a t)
            let v = number(&eval(&mut e, "rewrite(y(0.5)(2.0), sol)"));
            assert!((v - 1f64.exp()).abs() < 1e-6, "{v}");
        }
        assert!(pictures.iter().all(|p| *p == pictures[0]), "every session draws the same picture");
    });
}

/// Interrupts a run of plots of a solution solved again, at many moments; each run returns promptly and the session
/// goes on.
#[test]
fn an_interrupted_plot_of_a_solution_returns_promptly() {
    finishes("interrupted plots", || {
        // an earlier session leaves the callee of the solution behind
        let mut e = engine();
        eval(&mut e, SOLVE);
        eval(&mut e, PLOT);
        drop(e);
        let runs = "table(plot(ysol(x), [x, 0, 30 - k / 7], plot_range: all), k, 1, 60)";
        for delay in [0u64, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144] {
            let mut e = engine();
            eval(&mut e, SOLVE);
            let stop = StopFlag::new();
            let (tx, rx) = mpsc::channel::<()>();
            let raiser = {
                let stop = stop.clone();
                // a timer waiting on a channel: it raises the flag after `delay` milliseconds
                std::thread::spawn(move || {
                    let _ = rx.recv_timeout(Duration::from_millis(delay));
                    stop.raise();
                })
            };
            let t0 = std::time::Instant::now();
            let r = e.eval(runs, &stop);
            raiser.join().unwrap();
            drop(tx);
            // an interrupt after the last plot leaves a finished table; either way it came back promptly
            assert!(t0.elapsed() < Duration::from_secs(60), "delay {delay} ms: {:?}", t0.elapsed());
            if let Ok(v) = r {
                assert!(v.printed.starts_with("[image("), "delay {delay} ms: {}", &v.printed[..v.printed.len().min(80)]);
            }
            assert_eq!(eval(&mut e, "1 + 1"), "2");
            let again = eval(&mut e, PLOT);
            assert!(is_picture(&again), "delay {delay} ms: the session plots again: {again}");
        }
    });
}

/// Sessions on several threads solve equal solutions and plot them at once, ending and starting again, so the
/// shared callee of a solution meets many dead and live copies concurrently.
#[test]
fn concurrent_plots_of_solutions_finish() {
    finishes("concurrent plots", || {
        let mut first = engine();
        eval(&mut first, SOLVE);
        let want = picture(&eval(&mut first, PLOT));
        drop(first);
        let workers: Vec<_> = (0..6)
            .map(|w| {
                let want = want.clone();
                std::thread::spawn(move || {
                    for round in 0..8 {
                        let mut e = engine();
                        if (w + round) % 2 == 0 {
                            eval(&mut e, SOLVE);
                            assert!(picture(&eval(&mut e, PLOT)) == want, "worker {w}, round {round}");
                        } else {
                            eval(&mut e, FAMILY);
                            assert!(is_picture(&eval(&mut e, FAMILY_PLOT)), "worker {w}, round {round}");
                        }
                    }
                })
            })
            .collect();
        for h in workers {
            h.join().unwrap();
        }
    });
}
