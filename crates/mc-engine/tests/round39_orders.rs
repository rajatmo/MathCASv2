//! Round 39: alphabetic orders of more languages and the options of alphabetic sorting, sequence alignments with
//! options (case, local alignment, unmerged differences), counted positions and operator forms, shears along the part
//! of a direction orthogonal to the normal.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn operator_forms_stay() {
    assert_eq!(ev("position(_)([a, b]) === position([a, b], _)"), "true");
    assert_eq!(ev("position([a, b, a, b], b, [1], 0)"), "[]");
    assert_eq!(ev("position([[a], [b, [a]]], a, inf, 1)"), "[[1, 1]]");
    // functions given more rules keep their unevaluated forms
    assert_eq!(
        ev("[mean(_), cross(_), replace(_), region_bounds(_), head(a, b, c)]"),
        "[mean(_), cross(_), replace(_), region_bounds(_), head(a, b, c)]"
    );
}

#[test]
fn alphabetic_orders_of_languages() {
    let l = "[\"A\", \"a\", \"æ\", \"Æ\", \"i\", \"I\"]";
    assert_eq!(ev(&format!("alphabetic_sort({l}, \"Danish\", case_ordering: \"UpperFirst\")")), "[\"A\", \"a\", \"I\", \"i\", \"Æ\", \"æ\"]");
    // Danish puts upper case first by default
    assert_eq!(ev(&format!("alphabetic_sort({l}, \"Danish\") === alphabetic_sort({l}, \"Danish\", case_ordering: \"UpperFirst\")")), "true");
    assert_eq!(ev("alphabetic_sort([\"ø\", \"å\", \"z\", \"æ\"], \"Danish\")"), "[\"z\", \"æ\", \"ø\", \"å\"]");
    assert_eq!(ev("alphabetic_sort([\"Ñ\", \"ĳ\", \"ñ\", \"Ş\", \"Ĳ\", \"ş\"], \"Dutch\", ignore_case: true)"), "[\"Ñ\", \"ñ\", \"Ş\", \"ş\", \"ĳ\", \"Ĳ\"]");
    let w = "[\"sonar\", \"cañón\", \"soñar\", \"cuña\", \"canon\", \"papá\", \"cuna\", \"papa\"]";
    assert_eq!(
        ev(&format!("alphabetic_sort({w}, \"Spanish\", ignore_diacritics: true)")),
        "[\"canon\", \"cañón\", \"cuna\", \"cuña\", \"papá\", \"papa\", \"sonar\", \"soñar\"]"
    );
    assert_eq!(
        ev(&format!("alphabetic_sort({w}, \"English\", ignore_diacritics: true)")),
        "[\"cañón\", \"canon\", \"cuña\", \"cuna\", \"papá\", \"papa\", \"sonar\", \"soñar\"]"
    );
    // with the accents the unaccented word comes first
    assert_eq!(ev("alphabetic_sort([\"cañón\", \"canon\"], \"English\", ignore_case: true)"), "[\"canon\", \"cañón\"]");
    let f = "[\"File-2\", \"File 1\", \"File4\", \"File 3\"]";
    assert_eq!(ev(&format!("alphabetic_sort({f}, ignore_punctuation: true)")), "[\"File 1\", \"File-2\", \"File 3\", \"File4\"]");
    assert_eq!(ev(&format!("alphabetic_sort({f}, ignore_punctuation: false)")), "[\"File 1\", \"File 3\", \"File-2\", \"File4\"]");
    let t = "[\"A\", \"a\", \"i\", \"I\", \"ı\", \"İ\"]";
    assert_eq!(ev(&format!("alphabetic_sort({t}, \"Turkish\", case_ordering: \"UpperFirst\")")), "[\"A\", \"a\", \"I\", \"ı\", \"İ\", \"i\"]");
    assert_eq!(
        ev(&format!("alphabetic_sort({t}, \"Turkish\", case_ordering: \"UpperFirst\", ignore_case: true)")),
        "[\"A\", \"a\", \"I\", \"ı\", \"i\", \"İ\"]"
    );
    // Turkish ç and ş after c and s
    assert_eq!(ev("alphabetic_sort([\"şu\", \"su\", \"çay\", \"cam\", \"dag\"], \"Turkish\")"), "[\"cam\", \"çay\", \"dag\", \"su\", \"şu\"]");
    // the order of the default language is unchanged
    assert_eq!(ev("alphabetic_sort([\"b\", \"a\", \"B\", \"A\"])"), "[\"a\", \"A\", \"b\", \"B\"]");
}

#[test]
fn alignments_with_options() {
    assert_eq!(ev("sequence_alignment(\"abcdefgHIJKlmn\", \"abCDEfgHIjklmn\", ignore_case: true)"), "[\"abcdefghijklmn\"]");
    let ab = "\"abcXXabcXabc\", \"abcabcYYYabc\"";
    assert_eq!(
        ev(&format!("sequence_alignment({ab}, merge_differences: false)")),
        "[\"abc\", [\"XX\", \"\"], \"abc\", [\"\", \"YY\"], [\"X\", \"Y\"], \"abc\"]"
    );
    assert_eq!(ev(&format!("sequence_alignment({ab}, method: \"Global\") === sequence_alignment({ab})")), "true");
    assert_eq!(
        ev(&format!("sequence_alignment({ab}, method: \"Local\")")),
        "[[\"abcXX\", \"\"], \"abc\", [\"X\", \"\"], \"abc\", [\"\", \"YYYabc\"]]"
    );
    assert_eq!(
        ev("sequence_alignment(\"One fish two fish\", \"One fish two fish red fish blue fish\", method: \"Local\")"),
        "[\"One fish two fish\", [\"\", \" red fish blue fish\"]]"
    );
    // the pieces put back together are the two inputs
    assert_eq!(
        ev(&format!(
            "r = sequence_alignment({ab}, method: \"Local\"); [string_join(map(p => if is_list(p) then p[1] else p, r)), string_join(map(p => if is_list(p) then p[2] else p, r))]"
        )),
        "[\"abcXXabcXabc\", \"abcabcYYYabc\"]"
    );
    assert!(ev(&format!("sequence_alignment({ab}, gap_penalty: 2)")).starts_with("sequence_alignment("));
}

#[test]
fn shears_along_the_orthogonal_part() {
    assert_eq!(ev("shearing_matrix(a, [1, 0], [1, 1]) == shearing_matrix(a, projection([1, 0], cross([1, 1])), [1, 1])"), "true");
    assert_eq!(ev("shearing_matrix(a, [1, 0], [0, 1])"), "[[1, tan(a)], [0, 1]]");
    // a shear keeps the normal component of every point: n . (S x) = n . x
    assert_eq!(ev("s = shearing_matrix(pi/5, [2, 1, 0], [0, 1, 1]); simplify(dot([0, 1, 1], dot(s, [x, y, z])) - (y + z))"), "0");
}
