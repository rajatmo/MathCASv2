//! Baseline: a reference-counted tree in the style of v1's `Expr` (symbols as shared strings compared by content,
//! structural equality and hashing by traversal, no sharing of equal subterms unless a workload reuses a handle).
use crate::{Store, View};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Tx {
    Int(i64),
    Sym(Rc<str>),
    App(Rc<TApp>),
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct TApp {
    pub head: Rc<str>,
    pub args: Vec<Tx>,
}

#[derive(Default)]
pub struct TreeStore;

impl Store for TreeStore {
    type T = Tx;
    type S = Rc<str>;

    fn name(&self) -> &'static str {
        "tree"
    }
    fn sym(&mut self, name: &str) -> Rc<str> {
        Rc::from(name)
    }
    fn sym_term(&mut self, s: &Rc<str>) -> Tx {
        Tx::Sym(s.clone())
    }
    fn int(&mut self, v: i64) -> Tx {
        Tx::Int(v)
    }
    fn app(&mut self, head: &Rc<str>, args: Vec<Tx>) -> Tx {
        Tx::App(Rc::new(TApp { head: head.clone(), args }))
    }
    fn view<'a>(&'a self, t: &'a Tx) -> View<'a, Tx, Rc<str>> {
        match t {
            Tx::Int(v) => View::Int(*v),
            Tx::Sym(s) => View::Sym(s),
            Tx::App(a) => View::App(&a.head, &a.args),
        }
    }
    fn same(&self, a: &Tx, b: &Tx) -> bool {
        a == b
    }
    fn hash_term(&self, t: &Tx) -> u64 {
        let mut h = rustc_hash::FxHasher::default();
        t.hash(&mut h);
        h.finish()
    }
    fn sym_is(&self, s: &Rc<str>, other: &Rc<str>) -> bool {
        // v1 compares symbol names by content
        **s == **other
    }
    fn sym_name(&self, s: &Rc<str>) -> String {
        s.to_string()
    }
}
