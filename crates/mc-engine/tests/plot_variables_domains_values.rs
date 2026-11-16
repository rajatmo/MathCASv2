//! Plots whose variables have values outside the plot, plots over regions (`element([x, y, z], region)` in space
//! plots, `region_plot3d` of a region), ranges whose ends depend on the first variable, the evaluation monitor of
//! plots of a complex variable, values with an imaginary part of rounding size, and functions of integers that
//! overflow machine numbers. Checks: the plots draw (an image), outside values are left alone, the monitor sees
//! complex sample points inside the plotted rectangle, and the slow exact route is not taken. Each test has a time
//! bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn draws(e: &mut CoreEngine, src: &str) {
    let out = ev(e, src);
    assert!(out.starts_with("image("), "{src} gave {out}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn plot_variables_are_local() {
    timed("local variables", 60, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "x = 3; n_u = 4; a = 2;");
        draws(&mut e, "plot(x^2, [x, 0, 1])");
        draws(&mut e, "plot3d(x*y, [x, 0, 1], [y, 0, 1])");
        draws(&mut e, "discrete_plot(n_u^2, [n_u, 1, 10])");
        // values of other symbols still enter the plot and its ranges
        draws(&mut e, "plot(a*t, [t, 0, 1 + a])");
        holds(&mut e, "[x, n_u, a] === [3, 4, 2]");
        // a definition for machine reals only is sampled at machine reals
        ev(&mut e, "rule f(t_: Float) := total(first(real_digits(t)));");
        draws(&mut e, "plot(f(x), [x, 0, 1])");
        // a plot left unevaluated shows its ranges evaluated
        holds(&mut e, "head(plot3d(q(u, v), element([u, v], tf(disk())))) === plot3d");
    });
}

#[test]
fn space_plots_over_regions() {
    timed("space plots over regions", 120, || {
        let mut e = CoreEngine::new();
        draws(&mut e, "contour_plot3d(x^3 + y^2 - z^2, element([x, y, z], ball([0, 0, 0], 3)))");
        draws(&mut e, "contour_plot3d(x + y + z, element([x, y, z], ball()))");
        draws(&mut e, "density_plot3d(x*y*z, element([x, y, z], cone()))");
        draws(&mut e, "region_plot3d(sin(x)*cos(y) + sin(y)*cos(z) + sin(z)*cos(x) > 0, element([x, y, z], ball([0, 0, 0], 10)))");
        draws(&mut e, "region_plot3d(sphere())");
        draws(&mut e, "region_plot3d(implicit_region(4 <= x^2 + y^2 + z^2 and x^2 + y^2 + z^2 <= 9, [x, y, z]))");
        // ranges with ends that depend on the first variable
        draws(&mut e, "plot3d(sin(5*x*y + y^2) + 1, [x, -1, 1], [y, -sqrt(1 - x^2), sqrt(1 - x^2)])");
        draws(&mut e, "parametric_plot([x, y], [x, 0, 1], [y, 0, 1 - x])");
        // a function the compiler cannot sample, evaluated point by point
        draws(&mut e, "plot3d(signed_region_distance(simplex(2), [x, y]), [x, -1, 2], [y, -1, 2])");
    });
}

#[test]
fn values_and_monitors() {
    timed("values and monitors", 120, || {
        let mut e = CoreEngine::new();
        // a closed form with roots of unity: real values with imaginary parts of rounding size
        ev(&mut e, "v = r_solve_value([a(n_u + 3) == 2*a(n_u), a(0) == 1, a(1) == 2, a(2) == -1], a(n_u), n_u);");
        draws(&mut e, "discrete_plot(v, [n_u, 15])");
        draws(&mut e, "list_plot(table(rewrite(v, n_u -> k), k, 15))");
        // powers of powers: machine numbers overflow, exact numbers are huge; drawn from 30-digit values, quickly
        let t0 = Instant::now();
        draws(&mut e, "discrete_plot(1/2 - (1/2)*2^2^(1 + n_u)*5^-2^n_u, [n_u, 0, 20])");
        assert!(t0.elapsed() < Duration::from_secs(5), "powers of powers took {:?}", t0.elapsed());
        // a list of functions in a style, one with its own style
        draws(&mut e, "discrete_plot(style([sin(t), style(cos(t), blue), sin(pi + t)], red), [t, 0, 2*pi, pi/12])");
        // a curve of the list that is not a list of three coordinates is not drawn
        draws(&mut e, "parametric_plot3d([[t, t^2, t^3], undefined_curve], [t, 0, 2])");
        // the monitor of a complex plot sees complex sample points in the rectangle
        holds(&mut e, "{ let s = reap(complex_vector_plot(im*z, [z, -2 - 2*im, 2 + 2*im], evaluation_monitor: (z => sow(z))))[-1, 1]; length(s) > 0 and every(s, w => abs(re(w)) <= 2 and abs(im_part(w)) <= 2) and length(select(s, w => im_part(w) != 0)) > 0 }");
    });
}
