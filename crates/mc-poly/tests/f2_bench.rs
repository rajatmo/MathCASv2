//! Item F2 timings (ignored; run with `--ignored --nocapture`): the generic Ducos chain over `Z[y]` against the
//! crate's subresultant resultant, and the generic univariate resultant against the modular one.
use mc_poly::resultant::resultant;
use mc_poly::ring::{Integers, PolyRing};
use mc_poly::unipoly;
use mc_poly::work::{Meter, Unlimited};
use mc_poly::{Poly, Z};
use std::time::Instant;

fn lcg(seed: &mut u64) -> i64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 33) % 19) as i64 - 9
}

/// A dense polynomial in x (variable 0) and the other variables with degrees `deg`.
fn random(nv: usize, degs: &[u32], seed: &mut u64) -> Poly {
    let mut terms = vec![];
    let mut e = vec![0u32; nv];
    loop {
        let c = lcg(seed);
        if c != 0 {
            terms.push((e.clone(), Z::S(c)));
        }
        let mut i = 0;
        loop {
            if i == nv {
                return Poly::from_terms(nv, terms);
            }
            e[i] += 1;
            if e[i] <= degs[i] {
                break;
            }
            e[i] = 0;
            i += 1;
        }
    }
}

fn coeffs(p: &Poly, v: usize) -> Vec<Poly> {
    let mut out = vec![Poly::zero(p.nvars()); p.degree(v).map_or(0, |d| d as usize + 1)];
    for (k, c) in p.coeffs_in(v) {
        out[k as usize] = c;
    }
    out
}

fn best<T>(n: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut t = f64::INFINITY;
    let mut last = None;
    for _ in 0..n {
        let s = Instant::now();
        let r = f();
        t = t.min(s.elapsed().as_secs_f64() * 1e3);
        last = Some(r);
    }
    (t, last.expect("one run"))
}

#[test]
#[ignore]
fn chain_timings() {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    let mut seed = 7;
    for (dx, dy, nv) in [(6u32, 3u32, 2usize), (8, 4, 2), (10, 3, 2), (5, 2, 3)] {
        let degs_a: Vec<u32> = (0..nv).map(|i| if i == 0 { dx } else { dy }).collect();
        let degs_b: Vec<u32> = (0..nv).map(|i| if i == 0 { dx - 1 } else { dy }).collect();
        let a = random(nv, &degs_a, &mut seed);
        let b = random(nv, &degs_b, &mut seed);
        let (t_old, r_old) = best(3, || resultant(&a, &b, 0, &mut m).unwrap());
        let r = PolyRing { nv };
        let (ca, cb) = (coeffs(&a, 0), coeffs(&b, 0));
        let (t_new, r_new) = best(3, || unipoly::resultant(&r, &ca, &cb, &mut m).unwrap());
        let (t_cl, _) = best(3, || unipoly::subresultant_chain_with(&r, &ca, &cb, false, &mut m).unwrap());
        assert_eq!(r_old, r_new);
        println!("Z[y] resultant deg x {dx}/{} deg y {dy} ({nv} vars): crate {t_old:.2} ms, Ducos chain {t_new:.2} ms, classical chain {t_cl:.2} ms", dx - 1);
    }
    for d in [20usize, 40, 80] {
        let a: Vec<Z> = (0..=d).map(|_| Z::S(lcg(&mut seed))).collect();
        let b: Vec<Z> = (0..d).map(|_| Z::S(lcg(&mut seed))).collect();
        let a = unipoly::trim(&Integers, a);
        let b = unipoly::trim(&Integers, b);
        let (pa, pb) = (Poly::from_dense(1, 0, &a), Poly::from_dense(1, 0, &b));
        let (t_mod, r1) = best(3, || resultant(&pa, &pb, 0, &mut m).unwrap());
        let (t_gen, r2) = best(3, || unipoly::resultant(&Integers, &a, &b, &mut m).unwrap());
        assert_eq!(r1.as_constant().unwrap(), r2);
        println!("Z resultant degree {d}/{}: modular {t_mod:.2} ms, generic Ducos chain {t_gen:.2} ms", d - 1);
    }
}
