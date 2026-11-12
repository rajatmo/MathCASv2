//! The decision service through the functions that use it: each tier on questions only it settles, questions that
//! must stay unknown, conditional results and their lifting, and the Rust API of the decider and the condition
//! accumulator.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn run(e: &mut CoreEngine, src: &str) -> String {
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn table(rows: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    let mut bad = vec![];
    for (src, want) in rows {
        let got = run(&mut e, src);
        if got != *want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} wrong:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn tier_structure_and_facts() {
    table(&[
        ("refine(abs(x), x < 0)", "-x"),
        ("assuming(x > 0, refine(sqrt(x^2)))", "x"),
        ("simplify(element(euler_phi(n)^3, integers), element(n, integers))", "true"),
        ("simplify(element(euler_phi(n)^2 + 1, positive_integers), element(n, integers))", "true"),
        ("refine(element((2*x + x^p)/(x*gamma(x + 2)), reals), x > 0 and p > 0)", "true"),
        ("refine(floor(2*a + 1), element(a, integers))", "1 + 2*a"),
        // closed parts by ball refinement
        ("refine(sign(pi - 3 + x), x > 0)", "1"),
    ]);
}

#[test]
fn tier_groebner() {
    table(&[
        ("refine(a^2 - b^2 + 1 == 0, a + b == 0)", "false"),
        ("refine(sign(x^2 + y^2 - 2), x^2 + y^2 == 1)", "-1"),
        ("refine(x*y == 1, x == 2 and y == 1/2)", "true"),
    ]);
}

#[test]
fn tier_balls_and_quadratic_forms() {
    table(&[
        // sin(x) + x^2 - 6 < 0 on (1, 2): a ball enclosure of the box
        ("assuming(x > 1 and x < 2, refine(sign(sin(x) + x^2 - 6)))", "-1"),
        // exp(x) - 3 x changes sign on (0, 2) but not on (0, 1/2): the pieces of the interval
        ("assuming(x > 0 and x < 1/2, refine(exp(x) - 3*x > 0))", "true"),
        ("assuming(x > 0 and x < 2, refine(exp(x) - 3*x > 0))", "-3*x + exp(x) > 0"),
        ("refine(sign(x^2 - x*y + y^2 + 1), element(x | y, reals))", "1"),
        ("refine(a^2 - a*b + b^2 >= 0, element(a | b, reals))", "true"),
        ("refine(a^2 - 3*a*b + b^2 >= 0, element(a | b, reals))", "-3*a*b + a^2 + b^2 >= 0"),
        // a quadratic form of complex variables is not decided
        ("refine(sign(x^2 + y^2 + 1))", "sign(1 + x^2 + y^2)"),
    ]);
}

#[test]
fn tier_root_isolation_and_reduce() {
    table(&[
        // (x - 1)(x - 2)(x - 3) - 10 < 0 on (0, 3)
        ("assuming(x > 0 and x < 3, refine(sign(x^3 - 6*x^2 + 11*x - 16)))", "-1"),
        // x^2 - 2 has a root in (1, 2)
        ("assuming(x > 1 and x < 2, refine(sign(x^2 - 2)))", "sign(-2 + x^2)"),
        ("assuming(x > 3/2 and x < 2, refine(sign(x^2 - 2)))", "1"),
        // (x - 1)^2 touches 0 at 1: nonnegative but not positive
        ("assuming(element(x, reals), refine(x^2 - 2*x + 1 >= 0))", "true"),
        ("assuming(element(x, reals), refine(x^2 - 2*x + 1 > 0))", "1 + -2*x + x^2 > 0"),
        // a polynomial fact on x: through reduce
        ("assuming(element(x, reals) and x^2 < 1, refine(x < 2))", "true"),
        ("assuming(element(x, reals) and x^2 < 1, refine(x^3 - 3 < 0))", "true"),
        // the variables of a polynomial inequality are real (decision F3.11)
        ("assuming(x^2 < 1, refine(x < 2))", "true"),
        // a non-polynomial inequality does not make its variables real
        ("assuming(exp(x) < 1, refine(x < 2))", "x < 2"),
        // the S-procedure: a quadratic on the set of one quadratic fact
        ("refine((x - 1)^2 + (y - 2)^2 < 3/2, x^2 + y^2 <= 1)", "false"),
        ("refine(x + y < 2, x^2 + y^2 <= 1)", "true"),
        // abs(x) < 2 is x^2 < 4 for a real x; the strict fact makes the certificate strict
        ("simplify(abs(x) < 2, x^2 + y^2 < 4)", "true"),
        ("simplify(abs(x) < 2, x^2 + y^2 <= 4)", "abs(x) < 2"),
    ]);
}

#[test]
fn refine_rules_of_package_f3() {
    table(&[
        ("refine(log(x), x < 0)", "log(-x) + 1im*pi"),
        ("refine((a^b)^c, -1 < b and b < 1)", "a^(b*c)"),
        ("refine(log(x^p), -1 < p and p < 1)", "p*log(x)"),
        ("refine(atan(tan(x)), -pi/2 < re(x) and re(x) < pi/2)", "x"),
        ("refine(mod(a, 4), element((a + 3)/4, integers))", "1"),
        ("refine(re(a + b*im), element(a | b, reals))", "a"),
        ("refine(im_part(x + im*y), element(x | y, reals))", "y"),
        ("refine(sign(im_part(x + im*y)), element(x | y, positive_reals))", "1"),
        ("refine(sign(im_part(x + im*y)), element(x | y, negative_reals))", "-1"),
    ]);
}

#[test]
fn reduce_domains() {
    table(&[
        ("reduce((2*x^2 - 1)*(4*x^2 - 1) == 0, x, rationals)", "x == -1/2 or x == 1/2"),
        ("reduce((x^3 - 8/27)*(x^3 - 2) == 0, x, rationals)", "x == 2/3"),
        ("reduce(x^2 - 4 == 0, x, positive_reals)", "x == 2"),
        ("reduce(x^2 < 4, x, negative_reals)", "-2 < x and x < 0"),
    ]);
}

#[test]
fn conditional_results() {
    table(&[
        ("conditional_expression(a, true)", "a"),
        ("conditional_expression(a, false)", "undefined"),
        ("sin(conditional_expression(x, x > 0) + 1)*conditional_expression(x, x < 1)^2", "conditional_expression(x^2*sin(1 + x), 0 < x and x < 1)"),
        ("conditional_expression(x, x > 0)^2 == 1 and conditional_expression(y, y < 0) > -1", "conditional_expression(x^2 == 1 and y > -1, x > 0 and y < 0)"),
        ("conditional_expression(x, x > 0) + conditional_expression(x, x < 0)", "undefined"),
        ("conditional_expression(p, c) and (conditional_expression(q, d) or r)", "conditional_expression(p and (q or r), c and d)"),
        ("conditional_expression(conditional_expression(x, x > 0), x > 1)", "conditional_expression(x, x > 1)"),
        ("simplify(conditional_expression(sqrt(x^2), x < 0))", "conditional_expression(-x, x < 0)"),
        ("reduce(conditional_expression(x^2 - 1, x > 0) == 0, x)", "x == 1"),
        ("integrate(conditional_expression(a^x, a > 0), x)", "conditional_expression(a^x/log(a), a > 0)"),
        ("limit(conditional_expression(a^x, 0 < a and a < 1), x -> inf)", "conditional_expression(0, 0 < a and a < 1)"),
        ("normal(conditional_expression(x, x > 0))", "x"),
        ("[conditional_expression(x^2, x > 0) == 1, conditional_expression(x^a, a > 0) < 2]", "[conditional_expression(x^2 == 1, x > 0), conditional_expression(x^a < 2, a > 0)]"),
        (
            "refine([conditional_expression(sqrt(x^2), x > 0), conditional_expression(sqrt(x^2), x < 0)])",
            "[conditional_expression(x, x > 0), conditional_expression(-x, x < 0)]",
        ),
        // a numeric point decides the condition
        ("conditional_expression(x^2, x > 0) /. x -> 2", "4"),
        ("conditional_expression(x^2, x > 0) /. x -> -2", "undefined"),
    ]);
}

#[test]
fn signs_of_limit_and_integrate() {
    table(&[
        ("assuming(a > 0, integrate(x^a, [x, 0, 1]))", "(1 + a)^-1"),
        ("assuming(a > 1, limit(a^x, x -> inf))", "inf"),
        ("assuming(a > 0, limit(x^a, x -> 0, direction: from_above))", "0"),
    ]);
}

/// `test_conditions(value, [c1, ...])`: the conditions recorded in a [`mc_decide::Conditions`] and attached to the
/// value, as definite integration (14D) will do; `test_sign(e)`: the decider's sign as -1, 0, 1 or `unknown`.
struct TestNative {
    sign: bool,
}

impl mc_eval::Native for TestNative {
    fn call<'h>(&self, cx: &mut mc_eval::NativeCx<'_, '_, 'h>, _t: mc_api::T<'h>, args: &[mc_api::T<'h>]) -> mc_eval::Res<mc_eval::Reply<'h>> {
        use mc_api::Store;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let h = cx.heap();
        if self.sign {
            let [x] = args else { return Ok(mc_eval::Reply::Decline) };
            let mut d = mc_decide::Decider::of_context(cx, &stop)?;
            let s = d.sign(cx, *x)?;
            return Ok(mc_eval::Reply::Done(match s.exact() {
                Some(k) => h.int(k as i64),
                None => h.sym_term(h.sym("unknown")),
            }));
        }
        let [v, cs] = args else { return Ok(mc_eval::Reply::Decline) };
        let mut c = mc_decide::Conditions::new();
        for &x in cx.as_list(*cs).unwrap_or(&[]) {
            c.record(x);
        }
        Ok(mc_eval::Reply::Done(c.finish(cx, &stop, *v)?))
    }
}

#[test]
fn rust_api() {
    let mut e = CoreEngine::new();
    e.install(|k, h| {
        use mc_api::Store;
        let a = mc_eval::Attributes { uses_assumptions: true, ..mc_eval::Attributes::default() };
        k.register(h.sym("test_conditions"), a, Box::new(TestNative { sign: false }));
        k.register(h.sym("test_sign"), a, Box::new(TestNative { sign: true }));
    });
    let rows = [
        ("test_conditions(1/(1 + a), [a > -1])", "conditional_expression((1 + a)^-1, a > -1)"),
        // a condition the context implies is dropped; one it refutes makes the value undefined
        ("assuming(a > 0, test_conditions(1/(1 + a), [a > -1]))", "(1 + a)^-1"),
        ("assuming(a < -2, test_conditions(1/(1 + a), [a > -1]))", "undefined"),
        // conditions implied by the others are dropped, bounds on one variable joined into an interval
        ("test_conditions(f(a), [a > 0, a > -1, a < 3])", "conditional_expression(f(a), 0 < a and a < 3)"),
        ("test_conditions(f(a), [a > 0, a < -1])", "undefined"),
        ("test_conditions(f(a), [])", "f(a)"),
        ("test_sign(x^2 + 1)", "unknown"),
        ("assuming(element(x, reals), test_sign(x^2 + 1))", "1"),
        ("assuming(x > 2, test_sign(1 - x))", "-1"),
        ("test_sign(pi - 22/7)", "-1"),
    ];
    let mut bad = vec![];
    for (src, want) in rows {
        let got = run(&mut e, src);
        if got != want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} wrong:\n{}", bad.len(), bad.join("\n"));
}
