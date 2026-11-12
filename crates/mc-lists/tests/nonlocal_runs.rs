//! Item 9B: `throw`/`catch` and `sow`/`reap` (mc-eval), `repeated` patterns in both matchers, held
//! failures, argument-head rules attached to symbol arguments, stepped spans and the options of the sequence
//! functions, on source text.
mod common;
use common::{check, Env};

#[test]
fn throw_and_catch() {
    check(
        &mut Env::new(),
        &[
            ("throw(1)", "failure(thrown, 1)"),
            ("throw(1, t)", "failure(thrown, 1, t)"),
            ("catch({ a; b; throw(c); d })", "c"),
            ("catch(1 + 2)", "3"),
            ("catch(throw(5) + 1)", "5"),
            ("catch([a, throw(b), throw(c)])", "b"),
            ("catch([catch([a, throw(b), c]), d])", "[b, d]"),
            ("catch(for k in range(100) { if k^2 > 50 then throw(k) })", "8"),
            ("catch(map(x => if x < 0 then throw(x) else x, [1, 2, -3, 4, -5]))", "-3"),
            ("catch(throw(a, u), u)", "a"),
            ("catch(throw(a, u), v)", "failure(thrown, a, u)"),
            ("catch(throw(a, u))", "failure(thrown, a, u)"),
            ("catch(throw(a, u(1)), u(_))", "a"),
            ("catch(throw(a, u), u, (v, t) => [t, v])", "[u, a]"),
            ("catch(catch(throw(1, small) + 2, big), small)", "1"),
        ],
    );
}

#[test]
fn sow_and_reap() {
    check(
        &mut Env::new(),
        &[
            ("sow(5)", "5"),
            ("reap(x)", "[x, []]"),
            ("reap({ sow(a); b; sow(c); sow(d); e })", "[e, [[a, c, d]]]"),
            ("reap({ sow(1, [x, x]); sow(2, y); sow(3, x) }, [x, x, y])", "[3, [[[1, 1, 3]], [[1, 1, 3]], [[2]]]]"),
            ("reap({ sow(1, x); sow(2, y); sow(3, x) }, _, f)", "[3, [f(x, [1, 3]), f(y, [2])]]"),
            ("reap({ sow(1, x); sow(2, y) }, x)", "[2, [[1]]]"),
            // a tag the inner reap does not take goes to the outer one
            ("reap(reap({ sow(1, x); sow(2, y); 0 }, x))", "[[0, [[1]]], [[2]]]"),
            ("reap(for k in range(10) { if k > 6 then sow(k) })", "[null, [[7, 8, 9, 10]]]"),
        ],
    );
}

#[test]
fn held_failures_are_data() {
    check(
        &mut Env::new(),
        &[
            ("if 1 < 0 then failure(t, 1) else 2", "2"),
            ("map(x => if x < 0 then failure(neg, x) else x, [1, -2, 3])", "[1, failure(neg, -2), 3]"),
            ("{ var n = 0; for k in range(5) { if k > 2 then n = n + 1 }; n }", "3"),
            ("1 + failure(t, 2)", "failure(t, 2)"),
        ],
    );
}

#[test]
fn repeated_patterns() {
    check(
        &mut Env::new(),
        &[
            ("match_q([a, a, a], [repeated(a)])", "true"),
            ("match_q([], [repeated(a)])", "false"),
            ("match_q([], [repeated(a, [0, inf])])", "true"),
            ("match_q([a, b], [repeated(a)])", "false"),
            ("match_q([3, 3, 3], [repeated(x_ when is_integer(x/3))])", "true"),
            ("match_q([3, 6, 9], [repeated(x_ when is_integer(x/3))])", "false"),
            ("match_q([1, 2, 3], [repeated(_: Int)])", "true"),
            ("match_q([1, 2, x], [repeated(_: Int), _])", "true"),
            ("rewrite([[], [a, a], [a, b], [a, a, a], [a]], [repeated(a)] -> x)", "[[], x, [a, b], x, x]"),
            ("rewrite([[], [a], [a, a], [a, a, a]], [repeated(a, [2])] -> x)", "[[], [a], x, [a, a, a]]"),
            ("rewrite([[], [a], [a, a], [a, a, a]], [repeated(a, [0, 2])] -> x)", "[x, x, x, [a, a, a]]"),
            ("rewrite([[], [a], [a, a], [a, a, a]], [repeated(a, 2)] -> x)", "[[], x, x, [a, a, a]]"),
            ("cases([f(1, 2, 3), f(1), f(), f(1, a)], f(repeated(_: Int, [0, inf])))", "[f(1, 2, 3), f(1), f()]"),
            ("cases([[1, 2], [a, b]], [repeated(_: Sym)])", "[[a, b]]"),
            // the evaluator's matcher: definitions with runs
            ("rule rr(xs @ [repeated(_: Int)]) := length(xs)", "null"),
            ("[rr([1, 2, 3]), rr([1, a])]", "[3, rr([1, a])]"),
            ("rule rr(repeated(a, [2])) := two", "null"),
            ("rr(a, a)", "two"),
        ],
    );
}

#[test]
fn rules_attached_to_symbol_arguments() {
    check(
        &mut Env::new(),
        &[
            ("@on(square) rule area(square) = s^2", "null"),
            ("area(square)", "s^2"),
            ("area(circle)", "area(circle)"),
            ("@on(h) rule f(h(x_)) := hf(x)", "null"),
            ("f(h(3))", "hf(3)"),
        ],
    );
}

#[test]
fn stepped_spans() {
    check(
        &mut Env::new(),
        &[
            ("range(10)[span(1, 10, 3)]", "[1, 4, 7, 10]"),
            ("range(10)[span(2, -1, 2)]", "[2, 4, 6, 8, 10]"),
            ("range(5)[span(-1, 1, -2)]", "[5, 3, 1]"),
            ("[[1, 2, 3], [4, 5, 6]][all, span(1, 3, 2)]", "[[1, 3], [4, 6]]"),
            ("range(5)[span(1, 5, 0)]", "failure(index, 0, 5)"),
        ],
    );
}

#[test]
fn sequence_options() {
    check(
        &mut Env::new(),
        &[
            ("sequence_position([a, b, b, a, b, b, b], [repeated(b)])", "[[2, 3], [3, 3], [5, 7], [6, 7], [7, 7]]"),
            ("sequence_position([a, b, a, b, a, b, a, b], [a, b, a, b], overlaps: false)", "[[1, 4], [5, 8]]"),
            ("sequence_position([a, b, c, d], [__], overlaps: all)", "[[1, 4], [1, 3], [1, 2], [1, 1], [2, 4], [2, 3], [2, 2], [3, 4], [3, 3], [4, 4]]"),
            ("sequence_cases([a, b, a, b, a, b, a, b], [a, b, a, b], overlaps: true)", "[[a, b, a, b], [a, b, a, b], [a, b, a, b]]"),
            ("sequence_cases([1, 2, 3, 4], [_, _], 1)", "[[1, 2]]"),
            ("sequence_split([x, x, a, b, y, a, c, z], [a, _], 2)", "[[x, x], [y, a, c, z]]"),
            ("sequence_split([1, 2, 3, 4, 5, 6, 7], [[2, 3] -> \"X\", [6] -> \"Y\"])", "[[1], \"X\", [4, 5], \"Y\", [7]]"),
            ("sequence_replace([1, 2, a, 3], [[_: Int, _: Int] -> p, [_: Int] -> q])", "[p, a, q]"),
            ("sequence_count([1, 2, 3], [_, _], overlaps: true)", "2"),
        ],
    );
}

#[test]
fn key_value_patterns() {
    check(
        &mut Env::new(),
        &[
            ("match_q(assoc(a -> 1, b -> 2, c -> 3), key_value_pattern(b -> 2))", "true"),
            ("match_q([a -> 1, b -> 2, c -> 3], key_value_pattern(b -> 2))", "true"),
            ("match_q(assoc(a -> 1), key_value_pattern(b -> _))", "false"),
            ("match_q(assoc(a -> 1, b -> 2), key_value_pattern([_ -> 2, _ -> 1]))", "true"),
            ("match_q(assoc(a -> 1), key_value_pattern([_, _]))", "false"),
            ("match_q([a], key_value_pattern([]))", "false"),
            ("match_q([], key_value_pattern([]))", "true"),
            ("rewrite(assoc(a -> 1, b -> 2, c -> 3), key_value_pattern([a -> x_, c -> y_]) :> [x, y])", "[1, 3]"),
            ("cases([assoc(n -> 1, ok -> true), assoc(n -> 2)], key_value_pattern(ok -> true))", "[assoc(n -> 1, ok -> true)]"),
        ],
    );
}

#[test]
fn library_patterns_match_modulo_ac() {
    check(
        &mut Env::new(),
        &[
            // the library functions use the evaluator's matcher (C9B.10)
            ("cases([a + b, a + b + c, x], u_ + v_)", "[a + b]"),
            ("cases([a + b + c], u_ + vs__ -> [u, vs])", "[[a, b + c]]"),
            ("count([2*x, 3, x*y], (n_: Int)*w_)", "1"),
            // a head test as a type (C9B.14)
            ("count([abs(x), y, abs(2*y)], _: Head[abs])", "2"),
            ("count(abs(x) + abs(y)^2, _: Head[abs], [0, inf])", "2"),
        ],
    );
}

#[test]
fn optional_arguments_inside_patterns() {
    check(
        &mut Env::new(),
        &[
            // C9B.13: optional(x_, d) among a call's arguments, not only a definition's own
            ("match_q([x], [x, optional(y_, 2)])", "true"),
            ("rewrite([x], [optional(a_, 0), optional(b_, 2)] :> [a + b])", "[2 + x]"),
            ("cases([[x], [y, z], [p, q, r]], [a_, optional(b_, 2)] :> a + b)", "[2 + x, y + z]"),
            ("rule fo(x_, optional(y_, 7)) := [x, y]", "null"),
            ("[fo(1), fo(1, 2)]", "[[1, 7], [1, 2]]"),
        ],
    );
}
