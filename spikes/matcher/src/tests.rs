//! Randomised checks of the properties the spike relies on:
//! - the naive and the smart matcher find the same first match and the same set of distinct matches;
//! - `instance_of` is sound: when it says A is an instance of B, B matches every instance of A;
//! - rule order and replacement (semantics 6.3), and read-set invalidation, on small hand cases.
use crate::eval::{Cfg, Evaluator, Track};
use crate::index::{Dispatch, OrderBy};
use crate::matcher::{Binds, Matcher, PlainEnv};
use crate::pat::{self, alt, blank, compile, cond, lit, papp, seq0, seq1, x, xt, Pat, Ty};
use crate::store::{Store, Term};
use crate::{fingerprint, Limit, Rng};
use std::collections::BTreeSet;
use std::rc::Rc;

struct W {
    add: Term,
    mul: Term,
    f: Term,
    g: Term,
    atoms: Vec<Term>,
}

fn world(st: &mut Store) -> W {
    let add = st.sym("add");
    let mul = st.sym("mul");
    let f = st.sym("f");
    let g = st.sym("g");
    let mut atoms: Vec<Term> = ["a", "b", "c"].iter().map(|s| st.sym(s)).collect();
    atoms.push(st.int(1));
    atoms.push(st.int(2));
    atoms.push(st.real(0.5));
    W { add, mul, f, g, atoms }
}

fn rand_term(st: &mut Store, w: &W, rng: &mut Rng, depth: usize) -> Term {
    if depth == 0 || rng.pct(40) {
        return w.atoms[rng.below(w.atoms.len() as u64) as usize];
    }
    match rng.below(4) {
        0 => {
            let a = rand_term(st, w, rng, depth - 1);
            st.app(w.f, &[a])
        }
        1 => {
            let a = rand_term(st, w, rng, depth - 1);
            let b = rand_term(st, w, rng, depth - 1);
            st.app(w.g, &[a, b])
        }
        k => {
            let h = if k == 2 { w.add } else { w.mul };
            let n = rng.below(6) as usize;
            let args: Vec<Term> = (0..n).map(|_| rand_term(st, w, rng, depth - 1)).collect();
            st.app_ac(h, &args)
        }
    }
}

fn rand_single(st: &mut Store, w: &W, rng: &mut Rng, depth: usize) -> Pat {
    let r = rng.below(100);
    if depth == 0 || r < 45 {
        return match rng.below(6) {
            0 => lit(w.atoms[rng.below(w.atoms.len() as u64) as usize]),
            1 => blank(),
            2 => xt(rng.below(3) as u16, [Ty::Integer, Ty::Symbol, Ty::Number, Ty::Head(w.f)][rng.below(4) as usize]),
            _ => x(rng.below(3) as u16),
        };
    }
    if r < 50 {
        let a = rand_single(st, w, rng, depth - 1);
        let b = rand_single(st, w, rng, depth - 1);
        return alt(vec![a, b]);
    }
    match rng.below(4) {
        0 => papp(w.f, vec![rand_single(st, w, rng, depth - 1)]),
        1 => {
            let n = rng.below(3) as usize;
            papp(w.g, rand_args(st, w, rng, depth - 1, n))
        }
        k => {
            let h = if k == 2 { w.add } else { w.mul };
            let n = rng.below(4) as usize;
            papp(h, rand_args(st, w, rng, depth - 1, n))
        }
    }
}

fn rand_args(st: &mut Store, w: &W, rng: &mut Rng, depth: usize, n: usize) -> Vec<Pat> {
    (0..n)
        .map(|_| {
            if rng.pct(25) {
                // sequence variables use numbers 3 and 4, single variables 0 to 2, so kinds never clash
                let v = 3 + rng.below(2) as u16;
                if rng.pct(50) {
                    seq0(v)
                } else {
                    seq1(v)
                }
            } else {
                rand_single(st, w, rng, depth)
            }
        })
        .collect()
}

fn all_fps(mt: &Matcher, st: &Store, p: &Pat, t: Term) -> BTreeSet<u64> {
    let mut s = BTreeSet::new();
    mt.all_matches(p, p.n_vars().max(5), t, |b| {
        s.insert(b.fingerprint(st));
    });
    s
}

#[test]
fn naive_and_smart_agree() {
    let mut st = Store::new();
    let w = world(&mut st);
    let (add, mul) = (w.add, w.mul);
    let is_ac = move |h: Term| h == add || h == mul;
    let mut rng = Rng::new(2024);
    let (mut matched, mut tried, mut skipped) = (0, 0, 0);
    for _ in 0..1500 {
        let p = rand_single(&mut st, &w, &mut rng, 3);
        let p = compile(&mut st, p, &is_ac);
        let ts: Vec<Term> = (0..20).map(|_| rand_term(&mut st, &w, &mut rng, 3)).collect();
        let env = PlainEnv(&st);
        let (mut s1, mut s2, mut o1, mut o2) = (Binds::default(), Binds::default(), Binds::default(), Binds::default());
        for &t in &ts {
            tried += 1;
            // the naive strategy is exponential on some random nested AC pairs: bound each pair, skip the slow ones
            let lim = Limit::new(Some(std::time::Duration::from_millis(20)));
            let naive = Matcher::new(&env, false, &lim);
            let smart = Matcher::new(&env, true, &lim);
            let nv = p.n_vars().max(5);
            let a = naive.first(&p, nv, t, &mut s1, &mut o1);
            let b = smart.first(&p, nv, t, &mut s2, &mut o2);
            let rest = Matcher::new(&env, false, &lim).with_rest(true);
            let (mut s3, mut o3) = (Binds::default(), Binds::default());
            let c = rest.first(&p, nv, t, &mut s3, &mut o3);
            let (fa, fb) = (all_fps(&naive, &st, &p, t), all_fps(&smart, &st, &p, t));
            if lim.hit() {
                skipped += 1;
                continue;
            }
            assert_eq!(a, b, "found differs");
            assert_eq!(a, c, "found differs (naive+rest)");
            if a {
                matched += 1;
                assert_eq!(o1.fingerprint(&st), o2.fingerprint(&st), "first match differs");
                assert_eq!(o1.fingerprint(&st), o3.fingerprint(&st), "first match differs (naive+rest)");
                assert_eq!(fa, fb, "match sets differ");
            }
        }
    }
    assert!(matched > 500, "too few matches to be a test: {matched} of {tried}");
    assert!(skipped * 100 < tried, "too many pairs skipped: {skipped} of {tried}");
    eprintln!("naive_and_smart_agree: {matched} matches in {tried} pairs, {skipped} skipped");
}

#[test]
fn ac_first_match_order() {
    let mut st = Store::new();
    let w = world(&mut st);
    let (add, a, b, c) = (w.add, w.atoms[0], w.atoms[1], w.atoms[2]);
    let is_ac = move |h: Term| h == add;
    // add(x_, ys__) against add(a, b, c): x takes the first argument in canonical order, ys the rest
    let p = compile(&mut st, papp(add, vec![seq1(1), x(0)]), &is_ac);
    let t = st.app_ac(add, &[c, b, a]);
    let lim = Limit::none();
    let env = PlainEnv(&st);
    for smart in [false, true] {
        let mt = Matcher::new(&env, smart, &lim);
        let (mut s, mut o) = (Binds::default(), Binds::default());
        assert!(mt.first(&p, 2, t, &mut s, &mut o));
        assert_eq!(o.one(0), a);
        assert_eq!(o.seq(1), &[b, c]);
    }
}

/// An instance of pattern `p` (variables replaced by random terms of the right type, consistently).
fn instantiate(st: &mut Store, w: &W, rng: &mut Rng, p: &Pat, env: &mut Vec<Option<Vec<Term>>>) -> Option<Vec<Term>> {
    let one = |st: &mut Store, rng: &mut Rng, ty: Option<Ty>| -> Term {
        match ty {
            None => rand_term(st, w, rng, 2),
            Some(Ty::Integer) => st.int(rng.below(3) as i64),
            Some(Ty::Real) => st.real(0.5),
            Some(Ty::Number) => st.int(1),
            Some(Ty::Symbol) => w.atoms[rng.below(3) as usize],
            Some(Ty::Head(h)) => {
                let a = rand_term(st, w, rng, 1);
                if h == w.g {
                    st.app(h, &[a, a])
                } else if h == w.add || h == w.mul {
                    st.app_ac(h, &[a])
                } else {
                    st.app(h, &[a])
                }
            }
        }
    };
    let mut var = |st: &mut Store, rng: &mut Rng, v: Option<u16>, make: &mut dyn FnMut(&mut Store, &mut Rng) -> Vec<Term>| {
        match v {
            None => make(st, rng),
            Some(v) => {
                if let Some(t) = &env[v as usize] {
                    return t.clone();
                }
                let t = make(st, rng);
                env[v as usize] = Some(t.clone());
                t
            }
        }
    };
    match p {
        Pat::Blank { var: v, ty } => Some(var(st, rng, *v, &mut |st, rng| vec![one(st, rng, *ty)])),
        Pat::Seq { var: v, min } => Some(var(st, rng, *v, &mut |st, rng| {
            let n = *min as usize + rng.below(3) as usize;
            (0..n).map(|_| one(st, rng, None)).collect()
        })),
        Pat::Lit(t) => Some(vec![*t]),
        Pat::Alt(ps) => {
            let i = rng.below(ps.len() as u64) as usize;
            instantiate(st, w, rng, &ps[i], env)
        }
        Pat::Cond(q, _) => instantiate(st, w, rng, q, env),
        Pat::App { head, args, info } => {
            let mut out = vec![];
            for a in args {
                out.extend(instantiate(st, w, rng, a, env)?);
            }
            let t = if info.ac { st.app_ac(*head, &out) } else { st.app(*head, &out) };
            Some(vec![t])
        }
    }
}

#[test]
fn instance_of_is_sound() {
    let mut st = Store::new();
    let w = world(&mut st);
    let (add, mul) = (w.add, w.mul);
    let is_ac = move |h: Term| h == add || h == mul;
    let mut rng = Rng::new(77);
    let lim = Limit::none();
    let mut proven = 0;
    for i in 0..4000 {
        let a = rand_single(&mut st, &w, &mut rng, 2);
        // B is often a generalisation of A by construction: replace some subpatterns by variables
        let b = if i % 2 == 0 { generalise(&a, &mut rng) } else { rand_single(&mut st, &w, &mut rng, 2) };
        let (a, b) = (compile(&mut st, a, &is_ac), compile(&mut st, b, &is_ac));
        if !pat::instance_of(&st, &a, &b, &lim).unwrap() {
            continue;
        }
        proven += 1;
        for _ in 0..10 {
            let mut env = vec![None; 8];
            let Some(ts) = instantiate(&mut st, &w, &mut rng, &a, &mut env) else { continue };
            let t = ts[0];
            let envp = PlainEnv(&st);
            let mt = Matcher::new(&envp, true, &lim);
            let mut s = Binds::default();
            if !mt.matches(&a, 8, t, &mut s) {
                continue; // instantiation can violate non-linear constraints across alternatives; skip
            }
            assert!(
                mt.matches(&b, 8, t, &mut s),
                "instance_of claimed A <= B, but B misses an instance of A: A = {}, B = {}, t = {}",
                pat::show(&st, &a),
                pat::show(&st, &b),
                pat::show_term(&st, t)
            );
        }
    }
    assert!(proven > 300, "too few proven instances to be a test: {proven}");
}

/// Replace random subpatterns by fresh variables (numbers 5 to 7, so that they do not clash).
fn generalise(p: &Pat, rng: &mut Rng) -> Pat {
    if !p.is_seq() && rng.pct(25) {
        return x(5 + rng.below(3) as u16);
    }
    match p {
        Pat::App { head, args, .. } => papp(*head, args.iter().map(|a| generalise(a, rng)).collect()),
        q => q.clone(),
    }
}

#[test]
fn rule_order_and_replacement() {
    for by in [OrderBy::Scan, OrderBy::Index] {
        let mut ev = Evaluator::new(Store::new(), Cfg { order: by, ..Cfg::default() }, Limit::none());
        let (h, a) = (ev.st.sym("h"), ev.st.sym("a"));
        let r = |k: i64| -> crate::index::Rhs { Rc::new(move |_, st| st.int(k)) };
        ev.define(papp(h, vec![x(0)]), r(0), None).unwrap(); // h(x_)
        ev.define(papp(h, vec![xt(0, Ty::Integer)]), r(1), None).unwrap(); // h(x_: integer), before h(x_)
        ev.define(papp(h, vec![lit(a)]), r(2), None).unwrap(); // h(a), before h(x_)
        let one = ev.st.int(1);
        ev.define(papp(h, vec![lit(one)]), r(3), None).unwrap(); // h(1), before h(x_: integer)
        ev.define(papp(h, vec![xt(5, Ty::Integer)]), r(4), None).unwrap(); // replaces rule 1
        let rs = ev.rules(h).unwrap();
        // h(a) goes before the first rule more general than it at its definition (h(x_)); h(1) before h(x_: integer)
        assert_eq!(rs.order, vec![3, 1, 2, 0], "{by:?}");
        assert_eq!(rs.len(), 4);
        let t = ev.st.int(7);
        let c = ev.st.app(h, &[t]);
        let v = ev.eval(c).unwrap();
        assert_eq!(ev.st.as_int(v), Some(4));
    }
}

#[test]
fn read_sets_invalidate() {
    for track in [Track::Mask, Track::Exact] {
        let mut ev = Evaluator::new(Store::new(), Cfg { memo: true, track, ..Cfg::default() }, Limit::none());
        let (f, lim, big, small) = (ev.st.sym("f"), ev.st.sym("lim"), ev.st.sym("big"), ev.st.sym("small"));
        let v = ev.st.int(5);
        ev.set_own(lim, v);
        ev.define(papp(f, vec![x(0)]), Rc::new(move |b, st| st.app(small, &[b.one(0)])), None).unwrap();
        let p = cond(papp(f, vec![xt(0, Ty::Integer)]), move |b, env| {
            let l = env.value(lim).and_then(|v| env.st().as_int(v)).unwrap();
            env.st().as_int(b.one(0)).unwrap() > l
        });
        ev.define(p, Rc::new(move |b, st| st.app(big, &[b.one(0)])), None).unwrap();
        let seven = ev.st.int(7);
        let t = ev.st.app(f, &[seven]);
        let r1 = ev.eval(t).unwrap();
        assert_eq!(ev.st.head(r1), Some(big));
        let ten = ev.st.int(10);
        ev.set_own(lim, ten);
        let r2 = ev.eval(t).unwrap();
        assert_eq!(ev.st.head(r2), Some(small), "{track:?}: stale result after redefining a read global");
        assert_ne!(fingerprint(&ev.st, r1), fingerprint(&ev.st, r2));
    }
}

#[test]
fn arg_head_rules_first() {
    for d in [Dispatch::Linear, Dispatch::Indexed] {
        let mut ev = Evaluator::new(Store::new(), Cfg { dispatch: d, ..Cfg::default() }, Limit::none());
        let (area, sq) = (ev.st.sym("area"), ev.st.sym("square"));
        // a rule on area that also matches area(square(_)), defined first: the argument-head rule still wins
        ev.define(papp(area, vec![x(0)]), Rc::new(|_, st| st.int(-1)), None).unwrap();
        ev.define(
            papp(area, vec![papp(sq, vec![xt(0, Ty::Integer)])]),
            Rc::new(|b, st| {
                let a = st.as_int(b.one(0)).unwrap();
                st.int(a * a)
            }),
            Some(sq),
        )
        .unwrap();
        let three = ev.st.int(3);
        let s3 = ev.st.app(sq, &[three]);
        let t = ev.st.app(area, &[s3]);
        let r = ev.eval(t).unwrap();
        assert_eq!(ev.st.as_int(r), Some(9));
    }
}
