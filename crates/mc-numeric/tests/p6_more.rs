//! Phase 6 (package p6-numeric): exact identities added to the elementary functions, infinities, imaginary
//! arguments, the functions defined by formulas (`sec` ... `abs_arg`), and machine contagion of complex numbers.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn trigonometric_tables() {
    check(&[
        ("cos(pi/5)", "(1/4)*(1 + 5^(1/2))"),
        ("sin(pi/10)", "(1/4)*(-1 + 5^(1/2))"),
        ("tan(pi/12)", "2 - 3^(1/2)"),
        ("tan(5*pi/12)", "2 + 3^(1/2)"),
        ("sin(-2*pi/5)", "-(5/8 + (1/8)*5^(1/2))^(1/2)"),
        ("cos(30*degree)", "(1/2)*3^(1/2)"),
        ("n(degree)", "0.017453292519943295"),
        ("sin(pi/8)", "sin((1/8)*pi)"),
        // the inverse functions keep the classic angles only
        ("asin(1/2)", "(1/6)*pi"),
        ("exp(im*pi/5)", "exp(complex(0, 1/5)*pi)"),
    ]);
}

#[test]
fn imaginary_arguments_and_logarithms() {
    check(&[
        ("cos(im)", "cosh(1)"),
        ("sin(2*im)", "1im*sinh(2)"),
        ("cosh(im*pi/5)", "(1/4)*(1 + 5^(1/2))"),
        ("tanh(im*pi/3)", "1im*3^(1/2)"),
        ("n(cos(im))", "1.5430806348152437"),
        ("exp(2*log(3))", "9"),
        ("exp(-log(2))", "1/2"),
        ("exp(2 + 3*log(2))", "8*exp(2)"),
        ("exp(a*log(x))", "x^a"),
        ("log(im)", "complex(0, 1/2)*pi"),
        ("log(-3/2)", "log(3/2) + 1im*pi"),
        ("log(0)", "-inf"),
    ]);
}

#[test]
fn infinities() {
    check(&[
        ("exp(-inf)", "0"),
        ("exp(inf)", "inf"),
        ("log(inf)", "inf"),
        ("atan(-inf)", "(-1/2)*pi"),
        ("tanh(inf)", "1"),
        ("abs(-inf)", "inf"),
        ("abs(im*inf)", "inf"),
        ("sign(-im*inf)", "-1im"),
        ("arg(-inf)", "pi"),
        ("re(im*inf)", "0"),
        ("im_part(im*inf)", "inf"),
        ("unit_step(inf)", "1"),
        ("clip(-inf)", "-1"),
    ]);
}

#[test]
fn powers() {
    check(&[
        ("sqrt(-9.0)", "complex(0.0, 3.0)"),
        ("sqrt(3 + 4*im)", "complex(2, 1)"),
        ("sqrt(-2*im)", "complex(1, -1)"),
        ("sqrt(im)", "(-1)^(1/4)"),
        ("(3 + 4*im)^(3/2)", "complex(2, 11)"),
        // queue G8: machine complex numbers make closed numeric terms machine numbers
        ("complex(3.0, 1.5)*sqrt(2)", "complex(4.242640687119286, 2.121320343559643)"),
        ("3*sqrt(3) + complex(3.0, 1.5)", "complex(8.196152422706632, 1.5)"),
    ]);
}

#[test]
fn reciprocal_and_inverse_functions() {
    check(&[
        ("sec(pi/3)", "2"),
        ("sec(-x)", "sec(x)"),
        ("csc(-x)", "-csc(x)"),
        ("cot(pi/4)", "1"),
        ("coth(inf)", "1"),
        ("sech(1.)", "0.6480542736638855"),
        // queue G12: the inverse hyperbolic functions
        ("atanh(0.5)", "0.5493061443340549"),
        ("n(acosh(2))", "1.3169578969248166"),
        ("atanh(2.)", "complex(0.5493061443340549, -1.5707963267948966)"),
        ("asinh(2.*im)", "complex(1.3169578969248168, 1.5707963267948966)"),
        ("atanh(im)", "complex(0, 1/4)*pi"),
        ("atanh(-x)", "-atanh(x)"),
        ("acosh(1)", "0"),
        ("acot(0)", "(1/2)*pi"),
        ("asec(2)", "(1/3)*pi"),
        ("asech(0)", "inf"),
    ]);
}

#[test]
fn other_formula_functions() {
    check(&[
        ("sinc(0)", "1"),
        ("sinc(pi/2)", "2/pi"),
        ("log10(100)", "2"),
        ("log2(1/8)", "-3"),
        ("log10(x)", "log(x)/log(10)"),
        ("fractional_part(7/3)", "1/3"),
        ("fractional_part(pi)", "-3 + pi"),
        ("integer_part(-7/3)", "-2"),
        ("unit_box(1/2)", "1"),
        ("unit_triangle(1/2)", "1/2"),
        ("ramp(-1)", "0"),
        // queue G1
        ("heaviside_theta(1.0)", "1"),
        ("heaviside_theta(-2)", "0"),
        ("heaviside_theta(0)", "heaviside_theta(0)"),
        ("heaviside_theta(x)", "heaviside_theta(x)"),
        ("surd(-8, 3)", "-2"),
        ("re_im(3 + 4*im)", "[3, 4]"),
        ("sech([0, 1.])", "[1, 0.6480542736638855]"),
    ]);
}

#[test]
fn uncertain_numbers() {
    check(&[
        ("around(1.0, 0.1) + around(2.0, 0.2)", "around(3.0, 0.223606797749979)"),
        ("around(2.0, 0.1)^2", "around(4.0, 0.4)"),
        ("interval([1, 2])*interval([-1, 3])", "interval([-2, 6])"),
        ("1/interval([-1, 2])", "interval([-inf, -1], [1/2, inf])"),
        ("abs(interval([-3, 5]))", "interval([0, 5])"),
        ("acos(interval([1/3, 1/2]))", "interval([(1/3)*pi, acos(1/3)])"),
        ("interval([1, 2]) < 3", "true"),
        ("interval([1, 2]) <= 2", "true"),
        ("interval([1, 2]) == 5", "false"),
        ("interval([1, 2]) < interval([1.5, 4])", "interval([1, 2]) < interval([1.5, 4])"),
    ]);
}

#[test]
fn quantities() {
    check(&[
        ("quantity(3, \"Meters\") + quantity(2, \"Feet\")", "quantity(1504/127, \"Feet\")"),
        ("quantity(1, \"Hours\") + quantity(30, \"Minutes\")", "quantity(90, \"Minutes\")"),
        ("quantity(6, \"Meters\")/quantity(2, \"Seconds\")", "quantity(3, \"Meters\"/\"Seconds\")"),
        ("quantity(3, \"Meters\")/quantity(2, \"Meters\")", "3/2"),
        ("unit_convert(quantity(1, \"Miles\"), \"Kilometers\")", "quantity(25146/15625, \"Kilometers\")"),
        ("unit_convert(quantity(30, \"DegreesCelsius\"), \"DegreesFahrenheit\")", "quantity(86, \"DegreesFahrenheit\")"),
        ("quantity_magnitude(quantity(3, \"Feet\"), \"Inches\")", "36"),
        ("quantity(3, \"Meters\") + quantity(1, \"Seconds\")", "failure(incompatible_units)"),
    ]);
}

#[test]
fn machine_lists() {
    // queue G48: the elementary functions of a list of machine numbers elementwise
    check(&[
        ("exp(im*range(0.0, 1.0, 0.01))[3]", "complex(0.9998000066665778, 0.01999866669333308)"),
        ("length(sin(range(0.0, 100.0, 0.01)))", "10001"),
        ("log(range(-1.0, 1.0, 0.01))[1]", "complex(0.0, 3.141592653589793)"),
        // complex leaves with a zero imaginary part stay complex, in compound lists as in packed arrays and alone
        ("exp(join([complex(-1.0, 0.0)], range(1.0, 70.0)))[1]", "complex(0.36787944117144233, 0.0)"),
        ("exp(join([complex(-1.0, 0.0)], range(1.0, 70.0)))[2]", "2.718281828459045"),
        ("exp(table(complex(-1.0*k, 0.0), k, 70))[1]", "complex(0.36787944117144233, 0.0)"),
        ("log(join([complex(-1.0, -0.0)], range(1.0, 70.0)))[1]", "complex(0.0, -3.141592653589793)"),
        ("log(complex(-1.0, -0.0))", "complex(0.0, -3.141592653589793)"),
    ]);
}
