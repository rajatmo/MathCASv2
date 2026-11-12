//! Editor warnings that are not errors (round 20). [`lint_calls`]: `x(x - a)` is a call of `x` on `x - a`, never a
//! product, but in typeset input (and to most readers) it looks like one. A call is flagged when its head is a
//! plain name that has no definition (the editor's `known` says which names have one; the text's own `fn`, `rule`,
//! `f(x) := ...`, `f = ...`, `sym` and `let` definitions count too), is not bound by a lambda, `fn` parameter,
//! `let`, `for` or a pattern in the same statement, and is used as an operand of arithmetic or a comparison in the
//! same statement (a variable: the `x` of `x - a`, `k + 1`, `y == 0`). A name that only appears as a plain argument
//! or list element (`d_solve(..., y, x)`, `free_of(h(1), h)`) is not counted as a variable, and a name written with
//! a prime (`y'(x)`) is a function. Real calls (`sin(x)`, a defined `f(x)`, a pure function's parameter
//! `g => g(1)`, the unknown `y(x)` of a differential equation) are never flagged. Each warning carries its quick
//! fix: `*` inserted after the name.
use crate::ast::{Arg, Expr, ExprKind, Param, ParamKind, Program, Stmt, StmtKind};
use std::collections::HashSet;
use std::ops::Range;

/// A warning on a range of the text, with its quick fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lint {
    /// the flagged text (the called name)
    pub range: Range<usize>,
    /// what the warning says
    pub message: String,
    /// the quick fix: each byte range replaced by its text, in text order and not overlapping (an insertion is an
    /// empty range); empty when there is nothing to fix by an edit
    pub fix: Vec<(Range<usize>, String)>,
    /// what the fix does, for a menu ("insert `*`", "write sin(x)")
    pub fix_label: String,
}

impl Lint {
    /// `src` with the quick fix applied.
    pub fn fixed(&self, src: &str) -> String {
        let mut out = src.to_string();
        for (r, t) in self.fix.iter().rev() {
            if src.get(r.clone()).is_some() {
                out.replace_range(r.clone(), t);
            }
        }
        out
    }

    /// The quick fix as one replacement: the range from the first edit's start to the last one's end and the text
    /// that replaces it (an editor applies it as one step). `None` when there is no edit.
    pub fn replacement(&self, src: &str) -> Option<(Range<usize>, String)> {
        let start = self.fix.first()?.0.start;
        let end = self.fix.last()?.0.end;
        let mut text = src.get(start..end)?.to_string();
        for (r, t) in self.fix.iter().rev() {
            text.replace_range(r.start - start..r.end - start, t);
        }
        Some((start..end, text))
    }
}

/// The warnings of [`lint`](self) for `src`; `known(name)` says whether a name has a definition (the session's
/// names, the library's functions).
/// Texts longer than [`crate::LEXICAL_ONLY_ABOVE`] get none (as they get no scopes in the colouring).
pub fn lint_calls(src: &str, known: &(dyn Fn(&str) -> bool + Sync)) -> Vec<Lint> {
    lint_calls_with(src, known, &|_: &str| false)
}

/// [`lint_calls`], where `function(name)` also says which names are functions (the session's and the library's):
/// such a name juxtaposed with an operand (`sin x`, `f (x)`) is a product that was likely meant as a call (round 35).
pub fn lint_calls_with(
    src: &str,
    known: &(dyn Fn(&str) -> bool + Sync),
    function: &(dyn Fn(&str) -> bool + Sync),
) -> Vec<Lint> {
    if src.len() > crate::LEXICAL_ONLY_ABOVE {
        return vec![];
    }
    // the parser and the walk recurse per nesting level: on the reserved stack, as the colouring
    crate::stack::on_big_stack(|| {
        let parsed = crate::parse(src);
        lint_program(src, &parsed.program, known, function)
    })
}

/// [`lint_calls_with`] on a parsed program.
pub fn lint_program(src: &str, program: &Program, known: &dyn Fn(&str) -> bool, function: &dyn Fn(&str) -> bool) -> Vec<Lint> {
    let mut defined = HashSet::new();
    let mut functions = HashSet::new();
    for s in &program.stmts {
        definitions(s, &mut defined);
        function_definitions(s, &mut functions);
    }
    let mut out = vec![];
    for s in &program.stmts {
        let mut w = Walk::default();
        w.stmt(s);
        for (name, range) in std::mem::take(&mut w.shadows) {
            out.push(Lint {
                message: format!(
                    "a local named `{name}` hides the function that the operators and brackets of the language lower to (`===` is `same`, `a[i]` is `part`, ...): inside its scope they call the local; pick another name"
                ),
                fix: vec![],
                fix_label: String::new(),
                range,
            });
        }
        // a function juxtaposed with an operand: a product (round 35)
        for (name, head, operand, grouped) in &w.juxtaposed {
            let gap = head.end..operand.start;
            if !(function(name) || functions.contains(name)) || w.bound.contains(name) || !is_space(src, gap.clone()) {
                continue;
            }
            let (Some(head_text), Some(op)) = (src.get(head.clone()), src.get(operand.clone())) else { continue };
            let grouped = *grouped && op.len() >= 2 && op.starts_with('(') && op.ends_with(')');
            let inner = if grouped { &op[1..op.len() - 1] } else { op };
            let call = format!("{head_text}({inner})");
            let fix = if grouped {
                vec![(gap, String::new())]
            } else {
                vec![(gap, "(".to_string()), (operand.end..operand.end, ")".to_string())]
            };
            out.push(Lint {
                range: head.clone(),
                message: format!("{head_text} {op} is the product {head_text}*{op}; for the function write {call}"),
                fix,
                fix_label: format!("write {call}"),
            });
        }
        // a parenthesised expression called with no space: `(a+b)(c+d)`
        for callee in &w.paren_calls {
            let Some(text) = src.get(callee.clone()) else { continue };
            if !text.starts_with('(') {
                continue;
            }
            out.push(Lint {
                range: callee.clone(),
                message: format!("this calls {text}; for the product write a space or `*`"),
                fix: vec![(callee.end..callee.end, "*".to_string())],
                fix_label: "insert `*`".to_string(),
            });
        }
        for (name, head, call) in &w.calls {
            if known(name)
                || defined.contains(name)
                || w.bound.contains(name)
                || w.functions.contains(name)
                || !w.bare.contains(name)
            {
                continue;
            }
            let (Some(head_text), Some(rest)) = (src.get(head.clone()), src.get(head.end..call.end)) else { continue };
            let product = format!("{head_text}*{}", rest.trim_start());
            out.push(Lint {
                range: head.clone(),
                message: format!("{head_text} is called as a function; did you mean {product}?"),
                fix: vec![(head.end..head.end, "*".to_string())],
                fix_label: "insert `*`".to_string(),
            });
        }
    }
    out.sort_by_key(|l| l.range.start);
    out
}

/// Whether `src[range]` is white space and comments only, and not empty: a juxtaposition written with a space.
fn is_space(src: &str, range: Range<usize>) -> bool {
    let Some(gap) = src.get(range) else { return false };
    !gap.is_empty() && gap.lines().all(|l| l.split('#').next().unwrap_or("").trim().is_empty())
}

/// Names the text defines as functions: `fn f`, `rule f(...)`, `f(...) := ...`, `f(...) = ...`.
fn function_definitions(s: &Stmt, out: &mut HashSet<String>) {
    match &s.kind {
        StmtKind::Fn { name, .. } => {
            out.insert(name.clone());
        }
        StmtKind::Rule { lhs, .. } => {
            if let Some(n) = head_name(lhs) {
                out.insert(n);
            }
        }
        StmtKind::Pub(inner) => function_definitions(inner, out),
        StmtKind::Module { body, .. } => body.iter().for_each(|b| function_definitions(b, out)),
        StmtKind::Expr(e) => {
            if let ExprKind::Binary(crate::ast::BinOp::Set | crate::ast::BinOp::SetDelayed, lhs, _) = &e.kind {
                if !matches!(lhs.kind, ExprKind::Name(_)) {
                    if let Some(n) = head_name(lhs) {
                        out.insert(n);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Names a statement defines for the whole text.
fn definitions(s: &Stmt, out: &mut HashSet<String>) {
    match &s.kind {
        StmtKind::Fn { name, .. } | StmtKind::Sym { name, .. } | StmtKind::Let { name, .. } => {
            out.insert(name.clone());
        }
        StmtKind::Rule { lhs, .. } => {
            if let Some(n) = head_name(lhs) {
                out.insert(n);
            }
        }
        StmtKind::Pub(inner) => definitions(inner, out),
        StmtKind::Module { body, .. } => body.iter().for_each(|b| definitions(b, out)),
        StmtKind::Expr(e) => {
            if let ExprKind::Binary(crate::ast::BinOp::Set | crate::ast::BinOp::SetDelayed, lhs, _) = &e.kind {
                if let Some(n) = head_name(lhs) {
                    out.insert(n);
                }
            }
        }
        StmtKind::Import { .. } => {}
    }
}

/// The name a definition's left side defines: `f` of `f`, `f(x_)`, `f(x_)(y_)`.
fn head_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Name(n) => Some(n.clone()),
        ExprKind::Call(f, _) | ExprKind::Typed(f, _) => head_name(f),
        ExprKind::Binary(crate::ast::BinOp::When, l, _) => head_name(l),
        _ => None,
    }
}

/// The functions the reader lowers operators, brackets and forms to, which a local of the same name would hide. The
/// ones people reach for as variable names: `same` for `===`, `index` and `tuple` for the bracket and comma forms,
/// the comparison and logic heads, `part` for `a[i]`, `list` for `[a, b]`.
const LOWERED: &[&str] = &[
    "same", "not_same", "index", "tuple", "part", "list", "equal", "unequal", "less", "less_equal", "greater",
    "greater_equal", "span", "rule", "rule_delayed", "options", "alternatives", "condition", "complex",
];

#[derive(Default)]
struct Walk {
    /// calls of plain names: the name, its range and the whole call's
    calls: Vec<(String, Range<usize>, Range<usize>)>,
    /// names used as operands of arithmetic or comparisons (variables)
    bare: HashSet<String>,
    /// names bound somewhere in the statement
    bound: HashSet<String>,
    /// names written with a prime (`y'`): functions
    functions: HashSet<String>,
    /// locals (`let`, `var`, parameters) named like a function the reader lowers syntax to (R24-06)
    shadows: Vec<(String, Range<usize>)>,
    /// inside arithmetic or a comparison (a name here is a variable), not a plain argument or list element
    arith: bool,
    /// a name followed by a factor in a product: the name, its range, the factor's range, whether the factor is a
    /// parenthesised group (`f (x)`); the lint keeps those written with a space
    juxtaposed: Vec<(String, Range<usize>, Range<usize>, bool)>,
    /// the callees of calls whose callee is arithmetic (`(a+b)(c+d)`); the lint keeps those in parentheses
    paren_calls: Vec<Range<usize>>,
}

impl Walk {
    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e),
            StmtKind::Let { name, value, .. } => {
                self.bound.insert(name.clone());
                self.shadow(name, s.span.start as usize..s.span.end as usize);
                self.expr(value);
            }
            StmtKind::Fn { name, params, options, condition, body, .. } => {
                self.bound.insert(name.clone());
                params.iter().chain(options).for_each(|p| self.param(p));
                if let Some(c) = condition {
                    self.expr(c);
                }
                self.expr(body);
            }
            StmtKind::Rule { lhs, rhs, .. } => {
                // the left side's names are patterns or the defined head: bound, not operands
                self.patterns(lhs);
                self.expr(rhs);
            }
            StmtKind::Pub(inner) => self.stmt(inner),
            StmtKind::Module { body, .. } => body.iter().for_each(|b| self.stmt(b)),
            StmtKind::Sym { name, .. } => {
                self.bound.insert(name.clone());
            }
            StmtKind::Import { .. } => {}
        }
    }

    /// Records a local `name` (searched for within `span`) that is a lowering target.
    fn shadow(&mut self, name: &str, span: Range<usize>) {
        if LOWERED.contains(&name) {
            self.shadows.push((name.to_string(), span));
        }
    }

    fn param(&mut self, p: &Param) {
        match &p.kind {
            ParamKind::Name(n, _) | ParamKind::Rest(n) => {
                self.bound.insert(n.clone());
                self.shadow(n, p.span.start as usize..p.span.end as usize);
            }
            ParamKind::Pattern(e) => self.patterns(e),
        }
        if let Some(d) = &p.default {
            self.expr(d);
        }
    }

    /// Every name of a pattern side counts as bound.
    fn patterns(&mut self, e: &Expr) {
        let mut names = vec![];
        names_in(e, &mut names);
        self.bound.extend(names);
    }

    /// A call's arguments: a plain name there is not (yet) a variable.
    fn args(&mut self, args: &[Arg]) {
        let outer = std::mem::replace(&mut self.arith, false);
        for a in args {
            self.expr(&a.value);
        }
        self.arith = outer;
    }

    /// `e` as an operand of arithmetic or a comparison.
    fn operand(&mut self, e: &Expr) {
        let outer = std::mem::replace(&mut self.arith, true);
        self.expr(e);
        self.arith = outer;
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Name(n) => {
                if self.arith {
                    self.bare.insert(n.clone());
                }
            }
            ExprKind::Pattern { name: Some(n), .. } => {
                self.bound.insert(n.clone());
            }
            // y', y'': y is a function (the unknown of a differential equation)
            ExprKind::Prime(x, _) if matches!(x.kind, ExprKind::Name(_)) => {
                if let ExprKind::Name(n) = &x.kind {
                    self.functions.insert(n.clone());
                }
            }
            ExprKind::Call(f, args) => {
                // derivative(1, 0)(u): u is a function (the unknown of a differential equation)
                if let ExprKind::Call(g, _) = &f.kind {
                    if matches!(&g.kind, ExprKind::Name(d) if d == "derivative") {
                        for a in args {
                            if let ExprKind::Name(n) = &a.value.kind {
                                self.functions.insert(n.clone());
                            }
                        }
                    }
                }
                if let ExprKind::Name(n) = &f.kind {
                    self.calls.push((n.clone(), f.span.start as usize..f.span.end as usize, e.span.start as usize..e.span.end as usize));
                } else {
                    // a string or named argument is a property or option (`(E(4) * E(7))("Coefficients")`), not a factor
                    let factors = args.iter().all(|a| a.name.is_none() && !matches!(a.value.kind, ExprKind::Str(_)));
                    if factors
                        && matches!(f.kind, ExprKind::Add(_) | ExprKind::Mul(_) | ExprKind::Neg(_) | ExprKind::Pow(..) | ExprKind::Num(_))
                    {
                        self.paren_calls.push(f.span.start as usize..f.span.end as usize);
                    }
                    self.expr(f);
                }
                self.args(args);
            }
            ExprKind::Add(xs) => xs.iter().for_each(|(_, x)| self.operand(x)),
            ExprKind::Mul(xs) => {
                for pair in xs.windows(2) {
                    let ((_, a), (op, b)) = (&pair[0], &pair[1]);
                    if let (ExprKind::Name(n), crate::ast::MulOp::Times) = (&a.kind, op) {
                        // a factor in its own parentheses: its span starts at `(` and it is no call or index
                        let grouped = !matches!(
                            b.kind,
                            ExprKind::Call(..) | ExprKind::Index(..) | ExprKind::Prime(..) | ExprKind::List(_) | ExprKind::Pow(..) | ExprKind::Lambda(..)
                        );
                        self.juxtaposed.push((
                            n.clone(),
                            a.span.start as usize..a.span.end as usize,
                            b.span.start as usize..b.span.end as usize,
                            grouped,
                        ));
                    }
                }
                xs.iter().for_each(|(_, x)| self.operand(x))
            }
            ExprKind::Pow(a, b) => {
                self.operand(a);
                self.operand(b);
            }
            ExprKind::Neg(x) | ExprKind::Not(x) => self.operand(x),
            ExprKind::Lambda(params, body) => {
                params.iter().for_each(|p| self.param(p));
                self.expr(body);
            }
            ExprKind::For(v, a, b) => {
                self.bound.insert(v.clone());
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Binary(op, l, r) => {
                use crate::ast::BinOp::*;
                match op {
                    // a definition's left side: its head and patterns are not operands
                    Set | SetDelayed if !matches!(l.kind, ExprKind::Name(_)) => {
                        self.patterns(l);
                        self.expr(r);
                    }
                    Set | SetDelayed => {
                        self.expr(l);
                        self.expr(r);
                    }
                    // comparisons, rules and the other operators: their sides are operands
                    _ => {
                        self.operand(l);
                        self.operand(r);
                    }
                }
            }
            ExprKind::Match(x, arms) => {
                self.expr(x);
                for arm in arms {
                    self.patterns(&arm.pattern);
                    self.expr(&arm.body);
                }
            }
            ExprKind::Block(stmts) => stmts.iter().for_each(|s| self.stmt(s)),
            _ => {
                let mut kids = vec![];
                children(e, &mut kids);
                for k in kids {
                    self.expr(k);
                }
            }
        }
    }
}

/// The direct sub-expressions of the kinds [`Walk::expr`] does not handle itself.
fn children<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match &e.kind {
        ExprKind::Typed(x, _) | ExprKind::Named(_, x) | ExprKind::Neg(x) | ExprKind::Not(x) | ExprKind::SlotFn(x) | ExprKind::Prime(x, _) => {
            out.push(x)
        }
        ExprKind::Add(xs) => out.extend(xs.iter().map(|(_, x)| x)),
        ExprKind::Mul(xs) => out.extend(xs.iter().map(|(_, x)| x)),
        ExprKind::Pow(a, b) | ExprKind::Span(a, b) | ExprKind::While(a, b) => out.extend([&**a, &**b]),
        ExprKind::Nary(_, xs) | ExprKind::List(xs) => out.extend(xs.iter()),
        ExprKind::Index(x, ys) => {
            out.push(x);
            out.extend(ys.iter());
        }
        ExprKind::If(c, a, b) => {
            out.extend([&**c, &**a]);
            if let Some(b) = b {
                out.push(b);
            }
        }
        _ => {}
    }
}

/// Every name in `e` (patterns' names included).
fn names_in(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Name(n) | ExprKind::Pattern { name: Some(n), .. } => out.push(n.clone()),
        ExprKind::Call(f, args) => {
            names_in(f, out);
            args.iter().for_each(|a| names_in(&a.value, out));
        }
        ExprKind::Binary(_, l, r) => {
            names_in(l, out);
            names_in(r, out);
        }
        ExprKind::Lambda(_, b) | ExprKind::For(_, _, b) => names_in(b, out),
        ExprKind::Match(x, _) => names_in(x, out),
        _ => {
            let mut kids = vec![];
            children(e, &mut kids);
            kids.into_iter().for_each(|k| names_in(k, out));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(src: &str) -> Vec<String> {
        let known = |n: &str| ["integrate", "sqrt", "sin", "solve", "table", "map", "d_solve", "plot", "sum"].contains(&n);
        lint_calls(src, &known).into_iter().map(|l| format!("{} {}", &src[l.range.clone()], l.message)).collect()
    }

    #[test]
    fn locals_named_like_lowered_functions_are_flagged() {
        let msgs = |src: &str| lint(src).len();
        assert_eq!(msgs("{ let same = 3; same + 1 }"), 1);
        assert_eq!(msgs("fn f(index) = index + 1"), 1);
        assert_eq!(msgs("map(tuple => tuple + 1, xs)"), 1);
        assert_eq!(msgs("{ let total = 3; total }"), 0);
    }

    #[test]
    fn calls_of_variables_are_flagged() {
        assert_eq!(
            lint("integrate(sqrt(x(x - a)), x)"),
            ["x x is called as a function; did you mean x*(x - a)?"]
        );
        assert!(lint("integrate(sqrt(x (x - a)), x)").is_empty(), "a space before the bracket is a product (round 35)");
        assert_eq!(lint("table(k(k + 1), [k, 1, 5])").len(), 1, "an iterator");
        assert_eq!(lint("solve(y(y - 1) == 0, y)").len(), 1, "a solve variable");
        assert_eq!(lint("t(1 + t)").len(), 1, "a bare operand of its own call");
        assert_eq!(lint("a = 1\nx(x - 1)").len(), 1);
    }

    #[test]
    fn real_calls_are_not_flagged() {
        assert!(lint("integrate(sin(x), x)").is_empty(), "a known function");
        assert!(lint("f(x) := x^2\nintegrate(f(x), x)").is_empty(), "defined in the text");
        assert!(lint("fn g(x) = x + 1\ng(g)").is_empty());
        assert!(lint("map(g => g(1) + g, [sin])").is_empty(), "a lambda's parameter");
        assert!(lint("fn apply(h, v) = h(v) + h").is_empty(), "a fn parameter");
        assert!(lint("y(2) + 3").is_empty(), "not used as a variable");
        assert!(lint("x(1)\nx + 1").is_empty(), "another statement");
        assert!(lint("rule q(x_) := x\nq(q)").is_empty(), "a rule's head");
        assert!(lint("integrate(x*(x - a), x)").is_empty());
        assert!(lint("d_solve(y'(x) == y(x), y(x), x)").is_empty(), "the unknown of a differential equation");
        assert!(lint("d_solve(y'(x) == 2*y(x), y, x)").is_empty(), "a function given by name");
    }

    #[test]
    fn the_fix_inserts_a_star() {
        let src = "integrate(sqrt(x(x - a)), x)";
        let l = &lint_calls(src, &|n: &str| n == "integrate" || n == "sqrt")[0];
        let fixed = l.fixed(src);
        assert_eq!(fixed, "integrate(sqrt(x*(x - a)), x)");
        assert_eq!(l.fix_label, "insert `*`");
        assert!(lint_calls(&fixed, &|n: &str| n == "integrate" || n == "sqrt").is_empty());
    }

    /// The round 35 lints: `known` and `function` as a session that knows `sin`, `f` as functions and `pi`, `a` as
    /// values.
    fn lint35(src: &str) -> Vec<Lint> {
        let known = |n: &str| ["sin", "f", "pi", "a", "integrate"].contains(&n);
        let function = |n: &str| ["sin", "f", "integrate"].contains(&n);
        lint_calls_with(src, &known, &function)
    }

    #[test]
    fn a_function_juxtaposed_with_an_operand_is_flagged() {
        let l = lint35("sin x");
        assert_eq!(l.len(), 1, "{l:?}");
        assert_eq!(l[0].message, "sin x is the product sin*x; for the function write sin(x)");
        assert_eq!(l[0].range, 0..3);
        assert_eq!(l[0].fixed("sin x"), "sin(x)");
        assert_eq!(l[0].fix_label, "write sin(x)");
        let l = lint35("1 + f (x + 1)");
        assert_eq!(l.len(), 1, "{l:?}");
        assert_eq!(l[0].message, "f (x + 1) is the product f*(x + 1); for the function write f(x + 1)");
        assert_eq!(l[0].fixed("1 + f (x + 1)"), "1 + f(x + 1)");
        // one factor only: `sin x y` is sin(x) y
        assert_eq!(lint35("2 sin x y")[0].fixed("2 sin x y"), "2 sin(x) y");
        assert_eq!(lint35("sin x^2")[0].fixed("sin x^2"), "sin(x^2)");
        // functions the text defines
        assert_eq!(lint_calls_with("g(x_) := x^2\ng y", &|_: &str| false, &|_: &str| false).len(), 1);
        assert_eq!(lint_calls_with("fn h(x) = x\nh 2", &|_: &str| false, &|_: &str| false).len(), 1);
        // the fixed text has no warning left
        for src in ["sin x", "f (x)", "2 sin x y"] {
            let fixed = lint35(src)[0].fixed(src);
            assert!(lint35(&fixed).is_empty(), "{src:?} -> {fixed:?}");
        }
    }

    #[test]
    fn products_of_values_are_not_flagged() {
        for src in ["2 pi r", "a b", "x y", "sin(x) y", "sin*x", "2 sin(x)", "map(sin => sin x, [1])", "x sin(y)"] {
            assert!(lint35(src).is_empty(), "{src:?}: {:?}", lint35(src));
        }
        // without the function predicate (the kernel's notes) nothing is a function
        assert!(lint_calls("sin x", &|n: &str| n == "sin").is_empty());
    }

    #[test]
    fn a_parenthesised_expression_called_is_flagged() {
        let src = "(a + b)(c + d)";
        let l = lint35(src);
        assert_eq!(l.len(), 1, "{l:?}");
        assert_eq!(l[0].message, "this calls (a + b); for the product write a space or `*`");
        assert_eq!(l[0].range, 0..7);
        assert_eq!(l[0].fixed(src), "(a + b)*(c + d)");
        assert!(lint35(&l[0].fixed(src)).is_empty());
        assert_eq!(lint35("(2 x)(y)").len(), 1);
        assert_eq!(lint35("(-x)(y)").len(), 1);
        // a space makes it a product; calls of a lambda, a name or a call are calls
        for src in ["(a + b) (c + d)", "(x => x^2)(3)", "(f)(x)", "f(1)(2)", "derivative(1)(y)"] {
            assert!(lint35(src).is_empty(), "{src:?}: {:?}", lint35(src));
        }
    }
}
