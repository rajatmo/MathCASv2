//! The decomposition through an engine session: documented answers, answers checked at random points against
//! the formula they describe, quantifier elimination, the fast paths, the decision service's hook, and limits.
use mc_api::{Engine, StopFlag};

fn eval(e: &mut mc_engine::CoreEngine, s: &str) -> String {
    let stop = StopFlag::new();
    e.eval(s, &stop).unwrap().printed
}

/// The answer read back and printed again (the printed form of the canonical term, so that equal answers compare equal).
fn norm(e: &mut mc_engine::CoreEngine, s: &str) -> String {
    let a = eval(e, s);
    eval(e, &a)
}

#[test]
fn cylindrical_answers() {
    let mut e = mc_engine::CoreEngine::new();
    let cases = [
        ("reduce(x^2 + y^2 < 1, [x, y])", "(-1 < x and x < 1) and (-(1 - x^2)^(1/2) < y and y < (1 - x^2)^(1/2))"),
        ("reduce(x^2 - 7*y^2 == 1 and x > 0 and y > 0, [x, y], reals)", "x > 1 and y == (1/7)*7^(1/2)*(-1 + x^2)^(1/2)"),
        (
            "reduce(x^2 + y^2 < 1 and y > x, [x, y])",
            "(-1 < x and x <= (-1/2)*2^(1/2)) and (-(1 - x^2)^(1/2) < y and y < (1 - x^2)^(1/2)) or ((-1/2)*2^(1/2) < x and x < (1/2)*2^(1/2)) and (x < y and y < (1 - x^2)^(1/2))",
        ),
        (
            "reduce(x^2 + y^2 < 2 and x - y >= 1, [x, y])",
            "((1/2)*(1 - 3^(1/2)) < x and x < (1/2)*(1 + 3^(1/2))) and (-(2 - x^2)^(1/2) < y and y <= -1 + x) or x == (1/2)*(1 + 3^(1/2)) and (-(2 - (1/4)*(1 + 3^(1/2))^2)^(1/2) < y and y < -1 + (1/2)*(1 + 3^(1/2))) or ((1/2)*(1 + 3^(1/2)) < x and x < 2^(1/2)) and (-(2 - x^2)^(1/2) < y and y < (2 - x^2)^(1/2))",
        ),
        ("reduce(exists([x, y], x^2 + a*y^2 <= 1 and x - y >= 2), a)", "a <= 1/3"),
        (
            "cylindrical_decomposition(x^2 + y^2 + z^2 < 1, [x, y, z])",
            "(-1 < x and x < 1) and (-(1 - x^2)^(1/2) < y and y < (1 - x^2)^(1/2)) and (-(1 - x^2 - y^2)^(1/2) < z and z < (1 - x^2 - y^2)^(1/2))",
        ),
        ("cylindrical_decomposition(exists(y, x^2 + y^2 < 1 and y > x), x)", "-1 < x and x < (1/2)*2^(1/2)"),
        (
            "cylindrical_decomposition(y^2 == x*(-1 + x^4), [x, y])",
            "x == -1 and y == 0 or (-1 < x and x < 0) and (y == -(-x + x^5)^(1/2) or y == (-x + x^5)^(1/2)) or x == 0 and y == 0 or x == 1 and y == 0 or x > 1 and (y == -(-x + x^5)^(1/2) or y == (-x + x^5)^(1/2))",
        ),
        ("cylindrical_decomposition((x - 1)*(x - 2)^2*(x - 3)*(x - 4)*(x - 5) > 0 and y == y, [x, y])", "x < 1 or 3 < x and x < 4 or x > 5"),
    ];
    for (i, w) in cases {
        let want = eval(&mut e, w);
        assert_eq!(norm(&mut e, i), want, "{i}");
    }
}

#[test]
fn quantifier_elimination() {
    let mut e = mc_engine::CoreEngine::new();
    let cases = [
        ("resolve(for_all(x, x^2 + 1 > 0))", "true"),
        ("resolve(exists([x, y], x^2 + y^2 < 1))", "true"),
        ("resolve(exists(x, x^7 + 3*x - 11 == 0))", "true"),
        ("resolve(exists(x, x^4 + 3*x + 11 <= 0), reals)", "false"),
        ("resolve(exists([x, y], x^2 + y^2 <= 11 and x^3 - y^2 == 2 and x*y > 3), reals)", "true"),
        ("resolve(for_all([a, b, c], exists(x, a*x^2 + b*x + c >= 0)), reals)", "false"),
        ("resolve(for_all([b, c], exists(x, x^2 + b*x + c >= 0)), reals)", "true"),
        ("resolve(for_all([a, b], a > 0 and b > 0, (a + b)/2 >= sqrt(a*b)))", "true"),
        ("resolve(exists([p, q], not implies(p and implies(p, q), q)))", "false"),
        // parameters: sign conditions
        (
            "resolve(exists([x, y], x != y and a*x^2 + b*x + c == 0 and a*y^2 + b*y + c == 0), reals)",
            "a == 0 and b == 0 and c == 0 or a != 0 and -b^2 + 4*a*c < 0",
        ),
        (
            "reduce(exists([x, y], x != y and a*x^2 + b*x + c == 0 and a*y^2 + b*y + c == 0), [a, b, c], reals)",
            "a < 0 and c > b^2/(4*a) or a == 0 and b == 0 and c == 0 or a > 0 and c < b^2/(4*a)",
        ),
        ("resolve(for_all(x, implies(a != 0 and a*x^2 + b*x + c == 0, x^2 >= 0)), reals)", "true"),
        // the complex numbers
        ("resolve(exists(x, (a^2 - b^3 + 1)*x^7 + (a^2 - a*b + 1)*x^2 - 3 == 0))", "1 - a*b + a^2 != 0 or 1 - b^3 + a^2 != 0"),
        ("resolve(for_all(c, exists(x, a*x^2 + b*x + c == 0)))", "a != 0 or b != 0"),
        ("resolve(for_all([a, b, c], exists(x, a*x^2 + b*x + c == 0)))", "false"),
        ("resolve(for_all(x, x^2 - x + 1 != 0))", "false"),
        // unresolved formulas come back with listed variables
        ("resolve(exists(x, 1 < x and x < 2, gamma(x) == x + 1), reals)", "exists([x], 1 < x and x < 2, gamma(x) == 1 + x)"),
        // the inert forms
        ("for_all(x, y == 0)", "y == 0"),
        ("not(for_all(x, x == a))", "exists(x, x != a)"),
    ];
    for (i, w) in cases {
        let want = eval(&mut e, w);
        assert_eq!(norm(&mut e, i), want, "{i}");
    }
}

#[test]
fn linear_virtual_substitution() {
    let mut e = mc_engine::CoreEngine::new();
    // (a, b) = (-1, -1): the line x + y == 1 misses the half plane x + y <= -1
    let r = eval(&mut e, "resolve(exists([x, y], a*x + b*y >= 1 and (a^2 - b)*x - (a - b^2)*y == 2), reals)");
    for (pt, want) in [("a -> -1, b -> -1", "false"), ("a -> 0, b -> 0", "false"), ("a -> 1, b -> 1", "false"), ("a -> 2, b -> 3", "true"), ("a -> -3, b -> 1/2", "true")] {
        assert_eq!(eval(&mut e, &format!("rewrite({r}, [{pt}])")), want, "{pt} in {r}");
    }
    let want = eval(&mut e, "a != 0 and b == 0 or a + b <= 0 and -a*b + a^2 + b^2 != 0");
    assert_eq!(norm(&mut e, "resolve(for_all(c, exists([x, y], a*x + b*y + c == 0 and c*x + a*y >= b)), reals)"), want);
}

/// A small deterministic generator.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: i64) -> i64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as i64).rem_euclid(n)
    }
}

/// Every answer of `reduce` agrees with its formula at random rational points (decision CAD.7).
#[test]
fn answers_hold_at_random_points() {
    let mut e = mc_engine::CoreEngine::new();
    let mut g = Lcg(7);
    let rels = ["<", "<=", ">", ">=", "=="];
    let mut checked = 0;
    for _ in 0..60 {
        let mut poly = || {
            let c: Vec<i64> = (0..6).map(|_| g.next(7) - 3).collect();
            format!("({}*x^2 + {}*y^2 + {}*x*y + {}*x + {}*y + {})", c[0], c[1], c[2], c[3], c[4], c[5])
        };
        let (p, q) = (poly(), poly());
        let r1 = rels[g.next(4) as usize];
        let r2 = rels[g.next(5) as usize];
        let join = if g.next(2) == 0 { "and" } else { "or" };
        let f = format!("{p} {r1} 0 {join} {q} {r2} 0");
        let ans = eval(&mut e, &format!("reduce({f}, [x, y], reals)"));
        if ans.starts_with("reduce(") || ans.starts_with("failure(") {
            continue;
        }
        for _ in 0..12 {
            let (x, y) = (format!("{}/{}", g.next(41) - 20, g.next(5) + 1), format!("{}/{}", g.next(41) - 20, g.next(5) + 1));
            let a = eval(&mut e, &format!("rewrite({f}, [x -> {x}, y -> {y}])"));
            let b = eval(&mut e, &format!("rewrite({ans}, [x -> {x}, y -> {y}])"));
            if (a == "true" || a == "false") && (b == "true" || b == "false") {
                assert_eq!(a, b, "{f} at ({x}, {y}): {ans}");
                checked += 1;
            }
        }
    }
    assert!(checked > 250, "only {checked} points decided");
}

#[test]
fn find_instance_over_the_reals() {
    let mut e = mc_engine::CoreEngine::new();
    for (f, vars) in [
        ("x^2 + y^2 < 2 and (x - 1)^2 + (y - 1)^2 < 2", "[x, y]"),
        ("x^2 + y^2 + z^2 <= 1 and 9*z^3 == 2*x - 5*y - 7", "[x, y, z]"),
        ("x^2 - 2*y + z^2 >= 1", "[x, y, z]"),
        ("x^2 + y*z == 1 and x + 2*y <= 3*z + 1 and x*y*z > 7", "[x, y, z]"),
    ] {
        let r = eval(&mut e, &format!("find_instance({f}, {vars}, reals)"));
        assert!(r.starts_with("[["), "{f}: {r}");
        assert_eq!(eval(&mut e, &format!("rewrite({f}, first({r}))")), "true", "{f}: {r}");
    }
    assert_eq!(eval(&mut e, "find_instance(x^2 + y^2 < 0, [x, y], reals)"), "[]");
    assert_eq!(eval(&mut e, "find_instance(x^2 + y^2 + z^2 <= 1 and 9*z^3 == 2*x - 5*y - 7, [x, y, z], reals)"), "[[x -> 0, y -> -41/64, z -> -3/4]]");
}

#[test]
fn decision_service_in_several_variables() {
    let mut e = mc_engine::CoreEngine::new();
    // positivity of a quartic on a disk: no quadratic form decides it, the decomposition does
    assert_eq!(eval(&mut e, "assuming(x^2 + y^2 < 1, simplify(sign(2 - x^4 - y^4 - x*y)))"), "1");
    assert_eq!(eval(&mut e, "refine(x*y < 1, x^2 + y^2 < 1 and element(x | y, reals))"), "true");
}

/// The fast paths and the projection operators against full decompositions (ignored: a measurement; run with
/// `--ignored --nocapture`).
#[test]
#[ignore]
fn measurements() {
    let mut e = mc_engine::CoreEngine::new();
    let problems = [
        ("x^2 + y^2 < 1 and y > x", "[x, y]"),
        ("x^2 + y^2 + z^2 < 1", "[x, y, z]"),
        ("x^4 >= x^2 + y^2", "[x, y]"),
        ("y^2 == x*(-1 + x^4)", "[x, y]"),
        ("exists([x, y], x != y and a*x^2 + b*x + c == 0 and a*y^2 + b*y + c == 0)", "[a, b, c]"),
        ("for_all(x, a*x^2 + b*x + c > 0)", "[a, b, c]"),
        ("exists([x, y], a*x + b*y >= 1 and (a^2 - b)*x - (a - b^2)*y == 2)", "[a, b]"),
        ("for_all(c, exists([x, y], a*x + b*y + c == 0 and c*x + a*y >= b))", "[a, b]"),
        ("exists([x, y], x^2 + y^2 <= 11 and x^3 - y^2 == 2 and x*y > 3)", "[]"),
        ("exists([x, y, z], x + y + z <= 1 and x - y >= 0 and z >= 2*x and y + 3*z >= 4)", "[]"),
        ("exists(y, x*y^2 + y + z == 0 and x^2 + z^2 <= 4)", "[x, z]"),
    ];
    println!("| problem | Lazard factors/cells/us | Brown | Brown + VS |");
    let status = std::env::var("MC_STATUS_DIR").ok().map(|d| std::path::PathBuf::from(d).join("cad-measure.json"));
    for (k, (f, v)) in problems.iter().enumerate() {
        if let Some(p) = &status {
            let _ = std::fs::write(p, format!(r#"{{"job":"cad-measure","owner":"mc-cad","title":"CAD measurements","done":{k},"total":{},"unit":"problems","state":"running"}}"#, problems.len()));
        }
        let mut row = format!("| `{f}` |");
        for (m, vs) in [("lazard", "false"), ("brown", "false"), ("brown", "true")] {
            let r = eval(&mut e, &format!("`cad$stats`({f}, {v}, \"{m}\", {vs})"));
            row.push_str(&format!(" {r} |"));
        }
        println!("{row}");
    }
    if let Some(p) = &status {
        let _ = std::fs::write(p, format!(r#"{{"job":"cad-measure","owner":"mc-cad","title":"CAD measurements","done":{0},"total":{0},"unit":"problems","state":"done"}}"#, problems.len()));
    }
}
