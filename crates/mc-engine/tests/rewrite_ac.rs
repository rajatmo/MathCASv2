//! Rules under associative-commutative heads (semantics 6.2): a sequence variable that takes arguments of an AC call
//! stands for that head applied to them, and `rewrite` also rewrites part of a longer call of the head.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => panic!("{src}: {err:?}"),
    }
}

/// `(input, printed value)`, evaluated in order in one session.
const SESSION: &[(&str, &str)] = &[
    // a rest variable inside a sum or product is a sum or product, not a list
    ("rewrite(1 + sin(t)^2 + cos(t)^2, [sin(a_)^2 + cos(a_)^2 + r___ -> 1 + r])", "2"),
    ("rewrite(a + b + c, [x_ + r__ -> f(r)])", "f(b + c)"),
    ("match a*b*c { x_*r__ => [x, r] }", "[a, b*c]"),
    ("rewrite(sin(t)^2 + cos(t)^2, [sin(a_)^2 + cos(a_)^2 + r___ -> 1 + r])", "1"),
    ("rule size(x_ + r__) := 1 + size(r)", "null"),
    ("rule size(x_) := 1", "null"),
    ("size(a + b + c + d)", "4"),
    // under a head that is not AC, a sequence variable is still a list
    ("rewrite(g(a, b, c), [g(x_, r__) -> h(r)])", "h([b, c])"),
    ("rewrite(sin(t)^2 + cos(t)^2 + 1, [sin(a_)^2 + cos(a_)^2 -> 1])", "2"),
    ("rewrite(x + sin(u)^2 + cos(u)^2 + y, [sin(a_)^2 + cos(a_)^2 -> 1])", "1 + x + y"),
    ("rewrite(2*a*b*c, [a*b -> q])", "2*c*q"),
    ("rewrite(a + b + c, [a + b -> z])", "c + z"),
    // the variables must still agree: sin(p)^2 + cos(q)^2 is not an instance
    ("rewrite(sin(p)^2 + cos(q)^2 + 3, [sin(a_)^2 + cos(a_)^2 -> 1])", "3 + cos(q)^2 + sin(p)^2"),
    ("rewrite(sin(x)^2 + cos(x)^2 + sin(y)^2 + cos(y)^2, [sin(a_)^2 + cos(a_)^2 -> 1], strategy: fixed_point)", "2"),
    // a pattern with more terms than the call does not match; a head that is not AC matches whole calls only
    ("rewrite(a + b, [a + b + c -> z])", "a + b"),
    ("rewrite(f(a, b, c), [f(a, b) -> z])", "f(a, b, c)"),
    // `match` and function rules still match whole terms
    ("match a + b + c { x_ + y_ => \"two\", _ => \"other\" }", "\"other\""),
    ("rule two(x_ + y_) := [x, y]", "null"),
    ("two(a + b + c)", "two(a + b + c)"),
];

#[test]
fn rules_under_ac_heads() {
    for threads in [1, 4] {
        let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
        for &(src, want) in SESSION {
            assert_eq!(eval(&mut e, src), want, "{src} at {threads} threads");
        }
    }
}
