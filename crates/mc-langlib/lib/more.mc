#! mathcas 2
# More core-language functions written in MathCAS itself, evaluated after the other libraries. Rules as for the
# language library: never define a symbol that has a native implementation; every public function is listed in
# `MORE_FUNCTIONS` (src/lib.rs) and has a page docs/functions/<name>.mcn. Helpers are prefixed by the function they
# serve and are not part of the interface.

# --- machine numbers ---

# max_machine_number: the largest finite machine number, (2 - 2^-52) 2^1023
rule max_machine_number := 1.7976931348623157e308

# --- unitize ---

# whether the numeric term x is zero: true, false, or null when it cannot be decided (a 30-digit enclosure that
# excludes 0 proves x nonzero; simplify giving 0 proves it zero)
fn unitize_zero(x) = {
  # a comparison that is undecided stays unevaluated: each test is read as true, false or neither
  let z = if is_number(x) then x == 0 else null
  let big = if is_number(x) then null else abs(n(x, 30)) > 0
  if z === true or z === false then z
  else if big === true then false
  else if not is_number(x) and simplify(x) === 0 then true
  else null
}

# 0 when x is zero (when abs(x) < d for d > 0), 1 otherwise; null when that is undecided
fn unitize_one(x, d) = {
  let c = if d === 0 then unitize_zero(x) else abs(x) < d
  if c === true then 0 else if c === false then 1 else null
}

# unitize(x): 0 when x is zero, else 1; unitize(x, d): 0 when abs(x) < d, else 1; lists elementwise; a value
# whose zero test is undecided stays unevaluated
rule unitize(x_) := map(y => unitize(y), x) when is_list(x)
rule unitize(x_, d_) := map(y => unitize(y, d), x) when is_list(x) and is_numeric(d)
rule unitize(x_) := unitize_one(x, 0) when not is_list(x) and is_numeric(x) and unitize_one(x, 0) !== null
rule unitize(x_, d_) := unitize_one(x, d) when not is_list(x) and is_numeric(x) and is_numeric(d) and d >= 0 and unitize_one(x, d) !== null

# --- lists ---

# is_duplicate_free(xs): whether no two elements of xs are identical; is_duplicate_free(xs, test): whether test(a, b)
# is true for no two elements a before b
fn is_duplicate_free(xs) when is_list(xs) = length(unique(xs)) == length(xs)
fn is_duplicate_free(xs, test) when is_list(xs) = {
  let m = length(xs)
  var ok = true
  var i = 1
  while ok and i < m {
    var j = i + 1
    while ok and j <= m {
      if test(xs[i], xs[j]) === true then ok = false
      j = j + 1
    }
    i = i + 1
  }
  ok
}

# matching_dissimilarity(u, v): the fraction of the positions where the Boolean vectors u and v (entries true and
# false, or 1 and 0) differ
fn matching_dissimilarity_bit(x) = if x === true or x === 1 then 1 else if x === false or x === 0 then 0 else null
fn matching_dissimilarity_bits(u) = is_list(u) and length(u) > 0 and every(u, x => matching_dissimilarity_bit(x) !== null)
fn matching_dissimilarity(u, v) when matching_dissimilarity_bits(u) and matching_dissimilarity_bits(v) and length(u) == length(v) =
  total(map(i => if matching_dissimilarity_bit(u[i]) == matching_dissimilarity_bit(v[i]) then 0 else 1, range(length(u))))/length(u)

# --- associations held in symbols ---

# associate_to(a, rules): the association that is the value of the symbol a with the rules (a rule, a list of rules,
# or an association) added, later keys replacing earlier ones; a is set to it
fn associate_to(held a: Sym, r) = {
  let v = eval(a)
  if not is_assoc(v) then failure(argument, associate_to, 1)
  else {
    let w = join(v, if is_assoc(r) then r else assoc(if is_list(r) then r else [r]))
    set(a, w)
    w
  }
}

# key_drop_from(a, k), key_drop_from(a, [k1, ...]): the association that is the value of the symbol a without those
# keys; a is set to it
fn key_drop_from(held a: Sym, k) = {
  let v = eval(a)
  if not is_assoc(v) then failure(argument, key_drop_from, 1)
  else {
    let w = key_drop(v, if is_list(k) then k else [k])
    set(a, w)
    w
  }
}

# --- uncertain vectors ---

# whether s is a nonempty list of non-list entries
fn vector_around_flat(s) = is_list(s) and length(s) > 0 and every(s, x => not is_list(x))

# vector_around(v, [s1, ...]): the vector v with independent standard uncertainties si; vector_around(v, c): with
# covariance matrix c; vector_around(v, [[s1, ...], r]) and vector_around(v, [[s1, ...], R]): uncertainties si with
# correlation r between every two entries, or the correlation matrix R, kept as the covariance matrix
rule vector_around(v_, c_) := vector_around(v, table(c[1][i]*c[1][j]*(if i == j then 1 else c[2]), i, length(v), j, length(v))) when is_list(v) and is_list(c) and length(c) == 2 and vector_around_flat(c[1]) and length(c[1]) == length(v) and not is_list(c[2]) and is_numeric(c[2])
rule vector_around(v_, c_) := vector_around(v, table(c[1][i]*c[1][j]*c[2][i][j], i, length(v), j, length(v))) when is_list(v) and is_list(c) and length(c) == 2 and vector_around_flat(c[1]) and length(c[1]) == length(v) and is_matrix(c[2]) and dimensions(c[2]) == [length(v), length(v)]

# the covariance matrix of vector_around(v, s)
fn vector_around_covariance(s) = if vector_around_flat(s) then diagonal_matrix(map(x => x^2, s)) else s

# --- uncertainty propagation through an expression ---

# the variables, means and covariance matrix of the rules of around_replace (x -> around(m, s) and
# [x1, ...] -> vector_around(ms, c)), as [vars, means, cov]; null when a rule is of neither form
fn around_replace_parts(rules) = {
  let rs = if is_list(rules) then rules else [rules]
  var vars = []
  var means = []
  var blocks = []
  var bad = false
  for r in rs {
    if not match_q(r, _ -> _) then bad = true
    else if is_symbol(r[1]) and match_q(r[2], around(_, _)) then {
      vars = append(vars, r[1])
      means = append(means, r[2][1])
      blocks = append(blocks, [[r[2][2]^2]])
    } else if is_list(r[1]) and every(r[1], is_symbol) and match_q(r[2], vector_around(_, _)) and is_list(r[2][1]) and length(r[2][1]) == length(r[1]) then {
      vars = join(vars, r[1])
      means = join(means, r[2][1])
      blocks = append(blocks, vector_around_covariance(r[2][2]))
    } else bad = true
  }
  if bad or length(vars) == 0 or not is_duplicate_free(vars) then null
  else {
    # the block and the position in it of each variable
    var owner = []
    for b in range(length(blocks)) { for k in range(length(blocks[b])) { owner = append(owner, [b, k]) } }
    let m = length(vars)
    let cov = table(if owner[i][1] == owner[j][1] then blocks[owner[i][1]][owner[i][2]][owner[j][2]] else 0, i, m, j, m)
    [vars, means, cov]
  }
}

# around(mean, sd) of e for the variables of the rules, to first order (order 1: mean e(mu), variance g C g with g
# the gradient at the means mu and C the covariance) or to second order (order 2: the mean adds tr(H C)/2 and the
# variance tr(H C H C)/2, H the Hessian at the means, as for normally distributed variables)
fn around_replace_moments(e, p, order) = {
  let vars = p[1]
  let sub = map(i => vars[i] -> p[2][i], range(length(vars)))
  let c = p[3]
  let f0 = rewrite(e, sub)
  let g = rewrite(grad(e, vars), sub)
  let v1 = dot(g, dot(c, g))
  if order == 1 then around(f0, sqrt(v1))
  else {
    let hc = dot(rewrite(d(e, [vars, 2]), sub), c)
    around(f0 + trace(hc)/2, sqrt(v1 + trace(dot(hc, hc))/2))
  }
}

# around_replace(e, rules), around_replace(e, rules, order): e with the variables of the rules replaced by uncertain
# values (x -> around(m, s), [x1, ...] -> vector_around(ms, c)), the uncertainty propagated through e as a whole (so
# variables occurring several times are correlated with themselves) to first order, or to second order
rule around_replace(e_, rules_) := around_replace_moments(e, around_replace_parts(rules), 1) when around_replace_parts(rules) !== null
rule around_replace(e_, rules_, k_) := around_replace_moments(e, around_replace_parts(rules), k) when (k === 1 or k === 2) and around_replace_parts(rules) !== null

# --- intervals ---

# the intersection of two lists of boxes [re lo, re hi, im lo, im hi]
fn interval_intersection_boxes(as, bs) = {
  var out = []
  for a in as {
    for b in bs {
      let lo = max(a[1], b[1])
      let hi = min(a[2], b[2])
      let ilo = max(a[3], b[3])
      let ihi = min(a[4], b[4])
      if lo <= hi and ilo <= ihi then out = append(out, [lo, hi, ilo, ihi])
    }
  }
  out
}

# interval_intersection(i1, i2, ...): the intersection of intervals and centered intervals: an interval of the real
# pieces common to all; the centered interval of the common box when a complex centered interval leaves one
fn interval_intersection(parts...) when length(parts) >= 1 and every(parts, t => head(t) === interval or head(t) === centered_interval) = {
  let bs = fold((acc, p) => interval_intersection_boxes(acc, b29_boxes(p)), b29_boxes(parts[1]), rest(parts))
  if every(bs, b => b[3] == 0 and b[4] == 0) then apply(interval, map(b => [b[1], b[2]], bs))
  else if length(bs) == 1 then {
    let b = bs[1]
    centered_interval((b[1] + b[2])/2 + im*(b[3] + b[4])/2, (b[2] - b[1])/2 + im*(b[4] - b[3])/2)
  } else failure(argument, interval_intersection, 1)
}

# --- divisions for ticks ---

# the step multipliers in base b: 1, 2 and 5 in base 10, 1 in other bases
fn find_divisions_mantissas(b) = if b == 10 then [1, 2, 5] else [1]

# the nice steps u m b^k (m a multiplier, k >= kmin) near the step span/count, as a list of exact numbers
fn find_divisions_steps(span, count, u, b, kmin) = {
  let k0 = floor(n(log(span/(count*u))/log(b)))
  let ks = select(range(k0 - 1, k0 + 2), k => k >= kmin)
  let ks2 = if length(ks) == 0 then [kmin] else ks
  flatten(map(k => map(m => u*m*b^k, find_divisions_mantissas(b)), ks2))
}

# the multiples of the nice step s from floor(lo/s) s to ceiling(hi/s) s whose number of intervals is closest to
# count (ties: the smaller step), among the steps ss
fn find_divisions_best(lo, hi, count, ss) = {
  let counts = map(s => ceiling(hi/s) - floor(lo/s), ss)
  let dist = map(c => abs(c - count), counts)
  let best = min(dist)
  let s = min(map(i => ss[i], select(range(length(ss)), i => dist[i] == best)))
  map(j => j*s, range(floor(lo/s), ceiling(hi/s)))
}

# one level of divisions of [lo, hi] into about count intervals; steps multiples of u (k >= 0) when u is given, the
# steps also dividing the step c of the level above when c is not null
fn find_divisions_level(lo, hi, count, u, b, c) = {
  let span = hi - lo
  let ss0 = find_divisions_steps(span, count, if u === null then 1 else u, b, if u === null then -inf else 0)
  let ss = if c === null then ss0 else select(ss0, s => is_integer(c/s))
  find_divisions_best(lo, hi, count, if length(ss) == 0 then [if c === null then ss0[1] else c] else ss)
}

# the step of a list of division points
fn find_divisions_step(xs) = if length(xs) >= 2 then xs[2] - xs[1] else null

# find_divisions([a, b], n): about n nice divisions covering [a, b] (multiples of 1, 2 or 5 times a power of 10);
# find_divisions([a, b, u], n): multiples of u; find_divisions(spec, n, base): powers of base; find_divisions([a, b],
# [n1, n2]): the divisions and, for each interval between them, its own divisions into about n2
fn find_divisions_run(lo, hi, ns, us, b) = {
  let top = find_divisions_level(lo, hi, ns[1], us[1], b, null)
  if length(ns) == 1 then top
  else {
    let c = find_divisions_step(top)
    [top, map(i => find_divisions_level(top[i], top[i + 1], ns[2], us[2], b, if us[2] === null then c else null), range(length(top) - 1))]
  }
}
fn find_divisions_ok(spec, ns, b) = is_list(spec) and (length(spec) == 2 or length(spec) == 3) and is_real(n(spec[1])) and is_real(n(spec[2])) and n(spec[1]) < n(spec[2]) and every(if is_list(ns) then ns else [ns], k => is_integer(k) and k >= 1) and (not is_list(ns) or (length(ns) >= 1 and length(ns) <= 2)) and is_integer(b) and b >= 2
fn find_divisions_units(spec, m) = if length(spec) == 2 then constant_array(null, m) else if is_list(spec[3]) then (if length(spec[3]) == m then spec[3] else null) else constant_array(spec[3], m)
fn find_divisions(spec, ns, b = 10) when find_divisions_ok(spec, ns, b) and find_divisions_units(spec, if is_list(ns) then length(ns) else 1) !== null = {
  let nl = if is_list(ns) then ns else [ns]
  find_divisions_run(spec[1], spec[2], nl, find_divisions_units(spec, length(nl)), b)
}
