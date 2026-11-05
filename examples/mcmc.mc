#! mathcas 2
# Bayesian fit of an ordinary differential equation by Markov chain Monte Carlo (a port of a research notebook).
# The model y' = s - k y, y(0) = 0 is solved numerically for each parameter pair (s, k); its value y(1) is compared
# with a measurement 2 +- 0.1 (chi-square), under a flat prior on 0 < s < 6, 0.05 < k < 3. A random-walk Metropolis
# chain of 10^4 steps (Gaussian proposals of width 0.15, started at (1, 1), seed 11) samples the posterior; the
# script prints the acceptance rate, posterior means, standard deviations and 90% intervals, and writes the
# scatter plot of the chain as mcmc_scatter.png and mcmc_scatter.pdf in the current folder.
# Run: mc-repl examples/mcmc.mc  (under a second; each stage prints its time).

# --- 1. the model and the posterior

seed(11)

# y(1) for y' = s - k y, y(0) = 0 as a function of the parameters s and k (exactly s/k (1 - e^-k)): the equation
# is reduced once, and each call f(s, k) integrates it numerically
f = parametric_n_dsolve_value([y'(t) == s - k*y(t), y(0) == 0], y(1), [t, 0, 1], [s, k])

mu = 2
sigma = 0.1
fn chi_square(th) = ((f(th[1], th[2]) - mu)/sigma)^2
# the flat prior: log 1 inside the box, log 0 outside
fn log_prior(th) = if 0 < th[1] and th[1] < 6 and 0.05 < th[2] and th[2] < 3 then 0 else -inf
# -2 log posterior (up to a constant); inf outside the prior's support
fn q(th) = if log_prior(th) === -inf then inf else chi_square(th) - 2*log_prior(th)

say("f(2, 1) = ", f(2.0, 1.0), "   exact ", n(2*(1 - exp(-1))))
say("chi^2 at (2, 1) = ", chi_square([2.0, 1.0]))

# --- 2. the Metropolis chain

# one step: a Gaussian proposal, accepted with probability min(1, exp(-(Q(prop) - Q(pt))/2)); a proposal outside
# the prior has Q = inf, so log r = -inf and it is always rejected
fn next_sample(pt) = {
  let prop = pt + 0.15*random_variate(normal_distribution(), 2)
  let lnr = -1/2*(q(prop) - q(pt))
  if log(random_real()) < lnr then prop else pt
}

nsteps = 10^4
run = timing(nest_list(next_sample, [1, 1], nsteps))
samples = run[2]
say("sampling: ", nsteps, " steps in ", run[1], " s")

# --- 3. posterior summaries

summaries = timing({
  let accepted = count_if(range(nsteps), i => samples[i + 1] !== samples[i])
  say("acceptance rate = ", n(accepted/nsteps))
  say("posterior mean [s, k] = ", mean(samples))
  say("posterior sd   [s, k] = ", std_dev(samples))
  say("5%, 50%, 95% quantiles of [s, k] = ", quantile(samples, [0.05, 0.5, 0.95]))
  say("mean y(1) over the chain = ", mean(map(th => f(th[1], th[2]), samples)))
  say("last sample = ", samples[-1])
})
say("summaries: ", summaries[1], " s")

# --- 4. the scatter plot of the chain (axis labels in LaTeX are not available yet)

plots = timing({
  let scatter = list_plot(samples)
  export_data("mcmc_scatter.png", scatter)
  export_data("mcmc_scatter.pdf", scatter)
})
say("plot: ", plots[1], " s")
