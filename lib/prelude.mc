#! mathcas 2
# The prelude: small general-purpose functions written in MathCAS itself, evaluated when the engine starts.
# Everything here is in the bare namespace (the prelude is not a module). Rules: never define a symbol that has a
# native implementation, and never reuse a name the benchmark cases define.

# is_even(n), is_odd(n): parity of an integer
fn is_even(n: Int) = mod(n, 2) == 0
fn is_odd(n: Int) = mod(n, 2) == 1

# clamp(x, lo, hi): x limited to the interval [lo, hi]
fn clamp(x, lo, hi) = min(max(x, lo), hi)

# log_base(b, x): the logarithm of x to base b (an exact integer when x is a power of the integer b)
fn log_base(b: Int, x: Int) when b > 1 and x > 0 and b^integer_exponent(x, b) == x = integer_exponent(x, b)
# numbers of which one is a machine number: the quotient of the logarithms of their exact values to 40 digits,
# rounded once to a machine number (so log_base(2.0, 8.0) is 3.0, not the quotient of two rounded logarithms)
fn log_base(b, x) when is_number(b) and is_number(x) and (is_machine_number(b) or is_machine_number(x)) = n(n(log(set_precision(x, inf))/log(set_precision(b, inf)), 40))
fn log_base(b, x) = log(x)/log(b)

# count_if(xs, p): how many elements x of xs have p(x) true
fn count_if(xs, p) = length(select(xs, p))

# every(xs, p), some(xs, p): whether p(x) is true for all elements, for at least one element
fn every(xs, p) = count_if(xs, p) == length(xs)
fn some(xs, p) = count_if(xs, p) > 0

# member(xs, x): whether x is an element of xs (compared by identity)
fn member(xs, x) = some(xs, y => y === x)

# zip(xs, ys): the list of pairs [xs[i], ys[i]], as long as the shorter list
fn zip(xs, ys) = range(min(length(xs), length(ys))) |> map(i => [xs[i], ys[i]], _)

# flat_map(f, xs): the lists f(x) for each element x, joined into one list
fn flat_map(f, xs) = flatten(map(f, xs), 1)

# accumulate(xs): the running sums [xs[1], xs[1] + xs[2], ...]
fn accumulate(xs) = if length(xs) == 0 then [] else fold_list(add, xs)

# fix_point(f, x): f applied repeatedly from x until the value no longer changes; numbers that agree (balls that
# overlap, equal machine reals) count as no change; fix_point(f, x, n): at most n applications; fix_point(f, x,
# same_test: g): until g(previous, next) is true. The last value of fixed_point_list.
fn fix_point(f, x, rest...) = last(apply(fixed_point_list, join([f, x], rest)))

# say(x, ...): print the arguments on one line (strings as their text, other values printed); the value is null
fn say(parts...) = { print(string_join(map(to_string, parts))); null }

# from_digits(s), (s, b): the integer whose digits in base b are the characters of the string s (0-9, then letters
# a-z or A-Z for 10-35); from_digits(s, "Roman"): the value of a Roman numeral (subtractive pairs such as XC)
# from_digits([ds, e]), ([ds, e], b): the number 0.ds times b^e (the form real_digits gives); a list as the last
# digit is a block repeated forever (0.a(r) = (A + R/(b^m - 1))/b^k)
rule from_digit_point(p_) := from_digit_point(p, 10)
# digits from indeterminate on are unknown (as real_digits gives them beyond a number's precision): the number of
# the known digits, with their precision (k digits of base b: k log10(b) decimal digits)
rule from_digit_point(p_, b_) := {
  let k = first(first(position(p[1], indeterminate, [1])))
  n(from_digit_point([take(p[1], k - 1), p[2]], b), (k - 1)*log10(b))
} when is_integer(b) and b > 1 and is_list(p[1]) and member(p[1], indeterminate) and not (first(p[1]) === indeterminate)
rule from_digit_point(p_, b_) := {
  let ds = p[1]
  let rep = length(ds) > 0 and is_list(last(ds))
  let a = if rep then most(ds) else ds
  let k = length(a)
  let fa = if k == 0 then 0 else from_digits(a, b)
  let fr = if rep and length(last(ds)) > 0 then from_digits(last(ds), b)/(b^length(last(ds)) - 1) else 0
  (fa + fr)/b^k * b^p[2]
} when is_integer(b) and b > 1
rule from_digit_string(s_) := from_digit_string(s, 10)
rule from_digit_string(s_, b_) := {
  let dv = k => if 48 <= k and k <= 57 then k - 48 else if 97 <= k and k <= 122 then k - 87 else if 65 <= k and k <= 90 then k - 55 else -1
  let ds = map(c => dv(to_character_code(c)[1]), characters(s))
  if every(ds, d => d >= 0) then from_digits(ds, b) else failure(argument, from_digits, "not a digit string")
} when is_integer(b) and b > 1
rule from_roman(s_, r_) := {
  let v = c => lookup(assoc("I" -> 1, "V" -> 5, "X" -> 10, "L" -> 50, "C" -> 100, "D" -> 500, "M" -> 1000), c, 0)
  let ds = map(v, characters(s))
  if length(ds) == 0 or member_of(ds, 0) then failure(argument, from_digits, "not a Roman numeral") else
    sum(map(i => if i < length(ds) and ds[i] < ds[i + 1] then -ds[i] else ds[i], range(length(ds))))
} when is_string(s)

# real_digits(x, b, len, n): len digits of |x| in base b starting with the coefficient of b^n, and the exponent
# n + 1 of the first ([digits of floor(|x| b^(len - 1 - n)) mod b^len, n + 1])
fn real_digits_at(x, b: Int, len: Int, start: Int) when len > 0 and b > 1 and is_numeric(x) = {
  if is_inexact(x) and x != 0 then {
    # the digits of x rounded at its precision; positions below it are indeterminate
    let e = floor(n(log(abs(x))/log(b))) + 1
    let lowest = e - ceiling(precision(x) * n(log(10)/log(b)))
    let q = round(rationalize(abs(x), 0) * b^(-lowest))
    [map(i => {
      let k = start - (i - 1)
      if k >= lowest then mod(floor(q / b^(k - lowest)), b) else indeterminate
    }, range(len)), start + 1]
  } else {
    # exact x: evaluated at enough digits for floor (the magnitude, the digits asked and a guard)
    let s = len - 1 - start
    let mag = if x == 0 then 0 else max(0, ceiling(n(log(abs(x))/log(10))))
    let y = if x == 0 then 0 else floor(n(abs(x) * b^s, mag + max(0, ceiling(s * n(log(b)/log(10)))) + 20))
    [integer_digits(mod(y, b^len), b, len), start + 1]
  }
}

# is_ordered(xs), is_ordered(xs, p): whether the elements of xs are in order (canonical, or by the predicate p)
fn is_ordered(xs, rest...) = apply(ordered, join([xs], rest))

# replace_repeated(e, rules): the rules applied over and over until e no longer changes
fn replace_repeated(e, rules) = rewrite(e, rules, strategy: fixed_point)

# block_map(f, xs, n), block_map(f, xs, n, d): f applied to the blocks partition(xs, n) (offset d)
# (a list n of block sizes cuts every dimension: f is applied at the level of the blocks, length(n) deep); the
# blocks of a term with another head keep it, the result is a list of the f(block)s
fn block_map(f, xs, n) = if is_list(n) then map(f, partition(xs, n), [length(n)]) else {
  let p = partition(xs, n)
  if not is_atom(xs) and not is_list(xs) and head(p) === head(xs) then map(f, apply(list, p)) else map(f, p)
}
fn block_map(f, xs, n, d) = if is_list(n) then map(f, partition(xs, n, d), [length(n)]) else {
  let p = partition(xs, n, d)
  if not is_atom(xs) and not is_list(xs) and head(p) === head(xs) then map(f, apply(list, p)) else map(f, p)
}

# --- item 9B: list, association and statistics helpers ---

# length_while(xs, f): how many elements from the start have f(x) true; take_while(xs, f): those elements
fn length_while(xs, f) = { var k = 0; let n = length(xs); while k < n and f(xs[k + 1]) === true { k = k + 1 }; k }
fn take_while(xs, f) = take(xs, length_while(xs, f))

# fold_while_list(f, x, xs, test), (..., m): x, f(x, xs[1]), ... while test of the latest value (the latest m
# values) is true, ending with the first value for which it is not; fold_while(...): its last value;
# fold_while(f, x, xs, test, m, k): the value k places before the last (k <= 0)
fn fold_while_list(f, x, xs, test) = fold_while_list(f, x, xs, test, 1)
fn fold_while_list(f, x, xs, test, m: Int) = {
  var vals = [x]
  var i = 0
  let n = length(xs)
  var go = test(x) === true
  while go and i < n {
    i = i + 1
    vals = append(vals, f(last(vals), xs[i]))
    go = apply(test, take(vals, -min(m, length(vals)))) === true
  }
  vals
}
fn fold_while(f, x, xs, test) when not is_integer(xs) = last(fold_while_list(f, x, xs, test, 1))
# the operator forms: fold_while(f, test)(x, xs), fold_while(f, test, m, k)(x, xs) (m an integer, k <= 0), nest(f, n)(x)
rule fold_while(f_, test_)(x_, xs_) := fold_while(f, x, xs, test)
rule fold_while(f_, test_, m_, k_)(x_, xs_) := fold_while(f, x, xs, test, m, k)
rule nest(f_, n_)(x_) := nest(f, x, n) when is_integer(n)
rule nest_list(f_, n_)(x_) := nest_list(f, x, n) when is_integer(n)
fn fold_while(f, x, xs, test, m: Int) = last(fold_while_list(f, x, xs, test, m))
fn fold_while(f, x, xs, test, m: Int, k: Int) when k <= 0 = { let vs = fold_while_list(f, x, xs, test, m); vs[max(1, length(vs) + k)] }

# boole(b): 1 for true, 0 for false (element-wise on lists); other arguments stay as they are
fn boole(true) = 1
fn boole(false) = 0
fn boole(bs) when is_list(bs) = map(boole, bs)

# is_numeric(x): whether x is a number or an expression that evaluates to one numerically (pi, sqrt(2) + 1)
fn is_numeric(x) = if is_number(x) then true else if is_string(x) or is_list(x) then false else is_number(n(x))

# digit_count(n), digit_count(n, b): the counts of the digits 1, 2, ..., b - 1, 0 of n in base b (10);
# digit_count(n, b, d): the count of digit d (a list of digits: their counts); digit_count(n, b, d, len): of the
# last len digits (leading zeros added when n has fewer)
fn digit_count(n: Int) = digit_count(n, 10)
fn digit_count(n: Int, b: Int) = map(d => digit_count(n, b, d), join(range(b - 1), [0]))
fn digit_count(n: Int, b: Int, d) = if is_list(d) then map(e => count(integer_digits(n, b), e), d) else count(integer_digits(n, b), d)
fn digit_count(n: Int, b: Int, d, len: Int) = { let ds = pad_left(integer_digits(n, b), len); if is_list(d) then map(e => count(ds, e), d) else count(ds, d) }

# digit_sum(n), digit_sum(n, b): the sum of the digits of |n| in base b (10); digit_sum(n, b, k): of the first k
# digits (the last -k for negative k); lists of numbers or of bases give lists
fn digit_sum(n: Int) = total(integer_digits(n))
fn digit_sum(n: Int, b: Int) = total(integer_digits(n, b))
fn digit_sum(n: Int, b: Int, k: Int) = { let ds = integer_digits(n, b); total(if k >= 0 then take(ds, min(k, length(ds))) else take(ds, max(k, -length(ds)))) }
fn digit_sum(n: Int, bs) when is_list(bs) = map(b => digit_sum(n, b), bs)
# in a mixed radix (the digits integer_digits gives in it)
fn digit_sum(n: Int, b) when match_q(b, mixed_radix(_)) = total(integer_digits(n, b))
fn digit_sum(n: Int, b, k: Int) when match_q(b, mixed_radix(_)) = { let ds = integer_digits(n, b); total(if k >= 0 then take(ds, min(k, length(ds))) else take(ds, max(k, -length(ds)))) }
fn digit_sum(ns, rest...) when is_list(ns) = map(m => apply(digit_sum, join([m], rest)), ns)

# key_value_map(f, a): [f(k1, v1), f(k2, v2), ...] for the entries of the association a
fn key_value_map(f, a) when is_assoc(a) = map(k => f(k, a[k]), keys(a))
# key_map(f, a): the association with every key k replaced by f(k)
fn key_map(f, a) when is_assoc(a) = assoc(map(k => f(k) -> a[k], keys(a)))
# association_map(f, xs): the association of x -> f(x) for the elements of xs (repeats once); for an association
# a, the association of the rules f(k -> v)
fn association_map(f, xs) when is_list(xs) = assoc(map(x => x -> f(x), unique(xs)))
fn association_map(f, a) when is_assoc(a) = assoc(map(k => f(k -> a[k]), keys(a)))
# association_thread(ks, vs): the association of ks[i] -> vs[i]
fn association_thread(ks, vs) when is_list(ks) and is_list(vs) and length(ks) == length(vs) = assoc(map(i => ks[i] -> vs[i], range(length(ks))))

# covariance(xs, ys): the sample covariance of two lists of equal length (divisor n - 1, the second list
# conjugated); covariance(m): the matrix of covariances of the columns of m; covariance(m1, m2): of the columns of
# m1 against those of m2. correlation(...): the same with each covariance divided by the standard deviations
fn covariance(xs, ys) when is_vector(xs) and is_vector(ys) and length(xs) == length(ys) and length(xs) > 1 = sum((xs - mean(xs))*conjugate(ys - mean(ys)))/(length(xs) - 1)
fn covariance(m) when is_matrix(m) = { let c = transpose(m); let k = length(c); table(covariance(c[i], c[j]), i, k, j, k) }
fn covariance(a, b) when is_matrix(a) and is_matrix(b) = { let c = transpose(a); let d = transpose(b); table(covariance(c[i], d[j]), i, length(c), j, length(d)) }
fn correlation(xs, ys) when is_vector(xs) and is_vector(ys) = covariance(xs, ys)/sqrt(covariance(xs, xs)*covariance(ys, ys))
fn correlation(m) when is_matrix(m) = { let c = transpose(m); let k = length(c); table(correlation(c[i], c[j]), i, k, j, k) }
fn correlation(a, b) when is_matrix(a) and is_matrix(b) = { let c = transpose(a); let d = transpose(b); table(correlation(c[i], d[j]), i, length(c), j, length(d)) }

# bin_counts(xs, w): the counts of the real numbers of xs in the bins [k w, (k + 1) w) from the lowest to the
# highest occupied; bin_counts(xs, [a, b, dx]): in the bins [a, a + dx), ... up to b; bin_counts(xs, [[e1, e2,
# ...]]): in [e1, e2), [e2, e3), ...
# quantities: data and bin specification in the unit of the first datum, counted as magnitudes
fn bin_counts(xs, spec) when is_list(xs) and length(xs) > 0 and head(xs[1]) === quantity = {
  let u = quantity_unit(xs[1])
  let mag = q => if head(q) === quantity then quantity_magnitude(q, u) else q
  bin_counts(map(mag, xs), map_all(mag, spec))
}
fn bin_counts(xs, w) when is_list(xs) and is_number(w) and w > 0 = {
  let vs = select(xs, x => is_real(n(x)))
  if length(vs) == 0 then [] else {
    let lo = floor(min(vs)/w)
    let hi = floor(max(vs)/w)
    let ix = tally(map(v => floor(v/w) - lo + 1, vs))
    var out = constant_array(0, hi - lo + 1)
    for p in ix { out = replace_part(out, [p[1]] -> p[2]) }
    out
  }
}
fn bin_counts(xs, spec) when is_list(xs) and is_list(spec) and length(spec) == 3 and is_real(n(spec[3])) = {
  let a = spec[1]
  let k = ceiling((spec[2] - a)/spec[3])
  bin_counts(xs, [map(i => a + i*spec[3], range(0, k))])
}
# bin_counts(xs, [a, b]): unit-width bins from a to b
fn bin_counts(xs, spec) when is_list(xs) and is_list(spec) and length(spec) == 2 and is_real(n(spec[1])) and is_real(n(spec[2])) = bin_counts(xs, [spec[1], spec[2], 1])
fn bin_counts(xs, spec) when is_list(xs) and is_list(spec) and length(spec) == 1 and is_list(spec[1]) = {
  let e = spec[1]
  let vs = select(xs, x => is_real(n(x)))
  map(i => count_if(vs, v => e[i] <= v and v < e[i + 1]), range(length(e) - 1))
}

# distribute(e), distribute(e, g): e = f(..., g(a, b), ...) spread over the arguments of g (add by default):
# g(f(..., a, ...), f(..., b, ...)), every combination, the first argument varying slowest; distribute(e, g, f):
# only when the head of e is f
fn distribute(e) = distribute(e, add)
fn distribute(e, g) = if is_atom(e) then e else {
  let parts = map(x => if not is_atom(x) and head(x) === g then apply(list, x) else [x], apply(list, e))
  if every(parts, p => length(p) == 1) then e else apply(g, map(t => apply(head(e), t), tuples(parts)))
}
fn distribute(e, g, f) = if not is_atom(e) and head(e) === f then distribute(e, g) else e

# sequence_fold_list(f, [x1, ..., xk], xs), (..., n): the list x1, ..., xk followed by each f(last k values, next n - k
# elements of xs) in turn (n is k + 1 by default); sequence_fold(...): its last value
fn sequence_fold_list(f, init, xs) = sequence_fold_list(f, init, xs, length(init) + 1)
fn sequence_fold_list(f, init, xs, n: Int) when n >= length(init) = {
  let k = length(init)
  let m = n - k
  var vals = init
  var i = 0
  while i + m <= length(xs) {
    vals = append(vals, apply(f, join(take(vals, -k), take(drop(xs, i), m))))
    i = i + 1
  }
  vals
}
fn sequence_fold(f, init, xs) = last(sequence_fold_list(f, init, xs))
fn sequence_fold(f, init, xs, n: Int) when n >= length(init) = last(sequence_fold_list(f, init, xs, n))

# curry_applied(f, spec): f taking its arguments over several calls, curry_applied(f, n)(a1)(a2)...(an) is
# f(a1, ..., an); a call may give several arguments or none, the calls so far collect into one
# (curry_applied(f, 3)(a)(b) is curry_applied(f, 3)(a, b)); the spec is n (the arguments in order), [i1, ..., ik]
# (f of arguments i1, ..., ik of max(i) arguments) or n -> [i1, ..., ik]; arguments past the count are given to
# the result; curry_applied(spec)(f) is curry_applied(f, spec). operator_applied(f, spec) is the same, and
# operator_applied(f)(x)(y) is f(y, x); reverse_applied(f)(a, b, ...) is f(..., b, a)
rule curry_applied(s_)(f_) := curry_applied(f, s) when is_integer(s) or is_list(s) or match_q(s, _ -> _)
rule curry_applied(f_, s_)(xs___)(ys___) := apply(curry_applied(f, s), join(xs, ys))
rule curry_applied(f_, s_)(xs___) := {
  let ar = if is_integer(s) then s else if is_list(s) then max(s) else s[1]
  let r = apply(f, map(i => xs[i], if is_integer(s) then range(s) else if is_list(s) then s else s[2]))
  if length(xs) > ar then apply(r, drop(xs, ar)) else r
} when length(xs) >= (if is_integer(s) then s else if is_list(s) then max(s) else s[1])
rule operator_applied(f_)(x_)(y_) := f(y, x)
rule operator_applied(f_, s_)(xs___)(ys___) := apply(operator_applied(f, s), join(xs, ys))
rule operator_applied(f_, s_)(xs___) := {
  let ar = if is_integer(s) then s else if is_list(s) then max(s) else s[1]
  let r = apply(f, map(i => xs[i], if is_integer(s) then range(s) else if is_list(s) then s else s[2]))
  if length(xs) > ar then apply(r, drop(xs, ar)) else r
} when length(xs) >= (if is_integer(s) then s else if is_list(s) then max(s) else s[1])
fn reverse_applied(f) = (args...) => apply(f, reverse(args))

# take_list(xs, [n1, n2, ...]): the successive pieces take(rest, ni) of what remains of xs (negative ni: from the
# end of what remains; all: everything that remains)
fn take_list(xs, ns) when is_list(ns) = {
  var left = xs
  var out = []
  for k in ns {
    if k === all then { out = append(out, left); left = take(left, 0) } else { out = append(out, take(left, k)); left = drop(left, k) }
  }
  out
}

# array_reshape(xs, dims), array_reshape(xs, dims, pad): the elements of flatten(xs) arranged in an array of the
# dimensions dims, cut when there are more and padded with pad (0) when there are fewer
fn array_reshape(xs, dims) when is_list(xs) and is_list(dims) = array_reshape(xs, dims, 0)
fn array_reshape(xs, dims, pad) when is_list(xs) and is_list(dims) and length(dims) > 0 = {
  let k = product(dims)
  let flat = flatten(xs)
  let full = if length(flat) >= k then take(flat, k) else join(flat, constant_array(pad, k - length(flat)))
  fold((acc, d) => partition(acc, d), full, reverse(rest(dims)))
}

# flatten_at(xs, i), flatten_at(xs, [i, j, ...]), flatten_at(xs, [[pos1], [pos2], ...]): xs with the part at each
# position replaced by its own arguments, spliced into its parent
fn flatten_at(xs, spec) = {
  let ps = if is_integer(spec) then [[spec]] else if every(spec, is_integer) then [spec] else spec
  var out = xs
  for p in reverse(sort(ps)) {
    let up = most(p)
    let i = last(p)
    let parent = if length(up) == 0 then out else extract(out, up)
    let n = length(parent)
    let j = if i < 0 then n + 1 + i else i
    let elem = parent[j]
    if not is_atom(elem) then {
      let spliced = apply(head(parent), join(take(apply(list, parent), j - 1), apply(list, elem), drop(apply(list, parent), j)))
      out = if length(up) == 0 then spliced else replace_part(out, up -> spliced)
    }
  }
  out
}

# key_complement([a1, a2, ...]): the entries of a1 whose keys are in none of a2, ... (each an association, a list
# of rules or a rule)
fn key_complement(xs) when is_list(xs) and length(xs) > 0 = {
  let ms = map(x => if is_assoc(x) then x else if is_list(x) then assoc(x) else assoc([x]), xs)
  let others = flatten(map(keys, rest(ms)), 1)
  select_keys(ms[1], k => not member(others, k))
}

# operate(p, e), operate(p, e, n): e with p applied to its head (n = 1), to the head of its head (n = 2), ...;
# operate(p, e, 0) is p(e)
fn operate(p, e) = operate(p, e, 1)
fn operate(p, e, n: Int) when n >= 0 = if n == 0 then p(e) else if is_atom(e) then e else apply(operate(p, head(e), n - 1), apply(list, e))

# numerical_sort(xs): the elements of xs (a list or another compound term) that have a real numerical value (inf and
# -inf included) in increasing order of it, followed by the others in canonical order
fn numerical_sort(xs) when not is_atom(xs) and not is_assoc(xs) = {
  let real = x => (x === inf or x === -inf or is_real(n(x))) === true
  let items = apply(list, xs)
  apply(head(xs), join(sort(select(items, real), (a, b) => n(a) <= n(b)), sort(select(items, x => not real(x)))))
}

# design_matrix(data, [f1, ..., fm], x), (..., [x1, ..., xk]): the matrix of the basis functions fj at the points
# of data, rows [x1, ..., xk, y] (a list of values y: the points 1, 2, ...)
fn design_matrix(data, funs, xv) when is_list(data) and is_list(funs) = {
  let vs = if is_list(xv) then xv else [xv]
  let k = length(vs)
  let rows = if every(data, d => not is_list(d)) then map(i => [i, data[i]], range(length(data))) else data
  map(r => map(f => rewrite(f, map(i => vs[i] -> r[i], range(k))), funs), rows)
}

# fit(data, [f1, ..., fm], x), (..., [x1, ..., xk]): the linear combination c1 f1 + ... + cm fm closest to the
# data in least squares (data as for design_matrix), the coefficients found numerically through the pseudo-inverse
# the option fit_regularization: ["Tikhonov", lambda], ["L2", lambda] or ["RidgeRegression", lambda] minimises the
# squared residual plus lambda times the squared norm of the coefficients (the normal equations with lambda added
# to the diagonal; Tikhonov, Solution of incorrectly formulated problems, 1963); ["LASSO", lambda] or ["L1", lambda]
# minimises the squared residual plus lambda times the sum of the absolute coefficients (many coefficients become 0)
fn fit(data, funs, xv; fit_regularization = none) when is_list(data) and is_list(funs) = {
  let vs = if is_list(xv) then xv else [xv]
  let num = v => if is_inexact(v) then v else n(v)
  let rows = if every(data, d => not is_list(d)) then map(i => [i, data[i]], range(length(data))) else data
  let a = map(r => map(num, r), design_matrix(rows, funs, vs))
  let y = map(r => num(r[length(vs) + 1]), rows)
  let ridge = is_list(fit_regularization) and length(fit_regularization) == 2 and some(["Tikhonov", "L2", "RidgeRegression"], s => s === fit_regularization[1])
  let lasso = is_list(fit_regularization) and length(fit_regularization) == 2 and some(["LASSO", "L1"], s => s === fit_regularization[1])
  let c = if ridge then {
    let m = length(funs)
    linear_solve(dot(transpose(a), a) + n(fit_regularization[2])*identity_matrix(m), dot(transpose(a), y))
  } else if lasso then b36_lasso(a, y, n(fit_regularization[2])) else dot(pseudo_inverse(a), y)
  sum(map(j => c[j]*funs[j], range(length(funs))))
}

# b36_lasso(a, y, lambda): the coefficients c minimising |y - a c|^2 + lambda |c|_1 by cyclic coordinate descent with
# soft thresholding (Friedman, Hastie and Tibshirani, Regularization paths for generalized linear models, 2010)
fn b36_lasso(a, y, lam) = {
  let cols = transpose(a)
  let m = length(cols)
  let z = map(c => dot(c, c), cols)
  var c = constant_array(0., m)
  var r = y
  var sweep = 0
  var moved = 1.
  while moved > 1e-15 and sweep < 200000 {
    moved = 0.
    for j in range(m) {
      if z[j] > 0 then {
        let rho = dot(cols[j], r) + z[j]*c[j]
        let t = lam/2
        let new = if rho > t then (rho - t)/z[j] else if rho < -t then (rho + t)/z[j] else 0.
        let d = new - c[j]
        if d != 0 then {
          r = r - d*cols[j]
          c[j] = new
          moved = max(moved, abs(d)*sqrt(z[j]))
        }
      }
    }
    sweep = sweep + 1
  }
  c
}

# operator forms: association_map(f)(xs), key_value_map(f)(a), key_map(f)(a); association_thread(ks -> vs)
fn association_map(f) = xs => association_map(f, xs)
fn key_value_map(f) = a => key_value_map(f, a)
fn key_map(f) = a => key_map(f, a)
fn association_thread(r) when match_q(r, _ -> _) = association_thread(r[1], r[2])

# --- item 9B part 2: coordinate bounds ---

# coordinate_bounds(pts), coordinate_bounds(pts, d): [[min, max], ...] of each coordinate of the points in pts (the
# innermost lists, at any depth); d pads every side: a number, scaled(s) (s times the width), or a list with one
# entry per coordinate, each a number, scaled(s) or [below, above]
fn coordinate_bounds(pts) when is_list(pts) = {
  let ps = if every(pts, x => not is_list(x)) then [pts] else cases(pts, p_ when is_list(p) and not some(p, is_list), inf)
  map(c => [min(c), max(c)], transpose(ps))
}
fn coordinate_bounds(pts, d) when is_list(pts) = {
  let bs = coordinate_bounds(pts)
  let side = (p, w) => if match_q(p, scaled(_)) then p[1]*w else p
  let pad = (b, p) => { let w = b[2] - b[1]; let q = if is_list(p) then p else [p, p]; [b[1] - side(q[1], w), b[2] + side(q[2], w)] }
  # a pair that is not one entry per coordinate is [below, above] for every coordinate
  map(i => pad(bs[i], if is_list(d) and length(d) == length(bs) then d[i] else d), range(length(bs)))
}

# coordinate_bounding_box(pts), coordinate_bounding_box(pts, d): the corners [[min x, min y, ...], [max x, max y,
# ...]] of the points in pts, padded as in coordinate_bounds
fn coordinate_bounding_box(pts, rest...) when is_list(pts) = transpose(apply(coordinate_bounds, join([pts], rest)))

# coordinate_bounds_array([[a1, b1], ...]), (..., d), (..., d, offset), (..., d, offset, pad): the array of the grid
# points a + offset + k d within each [a, b] (widened by pad as in coordinate_bounds); d is a step or into(n) (n
# equal parts), one for all coordinates or a list; offset is center (half a step), a number, scaled(s) (s steps)
# or a list of them
fn coordinate_bounds_array(bs) when is_list(bs) = coordinate_bounds_array(bs, 1, 0)
fn coordinate_bounds_array(bs, d) when is_list(bs) = coordinate_bounds_array(bs, d, 0)
fn coordinate_bounds_array(bs, d, o, pad) when is_list(bs) = coordinate_bounds_array(coordinate_bounds(transpose(bs), pad), d, o)
fn coordinate_bounds_array(bs, d, o) when is_list(bs) and every(bs, b => is_list(b) and length(b) == 2) = {
  let at = (s, i) => if is_list(s) then s[i] else s
  let axis = i => {
    let a = bs[i][1]
    let b = bs[i][2]
    let di = at(d, i)
    let step = if match_q(di, into(_)) then (b - a)/di[1] else di
    let oi = at(o, i)
    let off = if oi === center then step/2 else if match_q(oi, scaled(_)) then oi[1]*step else oi
    let r = (b - a - off)/step
    let k = floor(if is_inexact(r) then r + 1/10^9 else r)
    map(j => a + off + j*step, range(0, k))
  }
  apply(outer, join([list], map(axis, range(length(bs)))))
}

# coordinate_bounding_box_array([[a1, a2, ...], [b1, b2, ...]], ...): coordinate_bounds_array of the box with those
# corners
fn coordinate_bounding_box_array(corners, rest...) when is_list(corners) = apply(coordinate_bounds_array, join([transpose(corners)], rest))

# --- item 9B part 2: logic, division, echoes, binning ---

# nand(a, b, ...): not (a and b and ...); true arguments are dropped, a false one gives true, nand() is false;
# nor(a, b, ...): not (a or b or ...); false arguments are dropped, a true one gives false, nor() is true
fn nand() = false
fn nand(a) = not a
fn nand(a, b, rest...) when some(join([a, b], rest), x => x === true or x === false) = { let xs = join([a, b], rest); if some(xs, x => x === false) then true else apply(nand, select(xs, x => not (x === true))) }
fn nor() = true
fn nor(a) = not a
fn nor(a, b, rest...) when some(join([a, b], rest), x => x === true or x === false) = { let xs = join([a, b], rest); if some(xs, x => x === true) then false else apply(nor, select(xs, x => not (x === false))) }

# divide(a, b): a/b
fn divide(a, b) = a/b

# echo(x), echo(x, label), echo(x, label, f): print x (after the label; f(x) instead of x), and give x;
# echo_label(label)(x) is echo(x, label); echo_function(f)(x) prints f(x) and gives x, echo_function(label, f)(x)
# is echo(x, label, f)
fn echo(x) = { print(x); x }
fn echo(x, label) = { say(label, " ", x); x }
fn echo(x, label, f) = { say(label, " ", f(x)); x }
fn echo_label() = x => echo(x)
fn echo_label(label) = x => echo(x, label)
fn echo_function() = x => echo(x)
fn echo_function(f) = x => { print(f(x)); x }
fn echo_function(label, f) = x => echo(x, label, f)

# echo_evaluation(e), (e, label), (e, before -> after), (e, label, f): print e before it is evaluated and its value
# after (f(value) with f), each after its label, and give the value
fn echo_evaluation(held e) = { say(quote(e)); let v = e; print(v); v }
fn echo_evaluation(held e, label) = {
  let ls = if match_q(label, _ -> _) then [label[1], label[2]] else [label, label]
  say(ls[1], " ", quote(e))
  let v = e
  say(ls[2], " ", v)
  v
}
fn echo_evaluation(held e, label, f) = { say(label, " ", quote(e)); let v = e; say(label, " ", f(v)); v }

# echo_timing(e), echo_timing(e, label): print the wall-clock seconds e took (after the label) and give its value
fn echo_timing(held e) = { let r = timing(e); print(r[1]); r[2] }
fn echo_timing(held e, label) = { let r = timing(e); say(label, " ", r[1]); r[2] }

# bin_lists(xs, w), bin_lists(xs, [a, b, dx]), bin_lists(xs, [[e1, e2, ...]]): the real numbers of xs in each bin,
# the bins as for bin_counts
# data given as a list of x -> label rules: the labels binned by their x
# quantities: binned by their magnitudes in the unit of the first datum (the bins converted too), the data kept
fn bin_lists(xs, spec) when is_list(xs) and length(xs) > 0 and head(xs[1]) === quantity = {
  let u = quantity_unit(xs[1])
  let mag = q => if head(q) === quantity then quantity_magnitude(q, u) else q
  bin_lists(map(mag, xs) -> xs, map_all(mag, spec))
}
fn bin_lists(xs, w) when is_list(xs) and is_number(w) and w > 0 and length(xs) > 0 and every(xs, r => match_q(r, (_ -> _))) = bin_lists(map(first, xs) -> map(last, xs), w)
fn bin_lists(xs, w) when is_list(xs) and is_number(w) and w > 0 = {
  let vs = select(xs, x => is_real(n(x)))
  if length(vs) == 0 then [] else {
    let lo = floor(min(vs)/w)
    let hi = floor(max(vs)/w)
    map(k => select(vs, v => floor(v/w) == k), range(lo, hi))
  }
}
fn bin_lists(xs, spec) when is_list(xs) and is_list(spec) and length(spec) == 3 and is_real(n(spec[3])) = {
  let a = spec[1]
  let k = ceiling((spec[2] - a)/spec[3])
  bin_lists(xs, [map(i => a + i*spec[3], range(0, k))])
}
fn bin_lists(xs, spec) when is_list(xs) and is_list(spec) and length(spec) == 1 and is_list(spec[1]) = {
  let e = spec[1]
  let vs = select(xs, x => is_real(n(x)))
  map(i => select(vs, v => e[i] <= v and v < e[i + 1]), range(length(e) - 1))
}

# bin_lists(xs -> labels, spec), bin_lists(xs -> automatic, spec): the labels (the positions for automatic) of the
# elements of xs in each bin, the bins as for bin_lists(xs, spec)
fn nl_bin_index(xs, w) when is_number(w) and w > 0 = {
  let ix = select(range(length(xs)), i => is_real(n(xs[i])))
  if length(ix) == 0 then [] else {
    let ks = map(i => floor(xs[i]/w), ix)
    map(k => select(ix, i => floor(xs[i]/w) == k), range(min(ks), max(ks)))
  }
}
fn nl_bin_index(xs, spec) when is_list(spec) and length(spec) == 3 and is_real(n(spec[3])) = {
  let k = ceiling((spec[2] - spec[1])/spec[3])
  nl_bin_index(xs, [map(i => spec[1] + i*spec[3], range(0, k))])
}
fn nl_bin_index(xs, spec) when is_list(spec) and length(spec) == 1 and is_list(spec[1]) = {
  let e = spec[1]
  let ix = select(range(length(xs)), i => is_real(n(xs[i])))
  map(j => select(ix, i => e[j] <= xs[i] and xs[i] < e[j + 1]), range(length(e) - 1))
}
fn bin_lists(r, spec) when match_q(r, _ -> _) and is_list(r[1]) = {
  let labels = if r[2] === automatic then range(length(r[1])) else r[2]
  map(ix => map(i => labels[i], ix), nl_bin_index(r[1], spec))
}

# maximal_by(xs, f), minimal_by(xs, f): every element of xs with the largest (smallest) f(x), in their order (for
# an association, those entries); maximal_by(xs, f, n): the n elements with the largest keys, largest first, ties
# in their order (up_to(n): at most n); maximal_by(f)(xs) is maximal_by(xs, f)
# (keys that are not all numbers are ordered canonically, as by sort)
fn maximal_by(xs, f) when is_list(xs) = if length(xs) == 0 then [] else { let ks = map(f, xs); let m0 = max(ks); let m = if is_numeric(m0) then m0 else last(sort(ks)); select(xs, x => (f(x) == m) === true or f(x) === m) }
fn minimal_by(xs, f) when is_list(xs) = if length(xs) == 0 then [] else { let ks = map(f, xs); let m0 = min(ks); let m = if is_numeric(m0) then m0 else first(sort(ks)); select(xs, x => (f(x) == m) === true or f(x) === m) }
fn maximal_by(a, f) when is_assoc(a) = { let m = max(map(f, values(a))); select_keys(a, k => (f(a[k]) == m) === true) }
fn minimal_by(a, f) when is_assoc(a) = { let m = min(map(f, values(a))); select_keys(a, k => (f(a[k]) == m) === true) }
fn maximal_by(xs, f, n) when is_list(xs) = {
  let k = if match_q(n, up_to(_)) then min(n[1], length(xs)) else n
  let ks = map(f, xs)
  map(i => xs[i], take(reverse(sort_by(range(length(xs)), i => [ks[i], -i])), k))
}
fn minimal_by(xs, f, n) when is_list(xs) = {
  let k = if match_q(n, up_to(_)) then min(n[1], length(xs)) else n
  let ks = map(f, xs)
  map(i => xs[i], take(sort_by(range(length(xs)), i => [ks[i], i]), k))
}
fn maximal_by(f) = xs => maximal_by(xs, f)
fn minimal_by(f) = xs => minimal_by(xs, f)

# --- round 18 phase 9B: orders, operator forms, subsets, truth tables ---

# subtract(x, y): x - y
fn subtract(x, y) = x - y
# minus(x): -x (the negation as a function: key_sort_by(a, minus) sorts by descending keys)
fn minus(x) = -x

# greater_than(y), less_than(y), greater_equal_than(y), less_equal_than(y), equal_to(y), unequal_to(y): the
# comparison with y as a function of one argument (greater_than(3)(5) is 5 > 3)
fn greater_than(y) = x => x > y
fn less_than(y) = x => x < y
fn greater_equal_than(y) = x => x >= y
fn less_equal_than(y) = x => x <= y
fn equal_to(y) = x => x == y
fn unequal_to(y) = x => x != y

# order(a, b): 1 when a comes before b in the canonical order, -1 when after, 0 when they are the same term
fn order(a, b) = if a === b then 0 else if ordered([a, b]) then 1 else -1

# numerical_order(a, b): 1 when a is numerically smaller than b, -1 when larger, 0 when equal (complex numbers by
# real part, then size of the imaginary part); undecided comparisons give 0
# an argument that is not numeric (a symbol, a string) comes after the numbers; two of them in the canonical order
fn numerical_order_number(x) = is_numeric(x) or x === inf or x === -inf or (not is_atom(x) and head(x) === quantity)
fn numerical_order(a, b) when not numerical_order_number(a) or not numerical_order_number(b) = if numerical_order_number(a) then 1 else if numerical_order_number(b) then -1 else order(a, b)
fn numerical_order(a, b) = {
  let x = n(a)
  let y = n(b)
  let rx = re(x)
  let ry = re(y)
  let ix = abs(im_part(x))
  let iy = abs(im_part(y))
  if (rx < ry) === true then 1 else if (rx > ry) === true then -1 else if (ix < iy) === true then 1 else if (ix > iy) === true then -1 else 0
}

# lexicographic_order(a, b), lexicographic_order(a, b, p): compares the elements of a and b in turn (by order, or
# the ordering function p), a prefix first; strings by their characters, other atoms as one-element lists,
# associations by their values
fn lexicographic_order(a, b) = lexicographic_order(a, b, order)
fn lexicographic_order(a, b, p) = {
  let parts = u => if is_assoc(u) then values(u) else if is_string(u) then characters(u) else if is_atom(u) then [u] else apply(list, u)
  let xs = parts(a)
  let ys = parts(b)
  let m = min(length(xs), length(ys))
  var i = 1
  var r = 0
  while r === 0 and i <= m {
    r = p(xs[i], ys[i])
    r = if r === true then 1 else if r === false then -1 else r
    i = i + 1
  }
  if r !== 0 then r else if length(xs) < length(ys) then 1 else if length(xs) > length(ys) then -1 else 0
}

# lexicographic_sort(xs), lexicographic_sort(xs, p): the elements of xs in lexicographic order of their parts
fn lexicographic_sort(xs) = sort(xs, lexicographic_order)
fn lexicographic_sort(xs, p) = sort(xs, (a, b) => lexicographic_order(a, b, p))

# alphabetic_order(a, b): 1, 0 or -1 as the strings a and b are in alphabetic order (letter case folded first, then
# lower case before upper case); alphabetic_order() and alphabetic_order("English") are the ordering function
# a ligature or a letter with a diacritic sorts as its base letters (æ as ae, é as e, ß as ss, as the root collation of
# UTS #10 does at its first level); strings that differ only in that fall back to the code-point-aware order
fn l36_alpha_fold(s) = if any_true(to_character_code(s), c => c > 127) then string_replace(s, [
  "æ" -> "ae", "Æ" -> "AE", "œ" -> "oe", "Œ" -> "OE", "ß" -> "ss",
  "à" -> "a", "á" -> "a", "â" -> "a", "ã" -> "a", "ä" -> "a", "å" -> "a", "ā" -> "a", "ă" -> "a", "ą" -> "a",
  "À" -> "A", "Á" -> "A", "Â" -> "A", "Ã" -> "A", "Ä" -> "A", "Å" -> "A", "Ā" -> "A", "Ă" -> "A", "Ą" -> "A",
  "ç" -> "c", "ć" -> "c", "č" -> "c", "Ç" -> "C", "Ć" -> "C", "Č" -> "C", "ď" -> "d", "Ď" -> "D",
  "è" -> "e", "é" -> "e", "ê" -> "e", "ë" -> "e", "ē" -> "e", "ę" -> "e", "ě" -> "e",
  "È" -> "E", "É" -> "E", "Ê" -> "E", "Ë" -> "E", "Ē" -> "E", "Ę" -> "E", "Ě" -> "E",
  "ì" -> "i", "í" -> "i", "î" -> "i", "ï" -> "i", "ī" -> "i", "Ì" -> "I", "Í" -> "I", "Î" -> "I", "Ï" -> "I", "Ī" -> "I",
  "ł" -> "l", "Ł" -> "L", "ñ" -> "n", "ń" -> "n", "ň" -> "n", "Ñ" -> "N", "Ń" -> "N", "Ň" -> "N",
  "ò" -> "o", "ó" -> "o", "ô" -> "o", "õ" -> "o", "ö" -> "o", "ø" -> "o", "ō" -> "o", "ő" -> "o",
  "Ò" -> "O", "Ó" -> "O", "Ô" -> "O", "Õ" -> "O", "Ö" -> "O", "Ø" -> "O", "Ō" -> "O", "Ő" -> "O",
  "ř" -> "r", "Ř" -> "R", "ś" -> "s", "š" -> "s", "ş" -> "s", "Ś" -> "S", "Š" -> "S", "Ş" -> "S",
  "ť" -> "t", "Ť" -> "T", "ù" -> "u", "ú" -> "u", "û" -> "u", "ü" -> "u", "ū" -> "u", "ů" -> "u", "ű" -> "u",
  "Ù" -> "U", "Ú" -> "U", "Û" -> "U", "Ü" -> "U", "Ū" -> "U", "Ů" -> "U", "Ű" -> "U",
  "ý" -> "y", "ÿ" -> "y", "Ý" -> "Y", "ź" -> "z", "ż" -> "z", "ž" -> "z", "Ź" -> "Z", "Ż" -> "Z", "Ž" -> "Z"]) else s
fn alphabetic_order(a: Str, b: Str) = {
  let o = order(l36_alpha_fold(a), l36_alpha_fold(b))
  if o == 0 then order(a, b) else o
}
fn alphabetic_order() = alphabetic_order
fn alphabetic_order(lang: Str) when lang == "English" = alphabetic_order
# languages whose alphabets order some letters as letters of their own (round 37, the tailorings of the Unicode
# Common Locale Data Repository, CLDR, for the Latin script): a unit (a letter or a digraph) -> [the letter it
# follows, its rank after it]; rank 0 is the letter itself at the first level (Swedish ü is y). Serbian tailors
# only the Cyrillic script: Latin text has the root order.
fn r37_tailoring(lang) =
  if lang == "Slovak" then assoc("ä" -> ["a", 1], "č" -> ["c", 1], "ch" -> ["h", 1], "ô" -> ["o", 1], "š" -> ["s", 1], "ž" -> ["z", 1])
  else if lang == "Spanish" then assoc("ñ" -> ["n", 1])
  else if lang == "Swedish" then assoc("å" -> ["z", 1], "ä" -> ["z", 2], "æ" -> ["z", 2], "ö" -> ["z", 3], "ø" -> ["z", 3], "ü" -> ["y", 0])
  else if lang == "Serbian" then assoc()
  else null
# the first-level weights of a string under a tailoring: the units of the tailoring (two-letter ones first) weigh
# just after their letter, every other character as its root letters (l36_alpha_fold)
fn r37_weights(s, tab) = {
  let cs = characters(to_lower(s))
  let w = c => 4*to_character_code(c)[1]
  var out = []
  var i = 1
  while i <= length(cs) {
    let two = if i < length(cs) then string_join(cs[i], cs[i + 1]) else ""
    if two != "" and key_exists(tab, two) then {
      out = append(out, w(tab[two][1]) + tab[two][2])
      i = i + 2
    } else if key_exists(tab, cs[i]) then {
      out = append(out, w(tab[cs[i]][1]) + tab[cs[i]][2])
      i = i + 1
    } else {
      out = join(out, map(w, characters(l36_alpha_fold(cs[i]))))
      i = i + 1
    }
  }
  out
}
fn alphabetic_order(lang: Str) when r37_tailoring(lang) !== null = {
  let tab = r37_tailoring(lang)
  (a, b) => {
    let o = lexicographic_order(r37_weights(a, tab), r37_weights(b, tab))
    if o == 0 then alphabetic_order(a, b) else o
  }
}

# alphabetic_sort(xs), alphabetic_sort(xs, "English"): the strings of xs in alphabetic order
fn alphabetic_sort(xs) when is_list(xs) = sort(xs, alphabetic_order)
fn alphabetic_sort(xs, lang: Str) when is_list(xs) and lang == "English" = sort(xs, alphabetic_order)
fn alphabetic_sort(xs, lang: Str) when is_list(xs) and r37_tailoring(lang) !== null = sort(xs, alphabetic_order(lang))
# case_ordering: "LowerFirst" (the default order); "UpperFirst" is not defined and the call stays
rule alphabetic_sort(xs_, lang_, case_ordering: c_) := sort(xs, alphabetic_order) when is_list(xs) and lang === "English" and c === "LowerFirst"

# alphabet(), alphabet("English"): the lower-case letters a to z
fn alphabet() = character_range("a", "z")
fn alphabet(lang: Str) when lang == "English" = character_range("a", "z")

# split_by(xs, f): the runs of neighbours of xs with identical f(x); split_by(xs, [f1, f2, ...]): split by f1, each
# run then by f2, and so on; split_by(f)(xs)
fn split_by(xs, f) when is_list(xs) = if is_list(f) then (if length(f) == 0 then xs else map(ys => split_by(ys, rest(f)), split_by(xs, first(f)))) else split(xs, (a, b) => f(a) === f(b))
fn split_by(f) = xs => split_by(xs, f)

# count_distinct_by(xs, f): the number of distinct values f(x) over the elements of xs (an association: its
# values); count_distinct_by(f)(xs)
fn count_distinct_by(xs, f) = length(unique(map(f, if is_assoc(xs) then values(xs) else xs)))
fn count_distinct_by(f) = xs => count_distinct_by(xs, f)

# delete_adjacent_duplicates(xs), delete_adjacent_duplicates(xs, test): xs without each element identical to (or
# with test(previous, x) true for) the element just before it; any head; an association by its values
fn delete_adjacent_duplicates(xs) = delete_adjacent_duplicates(xs, same)
fn delete_adjacent_duplicates(a, test) when is_assoc(a) = {
  let ks = keys(a)
  let vs = values(a)
  let keep = select(range(length(vs)), i => i == 1 or test(vs[i - 1], vs[i]) !== true)
  association_thread(ks[keep], vs[keep])
}
fn delete_adjacent_duplicates(xs, test) when not is_atom(xs) and not is_assoc(xs) = {
  let keep = select(range(length(xs)), i => i == 1 or test(xs[i - 1], xs[i]) !== true)
  xs[keep]
}

# key_sort_by(a, f): the association a (or list of rules) with its entries in the canonical order of f(key);
# key_sort_by(f)(a)
fn key_sort_by(a, f) when is_assoc(a) = { let ks = sort_by(keys(a), f); association_thread(ks, map(k => a[k], ks)) }
fn key_sort_by(rs, f) when is_list(rs) = key_sort_by(apply(assoc, rs), f)
# r37_key_sort(a, p): the entries of a with its keys in the order of the ordering function p (`key_sort(a, p)`, round 37)
fn r37_key_sort(a, p) when is_assoc(a) = { let ks = sort(keys(a), p); association_thread(ks, map(k => a[k], ks)) }
fn key_sort_by(f) = a => key_sort_by(a, f)

# unique_elements([xs1, xs2, ...]): for each list, its distinct elements that are in none of the other lists
fn unique_elements(xss) when is_list(xss) and all_true(xss, is_list) =
  map(i => unique(select(xss[i], x => not member(catenate(join(take(xss, i - 1), drop(xss, i))), x))), range(length(xss)))

# subset_map(f, xs, positions): xs with the parts at the positions (integers or index lists) replaced by the
# elements of f applied to the list of those parts; subset_map(f, positions)(xs)
fn subset_map(f, xs, pos) when is_list(pos) = {
  let ps = map(p => if is_list(p) then p else [p], pos)
  let vs = f(map(p => extract(xs, p), ps))
  if is_list(vs) and length(vs) == length(ps) then replace_part(xs, map(i => ps[i] -> vs[i], range(length(ps)))) else failure(shape_mismatch, subset_map)
}
fn subset_map(f, pos) = xs => subset_map(f, xs, pos)

# operator forms of the subset functions: subset_cases(p)(xs), subset_position(p)(xs), subset_replace(r)(xs)
rule subset_cases(p_)(xs_) := subset_cases(xs, p)
rule subset_position(p_)(xs_) := subset_position(xs, p)
rule subset_replace(r_)(xs_) := subset_replace(xs, r)

# boolean_table(e, [x1, ..., xn]): the values of e for every assignment of true and false to the variables, from
# all true to all false (the last variable changing fastest); e may be a list or a rule of expressions
fn boolean_table(e, vars) when is_list(vars) = map(vals => rewrite(e, map(i => vars[i] -> vals[i], range(length(vars)))), tuples([true, false], length(vars)))

# is_tautology(e), is_tautology(e, vars): whether the Boolean expression e is true for every assignment of its
# variables (the symbols in it, at most 20); is_satisfiable(e), is_satisfiable(e, vars): whether it is true for
# some. Stays unevaluated when a value is not true or false.
fn is_tautology(e) = is_tautology(e, select(unique(level(e, [-1])), v => is_symbol(v) and v !== true and v !== false))
fn is_tautology(e, vars) when is_list(vars) and length(vars) <= 20 and all_true(boolean_table(e, vars), b => b === true or b === false) =
  all_true(boolean_table(e, vars), b => b === true)
fn is_satisfiable(e) = is_satisfiable(e, select(unique(level(e, [-1])), v => is_symbol(v) and v !== true and v !== false))
fn is_satisfiable(e, vars) when is_list(vars) and length(vars) <= 20 and all_true(boolean_table(e, vars), b => b === true or b === false) =
  any_true(boolean_table(e, vars), b => b === true)

# equivalent(a, b, ...): true when all the arguments are true or all false; with true among them the conjunction
# of the others, with false the conjunction of their negations, with both false; repeated arguments are dropped and
# a call with two or more distinct symbolic arguments stays
rule equivalent(xs___) := {
  let u = unique(xs)
  let others = select(u, x => x !== true and x !== false)
  if member(u, true) and member(u, false) then false
  else if member(u, true) then (if length(others) == 0 then true else apply(`and`, others))
  else if member(u, false) then (if length(others) == 0 then true else apply(`and`, map(x => not x, others)))
  else if length(u) < 2 then true else apply(equivalent, u)
} when length(xs) < 2 or length(unique(xs)) < length(xs) or member(xs, true) or member(xs, false)


# probability(c, x, d): the probability that the condition c on x holds when x has the distribution d. The condition
# is a comparison of x with a bound (x < b, b <= x, x == b, ...) or a conjunction of a lower and an upper bound; for
# the discrete families (binomial, Poisson, geometric) the bounds are rounded to the integers they allow; the
# probability of an interval is written with erf rather than a difference of erfc values
fn probability(c, x, d) when is_symbol(x) = {
  let discrete = match_q(d, binomial_distribution(___) | poisson_distribution(___) | geometric_distribution(___))
  let le = b => cdf(d, if discrete then floor(b) else b)
  let lt = b => if discrete then cdf(d, ceiling(b) - 1) else cdf(d, b)
  let flip = h => if h === less then greater else if h === less_equal then greater_equal else if h === greater then less else if h === greater_equal then less_equal else h
  let side = q => if is_atom(q) or length(q) != 2 then [null, null] else if q[1] === x and free_of(q[2], x) then [head(q), q[2]] else if q[2] === x and free_of(q[1], x) then [flip(head(q)), q[1]] else [null, null]
  let one = q => {
    let s = side(q)
    let h = s[1]
    let b = s[2]
    if h === less then lt(b) else if h === less_equal then le(b) else if h === greater then 1 - le(b) else if h === greater_equal then 1 - lt(b)
    else if h === equal then (if discrete then pdf(d, b) else 0) else null
  }
  let below = q => { let s = side(q); if s[1] === greater then le(s[2]) else if s[1] === greater_equal then lt(s[2]) else null }
  let upper = q => { let s = side(q); s[1] === less or s[1] === less_equal }
  let lower = q => { let s = side(q); s[1] === greater or s[1] === greater_equal }
  let r = if match_q(c, _ and _) and length(c) == 2 then (
      if lower(c[1]) and upper(c[2]) then rewrite(one(c[2]) - below(c[1]), [erfc(z_) :> 1 - erf(z)])
      else if upper(c[1]) and lower(c[2]) then rewrite(one(c[1]) - below(c[2]), [erfc(z_) :> 1 - erf(z)]) else null)
    else one(c)
  # erf is odd: erf(-a) is -erf(a) for a numeric a > 0
  let odd = u => rewrite(u, (erf(z_) when (n(z) < 0) === true) :> -erf(-z))
  if r === null then failure(argument, probability, 1) else (if is_inexact(r) then r else simplify(odd(simplify(r))))
}

# hazard_function(d, x): the hazard rate of the distribution d at x, pdf(d, x) divided by the probability of a value
# at least x (survival(d, x) for a continuous distribution)
fn hazard_function(d, x) = {
  let discrete = match_q(d, binomial_distribution(___) | poisson_distribution(___) | geometric_distribution(___))
  simplify(pdf(d, x) / (if discrete then pdf(d, x) + survival(d, x) else survival(d, x)))
}

# fold_while_list(f, x, xs, test, m, k), k <= 0: the values fold_while_list(f, x, xs, test, m) gives without the
# last -k
fn fold_while_list(f, x, xs, test, m: Int, k: Int) when k <= 0 = drop(fold_while_list(f, x, xs, test, m), k)

# distribute(e, g, f, gp, fp): distribute(e, g, f) built with the heads gp and fp in place of g and f (the products
# are made with fp directly, so they need not be what f would give)
fn distribute(e, g, f, gp, fp) = if not is_atom(e) and head(e) === f then {
  let parts = map(x => if not is_atom(x) and head(x) === g then apply(list, x) else [x], apply(list, e))
  apply(gp, map(t => apply(fp, t), tuples(parts)))
} else e

# map_at(f, positions)(x): map_at(f, x, positions)
rule map_at(f_, pos_)(x_) := map_at(f, x, pos)

# confirm(x), confirm(x, info), confirm(x, info, tag): x, unless it is a failure or missing(...): then info is
# evaluated and failure(confirmation_failed, x) is thrown to the enclosing enclose (with that tag); a failure x that
# is not caught stays the failure it is
fn confirm(x) = if is_failure(x) or is_missing(x) then throw(failure(confirmation_failed, x), enclose) else x
fn confirm(x, held info) = if is_failure(x) or is_missing(x) then { eval(info); throw(failure(confirmation_failed, x), enclose) } else x
fn confirm(x, held info, tag) = if is_failure(x) or is_missing(x) then { eval(info); throw(failure(confirmation_failed, x), tag) } else x

# enclose(e), enclose(e, f), enclose(e, f, tag): the value of e, or the failure a confirm inside it threw (f of it;
# only throws with that tag)
fn enclose(held e) = catch(e, enclose)
fn enclose(held e, f) = catch(e, enclose, (v, t) => f(v))
fn enclose(held e, f, tag) = catch(e, tag, (v, t) => f(v))

# ---- lists: matrix builders, triangular parts, partitions, cellular automata (round 28) ----

# array_pad(a, m), (a, m, pad), (a, m, "Fixed" | "Periodic" | "Reversed" | "Reflected"): the array a with m entries
# added on every side of every level (m = [l, r]: l before, r after; m = [[l1, r1], [l2, r2], ...] per level;
# a one-element [k] is [k, k]; negative counts remove entries); the new entries are pad (0) or taken from a by the
# named rule (clamped to the edge, repeated cyclically, repeated cyclically with every other copy reversed, or
# mirrored without repeating the edge)
fn array_pad(a, m) when is_list(a) = array_pad(a, m, 0)
fn array_pad(a, m, pad) when is_list(a) = {
  let depth = length(dimensions(a))
  let one = s => if is_integer(s) then [s, s] else if length(s) == 1 then [s[1], s[1]] else [s[1], s[2]]
  let specs = if is_integer(m) then constant_array([m, m], depth)
    else if every(m, is_integer) then constant_array(one(m), depth)
    else map(s => one(s), m)
  pad_levels(a, specs, pad)
}
# the modes that continue a level by its differences (point reflection about the edge value): "Extrapolated" (the
# last difference again), "ReflectedDifferences" (the differences mirrored, the edge difference repeated),
# "ReversedDifferences" (the differences mirrored with a zero difference at every edge); the left side is the right
# side of the reversed level
fn pad_diff_mode(pad) = is_string(pad) and (pad == "Extrapolated" or pad == "ReflectedDifferences" or pad == "ReversedDifferences")
fn pad_diff_step(mode, d, len, i) = {
  let m = len - 1
  if mode == "Extrapolated" then d[m]
  else if mode == "ReflectedDifferences" then { let p = mod(i - 1, 2*m); if p < m then d[m - p] else d[p - m + 1] }
  else { let q = mod(i - 1, 2*len); if q == 0 or q == len then 0*d[1] else if q < len then d[len - q] else d[q - len] }
}
fn pad_diff_right(mode, v, cnt) = {
  let len = length(v)
  if cnt <= 0 then [] else if len == 1 then constant_array(v[1], cnt) else {
    let d = map(i => v[i + 1] - v[i], range(len - 1))
    var out = []
    var cur = v[len]
    for i in range(cnt) {
      cur = cur + pad_diff_step(mode, d, len, i)
      out = append(out, cur)
    }
    out
  }
}
fn pad_levels(a, specs, pad) = if length(specs) == 0 or not is_list(a) then a else {
  let inner = if length(specs) == 1 then a else map(x => pad_levels(x, rest(specs), pad), a)
  let l = specs[1][1]
  let r = specs[1][2]
  let len = length(inner)
  if pad_diff_mode(pad) and len > 0 then {
    let full = join(reverse(pad_diff_right(pad, reverse(inner), l)), inner, pad_diff_right(pad, inner, r))
    let from = if l < 0 then 1 - l else 1
    let upto = length(full) + (if r < 0 then r else 0)
    map(k => full[k], range(from, upto))
  } else if len == 0 and is_string(pad) then inner else pad_levels_plain(inner, l, r, len, pad)
}
fn pad_levels_plain(inner, l, r, len, pad) = {
  let src = k => if is_string(pad) then pad_source(pad, k, len) else 0
  let fill = if is_string(pad) then null else if len == 0 then pad else {
    let d = dimensions(inner[1])
    if length(d) == 0 then pad else constant_array(pad, d)
  }
  let at = k => if k >= 1 and k <= len then inner[k] else if is_string(pad) then (if pad == "ReversedNegation" and mod(floor((k - 1)/len), 2) != 0 then -inner[pad_source(pad, k, len)] else inner[pad_source(pad, k, len)]) else fill
  map(k => at(k), range(1 - l, len + r))
}
fn pad_source(mode, k, len) = {
  let c = mod(k - 1, len) + 1
  let blk = floor((k - 1)/len)
  if mode == "Fixed" then clamp(k, 1, len)
  else if mode == "Periodic" then c
  else if mode == "Reversed" or mode == "ReversedNegation" then (if mod(blk, 2) == 0 then c else len + 1 - c)
  else if len == 1 then 1
  else { let j = mod(k - 1, 2*len - 2); if j < len then j + 1 else 2*len - 1 - j }
}

# a28_matrix(rows, cols, f): the matrix of f(i, j) (lexical index names: entries may contain any symbol). A matrix
# of more than 2^26 entries is failure(limit, memory, ...) before anything is built, as for the native array
# builders (constant_array): hankel_matrix(10000) took 98 s to reach the evaluation's memory limit.
fn a28_matrix(rows, cols, f) =
  if rows*cols > 67108864 then failure(limit, memory, [rows, cols])
  else map(i => map(j => f(i, j), range(cols)), range(rows))

# hankel_matrix(n): the n x n matrix of entries i + j - 1 (0 past n); hankel_matrix(c): the matrix with first column
# c and zeros below the antidiagonal; hankel_matrix(c, r): first column c, last row r
fn hankel_matrix(c: Int) when c >= 0 = hankel_matrix(range(c))
fn hankel_matrix(c) when is_list(c) = hankel_matrix(c, join([last(c)], constant_array(0, length(c) - 1)))
fn hankel_matrix(c, r) when is_list(c) and is_list(r) = {
  let m = length(c)
  a28_matrix(m, length(r), (i, j) => if i + j - 1 <= m then c[i + j - 1] else r[i + j - m])
}

# toeplitz_matrix(n): the n x n matrix of entries |i - j| + 1; toeplitz_matrix(c): first column c, first row its
# conjugate; toeplitz_matrix(c, r): first column c, first row r
fn toeplitz_matrix(c: Int) when c >= 0 = toeplitz_matrix(range(c))
fn toeplitz_matrix(c) when is_list(c) = toeplitz_matrix(c, map(x => if is_number(x) then conjugate(x) else x, c))
fn toeplitz_matrix(c, r) when is_list(c) and is_list(r) =
  a28_matrix(length(c), length(r), (i, j) => if i >= j then c[i - j + 1] else r[j - i + 1])

# center_array(n): a 1 in the middle of n zeros; center_array(a, n), (a, n, x), (a, [n1, n2, ...], x): the array a
# (a scalar is a one-entry array; an array of lower rank than the size list is wrapped in lists) centred in an array
# of the given size filled with x (0), the extra entry going to the end; a larger array a is cut around its centre
fn center_array(m) when is_integer(m) = center_array(1, m, 0)
fn center_array(a, dims) = center_array(a, dims, 0)
fn center_array(a, dims, x) when is_integer(dims) or (is_list(dims) and every(dims, is_integer)) = {
  let d = if is_list(dims) then dims else [dims]
  # rows of different lengths: each row centred in the longest first (a full array)
  var arr = if is_list(a) and length(a) > 0 and every(a, is_list) and not is_array(a) then map(r => center_array(r, max(map(length, a)), x), a) else a
  for k in range(max(0, length(d) - length(dimensions(a)))) { arr = [arr] }
  let have = dimensions(arr)
  array_pad(arr, map(i => [floor((d[i] - have[i])/2), d[i] - have[i] - floor((d[i] - have[i])/2)], range(length(d))), x)
}

# diagonal(m), diagonal(m, k): the entries m[i, i + k] of the k-th diagonal (k > 0 above, k < 0 below the main one)
fn diagonal(m) when is_matrix(m) = diagonal(m, 0)
fn diagonal(m, k: Int) when is_matrix(m) =
  if k >= 0 then map(i => m[i][i + k], range(max(0, min(length(m), length(m[1]) - k))))
  else map(i => m[i - k][i], range(max(0, min(length(m) + k, length(m[1])))))

# upper_triangularize(m), (m, k): m with the entries below the k-th diagonal replaced by zero (0. for a matrix of
# machine reals); lower_triangularize(m, k): with the entries above the k-th diagonal replaced
fn upper_triangularize(m) when is_matrix(m) = upper_triangularize(m, 0)
fn upper_triangularize(m, k: Int) when is_matrix(m) = {
  let z = if every(flatten(m), x => is_machine_number(x)) then 0.0 else 0
  a28_matrix(length(m), length(m[1]), (i, j) => if j - i >= k then m[i][j] else z)
}
# target_structure: "Dense", "Sparse" or automatic give the dense list form, as for the matrix constructors of
# mc-linalg (round 37); other structures are not done and the call stays
fn r37_dense_structure(s) = s === "Dense" or s === "Sparse" or s === automatic
rule hankel_matrix(c_, target_structure: s_) := hankel_matrix(c) when r37_dense_structure(s)
rule hankel_matrix(c_, r_, target_structure: s_) := hankel_matrix(c, r) when r37_dense_structure(s)
rule toeplitz_matrix(c_, target_structure: s_) := toeplitz_matrix(c) when r37_dense_structure(s)
rule toeplitz_matrix(c_, r_, target_structure: s_) := toeplitz_matrix(c, r) when r37_dense_structure(s)
rule upper_triangularize(m_, target_structure: s_) := upper_triangularize(m) when r37_dense_structure(s) and is_matrix(m)
rule upper_triangularize(m_, k_, target_structure: s_) := upper_triangularize(m, k) when r37_dense_structure(s) and is_matrix(m)
rule lower_triangularize(m_, target_structure: s_) := lower_triangularize(m) when r37_dense_structure(s) and is_matrix(m)
rule lower_triangularize(m_, k_, target_structure: s_) := lower_triangularize(m, k) when r37_dense_structure(s) and is_matrix(m)
fn lower_triangularize(m) when is_matrix(m) = lower_triangularize(m, 0)
fn lower_triangularize(m, k: Int) when is_matrix(m) = {
  let z = if every(flatten(m), x => is_machine_number(x)) then 0.0 else 0
  a28_matrix(length(m), length(m[1]), (i, j) => if j - i <= k then m[i][j] else z)
}

# upper_triangular_matrix_q(m), (m, k): whether every entry of m below the k-th diagonal is zero;
# lower_triangular_matrix_q(m, k): above it; diagonal_matrix_q(m), (m, k): off the k-th diagonal
fn upper_triangular_matrix_q(m) = upper_triangular_matrix_q(m, 0)
fn upper_triangular_matrix_q(m, k: Int) = is_matrix(m) and every(range(length(m)), i => every(range(length(m[1])), j => j - i >= k or m[i][j] == 0))
fn lower_triangular_matrix_q(m) = lower_triangular_matrix_q(m, 0)
fn lower_triangular_matrix_q(m, k: Int) = is_matrix(m) and every(range(length(m)), i => every(range(length(m[1])), j => j - i <= k or m[i][j] == 0))
fn diagonal_matrix_q(m) = diagonal_matrix_q(m, 0)
fn diagonal_matrix_q(m, k: Int) = is_matrix(m) and every(range(length(m)), i => every(range(length(m[1])), j => j - i == k or m[i][j] == 0))

# minors(m), minors(m, k), minors(m, k, f): the matrix of the k x k minors of m (k = n - 1 by default for an n-row
# matrix): the determinants of the submatrices on each choice of k rows (rows of the result, in lexicographic
# order) and k columns; f is applied to each submatrix instead of det
fn minors(m) when is_matrix(m) = minors(m, length(m) - 1)
fn minors(m, k: Int) when is_matrix(m) = minors(m, k, det)
fn minors(m, k: Int, f) when is_matrix(m) and k >= 0 and k <= min(length(m), length(m[1])) = {
  let rs = subsets(range(length(m)), [k])
  let cs = subsets(range(length(m[1])), [k])
  map(r => map(c => if k == 0 then 1 else f(map(i => map(j => m[i][j], c), r)), cs), rs)
}

# adjugate(m): the transpose of the matrix of cofactors of the square matrix m, so that m . adjugate(m) = det(m) I
fn adjugate(m) when is_matrix(m) and length(m) == length(m[1]) = {
  let k = length(m)
  if k == 1 then [[1]] else a28_matrix(k, k, (i, j) => (-1)^(i + j)*det(map(r => drop_at(r, i), drop_at(m, j))))
}
fn drop_at(xs, i) = join(take(xs, i - 1), drop(xs, i))

# array_flatten(a): the matrix built from the block matrix a (a matrix of matrices; a number as a block stands for
# a constant block of the size the other blocks of its row and column give)
fn array_flatten(a, r) when r == 1 and is_list(a) = flatten(a, 1)
fn array_flatten(a, r) when r == 2 = array_flatten(a)
fn array_flatten(a) when is_list(a) and length(a) > 0 and every(a, r => is_list(r) and length(r) == length(a[1])) = {
  let h = map(r => max(map(b => if is_list(b) then length(b) else 1, r)), a)
  let w = map(j => max(map(r => if is_list(r[j]) then length(r[j][1]) else 1, a)), range(length(a[1])))
  let blk = (i, j) => if is_list(a[i][j]) then a[i][j] else constant_array(a[i][j], [h[i], w[j]])
  flatten(map(i => map(row => flatten(map(j => blk(i, j)[row], range(length(a[1]))), 1), range(h[i])), range(length(a))), 1)
}

# projection(u, v), (u, v, f): the projection of u onto v, <u, v>/<v, v> v with <a, b> = a . conjugate(b) or f(a, b)
# (any objects u and v for a given f); projection(u, vs) for a matrix vs: onto the span of its rows
fn projection(u, v) when is_list(u) and is_list(v) and not is_matrix(v) = projection(u, v, (a, b) => dot(a, conjugate(b)))
fn projection(u, vs) when is_list(u) and is_matrix(vs) = {
  let basis = orthogonalize(vs)
  if length(basis) == 0 then constant_array(0, length(u)) else total(map(e => projection(u, e), basis))
}
fn projection(u, v, f) = f(u, v)/f(v, v)*v

# orthogonalize(vs), orthogonalize(vs, f): an orthonormal basis of the span of the vectors vs by the Gram-Schmidt
# process (vectors dependent on earlier ones dropped), with <a, b> = a . conjugate(b) or f(a, b)
# symbols for the vectors (abstract vectors, round 36): Gram-Schmidt with the inner products left as dot(e, w)
# (every remainder taken as nonzero)
fn orthogonalize(vs) when is_list(vs) and length(vs) > 0 and every(vs, v => is_symbol(v)) = {
  var basis = []
  for v in vs {
    var w = v
    for e in basis { w = w - dot(e, w)*e }
    basis = append(basis, w/sqrt(dot(w, w)))
  }
  basis
}
fn orthogonalize(vs) when is_list(vs) = orthogonalize_with(vs, (a, b) => dot(a, conjugate(b)), true)
fn orthogonalize(vs, f) when is_list(vs) = orthogonalize_with(vs, f, false)
fn orthogonalize_with(vs, f, standard) = {
  var basis = []
  for v in vs {
    var w = v
    for e in basis { w = w - f(w, e)*e }
    # machine or high-precision numbers: a second sweep ("twice is enough", Giraud et al. 2005) keeps the basis
    # orthogonal to working precision however ill-conditioned the vectors are
    if length(basis) > 0 and is_inexact(total(flatten([v]))) then { for e in basis { w = w - f(w, e)*e } }
    let nn = if standard then total(map(c => abs(c)^2, w)) else f(w, w)
    let nw = if is_inexact(nn) then nn else simplify(nn)
    let zero = if is_inexact(nw) then abs(nw) < 1.0e-24*max(1, abs(if standard then total(map(c => abs(c)^2, v)) else f(v, v))) else nw === 0
    if not zero then basis = append(basis, w/sqrt(nw))
  }
  basis
}

# euclidean_distance(u, v): the Euclidean distance sqrt(|u1 - v1|^2 + ...) of two vectors (or numbers); entries that
# agree are left out, a single difference gives |u_i - v_i|
fn euclidean_distance(u, v) when is_list(u) and is_list(v) and length(u) == length(v) = {
  let diffs = select(map(i => u[i] - v[i], range(length(u))), t => not (t === 0))
  if length(diffs) == 0 then 0 else if length(diffs) == 1 then abs(diffs[1]) else sqrt(total(map(t => abs(t)^2, diffs)))
}
fn euclidean_distance(u, v) when not is_list(u) and not is_list(v) = abs(u - v)

# range_space(m): a basis (the nonzero rows of the reduced row echelon form of the transpose) of the span of the
# columns of the exact matrix m
fn range_space(m) when is_matrix(m) = { let r = row_reduce(transpose(m)); select(r, row => not every(row, x => x == 0)) }

# integer_partitions(n), (n, k), (n, [kmin, kmax]), (n, [k]), (n, [kmin, kmax, dk]), (n, k, ps), (n, k, ps, m): the
# partitions of the positive integer n into at most k parts (any number for All; exactly k for [k]; kmin to kmax
# parts in steps of dk), in decreasing lexicographic order; ps the allowed parts (positive numbers, repeatable) and
# m the number of partitions kept (the first m, or the last -m for negative m)
fn integer_partitions(m) = if is_rational(m) and not is_integer(m) then [] else integer_partitions(m, all, all, all)
fn integer_partitions(m, k) = integer_partitions(m, k, all, all)
fn integer_partitions(m, k, ps) = integer_partitions(m, k, ps, all)
fn integer_partitions(m, k, ps, cnt) when is_rational(m) and m >= 0 and (is_integer(m) or not (ps === all)) = {
  let avail = if ps === all then reverse(range(max(m, 1))) else reverse(sort(unique(ps)))
  let kmin = if k === all or is_integer(k) then 0 else k[1]
  let kmax = if k === all then m*10^6 else if is_integer(k) then k else if length(k) == 1 then k[1] else k[2]
  let kmax2 = if kmax === inf then m*10^6 else kmax
  let step = if is_list(k) and length(k) == 3 then k[3] else 1
  let cap = if ps === all then m else min(kmax2, if m == 0 then 0 else ceiling(m/min(avail)))
  let all_p = ip_gen(m, avail, min(kmax2, cap))
  let ok = p => length(p) >= kmin and length(p) <= kmax2 and mod(length(p) - kmin, step) == 0
  let sel = select(all_p, ok)
  if cnt === all then sel else if cnt >= 0 then take(sel, min(cnt, length(sel))) else take(sel, max(cnt, -length(sel)))
}
fn ip_gen(rest_sum, avail, kmax) = if rest_sum == 0 then [[]] else if kmax == 0 or length(avail) == 0 then [] else
  flatten(map(i => if avail[i] > rest_sum then [] else map(p => join([avail[i]], p), ip_gen(rest_sum - avail[i], drop(avail, i - 1), kmax - 1)), range(length(avail))), 1)

# cellular_automaton(rl, init, t): the evolution of a one-dimensional cellular automaton for t steps, the rows at
# steps 0, ..., t; rule is the rule number n (two colours, range 1), [n, k, r] (k colours, range r) or
# [n, [k, weights], r] (the neighbourhood sum of the cells times the weights is the digit of n that gives the new
# cell); init is a list of cells (a periodic row of that width) or [cells, background] (a row on an infinite
# background; the result is wide enough to hold the spread of t steps); t is a step count, [[t]] (only step t),
# [[t1, t2]] (steps t1 to t2), [t, all] (every cell the pattern can reach) or [t, [[c]]] (the column c of a single
# starting cell, c = 0 that cell, for steps 0 to t). The rule may also be a list of rules [l1, ..., lm] -> v for the
# neighbourhoods (the first that matches gives the new cell; m cells, range (m - 1)/2) or [f, [], r]: the new cell
# is f(neighbourhood, step), the step counted from 1. A range r is an integer (cells -r to r), a half integer (cells
# -r - 1/2 to r - 1/2: 1/2 is the cell and its left neighbour) or a list of offsets [[o1], [o2], ...].
fn cellular_automaton(rl, init, t) when is_list(init) and ca_spec(rl) !== null = {
  let spec = ca_spec(rl)
  let rr = max(map(o => abs(o), spec[2]))
  # a flat row of cells is periodic; [cells, background] sits on an infinite background
  let periodic = length(init) == 0 or not is_list(init[1])
  let cells = if periodic then init else init[1]
  let bg0 = if periodic or length(init) < 2 or is_list(init[2]) then 0 else init[2]
  let column = is_list(t) and length(t) == 2 and is_list(t[2]) and length(t[2]) == 1 and is_list(t[2][1]) and length(t[2][1]) == 1
  let last_step = if is_integer(t) then t else if length(t) == 1 and is_list(t[1]) then last(t[1]) else t[1]
  # [t, all]: every cell the pattern can reach, the window not cut
  let all_cells = is_list(t) and length(t) == 2 and t[2] === all
  let pad = if periodic then 0 else rr*last_step
  var state = join(constant_array(bg0, pad), cells, constant_array(bg0, pad))
  var bg = bg0
  var rows = [state]
  var bgs = [bg0]
  for s in range(last_step) {
    state = ca_step(state, spec, bg, periodic, s)
    bg = spec[1](constant_array(bg, length(spec[2])), s)
    rows = append(rows, state)
    bgs = append(bgs, bg)
  }
  # [t, [[c]]]: the column c of the (single) starting cell, step by step
  if column then map(r => r[pad + 1 + t[2][1][1]], rows) else {
    # on an infinite background the result is cut to the columns where some step differs from the background of
    # that step (the uniform background evolves by the rule too): a rule that shifts the pattern one way gives a
    # one-sided window, a rule that kills it one column; cells compare as terms, so symbolic cells work too
    if not periodic and not all_cells then {
      let diff = map(k => select(range(length(rows[k])), j => rows[k][j] !== bgs[k]), range(length(rows)))
      let used = flatten(diff)
      if length(used) > 0 then {
        let lo = min(used)
        let hi = max(used)
        rows = map(r => take(drop(r, lo - 1), hi - lo + 1), rows)
      } else rows = map(r => take(drop(r, pad), 1), rows)
    }
    if is_integer(t) or all_cells then rows
    else if length(t[1]) == 1 then rows[t[1][1] + 1]
    else slice_rows(rows, t[1][1] + 1, t[1][2] + 1)
  }
}
fn slice_rows(rows, a, b) = take(drop(rows, a - 1), b - a + 1)
# the rule as [g, offsets], g(neighbourhood, step) the new cell; null when rl is not a rule
fn ca_offsets(rr) = if is_list(rr) then (if length(rr) > 0 and is_list(rr[1]) then map(o => o[1], rr) else range(-rr[1], rr[1]))
  else if is_integer(rr) then range(-rr, rr) else range(-(rr + 1/2), rr - 1/2)
fn ca_is_rule(q) = (match_q(q, _ -> _) or match_q(q, _ :> _)) and is_list(q[1])
# a function of m cells (a pure function with m parameters, boolean_function(n, m)): the cells of a neighbourhood of
# m cells as its arguments, 0 and 1 read as false and true and the result written back as 0 or 1
fn ca_bits(nb) = every(nb, c => c === true or c === false)
fn ca_spec(rl) =
  if is_integer(rl) then ca_digits(rl, 2, [4, 2, 1], [-1, 0, 1])
  else if not is_atom(rl) and (head(rl) === `fn` or head(rl) === boolean_function) then {
    let m = if head(rl) === boolean_function then rl[2] else length(rl[1])
    let g = (nb, s) => {
      let v = apply(rl, map(c => c === 1 or c === true, nb))
      if ca_bits(nb) then v else boole(v)
    }
    if is_integer(m) and m > 0 then [g, range(-floor(m/2), m - 1 - floor(m/2))] else null
  }
  else if not is_list(rl) or length(rl) == 0 then null
  else if every(rl, ca_is_rule) then {
    let m = length(rl[1][1])
    [(nb, s) => rewrite(nb, first(select(rl, q => match_q(nb, q[1])))), range(-floor(m/2), m - 1 - floor(m/2))]
  }
  else if length(rl) >= 2 and rl[2] === [] then {
    let f = rl[1]
    [(nb, s) => f(nb, s), ca_offsets(if length(rl) < 3 then 1 else rl[3])]
  }
  else if not is_integer(rl[1]) then null
  else {
    let k = if is_list(rl[2]) then rl[2][1] else rl[2]
    let offs = ca_offsets(if length(rl) < 3 then 1 else rl[3])
    let w = if is_list(rl[2]) then (if is_list(rl[2][2]) then rl[2][2] else constant_array(rl[2][2], length(offs))) else map(j => k^(length(offs) - j), range(length(offs)))
    ca_digits(rl[1], k, w, offs)
  }
fn ca_digits(n, k, w, offs) = [(nb, s) => mod(floor(n/k^dot(w, nb)), k), offs]
fn ca_step(state, spec, bg, periodic, s) = {
  let rr = max(map(o => abs(o), spec[2]))
  let len = length(state)
  let ext = if periodic then join(take(state, -rr), state, take(state, rr)) else join(constant_array(bg, rr), state, constant_array(bg, rr))
  map(i => spec[1](map(o => ext[i + rr + o], spec[2]), s), range(len))
}

# bell_y(n, k, [x1, x2, ...]): the partial Bell polynomial B(n, k) in the x_i, the sum over the partitions of n into k
# parts of n!/(prod of (j!)^m_j m_j!) x_1^m_1 x_2^m_2 ...; bell_y(n, [x1, ...]) sums them over k
fn bell_y(m, k, xs) when is_integer(m) and is_integer(k) and is_list(xs) and m >= 0 and k >= 0 and (m == 0 or k == 0 or length(xs) >= m - k + 1) = {
  var rows = [join([1], constant_array(0, k))]
  for t in range(m) {
    let row = join([0], map(j => sum(map(i => binomial(t - 1, i - 1)*xs[i]*rows[t - i + 1][j], range(min(t - j + 1, length(xs))))), range(k)))
    rows = append(rows, row)
  }
  expand(rows[m + 1][k + 1])
}
fn bell_y(m, xs) when is_integer(m) and is_list(xs) and m >= 0 and length(xs) >= m = sum(map(k => bell_y(m, k, xs), range(0, m)))

# bell_y(m), m a matrix of n rows with one column [x1, ..., xn] (the complete Bell polynomial) or two columns [ai, xi]
# (sum over k of a_k B(n, k, x))
fn bell_y(mat) when is_matrix(mat) and length(mat[1]) == 1 = bell_y(length(mat), map(r => r[1], mat))
# a matrix of m >= 2 columns is the composition of m functions (Faa di Bruno): the first column holds the derivatives a_k
# of the outermost one, and each further column c turns the sequence v_k into sum over j <= k of v_j B(k, j, c)
fn bell_y(mat) when is_matrix(mat) and length(mat[1]) >= 2 = {
  let n = length(mat)
  var v = map(r => r[1], mat)
  for c in range(2, length(mat[1])) {
    let ys = map(r => r[c], mat)
    v = map(k => expand(sum(map(j => v[j]*bell_y(k, j, ys), range(k)))), range(n))
  }
  v[n]
}

# cube_root(x): the real cube root of a real number x (the principal root of other numbers)
fn cube_root(x) when is_number(x) = if is_real(x) then surd(x, 3) else x^(1/3)
fn cube_root(x) when is_list(x) = map(cube_root, x)
# a closed real expression (1/2 - sqrt(15)/3): the real root, by surd
fn cube_root(x) when not is_number(x) and is_numeric(x) and is_real(n(x)) = surd(x, 3)
# its derivatives: those of surd(x, 3) = x^(1/3) for x > 0 (1/(3 cube_root(x)^2), ...), written in cube_root
rule derivative(k_)(cube_root)(x_) := rewrite(rewrite(d(surd(pl_cr_t, 3), [pl_cr_t, k]), pl_cr_t -> x), surd(pl_cr_u_, 3) :> cube_root(pl_cr_u)) when is_integer(k) and k > 0
fn cube_root(x) when x === inf or x === -inf = x

# real_exponent(x), real_exponent(x, b): log(|x|)/log(b) (base 10) as a machine number; -inf for the exact 0, minus the
# accuracy (in that base) for an inexact zero (machine 0. or a ball that contains 0)
fn real_exponent(x) when is_numeric(x) or is_list(x) = real_exponent(x, 10)
fn real_exponent(x, b) when is_list(x) = map(t => real_exponent(t, b), x)
fn real_exponent(x, b) when is_list(b) = map(c => real_exponent(x, c), b)
fn real_exponent(x, b) when is_numeric(x) and is_numeric(b) and not is_list(x) and not is_list(b) = {
  let zero = x == 0
  if zero === true and is_exact(x) then -inf
  else if is_inexact(x) and not (zero === false) then -accuracy(x)*n(log(10)/log(b))
  else if is_machine_number(x) then n(n(log(set_precision(abs(x), 30))/log(n(b, 30)), 30))
  else if is_inexact(x) then n(n(log(abs(x))/log(b), 10))
  else n(n(log(n(abs(x), 30))/log(n(b, 30)), 30))
}

# number_decompose(x, [u1, u2, ..., un]): [c1, ..., c(n-1), r]: x written as c1 u1 + ... + c(n-1) u(n-1) + r un with
# integer counts c_i (truncated toward zero) taken greedily
fn number_decompose(x, us) when is_list(us) and length(us) > 0 = {
  var rem = x
  var out = []
  for u in most(us) {
    let q = trunc_toward_zero(rem/u)
    out = append(out, q)
    rem = rem - q*u
  }
  append(out, rem/last(us))
}
fn trunc_toward_zero(q) = if n(q) >= 0 then floor(q) else ceiling(q)

# integer_string(n), (n, b), (n, b, len): the digits of |n| in base b (10) as a string (a-z for digits 10 to 35),
# padded with zeros on the left to len characters or cut to the last len
fn integer_string(m) when is_integer(m) = integer_string(m, 10)
fn integer_string(m, b) when is_integer(m) and is_integer(b) and b >= 2 and b <= 36 = {
  let alphabet = characters("0123456789abcdefghijklmnopqrstuvwxyz")
  string_join(map(d => alphabet[d + 1], integer_digits(abs(m), b)))
}
fn integer_string(m, style) when is_integer(m) and m > 0 and m < 4000 and style == "Roman" = {
  let vals = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1]
  let syms = ["M", "CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I"]
  var left = m
  var out = []
  for k in range(13) {
    while left >= vals[k] { out = append(out, syms[k]); left = left - vals[k] }
  }
  string_join(out)
}
fn integer_string(m, b, len: Int) when is_integer(m) and is_integer(b) = {
  let s = integer_string(m, b)
  let l = string_length(s)
  if l >= len then string_take(s, -len) else string_join(join(constant_array("0", len - l), [s]))
}

# indexed(x, i), indexed(x, [i1, i2, ...]): the part of the list x at the index (the part along the path of
# indices); it stays indexed(x, i) for a symbol x or a symbolic index
rule indexed(x_, i_) := x[i] when is_list(x) and is_integer(i) and abs(i) >= 1 and abs(i) <= length(x)
rule indexed(x_, i_) := extract(x, i) when is_list(x) and is_list(i) and length(i) > 0 and every(i, k => is_integer(k) and k != 0) and length(dimensions(x)) >= length(i) and is_valid_path(x, i)
fn is_valid_path(x, path) = if length(path) == 0 then true else is_list(x) and abs(path[1]) <= length(x) and is_valid_path(x[path[1]], rest(path))

# integer_name(n): the English name of the integer n: words below 1000 ("one hundred twenty-three"), digits and
# scale names above ("1 million 234 thousand 567"); integer_name(n, "Words") spells every group; a list of
# integers gives a list
fn integer_name(m) when is_integer(m) = integer_name(m, "Short")
fn integer_name(ms) when is_list(ms) = map(integer_name, ms)
fn integer_name(m, style) when is_integer(m) = {
  let ones = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"]
  let tens = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"]
  let scales = ["", "thousand", "million", "billion", "trillion", "quadrillion", "quintillion", "sextillion", "septillion", "octillion", "nonillion", "decillion", "undecillion", "duodecillion", "tredecillion", "quattuordecillion", "quindecillion", "sexdecillion", "septendecillion", "octodecillion", "novemdecillion", "vigintillion"]
  let below_hundred = k => if k < 20 then ones[k + 1] else if mod(k, 10) == 0 then tens[quotient(k, 10) + 1] else string_join([tens[quotient(k, 10) + 1], "‐", ones[mod(k, 10) + 1]])
  let below_thousand = k => if k < 100 then below_hundred(k) else if mod(k, 100) == 0 then string_join([ones[quotient(k, 100) + 1], " hundred"]) else string_join([ones[quotient(k, 100) + 1], " hundred ", below_hundred(mod(k, 100))])
  let a = abs(m)
  let sign_word = if m < 0 then "negative " else ""
  if a < 1000 then string_join([sign_word, below_thousand(a)]) else {
    let groups = reverse(integer_digits(a, 1000))
    let parts = select(map(g => [g, groups[g]], range(length(groups))), p => p[2] != 0)
    let render = p => if style == "Words" then string_join([below_thousand(p[2]), if p[1] > 1 then string_join([" ", scales[p[1]]]) else ""]) else string_join([to_string(p[2]), if p[1] > 1 then string_join([" ", scales[p[1]]]) else ""])
    string_join([sign_word, string_join(riffle(map(render, reverse(parts)), if style == "Words" then ", " else " "))])
  }
}

# find_sequence_function(xs, n): a closed form f(n) with f(k) = xs[k] for k = 1, 2, ...: tries c*g(n) for the
# bases g = 1, n!, fibonacci(n), (-1)^n, 2^n, 3^n, 4^n, 5^n with c a polynomial, a polynomial over n or over
# n + 1 (at least two terms must be left over to confirm the fit)
fn find_sequence_function(xs, x) when is_list(xs) and length(xs) >= 3 and is_symbol(x) = {
  let m = length(xs)
  let bases = [[1, k => 1], [factorial(x), k => factorial(k)], [fibonacci(x), k => fibonacci(k)], [(-1)^x, k => (-1)^k], [2^x, k => 2^k], [3^x, k => 3^k], [4^x, k => 4^k], [5^x, k => 5^k]]
  let dens = [[1, k => 1], [x, k => k], [x + 1, k => k + 1]]
  var found = null
  for d in dens {
    for b in bases {
      if found === null then {
        let seq = map(k => xs[k]*d[2](k)/b[2](k), range(m))
        let p = seq_poly_fit(seq, x)
        if not (p === null) then found = b[1]*p/d[1]
      }
    }
  }
  if found === null then {
    let d = map(k => xs[k + 1] - xs[k], range(m - 1))
    if every(d, t => not (simplify(t) === 0)) then {
      let r = simplify(d[2]/d[1])
      if every(range(m - 2), k => simplify(d[k + 1] - r*d[k]) === 0) and not (simplify(r - 1) === 0) then found = simplify(xs[1] + d[1]*(r^(x - 1) - 1)/(r - 1))
    }
  }
  if found === null and m >= 6 then found = seq_catalogue(xs, x)
  if found === null then failure(no_sequence_function, xs) else found
}
# the integer functions of the library whose values at 1, ..., m are the sequence (all m terms checked, at least
# six; the first in this order that fits): number-theoretic and combinatorial functions, divisor_sigma(k, n) and
# squares_r(k, n) for small k, and mod(n, k) for a period k that repeats at least twice in the data
fn seq_catalogue(xs, x) = {
  let m = length(xs)
  let fs = join([prime, prime_pi, euler_phi, carmichael_lambda, moebius_mu, liouville_lambda, prime_nu, prime_omega,
    partitions_p, partitions_q, bell_b, catalan_number, euler_e, finite_abelian_group_count, finite_group_count],
    map(k => (j => divisor_sigma(k, j)), range(0, 3)), map(k => (j => squares_r(k, j)), range(2, 4)),
    map(k => (j => mod(j, k)), range(2, floor(m/2))))
  var found = null
  for f in fs {
    if found === null and every(range(m), j => f(j) === xs[j]) then found = f(x)
  }
  found
}
fn seq_poly_fit(seq, x) = {
  let m = length(seq)
  var out = null
  for deg in range(0, m - 3) {
    if out === null then {
      let diffs = differences(seq, deg + 1)
      if every(diffs, v => v == 0) then {
        out = expand(sum(map(k => (if k == 0 then seq[1] else first(differences(seq, k)))*product(map(j => x - 1 - j, range(0, k - 1)))/factorial(k), range(0, deg))))
      }
    }
  }
  out
}

# linear_optimization(f, cons, vars), linear_fractional_optimization(f, cons, vars), convex_optimization(f, cons,
# vars): the point [x -> ..., y -> ...] minimizing f under the constraints cons (a list of conditions, read as
# their conjunction); the problem goes to minimize, so the objective may be linear, a ratio of linear functions or
# convex; vars is a variable or a list of them. A vector variable v (the objective dot(c, v)) gives [v -> [..]];
# the conditions may be vector_less_equal([a, b, ...]), vector_greater_equal(...) and equalities of lists.
# linear_optimization(c, [a, b]), (c, [a, b], [aeq, beq]): the vector x minimizing dot(c, x) under
# dot(a, x) + b >= 0 (and dot(aeq, x) + beq == 0)
rule linear_optimization(c_, ab_) := lo_matrix(c, ab, [[], []]) when is_vector(c) and lo_pair(ab)
rule linear_optimization(c_, ab_, eq_) := lo_matrix(c, ab, eq) when is_vector(c) and lo_pair(ab) and lo_pair(eq)
rule linear_optimization(f_, cons_, vars_) := c30_opt(f, cons, vars) when not is_vector(f)
rule linear_fractional_optimization(f_, cons_, vars_) := c30_opt(f, cons, vars)
rule convex_optimization(f_, cons_, vars_) := c30_opt(f, cons, vars)
fn lo_pair(ab) = is_list(ab) and length(ab) == 2 and is_list(ab[1]) and is_list(ab[2]) and (length(ab[1]) == 0 or is_matrix(ab[1]))
fn lo_matrix(c, ab, eq) = {
  let nvar = length(c)
  let xs = map(k => to_expression(string_join(["lo_x", to_string(k)])), range(nvar))
  let ineq = map(k => dot(ab[1][k], xs) + ab[2][k] >= 0, range(length(ab[1])))
  let equ = map(k => dot(eq[1][k], xs) + eq[2][k] == 0, range(length(eq[1])))
  let all_c = join(ineq, equ)
  let r = minimize([dot(c, xs), if length(all_c) == 0 then true else fold((a, b) => a and b, all_c[1], rest(all_c))], xs)
  if is_list(r) and length(r) == 2 and is_list(r[2]) then map(t => t[2], r[2]) else r
}
fn lo_scalars(c) = {
  let cmp = (a, b, op) => lo_cmp(a, b, op)
  if is_atom(c) then [c]
  else if head(c) === vector_less_equal and length(c) == 1 and is_list(c[1]) then flatten(map(k => cmp(c[1][k], c[1][k + 1], (a, b) => a <= b), range(length(c[1]) - 1)), 1)
  else if head(c) === vector_greater_equal and length(c) == 1 and is_list(c[1]) then flatten(map(k => cmp(c[1][k], c[1][k + 1], (a, b) => a >= b), range(length(c[1]) - 1)), 1)
  else if head(c) === equal and length(c) == 2 and (is_list(c[1]) or is_list(c[2])) then cmp(c[1], c[2], (a, b) => a == b)
  else [c]
}
fn optimization_point(f, cons, vars) = {
  let cl = if is_list(cons) then cons else [cons]
  let parts = flatten(map(lo_scalars, cl), 1)
  let vector_var = not is_list(vars) and not is_atom(vars) == false and false
  let vs = if is_list(vars) then vars else [vars]
  let all_vector = length(vs) == 1 and is_symbol(vs[1]) and not free_of([f, parts], vs[1]) and lo_is_vector_use(f, vs[1])
  if all_vector then {
    let dim = lo_dimension([f, parts], vs[1])
    let xs = map(k => to_expression(string_join(["lo_v", to_string(k)])), range(dim))
    let r = optimization_point(rewrite(f, vs[1] -> xs), rewrite(cl, vs[1] -> xs), xs)
    if is_list(r) and every(r, t => match_q(t, _ -> _)) then [vs[1] -> map(t => t[2], r)] else r
  } else {
    let c = if length(parts) == 0 then true else fold((a, b) => a and b, parts[1], rest(parts))
    let r = minimize([f, c], vs)
    if is_list(r) and length(r) == 2 and is_list(r[2]) then r[2] else r
  }
}
fn lo_is_vector_use(f, v) = not free_of(f, dot) or not free_of(f, total) or not free_of(f, indexed)
fn lo_dimension(e, v) = {
  let found = cases(e, dot(u_, w_) :> length(u) when is_list(u) and w === v, [0, inf])
  let found2 = cases(e, dot(w_, u_) :> length(u) when is_list(u) and w === v, [0, inf])
  let all_d = join(found, found2)
  if length(all_d) > 0 then all_d[1] else 1
}

# machine_precision: the precision of machine numbers, 53 log10(2) = 15.954589770191003...
rule machine_precision := 15.954589770191003


# numeric_array_values(xs, type), (xs, type, mode): the entries of numeric_array(xs, type[, mode]) as normal gives
# them: integers kept to the range of an integer type (mode "ClipAndCoerce" clips to it, "Round" rounds real
# entries first), real types as machine reals (no single-precision rounding yet)
# na_single(x): the machine number (or complex number of machine parts) x rounded to the nearest single-precision
# value (24 significant bits, ties to even)
fn na_single(x) = if is_real(x) then na_single_real(x) else na_single_real(re(x)) + im*na_single_real(im_part(x))
fn na_single_real(x) = if not is_real(x) or x == 0 or abs(x) > 3.4e38 or abs(x) < 1.2e-38 then x else {
  var e = floor(log(abs(x))/log(2))
  if 2^e > abs(x) then e = e - 1
  if 2^(e + 1) <= abs(x) then e = e + 1
  let scale = 2^(e - 23)
  n(round(rationalize(x, 0)/scale)*scale)
}
fn numeric_array_values(xs, ty, mode) when is_list(xs) = map(x => numeric_array_values(x, ty, mode), xs)
fn numeric_array_values(xs, ty) when is_list(xs) = map(x => numeric_array_values(x, ty, "Coerce"), xs)
fn numeric_array_values(x, ty) = numeric_array_values(x, ty, "Coerce")
fn numeric_array_values(x, ty, mode) when not is_list(x) = {
  let bits = if string_contains(ty, "8") then 8 else if string_contains(ty, "16") then 16 else if string_contains(ty, "32") then 32 else 64
  let unsigned = string_contains(ty, "Unsigned")
  let is_int_type = string_contains(ty, "Integer")
  if not is_int_type then (if bits == 32 then na_single(n(x)) else n(x)) else {
    let v = if mode == "Round" then round(x) else x
    let lo = if unsigned then 0 else -2^(bits - 1)
    let hi = if unsigned then 2^bits - 1 else 2^(bits - 1) - 1
    if mode == "ClipAndCoerce" or mode == "Round" then clamp(v, lo, hi) else v
  }
}
