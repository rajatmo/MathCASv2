//! The natives of the region functions.
use crate::clip::{self, Op, P};
use crate::ex::{inexact, num, nums, points, E};
use crate::metric::{self, Query};
use crate::props;
use crate::region::{call_parts, is_vector, read, Radius, Region};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{Options, Var};

type Body = for<'a, 'b, 'h> fn(&mut NativeCx<'a, 'b, 'h>, &[T<'h>]) -> Res<Reply<'h>>;

struct Fun(Body);

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        cx.tick(1)?;
        (self.0)(cx, args)
    }
}

/// Register the natives.
pub fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    let mut reg = |name: &str, f: Body| k.register(h.sym(name), a(), Box::new(Fun(f)));
    for inert in ["region", "implicit_region", "parametric_region", "tetrahedron", "empty_region"] {
        reg(inert, |_, _| Ok(Reply::Decline));
    }
    // a simplex of one point is that point
    reg("simplex", |cx, a| {
        let h = cx.heap();
        Ok(match a {
            [p] => match cx.as_list(*p) {
                Some(&[q]) if cx.as_list(q).is_some_and(|c| !c.is_empty() && c.iter().all(|&x| cx.as_list(x).is_none())) => Reply::Done(h.call(h.sym("point"), &[q])),
                _ => Reply::Decline,
            },
            _ => Reply::Decline,
        })
    });
    reg("region_member", region_member);
    reg("region_measure", |cx, a| measure_of(cx, a, None));
    reg("area", |cx, a| match lens(cx, a, 1)? {
        Some(r) => Ok(r),
        None => measure_of(cx, a, Some(2)),
    });
    reg("volume", |cx, a| measure_of(cx, a, Some(3)));
    reg("arc_length", |cx, a| measure_of(cx, a, Some(1)));
    reg("region_centroid", region_centroid);
    reg("region_bounds", region_bounds);
    reg("region_dimension", |cx, a| {
        let Some(r) = one_region(cx, a) else { return Ok(Reply::Decline) };
        Ok(match props::dimension(cx, &r)? {
            Some(d) => Reply::Done(cx.heap().int(d as i64)),
            None => Reply::Decline,
        })
    });
    reg("region_embedding_dimension", |cx, a| {
        let Some(r) = one_region(cx, a) else { return Ok(Reply::Decline) };
        Ok(Reply::Done(cx.heap().int(r.embedding() as i64)))
    });
    reg("is_bounded_region", |cx, a| {
        let Some(r) = one_region(cx, a) else {
            // a set operation left unevaluated: a union is bounded when every part is, a difference when its first
            // region is
            if let [t] = a {
                if let Some(b) = bounded_set_operation(cx, *t)? {
                    return Ok(Reply::Done(metric::truth(cx.heap(), b)));
                }
            }
            // anything else that is not a region is not a bounded region
            return Ok(match a {
                [_] => Reply::Done(metric::truth(cx.heap(), false)),
                _ => Reply::Decline,
            });
        };
        Ok(match props::bounded(cx, &r)? {
            Some(b) => Reply::Done(metric::truth(cx.heap(), b)),
            None => Reply::Decline,
        })
    });
    reg("region_distance", |cx, a| distance(cx, a, Query::Distance));
    reg("signed_region_distance", |cx, a| distance(cx, a, Query::Signed));
    reg("region_nearest", |cx, a| distance(cx, a, Query::Nearest));
    reg("region_union", |cx, a| boolean(cx, a, Op::Union));
    reg("region_intersection", |cx, a| boolean(cx, a, Op::Intersection));
    reg("region_difference", |cx, a| boolean(cx, a, Op::Difference));
    reg("region_integrate", region_integrate);
    // round 18: more primitives stay inert; mesh regions, triangles, polygons
    reg("mesh_cells", |cx, a| crate::mesh::cells(cx, a, false));
    reg("mesh_primitives", |cx, a| crate::mesh::cells(cx, a, true));
    reg("mesh_cell_count", crate::mesh::cell_count);
    reg("mesh_coordinates", crate::mesh::coordinates);
    reg("is_mesh_region", |cx, a| crate::mesh::is_mesh(cx, a, false));
    reg("is_boundary_mesh_region", |cx, a| crate::mesh::is_mesh(cx, a, true));
    reg("dimensional_mesh_components", crate::mesh::dimensional);
    reg("connected_mesh_components", crate::mesh::connected);
    reg("sss_triangle", |cx, a| crate::construct::triangle_of(cx, a, "sss"));
    reg("sas_triangle", |cx, a| crate::construct::triangle_of(cx, a, "sas"));
    reg("asa_triangle", |cx, a| crate::construct::triangle_of(cx, a, "asa"));
    reg("aas_triangle", |cx, a| crate::construct::triangle_of(cx, a, "aas"));
    reg("perimeter", |cx, a| match crate::coords::perimeter(cx, a)? {
        Some(t) => Ok(Reply::Done(t)),
        None => crate::construct::perimeter(cx, a),
    });
    reg("circumsphere", crate::construct::circumsphere);
    reg("region_moment", |cx, a| match lens(cx, a, 2)? {
        Some(r) => Ok(r),
        None => crate::construct::region_moment(cx, a),
    });
    reg("is_constant_region", crate::construct::is_constant_region);
    reg("is_simple_polygon", |cx, a| crate::construct::polygon_test(cx, a, false));
    reg("is_convex_polygon", |cx, a| crate::construct::polygon_test(cx, a, true));
    reg("canonicalize_polygon", |cx, a| crate::construct::polygon_form(cx, a, "canonical"));
    reg("outer_polygon", |cx, a| crate::construct::polygon_form(cx, a, "outer"));
    reg("inner_polygon", |cx, a| crate::construct::polygon_form(cx, a, "inner"));
    reg("polygon_coordinates", |cx, a| crate::construct::polygon_form(cx, a, "coordinates"));
    reg("bounding_region", crate::construct::bounding_region);
    reg("winding_count", |cx, a| crate::construct::winding(cx, a, false));
    reg("crossing_count", |cx, a| crate::construct::winding(cx, a, true));
    reg("region_equal", |cx, a| crate::construct::relation(cx, a, "equal"));
    reg("region_within", |cx, a| crate::construct::relation(cx, a, "within"));
    reg("region_disjoint", |cx, a| crate::construct::relation(cx, a, "disjoint"));
    reg("winding_polygon", |cx, a| crate::derived::enclosed(cx, a, clip::Fill::NonZero));
    reg("crossing_polygon", |cx, a| crate::derived::enclosed(cx, a, clip::Fill::EvenOdd));
    reg("polygon_decomposition", crate::derived::decomposition);
    reg("boolean_region", crate::derived::boolean_region);
    reg("transformed_region", crate::derived::transformed);
    reg("discretize_region", crate::derived::discretize);
    reg("highlight_mesh", crate::derived::highlight);
    reg("capsule_shape", crate::derived::capsule);
    let rest = || Attributes { hold: mc_eval::Hold::Rest, ..Attributes::default() };
    k.register(h.sym("mesh_region"), rest(), Box::new(Fun(|cx, a| crate::mesh::construct(cx, "mesh_region", a))));
    k.register(h.sym("boundary_mesh_region"), rest(), Box::new(Fun(|cx, a| crate::mesh::construct(cx, "boundary_mesh_region", a))));
}

/// Is the unevaluated `region_union` or `region_difference` bounded (`None`: not such a
/// term, or not decided)? Each part is a region or again such a term.
fn bounded_set_operation<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<bool>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(t) else { return Ok(None) };
    let View::Sym(s) = h.view(head) else { return Ok(None) };
    let name = h.sym_name(s);
    if !matches!(name.as_str(), "region_union" | "region_difference" | "region_intersection") || args.is_empty() {
        return Ok(None);
    }
    let part = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>| -> Res<Option<bool>> {
        match read(cx, u) {
            Some(r) => props::bounded(cx, &r),
            None => bounded_set_operation(cx, u),
        }
    };
    match name.as_str() {
        "region_difference" => part(cx, args[0]),
        "region_union" => {
            let mut all = true;
            for &u in args {
                match part(cx, u)? {
                    Some(true) => {}
                    Some(false) => all = false,
                    None => return Ok(None),
                }
            }
            Ok(Some(all))
        }
        // an unevaluated intersection is not decided
        _ => Ok(None),
    }
}

fn one_region<'h>(cx: &NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Option<Region<'h>> {
    match a {
        [r] => read(cx, *r),
        _ => None,
    }
}

/// The value of a built term: evaluated, and made numeric when the region or point was.
fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, numeric: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let v = cx.eval(t)?;
    Ok(Reply::Done(if numeric { cx.eval(h.call(h.sym("n"), &[v]))? } else { v }))
}

fn region_member<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [r, p] = a else { return Ok(Reply::Decline) };
    // the intersection of two disks kept whole: in both
    if let Some(("region_intersection", parts)) = call_parts(h, *r).as_ref().map(|(n, x)| (n.as_str(), *x)) {
        if two_disks(cx, parts)?.is_some() {
            let both = |q: T<'h>| {
                let m: Vec<T<'h>> = parts.iter().map(|&d| h.call(h.sym("region_member"), &[d, q])).collect();
                h.call(h.sym("and"), &m)
            };
            let t = if is_vector(cx, *p) {
                both(*p)
            } else {
                let Some(ps) = cx.as_list(*p).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
                cx.list(&ps.iter().map(|&q| both(q)).collect::<Vec<_>>())
            };
            return Ok(Reply::Done(cx.eval(t)?));
        }
    }
    let Some(reg) = read(cx, *r) else { return Ok(Reply::Decline) };
    // several points
    if !is_vector(cx, *p) {
        let Some(ps) = cx.as_list(*p) else { return Ok(Reply::Decline) };
        let mut out = Vec::with_capacity(ps.len());
        for &q in ps {
            let Some(c) = cx.as_list(q).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
            match metric::member(cx, &reg, &c)? {
                Some(t) => out.push(cx.eval(t)?),
                None => return Ok(Reply::Decline),
            }
        }
        return Ok(Reply::Done(cx.list(&out)));
    }
    let c = cx.as_list(*p).map(|l| l.to_vec()).unwrap_or_default();
    let symbolic = crate::ex::nums(cx, &c)?.is_none();
    Ok(match metric::member(cx, &reg, &c)? {
        Some(t) => {
            let v = cx.eval(t)?;
            if !symbolic || matches!(h.view(v), View::Sym(_)) {
                // a numeric point gives true or false
                return Ok(Reply::Done(v));
            }
            // a symbolic point: the coordinates are real, and the condition holds
            let e = E { h };
            let vars = if c.len() == 1 { c[0] } else { e.f("alternatives", &c) };
            let mut parts = vec![e.f("element", &[vars, e.sym("reals")])];
            match call_parts(h, v) {
                Some((n, a)) if n == "and" => parts.extend(a.iter().copied()),
                _ => parts.push(v),
            }
            Reply::Done(e.f("and", &parts))
        }
        None => Reply::Decline,
    })
}

fn measure_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], want: Option<usize>) -> Res<Reply<'h>> {
    let h = cx.heap();
    // the intersection of two exact disks: the lens
    if let ([t], None | Some(2)) = (a, want) {
        if let Some((n, args)) = call_parts(h, *t) {
            if n == "region_intersection" {
                if let Some(d) = disks(cx, args, true)? {
                    if let Some(v) = lens_area(cx, &d)? {
                        return Ok(Reply::Done(v));
                    }
                }
            }
        }
    }
    // an annulus with angles `annulus(c, [r0, r1], [t0, t1])`: the area of the sector, (t1 - t0) (r1^2 - r0^2) / 2
    if let ([t], Some(2)) = (a, want) {
        if let Some((n, &[_, rr, ang])) = call_parts(h, *t).filter(|(n, _)| n == "annulus") {
            let _ = n;
            if let (Some(&[r0, r1]), Some(&[t0, t1])) = (cx.as_list(rr), cx.as_list(ang)) {
                let e = E { h };
                let v = e.mul(&[e.rat(1, 2), e.sub(t1, t0), e.sub(e.sq(r1), e.sq(r0))]);
                let v = cx.eval(e.f("simplify", &[v]))?;
                return Ok(Reply::Done(v));
            }
        }
    }
    // a map with its ranges (and maybe a coordinate system): the parametric region of the map
    if a.len() >= 2 {
        let name = match want {
            Some(1) => "arc_length",
            Some(2) => "area",
            Some(3) => "volume",
            _ => "",
        };
        if !name.is_empty() {
            if let Some(r) = crate::coords::reply(cx, name, a)? {
                return Ok(r);
            }
        }
    }
    if let [t] = a {
        if matches!(want, None | Some(3)) {
            if let Some(v) = crate::derived::capsule_volume(cx, *t) {
                return finish(cx, v, inexact(h, *t));
            }
        }
    }
    let Some(r) = one_region(cx, a) else { return Ok(Reply::Decline) };
    if let Some(d) = want {
        // area of a curve is 0, length of a solid infinite: no convention is fixed; decline
        match props::dimension(cx, &r)? {
            Some(k) if k == d => {}
            Some(k) if k < d => return Ok(Reply::Done(h.int(0))),
            Some(_) => return Ok(Reply::Done(h.sym_term(h.sym("undefined")))),
            None => return Ok(Reply::Decline),
        }
    }
    let numeric = inexact(h, a[0]);
    match props::measure(cx, &r)? {
        Some(t) => finish(cx, t, numeric),
        None => {
            let approx = implicit_measure(cx, &r)?;
            if !numeric {
                if let Some(t) = exact_implicit_measure(cx, &r, approx)? {
                    return Ok(Reply::Done(t));
                }
            }
            // the numeric volume of a solid given by exact inequalities is a check on the exact value, not an answer
            let solid_exact = !numeric && matches!(&r, Region::Implicit { vars, .. } if vars.len() == 3);
            Ok(match approx {
                Some(v) if !solid_exact => Reply::Done(h.real(v)),
                _ => Reply::Decline,
            })
        }
    }
}

/// The exact measure of an implicit region of at most three variables (round 19, Q19-28): the cylindrical
/// decomposition of its condition (mc-cad), each full-dimensional cell `l_1 < x_1 < u_1, l_2(x_1) < x_2 <
/// u_2(x_1), ...` integrated symbolically as a nested definite integral of 1, the cells summed. Accepted only
/// when every integral has a closed form and the sum agrees with the numeric measure `approx` (to 10^-4
/// relatively), so a wrong cell or integral can only fall back to the numeric value.
fn exact_implicit_measure<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, approx: Option<f64>) -> Res<Option<T<'h>>> {
    exact_implicit_integral(cx, r, None, approx)
}

/// The same cells with an integrand `f` (in the region's variables): the integral of `f`, the cells checked by the
/// measure (the integral of 1) against `approx`.
fn exact_implicit_integral<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, f: Option<T<'h>>, approx: Option<f64>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Region::Implicit { cond, vars } = r else { return Ok(None) };
    let Some(approx) = approx.filter(|v| v.is_finite()) else { return Ok(None) };
    if vars.is_empty() || vars.len() > 3 {
        return Ok(None);
    }
    let is_head = |t: T<'h>, n: &str| matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(n)));
    let cad = cx.eval(h.call(h.sym("cylindrical_decomposition"), &[*cond, cx.list(vars)]))?;
    if is_head(cad, "cylindrical_decomposition") || is_head(cad, "failure") {
        return Ok(None);
    }
    if cad == h.sym_term(h.sym("false")) {
        return Ok(Some(h.int(0)));
    }
    let Some(cells) = dnf(h, cad, 64) else { return Ok(None) };
    let mentions = |t: T<'h>, xs: &[T<'h>]| {
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if xs.contains(&u) {
                return true;
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        false
    };
    let mut parts = vec![];
    let mut weighted = vec![];
    'cells: for cell in cells {
        cx.tick(1)?;
        let mut lo: Vec<Option<T<'h>>> = vec![None; vars.len()];
        let mut hi: Vec<Option<T<'h>>> = vec![None; vars.len()];
        for atom in cell {
            if atom == h.sym_term(h.sym("true")) {
                continue;
            }
            let View::App { head, args: &[l, rr] } = h.view(atom) else { return Ok(None) };
            let View::Sym(rel) = h.view(head) else { return Ok(None) };
            let rel = h.sym_name(rel);
            let level = |v: T<'h>, other: T<'h>| vars.iter().position(|&x| x == v).filter(|&k| !mentions(other, &vars[k..]));
            let (k, bound, lower) = match (level(l, rr), level(rr, l)) {
                (Some(k), _) => (k, rr, matches!(rel.as_str(), "greater" | "greater_equal")),
                (None, Some(k)) => (k, l, matches!(rel.as_str(), "less" | "less_equal")),
                _ => return Ok(None),
            };
            match rel.as_str() {
                "equal" => continue 'cells,
                "unequal" => continue,
                "less" | "less_equal" | "greater" | "greater_equal" => {}
                _ => return Ok(None),
            }
            let slot = if lower { &mut lo[k] } else { &mut hi[k] };
            if slot.is_some() {
                return Ok(None);
            }
            *slot = Some(bound);
        }
        // a bounded cell: every variable between two bounds
        let mut ranges = vec![];
        for k in 0..vars.len() {
            let (Some(a), Some(b)) = (lo[k], hi[k]) else { return Ok(None) };
            ranges.push(cx.list(&[vars[k], a, b]));
        }
        let mut args = vec![h.int(1)];
        args.extend(ranges);
        let v = cx.eval(h.call(h.sym("integrate"), &args))?;
        if is_head(v, "integrate") || is_head(v, "failure") || mentions(v, vars) {
            return Ok(None);
        }
        parts.push(v);
        if let Some(f) = f {
            let mut args = vec![f];
            args.extend(ranges_of_cell(cx, vars, &lo, &hi));
            let w = cx.eval(h.call(h.sym("integrate"), &args))?;
            if is_head(w, "integrate") || is_head(w, "failure") || mentions(w, vars) {
                return Ok(None);
            }
            weighted.push(w);
        }
    }
    let total = cx.eval(h.call(h.sym("simplify"), &[h.call(h.sym("add"), &parts)]))?;
    let nv = cx.eval(h.call(h.sym("n"), &[total]))?;
    let tol = if vars.len() == 3 { 6e-3 } else { 1e-4 };
    let agrees = match h.view(nv) {
        View::Real(v) => (v - approx).abs() <= tol * (1.0 + approx.abs()),
        View::Int(v) => (v as f64 - approx).abs() <= tol * (1.0 + approx.abs()),
        _ => false,
    };
    if !agrees {
        return Ok(None);
    }
    if f.is_some() {
        return Ok(Some(cx.eval(h.call(h.sym("simplify"), &[h.call(h.sym("add"), &weighted)]))?));
    }
    Ok(Some(total))
}

/// The ranges `[x_k, lo_k, hi_k]` of a bounded cell.
fn ranges_of_cell<'h>(cx: &mut NativeCx<'_, '_, 'h>, vars: &[T<'h>], lo: &[Option<T<'h>>], hi: &[Option<T<'h>>]) -> Vec<T<'h>> {
    (0..vars.len()).filter_map(|k| Some(cx.list(&[vars[k], lo[k]?, hi[k]?]))).collect()
}

/// The disjunctive normal form of a formula of `and`, `or` and atoms; `None` past `cap` conjunctions.
fn dnf<'h>(h: &'h Heap, t: T<'h>, cap: usize) -> Option<Vec<Vec<T<'h>>>> {
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(h.sym("or")) => {
            let mut out = vec![];
            for &a in args {
                out.extend(dnf(h, a, cap)?);
                if out.len() > cap {
                    return None;
                }
            }
            Some(out)
        }
        View::App { head, args } if head == h.sym_term(h.sym("and")) => {
            let mut acc: Vec<Vec<T<'h>>> = vec![vec![]];
            for &a in args {
                let d = dnf(h, a, cap)?;
                let mut next = Vec::with_capacity(acc.len() * d.len());
                for c in &acc {
                    for e in &d {
                        let mut x = c.clone();
                        x.extend_from_slice(e);
                        next.push(x);
                    }
                }
                if next.len() > cap {
                    return None;
                }
                acc = next;
            }
            Some(acc)
        }
        _ => Some(vec![vec![t]]),
    }
}

fn region_centroid<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(r) = one_region(cx, a) else { return Ok(Reply::Decline) };
    let numeric = inexact(h, a[0]);
    match props::centroid(cx, &r)? {
        Some(c) => {
            let t = cx.list(&c);
            // a simplified form: together of each coordinate
            finish(cx, t, numeric)
        }
        None => Ok(Reply::Decline),
    }
}

fn region_bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(r) = (match a {
        [r] | [r, _] => read(cx, *r),
        _ => None,
    }) else {
        return Ok(Reply::Decline);
    };
    let numeric = inexact(h, a[0]);
    match props::bounds(cx, &r)? {
        Some(b) => {
            let rows: Vec<T<'h>> = b.iter().map(|&(lo, hi)| cx.list(&[lo, hi])).collect();
            let t = cx.list(&rows);
            finish(cx, t, numeric)
        }
        None => Ok(match numeric_bounds(cx, &r)? {
            Some(b) => Reply::Done(cx.list(&b.iter().map(|&(lo, hi)| h.reals(&[lo, hi])).collect::<Vec<_>>())),
            None => Reply::Decline,
        }),
    }
}

fn distance<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], q: Query) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [r, p] = a else { return Ok(Reply::Decline) };
    let Some(reg) = read(cx, *r) else { return Ok(Reply::Decline) };
    let numeric = inexact(h, *r) || inexact(h, *p);
    let one = |cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>| -> Res<Option<T<'h>>> {
        let Some(c) = cx.as_list(p).map(|l| l.to_vec()) else { return Ok(None) };
        let Some(t) = metric::query(cx, &reg, &c, q)? else {
            // an implicit region: the least squared distance to the points that satisfy the condition
            if let (Region::Implicit { cond, vars }, Query::Nearest | Query::Distance) = (&reg, q) {
                if vars.len() == c.len() {
                    let e = E { h };
                    let sq: Vec<T<'h>> = vars.iter().zip(&c).map(|(&x, &y)| e.sq(e.sub(x, y))).collect();
                    let m = cx.eval(e.f("minimize", &[e.list(&[e.add(&sq), *cond]), e.list(vars)]))?;
                    let Some(&[value, rules]) = cx.as_list(m) else { return Ok(None) };
                    if call_parts(h, value).is_some_and(|(n, _)| n == "minimize") {
                        return Ok(None);
                    }
                    let t = if q == Query::Distance {
                        e.sqrt(value)
                    } else {
                        let Some(rs) = cx.as_list(rules).map(|l| l.to_vec()) else { return Ok(None) };
                        let mut pt = Vec::with_capacity(vars.len());
                        for &x in vars {
                            let Some(r) = rs.iter().find_map(|&r| match call_parts(h, r) {
                                Some((n, &[l, v])) if n == "rule" && l == x => Some(v),
                                _ => None,
                            }) else {
                                return Ok(None);
                            };
                            pt.push(r);
                        }
                        e.list(&pt)
                    };
                    let v = cx.eval(t)?;
                    return Ok(Some(if numeric { cx.eval(h.call(h.sym("n"), &[v]))? } else { v }));
                }
            }
            return Ok(None);
        };
        let v = cx.eval(t)?;
        Ok(Some(if numeric { cx.eval(h.call(h.sym("n"), &[v]))? } else { v }))
    };
    // a 1D region takes a number for its point
    let p = if reg.embedding() == 1 && cx.as_list(*p).is_none() { cx.list(&[*p]) } else { *p };
    if !is_vector(cx, p) {
        if let Some(ps) = cx.as_list(p) {
            let mut out = vec![];
            for &x in ps {
                match one(cx, x)? {
                    Some(v) => out.push(v),
                    None => return Ok(Reply::Decline),
                }
            }
            return Ok(Reply::Done(cx.list(&out)));
        }
    }
    Ok(match one(cx, p)? {
        Some(v) => Reply::Done(v),
        None => Reply::Decline,
    })
}

// ---- numeric bounds and measures of implicit and parametric regions

/// A compiled real function of the coordinates.
fn compiled<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, vars: &[Sym]) -> Res<Box<dyn mc_numfn::NumFn<'h, f64> + 'h>> {
    let vs: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
    mc_numfn::compile_expr(cx, f, &vs, &Options::default())
}

/// The value function whose sign tells the region of an implicit region: `(f, curve)`, where `curve` means the
/// region is where f changes sign (an equation), else where f >= 0.
fn sign_function<'h>(h: &'h Heap, cond: T<'h>) -> Option<(T<'h>, bool)> {
    let e = E { h };
    if let Some((n, a)) = call_parts(h, cond) {
        if n == "equal" && a.len() == 2 {
            return Some((e.sub(a[0], a[1]), true));
        }
    }
    crate::margin(h, cond).map(|m| (m, false))
}

fn syms<'h>(h: &'h Heap, vars: &[T<'h>]) -> Option<Vec<Sym>> {
    vars.iter()
        .map(|&v| match h.view(v) {
            View::Sym(s) => Some(s),
            _ => None,
        })
        .collect()
}

/// The bounds of an implicit region (sampled on growing boxes, the extreme samples refined by bisection) or of a
/// parametric one (sampled over its parameters).
fn numeric_bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<Vec<(f64, f64)>>> {
    let h = cx.heap();
    match r {
        Region::Implicit { cond, vars } if (1..=3).contains(&vars.len()) => {
            let (Some((f, curve)), Some(vs)) = (sign_function(h, *cond), syms(h, vars)) else { return Ok(None) };
            let mut fun = compiled(cx, f, &vs)?;
            let n = vars.len();
            let m: usize = [0, 2001, 201, 41][n];
            let mut out = [0.0];
            for radius in [1.0, 4.0, 16.0, 100.0, 1000.0] {
                let step = 2.0 * radius / (m - 1) as f64;
                let coord = |i: usize| -radius + step * i as f64;
                let total = m.pow(n as u32);
                let mut vals = vec![f64::NAN; total];
                let idx = |k: usize| -> Vec<usize> { (0..n).map(|d| (k / m.pow(d as u32)) % m).collect() };
                for (k, v) in vals.iter_mut().enumerate() {
                    if k % 4096 == 0 {
                        cx.tick(1)?;
                    }
                    let p: Vec<f64> = idx(k).into_iter().map(coord).collect();
                    if fun.eval(cx, &p, &mut out)? && out[0].is_finite() {
                        *v = out[0];
                    }
                }
                // the samples in the region (or next to a sign change) give the box
                let mut lo = vec![f64::INFINITY; n];
                let mut hi = vec![f64::NEG_INFINITY; n];
                let mut edge = false;
                for k in 0..total {
                    let v = vals[k];
                    if !v.is_finite() {
                        continue;
                    }
                    let ix = idx(k);
                    let mut hits: Vec<Vec<f64>> = vec![];
                    if curve {
                        for d in 0..n {
                            if ix[d] + 1 < m {
                                let k2 = k + m.pow(d as u32);
                                let w = vals[k2];
                                if w.is_finite() && (v == 0.0 || v.signum() != w.signum()) {
                                    // the crossing by bisection
                                    let (mut a, mut b) = (0.0f64, 1.0f64);
                                    let base: Vec<f64> = ix.iter().map(|&i| coord(i)).collect();
                                    for _ in 0..50 {
                                        let t = 0.5 * (a + b);
                                        let mut q = base.clone();
                                        q[d] += step * t;
                                        let ok = fun.eval(cx, &q, &mut out)?;
                                        if ok && out[0].signum() == v.signum() && out[0] != 0.0 {
                                            a = t;
                                        } else {
                                            b = t;
                                        }
                                    }
                                    let mut q = base.clone();
                                    q[d] += step * 0.5 * (a + b);
                                    hits.push(q);
                                }
                            }
                        }
                    } else if v >= 0.0 {
                        hits.push(ix.iter().map(|&i| coord(i)).collect());
                        // refine outward along each axis where the neighbour is outside
                        for d in 0..n {
                            for dir in [-1i64, 1] {
                                let j = ix[d] as i64 + dir;
                                if j < 0 || j >= m as i64 {
                                    edge = true;
                                    continue;
                                }
                                let k2 = (k as i64 + dir * m.pow(d as u32) as i64) as usize;
                                if vals[k2].is_finite() && vals[k2] >= 0.0 {
                                    continue;
                                }
                                let base: Vec<f64> = ix.iter().map(|&i| coord(i)).collect();
                                let (mut a, mut b) = (0.0f64, 1.0f64);
                                for _ in 0..50 {
                                    let t = 0.5 * (a + b);
                                    let mut q = base.clone();
                                    q[d] += dir as f64 * step * t;
                                    let ok = fun.eval(cx, &q, &mut out)?;
                                    if ok && out[0] >= 0.0 {
                                        a = t;
                                    } else {
                                        b = t;
                                    }
                                }
                                let mut q = base.clone();
                                q[d] += dir as f64 * step * a;
                                hits.push(q);
                            }
                        }
                    }
                    for q in hits {
                        for d in 0..n {
                            lo[d] = lo[d].min(q[d]);
                            hi[d] = hi[d].max(q[d]);
                        }
                        if q.iter().any(|x| x.abs() >= radius - step) {
                            edge = true;
                        }
                    }
                }
                if lo[0].is_finite() && !edge {
                    return Ok(Some(lo.into_iter().zip(hi).collect()));
                }
                if lo[0].is_finite() && radius >= 1000.0 {
                    // unbounded
                    return Ok(Some(lo.iter().zip(&hi).map(|(&a, &b)| (if a <= -radius + step { f64::NEG_INFINITY } else { a }, if b >= radius - step { f64::INFINITY } else { b })).collect()));
                }
            }
            Ok(None)
        }
        Region::Parametric { fs, specs } if (1..=3).contains(&specs.len()) => {
            let us: Vec<T<'h>> = specs.iter().map(|s| s.0).collect();
            let Some(vs) = syms(h, &us) else { return Ok(None) };
            let mut ranges = vec![];
            for &(_, a, b) in specs {
                let (Some(a), Some(b)) = (num(cx, a)?, num(cx, b)?) else { return Ok(None) };
                ranges.push((a, b));
            }
            let n = specs.len();
            let m: usize = [0, 4001, 301, 61][n];
            let mut funs = vec![];
            for &f in fs {
                funs.push(compiled(cx, f, &vs)?);
            }
            let mut lo = vec![f64::INFINITY; fs.len()];
            let mut hi = vec![f64::NEG_INFINITY; fs.len()];
            let mut out = [0.0];
            for k in 0..m.pow(n as u32) {
                if k % 4096 == 0 {
                    cx.tick(1)?;
                }
                let p: Vec<f64> = (0..n)
                    .map(|d| {
                        let i = (k / m.pow(d as u32)) % m;
                        ranges[d].0 + (ranges[d].1 - ranges[d].0) * i as f64 / (m - 1) as f64
                    })
                    .collect();
                for (c, fun) in funs.iter_mut().enumerate() {
                    if fun.eval(cx, &p, &mut out)? && out[0].is_finite() {
                        lo[c] = lo[c].min(out[0]);
                        hi[c] = hi[c].max(out[0]);
                    }
                }
            }
            Ok(lo.iter().all(|x| x.is_finite()).then(|| lo.into_iter().zip(hi).collect()))
        }
        _ => Ok(None),
    }
}

/// The measure of an implicit region of one or two dimensions given by inequalities: the lengths of the
/// intervals where it holds (by sampling and bisection), and in the plane those lengths integrated over x
/// (Gauss-Legendre on many panels).
fn implicit_measure<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let Region::Implicit { cond, vars } = r else { return Ok(None) };
    let (Some((f, false)), Some(vs)) = (sign_function(h, *cond), syms(h, vars)) else { return Ok(None) };
    if vars.len() > 3 {
        return Ok(None);
    }
    let Some(b) = numeric_bounds(cx, r)? else { return Ok(None) };
    if b.iter().any(|x| !x.0.is_finite() || !x.1.is_finite()) {
        return Ok(Some(f64::INFINITY));
    }
    let mut fun = compiled(cx, f, &vs)?;
    let mut out = [0.0];
    // the length of the set {t in [a, b] : g(t) >= 0}
    let slice = |cx: &mut NativeCx<'_, '_, 'h>, g: &mut dyn FnMut(&mut NativeCx<'_, '_, 'h>, f64) -> Res<f64>, a: f64, b: f64, m: usize| -> Res<f64> {
        let pad = (b - a) * 1e-3 + 1e-12;
        let (a, b) = (a - pad, b + pad);
        let step = (b - a) / m as f64;
        let mut len = 0.0;
        let mut prev = g(cx, a)?;
        for i in 1..=m {
            let t1 = a + step * i as f64;
            let cur = g(cx, t1)?;
            let (t0, p, c) = (t1 - step, prev >= 0.0, cur >= 0.0);
            if p && c {
                len += step;
            } else if p != c {
                let (mut lo, mut hi) = (t0, t1);
                for _ in 0..45 {
                    let mid = 0.5 * (lo + hi);
                    if (g(cx, mid)? >= 0.0) == p {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                len += if p { lo - t0 } else { t1 - lo };
            }
            prev = cur;
        }
        Ok(len)
    };
    if vars.len() == 1 {
        let mut g = |cx: &mut NativeCx<'_, '_, 'h>, t: f64| -> Res<f64> { Ok(if fun.eval(cx, &[t], &mut out)? { out[0] } else { -1.0 }) };
        return Ok(Some(slice(cx, &mut g, b[0].0, b[0].1, 20000)?));
    }
    if vars.len() == 3 {
        // the volume: Gauss-Legendre on panels across x and y, the length of the set along z bisected (good to
        // about a part in a thousand)
        const X3: [f64; 5] = [-0.906_179_845_938_664, -0.538_469_310_105_683, 0.0, 0.538_469_310_105_683, 0.906_179_845_938_664];
        const W3: [f64; 5] = [0.236_926_885_056_189, 0.478_628_670_499_366, 0.568_888_888_888_889, 0.478_628_670_499_366, 0.236_926_885_056_189];
        let panels = 40;
        let ((x0, x1), (y0, y1)) = (b[0], b[1]);
        let (wx, wy) = ((x1 - x0) / panels as f64, (y1 - y0) / panels as f64);
        let mut total = 0.0;
        for px in 0..panels {
            cx.tick(1)?;
            for kx in 0..5 {
                let x = x0 + wx * (px as f64 + 0.5 + 0.5 * X3[kx]);
                let mut area = 0.0;
                for py in 0..panels {
                    for ky in 0..5 {
                        let y = y0 + wy * (py as f64 + 0.5 + 0.5 * X3[ky]);
                        let mut g = |cx: &mut NativeCx<'_, '_, 'h>, z: f64| -> Res<f64> { Ok(if fun.eval(cx, &[x, y, z], &mut out)? { out[0] } else { -1.0 }) };
                        area += 0.5 * wy * W3[ky] * slice(cx, &mut g, b[2].0, b[2].1, 60)?;
                    }
                }
                total += 0.5 * wx * W3[kx] * area;
            }
        }
        return Ok(Some(total));
    }
    // Gauss-Legendre, 5 points, on panels across x
    const X: [f64; 5] = [-0.906_179_845_938_664, -0.538_469_310_105_683, 0.0, 0.538_469_310_105_683, 0.906_179_845_938_664];
    const W: [f64; 5] = [0.236_926_885_056_189, 0.478_628_670_499_366, 0.568_888_888_888_889, 0.478_628_670_499_366, 0.236_926_885_056_189];
    let panels = 400;
    let (x0, x1) = b[0];
    let w = (x1 - x0) / panels as f64;
    let mut total = 0.0;
    for p in 0..panels {
        cx.tick(1)?;
        for k in 0..5 {
            let x = x0 + w * (p as f64 + 0.5 + 0.5 * X[k]);
            let mut g = |cx: &mut NativeCx<'_, '_, 'h>, y: f64| -> Res<f64> { Ok(if fun.eval(cx, &[x, y], &mut out)? { out[0] } else { -1.0 }) };
            total += 0.5 * w * W[k] * slice(cx, &mut g, b[1].0, b[1].1, 200)?;
        }
    }
    Ok(Some(total))
}

/// `area(region_intersection(disk(c1, r1), disk(c2, r2)))` and `region_moment(that, [0, 0])` (`n` arguments): the
/// lens area of [`lens_area`] (exact data give the exact value, machine data a machine value; round 36, one
/// implementation for both); `None` for anything else.
fn lens<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], n: usize) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    if a.len() != n {
        return Ok(None);
    }
    if n == 2 {
        let zero = h.int(0);
        if cx.as_list(a[1]) != Some(&[zero, zero][..]) {
            return Ok(None);
        }
    }
    let View::App { head, args } = h.view(a[0]) else { return Ok(None) };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "region_intersection") {
        return Ok(None);
    }
    let Some(d) = disks(cx, args, true)? else { return Ok(None) };
    Ok(lens_area(cx, &d)?.map(Reply::Done))
}

// ---- Boolean operations on plane polygons

/// The rings of a plane region as machine numbers, with whether its coordinates were exact.
pub(crate) fn plane_rings<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Vec<Vec<P>>, bool)>> {
    let h = cx.heap();
    // a Boolean region left unevaluated (of disks): its pieces as polygons, when they have no holes
    if let View::App { head, args } = h.view(t) {
        let op = match h.view(head) {
            View::Sym(s) => match h.sym_name(s).as_str() {
                "region_union" => Some(Op::Union),
                "region_intersection" => Some(Op::Intersection),
                "region_difference" if args.len() == 2 => Some(Op::Difference),
                _ => None,
            },
            _ => None,
        };
        if let (Some(op), false) = (op, args.is_empty()) {
            let mut acc: Option<Vec<Vec<P>>> = None;
            for &x in args {
                let Some((rings, _)) = plane_rings(cx, x)? else { return Ok(None) };
                acc = Some(match acc {
                    None => rings,
                    Some(prev) => {
                        let shapes = clip::boolean(&prev, &rings, op);
                        if shapes.iter().any(|s| !s.1.is_empty()) {
                            return Ok(None);
                        }
                        shapes.into_iter().map(|s| s.0).collect()
                    }
                });
            }
            return Ok(acc.map(|r| (r, false)));
        }
    }
    let Some(r) = read(cx, t) else { return Ok(None) };
    if r.embedding() != 2 {
        return Ok(None);
    }
    let exact = !inexact(h, t);
    let to = |v: Vec<Vec<f64>>| v.into_iter().map(|p| [p[0], p[1]]).collect::<Vec<P>>();
    Ok(Some(match &r {
        Region::Polygon(rings) => {
            let mut out = vec![];
            for ring in rings {
                for pts in std::iter::once(&ring.outer).chain(&ring.holes) {
                    let Some(v) = points(cx, pts)? else { return Ok(None) };
                    out.push(to(v));
                }
            }
            (out, exact)
        }
        Region::Simplex(v) if v.len() == 3 => {
            let Some(v) = points(cx, v)? else { return Ok(None) };
            (vec![to(v)], exact)
        }
        Region::Boxed { lo, hi } => {
            let (Some(a), Some(b)) = (nums(cx, lo)?, nums(cx, hi)?) else { return Ok(None) };
            (vec![vec![[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]]], exact)
        }
        Region::Ball { c, r: rad, solid: true } => {
            let Some(c) = nums(cx, c)? else { return Ok(None) };
            let (rx, ry) = match rad {
                Radius::One(r) => match num(cx, *r)? {
                    Some(r) => (r, r),
                    None => return Ok(None),
                },
                Radius::Axes(rs) => match (num(cx, rs[0])?, num(cx, rs[1])?) {
                    (Some(a), Some(b)) => (a, b),
                    _ => return Ok(None),
                },
            };
            let n = 256;
            let ring = (0..n)
                .map(|k| {
                    let a = std::f64::consts::TAU * k as f64 / n as f64;
                    [c[0] + rx * a.cos(), c[1] + ry * a.sin()]
                })
                .collect();
            (vec![ring], false)
        }
        _ => return Ok(None),
    }))
}

/// A machine number as a term: the small-denominator rational it is (within rounding) when the input was exact.
pub(crate) fn coordinate<'h>(h: &'h Heap, v: f64, exact: bool) -> T<'h> {
    let e = E { h };
    if exact {
        // continued fraction
        let (mut p0, mut q0, mut p1, mut q1) = (0i64, 1i64, 1i64, 0i64);
        let mut x = v;
        for _ in 0..20 {
            let a = x.floor();
            if a.abs() > 1e12 {
                break;
            }
            let a = a as i64;
            let (p2, q2) = (a.saturating_mul(p1).saturating_add(p0), a.saturating_mul(q1).saturating_add(q0));
            (p0, q0, p1, q1) = (p1, q1, p2, q2);
            if q1 > 100_000 {
                break;
            }
            if (v - p1 as f64 / q1 as f64).abs() <= 1e-11 * v.abs().max(1.0) {
                return e.rat(p1, q1);
            }
            let frac = x - a as f64;
            if frac.abs() < 1e-15 {
                break;
            }
            x = 1.0 / frac;
        }
    }
    h.real(v)
}

pub(crate) fn ring_term<'h>(cx: &NativeCx<'_, '_, 'h>, r: &[P], exact: bool) -> T<'h> {
    let h = cx.heap();
    let pts: Vec<T<'h>> = r.iter().map(|p| cx.list(&[coordinate(h, p[0], exact), coordinate(h, p[1], exact)])).collect();
    cx.list(&pts)
}

/// The centres and radii of two exact solid disks of the plane (`disk(c, r)` with numeric exact data), the
/// arguments of `region_intersection(d1, d2)`.
type Disk<'h> = (Vec<T<'h>>, T<'h>);
pub(crate) fn two_disks<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Option<[Disk<'h>; 2]>> {
    disks(cx, a, false)
}

/// The two solid plane disks of `a` with numeric data and positive radii (machine data too when `inexact`).
fn disks<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], inexact_ok: bool) -> Res<Option<[Disk<'h>; 2]>> {
    let h = cx.heap();
    let [p, q] = a else { return Ok(None) };
    let mut out = vec![];
    for &t in [p, q] {
        if !inexact_ok && inexact(h, t) {
            return Ok(None);
        }
        let Some(Region::Ball { c, r: Radius::One(r), solid: true }) = read(cx, t) else { return Ok(None) };
        if c.len() != 2 || nums(cx, &c)?.is_none() || !num(cx, r)?.is_some_and(|v| v > 0.0) {
            return Ok(None);
        }
        out.push((c, r));
    }
    let b = out.pop().expect("two");
    let a = out.pop().expect("two");
    Ok(Some([a, b]))
}

/// The exact area of the intersection of two disks: 0 apart, the smaller disk when one lies in the other, else
/// the lens r1^2 acos((d^2 + r1^2 - r2^2)/(2 d r1)) + r2^2 acos((d^2 + r2^2 - r1^2)/(2 d r2))
/// - sqrt((-d + r1 + r2)(d + r1 - r2)(d - r1 + r2)(d + r1 + r2))/2 (the two circular segments).
fn lens_area<'h>(cx: &mut NativeCx<'_, '_, 'h>, d: &[Disk<'h>; 2]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let [(c1, r1), (c2, r2)] = d;
    let (r1, r2) = (*r1, *r2);
    let dd = e.dist(c1, c2);
    let (Some(dv), Some(a), Some(b)) = (num(cx, dd)?, num(cx, r1)?, num(cx, r2)?) else { return Ok(None) };
    let v = if dv >= a + b {
        h.int(0)
    } else if dv <= (a - b).abs() {
        let r = if a <= b { r1 } else { r2 };
        e.mul(&[e.pi(), e.sq(r)])
    } else {
        let seg = |ra: T<'h>, rb: T<'h>| {
            let arg = e.div(e.sub(e.add(&[e.sq(dd), e.sq(ra)]), e.sq(rb)), e.mul(&[e.int(2), dd, ra]));
            e.mul(&[e.sq(ra), e.f("acos", &[arg])])
        };
        let k = e.f("expand", &[e.mul(&[
            e.add(&[e.neg(dd), r1, r2]),
            e.add(&[dd, r1, e.neg(r2)]),
            e.add(&[dd, e.neg(r1), r2]),
            e.add(&[dd, r1, r2]),
        ])]);
        e.sub(e.add(&[seg(r1, r2), seg(r2, r1)]), e.mul(&[e.rat(1, 2), e.sqrt(k)]))
    };
    Ok(Some(cx.eval(e.f("simplify", &[v]))?))
}

fn boolean<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], op: Op) -> Res<Reply<'h>> {
    if a.is_empty() || (op == Op::Difference && a.len() != 2) {
        return Ok(Reply::Decline);
    }
    // two exact disks: kept as the intersection (its area is the exact lens, `measure_of`; `discretize_region` takes
    // the polygon), not replaced by the intersection of two 256-gons, whose area is off by 2e-4
    if op == Op::Intersection && two_disks(cx, a)?.is_some() {
        return Ok(Reply::Decline);
    }
    boolean_polygons(cx, a, op)
}

/// The Boolean operation on the polygons of the regions (curved ones as 256-gons).
pub(crate) fn boolean_polygons<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], op: Op) -> Res<Reply<'h>> {
    let mut parts = vec![];
    let mut exact = true;
    // a disk (or ellipse) would enter as a 256-gon: the result would be a polygon only near the region, and its
    // measures off by 1e-4 (round 36); the Boolean region stays as it is (discretize_region meshes it)
    if a.iter().any(|&t| matches!(read(cx, t), Some(Region::Ball { .. }))) {
        return Ok(Reply::Decline);
    }
    for &t in a {
        let Some((rings, ex)) = plane_rings(cx, t)? else { return Ok(Reply::Decline) };
        exact &= ex;
        parts.push(rings);
    }
    let result = if parts.len() == 1 {
        clip::boolean(&parts[0], &[], Op::Union)
    } else {
        let mut acc: Vec<Vec<P>> = parts[0].clone();
        let mut shapes = vec![];
        for p in &parts[1..] {
            shapes = clip::boolean(&acc, p, op);
            acc = shapes.iter().flat_map(|(o, hs)| std::iter::once(o.clone()).chain(hs.iter().cloned())).collect();
        }
        shapes
    };
    Ok(Reply::Done(shapes_term(cx, &result, exact)))
}

/// The result of a Boolean operation as a term: `empty_region(2)`, a polygon (with its holes as a rule), several
/// pieces as a polygon of rings, or a list of polygons.
pub(crate) fn shapes_term<'h>(cx: &NativeCx<'_, '_, 'h>, result: &clip::Shapes, exact: bool) -> T<'h> {
    let h = cx.heap();
    let e = E { h };
    match result.len() {
        0 => e.f("empty_region", &[h.int(2)]),
        1 if result[0].1.is_empty() => e.f("polygon", &[ring_term(cx, &clip::canonical(result[0].0.clone()), exact)]),
        1 => {
            let holes: Vec<T<'h>> = result[0].1.iter().map(|r| ring_term(cx, &clip::canonical(r.clone()), exact)).collect();
            let outer = ring_term(cx, &clip::canonical(result[0].0.clone()), exact);
            e.f("polygon", &[e.f("rule", &[outer, cx.list(&holes)])])
        }
        _ if result.iter().all(|s| s.1.is_empty()) => {
            let rings: Vec<T<'h>> = result.iter().map(|s| ring_term(cx, &clip::canonical(s.0.clone()), exact)).collect();
            e.f("polygon", &[cx.list(&rings)])
        }
        _ => {
            let polys: Vec<T<'h>> = result
                .iter()
                .map(|(o, hs)| {
                    let outer = ring_term(cx, &clip::canonical(o.clone()), exact);
                    if hs.is_empty() {
                        e.f("polygon", &[outer])
                    } else {
                        let holes: Vec<T<'h>> = hs.iter().map(|r| ring_term(cx, &clip::canonical(r.clone()), exact)).collect();
                        e.f("polygon", &[e.f("rule", &[outer, cx.list(&holes)])])
                    }
                })
                .collect();
            cx.list(&polys)
        }
    }
}

// ---- integrals over regions

/// A fresh parameter symbol of the parametrisations.
fn param<'h>(h: &'h Heap, k: usize) -> T<'h> {
    h.sym_term(h.sym(&format!("region$u{k}")))
}

/// Ear clipping of a simple plane ring (machine numbers): triangles as vertex indices.
pub(crate) fn triangulate(r: &[P]) -> Vec<[usize; 3]> {
    let n = r.len();
    let mut idx: Vec<usize> = (0..n).collect();
    if clip::area(r) < 0.0 {
        idx.reverse();
    }
    let mut out = vec![];
    let cross = |o: P, a: P, b: P| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut guard = 0;
    while idx.len() > 3 && guard < 10 * n * n {
        guard += 1;
        let m = idx.len();
        let mut cut = None;
        for i in 0..m {
            let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            if cross(r[a], r[b], r[c]) <= 0.0 {
                continue;
            }
            let inside = idx.iter().any(|&k| {
                k != a && k != b && k != c && cross(r[a], r[b], r[k]) >= 0.0 && cross(r[b], r[c], r[k]) >= 0.0 && cross(r[c], r[a], r[k]) >= 0.0
            });
            if !inside {
                cut = Some(i);
                break;
            }
        }
        let i = cut.unwrap_or(0);
        let m = idx.len();
        out.push([idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]]);
        idx.remove(i);
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

/// The integral of `f` (in `vars`) over the simplex on the points `v`, as `vol * integrate(f(v0 + sum s_i e_i), ...)`
/// over the standard simplex of the parameters.
fn simplex_integral<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, vars: &[T<'h>], v: &[Vec<T<'h>>]) -> T<'h> {
    let h = cx.heap();
    let e = E { h };
    let k = v.len() - 1;
    if k == 0 {
        return subst(e, f, vars, &v[0]);
    }
    let edges: Vec<Vec<T<'h>>> = v[1..].iter().map(|p| e.vsub(p, &v[0])).collect();
    let s: Vec<T<'h>> = (0..k).map(|i| param(h, i)).collect();
    let mut x = v[0].clone();
    for (si, ed) in s.iter().zip(&edges) {
        x = e.vadd(&x, &e.scale(*si, ed));
    }
    // the Jacobian: |det| for a full simplex, sqrt(det(Gram)) otherwise
    let jac = if k == v[0].len() {
        e.f("abs", &[e.f("det", &[e.list(&edges.iter().map(|r| e.list(r)).collect::<Vec<_>>())])])
    } else {
        let g: Vec<T<'h>> = edges.iter().map(|a| e.list(&edges.iter().map(|b| e.dot(a, b)).collect::<Vec<_>>())).collect();
        e.sqrt(e.f("det", &[e.list(&g)]))
    };
    let mut args = vec![subst(e, f, vars, &x)];
    for i in 0..k {
        let upper = e.sub(e.int(1), e.add(&s[..i]));
        args.push(e.list(&[s[i], e.int(0), upper]));
    }
    e.mul(&[jac, e.f("integrate", &args)])
}

/// f with `vars` replaced by the coordinates `x`.
fn subst<'h>(e: E<'h>, f: T<'h>, vars: &[T<'h>], x: &[T<'h>]) -> T<'h> {
    let rules: Vec<T<'h>> = vars.iter().zip(x).map(|(&v, &c)| e.f("rule", &[v, c])).collect();
    e.f("rewrite", &[f, e.list(&rules)])
}

fn region_integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let (f, vars, rt) = match a {
        [f, vars, r] => (*f, *vars, *r),
        [f, el] => match call_parts(h, *el) {
            Some((n, &[vars, r])) if n == "element" => (*f, vars, r),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let vars: Vec<T<'h>> = match cx.as_list(vars) {
        Some(l) => l.to_vec(),
        None => vec![vars],
    };
    let Some(r) = read(cx, rt) else { return Ok(Reply::Decline) };
    if vars.len() != r.embedding() || syms(h, &vars).is_none() {
        return Ok(Reply::Decline);
    }
    let n = vars.len();
    let (t, p) = (param(h, 100), param(h, 101));
    let term = match &r {
        Region::Points(ps) => e.add(&ps.iter().map(|q| subst(e, f, &vars, q)).collect::<Vec<_>>()),
        Region::Lines(ls) => {
            let mut parts = vec![];
            for l in ls {
                for w in l.windows(2) {
                    parts.push(simplex_integral(cx, f, &vars, w));
                }
            }
            e.add(&parts)
        }
        Region::Simplex(v) => simplex_integral(cx, f, &vars, v),
        Region::Polygon(rings) => {
            let mut parts = vec![];
            for ring in rings {
                for (pts, sign) in std::iter::once((&ring.outer, 1)).chain(ring.holes.iter().map(|x| (x, -1))) {
                    let tris: Vec<[usize; 3]> = if pts[0].len() == 2 {
                        let Some(nv) = points(cx, pts)? else { return Ok(Reply::Decline) };
                        triangulate(&nv.iter().map(|q| [q[0], q[1]]).collect::<Vec<P>>())
                    } else {
                        (1..pts.len() - 1).map(|i| [0, i, i + 1]).collect()
                    };
                    for tri in tris {
                        let v: Vec<Vec<T<'h>>> = tri.iter().map(|&i| pts[i].clone()).collect();
                        let piece = simplex_integral(cx, f, &vars, &v);
                        parts.push(if sign < 0 { e.neg(piece) } else { piece });
                    }
                }
            }
            e.add(&parts)
        }
        Region::Boxed { lo, hi } => {
            let mut args = vec![f];
            for k in 0..n {
                args.push(e.list(&[vars[k], lo[k], hi[k]]));
            }
            e.f("integrate", &args)
        }
        Region::Intervals(iv) => e.add(&iv.iter().map(|&(lo, hi)| e.f("integrate", &[f, e.list(&[vars[0], lo, hi])])).collect::<Vec<_>>()),
        Region::Ball { c, r: rad, solid } => {
            // per-axis radii: the unit ball stretched; Jacobian the product of the radii
            let (rs, scale): (Vec<T<'h>>, T<'h>) = match rad {
                Radius::One(r) => (vec![*r; n], e.int(1)),
                Radius::Axes(v) => (v.clone(), e.mul(v)),
            };
            if matches!(rad, Radius::Axes(_)) && !*solid {
                return Ok(Reply::Decline);
            }
            let one = |k: usize| match rad {
                Radius::One(_) => e.int(1),
                Radius::Axes(_) => rs[k],
            };
            let rr = match rad {
                Radius::One(r) => *r,
                Radius::Axes(_) => e.int(1),
            };
            let (sin, cos) = (|x: T<'h>| e.f("sin", &[x]), |x: T<'h>| e.f("cos", &[x]));
            match (n, solid) {
                (2, true) => {
                    let x = vec![e.add(&[c[0], e.mul(&[one(0), p, cos(t)])]), e.add(&[c[1], e.mul(&[one(1), p, sin(t)])])];
                    e.mul(&[scale, e.f("integrate", &[e.mul(&[subst(e, f, &vars, &x), p]), e.list(&[t, e.int(0), e.mul(&[e.int(2), e.pi()])]), e.list(&[p, e.int(0), rr])])])
                }
                (2, false) => {
                    let x = vec![e.add(&[c[0], e.mul(&[rr, cos(t)])]), e.add(&[c[1], e.mul(&[rr, sin(t)])])];
                    e.f("integrate", &[e.mul(&[subst(e, f, &vars, &x), rr]), e.list(&[t, e.int(0), e.mul(&[e.int(2), e.pi()])])])
                }
                (3, true) => {
                    let q = param(h, 102);
                    let x = vec![
                        e.add(&[c[0], e.mul(&[one(0), p, sin(q), cos(t)])]),
                        e.add(&[c[1], e.mul(&[one(1), p, sin(q), sin(t)])]),
                        e.add(&[c[2], e.mul(&[one(2), p, cos(q)])]),
                    ];
                    let body = e.mul(&[subst(e, f, &vars, &x), e.sq(p), sin(q)]);
                    e.mul(&[
                        scale,
                        e.f("integrate", &[body, e.list(&[t, e.int(0), e.mul(&[e.int(2), e.pi()])]), e.list(&[q, e.int(0), e.pi()]), e.list(&[p, e.int(0), rr])]),
                    ])
                }
                (3, false) => {
                    let q = param(h, 102);
                    let x = vec![
                        e.add(&[c[0], e.mul(&[rr, sin(q), cos(t)])]),
                        e.add(&[c[1], e.mul(&[rr, sin(q), sin(t)])]),
                        e.add(&[c[2], e.mul(&[rr, cos(q)])]),
                    ];
                    let body = e.mul(&[subst(e, f, &vars, &x), e.sq(rr), sin(q)]);
                    e.f("integrate", &[body, e.list(&[t, e.int(0), e.mul(&[e.int(2), e.pi()])]), e.list(&[q, e.int(0), e.pi()])])
                }
                (1, true) => e.f("integrate", &[f, e.list(&[vars[0], e.sub(c[0], rr), e.add(&[c[0], rr])])]),
                _ => return Ok(Reply::Decline),
            }
        }
        Region::Parametric { fs, specs } => {
            let us: Vec<T<'h>> = specs.iter().map(|s| s.0).collect();
            let jac: Vec<Vec<T<'h>>> = us.iter().map(|&u| fs.iter().map(|&g| e.f("d", &[g, u])).collect()).collect();
            let density = if us.len() == fs.len() {
                crate::props::jacobian_density(cx, e.f("det", &[e.list(&jac.iter().map(|r| e.list(r)).collect::<Vec<_>>())]), specs)?
            } else {
                let g: Vec<T<'h>> = jac.iter().map(|x| e.list(&jac.iter().map(|y| e.dot(x, y)).collect::<Vec<_>>())).collect();
                e.sqrt(e.f("simplify", &[e.f("det", &[e.list(&g)])]))
            };
            let mut args = vec![e.mul(&[subst(e, f, &vars, fs), density])];
            for &(u, lo, hi) in specs {
                args.push(e.list(&[u, lo, hi]));
            }
            e.f("integrate", &args)
        }
        Region::Implicit { vars: rv, .. } if rv.len() == vars.len() && rv.iter().all(|v| matches!(h.view(*v), View::Sym(_))) => {
            let approx = implicit_measure(cx, &r)?;
            let g = subst(e, f, &vars, rv);
            let g = cx.eval(g)?;
            match exact_implicit_integral(cx, &r, Some(g), approx)? {
                Some(t) => return Ok(Reply::Done(t)),
                None => return Ok(Reply::Decline),
            }
        }
        Region::Annulus { c, r0, r1 } => {
            let x = vec![e.add(&[c[0], e.mul(&[p, e.f("cos", &[t])])]), e.add(&[c[1], e.mul(&[p, e.f("sin", &[t])])])];
            e.f("integrate", &[e.mul(&[subst(e, f, &vars, &x), p]), e.list(&[t, e.int(0), e.mul(&[e.int(2), e.pi()])]), e.list(&[p, *r0, *r1])])
        }
        Region::Polyhedron { verts, faces } => {
            let tets = crate::props::tetrahedra(verts, faces);
            e.add(&tets.iter().map(|v| simplex_integral(cx, f, &vars, v)).collect::<Vec<_>>())
        }
        Region::Affine { p: base, free, pos } if free.len() + pos.len() == 2 && n == 3 && pos.is_empty() => {
            // a plane: base + s u + t v over the whole parameter plane, with the area element |u x v|
            let (u, v) = (&free[0], &free[1]);
            let x: Vec<T<'h>> = (0..3).map(|k| e.add(&[base[k], e.mul(&[t, u[k]]), e.mul(&[p, v[k]])])).collect();
            let el = e.sqrt(e.norm2(&e.cross(u, v)));
            let inf = e.sym("inf");
            e.mul(&[el, e.f("integrate", &[subst(e, f, &vars, &x), e.list(&[t, e.neg(inf), inf]), e.list(&[p, e.neg(inf), inf])])])
        }
        Region::Implicit { .. } | Region::Torus { .. } | Region::Affine { .. } | Region::Union(_) | Region::Revolved { .. } => return Ok(Reply::Decline),
    };
    Ok(Reply::Again(term))
}
