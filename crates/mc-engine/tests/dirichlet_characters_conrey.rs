//! The Dirichlet characters under Conrey's labelling, for every modulus up to 30: the characters of a modulus are
//! orthogonal (the sum over a period of chi_j(n) conj(chi_l(n)) is euler_phi(k) for j = l and 0 otherwise, so the
//! labels name euler_phi(k) different characters), completely multiplicative, periodic, principal at label 1, and
//! symmetric in label and argument (chi_j(n) = chi_n(j), a property of Conrey's construction). `dirichlet_l` uses the
//! same labels: modulo 8, label 7 is the character induced from the odd character modulo 4, whose L-function has the
//! zero 1/2 + 18.29199319612353... i, and label 3 is (-8/n), with the zero 1/2 + 19.08644166006480... i.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

#[test]
fn characters_are_orthogonal_multiplicative_and_symmetric() {
    let mut e = CoreEngine::new();
    for k in 2..=30 {
        // the labels coprime to k and the values over one period, as machine numbers
        ev(&mut e, &format!("ls = select(range({k}), j => gcd(j, {k}) == 1)"));
        ev(&mut e, &format!("tb = table(n(dirichlet_character({k}, j, a)), [j, ls], [a, {k}])"));
        holds(&mut e, &format!("max(abs(flatten(dot(tb, conjugate(transpose(tb))) - euler_phi({k})*identity_matrix(length(ls))))) < 10^-9"));
        holds(&mut e, &format!("tb[1] == table(if gcd(a, {k}) == 1 then 1. else 0., [a, {k}])"));
        holds(&mut e, &format!(
            "max(flatten(table(abs(tb[i][mod(a*b - 1, {k}) + 1] - tb[i][a]*tb[i][b]), [i, length(ls)], [a, {k}], [b, {k}]))) < 10^-9"
        ));
        holds(&mut e, &format!(
            "max(flatten(table(abs(n(dirichlet_character({k}, j, a)) - n(dirichlet_character({k}, a, j))), [j, ls], [a, ls]))) < 10^-9"
        ));
        holds(&mut e, &format!(
            "max(flatten(table(abs(n(dirichlet_character({k}, j, a + {k}) - dirichlet_character({k}, j, a))), [j, ls], [a, {k}]))) < 10^-12"
        ));
    }
}

#[test]
fn dirichlet_l_follows_the_labels() {
    let mut e = CoreEngine::new();
    holds(&mut e, "dirichlet_character(8, 3, range(8)) == [1, 0, 1, 0, -1, 0, -1, 0]");
    holds(&mut e, "dirichlet_character(8, 7, range(8)) == [1, 0, -1, 0, 1, 0, -1, 0]");
    holds(&mut e, "abs(rewrite(t, find_root(dirichlet_l(8, 7, t) == 0, [t, 1/2 + 18*im])) - (1/2 + 18.29199319612353*im)) < 10^-9");
    holds(&mut e, "abs(rewrite(t, find_root(dirichlet_l(8, 3, t) == 0, [t, 1/2 + 19*im])) - (1/2 + 19.08644166006480*im)) < 10^-9");
}
