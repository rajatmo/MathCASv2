#! mathcas 2
# Special-function library: written in MathCAS itself, evaluated after the other libraries,
# defined in every session. Helpers are prefixed `ds_` and are not part of the interface.

fn ds_total(xs) = fold((p, q) => p + q, 0, xs)
# the sum and product of f(j) for j = a..b (lambdas, so that no global symbol is an index)
fn ds_sum(f, a, b) = ds_total(map(f, range(a, b)))
fn ds_product(f, a, b) = fold((p, q) => p*q, 1, map(f, range(a, b)))

# --- series of special functions ---
#
# The `series` native hands a call it declines to ds_series(e, x, x0, k) (crates/mc-special/src/series_fallback.rs). Each
# special function f(..., u, ...) of x in e whose expansion at u0 (the value of u at x0, or inf) is known is replaced
# by the expansion truncated after enough terms, and `series` runs again on the result; an expansion at infinity
# with an exponential or oscillating factor, ds_pieces([[p1, s1], ...]) for p1 s1 + ..., is only used when e is a
# constant multiple of the one function: the result is then c (p1 series(s1) + ...). ds_none: no expansion.

# the heads with expansions
fn ds_heads() = [sin_integral, sinh_integral, cos_integral, cosh_integral, exp_integral_ei, exp_integral_e,
  log_integral, bessel_j, bessel_y, bessel_i, bessel_k, kelvin_ber, kelvin_bei, kelvin_ker, kelvin_kei, struve_h, struve_l, elliptic_k,
  elliptic_e, elliptic_nome_q, elliptic_theta, log_gamma, gamma, factorial, digamma, polygamma, harmonic_number,
  polylog, product_log, weierstrass_p, weierstrass_p_prime, weierstrass_zeta, weierstrass_sigma, jacobi_cs,
  jacobi_ds, jacobi_ns, jacobi_nc, jacobi_nd, jacobi_sc, jacobi_sd, jacobi_cd, jacobi_dc, coulomb_f,
  jacobi_sn, jacobi_cn, jacobi_dn, jacobi_amplitude, jacobi_epsilon, jacobi_zn, inverse_jacobi_sn, inverse_jacobi_cn,
  inverse_jacobi_cd, inverse_jacobi_sd, inverse_jacobi_dc, inverse_jacobi_nc, inverse_jacobi_sc, inverse_jacobi_ns,
  inverse_jacobi_ds, inverse_jacobi_cs, heun_g, heun_c, heun_b, heun_d, heun_t, mathieu_c, mathieu_s, mathieu_characteristic_a,
  spheroidal_eigenvalue, mittag_leffler_e, q_pochhammer, whittaker_m, whittaker_w, q_hypergeometric_pfq, inverse_elliptic_nome_q,
  hypergeometric_pfq_regularized, anger_j, weber_e, zeta, factorial2, barnes_g, log_barnes_g, hyperfactorial, elliptic_theta_prime,
  hypergeometric_u, airy_ai, airy_bi, airy_ai_prime, airy_bi_prime, airy_ai_zero, airy_bi_zero, riemann_siegel_z,
  bessel_j_zero, bessel_y_zero, erf, erfc, erfi, dawson_f, fresnel_c, fresnel_s, gamma_regularized, hankel_h1, hankel_h2,
  spherical_hankel_h1, spherical_hankel_h2, lerch_phi, fibonacci, lucas_l, lommel_s1, lommel_s2, lommel_t1, lommel_t2]
fn ds_known(t) = not is_atom(t) and some(ds_heads(), f => f === head(t))

rule ds_series(e_, x_, x0_, k_, opts___) := ds_series_at(e, x, x0, k)

fn ds_series_at(e, x, x0, k) = {
  let dep = select(union(cases(e, (ds_term_ when ds_known(ds_term)), [0, inf])), t => not free_of(t, x))
  let good = select(map(t => [t, ds_one(t, x, x0, k + 3)], dep), r => not (r[2] === ds_none))
  if length(good) == 0 then ds_none
  else if some(good, r => not is_atom(r[2]) and head(r[2]) === ds_pieces) then {
    let c = together(e/good[1][1])
    if length(good) == 1 and free_of(c, x) then
      c*ds_total(map(p => p[1]*series(p[2], x, x0, max(k, 1)), good[1][2][1]))
    else ds_none
  } else ds_series_log(rewrite(e, map(r => r[1] -> r[2], good)), x, x0, k)
}

# ds_series_log(e, x, x0, k): series(e, x, x0, k) with log(x) at infinity and log(-x) at 0 kept as coefficients (each
# is constant to every power of x; the series kernels would write them as -log(x^-1) and log(x) + i pi)
fn ds_series_log(e, x, x0, k) = {
  let l = if x0 === inf then log(x) else if x0 === 0 then log(-x) else null
  # at infinity log(-x) is kept too (the inversion formulas of the polylogarithm and the Lerch function have it)
  let e2 = if x0 === inf and not free_of(e, log(-x)) then rewrite(e, log(-x) -> ds_logmx) else e
  let r = if not (l === null) and not free_of(e2, l) then rewrite(series(rewrite(e2, l -> ds_logx), x, x0, k), ds_logx -> l)
    else series(e2, x, x0, k)
  if e2 === e then r else rewrite(r, ds_logmx -> log(-x))
}

# ds_one(t, x, x0, k): the expansion of the call t at x -> x0 (k terms), or ds_none
fn ds_one(t, x, x0, k) = {
  let is = select(range(length(t)), i => not free_of(t[i], x))
  if not (length(is) == 1) then ds_none
  else {
    let u = t[is[1]]
    let u0 = if x0 === inf then (if u === x or limit(u, x -> inf) === inf then inf else ds_none)
      else if x0 === -inf then ds_none else rewrite(u, x -> x0)
    if u0 === ds_none then ds_none
    else {
      let r = ds_at(t, is[1], u0, k)
      if not is_atom(r) and head(r) === ds_at then ds_none else r
    }
  }
}

# psi(j + 1) for integers j >= 0
fn ds_psi1(j) = harmonic_number(j) - euler_gamma

# near 0: the sine and cosine integrals, their hyperbolic forms, Ei, E_n (Abramowitz and Stegun 5.2.14, 5.2.16,
# 5.1.10-12; DLMF 6.6, 8.19.8)
rule ds_at(sin_integral(u_), 1, 0, k_) := ds_sum(j => (-1)^j*u^(2*j + 1)/((2*j + 1)*factorial(2*j + 1)), 0, k)
rule ds_at(sinh_integral(u_), 1, 0, k_) := ds_sum(j => u^(2*j + 1)/((2*j + 1)*factorial(2*j + 1)), 0, k)
rule ds_at(cos_integral(u_), 1, 0, k_) := euler_gamma + log(u) + ds_sum(j => (-1)^j*u^(2*j)/(2*j*factorial(2*j)), 1, k)
rule ds_at(cosh_integral(u_), 1, 0, k_) := euler_gamma + log(u) + ds_sum(j => u^(2*j)/(2*j*factorial(2*j)), 1, k)
rule ds_at(exp_integral_ei(u_), 1, 0, k_) := {
  # on the negative axis Ei(u) = gamma + log(-u) + ... (real)
  euler_gamma + (if refine(u < 0) === true then log(-u) else log(u)) + ds_sum(j => u^j/(j*factorial(j)), 1, k)
}
rule ds_at(exp_integral_e(m_, u_), 2, 0, k_) := {
  (-u)^(m - 1)/factorial(m - 1)*(ds_psi1(m - 1) - log(u)) -
    ds_total(map(j => (-u)^j/((j - m + 1)*factorial(j)), select(range(0, k), j => not (j == m - 1))))
} when is_integer(m) and m >= 1
# li(u) = Ei(log u) near u = 1
rule ds_at(log_integral(u_), 1, 1, k_) := exp_integral_ei(log(u))

# near 0: Bessel functions of integer order (DLMF 10.2.2, 10.25.2, 10.8.1, 10.31.1)
fn ds_bj(nu, u, k, s) = ds_sum(j => s^j*(u/2)^(nu + 2*j)/(factorial(j)*factorial(nu + j)), 0, k)
rule ds_at(bessel_j(nu_, u_), 2, 0, k_) := ds_bj(nu, u, k, -1) when is_integer(nu) and nu >= 0
rule ds_at(bessel_i(nu_, u_), 2, 0, k_) := ds_bj(nu, u, k, 1) when is_integer(nu) and nu >= 0
rule ds_at(bessel_y(nu_, u_), 2, 0, k_) := {
  -(u/2)^(-nu)/pi*ds_sum(j => factorial(nu - j - 1)/factorial(j)*(u^2/4)^j, 0, nu - 1) +
  2/pi*(log(u) - log(2))*ds_bj(nu, u, k, -1) -
  (u/2)^nu/pi*ds_sum(j => (ds_psi1(j) + ds_psi1(nu + j))*(-u^2/4)^j/(factorial(j)*factorial(nu + j)), 0, k)
} when is_integer(nu) and nu >= 0
rule ds_at(bessel_k(nu_, u_), 2, 0, k_) := {
  (u/2)^(-nu)/2*ds_sum(j => factorial(nu - j - 1)/factorial(j)*(-u^2/4)^j, 0, nu - 1) +
  (-1)^(nu + 1)*(log(u) - log(2))*ds_bj(nu, u, k, 1) +
  (-1)^nu*(u/2)^nu/2*ds_sum(j => (ds_psi1(j) + ds_psi1(nu + j))*(u^2/4)^j/(factorial(j)*factorial(nu + j)), 0, k)
} when is_integer(nu) and nu >= 0
# Hankel functions of integer order: J +- i Y with the two expansions above (DLMF 10.4.3), so that a Laurent series
# with log(u) is given and not a Taylor series with the infinite values H(n, 0)
rule ds_at(hankel_h1(nu_, u_), 2, 0, k_) := ds_at(bessel_j(nu, u), 2, 0, k) + im*ds_at(bessel_y(nu, u), 2, 0, k) when is_integer(nu) and nu >= 0
rule ds_at(hankel_h2(nu_, u_), 2, 0, k_) := ds_at(bessel_j(nu, u), 2, 0, k) - im*ds_at(bessel_y(nu, u), 2, 0, k) when is_integer(nu) and nu >= 0
# Kelvin functions of order 0 (DLMF 10.65.1, 10.65.3)
fn ds_ber(u, k) = ds_sum(j => (-1)^j*(u^2/4)^(2*j)/factorial(2*j)^2, 0, k)
fn ds_bei(u, k) = ds_sum(j => (-1)^j*(u^2/4)^(2*j + 1)/factorial(2*j + 1)^2, 0, k)
rule ds_at(kelvin_ber(u_), 1, 0, k_) := ds_ber(u, k)
rule ds_at(kelvin_bei(u_), 1, 0, k_) := ds_bei(u, k)
rule ds_at(kelvin_ker(u_), 1, 0, k_) := {
  -(log(u) - log(2))*ds_ber(u, k) + pi/4*ds_bei(u, k) +
  ds_sum(j => (-1)^j*ds_psi1(2*j)*(u^2/4)^(2*j)/factorial(2*j)^2, 0, k)
}
rule ds_at(kelvin_kei(u_), 1, 0, k_) := {
  -(log(u) - log(2))*ds_bei(u, k) - pi/4*ds_ber(u, k) +
  ds_sum(j => (-1)^j*ds_psi1(2*j + 1)*(u^2/4)^(2*j + 1)/factorial(2*j + 1)^2, 0, k)
}
rule ds_at(kelvin_ker(nu_, u_), 2, 0, k_) := ds_at(kelvin_ker(u), 1, 0, k) when nu === 0
rule ds_at(kelvin_kei(nu_, u_), 2, 0, k_) := ds_at(kelvin_kei(u), 1, 0, k) when nu === 0
# Struve functions of integer order (DLMF 11.2.1, 11.2.2)
rule ds_at(struve_h(nu_, u_), 2, 0, k_) := {
  ds_sum(j => (-1)^j*(u/2)^(2*j + nu + 1)/(gamma(j + 3/2)*gamma(j + nu + 3/2)), 0, k)
} when is_integer(nu) and nu >= 0
rule ds_at(struve_l(nu_, u_), 2, 0, k_) := {
  ds_sum(j => (u/2)^(2*j + nu + 1)/(gamma(j + 3/2)*gamma(j + nu + 3/2)), 0, k)
} when is_integer(nu) and nu >= 0

# 1/Gamma(a), 0 at the poles of Gamma (1/Gamma is entire)
fn ds_rgamma(a) = if is_integer(a) and a <= 0 then 0 else 1/gamma(a)
# Bessel functions of an order that is not an integer near 0: J_nu = sum (-1)^j (u/2)^(nu + 2j)/(j! Gamma(nu + j + 1)),
# I_nu the same without (-1)^j (DLMF 10.2.2, 10.25.2), Y_nu = (cos(nu pi) J_nu - J_-nu)/sin(nu pi),
# K_nu = pi/2 (I_-nu - I_nu)/sin(nu pi) (DLMF 10.2.3, 10.27.4); Y and K at a numeric order only (at a symbolic order the
# quotient is not defined at the integers)
fn ds_bjnu(nu, u, k, s) = ds_sum(j => s^j*(u/2)^(nu + 2*j)*ds_rgamma(nu + j + 1)/factorial(j), 0, k)
rule ds_at(bessel_j(nu_, u_), 2, 0, k_) := ds_bjnu(nu, u, k, -1) when not is_integer(nu)
rule ds_at(bessel_i(nu_, u_), 2, 0, k_) := ds_bjnu(nu, u, k, 1) when not is_integer(nu)
rule ds_at(bessel_y(nu_, u_), 2, 0, k_) := {
  (cos(nu*pi)*ds_bjnu(nu, u, k, -1) - ds_bjnu(-nu, u, k, -1))/sin(nu*pi)
} when is_numeric(nu) and not is_integer(nu)
rule ds_at(bessel_k(nu_, u_), 2, 0, k_) := pi/2*(ds_bjnu(-nu, u, k, 1) - ds_bjnu(nu, u, k, 1))/sin(nu*pi) when is_numeric(nu) and not is_integer(nu)

# Lommel functions near 0: s_(mu, nu)(u) = sum_j (-1)^j u^(mu + 1 + 2j)/prod_(i=0..j) ((mu + 2i + 1)^2 - nu^2) (DLMF
# 11.9.3), t_(mu, nu) the same without (-1)^j; S = s + c (sin((mu - nu) pi/2) J_nu - cos((mu - nu) pi/2) Y_nu) (DLMF
# 11.9.5) and T = -t + c (I_nu + (sin(mu pi) + sin(nu pi))/pi K_nu), c = 2^(mu - 1) Gamma((mu - nu + 1)/2)
# Gamma((mu + nu + 1)/2); the series of s and t only where no denominator vanishes (mu +- nu not a negative odd integer)
fn ds_lommel_ok(mu, nu) = not (is_numeric(mu) and is_numeric(nu) and some([mu + nu, mu - nu], d => is_integer(d) and d < 0 and is_integer((d + 1)/2)))
fn ds_lommel_c(mu, nu) = 2^(mu - 1)*gamma((mu - nu + 1)/2)*gamma((mu + nu + 1)/2)
fn ds_lommel_sum(mu, nu, u, k, s) = {
  let n = k + 1 + (if is_numeric(mu) and is_real(mu) and mu < 0 then ceiling(-mu) else 0)
  ds_sum(j => s^j*u^(mu + 1 + 2*j)/ds_product(i => (mu + 2*i + 1)^2 - nu^2, 0, j), 0, n)
}
rule ds_at(lommel_s1(mu_, nu_, u_), 3, 0, k_) := ds_lommel_sum(mu, nu, u, k, -1) when is_numeric(mu) and ds_lommel_ok(mu, nu)
rule ds_at(lommel_t1(mu_, nu_, u_), 3, 0, k_) := ds_lommel_sum(mu, nu, u, k, 1) when is_numeric(mu) and ds_lommel_ok(mu, nu)
# a symbolic mu: u^(mu + 1) times a power series in u (the order counts the powers of that series)
fn ds_lommel_rel(mu, nu, u, n, s) = ds_sum(j => s^j*u^(2*j)/ds_product(i => (mu + 2*i + 1)^2 - nu^2, 0, j), 0, n)
rule ds_at(lommel_s1(mu_, nu_, u_), 3, 0, k_) := ds_pieces([[u^(mu + 1), ds_lommel_rel(mu, nu, u, max(0, floor((k - 3)/2)), -1)]]) when not is_numeric(mu)
rule ds_at(lommel_t1(mu_, nu_, u_), 3, 0, k_) := ds_pieces([[u^(mu + 1), ds_lommel_rel(mu, nu, u, max(0, floor((k - 3)/2)), 1)]]) when not is_numeric(mu)
rule ds_at(lommel_s2(mu_, nu_, u_), 3, 0, k_) := {
  lommel_s1(mu, nu, u) + ds_lommel_c(mu, nu)*(sin((mu - nu)*pi/2)*bessel_j(nu, u) - cos((mu - nu)*pi/2)*bessel_y(nu, u))
} when is_numeric(mu) and is_numeric(nu) and ds_lommel_ok(mu, nu) and not is_integer((mu - nu + 1)/2) and not is_integer((mu + nu + 1)/2)
rule ds_at(lommel_t2(mu_, nu_, u_), 3, 0, k_) := {
  -lommel_t1(mu, nu, u) + ds_lommel_c(mu, nu)*(bessel_i(nu, u) + (sin(mu*pi) + sin(nu*pi))/pi*bessel_k(nu, u))
} when is_numeric(mu) and is_numeric(nu) and ds_lommel_ok(mu, nu) and not is_integer((mu - nu + 1)/2) and not is_integer((mu + nu + 1)/2)

# near 0: complete elliptic integrals (DLMF 19.5.1, 19.5.2), the nome (the series of q in
# e = (1 - (1 - m)^(1/4))/(2 (1 + (1 - m)^(1/4))), DLMF 19.5.5 with OEIS A002103), theta functions in the nome
# (DLMF 20.2.1-4)
rule ds_at(elliptic_k(u_), 1, 0, k_) := pi/2*ds_sum(j => pochhammer(1/2, j)^2/factorial(j)^2*u^j, 0, k)
rule ds_at(elliptic_e(u_), 1, 0, k_) := pi/2*ds_sum(j => pochhammer(-1/2, j)*pochhammer(1/2, j)/factorial(j)^2*u^j, 0, k)
fn ds_nome_c() = [1, 2, 15, 150, 1707, 20910, 268616, 3567400, 48555069, 673458874, 9481557398, 135119529972,
  1944997539623, 28235172753886, 412850231439240, 6074299605306120]
rule ds_at(elliptic_nome_q(u_), 1, 0, k_) := {
  let ep = (1 - (1 - u)^(1/4))/(2*(1 + (1 - u)^(1/4)))
  ds_total(map(j => ds_nome_c()[j + 1]*ep^(4*j + 1), range(0, min(15, floor((k - 1)/4)))))
} when k <= 61
rule ds_at(elliptic_theta(a_, z_, q_), 3, 0, k_) := {
  let jm = floor(sqrt(k)) + 1
  if a === 1 then 2*ds_sum(j => (-1)^j*q^((j + 1/2)^2)*sin((2*j + 1)*z), 0, jm)
  else if a === 2 then 2*ds_sum(j => q^((j + 1/2)^2)*cos((2*j + 1)*z), 0, jm)
  else if a === 3 then 1 + 2*ds_sum(j => q^(j^2)*cos(2*j*z), 1, jm)
  else 1 + 2*ds_sum(j => (-1)^j*q^(j^2)*cos(2*j*z), 1, jm)
} when some([1, 2, 3, 4], b => b === a)

# gamma family at the poles 0, -1, ...: the recurrence moves the point to 1 (DLMF 5.5.2, 5.15.5)
rule ds_at(log_gamma(u_), 1, 0, k_) := log_gamma(u + 1) - log(u)
rule ds_at(digamma(u_), 1, u0_, k_) := digamma(u + 1 - u0) - ds_sum(j => 1/(u + j), 0, -u0) when is_integer(u0) and u0 <= 0
rule ds_at(polygamma(m_, u_), 2, u0_, k_) := {
  polygamma(m, u + 1 - u0) - (-1)^m*factorial(m)*ds_sum(j => (u + j)^(-m - 1), 0, -u0)
} when is_integer(u0) and u0 <= 0 and is_integer(m) and m >= 0
rule ds_at(harmonic_number(u_), 1, u0_, k_) := digamma(u + 1) + euler_gamma when is_integer(u0) and u0 <= -1

# near 0: the polylogarithm and the Lambert W function (DLMF 25.12.10, 4.13.5)
rule ds_at(polylog(s_, u_), 2, 0, k_) := ds_sum(j => u^j/j^s, 1, k)
rule ds_at(product_log(u_), 1, 0, k_) := ds_sum(j => (-j)^(j - 1)/factorial(j)*u^j, 1, k)

# near 0: the Weierstrass functions from the Laurent coefficients c_2 = g2/20, c_3 = g3/28,
# c_j = 3/((2j + 1)(j - 3)) sum_(i=2..j-2) c_i c_(j-i) of p = u^-2 + sum c_j u^(2j - 2) (DLMF 23.9.2-3)
fn ds_wp_c(g, jmax) = fold((cs, j) => append(cs, expand(3/((2*j + 1)*(j - 3))*ds_total(map(i => cs[i - 1]*cs[j - i - 1], range(2, j - 2))))),
  [g[1]/20, g[2]/28], range(4, jmax))
fn ds_wp(u, g, k) = {
  let jm = max(3, floor(k/2) + 2)
  let cs = ds_wp_c(g, jm)
  u^-2 + ds_total(map(j => cs[j - 1]*u^(2*j - 2), range(2, jm)))
}
rule ds_at(weierstrass_p(u_, g_), 1, 0, k_) := ds_wp(u, g, k) when is_list(g) and length(g) == 2
rule ds_at(weierstrass_p_prime(u_, g_), 1, 0, k_) := {
  let cs = ds_wp_c(g, max(3, floor(k/2) + 3))
  -2*u^-3 + ds_total(map(j => (2*j - 2)*cs[j - 1]*u^(2*j - 3), range(2, length(cs) + 1)))
} when is_list(g) and length(g) == 2
rule ds_at(weierstrass_zeta(u_, g_), 1, 0, k_) := {
  let cs = ds_wp_c(g, max(3, floor(k/2) + 2))
  1/u - ds_total(map(j => cs[j - 1]*u^(2*j - 1)/(2*j - 1), range(2, length(cs) + 1)))
} when is_list(g) and length(g) == 2
rule ds_at(weierstrass_sigma(u_, g_), 1, 0, k_) := {
  let cs = ds_wp_c(g, max(3, floor(k/2) + 2))
  u*exp(-ds_total(map(j => cs[j - 1]*u^(2*j)/((2*j - 1)*2*j), range(2, length(cs) + 1))))
} when is_list(g) and length(g) == 2

# the reciprocal and quotient Jacobi functions as quotients of sn, cn and dn, at any point
rule ds_at(jacobi_cs(u_, m_), 1, u0_, k_) := jacobi_cn(u, m)/jacobi_sn(u, m)
rule ds_at(jacobi_ds(u_, m_), 1, u0_, k_) := jacobi_dn(u, m)/jacobi_sn(u, m)
rule ds_at(jacobi_ns(u_, m_), 1, u0_, k_) := 1/jacobi_sn(u, m)
rule ds_at(jacobi_nc(u_, m_), 1, u0_, k_) := 1/jacobi_cn(u, m)
rule ds_at(jacobi_nd(u_, m_), 1, u0_, k_) := 1/jacobi_dn(u, m)
rule ds_at(jacobi_sc(u_, m_), 1, u0_, k_) := jacobi_sn(u, m)/jacobi_cn(u, m)
rule ds_at(jacobi_sd(u_, m_), 1, u0_, k_) := jacobi_sn(u, m)/jacobi_dn(u, m)
rule ds_at(jacobi_cd(u_, m_), 1, u0_, k_) := jacobi_cn(u, m)/jacobi_dn(u, m)
rule ds_at(jacobi_dc(u_, m_), 1, u0_, k_) := jacobi_dn(u, m)/jacobi_cn(u, m)

# the Jacobi functions as series in the parameter m about 0. The amplitude phi(u | m) = u + sum m^n phi_n(u)
# solves phi' = dn = (1 - m sin^2 phi)^(1/2), phi(0) = 0, so each phi_n is the integral of the m^n coefficient of the
# right side computed from phi_1 ... phi_(n-1) (Byrd and Friedman 710; Abramowitz and Stegun 16.13 for the first
# order); sn = sin(phi), cn = cos(phi), dn = phi', E(am u | m) = integral of dn^2, Z = E(am u | m) - u E(m)/K(m).
# Series are lists of the coefficients of m^0 ... m^K (index n + 1); the variable of the coefficients is ds_jz.
fn ds_lzero(n) = map(i => 0, range(1, n))
fn ds_ladd(a, b) = map(i => a[i] + b[i], range(1, length(a)))
fn ds_lscale(c, a) = map(x => c*x, a)
fn ds_lsum(ls, n) = fold((p, q) => ds_ladd(p, q), ds_lzero(n), ls)
fn ds_lmul(a, b) = map(n => ds_total(map(i => a[i + 1]*b[n - i + 1], range(0, n))), range(0, length(a) - 1))
fn ds_lpow(a, r) = fold((p, i) => ds_lmul(p, a), join([1], ds_lzero(length(a) - 1)), range(1, r))
fn ds_ldiv(a, b) = fold((c, n) => append(c, (a[n + 1] - ds_total(map(i => b[i + 1]*c[n - i + 1], range(1, n))))/b[1]), [a[1]/b[1]], range(1, length(a) - 1))
fn ds_lcos(d) = ds_lsum(map(r => ds_lscale((-1)^r/factorial(2*r), ds_lpow(d, 2*r)), range(0, length(d) - 1)), length(d))
fn ds_lsin(d) = ds_lsum(map(r => ds_lscale((-1)^r/factorial(2*r + 1), ds_lpow(d, 2*r + 1)), range(0, length(d) - 1)), length(d))
fn ds_jint(e) = integrate(rewrite(expand(trig_reduce(expand(e))), ds_jz -> ds_jt), [ds_jt, 0, ds_jz])
fn ds_jstep(phi) = {
  let n = length(phi)
  let d = join([0], map(i => phi[i], range(2, n)))
  let c2 = ds_lcos(ds_lscale(2, d))
  let s2 = ds_lsin(ds_lscale(2, d))
  let ss = map(i => ((if i == 1 then 1 else 0) - cos(2*ds_jz)*c2[i] + sin(2*ds_jz)*s2[i])/2, range(1, n))
  let y = join([0], map(i => -ss[i], range(1, n - 1)))
  let r = ds_lsum(map(j => ds_lscale(binomial(1/2, j), ds_lpow(y, j)), range(0, n - 1)), n)
  join([ds_jz], map(i => ds_jint(r[i]), range(2, n)))
}
# [sn, cn, dn, 1, amplitude, epsilon, zn] as series in m to order K
fn ds_jdata(K) = {
  let phi = fold((p, i) => ds_jstep(p), join([ds_jz], ds_lzero(K)), range(1, K))
  let d = join([0], map(i => phi[i], range(2, K + 1)))
  let cd = ds_lcos(d)
  let sd = ds_lsin(d)
  let c2 = ds_lcos(ds_lscale(2, d))
  let s2 = ds_lsin(ds_lscale(2, d))
  let sq = map(i => ((if i == 1 then 1 else 0) - cos(2*ds_jz)*c2[i] + sin(2*ds_jz)*s2[i])/2, range(1, K + 1))
  let y = join([0], map(i => -sq[i], range(1, K)))
  let rr = ds_lsum(map(j => ds_lscale(binomial(1/2, j), ds_lpow(y, j)), range(0, K)), K + 1)
  let sn = map(i => sin(ds_jz)*cd[i] + cos(ds_jz)*sd[i], range(1, K + 1))
  let cn = map(i => cos(ds_jz)*cd[i] - sin(ds_jz)*sd[i], range(1, K + 1))
  let eps = map(i => ds_jint(ds_lmul(rr, rr)[i]), range(1, K + 1))
  let ke = map(j => pochhammer(1/2, j)^2/factorial(j)^2, range(0, K))
  let ee = map(j => pochhammer(-1/2, j)*pochhammer(1/2, j)/factorial(j)^2, range(0, K))
  let ek = ds_ldiv(ee, ke)
  [sn, cn, rr, join([1], ds_lzero(K)), phi, eps, ds_ladd(eps, map(x => -ds_jz*x, ek))]
}
fn ds_lpoly(l, m, u) = ds_total(map(i => rewrite(l[i], ds_jz -> u)*m^(i - 1), range(1, length(l))))
fn ds_jr(u, m, k, p, q) = {
  let dt = ds_jdata(k - 3)
  ds_lpoly(ds_ldiv(dt[p], dt[q]), m, u)
}
rule ds_at(jacobi_sn(u_, m_), 2, 0, k_) := ds_lpoly(ds_jdata(k - 3)[1], m, u) when k >= 3 and k <= 8
rule ds_at(jacobi_cn(u_, m_), 2, 0, k_) := ds_lpoly(ds_jdata(k - 3)[2], m, u) when k >= 3 and k <= 8
rule ds_at(jacobi_dn(u_, m_), 2, 0, k_) := ds_lpoly(ds_jdata(k - 3)[3], m, u) when k >= 3 and k <= 8
rule ds_at(jacobi_amplitude(u_, m_), 2, 0, k_) := ds_lpoly(ds_jdata(k - 3)[5], m, u) when k >= 3 and k <= 8
rule ds_at(jacobi_epsilon(u_, m_), 2, 0, k_) := ds_lpoly(ds_jdata(k - 3)[6], m, u) when k >= 3 and k <= 8
rule ds_at(jacobi_zn(u_, m_), 2, 0, k_) := ds_lpoly(ds_jdata(k - 3)[7], m, u) when k >= 3 and k <= 8
rule ds_at(jacobi_cd(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 2, 3) when k >= 3 and k <= 8
rule ds_at(jacobi_cs(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 2, 1) when k >= 3 and k <= 8
rule ds_at(jacobi_dc(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 3, 2) when k >= 3 and k <= 8
rule ds_at(jacobi_ds(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 3, 1) when k >= 3 and k <= 8
rule ds_at(jacobi_nc(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 4, 2) when k >= 3 and k <= 8
rule ds_at(jacobi_nd(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 4, 3) when k >= 3 and k <= 8
rule ds_at(jacobi_ns(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 4, 1) when k >= 3 and k <= 8
rule ds_at(jacobi_sc(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 1, 2) when k >= 3 and k <= 8
rule ds_at(jacobi_sd(u_, m_), 2, 0, k_) := ds_jr(u, m, k, 1, 3) when k >= 3 and k <= 8

# the inverse functions to first order in m: u = u0 - pq_1(u0)/pq_0'(u0) m, from pq(u | m) = v (implicit differentiation
# at m = 0; pq_0 is the trigonometric function, pq_1 the m coefficient above)
fn ds_ij1(v, m, p, q, u0) = {
  let r = ds_ldiv(ds_jdata(1)[p], ds_jdata(1)[q])
  # the coefficient algebraic in v (sin(2 asin(v)) is 2 v sqrt(1 - v^2)) and as one fraction
  u0 - together(trig_expand(rewrite(r[2], ds_jz -> u0)/rewrite(d(r[1], ds_jz), ds_jz -> u0)))*m
}
rule ds_at(inverse_jacobi_sn(v_, m_), 2, 0, k_) := ds_ij1(v, m, 1, 4, asin(v)) when k == 4
rule ds_at(inverse_jacobi_cn(v_, m_), 2, 0, k_) := ds_ij1(v, m, 2, 4, acos(v)) when k == 4
rule ds_at(inverse_jacobi_cd(v_, m_), 2, 0, k_) := ds_ij1(v, m, 2, 3, acos(v)) when k == 4
rule ds_at(inverse_jacobi_sd(v_, m_), 2, 0, k_) := ds_ij1(v, m, 1, 3, asin(v)) when k == 4
rule ds_at(inverse_jacobi_dc(v_, m_), 2, 0, k_) := ds_ij1(v, m, 3, 2, asec(v)) when k == 4
rule ds_at(inverse_jacobi_nc(v_, m_), 2, 0, k_) := ds_ij1(v, m, 4, 2, asec(v)) when k == 4
rule ds_at(inverse_jacobi_sc(v_, m_), 2, 0, k_) := ds_ij1(v, m, 1, 2, atan(v)) when k == 4
rule ds_at(inverse_jacobi_ns(v_, m_), 2, 0, k_) := ds_ij1(v, m, 4, 1, acsc(v)) when k == 4
rule ds_at(inverse_jacobi_ds(v_, m_), 2, 0, k_) := ds_ij1(v, m, 3, 1, acsc(v)) when k == 4
rule ds_at(inverse_jacobi_cs(v_, m_), 2, 0, k_) := ds_ij1(v, m, 2, 1, acot(v)) when k == 4

# --- series: functions defined by a series, a differential equation or a known expansion in a parameter ---
# the Taylor/Frobenius coefficients y_0 ... y_K at a point of the solution of A y'' + B y' + C y = 0, with the polynomials
# given by their coefficient lists (powers of t = z - z0, index i + 1): at a regular singular point (A(z0) = 0) the
# solution of exponent 0 with y_0 = 1, else y_0 and y_1 given. The coefficient of t^n gives y_(n+1) (singular start) or
# y_(n+2): sum_i a_i (n+2-i)(n+1-i) y_(n+2-i) + sum_i b_i (n+1-i) y_(n+1-i) + sum_i c_i y_(n-i) = 0.
fn ds_l0(l, i) = if i < 0 or i >= length(l) then 0 else l[i + 1]
fn ds_ode_term(a, b, c, ys) = {
  let reg = a[1] === 0
  let m = length(ys) - 1
  let n = if reg then m else m - 1
  let y = j => if j < 0 then 0 else ys[j + 1]
  let sa = ds_sum(i => ds_l0(a, i)*(n + 2 - i)*(n + 1 - i)*y(n + 2 - i), if reg then 2 else 1, n + 2)
  let sb = ds_sum(i => ds_l0(b, i)*(n + 1 - i)*y(n + 1 - i), if reg then 1 else 0, n + 1)
  let sc = ds_sum(i => ds_l0(c, i)*y(n - i), 0, n)
  let den = if reg then (n + 1)*(n*ds_l0(a, 1) + ds_l0(b, 0)) else a[1]*(m + 1)*m
  -(sa + sb + sc)/den
}
fn ds_ode(a, b, c, ys0, K) = fold((ys, i) => append(ys, ds_ode_term(a, b, c, ys)), ys0, range(1, max(0, K + 1 - length(ys0))))
fn ds_cl(p, t, deg) = map(i => coefficient(expand(p), t, i), range(0, deg))
fn ds_poly(ys, t) = ds_total(map(j => ys[j + 1]*t^j, range(0, length(ys) - 1)))
fn ds_heun(A, B, C, z, z0, ys0, K) = {
  let sh = p => if z0 === 0 then p else rewrite(p, ds_z -> ds_z + z0)
  ds_poly(ds_ode(ds_cl(sh(A), ds_z, 4), ds_cl(sh(B), ds_z, 4), ds_cl(sh(C), ds_z, 4), ys0, K), z - z0)
}
rule ds_at(heun_g(a_, q_, al_, be_, ga_, de_, z_), 7, 0, k_) := {
  let ep = al + be + 1 - ga - de
  ds_heun(ds_z*(ds_z - 1)*(ds_z - a), ga*(ds_z - 1)*(ds_z - a) + de*ds_z*(ds_z - a) + ep*ds_z*(ds_z - 1), al*be*ds_z - q, z, 0, [1], k - 3)
} when k >= 3 and k <= 14
rule ds_at(heun_c(q_, al_, ga_, de_, ep_, z_), 6, 0, k_) := {
  ds_heun(ds_z*(ds_z - 1), ga*(ds_z - 1) + de*ds_z + ep*ds_z*(ds_z - 1), al*ds_z - q, z, 0, [1], k - 3)
} when k >= 3 and k <= 14
rule ds_at(heun_b(q_, al_, ga_, de_, ep_, z_), 6, 0, k_) := {
  ds_heun(ds_z, ga + de*ds_z + ep*ds_z^2, al*ds_z - q, z, 0, [1], k - 3)
} when k >= 3 and k <= 14
rule ds_at(heun_d(q_, al_, ga_, de_, ep_, z_), 6, 1, k_) := {
  ds_heun(ds_z^2, ep*ds_z^2 + de*ds_z + ga, al*ds_z - q, z, 1, [1, 0], k - 3)
} when k >= 3 and k <= 14
rule ds_at(heun_t(q_, al_, ga_, de_, ep_, z_), 6, 0, k_) := {
  ds_heun(1, ga + de*ds_z + ep*ds_z^2, al*ds_z - q, z, 0, [1, 0], k - 3)
} when k >= 3 and k <= 14

# Mathieu functions at 0: y'' + (a - 2 q cos 2z) y = 0 with y(0) = mathieu_c(a, q, 0), y'(0) = 0 (c) and y(0) = 0,
# y'(0) = mathieu_s_prime(a, q, 0) (s)
fn ds_mathieu(a, q, z, ys0, K) = {
  let cl = map(i => (if i == 0 then a else 0) - (if mod(i, 2) == 0 then 2*q*(-1)^(i/2)*2^i/factorial(i) else 0), range(0, K))
  ds_poly(ds_ode([1], [0], cl, ys0, K), z)
}
rule ds_at(mathieu_c(a_, q_, z_), 3, 0, k_) := ds_mathieu(a, q, z, [mathieu_c(a, q, 0), 0], k - 3) when k >= 3 and k <= 14
rule ds_at(mathieu_s(a_, q_, z_), 3, 0, k_) := ds_mathieu(a, q, z, [0, mathieu_s_prime(a, q, 0)], k - 3) when k >= 3 and k <= 14

# the characteristic value a_r(q) near q = 0 (DLMF 28.6.1-6; r >= 4: 28.6.14) and the spheroidal eigenvalue
# lambda = n (n + 1) - 2 (n^2 + n - 1 + m^2) g^2/((2 n - 1)(2 n + 3)) + O(g^4) (DLMF 30.9.1 less g^2)
rule ds_at(mathieu_characteristic_a(r_, q_), 2, 0, k_) := {
  if r == 0 then -q^2/2 + 7*q^4/128
  else if r == 1 then 1 + q - q^2/8 - q^3/64 - q^4/1536
  else if r == 2 then 4 + 5*q^2/12 - 763*q^4/13824
  else if r == 3 then 9 + q^2/16 + q^3/64 + 13*q^4/20480
  else r^2 + q^2/(2*(r^2 - 1)) + (5*r^2 + 7)*q^4/(32*(r^2 - 1)^3*(r^2 - 4))
} when is_integer(r) and r >= 0 and k <= 7
rule ds_at(spheroidal_eigenvalue(n_, m_, g_), 3, 0, k_) := n*(n + 1) - 2*(n^2 + n - 1 + m^2)*g^2/((2*n - 1)*(2*n + 3)) when k <= 6

# Mittag-Leffler, Pochhammer and Whittaker functions at 0 (the Euler pentagonal series; DLMF 13.14.2, 13.2.7)
rule ds_at(mittag_leffler_e(a_, b_, z_), 3, 0, k_) := ds_sum(j => z^j/gamma(b + a*j), 0, k)
rule ds_at(q_pochhammer(u_), 1, 0, k_) := ds_sum(j => (-1)^j*u^(j*(3*j - 1)/2), -ceiling(sqrt(2*k/3)) - 1, ceiling(sqrt(2*k/3)) + 1)
rule ds_at(whittaker_m(kk_, mu_, z_), 3, 0, k_) := {
  exp(-z/2)*z^(mu + 1/2)*ds_sum(j => pochhammer(1/2 + mu - kk, j)/pochhammer(1 + 2*mu, j)*z^j/factorial(j), 0, k)
} when not (is_integer(2*mu) and 2*mu <= -1)
rule ds_at(whittaker_w(kk_, mu_, z_), 3, 0, k_) := {
  let n = kk - mu - 1/2
  exp(-z/2)*z^(mu + 1/2)*(-1)^n*pochhammer(1 + 2*mu, n)*ds_sum(j => pochhammer(-n, j)/(pochhammer(1 + 2*mu, j)*factorial(j))*z^j, 0, n)
} when is_integer(kk - mu - 1/2) and kk - mu - 1/2 >= 0 and not (is_integer(2*mu) and 2*mu <= -1)
rule ds_at(q_hypergeometric_pfq(as_, bs_, q_, z_), 4, 0, k_) := {
  ds_sum(n => z^n*fold((p, a) => p*q_pochhammer(a, q, n), 1, as)/(fold((p, b) => p*q_pochhammer(b, q, n), 1, bs)*q_pochhammer(q, q, n))*((-1)^n*q^(n*(n - 1)/2))^(1 + length(bs) - length(as)), 0, k)
} when is_list(as) and is_list(bs) and k <= 12
# in the base q near 0, with r <= s (r, s the lengths of the lists): the n-th term has the factor
# q^(n (n - 1)(1 + s - r)/2) and the q-shifted factorials of parameters free of q tend to (1 - a) (n >= 1), so the terms
# with n (n - 1)/2 > k lie beyond the order; not where a lower parameter is 1 (the term is infinite at q = 0)
rule ds_at(q_hypergeometric_pfq(as_, bs_, q_, z_), 3, 0, k_) := {
  ds_sum(n => z^n*fold((p, a) => p*q_pochhammer(a, q, n), 1, as)/(fold((p, b) => p*q_pochhammer(b, q, n), 1, bs)*q_pochhammer(q, q, n))*((-1)^n*q^(n*(n - 1)/2))^(1 + length(bs) - length(as)), 0, ceiling(sqrt(2*k)) + 2)
} when is_list(as) and is_list(bs) and length(as) <= length(bs) and not some(bs, b => b === 1) and k <= 40

# the theta constants: elliptic_theta(a, q) is elliptic_theta(a, 0, q)
rule elliptic_theta(a_, q_) := elliptic_theta(a, 0, q) when is_integer(a) and a >= 1 and a <= 4

# the multinomial coefficient with a symbolic argument: (n1 + ... + nk)!/(n1! ... nk!); with one symbolic argument s and
# integers m_i it is the polynomial (s + 1)_(sum m_i)/(prod m_i!)
fn ds_multi(xs) = {
  let ints = select(xs, is_integer)
  let sy = select(xs, x => not is_integer(x))
  if length(sy) == 1 and every(ints, k => k >= 0) then pochhammer(sy[1] + 1, ds_total(ints))/fold((p, k) => p*factorial(k), 1, ints)
  else factorial(ds_total(xs))/fold((p, k) => p*factorial(k), 1, xs)
}
rule multinomial(a_, b_) := ds_multi([a, b]) when not (is_integer(a) and is_integer(b))
rule multinomial(a_, b_, c_) := ds_multi([a, b, c]) when not (is_integer(a) and is_integer(b) and is_integer(c))
rule multinomial(a_, b_, c_, d_) := ds_multi([a, b, c, d]) when not (is_integer(a) and is_integer(b) and is_integer(c) and is_integer(d))

# --- more series ---

# near 0 (more): the inverse nome as the quotient of theta constants lambda = 16 q (sum q^(j(j+1)))^4/(1 + 2 sum q^(j^2))^4
# (DLMF 20.9.1-3), the regularized pFq termwise, the Anger and Weber functions of a numeric order from their integral
# representations (DLMF 11.10.1-2, differentiated j times under the integral at z = 0), zeta from the functional
# equation with the Laurent series of zeta at 1 (DLMF 25.2.5, 25.4.1), the double factorial and the Barnes G function
# through gamma (DLMF 5.12 style recurrences G(z) = z G(z + 1)/gamma(z + 1))
rule ds_at(inverse_elliptic_nome_q(u_), 1, 0, k_) := {
  16*u*ds_sum(j => u^(j*(j + 1)), 0, ceiling(sqrt(k)) + 1)^4/(1 + 2*ds_sum(j => u^(j^2), 1, ceiling(sqrt(k)) + 1))^4
} when k <= 40
rule ds_at(hypergeometric_pfq_regularized(as_, bs_, z_), 3, 0, k_) := {
  ds_sum(j => z^j/factorial(j)*fold((p, a) => p*pochhammer(a, j), 1, as)/fold((p, b) => p*gamma(b + j), 1, bs), 0, k)
} when is_list(as) and is_list(bs)
rule ds_at(anger_j(nu_, z_), 2, 0, k_) := {
  ds_sum(j => integrate(sin(ds_t)^j*cos(nu*ds_t - j*pi/2), [ds_t, 0, pi])/(pi*factorial(j))*z^j, 0, k)
} when is_numeric(nu) and k <= 12
rule ds_at(weber_e(nu_, z_), 2, 0, k_) := {
  ds_sum(j => integrate((-sin(ds_t))^j*sin(nu*ds_t + j*pi/2), [ds_t, 0, pi])/(pi*factorial(j))*z^j, 0, k)
} when is_numeric(nu) and k <= 12
rule ds_at(zeta(u_), 1, 0, k_) := {
  2^u*pi^(u - 1)*sin(pi*u/2)*gamma(1 - u)*(-1/u + ds_sum(j => stieltjes_gamma(j)/factorial(j)*u^j, 0, k + 1))
} when k <= 40
# the Laurent series at the pole: zeta(u) = 1/(u - 1) + sum (-1)^j gamma_j/j! (u - 1)^j (Stieltjes constants, DLMF 25.2.4)
rule ds_at(zeta(u_), 1, 1, k_) := 1/(u - 1) + ds_sum(j => (-1)^j*stieltjes_gamma(j)/factorial(j)*(u - 1)^j, 0, k) when k <= 40
# Z(t) = e^(i theta(t)) zeta(1/2 + i t), theta(t) = Im log Gamma(1/4 + i t/2) - t log(pi)/2 (DLMF 25.10.3-4); Z is even, so
# the mean of the expansions at t and -t (the odd terms of each cancel only through zeta'(1/2) = -theta'(0) zeta(1/2))
fn ds_rsz(u) = exp(im*((log_gamma(1/4 + im*u/2) - log_gamma(1/4 - im*u/2))/(2*im) - u*log(pi)/2))*zeta(1/2 + im*u)
rule ds_at(riemann_siegel_z(u_), 1, 0, k_) := (ds_rsz(u) + ds_rsz(-u))/2 when k <= 8
rule ds_at(factorial2(u_), 1, 0, k_) := 2^(u/2 + 1/4 - cos(pi*u)/4)*pi^((cos(pi*u) - 1)/4)*gamma(u/2 + 1)
rule ds_at(barnes_g(u_), 1, 0, k_) := u*barnes_g(u + 1)/gamma(u + 1)
# log G(u) = log G(1 + u) - log Gamma(u) with log G(1 + u) = u log(2 pi)/2 - ((1 + gamma) u^2 + u)/2 + sum_(j>=2) (-1)^j
# zeta(j) u^(j+1)/(j + 1) (DLMF 5.17.3) and log Gamma(u) = -log u - gamma u + sum_(j>=2) (-1)^j zeta(j) u^j/j (DLMF 5.7.3)
rule ds_at(log_barnes_g(u_), 1, 0, k_) := {
  log(u) + u*(log(2*pi) - 1)/2 + euler_gamma*u - (1 + euler_gamma)*u^2/2 +
    ds_sum(j => (-1)^j*zeta(j)*(u^(j + 1)/(j + 1) - u^j/j), 2, k + 1)
} when k <= 40
rule ds_at(hyperfactorial(u_), 1, 0, k_) := gamma(u + 1)^u/barnes_g(u + 1)
rule ds_at(elliptic_theta_prime(a_, z_, q_), 3, 0, k_) := {
  let jm = floor(sqrt(k)) + 1
  if a === 1 then 2*ds_sum(j => (-1)^j*(2*j + 1)*q^((j + 1/2)^2)*cos((2*j + 1)*z), 0, jm)
  else if a === 2 then -2*ds_sum(j => (2*j + 1)*q^((j + 1/2)^2)*sin((2*j + 1)*z), 0, jm)
  else if a === 3 then -4*ds_sum(j => j*q^(j^2)*sin(2*j*z), 1, jm)
  else -4*ds_sum(j => (-1)^j*j*q^(j^2)*sin(2*j*z), 1, jm)
} when some([1, 2, 3, 4], b => b === a)

# near 0: the regular Coulomb function of integer order, C_l(eta) sum A_j u^j with A_(l+1) = 1,
# A_(l+2) = eta/(l + 1), (j + l)(j - l - 1) A_j = 2 eta A_(j-1) - A_(j-2) (DLMF 33.6.1, 33.2.5)
rule ds_at(coulomb_f(l_, eta_, u_), 3, 0, k_) := {
  let cl = 2^l*exp(-pi*eta/2 + re(log_gamma(l + 1 + im*eta)))/factorial(2*l + 1)
  let as = fold((cs, j) => append(cs, expand((2*eta*cs[length(cs)] - cs[length(cs) - 1])/((j + l)*(j - l - 1)))),
    [1, eta/(l + 1)], range(l + 3, k + l + 2))
  cl*ds_total(map(i => as[i]*u^(l + i), range(length(as))))
} when is_integer(l) and l >= 0


# at infinity: Stirling's series (DLMF 5.11.1, 5.11.2, 5.15.8), harmonic numbers H(u) = psi(u + 1) + gamma
fn ds_stirling(u, k) = ds_sum(j => bernoulli_b(2*j)/(2*j*(2*j - 1))*u^(1 - 2*j), 1, floor(k/2) + 1)
rule ds_at(log_gamma(u_), 1, inf, k_) := (u - 1/2)*log(u) - u + log(2*pi)/2 + ds_stirling(u, k)
# log G(u) = log G(1 + u) - log Gamma(u), log G(1 + u) ~ (u^2/2 - 1/12) log u - 3 u^2/4 + u log(2 pi)/2 + zeta'(-1) + sum_(j>=1)
# B_(2j+2)/(4 j (j + 1) u^(2j)) (DLMF 5.17.5), zeta'(-1) = 1/12 - log A (Glaisher's constant, DLMF 5.17.6)
rule ds_at(log_barnes_g(u_), 1, inf, k_) := {
  (u^2/2 - 1/12)*log(u) - 3*u^2/4 + u*log(2*pi)/2 + 1/12 - log(glaisher) +
    ds_sum(j => bernoulli_b(2*j + 2)/(4*j*(j + 1)*u^(2*j)), 1, floor(k/2) + 1) -
    ((u - 1/2)*log(u) - u + log(2*pi)/2 + ds_stirling(u, k))
}
rule ds_at(gamma(u_), 1, inf, k_) := ds_pieces([[exp(u*(log(u) - 1)), sqrt(2*pi)*u^(-1/2)*exp(ds_stirling(u, k))]])
rule ds_at(factorial(u_), 1, inf, k_) := ds_pieces([[exp(u*(log(u) - 1)), sqrt(2*pi)*u^(1/2)*exp(ds_stirling(u, k))]])
rule ds_at(digamma(u_), 1, inf, k_) := log(u) - 1/(2*u) - ds_sum(j => bernoulli_b(2*j)/(2*j*u^(2*j)), 1, floor(k/2) + 1)
rule ds_at(harmonic_number(u_), 1, inf, k_) := log(u) + euler_gamma + 1/(2*u) - ds_sum(j => bernoulli_b(2*j)/(2*j*u^(2*j)), 1, floor(k/2) + 1)
# Hurwitz zeta(s, u) ~ u^(1-s)/(s-1) + u^-s/2 + sum B_2j/(2j)! (s)_(2j-1) u^(1-s-2j) (DLMF 25.11.43), s != 1, and the
# generalized harmonic numbers H(u, r) = zeta(r) - zeta(r, u + 1) = zeta(r) + u^-r - zeta(r, u)
fn ds_hurwitz_tail(s, u, k) = ds_sum(j => bernoulli_b(2*j)/factorial(2*j)*pochhammer(s, 2*j - 1)*u^(1 - s - 2*j), 1, floor(k/2) + 1)
rule ds_at(zeta(s_, u_), 2, inf, k_) := u^(1 - s)/(s - 1) + u^(-s)/2 + ds_hurwitz_tail(s, u, k) when not (s === 1)
rule ds_at(harmonic_number(u_, r_), 1, inf, k_) := ds_at(harmonic_number(u), 1, inf, k) when r === 1
rule ds_at(harmonic_number(u_, r_), 1, inf, k_) := zeta(r) + u^(1 - r)/(1 - r) + u^(-r)/2 - ds_hurwitz_tail(r, u, k) when not (r === 1)
rule ds_at(polygamma(m_, u_), 2, inf, k_) := {
  (-1)^(m + 1)*(factorial(m - 1)/u^m + factorial(m)/(2*u^(m + 1)) + ds_sum(j => bernoulli_b(2*j)*factorial(2*j + m - 1)/(factorial(2*j)*u^(2*j + m)), 1, floor(k/2) + 1))
} when is_integer(m) and m >= 1
rule ds_at(polygamma(m_, u_), 2, inf, k_) := ds_at(digamma(u), 1, inf, k) when m === 0

# at infinity: U(a, b, u) ~ u^-a sum (a)_j (a - b + 1)_j/j! (-u)^-j (DLMF 13.7.3)
rule ds_at(hypergeometric_u(a_, b_, u_), 3, inf, k_) := u^(-a)*ds_sum(j => pochhammer(a, j)*pochhammer(a - b + 1, j)/factorial(j)*(-u)^(-j), 0, k)

# at infinity: Airy functions with zeta = 2/3 u^(3/2), u_j = gamma(3j + 1/2)/(54^j j! gamma(j + 1/2)),
# v_j = -(6j + 1)/(6j - 1) u_j (DLMF 9.7.5-8)
fn ds_airy_u(j) = gamma(3*j + 1/2)/(54^j*factorial(j)*gamma(j + 1/2))
fn ds_airy_v(j) = -(6*j + 1)/(6*j - 1)*ds_airy_u(j)
fn ds_airy(u, k, c, s) = {
  let z = 2/3*u^(3/2)
  ds_sum(j => s^j*c(j)*z^(-j), 0, k)
}
rule ds_at(airy_ai(u_), 1, inf, k_) := ds_pieces([[exp(-2/3*u^(3/2)), u^(-1/4)/(2*sqrt(pi))*ds_airy(u, k, ds_airy_u, -1)]])
rule ds_at(airy_bi(u_), 1, inf, k_) := ds_pieces([[exp(2/3*u^(3/2)), u^(-1/4)/sqrt(pi)*ds_airy(u, k, ds_airy_u, 1)]])
rule ds_at(airy_ai_prime(u_), 1, inf, k_) := ds_pieces([[exp(-2/3*u^(3/2)), -u^(1/4)/(2*sqrt(pi))*ds_airy(u, k, ds_airy_v, -1)]])
rule ds_at(airy_bi_prime(u_), 1, inf, k_) := ds_pieces([[exp(2/3*u^(3/2)), u^(1/4)/sqrt(pi)*ds_airy(u, k, ds_airy_v, 1)]])

# at infinity: Hankel's expansions with a_j(nu) = (4 nu^2 - 1^2)(4 nu^2 - 3^2)...(4 nu^2 - (2j - 1)^2)/(j! 8^j),
# omega = u - nu pi/2 - pi/4 (DLMF 10.17.3-4, 10.40.1-2)
fn ds_hankel_a(nu, j) = ds_product(i => 4*nu^2 - (2*i - 1)^2, 1, j)/(factorial(j)*8^j)
fn ds_hankel_p(nu, u, k) = ds_sum(j => (-1)^j*ds_hankel_a(nu, 2*j)*u^(-2*j), 0, floor(k/2) + 1)
fn ds_hankel_q(nu, u, k) = ds_sum(j => (-1)^j*ds_hankel_a(nu, 2*j + 1)*u^(-2*j - 1), 0, floor(k/2) + 1)
rule ds_at(bessel_j(nu_, u_), 2, inf, k_) := {
  let w = u - nu*pi/2 - pi/4
  ds_pieces([[cos(w), sqrt(2)*pi^(-1/2)*u^(-1/2)*ds_hankel_p(nu, u, k)], [sin(w), -sqrt(2)*pi^(-1/2)*u^(-1/2)*ds_hankel_q(nu, u, k)]])
}
rule ds_at(bessel_y(nu_, u_), 2, inf, k_) := {
  let w = u - nu*pi/2 - pi/4
  ds_pieces([[sin(w), sqrt(2)*pi^(-1/2)*u^(-1/2)*ds_hankel_p(nu, u, k)], [cos(w), sqrt(2)*pi^(-1/2)*u^(-1/2)*ds_hankel_q(nu, u, k)]])
}
rule ds_at(bessel_k(nu_, u_), 2, inf, k_) := ds_pieces([[exp(-u), sqrt(pi/2)*u^(-1/2)*ds_sum(j => ds_hankel_a(nu, j)*u^(-j), 0, k)]])
rule ds_at(bessel_i(nu_, u_), 2, inf, k_) := ds_pieces([[exp(u), u^(-1/2)/sqrt(2*pi)*ds_sum(j => (-1)^j*ds_hankel_a(nu, j)*u^(-j), 0, k)]])

# at infinity: the Hankel functions, H1 ~ sqrt(2/(pi u)) e^(i omega) sum i^j a_j(nu) u^-j and H2 its conjugate form
# (DLMF 10.17.5-6), the spherical Hankel functions of integer order n >= 0, finite sums
# h1_n(u) = e^(iu) sum_(j=0..n) i^(j - n - 1) a_j(n + 1/2) u^(-j - 1), a_j(n + 1/2) = (n + j)!/(2^j j! (n - j)!)
# (DLMF 10.49.1, 10.49.6), and h2_n with -i
rule ds_at(hankel_h1(nu_, u_), 2, inf, k_) := {
  ds_pieces([[exp(im*u - im*(nu/2 + 1/4)*pi), sqrt(2)*pi^(-1/2)*u^(-1/2)*ds_sum(j => im^j*ds_hankel_a(nu, j)*u^(-j), 0, k)]])
}
rule ds_at(hankel_h2(nu_, u_), 2, inf, k_) := {
  ds_pieces([[exp(-im*u + im*(nu/2 + 1/4)*pi), sqrt(2)*pi^(-1/2)*u^(-1/2)*ds_sum(j => (-im)^j*ds_hankel_a(nu, j)*u^(-j), 0, k)]])
}
fn ds_sph_a(n, j) = factorial(n + j)/(2^j*factorial(j)*factorial(n - j))
rule ds_at(spherical_hankel_h1(n_, u_), 2, inf, k_) := {
  ds_pieces([[exp(im*u), ds_sum(j => im^(j - n - 1)*ds_sph_a(n, j)*u^(-j - 1), 0, n)]])
} when is_integer(n) and n >= 0
rule ds_at(spherical_hankel_h2(n_, u_), 2, inf, k_) := {
  ds_pieces([[exp(-im*u), ds_sum(j => (-im)^(j - n - 1)*ds_sph_a(n, j)*u^(-j - 1), 0, n)]])
} when is_integer(n) and n >= 0

# at infinity (u -> +inf): erfc u ~ e^(-u^2)/(sqrt(pi) u) sum (-1)^j (1/2)_j u^(-2j) (DLMF 7.12.1), erf = 1 - erfc;
# the Fresnel integrals C = 1/2 + f sin(pi u^2/2) - g cos(pi u^2/2), S = 1/2 - f cos(pi u^2/2) - g sin(pi u^2/2)
# with f ~ (pi u)^-1 sum (-1)^m (1/2)_(2m) (pi u^2/2)^(-2m), g ~ (pi u)^-1 sum (-1)^m (1/2)_(2m+1) (pi u^2/2)^(-2m-1)
# (DLMF 7.5.3-4, 7.12.2-3); Si = pi/2 - f cos u - g sin u, Ci = f sin u - g cos u with f ~ u^-1 sum (-1)^m (2m)!
# u^(-2m), g ~ u^-2 sum (-1)^m (2m + 1)! u^(-2m) (DLMF 6.5.5-6, 6.12.3-4); Chi = (Ei(u) - E1(u))/2 from the expansions
# of Ei and E1 (DLMF 6.12.1-2), sinh u times the even terms and cosh u the odd ones of sum j!/u^(j+1) (Shi is left:
# its expansion depends on the direction by a constant); E_nu ~ e^-u/u sum (-1)^j (nu)_j u^-j (DLMF 8.20.2);
# Q(a, u) ~ u^(a-1) e^-u/Gamma(a) sum (a - 1)(a - 2)...(a - j) u^-j (DLMF 8.11.2)
fn ds_erfc_s(u, k) = pi^(-1/2)*ds_sum(j => (-1)^j*pochhammer(1/2, j)*u^(-2*j - 1), 0, floor(k/2) + 1)
rule ds_at(erfc(u_), 1, inf, k_) := ds_pieces([[exp(-u^2), ds_erfc_s(u, k)]])
rule ds_at(erf(u_), 1, inf, k_) := ds_pieces([[1, 1], [exp(-u^2), -ds_erfc_s(u, k)]])
fn ds_fres(u, k, r) = 1/(pi*u)*ds_sum(m => (-1)^m*pochhammer(1/2, 2*m + r)*(pi*u^2/2)^(-2*m - r), 0, floor(k/4) + 1)
rule ds_at(fresnel_c(u_), 1, inf, k_) := {
  ds_pieces([[1, 1/2], [sin(pi*u^2/2), ds_fres(u, k, 0)], [cos(pi*u^2/2), -ds_fres(u, k, 1)]])
}
rule ds_at(fresnel_s(u_), 1, inf, k_) := {
  ds_pieces([[1, 1/2], [cos(pi*u^2/2), -ds_fres(u, k, 0)], [sin(pi*u^2/2), -ds_fres(u, k, 1)]])
}
fn ds_trig_f(u, k, s) = ds_sum(m => s^m*factorial(2*m)*u^(-2*m - 1), 0, floor(k/2) + 1)
fn ds_trig_g(u, k, s) = ds_sum(m => s^m*factorial(2*m + 1)*u^(-2*m - 2), 0, floor(k/2) + 1)
rule ds_at(sin_integral(u_), 1, inf, k_) := {
  ds_pieces([[1, pi/2], [cos(u), -ds_trig_f(u, k, -1)], [sin(u), -ds_trig_g(u, k, -1)]])
}
rule ds_at(cos_integral(u_), 1, inf, k_) := ds_pieces([[sin(u), ds_trig_f(u, k, -1)], [cos(u), -ds_trig_g(u, k, -1)]])
rule ds_at(cosh_integral(u_), 1, inf, k_) := ds_pieces([[sinh(u), ds_trig_f(u, k, 1)], [cosh(u), ds_trig_g(u, k, 1)]])
rule ds_at(exp_integral_e(nu_, u_), 2, inf, k_) := {
  ds_pieces([[exp(-u), ds_sum(j => (-1)^j*ds_product(i => nu + i, 0, j - 1)*u^(-j - 1), 0, k)]])
}
rule ds_at(gamma_regularized(a_, u_), 2, inf, k_) := {
  ds_pieces([[exp(-u)*u^(a - 1), ds_sum(j => ds_product(i => a - i, 1, j)*u^(-j)/gamma(a), 0, k)]])
}

# at infinity: Whittaker W ~ e^(-u/2) u^kappa sum (1/2 + mu - kappa)_j (1/2 - mu - kappa)_j/j! (-u)^-j (DLMF 13.19.3),
# a finite sum when 1/2 + mu - kappa or 1/2 - mu - kappa is a nonpositive integer; M = e^(-u/2) u^(mu + 1/2)
# 1F1(mu - kappa + 1/2; 1 + 2 mu; u) (DLMF 13.14.2) only when the 1F1 is a polynomial (otherwise a growing part too)
rule ds_at(whittaker_w(ka_, mu_, u_), 3, inf, k_) := {
  ds_pieces([[exp(-u/2), u^ka*ds_sum(j => pochhammer(1/2 + mu - ka, j)*pochhammer(1/2 - mu - ka, j)/factorial(j)*(-u)^(-j), 0, k)]])
}
rule ds_at(whittaker_m(ka_, mu_, u_), 3, inf, k_) := {
  ds_pieces([[exp(-u/2), u^(mu + 1/2)*hypergeometric_1f1(mu - ka + 1/2, 1 + 2*mu, u)]])
} when is_integer(mu - ka + 1/2) and mu - ka + 1/2 <= 0 and not (is_integer(2*mu) and 2*mu < 0)

# at infinity: Li_s(u) = -(2 pi i)^s/s! B_s(1/2 + log(-u)/(2 pi i)) - (-1)^s Li_s(1/u) for integers s >= 1 (Jonquiere's
# inversion relation, DLMF 25.12.12), log(-u) kept; Phi(u, 1, a) = u^-a (-log(1 - u) - sum_(n<a) u^n/n) for
# integers a >= 1 (DLMF 25.14.1 summed), with log(1 - u) = log(-u) - sum u^-j/j; zeta(u) = sum j^-u (DLMF 25.2.1):
# each term exponentially smaller than the one before
rule ds_at(polylog(s_, u_), 2, inf, k_) := {
  let t = 1/2 + log(-u)/(2*pi*im)
  together(expand(-(2*pi*im)^s/factorial(s)*bernoulli_b(s, t))) - (-1)^s*ds_sum(j => u^(-j)/j^s, 1, k)
} when is_integer(s) and s >= 1
rule ds_at(lerch_phi(u_, s_, a_), 1, inf, k_) := {
  u^(-a)*(-log(-u) + ds_sum(j => u^(-j)/j, 1, k + a) - ds_sum(n => u^n/n, 1, a - 1))
} when s === 1 and is_integer(a) and a >= 1
rule ds_at(zeta(u_), 1, inf, k_) := ds_pieces(map(j => [j^(-u), 1], range(1, max(1, k - 3))))

# at infinity: zeros of Bessel functions (McMahon, DLMF 10.21.19) with beta = (k + nu/2 - 1/4) pi for J and
# (k + nu/2 - 3/4) pi for Y, mu = 4 nu^2; zeros of Airy functions a_k = -T(3 pi (4k - 1)/8), b_k = -T(3 pi (4k - 3)/8)
# with T(t) ~ t^(2/3) (1 + 5/48 t^-2 - 5/36 t^-4 + ...) (DLMF 9.9.6, 9.9.18)
fn ds_mcmahon(nu, b) = {
  let mu = 4*nu^2
  b - (mu - 1)/(8*b) - 4*(mu - 1)*(7*mu - 31)/(3*(8*b)^3) - 32*(mu - 1)*(83*mu^2 - 982*mu + 3779)/(15*(8*b)^5) -
    64*(mu - 1)*(6949*mu^3 - 153855*mu^2 + 1585743*mu - 6277237)/(105*(8*b)^7)
}
rule ds_at(bessel_j_zero(nu_, u_), 2, inf, k_) := ds_mcmahon(nu, (u + nu/2 - 1/4)*pi) when k <= 9
rule ds_at(bessel_y_zero(nu_, u_), 2, inf, k_) := ds_mcmahon(nu, (u + nu/2 - 3/4)*pi) when k <= 9
fn ds_airy_t(t) = t^(2/3)*(1 + 5/48*t^-2 - 5/36*t^-4 + 77125/82944*t^-6 - 108056875/6967296*t^-8
  + 162375596875/334430208*t^-10)
rule ds_at(airy_ai_zero(u_), 1, inf, k_) := -ds_airy_t(3*pi*(4*u - 1)/8) when k <= 15
rule ds_at(airy_bi_zero(u_), 1, inf, k_) := -ds_airy_t(3*pi*(4*u - 3)/8) when k <= 15

# the pieces ps of an expansion at infinity times c
fn ds_pscale(c, ps) = if c === 0 then [] else map(p => [p[1], c*p[2]], ps)

# at infinity: Struve functions, H_nu = Y_nu + (1/pi) sum_j Gamma(j + 1/2) (u/2)^(nu - 2j - 1)/Gamma(nu + 1/2 - j)
# (DLMF 11.6.1, |ph u| < pi) and L_nu = I_-nu - (1/pi) sum_j (-1)^j Gamma(j + 1/2) (u/2)^(nu - 2j - 1)/Gamma(nu + 1/2 - j)
# (DLMF 11.6.2, |ph u| < pi/2), with the expansions of Y and I above
fn ds_struve_alg(nu, u, k, s) = ds_sum(j => s^j*gamma(j + 1/2)*(u/2)^(nu - 2*j - 1)*ds_rgamma(nu + 1/2 - j), 0, floor(k/2) + 2)/pi
rule ds_at(struve_h(nu_, u_), 2, inf, k_) := ds_pieces(append(ds_at(bessel_y(nu, u), 2, inf, k)[1], [1, ds_struve_alg(nu, u, k, 1)]))
rule ds_at(struve_l(nu_, u_), 2, inf, k_) := ds_pieces(append(ds_at(bessel_i(-nu, u), 2, inf, k)[1], [1, -ds_struve_alg(nu, u, k, -1)]))

# at infinity: Kelvin functions of order nu. ker_nu + i kei_nu = e^(-nu pi i/2) K_nu(u e^(pi i/4)) (DLMF 10.61.2) with
# Hankel's expansion of K (DLMF 10.40.2) gives ker_nu ~ (pi/(2u))^(1/2) e^(-u/sqrt 2) sum_j a_j(nu) u^-j cos(b + j pi/4)
# and kei_nu ~ -(pi/(2u))^(1/2) e^(-u/sqrt 2) sum_j a_j(nu) u^-j sin(b + j pi/4), b = u/sqrt 2 + (nu/2 + 1/8) pi;
# ber_nu + i bei_nu = e^(nu pi i/2) I_nu(u e^(pi i/4)) (DLMF 10.61.1) with DLMF 10.40.5 (ph = pi/4 lies past the
# Stokes line ph = 0, so the recessive part has the multiplier i e^(nu pi i)) gives
# ber_nu ~ e^(u/sqrt 2)/(2 pi u)^(1/2) sum_j (-1)^j a_j(nu) u^-j cos(a - j pi/4) - (sin(2 nu pi) ker_nu + cos(2 nu pi) kei_nu)/pi
# bei_nu ~ e^(u/sqrt 2)/(2 pi u)^(1/2) sum_j (-1)^j a_j(nu) u^-j sin(a - j pi/4) + (cos(2 nu pi) ker_nu - sin(2 nu pi) kei_nu)/pi
# with a = u/sqrt 2 + (nu/2 - 1/8) pi (DLMF 10.67.1-4); the angles split by cos(a - j pi/4) = cos a cos(j pi/4) + ...
fn ds_kel_c(nu, u, k, s) = ds_sum(j => s^j*ds_hankel_a(nu, j)*cos(j*pi/4)*u^(-j), 0, k)
fn ds_kel_s(nu, u, k, s) = ds_sum(j => s^j*ds_hankel_a(nu, j)*sin(j*pi/4)*u^(-j), 0, k)
fn ds_ker_p(nu, u, k) = {
  let b = u/sqrt(2) + nu*pi/2 + pi/8
  let f = sqrt(pi/2)*u^(-1/2)
  [[exp(-u/sqrt(2))*cos(b), f*ds_kel_c(nu, u, k, 1)], [exp(-u/sqrt(2))*sin(b), -f*ds_kel_s(nu, u, k, 1)]]
}
fn ds_kei_p(nu, u, k) = {
  let b = u/sqrt(2) + nu*pi/2 + pi/8
  let f = sqrt(pi/2)*u^(-1/2)
  [[exp(-u/sqrt(2))*sin(b), -f*ds_kel_c(nu, u, k, 1)], [exp(-u/sqrt(2))*cos(b), -f*ds_kel_s(nu, u, k, 1)]]
}
fn ds_ber_p(nu, u, k) = {
  let a = u/sqrt(2) + nu*pi/2 - pi/8
  let f = u^(-1/2)/sqrt(2*pi)
  join([[exp(u/sqrt(2))*cos(a), f*ds_kel_c(nu, u, k, -1)], [exp(u/sqrt(2))*sin(a), f*ds_kel_s(nu, u, k, -1)]],
    join(ds_pscale(-sin(2*nu*pi)/pi, ds_ker_p(nu, u, k)), ds_pscale(-cos(2*nu*pi)/pi, ds_kei_p(nu, u, k))))
}
fn ds_bei_p(nu, u, k) = {
  let a = u/sqrt(2) + nu*pi/2 - pi/8
  let f = u^(-1/2)/sqrt(2*pi)
  join([[exp(u/sqrt(2))*sin(a), f*ds_kel_c(nu, u, k, -1)], [exp(u/sqrt(2))*cos(a), -f*ds_kel_s(nu, u, k, -1)]],
    join(ds_pscale(cos(2*nu*pi)/pi, ds_ker_p(nu, u, k)), ds_pscale(-sin(2*nu*pi)/pi, ds_kei_p(nu, u, k))))
}
rule ds_at(kelvin_ber(nu_, u_), 2, inf, k_) := ds_pieces(ds_ber_p(nu, u, k)) when is_numeric(nu) and is_real(nu)
rule ds_at(kelvin_bei(nu_, u_), 2, inf, k_) := ds_pieces(ds_bei_p(nu, u, k)) when is_numeric(nu) and is_real(nu)
rule ds_at(kelvin_ker(nu_, u_), 2, inf, k_) := ds_pieces(ds_ker_p(nu, u, k)) when is_numeric(nu) and is_real(nu)
rule ds_at(kelvin_kei(nu_, u_), 2, inf, k_) := ds_pieces(ds_kei_p(nu, u, k)) when is_numeric(nu) and is_real(nu)
rule ds_at(kelvin_ber(u_), 1, inf, k_) := ds_pieces(ds_ber_p(0, u, k))
rule ds_at(kelvin_bei(u_), 1, inf, k_) := ds_pieces(ds_bei_p(0, u, k))
rule ds_at(kelvin_ker(u_), 1, inf, k_) := ds_pieces(ds_ker_p(0, u, k))
rule ds_at(kelvin_kei(u_), 1, inf, k_) := ds_pieces(ds_kei_p(0, u, k))

# at infinity: the Mittag-Leffler function with 0 < a < 2, E_(a,b)(u) ~ u^((1 - b)/a) exp(u^(1/a))/a -
# sum_(j>=1) u^-j/Gamma(b - a j) for |ph u| <= m, a pi/2 < m < min(pi, a pi) (Gorenflo, Kilbas, Mainardi, Rogosin,
# Mittag-Leffler Functions, 2014, Theorem 4.3), so on the positive axis
rule ds_at(mittag_leffler_e(a_, b_, u_), 3, inf, k_) := {
  ds_pieces([[exp(u^(1/a)), u^((1 - b)/a)/a], [1, -ds_sum(j => u^(-j)*ds_rgamma(b - a*j), 1, k + 1)]])
} when is_numeric(a) and is_real(a) and a > 0 and a < 2
rule ds_at(mittag_leffler_e(a_, u_), 2, inf, k_) := ds_at(mittag_leffler_e(a, 1, u), 3, inf, k) when is_numeric(a) and is_real(a) and a > 0 and a < 2

# at infinity: Lommel functions. S_(mu, nu)(u) ~ u^(mu - 1) sum_j (-1)^j prod_(i=1..j) ((mu - 2i + 1)^2 - nu^2) u^(-2j)
# (DLMF 11.9.9, |ph u| < pi), T_(mu, nu) the same without (-1)^j (the formal solution of u^2 w'' + u w' - (u^2 + nu^2) w
# = -u^(mu + 1) in powers u^(mu - 1 - 2j)); s = S - c (sin((mu - nu) pi/2) J_nu - cos((mu - nu) pi/2) Y_nu) (DLMF 11.9.5)
# and t = -T + c I_nu + (a multiple of K_nu, recessive)
fn ds_lommel_alg(mu, nu, u, k, s) = {
  let n = floor(k/2) + 2 + (if is_numeric(mu) and is_real(mu) and mu > 0 then ceiling(mu/2) else 0)
  ds_sum(j => s^j*ds_product(i => (mu - 2*i + 1)^2 - nu^2, 1, j)*u^(-2*j), 0, n)
}
# the algebraic part as pieces: at a symbolic mu the power u^(mu - 1) stays outside the power series
fn ds_lommel_inf(mu, nu, u, k, s, c) = {
  if is_numeric(mu) then [[1, c*u^(mu - 1)*ds_lommel_alg(mu, nu, u, k, s)]] else [[u^(mu - 1), c*ds_lommel_alg(mu, nu, u, k, s)]]
}
fn ds_lommel_cok(mu, nu) = not some([(mu - nu + 1)/2, (mu + nu + 1)/2], g => is_integer(g) and g <= 0)
rule ds_at(lommel_s2(mu_, nu_, u_), 3, inf, k_) := ds_pieces(ds_lommel_inf(mu, nu, u, k, -1, 1)) when ds_lommel_ok(mu, nu)
rule ds_at(lommel_t2(mu_, nu_, u_), 3, inf, k_) := ds_pieces(ds_lommel_inf(mu, nu, u, k, 1, 1)) when ds_lommel_ok(mu, nu)
rule ds_at(lommel_s1(mu_, nu_, u_), 3, inf, k_) := {
  let c = ds_lommel_c(mu, nu)
  let a = (mu - nu)*pi/2
  ds_pieces(join(ds_lommel_inf(mu, nu, u, k, -1, 1), join(ds_pscale(-c*sin(a), ds_at(bessel_j(nu, u), 2, inf, k)[1]),
    ds_pscale(c*cos(a), ds_at(bessel_y(nu, u), 2, inf, k)[1]))))
} when ds_lommel_ok(mu, nu) and ds_lommel_cok(mu, nu)
rule ds_at(lommel_t1(mu_, nu_, u_), 3, inf, k_) := {
  ds_pieces(join(ds_lommel_inf(mu, nu, u, k, 1, -1), ds_pscale(ds_lommel_c(mu, nu), ds_at(bessel_i(nu, u), 2, inf, k)[1])))
} when ds_lommel_ok(mu, nu) and ds_lommel_cok(mu, nu)

# --- symbolic identities and special values ---
#
# Rules on the special-function natives, tried only when the native declines (symbolic arguments, points where the
# numerical paths give no value). Each is a classical identity (the DLMF chapter is cited) at the arguments shown.

# the functions vanish at their own zeros
rule airy_ai(airy_ai_zero(k_)) := 0
rule airy_bi(airy_bi_zero(k_)) := 0
# (the zeros of a continuous index k are those of J cos(pi t) + Y sin(pi t), DLMF 10.21(iii): only an integer k is a zero of
# the function itself)
rule bessel_j(nu_, bessel_j_zero(nu_, k_)) := 0 when is_integer(k)
rule bessel_y(nu_, bessel_y_zero(nu_, k_)) := 0 when is_integer(k)
rule zeta(zeta_zero(k_)) := 0
rule riemann_siegel_z(im_part(zeta_zero(k_))) := 0

# values at 0 and at infinity; Anger and Weber functions at 0 (DLMF 11.10.1-2)
rule anger_j(nu_, 0) := sinc(pi*nu)
rule weber_e(nu_, 0) := (if nu == 0 then 0 else (1 - cos(pi*nu))/(pi*nu)) when is_numeric(nu)

# --- closed forms at exact arguments and symmetries ---
# an exact number (rational, algebraic or a multiple of pi; not a machine or ball number)
fn ds_exact(z) = is_numeric(z) and precision(z) === inf
# z written with a negative sign: a negative rational, or a product with a negative rational coefficient
fn ds_negated(z) = (is_rational(z) and z < 0) or (not is_atom(z) and head(z) === mul and is_rational(z[1]) and z[1] < 0)
# Anger and Weber functions: from the integrals (1/pi) int_0^pi cos(nu t - z sin t) dt and the same with sin
# (DLMF 11.10.1-2), J_nu(-z) = J_-nu(z) and E_nu(-z) = -E_-nu(z); at an integer order and an exact z,
# J_n(z) = J_n(z) (DLMF 11.10.4) and E_n(z) = -H_n(z) + (1/pi) sum_(k < n/2) Gamma(k + 1/2) (z/2)^(n - 2k - 1)/
# Gamma(n + 1/2 - k) for n >= 0 (DLMF 11.10.15-16)
# (at an integer order the parities of J_n and of -H_n + the polynomial: J_n(-z) = (-1)^n J_n(z), E_n(-z) =
# (-1)^(n+1) E_n(z), and J_-n = (-1)^n J_n)
rule anger_j(nu_, z_) := anger_j(-nu, -z) when ds_negated(z) and not is_integer(nu)
rule weber_e(nu_, z_) := -weber_e(-nu, -z) when ds_negated(z) and not is_integer(nu)
rule anger_j(n_, z_) := (-1)^n*anger_j(n, -z) when is_integer(n) and ds_negated(z)
rule weber_e(n_, z_) := (-1)^(n + 1)*weber_e(n, -z) when is_integer(n) and ds_negated(z)
rule anger_j(n_, z_) := (-1)^n*anger_j(-n, z) when is_integer(n) and n < 0
rule anger_j(n_, z_) := bessel_j(n, z) when is_integer(n) and ds_exact(z)
rule weber_e(n_, z_) := {
  -struve_h(n, z) + ds_sum(k => gamma(k + 1/2)*(z/2)^(n - 2*k - 1)/gamma(n + 1/2 - k), 0, floor((n - 1)/2))/pi
} when is_integer(n) and n >= 0 and n <= 40 and ds_exact(z)
# the incomplete beta function of positive integer parameters, a + b <= 41: I_z(a, b) = sum_(j=a..n) C(n, j) z^j
# (1 - z)^(n - j), n = a + b - 1 (DLMF 8.17.5), at exact z (and I_z(1, 1) = z for any z); its inverse where the
# polynomial is solved by a root: (1, b): 1 - (1 - s)^(1/b), (a, 1): s^(1/a), a = b at s = 1/2: 1/2 (8.17.4 symmetry);
# Q(1, x) = e^-x, so its inverse is -log(s) (Gamma(1, x) = e^-x from the definition, DLMF 8.2.2)
fn ds_ibeta(z, a, b) = ds_sum(j => binomial(a + b - 1, j)*z^j*(1 - z)^(a + b - 1 - j), a, a + b - 1)
rule beta_regularized(z_, a_, b_) := z when a === 1 and b === 1
rule beta_regularized(z_, a_, b_) := ds_ibeta(z, a, b) when is_integer(a) and is_integer(b) and a >= 1 and b >= 1 and a + b <= 41 and ds_exact(z)
rule beta(z_, a_, b_) := beta(a, b)*ds_ibeta(z, a, b) when is_integer(a) and is_integer(b) and a >= 1 and b >= 1 and a + b <= 41 and ds_exact(z)
rule inverse_beta_regularized(s_, a_, b_) := 1 - (1 - s)^(1/b) when a === 1 and is_integer(b) and b >= 1 and ds_exact(s) and is_real(s) and s >= 0 and s <= 1
rule inverse_beta_regularized(s_, a_, b_) := s^(1/a) when b === 1 and is_integer(a) and a >= 1 and ds_exact(s) and is_real(s) and s >= 0 and s <= 1
rule inverse_beta_regularized(s_, a_, b_) := 1/2 when s === 1/2 and a === b and is_numeric(a) and is_real(a) and a > 0
rule inverse_gamma_regularized(a_, s_) := -log(s) when a === 1 and ds_exact(s) and is_real(s) and s > 0 and s <= 1
# U(a, a, z) = e^z Gamma(1 - a, z) (DLMF 13.6.6 with 13.2.40), at exact numbers; E_(1/2)(z) = e^(z^2) erfc(-z) (the
# Mittag-Leffler series DLMF 10.46.3 split into even and odd powers, the odd part e^(z^2) erf(z), DLMF 7.6.2); erf(i b) = i erfi(b), erfi(i b) = i erf(b)
# (erfi(z) = -i erf(i z), DLMF 7.2.1 and the definition of erfi); W(-pi/2) = i pi/2 (i pi/2 e^(i pi/2) = -pi/2, the principal branch: Im W in (-pi, pi));
# Hurwitz zeta at a half-integer above 1 by zeta(s, a + 1) = zeta(s, a) - a^-s down to zeta(s, 1/2) = (2^s - 1)
# zeta(s) (DLMF 25.11.3, 25.11.11); (q; q)_inf at q = 1 is 0
rule hypergeometric_u(a_, b_, z_) := exp(z)*gamma(1 - a, z) when a === b and is_rational(a) and ds_exact(z) and not (is_real(z) and z <= 0)
rule mittag_leffler_e(a_, z_) := exp(z^2)*erfc(-z) when a === 1/2 and ds_exact(z)
rule erf(z_) := im*erfi(im_part(z)) when ds_exact(z) and re(z) == 0 and not (z === 0)
rule erfi(z_) := im*erf(im_part(z)) when ds_exact(z) and re(z) == 0 and not (z === 0)
# erf(+-i inf) = +-i inf (erf(i y) = i erfi(y), erfi(y) -> inf); gamma(+-i inf) = 0 (|gamma(x + i y)| ~ sqrt(2 pi)
# |y|^(x - 1/2) e^(-pi |y|/2), DLMF 5.11.9); gamma(-inf) has no limit (poles at every nonpositive integer)
rule erf(z_) := z when z === im*inf or z === -im*inf
rule gamma(z_) := 0 when z === im*inf or z === -im*inf
rule gamma(-inf) := indeterminate
rule product_log(z_) := im*pi/2 when z === -pi/2
rule zeta(s_, a_) := zeta(s, a - 1) - (a - 1)^(-s) when is_integer(s) and s >= 2 and is_rational(a) and a > 1 and denominator(a) == 2
rule q_pochhammer(q_) := 0 when q === 1
rule barnes_g(inf) := inf
rule log_barnes_g(inf) := inf
rule log_barnes_g(0) := -inf
rule beta(inf, 0) := complex_infinity
rule beta(0, inf) := complex_infinity
rule product_log(complex_infinity) := inf
rule product_log(k_, 0) := -inf when is_integer(k) and not (k == 0)
# the trigonometric and exponential integrals have no limit at complex infinity (the directions disagree)
rule sin_integral(complex_infinity) := indeterminate
rule sinh_integral(complex_infinity) := indeterminate
rule cos_integral(complex_infinity) := indeterminate
rule cosh_integral(complex_infinity) := indeterminate
rule exp_integral_ei(complex_infinity) := indeterminate
# the Fresnel integrals along the imaginary axis: S(i x) = -i S(x), C(i x) = i C(x), the limits 1/2 at +inf (DLMF 7.4.3)
rule fresnel_s(1im*inf) := -1im/2
rule fresnel_s(-1im*inf) := 1im/2
rule fresnel_c(1im*inf) := 1im/2
rule fresnel_c(-1im*inf) := -1im/2
rule fresnel_c(complex_infinity) := indeterminate
rule fresnel_s(complex_infinity) := indeterminate
rule fresnel_f(complex_infinity) := indeterminate
rule fresnel_g(complex_infinity) := indeterminate
# Whittaker functions at 0: M ~ z^(mu + 1/2), W ~ gamma(2 mu)/gamma(1/2 + mu - k) z^(1/2 - mu) (DLMF 13.14.14-21)
rule whittaker_m(k_, mu_, 0) := 0 when is_numeric(mu) and re(mu) > -1/2
rule whittaker_w(k_, mu_, 0) := 0 when is_numeric(mu) and abs(re(mu)) < 1/2
rule whittaker_w(k_, mu_, 0) := (if is_integer(1 - k) and 1 - k <= 0 then 0 else 1/gamma(1 - k)) when mu === 1/2 or mu === -1/2
rule whittaker_w(k_, mu_, 0) := complex_infinity when is_numeric(mu) and is_numeric(k) and abs(re(mu)) > 1/2 and not (is_integer(1/2 + abs(mu) - k) and 1/2 + abs(mu) - k <= 0)

# parity: integer orders (DLMF 10.4.1, 10.27.1, 10.61.6), odd elliptic integrals and Jacobi functions
rule bessel_j(nu_, -z_) := (-1)^nu*bessel_j(nu, z) when is_integer(nu)
rule bessel_i(nu_, -z_) := (-1)^nu*bessel_i(nu, z) when is_integer(nu)
rule kelvin_ber(nu_, -z_) := (-1)^nu*kelvin_ber(nu, z) when is_integer(nu)
rule kelvin_bei(nu_, -z_) := (-1)^nu*kelvin_bei(nu, z) when is_integer(nu)
rule elliptic_e(-phi_, m_) := -elliptic_e(phi, m)
rule elliptic_f(-phi_, m_) := -elliptic_f(phi, m)
rule elliptic_pi(n_, -phi_, m_) := -elliptic_pi(n, phi, m)
rule jacobi_amplitude(-u_, m_) := -jacobi_amplitude(u, m)
rule jacobi_epsilon(-u_, m_) := -jacobi_epsilon(u, m)
rule jacobi_zn(-u_, m_) := -jacobi_zn(u, m)
rule inverse_jacobi_sn(-v_, m_) := -inverse_jacobi_sn(v, m)
rule inverse_jacobi_sc(-v_, m_) := -inverse_jacobi_sc(v, m)
rule inverse_jacobi_sd(-v_, m_) := -inverse_jacobi_sd(v, m)
rule inverse_jacobi_ns(-v_, m_) := -inverse_jacobi_ns(v, m)
rule inverse_jacobi_cs(-v_, m_) := -inverse_jacobi_cs(v, m)
rule inverse_jacobi_ds(-v_, m_) := -inverse_jacobi_ds(v, m)

# inverse Jacobi functions at m = 0 (circular), m = 1 (hyperbolic) and m = inf (DLMF 22.5.3-4)
rule inverse_jacobi_sn(v_, 0) := asin(v)
rule inverse_jacobi_cn(v_, 0) := acos(v)
rule inverse_jacobi_cd(v_, 0) := acos(v)
rule inverse_jacobi_sd(v_, 0) := asin(v)
rule inverse_jacobi_dc(v_, 0) := asec(v)
rule inverse_jacobi_nc(v_, 0) := asec(v)
rule inverse_jacobi_sc(v_, 0) := atan(v)
rule inverse_jacobi_ns(v_, 0) := acsc(v)
rule inverse_jacobi_ds(v_, 0) := acsc(v)
rule inverse_jacobi_cs(v_, 0) := acot(v)
rule inverse_jacobi_sn(v_, 1) := atanh(v)
rule inverse_jacobi_cn(v_, 1) := asech(v)
rule inverse_jacobi_dn(v_, 1) := asech(v)
rule inverse_jacobi_sd(v_, 1) := asinh(v)
rule inverse_jacobi_nd(v_, 1) := acosh(v)
rule inverse_jacobi_nc(v_, 1) := acosh(v)
rule inverse_jacobi_sc(v_, 1) := asinh(v)
rule inverse_jacobi_ns(v_, 1) := acoth(v)
rule inverse_jacobi_ds(v_, 1) := acsch(v)
rule inverse_jacobi_cs(v_, 1) := acsch(v)
rule inverse_jacobi_cd(v_, inf) := 0
rule inverse_jacobi_cn(v_, inf) := 0
rule inverse_jacobi_cs(v_, inf) := 0
rule inverse_jacobi_dn(v_, inf) := 0
rule inverse_jacobi_ds(v_, inf) := 0
rule inverse_jacobi_nc(v_, inf) := 0
rule inverse_jacobi_nd(v_, inf) := 0
rule inverse_jacobi_ns(v_, inf) := 0
rule inverse_jacobi_sc(v_, inf) := 0
rule inverse_jacobi_sd(v_, inf) := 0
rule inverse_jacobi_sc(inf, m_) := elliptic_k(m)

# the inverses at the quarter periods: sn(K) = 1, cn(K) = cd(K) = cs(K) = 0, cn(2K) = -1, ns(K) = 1, sc(i K') = i
# (DLMF 22.5.1, Table 22.4.3; principal values F(asin v | m), F(acos v | m))
rule inverse_jacobi_sn(1, m_) := elliptic_k(m)
rule inverse_jacobi_sn(-1, m_) := -elliptic_k(m)
rule inverse_jacobi_cn(0, m_) := elliptic_k(m)
rule inverse_jacobi_cn(-1, m_) := 2*elliptic_k(m)
rule inverse_jacobi_cd(0, m_) := elliptic_k(m)
rule inverse_jacobi_cs(0, m_) := elliptic_k(m)
rule inverse_jacobi_ns(1, m_) := elliptic_k(m)
rule inverse_jacobi_ns(-1, m_) := -elliptic_k(m)
rule inverse_jacobi_sc(1im, m_) := 1im*elliptic_k(1 - m)

# the amplitude, zeta and epsilon functions at m = 1 and at the quarter and half period (DLMF 22.16.1, 22.16.27-32):
# am(u | 1) = gd(u), Z(u | 1) = E(u | 1) = tanh(u), am(K) = pi/2, E(K) = E(m), Z(K) = 0, Z(K/2) = m/(2(1 + k'))
rule jacobi_amplitude(u_, 1) := gudermannian(u)
rule jacobi_zn(u_, 1) := tanh(u)
rule jacobi_epsilon(u_, 1) := tanh(u)
rule gudermannian(inf) := pi/2
rule gudermannian(-inf) := -pi/2
rule jacobi_amplitude(elliptic_k(m_), m_) := pi/2
rule jacobi_epsilon(elliptic_k(m_), m_) := elliptic_e(m)
rule jacobi_zn(elliptic_k(m_), m_) := 0
# whole periods drop: Z(u + 2 j K | m) = Z(u | m) (E(am(u + 2K)) = E(am u) + 2 E(m), DLMF 22.16.29), lambda(tau + 2 j)
# = lambda(tau) (lambda is invariant under the level-2 congruence group, DLMF 23.15.3), and the inverse Gudermannian
# 2 atanh(tan(x/2)) has the period 2 pi of tan(x/2)
rule jacobi_zn(u_ + c_*elliptic_k(m_), m_) := jacobi_zn(u, m) when is_integer(c/2)
rule modular_lambda(tau_ + c_) := modular_lambda(tau) when is_integer(c/2) and c != 0
rule inverse_gudermannian(x_ + c_*pi) := inverse_gudermannian(x) when is_integer(c/2)
rule jacobi_zn(c_*elliptic_k(m_), m_) := m/(2 + 2*sqrt(1 - m)) when c == 1/2
rule jacobi_epsilon(c_*elliptic_k(m_), m_) := elliptic_e(m)/2 + m/(2 + 2*sqrt(1 - m)) when c == 1/2

# 0F1 at a half-integer b: 0F1(;1/2;x) = cosh(2 sqrt x), 0F1(;3/2;x) = sinh(2 sqrt x)/(2 sqrt x), and the contiguous relation
# 0F1(;b-1;x) - 0F1(;b;x) = x/(b (b - 1)) 0F1(;b+1;x) (from the series, DLMF 16.3) in both directions
fn h0f1_half(b, x) = {
  let g = 2*sqrt(x)
  var i0 = 1/2
  var f0 = cosh(g)
  var f1 = sinh(g)/g
  while i0 < b {
    let nxt = (i0 + 1)*i0/x*(f0 - f1)
    f0 = f1
    f1 = nxt
    i0 = i0 + 1
  }
  while i0 > b {
    let prv = f0 + x*f1/(i0*(i0 - 1))
    f1 = f0
    f0 = prv
    i0 = i0 - 1
  }
  f0
}
rule hypergeometric_0f1(b_, x_) := h0f1_half(b, x) when is_rational(b) and denominator(b) == 2 and abs(b) < 30 and not (x === 0)
rule hypergeometric_0f1_regularized(b_, x_) := h0f1_half(b, x)/gamma(b) when is_rational(b) and denominator(b) == 2 and abs(b) < 30 and not (x === 0)

# the Hurwitz zeta function at 1/4 and 3/4: zeta(2, 1/4) = pi^2 + 8 G, zeta(2, 3/4) = pi^2 - 8 G (Catalan's G; DLMF 25.11.14
# with the reflection of the trigamma function)
rule zeta(2, a_) := pi^2 + 8*catalan when a === 1/4
rule zeta(2, a_) := pi^2 - 8*catalan when a === 3/4

# the Lerch function at a non-positive integer order: Phi(z, -n, a) = (a + z d/dz)^n 1/(1 - z) (the series sum (k + a)^n z^k)
fn lp_neg(z, n, a) = {
  var f = 1/(1 - z)
  for i in range(1, n) {
    f = a*f + z*d(f, z)
  }
  together(f)
}
rule lerch_phi(z_, s_, a_) := lp_neg(z, -s, a) when is_integer(s) and s <= 0 and s >= -40 and not is_numeric(z)

# Klein's J vanishes at the cube roots of unity rho = (-1 + i sqrt 3)/2 and rho + 1; log G(n) = -inf at the poles of 1/G
rule klein_invariant_j(t_) := 0 when t === (-1 + 1im*sqrt(3))/2 or t === (1 + 1im*sqrt(3))/2
rule log_barnes_g(n_) := -inf when is_integer(n) and n <= 0

# the poles of the Jacobi zeta and epsilon functions at the odd multiples of i K' (dn has simple poles there, so dn^2
# has double poles with residue 0 and its integral E(u) simple poles, DLMF 22.16.14, 22.16.32); at K + i K' the values
# E(K + i K') = E + i (K' - E') (DLMF 22.16.31 at u = K) and Z(K + i K') = E(K + i K') - (E/K)(K + i K') = -i pi/(2 K)
# by Legendre's relation E K' + E' K - K K' = pi/2 (DLMF 19.7.1)
fn ds_odd_ik(c) = is_numeric(c) and re(c) == 0 and is_integer(im_part(c)) and is_odd(im_part(c))
rule jacobi_epsilon(c_*elliptic_k(1 - m_), m_) := complex_infinity when ds_odd_ik(c)
rule jacobi_zn(c_*elliptic_k(1 - m_), m_) := complex_infinity when ds_odd_ik(c)
rule jacobi_epsilon(elliptic_k(m_) + c_*elliptic_k(1 - m_), m_) := elliptic_e(m) + im*(elliptic_k(1 - m) - elliptic_e(1 - m)) when c === im
rule jacobi_zn(elliptic_k(m_) + c_*elliptic_k(1 - m_), m_) := -im*pi/(2*elliptic_k(m)) when c === im

# quarter- and half-period shifts of the Jacobi zeta and epsilon functions (DLMF 22.16.27-32)
rule jacobi_zn(u_ + elliptic_k(m_), m_) := jacobi_zn(u, m) - m*jacobi_cd(u, m)*jacobi_sn(u, m)
rule jacobi_zn(u_ + 2im*elliptic_k(1 - m_), m_) := jacobi_zn(u, m) - im*pi/elliptic_k(m)
rule jacobi_epsilon(u_ + elliptic_k(m_), m_) := elliptic_e(m) + jacobi_epsilon(u, m) - m*jacobi_cd(u, m)*jacobi_sn(u, m)
rule jacobi_epsilon(u_ + 1im*elliptic_k(1 - m_), m_) := im*(elliptic_k(1 - m) - elliptic_e(1 - m)) + jacobi_cs(u, m)*jacobi_dn(u, m) + jacobi_epsilon(u, m)
rule jacobi_epsilon(u_ + 2im*elliptic_k(1 - m_), m_) := 2*im*(elliptic_k(1 - m) - elliptic_e(1 - m)) + jacobi_epsilon(u, m)
rule jacobi_epsilon(u_ + 2*elliptic_k(m_) + 2im*elliptic_k(1 - m_), m_) := 2*elliptic_e(m) + 2*im*(elliptic_k(1 - m) - elliptic_e(1 - m)) + jacobi_epsilon(u, m)
# Neville theta functions: theta_n vanishes at i K'; at m = 1 they are 1, 1, cosh, sinh (DLMF 20.9, 22.2)
rule neville_theta_n(1im*elliptic_k(1 - m_), m_) := 0
# at the quarter periods K, i K', K + i K' (z = pi/2, pi tau/2, pi/2 + pi tau/2 in theta_j(z, q)/theta_j(0, q), the
# half-period shifts of DLMF 20.2.11-14 and theta_2^2/theta_3^2 = m^(1/2), theta_4^2/theta_3^2 = (1 - m)^(1/2), 20.9.1;
# q the nome); checked numerically for real m < 0, 0 < m < 1, m > 1 and complex m
rule neville_theta_c(elliptic_k(m_), m_) := 0
rule neville_theta_d(elliptic_k(m_), m_) := (1 - m)^(1/4)
rule neville_theta_n(elliptic_k(m_), m_) := (1 - m)^(-1/4)
rule neville_theta_c(1im*elliptic_k(1 - m_), m_) := 1/(m^(1/4)*elliptic_nome_q(m)^(1/4))
rule neville_theta_d(1im*elliptic_k(1 - m_), m_) := m^(1/4)/elliptic_nome_q(m)^(1/4)
rule neville_theta_c(elliptic_k(m_) + 1im*elliptic_k(1 - m_), m_) := -im*(1 - m)^(1/4)/(m^(1/4)*elliptic_nome_q(m)^(1/4))
rule neville_theta_d(elliptic_k(m_) + 1im*elliptic_k(1 - m_), m_) := 0
rule neville_theta_n(elliptic_k(m_) + 1im*elliptic_k(1 - m_), m_) := m^(1/4)/((1 - m)^(1/4)*elliptic_nome_q(m)^(1/4))
rule neville_theta_c(u_, 1) := 1
rule neville_theta_d(u_, 1) := 1
rule neville_theta_n(u_, 1) := cosh(u)
rule neville_theta_s(u_, 1) := sinh(u)
rule elliptic_theta(1, z_, q_) := 0 when is_integer(z/pi)
rule modular_lambda(1im) := 1/2
rule elliptic_nome_q(1/2) := exp(-pi)
rule inverse_elliptic_nome_q(q_) := 1/2 when q === exp(-pi)

# Weierstrass functions at the half periods and their (quasi-)periodicity (DLMF 23.2.10-20)
rule weierstrass_p(weierstrass_half_period_w1(g_), g_) := weierstrass_e1(g)
rule weierstrass_p(weierstrass_half_period_w2(g_), g_) := weierstrass_e2(g)
rule weierstrass_p(weierstrass_half_period_w3(g_), g_) := weierstrass_e3(g)
rule weierstrass_zeta(weierstrass_half_period_w1(g_), g_) := weierstrass_eta1(g)
rule weierstrass_zeta(weierstrass_half_period_w2(g_), g_) := weierstrass_eta2(g)
rule weierstrass_zeta(weierstrass_half_period_w3(g_), g_) := weierstrass_eta3(g)
rule weierstrass_sigma(2*weierstrass_half_period_w1(g_), g_) := 0
rule weierstrass_sigma(2*weierstrass_half_period_w2(g_), g_) := 0
rule weierstrass_sigma(2*weierstrass_half_period_w3(g_), g_) := 0
rule weierstrass_zeta(z_ + 2*weierstrass_half_period_w1(g_), g_) := weierstrass_zeta(z, g) + 2*weierstrass_eta1(g)
rule weierstrass_zeta(z_ + 2*weierstrass_half_period_w2(g_), g_) := weierstrass_zeta(z, g) + 2*weierstrass_eta2(g)
rule weierstrass_zeta(z_ + 2*weierstrass_half_period_w3(g_), g_) := weierstrass_zeta(z, g) + 2*weierstrass_eta3(g)
rule weierstrass_p(z_ + 2*weierstrass_half_period_w1(g_), g_) := weierstrass_p(z, g)
rule weierstrass_p(z_ + 2*weierstrass_half_period_w2(g_), g_) := weierstrass_p(z, g)
rule weierstrass_p(z_ + 2*weierstrass_half_period_w3(g_), g_) := weierstrass_p(z, g)
# the degenerate curve y^2 = x^3: x = 1/u^2, y = -1/u^3; the pole at u = 0
rule elliptic_exp(u_, [0, 0]) := [u^-2, -u^-3]
rule elliptic_exp_prime(u_, [0, 0]) := [-2*u^-3, 3*u^-4]
rule elliptic_exp(0, g_) := [complex_infinity, complex_infinity] when is_list(g)
rule elliptic_exp_prime(0, g_) := [complex_infinity, complex_infinity] when is_list(g)

# orthogonal polynomials of symbolic or fractional degree (DLMF 18.5.1, 18.7.4, 18.7.9-10, 18.6.1)
rule chebyshev_t(nu_, 0) := cos(nu*pi/2)
rule chebyshev_u(nu_, 0) := cos(nu*pi/2)
rule chebyshev_t(nu_, x_) := cos(nu*acos(x)) when not is_numeric(x) and is_numeric(nu) and not is_integer(nu) and im_part(nu) == 0
rule chebyshev_u(nu_, x_) := sin((nu + 1)*acos(x))/(sqrt(1 - x)*sqrt(1 + x)) when not is_numeric(x) and is_numeric(nu) and not is_integer(nu) and im_part(nu) == 0
rule gegenbauer_c(nu_, 1/2, x_) := legendre_p(nu, x)
rule gegenbauer_c(nu_, 1, x_) := chebyshev_u(nu, x)
rule gegenbauer_c(nu_, x_) := 2/nu*chebyshev_t(nu, x) when not (nu === 0)
rule hermite_h(nu_, 0) := 2^nu*sqrt(pi)/gamma((1 - nu)/2)
rule legendre_p(nu_, 1) := 1

# the Lerch transcendent at s = 0 and a = 0, 1, 1/2 (DLMF 25.14.1-3)
rule lerch_phi(z_, 0, a_) := 1/(1 - z)
rule lerch_phi(z_, s_, 0) := polylog(s, z) when not (s === 0)
rule lerch_phi(z_, s_, 1) := polylog(s, z)/z when not (s === 0)
rule lerch_phi(-1, s_, 1/2) := 2^(-s)*(zeta(s, 1/4) - zeta(s, 3/4)) when not (s === 0)
rule polygamma(-1, z_) := log_gamma(z)
rule polylog(s_, -1) := -(1 - 2^(1 - s))*zeta(s)
rule polylog(3, 1/2) := 7/8*zeta(3) - pi^2*log(2)/12 + log(2)^3/6

# Heun functions that reduce to the Gauss function (DLMF 31.6); the confluent form with delta = 0 to Kummer's
rule heun_g(1, q_, al_, be_, ga_, de_, z_) := hypergeometric_2f1(al, be, ga, z) when expand(q - al*be) === 0
rule heun_g(a_, q_, al_, be_, ga_, de_, z_) := hypergeometric_2f1(al, be, ga, z) when expand(q - a*al*be) === 0 and expand(de - 1 - al - be + ga) === 0
rule heun_c(q_, q_, ga_, 0, -1, z_) := hypergeometric_1f1(-q, ga, z)

# hypergeometric functions in elementary and other special functions (DLMF 15.4, 15.4.20 Gauss, 13.6)
rule hypergeometric_2f1(1/2, 1/2, 1, z_) := 2*elliptic_k(z)/pi
rule hypergeometric_2f1(1, 1/2, 3/2, y_^2) := atanh(y)/y
rule hypergeometric_2f1(1, 1/2, 3/2, z_) := atanh(sqrt(z))/sqrt(z)
rule hypergeometric_2f1(a_, b_, c_, 1) := gamma(c)*gamma(c - a - b)/(gamma(c - a)*gamma(c - b)) when is_numeric(c - a - b) and re(c - a - b) > 0
rule hypergeometric_2f1(a_, b_, c_, z_) := complex_infinity when is_integer(c) and c <= 0 and not (is_integer(a) and a <= 0) and not (is_integer(b) and b <= 0)
rule hypergeometric_1f1(1/2, 1, x_) := exp(x/2)*bessel_i(0, x/2)
rule hypergeometric_1f1_regularized(1/2, 1, x_) := exp(x/2)*bessel_i(0, x/2)
# a = n + 1/2: 1F1(a; 2a; x) = n! e^(x/2) (x/4)^-n I_n(x/2) (DLMF 13.6.9), and by the derivative (13.3.15) with
# I_n' = I_(n+1) + (n/t) I_n, 1F1(a; 2a - 1; x) = (n - 1)! e^(x/2) (x/4)^(1 - n) (I_(n-1)(x/2) + I_n(x/2)) (n <= 20)
rule hypergeometric_1f1(a_, b_, x_) := factorial(a - 1/2)*exp(x/2)*(x/4)^(1/2 - a)*bessel_i(a - 1/2, x/2) when is_integer(a - 1/2) and a >= 3/2 and a <= 41/2 and b === 2*a and not (is_numeric(x) and x == 0)
rule hypergeometric_1f1(a_, b_, x_) := factorial(a - 3/2)*exp(x/2)*(x/4)^(3/2 - a)*(bessel_i(a - 3/2, x/2) + bessel_i(a - 1/2, x/2)) when is_integer(a - 1/2) and a >= 3/2 and a <= 41/2 and b === 2*a - 1 and not (is_numeric(x) and x == 0)
rule hypergeometric_1f1(1, 1/2, x_) := 1 + exp(x)*sqrt(pi)*sqrt(x)*erf(sqrt(x))
rule hypergeometric_1f1(a_, b_, x_) := exp(x)*ds_terminating([b - a], [b], -x) when is_integer(a - b) and a - b > 0 and is_integer(b) and b > 0 and a - b <= 60
# (not at x = 0: x^(1 - b) gamma(b - 1, x) is 0 times an infinity there, U(1, b, 0) = 1/(1 - b) for b < 1)
rule hypergeometric_u(1, b_, x_) := exp(x)*x^(1 - b)*gamma(b - 1, x) when not (is_numeric(x) and x == 0)
rule hypergeometric_u(1/2, 1, x_) := exp(x/2)*bessel_k(0, x/2)/sqrt(pi)
# a terminating series: an upper parameter -N (N <= 60, no lower parameter -M with M < N)
fn ds_top(as) = min(map(a => if is_integer(a) and a <= 0 then -a else inf, as))
fn ds_terminating(as, bs, z) = {
  let nn = ds_top(as)
  ds_sum(k => fold((p, a) => p*pochhammer(a, k), 1, as)/fold((p, b) => p*pochhammer(b, k), 1, bs)*z^k/factorial(k), 0, nn)
}
fn ds_terminates(as, bs) = {
  let nn = ds_top(as)
  is_integer(nn) and nn <= 60 and every(bs, b => not (is_integer(b) and b <= 0 and -b < nn))
}
rule hypergeometric_pfq(as_, bs_, z_) := ds_terminating(as, bs, z) when is_list(as) and is_list(bs) and length(as) >= 1 and ds_terminates(as, bs)

# --- numerical values the natives leave (complex or generalized arguments) ---
#
# Machine arguments only (an inexact number among them), by classical reductions to functions that evaluate there.

fn ds_inexact(xs) = every(xs, x => is_numeric(x)) and some(xs, x => is_machine_number(x)) and every(xs, x => is_exact(x) or is_machine_number(x))

# Ferrers Q from P off the integers (DLMF 14.9.10 at x in (-1, 1)): Q = pi/(2 sin((nu + mu) pi)) (cos((nu + mu) pi) P(x) - P(-x));
# also for arbitrary-precision arguments (balls: the combination keeps the enclosures of P), with exact degree and order
fn ds_approx(xs) = every(xs, x => is_numeric(x)) and some(xs, x => not is_exact(x)) and every(xs, x => is_exact(x) or is_machine_number(x) or im_part(x) == 0)
fn ds_sin_pi_nonzero(t) = not is_integer(t) and (is_exact(t) or (sin(t*pi) != 0) === true)
fn ds_ferrers_q_ok(nu, mu, x) = (ds_inexact([nu, mu, x]) or ds_approx([nu, mu, x])) and im_part(x) == 0 and -1 < x and x < 1 and ds_sin_pi_nonzero(nu + mu)
rule legendre_q(nu_, x_) := {
  pi/(2*sin(nu*pi))*(cos(nu*pi)*legendre_p(nu, x) - legendre_p(nu, -x))
} when ds_ferrers_q_ok(nu, 0, x)
rule legendre_q(nu_, mu_, x_) := {
  pi/(2*sin((nu + mu)*pi))*(cos((nu + mu)*pi)*legendre_p(nu, mu, x) - legendre_p(nu, mu, -x))
} when ds_ferrers_q_ok(nu, mu, x)
# the same combination off [-1, 1]: it is an identity of the functions analytic on the plane cut along (-inf, -1] and
# [1, inf), so it holds at every non-real x, and for order 0 on the cuts too, where P_nu(x) is real for x > 1 and
# P_nu(-x) is the 2F1 on its own cut
fn ds_q_off_ok(nu, mu, x) = (ds_inexact([nu, mu, x]) or ds_approx([nu, mu, x])) and ds_sin_pi_nonzero(nu + mu) and (im_part(x) != 0 or (mu == 0 and abs(x) > 1))
rule legendre_q(nu_, x_) := {
  pi/(2*sin(nu*pi))*(cos(nu*pi)*legendre_p(nu, x) - legendre_p(nu, -x))
} when ds_q_off_ok(nu, 0, x)
rule legendre_q(nu_, mu_, x_) := {
  pi/(2*sin((nu + mu)*pi))*(cos((nu + mu)*pi)*legendre_p(nu, mu, x) - legendre_p(nu, mu, -x))
} when ds_q_off_ok(nu, mu, x)
# where nu + mu is an integer and mu is not, by P of orders +-mu (DLMF 14.9.2):
# Q = pi/(2 sin(mu pi)) (cos(mu pi) P^mu(x) - Gamma(nu + mu + 1)/Gamma(nu - mu + 1) P^-mu(x))
# (also for balls nu + mu that hold an integer, where sin((nu + mu) pi) has no sign)
fn ds_ferrers_q2_ok(nu, mu, x) = (ds_inexact([nu, mu, x]) or ds_approx([nu, mu, x])) and im_part(x) == 0 and -1 < x and x < 1 and not ds_sin_pi_nonzero(nu + mu) and ds_sin_pi_nonzero(mu)
rule legendre_q(nu_, mu_, x_) := {
  let v = pi/(2*sin(mu*pi))*(cos(mu*pi)*legendre_p(nu, mu, x) - gamma(nu + mu + 1)/gamma(nu - mu + 1)*legendre_p(nu, -mu, x))
  # exact complex parameters leave gamma(nu - mu + 1) exact: the value at the machine arguments' precision
  if some([nu, mu, x], t => is_machine_number(t)) then n(v) else v
} when ds_ferrers_q2_ok(nu, mu, x)
# Ferrers Q of integer degree n >= 0 and order 1 <= m <= 20 (where the form above divides by sin(4 pi) = 0):
# Q_n^m(x) = (-1)^m (1 - x^2)^(m/2) d^m Q_n(x)/dx^m (DLMF 14.6.2), Q_n in logarithms; symbolic x or a real x in (-1, 1)
fn ds_ferrers_qm(n, m, x) = (-1)^m*(1 - x^2)^(m/2)*rewrite(d(legendre_q(n, ds_qm_t), [ds_qm_t, m]), ds_qm_t -> x)
rule legendre_q(nu_, mu_, x_) := ds_ferrers_qm(nu, mu, x) when is_integer(nu) and nu >= 0 and is_integer(mu) and mu >= 1 and mu <= 20 and (not is_numeric(x) or (im_part(x) == 0 and -1 < x and x < 1))

# Ferrers functions of order 1/2 and -1/2 at a degree nu with nu + 1/2 = +-n an integer (DLMF 14.5.11, 14.5.12 at
# x = cos t, with cos(n t) = T_n(x) and sin(n t) = sin(t) U_(n-1)(x); both even in nu + 1/2); symbolic x, or an exact
# real x in (-1, 1), where these are the Ferrers functions themselves
fn ds_half_order_x(x) = not is_numeric(x) or (is_exact(x) and im_part(x) == 0 and -1 < x and x < 1)
rule legendre_p(nu_, 1/2, x_) := sqrt(2/pi)*(1 - x^2)^(-1/4)*chebyshev_t(abs(nu + 1/2), x) when is_rational(nu) and is_integer(nu + 1/2) and ds_half_order_x(x)
rule legendre_p(nu_, -1/2, x_) := {
  let n = abs(nu + 1/2)
  if n == 0 then sqrt(2/pi)*(1 - x^2)^(-1/4)*acos(x) else sqrt(2/pi)*(1 - x^2)^(1/4)*chebyshev_u(n - 1, x)/n
} when is_rational(nu) and is_integer(nu + 1/2) and ds_half_order_x(x)

# Jacobi functions of a degree that is not an integer (DLMF 15.9.1 read as the definition off the integers):
# Gamma(nu + a + 1)/(Gamma(nu + 1) Gamma(a + 1)) 2F1(-nu, nu + a + b + 1; a + 1; (1 - x)/2), used when that 2F1 has a
# closed form
fn ds_jacobi_f(nu, al, be, x) = hypergeometric_2f1(-nu, nu + al + be + 1, al + 1, (1 - x)/2)
rule jacobi_p(nu_, al_, be_, x_) := {
  gamma(nu + al + 1)/(gamma(nu + 1)*gamma(al + 1))*ds_jacobi_f(nu, al, be, x)
} when is_rational(nu) and not is_integer(nu) and is_rational(al) and is_rational(be) and not (is_integer(al) and al < 0) and not (is_integer(nu + al) and nu + al < 0) and not is_numeric(x) and free_of(ds_jacobi_f(nu, al, be, x), hypergeometric_2f1)

# incomplete forms of the inverse error, gamma and beta functions: solve the generalized function for its second
# endpoint (erf(z) - erf(x0) = s, Q(a, z0) - Q(a, z) = s, I_z(a, b) - I_z0(a, b) = s)
rule inverse_erf(x0_, s_) := inverse_erf(s + erf(x0)) when ds_inexact([x0, s])
rule inverse_gamma_regularized(a_, z0_, s_) := inverse_gamma_regularized(a, gamma_regularized(a, z0) - s) when ds_inexact([a, z0, s])
rule inverse_beta_regularized(z0_, s_, a_, b_) := inverse_beta_regularized(s + beta_regularized(z0, a, b), a, b) when ds_inexact([z0, s, a, b])

# the inverse of sn at complex arguments by Carlson's form F(asin v | m) = v R_F(1 - v^2, 1 - m v^2, 1) (DLMF 19.25.5)
rule inverse_jacobi_sn(v_, m_) := v*carlson_rf(1 - v^2, 1 - m*v^2, 1) when ds_inexact([v, m]) and not (im_part(v) == 0 and im_part(m) == 0)

# the polylogarithm of a non-real order where its own series and log expansion may stop (|z| >= 1/2; the native
# declines in parts of 0.6 < |z| < 1 and beyond): Jonquiere's relation
# Li_s(z) = Gamma(1 - s)/(2 pi)^(1 - s) (i^(1 - s) zeta(1 - s, 1/2 + log(-z)/(2 pi i)) + i^(s - 1) zeta(1 - s, 1/2 - log(-z)/(2 pi i)))
# (DLMF 25.12.13), an identity of functions analytic in z off [0, inf) (both Hurwitz parameters have real parts in
# (0, 1) there) and entire in s off the positive integers; machine numbers off the real ray z >= 0
fn ds_jonquiere(s, z) = gamma(1 - s)/(2*pi)^(1 - s)*(im^(1 - s)*zeta(1 - s, 1/2 + log(-z)/(2*pi*im)) + im^(s - 1)*zeta(1 - s, 1/2 - log(-z)/(2*pi*im)))
rule polylog(s_, z_) := ds_jonquiere(n(s), n(z)) when ds_inexact([s, z]) and im_part(s) != 0 and (im_part(z) != 0 or re(z) < 0) and abs(z) >= 0.5

# the falling factorial with step h through gamma (machine complex arguments)
rule factorial_power(x_, k_, h_) := h^k*gamma(x/h + 1)/gamma(x/h - k + 1) when ds_inexact([x, k, h])

# a convergent pFq (p <= q, or p = q + 1 and |z| < 1) summed term by term, regularized: 1/gamma(b + k) = 0 at the
# poles, so the sum starts at the first k past them (machine numbers)
fn ds_rgamma(x) = if is_integer(x) and x <= 0 then 0 else 1/gamma(x)
fn ds_pfq_sum(as, bs, z, reg) = {
  let k0 = if reg then max(append(map(b => if is_integer(b) and b <= 0 then 1 - b else 0, bs), 0)) else 0
  let t0 = fold((p, a) => p*pochhammer(a, k0), 1, as)*z^k0/factorial(k0)*fold((p, b) => p*(if reg then ds_rgamma(b + k0) else 1/pochhammer(b, k0)), 1, bs)
  # state [k, term, sum, done]; at most 600 terms
  fold((st, it) => ds_pfq_step(as, bs, z, st), [k0, n(t0), n(t0), false], range(600))[3]
}
fn ds_pfq_step(as, bs, z, st) = {
  if st[4] then st
  else {
    let k = st[1]
    let t2 = st[2]*z/(k + 1)*fold((p, a) => p*(a + k), 1, as)/fold((p, b) => p*(b + k), 1, bs)
    [k + 1, t2, st[3] + t2, t2 == 0 or (k > 2 and abs(t2) <= 10^-17*abs(st[3]))]
  }
}
fn ds_pfq_ok(as, bs, z) = is_list(as) and is_list(bs) and ds_inexact(join(as, bs, [z])) and (length(as) <= length(bs) or (length(as) == length(bs) + 1 and abs(z) < 0.9))
rule hypergeometric_pfq_regularized(as_, bs_, z_) := ds_pfq_sum(as, bs, z, true) when ds_pfq_ok(as, bs, z)
rule hypergeometric_pfq(as_, bs_, z_) := ds_pfq_sum(as, bs, z, false) when ds_pfq_ok(as, bs, z) and every(bs, b => not (is_integer(b) and b <= 0))

# prime_zeta_p(s): the sum of p^-s over the primes p, for inexact s with re(s) > 1: the series
# sum over k of moebius_mu(k)/k log(zeta(k s)) (Froberg 1968), summed until the terms are below the precision of s
rule prime_zeta_p(s_) := {
  let digits = if is_machine_number(s) then 17 else precision(s)
  let top = ceiling((digits + 3)*3.33/re(s)) + 2
  sum(map(k => if moebius_mu(k) == 0 then 0 else moebius_mu(k)/k*log(zeta(k*s)), range(top)))
} when is_inexact(s) and is_real(re(s)) and re(s) > 1
# a real s in [1/5, 1): the same series; the terms with k s > 1 are real, and each log(zeta(k s)) with k s < 1 (zeta is
# negative on (0, 1)) is taken on the principal branch, log|zeta(k s)| + i pi (below 1/2 too, so that
# P(1/4) = ... + i pi/6 and P(4/11) = ... + i pi/2); the terms fall like 2^(-k s)/k, so (digits + 3) 3.33/s of them
rule prime_zeta_p(s_) := {
  let digits = if is_machine_number(s) then 17 else precision(s)
  let top = ceiling((digits + 3)*3.33/s) + 2
  sum(map(k => if moebius_mu(k) == 0 then 0 else moebius_mu(k)/k*log(zeta(k*s)), range(top)))
} when is_inexact(s) and is_real(s) and s >= 1/5 and s < 1
# s = 1/k for a squarefree k: the term moebius_mu(k)/k log(zeta(k s)) of the series has the pole of zeta at 1 inside its
# logarithm while every other term is finite (zeta has no zero on the positive real axis), so |P(s)| -> inf from every
# direction: a logarithmic branch point, not a pole, whose value is complex infinity
rule prime_zeta_p(s_) := complex_infinity when is_rational(s) and s > 0 and numerator(s) == 1 and denominator(s) <= 10^9 and moebius_mu(denominator(s)) != 0
# around(c, u): the value at c with the uncertainty |P'(c)| u propagated to first order, where P'(s) = sum over k of
# moebius_mu(k) zeta'(k s)/zeta(k s) (the derivative of the series above term by term)
fn pz_slope(s) = {
  let top = ceiling(60*3.33/re(s)) + 2
  sum(map(k => if moebius_mu(k) == 0 then 0 else moebius_mu(k)*rewrite(d(zeta(pz_t), pz_t), pz_t -> k*s)/zeta(k*s), range(top)))
}
rule prime_zeta_p(a_) := around(prime_zeta_p(a[1]), abs(pz_slope(a[1]))*a[2]) when not is_atom(a) and head(a) === around and is_real(a[1]) and a[1] > 1 and is_inexact(a[1])
# prime_zeta_p over a list or matrix of arguments
rule prime_zeta_p(xs_) := map(prime_zeta_p, xs) when is_list(xs)

# --- Fibonacci and Lucas functions of any order: derivatives in x ---
# With r = sqrt(x^2 + 4) and a = (x + r)/2, a' = a/r, so Binet's forms F_n(x) = (a^n - cos(pi n) a^-n)/r and
# L_n(x) = a^n + cos(pi n) a^-n (Koshy, Fibonacci and Lucas Numbers with Applications, 2001, ch. 37) give
# L_n' = n F_n and F_n' = (n L_n - x F_n)/(x^2 + 4), for every order n (for natural n the polynomial identities).
rule derivative(0, 1)(lucas_l)(n_, x_) := n*fibonacci(n, x)
rule derivative(0, 1)(fibonacci)(n_, x_) := (n*lucas_l(n, x) - x*fibonacci(n, x))/(x^2 + 4)
# where the Taylor coefficients fail (the branch points x = +-2i of sqrt(x^2 + 4), or infinity), series expands
# Binet's forms themselves
fn ds_binet(n, u, s) = {
  let r = sqrt(u^2 + 4)
  if s === 1 then 2^-n*(u + r)^n + cos(n*pi)*2^n*(u + r)^-n
  else (2^-n*(u + r)^n - cos(n*pi)*2^n*(u + r)^-n)/r
}
rule ds_at(fibonacci(n_, u_), 2, u0_, k_) := ds_binet(n, u, 0) when not is_integer(n)
rule ds_at(lucas_l(n_, u_), 2, u0_, k_) := ds_binet(n, u, 1) when not is_integer(n)
