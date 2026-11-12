//! Round 36: fixed wrong answers in linear algebra, lists, numerics, graphics and
//! special functions, one test per cause. Each value is also checked by an independent route.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn the_astroid_has_perimeter_six() {
    // the arc length integrand sqrt(9/8 (1 - cos 4t)) stayed an unevaluated integral
    let r = "R = parametric_region([r * cos(θ)^3, r * sin(θ)^3], [[r, 0, 1], [θ, 0, 4 * pi]]); ";
    assert_eq!(ev(&format!("{r}perimeter(R)")), "6");
    // half angles: 1 - cos t = 2 sin(t/2)^2, 1 + cos t = 2 cos(t/2)^2
    assert_eq!(ev("integrate(sqrt(1 - cos(t)), [t, 0, 2*pi])"), "4*2^(1/2)");
    assert_eq!(ev("integrate(sqrt(2 + 2*cos(t)), [t, 0, pi])"), "4");
    // independent: the astroid x^(2/3) + y^(2/3) = 1 has length 6 by quadrature of the same speed, and by the closed
    // form 3/2 |sin 2t| over a quarter period times 4
    assert_eq!(ev("abs(n_integrate(sqrt(9/8*(1 - cos(4*t))), [t, 0, 2*pi]) - 6) < 10^-8"), "true");
    assert_eq!(ev("4*integrate(3/2*sin(2*t), [t, 0, pi/2])"), "6");
}

#[test]
fn a_regularized_hypergeometric_function_of_a_centered_interval_is_its_tight_image() {
    // the z-derivative rule makes the function monotone on the spread visible: the image of [0.49, 0.51] (was the
    // slope estimate 0.98249 +- 0.00731, wider than the image)
    assert_eq!(
        ev("d(hypergeometric_pfq_regularized([1 / 2, 2 / 3, 3 / 4], [4 / 5, 5 / 6], t), t)"),
        "(1/4)*hypergeometric_pfq_regularized([3/2, 5/3, 7/4], [9/5, 11/6], t)"
    );
    let v = "v = hypergeometric_pfq_regularized([1 / 2, 2 / 3, 3 / 4], [4 / 5, 5 / 6], centered_interval(1 / 2, 1 / 100)); ";
    // independent: the values at the ends at 30 digits lie inside and within 1e-14 of the ends
    let ends = "e = n(hypergeometric_pfq_regularized([1 / 2, 2 / 3, 3 / 4], [4 / 5, 5 / 6], [49/100, 51/100]), 30); ";
    assert_eq!(
        ev(&format!("{v}{ends}c = v[1]; r = v[2]; [c - r <= e[1], e[2] <= c + r, abs(c - r - e[1]) < 10^-14, abs(c + r - e[2]) < 10^-14]")),
        "[true, true, true, true]"
    );
    // the derivative by the series: d/dz z^k/k! terms, at z = 3/10 to 20 digits
    assert_eq!(
        ev("abs(n(rewrite(d(hypergeometric_pfq_regularized([1 / 2, 2 / 3], [4 / 5], t), t), t -> 3/10) - (hypergeometric_pfq_regularized([1 / 2, 2 / 3], [4 / 5], 3/10 + 10^-12) - hypergeometric_pfq_regularized([1 / 2, 2 / 3], [4 / 5], 3/10 - 10^-12))/(2*10^-12), 40)) < 10^-20"),
        "true"
    );
}

#[test]
fn a_theta_function_of_a_complex_nome_stays_complex() {
    // elliptic_theta(2, 0, 0.2 + 0.0 im) had become real, and e1 computed from the half-periods lost its
    // 0.0 im
    assert_eq!(ev("elliptic_theta(2, 0, 0.2 + 0.0*im)"), "complex(1.3910654385883294, 0.0)");
    assert_eq!(ev("elliptic_theta(2, 0, 0.2)"), "1.3910654385883294");
    let e1 = "rule e1([g2_, g3_]) := { ω1 = weierstrass_half_period_w1([g2, g3]); ω3 = weierstrass_half_period_w3([g2, g3]); τ = ω3 / ω1; q = exp(im * pi * τ); (pi / (2 * ω1))^2 * ((elliptic_theta(2, q)^4 + 2 * elliptic_theta(4, q)^4) / 3) }; ";
    assert_eq!(ev(&format!("{e1}e1([12., 7.])")), "complex(1.971690456281308, 0.0)");
    // independent: e1 is the largest root of 4 t^3 - 12 t - 7
    assert_eq!(ev(&format!("{e1}abs(re(e1([12., 7.])) - 1.9716904562813086) < 10^-14")), "true");
    assert_eq!(ev("abs(4*1.9716904562813086^3 - 12*1.9716904562813086 - 7) < 10^-13"), "true");
}

#[test]
fn functions_of_several_centered_intervals_are_enclosures_or_unevaluated() {
    // beta is decreasing in both arguments here: the hull of the corner values (was the first-order slope estimate)
    let v = "v = beta(centered_interval(2, 1/10), centered_interval(3, 1/10)); ";
    let ends = "e = n([beta(21/10, 31/10), beta(19/10, 29/10)], 30); ";
    assert_eq!(
        ev(&format!("{v}{ends}[v[1] - v[2] <= e[1], e[2] <= v[1] + v[2], abs(v[1] - v[2] - e[1]) < 10^-14, abs(v[1] + v[2] - e[2]) < 10^-14]")),
        "[true, true, true, true]"
    );
    // no closed-form derivative in the order: the slope's sign by differences, then the corners (the slope estimate
    // gave radius 0.0194 for the true 0.000121). Independent: J decreases in the order and, at
    // x = 2 (J1' = J0 - J1/x < 0), in x, so the ends are J(1.001, 2.001) and J(0.999, 1.999) at 30 digits
    let v = "v = bessel_j(centered_interval(1, 1/1000), centered_interval(2, 1/1000)); ";
    let ends = "e = n([bessel_j(1001/1000, 2001/1000), bessel_j(999/1000, 1999/1000)], 30); ";
    assert_eq!(
        ev(&format!("{v}{ends}[v[1] - v[2] <= e[1], e[2] <= v[1] + v[2], abs(v[1] - v[2] - e[1]) < 10^-14, abs(v[1] + v[2] - e[2]) < 10^-14]")),
        "[true, true, true, true]"
    );
    // not monotone over the box (J1 has its maximum at x = 1.8412) and no ball value: unevaluated, not an estimate
    assert_eq!(
        ev("bessel_j(centered_interval(1, 1/10), centered_interval(1841/1000, 1/10))"),
        "bessel_j(centered_interval(1, 1/10), centered_interval(1841/1000, 1/10))"
    );
    // sums and products keep the exact ball rules
    assert_eq!(ev("centered_interval(1, 1/10) + centered_interval(2, 1/10)"), "centered_interval(3.0, 0.20000000000000304)");
}

#[test]
fn the_area_of_two_disks_has_one_implementation() {
    // area, region_measure and region_moment(., [0, 0]) of the lens agree, for exact and machine data
    let r = "R = region_intersection(disk([0, 0], 2), disk([1, 1], 1)); ";
    assert_eq!(ev(&format!("{r}area(R) == region_measure(R) and area(R) == region_moment(R, [0, 0])")), "true");
    assert_eq!(ev(&format!("{r}area(R)")), "acos((-1/4)*2^(1/2)) + (-1/2)*7^(1/2) + 4*acos((5/8)*2^(1/2))");
    let m = "R = region_intersection(disk([0, 0], 1.), disk([1, 0], 1)); ";
    assert_eq!(ev(&format!("{m}abs(area(R) - n((4*pi - 3*sqrt(3))/6)) < 10^-14")), "true");
    // independent: the lens of two unit disks at distance 1 is 2 pi/3 - sqrt(3)/2
    assert_eq!(ev("simplify(area(region_intersection(disk([0, 0], 1), disk([1, 0], 1))) - (2*pi/3 - sqrt(3)/2))"), "0");
}
