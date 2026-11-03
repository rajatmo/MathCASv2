#! mathcas 2
# A module file: `import vectors` in a script of this folder loads it. Members marked `pub` are exported; the
# others are private to the module.

module vectors {
  # u + v and k*v, componentwise
  pub fn plus(u, v) = zip(u, v) |> map(p => p[1] + p[2], _)
  pub fn times(k, v) = map(x => k*x, v)

  # the angle between u and v, through the cosine
  pub fn angle(u, v) = acos(cosine(u, v))

  # private: the cosine of the angle (dot and norm come from the prelude)
  fn cosine(u, v) = dot(u, v)/(norm(u)*norm(v))

  # the unit vectors of the plane
  pub e1 = [1, 0]
  pub e2 = [0, 1]
}
