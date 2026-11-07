#![allow(clippy::type_complexity, clippy::needless_range_loop)]
//! Graphs (networks): the term `graph(edges)` or `graph(vertices, edges)` (edges `a -> b` directed,
//! `undirected_edge(a, b)` or `two_way_rule(a, b)` undirected, `directed_edge(a, b)`), shown as its picture;
//! `graph_plot`, `layered_graph_plot` and `tree_plot` draw one; constructors (`complete_graph`, `cycle_graph`,
//! `path_graph`, `star_graph`, `grid_graph`) and queries (`vertex_list`, `edge_list`, `vertex_count`,
//! `edge_count`, `vertex_degree`, `adjacency_matrix`).
//!
//! Layouts: `spring` (force-directed, Fruchterman-Reingold from a circle, deterministic), `circular` (the
//! vertices in order on a circle), `layered` (longest-path layers of the edges' direction, cycles broken, the
//! order within layers improved by barycentre sweeps; for trees and directed graphs). The option
//! `graph_layout: "SpringEmbedding" | "CircularEmbedding" | "LayeredDigraphEmbedding" | "LayeredEmbedding" |
//! spring | circular | layered` chooses; `vertex_labels: automatic` (or `"Name"`) writes the vertices' names.
use crate::more::split;
use crate::{bad, finish_graphic};
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    k.register(h.sym("graph"), a(), Box::new(GraphTerm));
    for (name, layout) in [("graph_plot", Layout::Auto), ("layered_graph_plot", Layout::Layered), ("tree_plot", Layout::Layered)] {
        k.register(h.sym(name), a(), Box::new(GraphPlot { name, layout }));
    }
    for name in ["complete_graph", "cycle_graph", "path_graph", "star_graph", "grid_graph"] {
        k.register(h.sym(name), a(), Box::new(Make(name)));
    }
    for name in ["vertex_list", "edge_list", "vertex_count", "edge_count", "vertex_degree", "adjacency_matrix"] {
        k.register(h.sym(name), a(), Box::new(Query(name)));
    }
}

fn head_args<'h>(h: &'h Heap, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => match h.view(head) {
            View::Sym(s) => Some((h.sym_name(s), args)),
            _ => None,
        },
        _ => None,
    }
}

fn items<'h>(h: &'h Heap, t: T<'h>) -> Option<&'h [T<'h>]> {
    match head_args(h, t) {
        Some((n, a)) if n == names::LIST => Some(a),
        _ => None,
    }
}

/// A term without the wrappers that style or label it.
fn bare<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    match head_args(h, t) {
        Some((n, a)) if matches!(n.as_str(), "labeled" | "style" | "property" | "annotation" | "tooltip") && !a.is_empty() => bare(h, a[0]),
        _ => t,
    }
}

/// A graph read from its term: vertices, edges (by index, directed or not), options.
pub(crate) struct G<'h> {
    pub vertices: Vec<T<'h>>,
    pub edges: Vec<(usize, usize, bool)>,
    pub options: Option<T<'h>>,
}

fn edge<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, T<'h>, bool)> {
    let (n, a) = head_args(h, bare(h, t))?;
    match (n.as_str(), a) {
        (names::RULE | "directed_edge", &[x, y]) => Some((x, y, true)),
        ("undirected_edge" | "two_way_rule", &[x, y]) => Some((x, y, false)),
        _ => None,
    }
}

/// The graph of a `graph(...)` term, or of a list of edges.
pub(crate) fn read<'h>(h: &'h Heap, t: T<'h>) -> Option<G<'h>> {
    let (verts, edges, options) = match head_args(h, t) {
        Some((n, a)) if n == "graph" => {
            let (a, o) = split(h, a);
            match a.as_slice() {
                [e] => (vec![], items(h, *e)?.to_vec(), o),
                [v, e] => (items(h, *v)?.to_vec(), items(h, *e)?.to_vec(), o),
                _ => return None,
            }
        }
        Some((n, a)) if n == names::LIST => (vec![], a.to_vec(), None),
        _ => {
            // a single edge
            edge(h, t)?;
            (vec![], vec![t], None)
        }
    };
    let mut vertices: Vec<T<'h>> = vec![];
    let index = |v: T<'h>, vertices: &mut Vec<T<'h>>| -> usize {
        if let Some(k) = vertices.iter().position(|&w| w == v) {
            return k;
        }
        vertices.push(v);
        vertices.len() - 1
    };
    for v in verts {
        index(bare(h, v), &mut vertices);
    }
    let mut es = vec![];
    for e in edges {
        let (a, b, d) = edge(h, e)?;
        let (i, j) = (index(a, &mut vertices), index(b, &mut vertices));
        es.push((i, j, d));
    }
    Some(G { vertices, edges: es, options })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Layout {
    Auto,
    Spring,
    Circular,
    Layered,
}

/// Positions of the vertices.
pub(crate) fn layout(n: usize, edges: &[(usize, usize, bool)], kind: Layout) -> Vec<[f64; 2]> {
    if n == 0 {
        return vec![];
    }
    let circle = |n: usize| -> Vec<[f64; 2]> {
        (0..n)
            .map(|k| {
                let a = std::f64::consts::FRAC_PI_2 - std::f64::consts::TAU * k as f64 / n as f64;
                [a.cos(), a.sin()]
            })
            .collect()
    };
    match kind {
        Layout::Circular => circle(n),
        Layout::Layered => layered(n, edges),
        Layout::Spring | Layout::Auto => {
            if n == 1 {
                return vec![[0.0, 0.0]];
            }
            // Fruchterman-Reingold, starting from the circle (a fixed start: the same picture every time)
            let mut p = circle(n);
            // a little asymmetry so that symmetric starts can unfold
            for (k, q) in p.iter_mut().enumerate() {
                q[0] += 0.01 * ((k * 7919) % 13) as f64 / 13.0;
                q[1] += 0.01 * ((k * 104_729) % 17) as f64 / 17.0;
            }
            let kk = (4.0 / n as f64).sqrt();
            let mut temp = 0.3;
            let iters = 400;
            for _ in 0..iters {
                let mut disp = vec![[0.0f64; 2]; n];
                for i in 0..n {
                    for j in i + 1..n {
                        let d = [p[i][0] - p[j][0], p[i][1] - p[j][1]];
                        let dist = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
                        let f = kk * kk / dist;
                        for c in 0..2 {
                            disp[i][c] += d[c] / dist * f;
                            disp[j][c] -= d[c] / dist * f;
                        }
                    }
                }
                for &(a, b, _) in edges {
                    if a == b {
                        continue;
                    }
                    let d = [p[a][0] - p[b][0], p[a][1] - p[b][1]];
                    let dist = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
                    let f = dist * dist / kk;
                    for c in 0..2 {
                        disp[a][c] -= d[c] / dist * f;
                        disp[b][c] += d[c] / dist * f;
                    }
                }
                for i in 0..n {
                    let len = (disp[i][0].powi(2) + disp[i][1].powi(2)).sqrt().max(1e-12);
                    let step = len.min(temp);
                    for c in 0..2 {
                        p[i][c] += disp[i][c] / len * step;
                    }
                    // a weak pull to the centre keeps separate pieces near
                    for c in 0..2 {
                        p[i][c] *= 0.999;
                    }
                }
                temp = (temp * 0.985).max(0.002);
            }
            p
        }
    }
}

/// Longest-path layers (sources at the top), cycles broken by a depth-first order, the order within each layer
/// by barycentres of the layer above (a few sweeps both ways).
fn layered(n: usize, edges: &[(usize, usize, bool)]) -> Vec<[f64; 2]> {
    // the edges oriented along a depth-first order: back edges reversed
    let mut adj = vec![vec![]; n];
    for &(a, b, _) in edges {
        if a != b {
            adj[a].push(b);
        }
    }
    let mut order = vec![usize::MAX; n];
    let mut state = vec![0u8; n];
    let mut counter = 0;
    let mut dag: Vec<(usize, usize)> = vec![];
    // roots first: vertices nothing points to
    let mut indeg = vec![0; n];
    for &(_, b, _) in edges {
        indeg[b] += 1;
    }
    let mut starts: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    starts.extend(0..n);
    for s in starts {
        if state[s] != 0 {
            continue;
        }
        let mut stack = vec![(s, 0usize)];
        state[s] = 1;
        order[s] = counter;
        counter += 1;
        while let Some(&mut (v, ref mut i)) = stack.last_mut() {
            if *i < adj[v].len() {
                let w = adj[v][*i];
                *i += 1;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        order[w] = counter;
                        counter += 1;
                        dag.push((v, w));
                        stack.push((w, 0));
                    }
                    1 => dag.push((w, v)),
                    _ => dag.push((v, w)),
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
    // longest path layering
    let mut layer = vec![0usize; n];
    let mut changed = true;
    let mut guard = 0;
    while changed && guard < n + 2 {
        changed = false;
        guard += 1;
        for &(a, b) in &dag {
            if layer[b] < layer[a] + 1 {
                layer[b] = layer[a] + 1;
                changed = true;
            }
        }
    }
    let depth = layer.iter().copied().max().unwrap_or(0);
    let mut rows: Vec<Vec<usize>> = vec![vec![]; depth + 1];
    let mut by_order: Vec<usize> = (0..n).collect();
    by_order.sort_by_key(|&v| order[v]);
    for v in by_order {
        rows[layer[v]].push(v);
    }
    let mut pos = vec![0.0f64; n];
    let place = |rows: &Vec<Vec<usize>>, pos: &mut Vec<f64>| {
        for row in rows {
            for (k, &v) in row.iter().enumerate() {
                pos[v] = k as f64 - (row.len() as f64 - 1.0) / 2.0;
            }
        }
    };
    place(&rows, &mut pos);
    for sweep in 0..6 {
        let down = sweep % 2 == 0;
        let range: Vec<usize> = if down { (1..rows.len()).collect() } else { (0..rows.len().saturating_sub(1)).rev().collect() };
        for r in range {
            let mut bary: Vec<(f64, usize)> = rows[r]
                .iter()
                .map(|&v| {
                    let nbrs: Vec<f64> = dag
                        .iter()
                        .filter_map(|&(a, b)| {
                            if down && b == v && layer[a] + 1 == layer[v] {
                                Some(pos[a])
                            } else if !down && a == v && layer[b] == layer[v] + 1 {
                                Some(pos[b])
                            } else {
                                None
                            }
                        })
                        .collect();
                    (if nbrs.is_empty() { pos[v] } else { nbrs.iter().sum::<f64>() / nbrs.len() as f64 }, v)
                })
                .collect();
            bary.sort_by(|a, b| a.0.total_cmp(&b.0));
            rows[r] = bary.into_iter().map(|x| x.1).collect();
            place(&rows, &mut pos);
        }
    }
    (0..n).map(|v| [pos[v], -(layer[v] as f64)]).collect()
}

fn option<'h>(h: &'h Heap, o: Option<T<'h>>, key: &str) -> Option<T<'h>> {
    let (_, rules) = head_args(h, o?)?;
    rules.iter().find_map(|&r| match head_args(h, r) {
        Some((n, &[k, v])) if n == names::RULE && matches!(h.view(k), View::Sym(s) if h.sym_name(s) == key) => Some(v),
        _ => None,
    })
}

fn layout_option(h: &Heap, o: Option<T<'_>>) -> Option<Layout> {
    let v = option(h, o, "graph_layout")?;
    let name = match h.view(v) {
        View::Str(s) => s.to_string(),
        View::Sym(s) => h.sym_name(s),
        _ => return None,
    };
    let n = name.to_lowercase();
    Some(if n.contains("circ") {
        Layout::Circular
    } else if n.contains("layer") || n.contains("tree") {
        Layout::Layered
    } else {
        Layout::Spring
    })
}

/// The picture of a graph as a graphics term.
pub(crate) fn picture<'h>(h: &'h Heap, g: &G<'h>, kind: Layout, extra: Option<T<'h>>) -> T<'h> {
    let kind = layout_option(h, g.options).or_else(|| layout_option(h, extra)).unwrap_or(kind);
    let pos = layout(g.vertices.len(), &g.edges, kind);
    let list = |a: &[T<'h>]| h.call(h.sym(names::LIST), a);
    let pt = |p: [f64; 2]| h.reals(&p);
    let mut prims = vec![];
    // edges: dark gray lines, arrows when directed; loops as small circles
    let span = pos.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p[0]).min(p[1]), b.max(p[0]).max(p[1])));
    let size = (span.1 - span.0).max(1.0);
    let edge_col = h.call(h.sym("rgb_color"), &[h.real(0.42), h.real(0.45), h.real(0.55)]);
    let mut lines = vec![edge_col, h.call(h.sym("absolute_thickness"), &[h.real(1.2)]), h.call(h.sym("arrowheads"), &[h.real(0.035)])];
    for &(a, b, directed) in &g.edges {
        if a == b {
            let c = [pos[a][0], pos[a][1] + 0.06 * size];
            lines.push(h.call(h.sym("circle"), &[pt(c), h.real(0.06 * size)]));
            continue;
        }
        let (p, q) = (pos[a], pos[b]);
        if directed {
            // stop short of the vertex's dot
            let d = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt().max(1e-12);
            let cut = (0.03 * size).min(d / 3.0);
            let q2 = [q[0] - (q[0] - p[0]) / d * cut, q[1] - (q[1] - p[1]) / d * cut];
            lines.push(h.call(h.sym("arrow"), &[list(&[pt(p), pt(q2)])]));
        } else {
            lines.push(h.call(h.sym("line"), &[list(&[pt(p), pt(q)])]));
        }
    }
    prims.push(list(&lines));
    let vcol = h.call(h.sym("rgb_color"), &[h.real(0.85), h.real(0.38), h.real(0.15)]);
    let dots: Vec<T<'h>> = pos.iter().map(|&p| pt(p)).collect();
    prims.push(list(&[vcol, h.call(h.sym("absolute_point_size"), &[h.real(8.0)]), h.call(h.sym("point"), &[list(&dots)])]));
    let labels = option(h, g.options, "vertex_labels").or_else(|| option(h, extra, "vertex_labels"));
    let show = labels.is_some_and(|v| match h.view(v) {
        View::Str(s) => s == "Name",
        View::Sym(s) => matches!(h.sym_name(s).as_str(), "automatic" | "true" | "name"),
        _ => false,
    });
    if show {
        let mut ts = vec![h.call(h.sym("gray_level"), &[h.real(0.1)])];
        for (k, &v) in g.vertices.iter().enumerate() {
            let at = [pos[k][0] + 0.03 * size, pos[k][1] + 0.04 * size];
            ts.push(h.call(h.sym("text"), &[v, pt(at)]));
        }
        prims.push(list(&ts));
    }
    let rule = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v]);
    let pad = 0.08 * size;
    let (xs, ys): (Vec<f64>, Vec<f64>) = pos.iter().map(|p| (p[0], p[1])).unzip();
    let lo = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min) - pad;
    let hi = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max) + pad;
    let opts = vec![rule("plot_range", list(&[h.reals(&[lo(&xs), hi(&xs)]), h.reals(&[lo(&ys), hi(&ys)])])), rule("aspect_ratio", h.sym_term(h.sym("automatic")))];
    h.call(h.sym("graphics"), &[list(&prims), h.call(h.sym("options"), &opts)])
}

/// Whether `t` is a graph term (shown as its picture).
pub fn is_graph<'h>(h: &'h Heap, t: T<'h>) -> bool {
    matches!(head_args(h, t), Some((n, _)) if n == "graph") && read(h, t).is_some()
}

/// The picture of a graph term, for showing it.
pub fn graph_picture<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    let g = read(h, t)?;
    Some(picture(h, &g, Layout::Auto, None))
}

/// `graph(...)`: stays as it is (it is shown as its picture).
struct GraphTerm;

impl Native for GraphTerm {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(Reply::Decline)
    }
}

struct GraphPlot {
    name: &'static str,
    layout: Layout,
}

impl Native for GraphPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = split(h, args);
        let [g] = a.as_slice() else { return Ok(Reply::Decline) };
        // an adjacency matrix
        let g = match adjacency(cx, *g) {
            Some(t) => t,
            None => *g,
        };
        let Some(gr) = read(h, g) else { return Ok(bad(h, self.name, 1)) };
        let pic = picture(h, &gr, self.layout, o);
        finish_graphic(cx, pic, self.name, &[])
    }
}

/// The graph of a square 0/1 (or weight) matrix: an edge i -> j for every nonzero entry.
fn adjacency<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<T<'h>> {
    let h = cx.heap();
    let rows = cx.as_list(t)?;
    let n = rows.len();
    let mut edges = vec![];
    let mut sym = true;
    let mut m = vec![vec![0.0; n]; n];
    for (i, &r) in rows.iter().enumerate() {
        let r = cx.as_list(r)?;
        if r.len() != n {
            return None;
        }
        for (j, &x) in r.iter().enumerate() {
            m[i][j] = mc_graphics::num::number(h, x)?;
        }
    }
    for i in 0..n {
        for j in 0..n {
            if m[i][j] != m[j][i] {
                sym = false;
            }
        }
    }
    for i in 0..n {
        for j in 0..n {
            if m[i][j] != 0.0 && (!sym || j >= i) {
                let head = if sym { "undirected_edge" } else { names::RULE };
                edges.push(h.call(h.sym(head), &[h.int(i as i64 + 1), h.int(j as i64 + 1)]));
            }
        }
    }
    let verts: Vec<T<'h>> = (1..=n as i64).map(|k| h.int(k)).collect();
    Some(h.call(h.sym("graph"), &[cx.list(&verts), cx.list(&edges)]))
}

/// Graph constructors.
struct Make(&'static str);

impl Native for Make {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let ue = |a: i64, b: i64| h.call(h.sym("undirected_edge"), &[h.int(a), h.int(b)]);
        let (a, _o) = split(h, args);
        let count = |t: T<'h>| match h.view(t) {
            View::Int(n) if (1..=2000).contains(&n) => Some(n),
            _ => None,
        };
        let (verts, edges): (Vec<T<'h>>, Vec<T<'h>>) = match (self.0, a.as_slice()) {
            ("complete_graph", [n]) => {
                let Some(n) = count(*n) else { return Ok(Reply::Decline) };
                let mut e = vec![];
                for i in 1..=n {
                    for j in i + 1..=n {
                        e.push(ue(i, j));
                    }
                }
                ((1..=n).map(|k| h.int(k)).collect(), e)
            }
            ("cycle_graph", [n]) => {
                let Some(n) = count(*n) else { return Ok(Reply::Decline) };
                ((1..=n).map(|k| h.int(k)).collect(), (1..=n).map(|i| ue(i, i % n + 1)).collect())
            }
            ("star_graph", [n]) => {
                let Some(n) = count(*n) else { return Ok(Reply::Decline) };
                ((1..=n).map(|k| h.int(k)).collect(), (2..=n).map(|i| ue(1, i)).collect())
            }
            ("path_graph", [l]) => {
                let Some(vs) = cx.as_list(*l).map(|v| v.to_vec()) else { return Ok(Reply::Decline) };
                let e = vs.windows(2).map(|w| h.call(h.sym("undirected_edge"), &[w[0], w[1]])).collect();
                (vs, e)
            }
            ("grid_graph", [d]) => {
                let Some(&[m, n]) = cx.as_list(*d) else { return Ok(Reply::Decline) };
                let (Some(m), Some(n)) = (count(m), count(n)) else { return Ok(Reply::Decline) };
                let id = |i: i64, j: i64| i * n + j + 1;
                let mut e = vec![];
                for i in 0..m {
                    for j in 0..n {
                        if j + 1 < n {
                            e.push(ue(id(i, j), id(i, j + 1)));
                        }
                        if i + 1 < m {
                            e.push(ue(id(i, j), id(i + 1, j)));
                        }
                    }
                }
                ((1..=m * n).map(|k| h.int(k)).collect(), e)
            }
            _ => return Ok(Reply::Decline),
        };
        Ok(Reply::Done(h.call(h.sym("graph"), &[cx.list(&verts), cx.list(&edges)])))
    }
}

/// Graph queries.
struct Query(&'static str);

impl Native for Query {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let Some(&g) = args.first() else { return Ok(Reply::Decline) };
        if !matches!(head_args(h, g), Some((n, _)) if n == "graph") {
            return Ok(Reply::Decline);
        }
        let Some(gr) = read(h, g) else { return Ok(Reply::Decline) };
        let n = gr.vertices.len();
        let edge_term = |&(a, b, d): &(usize, usize, bool)| h.call(h.sym(if d { "directed_edge" } else { "undirected_edge" }), &[gr.vertices[a], gr.vertices[b]]);
        let degree = |v: usize| gr.edges.iter().map(|&(a, b, _)| usize::from(a == v) + usize::from(b == v)).sum::<usize>();
        Ok(Reply::Done(match (self.0, &args[1..]) {
            ("vertex_list", []) => cx.list(&gr.vertices),
            ("edge_list", []) => cx.list(&gr.edges.iter().map(edge_term).collect::<Vec<_>>()),
            ("vertex_count", []) => h.int(n as i64),
            ("edge_count", []) => h.int(gr.edges.len() as i64),
            ("vertex_degree", []) => cx.list(&(0..n).map(|v| h.int(degree(v) as i64)).collect::<Vec<_>>()),
            ("vertex_degree", [v]) => match gr.vertices.iter().position(|&w| w == *v) {
                Some(k) => h.int(degree(k) as i64),
                None => return Ok(Reply::Decline),
            },
            ("adjacency_matrix", []) => {
                let mut m = vec![vec![0i64; n]; n];
                for &(a, b, d) in &gr.edges {
                    m[a][b] += 1;
                    if !d && a != b {
                        m[b][a] += 1;
                    }
                }
                let rows: Vec<T<'h>> = m.iter().map(|r| cx.list(&r.iter().map(|&x| h.int(x)).collect::<Vec<_>>())).collect();
                cx.list(&rows)
            }
            _ => return Ok(Reply::Decline),
        }))
    }
}
