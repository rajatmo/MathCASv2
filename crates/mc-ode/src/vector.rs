//! Vector-valued unknowns: an unknown with a list as an initial value, `z(0) == [1, 0]`, stands for that many
//! scalar components `z[1], ..., z[m]`. Every occurrence `z(t)` or `derivative(k)(z)(t)` becomes the list of its
//! components' occurrences, the equations are evaluated (`dot(a, z(t))` gives a list) and an equation between
//! lists of equal length (or a list and a scalar) is threaded into one equation per component. The solution keeps
//! the components; `interpolating_function("z", ...)` gives the list of their values (see
//! [`crate::solution::Solution::vector`]).
use crate::system::{self, occurrence};
use crate::util::mentions;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// The vector unknowns of a system and their components.
#[derive(Default)]
pub(crate) struct Vectors {
    /// (the vector unknown, its components in order)
    pub groups: Vec<(Sym, Vec<Sym>)>,
}

/// The name of component k (from 1) of the unknown `name`.
pub(crate) fn component_name(name: &str, k: usize) -> String {
    format!("{name}[{k}]")
}

impl Vectors {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The unknowns with every vector unknown replaced by its components.
    pub fn scalars(&self, ys: &[Sym]) -> Vec<Sym> {
        let mut out = vec![];
        for &y in ys {
            match self.groups.iter().find(|g| g.0 == y) {
                Some((_, cs)) => out.extend_from_slice(cs),
                None => out.push(y),
            }
        }
        out
    }

    /// `t` with each occurrence of a vector unknown (`z(x)`, `derivative(k)(z)(x)`) replaced by the list of the
    /// components' occurrences; a bare vector unknown (or `derivative(k)(z)`) by `bare(name)` when given.
    pub fn spread<'h>(&self, h: &'h Heap, t: T<'h>, bare: Option<&dyn Fn(&str) -> T<'h>>) -> T<'h> {
        let syms: Vec<Sym> = self.groups.iter().map(|g| g.0).collect();
        let derivative = h.sym_term(h.sym("derivative"));
        let list = h.sym(mc_api::names::LIST);
        mc_numfn::replace(h, t, &mut |u| {
            if let Some((i, d, x)) = occurrence(h, &syms, derivative, u) {
                let items: Vec<T<'h>> = self.groups[i]
                    .1
                    .iter()
                    .map(|&c| {
                        let f = h.sym_term(c);
                        let f = if d == 0 { f } else { h.app(h.app(derivative, &[h.int(d as i64)]), &[f]) };
                        h.app(f, &[x])
                    })
                    .collect();
                return Some(h.call(list, &items));
            }
            let bare = bare?;
            match h.view(u) {
                View::Sym(s) if syms.contains(&s) => Some(bare(&h.sym_name(s))),
                View::App { head, args: &[y] } => {
                    let View::App { head: dh, args: &[n] } = h.view(head) else { return None };
                    let (View::Sym(s), View::Int(n @ 1..=1000)) = (h.view(y), h.view(n)) else { return None };
                    (dh == derivative && syms.contains(&s)).then(|| bare(&format!("{}{}", h.sym_name(s), "'".repeat(n as usize))))
                }
                _ => None,
            }
        })
    }
}

/// The vector unknowns among `ys`: those with an initial condition `z(s) == [v1, ..., vm]` (either side), a flat
/// list of m >= 1 entries.
pub(crate) fn find<'h>(cx: &mut NativeCx<'_, '_, 'h>, eqs: T<'h>, ys: &[Sym]) -> Res<Vectors> {
    let h = cx.heap();
    let equal = h.sym_term(h.sym("equal"));
    let derivative = h.sym_term(h.sym("derivative"));
    let mut dims: Vec<Option<usize>> = vec![None; ys.len()];
    for e in system::equations(cx, eqs)? {
        let View::App { head, args: &[l, r] } = h.view(e) else { continue };
        if head != equal {
            continue;
        }
        for (side, other) in [(l, r), (r, l)] {
            let Some((i, _, _)) = occurrence(h, ys, derivative, side) else { continue };
            if dims[i].is_some() || mentions(cx, other, ys)? {
                continue;
            }
            let v = cx.eval(other)?;
            if let Some(items) = cx.as_list(v) {
                if !items.is_empty() && items.iter().all(|&u| cx.as_list(u).is_none()) {
                    dims[i] = Some(items.len());
                }
            }
        }
    }
    let groups = ys
        .iter()
        .zip(&dims)
        .filter_map(|(&y, d)| {
            let m = (*d)?;
            let name = h.sym_name(y);
            Some((y, (1..=m).map(|k| h.sym(&component_name(&name, k))).collect()))
        })
        .collect();
    Ok(Vectors { groups })
}

/// The equations with the vector unknowns spread into components and threaded; `Err` with a message when two
/// sides are lists of different lengths.
pub(crate) fn thread<'h>(cx: &mut NativeCx<'_, '_, 'h>, eqs: T<'h>, v: &Vectors) -> Res<Result<T<'h>, String>> {
    let h = cx.heap();
    let equal = h.sym("equal");
    let equal_term = h.sym_term(equal);
    let mut out = vec![];
    for (k, e) in system::equations(cx, eqs)?.into_iter().enumerate() {
        let spread = v.spread(h, e, None);
        let View::App { head, args: &[l, r] } = h.view(spread) else {
            out.push(spread);
            continue;
        };
        if head != equal_term {
            out.push(spread);
            continue;
        }
        let (l, r) = (cx.eval(l)?, cx.eval(r)?);
        match (cx.as_list(l).map(<[_]>::to_vec), cx.as_list(r).map(<[_]>::to_vec)) {
            (Some(a), Some(b)) if a.len() == b.len() => out.extend(a.iter().zip(&b).map(|(&p, &q)| h.call(equal, &[p, q]))),
            (Some(_), Some(_)) => return Ok(Err(format!("the sides of equation {} have different lengths", k + 1))),
            (Some(a), None) => out.extend(a.iter().map(|&p| h.call(equal, &[p, r]))),
            (None, Some(b)) => out.extend(b.iter().map(|&q| h.call(equal, &[l, q]))),
            (None, None) => out.push(h.call(equal, &[l, r])),
        }
    }
    Ok(Ok(cx.list(&out)))
}
