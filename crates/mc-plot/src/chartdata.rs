//! Chart data read once into a tree: numbers (quantities give their magnitudes), lists, and entries that are not
//! numbers (`missing()`, symbols, complex values: left out where a chart draws), each with the label and the styles
//! its wrappers give. Wrappers: `style(x, d...)` (directives), `labeled(x, l, ...)`, `callout(x, l, ...)`,
//! `legended(x, l)` (a label), `tooltip`, `popup_window`, `mouseover`, `hyperlink`, `annotation`, `status_area`
//! (what they wrap), `x -> l` (a label; a list of values with a list of labels labels each), and associations (the
//! keys label the values).
use crate::values;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Groups of items, each item with its numbers: `(group, [(item, numbers)])`.
pub(crate) type Groups<'a, 'h> = Vec<(Option<&'a Node<'h>>, Vec<(&'a Node<'h>, Vec<f64>)>)>;

/// One entry of chart data.
#[derive(Clone, Debug)]
pub(crate) struct Node<'h> {
    pub(crate) v: Val<'h>,
    pub(crate) label: Option<T<'h>>,
    pub(crate) style: Vec<T<'h>>,
}

/// What an entry is.
#[derive(Clone, Debug)]
pub(crate) enum Val<'h> {
    Num(f64),
    /// not a number (left out)
    Bad,
    List(Vec<Node<'h>>),
}

/// At most this many entries are read (the rest are left out).
const MAX: usize = 4_000_000;

/// Read `t` as chart data.
pub(crate) fn read<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Node<'h>> {
    let mut seen = 0usize;
    read_at(cx, t, 0, &mut seen)
}

fn read_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize, seen: &mut usize) -> Res<Node<'h>> {
    let h = cx.heap();
    *seen += 1;
    if seen.is_multiple_of(4096) {
        cx.tick(1)?;
    }
    let (mut t, mut label, mut style) = (t, None, vec![]);
    for _ in 0..12 {
        let Some((n, args)) = values::call_of(h, t) else { break };
        match (n.as_str(), args) {
            ("style", &[x, ref s @ ..]) => {
                style.extend_from_slice(s);
                t = x;
            }
            ("labeled" | "callout" | "legended", &[x, l, ..]) => {
                label.get_or_insert(l);
                t = x;
            }
            ("callout", &[x]) => t = x,
            ("tooltip" | "popup_window" | "mouseover" | "hyperlink" | "annotation" | "status_area" | "event_handler", &[x, ..]) => t = x,
            (n, &[x, l]) if n == mc_api::names::RULE || n == "rule_delayed" => {
                if let (Some(xs), Some(ls)) = (cx.as_list(x), cx.as_list(l)) {
                    if xs.len() == ls.len() {
                        let (xs, ls) = (xs.to_vec(), ls.to_vec());
                        let mut out = Vec::with_capacity(xs.len());
                        for (x, l) in xs.into_iter().zip(ls) {
                            let mut c = read_at(cx, x, depth + 1, seen)?;
                            c.label.get_or_insert(l);
                            out.push(c);
                        }
                        return Ok(Node { v: Val::List(out), label, style });
                    }
                }
                label.get_or_insert(l);
                t = x;
            }
            _ => break,
        }
    }
    if depth > 12 || *seen > MAX {
        return Ok(Node { v: Val::Bad, label, style });
    }
    if let Some(entries) = values::assoc_entries(cx, t)? {
        let mut out = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let mut c = read_at(cx, v, depth + 1, seen)?;
            c.label.get_or_insert(h.string(&k));
            out.push(c);
        }
        return Ok(Node { v: Val::List(out), label, style });
    }
    let t = values::spread_quantities(cx, t, 0);
    if let Some(items) = cx.as_list(t) {
        let items = items.to_vec();
        let mut out = Vec::with_capacity(items.len());
        for e in items {
            out.push(read_at(cx, e, depth + 1, seen)?);
        }
        return Ok(Node { v: Val::List(out), label, style });
    }
    let v = match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x).filter(|x| x.is_finite()),
        View::Str(_) => None,
        _ => values::read(cx, t)?.v,
    };
    Ok(Node { v: v.map_or(Val::Bad, Val::Num), label, style })
}

impl<'h> Node<'h> {
    /// The children of a list (none for a number).
    pub(crate) fn items(&self) -> &[Node<'h>] {
        match &self.v {
            Val::List(l) => l,
            _ => &[],
        }
    }

    pub(crate) fn num(&self) -> Option<f64> {
        match self.v {
            Val::Num(x) => Some(x),
            _ => None,
        }
    }

    pub(crate) fn is_list(&self) -> bool {
        matches!(self.v, Val::List(_))
    }

    /// A list of exactly `k` numbers.
    pub(crate) fn tuple(&self, k: usize) -> Option<Vec<f64>> {
        let l = self.items();
        if !self.is_list() || l.len() != k {
            return None;
        }
        l.iter().map(Node::num).collect()
    }

    /// The numbers of a flat list (entries that are not numbers left out); `None` when some entry is a list.
    pub(crate) fn numbers(&self) -> Option<Vec<f64>> {
        if !self.is_list() || self.items().iter().any(Node::is_list) {
            return None;
        }
        Some(self.items().iter().filter_map(Node::num).collect())
    }

    /// Every number in the tree, in order.
    pub(crate) fn leaves(&self) -> Vec<f64> {
        let mut out = vec![];
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            match &n.v {
                Val::Num(x) => out.push(*x),
                Val::Bad => {}
                Val::List(l) => stack.extend(l.iter().rev()),
            }
        }
        out
    }

    /// The label as text (strings as they are, other labels printed).
    pub(crate) fn label_text(&self, h: &'h Heap) -> Option<String> {
        self.label.map(|l| match h.view(l) {
            View::Str(s) => s.to_string(),
            _ => mc_syntax::Printer::new(h).expr(l),
        })
    }

    /// Groups of items that are each a list of `k` numbers: one group (the items of this list), or several (the
    /// items of each item); entries that are not such tuples are left out. A bare tuple is one group of one.
    pub(crate) fn tuple_groups(&self, k: usize) -> Groups<'_, 'h> {
        if let Some(t) = self.tuple(k) {
            return vec![(None, vec![(self, t)])];
        }
        let items = self.items();
        let direct: Vec<(&Node<'h>, Vec<f64>)> = items.iter().filter_map(|n| n.tuple(k).map(|t| (n, t))).collect();
        let nested = items.iter().filter(|n| n.is_list() && n.tuple(k).is_none()).count();
        if !direct.is_empty() && direct.len() >= nested {
            return vec![(None, direct)];
        }
        items
            .iter()
            .filter(|n| n.is_list() && n.tuple(k).is_none())
            .map(|g| (Some(g), g.items().iter().filter_map(|n| n.tuple(k).map(|t| (n, t))).collect::<Vec<_>>()))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    /// Data sets of numbers: this list of numbers, or each item that is one (nested lists one level deeper are
    /// groups of data sets, flattened in order).
    pub(crate) fn data_sets(&self) -> Vec<(&Node<'h>, Vec<f64>)> {
        if let Some(v) = self.numbers() {
            return vec![(self, v)];
        }
        let mut out = vec![];
        for n in self.items() {
            if let Some(v) = n.numbers() {
                out.push((n, v));
            } else if n.is_list() {
                for m in n.items() {
                    if let Some(v) = m.numbers() {
                        out.push((m, v));
                    }
                }
            }
        }
        out
    }
}

/// The values of a data container (`weighted_data`, `event_data`, `time_series`, `temporal_data`); anything else
/// as it is. Charts draw the values: the weights, flags and times are not part of the picture.
pub(crate) fn container_values<'h>(h: &'h mc_core::Heap, t: T<'h>) -> T<'h> {
    match values::call_of(h, t) {
        Some((n, &[x, _])) if matches!(n.as_str(), "weighted_data" | "event_data" | "time_series" | "temporal_data") => x,
        _ => t,
    }
}
