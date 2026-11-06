#! mathcas 2
# The electroweak sphaleron's static profile and a path through it (a port of a research notebook).
#
# With the gauge profile f(xi) and the Higgs profile h(xi) (xi = g v r), the static field equations are
#   f'' = 2 f (1 - f)(1 - 2 f)/xi^2 - h^2 (1 - f)/4,
#   h'' = -2 h'/xi + 2 h (1 - f)^2/xi^2 + beta h (h^2 - 1),     beta = (m_H/m_W)^2/2,
# a two-point boundary value problem on [xi_min, xi_max] = [0.1, 6]: f = 0.25 xi^2 and h = 0.4 xi at xi_min (the
# small-xi behaviour), f = h = 1 at xi_max, for beta = 0.1 (the notebook shoots at 100 digits). It prints the slopes at
# xi_min and the profile at a few points, writes the profile to sphaleron_profile.csv, draws it (sphaleron_static.pdf,
# .png), and writes the path t (f, h) (t <= 1), (2 - t)(f, h) (t > 1) that winds through the sphaleron at t = 1 as an
# animated GIF of 181 frames (sphaleron_winding.gif).
# Run: mc-repl examples/sphaleron.mc  (about five seconds, nearly all of it the 181 frames of the animation).

say("beta for m_H = 125 GeV, m_W = 80.4 GeV: ", (125/80.4)^2/2.0)
beta = 0.1
xi_min = 0.1
xi_max = 6

# the boundary value problem, solved by n_dsolve with the conditions at both ends
stage_solve = timing(n_dsolve([
    f''(x) == 2*f(x)*(1 - f(x))*(1 - 2*f(x))/x^2 - h(x)^2*(1 - f(x))/4,
    h''(x) == -2*h'(x)/x + 2*h(x)*(1 - f(x))^2/x^2 + beta*h(x)*(h(x)^2 - 1),
    f(xi_min) == 0.25*xi_min^2, h(xi_min) == 0.4*xi_min, f(xi_max) == 1, h(xi_max) == 1],
  [f, h], [x, xi_min, xi_max], precision_goal: 12, accuracy_goal: 14))
sol = stage_solve[2]
say("f'(xi_min) = ", f'(xi_min) /. sol, ", h'(xi_min) = ", h'(xi_min) /. sol)
for x0 in [1, 3, 6] { say("xi = ", x0, ": f = ", f(x0) /. sol, ", h = ", h(x0) /. sol) }

stage_out = timing({
  export_data("sphaleron_profile.csv", table([x, f(x), h(x)] /. sol, x, range(xi_min, xi_max, 0.01)))
  let p = plot([f(x) /. sol, h(x) /. sol], [x, xi_min, xi_max], x_label: r"$\xi$", legend: [r"$f(\xi)$", r"$h(\xi)$"],
               legend_position: bottom_right)
  export_data("sphaleron_static.pdf", p)
  export_data("sphaleron_static.png", p)
  # the winding path as an animated GIF: t = 0.1, 0.11, ..., 1.9 (181 frames, as in the notebook)
  let frames = map(k => {
    let t = k/100
    let c = if t <= 1 then t else 2 - t
    plot([c*f(x) /. sol, c*h(x) /. sol], [x, xi_min, xi_max], plot_range: [[xi_min, xi_max], [0, 1]], x_label: r"$\xi$",
         legend: [r"$c(t)\,f(\xi)$", r"$c(t)\,h(\xi)$"], legend_title: string_join([r"$t = ", to_string(n(t, 3)), "$"]),
         legend_position: top_left)
  }, range(10, 190))
  export_animation("sphaleron_winding.gif", frames)
})
say("stage times (s): boundary value problem ", stage_solve[1], ", profile, plot and animation ", stage_out[1])