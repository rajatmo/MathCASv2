#! mathcas 2
# The language library: core-language functions written in MathCAS itself, evaluated after the
# prelude when the engine starts. Everything here is in the bare namespace. Rules as for the prelude: never define a
# symbol that has a native implementation. Every public function is listed in `FUNCTIONS` (src/lib.rs) and has a page
# docs/functions/<name>.mcn.

# --- threaded and elementwise ---

# threaded(v), threaded(v, n): v as an operand that `elementwise` (and a listable call that supports it) pairs with a
# trailing run of the levels of the other arguments (n omitted), or with the levels from level n on (n > 0), or
# from level r + n + 1 on (n < 0, r the rank of the array they are threaded against); inert by itself

# the levels (1-based, first and last) of an array of rank r that threaded(v, n) of rank k covers; a failure when v
# does not fit
fn threaded_window(r: Int, k: Int, n) = {
  let s = if n === null then r - k + 1 else if n > 0 then n else r + n + 1
  if k == 0 or s < 1 or s + k - 1 > r then failure(threaded_level, n) else [s, s + k - 1]
}

# v spread over an array of dimensions dims: the element at [i1, ..., ir] is the one of v at [i_s, ..., i_e]
fn threaded_tile(v, dims, lvl: Int, s: Int, e: Int) =
  if lvl > length(dims) then v
  else if lvl < s or lvl > e then constant_array(threaded_tile(v, dims, lvl + 1, s, e), dims[lvl])
  else map(x => threaded_tile(x, dims, lvl + 1, s, e), v)

# the arguments xs with every threaded(v) or threaded(v, n) spread over the dimensions of the arrays among them
# (over the longest threaded value when there are none)
fn threaded_expand(xs) = {
  let is_th = x => not is_atom(x) and head(x) === threaded
  let arrays = select(xs, x => is_list(x))
  let shapes = if length(arrays) > 0 then map(dimensions, arrays) else map(x => dimensions(x[1]), select(xs, is_th))
  let dims = last(sort_by(shapes, d => length(d)))
  var out = []
  var bad = null
  for x in xs {
    if is_th(x) and bad === null then {
      let v = x[1]
      let vd = dimensions(v)
      let w = threaded_window(length(dims), length(vd), if length(x) > 1 then x[2] else null)
      if is_failure(w) then { bad = w; out = append(out, x) }
      else if take(drop(dims, w[1] - 1), length(vd)) != vd then { bad = failure(shape_mismatch); out = append(out, x) }
      else out = append(out, threaded_tile(v, dims, 1, w[1], w[2]))
    } else out = append(out, x)
  }
  if bad === null then out else bad
}

fn elementwise_thread(f, xs) =
  if some(xs, is_list) then {
    let ls = select(xs, is_list)
    let n = length(ls[1])
    if some(ls, l => length(l) != n) then failure(shape_mismatch)
    else map(i => elementwise_thread(f, map(x => if is_list(x) then x[i] else x, xs)), range(n))
  } else if some(xs, is_assoc) then {
    let asc = select(xs, is_assoc)
    let ks = keys(asc[1])
    if some(asc, a => keys(a) !== ks) then failure(shape_mismatch)
    else assoc(map(k => k -> elementwise_thread(f, map(x => if is_assoc(x) then x[k] else x, xs)), ks))
  } else apply(f, xs)

# elementwise(f)(a, b, ...): f applied to the corresponding elements of its arguments: lists (and the values of
# associations) are paired element by element at every level, other arguments are used with each element, and a
# threaded(v) argument is spread over the levels of the arrays first (the result of threaded arguments only is
# threaded again)
fn elementwise_apply(f, xs) = {
  let is_th = x => not is_atom(x) and head(x) === threaded
  if not some(xs, is_th) then elementwise_thread(f, xs)
  else {
    let ex = threaded_expand(xs)
    if is_failure(ex) then ex
    else if some(xs, is_list) then elementwise_thread(f, ex)
    else threaded(elementwise_thread(f, ex))
  }
}
rule elementwise(f_)(xs___) := elementwise_apply(f, xs)

# --- join_across ---

# the key column names of a join: a key, a rule lk -> rk or a list of them, as a list of [lk, rk] (key(k) is k)
fn join_unkey(k) = if match_q(k, key(_)) then k[1] else k
fn join_pairs(key) = map(k => if match_q(k, _ -> _) then [join_unkey(k[1]), join_unkey(k[2])] else [join_unkey(k), join_unkey(k)], if is_list(key) then key else [key])

# the join type as "inner", "left", "right" or "outer" (a string in any case, or a symbol), else null
fn join_type(t) = {
  let s = if is_string(t) then to_lower(t) else if is_symbol(t) then to_lower(to_string(t)) else null
  if s === null or not member(["inner", "left", "right", "outer"], s) then null else s
}

# the keys of the records xs, in order of first appearance
fn join_schema(xs) = unique(flatten(map(keys, xs), 1))

# the entries of the joined record of l and r: shared-name join keys once, other keys of both resolved by f
fn join_record(l, r, shared, f) = {
  var out = []
  for k in keys(l) {
    if member(shared, k) or not key_exists(r, k) then out = append(out, k -> l[k])
    else if f === automatic or f === left then out = append(out, k -> l[k])
    else if f === right then out = append(out, k -> r[k])
    else { let p = f(k); out = join(out, [p[1] -> l[k], p[2] -> r[k]]) }
  }
  for k in keys(r) { if not key_exists(l, k) then out = append(out, k -> r[k]) }
  assoc(out)
}

# join_across(left, right, key), (..., type), (..., key_collision_function: f): the records of two lists of
# associations (or two datasets) joined on a key: a key present in both, a rule lk -> rk, or a list of them; the type
# "inner" (default), "left", "right" or "outer" decides whether records without a partner are kept (their other
# keys missing("Unmatched")); a key present in both records goes to the result with the left value, or by
# key_collision_function: left, right, or a function of the key giving the two new keys [kl, kr]
fn join_across(l, r, key, type = "inner"; key_collision_function = automatic) when is_dataset(l) and is_dataset(r) = {
  let t = join_type(type)
  if t === null then failure(argument, join_across, 4) else join_on(l, r, key, how: (if t === "inner" then inner else if t === "left" then left else if t === "right" then right else outer))
}
fn join_across(l, r, key, type = "inner"; key_collision_function = automatic) when is_list(l) and is_list(r) = {
  let t = join_type(type)
  if t === null then failure(argument, join_across, 4)
  else if not every(l, is_assoc) then failure(argument, join_across, 1)
  else if not every(r, is_assoc) then failure(argument, join_across, 2)
  else join_records(l, r, join_pairs(key), t, key_collision_function)
}

# the joined records of the lists of associations l and r on the key pairs ps, for the join type t
fn join_records(l, r, ps, t, f) = {
  let shared = map(p => p[1], select(ps, p => p[1] === p[2]))
  let lk = map(p => p[1], ps)
  let rk = map(p => p[2], ps)
  let tup_of = (a, ks) => if every(ks, k => key_exists(a, k)) then map(k => a[k], ks) else null
  let ls = join_schema(l)
  let rs = join_schema(r)
  let rtuples = map(a => tup_of(a, rk), r)
  let idx = group_by(select(range(length(r)), i => rtuples[i] !== null), i => rtuples[i])
  var used = constant_array(false, length(r))
  var out = []
  for a in l {
    let tp = tup_of(a, lk)
    let hits = if tp === null or not key_exists(idx, tp) then [] else idx[tp]
    for i in hits {
      used = replace_part(used, [i] -> true)
      out = append(out, join_record(a, r[i], shared, f))
    }
    if length(hits) == 0 and (t === "left" or t === "outer") then {
      let none = assoc(map(k => k -> (if member(shared, k) and key_exists(a, k) then a[k] else missing("Unmatched")), rs))
      out = append(out, join_record(a, none, shared, f))
    }
  }
  if t === "right" or t === "outer" then {
    for i in range(length(r)) {
      if not used[i] then {
        let b = r[i]
        let none = assoc(map(k => k -> (if member(shared, k) and key_exists(b, k) then b[k] else missing("Unmatched")), ls))
        out = append(out, join_record(none, b, shared, f))
      }
    }
  }
  out
}

# --- Boolean functions: boolean_function, boolean_convert, boolean_minimize, logical_expand ---

# The truth tables here are lists of true/false read by the assignment taken as a binary number (false 0, true 1,
# the first variable the most significant bit): entry i + 1 of a table is the value at index i. (boolean_table lists
# the same values the other way round, all true first.) A function of n variables is also its index
# k = sum of 2^i over the true entries, as in boolean_function(k, n).

# the symbols of a Boolean expression, in canonical order (in order of appearance for a boolean_function call)
fn bool_vars(e) = {
  let vs = select(unique(level(e, [-1])), v => is_symbol(v) and v !== true and v !== false)
  # the arguments of a call of a Boolean function keep their order
  if not is_atom(e) and not is_atom(head(e)) and head(head(e)) === boolean_function then vs else sort(vs)
}

# the truth table of e in the variables vars, or null when some assignment does not give true or false
fn bool_table(e, vars) = {
  let t = boolean_table(e, vars)
  if every(t, b => b === true or b === false) then reverse(t) else null
}

# the number n of variables of a truth table of length 2^n
fn bool_arity(t) = length(integer_digits(length(t), 2)) - 1

# the index of a truth table
fn bool_index(t) = sum(map(i => if t[i + 1] === true then 2^i else 0, range(0, length(t) - 1)))

# the truth table of the index k of n variables
fn bool_from_index(k, n) = map(b => b == 1, reverse(integer_digits(k, 2, 2^n)))

# the prime implicants [value, mask] (mask bits are dashes) of the minterms (indices) terms, by Quine-McCluskey
fn bool_primes(terms, n) = {
  var cur = map(m => [m, 0], terms)
  var primes = []
  while length(cur) > 0 {
    let present = assoc(map(p => p -> true, cur))
    var next = []
    var rest = []
    for p in cur {
      var paired = false
      for b in range(0, n - 1) {
        let bit = 2^b
        if bit_and(p[2], bit) == 0 then {
          if key_exists(present, [bit_xor(p[1], bit), p[2]]) then {
            paired = true
            if bit_and(p[1], bit) == 0 then next = append(next, [p[1], p[2] + bit])
          }
        }
      }
      if not paired then rest = append(rest, p)
    }
    primes = join(primes, rest)
    cur = unique(next)
  }
  primes
}

# does the implicant p cover the minterm m of n variables?
fn bool_covers(p, m, n) = bit_and(bit_xor(m, p[1]), 2^n - 1 - p[2]) == 0

# the number of literals of an implicant of n variables
fn bool_size(p, n) = count_if(range(0, n - 1), b => bit_and(p[2], 2^b) == 0)

# a cover of the minterms ones by prime implicants: the essential ones, then greedily the one covering most
fn bool_cover(primes, ones, n) = {
  let cov = map(p => select(ones, m => bool_covers(p, m, n)), primes)
  var chosen = []
  var left = ones
  for m in ones {
    let ps = select(range(length(primes)), i => member(cov[i], m))
    if length(ps) == 1 and not member(chosen, ps[1]) then chosen = append(chosen, ps[1])
  }
  for i in chosen { left = select(left, m => not member(cov[i], m)) }
  while length(left) > 0 {
    let free = select(range(length(primes)), i => not member(chosen, i))
    let best = last(sort_by(free, i => [length(select(left, m => member(cov[i], m))), -bool_size(primes[i], n), -i]))
    chosen = append(chosen, best)
    left = select(left, m => not member(cov[best], m))
  }
  map(i => primes[i], sort(chosen))
}

# the literals of an implicant over vars: the variable where the bit is 1, its negation where 0, nothing at a dash
fn bool_literals(p, vars) = {
  let n = length(vars)
  var out = []
  for j in range(0, n - 1) {
    let bit = 2^(n - 1 - j)
    if bit_and(p[2], bit) == 0 then out = append(out, if bit_and(p[1], bit) == 0 then not vars[j + 1] else vars[j + 1])
  }
  out
}

# the small sum of products of the function with minterms ones, free at the minterms dc: a list of lists of
# literals ([] is false, [[]] is true)
fn bool_sop(ones, dc, vars) = {
  let n = length(vars)
  if length(ones) == 0 then [] else bool_sort_terms(map(p => bool_literals(p, vars), bool_cover(bool_primes(join(ones, dc), n), ones, n)), vars)
}

# the terms (lists of literals in the order of vars) in lexicographic order of their literals, a variable before its
# negation and a variable before the next ones (so a before not a before b), a term before its extensions
fn bool_sort_terms(terms, vars) = {
  let n = length(vars)
  let base = 2*n + 2
  let key = t => fold((acc, l) => acc*base + 1 + 2*(position_of(vars, if match_q(l, not(_)) then l[1] else l)) + (if match_q(l, not(_)) then 1 else 0), 0, join(t, constant_array(-1, 0))) * base^(n - length(t))
  sort_by(terms, key)
}

# the negation of a literal
fn bool_neg(l) = if match_q(l, not(_)) then l[1] else not l

# the expression of a sum of products and the product of sums of the negated terms
fn bool_or(terms) = apply(`or`, map(t => apply(`and`, t), terms))
fn bool_and_of_or(terms) = apply(`and`, map(t => apply(`or`, map(bool_neg, t)), terms))

# the minterms of e under the condition cond (null: none) over vars: [ones, zeros, dont-cares], or null
fn bool_spec(e, cond, vars) = {
  let t = bool_table(e, vars)
  let c = if cond === null then constant_array(true, 2^length(vars)) else bool_table(cond, vars)
  if t === null or c === null then null
  else {
    let all = range(0, 2^length(vars) - 1)
    [select(all, i => t[i + 1] and c[i + 1]), select(all, i => not t[i + 1] and c[i + 1]), select(all, i => not c[i + 1])]
  }
}

# the algebraic normal form of a truth table: the exclusive or of the products of the variables of the subsets whose
# coefficient is 1 (Moebius transform over GF(2))
fn bool_anf(t, vars) = {
  let n = length(vars)
  var c = map(b => if b then 1 else 0, t)
  for b in range(0, n - 1) {
    let bit = 2^b
    let prev = c
    c = map(i => if bit_and(i, bit) != 0 then mod(prev[i + 1] + prev[i + 1 - bit], 2) else prev[i + 1], range(0, length(prev) - 1))
  }
  let monos = map(i => apply(`and`, select(vars, v => bit_and(i, 2^(n - 1 - position_of(vars, v))) != 0)), select(range(0, length(c) - 1), i => c[i + 1] == 1))
  if length(monos) == 0 then false else if length(monos) == 1 then monos[1] else apply(xor, monos)
}
fn position_of(xs, x) = first(first(position(xs, x))) - 1

# an expression of and, or, not rewritten with only the binary g (nand or nor)
fn bool_binary(e, g) = {
  let un = x => g(x, x)
  let is_nand = g(true, false) === true
  if is_atom(e) then e
  else if head(e) === `not` then un(bool_binary(e[1], g))
  else if head(e) === `and` or head(e) === `or` then {
    let xs = map(x => bool_binary(x, g), apply(list, e))
    let anded = head(e) === `and`
    let op = if anded == is_nand then (p, q) => un(g(p, q)) else (p, q) => g(un(p), un(q))
    fold(op, xs[1], rest(xs))
  } else e
}

# the form name as a lower case string, or null
fn bool_form(f) = {
  let s = if is_string(f) then to_lower(f) else if is_symbol(f) then to_lower(to_string(f)) else null
  let t = if s === null then null else string_replace(s, "_" -> "")
  if t !== null and member(["dnf", "cnf", "anf", "esop", "nand", "nor", "binarynand", "binarynor", "and", "or", "implies", "bff", "ite", "booleanfunction"], t) then (if t === "booleanfunction" then "bff" else t) else null
}

# a product of literals with only implies and not, and a sum of products
fn bool_imp_and(ls) = if length(ls) == 1 then ls[1] else not implies(ls[1], bool_neg(bool_imp_and(rest(ls))))
fn bool_imp_or(ts) = if length(ts) == 1 then ts[1] else implies(bool_neg(ts[1]), bool_imp_or(rest(ts)))

# the if-then-else tree of a truth table: the first variable chooses between the trees of the two halves (the
# variables are dropped where both halves agree)
fn bool_ite(t, vars) =
  if length(vars) == 0 then t[1]
  else {
    let h = length(t)/2
    let hi = bool_ite(drop(t, h), rest(vars))
    let lo = bool_ite(take(t, h), rest(vars))
    if hi === lo then hi else apply(`if`, [vars[1], hi, lo])
  }

# the form of the function with these minterm sets over vars, as the string f names it, or null if f is unknown
fn bool_form_of(f, ones, zeros, dc, vars) = {
  let dnf = () => bool_or(bool_sop(ones, dc, vars))
  let cnf = () => bool_and_of_or(bool_sop(zeros, dc, vars))
  if f === "dnf" then dnf()
  else if f === "cnf" then cnf()
  else if f === "anf" or f === "esop" then bool_anf(map(i => member(ones, i), range(0, 2^length(vars) - 1)), vars)
  else if f === "nand" then {
    let ts = bool_sop(ones, dc, vars)
    if length(ts) == 0 then false else apply(nand, map(t => if length(t) == 1 then bool_neg(t[1]) else apply(nand, t), ts))
  }
  else if f === "nor" then {
    let ts = bool_sop(zeros, dc, vars)
    if length(ts) == 0 then true else apply(nor, map(t => if length(t) == 1 then t[1] else apply(nor, map(bool_neg, t)), ts))
  }
  else if f === "binarynand" then bool_binary(dnf(), nand)
  else if f === "binarynor" then bool_binary(cnf(), nor)
  else if f === "and" then apply(`and`, map(t => if length(t) == 1 then bool_neg(t[1]) else not apply(`and`, t), bool_sop(zeros, dc, vars)))
  else if f === "or" then apply(`or`, map(t => if length(t) == 1 then t[1] else not apply(`or`, map(bool_neg, t)), bool_sop(ones, dc, vars)))
  else if f === "implies" then {
    let ts = bool_sop(ones, dc, vars)
    if length(ts) == 0 then false else if some(ts, t => length(t) == 0) then true else bool_imp_or(map(bool_imp_and, ts))
  }
  else if f === "ite" then bool_ite(map(i => member(ones, i), range(0, 2^length(vars) - 1)), vars)
  else if f === "bff" then apply(boolean_function(bool_index(map(i => member(ones, i), range(0, 2^length(vars) - 1))), length(vars)), vars)
  else null
}

# boolean_convert(e), (e, form), (e, form, cond), (e, cond): the Boolean expression e (in symbols; a boolean_function
# call too) in the form "dnf" (the default: a sum of products, by Quine-McCluskey with a greedy cover), "cnf", "anf"
# or "esop" (exclusive or of products), "nand", "nor", "binary_nand", "binary_nor" (only those operations), "and",
# "or", "implies" (with not) or "bff" (a boolean_function call); with cond the result need only agree with e where
# cond is true. Strings in any case, or symbols.
fn boolean_convert(e) = boolean_convert(e, "dnf", null)
fn boolean_convert(e, form) = if bool_form(form) !== null then boolean_convert(e, form, null) else if is_string(form) then failure(argument, boolean_convert, 2) else boolean_convert(e, "dnf", form)
fn boolean_convert(e, form, cond) = {
  let f = bool_form(form)
  let vars = bool_vars(if cond === null then e else [e, cond])
  let spec = if length(vars) > 14 then null else bool_spec(e, cond, vars)
  if f === null then failure(argument, boolean_convert, 2)
  else if spec === null then failure(argument, boolean_convert, 1)
  else {
    let r = bool_form_of(f, spec[1], spec[2], spec[3], vars)
    if r === null then failure(argument, boolean_convert, 2) else r
  }
}

# boolean_minimize(e), (e, form), (e, form, cond), (e, cond): the smallest sum of products of e (or of the named
# form), with the condition as in boolean_convert
fn boolean_minimize(e) = boolean_minimize(e, null, null)
fn boolean_minimize(e, form) = if bool_form(form) !== null then boolean_minimize(e, form, null) else boolean_minimize(e, null, form)
fn boolean_minimize(e, form, cond) = boolean_convert(e, if form === null then "dnf" else form, cond)

# boolean_function(k, n): the Boolean function of n variables whose truth table is the binary digits of k (bit i is
# its value at the assignment i read as a binary number, false 0 and true 1, the first argument the highest bit);
# boolean_function(k): with the fewest variables that hold k; boolean_function(table): from the list of values (true,
# false, or anything else for a value that does not matter: the function is then the smallest sum of products that
# agrees); boolean_function([[b1, ..., bn] -> v, ...]): from rules, the other assignments not mattering;
# boolean_function(spec, [x, ...]): the sum of products in those variables. A call f(a, ...) gives the value; some
# arguments true or false give the function of the others; the rest stays.
fn bool_least_arity(bits) = length(integer_digits(bits - 1, 2))

# the function of a specification: an index, a table (true, false, other for free values) or a list of rules
fn bool_function_of(spec) =
  if is_integer(spec) then boolean_function(spec, max(1, bool_least_arity(length(integer_digits(spec, 2)))))
  else if every(spec, r => match_q(r, _ -> _)) then {
    let n = length(spec[1][1])
    if some(spec, r => not is_list(r[1]) or length(r[1]) != n or not every(r[1], b => b === true or b === false)) then failure(argument, boolean_function, 1)
    else {
      let idx = r => from_digits(map(b => if b then 1 else 0, r[1]), 2)
      let tab = map(i => { let hits = select(spec, r => idx(r) == i); if length(hits) == 0 then free else last(hits)[2] }, range(0, 2^n - 1))
      bool_function_of(reverse(tab))
    }
  }
  else if length(spec) == 0 or length(spec) != 2^(length(integer_digits(length(spec), 2)) - 1) then failure(argument, boolean_function, 1)
  else {
    let n = length(integer_digits(length(spec), 2)) - 1
    let t = reverse(spec)
    let ones = select(range(0, length(t) - 1), i => t[i + 1] === true)
    let dc = select(range(0, length(t) - 1), i => t[i + 1] !== true and t[i + 1] !== false)
    if length(dc) == 0 then boolean_function(bool_index(map(v => v === true, t)), n)
    else {
      let ps = if length(ones) == 0 then [] else bool_cover(bool_primes(join(ones, dc), n), ones, n)
      boolean_function(bool_index(map(i => some(ps, p => bool_covers(p, i, n)), range(0, length(t) - 1))), n)
    }
  }
rule boolean_function(spec_) := bool_function_of(spec) when (is_integer(spec) and spec >= 0) or (is_list(spec) and length(spec) > 0)
rule boolean_function(spec_, vars_) := boolean_convert(apply(bool_function_of(spec), vars), "dnf") when is_list(vars) and (is_list(spec) or is_integer(spec))
rule boolean_function(k_, n_) := failure(argument, boolean_function, 1) when is_integer(k) and is_integer(n) and (k < 0 or k >= 2^(2^n))

# f(a, ...) for f = boolean_function(k, n): the value, or the function of the arguments that are not true or false
rule boolean_function(k_, n_)(xs___) := {
  if k == 0 then false else if k == 2^(2^n) - 1 then true else {
  let t = bool_from_index(k, n)
  let fixed = select(range(0, n - 1), i => xs[i + 1] === true or xs[i + 1] === false)
  let free = select(range(0, n - 1), i => not member(fixed, i))
  let m = length(free)
  let at = j => {
    # the table index of the free-variable index j with the fixed variables as given
    var i = 0
    for p in range(0, n - 1) {
      let bit = if member(fixed, p) then (if xs[p + 1] === true then 1 else 0) else bit_and(j, 2^(m - 1 - position_of(free, p)))/2^(m - 1 - position_of(free, p))
      i = i + bit*2^(n - 1 - p)
    }
    i
  }
  let sub = map(j => t[at(j) + 1], range(0, 2^m - 1))
  if m == 0 then sub[1] else apply(boolean_function(bool_index(sub), m), map(p => xs[p + 1], free))
  }
} when length(xs) == n and (some(xs, x => x === true or x === false) or k == 0 or k == 2^(2^n) - 1)

# boolean_counting_function(spec, [x, ...]): the sum of products in those variables
rule boolean_counting_function(k_, vars_) := boolean_convert(apply(boolean_counting_function(k, length(vars)), vars), "dnf") when is_list(vars)

# boolean_counting_function(k, n): true when at most k of its n arguments are true; ([kmin, kmax], n): when between
# kmin and kmax are; ([[k1, k2, ...]], n): when exactly k1, k2, ... are. A call with all arguments true or false
# gives the value, otherwise the function of the rest.
rule boolean_counting_function(k_, n_)(xs___) := {
  let fixed = select(xs, x => x === true or x === false)
  let free = select(xs, x => x !== true and x !== false)
  let c = count(fixed, true)
  let shifted = (lo, hi) => [lo - c, hi - c]
  if length(free) == 0 then (if is_integer(k) then c <= k else if (is_list(k) and length(k) == 1) then member(if is_list(k[1]) then k[1] else [k[1]], c) else k[1] <= c and c <= k[2])
  else if is_integer(k) then apply(boolean_counting_function(k - c, length(free)), free)
  else if (is_list(k) and length(k) == 1) then apply(boolean_counting_function([map(j => j - c, if is_list(k[1]) then k[1] else [k[1]])], length(free)), free)
  else apply(boolean_counting_function(shifted(k[1], k[2]), length(free)), free)
} when length(xs) == n and some(xs, x => x === true or x === false)


# logical_expand(e): the Boolean expression e (and, or, not, implies, equivalent, xor, nand, nor of any other
# expressions, such as equations) as a sum of products, by distributing and over or, without absorbing terms that are
# not repeats or contained in another
fn logical_terms(e, neg) =
  if is_atom(e) then (if e === true then (if neg then [] else [[]]) else if e === false then (if neg then [[]] else []) else [[if neg then not e else e]])
  else {
    let h = head(e)
    let args = apply(list, e)
    let both = (ts, us) => flatten(map(t => map(u => join(t, u), us), ts), 1)
    let all = xs => fold(both, [[]], map(x => logical_terms(x, neg), xs))
    let any = xs => flatten(map(x => logical_terms(x, neg), xs), 1)
    if h === `not` and length(args) == 1 then logical_terms(args[1], not neg)
    else if h === `and` then (if neg then any(args) else all(args))
    else if h === `or` then (if neg then all(args) else any(args))
    else if h === implies and length(args) == 2 then logical_terms(apply(`or`, [not args[1], args[2]]), neg)
    else if h === nand then logical_terms(apply(`and`, args), not neg)
    else if h === nor then logical_terms(apply(`or`, args), not neg)
    else if h === equivalent and length(args) >= 2 then logical_terms(apply(`or`, [apply(`and`, args), apply(`and`, map(x => not x, args))]), neg)
    else if h === xor and length(args) == 2 then logical_terms(apply(`or`, [args[1] and not args[2], not args[1] and args[2]]), neg)
    else if h === xor and length(args) > 2 then logical_terms(apply(xor, [args[1], apply(xor, rest(args))]), neg)
    else [[if neg then not e else e]]
  }
# logical_expand of an equation whose left side is a series at 0: the equations of the coefficients up to the order
fn logical_expand(e) when head(e) === equal and head(e[1]) === series_data and e[1][2] === 0 = {
  let s = e[1]
  let r = expand(e[2])
  apply(`and`, map(k => (if k >= s[4] then s[3][k - s[4] + 1] else 0) - coefficient(r, s[1], k) == 0, range(0, s[5] - 1)))
}
fn logical_expand(e) = {
  let ts = logical_terms(e, false)
  # a term with a literal and its negation is false; repeated literals and terms collapse
  let clean = select(map(unique, ts), t => not some(t, l => member(t, bool_neg(l))))
  let kept = unique(clean)
  let absorbed = select(kept, t => not some(kept, u => u != t and every(u, l => member(t, l))))
  # a sum that is true or false for every assignment of its few symbols is that constant
  let vars = bool_vars(absorbed)
  let atoms = unique(map(l => if match_q(l, not(_)) then l[1] else l, flatten(absorbed, 1)))
  if length(vars) > 0 and length(vars) <= 8 and length(vars) == length(atoms) then {
    let t = bool_table(bool_or(absorbed), vars)
    if t !== null and every(t, b => b) then true else bool_or(absorbed)
  } else if length(absorbed) == 1 and length(absorbed[1]) == 0 then true
  else bool_or(absorbed)
}

# --- nest_graph ---

# nest_graph(f, start), (f, start, n), with the options of graph: the graph of repeated application: the vertices are
# start (one vertex, or the elements of a list), then every value f(v) of the vertices of the last level, for n levels
# (1 by default, inf: until no new vertices); f(v) is one vertex or a list of them; the edges go from v to each of its
# values (repeats once), in the order the vertices are found. directed_edges: false makes the edges undirected.
fn nest_graph(f, start, rest...) = {
  let opts = select(rest, x => not is_atom(x) and head(x) === options)
  let pos = select(rest, x => is_atom(x) or head(x) !== options)
  let n = if length(pos) > 0 then pos[1] else 1
  let directed = not some(opts, o => some(apply(list, o), r => r[1] === directed_edges and r[2] === false))
  let kept = select(opts, o => not some(apply(list, o), r => r[1] === directed_edges))
  var verts = unique(if is_list(start) then start else [start])
  var seen = assoc(map(v => v -> true, verts))
  var frontier = verts
  var edges = []
  var depth = 0
  while length(frontier) > 0 and (n === inf or depth < n) {
    var next = []
    for v in frontier {
      let r = f(v)
      for w in (if is_list(r) then r else [r]) {
        edges = append(edges, [v, w])
        if not key_exists(seen, w) then {
          seen = insert(seen, w, true)
          verts = append(verts, w)
          next = append(next, w)
        }
      }
    }
    frontier = next
    depth = depth + 1
  }
  let es = map(p => if directed then directed_edge(p[1], p[2]) else undirected_edge(p[1], p[2]), unique(edges))
  apply(graph, join([verts, es], kept))
}

# --- array_resample ---

# the array element at the integer position i (1 to m inside): outside it as the padding says (fixed: the nearest
# element, periodic, reflected: the neighbouring elements mirrored without repeating the edge, a number: that constant)
fn resample_elem(a, i, m, pad) =
  if i >= 1 and i <= m then a[i]
  else if pad === "fixed" then a[if i < 1 then 1 else m]
  else if pad === "periodic" then a[mod(i - 1, m) + 1]
  else if pad === "reflected" then (if m == 1 then a[1] else { let j = mod(i - 1, 2*(m - 1)); a[(if j >= m then 2*(m - 1) - j else j) + 1] })
  else 0*a[1] + pad

# the cardinal B-spline of degree 2 or 3 at t
fn bspline(deg, t) = {
  let u = abs(t)
  if deg == 3 then (if u < 1 then 2/3 - u^2 + u^3/2 else if u < 2 then (2 - u)^3/6 else 0)
  else (if u < 1/2 then 3/4 - u^2 else if u < 3/2 then (3/2 - u)^2/2 else 0)
}

# the elements of a interpolated at the positions pos by the cardinal B-spline of degree deg (2: quadratic, 3: cubic)
# through the elements, with the data continued beyond the ends by the padding: the spline coefficients come from
# the recursive filter of the spline's pole on 30 continued elements each side (the error is below 1e-16), the
# values are machine reals (Unser, Aldroubi, Eden, "B-spline signal processing", IEEE Trans. Signal Process. 1993)
fn resample_spline(a, pos, m, deg, pad) = {
  let k = 30
  let p = if deg == 3 then n(-2 + sqrt(3)) else n(-3 + 2*sqrt(2))
  let gain = (if deg == 3 then -6 else -8)*p
  let s = map(i => resample_elem(a, i, m, pad)*1.0, range(1 - k, m + k))
  var y = [s[1]]
  for i in range(2, length(s)) { y = append(y, s[i] + p*y[i - 1]) }
  var c = [last(y)]
  for i in range(length(s) - 1) { c = join([y[length(s) - i] + p*c[1]], c) }
  map(x => { let x0 = floor(x); sum(map(j => gain*c[j + k]*bspline(deg, x - j), range(x0 - 1, x0 + 2))) }, pos)
}

# the elements of a (m of them along the first axis) interpolated at the positions pos
fn resample_axis(a, pos, m, how, pad) = if how === "cubic" or how === "quadratic" then resample_spline(a, pos, m, if how === "cubic" then 3 else 2, pad) else map(p => {
  let i0 = floor(p)
  let t = p - i0
  let at = k => resample_elem(a, k, m, pad)
  if t == 0 then at(i0)
  else if how === "linear" then at(i0)*(1 - t) + at(i0 + 1)*t
  else if t < 1/2 then at(i0)
  else if t > 1/2 then at(i0 + 1)
  else if how === "nearest_left" then at(i0)
  else if how === "nearest_right" then at(i0 + 1)
  else at(i0)/2 + at(i0 + 1)/2
}, pos)

# resample every axis of a: the positions along each axis, the padding of each axis
fn resample_all(a, poss, how, pads) =
  if length(poss) == 1 then resample_axis(a, poss[1], length(a), how, pads[1])
  else resample_axis(map(row => resample_all(row, rest(poss), how, rest(pads)), a), poss[1], length(a), how, pads[1])

# the number of output samples along an axis of m samples (with positions from lo to hi in point mode) for the size
# spec s (an integer or scaled(f))
fn resample_size(s, m, bin, span) = if match_q(s, scaled(_)) then (if bin then round(s[1]*m) else round(s[1]*span) + 1) else s

# the name of a resampling, padding or mode setting in lower case without underscores
fn resample_name(x) = if is_string(x) then string_replace(to_lower(x), "_" -> "") else if is_symbol(x) then string_replace(to_lower(to_string(x)), "_" -> "") else x

# array_resample(a, n), (a, n, mode), (a, n, mode, [lo, hi]): the array a (a list, or an array of lists, of numbers or
# symbolic elements) resampled to n samples along the first axis and in proportion along the others (n a list: along
# each; scaled(f): f times as many); mode "point" (default: the samples run from the first to the last element, or
# over the positions lo to hi) or "bin" (each element a cell of width 1: the samples are cell centres), also
# ["bin", d] with a shift d of the cells (-1 to 1). Options: resampling: "linear" (default), "nearest",
# "nearest_left", "nearest_right", "quadratic", "cubic"; padding: "fixed" (default), "periodic", "reflected" or a
# number (one for all axes, or a list with one for each), for positions outside the data; data_range: [a, b] (a list of
# them for several axes) the coordinates of the first and last element, for the positions lo to hi.
fn array_resample(a, n, mode = "point", rng = automatic; resampling = "linear", padding = "fixed", data_range = automatic) = {
  let m = dimensions(a)
  let md = resample_name(if is_list(mode) and length(mode) == 2 then mode[1] else mode)
  let shift = if is_list(mode) and length(mode) == 2 then mode[2] else 0
  let how = resample_name(resampling)
  let rank = length(m)
  let pads = if is_list(padding) and rank > 1 and length(padding) == rank then map(resample_name, padding) else constant_array(resample_name(padding), max(rank, 1))
  if rank == 0 or not is_list(a) or some(m, d => d == 0) then failure(argument, array_resample, 1)
  else if md !== "point" and md !== "bin" then failure(argument, array_resample, 3)
  else if not member(["linear", "nearest", "nearestleft", "nearestright", "cubic", "quadratic"], how) then failure(argument, array_resample, 5)
  else {
    let bin = md === "bin"
    let how2 = if how === "nearestleft" then "nearest_left" else if how === "nearestright" then "nearest_right" else how
    # the data coordinates of the first and last element along each axis, and the range to sample in them
    let drs = if data_range === automatic then map(d => [1, d], m) else if rank == 1 then [data_range] else data_range
    let toidx = (i, x) => 1 + (x - drs[i][1])*(m[i] - 1)/(drs[i][2] - drs[i][1])
    let ranges = if rng === automatic then map(d => [1, d], m) else map(i => map(x => toidx(i, x), if rank == 1 then rng else rng[i]), range(rank))
    # the number of samples along each axis
    let sizes = if is_list(n) then map(i => resample_size(n[i], m[i], bin, ranges[i][2] - ranges[i][1]), range(rank))
      else if match_q(n, scaled(_)) then map(i => resample_size(n, m[i], bin, ranges[i][2] - ranges[i][1]), range(rank))
      else if is_integer(n) and n >= 1 then {
        let s = if bin then n/m[1] else (if m[1] == 1 then 1 else (n - 1)/(ranges[1][2] - ranges[1][1]))
        join([n], map(i => if bin then round(s*m[i]) else round(s*(ranges[i][2] - ranges[i][1])) + 1, range(2, rank)))
      } else null
    if sizes === null or length(sizes) != rank or some(sizes, s => not is_integer(s) or s < 1) then failure(argument, array_resample, 2)
    else if not bin and some(sizes, s => s < 2) then failure(argument, array_resample, 2)
    else {
      let poss = map(i => if bin then map(j => 1/2 + (j - 1/2 - shift/2)*m[i]/sizes[i], range(sizes[i])) else map(j => ranges[i][1] + (j - 1)*(ranges[i][2] - ranges[i][1])/(sizes[i] - 1), range(sizes[i])), range(rank))
      resample_all(a, poss, how2, pads)
    }
  }
}

# --- arguments_options, check_arguments ---

# is x an option: a rule, or a list of options (any depth)?
fn is_option_item(x) = match_q(x, _ -> _) or match_q(x, _ :> _) or (is_list(x) and every(x, is_option_item)) or (not is_atom(x) and head(x) === options)

# the rules of an option item or list of them, flattened
fn option_rules(x) = if is_list(x) then flatten(map(option_rules, x), 1) else if not is_atom(x) and head(x) === options then apply(list, x) else [x]

# [min, max] from the argument count spec: n, or [min, max] (max may be inf), or null
fn argument_bounds(spec) =
  if is_integer(spec) and spec >= 0 then [spec, spec]
  else if is_list(spec) and length(spec) == 2 and is_integer(spec[1]) and spec[1] >= 0 and (spec[2] === inf or (is_integer(spec[2]) and spec[2] >= spec[1])) then spec
  else null

# arguments_options(f(a1, ...), n), (call, [min, max]): [arguments, options] of a call: the leading arguments (between
# min and max of them) and the options after them: rules, or lists of rules at any depth, flattened to one list of
# rules; failure(argument_count, f, given, spec) when the call has too few or too many arguments for the spec (n, or
# [min, max] with max possibly inf). A rule that is needed to reach the minimum counts as an argument. The call is
# held (f itself is not evaluated, so f may call this function on its own arguments); its arguments are evaluated,
# unless wrapper: h is given (or "Head" in the third argument): then the result is [h(arguments), h(options)] of the unevaluated ones. options_mode: none
# takes every item as an argument, "shortest" as many as the spec allows (the rest is the options). The settings may also come as a third argument, an association with the keys
# "Head", "OptionsMode". The options f declares are not known: any trailing rule is an option.
fn arguments_options(held call, spec, cfg = automatic; options_mode = automatic, wrapper = automatic, extra_options = automatic) = {
  let b = argument_bounds(spec)
  let q = quote(call)
  let setting = (opt, key) => if opt !== automatic then opt else if is_assoc(cfg) and key_exists(cfg, key) then cfg[key] else automatic
  let h = setting(wrapper, "Head")
  let mode = setting(options_mode, "OptionsMode")
  let nomode = mode === none or mode === "none"
  if b === null then failure(argument, arguments_options, 2)
  else if is_atom(q[1]) then failure(argument, arguments_options, 1)
  else {
    # a call written apply(g, [a, b, ...]) (a sequence variable's way of writing g(a, b, ...), which would run g) is the call g(a, b, ...)
    let spread = head(q[1]) === apply and length(q[1]) == 2 and is_list(q[1][2])
    let f = if spread then q[1][1] else head(q[1])
    let n = if spread then length(q[1][2]) else length(q[1])
    let items = if spread then q[1][2] else if h === automatic then apply(list, q[1]) else map(i => part(q, 1, i), range(n))
    var cut = n
    while not nomode and cut > 0 and is_option_item(items[cut]) { cut = cut - 1 }
    # positional arguments are the items up to cut, extended by option-like items while below the minimum
    var p = cut
    while p < b[1] and p < n { p = p + 1 }
    # the shortest option list: as many leading items as arguments as the spec allows
    if (mode === "Shortest" or mode === "shortest") and p < n then p = min(n, if b[2] === inf then n else b[2])
    if p < b[1] or (b[2] !== inf and p > b[2]) then failure(argument_count, f, p, spec)
    else if h === automatic then [take(items, p), flatten(map(option_rules, drop(items, p)), 1)]
    else [apply(h, take(items, p)), apply(h, flatten(map(option_rules, drop(items, p)), 1))]
  }
}

# check_arguments(f(a, ...), n), (call, [min, max]): true when the call has an acceptable number of arguments, the
# options after them not counted (as arguments_options splits them), else false (arguments_options says why)
fn check_arguments(held call, spec, cfg = automatic; options_mode = automatic, extra_options = automatic) =
  not is_failure(arguments_options(call, spec, cfg, options_mode: options_mode))

# --- exceptions: typed failures over throw and catch ---

# Exception types form a hierarchy: register_exception_type(type, parent) makes type a kind of parent. An exception
# is the value exception(type, data) with the data an association; throw_exception(exc) throws it (a failure that
# propagates as any failure does) with its type as the tag, and catch_exception(body, type) returns the first
# exception of that type or of a kind of it thrown in body. Registered parents are kept in exception_parents(type).

# register_exception_type(type), (type, parent), (type, [parent, ...]): declares the type (with the parents given); the
# value is the association of the type and its parents
fn register_exception_type(type) = register_exception_type(type, [])
fn register_exception_type(type, parents) = {
  let ps = if is_list(parents) then parents else [parents]
  exception_parents(type) = ps
  exception_known() = unique(append(exception_known(), type))
  assoc(exception_type -> type, parent_exception_types -> ps)
}

# the chain of a type: itself, then its parents, their parents, ... (without repeats)
fn exception_chain(type) = {
  var out = [type]
  var todo = [type]
  while length(todo) > 0 {
    let t = todo[1]
    todo = rest(todo)
    let ps = exception_parents(t)
    if is_list(ps) then for p in ps { if not member(out, p) then { out = append(out, p); todo = append(todo, p) } }
  }
  out
}

# exception(type), (type, data): the exception of a type with the data (an association; any other value is kept under
# the key payload): a plain value, not a failure, so it can be passed to functions; throw_exception makes it propagate
rule exception(t_) := exception(t, assoc())
rule exception(t_, d_) := exception(t, assoc(payload -> d)) when not is_assoc(d)

# exception_type(exc), exception_data(exc), exception_tags(exc): the type, the data association, the chain of types
fn exception_type(exc) when is_exception(exc) = exc[1]
fn exception_data(exc) when is_exception(exc) = exc[2]
fn exception_tags(exc) when is_exception(exc) = exception_chain(exc[1])

# is_exception(x): whether x is an exception
fn is_exception(x) = not is_atom(x) and head(x) === exception and length(x) == 2

# exception_types(): the registered types
rule exception_known() := []
fn exception_types() = exception_known()

# the pattern of a type and the registered types that are kinds of it
fn exception_kind_pattern(type) = apply(alternatives, unique(join([type], select(exception_known(), t => member(exception_chain(t), type)))))

# throw_exception(exc): throw the exception, tagged by its type
fn throw_exception(exc) when is_exception(exc) = throw(exc, exc[1])

# catch_exception(body, type), (body, type, f): the value of body, unless an exception of that type or a kind of it
# is thrown in it: then the exception, or f of it
fn catch_exception(held body, type) = catch_exception(quote(body), type, e => e)
fn catch_exception(held body, type, f) = catch(unquote(body), exception_kind_pattern(type), (e, tag) => f(e))

# --- associations and lists: key_union, key_intersection, key_member_q, key_free_q, disjoint_q, intersecting_q,
#     compose_list, longest_common_subsequence, longest_common_sequence ---

# the associations of a list of associations or lists of rules
fn assocs_of(xs) = map(x => if is_assoc(x) then x else assoc(x), xs)

# key_union([a1, a2, ...]), (list, f): the associations with every key of any of them, in order of first appearance;
# a key an association lacks gets missing("KeyAbsent", key), or f(key)
fn key_union(xs) = key_union(xs, automatic)
fn key_union(xs, f) when is_list(xs) and every(xs, x => is_assoc(x) or (is_list(x) and every(x, y => match_q(y, _ -> _)))) = {
  let al = assocs_of(xs)
  let ks = unique(flatten(map(keys, al), 1))
  map(a => assoc(map(k => k -> (if key_exists(a, k) then a[k] else if f === automatic then missing("KeyAbsent", k) else f(k)), ks)), al)
}

# key_intersection([a1, a2, ...]): the associations restricted to the keys all of them have
fn key_intersection(xs) when is_list(xs) and every(xs, x => is_assoc(x) or (is_list(x) and every(x, y => match_q(y, _ -> _)))) = {
  let al = assocs_of(xs)
  if length(al) == 0 then [] else {
    let common = select(keys(al[1]), k => every(al, a => key_exists(a, k)))
    map(a => assoc(map(k => k -> a[k], common)), al)
  }
}

# key_member_q(a, k), key_free_q(a, k): whether the association (or list of rules) a has the key k, or lacks it
fn key_member_q(a, k) = {
  let ks = if is_assoc(a) then keys(a) else if is_list(a) and every(a, r => match_q(r, _ -> _)) then map(r => r[1], a) else null
  ks !== null and (member(ks, k) or some(ks, x => match_q(x, k)))
}
fn key_free_q(a, k) = not key_member_q(a, k)
fn key_member_q(k) = a => key_member_q(a, k)
fn key_free_q(k) = a => key_free_q(a, k)

# disjoint_q(xs, ys), intersecting_q(xs, ys): whether the lists have no element in common, or at least one
fn intersecting_q(xs, ys) when is_list(xs) and is_list(ys) = length(intersection(xs, ys)) > 0
fn disjoint_q(xs, ys) when is_list(xs) and is_list(ys) = length(intersection(xs, ys)) == 0

# compose_list([f1, f2, ...], x): the list x, f1(x), f2(f1(x)), ...
fn compose_list(fs, x) when is_list(fs) = fold_list((acc, f) => f(acc), x, fs)

# longest_common_subsequence(a, b): the longest run of consecutive elements (characters of strings) both have, the
# first one by position in a
fn longest_common_subsequence(a, b) when is_string(a) and is_string(b) = string_join(longest_common_subsequence(characters(a), characters(b)))
fn longest_common_subsequence(a, b) when is_list(a) and is_list(b) = {
  let n = length(a)
  let m = length(b)
  var best = 0
  var end_at = 0
  # the positions of each value in b: a row of the run-length table only has entries where the elements are equal,
  # kept as an association position -> length (no n x m table; a few hundred ms for two lists of 1500)
  let at = if m == 0 then assoc() else group_by(range(m), j => b[j])
  var prev = assoc()
  for i in range(n) {
    let p = prev
    let cur = assoc(map(j => j -> lookup(p, j - 1, 0) + 1, lookup(at, a[i], [])))
    if length(cur) > 0 then {
      let top = max(values(cur))
      if top > best then { best = top; end_at = i }
    }
    prev = cur
  }
  if best == 0 then [] else take(drop(a, end_at - best), best)
}

# longest_common_sequence(a, b): a longest list of elements (characters of strings) that occur in both in the same
# order, not necessarily next to each other
fn longest_common_sequence(a, b) when is_string(a) and is_string(b) = string_join(longest_common_sequence(characters(a), characters(b)))
fn longest_common_sequence(a, b) when is_list(a) and is_list(b) = reverse(lcs_hunt_szymanski(reverse(a), reverse(b)))
fn lcs_hunt_szymanski(a, b) = {
  let n = length(a)
  let m = length(b)
  # Hunt and Szymanski (1977): th[k] is the least position in b where a common sequence of length k can end,
  # lk[k] the node of that sequence (nv: its last element, np: the node before it). Only the pairs of equal
  # elements are visited, in a row by descending position in b, so the table of n x m cells is never built.
  let at = if m == 0 then assoc() else group_by(range(m), j => b[j])
  var th = []
  var lk = []
  var nv = []
  var np = []
  for i in range(n) {
    let ai = a[i]
    for j in reverse(lookup(at, ai, [])) {
      var lo = 1
      var hi = length(th) + 1
      while lo < hi {
        let mid = floor((lo + hi) / 2)
        if th[mid] >= j then hi = mid else lo = mid + 1
      }
      nv = append(nv, ai)
      np = append(np, if lo > 1 then lk[lo - 1] else 0)
      if lo > length(th) then { th = append(th, j); lk = append(lk, length(nv)) }
      else { th = replace_part(th, [lo] -> j); lk = replace_part(lk, [lo] -> length(nv)) }
    }
  }
  var out = []
  var id = if length(lk) == 0 then 0 else lk[length(lk)]
  while id > 0 {
    out = append(out, nv[id])
    id = np[id]
  }
  reverse(out)
}
