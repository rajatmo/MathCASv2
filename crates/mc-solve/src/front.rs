//! The front end of the solving functions (item 11C): options, the settings they give the solver, equations
//! normalised (vector and matrix equations to scalar ones), the value forms (`solve_values`, `n_solve_values`) and
//! solving modulo an integer.
//!
//! **Options** (a trailing `options(k -> v, ...)`, written `f(..., k: v)`): `cubics`, `quartics` (radicals instead
//! of root objects for cubics and quartics with numeric coefficients), `generated_parameters` (the head of the
//! integer parameters of periodic solutions, `c` by default), `max_roots` (at most that many solutions),
//! `verify_solutions` (accepted; every solution is checked by substitution in any case), `inverse_functions`
//! (`false`: no inversion of functions of the unknowns), `modulus` (solutions modulo an integer), `method` and
//! `working_precision` (accepted), and for `roots` also `multiplicity` and `equated_to`. The settings apply to the
//! call and to the calls of the solving functions it makes (they are kept for the dynamic extent of the call, so
//! that the domain and condition layer, which calls `solve` again, solves with the same settings; decision 11C.1).
use crate::cx::Cfg;
use mc_api::{names, Store, View, T};
use mc_core::Heap;
use std::cell::RefCell;

/// The settings an option list gives (unset fields are inherited from the enclosing call).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Settings {
    pub cubics: Option<bool>,
    pub quartics: Option<bool>,
    pub periodic: Option<bool>,
    pub gen: Option<String>,
    pub max_roots: Option<usize>,
    pub inverse_functions: Option<bool>,
    /// solving over the reals (set by the domain layer for its inner calls)
    pub reals: Option<bool>,
}

impl Settings {
    /// `self` with the fields `inner` sets replaced.
    fn merged(&self, inner: &Settings) -> Settings {
        Settings {
            cubics: inner.cubics.or(self.cubics),
            quartics: inner.quartics.or(self.quartics),
            periodic: inner.periodic.or(self.periodic),
            gen: inner.gen.clone().or_else(|| self.gen.clone()),
            max_roots: inner.max_roots.or(self.max_roots),
            inverse_functions: inner.inverse_functions.or(self.inverse_functions),
            reals: inner.reals.or(self.reals),
        }
    }

    /// The solver settings for a function whose own default for the formulas is `formulas` (`roots` gives radicals
    /// for cubics and quartics, `solve` root objects).
    pub fn cfg(&self, formulas: bool) -> Cfg {
        Cfg {
            cubics: self.cubics.unwrap_or(formulas),
            quartics: self.quartics.unwrap_or(formulas),
            periodic: self.periodic.unwrap_or(true) && self.inverse_functions != Some(false),
            reals: self.reals.unwrap_or(false),
        }
    }
}

thread_local! {
    /// The settings of the solving calls in progress on this thread, innermost last.
    static STACK: RefCell<Vec<Settings>> = const { RefCell::new(Vec::new()) };
}

/// The settings in force: those of the innermost call that set any.
pub(crate) fn current() -> Settings {
    STACK.with(|s| s.borrow().last().cloned().unwrap_or_default())
}

/// The settings of a call, in force while it lives.
pub(crate) struct Scope {
    pushed: bool,
}

impl Scope {
    /// Push `own` merged into the enclosing settings (nothing when `own` sets nothing).
    pub fn enter(own: &Settings) -> Scope {
        if *own == Settings::default() {
            return Scope { pushed: false };
        }
        let merged = current().merged(own);
        STACK.with(|s| s.borrow_mut().push(merged));
        Scope { pushed: true }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.pushed {
            STACK.with(|s| {
                s.borrow_mut().pop();
            });
        }
    }
}

/// What the options of a call ask for.
#[derive(Default)]
pub(crate) struct Options<'h> {
    pub settings: Settings,
    pub modulus: Option<T<'h>>,
    /// `roots`: each multiplicity times this
    pub multiplicity: Option<i64>,
    /// `roots`: the expression the roots are equated to instead of the unknown
    pub equated_to: Option<T<'h>>,
    /// `n_solve`: digits of working precision
    pub digits: Option<u64>,
    /// the solver's own `assumptions -> false` marker was there
    pub no_facts: bool,
    /// `max_roots: inf`: every solution, families kept as families
    pub all_roots: bool,
}

/// The option names each function accepts besides `assumptions`.
fn accepts(f: &str, key: &str) -> bool {
    let common = matches!(key, "max_roots" | "method" | "verify_solutions" | "working_precision" | "generated_parameters");
    match f {
        "solve" | "solve_values" => {
            common || matches!(key, "cubics" | "quartics" | "inverse_functions" | "modulus" | "mode" | "backsubstitution")
        }
        "roots" => matches!(key, "cubics" | "quartics" | "modulus" | "multiplicity" | "equated_to" | "method"),
        "n_solve" | "n_solve_values" => common || matches!(key, "digits" | "precision_goal" | "accuracy_goal"),
        _ => false,
    }
}

/// The arguments without a trailing option list, and the options: `Err(message)` for an option the function does
/// not take or a malformed value.
pub(crate) fn split<'a, 'h>(h: &'h Heap, f: &str, args: &'a [T<'h>]) -> Result<(Vec<T<'h>>, Options<'h>), String> {
    let mut opts = Options::default();
    let Some((&last, rest)) = args.split_last() else { return Ok((args.to_vec(), opts)) };
    let rules: Vec<T<'h>> = match h.view(last) {
        View::App { head, args: rs } if head == h.sym_term(h.sym("options")) => rs.to_vec(),
        // a bare rule `k -> v` as the last argument (an option written without `options(...)`)
        View::App { head, args: &[k, _] } if head == h.sym_term(h.sym(names::RULE)) && matches!(h.view(k), View::Sym(s) if accepts(f, &h.sym_name(s))) => {
            vec![last]
        }
        _ => return Ok((args.to_vec(), opts)),
    };
    let truth = |v: T<'h>| -> Option<bool> {
        if v == h.sym_term(h.sym(names::TRUE)) {
            Some(true)
        } else if v == h.sym_term(h.sym(names::FALSE)) {
            Some(false)
        } else {
            None
        }
    };
    for r in rules {
        let View::App { args: &[k, v], .. } = h.view(r) else { return Err("malformed options".into()) };
        let View::Sym(ks) = h.view(k) else { return Err("malformed options".into()) };
        let key = h.sym_name(ks);
        if key == "assumptions" {
            if truth(v) == Some(false) {
                opts.no_facts = true;
                continue;
            }
            return Err("assumptions here must be false".into());
        }
        if !accepts(f, &key) {
            return Err(format!("{f} does not take the option {key}"));
        }
        match key.as_str() {
            "cubics" => opts.settings.cubics = Some(truth(v).ok_or("cubics must be true or false")?),
            "quartics" => opts.settings.quartics = Some(truth(v).ok_or("quartics must be true or false")?),
            "inverse_functions" => {
                // true and automatic both invert
                opts.settings.inverse_functions = Some(truth(v) != Some(false));
            }
            "generated_parameters" => match h.view(v) {
                View::Sym(s) => opts.settings.gen = Some(h.sym_name(s)),
                _ => return Err("generated_parameters must be a symbol".into()),
            },
            "max_roots" => match h.view(v) {
                View::Int(n) if n >= 1 => opts.settings.max_roots = Some(n.min(1 << 30) as usize),
                View::Sym(s) if h.sym_name(s) == "inf" => opts.all_roots = true,
                _ => return Err("max_roots must be a positive integer".into()),
            },
            "modulus" => match h.view(v) {
                View::Int(0) => {}
                View::Int(n) if n >= 2 => opts.modulus = Some(v),
                _ => return Err("the modulus must be an integer of at least 2".into()),
            },
            "multiplicity" => match h.view(v) {
                View::Int(n) if (1..=1000).contains(&n) => opts.multiplicity = Some(n),
                _ => return Err("multiplicity must be a positive integer".into()),
            },
            "equated_to" => opts.equated_to = Some(v),
            "digits" | "working_precision" => match h.view(v) {
                View::Int(d @ 1..=100_000) => opts.digits = Some(d as u64),
                View::Sym(s) if h.sym_name(s) == "machine_precision" => {}
                _ => {
                    if key == "digits" {
                        return Err("digits must be a positive integer".into());
                    }
                }
            },
            _ => {}
        }
    }
    Ok((rest.to_vec(), opts))
}

/// Scalar equations from vector and matrix ones: `[a, b] == [c, d]` is `a == c and b == d` (nested lists element
/// by element, a list against a scalar elementwise); `None` when nothing changes.
pub(crate) fn scalar_equations<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    let list = h.sym_term(h.sym(names::LIST));
    let and = h.sym("and");
    let equal = h.sym("equal");
    let elems = |u: T<'h>| -> Option<&'h [T<'h>]> {
        match h.view(u) {
            View::App { head, args } if head == list => Some(args),
            _ => None,
        }
    };
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(equal) && args.len() == 2 => {
            let (a, b) = (args[0], args[1]);
            let pairs: Vec<(T<'h>, T<'h>)> = match (elems(a), elems(b)) {
                (Some(xs), Some(ys)) if xs.len() == ys.len() => xs.iter().copied().zip(ys.iter().copied()).collect(),
                (Some(xs), None) => xs.iter().map(|&x| (x, b)).collect(),
                (None, Some(ys)) => ys.iter().map(|&y| (a, y)).collect(),
                _ => return None,
            };
            let parts: Vec<T<'h>> = pairs
                .into_iter()
                .map(|(x, y)| {
                    let e = h.call(equal, &[x, y]);
                    scalar_equations(h, e).unwrap_or(e)
                })
                .collect();
            Some(h.call(and, &parts))
        }
        View::App { head, args } if head == h.sym_term(and) || head == list => {
            let mut changed = false;
            let parts: Vec<T<'h>> = args
                .iter()
                .map(|&a| match scalar_equations(h, a) {
                    Some(b) => {
                        changed = true;
                        b
                    }
                    None => a,
                })
                .collect();
            changed.then(|| h.app(head, &parts))
        }
        _ => None,
    }
}

/// The values of a list of solutions: `[v1, v2, ...]` for one unknown given alone, `[[v1, w1], ...]` for a list of
/// unknowns (an unknown no rule determines is its own value); anything else (a failure) as it is.
pub(crate) fn values<'h>(h: &'h Heap, sols: T<'h>, xs: T<'h>) -> T<'h> {
    let list = h.sym_term(h.sym(names::LIST));
    let elems = |u: T<'h>| -> Option<&'h [T<'h>]> {
        match h.view(u) {
            View::App { head, args } if head == list => Some(args),
            _ => None,
        }
    };
    let Some(items) = elems(sols) else { return sols };
    let (vars, single) = match elems(xs) {
        Some(v) => (v.to_vec(), false),
        None => (vec![xs], true),
    };
    let mut out = vec![];
    for &s in items {
        let Some(rules) = elems(s) else { return sols };
        let mut vals = vec![];
        for &x in &vars {
            let v = rules.iter().find_map(|&r| match h.view(r) {
                View::App { args: &[a, b], .. } if a == x => Some(b),
                _ => None,
            });
            vals.push(v.unwrap_or(x));
        }
        out.push(if single { vals[0] } else { h.app(list, &vals) });
    }
    h.app(list, &out)
}
