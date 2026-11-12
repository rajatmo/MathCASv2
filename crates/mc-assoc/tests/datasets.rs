//! Datasets on source text: construction and normal form, the list functions on rows, columns,
//! grouping, aggregates, pivot tables, joins, missing values, the printed table and every failure.
mod common;
use common::{check, engine};
use mc_engine::CoreEngine;

/// The printed value of `src`; for `to_string(...)`, the text of the string (no quotes, no escapes).
fn eval(e: &mut CoreEngine, src: &str) -> String {
    let out = common::eval(e, src);
    match out.strip_prefix('"').and_then(|o| o.strip_suffix('"')) {
        Some(inner) if src.starts_with("to_string(") => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        _ => out,
    }
}

/// Evaluate every case in order in one session and report every one whose printed value differs.
fn run(e: &mut CoreEngine, cases: &[(String, &str)]) {
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|(src, want)| {
            let got = eval(e, src);
            (got != *want).then(|| format!("{src}
  got:  {got}
  want: {want}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("
"));
}

/// The people dataset most cases start from (Bob has no city).
const PEOPLE: &str = "ds = dataset([assoc(\"name\" -> \"Ann\", \"age\" -> 31, \"city\" -> \"Oslo\"), \
    assoc(\"name\" -> \"Bob\", \"age\" -> 25), assoc(\"name\" -> \"Cy\", \"age\" -> 47, \"city\" -> \"Rome\")]); null";

/// The term of a value (the engine shows datasets as tables).
fn t(src: &str) -> String {
    format!("to_string({src})")
}

#[test]
fn construction() {
    let mut e = engine(1, false);
    check(&mut e, &[(PEOPLE, "null")]);
    let cases = [
        // rows as associations: every key is a column, absent ones are missing
        (t("ds"), "dataset(assoc(\"name\" -> [\"Ann\", \"Bob\", \"Cy\"], \"age\" -> [31, 25, 47], \"city\" -> [\"Oslo\", missing(), \"Rome\"]))"),
        // the normal form is kept, and a dataset of a dataset is the dataset
        (t("dataset(assoc(a -> [1, 2], b -> [x, y]))"), "dataset(assoc(a -> [1, 2], b -> [x, y]))"),
        (t("dataset(ds) === ds"), "true"),
        // a table with a header row, and rows under given names; short rows end in missing values
        (t("dataset([[a, b], [1], [2, 3]])"), "dataset(assoc(a -> [1, 2], b -> [missing(), 3]))"),
        (t("dataset([[1, 2], [3, 4]], [\"x\", \"y\"])"), "dataset(assoc(\"x\" -> [1, 3], \"y\" -> [2, 4]))"),
        (t("dataset([[\"a\"]])"), "dataset(assoc(\"a\" -> []))"),
        (t("dataset([])"), "dataset(assoc())"),
        (t("dataset(assoc())"), "dataset(assoc())"),
        (t("dataset([assoc(), assoc(a -> 1)])"), "dataset(assoc(a -> [missing(), 1]))"),
        ("is_dataset(ds)".into(), "true"),
        ("is_dataset(assoc(a -> [1]))".into(), "false"),
        ("is_dataset(dataset(assoc(a -> [1])))".into(), "true"),
        ("is_dataset([])".into(), "false"),
        // a numeric column of 64 or more values is a packed array, like any list of machine numbers
        ("column(dataset(assoc(\"i\" -> range(100))), \"i\") === range(100)".into(), "true"),
        // symbols stay unevaluated
        (t("dataset()"), "dataset()"),
        (t("dataset(x)"), "dataset(x)"),
        (t("dataset(x, [a])"), "dataset(x, [a])"),
        // failures
        ("dataset(5)".into(), "failure(argument, dataset, 1)"),
        ("dataset(\"s\")".into(), "failure(argument, dataset, 1)"),
        ("dataset([assoc(a -> 1), 3])".into(), "failure(argument, dataset, 1)"),
        ("dataset([1, 2])".into(), "failure(argument, dataset, 1)"),
        // an association of single values is one row (G57)
        (t("dataset(assoc(a -> 1))"), "dataset(assoc(a -> [1]))"),
        ("dataset(assoc(a -> [1], b -> [1, 2]))".into(), "failure(shape_mismatch, dataset)"),
        ("dataset([[a, b], [1, 2, 3]])".into(), "failure(shape_mismatch, dataset)"),
        ("dataset([[a, a], [1, 2]])".into(), "failure(duplicate_column, a)"),
        ("dataset([[1]], 3)".into(), "failure(argument, dataset, 2)"),
        ("dataset([1], [a])".into(), "failure(argument, dataset, 1)"),
        ("dataset(1/0)".into(), "failure(division_by_zero)"),
    ];
    run(&mut e, &cases);
}

#[test]
fn rows_as_list_elements() {
    let mut e = engine(1, false);
    check(&mut e, &[(PEOPLE, "null")]);
    let cases = [
        ("length(ds)".to_string(), "3"),
        ("length(dataset([]))".into(), "0"),
        ("ds[2]".into(), "assoc(\"name\" -> \"Bob\", \"age\" -> 25, \"city\" -> missing())"),
        ("ds[-1]".into(), "assoc(\"name\" -> \"Cy\", \"age\" -> 47, \"city\" -> \"Rome\")"),
        ("ds[2, \"name\"]".into(), "\"Bob\""),
        ("ds[\"age\"]".into(), "[31, 25, 47]"),
        ("ds[\"age\", 2]".into(), "25"),
        (t("ds[[3, 1]]"), "dataset(assoc(\"name\" -> [\"Cy\", \"Ann\"], \"age\" -> [47, 31], \"city\" -> [\"Rome\", \"Oslo\"]))"),
        (t("ds[2:3]"), "dataset(assoc(\"name\" -> [\"Bob\", \"Cy\"], \"age\" -> [25, 47], \"city\" -> [missing(), \"Rome\"]))"),
        (t("ds[4:3]"), "dataset(assoc(\"name\" -> [], \"age\" -> [], \"city\" -> []))"),
        ("ds[1:2, \"name\"]".into(), "[\"Ann\", \"Bob\"]"),
        ("first(ds)[\"name\"]".into(), "\"Ann\""),
        ("last(ds)[\"name\"]".into(), "\"Cy\""),
        ("column(rest(ds), \"name\")".into(), "[\"Bob\", \"Cy\"]"),
        ("column(most(ds), \"name\")".into(), "[\"Ann\", \"Bob\"]"),
        ("column(reverse(ds), \"age\")".into(), "[47, 25, 31]"),
        ("column(take(ds, 2), \"name\")".into(), "[\"Ann\", \"Bob\"]"),
        ("column(take(ds, -1), \"name\")".into(), "[\"Cy\"]"),
        ("column(drop(ds, 1), \"name\")".into(), "[\"Bob\", \"Cy\"]"),
        ("column(drop(ds, -1), \"name\")".into(), "[\"Ann\", \"Bob\"]"),
        ("length(take(ds, 0))".into(), "0"),
        ("column(select(ds, r => r[\"age\"] > 30), \"name\")".into(), "[\"Ann\", \"Cy\"]"),
        ("column(select(ds, r => is_missing(r[\"city\"])), \"name\")".into(), "[\"Bob\"]"),
        ("length(select(ds, r => 7))".into(), "0"),
        ("column(sort_by(ds, \"age\"), \"name\")".into(), "[\"Bob\", \"Ann\", \"Cy\"]"),
        ("column(sort_by(ds, r => -r[\"age\"]), \"name\")".into(), "[\"Cy\", \"Ann\", \"Bob\"]"),
        // missing keys sort last; the sort is stable
        ("column(sort_by(ds, \"city\"), \"name\")".into(), "[\"Ann\", \"Cy\", \"Bob\"]"),
        ("column(sort_by(dataset(assoc(k -> [2, 1, 2, 1], v -> [1, 2, 3, 4])), [k]), v)".into(), "[2, 4, 1, 3]"),
        ("column(sort_by(dataset(assoc(\"k\" -> [2, 1, 2, 1], \"v\" -> [4, 2, 3, 1])), [\"k\", \"v\"]), \"v\")".into(), "[1, 2, 3, 4]"),
        // join stacks rows; the columns are every dataset's, missing where one lacks a column
        (t("join(dataset(assoc(a -> [1])), dataset(assoc(b -> [2], a -> [3])))"), "dataset(assoc(a -> [1, 3], b -> [missing(), 2]))"),
        ("length(join(ds, ds, ds))".into(), "9"),
        // calls the rows cannot answer stay unevaluated
        (t("take(ds, n)[2]"), "n"),
        (t("ds[k]"), "dataset(assoc(\"name\" -> [\"Ann\", \"Bob\", \"Cy\"], \"age\" -> [31, 25, 47], \"city\" -> [\"Oslo\", missing(), \"Rome\"]))[k]"),
        ("is_dataset(take(ds))".into(), "false"),
        // failures
        ("ds[0]".into(), "failure(index, 0, 3)"),
        ("ds[4]".into(), "failure(index, 4, 3)"),
        ("ds[5:6]".into(), "failure(index, 5, 3)"),
        ("ds[1:a]".into(), "failure(argument, part, 2)"),
        ("ds[[1, 7]]".into(), "failure(index, 7, 3)"),
        ("ds[\"nope\"]".into(), "failure(missing_key, \"nope\")"),
        ("first(dataset([]))".into(), "failure(index, 1, 0)"),
        ("last(dataset([]))".into(), "failure(index, -1, 0)"),
        ("rest(dataset([]))".into(), "failure(index, 1, 0)"),
        ("take(ds, 4)".into(), "failure(index, 4, 3)"),
        ("drop(ds, -5)".into(), "failure(index, -5, 3)"),
        ("take(ds, \"a\")".into(), "failure(argument, take, 2)"),
        ("select(ds, r => failure(oops))".into(), "failure(oops)"),
        ("sort_by(ds, \"nope\")".into(), "failure(missing_key, \"nope\")"),
        ("sort_by(ds, [\"age\", \"nope\"])".into(), "failure(missing_key, \"nope\")"),
        ("join(ds, 3)".into(), "failure(argument, join, 2)"),
    ];
    run(&mut e, &cases);
    // the structural meaning on lists is unchanged
    check(&mut e, &[("take(f(a, b, c), 2)", "f(a, b)"), ("length([1, 2])", "2"), ("join([1], [2])", "[1, 2]")]);
}

#[test]
fn columns_and_computed_columns() {
    let mut e = engine(1, false);
    check(&mut e, &[(PEOPLE, "null")]);
    let cases = [
        ("columns(ds)".to_string(), "[\"name\", \"age\", \"city\"]"),
        ("column(ds, \"city\")".into(), "[\"Oslo\", missing(), \"Rome\"]"),
        (t("columns(ds, [\"age\", \"name\"])"), "dataset(assoc(\"age\" -> [31, 25, 47], \"name\" -> [\"Ann\", \"Bob\", \"Cy\"]))"),
        (t("columns(ds, \"age\")"), "dataset(assoc(\"age\" -> [31, 25, 47]))"),
        ("rows(dataset(assoc(a -> [1, 2], b -> [x, y])))".into(), "[assoc(a -> 1, b -> x), assoc(a -> 2, b -> y)]"),
        ("rows(dataset([]))".into(), "[]"),
        ("dataset(rows(ds)) === ds".into(), "true"),
        ("column(add_column(ds, \"next\", r => r[\"age\"] + 1), \"next\")".into(), "[32, 26, 48]"),
        ("columns(add_column(ds, \"n\", [1, 2, 3]))".into(), "[\"name\", \"age\", \"city\", \"n\"]"),
        // an existing column is replaced in place
        ("columns(add_column(ds, \"name\", [1, 2, 3]))".into(), "[\"name\", \"age\", \"city\"]"),
        ("column(add_column(ds, \"name\", [1, 2, 3]), \"name\")".into(), "[1, 2, 3]"),
        ("length(add_column(dataset([]), \"a\", [1, 2]))".into(), "2"),
        (t("column(x, \"a\")"), "column(x, \"a\")"),
        ("column(5, \"a\")".into(), "failure(argument, column, 1)"),
        ("column(ds, \"nope\")".into(), "failure(missing_key, \"nope\")"),
        ("columns(ds, [\"age\", \"age\"])".into(), "failure(duplicate_column, \"age\")"),
        ("columns(ds, [\"x\"])".into(), "failure(missing_key, \"x\")"),
        ("add_column(ds, \"z\", [1, 2])".into(), "failure(shape_mismatch, add_column)"),
        ("add_column(ds, \"z\", r => failure(bad))".into(), "failure(bad)"),
        ("rows(assoc())".into(), "failure(argument, rows, 1)"),
        ("is_missing(missing())".into(), "true"),
        ("is_missing(missing(\"not measured\"))".into(), "true"),
        ("is_missing(null)".into(), "false"),
        ("missing()".into(), "missing()"),
    ];
    run(&mut e, &cases);
}

/// Sales by region and year (one amount missing).
const SALES: &str = "sales = dataset([[\"region\", \"year\", \"amount\"], [\"north\", 2020, 10], [\"south\", 2020, 7], \
    [\"north\", 2021, 12], [\"south\", 2021, missing()], [\"north\", 2021, 3]]); null";

#[test]
fn grouping_and_aggregates() {
    let mut e = engine(1, false);
    check(&mut e, &[(PEOPLE, "null"), (SALES, "null")]);
    let cases = [
        (t("group_by(ds, r => r[\"age\"] > 30)"), "assoc(true -> dataset(assoc(\"name\" -> [\"Ann\", \"Cy\"], \"age\" -> [31, 47], \"city\" -> [\"Oslo\", \"Rome\"])), false -> dataset(assoc(\"name\" -> [\"Bob\"], \"age\" -> [25], \"city\" -> [missing()])))"),
        (t("group_by(sales, \"region\", length)"), "assoc(\"north\" -> 3, \"south\" -> 2)"),
        (t("group_by(sales, [\"region\", \"year\"], length)"), "assoc([\"north\", 2020] -> 1, [\"south\", 2020] -> 1, [\"north\", 2021] -> 2, [\"south\", 2021] -> 1)"),
        (t("map_values(length, group_by(ds, \"city\"))"), "assoc(\"Oslo\" -> 1, missing() -> 1, \"Rome\" -> 1)"),
        // the whole dataset: mean of integers is exact; count is the number of rows
        (t("aggregate(ds, [\"age\" -> mean, \"oldest\" -> [\"age\", max], count])"), "dataset(assoc(\"age\" -> [103/3], \"oldest\" -> [47], \"count\" -> [3]))"),
        // missing values are skipped: count of a column counts its values
        (t("aggregate(ds, [\"city\" -> count, \"rows\" -> [\"city\", length]])"), "dataset(assoc(\"city\" -> [2], \"rows\" -> [2]))"),
        (t("aggregate(sales, \"region\", [\"amount\" -> total, \"avg\" -> [\"amount\", mean], count])"), "dataset(assoc(\"region\" -> [\"north\", \"south\"], \"amount\" -> [25, 7], \"avg\" -> [25/3, 7], \"count\" -> [3, 2]))"),
        (t("aggregate(sales, [\"region\", \"year\"], \"amount\" -> total)"), "dataset(assoc(\"region\" -> [\"north\", \"south\", \"north\", \"south\"], \"year\" -> [2020, 2020, 2021, 2021], \"amount\" -> [10, 7, 15, 0]))"),
        (t("aggregate(sales, r => r[\"year\"] > 2020, \"amount\" -> min)"), "dataset(assoc(\"key\" -> [false, true], \"amount\" -> [7, 3]))"),
        (t("aggregate(group_by(sales, \"region\"), [\"amount\" -> max])"), "dataset(assoc(\"key\" -> [\"north\", \"south\"], \"amount\" -> [12, 7]))"),
        // a custom function gets the list of the values that are not missing
        (t("aggregate(sales, \"region\", \"amount\" -> (xs => xs))"), "dataset(assoc(\"region\" -> [\"north\", \"south\"], \"amount\" -> [[10, 12, 3], [7]]))"),
        // no values: total and count 0, mean, min, max missing
        (t("aggregate(dataset(assoc(\"x\" -> [missing(), missing()])), [\"x\" -> mean, \"t\" -> [\"x\", total], \"m\" -> [\"x\", max], count])"), "dataset(assoc(\"x\" -> [missing()], \"t\" -> [0], \"m\" -> [missing()], \"count\" -> [2]))"),
        // integers and reals: a mixed mean is real, min and max give the element as it is
        (t("aggregate(dataset(assoc(\"x\" -> [1.5, 2, missing()])), [\"x\" -> mean, \"lo\" -> [\"x\", min], \"hi\" -> [\"x\", max]])"), "dataset(assoc(\"x\" -> [1.75], \"lo\" -> [1.5], \"hi\" -> [2]))"),
        (t("aggregate(dataset(assoc(\"x\" -> [9223372036854775807, 9223372036854775807])), [\"x\" -> total])"), "dataset(assoc(\"x\" -> [18446744073709551614]))"),
        (t("aggregate(dataset(assoc(\"x\" -> [1/2, 1/3, missing()])), [\"x\" -> total, \"m\" -> [\"x\", mean], \"lo\" -> [\"x\", min]])"), "dataset(assoc(\"x\" -> [5/6], \"m\" -> [5/12], \"lo\" -> [1/3]))"),
        (t("aggregate(dataset(assoc(\"x\" -> [a, b])), [\"x\" -> total])"), "dataset(assoc(\"x\" -> [a + b]))"),
        // failures
        ("group_by(ds, \"nope\")".into(), "failure(missing_key, \"nope\")"),
        ("aggregate(ds, [\"nope\" -> total])".into(), "failure(missing_key, \"nope\")"),
        ("aggregate(ds, [\"n\" -> [\"nope\", total]])".into(), "failure(missing_key, \"nope\")"),
        ("aggregate(ds, [3])".into(), "failure(argument, aggregate, 2)"),
        ("aggregate(ds, \"name\", [3])".into(), "failure(argument, aggregate, 3)"),
        ("aggregate(ds, \"name\", [\"name\" -> count])".into(), "failure(duplicate_column, \"name\")"),
        ("aggregate(ds, \"name\", [\"age\" -> (xs => xs[9])])".into(), "failure(index, 9, 1)"),
        ("aggregate(assoc(a -> 1), [count])".into(), "failure(argument, aggregate, 1)"),
        ("aggregate(5, [count])".into(), "failure(argument, aggregate, 1)"),
        (t("aggregate(x, [count])"), "aggregate(x, [count])"),
    ];
    run(&mut e, &cases);
}

#[test]
fn pivot_tables() {
    let mut e = engine(1, false);
    check(&mut e, &[(SALES, "null")]);
    let cases = [
        (t("pivot(sales, \"region\", \"year\", \"amount\", total)"), "dataset(assoc(\"region\" -> [\"north\", \"south\"], 2020 -> [10, 7], 2021 -> [15, 0]))"),
        (t("pivot(sales, \"region\", \"year\", \"amount\", count)"), "dataset(assoc(\"region\" -> [\"north\", \"south\"], 2020 -> [1, 1], 2021 -> [2, 0]))"),
        (t("pivot(drop(sales, -1), \"region\", \"year\", \"amount\")"), "dataset(assoc(\"region\" -> [\"north\", \"south\"], 2020 -> [10, 7], 2021 -> [12, missing()]))"),
        (t("pivot(take(sales, 3), \"year\", \"region\", \"amount\")"), "dataset(assoc(\"year\" -> [2020, 2021], \"north\" -> [10, 12], \"south\" -> [7, missing()]))"),
        ("pivot(sales, \"region\", \"year\", \"amount\")".into(), "failure(duplicate_entry, [\"north\", 2021])"),
        ("pivot(sales, \"region\", \"nope\", \"amount\")".into(), "failure(missing_key, \"nope\")"),
        ("pivot(dataset(assoc(\"a\" -> [\"a\"], \"b\" -> [\"a\"], \"v\" -> [1])), \"a\", \"b\", \"v\")".into(), "failure(duplicate_column, \"a\")"),
        ("pivot(sales, \"region\", \"year\", \"amount\", xs => failure(g))".into(), "failure(g)"),
    ];
    run(&mut e, &cases);
}

#[test]
fn joins_on_keys() {
    let mut e = engine(1, false);
    check(&mut e, &[
        (PEOPLE, "null"),
        ("cities = dataset([[\"city\", \"country\"], [\"Oslo\", \"Norway\"], [\"Rome\", \"Italy\"], [\"Paris\", \"France\"]]); null", "null"),
        ("l = dataset(assoc(\"k\" -> [1, 2, missing()], \"a\" -> [1, 2, 3])); r = dataset(assoc(\"k\" -> [2, missing(), 2, 4], \"b\" -> [5, 6, 7, 8])); null", "null"),
    ]);
    let cases = [
        (t("join_on(ds, cities, \"city\")"), "dataset(assoc(\"city\" -> [\"Oslo\", \"Rome\"], \"name\" -> [\"Ann\", \"Cy\"], \"age\" -> [31, 47], \"country\" -> [\"Norway\", \"Italy\"]))"),
        (t("join_on(ds, cities, \"city\", how: left)"), "dataset(assoc(\"city\" -> [\"Oslo\", missing(), \"Rome\"], \"name\" -> [\"Ann\", \"Bob\", \"Cy\"], \"age\" -> [31, 25, 47], \"country\" -> [\"Norway\", missing(), \"Italy\"]))"),
        (t("join_on(ds, cities, \"city\", how: \"right\")"), "dataset(assoc(\"city\" -> [\"Oslo\", \"Rome\", \"Paris\"], \"name\" -> [\"Ann\", \"Cy\", missing()], \"age\" -> [31, 47, missing()], \"country\" -> [\"Norway\", \"Italy\", \"France\"]))"),
        // every pair of matching rows; missing keys match nothing
        (t("join_on(l, r, \"k\")"), "dataset(assoc(\"k\" -> [2, 2], \"a\" -> [2, 2], \"b\" -> [5, 7]))"),
        (t("join_on(l, r, \"k\", how: outer)"), "dataset(assoc(\"k\" -> [1, 2, 2, missing(), missing(), 4], \"a\" -> [1, 2, 2, 3, missing(), missing()], \"b\" -> [missing(), 5, 7, missing(), 6, 8]))"),
        (t("join_on(l, r, \"k\", how: right)"), "dataset(assoc(\"k\" -> [2, missing(), 2, 4], \"a\" -> [2, missing(), 2, missing()], \"b\" -> [5, 6, 7, 8]))"),
        // a name both sides have gets the suffix _right
        ("columns(join_on(ds, ds, \"name\"))".into(), "[\"name\", \"age\", \"city\", \"age_right\", \"city_right\"]"),
        ("length(join_on(ds, ds, [\"name\", \"age\"]))".into(), "3"),
        ("length(join_on(ds, dataset([]) , \"name\"))".into(), "failure(missing_key, \"name\")"),
        ("join_on(ds, ds, \"name\", how: sideways)".into(), "failure(bad_option, how, sideways)"),
        ("join_on(ds, ds, \"name\", size: 1)".into(), "failure(unknown_option, size)"),
        ("join_on(ds, 2, \"name\")".into(), "failure(argument, join_on, 2)"),
        ("join_on(dataset(assoc(1 -> [1], 2 -> [1])), dataset(assoc(1 -> [1], 2 -> [2])), 1)".into(), "failure(duplicate_column, 2)"),
    ];
    run(&mut e, &cases);
}

#[test]
fn printed_table() {
    let mut e = engine(1, false);
    check(&mut e, &[(PEOPLE, "null")]);
    let table = "name | age | city\n\
                 -----+-----+--------\n\
                 Ann  |  31 | Oslo\n\
                 Bob  |  25 | missing\n\
                 Cy   |  47 | Rome\n\
                 dataset: 3 rows, 3 columns";
    assert_eq!(eval(&mut e, "ds"), table);
    assert_eq!(eval(&mut e, "dataset([])"), "dataset: 0 rows, 0 columns");
    assert_eq!(eval(&mut e, "dataset([[\"a\"]])"), "a\n---\ndataset: 0 rows, 1 column");
    // long datasets show their first 15 and last 5 rows
    let long = eval(&mut e, "dataset(assoc(\"i\" -> range(1000), \"x\" -> 0.5 * range(1000)))");
    let lines: Vec<&str> = long.lines().collect();
    assert_eq!(lines.len(), 2 + 21 + 1, "{long}");
    assert_eq!(lines[0], "   i |     x");
    assert_eq!(lines[2], "   1 |   0.5");
    assert_eq!(lines[17], " ... |   ...");
    assert_eq!(lines[22], "1000 | 500.0");
    assert_eq!(lines[23], "dataset: 1000 rows, 2 columns");
    // long cells are cut, columns past the line width are counted
    let wide = eval(&mut e, "dataset([assoc(map(k => k -> string_join(table(\"x\", 40)), range(10)))])");
    assert!(wide.lines().all(|l| l.chars().count() <= mc_assoc::MAX_WIDTH), "{wide}");
    assert!(wide.contains("xxx...") && wide.ends_with("dataset: 1 row, 10 columns (7 not shown)"), "{wide}");
    // inside other values a dataset prints as its term
    assert_eq!(eval(&mut e, "[dataset(assoc(a -> [1]))]"), "[dataset(assoc(a -> [1]))]");
}
