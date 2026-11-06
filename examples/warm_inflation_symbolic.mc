#! mathcas 2
# The slow-roll temperature of warm inflation in closed form (a port of a short research notebook).
#
# In slow roll phi' = -V'/(3H + Upsilon), and the radiation produced by the friction Upsilon = 3 H Q balances its
# dilution, 4 H rho_R = Upsilon phi'^2 with rho_R = alpha T^4. For V = lam phi^k/Mp^(4 - k) the script solves for
# rho_R = alpha T^4, takes the k = 4 temperature, and inverts it for the field value at a given T.
# Run: mc-repl examples/warm_inflation_symbolic.mc

stage = timing({
  let phidot = -v1/(3*h + ups)
  let rho_r = al*t^4
  let hub = sqrt(v/(3*mp^2))
  let pot = [v -> lam*phi^k/mp^(4 - k), v1 -> lam*k*phi^(k - 1)/mp^(4 - k)]
  # Upsilon = 3 H Q
  let eq = rewrite(4*h*rho_r == ups*phidot^2, [ups -> 3*h*q]) /. [h -> hub] /. pot
  let sols = assuming(t > 0 and q > 0 and lam > 0 and phi > 0 and mp > 0 and al > 0, solve(eq, t))
  # exponents collected with expand   # GAP G51 (simplify leaves mp^(-2*(4 - k) - (-2 - (4 - k))))
  let rho = unique(map(sl => power_expand(simplify(rho_r /. sl)) /. [mp^e_ :> mp^expand(e), phi^e_ :> phi^expand(e)],
                       sols))
  say("rho_R = alpha T^4 = ", rho)
  # the temperature for k = 4 and the field value where the bath has temperature T
  let t4 = power_expand((first(rho)/al)^(1/4) /. k -> 4)
  say("T(k = 4) = ", simplify(t4))
  # the positive root (solve gives both signs although phi > 0 is assumed   # GAP G51)
  let roots = assuming(t > 0 and q > 0 and lam > 0 and mp > 0 and al > 0 and phi > 0,
                       phi /. solve(al*t^4 == (first(rho) /. k -> 4), phi))
  let pos = select(roots, r => (r /. [al -> 1, t -> 1, q -> 1, lam -> 1, mp -> 1]) > 0)
  say("phi(T, k = 4) = ", power_expand(factor(first(pos))))
})
say("time (s): ", stage[1])
