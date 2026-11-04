//! Package N4b through the engine: compiled functions that draw give exactly the interpreter's draws (seeded, at 1
//! and 4 threads, inside `map` where speculation reaches them, after declines), and random complex expressions
//! compile to the interpreter's values bit for bit.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn engine(threads: usize) -> CoreEngine {
    let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
    e.configure("memo", "off").expect("the memo setting");
    e
}

/// The value of `key -> v` in `compiled_info(f)`'s printed list (a number).
fn info_num(e: &mut CoreEngine, f: &str, key: &str) -> u64 {
    let s = eval(e, &format!("compiled_info({f})"));
    let at = s.find(&format!("{key} -> ")).unwrap_or_else(|| panic!("{key} in {s}")) + key.len() + 4;
    s[at..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or_else(|_| panic!("{s}"))
}

/// Functions that draw, and calls of them (each program is run after `seed(...)`).
const DRAWS: &[(&str, &str)] = &[
    ("pi_hits", "fn pi_hits(m: Int) = { var h = 0; for k in range(m) { let x = random_real(); let y = random_real(); if x^2 + y^2 < 1.0 then h = h + 1 }; h }"),
    ("walk", "fn walk(n: Int, s: Real) = { var x = 0.0; for k in range(n) { x = x + random_normal(0.0, s) }; x }"),
    ("dice", "fn dice(n: Int) = { var t = 0; for k in range(n) { t = t + random_integer([1, 6]) * random_integer(k) }; t }"),
    ("mixed", "fn mixed(a: Real, b: Real) = [random_real([a, b]), random_real(b), random_normal(), random_normal(a, 0.5), random_integer([-3, 3]), random_real([1, 2])]"),
    // draws in a condition, in short-circuit operands and in branches: only the evaluated ones draw
    ("branchy", "fn branchy(n: Int) = { var c = 0; for k in range(n) { if random_real() < 0.3 and random_real() < 0.5 or random_integer(1) == 1 then c = c + 1 else c = c - random_integer([0, 2]) }; c }"),
    ("loop_until", "fn loop_until(p: Real) = { var k = 0; while random_real() > p { k = k + 1 }; k }"),
    // an inlined function that draws
    ("coin", "fn coin(p: Int) = random_integer(p)"),
    ("coins", "fn coins(n: Int) = { var s = 0; for k in range(n) { s = s + coin(1) }; s }"),
    // declines after drawing: an overflow, a refused range (the interpreter then fails), a domain error
    ("late", "fn late(n: Int, big: Int) = { let x = random_integer([0, 10]); let y = random_real(); x + y + big*n }"),
    ("badrange", "fn badrange(a: Real) = random_real() + random_real([a, 0.0])"),
    ("dom", "fn dom(a: Real) = { let u = random_real(); log(a - u) }"),
];

const CALLS: &[&str] = &[
    "pi_hits(1000)",
    "walk(100, 2.0)",
    "dice(50)",
    "mixed(-1.5, 2.5)",
    "branchy(200)",
    "loop_until(0.05)",
    "coins(100)",
    "late(2, 3)",
    "late(2, 9223372036854775807)",
    "[badrange(-1.0), random_real()]",
    "[badrange(1.0), random_real()]",
    "[dom(0.0), random_real()]",
    "[dom(2.0), random_real()]",
    "map(k => pi_hits(k), range(30))",
    "table(walk(k, 1.0), [k, 20])",
    "[dice(3), random_real(), dice(4)]",
];

fn setup(e: &mut CoreEngine, compile: bool) {
    for (name, def) in DRAWS {
        assert_eq!(eval(e, def), "null", "{def}");
        if compile {
            assert_eq!(eval(e, &format!("compile({name})")), *name);
        }
    }
}

#[test]
fn compiled_draws_are_the_interpreters() {
    let mut plain = engine(1);
    setup(&mut plain, false);
    let mut want = vec![];
    for (i, call) in CALLS.iter().enumerate() {
        want.push(eval(&mut plain, &format!("seed({}); {call}", 100 + i)));
    }
    for threads in [1, 4] {
        for back_end in ["cranelift", "interpreter"] {
            let mut e = engine(threads);
            for (name, def) in DRAWS {
                assert_eq!(eval(&mut e, def), "null");
                assert_eq!(eval(&mut e, &format!("compile({name}, back_end: \"{back_end}\")")), *name);
            }
            for (i, call) in CALLS.iter().enumerate() {
                let got = eval(&mut e, &format!("seed({}); {call}", 100 + i));
                assert_eq!(got, want[i], "{call} at {threads} threads, {back_end}");
            }
            // the loops really ran compiled, and the declining calls declined
            for f in ["pi_hits", "walk", "dice", "mixed", "branchy", "loop_until", "coins"] {
                assert!(info_num(&mut e, f, "runs") > 0, "{f} never ran compiled");
            }
            for f in ["late", "badrange", "dom"] {
                assert!(info_num(&mut e, f, "declines") > 0, "{f} never declined");
            }
        }
    }
}

#[test]
fn the_monte_carlo_checksum_is_the_interpreters() {
    // L10 at 10^5 here (10^6 in the benchmark): the same count with and without the compiler
    let mut plain = engine(1);
    setup(&mut plain, false);
    let mut e = engine(4);
    setup(&mut e, true);
    let p = "seed(2026); pi_hits(100000)";
    assert_eq!(eval(&mut e, p), eval(&mut plain, p));
    // the draws continue from the same stream position afterwards
    assert_eq!(eval(&mut e, "seed(7); pi_hits(1000); random_real()"), eval(&mut plain, "seed(7); pi_hits(1000); random_real()"));
}

#[test]
fn draws_outside_compiled_functions_are_refused() {
    let mut e = engine(1);
    let refused = [
        ("cnt", "fn cnt(n: Int) = random_real(1.0, n)", "a count"),
        ("ch", "fn ch(n: Int) = random_choice([1, 2, n])", "`random_choice`"),
        ("rr", "fn rr(x: Real) = random_integer([0.5, x])", "not of integers"),
    ];
    for (name, def, why) in refused {
        assert_eq!(eval(&mut e, def), "null");
        let r = eval(&mut e, &format!("compile({name})"));
        assert!(r.starts_with("failure(not_compilable") && r.contains(why), "{r}");
    }
}

/// A small deterministic generator (xorshift64*) for random expressions.
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A complex-valued expression in the complex locals `z`, `w` and the reals `x`, `y`.
    fn complex(&mut self, d: u32) -> String {
        if d == 0 {
            return ["z", "w", "complex(x, y)", "(x + y*im)", "complex(y, 0.5)", "(2.0 + 3im)"][self.below(6) as usize].to_string();
        }
        let (a, b) = (self.complex(d - 1), self.complex(d - 1));
        match self.below(14) {
            0 => format!("({a} + {b})"),
            1 => format!("({a} * {b})"),
            2 => format!("({a} - {b})"),
            3 => format!("({a} / {b})"),
            4 => format!("{a}^{}", [2, 3, -1, -2, 5, 7][self.below(6) as usize]),
            5 => format!("({a} + {b} + x)"),
            6 => format!("(x*{a} + y*{b} + 1)"),
            7 => format!("{}({a})", ["exp", "log", "sin", "cos", "tanh", "sqrt", "atan"][self.below(7) as usize]),
            8 => format!("conjugate({a})"),
            9 => format!("{a}^{}", ["0.5", "(1/3)", "x", "{b}"][self.below(4) as usize]).replace("{b}", &b),
            10 => format!("(im*{a})"),
            11 => format!("(3*{a} - {b}*2.5)"),
            12 => format!("({a} * {b} * {a})"),
            _ => format!("(-{a})"),
        }
    }

    /// A real-valued expression built from complex ones.
    fn real(&mut self, d: u32) -> String {
        let a = self.complex(d);
        match self.below(4) {
            0 => format!("abs({a})"),
            1 => format!("re({a})"),
            2 => format!("im_part({a})"),
            _ => format!("(abs({a}) + re({a}))"),
        }
    }
}

#[test]
fn random_complex_expressions_compile_to_the_interpreters_values() {
    let mut g = Gen(0x00c0_ffee_d00d_f00d);
    let args = ["1.5, -2.0", "-0.0, 3.0", "0.0, 0.0", "0.25, 1e-300", "1e200, -1e200", "-3.5, 0.125", "1e308*10.0, 1.0", "0.0*(1e308*10.0), 2.0"];
    let (mut plain, mut comp) = (engine(1), engine(1));
    let (mut compiled, mut refused, mut runs) = (0, 0, 0);
    let mut reasons = vec![];
    for k in 0..150 {
        let (name, d) = (format!("cx{k}"), 1 + (k % 3) as u32);
        let body = if k % 2 == 0 { g.complex(d) } else { g.real(d) };
        let def = format!("fn {name}(x: Real, y: Real) = {{ let z = complex(x, -y); var w = complex(y, x); w = w*z + 1; {body} }}");
        assert_eq!(eval(&mut plain, &def), "null", "{def}");
        assert_eq!(eval(&mut comp, &def), "null", "{def}");
        let c = eval(&mut comp, &format!("compile({name})"));
        if c != name {
            refused += 1;
            reasons.push(format!("{c} in {body}"));
            assert!(c.starts_with("failure(not_compilable"), "{c}");
            continue;
        }
        compiled += 1;
        for a in args {
            let call = format!("{name}({a})");
            assert_eq!(eval(&mut comp, &call), eval(&mut plain, &call), "{def}\n{call}");
        }
        runs += info_num(&mut comp, &name, "runs");
    }
    assert!(compiled > 130 && refused < 20, "{compiled} compiled, {refused} refused: {reasons:#?}");
    assert!(runs > 500, "{runs} compiled runs");
}
