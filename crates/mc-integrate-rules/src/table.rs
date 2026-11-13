//! The rule table: the rule files parsed once per process, their names checked, and the dispatch keys.
//!
//! **Dispatch.** Every rule's left side names the functions it needs (`sin`, `cos`, `atan`, ...); an integrand
//! can only match rules whose functions all occur in it (in a subterm that depends on `x`). Each function has a
//! bit, each rule a mask; the candidates of an integrand are the rules whose mask is inside the integrand's, found
//! by one pass over a list of masks grouped by mask (a few hundred distinct masks for thousands of rules), in rule
//! order. This is a decision on head structure in the sense of a discrimination net, flattened to the one question
//! that separates the rule sections (which functions occur); the matcher then decides the rest.
use crate::syntax::{parse_file, RuleText, N};
use std::sync::OnceLock;

/// The rule files, in the order they are tried (a file's rules in their order).
pub const FILES: &[(&str, &str)] = &[
    ("trig_sin_cos", include_str!("../rules/trig_sin_cos.rules")),
    ("trig_tan_sec", include_str!("../rules/trig_tan_sec.rules")),
    ("trig_x", include_str!("../rules/trig_x.rules")),
    ("hyper", include_str!("../rules/hyper.rules")),
    ("inverse_trig", include_str!("../rules/inverse_trig.rules")),
    ("inverse_hyper", include_str!("../rules/inverse_hyper.rules")),
    ("binomial", include_str!("../rules/binomial.rules")),
    ("log", include_str!("../rules/log.rules")),
    ("special", include_str!("../rules/special.rules")),
];

/// Functions that have a dispatch bit (the functions rules are written for). A function outside the list in an
/// integrand sets the bit `OTHER`; rules never require it.
pub const FUNCTIONS: &[&str] = &[
    "sin",
    "cos",
    "tan",
    "cot",
    "sec",
    "csc",
    "sinh",
    "cosh",
    "tanh",
    "coth",
    "sech",
    "csch",
    "asin",
    "acos",
    "atan",
    "acot",
    "asec",
    "acsc",
    "asinh",
    "acosh",
    "atanh",
    "acoth",
    "asech",
    "acsch",
    "log",
    "exp",
    "erf",
    "erfc",
    "erfi",
    "exp_integral_ei",
    "exp_integral_e",
    "log_integral",
    "sin_integral",
    "cos_integral",
    "sinh_integral",
    "cosh_integral",
    "fresnel_s",
    "fresnel_c",
    "gamma",
    "polylog",
    "product_log",
    "zeta",
    "abs",
    "polygamma",
    "airy_ai_prime",
    "airy_bi_prime",
    "weierstrass_p_prime",
    "mathieu_c_prime",
    "mathieu_s_prime",
    "elliptic_exp_prime",
];

/// Functions a right side may build (beyond [`FUNCTIONS`]): special functions of answers.
pub const ANSWER_FUNCTIONS: &[&str] = &[
    "hypergeometric_2f1",
    "hypergeometric_pfq",
    "appell_f1",
    "elliptic_e",
    "elliptic_f",
    "elliptic_pi",
    "beta",
    "sign",
    "sqrt",
    "airy_ai",
    "airy_bi",
    "weierstrass_p",
    "mathieu_c",
    "mathieu_s",
    "elliptic_exp",
];

/// The utilities of right sides (evaluated by the interpreter, see `crate::run`).
pub const RHS_UTILITIES: &[&str] = &["int", "subst", "int_part", "frac_part", "numerator", "denominator", "simp"];

/// Functions of the language a right side calls through the evaluator (algebraic rewriting of an integrand).
pub const EVAL_UTILITIES: &[&str] = &["expand", "together", "trig_expand", "trig_reduce", "trig_to_exp"];

/// The predicates of conditions (see `crate::cond`).
pub const PREDICATES: &[&str] = &[
    "free",
    "integer",
    "rational",
    "eq",
    "ne",
    "pos",
    "neg",
    "half_integer",
    "odd",
    "even",
    "nonzero_number",
    "number",
    "symbol",
    "complex_free",
];

/// The bit of functions not in [`FUNCTIONS`].
pub const OTHER: u64 = 1 << 63;

/// The bit of a function name.
pub fn bit(name: &str) -> u64 {
    FUNCTIONS.iter().position(|f| *f == name).map_or(OTHER, |i| 1u64 << i)
}

/// One rule of the table.
#[derive(Debug)]
pub struct Rule {
    /// its number in the table
    pub id: usize,
    /// the file it comes from
    pub file: &'static str,
    /// the rule as written
    pub text: RuleText,
    /// the functions it needs
    pub mask: u64,
    /// per pattern variable: [`RUN_ADD`] or [`RUN_MUL`] when every occurrence is an argument of a sum (a
    /// product), so it may take several of its arguments (it stands for their sum or product); [`SINGLE`]
    /// otherwise
    pub runs: Vec<u8>,
}

/// A pattern variable that takes one term.
pub const SINGLE: u8 = 0;
/// A pattern variable that takes one or more terms of a sum.
pub const RUN_ADD: u8 = 1;
/// A pattern variable that takes one or more factors of a product.
pub const RUN_MUL: u8 = 2;

/// The kinds of the pattern variables of a left side (see [`Rule::runs`]).
pub fn run_kinds(lhs: &N, nvars: usize) -> Vec<u8> {
    // per variable: the parent kinds seen (bit 0 sum, bit 1 product, bit 2 anything else)
    fn walk(n: &N, parent: u8, seen: &mut [u8]) {
        match n {
            N::Var(i) | N::Opt(i) => seen[*i as usize] |= parent,
            N::Add(v) => v.iter().for_each(|a| walk(a, 1, seen)),
            N::Mul(v) => v.iter().for_each(|a| walk(a, 2, seen)),
            N::Pow(a, b) => {
                walk(a, 4, seen);
                walk(b, 4, seen);
            }
            N::Call(_, v) => v.iter().for_each(|a| walk(a, 4, seen)),
            _ => {}
        }
    }
    let mut seen = vec![0u8; nvars];
    walk(lhs, 4, &mut seen);
    seen.iter()
        .map(|&s| match s {
            1 => RUN_ADD,
            2 => RUN_MUL,
            _ => SINGLE,
        })
        .collect()
}

/// The table.
#[derive(Debug)]
pub struct Table {
    /// all rules in order
    pub rules: Vec<Rule>,
    /// the distinct masks with the rules having each (rules in table order within a group)
    pub groups: Vec<(u64, Vec<usize>)>,
}

impl Table {
    /// The rules that may match an integrand whose functions are `mask`, in table order.
    pub fn candidates(&self, mask: u64, out: &mut Vec<usize>) {
        out.clear();
        for (m, ids) in &self.groups {
            if m & !mask == 0 {
                out.extend_from_slice(ids);
            }
        }
        out.sort_unstable();
    }
}

static TABLE: OnceLock<Result<Table, String>> = OnceLock::new();

/// The table, built on first use; an error names the file and line of a bad rule.
pub fn table() -> Result<&'static Table, &'static str> {
    TABLE.get_or_init(build).as_ref().map_err(|e| e.as_str())
}

fn build() -> Result<Table, String> {
    let mut rules = vec![];
    for (file, text) in FILES {
        let parsed = parse_file(text).map_err(|e| format!("{file}.rules {e}"))?;
        for r in parsed {
            check(&r.lhs, true).map_err(|e| format!("{file}.rules line {}: {e}", r.line))?;
            check(&r.rhs, false).map_err(|e| format!("{file}.rules line {}: {e}", r.line))?;
            if let Some(c) = &r.cond {
                check_cond(c).map_err(|e| format!("{file}.rules line {}: {e}", r.line))?;
            }
            let mut mask = 0;
            heads(&r.lhs, &mut mask);
            let runs = run_kinds(&r.lhs, r.vars.len());
            rules.push(Rule { id: rules.len(), file, text: r, mask, runs });
        }
    }
    let mut groups: Vec<(u64, Vec<usize>)> = vec![];
    for r in &rules {
        match groups.iter_mut().find(|(m, _)| *m == r.mask) {
            Some((_, v)) => v.push(r.id),
            None => groups.push((r.mask, vec![r.id])),
        }
    }
    Ok(Table { rules, groups })
}

fn heads(n: &N, mask: &mut u64) {
    match n {
        N::Call(f, args) => {
            if &**f != "sqrt" {
                *mask |= bit(f);
            }
            args.iter().for_each(|a| heads(a, mask));
        }
        N::Add(v) | N::Mul(v) | N::And(v) | N::Or(v) => v.iter().for_each(|a| heads(a, mask)),
        N::Pow(a, b) | N::Cmp(_, a, b) => {
            heads(a, mask);
            heads(b, mask);
        }
        N::Not(a) => heads(a, mask),
        _ => {}
    }
}

fn check(n: &N, lhs: bool) -> Result<(), String> {
    match n {
        N::Call(f, args) => {
            let known = FUNCTIONS.contains(&&**f) || ANSWER_FUNCTIONS.contains(&&**f);
            let util = !lhs && (RHS_UTILITIES.contains(&&**f) || EVAL_UTILITIES.contains(&&**f));
            if !known && !util {
                return Err(format!("unknown function {f}"));
            }
            args.iter().try_for_each(|a| check(a, lhs))
        }
        N::Add(v) | N::Mul(v) => v.iter().try_for_each(|a| check(a, lhs)),
        N::Pow(a, b) => {
            check(a, lhs)?;
            check(b, lhs)
        }
        N::Cmp(..) | N::And(_) | N::Or(_) | N::Not(_) => Err("a condition outside the condition".into()),
        N::Opt(_) if !lhs => Err("an optional variable on the right side".into()),
        _ => Ok(()),
    }
}

fn check_cond(n: &N) -> Result<(), String> {
    match n {
        N::And(v) | N::Or(v) => v.iter().try_for_each(check_cond),
        N::Not(a) => check_cond(a),
        N::Cmp(_, a, b) => {
            check(a, false)?;
            check(b, false)
        }
        N::Call(f, args) if PREDICATES.contains(&&**f) => args.iter().try_for_each(|a| check(a, false)),
        N::Call(f, _) => Err(format!("unknown predicate {f}")),
        _ => Err("a condition must be a predicate, a comparison or a combination of them".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_builds() {
        let t = super::table().unwrap();
        assert!(!t.rules.is_empty());
        let mut c = vec![];
        t.candidates(super::bit("sin") | super::bit("cos"), &mut c);
        assert!(c.iter().all(|&i| t.rules[i].mask & !(super::bit("sin") | super::bit("cos")) == 0));
    }
}
