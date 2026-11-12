//! Mesh regions: `mesh_region(points, cells)` (a region made of cells: points, line segments, polygons,
//! tetrahedra, hexahedra, prisms, pyramids) and `boundary_mesh_region(points, boundary cells, ...)` (the solid
//! bounded by the cells), their canonical form, their cells of each dimension (`mesh_cells`, `mesh_cell_count`,
//! `mesh_primitives`, `mesh_coordinates`) and their reading as regions for the region functions.
//!
//! The canonical form: coordinates as machine reals; cells grouped by kind, kinds in order of decreasing
//! dimension, each kind one primitive with a list of index lists; lines as two-point segments; triangles and
//! simplices as polygons, lines, points or tetrahedra; vertices no cell uses are dropped and the others numbered in
//! order. The cells of lower dimension are the faces of the given cells taken in order (each face once, with the
//! orientation it has where it is first met), then the given cells of that dimension; the faces of a tetrahedron
//! (a, b, c, d) are (c, b, a), (b, c, d), (c, a, d), (a, b, d), all turned the same way (the usual outward
//! orientation of a positively oriented tetrahedron, as in finite-element codes).
use crate::region::{call_parts, Pt, Region, Ring};
use crate::shapes::{HEXAHEDRON, PRISM, PYRAMID};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// A cell's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// a vertex
    Point,
    /// a segment
    Line,
    /// a polygon (triangles too)
    Polygon,
    /// a tetrahedron
    Tetrahedron,
    /// a hexahedron
    Hexahedron,
    /// a triangular prism
    Prism,
    /// a pyramid with a quadrilateral base
    Pyramid,
}

impl Kind {
    /// The cell's dimension.
    pub fn dim(self) -> usize {
        match self {
            Kind::Point => 0,
            Kind::Line => 1,
            Kind::Polygon => 2,
            _ => 3,
        }
    }
    fn head(self) -> &'static str {
        match self {
            Kind::Point => "point",
            Kind::Line => "line",
            Kind::Polygon => "polygon",
            Kind::Tetrahedron => "tetrahedron",
            Kind::Hexahedron => "hexahedron",
            Kind::Prism => "prism",
            Kind::Pyramid => "pyramid",
        }
    }
}

/// One cell: its kind and its vertices (indices from 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// the kind
    pub kind: Kind,
    /// the vertices
    pub v: Vec<usize>,
}

/// A mesh read from its term.
#[derive(Clone, Debug)]
pub struct Mesh<'h> {
    /// the coordinates
    pub pts: Vec<Vec<f64>>,
    /// the cells (for a boundary mesh region, the boundary cells), grouped as given
    pub groups: Vec<Vec<Cell>>,
    /// a boundary mesh region
    pub boundary: bool,
    /// the options term given after the cells
    pub opts: Option<T<'h>>,
}

impl Mesh<'_> {
    /// Every cell, in order.
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.groups.iter().flatten()
    }
    /// The greatest dimension of a cell.
    pub fn dim(&self) -> usize {
        self.cells().map(|c| c.kind.dim()).max().unwrap_or(0)
    }
}

fn int_of(h: &Heap, t: T<'_>) -> Option<i64> {
    match h.view(t) {
        View::Int(i) => Some(i),
        _ => None,
    }
}

/// The index lists of a cell's argument: one list, or a list of lists (indices from 1, checked against `n`).
fn index_lists<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, n: usize) -> Option<Vec<Vec<usize>>> {
    let h = cx.heap();
    let one = |l: &[T<'h>]| -> Option<Vec<usize>> {
        l.iter().map(|&x| int_of(h, x).filter(|&i| i >= 1 && i as usize <= n).map(|i| i as usize - 1)).collect()
    };
    if let Some(i) = int_of(h, t) {
        return one(&[t]).filter(|_| i >= 1).map(|v| vec![v]);
    }
    let l = cx.as_list(t)?;
    if l.iter().all(|&x| int_of(h, x).is_some()) {
        return Some(vec![one(l)?]);
    }
    l.iter().map(|&x| cx.as_list(x).and_then(one)).collect()
}

/// The cells of one cell term (`line([1, 2, 3])` is two segments).
fn cells_of<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, n: usize, out: &mut Vec<Cell>) -> Option<()> {
    let h = cx.heap();
    let (name, args) = call_parts(h, t)?;
    let [a] = args else { return None };
    let lists = index_lists(cx, *a, n)?;
    for v in lists {
        let (kind, ok) = match name.as_str() {
            "point" => {
                for &i in &v {
                    out.push(Cell { kind: Kind::Point, v: vec![i] });
                }
                continue;
            }
            "line" => {
                if v.len() < 2 {
                    return None;
                }
                for w in v.windows(2) {
                    out.push(Cell { kind: Kind::Line, v: w.to_vec() });
                }
                continue;
            }
            "polygon" | "triangle" => (Kind::Polygon, v.len() >= 3 && (name == "polygon" || v.len() == 3)),
            "simplex" => match v.len() {
                1 => (Kind::Point, true),
                2 => (Kind::Line, true),
                3 => (Kind::Polygon, true),
                4 => (Kind::Tetrahedron, true),
                _ => (Kind::Point, false),
            },
            "tetrahedron" => (Kind::Tetrahedron, v.len() == 4),
            "hexahedron" => (Kind::Hexahedron, v.len() == 8),
            "prism" => (Kind::Prism, v.len() == 6),
            "pyramid" => (Kind::Pyramid, v.len() == 5),
            _ => return None,
        };
        if !ok {
            return None;
        }
        out.push(Cell { kind, v });
    }
    Some(())
}

/// Read a mesh from the arguments of `mesh_region` or `boundary_mesh_region`.
pub fn parse<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<Option<Mesh<'h>>> {
    let h = cx.heap();
    let (args, opts) = match args.split_last() {
        Some((&o, rest)) if call_parts(h, o).is_some_and(|(n, _)| n == "options") => (rest, Some(o)),
        _ => (args, None),
    };
    let [p, specs @ ..] = args else { return Ok(None) };
    if specs.is_empty() {
        return Ok(None);
    }
    let Some(rows) = cx.as_list(*p).map(|l| l.to_vec()) else { return Ok(None) };
    let mut pts = Vec::with_capacity(rows.len());
    for r in rows {
        let Some(c) = cx.as_list(r).map(|l| l.to_vec()) else { return Ok(None) };
        match crate::ex::nums(cx, &c)? {
            Some(v) if !v.is_empty() && v.iter().all(|x| x.is_finite()) => pts.push(v),
            _ => return Ok(None),
        }
    }
    if pts.is_empty() || pts.iter().any(|q| q.len() != pts[0].len()) {
        return Ok(None);
    }
    let n = pts.len();
    let mut groups = vec![];
    for &s in specs {
        let mut g = vec![];
        let items: Vec<T<'h>> = match cx.as_list(s) {
            Some(l) => l.to_vec(),
            None => vec![s],
        };
        for it in items {
            if cells_of(cx, it, n, &mut g).is_none() {
                return Ok(None);
            }
        }
        if g.is_empty() {
            return Ok(None);
        }
        groups.push(g);
    }
    Ok(Some(Mesh { pts, groups, boundary: name == "boundary_mesh_region", opts }))
}

/// The mesh with unused vertices dropped and the others renumbered in order; for a mesh region the cells merged
/// into one group sorted by decreasing dimension (stable).
pub fn canonical(mut m: Mesh<'_>) -> Mesh<'_> {
    let mut used = vec![false; m.pts.len()];
    for c in m.cells() {
        for &i in &c.v {
            used[i] = true;
        }
    }
    let mut map = vec![usize::MAX; m.pts.len()];
    let mut pts = vec![];
    for (i, p) in m.pts.iter().enumerate() {
        if used[i] {
            map[i] = pts.len();
            pts.push(p.clone());
        }
    }
    for g in m.groups.iter_mut() {
        for c in g.iter_mut() {
            for i in c.v.iter_mut() {
                *i = map[*i];
            }
        }
    }
    m.pts = pts;
    // tetrahedra turned positively (corners 2 and 3 swapped when the orientation is negative)
    let pts = m.pts.clone();
    for g in m.groups.iter_mut() {
        for c in g.iter_mut() {
            if c.kind == Kind::Tetrahedron && pts[0].len() == 3 {
                let [a, b, cc, d] = [c.v[0], c.v[1], c.v[2], c.v[3]];
                let s = |i: usize| -> Vec<f64> { (0..3).map(|k| pts[i][k] - pts[a][k]).collect() };
                let (u, v, w) = (s(b), s(cc), s(d));
                let det = u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0]) + u[2] * (v[0] * w[1] - v[1] * w[0]);
                if det < 0.0 {
                    c.v.swap(1, 2);
                }
            }
        }
    }
    // the loops of a plane boundary mesh region turned anticlockwise (holes clockwise)
    if m.boundary && pts[0].len() == 2 {
        let loops: Vec<Option<Vec<usize>>> = m.groups.iter().map(|g| single_loop(g)).collect();
        let rings: Vec<Vec<Vec<f64>>> = loops.iter().map(|l| l.as_ref().map_or(vec![], |r| r.iter().map(|&i| pts[i].clone()).collect())).collect();
        for (gi, l) in loops.iter().enumerate() {
            let Some(ring) = l else { continue };
            let depth = (0..rings.len()).filter(|&j| j != gi && rings[j].len() >= 3 && crate::metric::in_polygon(&pts[ring[0]], &rings[j..j + 1])).count();
            let area: f64 = (0..ring.len()).map(|i| {
                let (p, q) = (&pts[ring[i]], &pts[ring[(i + 1) % ring.len()]]);
                p[0] * q[1] - q[0] * p[1]
            }).sum();
            if (area < 0.0) == (depth % 2 == 0) {
                let mut r = ring.clone();
                r[1..].reverse();
                m.groups[gi] = (0..r.len()).map(|i| Cell { kind: Kind::Line, v: vec![r[i], r[(i + 1) % r.len()]] }).collect();
            }
        }
    }
    if !m.boundary {
        let mut all: Vec<Cell> = m.groups.drain(..).flatten().collect();
        all.sort_by_key(|c| std::cmp::Reverse(c.kind.dim()));
        m.groups = vec![all];
    }
    m
}

/// The ring of a group of segments that form exactly one closed loop in order.
fn single_loop(g: &[Cell]) -> Option<Vec<usize>> {
    if g.len() < 3 || g.iter().any(|c| c.kind != Kind::Line) {
        return None;
    }
    for w in 0..g.len() {
        if g[w].v[1] != g[(w + 1) % g.len()].v[0] {
            return None;
        }
    }
    Some(g.iter().map(|c| c.v[0]).collect())
}

fn ints<'h>(h: &'h Heap, v: &[usize]) -> T<'h> {
    h.call(h.sym("list"), &v.iter().map(|&i| h.int(i as i64 + 1)).collect::<Vec<_>>())
}

/// Cells as primitives, one per kind in order of first appearance: `line([[1, 2], [2, 3]])`.
fn grouped<'h>(h: &'h Heap, cells: &[Cell]) -> Vec<T<'h>> {
    let mut kinds: Vec<Kind> = vec![];
    for c in cells {
        if !kinds.contains(&c.kind) {
            kinds.push(c.kind);
        }
    }
    kinds
        .into_iter()
        .map(|k| {
            let lists: Vec<T<'h>> = cells.iter().filter(|c| c.kind == k).map(|c| ints(h, &c.v)).collect();
            h.call(h.sym(k.head()), &[h.call(h.sym("list"), &lists)])
        })
        .collect()
}

fn coords<'h>(h: &'h Heap, p: &[f64]) -> T<'h> {
    h.reals(p)
}

/// The canonical term of a mesh.
pub fn term<'h>(h: &'h Heap, m: &Mesh<'h>) -> T<'h> {
    let list = |v: &[T<'h>]| h.call(h.sym("list"), v);
    let pts = list(&m.pts.iter().map(|p| coords(h, p)).collect::<Vec<_>>());
    let mut args = vec![pts];
    if m.boundary {
        for g in &m.groups {
            args.push(list(&grouped(h, g)));
        }
        let opts = m.opts.unwrap_or_else(|| {
            let sep = h.call(h.sym("rule"), &[h.string("SeparateBoundaries"), h.sym_term(h.sym("false"))]);
            h.call(h.sym("options"), &[h.call(h.sym("rule"), &[h.sym_term(h.sym("method")), list(&[sep])])])
        });
        args.push(opts);
    } else {
        args.push(list(&grouped(h, &m.groups[0])));
        if let Some(o) = m.opts {
            args.push(o);
        }
    }
    h.call(h.sym(if m.boundary { "boundary_mesh_region" } else { "mesh_region" }), &args)
}

/// The faces of dimension `dim(cell) - 1` of a cell.
fn faces(c: &Cell) -> Vec<Cell> {
    let v = &c.v;
    let poly = |idx: &[usize]| Cell { kind: Kind::Polygon, v: idx.iter().map(|&i| v[i]).collect() };
    match c.kind {
        Kind::Point => vec![],
        Kind::Line => v.iter().map(|&i| Cell { kind: Kind::Point, v: vec![i] }).collect(),
        Kind::Polygon => (0..v.len()).map(|i| Cell { kind: Kind::Line, v: vec![v[i], v[(i + 1) % v.len()]] }).collect(),
        Kind::Tetrahedron => [[2, 1, 0], [1, 2, 3], [2, 0, 3], [0, 1, 3]].iter().map(|f| poly(f)).collect(),
        Kind::Hexahedron => HEXAHEDRON.iter().map(|f| poly(f)).collect(),
        Kind::Prism => PRISM.iter().map(|f| poly(f)).collect(),
        Kind::Pyramid => PYRAMID.iter().map(|f| poly(f)).collect(),
    }
}

/// The cells of dimension `d`: for d = 0 every vertex; otherwise the d-faces of the cells in order (each face
/// once), with the given d-cells in their place.
pub fn cells_of_dim(m: &Mesh<'_>, d: usize) -> Vec<Cell> {
    if d == 0 {
        return (0..m.pts.len()).map(|i| Cell { kind: Kind::Point, v: vec![i] }).collect();
    }
    let mut out: Vec<Cell> = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut push = |c: Cell, out: &mut Vec<Cell>| {
        let mut key = c.v.clone();
        key.sort_unstable();
        if seen.insert(key) {
            out.push(c);
        }
    };
    for c in m.cells() {
        let mut level = vec![c.clone()];
        if c.kind.dim() < d {
            continue;
        }
        while level[0].kind.dim() > d {
            level = level.iter().flat_map(faces).collect();
        }
        for f in level {
            push(f, &mut out);
        }
    }
    out
}

/// A cell as a primitive of indices (`line([1, 2])`) or of coordinates (`line([[0.0, 0.0], [1.0, 0.0]])`).
fn cell_term<'h>(h: &'h Heap, m: &Mesh<'h>, c: &Cell, primitive: bool) -> T<'h> {
    let arg = if primitive {
        if c.kind == Kind::Point {
            coords(h, &m.pts[c.v[0]])
        } else {
            h.call(h.sym("list"), &c.v.iter().map(|&i| coords(h, &m.pts[i])).collect::<Vec<_>>())
        }
    } else if c.kind == Kind::Point {
        h.int(c.v[0] as i64 + 1)
    } else {
        ints(h, &c.v)
    };
    h.call(h.sym(c.kind.head()), &[arg])
}

/// The region a mesh describes: a union of its cells (for a boundary mesh region, the solid they bound).
pub fn as_region<'h>(h: &'h Heap, m: &Mesh<'h>) -> Option<Region<'h>> {
    let pt = |i: usize| -> Pt<'h> { m.pts[i].iter().map(|&x| h.real(x)).collect() };
    let dim = m.dim();
    let n = m.pts[0].len();
    if m.boundary {
        return match (n, dim) {
            (1, 0) => {
                let mut xs: Vec<f64> = m.cells().map(|c| m.pts[c.v[0]][0]).collect();
                xs.sort_by(f64::total_cmp);
                xs.len().is_multiple_of(2).then(|| Region::Intervals(xs.chunks(2).map(|w| (h.real(w[0]), h.real(w[1]))).collect()))
            }
            (2, 1) => {
                let segs: Vec<(usize, usize)> = m.cells().filter(|c| c.kind == Kind::Line).map(|c| (c.v[0], c.v[1])).collect();
                let loops = rings_of(&segs)?;
                Some(Region::Polygon(nest(&m.pts, loops).into_iter().map(|(o, hs)| Ring { outer: o.iter().map(|&i| pt(i)).collect(), holes: hs.iter().map(|r| r.iter().map(|&i| pt(i)).collect()).collect() }).collect()))
            }
            (3, 2) => {
                let faces: Vec<Vec<usize>> = m.cells().filter(|c| c.kind == Kind::Polygon).map(|c| c.v.clone()).collect();
                Some(Region::Polyhedron { verts: (0..m.pts.len()).map(pt).collect(), faces })
            }
            _ => None,
        };
    }
    let mut parts = vec![];
    let (mut points, mut lines, mut rings) = (vec![], vec![], vec![]);
    for c in m.cells() {
        match c.kind {
            Kind::Point => points.push(pt(c.v[0])),
            Kind::Line => lines.push(vec![pt(c.v[0]), pt(c.v[1])]),
            Kind::Polygon => rings.push(Ring { outer: c.v.iter().map(|&i| pt(i)).collect(), holes: vec![] }),
            Kind::Tetrahedron => parts.push(Region::Simplex(c.v.iter().map(|&i| pt(i)).collect())),
            k => {
                let f: &[&[usize]] = match k {
                    Kind::Hexahedron => HEXAHEDRON,
                    Kind::Prism => PRISM,
                    _ => PYRAMID,
                };
                parts.push(Region::Polyhedron { verts: c.v.iter().map(|&i| pt(i)).collect(), faces: f.iter().map(|x| x.to_vec()).collect() });
            }
        }
    }
    if !rings.is_empty() {
        parts.insert(0, Region::Polygon(rings));
    }
    if !lines.is_empty() {
        parts.push(Region::Lines(lines));
    }
    if !points.is_empty() {
        parts.push(Region::Points(points));
    }
    Some(if parts.len() == 1 { parts.pop()? } else { Region::Union(parts) })
}

/// Closed loops of segments (each vertex on two segments of its loop).
fn rings_of(segs: &[(usize, usize)]) -> Option<Vec<Vec<usize>>> {
    let mut left: Vec<(usize, usize)> = segs.to_vec();
    let mut out = vec![];
    while let Some((a, b)) = left.pop() {
        let mut ring = vec![a, b];
        while ring.last() != ring.first() {
            let last = *ring.last()?;
            let k = left.iter().position(|&(x, y)| x == last || y == last)?;
            let (x, y) = left.remove(k);
            ring.push(if x == last { y } else { x });
        }
        ring.pop();
        if ring.len() < 3 {
            return None;
        }
        out.push(ring);
    }
    Some(out)
}

/// Rings sorted into outer boundaries with their holes: a ring inside exactly one other is a hole of it.
fn nest(pts: &[Vec<f64>], rings: Vec<Vec<usize>>) -> Vec<(Vec<usize>, Vec<Vec<usize>>)> {
    let polys: Vec<Vec<Vec<f64>>> = rings.iter().map(|r| r.iter().map(|&i| pts[i].clone()).collect()).collect();
    let inside = |i: usize, j: usize| -> bool { i != j && crate::metric::in_polygon(&polys[i][0], &polys[j..j + 1]) };
    let parent: Vec<Option<usize>> = (0..rings.len())
        .map(|i| {
            let holders: Vec<usize> = (0..rings.len()).filter(|&j| inside(i, j)).collect();
            (holders.len() == 1).then(|| holders[0])
        })
        .collect();
    let mut out = vec![];
    for (i, r) in rings.iter().enumerate() {
        if parent[i].is_none() {
            let holes = (0..rings.len()).filter(|&k| parent[k] == Some(i)).map(|k| rings[k].clone()).collect();
            out.push((r.clone(), holes));
        }
    }
    out
}

/// The region of a `mesh_region(...)` or `boundary_mesh_region(...)` term (no evaluation: coordinates must be
/// literal numbers).
pub fn region<'h>(cx: &NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Option<Region<'h>> {
    let h = cx.heap();
    let (args, opts) = match args.split_last() {
        Some((&o, rest)) if call_parts(h, o).is_some_and(|(n, _)| n == "options") => (rest, Some(o)),
        _ => (args, None),
    };
    let [p, specs @ ..] = args else { return None };
    let rows = cx.as_list(*p)?;
    let pts: Vec<Vec<f64>> = rows.iter().map(|&r| cx.as_list(r)?.iter().map(|&x| crate::ex::lit(h, x)).collect::<Option<Vec<f64>>>()).collect::<Option<_>>()?;
    if pts.is_empty() || pts.iter().any(|q| q.len() != pts[0].len()) {
        return None;
    }
    let mut groups = vec![];
    for &s in specs {
        let mut g = vec![];
        let items: Vec<T<'h>> = match cx.as_list(s) {
            Some(l) => l.to_vec(),
            None => vec![s],
        };
        for it in items {
            cells_of(cx, it, pts.len(), &mut g)?;
        }
        groups.push(g);
    }
    let m = Mesh { pts, groups, boundary: name == "boundary_mesh_region", opts };
    as_region(h, &m)
}

// ---- natives

fn mesh_arg<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Mesh<'h>>> {
    let h = cx.heap();
    match call_parts(h, t) {
        Some((n, a)) if n == "mesh_region" || n == "boundary_mesh_region" => Ok(parse(cx, &n, a)?.map(canonical)),
        _ => Ok(None),
    }
}

/// `mesh_region(...)`, `boundary_mesh_region(...)`: the canonical form (unchanged when already canonical).
pub fn construct<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // the cells are held (a cell's index list is not a primitive's coordinates): their arguments are evaluated here
    let mut ev = Vec::with_capacity(a.len());
    for (i, &x) in a.iter().enumerate() {
        ev.push(if i == 0 { x } else { eval_cells(cx, x, 0)? });
    }
    let a = &ev[..];
    let Some(m) = parse(cx, name, a)? else { return Ok(Reply::Decline) };
    let t = term(h, &canonical(m));
    // already canonical: leave the term as it is, so evaluation stops
    let pr = mc_syntax::Printer::new(h);
    let same = pr.expr(t) == pr.expr(h.call(h.sym(name), a));
    Ok(if same { Reply::Decline } else { Reply::Done(t) })
}

const CELL_HEADS: &[&str] = &["point", "line", "polygon", "triangle", "simplex", "tetrahedron", "hexahedron", "prism", "pyramid"];

/// A held cell specification evaluated below the cell heads: lists element by element, a cell's arguments, and
/// anything else evaluated (once more if it gives a list or a cell).
fn eval_cells<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize) -> Res<T<'h>> {
    let h = cx.heap();
    if let Some(items) = cx.as_list(t).map(|l| l.to_vec()) {
        let mut out = Vec::with_capacity(items.len());
        for x in items {
            out.push(eval_cells(cx, x, depth)?);
        }
        return Ok(cx.list(&out));
    }
    if let View::App { head, args } = h.view(t) {
        if let View::Sym(s) = h.view(head) {
            let n = h.sym_name(s);
            if CELL_HEADS.contains(&n.as_str()) {
                let mut out = Vec::with_capacity(args.len());
                for &x in args {
                    out.push(cx.eval(x)?);
                }
                return Ok(h.call(h.sym(&n), &out));
            }
            if n == "options" {
                return cx.eval(t);
            }
        }
    }
    if depth > 0 {
        return Ok(t);
    }
    let v = cx.eval(t)?;
    eval_cells(cx, v, depth + 1)
}

/// Which cells a specification picks: (dimension, indices from 0 or all).
fn selection<'h>(cx: &NativeCx<'_, '_, 'h>, m: &Mesh<'h>, spec: T<'h>) -> Option<Vec<(usize, Option<Vec<usize>>)>> {
    let h = cx.heap();
    let top = m.dim();
    if let Some(d) = int_of(h, spec) {
        return (d >= 0 && d as usize <= top).then(|| vec![(d as usize, None)]);
    }
    if matches!(h.view(spec), View::Sym(s) if h.sym_name(s) == "all") {
        return Some((0..=top).map(|d| (d, None)).collect());
    }
    if let Some((n, alts)) = call_parts(h, spec).filter(|(n, _)| n == "alternatives") {
        let _ = n;
        let mut ds: Vec<usize> = alts.iter().map(|&x| int_of(h, x).filter(|&d| d >= 0 && d as usize <= top).map(|d| d as usize)).collect::<Option<_>>()?;
        ds.sort_unstable();
        return Some(ds.into_iter().map(|d| (d, None)).collect());
    }
    let &[d, k] = cx.as_list(spec)? else { return None };
    let d = int_of(h, d).filter(|&d| d >= 0 && d as usize <= top)? as usize;
    if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "all") {
        return Some(vec![(d, None)]);
    }
    let count = cells_of_dim(m, d).len();
    let idx = |x: T<'h>| int_of(h, x).filter(|&i| i >= 1 && i as usize <= count).map(|i| i as usize - 1);
    match idx(k) {
        Some(i) => Some(vec![(d, Some(vec![i]))]),
        None => Some(vec![(d, Some(cx.as_list(k)?.iter().map(|&x| idx(x)).collect::<Option<_>>()?))]),
    }
}

/// `mesh_cells(m, spec)` (primitive false) and `mesh_primitives(m, spec)` (true).
pub fn cells<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], primitive: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [mt, spec] = a else { return Ok(Reply::Decline) };
    let Some(m) = mesh_arg(cx, *mt)? else { return Ok(Reply::Decline) };
    let single_dim = int_of(h, *spec).is_some() || cx.as_list(*spec).is_some_and(|l| l.len() == 2 && int_of(h, l[0]).is_some());
    // a list of cells: those cells (segments for a line)
    if !single_dim {
        if let Some(items) = cx.as_list(*spec).map(|l| l.to_vec()) {
            if items.iter().all(|&x| call_parts(h, x).is_some()) {
                let mut cs = vec![];
                for it in items {
                    if cells_of(cx, it, m.pts.len(), &mut cs).is_none() {
                        return Ok(Reply::Decline);
                    }
                }
                // each as the mesh's own cell (with its orientation) where it is one
                let own = |c: &Cell| -> Cell {
                    let mut key = c.v.clone();
                    key.sort_unstable();
                    cells_of_dim(&m, c.kind.dim()).into_iter().find(|o| {
                        let mut k = o.v.clone();
                        k.sort_unstable();
                        k == key
                    }).unwrap_or_else(|| c.clone())
                };
                return Ok(Reply::Done(cx.list(&cs.iter().map(|c| cell_term(h, &m, &own(c), primitive)).collect::<Vec<_>>())));
            }
        }
    }
    let Some(sel) = selection(cx, &m, *spec) else { return Ok(Reply::Decline) };
    let mut out = vec![];
    for (d, idx) in &sel {
        let all = cells_of_dim(&m, *d);
        let picked: Vec<T<'h>> = match idx {
            None => all.iter().map(|c| cell_term(h, &m, c, primitive)).collect(),
            Some(ix) => ix.iter().map(|&i| cell_term(h, &m, &all[i], primitive)).collect(),
        };
        out.push(picked);
    }
    let one_index = cx.as_list(*spec).is_some_and(|l| l.len() == 2 && int_of(h, l[1]).is_some());
    Ok(Reply::Done(if sel.len() == 1 {
        if one_index && out[0].len() == 1 {
            out[0][0]
        } else {
            cx.list(&out[0])
        }
    } else {
        cx.list(&out.iter().map(|v| cx.list(v)).collect::<Vec<_>>())
    }))
}

/// `mesh_cell_count(m)`, `mesh_cell_count(m, d)`, `mesh_cell_count(m, all)`.
pub fn cell_count<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(&mt) = a.first() else { return Ok(Reply::Decline) };
    let Some(m) = mesh_arg(cx, mt)? else { return Ok(Reply::Decline) };
    let counts: Vec<usize> = (0..=m.dim()).map(|d| cells_of_dim(&m, d).len()).collect();
    Ok(Reply::Done(match a {
        [_] => cx.list(&counts.iter().map(|&c| h.int(c as i64)).collect::<Vec<_>>()),
        [_, d] => match int_of(h, *d) {
            Some(d) if d >= 0 && (d as usize) < counts.len() => h.int(counts[d as usize] as i64),
            Some(_) => h.int(0),
            None if matches!(h.view(*d), View::Sym(s) if h.sym_name(s) == "all") => h.int(counts.iter().sum::<usize>() as i64),
            None => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    }))
}

/// `mesh_coordinates(m)`.
pub fn coordinates<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [mt] = a else { return Ok(Reply::Decline) };
    let Some(m) = mesh_arg(cx, *mt)? else { return Ok(Reply::Decline) };
    Ok(Reply::Done(cx.list(&m.pts.iter().map(|p| coords(h, p)).collect::<Vec<_>>())))
}

/// `is_mesh_region(m)` / `is_boundary_mesh_region(m)`.
pub fn is_mesh<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], boundary: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [mt] = a else { return Ok(Reply::Decline) };
    let ok = match mesh_arg(cx, *mt)? {
        Some(m) => m.boundary == boundary,
        None => false,
    };
    Ok(Reply::Done(crate::metric::truth(h, ok)))
}

/// The mesh of the cells of one dimension, or of one connected piece.
fn sub_mesh<'h>(h: &'h Heap, m: &Mesh<'h>, cells: Vec<Cell>) -> T<'h> {
    let sub = Mesh { pts: m.pts.clone(), groups: vec![cells], boundary: false, opts: None };
    term(h, &canonical(sub))
}

/// `dimensional_mesh_components(m)`: the cells of each dimension as meshes (lowest first).
pub fn dimensional<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [mt] = a else { return Ok(Reply::Decline) };
    let Some(m) = mesh_arg(cx, *mt)? else { return Ok(Reply::Decline) };
    let mut out = vec![];
    for d in 0..=m.dim() {
        let cs: Vec<Cell> = m.cells().filter(|c| c.kind.dim() == d).cloned().collect();
        out.push(if cs.is_empty() { h.call(h.sym("empty_region"), &[h.int(m.pts[0].len() as i64)]) } else { sub_mesh(h, &m, cs) });
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `connected_mesh_components(m)`: the pieces of cells joined through shared vertices, in the order of their
/// first cell.
pub fn connected<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [mt] = a else { return Ok(Reply::Decline) };
    let Some(m) = mesh_arg(cx, *mt)? else { return Ok(Reply::Decline) };
    let cells: Vec<Cell> = m.cells().cloned().collect();
    // union-find over vertices
    let mut parent: Vec<usize> = (0..m.pts.len()).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        p[i] = r;
        r
    }
    for c in &cells {
        for w in c.v.windows(2) {
            let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
            parent[a] = b;
        }
    }
    let mut roots: Vec<usize> = vec![];
    let mut pieces: Vec<Vec<Cell>> = vec![];
    for c in cells {
        let r = find(&mut parent, c.v[0]);
        match roots.iter().position(|&x| x == r) {
            Some(k) => pieces[k].push(c),
            None => {
                roots.push(r);
                pieces.push(vec![c]);
            }
        }
    }
    if m.boundary {
        // each piece keeps its boundary groups
        let mut out = vec![];
        for r in roots {
            let groups: Vec<Vec<Cell>> = m.groups.iter().filter(|g| g.iter().any(|c| find(&mut parent, c.v[0]) == r)).cloned().collect();
            let sub = Mesh { pts: m.pts.clone(), groups, boundary: true, opts: m.opts };
            out.push(term(h, &canonical(sub)));
        }
        return Ok(Reply::Done(cx.list(&out)));
    }
    Ok(Reply::Done(cx.list(&pieces.into_iter().map(|p| sub_mesh(h, &m, p)).collect::<Vec<_>>())))
}
