#! mathcas 2
# Calculus library: functions written in MathCAS itself, evaluated after the prelude and the
# language library, defined in every session. Helper functions are prefixed `calc_` and not part of the interface.
# Methods and literature: crates/mc-calculus/NOTES.md, section "Round 28: the calculus library".

# calc_real(x): whether x has a real numerical value (inf excluded)
fn calc_real(x) = is_real(n(x)) === true

# calc_range(r): whether r is a pair [min, max] of a wave's range
fn calc_range(r) = is_list(r) and length(r) == 2

# calc_total(xs): the sum of the elements of the list xs (0 for the empty list)
fn calc_total(xs) = fold((a, b) => a + b, 0, xs)

# --- periodic waves (period 1): sawtooth_wave, square_wave, triangle_wave ---

# sawtooth_wave(x): x - floor(x), rising from 0 to 1 on each unit interval; sawtooth_wave([a, b], x) the same
# wave between a and b; element-wise on lists
rule sawtooth_wave(x_) := map(sawtooth_wave, x) when is_list(x)
rule sawtooth_wave(x_) := x - floor(x) when calc_real(x)
rule sawtooth_wave(r_, x_) := map(t => sawtooth_wave(r, t), x) when calc_range(r) and is_list(x)
rule sawtooth_wave(r_, x_) := r[1] + (r[2] - r[1])*sawtooth_wave(x) when calc_range(r) and calc_real(x)

# square_wave(x): 1 on the first half of each unit interval, -1 on the second; square_wave([a, b], x) b and a
rule square_wave(x_) := map(square_wave, x) when is_list(x)
rule square_wave(x_) := (if x - floor(x) < 1/2 then 1 else -1) when calc_real(x)
rule square_wave(r_, x_) := map(t => square_wave(r, t), x) when calc_range(r) and is_list(x)
rule square_wave(r_, x_) := (if x - floor(x) < 1/2 then r[2] else r[1]) when calc_range(r) and calc_real(x)

# triangle_wave(x): the triangle wave 0 at 0, 1 at 1/4, 0 at 1/2, -1 at 3/4; triangle_wave([a, b], x) between a
# and b
rule triangle_wave(x_) := map(triangle_wave, x) when is_list(x)
rule triangle_wave(x_) := 1 - 4*abs(sawtooth_wave(x + 1/4) - 1/2) when calc_real(x)
rule triangle_wave(r_, x_) := map(t => triangle_wave(r, t), x) when calc_range(r) and is_list(x)
rule triangle_wave(r_, x_) := (r[1] + r[2])/2 + (r[2] - r[1])/2*triangle_wave(x) when calc_range(r) and calc_real(x)

# the waves between a and b as affine images of the unit waves (identities for every x): the function properties
# take them in that form (the coefficient's sign from the assumptions)
fn calc_wave_aff(f) = rewrite(f, [
  sawtooth_wave(r_, u_) :> (if calc_range(r) then r[1] + (r[2] - r[1])*sawtooth_wave(u) else sawtooth_wave(r, u)),
  square_wave(r_, u_) :> (if calc_range(r) then (r[1] + r[2])/2 + (r[2] - r[1])/2*square_wave(u) else square_wave(r, u)),
  triangle_wave(r_, u_) :> (if calc_range(r) then (r[1] + r[2])/2 + (r[2] - r[1])/2*triangle_wave(u) else triangle_wave(r, u))])
fn calc_ranged_wave(f) = not (calc_wave_aff(f) === f)
rule function_injective(f_, rest___) := apply(function_injective, join([calc_wave_aff(f)], rest)) when calc_ranged_wave(f)
rule function_convexity(f_, rest___) := apply(function_convexity, join([calc_wave_aff(f)], rest)) when calc_ranged_wave(f)
rule function_monotonicity(f_, rest___) := apply(function_monotonicity, join([calc_wave_aff(f)], rest)) when calc_ranged_wave(f)
rule function_sign(f_, rest___) := apply(function_sign, join([calc_wave_aff(f)], rest)) when calc_ranged_wave(f)

# a definite integral of a wave of x over real bounds: the integral of its piecewise expansion (floor of linear
# functions of x), which integrate takes cell by cell
fn calc_wave_call(w, x) = not is_atom(w) and (head(w) === sawtooth_wave or head(w) === square_wave or head(w) === triangle_wave) and not free_of(w, x)
rule integrate(w_, s_) := integrate(piecewise_expand(w), s) when is_list(s) and length(s) == 3 and is_symbol(s[1]) and calc_wave_call(w, s[1]) and calc_real(s[2]) and calc_real(s[3]) and free_of(piecewise_expand(w), head(w))

# --- Heaviside shapes: heaviside_lambda, heaviside_pi ---

# heaviside_lambda(x): the triangle 1 - |x| for |x| < 1, else 0; heaviside_lambda(x, y, ...) the product
rule heaviside_lambda(x_) := map(heaviside_lambda, x) when is_list(x)
rule heaviside_lambda(xs__) := {
  let ys = xs
  let vals = map(x => if abs(x) < 1 then 1 - abs(x) else 0, ys)
  if some(ys, is_inexact) and every(vals, v => v == 0) then 0.0 else product(vals)
} when every(xs, calc_real)

# heaviside_pi(x): 1 for |x| < 1/2, 0 for |x| > 1/2, undefined at |x| = 1/2; heaviside_pi(x, y, ...) the product
rule heaviside_pi(x_) := map(heaviside_pi, x) when is_list(x)
rule heaviside_pi(x_) := heaviside_pi(-x) when calc_real(x) and x === -1/2
rule heaviside_pi(xs__) := {
  let ys = xs
  if some(ys, x => abs(x) > 1/2) then 0 else if every(ys, x => abs(x) < 1/2) then 1 else apply(heaviside_pi, select(ys, x => abs(x) == 1/2))
} when every(xs, calc_real) and some(xs, x => abs(x) != 1/2)

# --- harmonic-type numbers: alternating_harmonic_number, alternating_factorial, hyper_harmonic_number,
# multiple_harmonic_number ---

# calc_count(m): whether m is an exact non-negative integer
fn calc_count(m) = is_integer(m) and m >= 0

# calc_inexact(xs): whether every element of xs is a number and one of them is inexact
fn calc_inexact(xs) = every(xs, is_numeric) and some(xs, is_inexact)

# alternating_harmonic_number(m), (m, r), (m, r, s): the sum of (-1)^(k + 1) s^k/k^r for k = 1, ..., m (r = 1 and
# s = 1 by default); m = inf gives log(2), the Dirichlet eta function (1 - 2^(1 - r)) zeta(r) and -polylog(r, -s);
# inexact m by the Hurwitz zeta (digamma, Lerch) form of the tail
rule alternating_harmonic_number(m_) := alternating_harmonic_number(m, 1)
rule alternating_harmonic_number(m_, r_) := calc_total(map(k => (-1)^(k + 1)/k^r, range(m))) when calc_count(m)
rule alternating_harmonic_number(m_, r_) := (if r === 1 then log(2) else (1 - 2^(1 - r))*zeta(r)) when m === inf
rule alternating_harmonic_number(m_, r_) := {
  if r == 1 then log(2) - (-1)^m/2*(digamma((m + 2)/2) - digamma((m + 1)/2))
  else (1 - 2^(1 - r))*zeta(r) - (-1)^m*2^(-r)*(zeta(r, (m + 1)/2) - zeta(r, (m + 2)/2))
} when calc_inexact([m, r])
rule alternating_harmonic_number(m_, r_, s_) := calc_total(map(k => (-1)^(k + 1)*s^k/k^r, range(m))) when calc_count(m)
rule alternating_harmonic_number(m_, r_, s_) := -polylog(r, -s) when m === inf
rule alternating_harmonic_number(m_, r_, s_) := -polylog(r, -s) - (-1)^m*s^(m + 1)*lerch_phi(-s, r, m + 1) when calc_inexact([m, r, s])

# alternating_factorial(m): m! - (m - 1)! + ... +- 1!, the sum of (-1)^(m - k) k! for k = 1, ..., m; at inexact m
# the continuation e gamma(m + 2) gamma(-m - 1, 1) - cos(pi m) (1 + e exp_integral_ei(-1))
rule alternating_factorial(m_) := map(alternating_factorial, m) when is_list(m)
# exact m by a(k) = k! - a(k - 1) with a running factorial: one product and one difference a step (each factorial
# afresh took 3.7 s at m = 10^4); Re m < -1 by Gamma(m + 2) Gamma(-m - 1) = pi/sin(pi m) (DLMF 5.5.3), so that
# e gamma(m + 2) gamma(-m - 1, 1) = e pi Q(-m - 1, 1)/sin(pi m): the product of an underflowing and an overflowing
# factor gave indeterminate at m = -1000.5 (the value is -e pi Q(999.5, 1) = -8.5397...)
rule alternating_factorial(m_) := {
  var a = 0
  var f = 1
  for k in range(m) {
    f = f*k
    a = f - a
  }
  a
} when calc_count(m)
rule alternating_factorial(m_) := e*pi*gamma_regularized(-m - 1, 1)/sin(pi*m) - cos(pi*m)*(1 + e*exp_integral_ei(-1)) when calc_inexact([m]) and re(m) < -1
rule alternating_factorial(m_) := e*gamma(m + 2)*gamma(-m - 1, 1) - cos(pi*m)*(1 + e*exp_integral_ei(-1)) when calc_inexact([m])

# hyper_harmonic_number(r, m): the hyperharmonic number, H(0, m) = 1/m and H(r, m) = H(r - 1, 1) + ... +
# H(r - 1, m), which is binomial(m + r - 1, r - 1) (H(m + r - 1) - H(r - 1)) and at inexact arguments
# gamma(m + r)/(gamma(m + 1) gamma(r)) (digamma(m + r) - digamma(r)); hyper_harmonic_number(r, m, s): from
# H(0, m) = 1/m^s
rule hyper_harmonic_number(r_, m_) := (if r == 0 then 1/m else binomial(m + r - 1, r - 1)*(harmonic_number(m + r - 1) - harmonic_number(r - 1))) when calc_count(r) and calc_count(m) and not (r == 0 and m == 0)
rule hyper_harmonic_number(r_, m_) := gamma(m + r)/(gamma(m + 1)*gamma(r))*(digamma(m + r) - digamma(r)) when calc_inexact([r, m])
rule hyper_harmonic_number(r_, m_, s_) := harmonic_number(m, s) when r === 1
rule hyper_harmonic_number(r_, m_, s_) := {
  var c = map(k => 1/k^s, range(m))
  for j in range(r) {
    c = accumulate(c)
  }
  if m == 0 then 0 else last(c)
} when calc_count(r) and r >= 2 and calc_count(m)

# multiple_harmonic_number(m, [r1, ..., rk]), (m, [r1, ..., rk], [s1, ..., sk]): the sum over m >= n1 > n2 > ... >
# nk >= 1 of the products s1^n1/n1^r1 ... sk^nk/nk^rk (all si = 1 by default), by running sums from the innermost
# index outward
rule multiple_harmonic_number(m_, rs_) := multiple_harmonic_number(m, rs, constant_array(1, length(rs))) when is_list(rs)
rule multiple_harmonic_number(m_, rs_, ss_) := {
  var c = constant_array(1, m + 1)
  for j in reverse(range(length(rs))) {
    let a = map(t => ss[j]^t/t^rs[j]*c[t], range(m))
    c = join([0], accumulate(a))
  }
  c[m + 1]
} when calc_count(m) and is_list(rs) and is_list(ss) and length(rs) == length(ss) and length(rs) > 0
rule multiple_harmonic_number(m_, rs_, ss_) := harmonic_number(m, rs[1]) when is_list(rs) and length(rs) == 1 and ss === [1]
# depth 2 at a symbolic m, in harmonic numbers H(r) = harmonic_number(m, r): the quasi-shuffle (stuffle) product
# H(a) H(b) = S(a, b) + S(b, a) + H(a + b) gives S(a, a) = (H(a)^2 - H(2 a))/2; and S(a, 0) = sum (n - 1)/n^a =
# H(a - 1) - H(a), S(0, b) = sum (m - n)/n^b = m H(b) - H(b - 1) (Hoffman, Multiple harmonic series, Pacific J.
# Math. 152 (1992), section 2)
rule multiple_harmonic_number(m_, rs_, ss_) := {
  let a = rs[1]
  let b2 = rs[2]
  let hn = r => if r === 1 then harmonic_number(m) else harmonic_number(m, r)
  if a === b2 then (hn(a)^2 - hn(2*a))/2
  else if b2 === 0 then hn(a - 1) - hn(a)
  else m*hn(b2) - hn(b2 - 1)
} when not calc_count(m) and is_list(rs) and length(rs) == 2 and ss === [1, 1] and every(rs, r => is_integer(r) and r >= 0) and (rs[1] === rs[2] or rs[1] === 0 or rs[2] === 0)

# --- asymptotic relations at a point: asymptotic_less_equal, asymptotic_equal, asymptotic_greater_equal,
# asymptotic_equivalent, asymptotic_greater, asymptotic_less ---
# Decided by limits of the ratio q = f/g at the point (Landau's symbols, de Bruijn, Asymptotic Methods in Analysis,
# ch. 1): f = O(g) when limsup |q| < inf, f = o(g) when lim |q| = 0, f ~ g when lim q = 1, f and g of the same order
# when both f = O(g) and g = O(f). An undecided limit gives failure(undecided, name).

# calc_limit(kind, e, spec, dir, assm): limit, max_limit or min_limit of e at spec, with the direction and the
# assumptions unless automatic
fn calc_limit(kind, e, spec, dir, assm) = {
  let f = if kind === max then max_limit else if kind === min then min_limit else limit
  if dir === automatic and assm === automatic then f(e, spec)
  else if assm === automatic then f(e, spec, direction: dir)
  else if dir === automatic then assuming(assm, f(e, spec))
  else assuming(assm, f(e, spec, direction: dir))
}

# calc_lim(kind, e, spec, dir, assm): calc_limit, calc_failed in place of a failure (a failure cannot be passed on)
fn calc_lim(kind, e, spec, dir, assm) = if is_failure(calc_limit(kind, e, spec, dir, assm)) then calc_failed else calc_limit(kind, e, spec, dir, assm)

# calc_bounded(v): true when v is a finite real number, false when it is inf or a failure (a quotient by a function
# that vanishes on the side approached), null otherwise
fn calc_bounded_old(v) = if calc_real(v) then true else if v === inf or v === calc_failed then false else null

fn calc_held_limit(v) = not is_atom(v) and (head(v) === max_limit or head(v) === min_limit or head(v) === limit)
fn calc_bounded(v) = if calc_held_limit(v) then null else calc_bounded_old(v)
# calc_order1(kind, f, g, spec, dir, assm): the relation kind (o_big, o_small, theta, equivalent) of f to g along one
# real direction, or null
fn calc_order1(kind, f, g, spec, dir, assm) = {
  let z = if is_symbol(spec[1]) and calc_real(spec[2]) and (dir === from_above or dir === from_below) then calc_side_zero(kind, f, g, spec[1], spec[2], dir) else null
  if z === null then calc_order1b(kind, f, g, spec, dir, assm) else z
}
# calc_side_zero(kind, f, g, x, a, dir): the relation when f or g is identically 0 on the side (x = a + t or a - t,
# t > 0, refined: unit_step(x) for x < 0): f = 0 is O(g) and o(g) of a g that is not, neither of the same order nor
# equivalent; a g = 0 bounds no f that is not 0; null when neither is 0 or both are
fn calc_side_zero(kind, f, g, x, a, dir) = {
  let t = calc_order_t
  let side = h => assuming(t > 0, refine(rewrite(h, x -> a + (if dir === from_above then t else -t))))
  let fz = side(f) === 0
  let gz = side(g) === 0
  if fz === gz then null
  else if fz then (kind === o_big or kind === o_small)
  else false
}
fn calc_order1b(kind, f, g, spec, dir, assm) = {
  let r = calc_order1a(kind, f, g, spec, dir, assm, false)
  let r2 = if r === null and is_symbol(spec[1]) and calc_real(spec[2]) and (dir === from_above or dir === from_below) then calc_series_order(kind, f/g, spec[1], spec[2], dir, assm) else r
  if r2 === null then calc_order1a(kind, f, g, spec, dir, assm, true) else r2
}
# calc_series_order(kind, q, x, a, dir, assm): the relation from the leading term c (x - a)^e of the series of q = f/g
# (taken in t = |x - a| on the side dir): bounded when e >= 0, o(1) when e > 0, of the order 1 when e = 0, ~ 1 when
# e = 0 and c = 1; null when the series is not found
fn calc_series_order(kind, q, x, a, dir, assm) = {
  let t = calc_order_t
  # a power whose base is on the negative real axis there is written with its principal branch (calc_branch)
  let sq = calc_branch(rewrite(q, x -> a + (if dir === from_above then t else -t)), t)
  # t > 0 in the assumptions: a step function on a jump (fractional_part(x^2) at x = 2) is expanded on its side
  let lead0 = if assm === automatic then assuming(t > 0, cb_lead(sq, t, 0)) else assuming(assm and t > 0, cb_lead(sq, t, 0))
  # a leading term that comes with its order term: the term alone
  let lead = if not is_failure(lead0) and not free_of(lead0, O) then normal(lead0) else lead0
  if is_failure(lead) or head(lead) === add then null
  else if lead === 0 or not free_of(lead, series) then null else {
    let e = simplify(t*d(lead, t)/lead)
    let c = simplify(lead/t^e)
    if not calc_real(e) or not free_of([e, c], t) then null
    else if kind === o_big then e >= 0
    else if kind === o_small then e > 0
    else if kind === theta then e == 0
    else e == 0 and simplify(c) == 1
  }
}
fn calc_order1a(kind, f, g, spec, dir, assm, legacy) = {
  let q = f/g
  let calc_bd = v => if legacy then calc_bounded_old(v) else calc_bounded(v)
  if kind === o_big then calc_bd(calc_lim(max, abs(q), spec, dir, assm))
  else if kind === o_small then {
    let v = calc_lim(max, abs(q), spec, dir, assm)
    # a limit left unevaluated decides nothing (its numerical value is not the limit)
    if calc_held_limit(v) then null else if calc_real(v) then v == 0 else if v === inf or v === calc_failed then false else null
  } else if kind === theta then {
    let hi = calc_bd(calc_lim(max, abs(q), spec, dir, assm))
    let lo = calc_bd(calc_lim(max, abs(1/q), spec, dir, assm))
    if hi === false or lo === false then false else if hi === true and lo === true then true else null
  } else {
    let v = calc_lim(none, q, spec, dir, assm)
    if calc_held_limit(v) then null else if v === calc_failed then false else if is_numeric(v) then v == 1 else if v === inf or v === -inf or v === complex_infinity then false else null
  }
}

# calc_all(vs): true when every value is true, false when one is false, null otherwise
fn calc_all(vs) = if some(vs, v => v === false) then false else if every(vs, v => v === true) then true else null

# calc_branch(e, t): e with every non-integer power b^p whose base is a negative real number at t = 0 and has a
# non-real first-order term written with the branch of the principal power on that side of the cut: b = exp(i pi s)
# (-b) with s the sign of the imaginary part of b (t > 0), so b^p = exp(i pi s p) (-b)^p and the series of the
# power can be taken at a base with positive real part
fn calc_branch(e, t) =
  if is_atom(e) then e
  else if head(e) === pow and length(e) == 2 and is_rational(e[2]) and not is_integer(e[2]) then {
    let b = calc_branch(e[1], t)
    let b0 = rewrite(b, t -> 0)
    let c1 = rewrite(d(b, t), t -> 0)
    if is_numeric(b0) and is_real(b0) and b0 < 0 and is_numeric(c1) and not is_real(c1) then exp(im*pi*sign(im_part(c1))*e[2])*(-b)^e[2]
    # a base that stays on the negative real axis (a rational function of t with real coefficients, negative at t =
    # 0): the principal power there is exp(i pi p) (-b)^p
    else if is_numeric(b0) and is_real(b0) and b0 < 0 and free_of(b, im) and calc_real_rational(b, t) then exp(im*pi*e[2])*(-b)^e[2]
    else b^e[2]
  }
  else apply(head(e), map(u => calc_branch(u, t), apply(list, e)))

fn calc_real_rational(b, t) = {
  let tb = together(b)
  polynomial_expression_q(numerator(tb), [t]) and polynomial_expression_q(denominator(tb), [t]) and every(coefficient_list(expand(numerator(tb)), t), c => is_numeric(c) and is_real(c)) and every(coefficient_list(expand(denominator(tb)), t), c => is_numeric(c) and is_real(c))
}

# calc_order(kind, f, g, spec, dir, assm): at a finite point a two-sided (or reals) relation holds when it holds from
# both sides (the two-sided limits decide when the sides do not), complexes when also along the imaginary directions; a complex direction d approaches along a - d t,
# t -> 0 from above
fn calc_order(kind, f, g, spec, dir, assm) = if is_list(spec[1]) then calc_order_multi(kind, f, g, spec, assm) else {
  let x = spec[1]
  let a = spec[2]
  let finite = not (a === inf or a === -inf or a === complex_infinity)
  if finite and (dir === automatic or dir === two_sided or dir === reals) then {
    let sides = calc_all(map(dd => calc_order(kind, f, g, spec, dd, assm), [from_above, from_below]))
    if sides === null then calc_order1(kind, f, g, spec, dir, assm) else sides
  }
  else if finite and dir === complexes then calc_all(map(dd => calc_order(kind, f, g, spec, dd, assm), [from_above, from_below, im, -im]))
  else if finite and is_number(dir) and not is_real(dir) then {
    let sub = h => calc_branch(rewrite(h, x -> a - dir*calc_order_t), calc_order_t)
    calc_order1(kind, sub(f), sub(g), calc_order_t -> 0, from_above, assm)
  }
  else calc_order1(kind, f, g, spec, dir, assm)
}

# calc_order_multi(kind, f, g, spec, assm): the relation when all the variables of spec = [[x1, ..., xn] -> inf] grow
# together, tested along the curves x_i = c_i t^k_i (t -> inf; exponents 1/2, 1, 2 and coefficients 1, 3): a counter
# example along one curve decides false, agreement along all of them gives true (a sampled, not a proved, answer)
fn calc_curves(n) = if n == 0 then [[]] else flat_map(rest_c => flat_map(k => map(c => join([[c, k]], rest_c), [1, 3]), [1/2, 1, 2]), calc_curves(n - 1))
fn calc_order_multi(kind, f, g, spec, assm) = {
  let sep = calc_order_sep(kind, f, g, spec, assm)
  if sep === null then calc_order_curves(kind, f, g, spec, assm) else sep
}

# calc_order_sep(kind, f, g, spec, assm): the relation at [x1, ..., xn] -> [a1, ..., an] (or -> a for all) when the
# ratio q = f/g is a product c h1(x1) ... hn(xn) of factors in one variable each. Near the point the factors vary
# independently, so q -> 0 when every h_i is bounded and one tends to 0; q is bounded when every h_i is; |q| is
# bounded below when every |h_i| is; q is unbounded (or not bounded below) when one factor is and every other is
# bounded below (above); q -> 1 when every h_i has a finite limit whose product with c is 1. The one-variable
# relations of h_i to 1 are taken two-sided at a finite point (calc_order); null when a factor depends on two
# variables or a relation is undecided
fn calc_sep_rel(k, h, x, a, assm) = if free_of(h, x) then (if k === o_small then false else true) else calc_order(k, h, 1, x -> a, automatic, assm)
fn calc_order_sep(kind, f, g, spec, assm) = {
  let vs = spec[1]
  let pts = if is_list(spec[2]) then spec[2] else map(v => spec[2], vs)
  let q = factor(together(f/g))
  let fs = calc_factors(q)
  let c = product(select(fs, u => every(vs, v => free_of(u, v))))
  let groups = map(v => product(select(fs, u => not free_of(u, v))), vs)
  let mixed = some(fs, u => length(select(vs, v => not free_of(u, v))) > 1)
  if not (length(pts) == length(vs)) or mixed or not is_numeric(c) or c == 0 or not is_numeric(abs(c)) then null
  else {
    let n = length(vs)
    let big = map(i => calc_sep_rel(o_big, groups[i], vs[i], pts[i], assm), range(n))
    let low = map(i => calc_sep_rel(o_big, 1/groups[i], vs[i], pts[i], assm), range(n))
    let small = map(i => calc_sep_rel(o_small, groups[i], vs[i], pts[i], assm), range(n))
    let others = (i, xs) => every(range(n), j => j == i or xs[j] === true)
    let unbounded = some(range(n), i => big[i] === false and others(i, low))
    let tozero = every(big, b => b === true) and some(small, s => s === true)
    let awayzero = every(low, b => b === true)
    let notlow = some(range(n), i => low[i] === false and others(i, big))
    if kind === o_small then (if tozero then true else if awayzero or unbounded then false else null)
    else if kind === o_big then (if every(big, b => b === true) then true else if unbounded then false else null)
    else if kind === theta then (if every(big, b => b === true) and awayzero then true else if unbounded or notlow then false else null)
    else if tozero or unbounded then false
    else {
      let ls = map(i => calc_lim(none, groups[i], vs[i] -> pts[i], automatic, assm), range(n))
      if every(ls, l => is_numeric(l)) then simplify(c*product(ls)) == 1 else null
    }
  }
}

fn calc_order_curves(kind, f, g, spec, assm) = {
  let vs = spec[1]
  if not (spec[2] === inf) or length(vs) > 3 then null
  else {
    let t = calc_order_t
    let curves = calc_curves(length(vs))
    let one = cv => {
      let rules = map(i => vs[i] -> cv[i][1]*t^cv[i][2], range(length(vs)))
      calc_order1(kind, rewrite(f, rules), rewrite(g, rules), t -> inf, automatic, assm)
    }
    calc_all(map(one, curves))
  }
}

# calc_relation(name, kind, f, g, spec, dir, assm): calc_order under the assumptions (whose context also simplifies the
# ratio), a failure when undecided
fn calc_relation(name, kind, f, g, spec, dir, assm) = {
  let v0 = calc_relation1(kind, f, g, spec, dir, assm)
  let v = if v0 === true or v0 === false then v0 else calc_relation_split(kind, f, g, spec, dir, assm)
  if v === true or v === false then v else failure(undecided, name, "the limits of the ratio are not decided")
}
fn calc_relation1(kind, f, g, spec, dir, assm) = if assm === automatic then calc_order(kind, f, g, spec, dir, assm) else assuming(assm, calc_order(kind, f, g, spec, dir, assm))
# an assumption p >= c (or p <= c) on a parameter p, c a number: the relation decided at p = c and under p > c (p < c)
# separately, and the same verdict for both is the verdict (x^a at a = 0 is a constant, for a > 0 it tends to 0)
fn calc_relation_split(kind, f, g, spec, dir, assm) = {
  let parts = if assm === automatic then [] else calc_conj_list(assm)
  let weak = select(parts, q => not is_atom(q) and (head(q) === greater_equal or head(q) === less_equal) and length(q) == 2 and is_symbol(q[1]) and is_numeric(q[2]) and not (q[1] === spec[1]))
  if length(weak) != 1 then null else {
    let q = weak[1]
    let more = select(parts, u => not (u === q))
    let others = if length(more) == 0 then automatic else fold((a, b) => a and b, more[1], rest(more))
    let open = if head(q) === greater_equal then q[1] > q[2] else q[1] < q[2]
    let at = calc_relation1(kind, rewrite(f, q[1] -> q[2]), rewrite(g, q[1] -> q[2]), spec, dir, others)
    let inside = calc_relation1(kind, f, g, spec, dir, if others === automatic then open else others and open)
    if (at === true or at === false) and at === inside then at else null
  }
}

fn asymptotic_less_equal(f, g, spec; direction = automatic, assumptions = automatic) when match_q(spec, _ -> _) =
  calc_relation(asymptotic_less_equal, o_big, f, g, spec, direction, assumptions)
fn asymptotic_greater_equal(f, g, spec; direction = automatic, assumptions = automatic) when match_q(spec, _ -> _) =
  calc_relation(asymptotic_greater_equal, o_big, g, f, spec, direction, assumptions)
fn asymptotic_less(f, g, spec; direction = automatic, assumptions = automatic) when match_q(spec, _ -> _) =
  calc_relation(asymptotic_less, o_small, f, g, spec, direction, assumptions)
fn asymptotic_greater(f, g, spec; direction = automatic, assumptions = automatic) when match_q(spec, _ -> _) =
  calc_relation(asymptotic_greater, o_small, g, f, spec, direction, assumptions)
fn asymptotic_equal(f, g, spec; direction = automatic, assumptions = automatic) when match_q(spec, _ -> _) =
  calc_relation(asymptotic_equal, theta, f, g, spec, direction, assumptions)
fn asymptotic_equivalent(f, g, spec; direction = automatic, assumptions = automatic) when match_q(spec, _ -> _) =
  calc_relation(asymptotic_equivalent, equivalent, f, g, spec, direction, assumptions)

# --- difference quotients, Wronskians and Casoratians, implicit derivatives ---

# calc_dq(f, spec): one difference quotient: [x, h] gives (f(x + h) - f(x))/h, [x, k, h] the k-th forward difference
# sum((-1)^(k - j) binomial(k, j) f(x + j h), j = 0..k)/h^k
fn calc_dq(f, spec) = {
  let x = spec[1]
  let k = if length(spec) == 3 then spec[2] else 1
  let h = last(spec)
  let num = calc_total(map(j => (-1)^(k - j)*binomial(k, j)*rewrite(f, x -> x + j*h), range(0, k)))
  cancel(num/h^k)
}

# difference_quotient(f, [x, h]), (f, [x, k, h]), (f, spec1, spec2, ...): the difference quotient of f in x with step
# h (the k-th one), applied for each specification in turn; lists of functions element-wise
fn difference_quotient(f, specs...; assumptions = automatic) when length(specs) > 0 and every(specs, s => is_list(s) and (length(s) == 2 or length(s) == 3)) =
  if assumptions !== automatic then assuming(assumptions, refine(apply(difference_quotient, join([f], specs))))
  else if is_list(f) then map(g => apply(difference_quotient, join([g], specs)), f) else fold(calc_dq, f, specs)

# wronskian([f1, ..., fk], x): the determinant of the matrix of the derivatives d(fi, [x, j]), j = 0..k - 1;
# wronskian(eq, y, x) for a linear ordinary differential equation a_k y^(k) + a_(k - 1) y^(k - 1) + ... = r: Abel's
# identity exp(-integrate(a_(k - 1)/a_k, x)) (the Wronskian of the fundamental system normalised to 1 at a point)
# the Wronskians of the classical pairs of solutions (Abramowitz and Stegun 9.1.16, 9.1.15, 10.1.6, 10.2.?, 9.2.8)
rule wronskian([airy_ai(x_), airy_bi(x_)], x_) := 1/pi
rule wronskian([bessel_j(n_, x_), bessel_y(n_, x_)], x_) := 2/(pi*x)
rule wronskian([bessel_i(n_, x_), bessel_k(n_, x_)], x_) := -1/x
fn wronskian(fs, x) when is_list(fs) and length(fs) > 0 = {
  let k = length(fs)
  simplify(det(map(j => map(f => d(f, [x, j]), fs), range(0, k - 1))))
}
# wronskian([y1'(x) == ..., ...], [y1, ..., yk], x) for a linear first-order system y' = A(x) y + r: Liouville's
# formula exp(integrate(trace(A), x)) (the Wronskian of the fundamental matrix that is the identity at a point;
# Coddington and Levinson, Theory of Ordinary Differential Equations, ch. 1, theorem 7.3)
fn calc_wr_system(eqs, ys, x) = {
  let dys = map(y => derivative(1)(y)(x), ys)
  let sol = solve(eqs, dys)
  if not is_list(sol) or length(sol) != 1 then null else {
    let rhs = rewrite(dys, sol[1])
    let vals = map(y => y(x), ys)
    let lin1 = r => {
      let er = expand(r)
      let cs = map(v => coefficient(er, v), vals)
      every(cs, c => every(vals, v => free_of(c, v))) and every(vals, v => free_of(expand(er - calc_total(map(i => cs[i]*vals[i], range(length(vals))))), v)) and free_of(r, dys)
    }
    let lin = every(rhs, lin1)
    if not lin then null else simplify(exp(integrate(calc_total(map(i => coefficient(expand(rhs[i]), vals[i]), range(length(ys)))), x)))
  }
}
fn wronskian(eqs, ys, x) when is_list(eqs) and is_list(ys) and length(eqs) == length(ys) and length(ys) > 0 and every(eqs, e => head(e) === equal) and every(ys, is_symbol) and not (calc_wr_system(eqs, ys, x) === null) =
  calc_wr_system(eqs, ys, x)
fn wronskian(eq, y, x) when head(eq) === equal = {
  let e = expand(eq[1] - eq[2])
  let ders = select(range(1, 20), j => not free_of(e, derivative(j)(y)(x)))
  let k = if length(ders) == 0 then 0 else max(ders)
  let ak = coefficient(e, derivative(k)(y)(x))
  let low = if k == 1 then y(x) else derivative(k - 1)(y)(x)
  let a1 = coefficient(e, low)
  simplify(exp(-integrate(a1/ak, x)))
}

# casoratian([f1, ..., fk], m): the determinant of the matrix of the shifted values fi(m + j), j = 0..k - 1
# the Fibonacci and Lucas sequences: F(m) L(m + 1) - F(m + 1) L(m) = -2 (-1)^m
rule casoratian([fibonacci(m_), lucas_l(m_)], m_) := -2*(-1)^m
fn casoratian(fs, m) when is_list(fs) and length(fs) > 0 = {
  let k = length(fs)
  simplify(det(map(j => map(f => rewrite(f, m -> m + j), fs), range(0, k - 1))))
}

# calc_implicit(eqs, ys, t): the derivatives of the dependent variables ys in t defined by the equations eqs (each
# lhs == rhs): the solution of the linear system J y' = -F_t with J the Jacobian of F = lhs - rhs in ys
fn calc_implicit(eqs, ys, t) = {
  let fs = map(e => e[1] - e[2], eqs)
  let jac = map(f => map(y => d(f, y), ys), fs)
  let ft = map(f => -d(f, t), fs)
  linear_solve(jac, ft)
}

# implicit_d(f, eqs, ys, x), (f, eqs, ys, [x, k]), (f, eqs, ys, x1, x2, ...): the derivative of f in x when the
# variables ys are functions of x defined implicitly by the equations eqs (a single equation and variable need no
# list); implicit_d(eq, y, x): the derivative of y itself; a variable list [[x1, x2, ...]] gives the list of derivatives
fn implicit_d(eq, y, x) when head(eq) === equal and not is_list(y) = implicit_d(y, eq, y, x)
fn implicit_d(f, eqs, ys, specs...) when length(specs) == 1 and is_list(specs[1]) and length(specs[1]) == 1 and is_list(specs[1][1]) = map(v => implicit_d(f, eqs, ys, v), specs[1][1])
fn implicit_d(f, eqs, ys, specs...) when length(specs) > 0 = {
  let es = if is_list(eqs) then eqs else [eqs]
  let vs = if is_list(ys) then ys else [ys]
  var g = f
  for s in specs {
    let t = if is_list(s) then s[1] else s
    let k = if is_list(s) then s[2] else 1
    let yd = map(simplify, calc_implicit(es, vs, t))
    for i in range(k) {
      g = simplify(d(g, t) + calc_total(map(j => d(g, vs[j])*yd[j], range(length(vs)))))
    }
  }
  g
}

# alternating_factorial(-1) = 0! - alternating_factorial(0) = 1, from a(m) + a(m - 1) = m!
rule alternating_factorial(m_) := 1 when m === -1


# --- series coefficients ---

# calc_series_coeff(s, k): the coefficient of (x - x0)^k in the series s = series_data(x, x0, cs, nmin, nmax): 0 below
# nmin, indeterminate from nmax on (the order is not known there)
fn calc_series_coeff(s, k) = {
  let cs = s[3]
  if k < s[4] then 0 else if k >= s[5] then indeterminate else if k - s[4] + 1 > length(cs) then 0 else cs[k - s[4] + 1]
}

# series_coefficient(s, k), (s, [k1, k2, ...]): the coefficient of order k of the series s (nested for series in
# several variables); series_coefficient(f, [x, x0, k]), (f, [x, x0, k], [y, y0, j], ...): the coefficient of
# (x - x0)^k in the expansion of f (an exact integer k), in each variable in turn
rule series_coefficient(s_, k_) := calc_series_coeff(s, k) when head(s) === series_data and is_integer(k)
rule series_coefficient(s_, ks_) := fold((t, k) => if head(t) === series_data then calc_series_coeff(t, k) else (if k == 0 then t else 0), s, ks) when head(s) === series_data and is_list(ks) and every(ks, is_integer)
rule series_coefficient(f_, specs__) := fold((g, sp) => {
  let s = series(g, sp[1], sp[2], sp[3])
  if head(s) === series_data then calc_series_coeff(s, sp[3]) else coefficient(normal(s), sp[1] - sp[2], sp[3])
}, f, specs) when not (head(f) === series_data) and every(specs, sp => is_list(sp) and length(sp) == 3 and is_integer(sp[3]) and not (sp[2] === inf))

# series_coefficient(f, [x, x0, n]) with a symbolic n: the general coefficient of (x - x0)^n for a sum of terms
# c x^m h(x) with h one of exp(a x), sin(a x), cos(a x), sinh(a x), cosh(a x) and (u + v x)^p (the Taylor series of
# these: DLMF 4.6, 4.15, 15.4 and the binomial series); valid for the integers n >= m
fn calc_sc_h(h, x, k) = {
  # coefficient of x^k in h (a function of x), k symbolic >= 0; null when unknown
  if head(h) === pow and h[1] === e and not free_of(h[2], x) and simplify(d(h[2], x, x)) === 0 and rewrite(h[2], x -> 0) === 0 then simplify(d(h[2], x))^k/factorial(k)
  else if head(h) === pow and free_of(h[2], x) and not free_of(h[1], x) then {
    let b = h[1]
    let u = rewrite(b, x -> 0)
    let v = simplify(d(b, x))
    if not free_of(v, x) or not free_of(simplify(b - u - v*x), x) or simplify(b - u - v*x) != 0 or u === 0 then null
    else u^h[2]*(if is_integer(h[2]) and h[2] < 0 then (-1)^k*binomial(k - h[2] - 1, k) else binomial(h[2], k))*(v/u)^k
  }
  else if head(h) === exp and free_of(simplify(d(h[1], x, x)), x) and simplify(d(h[1], x, x)) === 0 and rewrite(h[1], x -> 0) === 0 then simplify(d(h[1], x))^k/factorial(k)
  else if (head(h) === sin or head(h) === cos or head(h) === sinh or head(h) === cosh) and simplify(d(h[1], x, x)) === 0 and rewrite(h[1], x -> 0) === 0 then {
    let a = simplify(d(h[1], x))
    if head(h) === sin then a^k*sin(k*pi/2)/factorial(k)
    else if head(h) === cos then a^k*cos(k*pi/2)/factorial(k)
    else if head(h) === sinh then a^k*(1 - (-1)^k)/(2*factorial(k))
    else a^k*(1 + (-1)^k)/(2*factorial(k))
  }
  else calc_sc_h2(h, x, k)
}
# more series with a known general term, h of a x (a free of x): polylog(s, a x) =
# sum a^k k^-s x^k (k >= 1, DLMF 25.12.10); the hypergeometric series pFq(as; bs; a x) = sum prod (as)_k/prod (bs)_k
# a^k/k! x^k (DLMF 16.2.1); bessel_i(nu, a x) and bessel_j(nu, a x) for an integer nu >= 0: the term
# ((a/2)^(2 j + nu)/(j! (j + nu)!)) x^(2 j + nu), times (-1)^j for J (DLMF 10.2.2, 10.25.2), so the coefficient of
# x^k is 0 unless k - nu is even and >= 0
fn calc_sc_lin(u, x) = {
  let a = simplify(d(u, x))
  if free_of(a, x) and simplify(u - a*x) === 0 then a else null
}
fn calc_sc_h2(h, x, k) = {
  let hd = head(h)
  if hd === polylog and length(h) == 2 and free_of(h[1], x) and not (calc_sc_lin(h[2], x) === null) then {
    let a = calc_sc_lin(h[2], x)
    piecewise([[a^k*k^(-h[1]), k >= 1]], 0)
  }
  else if (hd === hypergeometric_pfq and length(h) == 3 and is_list(h[1]) and is_list(h[2])) and free_of([h[1], h[2]], x) and not (calc_sc_lin(h[3], x) === null) then {
    let a = calc_sc_lin(h[3], x)
    product(map(p => pochhammer(p, k), h[1]))/product(map(q => pochhammer(q, k), h[2]))*a^k/factorial(k)
  }
  else if (hd === bessel_i or hd === bessel_j) and length(h) == 2 and is_integer(h[1]) and h[1] >= 0 and not (calc_sc_lin(h[2], x) === null) then {
    let a = calc_sc_lin(h[2], x)
    let nu = h[1]
    let j = (k - nu)/2
    piecewise([[(if hd === bessel_j then (-1)^j else 1)*(a/2)^k/(factorial(j)*factorial(j + nu)), mod(k - nu, 2) == 0 and k >= nu]], 0)
  }
  else null
}
fn calc_sc_term(t, x, n) = {
  let fs = calc_factors(t)
  let cs = select(fs, f => free_of(f, x))
  let xs = select(fs, f => f === x or (head(f) === pow and f[1] === x and is_integer(f[2])))
  let hs = select(fs, f => not free_of(f, x) and not some(xs, g => g === f))
  let m = calc_total(map(f => if f === x then 1 else f[2], xs))
  let c = product(cs)
  if length(hs) == 0 then null
  else if length(hs) > 1 then null
  else {
    let r = calc_sc_h(hs[1], x, n - m)
    # the formulas hold for the orders n >= m; below, the coefficient is 0 (x/(x - 5) has no constant term)
    if r === null then null
    else if head(r) === piecewise and length(r) == 2 and r[2] === 0 then c*piecewise(map(pc => [pc[1], pc[2] and n >= m], r[1]), 0)
    else c*piecewise([[r, n >= m]], 0)
  }
}
# a rational function: on its partial fractions over the complex numbers (calc_hb_pf), the
# coefficient of x^n in (x - z)^-k is (-1)^k binomial(n + k - 1, k - 1) z^(-n - k) (n >= 0, the binomial series),
# and the polynomial part adds its coefficients at the orders 0..d: piecewise in n, the orders up to d by value
fn calc_sc_rat(f, x, n) = {
  let pf = calc_hb_pf(f, x)
  if pf === null or length(pf[2]) == 0 then null
  else {
    let gen = calc_total(map(p => p[3]*(-1)^p[2]*binomial(n + p[2] - 1, p[2] - 1)*p[1]^(-n - p[2]), pf[2]))
    let d = if pf[1] === 0 then -1 else degree(pf[1], x)
    let low = map(j => [simplify(coefficient(pf[1], x, j) + rewrite(gen, n -> j)), n == j], range(0, d))
    let pieces = select(low, pc => not (pc[1] === 0))
    piecewise(join(pieces, [[gen, if d < 0 then n >= 0 else n > d]]), 0)
  }
}
fn calc_sc_sym(f, x, n) = {
  let r = together(f)
  if polynomial_expression_q(numerator(r), x) and polynomial_expression_q(denominator(r), x) and not free_of(denominator(r), x) then calc_sc_rat(f, x, n)
  else {
    let ts = calc_terms(expand(f))
    let rs = map(t => calc_sc_term(t, x, n), ts)
    if some(rs, r => r === null) then null else calc_total(rs)
  }
}
rule series_coefficient(f_, [x_, x0_, n_], options(assumptions -> a_)) := simplify(assuming(a, refine(calc_sc_sym(if x0 === 0 then f else rewrite(f, x -> x + x0), x, n))), element(n, integers) and a) when is_symbol(x) and is_symbol(n) and is_numeric(x0) and not (x0 === inf) and not (calc_sc_sym(if x0 === 0 then f else rewrite(f, x -> x + x0), x, n) === null)
rule series_coefficient(f_, [x_, x0_, n_]) := simplify(calc_sc_sym(if x0 === 0 then f else rewrite(f, x -> x + x0), x, n), element(n, integers)) when is_symbol(x) and is_symbol(n) and is_numeric(x0) and not (x0 === inf) and not (calc_sc_sym(if x0 === 0 then f else rewrite(f, x -> x + x0), x, n) === null)

# --- polynomial structure: factor_list, factor_terms, exponent, collect, cyclotomic, polynomial_reduce,
# polynomial_mod, partial_fractions, polynomial_expression_q, possible_zero_q ---

# calc_terms(e), calc_factors(e): the summands of e, the factors of e (a one-element list when e is not a sum, a
# product)
fn calc_terms(e) = if head(e) === add then apply(list, e) else [e]
fn calc_factors(e) = if head(e) === mul then apply(list, e) else [e]

# calc_num_part(t): the product of the numeric factors of the term t; calc_xexp(t, x): the exponent of x in t
fn calc_num_part(t) = product(select(calc_factors(t), is_number))
fn calc_xexp(t, x) = calc_total(map(f => if f === x then 1 else if head(f) === pow and f[1] === x then f[2] else 0, calc_factors(t)))

# factor_list(p): [[c, 1], [f1, e1], [f2, e2], ...], the numeric factor c and the irreducible factors fi with their
# multiplicities ei of factor(p); the factors of the denominator of a rational function have negative multiplicities
# to_rules(cond): the equations x == v of a condition (as reduce writes it) as rules: a conjunction gives one list of
# rules, a disjunction (after logical_expand) a list of them, one per case (no sequence is spliced)
fn calc_rules_conj(c) =
  if not is_atom(c) and head(c) === `and` then flatten(map(calc_rules_conj, apply(list, c)))
  else if not is_atom(c) and head(c) === equal and length(c) == 2 then [c[1] -> c[2]]
  else []
rule to_rules(e_) := {
  let d = logical_expand(e)
  if not is_atom(d) and head(d) === `or` then map(calc_rules_conj, apply(list, d)) else calc_rules_conj(d)
} when not is_atom(e) and (head(e) === equal or head(e) === `and` or head(e) === `or`)

# over a finite field the constant factor is a field element (a constant finite_field_element is a constant)
fn calc_is_constant(f) = is_number(f) or (not is_atom(f) and head(f) === finite_field_element)
fn calc_factor_pairs(p) = {
  let fs = calc_factors(factor(p))
  let pairs = map(f => if head(f) === pow and is_integer(f[2]) then [f[1], f[2]] else [f, 1], fs)
  [product(map(q => q[1]^q[2], select(pairs, q => calc_is_constant(q[1])))), select(pairs, q => not calc_is_constant(q[1]))]
}
fn calc_field_one(p) = {
  let es = cases([p], finite_field_element(_, _), inf)
  if length(es) == 0 then 1 else es[1]/es[1]
}
fn factor_list(p) when not is_list(p) = {
  let r = together(p)
  let a = calc_factor_pairs(numerator(r))
  let b = calc_factor_pairs(denominator(r))
  let c = a[1]/b[1]
  join([[if is_number(c) then c*calc_field_one(p) else c, 1]], sort(a[2]), map(q => [q[1], -q[2]], sort(b[2])))
}

# factor_terms(p): the numerical content of p pulled out (c (p/c) with c the gcd of the numeric coefficients);
# factor_terms(p, x), (p, [x1, ...]): also the factors free of the variables (the polynomial gcd of the
# coefficients in x, then in the next variable, ...); element-wise on lists
fn calc_content(p) = {
  let ts = calc_terms(expand(p))
  let cs = map(calc_num_part, ts)
  if length(cs) > 0 and every(cs, is_integer) then apply(gcd, cs) else 1
}
fn calc_factor_terms(p, vs) = {
  let c = calc_content(p)
  var rest = expand(p/c)
  var out = [c]
  for v in vs {
    let cl = coefficient_list(rest, v)
    let g = fold((a, b) => polynomial_gcd(a, b), 0, cl)
    if not (g === 0) and not is_number(g) then {
      out = join(out, [factor(g)])
      rest = cancel(rest/g)
    }
  }
  if length(out) == 1 and c === 1 then p else product(join(out, [expand(rest)]))
}
# the same modulo an integer m (factor_terms(p, modulus: m)): the coefficients reduced, then the leading coefficient
# pulled out when it is invertible (2 x^2 + 2 modulo 3 is 2 (x^2 + 1))
fn calc_factor_terms_mod(p, m) = {
  let q = polynomial_mod(p, m)
  let vs = variables(q)
  if length(vs) == 0 then q else {
    let lc = calc_num_part(coefficient(q, vs[1], degree(q, vs[1])))
    if not is_integer(lc) or lc == 1 or gcd(lc, m) != 1 then q
    else product([lc, polynomial_mod(expand(q*power_mod(lc, -1, m)), m)])
  }
}
fn factor_terms(p; modulus = 0, trig = false) =
  if trig === true then factor_terms(trig_reduce(p), modulus: modulus)
  else if modulus != 0 and is_integer(modulus) then (if is_list(p) then map(q => calc_factor_terms_mod(q, modulus), p) else calc_factor_terms_mod(p, modulus))
  else if is_list(p) then map(factor_terms, p) else if head(p) === less or head(p) === equal or head(p) === greater then apply(head(p), map(factor_terms, apply(list, p))) else calc_factor_terms(p, [])
fn factor_terms(p, v) = if is_list(p) then map(q => factor_terms(q, v), p) else calc_factor_terms(p, if is_list(v) then v else [v])

# exponent(e, x), (e, x, h): the largest power of x in the expanded e (h(all powers) for another function h, such
# as min or list; -inf for e = 0)
# (e, x, modulus: m): of e with its coefficients reduced modulo m; (e, x, trig: true): of e with tan, cot, sec and
# csc written by sin and cos (tan(x) has exponent -1 in cos(x))
fn calc_exponents(e, x) = sort(unique(map(t => calc_xexp(t, x), calc_terms(expand(e)))))
fn calc_sin_cos(e) = rewrite(e, [tan(u_) -> sin(u)/cos(u), cot(u_) -> cos(u)/sin(u), sec(u_) -> 1/cos(u), csc(u_) -> 1/sin(u)])
fn calc_exponent_form(e, modulus, trig) = {
  let f = if trig === true then calc_sin_cos(e) else e
  if is_integer(modulus) and modulus != 0 then polynomial_mod(expand(f), modulus) else f
}
fn exponent(e, x; modulus = 0, trig = false) = {
  let f = calc_exponent_form(e, modulus, trig)
  if f === 0 then -inf else apply(max, calc_exponents(f, x))
}
fn exponent(e, x, h) = if e === 0 then -inf else apply(h, calc_exponents(e, x))

# collect(e, x), (e, x, h), (e, [x1, x2, ...]): e expanded and its terms gathered by powers of x, the coefficient of
# each power (h applied to it) in front; several variables recursively in order
fn calc_collect(e, xs, h) = {
  if length(xs) == 0 then h(e) else {
    let x = xs[1]
    let ts = calc_terms(expand(e))
    let ks = sort(unique(map(t => calc_xexp(t, x), ts)))
    calc_total(map(k => calc_collect(calc_total(map(t => t/x^k, select(ts, t => calc_xexp(t, x) === k))), rest(xs), h)*x^k, ks))
  }
}
# (e, x, modulus: m): e expanded with its coefficients reduced modulo m, then collected
fn collect(e, x; modulus = 0) =
  if is_list(e) then map(t => collect(t, x, modulus: modulus), e)
  else calc_collect(if is_integer(modulus) and modulus != 0 then polynomial_mod(expand(e), modulus) else e, if is_list(x) then x else [x], t => t)
fn collect(e, x, h) = if is_list(e) then map(t => collect(t, x, h), e) else calc_collect(e, if is_list(x) then x else [x], h)

# cyclotomic(k, x): the k-th cyclotomic polynomial, the product of (x^d - 1)^moebius_mu(k/d) over the divisors d of k
fn calc_cyclotomic(k, x) = expand(cancel(product(map(dd => (x^dd - 1)^moebius_mu(k/dd), divisors(k)))))
fn cyclotomic(k, x) when is_integer(k) and k >= 1 = if is_symbol(x) then calc_cyclotomic(k, x) else rewrite(calc_cyclotomic(k, calc_cyclotomic_var), calc_cyclotomic_var -> x)
fn cyclotomic(k, x) when is_integer(k) and k == 0 = 1

# polynomial_mod(p, m): p with its numeric coefficients reduced modulo the integer m, or the remainder of p on
# division by the polynomial m (in its first variable); polynomial_mod(p, [m1, m2, ...]): each modulus in turn
fn calc_coeff_mod(p, m) = calc_total(map(t => {
  let c = calc_num_part(t)
  let r = if is_integer(c) then mod(c, m) else if is_rational(c) and gcd(denominator(c), m) == 1 then mod(numerator(c)*power_mod(denominator(c), -1, m), m) else c
  # the zero polynomial is its own reduction (t/c would divide by zero)
  if c === 0 then 0 else r*(t/c)
}, calc_terms(expand(p))))
fn polynomial_mod(p, m; modulus = 0) = {
  if is_list(m) then fold((a, b) => polynomial_mod(a, b, modulus: modulus), p, m)
  else if is_integer(m) then calc_coeff_mod(p, m)
  # modulus: a prime: the remainder in F_prime[x] of the polynomial in one variable
  else if is_integer(modulus) and modulus > 1 then {
    let vs = union(variables(p), variables(m))
    if length(vs) == 1 then polynomial_remainder(p, m, vs[1], modulus: modulus) else failure(argument, polynomial_mod, "with modulus: p the polynomials must have one variable")
  }
  else {
    let vs = variables(m)
    let common = select(variables(p), v => count(vs, v) > 0)
    let all = union(variables(p), vs)
    # several variables: the remainder of the multivariate division (a polynomial: x^2 y mod x y - 1 is x, not 1/y),
    # in the lexicographic order with the later variables larger (x before a in a x - 1); one variable: the univariate remainder
    if length(vs) == 0 then p
    else if length(all) > 1 then polynomial_reduce(p, [m], reverse(all))[2]
    else expand(polynomial_remainder(p, m, if length(common) > 0 then common[1] else vs[1]))
  }
}

# partial_fractions(f, x): the partial fraction decomposition of the rational function f over the complex numbers
# (the polynomial part plus residue/(x - a) for each simple root a of the denominator; apart's rational form when
# the denominator has repeated roots or roots solve cannot give); extension: reals keeps irreducible real factors
fn calc_pf_complex(f, x) = {
  let r = together(f)
  let num = numerator(r)
  let den = denominator(r)
  let dd = d(den, x)
  let sq = degree(polynomial_gcd(den, dd), x) == 0
  let sols = if sq then solve(den == 0, x) else []
  if not sq or not is_list(sols) or length(sols) != degree(den, x) then apart(f, x) else {
    let q = polynomial_quotient(num, den, x)
    let rm = polynomial_remainder(num, den, x)
    q + calc_total(map(s => {
      let a = rewrite(x, s)
      simplify(rewrite(rm, s)/rewrite(dd, s))/(x - a)
    }, sols))
  }
}
fn partial_fractions(f, x; extension = automatic) = if extension === reals then apart(f, x) else calc_pf_complex(f, x)

# polynomial_reduce(p, [g1, ..., gk], [x1, ..., xm]): [[q1, ..., qk], r] with p = q1 g1 + ... + qk gk + r and no term
# of r divisible by a leading term of the gi, by the multivariate division algorithm in the lexicographic order of
# the variables (Cox, Little and O'Shea, Ideals, Varieties, and Algorithms, 2.3)
fn calc_expvec(t, xs) = map(x => calc_xexp(t, x), xs)
fn calc_lex_greater(a, b) = {
  var res = null
  for i in range(length(a)) {
    if res === null and a[i] != b[i] then res = a[i] > b[i]
  }
  res === true
}
fn calc_lead(p, xs) = fold((best, t) => if best === null or calc_lex_greater(calc_expvec(t, xs), calc_expvec(best, xs)) then t else best, null, calc_terms(expand(p)))
# the same in a monomial order of groebner_basis: lexicographic, degree_lexicographic, degree_reverse_lexicographic
# (total degree first; ties by lex, or by the reverse of the last variable's exponent)
fn calc_order_greater(a, b, o) =
  if o === lexicographic then calc_lex_greater(a, b)
  else if total(a) != total(b) then total(a) > total(b)
  else if o === degree_lexicographic then calc_lex_greater(a, b)
  else calc_lex_greater(reverse(b), reverse(a))
fn calc_lead_in(p, xs, o) = if o === lexicographic then calc_lead(p, xs) else fold((best, t) => if best === null or calc_order_greater(calc_expvec(t, xs), calc_expvec(best, xs), o) then t else best, null, calc_terms(expand(p)))
fn calc_divides(a, b) = every(range(length(a)), i => a[i] <= b[i])
# options: monomial_order (lexicographic, degree_lexicographic, degree_reverse_lexicographic) and
# coefficient_domain: rationals (the only domain)
fn polynomial_reduce(p, gs, xs; monomial_order = lexicographic, coefficient_domain = rationals) =
  if coefficient_domain !== rationals then failure(argument, polynomial_reduce, "the coefficient domain is rationals")
  else if not (monomial_order === lexicographic or monomial_order === degree_lexicographic or monomial_order === degree_reverse_lexicographic) then failure(argument, polynomial_reduce, "unknown monomial order")
  else calc_preduce(p, gs, xs, monomial_order)
fn calc_preduce(p, gs, xs, o) = {
  let g = if is_list(gs) then gs else [gs]
  let vs = if is_list(xs) then xs else [xs]
  var qs = constant_array(0, length(g))
  var r = 0
  var h = expand(p)
  var steps = 0
  while not (h === 0) and steps < 10000 {
    steps = steps + 1
    let lt = calc_lead_in(h, vs, o)
    let ev = calc_expvec(lt, vs)
    var done = false
    for i in range(length(g)) {
      if not done then {
        let lg = calc_lead_in(g[i], vs, o)
        if calc_divides(calc_expvec(lg, vs), ev) then {
          let m = cancel(lt/lg)
          qs = replace_part(qs, i -> expand(qs[i] + m))
          h = expand(h - m*g[i])
          done = true
        }
      }
    }
    if not done then {
      r = r + lt
      h = expand(h - lt)
    }
  }
  [qs, r]
}

# polynomial_expression_q(e, x), (e, [x1, ...]), (e, xs, test): whether e is built from the variables and from
# expressions free of them (each passing test) by sums, products and non-negative integer powers
fn calc_poly_expr(e, xs, test) = {
  if some(xs, x => e === x) then true
  else if every(xs, x => free_of(e, x)) then (test === automatic or test(e) === true)
  else if head(e) === add or head(e) === mul then every(apply(list, e), t => calc_poly_expr(t, xs, test))
  else if head(e) === pow and is_integer(e[2]) and e[2] >= 0 then calc_poly_expr(e[1], xs, test)
  else false
}
fn polynomial_expression_q(e, x) = calc_poly_expr(e, if is_list(x) then x else [x], automatic)
fn polynomial_expression_q(e, x, test) = calc_poly_expr(e, if is_list(x) then x else [x], test)

# polynomial_q(e, x), (e, [x1, ...]): whether e is a polynomial in the variables: built by sums, products and
# non-negative integer powers from the variables and from coefficients that contain no symbol of the variables
# (so sec(x) is no coefficient of cos(x)); a power b^p is the power (b^q)^(p/q) of a variable b^q when p/q is a
# non-negative integer (x^(3/2) is the cube of sqrt(x)); polynomial_q(e) takes the variables of e
fn r38_poly_power(e, xs) = {
  let b = if head(e) === pow then e[1] else e
  let p = if head(e) === pow then e[2] else 1
  some(xs, x => not is_atom(x) and head(x) === pow and x[1] === b and is_rational(x[2]) and is_rational(p) and is_integer(p/x[2]) and p/x[2] >= 0)
}
fn r38_poly_q(e, xs, syms) = {
  if some(xs, x => e === x) then true
  else if every(syms, s => free_of(e, s)) then true
  else if r38_poly_power(e, xs) then true
  else if head(e) === add or head(e) === mul then every(apply(list, e), t => r38_poly_q(t, xs, syms))
  else if head(e) === pow and is_integer(e[2]) and e[2] >= 0 then r38_poly_q(e[1], xs, syms)
  else false
}
fn polynomial_q(e, x) = {
  let xs = if is_list(x) then x else [x]
  r38_poly_q(e, xs, unique(flat_map(calc_syms, xs)))
}
fn polynomial_q(e) = polynomial_q(e, variables(e))

# solve_always(eqs, vars): the values of the other symbols for which the polynomial (or rational) equations hold
# for every value of the variables: every coefficient of the numerator of lhs - rhs, as a polynomial in the
# variables, vanishes; solved by solve for the symbols of those coefficients (for the first of them in the canonical
# order when there are more symbols than equations)
fn r38_always_parts(eq, xs) = {
  let ex = if not is_atom(eq) and head(eq) === equal then eq[1] - eq[2] else eq
  let num = numerator(together(ex))
  if polynomial_expression_q(num, xs) then map(r => r[2], coefficient_rules(expand(num), xs)) else null
}
fn r38_solve_always(eqs, x) = {
  let xs = if is_list(x) then x else [x]
  let es = if is_list(eqs) then eqs else [eqs]
  let parts = map(q => r38_always_parts(q, xs), es)
  if some(parts, p => p === null) then null
  else {
    let cs = select(catenate(parts), c => not (c === 0))
    let unknowns = sort(select(unique(flat_map(calc_syms, cs)), s => not member(xs, s)))
    let eqs2 = map(c => c == 0, cs)
    # more symbols than coefficient equations: the first ones in the canonical order are solved for, in terms of
    # the others, when that system has solutions
    let first = if length(unknowns) > length(cs) then solve(eqs2, take(unknowns, length(cs))) else []
    if length(cs) == 0 then [[]]
    else if length(unknowns) == 0 then (if every(cs, c => possible_zero_q(c)) then [[]] else [])
    else if is_list(first) and length(first) >= 1 then first
    else solve(eqs2, unknowns)
  }
}
fn solve_always(eqs, x) when r38_solve_always(eqs, x) !== null = r38_solve_always(eqs, x)

# --- compose_series, inverse_series ---
# Power series as series_data(x, x0, [c_n0, ...], n0, n1) (the sum of c_k (x - x0)^k for n0 <= k < n1, plus
# O((x - x0)^n1)); a sum of terms and an O term stands for the series it truncates. The composition f(g) is the sum of
# the terms of f at g - x0 in the series arithmetic, which carries the orders (Knuth, The art of computer programming
# 2, 4.7); the reversion solves y = s(x) for x by the fixed-point iteration x = (u - (b_2 x^2 + ...))/b_1, one more
# correct coefficient a step (Knuth, 4.7, algorithm L in its simplest form; Henrici, Applied and computational complex
# analysis 1, 1.9), after (s/b_v)^(1/v) for a leading term b_v (x - x0)^v with v > 1 (a Puiseux series in
# (y - y0)^(1/v)).

# r38_order_of(t): [variable, point, order] of an O term O((x - a)^k) or O(x - a)^k, else null
fn r38_order_of(t) = {
  let k = if head(t) === pow and not is_atom(t[1]) and head(t[1]) === O then t[2] else 1
  let o = if head(t) === pow and not is_atom(t[1]) and head(t[1]) === O then t[1] else t
  if is_atom(o) or not (head(o) === O) then null
  else {
    let u = o[1]
    let bas = if not is_atom(u) and head(u) === pow then u[1] else u
    let kk = k*(if not is_atom(u) and head(u) === pow then u[2] else 1)
    let vs = calc_syms(bas)
    if length(vs) != 1 or not is_integer(kk) then null
    else {
      let x = vs[1]
      let a = x - bas
      if free_of(a, x) then [x, a, kk] else null
    }
  }
}
# r38_series(s): s as series_data, or null
fn r38_series(s) =
  if not is_atom(s) and head(s) === series_data then s
  else if not is_atom(s) and head(s) === add then {
    let ts = apply(list, s)
    let os = select(ts, t => r38_order_of(t) !== null)
    if length(os) != 1 then null else {
      let o = r38_order_of(os[1])
      let p = total(select(ts, t => r38_order_of(t) === null))
      if free_of(p, o[1]) then series_data(o[1], o[2], [p], 0, o[3])
      else {
        let r = series(p, [o[1], o[2], o[3] - 1])
        if not is_atom(r) and head(r) === series_data and r[5] >= o[3] then series_data(r[1], r[2], take(r[3], max(0, min(length(r[3]), o[3] - r[4]))), r[4], o[3]) else null
      }
    }
  }
  else null
# r38_coef(s, k): the coefficient of (x - x0)^k in the series_data s
fn r38_coef(s, k) = if k >= s[4] and k - s[4] < length(s[3]) then s[3][k - s[4] + 1] else 0
# r38_valuation(s, from): the least k >= from below the order with a non-zero coefficient, or null
fn r38_valuation(s, from) = {
  let ks = select(range(max(from, s[4]), s[5] - 1), k => not possible_zero_q(r38_coef(s, k)))
  if length(ks) == 0 then null else ks[1]
}
fn r38_compose(f, g) = {
  let x0 = f[2]
  let h = g - x0
  let hs = r38_series(h)
  let v = if hs === null then null else r38_valuation(hs, 0)
  if hs === null or v === null or v < 1 or not possible_zero_q(r38_coef(hs, 0)) then null
  else {
    let terms = map(k => r38_coef(f, k)*h^k, range(f[4], f[5] - 1))
    total(terms) + O((g[1] - g[2])^(v*f[5]))
  }
}
fn r38_compose_series(f, g) = {
  let fs = r38_series(f)
  let gs = r38_series(g)
  if fs === null or gs === null then null else r38_compose(fs, gs)
}
# a composition that does not exist (g - x0 of positive valuation is needed) stays unevaluated
fn compose_series(f, g) when r38_compose_series(f, g) !== null = r38_compose_series(f, g)
fn compose_series(f, g, h) = compose_series(compose_series(f, g), h)
fn compose_series(f, g, h, k) = compose_series(compose_series(compose_series(f, g), h), k)

fn r38_invert_unit(s, b1, n1, y, c0) = {
  # s: the series of valuation 1 at 0 in its variable; returns the series of x in u = y - c0 to O(u^n1)
  let u = series_data(y, c0, [1], 1, n1)
  var xs = u/b1
  for i in range(n1) {
    xs = (u - total(map(k => r38_coef(s, k)*xs^k, range(2, n1 - 1))))/b1
  }
  xs
}
fn inverse_series(s) when r38_series(s) !== null = inverse_series(s, r38_series(s)[1])
fn inverse_series(s) when r38_series(s) === null and r38_puiseux(s) !== null = inverse_series(s, r38_puiseux(s)[1])
# r38_puiseux(s): a sum of terms c x^p (rational p) and one O(x^q) at 0 as [x, v, the series_data in t = x^(1/v)],
# v the common denominator of the exponents; null otherwise
fn r38_puiseux(s) =
  if is_atom(s) or not (head(s) === add) then null
  else {
    let ts = apply(list, s)
    let os = select(ts, t => not is_atom(t) and head(t) === O)
    let u = if length(os) == 1 then os[1][1] else null
    let o = if u === null then null else if is_symbol(u) then [u, 1] else if not is_atom(u) and head(u) === pow and is_symbol(u[1]) then [u[1], u[2]] else null
    if o === null or not is_rational(o[2]) then null else {
      let x = o[1]
      let rest = select(ts, t => not (t === os[1]))
      let ps = map(t => exponent(t, x), rest)
      let cs = map(i => cancel(rest[i]/x^ps[i]), range(length(rest)))
      if not every(ps, p => is_rational(p)) or not every(cs, c => free_of(c, x)) then null else {
        let v = apply(lcm, map(denominator, join(ps, [o[2]])))
        let ks = map(p => p*v, ps)
        let lo = min(join(ks, [o[2]*v]))
        let hi = o[2]*v
        [x, v, series_data(r38_t, 0, map(k => total(map(i => if ks[i] == k then cs[i] else 0, range(length(ks)))), range(lo, hi - 1)), lo, hi)]
      }
    }
  }
fn r38_inverse_series(s, y) = {
  let ss = r38_series(s)
  let pu = if ss === null then r38_puiseux(s) else null
  if pu !== null then {
    # a Puiseux series in t = x^(1/v): invert in t, then x = t^v in the series arithmetic
    let ti = r38_inverse_series(pu[3], y)
    if ti !== null and not is_atom(ti) and head(ti) === series_data then ti^pu[2] else null
  }
  else if ss === null or ss[4] < 0 then null else {
    let x0 = ss[2]
    let c0 = r38_coef(ss, 0)
    let n1 = ss[5]
    let v = r38_valuation(ss, 1)
    if v === null then null
    else if v == 1 then x0 + r38_invert_unit(ss, r38_coef(ss, 1), n1, y, c0)
    else {
      # w = (x - x0) ((s - c0)/(b_v (x - x0)^v))^(1/v) has valuation 1 and is known to O((x - x0)^(n1 - v + 1))
      let bv = r38_coef(ss, v)
      let q = series_data(ss[1], x0, map(k => r38_coef(ss, k)/bv, range(v, n1 - 1)), 0, n1 - v)
      let w = series_data(ss[1], x0, [1], 1, n1 - v + 1)*q^(1/v)
      let ws = r38_series(w)
      if ws === null then null else {
        let m = n1 - v + 1
        let t = r38_invert_unit(ws, r38_coef(ws, 1), m, r38_t, 0)
        let tv = ((y - c0)/bv)^(1/v)
        x0 + total(map(k => r38_coef(t, k)*tv^k, range(1, m - 1))) + O((y - c0)^(m/v))
      }
    }
  }
}
fn inverse_series(s, y) when is_symbol(y) and r38_inverse_series(s, y) !== null = r38_inverse_series(s, y)

# possible_zero_q(e): whether e is probably zero: true when simplify gives 0; otherwise e is evaluated to 40 digits
# at its symbols set to three complex sample points, a value that cannot be told from 0 counting as zero
fn calc_syms(e) = if is_symbol(e) then (if is_numeric(e) then [] else [e]) else if is_atom(e) then [] else unique(flat_map(calc_syms, apply(list, e)))
fn calc_zero_at(e) =
  if not is_numeric(n(e)) then false
  else if abs(n(e)) > 10^-8 then false
  else if is_failure(n(e, 40)) then true
  else abs(n(e, 40)) < 10^-30
fn possible_zero_q(e; assumptions = automatic) = {
  let s = if assumptions === automatic then simplify(e) else simplify(e, assumptions)
  if s === 0 then true else {
    let vs = calc_syms(s)
    let pts = [3/7 + 2*im/11, 5/13 - 7*im/17, -2/3 + 3*im/19]
    if length(vs) == 0 then calc_zero_at(s)
    else every(range(3), k => calc_zero_at(rewrite(s, map(j => vs[j] -> pts[k] + j/23, range(length(vs))))))
  }
}

# --- arithmetic on the sides of equations and inequalities: add_sides, subtract_sides, multiply_sides,
# divide_sides ---

# calc_side_map(e, g): g applied to the relations in e: through and, or, the values of a piecewise and the value of
# a conditional_expression; true and false stay
fn calc_side_map(e, g) =
  if e === true or e === false then e
  else if head(e) === `and` or head(e) === `or` then apply(head(e), map(t => calc_side_map(t, g), apply(list, e)))
  else if head(e) === piecewise then {
    let ps = map(pc => [calc_side_map(pc[1], g), pc[2]], e[1])
    let dflt = calc_side_map(if length(e) > 1 then e[2] else 0, g)
    apply(piecewise, [ps, dflt])
  }
  else if head(e) === conditional_expression then conditional_expression(calc_side_map(e[1], g), e[2])
  else if calc_rel(e) then g(e)
  else e
fn calc_side_arg(e) = e === true or e === false or calc_rel(e) or some([`and`, `or`, piecewise, conditional_expression], h => head(e) === h)

# calc_rel(e): whether e is an equation, an inequation or an inequality (a chain of several sides included)
fn calc_rel(e) = some([equal, unequal, less, greater, less_equal, greater_equal], h => head(e) === h)

# calc_flip(h): the relation with its sides exchanged (for a negative factor)
fn calc_flip(h) = if h === less then greater else if h === greater then less else if h === less_equal then greater_equal else if h === greater_equal then less_equal else h

# calc_sides(e, f): the relation e with f applied to every side
fn calc_sides(e, f) = apply(head(e), map(f, apply(list, e)))

# calc_sign_of(c, assm): 1 or -1 when the sign of c is decided (under the assumptions), else 0
fn calc_sign_of(c, assm) = {
  let pos = if assm === automatic then simplify(c > 0) else simplify(c > 0, assm)
  let neg = if assm === automatic then simplify(c < 0) else simplify(c < 0, assm)
  if pos === true then 1 else if neg === true then -1 else 0
}

# add_sides(rel, c): c added to every side; add_sides(a == b, rel): a and b added to the sides of rel
fn calc_add_sides(e, c) =
  if head(e) === equal and calc_rel(c) and length(e) == 2 and length(c) == 2 then apply(head(c), [e[1] + c[1], e[2] + c[2]])
  else if head(c) === equal and calc_rel(c) and length(c) == 2 and length(e) == 2 then apply(head(e), [e[1] + c[1], e[2] + c[2]])
  else calc_sides(e, s => s + c)

# subtract_sides(rel, c): c subtracted from every side; subtract_sides(rel): the right side subtracted, rel(lhs - rhs, 0)
fn calc_subtract_sides1(e) = if length(e) == 2 then apply(head(e), [e[1] - e[2], 0]) else e
# an equation minus an inequality c1 > c2 crosses the sides, a - c2 > b - c1, so the direction stays that of c (at
# a == b it is c itself)
fn calc_subtract_sides(e, c) =
  if head(c) === equal and length(c) == 2 and length(e) == 2 then apply(head(e), [e[1] - c[1], e[2] - c[2]])
  else if head(e) === equal and calc_rel(c) and length(c) == 2 and length(e) == 2 then apply(head(c), [e[1] - c[2], e[2] - c[1]])
  else calc_sides(e, s => s - c)

# calc_scale(e, c, assm, times): the sides of e multiplied (times true) or divided by c: an inequality reverses for a
# negative c; for a c of undecided sign the result is a piecewise over the signs (c != 0 for an equation, which is
# kept as it is at c = 0)
fn calc_scale(e, c, assm, times) = {
  let f = if times then (s => s*c) else (s => s/c)
  let ineq = some([less, greater, less_equal, greater_equal], h => head(e) === h)
  let sg = calc_sign_of(c, assm)
  if not ineq then {
    let nz = if assm === automatic then simplify(c != 0) else simplify(c != 0, assm)
    let z = if assm === automatic then simplify(c == 0) else simplify(c == 0, assm)
    if nz === true or z === false or sg != 0 then calc_sides(e, f) else piecewise([[calc_sides(e, f), c != 0]], e)
  }
  else if sg == 1 then calc_sides(e, f)
  else if sg == -1 then apply(calc_flip(head(e)), map(f, apply(list, e)))
  else piecewise([[calc_sides(e, f), c > 0], [apply(calc_flip(head(e)), map(f, apply(list, e))), c < 0]], e)
}

# multiply_sides(rel, c), divide_sides(rel, c): every side multiplied or divided by c; with an equation a == b as c,
# by a and b side by side; divide_sides(rel): divided by the right side; assumptions: decide the sign of c
# generate_conditions: (default: the piecewise over the signs above) false: the sides scaled as for a positive c;
# true: the result for c > 0 (an inequality) or c != 0 (an equation) as a conditional_expression; automatic: the
# condition c > 0 for an inequality only. A sign decided under the assumptions needs no condition.
fn calc_scale_gc(e, c, assm, times, gc) = if gc === calc_no_gc then calc_scale(e, c, assm, times) else {
  let f = if times then (s => s*c) else (s => s/c)
  let ineq = some([less, greater, less_equal, greater_equal], h => head(e) === h)
  let sg = calc_sign_of(c, assm)
  let nz = if assm === automatic then simplify(c != 0) else simplify(c != 0, assm)
  if sg == 1 or (not ineq and (sg == -1 or nz === true)) then calc_sides(e, f)
  else if ineq and sg == -1 then apply(calc_flip(head(e)), map(f, apply(list, e)))
  else if gc === false then calc_sides(e, f)
  else if ineq then conditional_expression(calc_sides(e, f), c > 0)
  else if gc === true then conditional_expression(calc_sides(e, f), c != 0)
  else calc_sides(e, f)
}
fn calc_multiply_sides(e, c, assumptions, gc) =
  if head(c) === equal and length(c) == 2 and length(e) == 2 then apply(head(e), [e[1]*c[1], e[2]*c[2]]) else calc_scale_gc(e, c, assumptions, true, gc)
fn calc_divide_sides1(e, assumptions, gc) = calc_scale_gc(e, last(apply(list, e)), assumptions, false, gc)
fn calc_divide_sides(e, c, assumptions, gc) = {
  if head(c) === equal and length(c) == 2 and length(e) == 2 then {
    let r = apply(head(e), [e[1]/c[1], e[2]/c[2]])
    if calc_sign_of(c[1], assumptions) != 0 or calc_sign_of(c[2], assumptions) != 0 then r else piecewise([[r, c[1] != 0]], e)
  } else calc_scale_gc(e, c, assumptions, false, gc)
}
fn add_sides(e, c) when calc_side_arg(e) = calc_side_map(e, r => calc_add_sides(r, c))
fn subtract_sides(e) when calc_side_arg(e) = calc_side_map(e, calc_subtract_sides1)
fn subtract_sides(e, c) when calc_side_arg(e) = calc_side_map(e, r => calc_subtract_sides(r, c))
fn multiply_sides(e, c; assumptions = automatic, generate_conditions = calc_no_gc) when calc_side_arg(e) = calc_side_map(e, r => calc_multiply_sides(r, c, assumptions, generate_conditions))
fn divide_sides(e; assumptions = automatic, generate_conditions = calc_no_gc) when calc_side_arg(e) = calc_side_map(e, r => calc_divide_sides1(r, assumptions, generate_conditions))
fn divide_sides(e, c; assumptions = automatic, generate_conditions = calc_no_gc) when calc_side_arg(e) = calc_side_map(e, r => calc_divide_sides(r, c, assumptions, generate_conditions))

# --- repeats in sequences: find_repeat, find_transient_repeat ---

# calc_periodic(xs, p): whether xs[i] === xs[i + p] wherever both exist
fn calc_periodic(xs, p) = every(range(length(xs) - p), i => xs[i] === xs[i + p])

# calc_repeat_list(xs, k): the shortest block that xs repeats (a partial last copy allowed) at least k times, or
# null
fn calc_repeat_list(xs, k) = {
  let m = length(xs)
  let ps = select(range(m), p => m >= k*p and calc_periodic(xs, p))
  if length(ps) == 0 then null else take(xs, ps[1])
}

# find_repeat(xs), (xs, k), (xs, [k1, k2, ...]): the shortest block that the list, string or array xs repeats along
# each of its first levels (all levels, 1.5 copies each, when no k is given; a list k1, k2, ... names the levels and the
# copies), a partial last copy allowed; [] (the empty string) when some level has none. The period of level d is the
# least p with extent >= k_d p such that every slice along d equals the slice p further on. The option same_test: f
# replaces === for comparing the elements at the deepest named level.
fn calc_fr_slice(v, d, i) = if d == 1 then v[i] else map(r => calc_fr_slice(r, d - 1, i), v)
fn calc_fr_eq(a, b, depth, test) =
  if depth == 0 then test(a, b)
  else (is_list(a) and is_list(b) and length(a) == length(b) and every(range(length(a)), j => calc_fr_eq(a[j], b[j], depth - 1, test)))
fn calc_fr_extent(v, d) = if d == 1 then length(v) else calc_fr_extent(v[1], d - 1)
fn calc_fr_period(v, d, depth, k, test) = {
  let m = calc_fr_extent(v, d)
  let ps = select(range(m), p => m >= k*p and every(range(m - p), i => calc_fr_eq(calc_fr_slice(v, d, i), calc_fr_slice(v, d, i + p), depth - 1, test)))
  if length(ps) == 0 then null else ps[1]
}
fn calc_fr_block(v, ks, test) = {
  let depth = length(ks)
  let ps = map(d => calc_fr_period(v, d, depth, ks[d], test), range(depth))
  if some(ps, p => p === null) then [] else apply(take, join([v], ps))
}
fn calc_fr_rank(v) = if is_list(v) and length(v) > 0 then 1 + calc_fr_rank(v[1]) else 0
fn calc_find_repeat(xs, k, test) = {
  let ks = if is_list(k) then k else map(i => if k === automatic then 1.5 else k, range(max(1, calc_fr_rank(xs))))
  if length(xs) == 0 then [] else calc_fr_block(xs, ks, if test === automatic then ((a, b) => a === b) else test)
}
fn find_repeat(xs; same_test = automatic) = find_repeat(xs, automatic, same_test: same_test)
fn find_repeat(xs, k; same_test = automatic) = {
  if is_string(xs) then string_join(calc_find_repeat(characters(xs), k, same_test))
  else calc_find_repeat(xs, k, same_test)
}

# find_transient_repeat(xs, k): [transient, block], the shortest transient after which xs repeats the shortest
# block at least k times (a partial last copy allowed); [xs, []] when there is none
fn calc_transient(xs, k) = {
  var res = null
  for t in range(0, length(xs) - 1) {
    if res === null then {
      let r = calc_repeat_list(drop(xs, t), k)
      if not (r === null) then res = [take(xs, t), r]
    }
  }
  if res === null then [xs, []] else res
}
fn find_transient_repeat(xs, k) =
  if is_string(xs) then map(string_join, calc_transient(characters(xs), k)) else calc_transient(xs, k)

# --- series of algebraic functions given as roots: series(root(f, k), a, a0, n) ---

# f = &(P(&1, a)), P polynomial in &1 with coefficients polynomial in a. When P(y, a0) has only simple roots (its
# discriminant is not 0), the k-th root y(a) near a0 stays the continuation of the k-th root y0 at a0 (the roots
# neither meet nor change between real and complex), and it is analytic there (the implicit function theorem); its
# series is found by Newton's iteration y <- y - P(y)/P_y(y) on truncated series, which doubles the number of correct
# terms at each step (Brent and Kung, J. ACM 25 (1978)).
fn calc_root_series(f, k, a, a0, n) = {
  let p = f(calc_rs_y)
  let p0 = rewrite(p, a -> a0)
  # (a root free of a is a constant: the series engine takes it so, and this rule would call itself on it)
  let ok = not free_of(p, a) and polynomial_expression_q(p, [calc_rs_y, a]) and exponent(p0, calc_rs_y) >= 1 and not (discriminant(p0, calc_rs_y) === 0) and is_numeric(discriminant(p0, calc_rs_y))
  if not ok then null else {
    # the root at a0 in radicals (a root object in the arithmetic of the series is not simplified)
    let y0 = to_radicals(root(apply(`fn`, [[`&1`], rewrite(p0, calc_rs_y -> `&1`)]), k))
    let py = d(p, calc_rs_y)
    if not is_numeric(y0) or not free_of(y0, root) or simplify(rewrite(py, [calc_rs_y -> y0, a -> a0])) === 0 then null else {
      var y = y0
      for i in range(ceiling(log(2, n + 1)) + 2) {
        y = normal(series(y - rewrite(p, calc_rs_y -> y)/rewrite(py, calc_rs_y -> y), a, a0, n))
      }
      series(y, a, a0, n)
    }
  }
}
rule series(r_, a_, a0_, n_) := calc_root_series(r[1], r[2], a, a0, n) when not is_atom(r) and head(r) === root and length(r) == 2 and is_integer(r[2]) and is_symbol(a) and is_integer(n) and n >= 0 and is_numeric(a0) and not (calc_root_series(r[1], r[2], a, a0, n) === null)

# --- integrals of Bessel functions of odd integer order ---

# J_(n + 1) = J_(n - 1) - 2 J_n' (DLMF 10.6.1) gives, from integrate(J_1) = -J_0, integrate(J_n(u), u) = -J_0 - 2 (J_2 +
# J_4 + ... + J_(n - 1)) for odd n > 0, written with J_0 = (2/u) J_1 - J_2 (DLMF 10.6.1 again) for n >= 3:
# -(2/u) J_1 + J_2 - 2 (J_2 + ... + J_(n - 1)) (for n = 3: -J_2 - (2/u) J_1, which is 1 - J_2 - (2/u) J_1 + const);
# J_(-n) = -J_n for odd n. The term c bessel_j(n, a x + b) with c, a, b free of x, a != 0.
fn calc_bj_anti(n, u) = if n == 1 then -bessel_j(0, u) else -2*bessel_j(1, u)/u + bessel_j(2, u) - 2*calc_total(map(j => bessel_j(2*j, u), range(1, (n - 1)/2)))
fn calc_bj_term(f, x) = {
  let fs = calc_factors(f)
  let js = select(fs, g => not is_atom(g) and head(g) === bessel_j and length(g) == 2 and is_integer(g[1]) and mod(g[1], 2) == 1 and not free_of(g, x))
  let c = product(select(fs, g => not some(js, u => u === g)))
  let ab = if length(js) == 1 then calc_lin(js[1][2], x) else null
  if ab === null or not free_of(c, x) or ab[1] === 0 then null else {
    let n = abs(js[1][1])
    let sg = if js[1][1] < 0 then -1 else 1
    [c*sg/ab[1], n, js[1][2]]
  }
}
rule integrate(f_, x_) := (calc_bj_term(f, x)[1]*calc_bj_anti(calc_bj_term(f, x)[2], calc_bj_term(f, x)[3])) when is_symbol(x) and not (calc_bj_term(f, x) === null)
fn calc_bj_def(f, s) = {
  let t = calc_bj_term(f, s[1])
  if t === null then null else {
    let big = t[1]*calc_bj_anti(t[2], t[3])
    let hi = limit(big, s[1] -> s[3])
    let lo = limit(big, s[1] -> s[2])
    if calc_held_limit(hi) or calc_held_limit(lo) or is_failure(hi) or is_failure(lo) then null else simplify(hi - lo)
  }
}
rule integrate(f_, s_) := calc_bj_def(f, s) when is_list(s) and length(s) == 3 and is_symbol(s[1]) and not (calc_bj_def(f, s) === null)

# --- Mellin transforms of g(x) cut to an interval: unit_step(x - a) g(x), piecewise([[g(x), lo < x and x < hi]]) ---

# g = c x^m exp(-k x^q) (q > 0, k > 0, or k = 0): integrate(x^(s - 1) g, [x, lo, hi]) for 0 <= lo < hi <= inf is
# (c/q) k^(-(s + m)/q) (gamma((s + m)/q, k lo^q) - gamma((s + m)/q, k hi^q)) (DLMF 8.2.2, the substitution u = k x^q),
# gamma(w, 0) = gamma(w) and gamma(w, inf) = 0; for k = 0 it is c (hi^(s + m) - lo^(s + m))/(s + m) (hi finite).
# [c, m, k, q] or null
fn calc_mc_shape(g, x) = {
  let lg = simplify(x*d(g, x)/g)
  let c1 = simplify(d(lg, x))
  if free_of(lg, x) then [simplify(g/x^lg), lg, 0, 1] else {
    let q = simplify(1 + x*d(c1, x)/c1)
    let k = simplify(-c1/(q^2*x^(q - 1)))
    let m = simplify(lg + k*q*x^q)
    let c = simplify(g/(x^m*exp(-k*x^q)))
    if every([q, k, m, c], v => free_of(v, x)) and is_numeric(q) and q > 0 then [c, m, k, q] else null
  }
}
fn calc_mc_cut(g, x, s, lo, hi) = {
  let sh = calc_mc_shape(g, x)
  if sh === null then null else {
    let w = (s + sh[2])/sh[4]
    if sh[3] === 0 then (if hi === inf then null else sh[1]*(hi^(s + sh[2]) - (if lo === 0 then 0 else lo^(s + sh[2])))/(s + sh[2]))
    else if not (refine(sh[3] > 0) === true) then null else {
      let up = if hi === inf then 0 else gamma(w, sh[3]*hi^sh[4])
      let down = if lo === 0 then gamma(w) else gamma(w, sh[3]*lo^sh[4])
      simplify(sh[1]/sh[4]*sh[3]^(-w)*(down - up))
    }
  }
}
# f = unit_step(x - a) g(x) (a free of x): the cut [a, inf) for a > 0, the whole transform for a <= 0, and the
# piecewise of the two when the sign of a is not known; f = piecewise([[g, lo < x and x < hi]]) with numbers
# 0 <= lo < hi
fn calc_mellin_cut(f, x, s) = {
  let fs = calc_factors(f)
  let st = select(fs, u => not is_atom(u) and head(u) === unit_step and length(u) == 1 and not free_of(u, x) and not (calc_lin(u[1], x) === null) and calc_lin(u[1], x)[1] === 1)
  if length(st) == 1 then {
    let a = -calc_lin(st[1][1], x)[2]
    let g = product(select(fs, u => not (u === st[1])))
    let cut = calc_mc_cut(g, x, s, a, inf)
    let whole = calc_mc_cut(g, x, s, 0, inf)
    if not free_of(g, unit_step) or cut === null or whole === null then null
    else if refine(a > 0) === true then cut
    else if refine(a <= 0) === true then whole
    else piecewise([[whole, a <= 0]], cut)
  } else if length(fs) == 1 and not is_atom(f) and head(f) === piecewise and length(f) == 1 and length(f[1]) == 1 then {
    let g = f[1][1][1]
    let cd = f[1][1][2]
    let parts = calc_conj_list(cd)
    let lo = select(parts, p => head(p) === less and p[2] === x)
    let hi = select(parts, p => head(p) === less and p[1] === x)
    if length(parts) == 2 and length(lo) == 1 and length(hi) == 1 and is_numeric(lo[1][1]) and is_numeric(hi[1][2]) and lo[1][1] >= 0 and lo[1][1] < hi[1][2] then calc_mc_cut(g, x, s, lo[1][1], hi[1][2]) else null
  } else null
}
rule mellin_transform(f_, x_, s_) := calc_mellin_cut(f, x, s) when is_symbol(x) and is_symbol(s) and not (calc_mellin_cut(f, x, s) === null)

# --- the unit mass at a symbolic point: integrals and transforms of g(x) dirac_delta(c (x - a)) ---

# [g, a, w] for f = g(x) dirac_delta(u) with u = c x + d linear in x (c != 0 free of x), a = -d/c the point where u
# vanishes and w = 1/abs(c) its weight (the scaling rule of the delta); null for another f. The point a is a real
# number: a shift known not to be real gives null.
fn calc_mass(f, x) = {
  let fs = calc_factors(f)
  let ds = select(fs, g => not is_atom(g) and head(g) === dirac_delta and length(g) == 1 and not free_of(g, x))
  let g = product(select(fs, h => not some(ds, u => u === h)))
  let ab = if length(ds) == 1 then calc_lin(ds[1][1], x) else null
  if ab === null or ab[1] === 0 or not free_of(g, dirac_delta) then null else {
    let a = simplify(-ab[2]/ab[1])
    if refine(element(a, reals)) === false then null else [g, a, 1/abs(ab[1])]
  }
}
# integrate(g(x) dirac_delta(x - a), [x, lo, hi]) = g(a) heaviside_theta(a - lo) heaviside_theta(hi - a), the
# factor of an infinite end left out
fn calc_mass_int(f, s) = {
  let m = calc_mass(f, s[1])
  if m === null then null else {
    let lo = if s[2] === -inf then 1 else heaviside_theta(m[2] - s[2])
    let hi = if s[3] === inf then 1 else heaviside_theta(s[3] - m[2])
    simplify(m[3]*rewrite(m[1], s[1] -> m[2])*lo*hi)
  }
}
rule integrate(f_, s_) := calc_mass_int(f, s) when is_list(s) and length(s) == 3 and is_symbol(s[1]) and (s[2] === -inf or s[3] === inf or refine(s[2] < s[3]) === true) and not (calc_mass_int(f, s) === null)
# the transforms with the kernels exp(-t x) on [0, inf) and exp(i t x)/sqrt(2 pi) on the line
rule laplace_transform(f_, x_, t_) := calc_mass_int(f*exp(-t*x), [x, 0, inf]) when is_symbol(x) and not (calc_mass(f, x) === null)
rule fourier_transform(f_, x_, t_) := calc_mass_int(f*exp(im*t*x), [x, -inf, inf])/sqrt(2*pi) when is_symbol(x) and not (calc_mass(f, x) === null)

# --- truncate_sum, convolve, dt, inverse_function ---

# truncate_sum(e, k): every sum(f, [i, a, b]) and inactive(sum)(f, ...) in e cut to its first k terms, the index
# running from a (a, ..., a + k - 1); from -inf the last k terms (b - k + 1, ..., b), over the whole line the terms
# -k, ..., k; a finite range is cut only when it has more than 100 terms. A list k = [k1, k2, ...] names the ranges
# of a multiple sum in order (all keeps one whole). The sum is activated (added when k is an integer) unless
# activate_result: false; working_precision: d gives the result at d digits.
fn calc_ts_spec(sp, k) = {
  if not is_list(sp) or k === all then sp
  else {
    let i = sp[1]
    let a = if length(sp) >= 3 then sp[2] else 1
    let b = if length(sp) >= 3 then sp[3] else sp[2]
    if is_integer(a) and is_integer(b) then {
      if b - a + 1 > 100 then (if a == -b then [i, -k, k] else [i, a, if a == 1 then k else a + k - 1]) else sp
    }
    else if a === -inf and not is_integer(b) and b === inf then [i, -k, k]
    else if a === -inf then [i, b - k + 1, b]
    else [i, a, if a == 1 then k else a + k - 1]
  }
}
fn calc_ts_node(e, kl, activate) = {
  let args = apply(list, e)
  let specs = rest(args)
  let nsp = map(j => calc_ts_spec(specs[j], if is_list(kl) then (if j <= length(kl) then kl[j] else all) else kl), range(length(specs)))
  if activate then apply(sum, join([args[1]], nsp)) else apply(inactive(sum), join([args[1]], nsp))
}
fn calc_ts_walk(e, kl, activate) =
  if is_atom(e) then e
  else if (head(e) === sum or head(e) === inactive(sum)) and length(e) >= 2 and is_list(e[2]) then calc_ts_node(e, kl, activate)
  else apply(head(e), map(t => calc_ts_walk(t, kl, activate), apply(list, e)))
fn truncate_sum(e, k; activate_result = true, working_precision = automatic) = {
  let r = calc_ts_walk(e, k, activate_result)
  if working_precision === automatic then r else (if is_integer(working_precision) then n(r, working_precision) else n(r))
}

# convolve(f, g, x, y): the convolution integrate(f(x) g(y - x), [x, -inf, inf])
# several variables: convolve(f, g, [x1, ...], [y1, ...]); options assumptions: and generate_conditions:
# cv_convolve(f, g, x, y): the convolution of two piecewise polynomials with rational interval bounds: on each interval between the
# sums of the bounds, the sum of the integrals over the overlaps of the pieces (null when f or g is not of that kind)
# bounds [lo, hi] of the interval a condition in x describes, or null
fn cv_bound(c, x) = {
  if head(c) === head(cb_a and cb_b) then {
    let bs = map(q => cv_bound(q, x), apply(list, c))
    if some(bs, b => b === null) then null else [apply(max, map(b => b[1], bs)), apply(min, map(b => b[2], bs))]
  }
  else if head(c) === less or head(c) === less_equal or head(c) === greater or head(c) === greater_equal then {
    let dd = c[1] - c[2]
    let a = simplify(d(dd, x))
    let b = simplify(rewrite(dd, x -> 0))
    if not is_rational(a) or not is_rational(b) or a === 0 then null
    else {
      let r = -b/a
      let up = (head(c) === less or head(c) === less_equal) == (a > 0)
      if up then [-inf, r] else [r, inf]
    }
  }
  else null
}
fn cv_pieces(f, x) = {
  # list of [poly, lo, hi] with finite rational bounds
  if head(f) === piecewise then {
    let cases = f[1]
    let ps = map(cs => { let bd = cv_bound(cs[2], x); if bd === null then null else [cs[1], bd[1], bd[2]] }, cases)
    if some(ps, p => p === null) or not (f[2] === 0) then null
    else {
      let live = select(ps, p => p[3] > p[2])
      if every(live, p => is_rational(p[2]) and is_rational(p[3]) and polynomial_expression_q(p[1], x)) and length(live) > 0 then live else null
    }
  } else null
}
fn cv_convolve(f, g, x, y) = {
  let pf = cv_pieces(f, x)
  let pg = cv_pieces(g, x)
  if pf === null or pg === null then null else {
    let t = cv_t
    var breaks = []
    for p in pf { for q in pg { breaks = join(breaks, [p[2] + q[2], p[2] + q[3], p[3] + q[2], p[3] + q[3]]) } }
    let bs = sort(unique(breaks))
    let cases = map(k => {
      let lo = bs[k]
      let hi = bs[k + 1]
      let ym = (lo + hi)/2
      var tot = 0
      for p in pf { for q in pg {
        let a = max(p[2], ym - q[3])
        let b = min(p[3], ym - q[2])
        if b > a then {
          let la = if p[2] >= ym - q[3] then p[2] else y - q[3]
          let ub = if p[3] <= ym - q[2] then p[3] else y - q[2]
          let inner = integrate(p[1]*rewrite(q[1], x -> y - x), [x, la, ub])
          tot = tot + inner
        }
      } }
      [expand(tot), lo < y and y < hi]
    }, range(length(bs) - 1))
    let kept = select(cases, c => not (c[1] === 0))
    piecewise(kept, 0)
  }
}

fn calc_convolve(f, g, xs, ys, gc) = {
  let pw = if not is_list(xs) and not is_list(ys) and gc === automatic then cv_convolve(if head(f) === piecewise then f else piecewise_expand(f), if head(g) === piecewise then g else piecewise_expand(g), xs, ys) else null
  if not (pw === null) then pw else calc_convolve2(f, g, xs, ys, gc)
}
fn calc_convolve2(f, g, xs, ys, gc) = {
  let xl = if is_list(xs) then xs else [xs]
  let yl = if is_list(ys) then ys else [ys]
  let rules = map(i => xl[i] -> yl[i] - xl[i], range(1, length(xl)))
  let ranges = map(v => [v, -inf, inf], xl)
  let body = f*rewrite(g, rules)
  simplify(if gc === automatic then apply(integrate, join([body], ranges)) else integrate(body, ranges[1], generate_conditions: gc))
}
fn convolve(f, g, x, y; assumptions = automatic, generate_conditions = automatic) =
  if assumptions === automatic then calc_convolve(f, g, x, y, generate_conditions) else assuming(assumptions, calc_convolve(f, g, x, y, generate_conditions))

# dt(e, x): the total derivative of e in x, every other symbol a function of x whose derivative is dt(s, x);
# dt(e): the total differential, the sum of the partial derivatives in each symbol times dt(s); dt(e, x1, x2, ...)
# in turn
fn calc_dt1(e, x, cs) = {
  let ss = select(calc_syms(e), s => not (s === x) and count(cs, s) == 0)
  let rules = join([dt(s_, [x, k_]) :> derivative(k)(calc_dtf(s))(x), dt(s_, x) :> derivative(1)(calc_dtf(s))(x)], map(s => s -> calc_dtf(s)(x), ss))
  let r = d(rewrite(e, rules), x)
  rewrite(r, [derivative(k_)(calc_dtf(s_))(t_) :> (if k == 1 then dt(s, t) else dt(s, [t, k])), calc_dtf(s_)(t_) :> s])
}
rule dt(s_, t_) := 1 when is_symbol(s) and s === t
# calc_nopat(e): whether e contains no pattern (rules of dt are built with dt(s_, x) on their left side)
fn calc_nopat(e) = if is_atom(e) then true else not (head(e) === any) and every(apply(list, e), calc_nopat)
fn dt(e; constants = []) when (not is_symbol(e) or is_numeric(e)) and calc_nopat(e) = if is_numeric(e) then 0 else expand(calc_total(map(s => d(e, s)*dt(s), select(calc_syms(e), s => count(constants, s) == 0))))
fn dt(e, specs...; constants = []) when length(specs) > 0 and (not is_symbol(e) or is_numeric(e)) and calc_nopat(e) =
  fold((g, s) => if is_list(s) then nest(t => calc_dt1(t, s[1], constants), g, s[2]) else calc_dt1(g, s, constants), e, specs)

# inverse_function(f): the inverse of a named elementary function (sin -> asin, exp -> log, ...), or of a function
# of one argument whose equation f(y) == x solve can invert (the first solution, its constants c(k) set to 0); otherwise it stays unevaluated
fn calc_inverse_table() = [sin -> asin, cos -> acos, tan -> atan, cot -> acot, sec -> asec, csc -> acsc,
  asin -> sin, acos -> cos, atan -> tan, acot -> cot, asec -> sec, acsc -> csc, sinh -> asinh, cosh -> acosh,
  tanh -> atanh, coth -> acoth, sech -> asech, csch -> acsch, asinh -> sinh, acosh -> cosh, atanh -> tanh,
  acoth -> coth, asech -> sech, acsch -> csch, exp -> log, log -> exp, sqrt -> (t => t^2), identity -> identity,
  product_log -> (t => t*exp(t)), erf -> inverse_erf, erfc -> inverse_erfc, inverse_erf -> erf, inverse_erfc -> erfc]
rule inverse_function(inverse_function(f_)) := f
# the derivative of the inverse of a named function: 1/f'(f^-1(x))
rule derivative(1)(inverse_function(f_))(x_) := 1/derivative(1)(f)(inverse_function(f)(x)) when is_symbol(f)
rule inverse_function(f_) := rewrite(f, calc_inverse_table()) when is_symbol(f) and some(calc_inverse_table(), r => r[1] === f)
rule inverse_function(f_) := {
  let sols = solve(f(calc_inv_y) == calc_inv_x, calc_inv_y)
  let v0 = rewrite(calc_inv_y, sols[1])
  let v = rewrite(if head(v0) === conditional_expression then v0[1] else v0, c(k_) :> 0)
  apply(`fn`, [[`&1`], rewrite(v, calc_inv_x -> `&1`)])
} when not is_symbol(f) and is_list(solve(f(calc_inv_y) == calc_inv_x, calc_inv_y)) and length(solve(f(calc_inv_y) == calc_inv_x, calc_inv_y)) > 0

# --- Hankel and Hilbert transforms ---

# calc_at(g, s): g(s) computed for a symbol in place of s and then evaluated at s (numbers take the closed form)
fn calc_at(g, s) = if is_symbol(s) and not is_numeric(s) then g(s) else rewrite(g(calc_at_s), calc_at_s -> s)

# hankel_transform(f, r, s), (f, r, s, nu): the Hankel transform integrate(f(r) bessel_j(nu, s r) r, [r, 0, inf])
# (nu = 0) from closed forms for the summands c r^mu, c r^mu exp(-a r), c r^mu exp(-a r^2), c r^nu (r^2 + a^2)^-p and
# c g(r) dirac_delta(r - b) (DLMF 10.22.43, 10.22.49, 10.22.51; Erdelyi et al., Tables of Integral Transforms II,
# ch. 8); a summand of another form leaves the transform unevaluated. inverse_hankel_transform(g, s, r, nu) is the
# same integral (the transform is its own inverse).
fn calc_hankel_term(f, r, s, nu) = {
  let lg = simplify(r*d(f, r)/f)
  let lg1 = simplify(d(lg, r))
  if free_of(lg, r) then {
    let mu = lg
    let c = simplify(f/r^mu)
    c*2^(mu + 1)*gamma((nu + mu + 2)/2)/gamma((nu - mu)/2)*s^(-mu - 2)
  } else if free_of(lg1, r) then {
    let a = -lg1
    let mu = simplify(lg + a*r)
    let c = simplify(f/(r^mu*exp(-a*r)))
    if free_of(mu, r) and free_of(c, r) then simplify(c*gamma(mu + nu + 2)*(s/2)^nu/(gamma(nu + 1)*a^(mu + nu + 2))*hypergeometric_2f1((mu + nu + 2)/2, (mu + nu + 3)/2, nu + 1, -s^2/a^2)) else null
  } else if free_of(simplify(lg1/r), r) then {
    let a = -simplify(lg1/r)/4
    let mu = simplify(lg + 2*a*r^2)
    let c = simplify(f/(r^mu*exp(-a*r^2)))
    if free_of(mu, r) and free_of(c, r) then simplify(c*gamma((mu + nu + 2)/2)*(s/2)^nu/(2*gamma(nu + 1)*a^((mu + nu + 2)/2))*hypergeometric_1f1((mu + nu + 2)/2, nu + 1, -s^2/(4*a))) else null
  } else {
    let fs = calc_factors(f)
    let dep = select(fs, g => not free_of(g, r))
    let c = product(select(fs, g => free_of(g, r)))
    let others = if nu === 0 then dep else select(dep, g => not (g === r^nu))
    let deltas = select(dep, g => head(g) === dirac_delta)
    if length(others) == 1 and length(dep) == (if nu === 0 then 1 else 2) and head(others[1]) === pow and free_of(expand(others[1][1] - r^2), r) and free_of(others[1][2], r) then {
      let a = sqrt(expand(others[1][1] - r^2))
      let p = -others[1][2]
      simplify(c*a^(nu + 1 - p)*s^(p - 1)*bessel_k(nu + 1 - p, a*s)/(2^(p - 1)*gamma(p)))
    } else if length(deltas) == 1 and free_of(expand(deltas[1][1] - r), r) then {
      let b = -expand(deltas[1][1] - r)
      let g2 = product(select(dep, g => not (head(g) === dirac_delta)))
      c*rewrite(g2, r -> b)*b*bessel_j(nu, s*b)
    } else null
  }
}
fn calc_hankel(f, r, s, nu) = {
  let ts = map(t => calc_hankel_term(t, r, s, nu), calc_terms(expand(f)))
  if some(ts, t => t === null) then calc_hankel_mb(f, r, s, nu) else simplify(calc_total(ts))
}

# --- transforms as inverse Mellin transforms of products (Mellin-Parseval) ---

# When f has the Mellin transform F(z) on the strip a < re(z) < b, the Hankel transform H(s) = integrate(f(r)
# bessel_j(nu, s r) r, [r, 0, inf]) has the Mellin transform M[H](w) = M[bessel_j(nu, t)](w) F(2 - w) on the strip
# max(-re(nu), 2 - b) < re(w) < min(3/2, 2 - a), and the Mellin convolution of f and g has the transform F(z) G(z) on
# the intersection of their strips (Titchmarsh 1948, sections 1.5 and 2.15; Paris and Kaminski 2001, section 3.1).
# The product is written with gamma functions only (sin(pi u) = pi/(gamma(u) gamma(1 - u)), cos(pi u) = pi/(gamma(1/2 +
# u) gamma(1/2 - u)), and their reciprocals) and inverted by inverse_mellin_transform on that strip. The residue series
# closed to the left is the function on all of x > 0 only when the product of the gamma factors gamma(a w + c)^e has
# decay exponent sum(e |a|) > 0 (the Mellin-Barnes integral converges absolutely and is analytic in a sector about the
# positive axis), or decay exponent 0 and sum(e a) > 0 (the case q > p of a Meijer G-function, whose series in x
# converges everywhere); otherwise (as for p = q, a function with a jump at x = 1) the transform declines.

# the strip [lo, hi, rest] of a Mellin transform's condition c in z (re(z) bounds; -inf, inf for an open side; rest the
# conjunction of the parts free of z), or null for a condition of another form
fn calc_mstrip(c, z) = {
  let cs = if not is_atom(c) and head(c) === `and` then apply(list, c) else [c]
  let t = calc_mstrip_t
  var lo = -inf
  var hi = inf
  var rest = true
  var ok = true
  for q in cs {
    if free_of(q, z) then rest = rest and q
    else if (head(q) === less or head(q) === greater) and length(q) == 2 then {
      let dd = if head(q) === less then q[1] - q[2] else q[2] - q[1]
      let e = expand(rewrite(dd, re(u_) :> calc_mstrip_re(u, z, t)))
      let k = coefficient(e, t, 1)
      let d = expand(e - k*t)
      if not free_of(e, z) or not free_of(e, calc_mstrip_bad) or not is_number(k) or k == 0 or not free_of(d, t) then ok = false
      else if k > 0 then hi = min(hi, -d/k)
      else lo = max(lo, -d/k)
    } else ok = false
  }
  if ok then [lo, hi, rest] else null
}
fn calc_mstrip_re(u, z, t) = {
  let k = coefficient(expand(u), z, 1)
  let d = expand(u - k*z)
  if is_number(k) and is_real(k) and free_of(d, z) then k*t + re(d) else calc_mstrip_bad
}

# e with sin, cos, csc, sec of pi (a w + c) written with gamma functions
fn calc_gamma_form(e, w) = rewrite(e, [
  sin(u_) :> (if free_of(u, w) then sin(u) else pi/(gamma(u/pi)*gamma(1 - u/pi))),
  cos(u_) :> (if free_of(u, w) then cos(u) else pi/(gamma(1/2 + u/pi)*gamma(1/2 - u/pi))),
  csc(u_) :> (if free_of(u, w) then csc(u) else gamma(u/pi)*gamma(1 - u/pi)/pi),
  sec(u_) :> (if free_of(u, w) then sec(u) else gamma(1/2 + u/pi)*gamma(1/2 - u/pi)/pi)])

# [sum(e |a|), sum(e a)] over the gamma factors gamma(a w + c)^e of the product g, or null when a factor in w is not a
# gamma power, a power c^w or rational in w
fn calc_mb_slope(u, w) = {
  let k = coefficient(expand(u), w, 1)
  if is_number(k) and is_real(k) and k != 0 and free_of(expand(u - k*w), w) then k else null
}
fn calc_mb_weight(g, w) =
  if free_of(g, w) then [0, 0]
  else if head(g) === gamma then (if calc_mb_slope(g[1], w) === null then null else [abs(calc_mb_slope(g[1], w)), calc_mb_slope(g[1], w)])
  else if head(g) === pow and free_of(g[1], w) then [0, 0]
  else if head(g) === pow and head(g[1]) === gamma and is_integer(g[2]) then (if calc_mb_slope(g[1][1], w) === null then null else [g[2]*abs(calc_mb_slope(g[1][1], w)), g[2]*calc_mb_slope(g[1][1], w)])
  else if head(g) === pow and is_integer(g[2]) and polynomial_expression_q(g[1], [w]) then [0, 0]
  else if polynomial_expression_q(g, [w]) then [0, 0]
  else null
fn calc_mb_closes_left(g, w) = {
  let ws = map(f => calc_mb_weight(f, w), calc_factors(g))
  if some(ws, p => p === null) then false else {
    let dl = calc_total(map(p => p[1], ws))
    let ka = calc_total(map(p => p[2], ws))
    dl > 0 or (dl == 0 and ka > 0)
  }
}

# the inverse Mellin transform of the product g in w at x on the strip lo < re(w) < hi, or null
fn calc_mb_invert(g, w, x, lo, hi) = {
  let g2 = expand_all(calc_gamma_form(g, w))
  let g3 = if head(g2) === add then simplify(g2) else g2
  if lo === -inf or not (simplify(lo < hi) === true) or not calc_mb_closes_left(g3, w) then null else {
    let v = inverse_mellin_transform(g3, w, x, assumptions: lo < re(w) and re(w) < hi)
    if free_of(v, inverse_mellin_transform) and free_of(v, mellin_transform) and free_of(v, sum) and not (head(v) === failure) then v else null
  }
}

# the Hankel transform of order nu by the Mellin product, or null
fn calc_hankel_mb(f, r, s, nu) = {
  let z = calc_mb_z
  let w = calc_mb_w
  let mf = mellin_transform(f, r, z, generate_conditions: true)
  let st = if head(mf) === conditional_expression then calc_mstrip(mf[2], z) else null
  if st === null then null else
    calc_mb_invert(2^(w - 1)*gamma((nu + w)/2)/gamma(1 + (nu - w)/2)*rewrite(mf[1], z -> 2 - w), w, s, max(-re(nu), 2 - st[2]), min(3/2, 2 - st[1]))
}

# the Mellin convolution integrate(f(x) g(y/x)/x, [x, 0, inf]) by the Mellin product, or null; with gc, the
# conditions of the two transforms free of the transform variable as conditional_expression
fn calc_mconv_mb(f, g, x, y, gc) = {
  let z = calc_mb_z
  let mf = mellin_transform(f, x, z, generate_conditions: true)
  let mg = mellin_transform(g, x, z, generate_conditions: true)
  let sf = if head(mf) === conditional_expression then calc_mstrip(mf[2], z) else null
  let sg = if head(mg) === conditional_expression then calc_mstrip(mg[2], z) else null
  if sf === null or sg === null then null else {
    let v = calc_at(u => calc_mb_invert(mf[1]*mg[1], z, u, max(sf[1], sg[1]), min(sf[2], sg[2])), y)
    let c = simplify(sf[3] and sg[3])
    if v === null then null else if gc and not (c === true) then conditional_expression(v, c) else v
  }
}
# options: assumptions: (the transform computed under them), generate_conditions: true (the convergence conditions
# re(a) > 0 for a decay exp(-a r), and s >= 0, attached as conditional_expression)
fn calc_hk_cond(f, x, p) = {
  let a = simplify(-limit(d(f, x)/f, x -> inf))
  if free_of(a, x) and not (a === 0) and not (head(a) === limit) then (re(a) > 0 and p >= 0) else p >= 0
}
fn calc_hk_run(f, x, p, nu, asm, gc) = {
  let v = if asm === automatic then calc_at(w => calc_hankel(f, x, w, nu), p) else assuming(asm, calc_at(w => calc_hankel(f, x, w, nu), p))
  if v === null or not gc then v else conditional_expression(v, calc_hk_cond(f, x, p))
}
fn hankel_transform(f, r, s; assumptions = automatic, generate_conditions = false) when not (calc_hk_run(f, r, s, 0, assumptions, false) === null) = calc_hk_run(f, r, s, 0, assumptions, generate_conditions)
fn hankel_transform(f, r, s, nu; assumptions = automatic, generate_conditions = false) when not (head(nu) === options) and not (calc_hk_run(f, r, s, nu, assumptions, false) === null) = calc_hk_run(f, r, s, nu, assumptions, generate_conditions)
fn inverse_hankel_transform(g, s, r; assumptions = automatic, generate_conditions = false) when not (calc_hk_run(g, s, r, 0, assumptions, false) === null) = calc_hk_run(g, s, r, 0, assumptions, generate_conditions)
fn inverse_hankel_transform(g, s, r, nu; assumptions = automatic, generate_conditions = false) when not (head(nu) === options) and not (calc_hk_run(g, s, r, nu, assumptions, false) === null) = calc_hk_run(g, s, r, nu, assumptions, generate_conditions)

# --- Fourier transforms of radial functions of two variables ---

# f(x, y) = h(r), r = sqrt(x^2 + y^2): with the default parameters the transform (1/(2 pi)) integrate(f exp(i (u x + v y)))
# over the plane is the Hankel transform of order 0 of h at rho = sqrt(u^2 + v^2) (the angular integral is 2 pi
# bessel_j(0, rho r); Stein and Weiss, Introduction to Fourier Analysis on Euclidean Spaces, ch. IV.1). The inverse
# is the same integral, and for an even f (a radial one) the cosine transform (2/pi) integrate over the quadrant of f
# cos(u x) cos(v y) equals it.
fn calc_radial(f, xy) = {
  let g = simplify(rewrite(f, [xy[1] -> calc_rad_r*cos(calc_rad_t), xy[2] -> calc_rad_r*sin(calc_rad_t)]), assumptions: calc_rad_r > 0)
  if free_of(g, calc_rad_t) and free_of(g, xy[1]) and free_of(g, xy[2]) then g else null
}
fn calc_fourier2(f, xy, uv) = {
  let h = calc_radial(f, xy)
  let ht = if h === null then null else hankel_transform(h, calc_rad_r, calc_rad_s)
  if h === null or head(ht) === hankel_transform or is_failure(ht) then null else simplify(rewrite(ht, calc_rad_s -> sqrt(uv[1]^2 + uv[2]^2)))
}
fn calc_two_syms(v) = is_list(v) and length(v) == 2 and every(v, is_symbol)
rule fourier_transform(f_, xy_, uv_) := calc_fourier2(f, xy, uv) when calc_two_syms(xy) and calc_two_syms(uv) and not (calc_fourier2(f, xy, uv) === null)
rule inverse_fourier_transform(f_, xy_, uv_) := calc_fourier2(f, xy, uv) when calc_two_syms(xy) and calc_two_syms(uv) and not (calc_fourier2(f, xy, uv) === null)
rule fourier_cos_transform(f_, xy_, uv_) := calc_fourier2(f, xy, uv) when calc_two_syms(xy) and calc_two_syms(uv) and not (calc_fourier2(f, xy, uv) === null)
rule inverse_fourier_cos_transform(f_, xy_, uv_) := calc_fourier2(f, xy, uv) when calc_two_syms(xy) and calc_two_syms(uv) and not (calc_fourier2(f, xy, uv) === null)

# hilbert_transform(f, t, s): the Hilbert transform (1/pi) PV integrate(f(t)/(s - t), [t, -inf, inf]), by linearity
# from H(sin(a t + b)) = -sign(a) cos(a s + b), H(cos(a t + b)) = sign(a) sin(a s + b), H(exp(im (a t + b))) =
# -im sign(a) exp(im (a s + b)), H(dirac_delta(t)) = 1/(pi s), H(constant) = 0, and for a rational function
# vanishing at infinity H((t - z)^-k) = im sign(im(z)) (s - z)^-k on its partial fractions over the complex numbers
# (a function analytic in the lower half plane is multiplied by im, one analytic in the upper by -im: King, Hilbert
# Transforms I, ch. 4); inverse_hilbert_transform(g, s, t) is -hilbert_transform(g, s, t)
fn calc_lin(e, t) = {
  let a = simplify(d(e, t))
  if free_of(a, t) then [a, simplify(e - a*t)] else null
}
fn calc_hilbert_term(f, t, s) = {
  let fs = calc_factors(f)
  let c = product(select(fs, g => free_of(g, t)))
  let dep = select(fs, g => not free_of(g, t))
  if length(dep) == 0 then 0
  else if length(dep) == 1 and (some([sin, cos, exp, dirac_delta], h => head(dep[1]) === h) or head(dep[1]) === pow and dep[1][1] === e) then {
    let g = dep[1]
    let isexp = head(g) === exp or head(g) === pow
    let arg = if head(g) === pow then g[2] else g[1]
    let ab = if isexp then calc_lin(simplify(arg/im), t) else calc_lin(arg, t)
    if ab === null then null
    else if head(g) === dirac_delta then c/(pi*abs(ab[1])*(s + ab[2]/ab[1]))
    else {
      let sg = sign(ab[1])
      if not is_integer(sg) then null
      else if head(g) === sin then -c*sg*cos(ab[1]*s + ab[2])
      else if head(g) === cos then c*sg*sin(ab[1]*s + ab[2])
      else -im*c*sg*exp(im*(ab[1]*s + ab[2]))
    }
  }
  else null
}
fn calc_is_proper_rational(f, t) = {
  let r = together(f)
  polynomial_expression_q(numerator(r), t) and polynomial_expression_q(denominator(r), t) and degree(numerator(r), t) < degree(denominator(r), t)
}
fn calc_hilbert_rational(f, t, s) = {
  let vs = map(term => {
    let fs = calc_factors(term)
    let c = product(select(fs, g => free_of(g, t)))
    let dep = select(fs, g => not free_of(g, t))
    let base = if length(dep) == 1 then (if head(dep[1]) === pow then dep[1][1] else dep[1]) else null
    let k = if length(dep) == 1 and head(dep[1]) === pow then dep[1][2] else 1
    let z = if base === null then null else simplify(t - base)
    if base === null or not free_of(z, t) or not is_integer(k) or k >= 0 then null
    else {
      let sg = sign(im_part(n(z)))
      if not is_integer(sg) or sg == 0 then null else im*sg*c*(s - z)^k
    }
  }, calc_terms(expand(calc_pf_complex(f, t))))
  if some(vs, v => v === null) then null else simplify(calc_total(vs))
}
fn calc_hilbert_old(f, t, s) = {
  let ts = map(term => calc_hilbert_term(term, t, s), calc_terms(expand(f)))
  if not some(ts, v => v === null) then simplify(calc_total(ts))
  else if calc_is_proper_rational(f, t) then calc_hilbert_rational(f, t, s)
  else null
}

# The general method. Convention H f(s) = (1/pi) PV integrate(f(t)/(s - t), t), so H cos = sin,
# H(f') = (H f)' and H H = -1 (King, Hilbert Transforms I, ch. 3-4).
# 1. A sum of terms exp(im w t + b) R(t), R rational and w real of a known sign (sines and cosines of linear
#    arguments written as exponentials). A function analytic and bounded in the upper half plane has H g = -im g, in
#    the lower H g = im g. So for w != 0, with sigma = sign(w): exp(im w t) P(t) (P a polynomial) has the transform
#    -im sigma exp(im w s) P(s), and exp(im w t)/(t - z)^k = (exp(im w t) - T(t))/(t - z)^k + T(t)/(t - z)^k with T the
#    Taylor polynomial of exp(im w t) at z of degree k - 1: the first part is entire and bounded on the half plane where
#    exp(im w t) is (transform -im sigma times it), the second is rational.
# 2. A rational function vanishing at infinity, on its partial fractions over the complex numbers (multiple poles
#    included): (t - z)^-k gives im sign(im(z)) (s - z)^-k off the real line and, for a real pole (principal value
#    and distribution), -pi (-1)^(k-1)/(k-1)! dirac_delta^(k-1)(s - z) (from H(1/t) = -pi dirac_delta(s), the inverse
#    of H(dirac_delta) = 1/(pi s), and H(f') = (H f)'). A constant has the transform 0; another polynomial part none.
# 3. A piecewise polynomial of compact support (unit_box, sign, unit_step, piecewise): on each piece [a, b] with
#    polynomial p, (1/pi) (integrate((p(t) - p(s))/(s - t), [t, a, b]) + p(s) log(abs((s - a)/(s - b)))).
# The signs of w and im(z) are decided by refine under the assumptions; an undecided sign leaves the transform.
fn calc_hb_sign(e, asm) = {
  let v = if asm === automatic then refine(sign(simplify(e))) else assuming(asm, refine(sign(simplify(e))))
  if is_integer(v) then v else null
}
# [re(z), im(z)] through sums and numeric multiples: im(c u) = re(c) im(u) + im(c) re(u) (z = im a gives re(a))
fn calc_hb_reim(z) =
  if is_number(z) then [re(z), im_part(z)]
  else if head(z) === add then fold((acc, u) => { let p = calc_hb_reim(u); [acc[1] + p[1], acc[2] + p[2]] }, [0, 0], apply(list, z))
  else if head(z) === mul and is_number(calc_num_part(z)) and not (calc_num_part(z) === 1) then {
    let c = calc_num_part(z)
    let p = calc_hb_reim(z/c)
    [re(c)*p[1] - im_part(c)*p[2], re(c)*p[2] + im_part(c)*p[1]]
  }
  else [re(z), im_part(z)]
# the Laurent coefficients [j, c_j] of (t - z)^-j, j = 1..m, of rm/den at the root z of order m of den, with
# den = cofactor p^m and z a simple root of p: rm/den = h/(t - z)^m with h = rm/(cofactor (p/(t - z))^m)
fn calc_hb_laurent(rm, cof, p, m, t, z) = {
  let h = rm/(cof*polynomial_quotient(p, t - z, t)^m)
  map(j => [j, simplify(rewrite(d(h, [t, m - j]), t -> z)/factorial(m - j))], range(m))
}
# the roots of the polynomial p in t (solve, written by power_expand when that is still a root: im a for
# (-a^2)^(1/2)), or null
fn calc_hb_roots(p, t) = {
  let sols = solve(p == 0, t)
  if not is_list(sols) or length(sols) != degree(p, t) then null
  else map(sol => {
    let z = rewrite(t, sol)
    let z2 = power_expand(z)
    if expand(rewrite(p, t -> z2)) === 0 then z2 else z
  }, sols)
}
# [polynomial part, [[z, k, c], ...]] of the rational r in t over the complex numbers, or null: the denominator by
# factor_list into irreducible factors p^m, whose roots are simple
fn calc_hb_pf(r, t) = {
  let rr = together(r)
  let num = expand(numerator(rr))
  let den = expand(denominator(rr))
  if not polynomial_expression_q(num, t) or not polynomial_expression_q(den, t) then null
  else if free_of(den, t) then [simplify(num/den), []]
  else {
    let fl = select(factor_list(den), pm => not free_of(pm[1], t))
    let roots = map(pm => calc_hb_roots(pm[1], t), fl)
    if some(roots, rs => rs === null) then null
    else {
      let q = polynomial_quotient(num, den, t)
      let rm = polynomial_remainder(num, den, t)
      [q, flat_map(i => {
        let p = fl[i][1]
        let m = fl[i][2]
        let cof = cancel(den/p^m)
        flat_map(z => map(jc => [z, jc[1], jc[2]], calc_hb_laurent(rm, cof, p, m, t, z)), roots[i])
      }, range(length(fl)))]
    }
  }
}
# H((t - z)^-k)(s)
fn calc_hb_pole(z, k, s, asm) = {
  let sg = calc_hb_sign(calc_hb_reim(z)[2], asm)
  if sg === null then null
  else if sg == 0 then -pi*(-1)^(k - 1)/factorial(k - 1)*(if k == 1 then dirac_delta(s - z) else d(dirac_delta(s - z), [s, k - 1]))
  else im*sg*(s - z)^(-k)
}
# H of the rational r (w = 0): constants give 0, another polynomial part null
fn calc_hb_rat(r, t, s, asm) = {
  let pf = calc_hb_pf(r, t)
  if pf === null or not free_of(pf[1], t) then null
  else {
    let vs = map(p => {
      let h = calc_hb_pole(p[1], p[2], s, asm)
      if h === null then null else p[3]*h
    }, pf[2])
    if some(vs, v => v === null) then null else calc_total(vs)
  }
}
# H(exp(im w t) r(t))(s) for w != 0 of sign sg
fn calc_hb_osc(w, sg, r, t, s, asm) = {
  let pf = calc_hb_pf(r, t)
  if pf === null then null
  else {
    let e = u => exp(im*w*u)
    let poly = -im*sg*e(s)*rewrite(pf[1], t -> s)
    let vs = map(p => {
      let z = p[1]
      let k = p[2]
      let tay = calc_total(map(j => e(z)*(im*w)^j*(s - z)^j/factorial(j), range(0, k - 1)))
      let rest = map(j => {
        let h = calc_hb_pole(z, k - j, s, asm)
        if h === null then null else e(z)*(im*w)^j/factorial(j)*h
      }, range(0, k - 1))
      if some(rest, v => v === null) then null else p[3]*(-im*sg*(e(s) - tay)/(s - z)^k + calc_total(rest))
    }, pf[2])
    if some(vs, v => v === null) then null else poly + calc_total(vs)
  }
}
# a term c exp(E) R: [w, b, R] with E = im w t + b, or null
fn calc_hb_split(term, t) = {
  let fs = calc_factors(term)
  let isx = g => (head(g) === pow and g[1] === e and not free_of(g[2], t)) or (head(g) === exp and not free_of(g[1], t))
  let ex = select(fs, isx)
  let other = product(select(fs, g => not isx(g)))
  let big = calc_total(map(g => if head(g) === exp then g[1] else g[2], ex))
  let w = simplify(d(big, t)/im)
  let b = simplify(big - im*w*t)
  if not free_of(w, t) or not free_of(b, t) then null else [w, b, other]
}
fn calc_hilbert_osc(f, t, s, asm) = {
  let g = expand(trig_to_exp(f))
  let parts = map(term => calc_hb_split(term, t), calc_terms(g))
  if some(parts, p => p === null) then null
  else {
    let zero = calc_total(map(p => exp(p[2])*p[3], select(parts, p => simplify(p[1]) === 0)))
    let osc = select(parts, p => not (simplify(p[1]) === 0))
    let h0 = if zero === 0 then 0 else calc_hb_rat(zero, t, s, asm)
    let hs = map(p => {
      let sg = calc_hb_sign(calc_hb_reim(p[1])[1], asm)
      let ho = if sg === null or sg == 0 or not (calc_hb_sign(calc_hb_reim(p[1])[2], asm) === 0) then null else calc_hb_osc(p[1], sg, p[3], t, s, asm)
      if ho === null then null else exp(p[2])*ho
    }, osc)
    if h0 === null or some(hs, v => v === null) then null
    else {
      let v = simplify(h0 + calc_total(hs))
      if free_of(f, exp) and free_of(f, e) then calc_hb_trig(v) else v
    }
  }
}
# a transform of sines and cosines back in sines and cosines: exp(im u) = cos(u) + im sin(u) for the exponentials
# with an argument im u (u free of im), then trig_reduce
fn calc_hb_trig(v) = {
  let w = simplify(exp_to_trig(v))
  if free_of(w, exp) and free_of(w, e) then w
  else simplify(trig_reduce(expand(rewrite(v, exp(u_) :> (if free_of(expand(u/im), im) then cos(expand(u/im)) + im*sin(expand(u/im)) else exp(u))))))
}

# 3. piecewise polynomials of compact support: the breakpoints of the step functions and of the piecewise conditions,
# the polynomial on each piece by refine under the piece's bounds
fn calc_hb_calls(f, hs) = if is_atom(f) then [] else join(if some(hs, g => head(f) === g) then [f] else [], flat_map(u => calc_hb_calls(u, hs), apply(list, f)))
fn calc_free_all(f, hs) = every(hs, g => free_of(f, g))
fn calc_hb_breaks(f, t) = {
  let lin = (u, cs) => {
    let a = simplify(d(u, t))
    if not free_of(a, t) or a == 0 or not free_of(simplify(u - a*t), t) then null else map(c => simplify((c - (u - a*t))/a), cs)
  }
  let steps = select(calc_hb_calls(f, [unit_box, sign, unit_step, heaviside_theta]), g => length(g) == 1)
  let fromsteps = map(g => if head(g) === unit_box then lin(g[1], [-1/2, 1/2]) else lin(g[1], [0]), steps)
  let pws = calc_hb_calls(f, [piecewise])
  let conds = flat_map(p => map(c => c[2], p[1]), pws)
  let atoms = flat_map(c => calc_hb_calls(c, [less, less_equal, greater, greater_equal]), conds)
  let fromconds = map(c => {
    let sols = solve(c[1] == c[2], t)
    if is_list(sols) then map(sol => rewrite(t, sol), sols) else null
  }, atoms)
  let all = join(fromsteps, fromconds)
  if some(all, v => v === null) then null else {
    let pts = union(flatten(all))
    if every(pts, p => is_numeric(p) and is_real(n(p))) then sort_by(pts, p => n(p)) else null
  }
}
fn calc_hb_steps(f) = rewrite(f, [unit_box(u_) :> piecewise([[1, abs(u) < 1/2], [1/2, abs(u) == 1/2]], 0),
  unit_step(u_) :> piecewise([[1, u >= 0]], 0), heaviside_theta(u_) :> piecewise([[1, u > 0]], 0)])
fn calc_hb_piece(f, t, lo, hi) = {
  let v = simplify(assuming(lo < t and t < hi, refine(calc_hb_steps(f))))
  if polynomial_expression_q(v, t) then expand(v) else null
}
fn calc_hilbert_box(f, t, s) = {
  let bs = calc_hb_breaks(f, t)
  if bs === null or length(bs) < 2 then null
  else {
    let left = calc_hb_piece(f, t, -inf, bs[1])
    let right = calc_hb_piece(f, t, last(bs), inf)
    let ps = map(i => calc_hb_piece(f, t, bs[i], bs[i + 1]), range(length(bs) - 1))
    if not (left === 0) or not (right === 0) or some(ps, p => p === null) then null
    else {
      let vs = map(i => {
        let p = ps[i]
        let a = bs[i]
        let b = bs[i + 1]
        let pss = rewrite(p, t -> s)
        let q = expand(cancel((p - pss)/(s - t)))
        integrate(q, [t, a, b]) + pss*log(abs((s - a)/(s - b)))
      }, range(length(ps)))
      simplify(calc_total(vs)/pi)
    }
  }
}

fn calc_hilbert(f, t, s, asm) = {
  let steps = [unit_box, sign, unit_step, heaviside_theta, piecewise]
  let a = if calc_free_all(f, join(steps, [dirac_delta])) then calc_hilbert_osc(f, t, s, asm) else null
  let b = if not (a === null) then a else if asm === automatic then calc_hilbert_old(f, t, s) else assuming(asm, calc_hilbert_old(f, t, s))
  if b === null and not calc_free_all(f, steps) then calc_hilbert_box(f, t, s) else b
}
# computed in a symbol of its own (the transform variable may be the integration variable: hilbert_transform(f, t, t))
fn calc_hb_run(f, t, s, asm) = {
  let v = calc_hilbert(f, t, calc_hb_s, asm)
  if v === null then null else rewrite(v, calc_hb_s -> s)
}
# (the option assumptions: by a rule of its own: the conditions of a definition do not see its options)
rule hilbert_transform(f_, t_, s_) := calc_hb_run(f, t, s, automatic) when not (calc_hb_run(f, t, s, automatic) === null)
rule hilbert_transform(f_, t_, s_, options(assumptions -> a_)) := calc_hb_run(f, t, s, a) when not (calc_hb_run(f, t, s, a) === null)
rule inverse_hilbert_transform(g_, s_, t_) := -calc_hb_run(g, s, t, automatic) when not (calc_hb_run(g, s, t, automatic) === null)
rule inverse_hilbert_transform(g_, s_, t_, options(assumptions -> a_)) := -calc_hb_run(g, s, t, a) when not (calc_hb_run(g, s, t, a) === null)

# fourier_sequence_transform(f, k, w): the discrete-time Fourier transform, the sum of f(k) exp(-im k w) over all
# integers k, by linearity: discrete_delta(k - k0) gives exp(-im k0 w), a one-sided term g(k) unit_step(k - k0) the Z
# transform of g(k + k0) at z = exp(im w) times exp(-im k0 w), and g(k) unit_step(k1 - k) the Z transform of
# g(k1 - k) at z = exp(-im w) times exp(-im k1 w) (Oppenheim and Schafer, Discrete-Time Signal Processing, ch. 2)
fn calc_fst_term(f, k, w) = {
  let fs = calc_factors(f)
  let c = product(select(fs, g => free_of(g, k)))
  let dep = select(fs, g => not free_of(g, k))
  let steps = select(dep, g => head(g) === unit_step and length(g) == 1)
  let deltas = select(dep, g => head(g) === discrete_delta and length(g) == 1)
  let rest = product(select(dep, g => not (head(g) === unit_step or head(g) === discrete_delta)))
  if length(deltas) == 1 and length(steps) == 0 then {
    let ab = calc_lin(deltas[1][1], k)
    if ab === null or not (abs(ab[1]) === 1) then null else {
      let k0 = -ab[2]/ab[1]
      c*rewrite(rest, k -> k0)*exp(-im*k0*w)
    }
  } else if length(steps) == 1 and length(deltas) == 0 then {
    let ab = calc_lin(steps[1][1], k)
    if ab === null then null
    else if ab[1] === 1 then {
      let k0 = -ab[2]
      let zt = z_transform(rewrite(rest, k -> k + k0), k, calc_fst_z)
      if head(zt) === z_transform then null else simplify(c*exp(-im*k0*w)*rewrite(zt, calc_fst_z -> exp(im*w)))
    } else if ab[1] === -1 then {
      let k1 = ab[2]
      let zt = z_transform(rewrite(rest, k -> -k + k1), k, calc_fst_z)
      if head(zt) === z_transform then null else simplify(c*exp(-im*k1*w)*rewrite(zt, calc_fst_z -> exp(-im*w)))
    } else null
  } else null
}
fn calc_fst(f, k, w) = {
  let ts = map(t => calc_fst_term(t, k, w), calc_terms(expand(f)))
  if some(ts, t => t === null) then null else simplify(calc_total(ts))
}

# Two-sided sequences without steps, as distributions in w: a sum of exponentials
# c exp(im a k) (a real; sines and cosines written as exponentials) gives c dirac_comb((w - a)/(2 pi)), since
# sum over k of exp(im (a - w) k) = 2 pi sum over j of dirac_delta(w - a - 2 pi j) (Poisson summation; Oppenheim and
# Schafer, ch. 2); a sequence periodic of integer period P (built from mod(a k + b, N), a, b integers, P = lcm(N))
# by its discrete Fourier series f(k) = sum F_j exp(2 pi im j k/P), F_j = (1/P) sum over m < P of
# f(m) exp(-2 pi im j m/P). The sign of a and its reality by refine under the assumptions.
fn calc_fst_exp(f, k, w, asm) = {
  let parts = map(term => calc_hb_split(term, k), calc_terms(expand(trig_to_exp(f))))
  if some(parts, p => p === null) then null
  else {
    let vs = map(p => {
      let ri = calc_hb_reim(p[1])
      if not (calc_hb_sign(ri[2], asm) === 0) or not polynomial_expression_q(p[3], k) or not free_of(p[3], k) then null
      else p[3]*exp(p[2])*dirac_comb((w - p[1])/(2*pi))
    }, parts)
    if some(vs, v => v === null) then null else calc_total(vs)
  }
}
fn calc_fst_period(f, k) = {
  let ms = calc_hb_calls(f, [mod])
  let ok = length(ms) > 0 and every(ms, m => length(m) == 2 and is_integer(m[2]) and m[2] > 0 and not (calc_lin(m[1], k) === null) and is_integer(calc_lin(m[1], k)[1]))
  if not ok then null else {
    let p = apply(lcm, map(m => m[2], ms))
    let rest = rewrite(f, map(i => ms[i] -> calc_fst_slot(i), range(length(ms))))
    if free_of(rest, k) and p <= 1000 then p else null
  }
}
fn calc_fst_periodic(f, k, w) = {
  let p = calc_fst_period(f, k)
  if p === null then null
  else calc_total(map(j => {
    let fj = simplify(calc_total(map(m => rewrite(f, k -> m)*exp(-2*pi*im*j*m/p), range(0, p - 1)))/p)
    fj*dirac_comb((w - 2*pi*j/p)/(2*pi))
  }, range(0, p - 1)))
}
fn calc_fst1(f, k, w, asm) = {
  let a = if calc_free_all(f, [unit_step, discrete_delta, mod]) then calc_fst_exp(f, k, w, asm) else null
  let b = if a === null and calc_free_all(f, [unit_step, discrete_delta]) then calc_fst_periodic(f, k, w) else a
  if b === null then calc_fst(f, k, w) else simplify(b)
}
# several variables: a sum of products of factors in one variable each (unit_step(n1, n2) = unit_step(n1)
# unit_step(n2)) transforms factor by factor; a sequence periodic in every variable by its Fourier series over the
# period box
fn calc_us_split(e) = if is_atom(e) then e else if head(e) === unit_step and length(e) > 1 then product(map(u => unit_step(u), apply(list, e))) else apply(head(e), map(calc_us_split, apply(list, e)))
fn calc_fst_sep(f, ks, ws, asm) = {
  let g = expand(calc_us_split(f))
  let vs = map(term => {
    let fs = calc_factors(term)
    let c = product(select(fs, u => every(ks, kk => free_of(u, kk))))
    let mixed = some(fs, u => length(select(ks, kk => not free_of(u, kk))) > 1)
    if mixed then null else {
      let ps = map(i => calc_fst1(product(select(fs, u => not free_of(u, ks[i]))), ks[i], ws[i], asm), range(length(ks)))
      if some(ps, v => v === null) then null else c*product(ps)
    }
  }, calc_terms(g))
  if some(vs, v => v === null) then null else simplify(calc_total(vs))
}
fn calc_fst_box(f, ks, ws) = {
  let ps = map(kk => calc_fst_period(f, kk), ks)
  if some(ps, p => p === null) or product(ps) > 4096 then null else {
    let idx = tuples(map(p => range(0, p - 1), ps))
    calc_total(map(j => {
      let fj = simplify(calc_total(map(m => rewrite(f, map(i => ks[i] -> m[i], range(length(ks))))*exp(-2*pi*im*calc_total(map(i => j[i]*m[i]/ps[i], range(length(ks))))), idx))/product(ps))
      fj*product(map(i => dirac_comb((ws[i] - 2*pi*j[i]/ps[i])/(2*pi)), range(length(ks))))
    }, idx))
  }
}
fn calc_fst_run(f, k, w, asm, fp) = {
  # fourier_parameters: [a, b] (default [1, 1]): (|b|/(2 pi))^((1 - a)/2) sum f(k) exp(-im b k w), the transform at b w
  let wv = if fp === automatic then w else fp[2]*w
  let scale = if fp === automatic then 1 else (abs(fp[2])/(2*pi))^((1 - fp[1])/2)
  let v = if is_list(k) then (if is_list(w) and length(w) == length(k) then {
      let s = calc_fst_sep(f, k, if fp === automatic then w else map(u => fp[2]*u, w), asm)
      if s === null and calc_free_all(f, [unit_step, discrete_delta]) then calc_fst_box(f, k, if fp === automatic then w else map(u => fp[2]*u, w)) else s
    } else null)
    else calc_fst1(f, k, wv, asm)
  if v === null then null else simplify(scale*v)
}
fn calc_fst_ok(f, k, w) = (is_symbol(k) or (is_list(k) and every(k, is_symbol)))
rule fourier_sequence_transform(f_, k_, w_) := calc_fst_run(f, k, w, automatic, automatic) when calc_fst_ok(f, k, w) and not (calc_fst_run(f, k, w, automatic, automatic) === null)
rule fourier_sequence_transform(f_, k_, w_, options(assumptions -> a_)) := calc_fst_run(f, k, w, a, automatic) when calc_fst_ok(f, k, w) and not (calc_fst_run(f, k, w, a, automatic) === null)
rule fourier_sequence_transform(f_, k_, w_, options(fourier_parameters -> p_)) := calc_fst_run(f, k, w, automatic, p) when calc_fst_ok(f, k, w) and is_list(p) and length(p) == 2 and not (calc_fst_run(f, k, w, automatic, p) === null)

# --- limits of sequences: discrete_max_limit, discrete_min_limit ---

# calc_period(f, k): a period P of the oscillating factors of f over the integers k (2 for a power with a negative
# base, 2q for sin and cos of (p/q) pi k + b), inf when an angle is not a rational multiple of pi (the values then
# fill the circle densely), 1 when nothing oscillates
fn calc_period(f, k) = {
  if is_atom(f) then 1
  else {
    let own = if head(f) === pow and not free_of(f[2], k) and is_numeric(f[1]) and (n(f[1]) < 0) === true then 2
      else if (head(f) === sin or head(f) === cos) and not free_of(f[1], k) then {
        let ab = calc_lin(f[1], k)
        let q = if ab === null then null else simplify(ab[1]/pi)
        if q === null then inf else if is_rational(q) then 2*denominator(q) else inf
      } else if calc_mod_lin(f, k) then f[2] else 1
    let subs = map(g => calc_period(g, k), apply(list, f))
    if own === inf or some(subs, p => p === inf) then inf else apply(lcm, join([own], subs))
  }
}

# calc_freeze(f, k, r): f along the subsequence k = r (mod P): the oscillating factors (powers with a negative base,
# sin and cos of rational multiples of pi k) take their value at k = r, the rest keeps k
fn calc_freeze(f, k, r) = {
  if is_atom(f) then f
  else if head(f) === pow and not free_of(f[2], k) and is_numeric(f[1]) and (n(f[1]) < 0) === true then (-1)^rewrite(f[2], k -> r)*(-f[1])^f[2]
  else if (head(f) === sin or head(f) === cos) and not free_of(f[1], k) then rewrite(f, k -> r)
  else if calc_mod_lin(f, k) then rewrite(f, k -> r)
  else if calc_mod_int(f) then mod(calc_freeze(calc_mod_fact(f[1], k, f[2]), k, r), f[2])
  else apply(head(f), map(g => calc_freeze(g, k, r), apply(list, f)))
}

# mod(a k + b, m) with integers a, b and m > 0: periodic in k with period m, its value at k = r on k = r (mod m)
fn calc_mod_int(f) = not is_atom(f) and head(f) === mod and length(f) == 2 and is_integer(f[2]) and f[2] > 0
fn calc_mod_lin(f, k) = calc_mod_int(f) and not free_of(f[1], k) and {
  let ab = calc_lin(f[1], k)
  not (ab === null) and is_integer(ab[1]) and is_integer(ab[2])
}
# inside mod(u, m): the summands c factorial(k + j) (integers c, j) of u are multiples of m from k + j >= m on, so
# they are 0 eventually
fn calc_mod_fact(u, k, m) = calc_total(select(calc_terms(expand(u)), t => {
  let fs = calc_factors(t)
  let facts = select(fs, g => not is_atom(g) and head(g) === factorial and is_integer(simplify(g[1] - k)))
  let c = product(select(fs, g => not some(facts, h => h === g)))
  not (length(facts) == 1 and is_integer(c))
}))

# discrete_max_limit(f, k -> inf), discrete_min_limit(f, k -> inf): the upper and lower limits of the sequence f(k)
# over the integers: with a period P of the oscillating parts, the extreme of the limits of the P subsequences
# k = r (mod P); when the oscillation is dense (an angle not a rational multiple of pi), max_limit and min_limit over
# the reals, which agree with the sequence's by equidistribution (Weyl); k -> -inf by k -> -k; lists element-wise
fn calc_dlim(f, spec, kind) = {
  let k = spec[1]
  let g0 = if spec[2] === -inf then rewrite(f, k -> -k) else f
  # Fibonacci and Lucas numbers by Binet's formulas: their oscillating part (-1/golden_ratio)^k is seen
  let g = rewrite(g0, [
    fibonacci(u_) :> (if free_of(u, k) then fibonacci(u) else (golden_ratio^u - (-1/golden_ratio)^u)/sqrt(5)),
    lucas_l(u_) :> (if free_of(u, k) then lucas_l(u) else golden_ratio^u + (-1/golden_ratio)^u)])
  let p = calc_period(g, k)
  if p === inf then (if kind === max then max_limit(g, k -> inf) else min_limit(g, k -> inf))
  else if p > 60 then null
  else {
    let ls = map(r => limit(calc_freeze(g, k, r), k -> inf), range(0, p - 1))
    if every(ls, v => calc_real(v) or v === inf or v === -inf) then (if kind === max then apply(max, ls) else apply(min, ls)) else null
  }
}
# max_limit and min_limit of f(x, y) at a point [a, b] where f - its shift is homogeneous of degree 0, f(a + l u,
# b + l w) = f(a + u, b + w) for l > 0: f is constant on the rays from the point, so its upper (lower) limit there is
# the supremum (infimum) of its values on the directions: over (1, t), t real, and the direction (0, 1) (and their
# negatives, f being a function of the direction of degree 0: the values at -(u, w) are those of f(-1, -t), taken
# too). A rational function in which the denominator vanishes along a direction where the numerator does not is
# unbounded near it.
fn calc_hom0(f, xy, ab) = {
  let g = rewrite(f, [xy[1] -> ab[1] + calc_hom_u, xy[2] -> ab[2] + calc_hom_w])
  let scaled = rewrite(g, [calc_hom_u -> calc_hom_l*calc_hom_u, calc_hom_w -> calc_hom_l*calc_hom_w])
  if simplify(scaled - g, assumptions: calc_hom_l > 0) === 0 then g else null
}
fn calc_dir_extreme(f, xy, ab, kind) = {
  let g = calc_hom0(f, xy, ab)
  if g === null then null else {
    let opt = if kind === max then max_value else min_value
    let vs = [opt(rewrite(g, [calc_hom_u -> 1, calc_hom_w -> calc_hom_t]), calc_hom_t), opt(rewrite(g, [calc_hom_u -> -1, calc_hom_w -> calc_hom_t]), calc_hom_t), rewrite(g, [calc_hom_u -> 0, calc_hom_w -> 1]), rewrite(g, [calc_hom_u -> 0, calc_hom_w -> -1])]
    let ok = every(vs, v => (is_numeric(v) and is_real(v)) or v === inf or v === -inf or v === indeterminate or v === complex_infinity)
    let real = select(vs, v => (is_numeric(v) and is_real(v)) or v === inf or v === -inf)
    if not ok or length(real) == 0 then null else (if kind === max then apply(max, real) else apply(min, real))
  }
}
rule max_limit(f_, spec_) := calc_dir_extreme(f, spec[1], spec[2], max) when match_q(spec, _ -> _) and calc_two_syms(spec[1]) and is_list(spec[2]) and length(spec[2]) == 2 and every(spec[2], u => is_numeric(u)) and not (calc_dir_extreme(f, spec[1], spec[2], max) === null)
rule min_limit(f_, spec_) := calc_dir_extreme(f, spec[1], spec[2], min) when match_q(spec, _ -> _) and calc_two_syms(spec[1]) and is_list(spec[2]) and length(spec[2]) == 2 and every(spec[2], u => is_numeric(u)) and not (calc_dir_extreme(f, spec[1], spec[2], min) === null)
rule discrete_max_limit(f_, spec_) := map(g => discrete_max_limit(g, spec), f) when is_list(f)
rule discrete_max_limit(f_, spec_) := calc_dlim(f, spec, max) when match_q(spec, _ -> _) and is_symbol(spec[1]) and (spec[2] === inf or spec[2] === -inf) and not (calc_dlim(f, spec, max) === null)
rule discrete_min_limit(f_, spec_) := map(g => discrete_min_limit(g, spec), f) when is_list(f)
rule discrete_min_limit(f_, spec_) := calc_dlim(f, spec, min) when match_q(spec, _ -> _) and is_symbol(spec[1]) and (spec[2] === inf or spec[2] === -inf) and not (calc_dlim(f, spec, min) === null)

# Stirling expansion of the gamma-type factors of f(x), x -> inf: factorial, gamma, pochhammer and binomial are written
# with a private head stg(z) = gamma(z); for z = c x + d, log gamma(z) = (z - 1/2) log(c x) - c x + log(2 pi)/2
# + sum_(k >= 2) (-1)^k B_k(d) / (k (k - 1) (c x)^(k - 1)) (Stirling's series, generalized to the shift d by the
# Bernoulli polynomials: DLMF 5.11.8). f must be a product of powers of such factors times a function whose series
# at infinity exists. calc_st_expand(f, x, nterms, mode, order) is [prefactor, kept series terms, power of x in the
# prefactor], or null: mode "terms" keeps the first nterms terms, "order" those up to x^-order in absolute power
fn calc_st_atoms(f) = if is_atom(f) then [] else (if head(f) === stg then [f] else union(apply(join, map(calc_st_atoms, apply(list, f)))))
fn calc_st_info(atom, x, kk) = {
  let arg = atom[1]
  let c = simplify(d(arg, x))
  let dd = simplify(arg - c*x)
  [(c*x + dd - 1/2)*log(c*x) - c*x + log(2*pi)/2, sum((-1)^calc_j*bernoulli_b(calc_j, dd)/(calc_j*(calc_j - 1)*c^(calc_j - 1))*x^(1 - calc_j), [calc_j, 2, kk]), dd]
}
fn calc_st_pow(t, x) = simplify(x*d(t, x)/t)
fn calc_st_expand(f, x, nterms, mode, order) = {
  let g = rewrite(f, [factorial(m_) :> stg(m + 1), pochhammer(a_, m_) :> stg(a + m)/stg(a), binomial(a_, b_) :> stg(a + 1)/(stg(b + 1)*stg(a - b + 1)), gamma(m_) :> stg(m)])
  let atoms = calc_st_atoms(g)
  let big = select(atoms, a => not free_of(a, x))
  let small = select(atoms, a => free_of(a, x))
  if length(big) == 0 or not is_symbol(x) then null else {
    let kk = nterms + order + 4
    let idx = range(length(big))
    let infos = map(a => calc_st_info(a, x, kk), big)
    let us = map(j => symbol(join_strings("calc_st_u", to_string(j))), idx)
    let g1 = rewrite(rewrite(g, map(j => big[j] -> us[j], idx)), map(a => a -> gamma(a[1]), small))
    let es = map(j => simplify(us[j]*d(g1, us[j])/g1), idx)
    let h0 = simplify(g1/product(map(j => us[j]^es[j], idx)))
    let hf = if head(h0) === mul then apply(list, h0) else [h0]
    let hbig = select(hf, t => not free_of(t, x) and (head(t) === exp or (head(t) === pow and not free_of(t[2], x))))
    let h = product(select(hf, t => not some(hbig, b => b === t)))
    if not every(es, e => free_of(e, x) and every(us, u => free_of(e, u))) or not every(us, u => free_of(h, u)) then null else {
      let l0 = sum(map(j => es[j]*infos[j][1], idx))
      let sr = sum(map(j => es[j]*infos[j][2], idx))
      let p0 = simplify(sum(map(j => es[j]*(infos[j][3] - 1/2), idx)))
      let ser = normal(series(h*exp(sr), x, inf, kk))
      if not free_of(ser, series) then null else {
        let terms = select(if head(ser) === add then apply(list, ser) else [ser], t => not (t === 0))
        let sorted = sort(terms, (a, b) => calc_st_pow(a, x) > calc_st_pow(b, x))
        let kept = if mode === "terms" then take(sorted, min(nterms, length(sorted))) else select(sorted, t => t === sorted[1] or calc_st_pow(t, x) + p0 >= -order)
        [simplify(exp(l0)*product(hbig)), kept, p0]
      }
    }
  }
}
fn calc_st_value(f, x, nterms, mode, order) = {
  let r = calc_st_expand(f, x, nterms, mode, order)
  if r === null then null else (if length(r[2]) == 1 then simplify(rewrite(simplify(r[1]*r[2][1]), 2^(1/2)*2^e_ :> 2^(e + 1/2))) else r[1]*total(r[2]))
}
# difference_root(g)(m) with a symbolic m: the closed form of the recurrence when r_solve_value finds one
fn calc_st_roots(f) = rewrite(f, difference_root(g_)(m_) :> {
  let sol = r_solve_value(g(calc_dry, calc_drk), calc_dry, calc_drk)
  if free_of(sol, r_solve_value) then sol(m) else difference_root(g)(m)
})
fn calc_stirling(f) = rewrite(f, [
  binomial(a_, b_) :> calc_stirling_f(a)/(calc_stirling_f(b)*calc_stirling_f(a - b)),
  factorial(m_) :> calc_stirling_f(m),
  gamma(m_) :> calc_stirling_f(m)/m])
fn calc_stirling_f(m) = sqrt(2*pi)*m^(m + 1/2)*exp(-m)
fn discrete_asymptotic(f, spec; series_term_goal = 1) when match_q(spec, _ -> _) and spec[2] === inf = {
  let f1 = calc_st_roots(f)
  let r = calc_st_value(f1, spec[1], series_term_goal, "terms", 0)
  if not (r === null) then r else {
    let v = asymptotic(calc_stirling(f1), spec)
    if head(v) === asymptotic then v else simplify(v)
  }
}

# element(x, algebraics): whether the number x is algebraic: rationals, im and their sums, products and rational
# powers are; pi and e are not (Lindemann, Hermite), nor exp(a), log(a), sin(a), cos(a), tan(a) for algebraic a other
# than 0 (log: other than 0 and 1) (Lindemann-Weierstrass), nor b^c for algebraic b other than 0 and 1 and algebraic
# irrational c (Gelfond-Schneider; so exp(r pi) for rational r other than 0, while exp(r pi im) is a root of unity),
# nor a sum or a product with one transcendental term and algebraic (nonzero)
# others; other numbers stay undecided
fn calc_algebraic(x) = {
  if is_rational(x) or x === im then true
  else if is_number(x) then (if is_exact(x) then true else null)
  else if x === pi or x === e then false
  else if head(x) === add or head(x) === mul then {
    let vs = map(calc_algebraic, apply(list, x))
    let bad = select(range(length(vs)), i => vs[i] === false)
    if every(vs, v => v === true) then true
    else if length(bad) == 1 and every(range(length(vs)), i => i == bad[1] or vs[i] === true) and (head(x) === add or not some(apply(list, x), t => t === 0)) then false
    else null
  }
  else if head(x) === pow then {
    let b = calc_algebraic(x[1])
    let c = calc_algebraic(x[2])
    if b === true and is_rational(x[2]) then true
    else if x[1] === e and is_rational(simplify(x[2]/(im*pi))) then true
    else if x[1] === e and is_rational(simplify(x[2]/pi)) and not (x[2] === 0) then false
    else if x[1] === e and c === true and not (x[2] === 0) then false
    else if b === true and c === true and not is_rational(x[2]) and not (x[1] === 0) and not (x[1] === 1) then false
    else null
  }
  else if some([exp, sin, cos, tan, sinh, cosh, tanh], h => head(x) === h) and length(x) == 1 then {
    let q = simplify(x[1]/pi)
    if (head(x) === sin or head(x) === cos or head(x) === tan) and is_rational(q) then true
    else if calc_algebraic(x[1]) === true and not (x[1] === 0) then false
    else null
  }
  else if head(x) === log and length(x) == 1 then (if calc_algebraic(x[1]) === true and not (x[1] === 0) and not (x[1] === 1) then false else null)
  else null
}
rule element(x_, algebraics) := map(t => element(t, algebraics), x) when is_list(x)
rule element(x_, algebraics) := calc_algebraic(x) when is_numeric(x) and not (calc_algebraic(x) === null)

# --- holonomic functions given by their equations: difference_root, differential_root ---

# difference_root(f)(k): the value at the integer k of the sequence defined by f(y, n), a list of a linear recurrence
# for y(n) and enough initial values, run forward by recurrence_table (and expanded); lists of k element-wise
rule difference_root(f_)(k_) := map(j => difference_root(f)(j), k) when is_list(k)
# below the initial values the recurrence runs backward: w(t) = y(-t) satisfies the equations with n -> -n and every y(e)
# as w(-e), run forward to -k (difference_root(...)(-5) returned the equations: the unevaluated recurrence_table's first
# argument); no value (initial values that do not start the recurrence) leaves the call unevaluated
fn calc_dr_value(f, k) = {
  let t = recurrence_table(f(calc_dry, calc_drk), calc_dry, [calc_drk, k, k])
  if is_list(t) then expand(t[1]) else {
    let e = rewrite(rewrite(f(calc_dry, calc_drk), calc_drk -> -calc_drk), calc_dry(u_) :> calc_drw(-u))
    let r = recurrence_table(e, calc_drw, [calc_drk, -k, -k])
    if is_list(r) then expand(r[1]) else null
  }
}
rule difference_root(f_)(k_) := calc_dr_value(f, k) when is_integer(k) and not (calc_dr_value(f, k) === null)

# differential_root(f)(x0): the value at the inexact real x0 of the function defined by f(y, x), a list of a linear
# ordinary differential equation for y(x) and initial values at one point a, by n_dsolve_value from a to x0 (to 14 digits)
fn calc_ic_point(eqs) = {
  let ics = select(rest(eqs), e => head(e) === equal and not is_atom(e[1]) and is_numeric(e[1][1]))
  if length(ics) == 0 then null else ics[1][1][1]
}
rule differential_root(f_)(x0_) := map(t => differential_root(f)(t), x0) when is_list(x0)
rule differential_root(f_)(x0_) := {
  let eqs = f(calc_dfy, calc_dfx)
  let a = calc_ic_point(eqs)
  if x0 == a then n(rewrite(calc_dfy(a), select(rest(eqs), e => e[1] === calc_dfy(a))[1][1] -> select(rest(eqs), e => e[1] === calc_dfy(a))[1][2]))
  else n_dsolve_value(eqs, calc_dfy, [calc_dfx, min(n(a), x0), max(n(a), x0)], precision_goal: 14, accuracy_goal: 14)(x0)
} when is_inexact(x0) and calc_real(x0) and not (calc_ic_point(f(calc_dfy, calc_dfx)) === null)


# --- wave functions in the piecewise expansion (round 32; piecewise_expand calls calc_waves) ---

# calc_waves(f): the waves of f as floor and abs expressions (sawtooth u - floor(u), square wave
# 1 - 2 floor(2 u) + 4 floor(u), triangle wave 1 - 4 |t - 1/2| with t = frac(u + 1/4) and |t - 1/2| written without
# abs as (1/2 - t) + (2 t - 1) floor(2 t)), for piecewise_expand
fn calc_waves(f) = rewrite(f, [
  triangle_wave(u_) :> expand(1 - 4*((1/2 - (u + 1/4 - floor(u + 1/4))) + (2*(u + 1/4 - floor(u + 1/4)) - 1)*(floor(2*(u + 1/4)) - 2*floor(u + 1/4)))),
  sawtooth_wave(u_) :> u - floor(u),
  square_wave(u_) :> 1 - 2*floor(2*u) + 4*floor(u)
])
fn calc_has_waves(f) = not free_of(f, sawtooth_wave(_)) or not free_of(f, square_wave(_)) or not free_of(f, triangle_wave(_))

# ====== formerly the round 29 library file (merged in round 34, in load order) ======
# Algebra and calculus library: functions written in MathCAS itself, evaluated after the
# round 28 library above, defined in every session. Helper functions are prefixed `cb_` and not part of the interface.
# Methods and literature: crates/mc-calculus/NOTES.md, section "Round 29: the algebra and calculus library".

# cb_vars(e): the variables of the numerator and denominator of e
fn cb_vars(e) = {
  let t = together(e)
  union(variables(numerator(t)), variables(denominator(t)))
}

# cb_or(cs), cb_and(cs): the disjunction and conjunction of a list of conditions
fn cb_or(cs) = if length(cs) == 0 then false else if length(cs) == 1 then cs[1] else apply(head(cb_p or cb_q), cs)
fn cb_and(cs) = if length(cs) == 0 then true else if length(cs) == 1 then cs[1] else apply(head(cb_p and cb_q), cs)
fn cb_total(xs) = fold((p, q) => p + q, 0, xs)

# cb_poly(p, xs): whether p is a polynomial in the variables xs
fn cb_poly(p, xs) = polynomial_expression_q(expand(p), xs) === true

# --- between ---

# cb_ranges(r): the closed ranges [lo, hi] a range specification stands for (a pair, a list of pairs, an interval,
# a centered interval), or null
fn cb_pair(r) = is_list(r) and length(r) == 2 and not is_list(r[1]) and not is_list(r[2])
fn cb_ranges(r) = {
  if cb_pair(r) then [r]
  else if is_list(r) and length(r) > 0 and every(r, cb_pair) then r
  else if head(r) === interval then map(p => [p[1], p[2]], apply(list, r))
  else if head(r) === centered_interval and length(r) == 2 then [[r[1] - r[2], r[1] + r[2]]]
  else null
}

# between(x, [a, b]): a <= x <= b; between(x, [[a1, b1], ...]) in one of the ranges; intervals and centered intervals
# as ranges, and as x (contained in a range); between([a, b]) the operator form
rule between(x_, r_) := {
  let rs = cb_ranges(r)
  let xs = if head(x) === interval or head(x) === centered_interval then cb_ranges(x) else null
  if xs === null then cb_or(map(p => p[1] <= x and x <= p[2], rs))
  else cb_and(map(q => cb_or(map(p => p[1] <= q[1] and q[2] <= p[2], rs)), xs))
} when not (cb_ranges(r) === null)
rule between(r_)(x_) := between(x, r)

# element(x, booleans): true for true and false, false for numbers and strings
rule element(x_, booleans) := (x === true or x === false) when x === true or x === false or is_numeric(x) or is_string(x)

# --- the order term o ---

# o(x), o(x, x0): the order term O(x - x0), a series with no terms; o(x)^k is O(x^k), and adding one to an expression
# truncates its series there
rule o(x_) := series_data(x, 0, list(), 1, 1) when is_symbol(x)
rule o(x_, x0_) := series_data(x, x0, list(), 1, 1) when is_symbol(x) and not (x0 === inf)

# --- numerical roots: n_roots ---

# n_roots(p == q, x): the numerical roots of a polynomial equation as x == r1 or x == r2 ..., with multiplicity
# cb_nr_goal(opts): the value of the option precision_goal: among the options, null without it
fn cb_nr_goal(opts) = {
  let rs = flatten(map(o => if not is_atom(o) and head(o) === options then apply(list, o) else [o], opts))
  let hit = select(rs, r => match_q(r, precision_goal -> _))
  if length(hit) == 0 then null else hit[1][2]
}

# (precision_goal: d above machine precision: the roots computed with d digits)
rule n_roots(eq_, x_, opts___) := {
  let p = if head(eq) === equal then eq[1] - eq[2] else eq
  let goal = cb_nr_goal(opts)
  let sols = if is_number(goal) and goal > 16 then n_solve(p == 0, x, working_precision: goal) else n_solve(p == 0, x)
  cb_or(map(s => x == rewrite(x, s), sols))
} when is_symbol(x) and cb_poly(if head(eq) === equal then eq[1] - eq[2] else eq, [x])

# --- Lyapunov and Sylvester equations ---

# cb_vec(m): the columns of the matrix m one after the other; cb_unvec(v, r): the matrix of r rows with those columns
fn cb_vec(m) = flatten(transpose(m))
fn cb_unvec(v, r) = transpose(partition(v, r))
# cb_kron_solve(k, c): the matrix x with k . vec(x) == vec(c)
fn cb_kron_solve(k, c) = cb_unvec(linear_solve(k, cb_vec(c)), length(c))

# lyapunov_solve(a, c): x with a.x + x.a^H == c; lyapunov_solve(a, b, c): x with a.x + x.b == c (Sylvester), by the
# Kronecker form (I (x) a + b^T (x) I) vec(x) = vec(c)
rule lyapunov_solve(a_, c_) := lyapunov_solve(a, conjugate_transpose(a), c) when is_matrix(a) and is_matrix(c)
rule lyapunov_solve(a_, b_, c_) := {
  let ia = identity_matrix(length(a))
  let ib = identity_matrix(length(b))
  cb_kron_solve(kronecker_product(ib, a) + kronecker_product(transpose(b), ia), c)
} when is_matrix(a) and is_matrix(b) and is_matrix(c)

# discrete_lyapunov_solve(a, c): x with a.x.a^H - x == c; (a, b, c): a.x.b - x == c; ([a, d], [b, e], c):
# a.x.b - d.x.e == c
rule discrete_lyapunov_solve(a_, c_) := discrete_lyapunov_solve(a, conjugate_transpose(a), c) when is_matrix(a) and is_matrix(c)
rule discrete_lyapunov_solve(a_, b_, c_) := {
  let k = kronecker_product(transpose(b), a)
  cb_kron_solve(k - identity_matrix(length(k)), c)
} when is_matrix(a) and is_matrix(b) and is_matrix(c)
rule discrete_lyapunov_solve(ad_, be_, c_) := {
  let k = kronecker_product(transpose(be[1]), ad[1]) - kronecker_product(transpose(be[2]), ad[2])
  cb_kron_solve(k, c)
} when is_list(ad) and length(ad) == 2 and is_matrix(ad[1]) and is_list(be) and length(be) == 2 and is_matrix(c)

# --- algebraic Riccati equations ---

# cb_mmax(m): the largest absolute value of an entry of the matrix m
fn cb_mmax(m) = max(map(abs, flatten(m)))
# cb_block(m, i, j, k): the k x k block (i, j) of m (blocks counted from 1)
fn cb_block(m, i, j, k) = table(m[(i - 1)*k + r][(j - 1)*k + s], [r, 1, k], [s, 1, k])

# cb_msign(z, k): the matrix sign function of z by the scaled Newton iteration z <- (c z + z^-1/c)/2 with
# c = |det z|^(-1/n) (Byers 1987), at most 100 steps
fn cb_msign(z, k) = {
  let zi = inverse(z)
  let c = abs(det(z))^(-1/length(z))
  let z2 = (c*z + zi/c)/2
  if k >= 100 or cb_mmax(z2 - z) <= 10^-14*cb_mmax(z2) then z2 else cb_msign(z2, k + 1)
}

# riccati_solve([a, b], [q, r]): the stabilizing solution x of a^H.x + x.a - x.b.r^-1.b^H.x + q == 0, from the sign
# of the Hamiltonian matrix [[a, -g], [-q, -a^H]] with g = b.r^-1.b^H (Roberts 1971, Byers 1987): x solves
# [w12; w22 + I] x = -[w11 + I; w21] in the least-squares sense; machine numbers
rule riccati_solve(ab_, qr_, opts___) := {
  let a = n(ab[1])
  let b = n(ab[2])
  let q = n(qr[1])
  let r = n(qr[2])
  let k = length(a)
  let g = dot(b, inverse(r), conjugate_transpose(b))
  let ah = conjugate_transpose(a)
  let h = join(map(i => join(a[i], -g[i]), range(k)), map(i => join(-q[i], -ah[i]), range(k)))
  let w = cb_msign(h, 0)
  let id = identity_matrix(k)
  let m = join(cb_block(w, 1, 2, k), cb_block(w, 2, 2, k) + id)
  let rhs = -join(cb_block(w, 1, 1, k) + id, cb_block(w, 2, 1, k))
  let mh = conjugate_transpose(m)
  let x = dot(inverse(dot(mh, m)), mh, rhs)
  (x + conjugate_transpose(x))/2
} when is_list(ab) and length(ab) == 2 and is_list(qr) and length(qr) == 2 and is_matrix(ab[1]) and is_matrix(ab[2]) and is_matrix(qr[1]) and is_matrix(qr[2])

# cb_sda(a, g, h, k): the structure-preserving doubling iteration for x = a^H x (I + g x)^-1 a + h (Chu, Fan, Lin and
# Wang 2004): a <- a w a, g <- g + a w g a^H, h <- h + a^H h w a with w = (I + g h)^-1; h tends to x
fn cb_sda(a, g, h, k) = {
  let w = inverse(identity_matrix(length(a)) + dot(g, h))
  let ah = conjugate_transpose(a)
  let a2 = dot(a, w, a)
  let g2 = g + dot(a, w, g, ah)
  let h2 = h + dot(ah, h, w, a)
  if k >= 60 or cb_mmax(h2 - h) <= 10^-14*cb_mmax(h2) then h2 else cb_sda(a2, g2, h2, k + 1)
}

# discrete_riccati_solve([a, b], [q, r]): the stabilizing solution x of
# a^H.x.a - x - a^H.x.b.(r + b^H.x.b)^-1.b^H.x.a + q == 0 by doubling; machine numbers
rule discrete_riccati_solve(ab_, qr_, opts___) := {
  let a = n(ab[1])
  let b = n(ab[2])
  let g = dot(b, inverse(n(qr[2])), conjugate_transpose(b))
  let x = cb_sda(a, g, n(qr[1]), 0)
  (x + conjugate_transpose(x))/2
} when is_list(ab) and length(ab) == 2 and is_list(qr) and length(qr) == 2 and is_matrix(ab[1]) and is_matrix(ab[2]) and is_matrix(qr[1]) and is_matrix(qr[2])

# --- Pade approximants ---

# cb_pade_t(cs, m, k, t): the [m/k] Pade approximant p(t)/q(t) of the power series with coefficients cs (cs[1] the
# constant term): q(0) = 1 and the coefficients of t^(m+1) ... t^(m+k) of q(t) f(t) vanish (Baker and Graves-Morris,
# Pade Approximants, 1.1)
fn cb_coef(cs, i) = if i < 0 or i >= length(cs) then 0 else cs[i + 1]
fn cb_pade_t(cs, m, k, t) = {
  let mat = table(cb_coef(cs, m + i - j), [i, 1, k], [j, 1, k])
  # a singular system (the block of the Pade table is degenerate): the approximant of the lower denominator degree
  if k > 0 and simplify(det(mat)) === 0 then cb_pade_t(cs, m, k - 1, t) else {
    let qs = if k == 0 then [] else linear_solve(mat, table(-cb_coef(cs, m + i), [i, 1, k]))
    let q = join([1], qs)
    let p = table(cb_total(table(q[j + 1]*cb_coef(cs, i - j), [j, 0, min(i, k)])), [i, 0, m])
    cb_total(table(p[i + 1]*t^i, [i, 0, m]))/cb_total(table(q[j + 1]*t^j, [j, 0, k]))
  }
}

# cb_pade(f, x, x0, m, k): the [m/k] approximant at x0 (x0 = inf through x = 1/t); a pole or zero t^v of the series
# at x0 is taken out first and the degree of the denominator lowered by -v for a pole
fn cb_pade(f, x, x0, m, k) = {
  let t = cb_padet
  let g = if x0 === inf then rewrite(f, x -> 1/t) else rewrite(f, x -> x0 + t)
  # a series in fractional powers t^r (c0 + c1 t + ...) of one class r mod 1 (sin(sqrt(x)) = x^(1/2) (1 - x/6 + ...)):
  # t^r times the approximant of the power series, its numerator degree floor(m - r) so that no power passes t^m
  let r = cb_frac_class(series(g, t, 0, m + k + 2), t)
  if r === null then failure(unevaluated, pade_approximant, "the series mixes fractional powers of several classes")
  else {
    let rt = cb_pade_in(expand(g*t^(-r)), t, floor(m - r), k)*t^r
    if x0 === inf then rewrite(rt, t -> 1/x) else rewrite(rt, t -> x - x0)
  }
}
# cb_pade_in(g, t, m, k): the [m/k] approximant in t at 0 of g, whose series is in integer powers
fn cb_pade_in(g, t, m, k) = {
  let s = series(g, t, 0, m + k + 2)
  let v = if head(s) === series_data and s[4] < 0 then s[4] else 0
  let kk = k + v
  let h = expand(normal(series(g*t^(-v), t, 0, m + kk)))
  let cs = table(coefficient(h, t, i), [i, 0, m + kk])
  cb_pade_t(cs, m, kk, t)*t^v
}
# cb_frac_class(s, t): 0 for a series in integer powers of t (or no series), r in (0, 1) when every power is r mod 1,
# null when the powers fall in several classes
fn cb_frac_class(s, t) = if not free_of(s, series) or is_failure(s) then 0 else {
  let e = expand(normal(s))
  let terms = if head(e) === add then apply(list, e) else [e]
  let exps = map(u => exponent(u, t), terms)
  if every(exps, is_integer) then 0
  else if not every(exps, is_rational) then null
  else {
    let r = min(exps) - floor(min(exps))
    if every(exps, ee => is_integer(ee - r)) then r else null
  }
}

# pade_approximant(f, [x, x0, [m, k]]): the Pade approximant of f at x0 with numerator degree m and denominator
# degree k; pade_approximant(f, [x, x0, n]) the diagonal one [n/n]
rule pade_approximant(f_, spec_) := {
  let mk = if is_list(spec[3]) then spec[3] else [spec[3], spec[3]]
  cb_pade(f, spec[1], spec[2], mk[1], mk[2])
} when is_list(spec) and length(spec) == 3 and is_symbol(spec[1]) and (is_integer(spec[3]) or is_list(spec[3]) and length(spec[3]) == 2)

# --- partial fractions over square-free factors: apart_square_free, partial_fraction_elements ---

# cb_den_factors(d, x): [factor, multiplicity] pairs of the denominator d: each factor of d as written, split into
# its square-free parts (factors free of x dropped), equal factors merged
fn cb_den_factors(d, x) = {
  let fs = if head(d) === mul then apply(list, d) else [d]
  let pairs = flat_map(f => {
    let b = if head(f) === pow and is_integer(f[2]) then f[1] else f
    let e = if head(f) === pow and is_integer(f[2]) then f[2] else 1
    map(p => [p[1], p[2]*e], select(factor_square_free_list(b), p => not free_of(p[1], x)))
  }, fs)
  let bases = unique(map(p => expand(p[1]), pairs))
  map(b => [b, cb_total(map(p => p[2], select(pairs, p => expand(p[1]) === b)))], bases)
}

# cb_adic(r, d, e, x): r/d^e written as sum_j a_j/d^(e - j) with deg a_j < deg d (repeated division by d)
fn cb_adic(r, d, e, x) = {
  if e == 0 then 0 else {
    let qr = polynomial_quotient_remainder(r, d, x)
    qr[2]/d^e + cb_adic(qr[1], d, e - 1, x)
  }
}

# cb_xgcd(r0, r1, s0, s1, x): the extended Euclidean algorithm keeping the remainders polynomials in x (each is
# cleared of its denominator, free of x, with the cofactor scaled alike): [g, s] with s a == g mod m
fn cb_xgcd(r0, r1, s0, s1, x) = {
  if together(r1) === 0 then [r0, s0] else if free_of(r1, x) then [r1, s1] else {
    let qr = polynomial_quotient_remainder(r0, r1, x)
    let r2 = together(qr[2])
    let s2 = together(s0 - qr[1]*s1)
    cb_xgcd(r1, expand(numerator(r2)), s1, expand(s2*denominator(r2)), x)
  }
}
# cb_inv_mod(a, m, x): the inverse of a modulo m (coprime polynomials in x)
fn cb_inv_mod(a, m, x) = {
  let g = cb_xgcd(m, a, 0, 1, x)
  together(g[2]/g[1])
}

# cb_apart_sf(e, x): the square-free partial fraction decomposition of e in x: polynomial part plus
# sum_i sum_j a_ij/d_i^j over the square-free (pairwise coprime) factors d_i of the denominator, with deg a_ij <
# deg d_i; the numerators come from the extended gcd of each factor power with the rest of the denominator
fn cb_apart_sf(e, x) = {
  let t = if head(e) === add then together(e) else e
  let num = expand(numerator(t))
  let den = denominator(t)
  let qr = polynomial_quotient_remainder(num, expand(den), x)
  let fs = cb_den_factors(den, x)
  let lead = cancel(expand(den)/expand(product(map(p => p[1]^p[2], fs))))
  let rem = qr[2]/lead
  let parts = map(p => {
    let fi = expand(p[1]^p[2])
    let rest = expand(product(map(q => q[1]^q[2], select(fs, q => not (q === p)))))
    let s = cb_inv_mod(rest, fi, x)
    let tt = together(rem*s)
    let ri = polynomial_remainder(expand(numerator(tt)), fi, x)/denominator(tt)
    cb_adic(ri, p[1], p[2], x)
  }, fs)
  qr[1]/lead + cb_total(parts)
}

# apart_square_free(e), (e, x): e as a polynomial plus a sum of terms a/d^j over the square-free factors d of its
# denominator, deg a < deg d (no factoring beyond square-free); without x in the last variable; element-wise on
# lists and the sides of equations and inequalities
fn cb_structural(e) = is_list(e) or some([equal, less, less_equal, greater, greater_equal, unequal, head(cb_p and cb_q), head(cb_p or cb_q), head(not cb_p)], h => head(e) === h)
rule apart_square_free(e_) := apply(head(e), map(apart_square_free, apply(list, e))) when cb_structural(e)
rule apart_square_free(e_) := apart_square_free(e, last(cb_vars(e))) when not cb_structural(e) and length(cb_vars(e)) > 0
rule apart_square_free(e_) := e when not cb_structural(e) and length(cb_vars(e)) == 0
rule apart_square_free(e_, x_) := apply(head(e), map(t => apart_square_free(t, x), apply(list, e))) when cb_structural(e)
rule apart_square_free(e_, x_) := cb_apart_sf(e, x) when not cb_structural(e) and is_symbol(x) and cb_poly(numerator(together(e)), [x]) and cb_poly(denominator(together(e)), [x])
rule apart_square_free(e_, x_) := e when not cb_structural(e) and is_symbol(x) and free_of(e, x)

# partial_fraction_elements(r, x): the parts of the partial fraction decomposition of the rational function r as an
# association: "Decomposition", "PolynomialPart", "FractionalPart", "PolynomialCoefficients" (constant term first),
# "FractionalCoefficients" ([pole, multiplicity, [c_1, ..., c_m]] with the term c_j/(x - pole)^j), "Poles",
# "PoleMultiplicities"; partial_fraction_elements(r, x, key) one part
fn cb_pfe(r, x) = {
  let t = together(r)
  let num = expand(numerator(t))
  let den = expand(denominator(t))
  let qr = polynomial_quotient_remainder(num, den, x)
  let poly = qr[1]
  let roots = solve(den == 0, x)
  let poles = unique(map(s => rewrite(x, s), roots))
  let ps = sort(poles)
  let mult = map(p => count(map(s => rewrite(x, s), roots), p), ps)
  let rest = qr[2]/den
  let coefs = map(i => {
    let p = ps[i]
    let m = mult[i]
    let g = cancel(rest*(x - p)^m)
    [p, m, table(simplify(rewrite(d(g, [x, m - j]), x -> p)/factorial(m - j)), [j, 1, m])]
  }, range(length(ps)))
  let frac = cb_total(map(c => cb_total(table(c[3][j]/(x - c[1])^j, [j, 1, c[2]])), coefs))
  assoc("Decomposition" -> poly + frac, "PolynomialPart" -> poly, "FractionalPart" -> frac,
    "PolynomialCoefficients" -> coefficient_list(poly, x), "FractionalCoefficients" -> coefs,
    "Poles" -> ps, "PoleMultiplicities" -> mult)
}
rule partial_fraction_elements(r_, x_) := cb_pfe(r, x) when is_symbol(x) and cb_poly(numerator(together(r)), [x]) and cb_poly(denominator(together(r)), [x])
rule partial_fraction_elements(r_, x_, key_) := cb_pfe(r, x)[key] when is_string(key) and is_symbol(x) and cb_poly(numerator(together(r)), [x]) and cb_poly(denominator(together(r)), [x])

# --- reductions: polynomial_reduction, symmetric_reduction ---

# polynomial_reduction(p, [g1, ...], xs): the remainder of p on division by the polynomials gi (polynomial_reduce)
rule polynomial_reduction(p_, gs_, xs_) := polynomial_reduce(p, gs, xs)[2] when is_list(gs) and is_list(polynomial_reduce(p, gs, xs))
# the options (monomial_order, coefficient_domain) are polynomial_reduce's
rule polynomial_reduction(p_, gs_, xs_, o_) := polynomial_reduce(p, gs, xs, o)[2] when is_list(gs) and not is_atom(o) and head(o) === options and is_list(polynomial_reduce(p, gs, xs, o))

# cb_lex_max(rs): the rule exponents -> coefficient of rs with the lexicographically largest exponents
fn cb_lex_max(rs) = last(sort_by(rs, r => r[1]))
# cb_nonincreasing(es): whether e1 >= e2 >= ...
fn cb_nonincreasing(es) = every(range(length(es) - 1), i => es[i] >= es[i + 1])
fn cb_mono(xs, es) = product(map(i => xs[i]^es[i], range(length(xs))))

# cb_symred(f, xs, ss, p, r): the reduction loop: the lexicographically leading term c x^a of f is replaced by
# c s1^(a1 - a2) ... sn^an when a1 >= ... >= an (the fundamental theorem, Gauss's algorithm), else moved to the
# remainder; terms free of xs (all exponents 0) belong to the symmetric part
fn cb_symred(f, xs, ss, p, r) = {
  if f === 0 then [p, r] else {
    let rs = coefficient_rules(f, xs)
    let lt = cb_lex_max(rs)
    let es = lt[1]
    let c = lt[2]
    if every(es, e => e == 0) then [p + c, r]
    else if cb_nonincreasing(es) then {
      let ks = map(i => if i < length(es) then es[i] - es[i + 1] else es[i], range(length(es)))
      let es_poly = product(map(i => symmetric_polynomial(i, xs)^ks[i], range(length(xs))))
      let s_poly = product(map(i => ss[i]^ks[i], range(length(xs))))
      cb_symred(expand(f - c*es_poly), xs, ss, p + c*s_poly, r)
    } else cb_symred(expand(f - c*cb_mono(xs, es)), xs, ss, p, r + c*cb_mono(xs, es))
  }
}

# symmetric_reduction(f, [x1, ..., xn]): [p, q] with f = p + q, p a polynomial in the elementary symmetric
# polynomials of the xi (written out) and q the remainder; symmetric_reduction(f, xs, [s1, ..., sn]) writes p in the
# names si
rule symmetric_reduction(f_, xs_) := {
  let es = map(i => symmetric_polynomial(i, xs), range(length(xs)))
  cb_symred(expand(f), xs, es, 0, 0)
} when is_list(xs) and every(xs, is_symbol)
rule symmetric_reduction(f_, xs_, ss_) := cb_symred(expand(f), xs, ss, 0, 0) when is_list(xs) and is_list(ss) and length(xs) == length(ss)

# --- generating functions: find_generating_function ---

# cb_recurrence(a, k): the coefficients c1..ck of a recurrence a_n = c1 a_(n-1) + ... + ck a_(n-k) that every term
# of the list a after the first k satisfies, or null (the Hankel system of the first k equations, then a check)
fn cb_recurrence(a, k) = {
  let mat = table(a[i + k - j], [i, 1, k], [j, 1, k])
  if simplify(det(mat)) === 0 then null else {
    let cs = map(simplify, linear_solve(mat, table(a[i + k], [i, 1, k])))
    if every(range(k + 1, length(a)), i => simplify(a[i] - cb_total(table(cs[j]*a[i - j], [j, 1, k]))) === 0) then cs else null
  }
}
fn cb_first_recurrence(a, k) = {
  if 2*k > length(a) - 1 then null else {
    let c = cb_recurrence(a, k)
    if c === null then cb_first_recurrence(a, k + 1) else c
  }
}

# find_generating_function(a, x): a rational generating function sum_n a_(n+1) x^n of the list a: the shortest
# linear recurrence with constant coefficients that the terms satisfy (checked on at least one term beyond those
# that determine it) gives the denominator 1 - c1 x - ... - ck x^k and the numerator by truncation
rule find_generating_function(a_, x_) := {
  let cs = cb_first_recurrence(a, 1)
  let k = length(cs)
  let q = 1 - cb_total(table(cs[j]*x^j, [j, 1, k]))
  let s = cb_total(table(a[i]*x^(i - 1), [i, 1, length(a)]))
  let p = cb_total(table(coefficient(expand(q*s), x, i)*x^i, [i, 0, k - 1]))
  factor(p)/factor(q)
} when is_list(a) and is_symbol(x) and length(a) >= 3 and not (cb_first_recurrence(a, 1) === null)

# --- series in several variables ---

# series(f, [x, x0, n], [y, y0, m], ...): the series in x to order n whose coefficients (exact functions of the
# other variables) are series in y to order m, and so on: the iterated expansion, x outermost
fn calc_series_multi(f, specs) = {
  let sp = specs[1]
  let s = series(f, sp[1], sp[2], sp[3])
  if length(specs) == 1 then (if head(s) === series_data then s else null)
  else if not (head(s) === series_data) then null
  else {
    let cs = map(c => if free_of(c, specs[2][1]) then c else calc_series_multi(c, rest(specs)), apply(list, s[3]))
    if some(cs, c => c === null) then null else series_data(s[1], s[2], cs, s[4], s[5])
  }
}
fn calc_series_multi_ok(specs) = every(specs, sp => is_list(sp) and length(sp) == 3 and is_symbol(sp[1]) and is_integer(sp[3]))
rule series(f_, s1_, s2_) := calc_series_multi(f, [s1, s2]) when is_list(s1) and is_list(s2) and calc_series_multi_ok([s1, s2]) and not (calc_series_multi(f, [s1, s2]) === null)
rule series(f_, s1_, s2_, s3_) := calc_series_multi(f, [s1, s2, s3]) when is_list(s1) and is_list(s2) and is_list(s3) and calc_series_multi_ok([s1, s2, s3]) and not (calc_series_multi(f, [s1, s2, s3]) === null)

# --- integrals over regions given by boole or piecewise ---

# integrate(g boole(c), [x1, a1, b1], ..., [xn, an, bn]) (or piecewise([[g, c]]) with default 0): the region c and the
# bounds by a cylindrical decomposition (reduce over the reals in the order of the integration variables, x1 outermost),
# then on each cell an iterated integral with the cell's bounds; cells of lower dimension (x == c) have measure 0
# (Collins' cylindrical algebraic decomposition; Fubini). null when reduce gives no decomposition or a cell is not of
# the nested form lo(x1..x(i-1)) < xi < hi(x1..x(i-1)).
fn calc_bi_atoms(c) = if not is_atom(c) and head(c) === `and` then flat_map(calc_bi_atoms, apply(list, c)) else [c]
fn calc_bi(form, vs, i, g) = {
  if form === false then 0
  else if i > length(vs) then (if form === true then g else null)
  else if not is_atom(form) and head(form) === `or` then {
    let ps = map(u => calc_bi(u, vs, i, g), apply(list, form))
    if some(ps, p => p === null) then null else calc_total(ps)
  } else {
    let x = vs[i]
    let atoms = calc_bi_atoms(form)
    let mine = select(atoms, a => not is_atom(a) and length(a) == 2 and (a[1] === x or a[2] === x) and calc_free_all(if a[1] === x then a[2] else a[1], drop(vs, i - 1)))
    let others = select(atoms, a => not some(mine, m => m === a))
    if some(mine, a => head(a) === equal) then 0
    else {
      let los = map(a => if a[1] === x then a[2] else a[1], select(mine, a => (a[2] === x and (head(a) === less or head(a) === less_equal)) or (a[1] === x and (head(a) === greater or head(a) === greater_equal))))
      let his = map(a => if a[1] === x then a[2] else a[1], select(mine, a => (a[1] === x and (head(a) === less or head(a) === less_equal)) or (a[2] === x and (head(a) === greater or head(a) === greater_equal))))
      if length(los) > 1 or length(his) > 1 or length(los) + length(his) != length(mine) then null
      else {
        let lo = if length(los) == 0 then -inf else los[1]
        let hi = if length(his) == 0 then inf else his[1]
        let rest_form = if length(others) == 0 then true else fold((p, q) => p and q, others[1], rest(others))
        let body = calc_bi(rest_form, vs, i + 1, g)
        if body === null then null else integrate(body, [x, lo, hi])
      }
    }
  }
}
fn calc_bi_parts(f) = {
  let fs = calc_factors(f)
  let bs = select(fs, u => head(u) === boole)
  if length(bs) == 1 then [product(select(fs, u => not (head(u) === boole))), bs[1][1]]
  else if head(f) === piecewise and length(f) == 1 and length(f[1]) == 1 then [f[1][1][1], f[1][1][2]]
  else if head(f) === piecewise and length(f) == 2 and f[2] === 0 and length(f[1]) == 1 then [f[1][1][1], f[1][1][2]]
  else null
}
fn calc_bint(f, specs) = {
  let parts = calc_bi_parts(f)
  if parts === null or not every(specs, s => is_list(s) and length(s) == 3 and is_symbol(s[1])) then null
  else {
    let vs = map(s => s[1], specs)
    let bounds = flat_map(s => join(if s[2] === -inf then [] else [s[2] <= s[1]], if s[3] === inf then [] else [s[1] <= s[3]]), specs)
    let region = fold((p, q) => p and q, parts[2], bounds)
    let form = reduce(region, vs, reals)
    if head(form) === reduce or is_failure(form) then null
    else {
      let v = calc_bi(form, vs, 1, parts[1])
      if v === null or not free_of(v, integrate) then null else simplify(v)
    }
  }
}
fn calc_bint_ok(f, specs) = (not free_of(f, boole) or not free_of(f, piecewise)) and not (calc_bint(f, specs) === null)
rule integrate(f_, s1_, s2_) := calc_bint(f, [s1, s2]) when calc_bint_ok(f, [s1, s2])
rule integrate(f_, s1_, s2_, s3_) := calc_bint(f, [s1, s2, s3]) when calc_bint_ok(f, [s1, s2, s3])

# --- extrema of sums of periodic functions with incommensurable periods ---

# f = g + h + c, g and h continuous and periodic in x with periods P and Q whose ratio is irrational (an algebraic
# number of degree at least 2), c free of x: the curve x -> (x mod P, x mod Q) is dense in the torus (Kronecker's
# theorem; Hardy and Wright, An Introduction to the Theory of Numbers, theorem 442), so sup f = sup g + sup h + c and
# inf f = inf g + inf h + c (the values only: whether the bound is attained is not decided here). The terms of f are
# grouped by commensurable periods; null unless there are exactly two groups.
fn calc_kron_period(g, x) = {
  let p = function_period(g, x)
  if is_numeric(p) and not (head(p) === function_period) and p > 0 then p else null
}
fn calc_kron_join(groups, t, p, x) = {
  let is = select(range(length(groups)), i => is_rational(simplify(groups[i][2]/p)))
  if length(is) == 0 then join(groups, [[t, p]]) else map(i => if i == is[1] then [groups[i][1] + t, function_period(groups[i][1] + t, x)] else groups[i], range(length(groups)))
}
fn calc_kron(f, x, kind) = {
  let ts = calc_terms(f)
  let c = calc_total(select(ts, t => free_of(t, x)))
  let vs = select(ts, t => not free_of(t, x))
  let ps = map(t => calc_kron_period(t, x), vs)
  if length(vs) < 2 or some(ps, p => p === null) then null else {
    let groups = fold((gs, i) => calc_kron_join(gs, vs[i], ps[i], x), [], range(length(vs)))
    if length(groups) != 2 or some(groups, gr => not is_numeric(gr[2]) or head(gr[2]) === function_period) then null else {
      let r = simplify(groups[1][2]/groups[2][2])
      let mp = minimal_polynomial(r, calc_kron_t)
      let irrational = is_numeric(r) and polynomial_expression_q(mp, [calc_kron_t]) and exponent(mp, calc_kron_t) >= 2
      let cont = every(groups, gr => function_continuous(gr[1], x) === true)
      let ext = map(gr => kind(gr[1], x), groups)
      if not irrational or not cont or some(ext, v => not is_numeric(v) or not free_of(v, kind)) then null else simplify(c + calc_total(ext))
    }
  }
}
rule max_value(f_, x_) := calc_kron(f, x, max_value) when is_symbol(x) and head(f) === add and not (calc_kron(f, x, max_value) === null)
rule min_value(f_, x_) := calc_kron(f, x, min_value) when is_symbol(x) and head(f) === add and not (calc_kron(f, x, min_value) === null)

# --- optimization over a vector variable ---

# minimize, maximize, min_value, max_value, arg_min, arg_max of [f, cons] in one symbol v used as a vector (through
# dot): v is split into its entries v1, ..., vn, the vector inequalities into scalar ones (lo_scalars), the problem
# solved in the entries and v given back as the vector of their values. The dimension is that of the vectors and
# matrices v meets: dot(m, v) with m a matrix (its columns) or a vector, dot(v, w) (w's length); several that
# disagree, a cone, or no dimension found leave the problem.
fn calc_vdims(e, v) = {
  let ds = cases(e, dot(a_, b_) :> [a, b], [0, inf])
  join(map(p => if is_matrix(p[1]) then length(p[1][1]) else length(p[1]), select(ds, p => is_list(p[1]) and p[2] === v)),
    map(p => length(p[2]), select(ds, p => is_list(p[2]) and p[1] === v)))
}
fn calc_vopt(fname, fc, v) = {
  let f = if is_list(fc) then fc[1] else fc
  let cons = if is_list(fc) and length(fc) == 2 then fc[2] else true
  let ds = unique(calc_vdims([f, cons], v))
  if length(ds) != 1 or not is_integer(ds[1]) or ds[1] < 1 or ds[1] > 64 then null
  else {
    let xs = map(k => to_expression(string_join(["calc_v", to_string(k)])), range(ds[1]))
    let parts = flatten(map(lo_scalars, calc_conj_list(rewrite(cons, v -> xs))), 1)
    if some(parts, p => not calc_free_all(p, [vector_greater_equal, vector_less_equal, vector_greater, vector_less, v])) then null
    else {
      let c = if length(parts) == 0 then true else fold((a, b) => a and b, parts[1], rest(parts))
      let g = rewrite(f, v -> xs)
      let r = fname([g, c], xs)
      if head(r) === fname or is_failure(r) then null
      else if fname === minimize or fname === maximize then (if is_list(r) and length(r) == 2 and is_list(r[2]) then [r[1], [v -> rewrite(xs, r[2])]] else null)
      else r
    }
  }
}
fn calc_conj_list(c) = if not is_atom(c) and head(c) === `and` then flat_map(calc_conj_list, apply(list, c)) else if c === true then [] else [c]
fn calc_vopt_ok(fname, fc, v) = is_symbol(v) and not free_of(fc, dot) and not (calc_vopt(fname, fc, v) === null)
rule minimize(fc_, v_) := calc_vopt(minimize, fc, v) when calc_vopt_ok(minimize, fc, v)
rule maximize(fc_, v_) := calc_vopt(maximize, fc, v) when calc_vopt_ok(maximize, fc, v)
rule min_value(fc_, v_) := calc_vopt(min_value, fc, v) when calc_vopt_ok(min_value, fc, v)
rule max_value(fc_, v_) := calc_vopt(max_value, fc, v) when calc_vopt_ok(max_value, fc, v)
rule arg_min(fc_, v_) := calc_vopt(arg_min, fc, v) when calc_vopt_ok(arg_min, fc, v)
rule arg_max(fc_, v_) := calc_vopt(arg_max, fc, v) when calc_vopt_ok(arg_max, fc, v)

# The global extrema of a quadratic a x^2 + b x + c in x whose leading coefficients are real parameters, by the
# complete case split: the maximum is c - b^2/(4 a) at -b/(2 a) when a < 0, c (at x = 0, every point being a maximum)
# when a = b = 0, and unbounded (inf, the point indeterminate) otherwise; the minimum likewise with a > 0. Used when
# a coefficient is not a number.
fn calc_pq_parts(f, x) =
  if not is_symbol(x) or is_list(f) or not polynomial_expression_q(f, [x]) or exponent(f, x) > 2 or exponent(f, x) < 1 then null
  else {
    let a2 = coefficient(f, x, 2)
    let a1 = coefficient(f, x, 1)
    let a0 = expand(f - a2*x^2 - a1*x)
    if every([a2, a1, a0], is_number) or not free_of([a2, a1, a0], x) or not free_of([a2, a1, a0], im) then null else [a2, a1, a0]
  }
# [value, point] of the maximum (s = 1) or minimum (s = -1)
fn calc_pq_opt(f, x, s) = {
  let cs = calc_pq_parts(f, x)
  let a2 = cs[1]
  let a1 = cs[2]
  let a0 = cs[3]
  let big = if s == 1 then inf else -inf
  if a2 === 0 then [piecewise([[a0, a1 == 0]], big), piecewise([[0, a1 == 0]], indeterminate)]
  else {
    let opens = if s == 1 then a2 < 0 else a2 > 0
    [piecewise([[a0 - a1^2/(4*a2), opens], [a0, a2 == 0 and a1 == 0]], big), piecewise([[-a1/(2*a2), opens], [0, a2 == 0 and a1 == 0]], indeterminate)]
  }
}
rule maximize(f_, x_) := (r => [r[1], [x -> r[2]]])(calc_pq_opt(f, x, 1)) when not (calc_pq_parts(f, x) === null)
rule minimize(f_, x_) := (r => [r[1], [x -> r[2]]])(calc_pq_opt(f, x, -1)) when not (calc_pq_parts(f, x) === null)
rule max_value(f_, x_) := calc_pq_opt(f, x, 1)[1] when not (calc_pq_parts(f, x) === null)
rule min_value(f_, x_) := calc_pq_opt(f, x, -1)[1] when not (calc_pq_parts(f, x) === null)
rule arg_max(f_, x_) := calc_pq_opt(f, x, 1)[2] when not (calc_pq_parts(f, x) === null)
rule arg_min(f_, x_) := calc_pq_opt(f, x, -1)[2] when not (calc_pq_parts(f, x) === null)

# --- truth-value search: find_instance over booleans ---

# cb_assignments(vs): every assignment of true and false to the symbols vs, as lists of rules (false first)
fn cb_assignments(vs) = if length(vs) == 0 then [[]] else flat_map(b => map(a => join([vs[1] -> b], a), cb_assignments(rest(vs))), [false, true])

# find_instance(c, vs, booleans): an assignment of truth values to vs that makes c true, [] when there is none
# (the 2^n assignments in turn, at most 2^16)
rule find_instance(c_, vs_, booleans) := {
  let ws = if is_list(vs) then vs else [vs]
  let hits = select(cb_assignments(ws), a => rewrite(c, a) === true)
  if length(hits) == 0 then [] else [hits[1]]
} when (is_list(vs) and length(vs) <= 16 and every(vs, is_symbol)) or is_symbol(vs)

# --- change of variables in integrals: integrate_change_variables ---

# cb_icv1(f, spec, u, eq): one integral in x (spec x or [x, a, b]) rewritten in u with x = h(u) from eq, f(h(u)) h'(u),
# the limits mapped through u = g(x); null when eq does not give x in terms of u
fn cb_plain(v) = if head(v) === conditional_expression then rewrite(v[1], c(1) -> 0) else v
fn cb_icv1(f, spec, u, eq) = {
  let x = if is_list(spec) then spec[1] else spec
  # u == g(x): f/g' with g(x) written as u, when that removes x; else x = h(u) and f(h(u)) h'(u)
  let gx = if eq[1] === u then eq[2] else if eq[2] === u then eq[1] else null
  let direct = if gx === null then null else simplify(rewrite(f/d(gx, x), gx -> u))
  let xs = solve(eq, x)
  let us = solve(eq, u)
  let g = if not (direct === null) and free_of(direct, x) then direct
    else if is_list(xs) and length(xs) > 0 then simplify(power_expand(cb_plain(rewrite(f, x -> cb_plain(rewrite(x, xs[1]))))*d(cb_plain(rewrite(x, xs[1])), u)))
    else null
  if g === null then null
  else if is_list(spec) and length(spec) == 3 then {
    let lim = s => if not (gx === null) then simplify(rewrite(gx, x -> s)) else simplify(cb_plain(rewrite(rewrite(u, us[1]), x -> s)))
    [g, [u, lim(spec[2]), lim(spec[3])]]
  } else [g, u]
}

# integrate_change_variables(inactive(integrate)(f, x), u, eq): the inactive integral rewritten in the new variable u
# related to x by the equation eq (u == g(x) or x == h(u)): the integrand f(h(u)) h'(u), definite limits mapped;
# integrate_change_variables(inactive(integrate)(f, x, y), [u, v], [eq1, eq2]) with the Jacobian determinant
rule integrate_change_variables(i_, u_, eq_) := {
  let parts = apply(list, i)
  let r = cb_icv1(parts[1], parts[2], u, eq)
  if r === null then failure(argument, integrate_change_variables, "the equation does not give the variable") else apply(head(i), r)
} when head(i) === inactive(integrate) and length(i) == 2 and is_symbol(u) and head(eq) === equal
rule integrate_change_variables(i_, us_, eqs_) := {
  let parts = apply(list, i)
  let f = parts[1]
  let xs = rest(parts)
  let sol = solve(eqs, xs)
  let hs = map(x => rewrite(x, sol[1]), xs)
  let jac = simplify(abs(det(map(h => d(h, us), hs))))
  let g = simplify(rewrite(f, map(k => xs[k] -> hs[k], range(length(xs))))*jac)
  apply(head(i), join([g], us))
} when head(i) === inactive(integrate) and is_list(us) and is_list(eqs) and length(us) == length(eqs) and length(i) == length(us) + 1 and every(rest(apply(list, i)), is_symbol)

# --- Radon transform ---

# radon_transform(f, [x, y], [p, phi]): the integral of f over the line x cos(phi) + y sin(phi) = p, parametrized by
# t: x = p cos(phi) - t sin(phi), y = p sin(phi) + t cos(phi), integrated over the real t (Deans, The Radon
# Transform and Some of Its Applications, chapter 1)
# Before integrating, powers sin(phi)^n (n >= 2) are written with cos(phi), so that x^2 + y^2 becomes p^2 + t^2. A factor
# unit_step(q(t)) with q quadratic in t and a negative leading coefficient restricts t to the interval between the
# roots of q: the transform is the integral over that interval where the discriminant D of q is positive, and 0
# elsewhere (piecewise, D > 0 solved for p). A factor dirac_delta(x - a, y - b) (the unit mass at (a, b)) gives c(a,
# b) dirac_delta(p - a cos(phi) - b sin(phi)), the rotation (x, y) -> (p, t) having Jacobian 1. Options: assumptions:
# (the integral computed under them), generate_conditions: true (its convergence conditions as
# conditional_expression).
fn cb_rad_trig(e, phi) = expand(rewrite(expand_all(e), sin(u_)^n_ :> (if u === phi and is_integer(n) and n >= 2 then sin(u)^mod(n, 2)*(1 - cos(u)^2)^quotient(n, 2) else sin(u)^n)))
fn cb_find_step(e, t) =
  if is_atom(e) then null
  else if head(e) === unit_step and length(e) == 1 and not free_of(e, t) then e
  else fold((a, u) => if a === null then cb_find_step(u, t) else a, null, apply(list, e))
fn cb_rad_step(g, t, p, asm) = {
  let s1 = cb_find_step(g, t)
  let rest0 = if s1 === null then null else rewrite(g, s1 -> 1)
  let st = if s1 === null or not free_of(rest0, unit_step) or not (expand(g - rest0*s1) === 0) then [] else [s1]
  if length(st) != 1 then null else {
    let q = expand(st[1][1])
    let ok = polynomial_expression_q(q, [t]) and exponent(q, t) == 2
    let a2 = if ok then coefficient(q, t, 2) else 0
    if not ok or not is_number(a2) or not (a2 < 0) then null else {
      let a1 = coefficient(q, t, 1)
      let a0 = expand(q - a2*t^2 - a1*t)
      let dd = simplify(a1^2 - 4*a2*a0)
      let rest = rest0
      let v = integrate(rest, [t, (-a1 + sqrt(dd))/(2*a2), (-a1 - sqrt(dd))/(2*a2)])
      let c = if polynomial_expression_q(dd, [p], is_rational) then reduce(dd > 0, p, reals) else dd > 0
      if not free_of(v, integrate) or head(v) === failure then null else {
        let pw = piecewise([[simplify(v), if head(c) === reduce or head(c) === failure then dd > 0 else c]], 0)
        if asm === true then pw else simplify(pw, assumptions: asm)
      }
    }
  }
}
fn cb_rad_delta(f, xy) = {
  let fs = calc_factors(f)
  let ds = select(fs, u => head(u) === dirac_delta and length(u) == 2)
  if length(ds) != 1 then null else {
    let a = expand(xy[1] - ds[1][1])
    let b = expand(xy[2] - ds[1][2])
    let rest = product(select(fs, u => not (u === ds[1])))
    if free_of(a, xy[1]) and free_of(a, xy[2]) and free_of(b, xy[1]) and free_of(b, xy[2]) then [rewrite(rest, [xy[1] -> a, xy[2] -> b]), a, b] else null
  }
}
# an undefined function of the two variables, u(x, y) (a symbol without attributes: the library's functions of two
# arguments are listable, the arithmetic is flat): its transform stays (held)
fn cb_rad_undef(u) = is_symbol(u) and attributes(u) === []
fn cb_rad_held(f, xy) = not is_atom(f) and cb_rad_undef(head(f)) and apply(list, f) === xy
# [u, a, b] for a factor that is the partial derivative derivative(a, b)(u)(x, y) (a = b = 0 for u(x, y)), or null
fn cb_rad_dpart(g, xy) =
  if cb_rad_held(g, xy) then [head(g), 0, 0]
  else if not is_atom(g) and apply(list, g) === xy and not is_atom(head(g)) and not is_atom(head(head(g))) and head(head(head(g))) === derivative and length(head(head(g))) == 2 and length(head(g)) == 1 and cb_rad_undef(head(g)[1]) and is_integer(head(head(g))[1]) and is_integer(head(head(g))[2]) then [head(g)[1], head(head(g))[1], head(head(g))[2]]
  else null
fn cb_rad_has_held(f, xy) = not is_atom(f) and (not (cb_rad_dpart(f, xy) === null) or some(apply(list, f), u => cb_rad_has_held(u, xy)))
# the transform of a linear combination of an undefined u and its partial derivatives (and terms free of u):
# R[derivative(a, b)(u)] = cos(phi)^a sin(phi)^b d^(a + b)/dp^(a + b) R[u] (Deans, The Radon Transform and Some of Its
# Applications, section 2.4); null for a term of another form (u times a function of x, y)
fn cb_rad_linear(f, xy, pf, asm, gc) = {
  let ts = calc_terms(expand(f))
  let one = t => {
    let fs = calc_factors(t)
    let hs = select(fs, g => not (cb_rad_dpart(g, xy) === null))
    let c = product(select(fs, g => cb_rad_dpart(g, xy) === null))
    if length(hs) == 0 then cb_radon(t, xy, pf, asm, gc)
    else if length(hs) > 1 or not free_of(c, xy[1]) or not free_of(c, xy[2]) then null else {
      let q = cb_rad_dpart(hs[1], xy)
      let r0 = radon_transform(q[1](xy[1], xy[2]), xy, pf)
      c*cos(pf[2])^q[2]*sin(pf[2])^q[3]*(if q[2] + q[3] == 0 then r0 else d(r0, [pf[1], q[2] + q[3]]))
    }
  }
  let vs = map(one, ts)
  if some(vs, v => v === null or head(v) === failure) then failure(unevaluated, radon_transform, "the line integral has no closed form")
  else simplify(calc_total(vs))
}
fn cb_radon(f, xy, pf, asm, gc) = {
  let dl = cb_rad_delta(f, xy)
  if not is_atom(f) and head(f) === equal and length(f) == 2 then {
    let l = cb_radon(f[1], xy, pf, asm, gc)
    let r = cb_radon(f[2], xy, pf, asm, gc)
    if head(l) === failure then l else if head(r) === failure then r else l == r
  }
  else if cb_rad_held(f, xy) then radon_transform(f, xy, pf)
  else if cb_rad_has_held(f, xy) then cb_rad_linear(f, xy, pf, asm, gc)
  else if not is_atom(f) and head(f) === inverse_radon_transform and length(f) == 3 and f[3] === xy and is_list(f[2]) and length(f[2]) == 2 then rewrite(f[1], [f[2][1] -> pf[1], f[2][2] -> pf[2]])
  else if not (dl === null) then dl[1]*dirac_delta(pf[1] - dl[2]*cos(pf[2]) - dl[3]*sin(pf[2])) else {
    let g = cb_rad_trig(rewrite(f, [xy[1] -> pf[1]*cos(pf[2]) - cb_radt*sin(pf[2]), xy[2] -> pf[1]*sin(pf[2]) + cb_radt*cos(pf[2])]), pf[2])
    let sv = cb_rad_step(g, cb_radt, pf[1], asm)
    if not (sv === null) then sv else {
      let r = if asm === true then integrate(g, [cb_radt, -inf, inf], generate_conditions: gc) else integrate(g, [cb_radt, -inf, inf], assumptions: asm, generate_conditions: gc)
      # a product is written with its polynomial factors' integer content taken out (factor)
      let tidy0 = e => simplify(trig_reduce(rewrite(cb_plain(e), sin(pf[2])^2 -> 1 - cos(pf[2])^2)))
      let tidy = e => if head(tidy0(e)) === mul then factor(tidy0(e)) else tidy0(e)
      if not free_of(r, integrate) then failure(unevaluated, radon_transform, "the line integral has no closed form")
      else if head(r) === conditional_expression then conditional_expression(tidy(r[1]), r[2])
      else tidy(r)
    }
  }
}
rule radon_transform(f_, xy_, pf_, opts___) := cb_radon(f, xy, pf, nl_asm(opts), nl_gc(opts)) when is_list(xy) and length(xy) == 2 and is_list(pf) and length(pf) == 2 and not cb_rad_held(f, xy)

# --- asymptotic expansions of integrals, sums and products ---

# cb_lead(f, x, x0): the lowest-order nonzero term of the series of f at x0
fn cb_lead(f, x, x0) = {
  let s = series(f, x, x0, 1)
  if head(s) === series_data and length(select(apply(list, s[3]), c => not (c === 0))) > 0 then {
    let k = s[4]
    let c = select(apply(list, s[3]), c => not (c === 0))[1]
    let j = first(select(range(length(s[3])), i => not (s[3][i] === 0))) - 1
    if x0 === inf then c*x^(-(k + j)) else c*(x - x0)^(k + j)
  } else s
}
# cb_series(s, x, x0, k): the series of s to order k at x0; at infinity gamma-type factors by Stirling's series
# (calc_st_expand) and an exponential factor kept whole beside the series of the rest
fn cb_series(s, x, x0, k) = {
  if x0 === inf then {
    let r = calc_st_value(s, x, k, "order", k)
    if not (r === null) then r else {
      let r0 = normal(series(s, x, x0, k))
      if free_of(r0, series) then r0 else {
        let fs = if head(s) === mul then apply(list, s) else [s]
        let big = select(fs, t => not free_of(t, x) and (head(t) === exp or (head(t) === pow and not free_of(t[2], x))))
        let rest = select(fs, t => not some(big, b => b === t))
        let r1 = if length(big) == 0 then r0 else normal(series(product(rest), x, x0, k))
        if length(big) > 0 and free_of(r1, series) then product(big)*r1 else r0
      }
    }
  } else normal(series(s, x, x0, k))
}
# cb_lead2(s, x, x0): the leading term at x0, by Stirling's series at infinity when s has gamma-type factors
fn cb_lead2(s, x, x0) = {
  let r = if x0 === inf then calc_st_value(s, x, 1, "terms", 0) else null
  if r === null then cb_lead(s, x, x0) else r
}
fn cb_is_rule(sp) = head(sp) === head(cb_a -> cb_b)
fn cb_asym_spec(sp) = is_list(sp) and length(sp) == 3 or cb_is_rule(sp)

# cb_asym_int(f, ispec, x, x0, k): the series of the integrand in x integrated term by term when x0 is finite and x is
# not the integration variable, else the series of the integral
# cb_upto(e, x, x0, k): the terms of the expanded e of degree at most k in x - x0, logarithms not counted (x0 = 0
# only: a series of x^(x^x) holds a term or two beyond its order)
fn cb_upto(e, x, x0, k) = if not (x0 === 0) then e else {
  let ts = if head(e) === add then apply(list, e) else [e]
  cb_total(select(ts, u => exponent(rewrite(u, log(x) -> cb_upto_log), x) <= k))
}
fn cb_asym_int(f, ispec, x, x0, k) = {
  let t = if is_list(ispec) then ispec[1] else ispec
  if not (t === x) and not (x0 === inf) then expand(integrate(normal(series(f, x, x0, k)), ispec))
  else {
    let big = integrate(f, ispec)
    # an antiderivative without a closed form at a finite point: the integrand's series (with its logarithms,
    # x^x = 1 + x log(x) + ...) integrated term by term to the order k (the constant of integration 0)
    if not free_of(big, integrate) and not is_list(ispec) and not (x0 === inf) then {
      let s = series(f, x, x0, k - 1)
      if free_of(normal(s), series) and not is_failure(s) then cb_upto(expand(integrate(expand(normal(s)), x)), x, x0, k) else normal(series(big, x, x0, k))
    } else normal(series(big, x, x0, k))
  }
}

# --- Laplace's method: integrals of g(t) exp(-x phi(t)) over [a, b] as x -> inf ---
# The minimum c of phi on [a, b] decides (Laplace's method and Watson's lemma; Olver, Asymptotics and Special
# Functions, ch. 3, 7-8; Bender and Orszag, Advanced Mathematical Methods, 6.4): with phi(c + tau) - phi(c) = u^p
# (p = 1 at an end where phi' > 0, p = 2 where phi' = 0 and phi'' > 0), tau = tau(u) by series reversion and
# G(u) = g(c + tau(u)) tau'(u) = sum a_m u^m, the integral is exp(-x phi(c)) times sum a_m m!/x^(m+1) (p = 1),
# sum a_m Gamma((m+1)/2)/(2 x^((m+1)/2)) (p = 2 at an end) or sum over even m of a_m Gamma((m+1)/2)/x^((m+1)/2) (p = 2
# inside). The other ends and points add exponentially smaller terms, which are left out. k terms are given (k even
# orders inside). null when f is not of that form or the minimum is not a single nondegenerate point.

# cb_revert(w, s, u, n): the series s(u) to order n with w(s(u)) = u, w a polynomial in s with w(0) = 0, w'(0) != 0
fn cb_revert(w, s, u, n) = {
  let w1 = coefficient(w, s, 1)
  let step = tt => cb_trunc((u - (expand(rewrite(w, s -> tt)) - w1*tt))/w1, u, 0, n)
  nest(step, u/w1, n + 1)
}
fn cb_laplace(f, t, a, b, x, k) = {
  let phi = simplify(-d(log(f), x))
  let g = simplify(f*exp(x*phi))
  if not free_of(phi, x) or not free_of(g, x) or not is_integer(k) or k < 1 then null else {
    let dphi = d(phi, t)
    # on a bounded interval inside it (the critical points of sin(t) on [0, pi], not the family pi/2 + pi c(1))
    let finite_ab = not (a === -inf) and not (b === inf)
    let sol = if free_of(dphi, t) then [] else solve(if finite_ab then dphi == 0 and a < t and t < b else dphi == 0, t, reals)
    let crit = if is_list(sol) then select(union(map(r => rewrite(t, r), sol)), c => is_numeric(n(c)) and free_of(c, t) and n(a) < n(c) and n(c) < n(b)) else []
    let ends = select([a, b], e => not (e === inf or e === -inf))
    let at = c => n(rewrite(phi, t -> c))
    let cands = join(crit, ends)
    let vals = map(at, cands)
    # an infinite end must not lower phi
    let tail_ok = every(select([a, b], e => e === inf or e === -inf), e => {
      let l = limit(phi, t -> e)
      l === inf or (is_numeric(n(l)) and n(l) > min(vals))
    })
    if length(cands) == 0 or not every(vals, is_real) or not tail_ok then null else {
      let mv = min(vals)
      let at_min = select(range(length(cands)), i => abs(vals[i] - mv) <= 10^-10*(1 + abs(mv)))
      if length(at_min) != 1 then null else {
        let i = at_min[1]
        let c = cands[i]
        let inside = i <= length(crit)
        # the end b as the start of s = b - t
        let upper = not inside and c === b and not (a === b)
        let ph = if upper then rewrite(phi, t -> b - cb_lt) else rewrite(phi, t -> c + cb_lt)
        let gg = if upper then rewrite(g, t -> b - cb_lt) else rewrite(g, t -> c + cb_lt)
        # p: the order of the first nonzero derivative at c (positive; even inside)
        let ds = table(n(simplify(rewrite(d(ph, [cb_lt, j]), cb_lt -> 0))), [j, 1, 6])
        let nz = select(range(6), j => not (abs(ds[j]) < 10^-12))
        let p = if length(nz) == 0 then 0 else if ds[nz[1]] > 0 and (not inside or is_even(nz[1])) then nz[1] else 0
        if p == 0 then null else {
          let nmax = if inside then 2*k - 1 else k - 1
          let psi = cb_trunc(ph - rewrite(ph, cb_lt -> 0), cb_lt, 0, nmax + p + 1)
          let r = cb_trunc((expand(psi/cb_lt^p))^(1/p), cb_lt, 0, nmax + 1)
          let w = expand(cb_lt*r)
          let tau = cb_revert(w, cb_lt, cb_lu, nmax + 1)
          let gu = cb_trunc(rewrite(gg, cb_lt -> tau)*d(tau, cb_lu), cb_lu, 0, nmax)
          let am = m => coefficient(gu, cb_lu, m)
          # int_0^inf u^m exp(-x u^p) du = gamma((m + 1)/p)/(p x^((m + 1)/p)); inside, both sides: the even m twice
          let terms = if inside then table(2*am(2*j)*gamma((2*j + 1)/p)/(p*x^((2*j + 1)/p)), [j, 0, k - 1])
            else table(am(m)*gamma((m + 1)/p)/(p*x^((m + 1)/p)), [m, 0, nmax])
          let e0 = simplify(rewrite(phi, t -> c))
          exp(-x*e0)*cb_total(map(simplify, terms))
        }
      }
    }
  }
}

# asymptotic_integrate(f, [t, a, b], [x, x0, k]): the expansion of the integral to order k in x at x0;
# asymptotic_integrate(f, x, [x, x0, k]) of the antiderivative; x -> x0 for the leading term
rule asymptotic_integrate(f_, ispec_, sp_) := {
  let lap = if sp[2] === inf and is_list(ispec) and length(ispec) == 3 and not (ispec[1] === sp[1]) then cb_laplace(f, ispec[1], ispec[2], ispec[3], sp[1], if cb_is_rule(sp) then 1 else sp[3]) else null
  if not (lap === null) then lap
  else if cb_is_rule(sp) then {
    let t = if is_list(ispec) then ispec[1] else ispec
    if not (t === sp[1]) and not (sp[2] === inf) then cb_lead(cb_asym_int(f, ispec, sp[1], sp[2], 4), sp[1], sp[2])
    else if not (sp[2] === inf) and not is_list(ispec) and not free_of(integrate(f, ispec), integrate) then cb_lead(cb_asym_int(f, ispec, sp[1], sp[2], 3), sp[1], sp[2])
    else cb_lead(integrate(f, ispec), sp[1], sp[2])
  } else cb_asym_int(f, ispec, sp[1], sp[2], sp[3])
} when cb_asym_spec(sp)

# asymptotic_sum(f, [k, a, b], [x, x0, m]): the expansion of the sum to order m in x at x0: the summand expanded in a
# parameter x at a finite point and summed term by term, else the series of the closed-form sum; x -> x0 for the
# leading term
rule asymptotic_sum(f_, kspec_, sp_) := {
  let x = sp[1]
  let x0 = sp[2]
  let k = if is_list(kspec) then kspec[1] else kspec
  let param = not (x === k) and not (x0 === inf) and not (is_list(kspec) and not free_of(rest(kspec), x))
  # over an infinite range the summand's expansion need not sum term by term (e^(-a k) at a = 0: the terms 1, k, ...
  # diverge): the closed-form sum expanded first; term by term only when every coefficient sum converges
  let infinite = is_list(kspec) and length(kspec) == 3 and (kspec[3] === inf or kspec[2] === -inf)
  let closed = if param and infinite then sum(f, kspec) else null
  if param and infinite and free_of(closed, sum) and not is_failure(closed) then {
    if cb_is_rule(sp) then cb_lead(closed, x, x0) else cb_series(closed, x, x0, sp[3])
  } else if param then {
    let r = expand(sum(normal(series(f, x, x0, if cb_is_rule(sp) then 3 else sp[3])), kspec))
    if infinite and (not free_of(r, sum) or not free_of(r, inf) or not free_of(r, complex_infinity) or is_failure(r)) then failure(unevaluated, asymptotic_sum, "the expansion of the summand does not sum term by term over the infinite range")
    else if cb_is_rule(sp) then cb_lead(r, x, x0) else r
  } else {
    let s = sum(f, kspec)
    if not free_of(s, sum) then failure(unevaluated, asymptotic_sum, "the sum has no closed form")
    else if cb_is_rule(sp) then cb_lead2(s, x, x0) else cb_series(s, x, x0, sp[3])
  }
} when cb_asym_spec(sp)

# asymptotic_product(f, [k, a, b], [x, x0, m]): the expansion of the closed-form product to order m in x at x0;
# x -> x0 for the leading term
rule asymptotic_product(f_, kspec_, sp_) := {
  let s = product(f, kspec)
  if not free_of(s, product) then failure(unevaluated, asymptotic_product, "the product has no closed form")
  else if cb_is_rule(sp) then cb_lead2(s, sp[1], sp[2]) else cb_series(s, sp[1], sp[2], sp[3])
} when cb_asym_spec(sp)
# series_term_goal: g with x -> inf: the leading term and g more terms of Stirling's series (the count for
# products: goal 2 gives three terms)
rule asymptotic_product(f_, kspec_, sp_, options(series_term_goal -> g_)) := {
  let s = product(f, kspec)
  let r = if free_of(s, product) then calc_st_value(s, sp[1], g + 1, "terms", 0) else null
  if not free_of(s, product) then failure(unevaluated, asymptotic_product, "the product has no closed form")
  else if r === null then asymptotic_product(f, kspec, sp) else r
} when cb_is_rule(sp) and sp[2] === inf and is_integer(g) and g >= 0

# cb_newton(fs, ys, cur, x, x0, k, it): the power-series solution ys(x) of fs == 0 from the start cur to order k by
# Newton's iteration on truncated series (each step doubles the order; the Jacobian is invertible at the point)
fn cb_trunc(t, x, x0, k) = expand(normal(series(t, x, x0, k)))
fn cb_newton_step(fs, ys, cur, x, x0, k) = {
  let sub = map(i => ys[i] -> cur[i], range(length(ys)))
  let fv = map(f => rewrite(f, sub), fs)
  let jac = map(f => map(y => rewrite(d(f, y), sub), ys), fs)
  let step = linear_solve(jac, fv)
  map(i => cb_trunc(cur[i] - step[i], x, x0, k), range(length(ys)))
}
fn cb_newton(fs, ys, cur, x, x0, k, it) = {
  let nxt = cb_newton_step(fs, ys, cur, x, x0, k)
  if it <= 0 or every(range(length(ys)), i => expand(nxt[i] - cur[i]) === 0) then nxt else cb_newton(fs, ys, nxt, x, x0, k, it - 1)
}
fn cb_eqs(eqs) = map(e => if head(e) === equal then e[1] - e[2] else e, if is_list(eqs) then eqs else if head(eqs) === head(cb_p and cb_q) then apply(list, eqs) else [eqs])
fn cb_regular(fs, ys, pt, x, x0) = {
  let sub = join(map(i => ys[i] -> pt[i], range(length(ys))), [x -> x0])
  not (simplify(det(map(f => map(y => rewrite(d(f, y), sub), ys), fs))) === 0)
}

# asymptotic_solve(eqs, [y, y0], [x, x0, k]): the solution y(x) of eqs through y(x0) = y0 as a series to order k;
# asymptotic_solve(eqs, [y], ...) for each root y0 of eqs at x0; asymptotic_solve(eqs, [[y1, y2], [a1, a2]], ...) for
# systems; regular points only (an invertible Jacobian in the ys)
rule asymptotic_solve(eqs_, yspec_, xspec_, opts___) := {
  let fs0 = cb_eqs(eqs)
  let several = is_list(yspec[1])
  let ys = if several then yspec[1] else [yspec[1]]
  let x = cb_shift
  let x0 = 0
  let fs = map(e => rewrite(e, xspec[1] -> xspec[2] + cb_shift), fs0)
  let k = xspec[3]
  let starts = if length(yspec) == 2 then [if several then yspec[2] else [yspec[2]]] else {
    let sols = solve(map(f => rewrite(f, x -> x0) == 0, fs), ys)
    map(s => map(y => rewrite(y, s), ys), sols)
  }
  # reals: the real branches (a regular branch through a real start has real coefficients when the equations do)
  let realq = some(flatten([opts]), o => o === reals)
  let starts1 = if realq and is_list(starts) then select(starts, pt => every(pt, v => im_part(n(v)) == 0)) else starts
  let good = if is_list(starts1) then select(starts1, pt => cb_regular(fs, ys, pt, x, x0)) else []
  # every branch or none: a start where the Jacobian is singular (y^5 - x y + x^2 at y = 0), or branches that leave
  # to infinity (x y^4 - (x + 1) y^2 + x == 1: the degree in y drops at x = 0) need Puiseux expansions in fractional
  # powers, which are not implemented; leaving them out would give an incomplete list
  let lost = length(ys) == 1 and length(fs) == 1 and polynomial_expression_q(fs[1], [ys[1]]) and exponent(rewrite(fs[1], x -> x0), ys[1]) < exponent(fs[1], ys[1])
  if not is_list(starts1) or length(starts) == 0 or length(good) < length(starts1) or lost then
    failure(unevaluated, asymptotic_solve, "a branch through a singular point or to infinity needs a Puiseux expansion")
  else map(pt => {
    let r = cb_newton(fs, ys, pt, x, x0, k, k + 2)
    map(i => ys[i] -> rewrite(r[i], cb_shift -> xspec[1] - xspec[2]), range(length(ys)))
  }, good)
} when is_list(yspec) and is_list(xspec) and length(xspec) == 3 and is_symbol(xspec[1]) and not (xspec[2] === inf)

# --- stability of equilibria: d_stability_conditions, r_stability_conditions ---

# cb_unknowns(vs, t): the unknown functions named by vs (y, y(t), or a list of them)
fn cb_unknowns(vs) = map(v => if is_symbol(v) then v else head(v), if is_list(vs) then vs else [vs])

# cb_order(eqs, y, t, disc): the highest derivative (disc false) or shift (disc true) of y in eqs, at most 8
fn cb_term(y, t, j, disc) = if j == 0 then y(t) else if disc then y(t + j) else derivative(j)(y)(t)
fn cb_order(eqs, y, t, disc) = last(join([0], select(range(8), j => not free_of(eqs, cb_term(y, t, j, disc)))))

# cb_first_order(eqs, ys, t, disc): the equations as a first-order system: the state [y_1, y_1', ..., y_2, ...] (for
# each unknown its value and derivatives or shifts below its order), and the right-hand sides of the next state,
# from solving the equations for the highest terms; null when that fails
fn cb_first_order(eqs, ys, t, disc) = {
  let es = if is_list(eqs) then eqs else [eqs]
  let ords = map(y => max(1, cb_order(es, y, t, disc)), ys)
  let tops = map(i => cb_term(ys[i], t, ords[i], disc), range(length(ys)))
  let sol = solve(es, tops)
  if not is_list(sol) or length(sol) == 0 then null else {
    let state = flat_map(i => table(cb_term(ys[i], t, j, disc), [j, 0, ords[i] - 1]), range(length(ys)))
    let names = flat_map(i => table(cb_stv(i, j), [j, 0, ords[i] - 1]), range(length(ys)))
    let back = map(k => state[k] -> names[k], range(length(state)))
    let nxt = flat_map(i => join(table(cb_stv(i, j + 1), [j, 0, ords[i] - 2]), [rewrite(rewrite(tops[i], sol[1]), back)]), range(length(ys)))
    [names, nxt]
  }
}

# cb_stability(eqs, vs, t, pts, disc): [[point, condition], ...]: at each equilibrium the eigenvalues of the
# Jacobian of the first-order system, the condition Re(lambda) < 0 (continuous) or |lambda| < 1 (discrete) for all of
# them, decided for numbers (Lyapunov's indirect method; boundary cases count as not stable)
fn cb_stab_cond(l, disc) = {
  let v = if disc then (if free_of(l, im) then -1 < l and l < 1 else abs(l) < 1) else re(l) < 0
  if is_numeric(l) then (if disc then n(abs(l)) < 1 else n(re(l)) < 0) else v
}
# cb_jury(jp): the conditions for all roots of the characteristic polynomial of the 1 x 1 or 2 x 2 matrix jp to lie in
# the open unit disk for real parameters: z^2 + a1 z + a0 has them when a0 < 1 and |a1| < 1 + a0 (Jury's test; Schur
# and Cohn), z + a0 when |a0| < 1
fn cb_jury(jp) = {
  let cp = coefficient_list(characteristic_polynomial(jp, cb_z), cb_z)
  let m = length(cp) - 1
  let a = map(c => simplify(c/cp[m + 1]), cp)
  if m == 1 then (-1 < simplify(-a[1]) and simplify(-a[1]) < 1)
  else cb_and([a[1] < 1, -1 - a[1] < a[2], a[2] < 1 + a[1]])
}
# cb_stable_edge(g, x, x0, l): a fixed point x0 of the map x -> g(x) whose multiplier l is 1 or -1 attracts when the
# Schwarzian derivative (third derivative over the first, less 3/2 the square of second over first) is negative
# there (l = -1), resp. when the second derivative is 0 and the third negative (l = 1); otherwise it is not
# asymptotically stable
fn cb_stable_edge(g, x, x0, l) = {
  let g1 = rewrite(d(g, x), x -> x0)
  let g2 = rewrite(d(g, x, x), x -> x0)
  let g3 = rewrite(d(g, x, x, x), x -> x0)
  if l == -1 then simplify(g3/g1 - 3/2*(g2/g1)^2) < 0 else (simplify(g2) == 0 and simplify(g3) < 0)
}
fn cb_stability(eqs, vs, t, pts, disc) = {
  let ys = cb_unknowns(vs)
  let fo = cb_first_order(eqs, ys, t, disc)
  let names = fo[1]
  let rhs = fo[2]
  let jac = map(g => map(s => d(g, s), names), rhs)
  let points = if pts === null then (if disc then r_fixed_points(eqs, vs, t) else d_fixed_points(eqs, vs, t)) else pts
  # a linearization that depends on n or t ((-1)^n): the eigenvalues of the Jacobian at one step do not decide (the
  # products over a period do); not implemented. (b^n a(n+1) a(n)^2 has the Jacobian 0 at 0: decided.)
  let moving = not free_of(rhs, t) and (not is_list(points) or some(points, pt => {
    let full = if length(pt) == length(names) then pt else join(pt, table(0, [length(names) - length(pt)]))
    not free_of(simplify(rewrite(jac, map(k => names[k] -> full[k], range(length(names))))), t)
  }))
  if moving then failure(unevaluated, if disc then r_stability_conditions else d_stability_conditions, "the equations are not autonomous: their coefficients depend on the variable")
  else {
  map(pt => {
    let full = if length(pt) == length(names) then pt else join(pt, table(0, [length(names) - length(pt)]))
    let jp = rewrite(jac, map(k => names[k] -> full[k], range(length(names))))
    let ls = map(simplify, eigenvalues(jp))
    let symbolic = disc and length(names) <= 2 and not every(ls, l => is_numeric(l))
    let edge = disc and length(names) == 1 and is_numeric(ls[1]) and simplify(abs(ls[1])) === 1
    [pt, if symbolic then cb_jury(jp) else (if edge then cb_stable_edge(rhs[1], names[1], full[1], ls[1]) else cb_and(map(l => cb_stab_cond(l, disc), ls)))]
  }, points)
  }
}

# d_stability_conditions(eqs, vs, t): for each equilibrium of the autonomous differential equations, [point,
# condition for asymptotic stability]; d_stability_conditions(eqs, vs, t, points) at the given points
rule d_stability_conditions(eqs_, vs_, t_) := cb_stability(eqs, vs, t, null, false) when is_symbol(t) and not (cb_first_order(eqs, cb_unknowns(vs), t, false) === null)
rule d_stability_conditions(eqs_, vs_, t_, pts_) := cb_stability(eqs, vs, t, pts, false) when is_symbol(t) and is_list(pts) and not (cb_first_order(eqs, cb_unknowns(vs), t, false) === null)

# r_stability_conditions(eqs, vs, n): the same for the recurrence equations, |lambda| < 1
rule r_stability_conditions(eqs_, vs_, t_) := cb_stability(eqs, vs, t, null, true) when is_symbol(t) and not (cb_first_order(eqs, cb_unknowns(vs), t, true) === null)
rule r_stability_conditions(eqs_, vs_, t_, pts_) := cb_stability(eqs, vs, t, pts, true) when is_symbol(t) and is_list(pts) and not (cb_first_order(eqs, cb_unknowns(vs), t, true) === null)

# --- identities of sequences: difference_root_reduce ---

# difference_root_reduce(lhs == rhs, n): true when the identity holds for the integers n = 0, ..., 40 (and false at
# the first integer where it fails); for holonomic sides of total order below 40 this decides the identity, since a
# holonomic sequence vanishing on more consecutive terms than its order (and its leading-coefficient zeros) is zero
fn cb_seq_ok(v) = is_numeric(v) and simplify(v) === 0
# e with the free occurrences of the symbol k replaced by v: pure functions (the defining function of a
# difference_root, whose bound variable may be k) are left alone
fn cb_subst_free(e, k, v) =
  if e === k then v
  else if is_atom(e) then e
  else if head(e) === `fn` then e
  else apply(cb_subst_free(head(e), k, v), map(u => cb_subst_free(u, k, v), apply(list, e)))
rule difference_root_reduce(eq_, k_, opts___) := {
  let vals = map(j => simplify(cb_subst_free(eq[1] - eq[2], k, j)), range(0, 40))
  if every(vals, v => v === 0) then true else if some(vals, v => is_numeric(v) and not (v === 0)) then false else difference_root_reduce(eq, k)
} when head(eq) === equal and is_symbol(k) and every(map(j => cb_subst_free(eq[1] - eq[2], k, j), range(0, 3)), is_numeric)

# --- Green's functions of second-order linear boundary and initial value problems: green_function ---

# cb_bc(bc, u, f, x): the boundary functional bc (an equation in u(p) and u'(p)) applied to the function f of x
fn cb_bc(bc, u, f, x) = simplify(rewrite(bc[1] - bc[2], u -> (z => rewrite(f, x -> z))))
# cb_bc_point(bc): the point p of u(p) or u'(p) on the left side of bc
fn cb_bc_point(bc) = bc[1][1]

# green_function([L, bc1, bc2], u(x), [x, a, b], y): the Green's function G(x, y) of the second-order linear
# differential operator L (an expression in u(x), u'(x), u''(x)) with two homogeneous conditions: L G = delta(x - y).
# Conditions at one point (an initial value problem) give the causal G = theta(x - y) (u1(y) u2(x) - u1(x) u2(y))/
# (a2(y) W(y)); conditions at two points give G = ua(min) ub(max)/(a2(y) W(ua, ub)(y)) with ua, ub the homogeneous
# solutions satisfying the left and right conditions (variation of parameters; Stakgold, Green's Functions and
# Boundary Value Problems, 3.2)
rule green_function(eqs_, ux_, xspec_, y_) := {
  let es = if is_list(eqs) then eqs else [eqs]
  let op = if head(es[1]) === equal then es[1][1] - es[1][2] else es[1]
  let bcs = rest(es)
  let u = head(ux)
  let x = xspec[1]
  let a2 = coefficient(expand(op), derivative(2)(u)(x))
  let sol = rewrite(u(x), d_solve(op == 0, u(x), x)[1])
  let u1 = coefficient(expand(sol), c(1))
  let u2 = coefficient(expand(sol), c(2))
  let at = (f, z) => rewrite(f, x -> z)
  let a2y = at(a2, y)
  if cb_bc_point(bcs[1]) === cb_bc_point(bcs[2]) then {
    let w = at(u1*d(u2, x) - d(u1, x)*u2, y)
    simplify((at(u1, y)*u2 - u1*at(u2, y))/(a2y*w))*heaviside_theta(x - y)
  } else {
    let ua = expand(cb_bc(bcs[1], u, u2, x)*u1 - cb_bc(bcs[1], u, u1, x)*u2)
    let ub = expand(cb_bc(bcs[2], u, u2, x)*u1 - cb_bc(bcs[2], u, u1, x)*u2)
    let w = at(ua*d(ub, x) - d(ua, x)*ub, y)
    simplify(at(ua, y)*ub/(a2y*w))*heaviside_theta(x - y) + simplify(ua*at(ub, y)/(a2y*w))*heaviside_theta(y - x)
  }
} when is_list(eqs) and length(eqs) == 3 and is_list(xspec) and is_symbol(y) and not is_atom(ux)

# green_function(L, u(x, t), [x, -inf, inf], t, [y, s]): the causal fundamental solution of a constant-coefficient
# heat or wave operator on the whole line, L G = dirac_delta(x - y) dirac_delta(t - s) (Evans, Partial Differential
# Equations, 2.3.1 and 2.4.1): for a u_t - b u_xx (k = b/a > 0) heaviside_theta(t - s) exp(-(x - y)^2/(4 k (t -
# s)))/(2 a sqrt(pi) sqrt(k (t - s))); for a u_tt - b u_xx (c^2 = b/a > 0) heaviside_theta(t - s - abs(x - y)/c)/(2 a
# c). Another operator (lower-order terms, variable coefficients) is not done.
fn cb_green_pde(op0, ux, xspec, t, ys) = {
  let op = expand(if is_list(op0) then op0[1] else op0)
  let u = head(ux)
  let x = xspec[1]
  let ut = coefficient(op, derivative(0, 1)(u)(x, t))
  let utt = coefficient(op, derivative(0, 2)(u)(x, t))
  let uxx = coefficient(op, derivative(2, 0)(u)(x, t))
  let rest = expand(op - ut*derivative(0, 1)(u)(x, t) - utt*derivative(0, 2)(u)(x, t) - uxx*derivative(2, 0)(u)(x, t))
  let consts = every([ut, utt, uxx], c => is_numeric(c) and is_real(c))
  let y = ys[1]
  let s = ys[2]
  if not (rest === 0) or not consts then null
  else if utt === 0 and not (ut === 0) and -uxx/ut > 0 then {
    let k = -uxx/ut
    heaviside_theta(t - s)*exp(-(x - y)^2/(4*k*(t - s)))/(2*ut*sqrt(pi)*sqrt(k*(t - s)))
  }
  else if ut === 0 and not (utt === 0) and -uxx/utt > 0 then heaviside_theta(t - s - abs(x - y)/sqrt(-uxx/utt))/(2*utt*sqrt(-uxx/utt))
  else null
}
rule green_function(op_, ux_, xspec_, t_, ys_) := cb_green_pde(op, ux, xspec, t, ys) when not is_atom(ux) and length(ux) == 2 and is_list(xspec) and length(xspec) == 3 and xspec[2] === -inf and xspec[3] === inf and is_symbol(t) and apply(list, ux) === [xspec[1], t] and is_list(ys) and length(ys) == 2 and (not is_list(op) or length(op) == 1) and not (cb_green_pde(op, ux, xspec, t, ys) === null)

# ====== formerly the round 31 library file (merged in round 34; loaded after the libraries above, before the round 30
# ones, whose helpers it calls only when its functions run) ======
# Calculus library of round 31: functions written in MathCAS itself, evaluated after the round 30 coverage
# library (its helpers c30_sph_coefs and c30_sph_j_all are used). Helper functions are prefixed `cf_`.
# Methods and literature: crates/mc-calculus/NOTES.md, section "Round 31".

# --- spheroidal_s1_prime ---
# The z-derivative of the radial spheroidal function of the first kind. With S1(z) = P(z) N(z)/D, P = (z^2 - 1)^(m/2)
# z^(-m), N = sum a_k w_k j_v(gamma z) (v = n + 2k) and D = sum (-1)^k a_k w_k (see spheroidal_s1):
# S1' = P' N/D + P gamma sum a_k w_k j_v'(gamma z)/D, P'/P = m/(z (z^2 - 1)), and j_v'(x) = j_(v-1)(x) - (v + 1) j_v(x)/x
# for v >= 1, j_0' = -j_1 (DLMF 10.51.2).
fn cf_s1p(nn, m, g, z) = {
  let p = if is_machine_number(n(abs(z))) and is_machine_number(n(abs(g))) then 16 else min(precision(z), precision(g))
  let lam = spheroidal_eigenvalue(nn, m, g)
  let kk = 20 + ceiling(abs(n(g))) + floor(p/2)
  let a = c30_sph_coefs(nn, m, g^2, lam, kk)
  let k0 = -floor((nn - m)/2)
  let x = g*z
  let js = c30_sph_j_all(nn + 2*kk + 1, x)
  let w = k => factorial(nn + m + 2*k)/factorial(nn - m + 2*k)
  let nu = i => nn + 2*(k0 + i - 1)
  let jp = i => if nu(i) == 0 then -js[2] else js[nu(i)] - (nu(i) + 1)*js[nu(i) + 1]/x
  let num = total(map(i => a[i]*w(k0 + i - 1)*js[nu(i) + 1], range(length(a))))
  let dnum = total(map(i => a[i]*w(k0 + i - 1)*jp(i), range(length(a))))
  let den = total(map(i => (-1)^(k0 + i - 1)*a[i]*w(k0 + i - 1), range(length(a))))
  let pz = (z^2 - 1)^(m/2)*z^(-m)
  c30_cx(pz*(m/(z*(z^2 - 1))*num + g*dnum)/den)
}

rule spheroidal_s1_prime(nn_, m_, g_, z_) := map(w => spheroidal_s1_prime(nn, m, g, w), z) when is_list(z)
rule spheroidal_s1_prime(nn_, m_, g_, z_) := cf_s1p(round(nn), round(m), g, z) when c30_intq(nn) and c30_intq(m) and nn >= m and m >= 0 and is_numeric(g) and is_numeric(z) and not is_list(z) and (is_inexact(g) or is_inexact(z)) and z != 0 and g != 0 and abs(z) != 1

# --- differences with a step and along several levels ---

# differences(xs, n, s): the n-th differences with step s, x[i + s] - x[i] each time (the native handles step 1);
# differences(xs, [n1, n2, ...]): n_d differences along level d of an array
rule differences(xs_, n_, s_) := nest(l => map(i => l[i + s] - l[i], range(length(l) - s)), xs, n) when is_list(xs) and is_integer(n) and n >= 0 and is_integer(s) and s >= 1
fn calc_diff_level(xs, d, n) = if d == 1 then differences(xs, n) else map(r => calc_diff_level(r, d - 1, n), xs)
rule differences(xs_, ns_) := fold((a, d) => (if ns[d] == 0 then a else calc_diff_level(a, d, ns[d])), xs, range(length(ns))) when is_list(xs) and is_list(ns) and length(ns) > 0 and every(ns, is_integer)

# difference_quotient of a falling power with the step of the quotient: n factorial_power(x, n - 1, h) (the step-h
# factorial power x (x - h) ... (x - (n - 1) h) satisfies Delta_h x^(n, h) = n h x^(n - 1, h))
rule difference_quotient(factorial_power(x_, n_, h_), [x_, h_]) := n*factorial_power(x, n - 1, h) when is_symbol(x) and is_symbol(h) and not is_integer(n)

# a sum of a list-valued summand over an infinite range is taken elementwise
rule sum(f_, sp_) := map(g => sum(g, sp), f) when is_list(f) and is_list(sp) and length(sp) == 3 and sp[3] === inf
# the angle of the origin is indeterminate
rule atan(x_, y_) := indeterminate when x === 0 and y === 0

# calc_to_radicals(e): every root(p, k) in e whose polynomial p has degree 2 to 4 and numeric coefficients, replaced by
# the radical expression (from roots) nearest to the numeric value of the k-th root; others are kept
fn calc_to_rad_root(e) = {
  let p = e[1]
  let k = e[2]
  let poly = p(calc_trx)
  let eqs = roots(poly == 0, calc_trx)
  let vals = if head(eqs) === head(cb_a or cb_b) then map(q => q[2], apply(list, eqs)) else (if head(eqs) === equal then [eqs[2]] else [])
  let target = n(e)
  let deg = degree(poly, calc_trx)
  if length(vals) == deg and deg >= 2 and deg <= 4 and is_numeric(target) and free_of(vals, root) then {
    let errs = map(v => abs(n(v) - target), vals)
    vals[first(select(range(length(vals)), i => errs[i] == min(errs)))]
  } else e
}
fn calc_to_radicals(e) =
  if is_atom(e) then e
  else if head(e) === root and length(e) == 2 then calc_to_rad_root(e)
  else apply(head(e), map(calc_to_radicals, apply(list, e)))

# to_radicals(e, options): cubics: false keeps the roots of irreducible cubics other than a x^3 + b (the ones that
# need Cardano's formula), quartics: false those of irreducible quartics other than a x^4 + b x^2 + c (Ferrari's);
# assumptions: (none for no assumptions) decide the roots of a polynomial with parameters: its roots from roots, the
# real ones by the assumptions, root(p, k) the k-th smallest real root when every comparison needed is decided by
# the assumptions (k past the real roots, or a comparison left open, keeps root(p, k))
fn calc_trad_opts(opts) = {
  let rs = flatten(map(o => if not is_atom(o) and head(o) === options then apply(list, o) else [o], opts))
  let known = every(rs, r => match_q(r, cubics -> _) or match_q(r, quartics -> _) or match_q(r, assumptions -> _))
  let get = (nm, dflt) => {
    let hit = select(rs, r => r[1] === nm)
    if length(hit) == 0 then dflt else hit[1][2]
  }
  if not known or length(rs) == 0 then null else {
    let asm = get(assumptions, true)
    [not (get(cubics, true) === false), not (get(quartics, true) === false), if asm === none then true else asm]
  }
}
fn calc_trad_plain(poly, x, deg) = {
  let cs = coefficient_list(poly, x)
  let mid = if deg == 3 then [cs[2], cs[3]] else [cs[2], cs[4]]
  every(mid, c => c === 0)
}
fn calc_to_rad_root_o(e, o) = {
  let poly = e[1](calc_trx)
  let deg = degree(poly, calc_trx)
  if not is_numeric(n(e)) then calc_to_rad_param(e, o[3])
  else if (deg == 3 and not o[1] or deg == 4 and not o[2]) and not calc_trad_plain(poly, calc_trx, deg) then e
  else calc_to_rad_root(e)
}
fn calc_to_rad_param(e, asm) = {
  let k = e[2]
  let eqs = roots(e[1](calc_trx) == 0, calc_trx)
  let vals = if head(eqs) === head(cb_a or cb_b) then map(q => q[2], apply(list, eqs)) else (if head(eqs) === equal then [eqs[2]] else [])
  let deg = degree(e[1](calc_trx), calc_trx)
  let ps = select(variables(e[1](calc_trx)), v => not (v === calc_trx))
  # a statement c decided true by simplify or, with one parameter, by reduce of its negation over the reals
  let holds = c => simplify(c, assumptions: asm) === true or (length(ps) == 1 and reduce(asm and not c, ps[1], reals) === false)
  # real by simplify; not real when v^2 is real and negative
  let isreal = map(v => if simplify(element(v, reals), assumptions: asm) === true then true else if simplify(element(expand(v^2), reals), assumptions: asm) === true and holds(expand(v^2) < 0) then false else null, vals)
  if length(vals) != deg or not free_of(vals, root) or not every(isreal, r => r === true or r === false) or not is_integer(k) then e else {
    var rest = select(range(deg), i => isreal[i] === true)
    var pick = null
    var ok = true
    for step in range(k) {
      if ok then {
        let ms = select(rest, i => every(rest, j => i === j or holds(vals[i] <= vals[j])))
        if length(ms) == 0 then ok = false else {
          pick = vals[ms[1]]
          rest = select(rest, j => not (j === ms[1]))
        }
      }
    }
    if ok and not (pick === null) then pick else e
  }
}
fn calc_to_radicals_o(e, o) =
  if is_atom(e) then e
  else if head(e) === root and length(e) == 2 then calc_to_rad_root_o(e, o)
  else apply(head(e), map(t => calc_to_radicals_o(t, o), apply(list, e)))
rule to_radicals(e_, opts___) := calc_to_radicals_o(e, calc_trad_opts(opts)) when calc_trad_opts(opts) !== null

# eigenvalues(m, cubics: true), (m, k, quartics: true): the eigenvalues with the roots of cubics (quartics) written
# with radicals
fn calc_eig_rad(opts) = {
  let rs = flatten(map(o => if not is_atom(o) and head(o) === options then apply(list, o) else [o], opts))
  if length(rs) > 0 and every(rs, r => match_q(r, cubics -> true) or match_q(r, quartics -> true)) then [some(rs, r => r[1] === cubics), some(rs, r => r[1] === quartics), true] else null
}
rule eigenvalues(m_, opts___) := calc_to_radicals_o(eigenvalues(m), calc_eig_rad(opts)) when is_list(m) and calc_eig_rad(opts) !== null
rule eigenvalues(m_, k_, opts___) := calc_to_radicals_o(eigenvalues(m, k), calc_eig_rad(opts)) when is_list(m) and is_integer(k) and calc_eig_rad(opts) !== null

# derivatives of the zeta function at 0: zeta'(0) = -log(2 pi)/2 (from the functional equation), and zeta''(0) =
# (12 gamma^2 - pi^2 - 12 log(2 pi)^2 + 24 gamma_1)/24 (Stieltjes constant gamma_1; the Laurent coefficients of the
# functional equation at s = 0)
rule derivative(1)(zeta)(0) := -log(2*pi)/2
rule derivative(2)(zeta)(0) := (12*euler_gamma^2 - pi^2 - 12*log(2*pi)^2 + 24*stieltjes_gamma(1))/24

# the Weierstrass product: prod_(n >= 1) (1 + z/n) exp(-z/n) = exp(-euler_gamma z)/gamma(1 + z) (Whittaker and Watson, 12.1)
rule product(f_, [n_, 1, inf]) := {
  let efs = select(calc_factors(f), t => head(t) === pow and t[1] === e and not free_of(t[2], n))
  let w = if length(efs) == 1 then -efs[1][2] else null
  let z = if w === null then null else simplify(n*w)
  if z === null or not free_of(z, n) or not (simplify(f/efs[1] - 1 - w) === 0) then null else exp(-euler_gamma*z)/gamma(1 + z)
} when not (head(f) === pow) and is_symbol(n) and calc_is_weierstrass(f, n)
fn calc_is_weierstrass(f, n) = {
  let efs = select(calc_factors(f), t => head(t) === pow and t[1] === e and not free_of(t[2], n))
  if length(efs) != 1 then false else {
    let w = -efs[1][2]
    let z = simplify(n*w)
    free_of(z, n) and simplify(f/efs[1] - 1 - w) === 0
  }
}

# --- vector calculus in orthogonal coordinates: grad, div, curl, laplacian with a coordinate system ---
# grad(f, q, sys), div(v, q, sys), curl(v, q, sys), laplacian(f, q, sys) for sys "Cartesian", "Polar" (r, t),
# "Cylindrical" (r, t, z) and "Spherical" (r, t, p: t the angle from the z axis, p the azimuth), from the scale
# factors h_i of the coordinates (Morse and Feshbach, Methods of Theoretical Physics, 1953, section 1.4): the
# gradient (1/h_i) df/dq_i; the divergence (1/H) sum d(H v_i/h_i)/dq_i with H = h_1 h_2 h_3; the curl
# (1/(h_j h_k)) (d(h_k v_k)/dq_j - d(h_j v_j)/dq_k) for (i, j, k) cyclic (in the plane the scalar
# (1/(h_1 h_2)) (d(h_2 v_2)/dq_1 - d(h_1 v_1)/dq_2)); the Laplacian (1/H) sum d((H/h_i^2) df/dq_i)/dq_i
fn vc_scale(sys, q) =
  if not (is_string(sys) and is_list(q)) then null
  else if sys == "Cartesian" then constant_array(1, length(q))
  else if sys == "Polar" and length(q) == 2 then [1, q[1]]
  else if sys == "Cylindrical" and length(q) == 3 then [1, q[1], 1]
  else if sys == "Spherical" and length(q) == 3 then [1, q[1], q[1]*sin(q[2])]
  else null
fn vc_ok(sys, q) = not (vc_scale(sys, q) === null) and every(q, x => is_symbol(x))
rule grad(f_, q_, sys_) := { let hs = vc_scale(sys, q); map(i => simplify(d(f, q[i])/hs[i]), range(length(q))) } when vc_ok(sys, q) and not is_list(f)
rule div(v_, q_, sys_) := {
  let hs = vc_scale(sys, q)
  let big = apply(mul, hs)
  simplify(total(map(i => d(big*v[i]/hs[i], q[i]), range(length(q))))/big)
} when vc_ok(sys, q) and is_list(v) and length(v) == length(q)
rule curl(v_, q_, sys_) := {
  let hs = vc_scale(sys, q)
  if length(q) == 2 then simplify((d(hs[2]*v[2], q[1]) - d(hs[1]*v[1], q[2]))/(hs[1]*hs[2]))
  else map(i => {
    let j = mod(i, 3) + 1
    let k = mod(i + 1, 3) + 1
    simplify((d(hs[k]*v[k], q[j]) - d(hs[j]*v[j], q[k]))/(hs[j]*hs[k]))
  }, range(3))
} when vc_ok(sys, q) and is_list(v) and length(v) == length(q) and (length(q) == 2 or length(q) == 3)
rule laplacian(f_, q_, sys_) := {
  let hs = vc_scale(sys, q)
  let big = apply(mul, hs)
  simplify(total(map(i => d(big/hs[i]^2*d(f, q[i]), q[i]), range(length(q))))/big)
} when vc_ok(sys, q) and not is_list(f)
