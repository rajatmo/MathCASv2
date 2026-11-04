//! Gauss–Kronrod rules on [-1, 1], computed once per process (decision N2.2).
//!
//! The Gauss–Legendre nodes are the zeros of P_n (Newton's method from the usual asymptotic guesses), the weights
//! 2 / ((1 - x^2) P_n'(x)^2). The n + 1 Kronrod nodes are the zeros of the Stieltjes polynomial E_{n+1}, the monic
//! polynomial of degree n + 1 orthogonal to P_n x^k (k <= n): written in the Legendre basis,
//! E_{n+1} = P_{n+1} + sum c_j P_j, its coefficients solve a small linear system whose entries are the integrals
//! of triple products of Legendre polynomials (a closed form). Its zeros interlace with the Gauss nodes, so each
//! lies in a known bracket and bisection finds it. The Kronrod weights make the 2n + 1 point rule exact for P_0
//! ... P_2n (a linear system, halved by symmetry). The tests check exactness up to degree 3n + 1.
use std::sync::OnceLock;

/// A Gauss rule embedded in its Kronrod extension: nodes in [-1, 1] (ascending), Kronrod weights and the Gauss
/// weights (zero at the nodes the Gauss rule does not have).
#[derive(Clone, Debug)]
pub struct Rule {
    /// the 2n + 1 nodes, ascending
    pub nodes: Vec<f64>,
    /// the Kronrod weights
    pub kronrod: Vec<f64>,
    /// the Gauss weights (n of them nonzero)
    pub gauss: Vec<f64>,
}

impl Rule {
    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

/// P_0(x) ... P_m(x).
fn legendre_all(m: usize, x: f64) -> Vec<f64> {
    let mut p = Vec::with_capacity(m + 1);
    p.push(1.0);
    if m >= 1 {
        p.push(x);
    }
    for k in 1..m {
        let next = ((2 * k + 1) as f64 * x * p[k] - k as f64 * p[k - 1]) / (k + 1) as f64;
        p.push(next);
    }
    p
}

/// P_n(x) and P_n'(x) (n >= 1, |x| < 1).
fn legendre(n: usize, x: f64) -> (f64, f64) {
    let p = legendre_all(n, x);
    let d = n as f64 * (x * p[n] - p[n - 1]) / (x * x - 1.0);
    (p[n], d)
}

/// The n-point Gauss–Legendre nodes (ascending) and weights.
pub fn gauss_legendre(n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut xs = vec![0.0; n];
    let mut ws = vec![0.0; n];
    for i in 0..n.div_ceil(2) {
        // the i-th largest zero: Tricomi's guess, then Newton (quadratic convergence; a fixed count suffices)
        let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        for _ in 0..100 {
            let (p, d) = legendre(n, x);
            let dx = p / d;
            x -= dx;
            if dx.abs() <= 1e-17 {
                break;
            }
        }
        let (_, d) = legendre(n, x);
        let w = 2.0 / ((1.0 - x * x) * d * d);
        xs[n - 1 - i] = x;
        ws[n - 1 - i] = w;
        xs[i] = -x;
        ws[i] = w;
    }
    if n % 2 == 1 {
        // the middle node is exactly zero
        xs[n / 2] = 0.0;
        let (_, d) = legendre(n, 0.0);
        ws[n / 2] = 2.0 / (d * d);
    }
    (xs, ws)
}

/// prod_{i=1..p} (2i - 1) / (2i).
fn half_ratio(p: usize) -> f64 {
    (1..=p).fold(1.0, |a, i| a * (2 * i - 1) as f64 / (2 * i) as f64)
}

/// The integral of P_l P_m P_n over [-1, 1] (Adams–Neumann): zero unless l + m + n = 2s is even and the triangle
/// inequalities hold; then 2 / (2s + 1) A(s - l) A(s - m) A(s - n) / A(s) with A(p) = (2p)! / (2^p p!)^2.
fn triple(l: usize, m: usize, n: usize) -> f64 {
    let s2 = l + m + n;
    if s2 % 2 == 1 {
        return 0.0;
    }
    let s = s2 / 2;
    if s < l || s < m || s < n {
        return 0.0;
    }
    2.0 / (2 * s + 1) as f64 * half_ratio(s - l) * half_ratio(s - m) * half_ratio(s - n) / half_ratio(s)
}

/// Solve the square system `a x = b` by Gaussian elimination with partial pivoting (small, well-conditioned).
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs())).expect("rows");
        a.swap(col, piv);
        b.swap(col, piv);
        let (top, rest) = a.split_at_mut(col + 1);
        let pivot = &top[col];
        for (i, r) in rest.iter_mut().enumerate() {
            let f = r[col] / pivot[col];
            if f != 0.0 {
                for (x, &p) in r[col..].iter_mut().zip(&pivot[col..]) {
                    *x -= f * p;
                }
                b[col + 1 + i] -= f * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    x
}

/// The Gauss–Kronrod rule extending the n-point Gauss rule (2n + 1 nodes).
pub fn gauss_kronrod(n: usize) -> Rule {
    let (gx, gw) = gauss_legendre(n);
    // E_{n+1} = P_{n+1} + sum_j c_j P_j with j of the parity of n + 1; orthogonal to P_n P_k for odd k <= n
    let js: Vec<usize> = (0..=n).filter(|j| (j + n + 1).is_multiple_of(2)).collect();
    let ks: Vec<usize> = (0..=n).filter(|k| k % 2 == 1).collect();
    debug_assert_eq!(js.len(), ks.len());
    let a: Vec<Vec<f64>> = ks.iter().map(|&k| js.iter().map(|&j| triple(n, j, k)).collect()).collect();
    let b: Vec<f64> = ks.iter().map(|&k| -triple(n, n + 1, k)).collect();
    let c = solve(a, b);
    let stieltjes = |x: f64| {
        let p = legendre_all(n + 1, x);
        p[n + 1] + js.iter().zip(&c).map(|(&j, &cj)| cj * p[j]).sum::<f64>()
    };
    // one zero in each gap of -1, the Gauss nodes, 1
    let mut ends = vec![-1.0];
    ends.extend_from_slice(&gx);
    ends.push(1.0);
    let mut kx = Vec::with_capacity(n + 1);
    for w in ends.windows(2) {
        let (mut lo, mut hi) = (w[0], w[1]);
        let flo = stieltjes(lo);
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if mid <= lo || mid >= hi {
                break;
            }
            if (stieltjes(mid) > 0.0) == (flo > 0.0) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        kx.push(0.5 * (lo + hi));
    }
    // symmetric: the middle Kronrod node of an even n is exactly 0
    let m = kx.len();
    for i in 0..m / 2 {
        let v = 0.5 * (kx[m - 1 - i] - kx[i]);
        kx[i] = -v;
        kx[m - 1 - i] = v;
    }
    if m % 2 == 1 {
        kx[m / 2] = 0.0;
    }
    let mut nodes: Vec<(f64, f64)> = gx.iter().zip(&gw).map(|(&x, &w)| (x, w)).chain(kx.iter().map(|&x| (x, 0.0))).collect();
    nodes.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Kronrod weights: exact for P_0, P_2, ..., P_2n; unknowns for the nonnegative nodes (symmetric weights)
    let half: Vec<f64> = nodes.iter().map(|v| v.0).filter(|&x| x >= 0.0).collect();
    let rows: Vec<Vec<f64>> = (0..=n)
        .map(|i| {
            let k = 2 * i;
            half.iter().map(|&x| if x == 0.0 { legendre_all(k, 0.0)[k] } else { 2.0 * legendre_all(k, x)[k] }).collect()
        })
        .collect();
    let rhs: Vec<f64> = (0..=n).map(|i| if i == 0 { 2.0 } else { 0.0 }).collect();
    let wh = solve(rows, rhs);
    let kronrod: Vec<f64> = nodes
        .iter()
        .map(|v| {
            let i = half.iter().position(|&x| x == v.0.abs()).expect("symmetric node");
            wh[i]
        })
        .collect();
    let gauss: Vec<f64> = nodes.iter().map(|v| v.1).collect();
    Rule { nodes: nodes.iter().map(|v| v.0).collect(), kronrod: normalised(kronrod), gauss: normalised(gauss) }
}

/// Weights scaled so that their sum, accumulated in node order as the rules use them, is 2 as nearly as machine
/// arithmetic allows (constants then integrate to their exact value).
fn normalised(w: Vec<f64>) -> Vec<f64> {
    let mut w = w;
    for _ in 0..4 {
        let s: f64 = w.iter().sum();
        if s == 2.0 {
            break;
        }
        let f = 2.0 / s;
        w.iter_mut().for_each(|v| *v *= f);
    }
    w
}

/// The 7-point Gauss rule in the 15-point Kronrod rule (QUADPACK's `qk15`): the default rule.
pub fn gk15() -> &'static Rule {
    static R: OnceLock<Rule> = OnceLock::new();
    R.get_or_init(|| gauss_kronrod(7))
}

/// The 30-point Gauss rule in the 61-point Kronrod rule (QUADPACK's `qk61`): the rule for smooth intervals that
/// the 15-point rule does not resolve (decision N2.4).
pub fn gk61() -> &'static Rule {
    static R: OnceLock<Rule> = OnceLock::new();
    R.get_or_init(|| gauss_kronrod(30))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integrate(xs: &[f64], ws: &[f64], k: usize) -> f64 {
        xs.iter().zip(ws).map(|(&x, &w)| w * x.powi(k as i32)).sum()
    }

    fn exact(k: usize) -> f64 {
        if k % 2 == 1 {
            0.0
        } else {
            2.0 / (k + 1) as f64
        }
    }

    #[test]
    fn gauss_is_exact_to_degree_2n_minus_1() {
        for n in [1, 2, 3, 7, 10, 30] {
            let (x, w) = gauss_legendre(n);
            for k in 0..2 * n {
                assert!((integrate(&x, &w, k) - exact(k)).abs() < 1e-14, "n {n} degree {k}");
            }
            if n <= 10 {
                assert!((integrate(&x, &w, 2 * n) - exact(2 * n)).abs() > 1e-12, "n {n}: degree 2n too");
            }
        }
    }

    #[test]
    fn kronrod_is_exact_to_degree_3n_plus_1() {
        for n in [7, 10, 15, 20, 30] {
            let r = gauss_kronrod(n);
            assert_eq!(r.len(), 2 * n + 1);
            assert_eq!(r.gauss.iter().filter(|&&w| w != 0.0).count(), n);
            assert!(r.kronrod.iter().all(|&w| w > 0.0), "n {n}: positive weights");
            assert!(r.nodes.windows(2).all(|w| w[0] < w[1]) && r.nodes[0] > -1.0 && r.nodes[2 * n] < 1.0);
            for k in 0..=3 * n + 1 {
                let e = (integrate(&r.nodes, &r.kronrod, k) - exact(k)).abs();
                assert!(e < 1e-14, "n {n} degree {k}: error {e:e}");
            }
            for k in 0..2 * n {
                assert!((integrate(&r.nodes, &r.gauss, k) - exact(k)).abs() < 1e-14, "n {n} Gauss degree {k}");
            }
        }
    }

    #[test]
    fn quadpack_values() {
        // QUADPACK's qk15 table (the largest nodes and their weights)
        let r = gk15();
        let near = |a: f64, b: f64| (a - b).abs() < 2e-16;
        assert!(near(r.nodes[14], 0.991_455_371_120_812_6));
        assert!(near(r.nodes[13], 0.949_107_912_342_758_5));
        assert!(near(r.kronrod[14], 0.022_935_322_010_529_224));
        assert!(near(r.kronrod[7], 0.209_482_141_084_727_82));
        assert!(near(r.gauss[13], 0.129_484_966_168_869_7));
        assert!(near(r.gauss[7], 0.417_959_183_673_469_4));
        assert_eq!(gk61().len(), 61);
    }
}
