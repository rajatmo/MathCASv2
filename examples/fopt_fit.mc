#! mathcas 2
# A fit of the progress of a first-order phase transition (a port of a research notebook).
#
# A simulation gives the fraction P(T) of space converted to the new phase as a function of the temperature. The
# notebook fits it with P(T) = (1 + tanh((T/T_b - 1)/b))/2, where T_b is tied to the percolation temperature T_p by
# P(T_p) = 0.7, i.e. T_b = T_p/(1 + b artanh(0.4)). T_p comes from the interpolated data (P = 0.7), then b from a
# least-squares fit with T_p fixed. The script prints T_p, b and the fitted function, writes them to
# fopt_fit_parameters.csv, and draws the fit with the data (fopt_fit.pdf and .png in the current folder).
#
# Data: the simulation's table (two columns T [GeV], P, tab- or space-separated) read from
# T-P(t)-Lambda=586-vw=0.55.dat in the current folder; without it the script makes a stand-in table from the fit of
# the original data (T_p = 21.268, b = 0.12822) with a small deterministic ripple, so that it runs anywhere.
# Run: mc-repl examples/fopt_fit.mc  (a fraction of a second).

fn p_model(t, tp, b) = (1 + tanh((t/(tp/(1 + b*atanh(2*0.7 - 1))) - 1)/b))/2

file = "T-P(t)-Lambda=586-vw=0.55.dat"
# (a failure assigned to a name fails the assignment, so the test is on the call itself)
have_data = not is_failure(import_data(file, "table"))
data = if not have_data then table([t, p_model(t, 21.268, 0.12822) + 1.0e-3*sin(7.0*t)*exp(-(t - 21.268)^2/4)],
                                      t, range(0, 64.2, 0.2))
       else import_data(file, "table")
say(if have_data then "data read from the file" else "no data file: a stand-in table", ", ", length(data), " points")


stage_fit = timing({
  # T_p where the interpolated data reach 0.7
  let ip = interpolation(data)
  let tp = t /. find_root(ip(t) - 0.7, [t, 20])
  # b by least squares with T_p fixed
  let fit = find_fit(data, p_model(t, tp, bb), [[bb, 0.1]], t)
  let bv = bb /. fit
  [tp, bv, total(map(pt => (p_model(pt[1], tp, bv) - pt[2])^2, data))]
})
tp = stage_fit[2][1]
b = stage_fit[2][2]
say("T_p = ", tp, " GeV, b = ", b, " (sum of squared residuals ", stage_fit[2][3], ")")
say("P(T) = (1 + tanh(", 1/b, " (", (1 + b*atanh(0.4))/tp, " T - 1)))/2")
export_data("fopt_fit_parameters.csv", [["T_p", tp], ["b", b]])

stage_plot = timing({
  let lo = tp*(1 - 5*b)
  let hi = tp*(1 + 5*b)
  let points = select(data, pt => lo <= pt[1] and pt[1] <= hi)
  # the fitted curve, with the simulation's points drawn over it
  let p = plot(p_model(t, tp, b), [t, lo, hi], colors: ["#560591"], x_label: r"$T$ (GeV)", legend: [r"$f^S(T)$"],
               legend_title: "FOPT progression", legend_position: bottom_right, epilog: [point(points)])
  export_data("fopt_fit.pdf", p)
  export_data("fopt_fit.png", p)
})
say("stage times (s): fit ", stage_fit[1], ", plot ", stage_plot[1])
