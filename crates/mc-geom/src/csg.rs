//! Boolean operations on mesh regions: union, intersection, difference and any Boolean function of the regions
//! (`boolean_region`), for meshes of the line, plane boundary meshes and space boundary meshes. The result is a mesh
//! of the same kind as the first region.
//!
//! - On the line, a region is a union of intervals. The endpoints of all the regions cut the line into elementary
//!   intervals; each is kept when the Boolean function of the memberships of its midpoint is true, and kept
//!   neighbours are joined.
//! - In the plane, the rings of the boundary meshes go through the arrangement of [`crate::clip`]; the outer rings and
//!   holes of the result become the loops of a boundary mesh.
//! - In space, the faces of the boundary meshes go into binary space partitioning trees (Thibault and Naylor, "Set
//!   operations on polyhedra using binary space partitioning trees", SIGGRAPH 1987; Naylor, Amanatides and Thibault,
//!   "Merging BSP trees yields polyhedral set operations", SIGGRAPH 1990): the faces of each solid are clipped by the
//!   tree of the other, keeping the parts outside (union) or inside (intersection), and difference is intersection
//!   with the complement (a tree with every plane and face turned over). The pieces of each input face are joined
//!   again in the face's plane (by the plane arrangement), vertices closer than a tolerance are merged and every
//!   vertex lying on an edge of another face is put into that edge, so the faces meet edge to edge.
//!
//! Coordinates are machine numbers, as in every mesh; a piece of a face is classified by the sign of its distance to
//! a plane with a tolerance of 1e-9 times the size of the scene.
use crate::clip::{self, Op, P};
use crate::mesh::{self, Kind};
use crate::region::call_parts;
use mc_core::{Heap, Store, T};
use mc_eval::{NativeCx, Res};

/// A point of space.
type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn lerp(a: V3, b: V3, t: f64) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// The oriented plane n.x = w (|n| = 1).
#[derive(Clone, Copy, Debug)]
struct Plane {
    n: V3,
    w: f64,
}

impl Plane {
    /// The plane of a polygon, its normal by Newell's method (the polygon anticlockwise seen from the normal).
    fn of(v: &[V3]) -> Option<Plane> {
        let mut n = [0.0; 3];
        for i in 0..v.len() {
            let (a, b) = (v[i], v[(i + 1) % v.len()]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        let len = dot(n, n).sqrt();
        if len.is_nan() || len <= 1e-300 {
            return None;
        }
        let n = [n[0] / len, n[1] / len, n[2] / len];
        let c = v.iter().fold([0.0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
        let k = v.len() as f64;
        Some(Plane { n, w: dot(n, [c[0] / k, c[1] / k, c[2] / k]) })
    }
    fn flip(self) -> Plane {
        Plane { n: [-self.n[0], -self.n[1], -self.n[2]], w: -self.w }
    }
}

/// A convex or concave planar face: its corners, its plane and the input face it is a piece of (with whether it is
/// turned over relative to that face).
#[derive(Clone, Debug)]
struct Poly {
    v: Vec<V3>,
    plane: Plane,
    id: usize,
    flipped: bool,
}

impl Poly {
    fn flip(&mut self) {
        self.v.reverse();
        self.plane = self.plane.flip();
        self.flipped = !self.flipped;
    }
}

/// Where a face lies relative to a plane: (coplanar and facing the same way, coplanar and facing the other way,
/// in front, behind); a face across the plane is cut in two.
struct Split {
    same: Vec<Poly>,
    opposite: Vec<Poly>,
    front: Vec<Poly>,
    back: Vec<Poly>,
}

fn split(pl: Plane, p: Poly, eps: f64, out: &mut Split) {
    const ON: u8 = 0;
    const FRONT: u8 = 1;
    const BACK: u8 = 2;
    let d: Vec<f64> = p.v.iter().map(|&x| dot(pl.n, x) - pl.w).collect();
    let kinds: Vec<u8> = d.iter().map(|&t| if t > eps { FRONT } else if t < -eps { BACK } else { ON }).collect();
    let all = kinds.iter().fold(0u8, |a, &k| a | k);
    match all {
        ON => {
            if dot(pl.n, p.plane.n) > 0.0 {
                out.same.push(p)
            } else {
                out.opposite.push(p)
            }
        }
        FRONT => out.front.push(p),
        BACK => out.back.push(p),
        _ => {
            let (mut f, mut b) = (vec![], vec![]);
            let n = p.v.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let (ki, kj) = (kinds[i], kinds[j]);
                let (vi, vj) = (p.v[i], p.v[j]);
                if ki != BACK {
                    f.push(vi);
                }
                if ki != FRONT {
                    b.push(vi);
                }
                if ki | kj == FRONT | BACK {
                    let t = d[i] / (d[i] - d[j]);
                    let x = lerp(vi, vj, t);
                    f.push(x);
                    b.push(x);
                }
            }
            if f.len() >= 3 {
                out.front.push(Poly { v: f, ..p.clone() });
            }
            if b.len() >= 3 {
                out.back.push(Poly { v: b, ..p });
            }
        }
    }
}

/// A node of a BSP tree: its plane, the faces in it, and its children in front and behind.
struct Node {
    plane: Plane,
    polys: Vec<Poly>,
    front: Option<usize>,
    back: Option<usize>,
}

/// A BSP tree of a solid (nodes in an arena, the root first): the cells in front of the leaves are outside, the
/// cells behind them inside.
struct Tree {
    nodes: Vec<Node>,
    eps: f64,
}

impl Tree {
    fn new(eps: f64) -> Tree {
        Tree { nodes: vec![], eps }
    }

    /// Insert faces: each node keeps the faces in its plane and passes the others to its children.
    fn build<'h>(&mut self, cx: &mut NativeCx<'_, '_, 'h>, polys: Vec<Poly>) -> Res<()> {
        if polys.is_empty() {
            return Ok(());
        }
        if self.nodes.is_empty() {
            self.nodes.push(Node { plane: polys[0].plane, polys: vec![], front: None, back: None });
        }
        let mut stack = vec![(0usize, polys)];
        while let Some((k, ps)) = stack.pop() {
            cx.tick(1 + ps.len() as u64)?;
            let pl = self.nodes[k].plane;
            let mut s = Split { same: vec![], opposite: vec![], front: vec![], back: vec![] };
            for p in ps {
                split(pl, p, self.eps, &mut s);
            }
            self.nodes[k].polys.extend(s.same);
            self.nodes[k].polys.extend(s.opposite);
            for (side, ps) in [(true, s.front), (false, s.back)] {
                if ps.is_empty() {
                    continue;
                }
                let child = if side { self.nodes[k].front } else { self.nodes[k].back };
                let c = match child {
                    Some(c) => c,
                    None => {
                        self.nodes.push(Node { plane: ps[0].plane, polys: vec![], front: None, back: None });
                        let c = self.nodes.len() - 1;
                        if side {
                            self.nodes[k].front = Some(c);
                        } else {
                            self.nodes[k].back = Some(c);
                        }
                        c
                    }
                };
                stack.push((c, ps));
            }
        }
        Ok(())
    }

    /// The parts of the faces outside the solid of this tree (in front of its leaves); faces in a plane of the tree
    /// go with the side their normal faces.
    fn clip<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, polys: Vec<Poly>) -> Res<Vec<Poly>> {
        if self.nodes.is_empty() {
            return Ok(polys);
        }
        let mut out = vec![];
        let mut stack = vec![(0usize, polys)];
        while let Some((k, ps)) = stack.pop() {
            cx.tick(1 + ps.len() as u64)?;
            let node = &self.nodes[k];
            let mut s = Split { same: vec![], opposite: vec![], front: vec![], back: vec![] };
            for p in ps {
                split(node.plane, p, self.eps, &mut s);
            }
            let mut front = s.front;
            front.extend(s.same);
            let mut back = s.back;
            back.extend(s.opposite);
            match node.front {
                Some(c) => stack.push((c, front)),
                None => out.extend(front),
            }
            if let Some(c) = node.back {
                stack.push((c, back));
            }
        }
        Ok(out)
    }

    /// Clip the faces of every node by another tree.
    fn clip_to<'h>(&mut self, cx: &mut NativeCx<'_, '_, 'h>, other: &Tree) -> Res<()> {
        for k in 0..self.nodes.len() {
            let ps = std::mem::take(&mut self.nodes[k].polys);
            self.nodes[k].polys = other.clip(cx, ps)?;
        }
        Ok(())
    }

    /// The complement: every plane and face turned over, front and back exchanged.
    fn invert(&mut self) {
        for n in &mut self.nodes {
            for p in &mut n.polys {
                p.flip();
            }
            n.plane = n.plane.flip();
            std::mem::swap(&mut n.front, &mut n.back);
        }
    }

    fn faces(&self) -> Vec<Poly> {
        self.nodes.iter().flat_map(|n| n.polys.iter().cloned()).collect()
    }
}

/// A solid of space as the faces of its boundary (outward normals).
type Solid = Vec<Poly>;

/// `a op b` for solids given by their boundary faces.
fn solid_op<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &Solid, b: &Solid, op: Op, eps: f64) -> Res<Solid> {
    let (mut ta, mut tb) = (Tree::new(eps), Tree::new(eps));
    ta.build(cx, a.clone())?;
    tb.build(cx, b.clone())?;
    match op {
        Op::Union => {
            ta.clip_to(cx, &tb)?;
            tb.clip_to(cx, &ta)?;
            // faces of b coplanar with faces of a and facing the same way are dropped once
            tb.invert();
            tb.clip_to(cx, &ta)?;
            tb.invert();
            let f = tb.faces();
            ta.build(cx, f)?;
        }
        Op::Intersection => {
            ta.invert();
            tb.clip_to(cx, &ta)?;
            tb.invert();
            ta.clip_to(cx, &tb)?;
            tb.clip_to(cx, &ta)?;
            let f = tb.faces();
            ta.build(cx, f)?;
            ta.invert();
        }
        Op::Difference => {
            ta.invert();
            ta.clip_to(cx, &tb)?;
            tb.clip_to(cx, &ta)?;
            tb.invert();
            tb.clip_to(cx, &ta)?;
            tb.invert();
            let f = tb.faces();
            ta.build(cx, f)?;
            ta.invert();
        }
    }
    Ok(ta.faces())
}

/// The signed volume bounded by faces (positive when the normals point out).
fn volume(s: &[Poly]) -> f64 {
    let mut v = 0.0;
    for p in s {
        for i in 1..p.v.len().saturating_sub(1) {
            v += dot(p.v[0], cross(p.v[i], p.v[i + 1]));
        }
    }
    v / 6.0
}

// ---- reading the regions

/// A region given to a Boolean operation, read as numbers.
enum Shape {
    /// intervals of the line, sorted and disjoint
    Line(Vec<(f64, f64)>),
    /// rings of the plane (even-odd)
    Plane(Vec<Vec<P>>),
    /// the boundary faces of a solid
    Space(Solid),
}

/// Which kind of mesh the result is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Out {
    Mesh,
    Boundary,
}

fn is_mesh_term<'h>(h: &'h Heap, t: T<'h>) -> bool {
    call_parts(h, t).is_some_and(|(n, _)| n == "mesh_region" || n == "boundary_mesh_region")
}

/// Disjoint sorted intervals from any intervals (overlapping and touching ones joined).
fn merged(mut v: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    v.retain(|w| w.1 > w.0);
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = vec![];
    for w in v {
        match out.last_mut() {
            Some(l) if w.0 <= l.1 => l.1 = l.1.max(w.1),
            _ => out.push(w),
        }
    }
    out
}

/// The faces of a polyhedron in space as polygons with outward normals (all turned over when the volume they bound
/// is negative); `None` when it bounds no volume or a face is degenerate.
fn solid_of(pts: &[Vec<f64>], faces: &[Vec<usize>], first: usize) -> Option<(Solid, usize)> {
    let mut s = vec![];
    for (k, f) in faces.iter().enumerate() {
        let v: Vec<V3> = f.iter().map(|&i| [pts[i][0], pts[i][1], pts[i][2]]).collect();
        let plane = Plane::of(&v)?;
        s.push(Poly { v, plane, id: first + k, flipped: false });
    }
    let vol = volume(&s);
    if !vol.is_finite() || vol.abs() <= 1e-300 {
        return None;
    }
    if vol < 0.0 {
        for p in &mut s {
            p.flip();
            p.flipped = false;
        }
    }
    let n = s.len();
    Some((s, first + n))
}

/// Read a mesh region for a Boolean operation (`next_id` numbers the faces of solids).
fn shape<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, next_id: &mut usize) -> Res<Option<(Shape, Out)>> {
    let h = cx.heap();
    let Some((name, args)) = call_parts(h, t) else { return Ok(None) };
    if name != "mesh_region" && name != "boundary_mesh_region" {
        return Ok(None);
    }
    let Some(m) = mesh::parse(cx, &name, args)?.map(mesh::canonical) else { return Ok(None) };
    let out = if m.boundary { Out::Boundary } else { Out::Mesh };
    let n = m.pts[0].len();
    let dim = m.dim();
    Ok(match (n, dim, m.boundary) {
        (1, 0, true) => {
            let mut xs: Vec<f64> = m.cells().map(|c| m.pts[c.v[0]][0]).collect();
            xs.sort_by(f64::total_cmp);
            if !xs.len().is_multiple_of(2) {
                return Ok(None);
            }
            Some((Shape::Line(merged(xs.chunks(2).map(|w| (w[0], w[1])).collect())), out))
        }
        (1, 1, false) if m.cells().all(|c| c.kind == Kind::Line) => {
            let v = m.cells().map(|c| {
                let (a, b) = (m.pts[c.v[0]][0], m.pts[c.v[1]][0]);
                (a.min(b), a.max(b))
            });
            Some((Shape::Line(merged(v.collect())), out))
        }
        (2, 1, true) => crate::natives::plane_rings(cx, t)?.map(|(rings, _)| (Shape::Plane(rings), out)),
        (3, 2, true) => {
            let faces: Vec<Vec<usize>> = m.cells().filter(|c| c.kind == Kind::Polygon).map(|c| c.v.clone()).collect();
            if faces.len() != m.cells().count() {
                return Ok(None);
            }
            match solid_of(&m.pts, &faces, *next_id) {
                Some((s, next)) => {
                    *next_id = next;
                    Some((Shape::Space(s), out))
                }
                None => None,
            }
        }
        _ => None,
    })
}

/// The Boolean function `f` (of the k memberships) of mesh regions, all of one embedding dimension; `None` when the
/// regions are not of the kinds handled or the result would be unbounded.
pub fn combine<'a, 'b, 'h, F>(cx: &mut NativeCx<'a, 'b, 'h>, items: &[T<'h>], mut f: F) -> Res<Option<T<'h>>>
where
    F: FnMut(&mut NativeCx<'a, 'b, 'h>, &[bool]) -> Res<Option<bool>>,
{
    let h = cx.heap();
    if items.is_empty() || items.len() > 5 || !items.iter().all(|&t| is_mesh_term(h, t)) {
        return Ok(None);
    }
    // the complement of all the regions is unbounded
    if f(cx, &vec![false; items.len()])? != Some(false) {
        return Ok(None);
    }
    let mut shapes = vec![];
    let mut next_id = 0;
    for &t in items {
        let Some(s) = shape(cx, t, &mut next_id)? else { return Ok(None) };
        shapes.push(s);
    }
    let out = shapes[0].1;
    let k = shapes.len();
    match &shapes[0].0 {
        Shape::Line(_) => {
            let mut sets = vec![];
            for (s, _) in &shapes {
                let Shape::Line(v) = s else { return Ok(None) };
                sets.push(v.clone());
            }
            let mut xs: Vec<f64> = sets.iter().flatten().flat_map(|w| [w.0, w.1]).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup();
            let mut kept = vec![];
            for w in xs.windows(2) {
                cx.tick(1)?;
                let mid = 0.5 * (w[0] + w[1]);
                let bits: Vec<bool> = sets.iter().map(|s| s.iter().any(|iv| iv.0 < mid && mid < iv.1)).collect();
                match f(cx, &bits)? {
                    Some(true) => kept.push((w[0], w[1])),
                    Some(false) => {}
                    None => return Ok(None),
                }
            }
            Ok(Some(line_term(cx, &merged(kept), out)))
        }
        Shape::Plane(_) => {
            let mut sets = vec![];
            for (s, _) in &shapes {
                let Shape::Plane(v) = s else { return Ok(None) };
                sets.push(v.clone());
            }
            let flat = |s: &clip::Shapes| -> Vec<Vec<P>> { s.iter().flat_map(|(o, hs)| std::iter::once(o.clone()).chain(hs.iter().cloned())).collect() };
            let mut result: Vec<Vec<P>> = vec![];
            for mask in 1u32..(1 << k) {
                let bits: Vec<bool> = (0..k).map(|i| mask >> i & 1 == 1).collect();
                match f(cx, &bits)? {
                    Some(true) => {}
                    Some(false) => continue,
                    None => return Ok(None),
                }
                cx.tick(1)?;
                let inside: Vec<usize> = (0..k).filter(|&i| bits[i]).collect();
                let mut acc = sets[inside[0]].clone();
                for &i in &inside[1..] {
                    acc = flat(&clip::boolean(&acc, &sets[i], Op::Intersection));
                }
                for i in (0..k).filter(|&i| !bits[i]) {
                    acc = flat(&clip::boolean(&acc, &sets[i], Op::Difference));
                }
                if !acc.is_empty() {
                    result = flat(&clip::boolean(&result, &acc, Op::Union));
                }
            }
            let shapes = clip::boolean(&result, &[], Op::Union);
            Ok(Some(plane_term(cx, &shapes)))
        }
        Shape::Space(_) => {
            let mut sets = vec![];
            for (s, _) in &shapes {
                let Shape::Space(v) = s else { return Ok(None) };
                sets.push(v.clone());
            }
            let scale = sets.iter().flatten().flat_map(|p| p.v.iter()).flat_map(|x| x.iter()).fold(0.0f64, |m, c| m.max(c.abs())).max(1e-300);
            let eps = 1e-9 * scale;
            let mut result: Option<Solid> = None;
            for mask in 1u32..(1 << k) {
                let bits: Vec<bool> = (0..k).map(|i| mask >> i & 1 == 1).collect();
                match f(cx, &bits)? {
                    Some(true) => {}
                    Some(false) => continue,
                    None => return Ok(None),
                }
                let inside: Vec<usize> = (0..k).filter(|&i| bits[i]).collect();
                let mut acc = sets[inside[0]].clone();
                for &i in &inside[1..] {
                    acc = solid_op(cx, &acc, &sets[i], Op::Intersection, eps)?;
                }
                for i in (0..k).filter(|&i| !bits[i]) {
                    if acc.is_empty() {
                        break;
                    }
                    acc = solid_op(cx, &acc, &sets[i], Op::Difference, eps)?;
                }
                if acc.is_empty() {
                    continue;
                }
                result = Some(match result {
                    None => acc,
                    Some(r) => solid_op(cx, &r, &acc, Op::Union, eps)?,
                });
            }
            let faces = result.unwrap_or_default();
            space_term(cx, faces, eps * 10.0)
        }
    }
}

/// A segment of the line: its ends as (value, term), the lower first.
type Segment<'h> = ((f64, T<'h>), (f64, T<'h>));

/// Union, intersection or difference of segments of the line given as `line` primitives (`line([[1], [3]])`): the
/// segments of the result as one `line` primitive, with the given end points (exact when they were); `None` for
/// other regions.
pub fn line_segments<'h>(cx: &mut NativeCx<'_, '_, 'h>, items: &[T<'h>], op: Op) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if items.is_empty() || !items.iter().all(|&t| call_parts(h, t).is_some_and(|(n, _)| n == "line")) {
        return Ok(None);
    }
    // each region: its segments as (value, term) pairs
    let mut sets: Vec<Vec<Segment<'h>>> = vec![];
    for &t in items {
        let Some(crate::region::Region::Lines(ls)) = crate::region::read(cx, t) else { return Ok(None) };
        let mut set = vec![];
        for l in ls {
            if l.len() != 2 || l[0].len() != 1 {
                return Ok(None);
            }
            let (Some(a), Some(b)) = (crate::ex::num(cx, l[0][0])?, crate::ex::num(cx, l[1][0])?) else { return Ok(None) };
            let (lo, hi) = if a <= b { ((a, l[0][0]), (b, l[1][0])) } else { ((b, l[1][0]), (a, l[0][0])) };
            set.push((lo, hi));
        }
        sets.push(set);
    }
    let mut ends: Vec<(f64, T<'h>)> = sets.iter().flatten().flat_map(|s| [s.0, s.1]).collect();
    ends.sort_by(|x, y| x.0.total_cmp(&y.0));
    ends.dedup_by(|x, y| x.0 == y.0);
    let mut kept: Vec<Segment<'h>> = vec![];
    for w in ends.windows(2) {
        cx.tick(1)?;
        let mid = 0.5 * (w[0].0 + w[1].0);
        let bits: Vec<bool> = sets.iter().map(|s| s.iter().any(|iv| iv.0 .0 < mid && mid < iv.1 .0)).collect();
        if op_truth(op, &bits) {
            match kept.last_mut() {
                Some(last) if last.1 .0 == w[0].0 => last.1 = w[1],
                _ => kept.push((w[0], w[1])),
            }
        }
    }
    if kept.is_empty() {
        return Ok(Some(empty(h, 1)));
    }
    let segs: Vec<T<'h>> = kept.iter().map(|(a, b)| cx.list(&[cx.list(&[a.1]), cx.list(&[b.1])])).collect();
    Ok(Some(h.call(h.sym("line"), &[cx.list(&segs)])))
}

// ---- the results as meshes

fn reals<'h>(h: &'h Heap, p: &[f64]) -> T<'h> {
    h.reals(p)
}

fn index_list<'h>(cx: &NativeCx<'_, '_, 'h>, v: &[usize]) -> T<'h> {
    let h = cx.heap();
    cx.list(&v.iter().map(|&i| h.int(i as i64 + 1)).collect::<Vec<_>>())
}

fn empty<'h>(h: &'h Heap, d: i64) -> T<'h> {
    h.call(h.sym("empty_region"), &[h.int(d)])
}

/// The canonical mesh term of points and cells.
fn mesh_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, pts: Vec<T<'h>>, cells: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let t = h.call(h.sym(name), &[cx.list(&pts), cells]);
    cx.eval(t)
}

fn line_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, iv: &[(f64, f64)], out: Out) -> T<'h> {
    let h = cx.heap();
    if iv.is_empty() {
        return empty(h, 1);
    }
    let pts: Vec<T<'h>> = iv.iter().flat_map(|w| [reals(h, &[w.0]), reals(h, &[w.1])]).collect();
    let (name, cell) = match out {
        Out::Mesh => {
            let segs: Vec<T<'h>> = (0..iv.len()).map(|i| index_list(cx, &[2 * i, 2 * i + 1])).collect();
            ("mesh_region", h.call(h.sym("line"), &[cx.list(&segs)]))
        }
        Out::Boundary => {
            let ends: Vec<T<'h>> = (0..2 * iv.len()).map(|i| index_list(cx, &[i])).collect();
            ("boundary_mesh_region", h.call(h.sym("point"), &[cx.list(&ends)]))
        }
    };
    let cells = cx.list(&[cell]);
    mesh_term(cx, name, pts, cells).unwrap_or_else(|_| empty(h, 1))
}

/// The loops of a plane result as a boundary mesh: each ring's points in order, a segment from each to the next.
fn plane_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, shapes: &clip::Shapes) -> T<'h> {
    let h = cx.heap();
    if shapes.is_empty() {
        return empty(h, 2);
    }
    let mut pts = vec![];
    let mut segs = vec![];
    for (outer, holes) in shapes {
        for ring in std::iter::once(outer).chain(holes) {
            let first = pts.len();
            let n = ring.len();
            pts.extend(ring.iter().map(|p| reals(h, p)));
            for i in 0..n {
                segs.push(index_list(cx, &[first + i, first + (i + 1) % n]));
            }
        }
    }
    let cells = cx.list(&[h.call(h.sym("line"), &[cx.list(&segs)])]);
    mesh_term(cx, "boundary_mesh_region", pts, cells).unwrap_or_else(|_| empty(h, 2))
}

/// An orthonormal pair (u, v) in a plane with u x v = n.
fn basis(n: V3) -> (V3, V3) {
    let a = if n[0].abs() < 0.6 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = cross(a, n);
    let l = dot(u, u).sqrt();
    let u = [u[0] / l, u[1] / l, u[2] / l];
    (u, cross(n, u))
}

/// The faces of a space result as a boundary mesh: the pieces of each input face joined in its plane, vertices
/// merged within `tol`, and every vertex on the inside of a face's edge put into that edge.
fn space_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, faces: Solid, tol: f64) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if faces.is_empty() {
        return Ok(Some(empty(h, 3)));
    }
    // join the pieces of each face
    let mut groups: std::collections::BTreeMap<(usize, bool), Vec<Poly>> = std::collections::BTreeMap::new();
    for p in faces {
        groups.entry((p.id, p.flipped)).or_default().push(p);
    }
    let mut polys: Vec<Vec<V3>> = vec![];
    for (_, g) in groups {
        cx.tick(g.len() as u64)?;
        if g.len() == 1 {
            polys.push(g[0].v.clone());
            continue;
        }
        let pl = g[0].plane;
        let (u, v) = basis(pl.n);
        let o = [pl.n[0] * pl.w, pl.n[1] * pl.w, pl.n[2] * pl.w];
        let rings: Vec<Vec<P>> = g.iter().map(|p| p.v.iter().map(|&x| [dot(sub(x, o), u), dot(sub(x, o), v)]).collect()).collect();
        let joined = clip::boolean(&rings, &[], Op::Union);
        if joined.is_empty() || joined.iter().any(|s| !s.1.is_empty()) {
            // a face with a hole is kept in its pieces
            polys.extend(g.iter().map(|p| p.v.clone()));
            continue;
        }
        for (ring, _) in joined {
            let mut r = ring;
            if clip::area(&r) < 0.0 {
                r.reverse();
            }
            polys.push(r.iter().map(|q| [o[0] + q[0] * u[0] + q[1] * v[0], o[1] + q[0] * u[1] + q[1] * v[1], o[2] + q[0] * u[2] + q[1] * v[2]]).collect());
        }
    }
    // merge vertices within tol (a grid of cells of size tol, the neighbouring cells searched)
    let mut pts: Vec<V3> = vec![];
    let mut grid: std::collections::HashMap<[i64; 3], Vec<usize>> = std::collections::HashMap::new();
    let key = |p: V3| [(p[0] / tol).floor() as i64, (p[1] / tol).floor() as i64, (p[2] / tol).floor() as i64];
    let mut cells: Vec<Vec<usize>> = vec![];
    for poly in &polys {
        cx.tick(poly.len() as u64)?;
        let mut idx: Vec<usize> = vec![];
        for &p in poly {
            let k = key(p);
            let mut found = None;
            'search: for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(list) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                            for &i in list {
                                let d = sub(pts[i], p);
                                if dot(d, d) <= tol * tol {
                                    found = Some(i);
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
            let i = found.unwrap_or_else(|| {
                pts.push(p);
                grid.entry(k).or_default().push(pts.len() - 1);
                pts.len() - 1
            });
            if idx.last() != Some(&i) {
                idx.push(i);
            }
        }
        while idx.len() > 1 && idx.first() == idx.last() {
            idx.pop();
        }
        if idx.len() >= 3 {
            cells.push(idx);
        }
    }
    // vertices on the inside of an edge go into it (sorted along the edge)
    let mut fixed = Vec::with_capacity(cells.len());
    for c in &cells {
        cx.tick((c.len() * pts.len()) as u64 / 64 + 1)?;
        let mut out = vec![];
        for i in 0..c.len() {
            let (a, b) = (c[i], c[(i + 1) % c.len()]);
            out.push(a);
            let d = sub(pts[b], pts[a]);
            let len2 = dot(d, d);
            if len2 <= 0.0 {
                continue;
            }
            let mut on: Vec<(f64, usize)> = vec![];
            for (j, &q) in pts.iter().enumerate() {
                if j == a || j == b {
                    continue;
                }
                let w = sub(q, pts[a]);
                let t = dot(w, d) / len2;
                if t <= 0.0 || t >= 1.0 {
                    continue;
                }
                let x = cross(w, d);
                if dot(x, x) <= tol * tol * len2 {
                    on.push((t, j));
                }
            }
            on.sort_by(|x, y| x.0.total_cmp(&y.0));
            out.extend(on.into_iter().map(|x| x.1));
        }
        fixed.push(out);
    }
    let pts: Vec<T<'h>> = pts.iter().map(|p| reals(h, p)).collect();
    let faces: Vec<T<'h>> = fixed.iter().map(|c| index_list(cx, c)).collect();
    let cells = cx.list(&[h.call(h.sym("polygon"), &[cx.list(&faces)])]);
    Ok(Some(mesh_term(cx, "boundary_mesh_region", pts, cells)?))
}

/// The truth function of a set operation on k regions: union (any), intersection (all), difference (the first and
/// none of the others).
pub fn op_truth(op: Op, bits: &[bool]) -> bool {
    match op {
        Op::Union => bits.iter().any(|&b| b),
        Op::Intersection => bits.iter().all(|&b| b),
        Op::Difference => bits[0] && !bits[1..].iter().any(|&b| b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tet(p: [V3; 4], id: usize) -> Solid {
        let faces = [[2, 1, 0], [0, 1, 3], [1, 2, 3], [3, 2, 0]];
        let pts: Vec<Vec<f64>> = p.iter().map(|x| x.to_vec()).collect();
        let f: Vec<Vec<usize>> = faces.iter().map(|f| f.to_vec()).collect();
        solid_of(&pts, &f, id).expect("a solid").0
    }

    #[test]
    fn split_keeps_both_sides() {
        let sq = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 0.0]];
        let p = Poly { plane: Plane::of(&sq).expect("plane"), v: sq, id: 0, flipped: false };
        let mut s = Split { same: vec![], opposite: vec![], front: vec![], back: vec![] };
        split(Plane { n: [1.0, 0.0, 0.0], w: 1.0 }, p, 1e-12, &mut s);
        assert_eq!((s.front.len(), s.back.len()), (1, 1));
        let area = |v: &[V3]| {
            let mut a = [0.0; 3];
            for i in 1..v.len() - 1 {
                let c = cross(sub(v[i], v[0]), sub(v[i + 1], v[0]));
                a = [a[0] + c[0], a[1] + c[1], a[2] + c[2]];
            }
            dot(a, a).sqrt() / 2.0
        };
        assert!((area(&s.front[0].v) - 2.0).abs() < 1e-12 && (area(&s.back[0].v) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn orientation_is_outward() {
        let t = tet([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], 0);
        assert!((volume(&t) - 1.0 / 6.0).abs() < 1e-15);
    }
}
