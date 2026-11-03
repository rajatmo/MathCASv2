//! Test helpers shared by the integration tests.
use mc_api::{Store, View, T};

/// The structure of a term with no special syntax: `head(arg, ...)`, numbers in decimal, strings quoted.
pub fn raw<S: Store + ?Sized>(store: &S, t: T<'_>) -> String {
    match store.view(t) {
        View::Int(v) => v.to_string(),
        View::BigInt { neg, mag } => format!("{}big{:?}", if neg { "-" } else { "" }, mag),
        View::Rational { num, den } => format!("{}/{}", raw(store, num), raw(store, den)),
        View::Real(v) => format!("{v:?}"),
        View::Ball { neg, words } => format!("ball({neg}, {words:?})"),
        View::Str(s) => format!("{s:?}"),
        View::Sym(s) => store.sym_name(s),
        View::App { head, args } => {
            let a: Vec<String> = args.iter().map(|&a| raw(store, a)).collect();
            format!("{}({})", raw(store, head), a.join(", "))
        }
    }
}
