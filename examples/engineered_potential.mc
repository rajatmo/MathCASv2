#! mathcas 2
# The largest admissible beta of an engineered potential (a port of a short research notebook): with
# T(y, lambda) = 2 y/sqrt(lambda), beta_max = (sqrt(2 - T^2) - T)/2, real for T^2 <= 2 and positive for T < 1. The
# script draws the zero contour of beta_max in the (y, lambda) unit square, the curve y = sqrt(lambda)/2 (written as
# engineered_potential.pdf and .png in the current folder), and checks a point on it.
# Run: mc-repl examples/engineered_potential.mc  (a fraction of a second).

fn t_of(y, lam) = 2*y/sqrt(lam)
fn beta_max(y, lam) = (sqrt(2 - t_of(y, lam)^2) - t_of(y, lam))/2

say("beta_max on the curve y = sqrt(lambda)/2, lambda = 0.49: ", beta_max(0.35, 0.49))
say("beta_max at (y, lambda) = (0.1, 0.5): ", beta_max(0.1, 0.5), "; at (0.6, 0.5), where T^2 > 2: ", beta_max(0.6, 0.5))

stage = timing({
  let p = contour_plot(beta_max(y, lam), [y, 0, 1], [lam, 0.001, 1], contours: [0],
                       x_label: r"$y$", y_label: r"$\lambda$")
  export_data("engineered_potential.pdf", p)
  export_data("engineered_potential.png", p)
})
say("plot: ", stage[1], " s")
