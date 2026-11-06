#! mathcas 2
# Two growing bubbles merging, as an animation (a port of a short research notebook).
#
# Bubbles of radii R1 = 2t and R2 = t at distance d = 5 grow from t = 1 to 3; each frame draws the outer outline of
# the two bubbles (the arcs outside the overlap) over a fixed field of random points, and the frames are written as
# an animated GIF (and the first and last frames as PNG).
# Run: mc-repl examples/bubble_collision_animation.mc

seed(3)
# the half-angle of the arc of the first circle inside the second
fn half_angle(d, r1, r2) = acos(min((d^2 + r1^2 - r2^2)/(2*d*r1), 1))
background = point(table([random_real([-7, 9]), random_real([-7, 7])], i, range(0, 200)))
fn two_bubbles(d, r1, r2) = graphics([circle([0, 0], r1, [half_angle(d, r1, r2), 2*pi - half_angle(d, r1, r2)]),
                                      circle([d, 0], r2, [pi + half_angle(d, r2, r1), 3*pi - half_angle(d, r2, r1)]),
                                      background], plot_range: [[-7, 9], [-7, 7]], image_size: 400)
stage = timing({
  let frames = table(two_bubbles(5, 2*t, t), t, range(1, 3, 0.05))
  export_data("bubble_frame_first.png", first(frames))
  export_data("bubble_frame_last.png", last(frames))
  export_animation("bubble_collision.gif", frames)
  length(frames)
})
say(stage[2], " frames in ", stage[1], " s")
