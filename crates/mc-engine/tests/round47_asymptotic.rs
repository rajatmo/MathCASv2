//! Asymptotic relations with parameters: `conditional_expression(value, region)` where the leading exponents of f and
//! g depend on parameters, the equation `c == 1` for an equivalence whose ratio tends to a parameter-dependent
//! constant, and a weak assumption `a >= c` split into the value at `a = c` and on `a > c`. Every new answer is
//! checked independently: the relation at sample parameter values inside the region (decided by the parameter-free
//! code), and the ratio |f/g| at points approaching the limit point (it falls by orders of magnitude where q -> 0 and
//! grows where |q| -> inf). Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` evaluates to the same expression as `want` (both evaluated; structural equality).
fn same(e: &mut CoreEngine, src: &str, want: &str) {
    let r = ev(e, &format!("vv47 = {src}; vv47 === ({want})"));
    assert_eq!(r, "true", "{src}: {}", ev(e, "vv47"));
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// The ratio `q` (in x, and y when given) at the parameter values `subs` falls (`to_zero`) or grows by more than a
/// factor 100 from the point `near(10)` to `near(20)`, `near(k)` being a substitution in k (x = x0 +- 10^-k at a
/// finite point, x = 10 k or 10^k at inf).
fn trend(e: &mut CoreEngine, q: &str, subs: &str, near: &str, to_zero: bool) {
    let cmp = if to_zero { "m(20) < m(10)/100" } else { "m(20) > 100*m(10)" };
    let src = format!("{{ let qq = rewrite({q}, {subs}); let m = k => abs(n(rewrite(qq, {near}), 40)); {cmp} }}");
    holds(e, &src);
}

#[test]
fn powers_at_a_finite_point() {
    timed("powers at a point", 120, || {
        let mut e = CoreEngine::new();
        let (f, g) = ("1 / (x - 1)^p", "10 / (x - 1)");
        for (name, want) in [
            ("asymptotic_equal", "false"),
            ("asymptotic_greater", "false"),
            ("asymptotic_greater_equal", "false"),
            ("asymptotic_less", "true"),
            ("asymptotic_less_equal", "true"),
        ] {
            same(&mut e, &format!("{name}({f}, {g}, x -> 1)"), &format!("conditional_expression({want}, p < 1)"));
            // inside p < 1 the parameter-free relation agrees
            for p in ["1/2", "-1", "0"] {
                same(&mut e, &format!("{name}(rewrite({f}, p -> {p}), {g}, x -> 1)"), want);
            }
        }
        // q = f/g -> 0 inside (from both sides), |q| -> inf outside (p = 2: the values there differ)
        for s in ["1", "-1"] {
            for p in ["1/2", "-1"] {
                trend(&mut e, &format!("({f})/({g})"), &format!("p -> {p}"), &format!("x -> 1 + {s}*10^-k"), true);
            }
            trend(&mut e, &format!("({f})/({g})"), "p -> 2", &format!("x -> 1 + {s}*10^-k"), false);
        }
        same(&mut e, "asymptotic_less(1/(x - 1)^2, 10/(x - 1), x -> 1)", "false");

        same(&mut e, "asymptotic_equivalent((x - 1)^p, (x - 1)^q, x -> 1)", "conditional_expression(false, p > q)");
        same(&mut e, "asymptotic_equivalent((x - 1)^2, (x - 1)^1, x -> 1)", "false");
        trend(&mut e, "(x - 1)^p/(x - 1)^q", "[p -> 2, q -> 1]", "x -> 1 - 10^-k", true);

        same(&mut e, "asymptotic_equivalent(sin(x - 2)^p, sinh(x - 2), x -> 2)", "conditional_expression(false, 0 < p and p < 1)");
        same(&mut e, "asymptotic_equivalent(sin(x - 2)^(1/2), sinh(x - 2), x -> 2)", "false");
        for s in ["1", "-1"] {
            trend(&mut e, "sin(x - 2)^p/sinh(x - 2)", "p -> 1/2", &format!("x -> 2 + {s}*10^-k"), false);
        }

        same(&mut e, "asymptotic_equal(sin(p * (x - 1)), (x - 1)^q, x -> 1)", "conditional_expression(false, p > 0 and q > 1)");
        same(&mut e, "asymptotic_equal(sin(3 * (x - 1)), (x - 1)^2, x -> 1)", "false");
        trend(&mut e, "sin(p*(x - 1))/(x - 1)^q", "[p -> 3, q -> 3/2]", "x -> 1 + 10^-k", false);
        trend(&mut e, "sin(p*(x - 1))/(x - 1)^q", "[p -> 3, q -> 3/2]", "x -> 1 - 10^-k", false);

        // the ratio tends to 1/p: the equation
        same(&mut e, "asymptotic_equivalent(1 / sinh(p * (x - 1)), 1 / (x - 1), x -> 1)", "p^-1 == 1");
        same(&mut e, "asymptotic_equivalent(1 / sinh(1 * (x - 1)), 1 / (x - 1), x -> 1)", "true");
        same(&mut e, "asymptotic_equivalent(1 / sinh(2 * (x - 1)), 1 / (x - 1), x -> 1)", "false");
    });
}

#[test]
fn exponentials_at_infinity_and_two_variables() {
    timed("exponentials and two variables", 120, || {
        let mut e = CoreEngine::new();
        let spec = "[x, y] -> [inf, 1]";
        for (name, f, g, want) in [
            ("asymptotic_equal", "sinh(x) * y", "sinh(a * x) * y^2", "false"),
            ("asymptotic_equivalent", "sinh(x) * y", "sinh(a * x) * y^2", "false"),
            ("asymptotic_greater", "sinh(x) * y", "sinh(a * x) * y^n_u", "false"),
            ("asymptotic_greater_equal", "sinh(x) * y", "sinh(a * x) * sqrt(y)", "false"),
            ("asymptotic_less", "sinh(x) * y", "sinh(a * x) * y^n_u", "true"),
            ("asymptotic_less_equal", "sinh(x) * y", "sinh(a * x) * y^2", "true"),
        ] {
            same(&mut e, &format!("{name}({f}, {g}, {spec})"), &format!("conditional_expression({want}, a > 1)"));
            // at a = 2 and a = 3/2 (n_u = 3) the relation in x alone (y -> 1 is a finite nonzero factor) agrees
            for a in ["2", "3/2"] {
                same(&mut e, &format!("{name}(sinh(x), sinh({a} * x), x -> inf)"), want);
            }
            for a in ["2", "3/2"] {
                let subs = format!("[a -> {a}, n_u -> 3]");
                trend(&mut e, &format!("({f})/({g})"), &subs, "[x -> 10*k, y -> 1 + 10^-k]", true);
            }
        }
        // outside, a = 1/2: |q| -> inf (asymptotic_less false there)
        trend(&mut e, "sinh(x)/sinh(a*x)", "a -> 1/2", "x -> 10*k", false);
        same(&mut e, "asymptotic_less(sinh(x), sinh(x/2), x -> inf)", "false");
        // exp(p x) against x at inf
        same(&mut e, "asymptotic_less(exp(p*x), x, x -> inf)", "conditional_expression(true, p < 0)");
        trend(&mut e, "exp(p*x)/x", "p -> -1/2", "x -> 10*k", true);
    });
}

#[test]
fn weak_assumptions_and_unchanged_cases() {
    timed("weak assumptions", 120, || {
        let mut e = CoreEngine::new();
        // at n_u = 0 the ratio (1 + x^0)/x^0 is 2: equivalent only for n_u > 0
        same(&mut e, "asymptotic_equivalent(1 + x^n_u, x^n_u, x -> inf, assumptions: n_u >= 0)", "conditional_expression(true, n_u > 0)");
        same(&mut e, "asymptotic_equivalent(1 + x^0, x^0, x -> inf)", "false");
        same(&mut e, "asymptotic_equivalent(1 + x^(1/2), x^(1/2), x -> inf)", "true");
        trend(&mut e, "(1 + x^n_u)/x^n_u - 1", "n_u -> 1/2", "x -> 10^k", true);
        // at a = 0, x^a = 1 is not o(1): x^a = o(1) only for a > 0
        same(&mut e, "asymptotic_greater(1, x^a, x -> 0, assumptions: a >= 0)", "conditional_expression(true, a > 0)");
        same(&mut e, "asymptotic_greater(1, x^0, x -> 0)", "false");
        same(&mut e, "asymptotic_greater(1, x^(1/2), x -> 0)", "true");
        // decided cases stay as they were
        same(&mut e, "asymptotic_less(1, x^a, x -> 0, assumptions: a >= 0)", "false");
        same(&mut e, "asymptotic_greater(1, x^a, x -> 0, assumptions: a < 0)", "false");
        same(&mut e, "asymptotic_equivalent(1 + x^n_u, x^n_u, x -> inf, assumptions: n_u < 0)", "false");
        // no parameter-free answer is invented where the leading term is not a single power
        holds(&mut e, "is_failure(asymptotic_less((x - 1)^p - (x - 1)^q, x - 1, x -> 1))");
    });
}
