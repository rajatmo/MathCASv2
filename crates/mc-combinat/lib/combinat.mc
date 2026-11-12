#! mathcas 2
# The number-sequence library: functions written in MathCAS itself, evaluated after the
# prelude when the engine starts. Everything here is in the bare namespace. Rules as for the prelude: never define a
# symbol that has a native implementation (rules for the forms a native declines are fine). Every public function is
# listed in `FUNCTIONS` (src/lib.rs) and has a page docs/functions/<name>.mcn.

# --- polygonal numbers, bits and small constants ---

# polygonal_number(k): the k-th triangular number; polygonal_number(r, k): the k-th r-gonal number
# ((r - 2) k^2 - (r - 4) k)/2 (a polynomial in both arguments, so symbolic arguments work); lists thread
rule polygonal_number(ks_) := map(polygonal_number, ks) when is_list(ks)
rule polygonal_number(k_) := k*(k + 1)/2 when not is_list(k)
# a regular polygon stands for its number of sides
rule polygonal_number(p_, k_) := polygonal_number(length(p[1]), k) when not is_atom(p) and head(p) === polygon
rule polygonal_number(p_, k_) := polygonal_number(last(p), k) when not is_atom(p) and head(p) === regular_polygon
rule polygonal_number(r_, k_) := ((r - 2)*k^2 - (r - 4)*k)/2 when not is_list(r) and not is_list(k)
rule polygonal_number(r_, ks_) := map(j => polygonal_number(r, j), ks) when is_list(ks) and not is_list(r)
rule polygonal_number(rs_, k_) := map(j => polygonal_number(j, k), rs) when is_list(rs) and not is_list(k)
# two lists of one length pair up
rule polygonal_number(rs_, ks_) := map(i => polygonal_number(rs[i], ks[i]), range(length(rs))) when is_list(rs) and is_list(ks) and length(rs) == length(ks)

# thue_morse(k): the parity of the number of 1 bits of the integer k >= 0 (lists thread)
rule thue_morse(ks_) := map(thue_morse, ks) when is_list(ks)
rule thue_morse(k_) := mod(digit_sum(k, 2), 2) when is_integer(k) and k >= 0

# golden_angle: pi (3 - sqrt(5)), the angle in radians that divides a circle in the golden ratio
golden_angle := pi*(3 - sqrt(5))

# bit_length(k): the number of bits of k (of -k - 1 for negative k); bit_shift_right(k, s): floor(k / 2^s);
# bit_flip(k, i): k with bit i (0 the least significant) flipped; all thread over lists
rule bit_length(ks_) := map(bit_length, ks) when is_list(ks)
rule bit_length(k_) := integer_length(if k < 0 then -k - 1 else k, 2) when is_integer(k)
rule bit_shift_right(ks_, s_) := map(j => bit_shift_right(j, s), ks) when is_list(ks)
rule bit_shift_right(k_, s_) := quotient(k, 2^s) when is_integer(k) and is_integer(s) and s >= 0
rule bit_flip(ks_, i_) := map(j => bit_flip(j, i), ks) when is_list(ks)
rule bit_flip(k_, i_) := bit_xor(k, 2^i) when is_integer(k) and is_integer(i) and i >= 0

# --- Roman numerals ---

# roman_numeral(k): the Roman numeral of the integer k >= 0 as a string (0 is "N"; the thousands are repeated M);
# from_roman_numeral(s): the integer of a Roman numeral string
rule roman_numeral(ks_) := map(roman_numeral, ks) when is_list(ks)
rule roman_numeral(0) := "N"
rule roman_numeral(k_) := {
  let vals = [900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1]
  let syms = ["CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I"]
  var r = mod(k, 1000)
  var out = [string_join(constant_array("M", quotient(k, 1000)))]
  for i in range(12) {
    while r >= vals[i] {
      out = append(out, syms[i])
      r = r - vals[i]
    }
  }
  string_join(out)
} when is_integer(k) and k > 0
rule from_roman_numeral(ss_) := map(from_roman_numeral, ss) when is_list(ss)
rule from_roman_numeral(s_) := from_digits(to_upper(s), "Roman") when is_string(s)

# --- polynomial forms of the Bernoulli, Euler and Bell numbers ---

# bell_b(m, x): the Bell polynomial, the sum of stirling_s2(m, j) x^j; bernoulli_b(m, x): the Bernoulli polynomial,
# the sum of binomial(m, j) B_j x^(m - j); euler_e(m, x): the Euler polynomial, the sum of binomial(m, j) E_j
# (x - 1/2)^(m - j) / 2^j. The integer-index numbers are natives and x any expression (a list threads).
rule bell_b(m_, x_) := expand(sum(stirling_s2(m, j)*x^j, [j, 0, m])) when is_integer(m) and m >= 0 and not is_list(x)
rule bernoulli_b(m_, x_) := expand(sum(binomial(m, j)*bernoulli_b(j)*x^(m - j), [j, 0, m])) when is_integer(m) and m >= 0 and not is_list(x)
rule euler_e(m_, x_) := expand(sum(binomial(m, j)*euler_e(j)/2^j*(x - 1/2)^(m - j), [j, 0, m])) when is_integer(m) and m >= 0 and not is_list(x)
rule bell_b(m_, xs_) := map(x => bell_b(m, x), xs) when is_list(xs)
rule bernoulli_b(m_, xs_) := map(x => bernoulli_b(m, x), xs) when is_list(xs)
rule euler_e(m_, xs_) := map(x => euler_e(m, x), xs) when is_list(xs)
rule bell_b(ms_) := map(bell_b, ms) when is_list(ms)
rule bernoulli_b(ms_) := map(bernoulli_b, ms) when is_list(ms)
rule euler_e(ms_) := map(euler_e, ms) when is_list(ms)

# norlund_b(m, a): the Norlund (generalised Bernoulli) number, m! times the coefficient of t^m in (t/(e^t - 1))^a,
# a polynomial in a; norlund_b(m, a, x): the Norlund polynomial, with e^(x t) in the generating function. The power
# series of g = f^a, f = t/(e^t - 1), by the recurrence m g_m = sum over j of (a j - m + j) f_j g_(m - j).
fn norlund_series(m: Int, a) = {
  var g = [1]
  for j in range(m) {
    var s = 0
    for i in range(j) {
      s = s + (a*i - j + i)*bernoulli_b(i)/factorial(i)*g[j - i + 1]
    }
    g = append(g, expand(s/j))
  }
  g
}
rule norlund_b(m_, a_) := expand(factorial(m)*last(norlund_series(m, a))) when is_integer(m) and m >= 0
rule norlund_b(m_, a_, x_) := {
  let g = norlund_series(m, a)
  expand(sum(binomial(m, j)*factorial(j)*g[j + 1]*x^(m - j), [j, 0, m]))
} when is_integer(m) and m >= 0

# --- chinese remainders, Farey sequences and convergents ---

# crt_solve(rs, ms): [x, M], x the least non-negative solution of x = r_i mod m_i and M the lcm of the moduli, or []
# when the congruences are inconsistent
fn crt_solve(rs, ms) = {
  var x = 0
  var big = 1
  var ok = true
  for i in range(length(rs)) {
    if ok then {
      let mi = ms[i]
      let g = gcd(big, mi)
      let d = rs[i] - x
      if mod(d, g) != 0 then ok = false else {
        let q = quotient(mi, g)
        let t = if q == 1 then 0 else mod(quotient(d, g)*modular_inverse(quotient(big, g), q), q)
        x = x + big*t
        big = big*q
        x = mod(x, big)
      }
    }
  }
  if ok then [x, big] else []
}

# chinese_remainder(rs, ms): the least non-negative x with x = r_i mod m_i (positive integer moduli, not
# necessarily coprime; no value when the congruences are inconsistent); chinese_remainder(rs, ms, d): the least x >= d
rule chinese_remainder(rs_, ms_) := crt_solve(rs, ms)[1] when is_list(rs) and is_list(ms) and length(rs) == length(ms) and length(rs) > 0 and every(rs, is_integer) and every(ms, m => is_integer(m) and m > 0) and length(crt_solve(rs, ms)) == 2
rule chinese_remainder(rs_, ms_, d_) := {
  let s = crt_solve(rs, ms)
  s[1] + s[2]*ceiling((d - s[1])/s[2])
} when is_list(rs) and is_list(ms) and length(rs) == length(ms) and length(rs) > 0 and every(rs, is_integer) and every(ms, m => is_integer(m) and m > 0) and is_integer(d) and length(crt_solve(rs, ms)) == 2

# farey_sequence(k): the fractions in [0, 1] with denominator at most k, in increasing order; farey_sequence(k, j):
# the j-th of them
fn farey_list(k: Int) = {
  var out = [0, 1/k]
  var a = 0
  var b = 1
  var c = 1
  var d = k
  while c < d {
    let q = quotient(k + b, d)
    let e = q*c - a
    let f = q*d - b
    a = c
    b = d
    c = e
    d = f
    out = append(out, c/d)
  }
  out
}
rule farey_sequence(ks_) := map(farey_sequence, ks) when is_list(ks)
rule farey_sequence(k_) := { if k == 1 then [0, 1] else farey_list(k) } when is_integer(k) and k >= 1
rule farey_sequence(k_, j_) := farey_list(k)[j] when is_integer(k) and k >= 2 and is_integer(j) and j >= 1 and j <= length(farey_list(k))

# conv_terms(ts): the convergents of the continued fraction with terms ts, by p_k = a_k p_(k-1) + p_(k-2)
fn conv_terms(ts) = {
  var p0 = 1
  var q0 = 0
  var p1 = ts[1]
  var q1 = 1
  var out = [ts[1]]
  for i in range(2, length(ts)) {
    let p2 = expand(ts[i]*p1 + p0)
    let q2 = expand(ts[i]*q1 + q0)
    p0 = p1
    q0 = q1
    p1 = p2
    q1 = q2
    out = append(out, p1/q1)
  }
  out
}
# convergents(x), convergents(x, k): the convergents of the continued fraction of x (a list: of the continued
# fraction with these terms); a periodic list [a0, ..., [b1, ..., bk]] gives the convergents of a0, ..., b(k-1) and
# then the exact value of the periodic fraction, where the first period closes
rule convergents(ts_) := {
  let per = last(ts)
  let head = join(most(ts), most(per))
  append(if length(head) > 0 then conv_terms(head) else [], from_continued_fraction(ts))
} when is_list(ts) and length(ts) > 0 and is_list(last(ts)) and length(last(ts)) > 0
rule convergents(ts_) := conv_terms(ts) when is_list(ts) and length(ts) > 0 and not is_list(last(ts))
rule convergents(x_) := conv_terms(continued_fraction(x)) when is_rational(x) or (is_inexact(x) and is_real(x))
rule convergents(x_, k_) := conv_terms(continued_fraction(rewrite(x, golden_ratio -> (1 + sqrt(5))/2), k)) when is_integer(k) and k >= 1 and not is_list(x)

# --- mantissa and exponent, digits of numbers, expansions ---

# mx_cmp(a, b): the sign of a - b for a closed numeric pair (exact when both are rational)
fn mx_cmp(a, b) = if is_rational(a) and is_rational(b) then sign(a - b) else if is_inexact(a) or is_inexact(b) then sign(a - b) else sign(n(a - b, 40))

# mantissa_exponent(x), mantissa_exponent(x, b): [m, e] with x = m b^e and 1/b <= |m| < 1 (b = 10 by default; zero
# gives [0, 0]); exact for exact x, so the mantissa of 123451 is 123451/1000000
rule mantissa_exponent(xs_) := map(mantissa_exponent, xs) when is_list(xs)
rule mantissa_exponent(x_) := mantissa_exponent(x, 10) when is_numeric(x) and not is_list(x)
rule mantissa_exponent(x_, b_) := {
  if x == 0 then [0, 0] else {
    let ax = abs(x)
    var e = floor(n(log(ax)/log(b))) + 1
    while mx_cmp(ax, b^e) >= 0 { e = e + 1 }
    while mx_cmp(ax, b^(e - 1)) < 0 { e = e - 1 }
    [if is_inexact(x) then x*b^(-e) else x/b^e, e]
  }
} when is_numeric(x) and is_numeric(b) and not is_list(x) and mx_cmp(b, 1) > 0

# number_digit(x, k), number_digit(x, k, b): the digit of |x| at position k in base b (b^k's place; negative k after
# the point); a list of positions gives a list, a list of numbers a list of those lists
rule number_digit(xs_, ks_) := map(x => number_digit(x, ks), xs) when is_list(xs)
rule number_digit(xs_, ks_, b_) := map(x => number_digit(x, ks, b), xs) when is_list(xs)
rule number_digit(x_, ks_) := number_digit(x, ks, 10) when is_list(ks)
rule number_digit(x_, k_) := number_digit(x, k, 10) when is_integer(k)
rule number_digit(x_, ks_, b_) := map(k => number_digit(x, k, b), ks) when is_list(ks)
rule number_digit(x_, k_, b_) := {
  let ax = if is_inexact(x) then rationalize(abs(x), 0) else abs(x)
  # an inexact number has ceiling(precision) significant digits, the places below are unknown
  let lowest = if is_inexact(x) and x != 0 then real_digits(abs(x), b, 1)[2] - ceiling(precision(x)*n(log(10)/log(b))) else null
  if lowest !== null and k < lowest then indeterminate else mod(floor(ax/b^k), b)
} when is_integer(k) and is_integer(b) and b > 1 and is_numeric(x)

# number_expand(x), (x, b), (x, b, len): the digits of x in base b times their place values, 157 as [100, 50, 7]; a
# fraction stays in the last term (the units place), len terms keep the lower digits in the last one; complex
# numbers expand both parts
fn expand_real_old(x, b: Int, len) = {
  let ax = abs(x)
  let ip = floor(ax)
  let ds = if ip == 0 then [0] else integer_digits(ip, b)
  let nd = length(ds)
  let m = if len === null then nd else min(len, nd)
  var out = map(i => ds[i]*b^(nd - i), range(m))
  if m < nd or ax != ip then out = append(most(out), ax - total(most(out))) else out = out
  map(t => sign(x)*t, out)
}
# the place of the leading digit of a nonzero real: b^top <= |x| < b^(top + 1)
fn ne_top(ax, b) = real_digits(ax, b, 1)[2] - 1
# n terms of |x|: its digits at the places top, top - 1, ... times their place values, then the rest in the last term
# (an inexact number is first rounded to its n - 1 digits)
fn ne_terms(ax, b: Int, top, n) = {
  let q = b^(top - n + 2)
  let ar = if is_inexact(ax) then round(rationalize(ax, 0)/q)*q else ax
  let ds = map(j => mod(floor(ar/b^(top - j + 1)), b)*b^(top - j + 1), range(n - 1))
  append(ds, ax - total(ds))
}
# the number of digits of a terminating |x| in base b, or none
fn ne_exact_digits(ax, b: Int, top) = {
  var d = denominator(ax)
  var k = 0
  while d != 1 and k < 10000 { let g = gcd(d, b); if g == 1 then k = 10000 else { d = d/g; k = k + 1 } }
  if d == 1 then top + 1 + max(0, ne_scale(ax, b)) else none
}
fn ne_scale(ax, b: Int) = { var k = 0; var v = ax; while v != floor(v) { v = v*b; k = k + 1 }; k }
fn expand_real(x, b: Int, len) = expand_parts([x], b, len)[1]
# the expansion of several real parts over common places
fn expand_parts(xs, b: Int, len) = {
  let axs = map(abs, xs)
  let nz = select(axs, a => a != 0)
  let inexact = some(xs, is_inexact)
  if length(nz) == 0 then map(x => [x], xs) else {
    let top = max(map(a => ne_top(a, b), nz))
    let n = if len !== null then len
      else if inexact then ceiling(min(map(precision, select(xs, is_inexact)))*n(log(10)/log(b)))
      else {
        let ds = map(a => if a == 0 then 1 else ne_exact_digits(a, b, top), axs)
        if some(ds, d => d === none) then none else max(ds)
      }
    if n === none then map(x => expand_real_old(x, b, null), xs)
    else map(i => map(t => sign(xs[i])*t, ne_terms(axs[i], b, top, max(n, 1))), range(length(xs)))
  }
}
rule number_expand(x_) := number_expand(x, 10) when is_numeric(x)
rule number_expand(x_, b_) := number_expand(x, b, null) when is_numeric(x)
rule number_expand(x_, b_, len_) := {
  if is_real(x) then expand_real(x, b, len) else {
    let r = expand_parts([re(x), im_part(x)], b, len)
    map(i => r[1][i] + sqrt(-1)*r[2][i], range(length(r[1])))
  }
} when is_numeric(x) and is_integer(b) and b > 1 and (len === null or (is_integer(len) and len > 0))

# number_compose(digits, bases): the sum of digit times base, the inverse of the decomposition of number_decompose
rule number_compose(ds_, bs_) := sum(map(i => ds[i]*bs[i], range(length(ds)))) when is_list(ds) and is_list(bs) and length(ds) == length(bs)

# --- de Bruijn sequences, group counts, Mersenne exponents ---

# de_bruijn_sequence(k, m), (alphabet, m): the lexicographically least cyclic sequence of the k^m words of length m
# over k symbols (the alphabet's, or 0..k-1); a string alphabet gives a string. FKM construction by the Lyndon words
# whose length divides m (Fredricksen, Kessler, Maiorana; Ruskey's formulation)
fn de_bruijn_indices(k: Int, m: Int) = {
  var w = [0]
  var out = []
  var go = true
  while go {
    if mod(m, length(w)) == 0 then out = join(out, w)
    let l = length(w)
    w = map(i => w[mod(i - 1, l) + 1], range(m))
    while length(w) > 0 and last(w) == k - 1 { w = most(w) }
    if length(w) == 0 then go = false else w = append(most(w), last(w) + 1)
  }
  out
}
rule de_bruijn_sequence(k_, m_) := de_bruijn_indices(k, m) when is_integer(k) and k >= 1 and is_integer(m) and m >= 1
rule de_bruijn_sequence(a_, m_) := map(i => a[i + 1], de_bruijn_indices(length(a), m)) when is_list(a) and length(a) >= 1 and is_integer(m) and m >= 1
rule de_bruijn_sequence(s_, m_) := string_join(map(i => characters(s)[i + 1], de_bruijn_indices(string_length(s), m))) when is_string(s) and string_length(s) >= 1 and is_integer(m) and m >= 1

# the exponents of the Mersenne primes found so far (2^p - 1 for p in the list; the 52nd is 136279841)
combinat_mersenne_exponents := [2, 3, 5, 7, 13, 17, 19, 31, 61, 89, 107, 127, 521, 607, 1279, 2203, 2281, 3217, 4253, 4423, 9689, 9941, 11213, 19937, 21701, 23209, 44497, 86243, 110503, 132049, 216091, 756839, 859433, 1257787, 1398269, 2976221, 3021377, 6972593, 13466917, 20996011, 24036583, 25964951, 30402457, 32582657, 37156667, 42643801, 43112609, 57885161, 74207281, 77232917, 82589933, 136279841]
# mersenne_prime_exponent(k): the k-th known Mersenne prime exponent, k = 1 to 52; mersenne_prime_exponent_q(p):
# whether p is one of the known exponents; lists thread
rule mersenne_prime_exponent(ks_) := map(mersenne_prime_exponent, ks) when is_list(ks)
rule mersenne_prime_exponent(k_) := combinat_mersenne_exponents[k] when is_integer(k) and k >= 1 and k <= 52
rule mersenne_prime_exponent_q(ps_) := map(mersenne_prime_exponent_q, ps) when is_list(ps)
rule mersenne_prime_exponent_q(p_) := if is_integer(p) then member_of(combinat_mersenne_exponents, p) else false

# finite_abelian_group_count(k): the number of abelian groups of order k, the product of partitions_p(e) over the
# prime powers p^e exactly dividing k
rule finite_abelian_group_count(ks_) := map(finite_abelian_group_count, ks) when is_list(ks)
rule finite_abelian_group_count(k_) := product(map(f => partitions_p(f[2]), factor_integer(k))) when is_integer(k) and k >= 1

# finite_group_count(k): the number of groups of order k up to isomorphism: from a table for k up to 64, and by
# Hoelder's formula for squarefree k (the sum over m | k of the products over primes p | m of (p^c - 1)/(p - 1),
# c the number of primes q | k/m with q = 1 mod p); other orders stay unevaluated
combinat_group_counts := [1, 1, 1, 2, 1, 2, 1, 5, 2, 2, 1, 5, 1, 2, 1, 14, 1, 5, 1, 5, 2, 2, 1, 15, 2, 2, 5, 4, 1, 4, 1, 51, 1, 2, 1, 14, 1, 2, 2, 14, 1, 6, 1, 4, 2, 2, 1, 52, 2, 5, 1, 5, 1, 15, 2, 13, 2, 2, 1, 13, 1, 2, 4, 267]
fn holder_count(k: Int) = {
  let ps = map(f => f[1], factor_integer(k))
  sum(map(m => {
    let mp = select(ps, p => mod(m, p) == 0)
    let rst = select(ps, p => mod(m, p) != 0)
    product(map(p => (p^count_if(rst, q => mod(q, p) == 1) - 1)/(p - 1), mp))
  }, divisors(k)))
}
rule finite_group_count(ks_) := map(finite_group_count, ks) when is_list(ks)
rule finite_group_count(k_) := combinat_group_counts[k] when is_integer(k) and k >= 1 and k <= 64
rule finite_group_count(k_) := holder_count(k) when is_integer(k) and k > 64 and is_square_free(k)

# --- representations of integers ---

# frobenius_solve(coins, k): the lists of non-negative integers x with coins.x = k, in increasing lexicographic
# order; frobenius_solve(coins, k, m): at most m of them
fn frobenius_list(cs, k: Int) = {
  if length(cs) == 1 then (if mod(k, cs[1]) == 0 then [[quotient(k, cs[1])]] else [])
  else flat_map(x => map(r => join([x], r), frobenius_list(rest(cs), k - x*cs[1])), range(0, quotient(k, cs[1])))
}
rule frobenius_solve(cs_, k_) := frobenius_list(cs, k) when is_list(cs) and length(cs) > 0 and every(cs, c => is_integer(c) and c > 0) and is_integer(k) and k >= 0
rule frobenius_solve(cs_, k_, m_) := take(frobenius_list(cs, k), min(m, length(frobenius_list(cs, k)))) when is_list(cs) and length(cs) > 0 and every(cs, c => is_integer(c) and c > 0) and is_integer(k) and k >= 0 and is_integer(m) and m >= 0

# frobenius_number(coins): the largest integer that is not a non-negative combination of the positive integers
# coins (inf when their gcd exceeds 1), by shortest paths over the residues modulo the smallest coin
rule frobenius_number(cs_) := {
  if apply(gcd, cs) > 1 then inf else {
    let a = min(cs)
    var dist = join([0], constant_array(10^100, a - 1))
    var changed = true
    while changed {
      changed = false
      for r in range(0, a - 1) {
        for c in cs {
          let nx = mod(r + c, a)
          if dist[r + 1] + c < dist[nx + 1] then {
            dist[nx + 1] = dist[r + 1] + c
            changed = true
          }
        }
      }
    }
    max(dist) - a
  }
} when is_list(cs) and length(cs) > 0 and every(cs, c => is_integer(c) and c > 0)

# --- sums of squares and of powers ---

# sq_chi(m): the character modulo 4 (1 for m = 1 mod 4, -1 for m = 3 mod 4, else 0)
fn sq_chi(m: Int) = if mod(m, 4) == 1 then 1 else if mod(m, 4) == 3 then -1 else 0

# squares_r(d, k): the number of ways to write the integer k >= 0 as a sum of d squares of integers (order and signs
# count). d = 2, 4, 6, 8 by the classical divisor-sum formulas (Jacobi, Eisenstein, Liouville), the other d by
# convolving the one-square counts
fn squares_convolve(d: Int, k: Int) = {
  var one = constant_array(0, k + 1)
  one[1] = 1
  var j = 1
  while j*j <= k {
    one[j*j + 1] = 2
    j = j + 1
  }
  var cur = one
  for step in range(d - 1) {
    var nxt = constant_array(0, k + 1)
    for t in range(0, k) {
      if one[t + 1] != 0 then {
        for u in range(0, k - t) {
          nxt[t + u + 1] = nxt[t + u + 1] + one[t + 1]*cur[u + 1]
        }
      }
    }
    cur = nxt
  }
  cur[k + 1]
}
rule squares_r(d_, k_) := map(j => squares_r(d, j), k) when is_list(k)
rule squares_r(ds_, k_) := map(j => squares_r(j, k), ds) when is_list(ds) and not is_list(k)
rule squares_r(0, k_) := { if k == 0 then 1 else 0 } when is_integer(k) and k >= 0
rule squares_r(d_, 0) := 1 when is_integer(d) and d >= 0
rule squares_r(2, k_) := 4*sum(map(m => sq_chi(m), divisors(k))) when is_integer(k) and k >= 1
rule squares_r(4, k_) := 8*sum(select(divisors(k), m => mod(m, 4) != 0)) when is_integer(k) and k >= 1
rule squares_r(6, k_) := 16*sum(map(m => sq_chi(quotient(k, m))*m^2, divisors(k))) - 4*sum(map(m => sq_chi(m)*m^2, divisors(k))) when is_integer(k) and k >= 1
rule squares_r(8, k_) := 16*sum(map(m => (-1)^(k + m)*m^3, divisors(k))) when is_integer(k) and k >= 1
rule squares_r(d_, k_) := squares_convolve(d, k) when is_integer(d) and is_integer(k) and k >= 1 and d >= 1 and d != 2 and d != 4 and d != 6 and d != 8 and k <= 3000

# pw_root(m, p): the integer part of the p-th root of m >= 0, exactly
fn pw_root(m: Int, p: Int) = {
  var r = floor(n(m^(1/p), 30))
  if m > 10^15 then r = if p == 2 then floor(sqrt(m)) else r
  while r^p > m { r = r - 1 }
  while (r + 1)^p <= m { r = r + 1 }
  r
}

# the representations of m as a sum of k p-th powers x_1 <= ... <= x_k with x_1 >= lo, in lexicographic order
fn pw_reps(m: Int, k: Int, p: Int, lo: Int) = {
  if k == 1 then {
    let r = pw_root(m, p)
    if r^p == m and r >= lo then [[r]] else []
  } else {
    var out = []
    var x = lo
    while k*x^p <= m {
      for t in pw_reps(m - x^p, k - 1, p, x) {
        out = append(out, join([x], t))
      }
      x = x + 1
    }
    out
  }
}

# gauss_mul(a, b): the product of two Gaussian integers given as [re, im]
fn gauss_mul(a, b) = [a[1]*b[1] - a[2]*b[2], a[1]*b[2] + a[2]*b[1]]

# the Gaussian integer [a, b] with a^2 + b^2 = p for a prime p = 1 mod 4 (Hermite-Serret)
fn gauss_prime(p: Int) = {
  var c = 2
  var t = power_mod(c, quotient(p - 1, 4), p)
  while mod(t*t, p) != p - 1 {
    c = c + 1
    t = power_mod(c, quotient(p - 1, 4), p)
  }
  var a = p
  var b = t
  while b*b > p {
    let r = mod(a, b)
    a = b
    b = r
  }
  [b, pw_root(p - b*b, 2)]
}

# the representations of m as a sum of two squares x <= y, from the factorisation over the Gaussian integers
fn two_squares(m: Int) = {
  var items = [[1, 0]]
  var ok = true
  for f in factor_integer(m) {
    let p = f[1]
    let e = f[2]
    if p == 2 then {
      let shift = if mod(e, 2) == 0 then [2^quotient(e, 2), 0] else [2^quotient(e, 2), 2^quotient(e, 2)]
      items = map(g => gauss_mul(g, shift), items)
    } else if mod(p, 4) == 3 then {
      if mod(e, 2) == 1 then ok = false else items = map(g => gauss_mul(g, [p^quotient(e, 2), 0]), items)
    } else {
      let pi0 = gauss_prime(p)
      let cj = [pi0[1], -pi0[2]]
      var pows = []
      for j in range(0, e) {
        var v = [1, 0]
        for u in range(j) { v = gauss_mul(v, pi0) }
        for u in range(e - j) { v = gauss_mul(v, cj) }
        pows = append(pows, v)
      }
      items = flat_map(g => map(v => gauss_mul(g, v), pows), items)
    }
  }
  if not ok then [] else sort(unique(map(g => sort([abs(g[1]), abs(g[2])]), items)))
}

# powers_representations(m, k, p): the ways to write the integer m >= 0 as a sum of k p-th powers of non-negative
# integers, as increasing lists, in lexicographic order; for two squares of a large m by the Gaussian factorisation
# the representations of m as a sum of two cubes 0 <= a <= b: a^3 + b^3 = s (s^2 - 3 a b) for s = a + b, so s divides m
# and lies between the cube roots of m and 4 m; then a b = (s^2 - m/s)/3 and a, b are the roots of t^2 - s t + a b
fn two_cubes(m: Int) = {
  var out = []
  let r0 = pw_root(m, 3)
  if r0^3 == m then out = [[0, r0]]
  let hi = pw_root(4*m, 3) + 1
  var pairs = []
  for s in select(divisors(m), d => d >= r0 and d <= hi) {
    let t = s^2 - quotient(m, s)
    if t > 0 and mod(t, 3) == 0 then {
      let d = s^2 - 4*quotient(t, 3)
      if d >= 0 then {
        let q = pw_root(d, 2)
        if q^2 == d and mod(s + q, 2) == 0 and s - q >= 2 then pairs = append(pairs, [quotient(s - q, 2), quotient(s + q, 2)])
      }
    }
  }
  join(out, sort(pairs))
}
rule powers_representations(m_, k_, p_) := {
  if k == 2 and p == 2 and m > 10^7 then two_squares(m) else if k == 2 and p == 3 and m > 10^6 then two_cubes(m) else pw_reps(m, k, p, 0)
} when is_integer(m) and m >= 0 and is_integer(k) and k >= 1 and is_integer(p) and p >= 1

# --- continued fraction K ---

# cfk_value(fs, gs): fs[1]/(gs[1] + fs[2]/(gs[2] + ... + fs[m]/gs[m]))
fn cfk_value(fs, gs) = {
  var t = last(gs)
  for i in range(length(gs) - 1, 1, -1) {
    t = gs[i] + fs[i + 1]/t
  }
  fs[1]/t
}
fn cfk_spec(sp) = if length(sp) == 2 then [sp[1], 1, sp[2]] else sp
fn cfk_terms(e, sp) = map(j => rewrite(e, sp[1] -> j), range(sp[2], sp[3]))
# continued_fraction_k(f, g, [i, imin, imax]): the finite continued fraction f(imin)/(g(imin) + f(imin + 1)/(g(imin + 1)
# + ...)) up to imax; continued_fraction_k(f, [i, imin, imax]): the same with every numerator 1; [i, imax] means
# [i, 1, imax]
# an infinite continued fraction with constant numerator f and denominator g: the limit of the convergents is the
# root of K^2 + g K - f = 0 of smaller modulus (the ratio of the minimal solution of the recurrence; equal moduli
# do not converge); other infinite fractions are not evaluated
fn cfk_fix(f, g) = {
  let d = sqrt(g^2 + 4*f)
  let r1 = simplify((d - g)/2)
  let r2 = simplify((-d - g)/2)
  let a1 = n(abs(r1), 30)
  let a2 = n(abs(r2), 30)
  if a1 < a2 then r1 else if a2 < a1 then r2 else failure(divergent)
}
fn cfk_inf(f, g, i) = if free_of(f, i) and free_of(g, i) and is_numeric(f) and is_numeric(g) then cfk_fix(f, g) else failure(infinite_continued_fraction)
fn continued_fraction_k(held f, held g, held sp) = {
  let s = cfk_spec(quote(sp)[1])
  if s[3] === inf then cfk_inf(unquote(quote(f)), unquote(quote(g)), s[1])
  else cfk_value(cfk_terms(unquote(quote(f)), s), cfk_terms(unquote(quote(g)), s))
}
fn continued_fraction_k(held g, held sp) = {
  let s = cfk_spec(quote(sp)[1])
  if s[3] === inf then cfk_inf(1, unquote(quote(g)), s[1])
  else {
    let gs = cfk_terms(unquote(quote(g)), s)
    cfk_value(constant_array(1, length(gs)), gs)
  }
}

# --- Clebsch-Gordan coefficients and 3j, 6j symbols, Wigner functions ---

# the angular momentum helpers: am_int(x) holds when x is a non-negative integer, am_ok(x) when 2x is an integer
fn am_int(x) = is_integer(x) and x >= 0
fn am_hint(x) = is_integer(2*x)
fn am_fact(x) = if is_integer(x) and x >= 0 then factorial(x) else null
# am_tri(a, b, c): whether a, b, c satisfy the triangle condition with integer perimeter a + b + c
fn am_tri(a, b, c) = am_int(a + b - c) and am_int(a - b + c) and am_int(-a + b + c)
fn am_delta(a, b, c) = sqrt(factorial(a + b - c)*factorial(a - b + c)*factorial(-a + b + c)/factorial(a + b + c + 1))

# clebsch_gordan([j1, m1], [j2, m2], [j, m]): the Clebsch-Gordan coefficient of j1, j2 coupled to j, by Racah's
# sum; zero outside the selection rules; exact (square roots of rationals)
fn cg_value(j1, m1, j2, m2, j, m) = {
  let lo = max(0, j2 - j - m1, j1 - j + m2)
  let hi = min(j1 + j2 - j, j1 - m1, j2 + m2)
  let s = sum(map(k => (-1)^k/(factorial(k)*factorial(j1 + j2 - j - k)*factorial(j1 - m1 - k)*factorial(j2 + m2 - k)*factorial(j - j2 + m1 + k)*factorial(j - j1 - m2 + k)), range(lo, hi)))
  sqrt((2*j + 1)*factorial(j + j1 - j2)*factorial(j - j1 + j2)*factorial(j1 + j2 - j)/factorial(j1 + j2 + j + 1))*sqrt(factorial(j + m)*factorial(j - m)*factorial(j1 - m1)*factorial(j1 + m1)*factorial(j2 - m2)*factorial(j2 + m2))*s
}
fn am_ready(j1, m1, j2, m2, j, m) = is_numeric(j1) and is_numeric(m1) and is_numeric(j2) and is_numeric(m2) and is_numeric(j) and is_numeric(m) and is_rational(j1) and is_rational(m1) and is_rational(j2) and is_rational(m2) and is_rational(j) and is_rational(m)
fn am_valid(j1, m1, j2, m2, j, m) = am_ready(j1, m1, j2, m2, j, m) and am_hint(j1) and am_hint(j2) and am_hint(j) and am_hint(m1) and am_hint(m2) and am_hint(m)
fn am_zero(j1, m1, j2, m2, j, m) = not (m == m1 + m2 and am_tri(j1, j2, j) and am_int(j1 - m1) and am_int(j1 + m1) and am_int(j2 - m2) and am_int(j2 + m2) and am_int(j - m) and am_int(j + m))
rule clebsch_gordan([j1_, m1_], [j2_, m2_], [j_, m_]) := { if am_zero(j1, m1, j2, m2, j, m) then 0 else cg_value(j1, m1, j2, m2, j, m) } when am_valid(j1, m1, j2, m2, j, m)

# three_j_symbol([j1, m1], [j2, m2], [j3, m3]): Wigner's 3j symbol, (-1)^(j1 - j2 - m3)/sqrt(2 j3 + 1) times the
# Clebsch-Gordan coefficient of j1, j2 to j3 with -m3
rule three_j_symbol([j1_, m1_], [j2_, m2_], [j3_, m3_]) := { if am_zero(j1, m1, j2, m2, j3, -m3) then 0 else (-1)^(j1 - j2 - m3)/sqrt(2*j3 + 1)*cg_value(j1, m1, j2, m2, j3, -m3) } when am_valid(j1, m1, j2, m2, j3, -m3)

# six_j_symbol([j1, j2, j3], [j4, j5, j6]): Wigner's 6j symbol by Racah's formula; zero unless the four triples
# (j1 j2 j3), (j1 j5 j6), (j4 j2 j6), (j4 j5 j3) satisfy the triangle conditions
rule six_j_symbol([j1_, j2_, j3_], [j4_, j5_, j6_]) := {
  if not (am_tri(j1, j2, j3) and am_tri(j1, j5, j6) and am_tri(j4, j2, j6) and am_tri(j4, j5, j3)) then 0 else {
    let lo = max(j1 + j2 + j3, j1 + j5 + j6, j4 + j2 + j6, j4 + j5 + j3)
    let hi = min(j1 + j2 + j4 + j5, j2 + j3 + j5 + j6, j3 + j1 + j6 + j4)
    am_delta(j1, j2, j3)*am_delta(j1, j5, j6)*am_delta(j4, j2, j6)*am_delta(j4, j5, j3)*sum(map(t => (-1)^t*factorial(t + 1)/(factorial(t - j1 - j2 - j3)*factorial(t - j1 - j5 - j6)*factorial(t - j4 - j2 - j6)*factorial(t - j4 - j5 - j3)*factorial(j1 + j2 + j4 + j5 - t)*factorial(j2 + j3 + j5 + j6 - t)*factorial(j3 + j1 + j6 + j4 - t)), range(lo, hi)))
  }
} when is_rational(j1) and is_rational(j2) and is_rational(j3) and is_rational(j4) and is_rational(j5) and is_rational(j6) and am_hint(j1) and am_hint(j2) and am_hint(j3) and am_hint(j4) and am_hint(j5) and am_hint(j6)

# wigner_d([j, m1, m2], theta): the Wigner small d function d^j_(m2, m1)(theta) (Sakurai's convention, from the
# explicit sum over s of Wigner's formula); wigner_d([j, m1, m2], theta, phi): the same times exp(i m2 phi);
# wigner_d([j, m1, m2], psi, theta, phi): the Wigner D function exp(i m1 psi + i m2 phi) d^j_(m2, m1)(theta)
fn wd_small(j, mp, m, th) = {
  let lo = max(0, m - mp)
  let hi = min(j + m, j - mp)
  sqrt(factorial(j + mp)*factorial(j - mp)*factorial(j + m)*factorial(j - m))*sum(map(s => (-1)^(mp - m + s)/(factorial(j + m - s)*factorial(s)*factorial(mp - m + s)*factorial(j - mp - s))*cos(th/2)^(2*j + m - mp - 2*s)*sin(th/2)^(mp - m + 2*s), range(lo, hi)))
}
fn wd_ok(j, m1, m2) = is_rational(j) and is_rational(m1) and is_rational(m2) and am_hint(j) and am_hint(m1) and am_hint(m2) and j >= 0 and am_int(j - m1) and am_int(j + m1) and am_int(j - m2) and am_int(j + m2)
rule wigner_d([j_, m1_, m2_], th_) := wd_small(j, m2, m1, th) when wd_ok(j, m1, m2)
rule wigner_d([j_, m1_, m2_], th_, ph_) := exp(sqrt(-1)*m2*ph)*wd_small(j, m2, m1, th) when wd_ok(j, m1, m2)
rule wigner_d([j_, m1_, m2_], ps_, th_, ph_) := exp(sqrt(-1)*(m1*ps + m2*ph))*wd_small(j, m2, m1, th) when wd_ok(j, m1, m2)

# --- expressions: sides of relations, numerators and denominators, polynomial helpers ---

# apply_sides(f, r): the relation r (==, !=, <, <=, >, >=) with f applied to both sides; f is not simplified away
# (apply_sides(exp, log(x) == 2) is exp(log(x)) == exp(2) as the evaluator leaves it); lists thread
fn rel_head(r) = not is_atom(r) and length(r) == 2 and (head(r) === equal or head(r) === unequal or head(r) === less or head(r) === greater or head(r) === less_equal or head(r) === greater_equal)
rule apply_sides(f_, r_) := apply(head(r), [f(r[1]), f(r[2])]) when rel_head(r)
rule apply_sides(f_, rs_) := map(r => apply_sides(f, r), rs) when is_list(rs)
# through and, or, piecewise and conditional_expression (calc_side_map), to every side of a chain; true and false stay
rule apply_sides(f_, r_) := calc_side_map(r, q => calc_sides(q, f)) when not is_list(r) and not rel_head(r) and calc_side_arg(r)

# numerator_denominator(e): [numerator(e), denominator(e)]
rule numerator_denominator(e_) := [numerator(e), denominator(e)]

# rational_expression_q(e, x), (e, [x, y, ...]), (e): whether e is a quotient of polynomials in the variables (every
# variable of e when none is given)
fn rx_poly(e, vs) = if every(vs, v => free_of(e, v)) then true else if member(vs, e) then true else if is_atom(e) then false
  else if head(e) === add or head(e) === mul then every(map(i => rx_poly(e[i], vs), range(length(e))), x => x === true)
  else if head(e) === pow then (is_integer(e[2]) and e[2] >= 0 and rx_poly(e[1], vs))
  else false
fn rx_vars(v) = if is_list(v) then v else [v]
rule rational_expression_q(e_) := rx_poly(numerator(together(e)), variables(e, kernels: true)) and rx_poly(denominator(together(e)), variables(e, kernels: true))
rule rational_expression_q(e_, v_) := rx_poly(numerator(together(e)), rx_vars(v)) and rx_poly(denominator(together(e)), rx_vars(v))

# the walkers of expand_numerator, expand_denominator and expand_all
fn ex_parts(e) = map(i => e[i], range(length(e)))
fn ex_numer(e) = if is_atom(e) then e
  else if head(e) === add then apply(add, map(ex_numer, ex_parts(e)))
  else if (head(e) === mul or head(e) === pow) and denominator(e) !== 1 then expand(numerator(e))/denominator(e)
  else apply(head(e), map(ex_numer, ex_parts(e)))
fn ex_denom(e) = if is_atom(e) then e
  else if head(e) === add then apply(add, map(ex_denom, ex_parts(e)))
  else if (head(e) === mul or head(e) === pow) and denominator(e) !== 1 then numerator(e)/expand(denominator(e))
  else apply(head(e), map(ex_denom, ex_parts(e)))
fn ex_all(e) = if is_atom(e) then e else {
  let ps = map(ex_all, ex_parts(e))
  if head(e) === pow and is_integer(ps[2]) and ps[2] < 0 then expand(ps[1])^ps[2]
  else if head(e) === add or head(e) === mul or head(e) === pow then expand(apply(head(e), ps))
  else apply(head(e), ps)
}
# expand_numerator(e): the numerators of all fractions in e expanded; expand_denominator(e): the denominators;
# expand_all(e): everything expanded, inside function arguments too, numerators and denominators
rule expand_numerator(e_) := ex_numer(e)
rule expand_denominator(e_) := ex_denom(e)
rule expand_all(e_) := ex_all(e)

# expand(e, patt): e expanded with every part free of patt kept as it is: in the tree of sums, products and powers
# a subterm free of patt is a leaf, replaced by an opaque kernel ex_leaf(i) for the expansion and put back after
fn ex_leaves(e, p) = if is_atom(e) then []
  else if free_of(e, p) then [e]
  else if head(e) === add or head(e) === mul or head(e) === pow then flatten(map(t => ex_leaves(t, p), ex_parts(e)), 1)
  else [e]
rule expand(e_, p_) := {
  let ls = union(ex_leaves(e, p))
  let fwd = map(i => ls[i] -> ex_leaf(i), range(length(ls)))
  let back = map(i => ex_leaf(i) -> ls[i], range(length(ls)))
  rewrite(expand(rewrite(e, fwd)), back)
} when not is_list(e) and not (not is_atom(p) and head(p) === options)

# --- the modulus: m option of the expansion and fraction functions ---
# Over Z/m (m > 1, prime or not) a polynomial is expanded over the integers and its coefficients reduced to 0..m-1
# (reduction mod m is a ring map, so expanding first is expanding over Z/m; a rational coefficient needs its
# denominator invertible mod m). A fraction is its numerator and denominator reduced, then multiplied by the inverse
# of the denominator's leading coefficient (lex in the variables' canonical order) when that is a unit, so the
# denominator leads with 1.
fn md_only(o) = not is_atom(o) and head(o) === options and length(o) == 1 and o[1][1] === modulus and is_integer(o[1][2]) and o[1][2] > 1
fn md_m(o) = o[1][2]
# every rational number of e has a denominator that is a unit mod m
fn md_rats(e, m) = if is_atom(e) then (not is_rational(e) or is_integer(e) or gcd(denominator(e), m) == 1) else every(ex_parts(e), t => md_rats(t, m))
fn md_opt(e, o) = md_only(o) and md_rats(e, md_m(o))
fn md_red(p, m) = polynomial_mod(expand(p), m)
fn md_lc(d) = if is_number(d) then d else first(coefficient_rules(d, variables(d)))[2]
fn md_frac(n, d, m) = {
  let n1 = md_red(n, m)
  let d1 = md_red(d, m)
  let c = md_lc(d1)
  if is_integer(c) and c != 0 and c != 1 and gcd(c, m) == 1 then {
    let u = power_mod(c, -1, m)
    [md_red(n1*u, m), md_red(d1*u, m)]
  } else [n1, d1]
}
# a denominator that vanishes mod m (or a coefficient whose denominator is not a unit) leaves the call as it is
fn md_ints(p) = every(calc_terms(expand(p)), t => is_integer(calc_num_part(t)))
fn md_ok(e, m) = {
  let d = md_red(denominator(e), m)
  d !== 0 and md_ints(d) and md_ints(md_red(numerator(e), m))
}
fn ex_numer_m(e, m) = if is_atom(e) then e
  else if head(e) === add then apply(add, map(t => ex_numer_m(t, m), ex_parts(e)))
  else if (head(e) === mul or head(e) === pow) and denominator(e) !== 1 then md_red(numerator(e), m)/denominator(e)
  else if head(e) === mul or head(e) === pow then md_red(e, m)
  else apply(head(e), map(t => ex_numer_m(t, m), ex_parts(e)))
fn ex_denom_m(e, m) = if is_atom(e) then e
  else if head(e) === add then apply(add, map(t => ex_denom_m(t, m), ex_parts(e)))
  else if (head(e) === mul or head(e) === pow) and denominator(e) !== 1 then numerator(e)/md_red(denominator(e), m)
  else apply(head(e), map(t => ex_denom_m(t, m), ex_parts(e)))
fn ex_all_m(e, m) = if is_atom(e) then (if is_integer(e) then mod(e, m) else e) else {
  let ps = map(t => ex_all_m(t, m), ex_parts(e))
  if head(e) === pow and is_integer(e[2]) and e[2] < 0 then md_red(ps[1]^(-e[2]), m)^(-1)
  else if head(e) === add or head(e) === mul or head(e) === pow then md_red(apply(head(e), ps), m)
  else apply(head(e), ps)
}
rule expand_numerator(e_, o_) := ex_numer_m(e, md_m(o)) when md_opt(e, o)
rule expand_denominator(e_, o_) := ex_denom_m(e, md_m(o)) when md_opt(e, o)
rule expand_all(e_, o_) := ex_all_m(e, md_m(o)) when md_opt(e, o)
rule numerator(e_, o_) := md_frac(numerator(e), denominator(e), md_m(o))[1] when md_opt(e, o) and md_ok(e, md_m(o))
rule denominator(e_, o_) := md_frac(numerator(e), denominator(e), md_m(o))[2] when md_opt(e, o) and md_ok(e, md_m(o))
rule numerator_denominator(e_, o_) := md_frac(numerator(e), denominator(e), md_m(o)) when md_opt(e, o) and md_ok(e, md_m(o))
rule coefficient_rules(p_, v_, o_) := coefficient_rules(md_red(p, md_m(o)), v) when md_opt(p, o) and denominator(p) === 1
rule monomial_list(p_, v_, o_) := monomial_list(md_red(p, md_m(o)), v) when md_opt(p, o) and denominator(p) === 1
rule polynomial_quotient_remainder(p_, q_, x_, o_) := [polynomial_quotient(p, q, x, o), polynomial_remainder(p, q, x, o)] when md_only(o)
# the extended Euclidean algorithm in F_p[x] (p prime, one variable): the degree of r1 falls at every step, so at
# most degree(q) + 1 steps
fn md_univariate(p, q, x) = every(variables(p*q), v => v === x) and denominator(together(p)) === 1 and denominator(together(q)) === 1
rule polynomial_extended_gcd(p_, q_, x_, o_) := {
  let m = md_m(o)
  var r0 = md_red(p, m)
  var r1 = md_red(q, m)
  var s0 = 1
  var s1 = 0
  var t0 = 0
  var t1 = 1
  while r1 !== 0 {
    let qq = polynomial_quotient(r0, r1, x, o)
    let r2 = md_red(r0 - qq*r1, m)
    let s2 = md_red(s0 - qq*s1, m)
    let t2 = md_red(t0 - qq*t1, m)
    r0 = r1
    r1 = r2
    s0 = s1
    s1 = s2
    t0 = t1
    t1 = t2
  }
  if r0 === 0 then [0, [0, 0]] else {
    let u = power_mod(coefficient(r0, x, exponent(r0, x)), -1, m)
    [md_red(r0*u, m), [md_red(s0*u, m), md_red(t0*u, m)]]
  }
} when md_only(o) and is_prime(md_m(o)) and md_univariate(p, q, x)

# factor_square_free_list(p): [[c, 1], [f1, k1], ...], p = c f1^k1 ..., the factors square free and relatively prime,
# in order of increasing multiplicity
fn fsf_split(f) = if not is_atom(f) and head(f) === pow and is_integer(f[2]) then [f[1], f[2]] else [f, 1]
# a factor in one variable with a non-real coefficient (over the Gaussian rationals) is made monic, its leading
# coefficient going into the constant
fn fsf_monic(fe) = {
  let vs = variables(fe[1])
  if length(vs) == 1 and some(coefficient_list(fe[1], vs[1]), t => is_number(t) and not (im_part(t) === 0)) then {
    let lc = coefficient(fe[1], vs[1], exponent(fe[1], vs[1]))
    [lc^fe[2], [expand(fe[1]/lc), fe[2]]]
  } else [1, fe]
}
rule factor_square_free_list(p_) := fsf_list_of(factor_square_free(p), p)
# modulus: p (a prime, one variable): the factors of factor_square_free over F_p (no answer when it has none)
rule factor_square_free_list(p_, o_) := fsf_list_of(factor_square_free(p, o), p) when md_opt(p, o) and is_prime(md_m(o)) and fsf_done(factor_square_free(p, o))
# (the factors by degree, then in canonical order)
rule factor_list(p_, o_) := {
  let l = fsf_list_of(factor(p, o), p)
  let vs = variables(p)
  join([l[1]], sort_by(rest(l), q => [if length(vs) == 1 then exponent(q[1], vs[1]) else 0, q[1], q[2]]))
} when md_opt(p, o) and is_prime(md_m(o)) and fsf_done(factor(p, o))
# gaussian_integers: true: the factors of factor over the Gaussian rationals (a polynomial, one variable)
rule factor_list(p_, o_) := {
  let f = factor(p, o)
  let ps = map(fsf_split, if not is_atom(f) and head(f) === mul then apply(list, f) else [f])
  let cs = select(ps, q => is_number(q[1]))
  # each factor with a non-real coefficient made monic, its leading coefficient going into the constant
  let others = map(fsf_monic, select(ps, q => not is_number(q[1])))
  join([[expand(product(map(q => q[1]^q[2], cs))*product(map(first, others))), 1]], sort(map(last, others)))
} when gi_opt(o) and denominator(together(p)) === 1 and length(variables(p)) == 1 and fsf_done(factor(p, o))
fn fsf_done(f) = is_atom(f) or not (head(f) === factor or head(f) === factor_square_free or head(f) === failure)
fn fsf_list_of(f, p) = {
  let ps = if not is_atom(f) and head(f) === mul then map(i => f[i], range(length(f))) else [f]
  let consts = select(ps, fsf_constant)
  let others = map(fe => fsf_monic(fsf_split(fe)), select(ps, q => not fsf_constant(q)))
  let c = (if length(consts) == 0 then 1 else product(consts))*product(map(first, others))
  join([[if is_number(c) then c*fsf_field_one(p) else c, 1]], sort_by(map(last, others), q => q[2]))
}
# over a finite field the constant is a field element
fn fsf_constant(q) = is_number(q) or (not is_atom(q) and head(q) === finite_field_element)
fn fsf_field_one(p) = {
  let es = cases([p], finite_field_element(_, _), inf)
  if length(es) == 0 then 1 else es[1]/es[1]
}

# factor_terms_list(p), (p, x), (p, [x, y, ...]): p as a list of factors: the integer content, the factor free of the
# variables (when they are given; the gcd of the coefficients of the variables) and the primitive polynomial
# (p, [x1, ..., xn]): after the factor free of all the xi come the factors free of x1, ..., x(n-1), then of x1, ...,
# x(n-2), and so on, and the rest last. Gaussian integer coefficients have a Gaussian content, its unit chosen so
# that the leading coefficient of the rest lies in the first quadrant (5 i x^2 + 10 is 5 i (x^2 - 2 i)).
# modulus: m reduces the coefficients and pulls out the leading coefficient when it is invertible modulo m;
# trig: true reduces the trigonometric products to multiple angles first.
fn ftl_coeffs(p, vs) = flatten(coefficient_list(expand(p), vs))
fn ftl_gauss_int(z) = is_integer(re(z)) and is_integer(im_part(z))
fn ftl_gauss_round(z) = round(re(z)) + im*round(im_part(z))
fn ftl_gauss_gcd(a, b) = if b === 0 then a else ftl_gauss_gcd(b, expand(a - b*ftl_gauss_round(a/b)))
fn ftl_unit(z) = if re(z) > 0 and im_part(z) >= 0 then 1 else if im_part(z) > 0 and re(z) <= 0 then im else if re(z) < 0 and im_part(z) <= 0 then -1 else -im
fn ftl_content(q, cs) =
  if every(cs, is_integer) then apply(gcd, cs)
  else if every(cs, ftl_gauss_int) then {
    let g = fold(ftl_gauss_gcd, 0, cs)
    let vs = variables(q, kernels: true)
    let lc = if length(vs) == 0 then q else first(select(reverse(cs), t => not (t === 0)))
    g*ftl_unit(lc/g)
  } else 1
rule factor_terms_list(p_) := {
  let q = expand(p)
  let vs = variables(q, kernels: true)
  let cs = if length(vs) == 0 then [q] else ftl_coeffs(q, vs)
  let c = ftl_content(q, cs)
  [c, expand(q/c)]
} when is_polynomial_expr(p)
rule factor_terms_list(p_, v_) := {
  let q = expand(p)
  let cs = flatten(coefficient_list(q, variables(q, kernels: true)))
  let c = ftl_content(q, cs)
  var r = expand(q/c)
  let vs = rx_vars(v)
  var out = [c]
  for j in range(length(vs), 1, -1) {
    let g = last(fold_list(polynomial_gcd, flatten(coefficient_list(r, take(vs, j)))))
    out = append(out, g)
    r = expand(cancel(r/g))
  }
  append(out, r)
} when is_polynomial_expr(p) and not (not is_atom(v) and head(v) === options)
rule factor_terms_list(p_, o_) := ftl_options(p, o) when not is_atom(o) and head(o) === options
fn ftl_options(p; modulus = 0, trig = false) =
  if trig === true then factor_terms_list(trig_reduce(p))
  else if is_integer(modulus) and modulus != 0 then {
    let q = polynomial_mod(expand(p), modulus)
    let vs = variables(q)
    let lc = if length(vs) == 0 then q else coefficient(q, vs[1], exponent(q, vs[1]))
    if is_integer(lc) and lc != 0 and gcd(lc, modulus) == 1 then [lc, polynomial_mod(expand(q*power_mod(lc, -1, modulus)), modulus)] else [1, q]
  } else factor_terms_list(p)
fn is_polynomial_expr(p) = rx_poly(p, variables(p, kernels: true))

# polynomial_quotient_remainder(p, q, x): [polynomial_quotient(p, q, x), polynomial_remainder(p, q, x)]
rule polynomial_quotient_remainder(p_, q_, x_) := [polynomial_quotient(p, q, x), polynomial_remainder(p, q, x)]

# polynomial_extended_gcd(p, q, x): [g, [s, t]] with g the monic gcd and g = s p + t q (by the extended Euclidean
# algorithm over the rational functions of the other variables)
rule polynomial_extended_gcd(p_, q_, x_) := {
  var r0 = p
  var r1 = q
  var s0 = 1
  var s1 = 0
  var t0 = 0
  var t1 = 1
  var steps = 0
  # cancel keeps the coefficients (rational functions of the other variables) in lowest terms, so that a zero
  # remainder is exactly 0 and the degree falls at each step; the step count only guards against a stall
  while r1 !== 0 and steps < 200 {
    steps = steps + 1
    let qq = polynomial_quotient(r0, r1, x)
    let r2 = expand(cancel(r0 - qq*r1))
    let s2 = expand(cancel(s0 - qq*s1))
    let t2 = expand(cancel(t0 - qq*t1))
    r0 = r1
    r1 = r2
    s0 = s1
    s1 = s2
    t0 = t1
    t1 = t2
  }
  let lc = last(coefficient_list(expand(r0), x))
  [cancel(r0/lc), [cancel(s0/lc), cancel(t0/lc)]]
} when not is_list(p) and not is_list(q)

# horner_form(p), (p, x), (p, [x, y, ...]): the nested form of the polynomial p, in the variables in turn
fn horner1(cs, x) = {
  # cs: coefficients, lowest degree first
  let nz = select(range(length(cs)), i => cs[i] !== 0)
  if length(nz) == 0 then 0 else {
    let first = nz[1]
    let rest_nz = rest(nz)
    if length(rest_nz) == 0 then (if first == 1 then cs[1] else cs[first]*x^(first - 1))
    else {
      let gap = rest_nz[1] - first
      let inner = horner1(drop(cs, rest_nz[1] - 1), x)
      let tail = if gap == 1 then x*inner else x^gap*inner
      if first == 1 then cs[1] + tail else if first == 2 then x*(cs[first] + tail) else x^(first - 1)*(cs[first] + tail)
    }
  }
}
fn horner_vars(p, vs) = if length(vs) == 0 then p else {
  let x = vs[1]
  let cs = coefficient_list(expand(p), x)
  horner1(map(c => horner_vars(c, rest(vs)), cs), x)
}
rule horner_form(p_) := horner_form(p, variables(p, kernels: true)) when not is_list(p)
rule horner_form(p_, v_) := {
  let t = together(p)
  let vs = rx_vars(v)
  if denominator(t) === 1 then horner_vars(p, vs) else horner_vars(numerator(t), vs)/horner_vars(denominator(t), vs)
} when not is_list(p)

# coefficient_rules(p, [x, y, ...]): the rules {exponents} -> coefficient of the monomials of the polynomial p, in
# decreasing lexicographic order; monomial_list(p, vars): the terms (coefficient times monomial) in the same order
fn cr_expand(cs, depth, prefix) = if depth == 0 then (if cs === 0 then [] else [[prefix, cs]])
  else flat_map(i => cr_expand(cs[i], depth - 1, append(prefix, i - 1)), range(length(cs)))
fn cr_list(p, vs) = reverse(sort_by(cr_expand(coefficient_list(expand(p), vs), length(vs), []), t => t[1]))
rule coefficient_rules(p_, v_) := map(t => t[1] -> t[2], cr_list(p, rx_vars(v)))
rule coefficient_rules(p_) := coefficient_rules(p, variables(p, kernels: true))
rule monomial_list(p_, v_) := map(t => t[2]*product(map(i => rx_vars(v)[i]^t[1][i], range(length(rx_vars(v))))), cr_list(p, rx_vars(v)))
rule monomial_list(p_) := monomial_list(p, variables(p, kernels: true))

# symmetric_polynomial(k, [x1, ..., xn]): the elementary symmetric polynomial of degree k, the sum of the products of
# the k-element subsets
rule symmetric_polynomial(k_, xs_) := sum(map(s => product(s), select(subsets(xs), s => length(s) == k))) when is_integer(k) and k >= 0 and is_list(xs)

# residue(f, [x, a]): the coefficient of 1/(x - a) in the Laurent expansion of f around a (poles up to order 7)
# at an algebraic irrational point (a root object) the constant part of every argument is reduced first, so that a
# pole shows: gamma(u(x)) at a root of u(x) = -1 has the argument -1 + (...) t
fn rsd_num(e) = { let r = root_reduce(e); if not is_atom(r) and head(r) === root_reduce then e else r }
fn rsd_reduce(e) =
  if is_atom(e) then e
  else if is_numeric(e) then rsd_num(e)
  else {
    let e2 = if head(e) === add or head(e) === pow or head(e) === mul then expand(e) else e
    if is_atom(e2) then e2
    else if head(e2) === add then {
      let ts = apply(list, e2)
      rsd_num(total(select(ts, is_numeric))) + total(map(rsd_reduce, select(ts, u => not is_numeric(u))))
    } else apply(head(e2), map(rsd_reduce, apply(list, e2)))
  }
rule residue(f_, [x_, a_]) := {
  let t = rsd_t
  let g = rewrite(f, x -> a + t)
  coefficient(normal(series(if is_numeric(a) and not is_rational(a) then rsd_reduce(g) else g, t, 0, 7)), t, -1)
} when not is_list(f)

# --- the Gaussian integers: gaussian_integers: true of the number-theory functions ---
# A nonzero Gaussian integer z factors from its norm N = a^2 + b^2 (Hardy and Wright, An Introduction to the Theory of
# Numbers, 12.8; Cohen, A Course in Computational Algebraic Number Theory, 1.5.1): over a rational prime p | N lie
# 1 + i (p = 2, ramified), p itself (p = 3 mod 4, inert) or the two conjugate primes gcd(p, t + i) and its conjugate
# (p = 1 mod 4, split), t a square root of -1 mod p (t = c^((p - 1)/4) for a quadratic non-residue c). The exponent
# of each prime is found by exact division; what is left is a unit. Primes are taken in the first quadrant (re > 0,
# im >= 0) and listed by norm, then by real part.
fn gi_int(z) = is_number(z) and is_integer(re(z)) and is_integer(im_part(z))
fn gi_normsq(z) = re(z)^2 + im_part(z)^2
fn gi_assoc(z) = first(select([z, im*z, -z, -im*z], w => re(w) > 0 and im_part(w) >= 0))
fn gi_sqrt_m1(p) = {
  var c = 2
  while power_mod(c, (p - 1)/2, p) != p - 1 {
    c = c + 1
  }
  power_mod(c, (p - 1)/4, p)
}
fn gi_primes_over(p) =
  if p == 2 then [1 + im]
  else if mod(p, 4) == 3 then [p]
  else {
    let g = gi_assoc(ftl_gauss_gcd(p, gi_sqrt_m1(p) + im))
    [g, gi_assoc(conjugate(g))]
  }
# [unit, [[prime, exponent], ...]] of a nonzero Gaussian integer
fn gi_factor(z) = {
  var w = z
  var out = []
  for pe in factor_integer(gi_normsq(z)) {
    if pe[1] > 1 then {
      for q in gi_primes_over(pe[1]) {
        var k = 0
        while divisible(w, q) {
          w = expand(w/q)
          k = k + 1
        }
        if k > 0 then out = append(out, [q, k])
      }
    }
  }
  [w, sort_by(out, f => [gi_normsq(f[1]), re(f[1])])]
}
fn gi_opt(o) = not is_atom(o) and head(o) === options and length(o) == 1 and o[1][1] === gaussian_integers and o[1][2] === true
fn gi_arg(n) = gi_int(n) and n !== 0
fn gi_divisors(fs) = fold((ds, f) => flatten(map(d => map(k => d*f[1]^k, range(0, f[2])), ds), 1), [1], fs)
rule factor_integer(n_, o_) := { let f = gi_factor(n); if f[1] === 1 and length(f[2]) > 0 then f[2] else join([[f[1], 1]], f[2]) } when gi_opt(o) and gi_arg(n)
rule divisors(n_, o_) := sort_by(union(map(d => gi_assoc(expand(d)), gi_divisors(gi_factor(n)[2]))), d => [gi_normsq(d), re(d)]) when gi_opt(o) and gi_arg(n)
# sigma_k is multiplicative over the first-quadrant primes: the product of 1 + q^k + ... + q^(e k)
rule divisor_sigma(k_, n_, o_) := expand(product(map(f => total(map(j => f[1]^(j*k), range(0, f[2]))), gi_factor(n)[2]))) when gi_opt(o) and gi_arg(n) and is_integer(k)
rule prime_nu(n_, o_) := length(gi_factor(n)[2]) when gi_opt(o) and gi_arg(n)
rule prime_omega(n_, o_) := total(map(last, gi_factor(n)[2])) when gi_opt(o) and gi_arg(n)
rule liouville_lambda(n_, o_) := (-1)^total(map(last, gi_factor(n)[2])) when gi_opt(o) and gi_arg(n)
rule moebius_mu(n_, o_) := { let fs = gi_factor(n)[2]; if some(fs, f => f[2] > 1) then 0 else (-1)^length(fs) } when gi_opt(o) and gi_arg(n)
rule is_composite(n_, o_) := total(map(last, gi_factor(n)[2])) >= 2 when gi_opt(o) and gi_arg(n)
rule is_composite(ns_, o_) := map(n => is_composite(n, o), ns) when gi_opt(o) and is_list(ns)
rule is_prime_power(n_, o_) := length(gi_factor(n)[2]) == 1 when gi_opt(o) and gi_arg(n)
# --- mixed radix digits ---
# integer_digits(n, mixed_radix([b1, ..., bm])): the digits of n >= 0 in the radices b1 (most significant) to bm,
# taken from the last; b1 may be inf (no bound); what is left above the radices is one more leading digit, so the
# digits always give n back (n = d0 + bm (d1 + b(m-1) (d2 + ...)))
fn mr_digits(n, bs) = {
  var q = n
  var out = []
  for b in reverse(bs) {
    if b === inf then {
      out = prepend(out, q)
      q = 0
    } else {
      out = prepend(out, mod(q, b))
      q = quotient(q, b)
    }
  }
  if q > 0 then prepend(out, q) else out
}
fn mr_ok(n, r) = is_number(n) and is_integer(n) and n >= 0 and not is_atom(r) and head(r) === mixed_radix and length(r) == 1 and is_list(r[1]) and every(r[1], b => b === inf or (is_integer(b) and b > 1 or b === 1))
rule integer_digits(n_, r_) := mr_digits(n, r[1]) when mr_ok(n, r)
rule integer_length(n_, r_) := length(mr_digits(n, r[1])) when mr_ok(n, r)
# integer_string(n, mixed_radix(bs)), (n, mixed_radix(bs), len): the digits as characters 0-9, a-z (radices up to
# 36), the last len of them (padded with 0)
fn mr_char(d) = if d < 10 then 48 + d else 87 + d
fn mr_pad(ds, len) = if length(ds) >= len then take(ds, -len) else join(constant_array(0, len - length(ds)), ds)
rule integer_string(n_, r_) := from_characters(map(mr_char, mr_digits(n, r[1]))) when mr_ok(n, r) and every(mr_digits(n, r[1]), d => d < 36)
rule integer_string(n_, r_, len_) := from_characters(map(mr_char, mr_pad(mr_digits(n, r[1]), len))) when mr_ok(n, r) and is_integer(len) and len >= 0 and every(mr_digits(n, r[1]), d => d < 36)

# a Gaussian integer that is not real is factored in Z[i] without the option
fn gi_complex(z) = gi_int(z) and im_part(z) != 0
rule divisor_sigma(k_, z_) := divisor_sigma(k, z, gaussian_integers: true) when gi_complex(z) and is_integer(k)
rule liouville_lambda(z_) := liouville_lambda(z, gaussian_integers: true) when gi_complex(z)
rule moebius_mu(z_) := moebius_mu(z, gaussian_integers: true) when gi_complex(z)
# sigma_k(n) for a k that is not a natural number (rational, negative or symbolic): the product over p^e || n of
# (p^(k (e + 1)) - 1)/(p^k - 1), the geometric sums 1 + p^k + ... + p^(e k) (Hardy and Wright 16.7)
rule divisor_sigma(k_, n_) := { if n == 1 then 1 else product(map(f => (f[1]^(k*(f[2] + 1)) - 1)/(f[1]^k - 1), factor_integer(n))) } when is_number(n) and is_integer(n) and n >= 1 and (not is_integer(k) or k < 0) and (is_rational(k) or not is_number(k)) and not is_list(k)
# gaussian_integers: false is the plain function
rule liouville_lambda(n_, o_) := liouville_lambda(n) when not is_atom(o) and head(o) === options and length(o) == 1 and o[1][1] === gaussian_integers and o[1][2] === false

# --- Riemann's R function and Dirichlet L-functions ---

# riemann_r(x): Riemann's prime counting function R(x) = 1 + the sum over k >= 1 of (ln x)^k / (k k! zeta(k + 1))
# (Gram's series, DLMF 27.12 / Riesel and Goehl); evaluated for inexact real or complex x > 0 at the precision of x,
# riemann_r(1) = 1; lists thread
rule riemann_r(xs_) := map(riemann_r, xs) when is_list(xs)
rule riemann_r(1) := 1
rule riemann_r(x_) := {
  let p = precision(x)
  let l = log(x)
  var s = 1
  var pw = 1
  var k = 1
  var go = true
  while go {
    pw = pw*l/k
    let t = pw/(k*n(zeta(k + 1), ceiling(p) + 8))
    s = s + t
    if k > abs(l) and abs(t) < 10^(-p)*abs(s) then go = false
    k = k + 1
    if k > 20000 then go = false
  }
  s
} when is_inexact(x) and is_number(x) and abs(x) > 0

# dc_phase(q, e, a, j): the exponent r (the value exp(2 pi i r)) of the Conrey character modulo the prime power q = p^e
# with label j at a, or null when a shares a factor with q (the character is 0 there)
fn dc_dlog(g, a, q) = {
  var v = 1
  var t = 0
  while v != mod(a, q) and t < q {
    v = mod(v*g, q)
    t = t + 1
  }
  t
}
fn dc_phase(p: Int, e: Int, a: Int, j: Int) = {
  let q = p^e
  if mod(a, p) == 0 then null
  else if p == 2 and e == 1 then 0
  else if p == 2 then {
    # a = (-1)^a1 5^a2 modulo 2^e
    let am = mod(a, q)
    let jm = mod(j, q)
    let sg = if mod(am, 4) == 1 then 0 else 1
    let sj = if mod(jm, 4) == 1 then 0 else 1
    let a5 = if sg == 1 then q - am else am
    let j5 = if sj == 1 then q - jm else jm
    let l5 = if e > 2 then mod(dc_dlog(5, a5, q)*dc_dlog(5, j5, q), 2^(e - 2))/2^(e - 2) else 0
    mod(sg*sj/2 + l5, 1)
  } else {
    let g = primitive_root(q)
    let ph = euler_phi(q)
    mod(dc_dlog(g, a, q)*dc_dlog(g, j, q), ph)/ph
  }
}

# dirichlet_chi(k, j, a): the value of the Dirichlet character of modulus k and Conrey label j at the integer a, as a
# phase exp(2 pi i r) with r exact (zero when gcd(a, k) > 1)
fn dirichlet_exponent(k: Int, j: Int, a: Int) = {
  var r = 0
  var zero = false
  for f in factor_integer(k) {
    let c = dc_phase(f[1], f[2], a, j)
    if c === null then zero = true else r = r + c
  }
  if zero then null else mod(r, 1)
}

# dirichlet_l(k, j, s): the Dirichlet L-function of the character of modulus k and Conrey label j (gcd(j, k) = 1),
# k^(-s) times the sum over a of chi(a) zeta(s, a/k) (Hurwitz zeta values); k = 1 is the zeta function. Evaluated for
# inexact s at machine precision, symbolic otherwise; lists of s thread
rule dirichlet_l(k_, j_, ss_) := map(s => dirichlet_l(k, j, s), ss) when is_list(ss)
rule dirichlet_l(1, 1, s_) := zeta(s) when not is_list(s)
rule dirichlet_l(k_, j_, s_) := {
  var tot = 0
  for a in range(k) {
    let r = dirichlet_exponent(k, j, a)
    if r !== null then tot = tot + exp(2*pi*sqrt(-1)*r)*zeta(s, a/k)
  }
  tot*k^(-s)
} when is_integer(k) and k > 1 and is_integer(j) and gcd(j, k) == 1 and is_inexact(s) and is_number(s)
