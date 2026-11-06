#! mathcas 2
# The electroweak sphaleron's static profile and a path through it (a port of a research notebook).
#
# With the gauge profile f(xi) and the Higgs profile h(xi) (xi = g v r), the static field equations are
#   f'' = 2 f (1 - f)(1 - 2 f)/xi^2 - h^2 (1 - f)/4,
#   h'' = -2 h'/xi + 2 h (1 - f)^2/xi^2 + beta h (h^2 - 1),     beta = (m_H/m_W)^2/2,
# a two-point boundary value problem on [xi_min, xi_max] = [0.1, 6]: f = 0.25 xi^2 and h = 0.4 xi at xi_min (the
# small-xi behaviour), f = h = 1 at xi_max. The notebook solves it by shooting for beta = 0.1; so does the script: the
# unknown slopes f'(xi_min), h'(xi_min) are found by find_root, each trial an initial value problem (GAP G28: n_dsolve
# takes no boundary conditions at two points). It prints the slopes and the profile at a few points, writes the profile
# to sphaleron_profile.csv, draws it (sphaleron_static.pdf, .png), and draws ten frames of the path
# t (f, h) (t <= 1), (2 - t)(f, h) (t > 1) that winds through the sphaleron at t = 1 (sphaleron_frame_k.png; the
# notebook writes 181 frames as an animated GIF: GAP G26).
# Run: mc-repl examples/sphaleron.mc  (about a second).

say("beta for m_H = 125 GeV, m_W = 80.4 GeV: ", (125/80.4)^2/2.0)
beta = 0.1
xi_min = 0.1
xi_max = 6

# the profile for starting slopes [a, b] = [f'(xi_min), h'(xi_min)]
fn profile(a, b) = n_dsolve([
    f''(x) == 2*f(x)*(1 - f(x))*(1 - 2*f(x))/x^2 - h(x)^2*(1 - f(x))/4,
    h''(x) == -2*h'(x)/x + 2*h(x)*(1 - f(x))^2/x^2 + beta*h(x)*(h(x)^2 - 1),
    f(xi_min) == 0.25*xi_min^2, h(xi_min) == 0.4*xi_min, f'(xi_min) == a, h'(xi_min) == b],
  [f, h], [x, xi_min, xi_max], precision_goal: 12, accuracy_goal: 14)
# the misses at xi_max (find_root needs the two components as two expressions; each is a rule that waits for numbers)
rule miss(a_, b_) when is_number(a) and is_number(b) := {
  let s = profile(a, b)
  [(f(xi_max) /. s) - 1, (h(xi_max) /. s) - 1]
}
rule miss_f(a_, b_) when is_number(a) and is_number(b) := miss(a, b)[1]
rule miss_h(a_, b_) when is_number(a) and is_number(b) := miss(a, b)[2]

# start from the slopes of the small-xi forms (0.5 xi_min and 0.4)
stage_shoot = timing(find_root([miss_f(a, b), miss_h(a, b)], [[a, 0.05], [b, 0.4]]))
slopes = [a, b] /. stage_shoot[2]
sol = profile(slopes[1], slopes[2])
say("f'(xi_min) = ", slopes[1], ", h'(xi_min) = ", slopes[2], "; misses at xi_max: ", miss(slopes[1], slopes[2]))
for x0 in [1, 3, 6] { say("xi = ", x0, ": f = ", f(x0) /. sol, ", h = ", h(x0) /. sol) }

stage_out = timing({
  export_data("sphaleron_profile.csv", table([x, f(x), h(x)] /. sol, x, range(xi_min, xi_max, 0.01)))
  let p = plot([f(x) /. sol, h(x) /. sol], [x, xi_min, xi_max], x_label: r"$\xi$", legend: [r"$f(\xi)$", r"$h(\xi)$"],
               legend_position: bottom_right)
  export_data("sphaleron_static.pdf", p)
  export_data("sphaleron_static.png", p)
  # the winding path: ten frames, t = 0.1, 0.3, ..., 1.9
  for k in range(10) {
    let t = (2*k - 1)/10
    let c = if t <= 1 then t else 2 - t
    export_data(string_join(["sphaleron_frame_", to_string(k), ".png"]),
                plot([c*f(x) /. sol, c*h(x) /. sol], [x, xi_min, xi_max], plot_range: [[xi_min, xi_max], [0, 1]],
                     x_label: r"$\xi$", legend: [string_join([r"$c(t)\,f(\xi)$, $t = ", to_string(t), "$"]), r"$c(t)\,h(\xi)$"],
                     legend_position: top_left))
  }
})
say("stage times (s): shooting ", stage_shoot[1], ", profile, plots and frames ", stage_out[1])
