//! Definitions in the term shapes `mc-syntax` lowers to, installed by evaluating them, at 1 and 4 threads.
use mc_core::shapes::fingerprint;
use mc_core::{Config, Heap, Store, View, T};
use mc_eval::{session, EvalConfig, Split};

struct B<'h>(&'h Heap);

impl<'h> B<'h> {
    fn s(&self, n: &str) -> T<'h> {
        self.0.sym_term(self.0.sym(n))
    }
    fn c(&self, n: &str, a: &[T<'h>]) -> T<'h> {
        self.0.call(self.0.sym(n), a)
    }
    fn i(&self, v: i64) -> T<'h> {
        self.0.int(v)
    }
}

fn show(h: &Heap, t: T<'_>) -> String {
    match h.view(t) {
        View::Int(v) => v.to_string(),
        View::Sym(s) => h.sym_name(s),
        View::Str(s) => format!("{s:?}"),
        View::App { head, args } => format!("{}({})", show(h, head), args.iter().map(|&a| show(h, a)).collect::<Vec<_>>().join(", ")),
        v => format!("{v:?}"),
    }
}

#[test]
fn definition_terms_install_rules() {
    let mut results = vec![];
    for (threads, split) in [(1, Split::Never), (4, Split::Always)] {
        let mut s = session(Config::stress(), threads, EvalConfig { split, ..EvalConfig::default() });
        let defs = s.step(|h, k| {
            let b = B(h);
            let x = b.s("x");
            let any = |v| b.c("any", &[v]);
            let defs = vec![
                // fn fact(n: Int) when greater(n, 0) = imul(n, fact(isub(n, 1)))
                b.c("define_fn", &[
                    b.c("condition", &[b.c("fact", &[b.c("typed", &[any(b.s("n")), b.s("Int")])]), b.c("greater", &[b.s("n"), b.i(0)])]),
                    b.c("imul", &[b.s("n"), b.c("fact", &[b.c("isub", &[b.s("n"), b.i(1)])])]),
                    b.c("list", &[]),
                    b.s("null"),
                    b.c("list", &[]),
                ]),
                // rule fact(0) := 1
                b.c("define_rule", &[b.c("rule_delayed", &[b.c("fact", &[b.i(0)]), b.i(1)]), b.c("list", &[])]),
                // len(xs___) := ... the sequence variable stands for a list
                b.c("set_delayed", &[b.c("wrap", &[b.c("seq0", &[b.s("xs")])]), b.c("got", &[b.s("xs")])]),
                // pair(p @ g(_), p2_) := both(p, p2)
                b.c("set_delayed", &[
                    b.c("pair", &[b.c("named", &[b.s("p"), b.c("g", &[b.c("any", &[])])]), any(b.s("q"))]),
                    b.c("both", &[b.s("p"), b.s("q")]),
                ]),
                // sym m [associative, commutative]; m(a, x_) := x
                b.c("define_sym", &[b.s("m"), b.c("list", &[b.s("associative"), b.s("commutative")])]),
                b.c("set_delayed", &[b.c("m", &[b.s("a"), any(x)]), x]),
                // c = iadd(1, 2)
                b.c("set", &[b.s("c"), b.c("iadd", &[b.i(1), b.i(2)])]),
                // a malformed definition: a failure value, nothing installed (an optional argument whose pattern has
                // no variable naming it; an optional run `optional(xs___, d)` is accepted since r18c14.1)
                b.c("set_delayed", &[b.c("bad", &[b.c("g", &[b.c("optional", &[b.c("h", &[any(x)]), b.i(0)])])]), x]),
            ];
            let list = b.c("list", &defs);
            show(h, k.eval(h, list).value)
        });
        let uses = s.step(|h, k| {
            let b = B(h);
            let tests = b.c("list", &[
                b.c("fact", &[b.i(10)]),
                b.c("wrap", &[b.i(1), b.i(2)]),
                b.c("pair", &[b.c("g", &[b.i(5)]), b.i(6)]),
                b.c("pair", &[b.c("h", &[b.i(5)]), b.i(6)]),
                b.c("m", &[b.s("z"), b.s("a")]),
                b.c("iadd", &[b.s("c"), b.i(1)]),
            ]);
            let v = k.eval(h, tests).value;
            (show(h, v), fingerprint(h, v))
        });
        results.push((defs, uses));
    }
    assert_eq!(results[0], results[1]);
    let (defs, (uses, _)) = &results[0];
    assert_eq!(defs, "failure(definition, \"not supported yet: an optional argument without one variable naming it\")");
    assert_eq!(uses, "list(3628800, got(list(1, 2)), both(g(5), 6), pair(h(5), 6), z, 4)");
}
