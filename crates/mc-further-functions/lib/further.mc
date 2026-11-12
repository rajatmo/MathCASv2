#! mathcas 2
# Round 30 coverage library, evaluated after the other libraries, defined in every session.
# Helper functions are prefixed `c30_` and not part of the interface. Methods and literature:
# crates/mc-further-functions/NOTES.md.

# --- julia_set_iteration_count ---

# c30_julia(f, z, pts, maxit): the counts of the native for the rational map f of z
fn c30_julia(f, z, pts, maxit, wp) = {
  let g = together(f)
  c30_julia_counts(coefficient_list(numerator(g), z), coefficient_list(denominator(g), z), pts, maxit, wp)
}

# julia_set_iteration_count(f, z, z0): the number of iterations of the rational map f of z after which z0 is caught
# by an attracting cycle of f (or escapes, for a polynomial); max_iterations + 1 when it is not caught;
# julia_set_iteration_count(c, z0) for z^2 + c. z0 may be a list or an array of points. Exact points of a
# polynomial map, and every point with working_precision: d (digits), are iterated in ball arithmetic with the
# precision raised until every decision is certain.
fn julia_set_iteration_count(f, z, z0; max_iterations = 1000, working_precision = machine) when is_symbol(z) and is_numeric(n(rewrite(f, z -> 1/3))) =
  c30_julia(f, z, z0, max_iterations, working_precision)
fn julia_set_iteration_count(c, z0; max_iterations = 1000, working_precision = machine) when is_numeric(c) =
  c30_julia_counts([c, 0, 1], [1], z0, max_iterations, working_precision)

# --- meijer_g_reduce, fox_h_reduce ---
# The representations come from the Mellin transforms of the functions (Luke, The special functions and their
# approximations I, 5.6; Prudnikov, Brychkov, Marichev, Integrals and series 3, 8.4; Adamchik and Marichev,
# ISSAC 1990). c30_gt(a, b, z, r) is G^(m,n)_(p,q)(z^(1/r)) with a = [[a_1..a_n], [a_(n+1)..a_p]] and
# b = [[b_1..b_m], [b_(m+1)..b_q]]; at the end it becomes inactive(meijer_g)(a, b, z, r) (r = 1 left out) or, with
# G(z^(1/r)) = |r| H(z^(sign r)) at the scales |r|, inactive(fox_h).

# c30_split(e, x): [the constant part, the rest] of a sum
fn c30_split(e, x) = {
  let ts = if not is_atom(e) and head(e) === add then apply(list, e) else [e]
  let c = total(select(ts, t => free_of(t, x)))
  [c, e - c]
}

# c30_xpow(f, x): the exponent of a pure power of x, or none
fn c30_xpow(f, x) = if f === x then 1 else if not is_atom(f) and head(f) === pow and f[1] === x and free_of(f[2], x) then f[2] else none

# c30_atom(f, x): the reduction of one factor f of x, as c30_gt terms, or none
fn c30_atom(f, x) =
  if is_atom(f) then none
  else {
    let h = head(f)
    let u = f[length(f)]
    if h === sin then sqrt(pi)*c30_gt([[], []], [[1/2], [0]], u/2, 1/2)
    else if h === cos then sqrt(pi)*c30_gt([[], []], [[0], [1/2]], u/2, 1/2)
    else if h === sinh then sqrt(2)*pi^(3/2)*c30_gt([[], [3/4]], [[1/2], [3/4, 0]], u/2, 1/2)
    else if h === cosh then sqrt(2)*pi^(3/2)*c30_gt([[], [3/4]], [[0], [3/4, 1/2]], u/2, 1/2)
    else if h === log and length(f) == 1 then c30_gt([[1, 1], []], [[1], [0]], u, 1) - c30_gt([[1, 1], []], [[1], [0]], u, -1)
    else if h === asin then (-im/2)*pi^(-1/2)*c30_gt([[1, 1], []], [[1/2], [0]], im*u, 1/2)
    else if h === atanh then (-im/2)*c30_gt([[1/2, 1], []], [[1/2], [0]], im*u, 1/2)
    else if h === airy_ai then 3^(-1/6)/(2*pi)*c30_gt([[], []], [[0, 1/3], []], 3^(-2/3)*u, 1/3)
    else if h === airy_bi then 2*3^(-1/6)*pi*c30_gt([[], [1/6, 2/3]], [[0, 1/3], [1/6, 2/3]], 3^(-2/3)*u, 1/3)
    else if h === bessel_j and length(f) == 2 then c30_gt([[], []], [[f[1]/2], [-f[1]/2]], u/2, 1/2)
    else if h === bessel_k and length(f) == 2 then c30_gt([[], []], [[f[1]/2, -f[1]/2], []], u/2, 1/2)/2
    else if h === hypergeometric_1f1 then gamma(f[2])/gamma(f[1])*c30_gt([[1 - f[1]], []], [[0], [1 - f[2]]], -u, 1)
    else if h === hypergeometric_2f1 then gamma(f[3])/(gamma(f[1])*gamma(f[2]))*c30_gt([[1 - f[1], 1 - f[2]], []], [[0], [1 - f[3]]], -u, 1)
    else if h === elliptic_k and length(f) == 1 then c30_gt([[1/2, 1/2], []], [[0], [0]], -u, 1)/2
    else if h === elliptic_e and length(f) == 1 then -c30_gt([[1/2, 3/2], []], [[0], [0]], -u, 1)/4
    else if h === sin_integral then sqrt(pi)/2*c30_gt([[1], []], [[1/2], [0, 0]], u/2, 1/2)
    else if h === exp_integral_ei then -im*pi - c30_gt([[], [1]], [[0, 0], []], -u, 1)
    else if h === unit_box then c30_gt([[], [1]], [[0], []], 2*rewrite(u, abs(x) -> x), 1)
    else if h === unit_step and length(f) == 1 then c30_step(rewrite(u, abs(x) -> x), x)
    else if h === legendre_p and length(f) == 2 then conditional_expression(-sin(f[1]*pi)/pi*c30_gt([[1 + f[1], -f[1]], []], [[0], [0]], (u - 1)/2, 1), not_element(f[1], integers))
    else if h === legendre_q and length(f) == 2 then conditional_expression((c30_gt([[1 + f[1], -f[1]], []], [[0], [0]], (-1 - u)/2, 1) - cos(f[1]*pi)*c30_gt([[1 + f[1], -f[1]], []], [[0], [0]], (u - 1)/2, 1))/2, not_element(f[1], integers))
    else if h === pow and f[1] === e then c30_exp(f[2], x, false)
    else if h === pow and f[1] === sqrt(x) + sqrt(1 + x) and f[2] === -1/2 then pi^(-1/2)/4*c30_gt([[3/4], [5/4]], [[0, 1/2], []], x, 1)
    else if h === pow and free_of(f[2], x) then c30_binomial(f[1], -f[2], x)
    else none
  }

# e^w: the duplication form G^(2,0)_(0,2)(w^2/4 | 0, 1/2)/sqrt(pi); e^(-x^a) = G^(1,0)_(0,1)(x^a | 0) when a is not
# a number or a power of x multiplies it (simple)
fn c30_exp(w, x, simple) =
  if free_of(w, x) then none
  else if c30_xpow(-w, x) !== none and (simple or not is_number(c30_xpow(-w, x))) then c30_gt([[], []], [[0], []], -w, 1)
  else pi^(-1/2)*c30_gt([[], []], [[0, 1/2], []], -w/2, 1/2)

# (c + r)^(-s) = c^(-s)/gamma(s) G^(1,1)_(1,1)(r/c | 1 - s; 0)
fn c30_binomial(b, s, x) = {
  let p = c30_split(b, x)
  if p[1] === 0 or free_of(p[2], x) then none
  else p[1]^(-s)/gamma(s)*c30_gt([[1 - s], []], [[0], []], p[2]/p[1], 1)
}

# unit_step(c + r): theta(r/|c| - 1) for c < 0, theta(1 - (-r)/c) for c > 0
fn c30_step(w, x) = {
  let p = c30_split(w, x)
  if is_number(p[1]) and p[1] < 0 then c30_gt([[1], []], [[], [0]], -p[2]/p[1], 1)
  else if is_number(p[1]) and p[1] > 0 then c30_gt([[], [1]], [[0], []], -p[2]/p[1], 1)
  else none
}

# c30_pair(f, g, x): the reductions of products of two factors that have a single representation
fn c30_pair(f, g, x) =
  if is_atom(f) or is_atom(g) then none
  else if head(f) === asin and g === (1 - f[1]^2)^(-1/2) then (-im/2)*sqrt(pi)*c30_gt([[1/2, 1/2], []], [[1/2], [0]], im*f[1], 1/2)
  else if head(f) === bessel_j and head(g) === bessel_j and length(f) == 2 and length(g) == 2 and f[2] === g[2] then pi^(-1/2)*c30_gt([[0, 1/2], []], [[(f[1] + g[1])/2], [-(f[1] + g[1])/2, (f[1] - g[1])/2, (g[1] - f[1])/2]], f[2], 1/2)
  else if head(f) === acsc and head(g) === unit_step and rewrite(g[1], abs(x) -> x) === f[1] - 1 then (pi*c30_gt([[1], []], [[], [0]], f[1], 1/2) - sqrt(pi)*c30_gt([[1, 1/2], []], [[], [0, 0]], f[1], 1/2))/2
  else if head(f) === log and head(g) === unit_step and rewrite(g[1], abs(x) -> x) === 1 - f[1] then -c30_gt([[], [1, 1]], [[0, 0], []], f[1], 1)
  else none

# whether two factors are asin(u) and (1 - u^2)^(-1/2) (the Fox form keeps the second as a factor)
fn c30_asin_pair(sp, x) = some(sp, f => not is_atom(f) and head(f) === asin)

# c30_term(t, x, fox): the reduction of one term of a sum (none if a factor has no representation)
fn c30_term(t, x, fox) = {
  let fs = if not is_atom(t) and head(t) === mul then apply(list, t) else [t]
  let c = product(select(fs, f => free_of(f, x)))
  let pw = select(fs, f => not free_of(f, x) and c30_xpow(f, x) !== none)
  let sp = select(fs, f => not free_of(f, x) and c30_xpow(f, x) === none)
  let xp = total(map(f => c30_xpow(f, x), pw))
  if length(sp) == 0 then none
  else if length(sp) == 1 and xp !== 0 and not is_atom(sp[1]) and head(sp[1]) === pow and sp[1][1] === e and c30_exp(sp[1][2], x, true) !== none then {
    let g = c30_exp(sp[1][2], x, true)
    if fox then c*c30_absorb(g, xp, x) else c*x^xp*g
  }
  else if length(sp) == 2 and not (fox and c30_asin_pair(sp, x)) and c30_pair(sp[1], sp[2], x) !== none then c*x^xp*c30_pair(sp[1], sp[2], x)
  else if length(sp) == 2 and not (fox and c30_asin_pair(sp, x)) and c30_pair(sp[2], sp[1], x) !== none then c*x^xp*c30_pair(sp[2], sp[1], x)
  else if fox and length(sp) == 2 and not is_atom(sp[1]) and head(sp[1]) === asin and sp[2] === (1 - sp[1][1]^2)^(-1/2) then c*x^xp*sp[2]*c30_atom(sp[1], x)
  else if fox and length(sp) == 2 and not is_atom(sp[2]) and head(sp[2]) === asin and sp[1] === (1 - sp[2][1]^2)^(-1/2) then c*x^xp*sp[1]*c30_atom(sp[2], x)
  else {
    let rs = map(f => c30_atom(f, x), sp)
    if some(rs, r => r === none) or (length(rs) > 1 and not fox) then none
    else if fox and xp !== 0 and length(rs) == 1 then c*c30_absorb(rs[1], xp, x)
    else c*x^xp*product(rs)
  }
}

# the factor of z free of x
fn c30_kcoef(z, x) = {
  let fs = if not is_atom(z) and head(z) === mul then apply(list, z) else [z]
  product(select(fs, f => free_of(f, x)))
}

# x^s times a reduction: for a single c30_gt(a, b, k x^p, r), z^sigma G(z^(1/r)) is G at the parameters shifted by
# sigma r... written through the Fox form: x^s = (z/k)^(s/p) and z^t H(z | (a, A)) = H(z | (a + t A, A)) with the
# scales A = |r|; otherwise the power stays a factor
fn c30_absorb(g, s, x) =
  if not is_atom(g) and head(g) === c30_gt and g[4] > 0 and c30_xpow(g[3]/c30_kcoef(g[3], x), x) !== none then {
    let k = c30_kcoef(g[3], x)
    let p = c30_xpow(g[3]/k, x)
    let t = s/p
    k^(-t)*c30_gt(map(l => map(v => v + t*g[4], l), g[1]), map(l => map(v => v + t*g[4], l), g[2]), g[3], g[4])
  }
  else if not is_atom(g) and head(g) === mul then {
    let gs = apply(list, g)
    let ix = select(range(length(gs)), j => not is_atom(gs[j]) and head(gs[j]) === c30_gt)
    if length(ix) == 1 then product(map(j => if j == ix[1] then c30_absorb(gs[j], s, x) else gs[j], range(length(gs)))) else x^s*g
  }
  else x^s*g

# c30_reduce(e, x, fox): the reduction of e, or none
fn c30_reduce(e, x, fox) =
  if not is_atom(e) and head(e) === conditional_expression then {
    let r = c30_reduce(e[1], x, fox)
    if r === none then none else conditional_expression(r, e[2])
  }
  else {
    let ts = if not is_atom(e) and head(e) === add then apply(list, e) else [e]
    let rs = map(t => if free_of(t, x) then t else c30_term(t, x, fox), ts)
    if some(rs, r => r === none) then none else total(rs)
  }

# a Fox H argument k x^p with a symbolic p: H(k x^p | (a, A)) = H(k^(1/p) x | (a, A/p))/p for p > 0
fn c30_hx(a, b, z, x) = {
  let k = c30_kcoef(z, x)
  let p = c30_xpow(z/k, x)
  if p === none or is_number(p) then inactive(fox_h)(a, b, z)
  else conditional_expression(inactive(fox_h)(map(l => map(v => [v[1], v[2]/p], l), a), map(l => map(v => [v[1], v[2]/p], l), b), k^(1/p)*x)/p, p > 0)
}

# c30_to_g, c30_to_h: the final forms
fn c30_ex(a) = map(l => map(expand, l), a)
fn c30_to_g(r) = rewrite(r, c30_gt(a_, b_, z_, s_) :> if s === 1 then inactive(meijer_g)(c30_ex(a), c30_ex(b), z) else inactive(meijer_g)(c30_ex(a), c30_ex(b), z, s))
fn c30_to_h(r, x) = rewrite(r, c30_gt(a_, b_, z_, s_) :> c30_h1(a, b, z, s, x))
fn c30_h1(a, b, z, s, x) = {
  let w = abs(s)
  let pr = l => map(v => [v, w], l)
  w*c30_hx(map(pr, c30_ex(a)), map(pr, c30_ex(b)), if s > 0 then z else 1/z, x)
}

# meijer_g_reduce(e, x): e as a combination of Meijer G functions of x (inactive), for x > 0
fn meijer_g_reduce(e, x; assumptions = true) when is_symbol(x) and c30_reduce(e, x, false) !== none = c30_to_g(c30_reduce(e, x, false))
# fox_h_reduce(e, x): e as a combination of Fox H functions of x (inactive), for x > 0
fn fox_h_reduce(e, x; assumptions = true) when is_symbol(x) and c30_reduce(e, x, true) !== none = c30_distribute(c30_to_h(c30_reduce(e, x, true), x))

# a numeric factor of a sum distributed over its terms (also inside a conditional expression)
fn c30_distribute(r) =
  if not is_atom(r) and head(r) === conditional_expression then conditional_expression(c30_distribute(r[1]), r[2])
  else if not is_atom(r) and head(r) === mul and length(r) == 2 and is_number(r[1]) and not is_atom(r[2]) and head(r[2]) === add then total(map(t => r[1]*t, apply(list, r[2])))
  else r

# --- fox_h ---
# fox_h([[an..], [ap..]], [[bm..], [bq..]], z) with pairs [a_j, A_j], [b_j, B_j]: the Mellin-Barnes integral of
# prod G(b_j + B_j s) prod G(1 - a_j - A_j s)/(prod G(1 - b_j - B_j s) prod G(a_j + A_j s)) z^(-s) (Kilbas and
# Saigo, H-Transforms, 2004). Methods: crates/mc-further-functions/NOTES.md, section fox_h.

fn c30_pairs_ok(l) = is_list(l) and length(l) == 2 and is_list(l[1]) and is_list(l[2]) and every(join(l[1], l[2]), p => is_list(p) and length(p) == 2)

# a pair of the first a-group equal to one of the second b-group (or of the second a-group and the first b-group):
# their gamma factors cancel
fn c30_fox_cancel_at(a, b) = {
  let i = select(range(length(a[1])), j => member_of(b[2], a[1][j]))
  let k = select(range(length(a[2])), j => member_of(b[1], a[2][j]))
  if length(i) > 0 then [1, a[1][i[1]]] else if length(k) > 0 then [2, a[2][k[1]]] else none
}
fn c30_drop1(l, p) = {
  let i = select(range(length(l)), j => l[j] === p)
  delete(l, i[1])
}
fn c30_fox_cancel(a, b, z) = {
  let c = c30_fox_cancel_at(a, b)
  if c[1] == 1 then fox_h([c30_drop1(a[1], c[2]), a[2]], [b[1], c30_drop1(b[2], c[2])], z)
  else fox_h([a[1], c30_drop1(a[2], c[2])], [c30_drop1(b[1], c[2]), b[2]], z)
}

# the common scale of all pairs, or none
fn c30_fox_scale(a, b) = {
  let ps = join(a[1], a[2], b[1], b[2])
  if length(ps) == 0 then none else if every(ps, p => p[2] === ps[1][2]) then ps[1][2] else none
}

fn c30_fox_numeric(a, b) = every(join(a[1], a[2], b[1], b[2]), p => is_numeric(p[1]) and is_numeric(p[2]) and is_real(n(p[2])) and n(p[2]) > 0)

# z = 0: the leading pole s = -b_j/B_j of the first b-group (the smallest real part) decides
fn c30_fox_at0(a, b) = {
  let rs = map(p => re(n(p[1]/p[2])), b[1])
  if length(rs) == 0 then none
  else if min(rs) > 0 then 0
  else if min(rs) < 0 then complex_infinity
  else {
    let i = select(range(length(b[1])), j => p30_is0(b[1][j][1]))
    if length(i) != 1 then none
    else {
      product(map(j => if j == i[1] then 1/b[1][j][2] else gamma(b[1][j][1]), range(length(b[1]))))*product(map(p => gamma(1 - p[1]), a[1]))/(product(map(p => gamma(1 - p[1]), b[2]))*product(map(p => gamma(p[1]), a[2])))
    }
  }
}
fn p30_is0(v) = v === 0 or (is_number(v) and v == 0)

# whether the inexact z or parameters ask for a ball value (not machine numbers)
fn c30_fox_inexact(a, b, z) = some(join([z], flatten([a, b])), v => is_inexact(v))
fn c30_fox_machine_q(a, b, z) = every(join([z], flatten([a, b])), v => not is_inexact(v) or is_machine_number(v) or is_machine_number(re(v)))

# gamma(w)^-1 that is 0 at the poles
fn c30_rgamma(w) = if abs(n(w) - round(re(n(w)))) < 1.0e-14 and re(n(w)) < 0.5 then 0 else 1/gamma(w)

# the residue series at working precision wp (digits) of the poles of the b's (use_b) or of the first a-group
fn c30_fox_series(a, b, z, wp, use_b) = {
  let set = if use_b then b[1] else a[1]
  let lz = log(z)
  var sum = 0
  var big = 0.0
  for j in range(length(set)) {
    let c = set[j][1]
    let sc = set[j][2]
    var k = 0
    var small = 0
    var fact = 1
    while small < 4 and k < 3000 {
      let s = if use_b then -(c + k)/sc else (1 - c + k)/sc
      var t = (-1)^k/(fact*sc)
      for i in range(length(b[1])) { t = t*(if use_b and i == j then 1 else gamma(b[1][i][1] + b[1][i][2]*s)) }
      for i in range(length(a[1])) { t = t*(if use_b or i != j then gamma(1 - a[1][i][1] - a[1][i][2]*s) else 1) }
      for p in b[2] { t = t*c30_rgamma(1 - p[1] - p[2]*s) }
      for p in a[2] { t = t*c30_rgamma(p[1] + p[2]*s) }
      t = t*exp(-s*lz)
      sum = sum + t
      big = max(big, abs(n(t)))
      small = if k > 5 and (t == 0 or abs(t) < 10^(-wp - 3)*abs(sum)) then small + 1 else 0
      k = k + 1
      fact = fact*k
    }
  }
  [sum, big]
}

# whether two poles of the summed factors coincide (then the parameters are perturbed)
fn c30_fox_collide(set) = some(range(length(set)), i => some(range(length(set)), j => i < j and some(range(0, 60), k => {
  let t = ((re(n(set[i][1])) + k)/n(set[i][2]) - re(n(set[j][1]))/n(set[j][2]))*n(set[j][2])
  abs(im_part(n(set[i][1]))/n(set[i][2]) - im_part(n(set[j][1]))/n(set[j][2])) < 1.0e-9 and t > -1.0e-9 and abs(t - round(t)) < 1.0e-7
})))

# v (a number or nested lists) carried at wp digits: inexact numbers padded, exact ones approximated
fn c30_sp(v, wp) = if is_list(v) then map(w => c30_sp(w, wp), v) else if is_inexact(v) then set_precision(v, wp) else n(v, wp)

# the ball value at d digits
fn c30_fox_ball(a, b, z, d) = {
  let ps = join(a[1], a[2], b[1], b[2])
  let mu = n(total(map(p => p[2], join(b[1], b[2]))) - total(map(p => p[2], join(a[1], a[2]))))
  let dl = n(product(map(p => p[2]^(-p[2]), join(a[1], a[2])))*product(map(p => p[2]^p[2], join(b[1], b[2]))))
  let use_b = if abs(mu) < 1.0e-14 then abs(n(z)) < dl else mu > 0
  let set = if use_b then b[1] else a[1]
  if c30_fox_collide(set) then {
    let wp = 2*d + 30
    let ep = 10^(-floor((d + 12)/2))
    let sh = (l, sg) => map(i => [l[i][1] + sg*ep*(1 + i*37/100), l[i][2]], range(length(l)))
    let za = c30_sp(z, wp)
    let f = sg => c30_fox_series(c30_sp([sh(c30_sp(a, wp)[1], -sg), a[2]], wp), c30_sp([sh(c30_sp(b, wp)[1], sg), b[2]], wp), za, wp, use_b)[1]
    n((f(1) + f(-1))/2, d)
  }
  else {
    # d + 20 digits, again with the cancellation added when the largest term exceeds the sum by more than 10^12
    let first = c30_fox_series(c30_sp(a, d + 20), c30_sp(b, d + 20), c30_sp(z, d + 20), d + 20, use_b)
    let loss = if first[1] == 0 or n(first[1]) == 0 then 0 else log(first[2]/abs(n(first[1])))/log(10)
    if loss < 12 then n(first[1], d)
    else {
      let wp = d + 20 + ceiling(loss)
      n(c30_fox_series(c30_sp(a, wp), c30_sp(b, wp), c30_sp(z, wp), wp, use_b)[1], d)
    }
  }
}

# machine numbers: the native (simple poles), else the ball series at 17 digits
fn c30_fox_mach(a, b, z) = {
  let v = c30_fox_machine(n(a[1]), n(a[2]), n(b[1]), n(b[2]), n(z))
  if is_number(v) then v else n(c30_fox_ball(a, b, z, 17))
}
rule fox_h(a_, b_, z_) := map(w => fox_h(a, b, w), z) when is_list(z) and c30_pairs_ok(a) and c30_pairs_ok(b)
# the order of the pairs within a group does not matter: sorted
rule fox_h(a_, b_, z_) := fox_h(map(sort, a), map(sort, b), z) when c30_pairs_ok(a) and c30_pairs_ok(b) and (map(sort, a) !== a or map(sort, b) !== b)
# H^(1,0)_(0,1)(z | (b, B)) = z^(b/B) e^(-z^(1/B))/B
rule fox_h(a_, b_, z_) := z^(b[1][1][1]/b[1][1][2])*exp(-z^(1/b[1][1][2]))/b[1][1][2] when not is_list(z) and c30_pairs_ok(a) and c30_pairs_ok(b) and length(a[1]) + length(a[2]) + length(b[2]) == 0 and length(b[1]) == 1
rule fox_h(a_, b_, z_) := c30_fox_cancel(a, b, z) when not is_list(z) and c30_pairs_ok(a) and c30_pairs_ok(b) and c30_fox_cancel_at(a, b) !== none
rule fox_h(a_, b_, z_) := {
  let c = c30_fox_scale(a, b)
  let g = (l => map(q => map(p => p[1], q), l))
  if c === 1 then meijer_g(g(a), g(b), z) else meijer_g(g(a), g(b), z, c)/c
} when not is_list(z) and c30_pairs_ok(a) and c30_pairs_ok(b) and c30_fox_cancel_at(a, b) === none and c30_fox_scale(a, b) !== none
rule fox_h(a_, b_, z_) := c30_fox_at0(a, b) when (z === 0) and c30_pairs_ok(a) and c30_pairs_ok(b) and c30_fox_cancel_at(a, b) === none and c30_fox_scale(a, b) === none and c30_fox_numeric(a, b) and c30_fox_at0(a, b) !== none
rule fox_h(a_, b_, z_) := c30_fox_mach(a, b, z) when is_numeric(z) and not is_list(z) and z !== 0 and c30_pairs_ok(a) and c30_pairs_ok(b) and c30_fox_cancel_at(a, b) === none and c30_fox_scale(a, b) === none and c30_fox_numeric(a, b) and c30_fox_inexact(a, b, z) and c30_fox_machine_q(a, b, z)
rule fox_h(a_, b_, z_) := c30_fox_ball(a, b, z, floor(min(map(v => precision(v), select(join([z], flatten([a, b])), v => is_inexact(v) and v != 0))))) when is_numeric(z) and not is_list(z) and z !== 0 and c30_pairs_ok(a) and c30_pairs_ok(b) and c30_fox_cancel_at(a, b) === none and c30_fox_scale(a, b) === none and c30_fox_numeric(a, b) and c30_fox_inexact(a, b, z) and not c30_fox_machine_q(a, b, z)

# --- spheroidal_s1 ---
# The radial function of the first kind (Flammer, Spheroidal wave functions, 1957, 4.1.17; DLMF 30.11.3): with the
# coefficients a_k of spheroidal_ps (the continued fraction of the three-term recurrence DLMF 30.8.2-30.8.4 at the
# eigenvalue lambda = spheroidal_eigenvalue(n, m, gamma)) and w_k = (n + m + 2k)!/(n - m + 2k)!,
# S1(z) = (z^2 - 1)^(m/2) z^(-m) sum a_k w_k j_(n+2k)(gamma z) / sum (-1)^k a_k w_k.

# the recurrence coefficients A_k, B_k, C_k (A_k a_(k-1) + (B_k - lambda) a_k + C_k a_(k+1) = 0)
fn c30_sph_abc(nn, m, g2, k) = {
  let l = nn + 2*k
  [g2*(-(nn - m + 2*k - 1)*(nn - m + 2*k))/((2*nn + 4*k - 3)*(2*nn + 4*k - 1)),
   l*(l + 1) - g2*2*(l*(l + 1) + m^2 - 1)/((2*nn + 4*k - 1)*(2*nn + 4*k + 3)),
   g2*(-(nn + m + 2*k + 1)*(nn + m + 2*k + 2))/((2*nn + 4*k + 3)*(2*nn + 4*k + 5))]
}

# the ratios a_k/a_0 for k = k0, ..., kk (k0 = -floor((n - m)/2)): upwards by the continued fraction
# R_k = a_k/a_(k-1) = -A_k/(B_k - lambda + C_k R_(k+1)), downwards L_k = a_k/a_(k+1) = -C_k/(B_k - lambda + A_k L_(k-1))
fn c30_sph_coefs(nn, m, g2, lam, kk) = {
  let k0 = -floor((nn - m)/2)
  var r = table(0, [j, 0, kk + 1])
  var k = kk
  while k >= 1 {
    let c = c30_sph_abc(nn, m, g2, k)
    r[k + 1] = -c[1]/(c[2] - lam + c[3]*(if k == kk then 0 else r[k + 2]))
    k = k - 1
  }
  var up = [1]
  var cur = 1
  for j in range(1, kk) { cur = cur*r[j + 1]; up = append(up, cur) }
  var prev = 0
  var ls = []
  for j in range(k0, -1) {
    let c = c30_sph_abc(nn, m, g2, j)
    prev = -c[3]/(c[2] - lam + c[1]*prev)
    ls = append(ls, prev)
  }
  var cur2 = 1
  var dn = []
  for i in range(length(ls)) { cur2 = cur2*ls[length(ls) + 1 - i]; dn = prepend(dn, cur2) }
  join(dn, up)
}

# the spherical Bessel functions j_0(x), ..., j_L(x): for |x| > L by the forward recurrence
# j_(l+1) = (2l + 1)/x j_l - j_(l-1) (stable there), else by Miller's backward recurrence from N = L + 30,
# normalized by j_0 = sin(x)/x or j_1 = sin(x)/x^2 - cos(x)/x (the larger)
fn c30_sph_j_all(lmax, x) = {
  let j0 = sin(x)/x
  let j1 = sin(x)/x^2 - cos(x)/x
  if abs(n(x)) > lmax + 1 then {
    var js = [j0, j1]
    for l in range(1, lmax - 1) { js = append(js, (2*l + 1)/x*js[l + 1] - js[l]) }
    take(js, lmax + 1)
  }
  else {
    let top = lmax + 32
    var f = table(0*x, [j, 0, top + 1])
    f[top] = 1 + 0*x
    var l = top - 1
    while l >= 1 {
      f[l] = (2*l + 1)/x*f[l + 1] - f[l + 2]
      f = if abs(n(f[l])) > 1.0e200 then map(v => v*10^(-200), f) else f
      l = l - 1
    }
    let sc = if abs(n(j0)) >= abs(n(j1)) then j0/f[1] else j1/f[2]
    map(v => v*sc, take(f, lmax + 1))
  }
}

# a real value as a complex number with an inexact zero imaginary part (the radial functions are complex valued)
fn c30_cx(v) = if is_real(v) and is_inexact(v) then complex(v, if is_machine_number(v) then 0.0 else set_precision(0, floor(precision(v)))) else v

fn c30_s1(nn, m, g, z) = {
  let p = if is_machine_number(n(abs(z))) and is_machine_number(n(abs(g))) then 16 else min(precision(z), precision(g))
  let g2 = g^2
  let lam = spheroidal_eigenvalue(nn, m, g)
  let kk = 20 + ceiling(abs(n(g))) + floor(p/2)
  let a = c30_sph_coefs(nn, m, g2, lam, kk)
  let k0 = -floor((nn - m)/2)
  let js = c30_sph_j_all(nn + 2*kk, g*z)
  let w = k => factorial(nn + m + 2*k)/factorial(nn - m + 2*k)
  let num = total(map(i => a[i]*w(k0 + i - 1)*js[nn + 2*(k0 + i - 1) + 1], range(length(a))))
  let den = total(map(i => (-1)^(k0 + i - 1)*a[i]*w(k0 + i - 1), range(length(a))))
  c30_cx((z^2 - 1)^(m/2)*z^(-m)*num/den)
}

# an integer given as any kind of number (n() turns the orders into balls)
fn c30_intq(v) = is_integer(v) or (is_number(v) and is_real(v) and is_inexact(v) and abs(v - round(v)) < 10^(-10))

# spheroidal_s1(n, m, gamma, z): the radial spheroidal function of the first kind, for integers n >= m >= 0 and
# inexact gamma, z; at z = 0 only the j_0 term is left (m = 0)
rule spheroidal_s1(nn_, m_, g_, z_) := map(w => spheroidal_s1(nn, m, g, w), z) when is_list(z)
rule spheroidal_s1(nn_, m_, g_, z_) := c30_s1(round(nn), round(m), g, z) when c30_intq(nn) and c30_intq(m) and nn >= m and m >= 0 and is_numeric(g) and is_numeric(z) and not is_list(z) and (is_inexact(g) or is_inexact(z)) and z != 0 and g != 0
rule spheroidal_s1(nn_, m_, g_, z_) := c30_cx(n(c30_s1_at0(round(nn), g))) when c30_intq(nn) and c30_intq(m) and m == 0 and nn >= 0 and is_numeric(g) and (is_inexact(g) or is_inexact(z)) and z == 0 and g != 0
fn c30_s1_at0(nn, g) =
  if mod(nn, 2) == 1 then 0.0*g
  else {
    let kk = 20 + ceiling(abs(n(g)))
    let a = c30_sph_coefs(nn, 0, g^2, spheroidal_eigenvalue(nn, 0, g), kk)
    a[1]/total(map(i => (-1)^(i - 1 - nn/2)*a[i], range(length(a))))
  }

# --- trig_factor ---
# trig_factor(e): e with tangents, cotangents, secants and cosecants (circular and hyperbolic) written through sines
# and cosines, expanded to sines and cosines of the plain arguments (trig_expand), put over a common denominator, and
# numerator and denominator factored with the sines and cosines as variables; a factor a cos(u) + a sin(u) becomes
# sqrt(2) a sin(u + pi/4). Lists, equations, inequalities and logical combinations map over their parts.
fn c30_tf_heads() = [list, equal, unequal, less, less_equal, greater, greater_equal, `and`, `or`, `not`, inequality]
fn c30_tf_rules() = [tan(u_) :> sin(u)/cos(u), cot(u_) :> cos(u)/sin(u), sec(u_) :> 1/cos(u), csc(u_) :> 1/sin(u),
  tanh(u_) :> sinh(u)/cosh(u), coth(u_) :> cosh(u)/sinh(u), sech(u_) :> 1/cosh(u), csch(u_) :> 1/sinh(u)]
fn c30_tf_lin(f) =
  if not is_atom(f) and head(f) === add and length(f) == 2 and not is_atom(f[1]) and not is_atom(f[2]) then {
    let p = map(t => if not is_atom(t) and head(t) === mul and length(t) == 2 and is_number(t[1]) then [t[1], t[2]] else [1, t], apply(list, f))
    let c = select(p, q => not is_atom(q[2]) and head(q[2]) === cos)
    let s = select(p, q => not is_atom(q[2]) and head(q[2]) === sin)
    if length(c) == 1 and length(s) == 1 and c[1][2][1] === s[1][2][1] and abs(c[1][1]) === abs(s[1][1]) then
      sqrt(2)*s[1][1]*sin(s[1][2][1] + c[1][1]/s[1][1]*pi/4)
    else f
  }
  else f
fn c30_tf_poly(p) = {
  let f = factor(p)
  if not is_atom(f) and head(f) === mul then product(map(c30_tf_pow, apply(list, f))) else c30_tf_pow(f)
}
fn c30_tf_pow(f) = if not is_atom(f) and head(f) === pow then c30_tf_lin(f[1])^f[2] else c30_tf_lin(f)
fn c30_tf1(r) = {
  let t = together(r)
  c30_tf_poly(numerator(t))/c30_tf_poly(denominator(t))
}
# with and without trig_expand: the smaller result (by leaf count)
fn c30_tf(e) = {
  let r = rewrite(e, c30_tf_rules())
  let a = c30_tf1(r)
  let b = c30_tf1(trig_expand(r))
  if leaf_count(b) < leaf_count(a) then b else a
}
fn trig_factor(e) =
  if is_atom(e) then e
  else if member_of(c30_tf_heads(), head(e)) then apply(head(e), map(trig_factor, apply(list, e)))
  else c30_tf(e)

# --- moment_of_inertia ---
# moment_of_inertia(reg, p): the inertia tensor integrate(|r|^2 I - r r^T) over the region of unit density, r = x - p
# (p defaults to the origin); in the plane [[integrate(y^2), -integrate(x y)], [-integrate(x y), integrate(x^2)]];
# moment_of_inertia(reg, p, v): the moment v.I.v/(v.v) about the axis through p along v
fn c30_moi(reg, p) = {
  let d = region_embedding_dimension(reg)
  # an implicit region is integrated in its own variables
  let xs = if not is_atom(reg) and head(reg) === implicit_region then map(v => if is_list(v) then v[1] else v, reg[2]) else take([c30_x1, c30_x2, c30_x3], d)
  let r = map(i => xs[i] - p[i], range(d))
  let ii = (f => simplify(c30_reg_int(f, xs, reg)))
  if d == 2 then {
    let xy = ii(r[1]*r[2])
    [[ii(r[2]^2), -xy], [-xy, ii(r[1]^2)]]
  }
  else if d == 3 then {
    let m = map(i => map(j => if i <= j then ii(boole(i == j)*total(map(t => r[t]^2, range(3))) - r[i]*r[j]) else 0, range(3)), range(3))
    map(i => map(j => if i <= j then m[i][j] else m[j][i], range(3)), range(3))
  }
  else failure(argument, moment_of_inertia, 1)
}
# implicit regions of the form x^2 + y^2 <= c (with a range for z) as the disk (cylinder) they are; else as given
fn c30_implicit_std(reg) =
  if is_atom(reg) or head(reg) !== implicit_region or length(reg) != 2 or not is_list(reg[2]) then reg
  else {
    let c = reg[1]
    let vs = reg[2]
    let plain = select(vs, v => not is_list(v))
    let ranged = select(vs, is_list)
    if length(plain) == 2 and not is_atom(c) and head(c) === less_equal and expand(c[1] - plain[1]^2 - plain[2]^2) === 0 and free_of(c[2], plain) then {
      if length(ranged) == 0 and vs[1] === plain[1] then disk([0, 0], sqrt(c[2]))
      else if length(ranged) == 1 and length(ranged[1]) == 3 and vs[3] === ranged[1] then cylinder([[0, 0, ranged[1][2]], [0, 0, ranged[1][3]]], sqrt(c[2]))
      else reg
    }
    else reg
  }
# integrate(f, element(xs, reg)); a cylinder along the z axis in cylindrical coordinates
fn c30_reg_int(f, xs, reg) =
  if not is_atom(reg) and head(reg) === cylinder and length(reg) == 2 and length(xs) == 3 and reg[1][1][1] === 0 and reg[1][1][2] === 0 and reg[1][2][1] === 0 and reg[1][2][2] === 0 then
    integrate(c30_rho*rewrite(f, [xs[1] -> c30_rho*cos(c30_phi), xs[2] -> c30_rho*sin(c30_phi), xs[3] -> c30_zz]), [c30_zz, reg[1][1][3], reg[1][2][3]], [c30_rho, 0, reg[2]], [c30_phi, 0, 2*pi])
  else integrate(f, element(xs, reg))

fn moment_of_inertia(reg) = c30_moi(c30_implicit_std(reg), table(0, [j, region_embedding_dimension(reg)]))
fn moment_of_inertia(reg, p) when is_list(p) = c30_moi(c30_implicit_std(reg), p)
fn moment_of_inertia(reg, p, v) when is_list(p) and is_list(v) and length(p) == length(v) = simplify(dot(v, dot(c30_moi(c30_implicit_std(reg), p), v))/dot(v, v))

# --- number_field_discriminant ---
# The discriminant of the ring of integers of Q(theta): with g the monic integer polynomial of t = a theta (a the
# leading coefficient of the minimal polynomial), the Z-module M = Z[t] (basis 1, t, ..., t^(d-1)) is enlarged one
# prime p at a time by an algebraic integer w in M/p not in M while p^2 divides disc(M); disc(M) = disc(g)/[M : Z[t]]^2
# and the search ends at the maximal order (if M were smaller, O_K/M would have an element of prime order p, with
# p^2 | disc(M)). Elements are polynomials in t reduced modulo g; w is integral when its characteristic polynomial
# resultant(g(x), y - w(x), x) has integer coefficients (Cohen, A course in computational algebraic number theory,
# 6.1; the search over M/p replaces the Round 2 algorithm, fine for small degrees).
fn c30_nf_integral(g, w) = every(coefficient_list(resultant(g, c30_nf_y - w, c30_nf_t), c30_nf_y), is_integer)
fn c30_nf_disc(theta) = {
  let f = minimal_polynomial(theta, c30_nf_t)
  let cs = coefficient_list(f, c30_nf_t)
  let dg = length(cs) - 1
  let a = cs[dg + 1]
  # g(t) = a^(d-1) f(t/a): monic with integer coefficients
  let g = expand(a^(dg - 1)*rewrite(f, c30_nf_t -> c30_nf_t/a))
  var disc = discriminant(g, c30_nf_t)
  var basis = map(i => c30_nf_t^(i - 1), range(dg))
  var more = true
  while more {
    more = false
    let ps = map(q => q[1], select(factor_integer(abs(disc)), q => q[2] >= 2))
    for p in ps {
      if not more then {
        let cands = tuples(range(0, p - 1), dg)
        let hit = select_first(cands, c => some(c, v => v != 0) and c30_nf_integral(g, expand(total(map(i => c[i]*basis[i], range(dg)))/p)), none)
        if hit !== none then {
          let j = select_first(range(dg), i => mod(hit[i], p) != 0)
          let u = power_mod(hit[j], -1, p)
          let w2 = expand(total(map(i => mod(u*hit[i], p)*basis[i], range(dg)))/p)
          basis = map(i => if i == j then polynomial_remainder(w2, g, c30_nf_t) else basis[i], range(dg))
          disc = disc/p^2
          more = true
        }
      }
    }
  }
  disc
}

# number_field_discriminant(a): the discriminant of the number field Q(a) of the algebraic number a
fn number_field_discriminant(a) when is_list(a) = map(number_field_discriminant, a)
fn number_field_discriminant(a) when not is_list(a) and is_numeric(a) and is_exact(n(a, 20)) === false and not is_number(minimal_polynomial(a, c30_nf_t)) = c30_nf_disc(a)
fn number_field_discriminant(a) when is_rational(a) = 1
# the field of an algebraic_number is that of its generator
fn number_field_discriminant(a) when not is_atom(a) and head(a) === algebraic_number = number_field_discriminant(a[1])

# --- algebraic_number ---
# algebraic_number(theta, [c0, c1, ...]) = c0 + c1 theta + ... in the number field Q(theta). The normal form has
# theta an algebraic integer, root(g, k) of its monic integer minimal polynomial g (a radical when g is quadratic),
# and d = deg g coefficients (the polynomial reduced modulo g). theta with minimal polynomial f of leading
# coefficient a becomes a theta (the coefficients c_i become c_i/a^i); an algebraic number as theta becomes the root of
# its minimal polynomial; a rational theta, or a reduced polynomial of degree 0, gives the value.

# the pure function t => e(t) of an expression in the symbol t
fn c30_fn_of(e, t) = rewrite(`fn`([c30_q], c30_body), c30_body -> rewrite(e, t -> c30_q))

# the index k with root(g, k) = v (numerically)
fn c30_root_index(g, t, v) = {
  let dg = exponent(g, t)
  let f = c30_fn_of(g, t)
  let ds = map(k => n(abs(n(root(f, k), 30) - n(v, 30))), range(dg))
  select_first(range(dg), k => ds[k] <= min(ds), 1)
}

# the value of the coefficients at theta
fn c30_an_value(t, c) = total(map(i => c[i]*t^(i - 1), range(length(c))))

# [theta', coefficients] in normal form, or none when (t, c) is already normal
fn c30_an_normal(t, c) = {
  let f = minimal_polynomial(t, c30_an_t)
  let cs = coefficient_list(f, c30_an_t)
  let dg = length(cs) - 1
  let a = cs[dg + 1]
  let g = expand(a^(dg - 1)*rewrite(f, c30_an_t -> c30_an_t/a))
  let c2 = map(i => c[i]/a^(i - 1), range(length(c)))
  let red = polynomial_remainder(c30_an_value(c30_an_t, c2), g, c30_an_t)
  let cl = coefficient_list(red, c30_an_t)
  let cn = map(i => if i <= length(cl) then cl[i] else 0, range(dg))
  let t2 = if a === 1 and (head(t) === root or dg <= 2) then t else root(c30_fn_of(g, c30_an_t), c30_root_index(g, c30_an_t, a*t))
  if t2 === t and cn === c then none else [t2, cn]
}

rule algebraic_number(t_, c_) := c30_an_value(t, c) when is_list(c) and is_number(t) and (is_inexact(t) or is_rational(t))
rule algebraic_number(t_, c_) := c[1] when is_list(c) and length(c) >= 1 and every(c, is_rational) and every(drop(c, 1), v => v === 0)
rule algebraic_number(t_, c_) := c30_an_value(n(t), c) when is_list(c) and is_number(t) and some(c, is_inexact)
rule algebraic_number(t_, c_) := algebraic_number(root(c30_fn_of(minimal_polynomial(c30_an_value(t[1], t[2]), c30_an_t), c30_an_t), c30_root_index(minimal_polynomial(c30_an_value(t[1], t[2]), c30_an_t), c30_an_t, c30_an_value(t[1], t[2]))), c) when is_list(c) and not is_atom(t) and head(t) === algebraic_number
rule algebraic_number(t_, c_) := c30_an_reduce(c30_an_normal(t, c)) when is_list(c) and is_numeric(t) and not is_rational(t) and not (not is_atom(t) and head(t) === algebraic_number) and every(c, is_rational) and c30_an_normal(t, c) !== none
fn c30_an_reduce(r) = if every(drop(r[2], 1), v => v === 0) then r[2][1] else algebraic_number(r[1], r[2])

# --- polynomial_sum_of_squares_list ---
# polynomial_sum_of_squares_list(p, vars): [q1, q2, ...] with p = q1^2 + q2^2 + ... (expand(p - q.q) = 0). For p of
# total degree <= 2 the Gram matrix Q of p in the monomials [x1, ..., xk, 1] is decomposed by symmetric Gaussian
# elimination (L D L^T) from the last variable on: q = (sum_j Q[k][j] m_j)/sqrt(Q[k][k]) for each positive pivot.
# A negative pivot, or a zero pivot with a nonzero row, means p is not a sum of squares: unevaluated (as for higher
# degrees, which need semidefinite programming).
fn c30_sos_quadratic(p, vs) = {
  let ms = append(vs, 1)
  let k = length(ms)
  let cf = (i, j) => if i == k and j == k then rewrite(p, map(v => v -> 0, vs))
    else if i == k then coefficient(rewrite(p, map(v => v -> 0, delete(vs, j))), vs[j], 1)/2
    else if j == k then coefficient(rewrite(p, map(v => v -> 0, delete(vs, i))), vs[i], 1)/2
    else if i == j then coefficient(p, vs[i], 2)
    else coefficient(coefficient(p, vs[i], 1), vs[j], 1)/2
  var q = map(i => map(j => expand(cf(i, j)), range(k)), range(k))
  var out = []
  var ok = true
  var piv = k
  # the constant monomial last: pivots run over the variables from the last, then the constant
  let order = join(reverse(range(k - 1)), [k])
  for i in order {
    if ok then {
      let d = q[i][i]
      if is_numeric(d) and n(d) > 0 then {
        out = append(out, expand(total(map(j => q[i][j]*ms[j], range(k)))/sqrt(d)))
        q = map(r => map(c => expand(q[r][c] - q[r][i]*q[i][c]/d), range(k)), range(k))
      }
      else if is_numeric(d) and n(d) == 0 then { ok = every(range(k), j => q[i][j] === 0 or (is_numeric(q[i][j]) and n(q[i][j]) == 0)) }
      else { ok = false }
    }
  }
  if ok then out else none
}
fn c30_total_degree(p, vs) = max(map(t => total(map(v => exponent(t, v), vs)), if not is_atom(expand(p)) and head(expand(p)) === add then apply(list, expand(p)) else [expand(p)]))
fn polynomial_sum_of_squares_list(p, vs) when is_list(vs) and polynomial_expression_q(p, vs) and c30_total_degree(p, vs) <= 2 and c30_sos_quadratic(p, vs) !== none = c30_sos_quadratic(p, vs)

# --- inverse_radon_transform ---
# inverse_radon_transform(g, [p, phi], [x, y]) for g = e^(-p^2) Q(p, cos phi, sin phi), Q a polynomial: the preimage
# is e^(-x^2 - y^2) P(x, y) with deg P = deg Q, since in the rotated frame x = p c - t s, y = p s + t c the Radon
# transform of e^(-r^2) x^i y^j is e^(-p^2) sum binomial terms times the Gaussian moments
# integrate(t^k e^(-t^2), t) = gamma((k + 1)/2) (k even). The coefficients of P solve the linear equations of the
# identity at rational points (c, s) = ((1 - u^2)/(1 + u^2), 2u/(1 + u^2)) and integer p (Deans, The Radon transform
# and some of its applications, 1983, 7.2; the transform is injective, so a solution is the inverse).
fn c30_radon_mono(i, j, p, c, s) = total(flatten(map(a => map(b => {
  let k = a + b
  if mod(k, 2) == 1 then 0 else binomial(i, a)*binomial(j, b)*(p*c)^(i - a)*(-s)^a*(p*s)^(j - b)*c^b*gamma((k + 1)/2)
}, range(0, j)), range(0, i))))
fn c30_iradon(g, p, f, x, y) =
  if not is_atom(g) and head(g) === radon_transform and length(g) == 3 and g[3] === [p, f] and is_list(g[2]) and length(g[2]) == 2 then rewrite(g[1], [g[2][1] -> x, g[2][2] -> y])
  else if not is_atom(g) and head(g) === dirac_delta and length(g) == 1 then c30_iradon_delta(g[1], p, f, x, y)
  else if not is_atom(g) and head(g) === piecewise and length(g[1]) == 1 and g[2] === 0 then c30_iradon_disk(g[1][1][1], p, x, y)
  else {
    let r = c30_iradon_gauss(g, p, f, x, y)
    if r === none then c30_iradon_lin(g, p, f, x, y) else r
  }
# whether e is a call h(p, phi) of a function without rules (it stays a call on new symbols)
fn c30_undef_call(e, vs) = not is_atom(e) and is_symbol(head(e)) and apply(list, e) === vs and c30_held_probe(head(e))
fn c30_held_probe(h) = {
  let t = h(c30_q1, c30_q2)
  not is_atom(t) and head(t) === h and t[1] === c30_q1 and t[2] === c30_q2
}
# linearity: a sum term by term and constant factors outside; calls of undefined functions stay as transforms
fn c30_iradon_lin(g, p, f, x, y) = {
  let ts = if not is_atom(g) and head(g) === add then apply(list, g) else [g]
  if length(ts) == 1 and c30_undef_call(g, [p, f]) then none else c30_iradon_terms(ts, p, f, x, y)
}
fn c30_iradon_terms(ts, p, f, x, y) = {
  let parts = map(t => {
    let fs = if not is_atom(t) and head(t) === mul then apply(list, t) else [t]
    let c = product(select(fs, u => free_of(u, p) and free_of(u, f)))
    let rest = product(select(fs, u => not (free_of(u, p) and free_of(u, f))))
    if c30_undef_call(rest, [p, f]) then c*inverse_radon_transform(rest, [p, f], [x, y])
    else if length(ts) == 1 and c === 1 or rest === 1 then none
    else {
      let r = c30_iradon(rest, p, f, x, y)
      if r === none then none else c*r
    }
  }, ts)
  if some(parts, r => r === none) then none else total(parts)
}
# k 2 sqrt(r^2 - p^2) on |p| < r (the chord lengths of the disk of radius r): k times the disk's indicator
fn c30_iradon_disk(e, p, x, y) = {
  let r2 = simplify(rewrite(e, p -> 0)^2/4)
  let k = simplify(e/(2*sqrt(r2 - p^2)))
  if free_of(r2, p) and free_of(k, p) and is_numeric(r2) then k*unit_step(expand(2*pi*(sqrt(r2) - sqrt(x^2 + y^2)))) else none
}
# the line p = a cos(phi) + b sin(phi) of every phi passes through the point [a, b]: a point mass there
fn c30_iradon_delta(w, p, f, x, y) = {
  let a = -coefficient(expand(w), cos(f))
  let b = -coefficient(expand(w), sin(f))
  if free_of([a, b], f) and free_of([a, b], p) and expand(w - p + a*cos(f) + b*sin(f)) === 0 then dirac_delta(x - a, y - b) else none
}
fn c30_iradon_gauss(g, p, f, x, y) = {
  let h = expand(trig_expand(simplify(g*e^(p^2))))
  let hc = rewrite(h, [cos(f) -> c30_rc, sin(f) -> c30_rs])
  if not free_of(hc, f) or not polynomial_expression_q(hc, [p, c30_rc, c30_rs]) then none
  else {
    let dg = max(c30_total_degree(hc, [p, c30_rc, c30_rs]), 0)
    let mons = flatten(map(i => map(j => [i, j], range(0, dg - i)), range(0, dg)), 1)
    let us = map(m => c30_ru(m[1], m[2]), mons)
    let pts = flatten(map(pp => map(u => [pp, (1 - u^2)/(1 + u^2), 2*u/(1 + u^2)], take([0, 1/2, 1, 2, 1/3, 3, -1/2, -2, 1/5, 5, -3, 7/2], min(12, 2*dg + 3))), range(0, dg + 1)), 1)
    let eqs = map(q => total(map(i => us[i]*c30_radon_mono(mons[i][1], mons[i][2], q[1], q[2], q[3]), range(length(mons)))) == rewrite(hc, [p -> q[1], c30_rc -> q[2], c30_rs -> q[3]]), pts)
    let sol = solve(eqs, us)
    if length(sol) != 1 then none
    else e^(-x^2 - y^2)*factor(rewrite(total(map(i => us[i]*x^mons[i][1]*y^mons[i][2], range(length(mons)))), sol[1]))
  }
}
fn inverse_radon_transform(g, pf, xy) when is_list(pf) and is_list(xy) and length(pf) == 2 and length(xy) == 2 and is_symbol(xy[1]) and is_symbol(xy[2]) and c30_iradon(g, pf[1], pf[2], xy[1], xy[2]) !== none =
  c30_iradon(g, pf[1], pf[2], xy[1], xy[2])
fn inverse_radon_transform(g, pf, xy) when is_list(pf) and is_list(xy) and length(pf) == 2 and length(xy) == 2 and is_numeric(xy[1]) and is_numeric(xy[2]) and c30_iradon(g, pf[1], pf[2], c30_rx, c30_ry) !== none =
  rewrite(c30_iradon(g, pf[1], pf[2], c30_rx, c30_ry), [c30_rx -> xy[1], c30_ry -> xy[2]])

# --- complete_integral ---
# complete_integral(eq, u(x1, ..., xn), [x1, ..., xn]) for a first-order equation F(x, u, p) = 0 (p_i = du/dx_i) that
# is free of u and depends explicitly on at most one variable x_k (the last variable when none): the derivatives
# p_j = c(j' + 1) of the other variables are constants (numbered in the order of the variables, after the additive constant c(1)), p_k solves F = 0, and
# u = c(1) + integrate(p_k, x_k) + sum c(j' + 1) x_j, the separable complete integral (Courant and Hilbert, Methods of
# mathematical physics II, II.4; Evans, Partial differential equations, 3.1). Other equations (needing the
# characteristic strips of Lagrange and Charpit) stay unevaluated.
# Clairaut's equation u == sum x_i p_i + g(p): the complete integral u = sum c_i x_i + g(c) (Courant and Hilbert II.2)
fn c30_clairaut(eq, uc, xs) = {
  let k0 = length(xs)
  let ps = map(i => c30_cip(i), range(k0))
  let f0 = if not is_atom(eq) and head(eq) === equal then eq[1] - eq[2] else eq
  let f = rewrite(rewrite(f0, map(i => d(uc, xs[i]) -> ps[i], range(k0))), uc -> c30_cu)
  let s = d(f, c30_cu)
  if not is_numeric(s) or s === 0 then none else {
    let g = expand(c30_cu - total(map(i => xs[i]*ps[i], range(k0))) - f/s)
    if not free_of(g, c30_cu) or some(xs, v => not free_of(g, v)) then none
    else expand(total(map(i => c(i)*xs[i], range(k0))) + rewrite(g, map(i => ps[i] -> c(i), range(k0))))
  }
}
fn c30_ci(eq, uc, xs) = {
  let cl = c30_clairaut(eq, uc, xs)
  if cl !== none then cl else c30_ci_separable(eq, uc, xs)
}
fn c30_ci_separable(eq, uc, xs) = {
  let k0 = length(xs)
  let ps = map(i => c30_cip(i), range(k0))
  let f0 = if not is_atom(eq) and head(eq) === equal then eq[1] - eq[2] else eq
  let f = rewrite(f0, map(i => d(uc, xs[i]) -> ps[i], range(k0)))
  let explicit = select(range(k0), i => not free_of(f, xs[i]))
  if not free_of(f, uc) or not free_of(f, head(uc)) or length(explicit) > 1 then none
  else {
    # the variable whose derivative is solved for: the explicit one (else the last), and when its equation has not
    # exactly one solution the others in turn (u_t + u_x^2 == 0: p_t = -c^2 is unique, p_x = +-sqrt(-c) is not)
    let first = if length(explicit) == 1 then explicit[1] else k0
    let order = join([first], select(range(k0), i => i != first and length(explicit) == 0))
    var res = none
    for k in order {
      if res === none then {
        let others = select(range(k0), i => i != k)
        # the additive constant is c(1) when the equation is solved for a variable other than the preferred one
        # (u_t + u_x^2 == 0: u = c(1) + c(2) x - c(2)^2 t), else the last constant
        let shifted = k != first
        let cs = map(j => c(if shifted then j + 1 else j), range(length(others)))
        let fk = rewrite(f, map(j => ps[others[j]] -> cs[j], range(length(others))))
        let sol = solve(fk == 0, ps[k])
        if length(sol) == 1 then {
          let pk = rewrite(ps[k], sol[1])
          let ik = integrate(pk, xs[k])
          if free_of(ik, integrate) then res = expand(ik + total(map(j => cs[j]*xs[others[j]], range(length(others)))) + c(if shifted then 1 else length(others) + 1))
        }
      }
    }
    res
  }
}
fn complete_integral(eq, uc, xs) when is_list(xs) and not is_atom(uc) and apply(list, uc) === xs and c30_ci(eq, uc, xs) !== none = [[uc -> c30_ci(eq, uc, xs)]]
fn complete_integral(eq, u, xs) when is_list(xs) and is_symbol(u) and c30_ci(eq, apply(u, xs), xs) !== none =
  [[u -> rewrite(`fn`(c30_civars, c30_cibody), [c30_civars -> xs, c30_cibody -> c30_ci(eq, apply(u, xs), xs)])]]
