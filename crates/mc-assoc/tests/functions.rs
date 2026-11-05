//! Each function's behaviour on source text: normal cases, edge cases and failures. Expected values of the set
//! operations, `counts`, `group_by`, `merge` with a combiner and `key_take` agree with the reference system where
//! v2's meaning is the same (1B.8 names the deliberate difference of `is_subset`).
mod common;
use common::{check, engine};

#[test]
fn sets() {
    check(
        &mut engine(1, false),
        &[
            ("union([3, 1, 2, 1], [5, 1])", "[1, 2, 3, 5]"),
            ("union([b, a, b])", "[a, b]"),
            ("union()", "[]"),
            ("union([], [])", "[]"),
            ("union([1, 1.0])", "[1, 1.0]"),
            ("union([f(x), \"s\", x, 2, 1/2, 1.5, x])", "[1/2, 1.5, 2, \"s\", x, f(x)]"),
            ("union(f(b, a), f(c, a))", "f(a, b, c)"),
            ("union([a], f(b))", "failure(argument, union, 2)"),
            ("union([a], 3)", "failure(argument, union, 2)"),
            ("union(x, [1])", "union(x, [1])"),
            // a list keeps a failure as an element (list handles failures), so union sees it as one
            ("union([1/0], [1])", "[1, failure(division_by_zero)]"),
            ("intersection([3, 1, 2, 1], [2, 3, 4])", "[2, 3]"),
            ("intersection([3, 1, 2, 1], [2, 3, 4], [3])", "[3]"),
            ("intersection([b, a, b])", "[a, b]"),
            ("intersection([1, 2], [])", "[]"),
            ("intersection([1, 2], [1.0, 2])", "[2]"),
            ("intersection()", "intersection()"),
            ("intersection([1], \"s\")", "failure(argument, intersection, 2)"),
            ("complement([5, 3, 1, 2], [1], [5])", "[2, 3]"),
            ("complement([2, 1, 2])", "[1, 2]"),
            ("complement([1, 2], [1, 2])", "[]"),
            ("complement([], [1])", "[]"),
            ("complement()", "complement()"),
            ("complement([1], f(1))", "failure(argument, complement, 2)"),
            ("is_subset([3, 1], [1, 2, 3])", "true"),
            ("is_subset([3, 1, 3], [1, 3])", "true"),
            ("is_subset([1, 4], [1, 2, 3])", "false"),
            ("is_subset([], [])", "true"),
            ("is_subset([1], [])", "false"),
            ("is_subset([1.0], [1])", "false"),
            ("is_subset([1], [1], [1])", "is_subset([1], [1], [1])"),
            ("is_subset(1, [1])", "failure(argument, is_subset, 1)"),
        ],
    );
}

#[test]
fn construction_and_normal_form() {
    check(
        &mut engine(1, false),
        &[
            ("assoc(a -> 1, b -> 2)", "assoc(a -> 1, b -> 2)"),
            // a later entry replaces the value and keeps the first position (1B.2)
            ("assoc(a -> 1, b -> 2, a -> 3)", "assoc(a -> 3, b -> 2)"),
            ("assoc([a -> 1, b -> 2])", "assoc(a -> 1, b -> 2)"),
            ("assoc([a -> 1], b -> 2, [a -> 0])", "assoc(a -> 0, b -> 2)"),
            ("assoc(assoc(a -> 1, b -> 2), b -> 5, c -> 6)", "assoc(a -> 1, b -> 5, c -> 6)"),
            ("assoc()", "assoc()"),
            ("assoc([])", "assoc()"),
            // keys are compared by identity: 1 and 1.0 are different keys
            ("assoc(1 -> a, 1.0 -> b, 1 -> c)", "assoc(1 -> c, 1.0 -> b)"),
            ("assoc([1, 2] -> x, \"s\" -> y, f(z) -> w)", "assoc([1, 2] -> x, \"s\" -> y, f(z) -> w)"),
            ("assoc(1 + 1 -> 2 + 2)", "assoc(2 -> 4)"),
            ("assoc(1)", "failure(argument, assoc, 1)"),
            ("assoc(x)", "failure(argument, assoc, 1)"),
            ("assoc(a -> 1, [b -> 2, 3])", "failure(argument, assoc, 2)"),
            ("assoc(a :> 1)", "failure(argument, assoc, 1)"),
            ("assoc(a -> 1/0)", "failure(division_by_zero)"),
            ("is_assoc(assoc(a -> 1))", "true"),
            ("is_assoc(assoc())", "true"),
            ("is_assoc([a -> 1])", "false"),
            ("is_assoc(x)", "false"),
            // printing is faithful: the printed text reads back to the same association
            ("m = assoc(a -> 1, \"s\" -> [1, 2], 3 -> x)", "null"),
            ("m === assoc(a -> 1, \"s\" -> [1, 2], 3 -> x)", "true"),
            ("length(m)", "3"),
            ("length(assoc())", "0"),
            // structural functions of mc-lists rebuild an association, which is normalised again
            ("reverse(assoc(a -> 1, b -> 2))", "assoc(b -> 2, a -> 1)"),
            ("join(assoc(a -> 1, b -> 2), assoc(a -> 3, c -> 4))", "assoc(a -> 3, b -> 2, c -> 4)"),
            ("rest(assoc(a -> 1, b -> 2))", "assoc(b -> 2)"),
        ],
    );
}

#[test]
fn access() {
    check(
        &mut engine(1, false),
        &[
            ("m = assoc(a -> 1, \"s\" -> [1, 2], 3 -> x, [1] -> y, f(u) -> z)", "null"),
            ("keys(m)", "[a, \"s\", 3, [1], f(u)]"),
            ("values(m)", "[1, [1, 2], x, y, z]"),
            ("to_rules(m)", "[a -> 1, \"s\" -> [1, 2], 3 -> x, [1] -> y, f(u) -> z]"),
            ("keys(assoc())", "[]"),
            ("values(assoc())", "[]"),
            ("to_rules(assoc())", "[]"),
            ("keys(q)", "keys(q)"),
            ("keys([a -> 1])", "failure(argument, keys, 1)"),
            ("values(1/0)", "failure(division_by_zero)"),
            ("lookup(m, a)", "1"),
            ("lookup(m, \"s\")", "[1, 2]"),
            ("lookup(m, 3)", "x"),
            ("lookup(m, [1])", "y"),
            ("lookup(m, f(u))", "z"),
            ("lookup(m, q)", "failure(missing_key, q)"),
            ("lookup(m, 3.0)", "failure(missing_key, 3.0)"),
            ("lookup(m, q, 0)", "0"),
            ("lookup(m, a, 0)", "1"),
            ("lookup(assoc(), a)", "failure(missing_key, a)"),
            ("lookup(q, a)", "lookup(q, a)"),
            ("lookup(5, a)", "failure(argument, lookup, 1)"),
            ("lookup(m)", "lookup(assoc(a -> 1, \"s\" -> [1, 2], 3 -> x, [1] -> y, f(u) -> z))"),
            ("is_failure(lookup(m, q))", "true"),
            ("key_exists(m, 3)", "true"),
            ("key_exists(m, 4)", "false"),
            ("key_exists(assoc(), 4)", "false"),
            ("key_exists(7, 4)", "failure(argument, key_exists, 1)"),
            // part access through the argument-head rules on `assoc` (1B.4; `part` declines on associations, 1A.16)
            ("m[a]", "1"),
            ("m[f(u)]", "z"),
            ("lookup(m, [1])", "y"),
            ("m[q]", "failure(missing_key, q)"),
            ("m[a, 1]", "failure(index, 1, 0)"),
            ("assoc(a -> [5, 6])[a, 2]", "6"),
            ("assoc(a -> assoc(b -> 7))[a][b]", "7"),
            ("assoc(a -> assoc(b -> 7))[a, b]", "7"),
            // integer, string and list keys are keys too, never positions
            ("m[3]", "x"),
            ("m[[1]]", "y"),
            ("m[\"s\"]", "[1, 2]"),
        ],
    );
}

#[test]
fn changes() {
    check(
        &mut engine(1, false),
        &[
            ("m = assoc(a -> 1, b -> 2, c -> 3)", "null"),
            ("key_take(m, [c, a, zz])", "assoc(c -> 3, a -> 1)"),
            ("key_take(m, [c, c])", "assoc(c -> 3)"),
            ("key_take(m, b)", "assoc(b -> 2)"),
            ("key_take(m, [])", "assoc()"),
            ("key_take(assoc([1] -> x), [[1]])", "assoc([1] -> x)"),
            ("key_take([1], [1])", "failure(argument, key_take, 1)"),
            ("key_drop(m, [a, zz])", "assoc(b -> 2, c -> 3)"),
            ("key_drop(m, b)", "assoc(a -> 1, c -> 3)"),
            ("key_drop(m, zz) === m", "true"),
            ("key_drop(m, [a, b, c])", "assoc()"),
            (
                "key_sort(assoc(b -> 1, 2 -> x, \"s\" -> 3, a -> 2, 1 -> y))",
                "assoc(1 -> y, 2 -> x, \"s\" -> 3, a -> 2, b -> 1)",
            ),
            ("key_sort(assoc())", "assoc()"),
            ("key_sort(1)", "failure(argument, key_sort, 1)"),
            ("insert(m, b -> 9)", "assoc(a -> 1, b -> 9, c -> 3)"),
            ("insert(m, d -> 4)", "assoc(a -> 1, b -> 2, c -> 3, d -> 4)"),
            ("insert(m, d, 4)", "assoc(a -> 1, b -> 2, c -> 3, d -> 4)"),
            ("insert(assoc(), a, 1)", "assoc(a -> 1)"),
            ("insert(m, 3)", "failure(argument, insert, 2)"),
            ("insert([1], a -> 1)", "failure(argument, insert, 1)"),
            ("insert(q, a -> 1)", "insert(q, a -> 1)"),
            ("delete_key(m, b)", "assoc(a -> 1, c -> 3)"),
            ("delete_key(m, zz) === m", "true"),
            ("delete_key(assoc(a -> 1), a)", "assoc()"),
            ("delete_key(\"m\", a)", "failure(argument, delete_key, 1)"),
            ("m", "assoc(a -> 1, b -> 2, c -> 3)"),
        ],
    );
}

#[test]
fn combination() {
    check(
        &mut engine(1, false),
        &[
            ("merge([assoc(a -> 1, b -> 2), assoc(a -> 5, c -> 3)])", "assoc(a -> 5, b -> 2, c -> 3)"),
            ("merge(assoc(a -> 1, b -> 2), assoc(a -> 5, c -> 3))", "assoc(a -> 5, b -> 2, c -> 3)"),
            ("merge([assoc(a -> 1, b -> 2), assoc(a -> 5, c -> 3)], total)", "assoc(a -> 6, b -> 2, c -> 3)"),
            (
                "merge([assoc(a -> 1, b -> 2), assoc(a -> 5, c -> 3)], f)",
                "assoc(a -> f([1, 5]), b -> f([2]), c -> f([3]))",
            ),
            ("merge([assoc(a -> 1)], xs => 1/0)", "failure(division_by_zero)"),
            ("merge([])", "assoc()"),
            ("merge([], f)", "assoc()"),
            ("merge(assoc(a -> 1))", "assoc(a -> 1)"),
            ("merge([assoc(a -> 1), 2])", "failure(argument, merge, 1)"),
            ("merge(assoc(a -> 1), 2, 3)", "failure(argument, merge, 2)"),
            ("merge(q)", "merge(q)"),
            ("merge()", "merge()"),
            ("map_values(x => x^2, assoc(a -> 2, b -> 3))", "assoc(a -> 4, b -> 9)"),
            ("map_values(f, assoc(a -> 1))", "assoc(a -> f(1))"),
            ("map_values(f, assoc())", "assoc()"),
            ("map_values(x => 1/x, assoc(a -> 1, b -> 0))", "failure(division_by_zero)"),
            ("map_values(f, [1])", "failure(argument, map_values, 2)"),
            ("select_keys(assoc(1 -> a, 2 -> b, 3 -> c), k => k > 1)", "assoc(2 -> b, 3 -> c)"),
            ("select_keys(assoc(1 -> a, x -> b), k => k > 0)", "assoc(1 -> a)"),
            ("select_keys(assoc(1 -> a), k => 1/0)", "failure(division_by_zero)"),
            ("select_keys(assoc(), f)", "assoc()"),
            ("select_keys([1], f)", "failure(argument, select_keys, 1)"),
            ("counts([b, a, b, c])", "assoc(b -> 2, a -> 1, c -> 1)"),
            ("counts([1, 1.0, 1])", "assoc(1 -> 2, 1.0 -> 1)"),
            ("counts([])", "assoc()"),
            ("counts(f(a, a))", "assoc(a -> 2)"),
            ("counts(3)", "failure(argument, counts, 1)"),
            ("counts(q)", "counts(q)"),
            ("fn odd(k) = imod(k, 2) == 1", "null"),
            ("group_by([1, 2, 3, 4, 5], odd)", "assoc(true -> [1, 3, 5], false -> [2, 4])"),
            ("group_by([1, 2, 3, 4, 5], odd, total)", "assoc(true -> 9, false -> 6)"),
            ("group_by([\"ab\", \"c\", \"de\"], string_length)", "assoc(2 -> [\"ab\", \"de\"], 1 -> [\"c\"])"),
            ("group_by([], f)", "assoc()"),
            ("group_by([1, 2], k => 1/(k - 1))", "failure(division_by_zero)"),
            ("group_by([1, 2], odd, g => 1/0)", "failure(division_by_zero)"),
            ("group_by(1, f)", "failure(argument, group_by, 1)"),
        ],
    );
}

#[test]
fn failures() {
    check(
        &mut engine(1, false),
        &[
            ("is_failure(1/0)", "true"),
            ("is_failure(failure(x))", "true"),
            ("is_failure(failure())", "true"),
            ("is_failure(3)", "false"),
            // a failure inside a strict call propagates to the top first (semantics 10)
            // a list holding a failure is a list, not a failure
            ("is_failure([failure(x)])", "false"),
            ("is_failure(f(failure(x)))", "true"),
            ("failure_tag(1/0)", "division_by_zero"),
            ("failure_tag(failure(missing_key, a))", "missing_key"),
            ("failure_tag(lookup(assoc(), k))", "missing_key"),
            ("failure_tag(failure())", "failure(argument, failure_tag, 1)"),
            ("failure_tag(3)", "failure(argument, failure_tag, 1)"),
            ("is_failure()", "is_failure()"),
        ],
    );
}

/// Large associations take the indexed path (more than 16 keys); the results agree with the small path.
#[test]
fn large_associations() {
    check(
        &mut engine(2, false),
        &[
            ("big = assoc(map(i => i -> i^2, range(1000)))", "null"),
            ("length(big)", "1000"),
            ("lookup(big, 500)", "250000"),
            ("lookup(big, 1001)", "failure(missing_key, 1001)"),
            ("key_exists(big, 1000)", "true"),
            ("sum(map(i => lookup(big, i), range(1000)))", "333833500"),
            ("length(insert(big, 0, 0))", "1001"),
            ("lookup(insert(big, 7, x), 7)", "x"),
            ("length(delete_key(big, 7))", "999"),
            ("key_exists(delete_key(big, 7), 7)", "false"),
            ("lookup(delete_key(big, 7), 8)", "64"),
            ("key_take(big, [3, 2000, 1])", "assoc(3 -> 9, 1 -> 1)"),
            ("length(key_drop(big, range(10, 1000)))", "9"),
            ("values(key_sort(assoc(map(i => -i -> i, range(20)))))[1]", "20"),
            ("twice = assoc(join(to_rules(big), map(i => i -> 0, range(1000))))", "null"),
            ("[length(twice), total(values(twice))]", "[1000, 0]"),
            ("lookup(counts(map(i => imod(i, 7), range(1000))), 0)", "142"),
            ("lookup(counts(map(i => imod(i, 7), range(1000))), 3)", "143"),
            ("merged = merge([big, assoc(map(i => i -> 1, range(500, 1500)))], total)", "null"),
            ("[length(merged), lookup(merged, 499), lookup(merged, 500)]", "[1500, 249001, 250001]"),
            ("lookup(merged, 1500)", "1"),
            ("lookup(map_values(v => v + 1, big), 30)", "901"),
            ("length(select_keys(big, k => imod(k, 2) == 0))", "500"),
            ("length(union(range(1000), range(500, 1500)))", "1500"),
            ("length(intersection(range(1000), range(500, 1500)))", "501"),
            ("length(complement(range(1000), range(500, 1500)))", "499"),
            ("is_subset(range(500), range(1000))", "true"),
        ],
    );
}

/// The association form of `group_by` is this package's (the list form is `mc-lists`' `gather_by`): checked on the
/// assembled engine, so a second registration of the name at a merge shows up here.
#[test]
fn group_by_is_the_association_form() {
    check(&mut engine(1, false), &[("is_assoc(group_by([1, 2, 3], k => k > 1))", "true")]);
}
