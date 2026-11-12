//! Geometric transformations as functions: `rotation_transform`, `translation_transform`, `scaling_transform`,
//! `shearing_transform`, `reflection_transform`, `affine_transform`, `rescaling_transform` evaluate to
//! `transformation_function(m)`, with `m` the homogeneous (n + 1) x (n + 1) matrix of the affine map; a
//! `transformation_function(m)(p)` call maps a point (or each point of a list), and `transformation_matrix(f)` gives
//! `m`. Entries are built as terms and evaluated, so symbolic angles and points stay exact (`cos(theta)`, ...).
//! Rotations about an axis use Rodrigues' formula; rotations in the plane of two vectors use
//! `I + (cos t - 1)(u u' + v v') + sin t (v u' - u v')` for the orthonormalised pair (Gram-Schmidt).
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// Register the natives.
pub fn install(k: &mut Kernel, h: &Heap) {
    for name in [
        "rotation_transform",
        "translation_transform",
        "scaling_transform",
        "shearing_transform",
        "reflection_transform",
        "affine_transform",
        "rescaling_transform",
        "transformation_matrix",
    ] {
        k.register(h.sym(name), Attributes::default(), Box::new(Make(name)));
    }
    k.register(h.sym("transformation_function"), Attributes::default(), Box::new(Apply));
    k.wrap_native(h.sym("normal"), |inner| Box::new(NormalGeometry { inner }));
}

/// The registrations that wrap natives of crates installed after this one (`dot`): called once they are in.
pub fn install_late(k: &mut Kernel, h: &Heap) {
    k.wrap_native(h.sym("dot"), |inner| Box::new(DotTransforms { inner }));
}

/// Terms and their arithmetic (evaluated at the end).
#[derive(Clone, Copy)]
struct B<'h> {
    h: &'h Heap,
}

impl<'h> B<'h> {
    fn f(&self, n: &str, a: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(n), a)
    }
    fn add(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.f(names::ADD, &[a, b])
    }
    fn mul(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.f(names::MUL, &[a, b])
    }
    fn neg(&self, a: T<'h>) -> T<'h> {
        self.mul(self.h.int(-1), a)
    }
    fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.add(a, self.neg(b))
    }
    fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.mul(a, self.f(names::POW, &[b, self.h.int(-1)]))
    }
    fn int(&self, k: i64) -> T<'h> {
        self.h.int(k)
    }
    fn dot(&self, u: &[T<'h>], v: &[T<'h>]) -> T<'h> {
        u.iter().zip(v).fold(self.int(0), |s, (&a, &b)| self.add(s, self.mul(a, b)))
    }
    /// `v / |v|`
    fn unit(&self, v: &[T<'h>]) -> Vec<T<'h>> {
        let n = self.f("sqrt", &[self.dot(v, v)]);
        v.iter().map(|&a| self.div(a, n)).collect()
    }
    fn identity(&self, n: usize) -> Vec<Vec<T<'h>>> {
        (0..n).map(|i| (0..n).map(|j| self.int((i == j) as i64)).collect()).collect()
    }
    /// The homogeneous matrix of `x -> m x + b`.
    fn affine(&self, m: &[Vec<T<'h>>], b: &[T<'h>]) -> Vec<Vec<T<'h>>> {
        let n = m.len();
        let mut out: Vec<Vec<T<'h>>> = m.iter().zip(b).map(|(r, &bi)| r.iter().copied().chain([bi]).collect()).collect();
        out.push((0..=n).map(|j| self.int((j == n) as i64)).collect());
        out
    }
    /// `m` about the point `p`: `x -> m (x - p) + p`.
    fn about(&self, m: &[Vec<T<'h>>], p: Option<&[T<'h>]>) -> Vec<Vec<T<'h>>> {
        let n = m.len();
        let b: Vec<T<'h>> = match p {
            Some(p) => (0..n).map(|i| self.f("expand", &[self.sub(p[i], self.dot(&m[i], p))])).collect(),
            None => vec![self.int(0); n],
        };
        self.affine(m, &b)
    }
}

fn vector<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<T<'h>>> {
    let v = cx.as_list(t)?;
    (!v.is_empty() && v.iter().all(|&e| cx.as_list(e).is_none())).then(|| v.to_vec())
}

fn matrix<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Vec<T<'h>>>> {
    let rows = cx.as_list(t)?;
    let m: Vec<Vec<T<'h>>> = rows.iter().map(|&r| vector(cx, r)).collect::<Option<_>>()?;
    (!m.is_empty() && m.iter().all(|r| r.len() == m[0].len())).then_some(m)
}

/// The rotation by `t` in the plane of `u` and `v` (n dimensions).
fn plane_rotation<'h>(b: B<'h>, t: T<'h>, u: &[T<'h>], v: &[T<'h>]) -> Vec<Vec<T<'h>>> {
    let n = u.len();
    let u = b.unit(u);
    let uv = b.dot(&u, v);
    let w: Vec<T<'h>> = v.iter().zip(&u).map(|(&vi, &ui)| b.sub(vi, b.mul(uv, ui))).collect();
    let v = b.unit(&w);
    let (c1, s) = (b.sub(b.f("cos", &[t]), b.int(1)), b.f("sin", &[t]));
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let id = b.int((i == j) as i64);
                    let sym = b.add(b.mul(u[i], u[j]), b.mul(v[i], v[j]));
                    let skew = b.sub(b.mul(v[i], u[j]), b.mul(u[i], v[j]));
                    b.add(id, b.add(b.mul(c1, sym), b.mul(s, skew)))
                })
                .collect()
        })
        .collect()
}

/// Rodrigues: the rotation by `t` about the axis `w` in space.
fn axis_rotation<'h>(b: B<'h>, t: T<'h>, w: &[T<'h>]) -> Vec<Vec<T<'h>>> {
    let w = b.unit(w);
    let (c, s) = (b.f("cos", &[t]), b.f("sin", &[t]));
    let k = b.sub(b.int(1), c);
    let cross = |i: usize, j: usize| -> T<'h> {
        // the (i, j) entry of the cross-product matrix [w]x
        match (i, j) {
            (0, 1) => b.neg(w[2]),
            (0, 2) => w[1],
            (1, 0) => w[2],
            (1, 2) => b.neg(w[0]),
            (2, 0) => b.neg(w[1]),
            (2, 1) => w[0],
            _ => b.int(0),
        }
    };
    (0..3)
        .map(|i| (0..3).map(|j| b.add(b.add(if i == j { c } else { b.int(0) }, b.mul(s, cross(i, j))), b.mul(k, b.mul(w[i], w[j])))).collect())
        .collect()
}

struct Make(&'static str);

impl Native for Make {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B { h };
        if self.0 == "transformation_matrix" {
            let [f] = args else { return Ok(Reply::Decline) };
            return Ok(match h.view(*f) {
                View::App { head, args: &[m] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "transformation_function") => Reply::Done(m),
                _ => Reply::Decline,
            });
        }
        let m: Vec<Vec<T<'h>>> = match (self.0, args) {
            ("rotation_transform", [t]) => {
                let two = [b.int(1), b.int(0)];
                b.about(&plane_rotation(b, *t, &two, &[b.int(0), b.int(1)]), None)
            }
            ("rotation_transform", [t, x]) => match (vector(cx, *x), matrix(cx, *x)) {
                // about a point of the plane
                (Some(p), _) if p.len() == 2 => b.about(&plane_rotation(b, *t, &[b.int(1), b.int(0)], &[b.int(0), b.int(1)]), Some(&p)),
                (Some(w), _) if w.len() == 3 => b.about(&axis_rotation(b, *t, &w), None),
                (_, Some(uv)) if uv.len() == 2 => b.about(&plane_rotation(b, *t, &uv[0], &uv[1]), None),
                _ => return Ok(Reply::Decline),
            },
            ("rotation_transform", [t, x, p]) => match (vector(cx, *x), matrix(cx, *x), vector(cx, *p)) {
                (Some(w), _, Some(p)) if w.len() == 3 && p.len() == 3 => b.about(&axis_rotation(b, *t, &w), Some(&p)),
                (_, Some(uv), Some(p)) if uv.len() == 2 && p.len() == uv[0].len() => b.about(&plane_rotation(b, *t, &uv[0], &uv[1]), Some(&p)),
                _ => return Ok(Reply::Decline),
            },
            ("translation_transform", [v]) => {
                let Some(v) = vector(cx, *v) else { return Ok(Reply::Decline) };
                b.affine(&b.identity(v.len()), &v)
            }
            ("scaling_transform", [s, rest @ ..]) if rest.len() <= 2 => match (vector(cx, *s), rest) {
                // factors per axis, about the origin or a point
                (Some(s), []) => b.about(&diag(b, &s), None),
                (Some(s), [p]) => {
                    let Some(p) = vector(cx, *p) else { return Ok(Reply::Decline) };
                    b.about(&diag(b, &s), Some(&p))
                }
                // a factor along a direction
                (None, [v, ..]) => {
                    let Some(v) = vector(cx, *v) else { return Ok(Reply::Decline) };
                    let u = b.unit(&v);
                    let k = b.sub(*s, b.int(1));
                    let m: Vec<Vec<T<'h>>> =
                        (0..u.len()).map(|i| (0..u.len()).map(|j| b.add(b.int((i == j) as i64), b.mul(k, b.mul(u[i], u[j])))).collect()).collect();
                    let p = rest.get(1).and_then(|&p| vector(cx, p));
                    b.about(&m, p.as_deref())
                }
                _ => return Ok(Reply::Decline),
            },
            ("shearing_transform", [t, u, v, rest @ ..]) if rest.len() <= 1 => {
                let (Some(u), Some(v)) = (vector(cx, *u), vector(cx, *v)) else { return Ok(Reply::Decline) };
                if u.len() != v.len() {
                    return Ok(Reply::Decline);
                }
                let vn = b.unit(&v);
                let uv = b.dot(&u, &vn);
                let w: Vec<T<'h>> = u.iter().zip(&vn).map(|(&ui, &vi)| b.sub(ui, b.mul(uv, vi))).collect();
                let un = b.unit(&w);
                let tn = b.f("tan", &[*t]);
                let m: Vec<Vec<T<'h>>> = (0..u.len()).map(|i| (0..u.len()).map(|j| b.add(b.int((i == j) as i64), b.mul(tn, b.mul(un[i], vn[j])))).collect()).collect();
                let p = rest.first().and_then(|&p| vector(cx, p));
                b.about(&m, p.as_deref())
            }
            ("reflection_transform", [v, rest @ ..]) if rest.len() <= 1 => {
                let Some(v) = vector(cx, *v) else { return Ok(Reply::Decline) };
                let vv = b.dot(&v, &v);
                let m: Vec<Vec<T<'h>>> = (0..v.len())
                    .map(|i| (0..v.len()).map(|j| b.sub(b.int((i == j) as i64), b.div(b.mul(b.int(2), b.mul(v[i], v[j])), vv))).collect())
                    .collect();
                let p = rest.first().and_then(|&p| vector(cx, p));
                b.about(&m, p.as_deref())
            }
            ("affine_transform", [m]) => match matrix(cx, *m) {
                Some(m) if m.len() == m[0].len() => b.about(&m, None),
                _ => match cx.as_list(*m) {
                    Some(&[m, v]) => match (matrix(cx, m), vector(cx, v)) {
                        (Some(m), Some(v)) if m.len() == v.len() => b.affine(&m, &v),
                        _ => return Ok(Reply::Decline),
                    },
                    _ => return Ok(Reply::Decline),
                },
            },
            ("rescaling_transform", [from, rest @ ..]) if rest.len() <= 1 => {
                let Some(f) = matrix(cx, *from).filter(|f| f[0].len() == 2) else { return Ok(Reply::Decline) };
                let to = match rest.first() {
                    Some(&t) => match matrix(cx, t) {
                        Some(t) if t.len() == f.len() && t[0].len() == 2 => t,
                        _ => return Ok(Reply::Decline),
                    },
                    None => vec![vec![b.int(0), b.int(1)]; f.len()],
                };
                let n = f.len();
                let mut m = b.identity(n);
                let mut c = vec![];
                for i in 0..n {
                    // x' = c + (x - a)(d - c)/(b - a)
                    let k = b.div(b.sub(to[i][1], to[i][0]), b.sub(f[i][1], f[i][0]));
                    m[i][i] = k;
                    c.push(b.sub(to[i][0], b.mul(k, f[i][0])));
                }
                b.affine(&m, &c)
            }
            _ => return Ok(Reply::Decline),
        };
        let mut rows = Vec::with_capacity(m.len());
        for r in m {
            let mut row = Vec::with_capacity(r.len());
            for e in r {
                cx.tick(1)?;
                row.push(cx.eval(e)?);
            }
            rows.push(cx.list(&row));
        }
        let mt = cx.list(&rows);
        Ok(Reply::Done(h.call(h.sym("transformation_function"), &[mt])))
    }
}

fn diag<'h>(b: B<'h>, s: &[T<'h>]) -> Vec<Vec<T<'h>>> {
    (0..s.len()).map(|i| (0..s.len()).map(|j| if i == j { s[i] } else { b.int(0) }).collect()).collect()
}

/// `transformation_function(m)(p)`: the point `p` mapped (a list of points: each).
struct Apply;

impl Native for Apply {
    fn curried(&self) -> bool {
        true
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B { h };
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        let View::App { args: &[mt], .. } = h.view(head) else { return Ok(Reply::Decline) };
        let [p] = args else { return Ok(Reply::Decline) };
        let Some(m) = matrix(cx, mt) else { return Ok(Reply::Decline) };
        let n = m.len() - 1;
        let map = |cx: &mut NativeCx<'_, '_, 'h>, p: &[T<'h>]| -> Res<T<'h>> {
            let mut out = Vec::with_capacity(n);
            for row in m.iter().take(n) {
                let e = b.add(b.dot(&row[..n], p), row[n]);
                out.push(cx.eval(e)?);
            }
            Ok(cx.list(&out))
        };
        if let Some(v) = vector(cx, *p).filter(|v| v.len() == n) {
            return Ok(Reply::Done(map(cx, &v)?));
        }
        if let Some(ps) = matrix(cx, *p).filter(|ps| ps[0].len() == n) {
            let mut out = Vec::with_capacity(ps.len());
            for q in ps {
                out.push(map(cx, &q)?);
            }
            return Ok(Reply::Done(cx.list(&out)));
        }
        Ok(Reply::Decline)
    }
}

/// `normal(g)` of a transformed primitive: `geometric_transformation(g, f)`, `translate(g, v)`, `rotate(g, ...)` and
/// `scale(g, ...)` of a point, line, polygon, triangle, cuboid, sphere, ball or cylinder become the primitive with
/// the mapped coordinates (`normal(translate(point([x, y]), [a, b]))` is `point([a + x, b + y])`). Cuboids need an
/// axis-aligned map (translation, scaling), spheres, balls and cylinders a rigid one (translation, rotation) so the
/// radius is kept; anything else is left to the other definitions of `normal`.
struct NormalGeometry {
    inner: Box<dyn Native>,
}

impl Native for NormalGeometry {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if let [g] = args {
            if let Some(v) = transformed_primitive(cx, *g)? {
                return Ok(Reply::Done(v));
            }
        }
        self.inner.call(cx, t, args)
    }
    fn trace(&mut self, visit: &mut dyn FnMut(&mut mc_core::Raw)) {
        self.inner.trace(visit);
    }
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }
    fn curried(&self) -> bool {
        self.inner.curried()
    }
}

/// The kind of the map (what a primitive survives).
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Translate,
    Rotate,
    Scale,
    Any,
}

fn transformed_primitive<'h>(cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(g) else { return Ok(None) };
    let View::Sym(s) = h.view(head) else { return Ok(None) };
    let name = h.sym_name(s);
    let (prim, tf, kind) = match (name.as_str(), args) {
        ("geometric_transformation", [p, f]) => (*p, *f, Kind::Any),
        ("translate", [p, v]) if vector(cx, *v).is_some() => (*p, h.call(h.sym("translation_transform"), &[*v]), Kind::Translate),
        ("rotate", [p, rest @ ..]) if !rest.is_empty() => (*p, h.call(h.sym("rotation_transform"), rest), Kind::Rotate),
        ("scale", [p, rest @ ..]) if !rest.is_empty() => (*p, h.call(h.sym("scaling_transform"), rest), Kind::Scale),
        _ => return Ok(None),
    };
    let tf = cx.eval(tf)?;
    let View::App { head: th, .. } = h.view(tf) else { return Ok(None) };
    if !matches!(h.view(th), View::Sym(x) if h.sym_name(x) == "transformation_function") {
        return Ok(None);
    }
    let View::App { head: ph, args: pargs } = h.view(prim) else { return Ok(None) };
    let View::Sym(ps) = h.view(ph) else { return Ok(None) };
    let pname = h.sym_name(ps);
    let map = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>| -> Res<T<'h>> { cx.eval(h.app(tf, &[x])) };
    let one = |cx: &NativeCx<'_, '_, 'h>, x: T<'h>| cx.as_list(x).is_some();
    let rigid = matches!(kind, Kind::Translate | Kind::Rotate);
    let rebuilt = match (pname.as_str(), pargs) {
        ("point" | "line" | "polygon" | "triangle", [c]) if one(cx, *c) => Some(h.call(ps, &[map(cx, *c)?])),
        ("cuboid", [a, b]) if matches!(kind, Kind::Translate | Kind::Scale) => Some(h.call(ps, &[map(cx, *a)?, map(cx, *b)?])),
        ("cuboid", [a]) if matches!(kind, Kind::Translate | Kind::Scale) => {
            let Some(v) = vector(cx, *a) else { return Ok(None) };
            let up: Vec<T<'h>> = v.iter().map(|&x| h.call(h.sym(names::ADD), &[x, h.int(1)])).collect();
            let hi = cx.eval(cx.list(&up))?;
            Some(h.call(ps, &[map(cx, *a)?, map(cx, hi)?]))
        }
        ("sphere" | "ball", [c, r]) if rigid => Some(h.call(ps, &[map(cx, *c)?, *r])),
        ("sphere" | "ball", [c]) if rigid => Some(h.call(ps, &[map(cx, *c)?])),
        ("cylinder", []) if rigid => {
            let (lo, hi) = (cx.list(&[h.int(0), h.int(0), h.int(-1)]), cx.list(&[h.int(0), h.int(0), h.int(1)]));
            let ax = cx.list(&[lo, hi]);
            Some(h.call(ps, &[map(cx, ax)?, h.int(1)]))
        }
        ("cylinder", [a]) if rigid => Some(h.call(ps, &[map(cx, *a)?, h.int(1)])),
        ("cylinder", [a, r]) if rigid => Some(h.call(ps, &[map(cx, *a)?, *r])),
        _ => None,
    };
    Ok(rebuilt)
}

/// `dot(f1, f2, ...)` of transformation functions: their composition (the product of the homogeneous matrices), as
/// `transformation_function(m1 . m2 ...)`. Any other call goes to the matrix product.
struct DotTransforms {
    inner: Box<dyn Native>,
}

impl Native for DotTransforms {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let tf = h.sym("transformation_function");
        let matrices: Option<Vec<T<'h>>> = args
            .iter()
            .map(|&a| match h.view(a) {
                View::App { head, args: &[m] } if matches!(h.view(head), View::Sym(s) if s == tf) => Some(m),
                _ => None,
            })
            .collect();
        match matrices {
            Some(ms) if ms.len() > 1 => {
                let product = cx.eval(h.call(h.sym("dot"), &ms))?;
                Ok(Reply::Done(h.call(tf, &[product])))
            }
            _ => self.inner.call(cx, t, args),
        }
    }
    fn trace(&mut self, visit: &mut dyn FnMut(&mut mc_core::Raw)) {
        self.inner.trace(visit);
    }
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }
    fn curried(&self) -> bool {
        self.inner.curried()
    }
}
