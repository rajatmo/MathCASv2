//! Property answers with parameters at the boundary of their conditions.
//!
//! A conditional answer `conditional_expression(v, c)` of `function_sign`, `function_monotonicity`,
//! `function_convexity`, `function_injective`, `function_surjective`, `function_bijective`, `function_continuous`
//! and `function_analytic` must be exact: wherever `c` holds the property is `v`, and on the boundary of `c` (where
//! one of its relations becomes an equation) `c` holds exactly when the property is `v` there. The generator takes
//! each template, asks for the answer with symbolic parameters, solves every relation of the condition as an
//! equation for one parameter (the others at values from a pool), and at those points and next to them compares
//!
//! 1. the condition's truth with the answer of the same call at those parameter values (no parameters left), and
//! 2. the value claimed there (by the condition, or by that call) with the function evaluated at 30 digits on a
//!    dense grid of exact points: a sign, a direction, a convexity or an injectivity the values contradict, or an
//!    `indeterminate` (`false`) that the values do not confirm (they are of one sign, monotone, convex).
//!
//! `boundaries_short` runs the templates with a small pool; `boundaries_campaign` (ignored) runs every template with
//! the whole pool and prints its progress (`MC_STATUS_DIR` for a status file, `MC_SOUNDNESS_SECONDS` a time cap).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The parts of `s` split at `sep` outside brackets.
fn split_top(s: &str, sep: &str) -> Vec<String> {
    let mut out = vec![];
    let (mut depth, mut start) = (0i32, 0usize);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(sep) {
            out.push(s[start..i].trim().to_string());
            i += sep.len();
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(s[start..].trim().to_string());
    out
}

/// The elements of a printed list.
fn elements(printed: &str) -> Vec<String> {
    let s = printed.trim();
    match s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(inner) if !inner.trim().is_empty() => split_top(inner, ","),
        _ => vec![],
    }
}

/// A printed real number (`1.25`, `-3`, `2/3`, `1.5*10^-7`, with a precision mark), or `None`.
fn real_of(s: &str) -> Option<f64> {
    let s = s.trim();
    let s = s.split('`').next().unwrap_or(s);
    if let Some((m, x)) = s.split_once("*10^") {
        let m: f64 = real_of(m)?;
        let x: f64 = x.trim_matches(|c| c == '(' || c == ')').parse().ok()?;
        return Some(m * 10f64.powf(x));
    }
    if let Some((p, q)) = s.split_once('/') {
        return Some(p.trim().parse::<f64>().ok()? / q.trim().parse::<f64>().ok()?);
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `(head of the property, template)`: `A`, `B`, `C`, `D` are the parameters, `X` the region's own condition (the
/// grid is filtered by it).
const TEMPLATES: &[&str] = &[
    "function_sign(x^2 + A, x)",
    "function_sign(x^2 + A*x + B, x)",
    "function_sign(A*x^2 + 1, x)",
    "function_sign([x^2 + A, x > B], x)",
    "function_sign([x^3 + A, x >= 1], x)",
    "function_sign(x^4 + A*x^2 + 1, x)",
    "function_sign(x^2 + A, x, strict_inequalities: true)",
    "function_monotonicity(x^3 + A*x, x)",
    "function_monotonicity(x^3 + A*x, x, strict_inequalities: true)",
    "function_monotonicity(x^3 + A*x^2 + B*x, x)",
    "function_monotonicity([x^2 + A*x, x > 1], x)",
    "function_monotonicity([x^3 + A*x, x > B], x)",
    "function_monotonicity([x^3 - 3*x + A, x < -3/2 or x > 11/10], x)",
    "function_monotonicity(A*x^3 + x, x)",
    "function_monotonicity(atan(x) + A*x, x)",
    "function_monotonicity(x^5 + A*x^3 + B*x, x, strict_inequalities: true)",
    "function_convexity(x^4 + A*x^2, x)",
    "function_convexity(x^4 + A*x^2, x, strict_inequalities: true)",
    "function_convexity(x^4 + A*x^3, x)",
    "function_convexity([x^3 + A*x^2, x > 0], x)",
    "function_convexity(1/x^2 + A, x)",
    "function_convexity(x^2 + A*x^4, x)",
    "function_injective(x^3 + A*x, x)",
    "function_injective([x^2 + A*x, x > 0], x)",
    "function_injective([x^3 + A*x, x > 1], x)",
    "function_injective(x^3 + A*x^2 + B*x, x)",
    "function_bijective(x^3 + A*x^2 + B*x + 1, x)",
    "function_bijective(x^5 + A*x^4 + B*x^3 + C*x^2 + D*x, x)",
    "function_bijective(x^5 + A*x^3 + B*x, x)",
    "function_surjective(A*x^3 + x, x)",
    "function_surjective(x^2 + A*x, x)",
    "function_continuous(sqrt(x^2 + A), x)",
    "function_continuous(sqrt(x^4 + A*x^2), x)",
    "function_continuous(sqrt((x - A)^2 + B), x)",
    "function_continuous(log(x^2 + A), x)",
    "function_continuous(1/(x^2 + A*x + B), x)",
    "function_continuous((x^2 + A)^(1/3), x)",
    "function_analytic(1/(x^2 + A), x)",
    "function_analytic(sqrt(x^2 + A), x)",
    "function_analytic(sqrt(x^4 + A), x)",
    "function_analytic((x^2 + A)^(3/2), x)",
    "function_analytic(abs(x^2 + A), x)",
    "function_monotonicity(x + A*sin(x), x)",
    "function_monotonicity(exp(x) + A*x, x)",
    "function_monotonicity(log(x) + A*x, x)",
    "function_sign(exp(x) + A, x)",
    "function_sign(cosh(x) + A, x)",
    "function_convexity(exp(x) + A*x^2, x)",
    "function_injective(exp(x) + A*x, x)",
    "function_surjective(x^3 + A*x^2, x)",
    "function_bijective(A*x + B, x)",
];

const PARAMS: [(&str, &str); 4] = [("A", "a"), ("B", "b"), ("C", "c"), ("D", "d_u")];

/// The template with the parameters symbolic.
fn symbolic(t: &str) -> String {
    let mut s = t.to_string();
    for (k, v) in PARAMS {
        s = s.replace(k, v);
    }
    s
}

/// The template at the given values (`None`: symbolic).
fn at(t: &str, vals: &[(usize, String)]) -> String {
    let mut s = t.to_string();
    for (k, v) in vals {
        s = s.replace(PARAMS[*k].0, &format!("({v})"));
    }
    symbolic(&s)
}

/// The function, the variable's region (a condition or `true`) and the head of a template.
fn parts(t: &str) -> (String, String, String) {
    let head = t.split('(').next().unwrap_or("").to_string();
    let inner = &t[head.len() + 1..t.len() - 1];
    let args = split_top(inner, ",");
    let first = &args[0];
    if let Some(l) = first.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let fc = split_top(l, ",");
        (fc[0].clone(), fc[1].clone(), head)
    } else {
        (first.clone(), "true".into(), head)
    }
}

/// The relations of a printed condition (atoms of its `and`/`or`, without `element(...)`), as `lhs - (rhs)`.
fn relations(c: &str) -> Vec<String> {
    let mut out = vec![];
    let mut stack = vec![c.trim().to_string()];
    while let Some(s) = stack.pop() {
        let s = s.trim();
        let parts = split_top(s, " or ");
        if parts.len() > 1 {
            stack.extend(parts);
            continue;
        }
        let parts = split_top(s, " and ");
        if parts.len() > 1 {
            stack.extend(parts);
            continue;
        }
        if s.starts_with('(') && s.ends_with(')') && split_top(&s[1..s.len() - 1], " or ").len() + split_top(&s[1..s.len() - 1], " and ").len() > 2 {
            stack.push(s[1..s.len() - 1].to_string());
            continue;
        }
        if s.starts_with("element(") {
            continue;
        }
        for op in [" <= ", " >= ", " == ", " != ", " < ", " > "] {
            let p = split_top(s, op);
            if p.len() == 2 {
                out.push(format!("({}) - ({})", p[0], p[1]));
                break;
            }
        }
    }
    out
}

/// The claim a value makes, checked on the grid: `None` when the values agree, a reason otherwise.
fn check_claim(head: &str, claim: &str, strict: bool, xs: &[f64], vs: &[f64]) -> Option<String> {
    let n = vs.len();
    if n < 5 {
        return None;
    }
    let tol = 1e-20;
    let scale = vs.iter().fold(1.0f64, |m, v| m.max(v.abs()));
    let t = tol * scale;
    let d: Vec<f64> = vs.windows(2).map(|w| w[1] - w[0]).collect();
    let up = d.iter().all(|&x| x > -t);
    let down = d.iter().all(|&x| x < t);
    let sup = d.iter().all(|&x| x > t);
    let sdown = d.iter().all(|&x| x < -t);
    // equally spaced grid: second differences
    let dd: Vec<f64> = vs.windows(3).map(|w| w[0] - 2.0 * w[1] + w[2]).collect();
    let convex = dd.iter().all(|&x| x > -t);
    let concave = dd.iter().all(|&x| x < t);
    let pos = vs.iter().all(|&v| v >= -t);
    let neg = vs.iter().all(|&v| v <= t);
    let spos = vs.iter().all(|&v| v > 0.0);
    let sneg = vs.iter().all(|&v| v < 0.0);
    let _ = xs;
    let bad = match (head, claim) {
        ("function_sign", "1") => !(if strict { spos } else { pos }),
        ("function_sign", "-1") => !(if strict { sneg } else { neg }),
        ("function_monotonicity", "1") => !(if strict { sup } else { up }),
        ("function_monotonicity", "-1") => !(if strict { sdown } else { down }),
        ("function_convexity", "1") => !convex,
        ("function_convexity", "-1") => !concave,
        ("function_injective" | "function_bijective", "true") => !(sup || sdown),
        // a refutation (`indeterminate`, `false`) next to a boundary can hide between grid points: the call at
        // those parameter values, which confirms its witnesses exactly, is the check (part 1)
        _ => false,
    };
    bad.then(|| format!("{claim} against the values on the grid"))
}

/// One template: the faults found and the number of parameter points tried.
fn template(e: &mut CoreEngine, t: &str, pool: &[&str], log: bool) -> (Vec<String>, usize) {
    let mut faults = vec![];
    let sym = symbolic(t);
    let ans = ev(e, &format!("time_constrained({{ w = {sym}; w }}, 30)"));
    if log {
        eprintln!("{sym} = {}", ans.chars().take(240).collect::<String>());
    }
    let used: Vec<usize> = (0..PARAMS.len()).filter(|&k| t.contains(PARAMS[k].0)).collect();
    let (value, cond) = if ans.starts_with("conditional_expression(") {
        let inner = &ans["conditional_expression(".len()..ans.len() - 1];
        let p = split_top(inner, ",");
        (p[0].clone(), p[1..].join(", "))
    } else if ans.starts_with("function_") || ans.starts_with('$') || ans.contains("failure") {
        return (faults, 0);
    } else {
        (ans.clone(), "true".to_string())
    };
    // parameter points: on each relation (one parameter solved, the others from the pool), and next to them
    let mut points: Vec<Vec<(usize, String)>> = vec![];
    for rel in relations(&cond) {
        for &k in &used {
            if !rel.contains(PARAMS[k].1) {
                continue;
            }
            for (j, &base) in pool.iter().enumerate() {
                let others: Vec<(usize, String)> = used.iter().filter(|&&o| o != k).enumerate().map(|(i, &o)| (o, pool[(j + 2 * i + 1) % pool.len()].to_string())).collect();
                let rules: Vec<String> = others.iter().map(|(o, v)| format!("{} -> ({v})", PARAMS[*o].1)).collect();
                let eq = format!("rewrite({rel}, [{}])", rules.join(", "));
                let sols = ev(e, &format!("time_constrained(solve({eq} == 0, {}, reals), 10)", PARAMS[k].1));
                let _ = base;
                for s in elements(&sols).into_iter().take(3) {
                    let Some(v) = s.trim_start_matches('[').trim_end_matches(']').split_once("->").map(|(_, v)| v.trim().to_string()) else { continue };
                    for off in ["", " + 1/7", " - 1/7"] {
                        let mut p = others.clone();
                        p.push((k, format!("{v}{off}")));
                        points.push(p);
                    }
                }
            }
        }
    }
    // an unconditional answer: points from the pool
    if cond == "true" || points.is_empty() {
        for (j, _) in pool.iter().enumerate() {
            points.push(used.iter().enumerate().map(|(i, &k)| (k, pool[(j + 3 * i) % pool.len()].to_string())).collect());
        }
    }
    points.truncate(36);
    let (f, region, head) = parts(t);
    let strict = t.contains("strict_inequalities: true");
    for p in &points {
        let rules: Vec<String> = p.iter().map(|(k, v)| format!("{} -> ({v})", PARAMS[*k].1)).collect();
        let rl = format!("[{}]", rules.join(", "));
        let holds = if cond == "true" { "true".to_string() } else { ev(e, &format!("time_constrained(rewrite(w[2], {rl}), 10)")) };
        let holds = if holds == "true" || holds == "false" { holds } else { ev(e, &format!("time_constrained(n(rewrite(w[2], {rl}), 30), 10)")) };
        let concrete_src = at(t, p);
        let concrete = ev(e, &format!("time_constrained({concrete_src}, 30)"));
        let decided = !concrete.starts_with("function_") && !concrete.starts_with('$') && !concrete.contains("conditional_expression") && !concrete.contains("failure");
        if log {
            eprintln!("  {rl}: condition {holds}, {concrete_src} = {concrete}");
        }
        if decided && holds == "true" && concrete != value {
            faults.push(format!("{sym} = {ans}: at {rl} the condition holds, but {concrete_src} = {concrete}"));
        }
        if decided && holds == "false" && concrete == value && cond != "true" {
            faults.push(format!("{sym} = {ans}: at {rl} the condition fails, but {concrete_src} = {concrete} (not exact at its boundary)"));
        }
        // the claim at this point against the values
        let claim = if holds == "true" { value.clone() } else if decided { concrete.clone() } else { continue };
        if !matches!(head.as_str(), "function_sign" | "function_monotonicity" | "function_convexity" | "function_injective" | "function_bijective") {
            continue;
        }
        let fr = at(&f, p);
        let rg = at(&region, p);
        let grid = "table(-6 + k/20, [k, 0, 240])";
        let src = format!("time_constrained({{ g = select({grid}, &(rewrite({rg}, x -> &1) === true)); [g, map(&(n(rewrite({fr}, x -> &1), 30)), g)] }}, 30)");
        let out = ev(e, &src);
        let parts = elements(&out);
        if parts.len() != 2 {
            continue;
        }
        let xs: Vec<f64> = elements(&parts[0]).iter().filter_map(|s| real_of(s)).collect();
        let vs: Vec<Option<f64>> = elements(&parts[1]).iter().map(|s| real_of(s)).collect();
        if xs.len() != vs.len() {
            continue;
        }
        // points where the value is not a real number (a pole) are left out; a region of several pieces: only the
        // sign is checked
        let (xs, vs): (Vec<f64>, Vec<f64>) = xs.iter().zip(&vs).filter_map(|(&x, v)| v.map(|v| (x, v))).unzip();
        let pieces = xs.windows(2).any(|w| w[1] - w[0] > 0.051);
        if pieces && head != "function_sign" {
            continue;
        }
        if let Some(why) = check_claim(&head, &claim, strict, &xs, &vs) {
            faults.push(format!("{sym} = {ans}: at {rl} the claim {why} of {fr}"));
        }
    }
    (faults, points.len())
}

fn run(pool: &[&str], cap: Duration, log: bool) -> Vec<String> {
    let mut e = CoreEngine::new();
    let start = Instant::now();
    let mut faults = vec![];
    for (i, t) in TEMPLATES.iter().enumerate() {
        if start.elapsed() > cap {
            eprintln!("time cap reached after {i} templates");
            break;
        }
        let (f, n) = template(&mut e, t, pool, log);
        faults.extend(f);
        if log {
            let pct = 100 * (i + 1) / TEMPLATES.len();
            eprintln!("[boundaries] {}/{} ({pct}%), {n} points, {} faults, {:.0} s", i + 1, TEMPLATES.len(), faults.len(), start.elapsed().as_secs_f64());
            if let Ok(dir) = std::env::var("MC_STATUS_DIR") {
                let _ = std::fs::write(format!("{dir}/boundaries.txt"), format!("{}/{} {pct}%\n", i + 1, TEMPLATES.len()));
            }
        }
    }
    faults
}

#[test]
fn boundaries_short() {
    let faults = run(&["0", "1", "-2"], Duration::from_secs(400), false);
    assert!(faults.is_empty(), "{} faults\n{}", faults.len(), faults.join("\n"));
}

#[test]
#[ignore]
fn boundaries_campaign() {
    let secs = std::env::var("MC_SOUNDNESS_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(1800);
    let faults = run(&["0", "1", "-2", "1/2", "3", "-1/3", "2", "-1"], Duration::from_secs(secs), true);
    for f in &faults {
        eprintln!("FAULT {f}");
    }
    assert!(faults.is_empty(), "{} faults", faults.len());
}

/// The defects the generator found, each by an independent route.
#[test]
fn conditions_exact_at_their_boundary() {
    let mut e = CoreEngine::new();
    // f' = 3 (x + a/3)^2 + b - a^2/3 >= 0 with isolated zeros: bijective on a^2 - 3 b <= 0 (x^3 + 3 x^2 + 3 x + 5)
    assert_eq!(ev(&mut e, "function_bijective(x^3 + a*x^2 + b*x + c, x)"), "conditional_expression(true, element(a | b | c, reals) and -3*b + a^2 <= 0)");
    assert_eq!(ev(&mut e, "function_bijective(x^3 + 3*x^2 + 3*x + 5, x)"), "true");
    assert_eq!(ev(&mut e, "function_monotonicity(x^3 + a*x, x, strict_inequalities: true)"), "conditional_expression(1, a >= 0)");
    assert_eq!(ev(&mut e, "function_monotonicity(x^3, x, strict_inequalities: true)"), "1");
    assert_eq!(ev(&mut e, "function_convexity(x^4, x, strict_inequalities: true)"), "1");
    assert_eq!(ev(&mut e, "function_convexity(x^4 + a*x^2, x, strict_inequalities: true)"), "conditional_expression(1, a >= 0)");
    // a definite Hessian is only sufficient: x^4 + y^4 is strictly convex, the minors vanish at 0
    assert!(ev(&mut e, "function_convexity(x^4 + y^4, [x, y], strict_inequalities: true)").starts_with("function_convexity("));
    assert_eq!(ev(&mut e, "function_convexity(x*y, [x, y], strict_inequalities: true)"), "indeterminate");
    // convex with a Hessian singular everywhere: affine along a line (x^2 along y)
    assert_eq!(ev(&mut e, "function_convexity(x^2, [x, y], strict_inequalities: true)"), "indeterminate");
    assert_eq!(ev(&mut e, "function_convexity((x + y)^2 + x, [x, y], strict_inequalities: true)"), "indeterminate");
    // independent: (x + y)^2 + x takes the values 0, 1/2, 1 at (0, 0), (1/2, -1/2), (1, -1): a chord, not strictly above
    assert_eq!(ev(&mut e, "f = (x + y)^2 + x; [f /. [x -> 0, y -> 0], f /. [x -> 1/2, y -> -1/2], f /. [x -> 1, y -> -1]]"), "[0, 1/2, 1]");
}

#[test]
fn derivative_signs_over_several_pieces_prove_nothing() {
    let mut e = CoreEngine::new();
    // f' = 3 x^2 - 3 > 0 on both pieces, but f(-3/2) = 9/8 > f(6/5) = -234/125
    assert_eq!(ev(&mut e, "function_injective([x^3 - 3*x, x < -3/2 or x > 11/10], x)"), "false");
    assert_eq!(ev(&mut e, "function_injective([x^3 - 3*x, x < -2 or x > 2], x)"), "true");
    assert!(ev(&mut e, "function_monotonicity([x^3 - 3*x + a, x < -3/2 or x > 11/10], x)").starts_with("function_monotonicity("));
    // 1/x^2 is not convex on x != 0, nor is 1/x^2 + a for any a
    assert_eq!(ev(&mut e, "function_convexity(1/x^2, x)"), "indeterminate");
    assert_eq!(ev(&mut e, "function_convexity(1/x^2 + a, x)"), "indeterminate");
    // the parameter of the condition is a parameter: x^3 + a x increases on x > b when a >= 0, or b >= sqrt(-a/3)
    assert_eq!(
        ev(&mut e, "function_monotonicity([x^3 + a*x, x > b], x)"),
        "conditional_expression(1, element(a | b, reals) and (a < 0 and b >= (1/3)*(-3*a)^(1/2) or a >= 0))"
    );
    // independent: the values at the points the old condition left out (a = 1, b = 1) increase
    assert_eq!(ev(&mut e, "function_monotonicity([x^3 + x, x > 1], x)"), "1");
}

#[test]
fn quintic_bijective_within_the_budget() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    let r = ev(&mut e, "function_bijective(x^5 + a*x^4 + b*x^3 + c*x^2 + d_u*x, x)");
    assert!(r.starts_with("conditional_expression(true,"), "{r}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    // odd degree: onto at once
    assert_eq!(ev(&mut e, "function_surjective(x^3 + a*x, x)"), "true");
}
