#! mathcas 2
# Differential equations and differential algebra (round 31), evaluated after the other
# libraries, defined in every session. Helper functions are prefixed `dg_` and are not part of the interface.
# Methods and literature: crates/mc-diffalg/NOTES.md (decisions g31.N).

# --- small list helpers ---

# whether x is a nonempty list (false for a failure, which is not passed on)
fn dg_ok(x) = if x === none then false else is_list(x) and length(x) > 0

# a failure as none (a failure bound by let would end the block); the conditions of solutions dropped (the
# principal branch is taken, g31.5)
@handles_failures
fn dg_try(x) = if is_failure(x) then none else rewrite(x, conditional_expression(dg_ca_, dg_cb_) -> dg_ca)

# the function of the variables args with the value ex (built as a term, so that no name is renamed)
fn dg_lambda(args, ex) = apply(`fn`, [args, ex])

# the elements of xs not in ys, in the order of xs
fn dg_minus(xs, ys) = select(xs, x => not member(ys, x))

# the first index of y in xs, 0 if none
fn dg_index(xs, y) = {
  var k = 0
  var i = 1
  while k == 0 and i <= length(xs) { if xs[i] === y then k = i; i = i + 1 }
  k
}

# the unit multi-index e_i of length p
fn dg_unit(p, i) = table(if k == i then 1 else 0, [k, 1, p])

# the multi-indices of length p and total order k, in lexicographic order with the first slot largest first
fn dg_multi(p, k) =
  if p == 1 then [[k]]
  else flatten(map(a => map(r => join([a], r), dg_multi(p - 1, k - a)), reverse(range(0, k))), 1)

# the terms of a sum as a list
fn dg_terms(e) = if not is_atom(e) and head(e) === add then apply(list, e) else [e]

# the factors of a product as a list
fn dg_factors(e) = if not is_atom(e) and head(e) === mul then apply(list, e) else [e]

# --- jets ---
# A jet variable u_J (dependent variable u, multi-index J over the independent variables) is the symbol
# dg_j_<u>_<j1>_..._<jp>. A jet record is [symbol, index of u, J, |J|].

fn dg_jet(u, a) = to_expression(string_join(join(["dg_j_", to_string(u)], map(k => string_join(["_", to_string(k)]), a))))

# the jet records of the dependent variables us over the independent xs, orders 0..top
fn dg_jets(us, xs, top) =
  flatten(table(flatten(map(k => map(a => [dg_jet(us[i], a), i, a, k], dg_multi(length(xs), k)), range(0, top)), 1), [i, 1, length(us)]), 1)

# the user's term for a jet record: u(x, ...) or derivative(J)(u)(x, ...)
fn dg_user(r, us, xs) =
  if r[4] == 0 then apply(us[r[2]], xs) else apply(apply(derivative, r[3])(us[r[2]]), xs)

# the rules from the user's derivatives to jet symbols and back
fn dg_to_rules(jets, us, xs) = map(r => dg_user(r, us, xs) -> r[1], reverse(jets))
fn dg_from_rules(jets, us, xs) = map(r => r[1] -> dg_user(r, us, xs), jets)

# the order of the highest derivative of the us in e (at most 12)
fn dg_order(e, us, xs) = {
  let js = dg_jets(us, xs, 12)
  let je = rewrite(e, dg_to_rules(js, us, xs))
  max(join([0], map(r => r[4], select(js, r => not free_of(je, r[1])))))
}

# the equation e == 0 as an expression lhs - rhs
fn dg_lhs(e) = if not is_atom(e) and head(e) === equal then e[1] - e[2] else e

# the dependent variables as symbols (y(x) -> y) and the independent ones as a list
fn dg_deps(us) = map(u => if is_atom(u) then u else head(u), if is_list(us) then us else [us])
fn dg_indeps(xs) = if is_list(xs) then xs else [xs]

# the total derivative D_i f: the jets in f of order below top are raised along e_i
fn dg_total(f, i, xs, jets, top) = {
  let p = length(xs)
  var s = d(f, xs[i])
  for r in jets {
    if r[4] < top and not free_of(f, r[1]) then s = s + dg_jet_of(r, i, p, jets)*d(f, r[1])
  }
  s
}

# the symbol of the record r raised along e_i
fn dg_jet_of(r, i, p, jets) = {
  let a = r[3] + dg_unit(p, i)
  let ms = select(jets, q => q[2] == r[2] and q[3] === a)
  ms[1][1]
}

# --- the symmetry condition ---
# The infinitesimals are unknown functions of the base variables zs = xs and the order-0 jets: xi_i = dg_xi_i(zs),
# phi_k = dg_phi_k(zs). The prolongation (Olver, Applications of Lie groups to differential equations, Thm. 2.36,
# in the recursive form 2.44): phi_k^(J + e_i) = D_i phi_k^J - sum_j u^k_(J + e_j) D_i xi_j.

fn dg_xi(i) = to_expression(string_join(["dg_xi_", to_string(i)]))
fn dg_phi(k) = to_expression(string_join(["dg_phi_", to_string(k)]))

# [records with their prolongation coefficient], for all jets of order <= top
fn dg_prolong(us, xs, top) = {
  let p = length(xs)
  let jets = dg_jets(us, xs, top)
  let zs = join(xs, map(r => r[1], select(jets, r => r[4] == 0)))
  let xis = table(apply(dg_xi(i), zs), [i, 1, p])
  let dxi = table(table(dg_total(xis[j], i, xs, jets, top), [j, 1, p]), [i, 1, p])
  var out = []
  for r in jets {
    if r[4] == 0 then out = append(out, [r, apply(dg_phi(r[2]), zs)])
    else {
      let i = dg_index(map(k => k > 0, r[3]), true)
      let a = r[3] - dg_unit(p, i)
      let lo = select(out, q => q[1][2] == r[2] and q[1][3] === a)[1]
      let ph = dg_total(lo[2], i, xs, jets, top) - sum(dg_jet_of(lo[1], j, p, jets)*dxi[i][j], [j, 1, p])
      out = append(out, [r, expand(ph)])
    }
  }
  [jets, zs, xis, out]
}

# the leading jets: for each equation the jet of highest order not yet chosen that it contains
fn dg_leaders(es, jets) = {
  var ls = []
  for e in es {
    let cs = reverse(sort_by(select(jets, r => not free_of(e, r[1]) and not member(ls, r[1]) and r[4] > 0), r => r[4]))
    if length(cs) > 0 then ls = append(ls, select(cs, r => r[4] == cs[1][4])[length(select(cs, r => r[4] == cs[1][4]))][1])
  }
  ls
}

# dg_determining(eqs, us, xs): [the determining equations (expressions == 0, linear in the infinitesimals), zs,
# xis, phis, jets]
fn dg_determining(eqs, us, xs) = {
  let es0 = map(dg_lhs, if is_list(eqs) then eqs else [eqs])
  let top = max(map(e => dg_order(e, us, xs), es0))
  let pr = dg_prolong(us, xs, top)
  let jets = pr[1]
  let es = map(e => rewrite(e, dg_to_rules(jets, us, xs)), es0)
  let ls = dg_leaders(es, jets)
  let sol = dg_try(solve(map(e => e == 0, es), ls))
  let rules = if dg_ok(sol) then sol[1] else []
  let phis = map(q => q[2], select(pr[4], q => q[1][4] == 0))
  let splitvars = map(r => r[1], select(jets, r => r[4] > 0 and not member(ls, r[1])))
  var dets = []
  for e in es {
    var cond = sum(pr[3][i]*d(e, xs[i]), [i, 1, length(xs)])
    for q in pr[4] { if not free_of(e, q[1][1]) then cond = cond + q[2]*d(e, q[1][1]) }
    let num = expand(numerator(together(rewrite(cond, rules))))
    let present = select(splitvars, v => not free_of(num, v))
    let cfs = if length(present) == 0 then [num] else map(rl => rl[2], coefficient_rules(num, present))
    dets = join(dets, cfs)
  }
  [dets, pr[2], pr[3], phis, jets]
}

# --- the solver of linear homogeneous systems of partial differential equations ---
# State: [eqs, unknowns [F, args], constants, exprs, counter]. The steps (g31.2) are tried in order on every
# equation: constants only, a single derivative term (integrated at once), splitting by powers of a variable no
# unknown depends on, an unknown solved algebraically, a first derivative integrated, ordinary differential
# equations through d_solve.

fn dg_unknown_terms(e, unk) = select(unk, r => not free_of(e, r[1]))
fn dg_const_terms(e, cs) = select(cs, c => not free_of(e, c))

# the normal form of an equation: expanded numerator, factors free of the unknowns and constants removed
fn dg_norm(e, unk, cs, vars) = {
  let num = expand(numerator(together(e)))
  if num === 0 then 0
  else {
    let fs = dg_factors(factor_terms(num))
    let keep = select(fs, f => length(dg_unknown_terms(f, unk)) + length(dg_const_terms(f, cs)) > 0)
    var r = if length(keep) == 0 then num else expand(product(keep))
    for z in vars { r = dg_divide_out(r, z) }
    r
  }
}

# e divided by the highest power of z that divides every term
fn dg_divide_out(e, z) = {
  var r = e
  var go = not free_of(r, z)
  while go {
    let q = map(t => together(t/z), dg_terms(r))
    if every(q, t => free_of(denominator(t), z)) then r = total(q) else go = false
    if r === 0 then go = false
  }
  r
}

# a fresh unknown function (or constant when args is empty)
fn dg_fresh(st, args) = {
  let name = to_expression(string_join(["dg_f_", to_string(st[5])]))
  let st2 = replace_part(st, [5] -> st[5] + 1)
  if length(args) == 0 then [name, replace_part(st2, [3] -> append(st2[3], name))]
  else [apply(name, args), replace_part(st2, [2] -> append(st2[2], [name, args]))]
}

# substitute F(args) -> ex in the state and remove F from the unknowns
fn dg_subst_fun(st, f, args, ex) = {
  let rl = f -> dg_lambda(args, ex)
  let unk = select(st[2], r => r[1] !== f)
  [map(e => dg_norm(rewrite(e, rl), unk, st[3], st[6]), st[1]), unk, st[3], map(e => expand(rewrite(e, rl)), st[4]), st[5], st[6]]
}

fn dg_subst_const(st, c, v) = {
  let cs = select(st[3], k => k !== c)
  [map(e => dg_norm(rewrite(e, c -> v), st[2], cs, st[6]), st[1]), st[2], cs, map(e => expand(rewrite(e, c -> v)), st[4]), st[5], st[6]]
}

fn dg_clean(st) = replace_part(st, [1] -> unique(select(st[1], e => e !== 0)))

# step 1: an equation in the constants alone
fn dg_step_const(st, e, vars) =
  if length(dg_unknown_terms(e, st[2])) > 0 or length(dg_const_terms(e, st[3])) == 0 then none
  else {
    let vs = select(vars, v => not free_of(e, v))
    if length(vs) > 0 then {
      if polynomial_expression_q(e, vs) then replace_part(st, [1] -> join(select(st[1], q => q !== e), map(rl => rl[2], coefficient_rules(e, vs))))
      else none
    } else {
      let c = last(dg_const_terms(e, st[3]))
      let s = dg_try(solve(e == 0, c))
      if not dg_ok(s) then none else dg_subst_const(st, c, s[1][1][2])
    }
  }

# the unknown term of a one-term equation: [F, args, derivative orders] or none
fn dg_single(e, unk) = {
  let fs = select(dg_factors(e), f => length(dg_unknown_terms(f, unk)) > 0)
  if not is_atom(e) and head(e) === add then none
  else if length(fs) != 1 then none
  else {
    let t = fs[1]
    if is_atom(t) then none
    else if is_atom(head(t)) then [head(t), apply(list, t), table(0, [k, 1, length(t)])]
    else if not is_atom(head(head(t))) and head(head(head(t))) === derivative then [head(t)[1], apply(list, t), apply(list, head(head(t)))]
    else none
  }
}

# step 2: c D^a F = 0: F = 0, a polynomial in one variable, or a sum of functions of fewer variables
fn dg_step_single(st, e) = {
  let s = dg_single(e, st[2])
  if s === none then none
  else {
    let f = s[1]
    let args = s[2]
    let a = s[3]
    let nz = select(range(1, length(a)), i => a[i] > 0)
    if length(nz) == 0 then dg_subst_fun(st, f, args, 0)
    else if length(nz) == 1 then {
      let z = args[nz[1]]
      let rest = dg_minus(args, [z])
      var st2 = st
      var ex = 0
      for j in range(0, a[nz[1]] - 1) {
        let fr = dg_fresh(st2, rest)
        st2 = fr[2]
        ex = ex + fr[1]*z^j
      }
      dg_subst_fun(st2, f, args, ex)
    } else if length(nz) == 2 and a[nz[1]] == 1 and a[nz[2]] == 1 then {
      let f1 = dg_fresh(st, dg_minus(args, [args[nz[1]]]))
      let f2 = dg_fresh(f1[2], dg_minus(args, [args[nz[2]]]))
      dg_subst_fun(f2[2], f, args, f1[1] + f2[1])
    } else none
  }
}

# whether no unknown in e depends on z
fn dg_free_var(e, unk, z) = every(dg_unknown_terms(e, unk), r => not member(r[2], z))

# step 3: split e by the powers of a variable no unknown in it depends on
fn dg_step_split(st, e, vars) = {
  let vs = select(vars, z => not free_of(e, z) and dg_free_var(e, st[2], z) and polynomial_expression_q(e, z))
  if length(vs) == 0 then none
  else {
    let cl = select(coefficient_list(e, vs[1]), q => q !== 0)
    if length(cl) <= 1 then none
    else replace_part(st, [1] -> join(select(st[1], q => q !== e), cl))
  }
}

# the coefficient of the undifferentiated F(args) in the linear expression e and the rest
fn dg_split_term(e, f, args) = {
  let e2 = rewrite(e, apply(f, args) -> dg_tmp)
  let c = d(e2, dg_tmp)
  [c, expand(e2 - c*dg_tmp)]
}

# step 4: F = -rest/c for an undifferentiated unknown F whose arguments cover the rest
fn dg_step_alg(st, e) = {
  let rs = reverse(sort_by(dg_unknown_terms(e, st[2]), r => length(r[2])))
  var out = none
  for r in rs {
    if out === none and not free_of(e, apply(r[1], r[2])) then {
      let sp = dg_split_term(e, r[1], r[2])
      let others = dg_unknown_terms(sp[2], st[2])
      if sp[1] !== 0 and length(dg_unknown_terms(sp[1], st[2])) == 0 and free_of(sp[2], r[1]) and every(others, q => is_subset(q[2], r[2])) then {
        let ex = together(-sp[2]/sp[1])
        if every(st[6], z => member(r[2], z) or free_of(ex, z)) then out = dg_subst_fun(st, r[1], r[2], ex)
      }
    }
  }
  out
}

# step 5: c D_z^k F + rest = 0 with F only in that term, rest free of unknowns depending on z: F is the k-fold
# integral of -rest/c plus a polynomial of degree k - 1 in z with new unknowns of the other arguments
fn dg_step_int(st, e) = {
  var out = none
  let ts = dg_terms(e)
  for r in dg_unknown_terms(e, st[2]) {
    if out === none then {
      let mine = select(ts, t => not free_of(t, r[1]))
      let s = if length(mine) == 1 then dg_single(mine[1], [r]) else none
      if s !== none and s[2] === r[2] and length(select(s[3], k => k > 0)) == 1 then {
        let i = dg_index(map(k => k > 0, s[3]), true)
        let k = s[3][i]
        let z = r[2][i]
        let t = apply(apply(apply(derivative, s[3]), [r[1]]), r[2])
        let e2 = rewrite(e, t -> dg_tmp)
        let c = d(e2, dg_tmp)
        let rest = expand(e2 - c*dg_tmp)
        if free_of(rest, r[1]) and free_of(c, dg_tmp) and length(dg_unknown_terms(c, st[2])) == 0 and dg_free_var(rest, st[2], z) and every(dg_unknown_terms(rest, st[2]), q => is_subset(q[2], r[2])) then {
          let g = together(-rest/c)
          if every(st[6], v => member(r[2], v) or free_of(g, v)) then {
            var ig = g
            for j in range(1, k) { if free_of(ig, integrate) then ig = integrate(ig, z) }
            if free_of(ig, integrate) then {
              var st2 = st
              var ex = ig
              for j in range(0, k - 1) {
                let fr = dg_fresh(st2, dg_minus(r[2], [z]))
                st2 = fr[2]
                ex = ex + fr[1]*z^j
              }
              out = dg_subst_fun(st2, r[1], r[2], ex)
            }
          }
        }
      }
    }
  }
  out
}

# step 6: the equations whose unknowns are all functions of one variable z, solved by d_solve
fn dg_step_ode(st, e) = {
  let us = dg_unknown_terms(e, st[2])
  if length(us) == 0 or not every(us, r => length(r[2]) == 1 and r[2] === us[1][2]) then none
  else {
    let z = us[1][2][1]
    let sys = if length(us) == 1 then [e] else select(st[1], q => length(dg_unknown_terms(q, st[2])) > 0 and every(dg_unknown_terms(q, st[2]), r => member(us, r)))
    let fs = unique(flatten(map(q => dg_unknown_terms(q, st[2]), sys), 1))
    let sol = dg_try(d_solve(map(q => q == 0, sys), map(r => apply(r[1], [z]), fs), z))
    if not dg_ok(sol) then none
    else {
      let s1 = sol[1]
      let ks = unique(cases(map(rl => rl[2], s1), c(_), inf))
      var st2 = st
      var rl = []
      for k in ks {
        let fr = dg_fresh(st2, [])
        st2 = fr[2]
        rl = append(rl, k -> fr[1])
      }
      for f in fs {
        let v = select(s1, q => q[1] === apply(f[1], [z]))
        if length(v) > 0 then st2 = dg_subst_fun(st2, f[1], [z], expand(rewrite(v[1][2], rl)))
      }
      st2
    }
  }
}

# step 7: e = P + Q with Q free of z and containing unknowns, P depending on z: the consequence d(e, z) == 0
# (separation of variables, g31.2); the variable whose derivative has the fewest terms
fn dg_step_diff(st, e) = {
  let ts = dg_terms(e)
  var best = none
  for z in st[6] {
    let q = select(ts, t => free_of(t, z))
    if length(q) > 0 and length(q) < length(ts) and some(q, t => length(dg_unknown_terms(t, st[2])) > 0) then {
      let de = dg_norm(d(e, z), st[2], st[3], st[6])
      if de !== 0 and not member(st[1], de) and (best === none or leaf_count(de) < leaf_count(best)) then best = de
    }
  }
  if best === none then none else replace_part(st, [1] -> append(st[1], best))
}

# solve the system: the state after no step applies (st[6] holds the base variables)
fn dg_solve_system(st0) = {
  var st = dg_clean(st0)
  var go = true
  var count = 0
  while go and count < 400 {
    count = count + 1
    var nxt = none
    let es = sort_by(st[1], leaf_count)
    for e in es { if nxt === none then nxt = dg_step_const(st, e, st[6]) }
    for e in es { if nxt === none then nxt = dg_step_single(st, e) }
    for e in es { if nxt === none then nxt = dg_step_split(st, e, st[6]) }
    for e in es { if nxt === none then nxt = dg_step_alg(st, e) }
    for e in es { if nxt === none then nxt = dg_step_int(st, e) }
    for e in es { if nxt === none then nxt = dg_step_ode(st, e) }
    for e in es { if nxt === none then nxt = dg_step_diff(st, e) }
    if nxt === none then go = false else st = dg_clean(nxt)
  }
  st
}

# --- Lie point symmetries ---

# the generators from a solved state: one per constant, then one per remaining unknown function c(k)(args)
fn dg_generators(st) = {
  let ex = st[4]
  let gens = select(map(c => map(q => expand(d(q, c)), ex), select(st[3], c => not free_of(ex, c))), g => some(g, q => q !== 0))
  gens
}

# determining equations for the user: the infinitesimals as xi(...), eta(...) (xi1, ... and eta1, ... for several)
fn dg_user_names(xs, us) = join(
  if length(xs) == 1 then [xi] else table(to_expression(string_join(["xi", to_string(i)])), [i, 1, length(xs)]),
  if length(us) == 1 then [eta] else table(to_expression(string_join(["eta", to_string(i)])), [i, 1, length(us)]))

# symmetry_determining_equations(eqs, u, x): the determining equations of the Lie point symmetries of the
# differential equations eqs for the functions u (a name or a list) of x (a name or a list), as equations in the
# infinitesimals xi(x, u), eta(x, u)
fn symmetry_determining_equations(eqs, u0, x0) = {
  let us = dg_deps(u0)
  let xs = dg_indeps(x0)
  let dt = dg_determining(eqs, us, xs)
  let jets0 = select(dt[5], r => r[4] == 0)
  let base = join(xs, us)
  let names = dg_user_names(xs, us)
  let p = length(xs)
  let rl = join(table(dg_xi(i) -> names[i], [i, 1, p]), table(dg_phi(k) -> names[p + k], [k, 1, length(us)]))
  let back = map(r => r[1] -> us[r[2]], jets0)
  unique(select(map(e => rewrite(rewrite(e, rl), back) == 0, dt[1]), q => q !== true))
}

# lie_point_symmetries(eqs, u, x): a basis of the Lie point symmetries of eqs, each the list of the infinitesimals
# [xi_1, ..., xi_p, eta_1, ..., eta_q] of the generator sum xi_i d/dx_i + sum eta_k d/du_k, in the variables x and u
fn lie_point_symmetries(eqs, u0, x0; degree = none) = if degree !== none then dg_ansatz(eqs, dg_deps(u0), dg_indeps(x0), degree) else {
  let us = dg_deps(u0)
  let xs = dg_indeps(x0)
  let dt = dg_determining(eqs, us, xs)
  let zs = dt[2]
  let unk = join(table([dg_xi(i), zs], [i, 1, length(xs)]), table([dg_phi(k), zs], [k, 1, length(us)]))
  let st0 = [map(e => dg_norm(e, unk, [], zs), dt[1]), unk, [], join(dt[3], dt[4]), 1, zs]
  let st = dg_solve_system(st0)
  let jets0 = select(dt[5], r => r[4] == 0)
  let back = map(r => r[1] -> us[r[2]], jets0)
  let gens = map(g => rewrite(g, back), dg_generators(st))
  if length(st[1]) == 0 and length(st[2]) == 0 then gens
  else join(gens, [dg_rest(st, back)])
}

# the part left with unknown functions: conditional_expression of the generator in c(1)(...), ... and the equations
fn dg_rest(st, back) = {
  let fs = select(st[2], r => not free_of(st[4], r[1]))
  let rl = table(fs[k][1] -> c(k), [k, 1, length(fs)])
  let gen = rewrite(rewrite(st[4], rl), back)
  let cs = select(st[3], c => not free_of(gen, c))
  let gen2 = rewrite(gen, map(c => c -> 0, cs))
  conditional_expression(gen2, apply(`and`, join([true], map(e => rewrite(rewrite(e, rl), back) == 0, st[1]))))
}

# --- the Lie algebra ---

# symmetry_commutator(v, w, vars): the commutator [v, w] of the vector fields with components v, w over vars
fn symmetry_commutator(v, w, vars) =
  table(expand(sum(v[j]*d(w[i], vars[j]) - w[j]*d(v[i], vars[j]), [j, 1, length(vars)])), [i, 1, length(vars)])

# the coordinates of the field w in the basis gs, or none (g31.3): rational fields by exact equations at
# rational points; others by least squares at machine points, rationalised and checked with possible_zero_q
fn dg_coords(w, gs, vars) = {
  let r = length(gs)
  if every(w, q => q === 0) then table(0, [k, 1, r])
  else {
    let cs = table(to_expression(string_join(["dg_k_", to_string(k)])), [k, 1, r])
    let comb = table(w[i] - sum(cs[k]*gs[k][i], [k, 1, r]), [i, 1, length(vars)])
    let pts = table(map(v => v -> (37*j + 11*dg_index(vars, v))/(13 + j + 3*dg_index(vars, v)), vars), [j, 1, r + 3])
    let exact = every(join(w, flatten(gs, 1)), q => rational_expression_q(q, vars))
    var k = none
    if exact then {
      let s = dg_try(solve(flatten(map(pt => map(q => together(rewrite(q, pt)) == 0, comb), pts), 1), cs))
      if dg_ok(s) then k = rewrite(cs, s[1])
    } else {
      let rows = flatten(map(pt => map(q => n(rewrite(q, pt)), comb), pts), 1)
      let a = map(q => map(c => d(q, c), cs), rows)
      let b = map(q => -rewrite(q, map(c => c -> 0, cs)), rows)
      let ks = dg_try(least_squares(a, b))
      if dg_ok(ks) then k = map(q => rationalize(q, 1/10^9), ks)
    }
    if k === none then none
    else if every(comb, q => possible_zero_q(rewrite(q, map(i => cs[i] -> k[i], range(1, r))))) then k else none
  }
}

# lie_commutator_table(gens, vars): the matrix of the commutators [v_i, v_j] as combinations of v(1), v(2), ...
# (the basis gens over the variables vars); an entry outside the span is given as its components
fn lie_commutator_table(gs, vars) =
  table(table({
    let w = symmetry_commutator(gs[i], gs[j], vars)
    let k = dg_coords(w, gs, vars)
    if k === none then w else expand(sum(k[m]*v(m), [m, 1, length(gs)]))
  }, [j, 1, length(gs)]), [i, 1, length(gs)])

# --- symmetries from a polynomial ansatz ---

# the generators of degree <= deg in all variables (g31.4): the infinitesimals are polynomials with unknown
# coefficients, the determining equations are cleared of denominators and split by the monomials
fn dg_ansatz(eqs, us, xs, deg) = {
  let dt = dg_determining(eqs, us, xs)
  let zs = dt[2]
  let mons = flatten(map(k => map(a => product(table(zs[i]^a[i], [i, 1, length(zs)])), dg_multi(length(zs), k)), range(0, deg)), 1)
  let nf = length(xs) + length(us)
  let ks = table(table(to_expression(string_join(["dg_a_", to_string(i), "_", to_string(j)])), [j, 1, length(mons)]), [i, 1, nf])
  let polys = table(sum(ks[i][j]*mons[j], [j, 1, length(mons)]), [i, 1, nf])
  let names = join(table(dg_xi(i), [i, 1, length(xs)]), table(dg_phi(k), [k, 1, length(us)]))
  let rl = table(names[i] -> dg_lambda(zs, polys[i]), [i, 1, nf])
  let conds = map(e => expand(numerator(together(rewrite(e, rl)))), dt[1])
  let lin = flatten(map(q => if q === 0 then [] else if polynomial_expression_q(q, zs) then map(r => r[2], coefficient_rules(q, zs)) else [none], conds), 1)
  if member(lin, none) then none
  else {
    let unk = flatten(ks, 1)
    let s = if length(lin) == 0 then [[]] else dg_try(solve(map(q => q == 0, lin), unk))
    if not dg_ok(s) then []
    else {
      let gen = expand(rewrite(polys, s[1]))
      let free = select(unk, k => not free_of(gen, k))
      let back = map(r => r[1] -> us[r[2]], select(dt[5], r => r[4] == 0))
      select(map(k => rewrite(map(q => expand(d(q, k)), gen), back), free), g => some(g, q => q !== 0))
    }
  }
}

# --- canonical coordinates, reduction of order, integration ---

# canonical_coordinates([xi, eta], [x, y]): [r, s] with xi r_x + eta r_y = 0 and xi s_x + eta s_y = 1
fn canonical_coordinates(g, vars) = {
  let x = vars[1]
  let y = vars[2]
  if g[1] === 0 then [x, simplify(integrate(1/g[2], y))]
  else if g[2] === 0 then [y, simplify(integrate(1/g[1], x))]
  else {
    let sol = dg_try(d_solve(dg_y'(x) == rewrite(g[2]/g[1], y -> dg_y(x)), dg_y(x), x))
    if not dg_ok(sol) then none
    else {
      var out = none
      for br in reverse(sol) {
        if out === none then {
          let yx = br[1][2]
          let cs = dg_try(solve(yx == y, c(1)))
          if dg_ok(cs) then {
            let r = simplify(cs[1][1][2])
            let ig = integrate(together(rewrite(1/g[1], y -> yx)), x)
            if free_of(ig, integrate) then out = [r, simplify(rewrite(rewrite(ig, c(1) -> r), (dg_pa_^2)^(1/2) -> dg_pa))]
          }
        }
      }
      out
    }
  }
}

# the inverse of the coordinates: [x, y] in terms of r0, s0, or none; when the system is not solved at once, an
# equation in one variable is solved first
fn dg_inverse(rs, vars, r0, s0) = {
  let sol = dg_try(solve([rs[1] == r0, rs[2] == s0], vars))
  if dg_ok(sol) then rewrite(vars, sol[1])
  else {
    let es = [rs[1] - r0, rs[2] - s0]
    var out = none
    for i in [1, 2] {
      for j in [1, 2] {
        let v = vars[j]
        let o = vars[3 - j]
        if out === none and free_of(es[i], o) then {
          let s1 = dg_try(solve(es[i] == 0, v))
          if dg_ok(s1) then {
            let val = s1[1][1][2]
            let s2 = dg_try(solve(rewrite(es[3 - i], v -> val) == 0, o))
            if dg_ok(s2) then out = if j == 1 then [val, s2[1][1][2]] else [s2[1][1][2], val]
          }
        }
      }
    }
    out
  }
}

# symmetry_reduce_order(eq, y, x, gen, v, r): the equation of one order less for v(r), v = ds/dr in the
# canonical coordinates r, s of the generator gen = [xi, eta]: [reduced equation, r == r(x, y), v == ds/dr in x, y, y']
fn symmetry_reduce_order(eq, y0, x, g, v, r) = {
  let y = dg_deps(y0)[1]
  let rs = canonical_coordinates(g, [x, y])
  if rs === none then none
  else {
    let inv = dg_inverse(rs, [x, y], r, dg_s(r))
    if inv === none then none
    else {
      let top = dg_order(dg_lhs(eq), [y], [x])
      let dx = d(inv[1], r)
      var ds = [inv[2]]
      for k in range(1, top) { ds = append(ds, together(d(last(ds), r)/dx)) }
      let jets = dg_jets([y], [x], top)
      let je = rewrite(dg_lhs(eq), dg_to_rules(jets, [y], [x]))
      let sub = join([x -> inv[1]], table(jets[k + 1][1] -> ds[k + 1], [k, 0, top]))
      let red0 = numerator(together(rewrite(je, sub)))
      let vr = join(table(apply(derivative, [k])(dg_s)(r) -> apply(derivative, [k - 1])(v)(r), [k, top, 2, -1]), [dg_s'(r) -> v(r)])
      let red = simplify(factor(rewrite(red0, vr)))
      let red2 = if not is_atom(red) and head(red) === mul then product(select(apply(list, red), f => not free_of(f, v))) else red
      if not free_of(red2, dg_s) then none
      else {
        let yp = to_expression("dg_yp")
        let vxy = simplify((d(rs[2], x) + d(rs[2], y)*yp)/(d(rs[1], x) + d(rs[1], y)*yp))
        [expand(red2) == 0, r == rs[1], v == rewrite(vxy, yp -> y'(x))]
      }
    }
  }
}

# the solutions y(x) of s(x, y) == h(r(x, y)) + c(k), or the implicit equation
fn dg_back(rs, h, x, y, k) = {
  let eqn = rs[2] == rewrite(h, dg_r -> rs[1]) + c(k)
  let sol = dg_try(solve(eqn, y))
  if dg_ok(sol) and free_of(sol, solve) then map(q => [y(x) -> simplify(q[1][2])], sol) else [[eqn]]
}

# symmetry_d_solve(eq, y, x): solutions of a first- or second-order ordinary differential equation by a Lie point
# symmetry (canonical coordinates, then a quadrature or a first-order equation for d_solve); optional gen
fn symmetry_d_solve(eq, y0, x; generator = none) = {
  let y = dg_deps(y0)[1]
  let top = dg_order(dg_lhs(eq), [y], [x])
  let gs = if generator !== none then [generator] else dg_ode_symmetries(eq, y, x)
  var out = none
  for g in gs {
    if out === none then {
      let red = symmetry_reduce_order(eq, y, x, g, dg_v, dg_r)
      if red !== none then {
        let rs = [red[2][2], canonical_coordinates(g, [x, y])[2]]
        let vs = if top == 1 then dg_try(solve(red[1], dg_v(dg_r))) else dg_try(d_solve(red[1], dg_v(dg_r), dg_r))
        if dg_ok(vs) and free_of(vs, d_solve) and free_of(vs, solve) then {
          let h = integrate(vs[1][1][2], dg_r)
          if free_of(h, integrate) then out = dg_back(rs, h, x, y, top)
        }
      }
    }
  }
  out
}

# the symmetries tried by symmetry_d_solve: the basis, else polynomial generators of degree 1, then 2
fn dg_ode_symmetries(eq, y, x) = {
  let gs = select(lie_point_symmetries(eq, y, x), g => is_list(g))
  if length(gs) > 0 then gs
  else {
    let a1 = dg_ansatz(eq, [y], [x], 1)
    if is_list(a1) and length(a1) > 0 then a1 else { let a2 = dg_ansatz(eq, [y], [x], 2); if is_list(a2) then a2 else [] }
  }
}

# whether the generator g satisfies the determining equations of eqs (a check by substitution)
fn dg_check_generator(eqs, u0, x0, g) = {
  let us = dg_deps(u0)
  let xs = dg_indeps(x0)
  let names = dg_user_names(xs, us)
  let base = join(xs, us)
  let rl = map(i => names[i] -> dg_lambda(base, g[i]), range(1, length(g)))
  every(symmetry_determining_equations(eqs, u0, x0), q => simplify(rewrite(q, rl)) === true)
}

# whether the rules sol solve the ordinary differential equation eq for y of x (symbolically, else at points)
fn dg_check_solution(eq, y, x, sol) = {
  let b = sol[1][2]
  let r = simplify(rewrite(dg_lhs(eq), y -> dg_lambda([x], b)))
  r === 0 or every([1/3, 2/5, 3/7], p => abs(n(rewrite(r, join([x -> p], table(c(k) -> 1/(k + 1), [k, 1, 3]))))) < 10^-8)
}

# --- similarity reductions of partial differential equations ---

# the invariants of the generator [xi, tau, phi] over [x, t, u] with xi, tau free of u and phi linear in u (g31.6):
# [z(x, t), u in terms of x, t and f(z)], or none
fn dg_invariants(g, x, t, u, f) =
  if not (free_of(g[1], u) and free_of(g[2], u) and free_of(d(g[3], u, u), u) and d(g[3], u, u) === 0) then none
  else if g[1] === 0 and g[2] === 0 then none
  else {
    let par = if g[2] !== 0 then t else x
    let oth = if g[2] !== 0 then x else t
    let gp = if g[2] !== 0 then g[2] else g[1]
    let go = if g[2] !== 0 then g[1] else g[2]
    # the first integral z of d oth / d par = go/gp
    var z = none
    var oth_of = none
    if go === 0 then { z = oth; oth_of = c(1) }
    else {
      let s = dg_try(d_solve(dg_w'(par) == rewrite(go/gp, oth -> dg_w(par)), dg_w(par), par))
      if dg_ok(s) then {
        oth_of = s[1][1][2]
        let k = dg_try(solve(oth_of == oth, c(1)))
        if dg_ok(k) then z = simplify(k[1][1][2])
      }
    }
    if z === none then none
    else {
      # du/dpar = phi/gp along oth = oth_of(par, c(1))
      let rhs = rewrite(g[3]/gp, [oth -> rewrite(oth_of, c(1) -> dg_zz), u -> dg_w(par)])
      let s2 = dg_try(d_solve(dg_w'(par) == rhs, dg_w(par), par))
      if not dg_ok(s2) then none
      else {
        let ux = rewrite(s2[1][1][2], c(1) -> f(dg_zz))
        let ux2 = simplify(rewrite(ux, dg_zz -> z))
        [z, ux2]
      }
    }
  }

# similarity_reduction(eq, u, [x, t], gen, f, z): the ordinary differential equation for f(z) obtained from the
# invariant solutions of the symmetry gen = [xi, tau, phi]: [ode, u(x, t) -> solution in f, z -> z(x, t)]
fn similarity_reduction(eq, u0, xs, g, f, z0) = {
  let u = dg_deps(u0)[1]
  let x = xs[1]
  let t = xs[2]
  let inv = dg_invariants(g, x, t, u, f)
  if inv === none then none
  else {
    let zz = inv[1]
    let ux = inv[2]
    let e1 = rewrite(dg_lhs(eq), u -> dg_lambda([x, t], ux))
    let num = numerator(together(e1))
    # x (or t) in terms of z and the other variable
    let byx = dg_try(solve(zz == z0, x))
    let byt = dg_try(solve(zz == z0, t))
    let sub = if dg_ok(byx) then [x -> byx[1][1][2]] else if dg_ok(byt) then [t -> byt[1][1][2]] else none
    if sub === none then none
    else {
      let red = factor(simplify(rewrite(num, sub)))
      let fs = select(dg_factors(red), q => not free_of(q, f))
      let ode = if length(fs) == 0 then red else simplify(product(fs))
      let pw = if not is_atom(ode) and head(ode) === pow and is_number(ode[2]) and ode[2] > 0 then ode[1] else ode
      if free_of(pw, x) and free_of(pw, t) then [expand(pw) == 0, apply(u, xs) -> ux, z0 -> zz] else none
    }
  }
}

# --- conservation laws by multipliers (g31.7) ---

# the jets a multiplier may depend on: for one independent variable y, y', ..., y^(q); for several, the
# derivatives in the first variable only, u, u_x, ..., u_(x^q) (on the solutions of an evolution equation the
# others are eliminated)
fn dg_mult_jets(jets, p, q) = select(jets, r => r[4] <= q and (p == 1 or r[4] == r[3][1]))

# (-D)_J f
fn dg_minus_total(f, a, xs, jets, top) = {
  var g = f
  for i in range(1, length(a)) {
    for k in range(1, a[i]) { g = -dg_total(g, i, xs, jets, top) }
  }
  g
}

# the Euler operator E_u(f) for the dependent variable with index k
fn dg_euler(f, k, xs, jets, top) = {
  var s = 0
  for r in jets {
    if r[2] == k and not free_of(f, r[1]) then s = s + dg_minus_total(d(f, r[1]), r[3], xs, jets, top)
  }
  expand(s)
}

# the multipliers of the single equation eq for u of xs, depending on xs and the jets of dg_mult_jets
fn dg_multipliers(eq, us, xs, q, deg) = {
  let e0 = dg_lhs(eq)
  let ord = max(dg_order(e0, us, xs), q)
  let top = 2*ord + 1
  let jets = dg_jets(us, xs, top)
  let e = rewrite(e0, dg_to_rules(jets, us, xs))
  let mj = map(r => r[1], dg_mult_jets(jets, length(xs), q))
  let zs = join(xs, mj)
  if deg !== none then dg_multipliers_poly(e, xs, jets, top, zs, deg) else {
  let lam = apply(dg_lam, zs)
  let cond = dg_euler(lam*e, 1, xs, jets, top)
  let others = map(r => r[1], select(jets, r => not member(mj, r[1]) and not free_of(cond, r[1])))
  let num = expand(numerator(together(cond)))
  let dets = if length(others) == 0 then [num] else map(rl => rl[2], coefficient_rules(num, others))
  let unk = [[dg_lam, zs]]
  let st = dg_solve_system([map(q2 => dg_norm(q2, unk, [], zs), dets), unk, [], [lam], 1, zs])
  let done = length(st[1]) == 0 and length(st[2]) == 0
  let rest = if done then none else { let q3 = dg_rest(st, []); conditional_expression(q3[1][1], q3[2]) }
  [map(g => g[1], dg_generators(st)), jets, e, done, rest]
  }
}

# multipliers that are polynomials of total degree <= deg in zs with constant coefficients
fn dg_multipliers_poly(e, xs, jets, top, zs, deg) = {
  let mons = flatten(map(k => map(a => product(table(zs[i]^a[i], [i, 1, length(zs)])), dg_multi(length(zs), k)), range(0, deg)), 1)
  let ks = table(to_expression(string_join(["dg_a_", to_string(j)])), [j, 1, length(mons)])
  let lam = sum(ks[j]*mons[j], [j, 1, length(mons)])
  let cond = expand(numerator(together(dg_euler(lam*e, 1, xs, jets, top))))
  let vs = join(xs, select(map(r => r[1], jets), v => not free_of(cond, v)))
  let lin = if cond === 0 then [] else map(rl => rl[2], coefficient_rules(cond, vs))
  let sol = if length(lin) == 0 then [[]] else dg_try(solve(map(q2 => q2 == 0, lin), ks))
  if not dg_ok(sol) then [[], jets, e, true, none]
  else {
    let gen = expand(rewrite(lam, sol[1]))
    [select(map(k => expand(d(gen, k)), select(ks, k => not free_of(gen, k))), q2 => q2 !== 0), jets, e, true, none]
  }
}

# conservation_law_multipliers(eq, u, x): a basis of the multipliers Lambda (functions of x and of u and its
# derivatives up to order: q in the first variable) with E_u(Lambda eq) == 0, so that Lambda eq is a divergence
fn conservation_law_multipliers(eq, u0, x0; order = 0, degree = none) = {
  let us = dg_deps(u0)
  let xs = dg_indeps(x0)
  let m = dg_multipliers(eq, us, xs, order, degree)
  let back = dg_from_rules(m[2], us, xs)
  join(map(g => rewrite(g, back), m[1]), if m[5] === none then [] else [rewrite(m[5], back)])
}

# X with D_x X == g, by integrating the highest derivative (g linear in it) with respect to the jet one below;
# jx are the jet symbols u, u_x, u_xx, ... in order; none if g is not a total derivative
fn dg_integrate_total(g, i, xs, jets, top, jx) = {
  var rest = expand(g)
  var out = 0
  var go = true
  var count = 0
  while go and rest !== 0 and count < 40 {
    count = count + 1
    let ks = select(range(1, length(jx)), k => not free_of(rest, jx[k]))
    let k = if length(ks) == 0 then 0 else max(ks)
    if k <= 1 then {
      if k == 1 then go = false
      else {
        let ix = integrate(rest, xs[i])
        if free_of(ix, integrate) then { out = out + ix; rest = 0 } else go = false
      }
    } else {
      let a = d(rest, jx[k])
      if not free_of(a, jx[k]) then go = false
      else {
        let pp = integrate(a, jx[k - 1])
        if not free_of(pp, integrate) then go = false
        else { out = out + pp; rest = expand(rest - dg_total(pp, i, xs, jets, top)) }
      }
    }
  }
  if rest === 0 then expand(out) else none
}

# conservation_laws(eq, u, x): for an ordinary differential equation the pairs [Lambda, Phi] of a multiplier and
# the first integral with Lambda eq == D_x Phi; for an evolution equation u_t == F in [x, t] the triples
# [Lambda, T, X] with Lambda (u_t - F) == D_t T + D_x X (T from the homotopy formula, X on the solutions)
fn conservation_laws(eq, u0, x0; order = 0, degree = none) = {
  let us = dg_deps(u0)
  let xs = dg_indeps(x0)
  let m = dg_multipliers(eq, us, xs, order, degree)
  let jets = m[2]
  let top = max(map(r => r[4], jets))
  let e = m[3]
  let back = dg_from_rules(jets, us, xs)
  if length(xs) == 1 then {
    let jx = map(r => r[1], jets)
    map(lam => { let ph = dg_integrate_total(lam*e, 1, xs, jets, top, jx); [rewrite(lam, back), if ph === none then none else rewrite(ph, back)] }, m[1])
  } else {
    # u_t and its x-derivatives on the solutions
    let ut = select(jets, r => r[3] === join([0, 1], table(0, [k, 3, length(xs)])))[1][1]
    let sol = dg_try(solve(e == 0, ut))
    let fx = if dg_ok(sol) then sol[1][1][2] else none
    let jx = map(r => r[1], select(jets, r => r[3][1] == r[4]))
    map(lam => {
      let l = to_expression("dg_lambda_h")
      let lscaled = rewrite(lam, map(s => s -> l*s, jx))
      let tt = expand(integrate(jx[1]*lscaled, [l, 0, 1]))
      # D_t T on solutions: the t-derivatives of the x-jets replaced by the x-derivatives of F
      var dtt = d(tt, xs[2])
      for k in range(1, length(jx)) {
        if not free_of(tt, jx[k]) and fx !== none then {
          var fk = fx
          for j in range(2, k) { fk = dg_total(fk, 1, xs, jets, top) }
          dtt = dtt + fk*d(tt, jx[k])
        }
      }
      let flux = if fx === none then none else dg_integrate_total(-expand(dtt), 1, xs, jets, top, jx)
      [rewrite(lam, back), rewrite(tt, back), if flux === none then none else rewrite(flux, back)]
    }, m[1])
  }
}

# --- differential polynomials and rankings (g31.8) ---
# A context is [us, xs, jets, kind]: the dependent variables (the first ranks highest in an elimination ranking),
# the independent ones, the jet records up to the order bound, and the ranking `orderly` (total order, then the
# dependent variable, then the multi-index lexicographically) or `elimination` (the dependent variable first).
# Differential polynomials are kept in jet form inside and given back in the user's notation.

fn dg_ctx(ps, u0, x0, kind, extra) = {
  let us = dg_deps(u0)
  let xs = dg_indeps(x0)
  let top = max(join([0], map(q => dg_order(dg_lhs(q), us, xs), ps))) + extra
  [us, xs, dg_jets(us, xs, top), kind, top]
}

fn dg_in(p, cx) = expand(rewrite(dg_lhs(p), dg_to_rules(cx[3], cx[1], cx[2])))
fn dg_out(p, cx) = rewrite(p, dg_from_rules(cx[3], cx[1], cx[2]))

# the weight of a jet record: larger is higher in the ranking
fn dg_weight(r, cx) = {
  let q = length(cx[1])
  let lex = sum(r[3][i]*64^(length(r[3]) - i), [i, 1, length(r[3])])
  if cx[4] === elimination then (q - r[2] + 1)*64^(2*length(r[3]) + 2) + r[4]*64^(length(r[3]) + 1) + lex
  else r[4]*64^(2*length(r[3]) + 2) + (q - r[2] + 1)*64^(length(r[3]) + 1) + lex
}

# the jet records in p, highest first
fn dg_present(p, cx) = reverse(sort_by(select(cx[3], r => not free_of(p, r[1])), r => dg_weight(r, cx)))

# the leader of p (a jet record) or none when p has no derivative of the us
fn dg_leader(p, cx) = { let rs = dg_present(p, cx); if length(rs) == 0 then none else rs[1] }

fn dg_init(p, v) = expand(coefficient(p, v, exponent(p, v)))
fn dg_sep(p, v) = expand(d(p, v))

# whether the jet w is a derivative of the jet v (proper when not equal)
fn dg_is_deriv(w, v) = w[2] == v[2] and every(range(1, length(w[3])), i => w[3][i] >= v[3][i])

# D^theta p for the multi-index theta
fn dg_derive(p, theta, cx) = {
  var g = p
  for i in range(1, length(theta)) {
    for k in range(1, theta[i]) { g = expand(dg_total(g, i, cx[2], cx[3], cx[5])) }
  }
  g
}

# the pseudo-remainder of p by q in the variable w
fn dg_prem(p, q, w) = {
  let m = exponent(q, w)
  let lc = coefficient(q, w, m)
  var r = expand(p)
  var go = true
  var count = 0
  while go and count < 64 {
    count = count + 1
    let dp = exponent(r, w)
    if r === 0 or dp < m then go = false
    else r = expand(lc*r - coefficient(r, w, dp)*w^(dp - m)*q)
  }
  r
}

# the content of p in the base field removed: the gcd of the coefficients of its monomials in the jets
fn dg_primitive(p, cx) = {
  let vs = map(r => r[1], dg_present(p, cx))
  if p === 0 or length(vs) == 0 then p
  else {
    let cs = map(rl => rl[2], coefficient_rules(p, vs))
    let g = fold((a, b) => polynomial_gcd(a, b), cs[1], rest(cs))
    let q = expand(together(p/g))
    let q2 = if is_number(g) or not free_of(denominator(together(p/g)), cx[2][1]) then expand(p/(if is_number(g) and g != 0 then abs(g) else 1)) else q
    let c1 = coefficient_rules(q2, vs)[1][2]
    if is_number(c1) and c1 < 0 then expand(-q2) else q2
  }
}

# the full (Ritt) remainder of p by the list as: proper derivatives of the leaders are eliminated first (with the
# separants), then the leaders' degrees are lowered (with the initials)
fn dg_full_reduce(p, as, cx) = {
  var r = expand(p)
  var go = true
  var count = 0
  let ls = map(a => dg_leader(a, cx), as)
  while go and r !== 0 and count < 200 {
    count = count + 1
    var done = false
    for w in dg_present(r, cx) {
      if not done then {
        for i in range(1, length(as)) {
          if not done and dg_is_deriv(w, ls[i]) and w[3] !== ls[i][3] then {
            let th = w[3] - ls[i][3]
            let da = dg_derive(as[i], th, cx)
            r = dg_prem(r, da, w[1])
            done = true
          } else if not done and w[3] === ls[i][3] and w[2] == ls[i][2] and exponent(r, w[1]) >= exponent(as[i], w[1]) then {
            r = dg_prem(r, as[i], w[1])
            done = true
          }
        }
      }
    }
    if not done then go = false
  }
  dg_primitive(r, cx)
}

# the Delta-polynomial of two polynomials whose leaders are derivatives of the same function, or none
fn dg_delta(a, b, cx) = {
  let la = dg_leader(a, cx)
  let lb = dg_leader(b, cx)
  if la === none or lb === none or la[2] != lb[2] then none
  else {
    let th = table(max(la[3][i], lb[3][i]), [i, 1, length(la[3])])
    if sum(th[i], [i, 1, length(th)]) > cx[5] then none
    else expand(dg_sep(b, lb[1])*dg_derive(a, th - la[3], cx) - dg_sep(a, la[1])*dg_derive(b, th - lb[3], cx))
  }
}

fn dg_rank_opt(r) = if r === elimination then elimination else orderly

# differential_leader(p, u, x): the derivative of the functions u that ranks highest in p
fn differential_leader(p, u0, x0; ranking = orderly) = {
  let cx = dg_ctx([p], u0, x0, dg_rank_opt(ranking), 0)
  let v = dg_leader(dg_in(p, cx), cx)
  if v === none then none else dg_out(v[1], cx)
}

# differential_initial(p, u, x), differential_separant(p, u, x): the coefficient of the highest power of the leader
# and the derivative of p by its leader
fn differential_initial(p, u0, x0; ranking = orderly) = {
  let cx = dg_ctx([p], u0, x0, dg_rank_opt(ranking), 0)
  let q = dg_in(p, cx)
  let v = dg_leader(q, cx)
  if v === none then q else dg_out(dg_init(q, v[1]), cx)
}
fn differential_separant(p, u0, x0; ranking = orderly) = {
  let cx = dg_ctx([p], u0, x0, dg_rank_opt(ranking), 0)
  let q = dg_in(p, cx)
  let v = dg_leader(q, cx)
  if v === none then 0 else dg_out(dg_sep(q, v[1]), cx)
}

# differential_reduce(p, ps, u, x): the full remainder of p by the differential polynomials ps (their proper
# derivatives eliminated by the separants, then pseudo-division by the initials)
fn differential_reduce(p, ps, u0, x0; ranking = orderly) = {
  let cx = dg_ctx(join([p], ps), u0, x0, dg_rank_opt(ranking), 1)
  dg_out(dg_full_reduce(dg_in(p, cx), map(q => dg_in(q, cx), ps), cx), cx)
}

# delta_polynomial(p, q, u, x): the Delta-polynomial s_q D^a p - s_p D^b q of p and q, whose leaders are
# derivatives of the same function (D^a, D^b raise both leaders to their least common derivative)
fn delta_polynomial(p, q, u0, x0; ranking = orderly) = {
  let cx = dg_ctx([p, q], u0, x0, dg_rank_opt(ranking), 4)
  let r = dg_delta(dg_in(p, cx), dg_in(q, cx), cx)
  if r === none then none else dg_out(r, cx)
}

# --- Rosenfeld-Groebner (g31.9) ---
# A branch is [P, A, H]: equations still to process, the chain (autoreduced, increasing leaders), the inequations.
# Each step takes the lowest equation of P, reduces it by A; a nonzero remainder p with leader v splits the branch
# into p = 0 with initial and separant nonzero (the elements of A with leaders derivatives of v go back to P, the
# Delta-polynomials of p with the rest are added), p = 0 with separant 0, and p = 0 with initial 0. A branch whose
# inequation reduces to 0 or whose equation reduces to a nonzero element of the base field is dropped.

fn dg_lowest(ps, cx) = {
  let ws = map(q => { let l = dg_leader(q, cx); if l === none then -1 else dg_weight(l, cx)*1000 + exponent(q, l[1]) }, ps)
  dg_index(ws, min(ws))
}

fn dg_is_base(p, cx) = length(dg_present(p, cx)) == 0

# whether h is nonzero modulo the chain (its remainder is not 0)
fn dg_nonzero_mod(h, as, cx) = dg_full_reduce(h, as, cx) !== 0

fn dg_rg(ps, hs, cx) = {
  var queue = [[select(map(q => dg_primitive(q, cx), ps), q => q !== 0), [], hs]]
  var out = []
  var count = 0
  while length(queue) > 0 and count < 300 {
    count = count + 1
    let br = queue[1]
    queue = rest(queue)
    let pp = br[1]
    let as = br[2]
    let hh = br[3]
    if length(pp) == 0 then {
      let hr = map(h => dg_full_reduce(h, as, cx), hh)
      if every(hr, h => h !== 0) then out = append(out, [as, unique(select(hr, h => not dg_is_base(h, cx)))])
    } else {
      let k = dg_lowest(pp, cx)
      let p0 = pp[k]
      let rest0 = join(pp[1:k - 1], pp[k + 1:length(pp)])
      let p = dg_full_reduce(p0, as, cx)
      if p === 0 then queue = append(queue, [rest0, as, hh])
      else if dg_is_base(p, cx) then {
        if not is_number(p) then queue = append(queue, [rest0, as, hh])
      } else {
        let v = dg_leader(p, cx)
        let ini = dg_primitive(dg_init(p, v[1]), cx)
        let sep = dg_primitive(dg_sep(p, v[1]), cx)
        # the main branch
        let keep = select(as, a => not dg_is_deriv(dg_leader(a, cx), v))
        let back = select(as, a => dg_is_deriv(dg_leader(a, cx), v))
        let deltas = select(map(a => dg_delta(p, a, cx), keep), q => q !== none)
        let chain = sort_by(append(keep, p), a => dg_weight(dg_leader(a, cx), cx))
        let newh = unique(join(hh, select([ini, sep], q => not dg_is_base(q, cx))))
        queue = append(queue, [join(rest0, back, deltas), chain, newh])
        # the separant vanishes (when it is not the initial's case already)
        if not dg_is_base(sep, cx) then queue = append(queue, [join(rest0, [p, sep]), as, hh])
        if not dg_is_base(ini, cx) and ini !== sep then queue = append(queue, [join(rest0, [p, ini]), as, hh])
      }
    }
  }
  out
}

# rosenfeld_groebner(eqs, u, x): the radical of the differential ideal of eqs as an intersection of regular
# differential chains, each [[p1 == 0, ...], [h1 != 0, ...]]: the chain's equations (increasing leaders) and the
# inequations (initials and separants) on its generic solutions; inequations: [...] adds conditions
fn rosenfeld_groebner(eqs, u0, x0; ranking = orderly, inequations = []) = {
  let es = if is_list(eqs) then eqs else [eqs]
  let cx = dg_ctx(join(es, inequations), u0, x0, dg_rank_opt(ranking), 3)
  let hs = map(h => dg_in(if not is_atom(h) and head(h) === unequal then h[1] - h[2] else h, cx), inequations)
  let bs = dg_rg(map(q => dg_in(q, cx), es), hs, cx)
  map(b => [map(a => dg_out(a, cx) == 0, b[1]), map(h => dg_out(h, cx) != 0, b[2])], bs)
}

# --- Thomas decomposition for one independent variable (g31.10) ---
# A branch is [P, A, N] as in Rosenfeld-Groebner. A remainder r with leader v and degree k splits disjointly:
# initial = 0 (r without its leading term goes back), or initial != 0 and then the discriminant
# resultant(r, r_v, v) = 0 (r is replaced by its square-free part, the discriminant joins P) or != 0, in which case
# r joins the chain. For one derivation every derivative of a leader is reached by differentiation, so a simple
# system needs no further prolongation (Janet division is trivial).

fn dg_sqfree(p, v) = {
  let g = polynomial_gcd(p, d(p, v))
  if free_of(g, v) then p else expand(together(p/g))
}

# the square-free part of p in its leader
fn dg_rad(p, cx) = { let l = dg_leader(p, cx); if l === none then p else dg_sqfree(p, l[1]) }

fn dg_thomas(ps, hs, cx) = {
  var queue = [[select(map(q => dg_primitive(q, cx), ps), q => q !== 0), [], hs]]
  var out = []
  var count = 0
  while length(queue) > 0 and count < 300 {
    count = count + 1
    let br = queue[1]
    queue = rest(queue)
    let pp = br[1]
    let as = br[2]
    let hh = br[3]
    if length(pp) == 0 then {
      let hr = map(h => dg_full_reduce(h, as, cx), hh)
      if every(hr, h => h !== 0) then out = append(out, [as, unique(select(hr, h => not dg_is_base(h, cx)))])
    } else {
      let k = dg_lowest(pp, cx)
      let rest0 = join(pp[1:k - 1], pp[k + 1:length(pp)])
      let r = dg_full_reduce(pp[k], as, cx)
      if r === 0 then queue = append(queue, [rest0, as, hh])
      else if dg_is_base(r, cx) then {
        if not is_number(r) then queue = append(queue, [rest0, as, hh])
      } else {
        let v = dg_leader(r, cx)
        let deg = exponent(r, v[1])
        let ini = dg_rad(dg_primitive(dg_init(r, v[1]), cx), cx)
        let low = expand(r - dg_init(r, v[1])*v[1]^deg)
        let ini_known = dg_is_base(ini, cx) or member(hh, ini)
        if not ini_known then {
          # initial == 0
          queue = append(queue, [join(rest0, [ini, low]), as, hh])
        }
        let hh1 = if ini_known then hh else append(hh, ini)
        let sq = dg_sqfree(r, v[1])
        if sq !== r then queue = append(queue, [append(rest0, sq), as, hh1])
        else {
          let disc = if deg == 1 then 1 else dg_rad(dg_primitive(expand(resultant(r, d(r, v[1]), v[1])), cx), cx)
          let disc_known = dg_is_base(disc, cx) or member(hh1, disc)
          if not disc_known then queue = append(queue, [join(rest0, [disc, r]), as, hh1])
          let hh2 = if disc_known then hh1 else append(hh1, disc)
          let keep = select(as, a => not dg_is_deriv(dg_leader(a, cx), v))
          let back = select(as, a => dg_is_deriv(dg_leader(a, cx), v))
          let chain = sort_by(append(keep, r), a => dg_weight(dg_leader(a, cx), cx))
          queue = append(queue, [join(rest0, back), chain, hh2])
        }
      }
    }
  }
  out
}

# thomas_decomposition(eqs, u, x): the solutions of the ordinary differential system eqs as a disjoint union of
# simple systems [[p1 == 0, ...], [h1 != 0, ...]] (triangular, square-free in the leaders, initials and
# discriminants nonzero); inequations: [...] adds conditions
fn thomas_decomposition(eqs, u0, x0; ranking = orderly, inequations = []) =
  if is_list(x0) and length(x0) > 1 then failure(unsupported, thomas_decomposition, "one independent variable")
  else {
    let es = if is_list(eqs) then eqs else [eqs]
    let cx = dg_ctx(join(es, inequations), u0, x0, dg_rank_opt(ranking), 3)
    let hs = map(h => dg_in(if not is_atom(h) and head(h) === unequal then h[1] - h[2] else h, cx), inequations)
    let bs = dg_thomas(map(q => dg_in(q, cx), es), hs, cx)
    map(b => [map(a => dg_out(a, cx) == 0, b[1]), map(h => dg_out(h, cx) != 0, b[2])], bs)
  }

# --- the Painleve test (g31.11) ---
# Ablowitz, Ramani and Segur (J. Math. Phys. 21, 1980): leading orders y ~ a tau^p, tau = x - x0, from the
# balances of two or more terms; resonances, the roots of Q(r) from the linearisation of the dominant terms at
# a tau^p + b tau^(p + r); compatibility conditions at the positive integer resonances from the Laurent series
# substituted into the equation (the coefficient of each power solved in turn, the arbitrary ones checked).

# d^k/dtau^k of tau^p, as a coefficient: p (p - 1) ... (p - k + 1)
fn dg_ff(p, k) = product(table(p - j, [j, 0, k - 1]))

# the exponent alpha + beta p of a term in the jets ys (ys[k + 1] = y^(k)): [alpha, beta]
fn dg_term_exp(t, ys) = [-sum(k*exponent(t, ys[k + 1]), [k, 0, length(ys) - 1]), sum(exponent(t, ys[k + 1]), [k, 0, length(ys) - 1])]

fn dg_painleve_branch(e, ys, x, p, dom, x0) = {
  let tau = to_expression("dg_tau")
  let a = to_expression("dg_a0")
  let lead = expand(sum(rewrite(rewrite(t, x -> x0), table(ys[k + 1] -> a*dg_ff(p, k), [k, 0, length(ys) - 1])), [t, dom]))
  let asol = select(map(q => q[1][2], dg_try(solve(lead == 0, a))), q => q !== 0)
  map(a0 => {
    # Q(r): the linear part of the dominant terms at a0 tau^p + eps tau^(p + r)
    let rr = to_expression("dg_r")
    let lin = sum(sum(rewrite(rewrite(d(t, ys[k + 1]), x -> x0), table(ys[j + 1] -> a0*dg_ff(p, j), [j, 0, length(ys) - 1]))*dg_ff(p + rr, k), [k, 0, length(ys) - 1]), [t, dom])
    let qr = factor(expand(lin))
    let rs = if free_of(qr, rr) then [] else map(q => q[1][2], dg_try(solve(qr == 0, rr)))
    let ints = select(rs, q => is_integer(q) and q > 0)
    let ok_res = every(rs, q => is_integer(q))
    if not is_integer(p) or not ok_res or length(ints) == 0 then
      [leading_exponent -> p, leading_coefficient -> a0, resonances -> rs, conditions -> [], passes -> (is_integer(p) and ok_res)]
    else {
      let top = max(ints)
      let cs = table(to_expression(string_join(["dg_c_", to_string(j)])), [j, 1, top])
      let yser = a0*tau^p + sum(cs[j]*tau^(p + j), [j, 1, top])
      let sub = table(ys[k + 1] -> d(yser, [tau, k]), [k, 0, length(ys) - 1])
      let m = min(map(t => dg_term_exp(t, ys)[1] + dg_term_exp(t, ys)[2]*p, dom))
      let full = expand(rewrite(rewrite(e, x -> x0 + tau), sub)*tau^(-m))
      let ser = series(full, tau, 0, top)
      var vals = []
      var conds = []
      for j in range(1, top) {
        let ej = expand(rewrite(series_coefficient(ser, j), vals))
        if member(ints, j) then {
          let cj = simplify(ej)
          if cj !== 0 then conds = append(conds, cj)
        } else {
          let s = dg_try(solve(ej == 0, cs[j]))
          if dg_ok(s) then vals = append(vals, cs[j] -> simplify(s[1][1][2]))
        }
      }
      [leading_exponent -> p, leading_coefficient -> a0, resonances -> rs, conditions -> conds, passes -> length(conds) == 0]
    }
  }, asol)
}

# painleve_test(eq, y, x): the Painleve test of the ordinary differential equation eq (polynomial in y and its
# derivatives) at a movable singularity x0: for each dominant balance the rules leading_exponent, leading_coefficient,
# resonances, conditions (the compatibility conditions not satisfied: expressions that must vanish) and passes
fn painleve_test(eq, y0, x; point = x0) = {
  let y = dg_deps(y0)[1]
  let top = dg_order(dg_lhs(eq), [y], [x])
  let jets = dg_jets([y], [x], top)
  let ys = map(r => r[1], jets)
  let e = expand(numerator(together(rewrite(dg_lhs(eq), dg_to_rules(jets, [y], [x])))))
  let ts = select(dg_terms(e), t => some(ys, v => not free_of(t, v)))
  let ex = map(t => dg_term_exp(t, ys), ts)
  var ps = []
  for i in range(1, length(ts)) {
    for j in range(i + 1, length(ts)) {
      if ex[i][2] != ex[j][2] then {
        let p = (ex[j][1] - ex[i][1])/(ex[i][2] - ex[j][2])
        let vs = map(q => q[1] + q[2]*p, ex)
        if p < 0 and ex[i][1] + ex[i][2]*p == min(vs) and not member(ps, p) then ps = append(ps, p)
      }
    }
  }
  flatten(map(p => {
    let vs = map(q => q[1] + q[2]*p, ex)
    let dom = select(range(1, length(ts)), i => vs[i] == min(vs))
    let br = dg_painleve_branch(e, ys, x, p, map(i => ts[i], dom), point)
    map(b => rewrite(b, dg_back_rule(jets, y, x)), br)
  }, sort(ps)), 1)
}

fn dg_back_rule(jets, y, x) = dg_from_rules(jets, [y], [x])
