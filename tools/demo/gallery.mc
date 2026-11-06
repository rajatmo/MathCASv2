# The README's plot gallery: run from the repository root with
#     mc-repl tools/demo/gallery.mc
# It writes four PNG pictures into docs/media/gallery/.
p1 = plot([bessel_j(0, x), bessel_j(1, x), bessel_j(2, x)], [x, 0, 15],
          x_label: r"$x$", y_label: r"$J_n(x)$", legend: [r"$J_0$", r"$J_1$", r"$J_2$"])
export_data("docs/media/gallery/bessel.png", p1)
p2 = plot3d(sin(x)*cos(y)*exp(-(x^2 + y^2)/8), [x, -4, 4], [y, -4, 4], color_function: viridis, mesh: none,
            axes_label: [r"$x$", r"$y$", r"$z$"], image_size: 560)
export_data("docs/media/gallery/surface.png", p2)
p3 = complex_plot((z^3 - 1)/(z^2 + 1), [z, -2 - 2im, 2 + 2im])
export_data("docs/media/gallery/complex.png", p3)
p4 = contour_plot(sin(x) + cos(y), [x, 0, 2pi], [y, 0, 2pi], x_label: r"$x$", y_label: r"$y$")
export_data("docs/media/gallery/contour.png", p4)
