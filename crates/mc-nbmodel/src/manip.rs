//! Interactive controls (round 13): an output `manipulate(expr, [a, lo, hi], [b, lo, hi, step], [[c, init], lo, hi],
//! ...)` shows a slider per parameter; moving one evaluates `{ let a = v; let b = w; ...; expr }` in the kernel and
//! the result shows in the output below the controls (an Output cell marked `manipulated`).
//!
//! - The kernel keeps `manipulate(...)` as written (its arguments held, `mc_kernel::front_end`), so the notebook
//!   reads the expression and the controls from the printed value; no protocol message is added. The bounds, steps
//!   and initial values are numbers or simple constant expressions (`2*pi`, `-1/2`, `sqrt(2)`, `10^-3`).
//! - **Debounced, stale requests cancelled**: one evaluation of a `manipulate` runs at a time; positions reached while
//!   it runs replace each other (only the newest is evaluated next), and when the slider is let go while an older
//!   position is still evaluating, that evaluation is interrupted. Evaluations of controls are not numbered (no `out`
//!   entries, no labels).
//! - **Saved**: the positions are kept on the output (`controls=v1:v2:...`), the last result in the output after it,
//!   so a reopened notebook shows both without a kernel.
use crate::cell::{Cell, CellId, EvalMark};
use crate::eval::{KernelRequest, ManipRun};
use crate::notebook::Notebook;
use crate::style::Style;

/// One slider.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    /// the parameter's name
    pub name: String,
    /// the smallest value
    pub lo: f64,
    /// the largest value
    pub hi: f64,
    /// the step (`None`: continuous)
    pub step: Option<f64>,
    /// the value it starts at
    pub init: f64,
    /// the bounds and the step are integers: the values are written as integers
    pub integer: bool,
}

/// A `manipulate` output read from its printed value.
#[derive(Clone, Debug, PartialEq)]
pub struct Manipulate {
    /// the expression, as printed
    pub body: String,
    /// the controls, in order
    pub controls: Vec<Control>,
}

/// The top-level arguments of a call `head(...)` printed in `text` (strings and brackets respected).
fn call_args<'t>(text: &'t str, head: &str) -> Option<Vec<&'t str>> {
    let inner = text.trim().strip_prefix(head)?.strip_prefix('(')?.strip_suffix(')')?;
    split_top(inner)
}

/// `a, b, c` at bracket depth 0 (strings respected); `None` when the brackets do not balance.
fn split_top(s: &str) -> Option<Vec<&str>> {
    let (mut depth, mut start, mut out) = (0i32, 0usize, vec![]);
    let mut in_str = false;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 || in_str {
        return None;
    }
    let last = s[start..].trim();
    if !last.is_empty() || !out.is_empty() {
        out.push(last);
    }
    Some(out)
}

/// The elements of a list `[a, b, ...]`.
fn list(s: &str) -> Option<Vec<&str>> {
    let inner = s.trim().strip_prefix('[')?.strip_suffix(']')?;
    split_top(inner)
}

fn is_name(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_alphabetic() || c == '_') && cs.all(|c| c.is_alphanumeric() || c == '_')
}

/// The value of a constant expression: numbers, `pi`, `e`, `+ - * / ^`, parentheses, `sqrt(...)`.
pub fn constant(s: &str) -> Option<f64> {
    let toks: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut p = Parser { t: &toks, i: 0 };
    let v = p.sum()?;
    (p.i == toks.len() && v.is_finite()).then_some(v)
}

struct Parser<'a> {
    t: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.t.get(self.i).copied()
    }
    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        while let Some(c @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let w = self.product()?;
            v = if c == '+' { v + w } else { v - w };
        }
        Some(v)
    }
    fn product(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(c @ ('*' | '/')) = self.peek() {
            self.i += 1;
            let w = self.unary()?;
            v = if c == '*' { v * w } else { v / w };
        }
        Some(v)
    }
    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            '-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            '+' => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }
    fn power(&mut self) -> Option<f64> {
        let b = self.atom()?;
        if self.peek() == Some('^') {
            self.i += 1;
            // right associative, and the exponent may be negative (`10^-3`)
            let e = self.unary()?;
            return Some(b.powf(e));
        }
        Some(b)
    }
    fn atom(&mut self) -> Option<f64> {
        let c = self.peek()?;
        if c == '(' {
            self.i += 1;
            let v = self.sum()?;
            (self.peek()? == ')').then_some(())?;
            self.i += 1;
            return Some(v);
        }
        if c.is_ascii_digit() || c == '.' {
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                self.i += 1;
            }
            // an exponent: 1.5e-3
            if self.peek() == Some('e')
                && self.t.get(self.i + 1).is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '+')
            {
                self.i += 2;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
            }
            let s: String = self.t[start..self.i].iter().collect();
            return s.parse().ok();
        }
        if c.is_alphabetic() {
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                self.i += 1;
            }
            let name: String = self.t[start..self.i].iter().collect();
            return match name.as_str() {
                "pi" | "π" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                "sqrt" => {
                    (self.peek()? == '(').then_some(())?;
                    Some(self.atom()?.sqrt())
                }
                _ => None,
            };
        }
        None
    }
}

fn integer_literal(s: &str) -> bool {
    let s = s.trim().trim_start_matches('-');
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

impl Manipulate {
    /// The `manipulate` value printed as `printed`, when it is one with at least one well-formed control.
    pub fn parse(printed: &str) -> Option<Manipulate> {
        let args = call_args(printed, "manipulate")?;
        let (body, rest) = args.split_first()?;
        if body.is_empty() || rest.is_empty() {
            return None;
        }
        let mut controls = vec![];
        for a in rest {
            let parts = list(a)?;
            if !(3..=4).contains(&parts.len()) {
                return None;
            }
            let (name, init) = match list(parts[0]) {
                Some(v) if v.len() >= 2 && is_name(v[0]) => (v[0].to_string(), Some(constant(v[1])?)),
                _ if is_name(parts[0]) => (parts[0].to_string(), None),
                _ => return None,
            };
            let (lo, hi) = (constant(parts[1])?, constant(parts[2])?);
            let step = match parts.get(3) {
                Some(s) => Some(constant(s)?).filter(|s| *s > 0.0),
                None => None,
            };
            if hi <= lo {
                return None;
            }
            let integer = integer_literal(parts[1])
                && integer_literal(parts[2])
                && parts.get(3).is_some_and(|s| integer_literal(s));
            let mut c = Control { name, lo, hi, step, init: lo, integer };
            c.init = c.snap(init.unwrap_or(lo));
            controls.push(c);
        }
        Some(Manipulate { body: body.to_string(), controls })
    }

    /// The initial positions.
    pub fn initial(&self) -> Vec<f64> {
        self.controls.iter().map(|c| c.init).collect()
    }

    /// The positions kept on the output (`controls=v1:v2:...`), when they fit the controls.
    pub fn values_from(&self, attr: Option<&str>) -> Option<Vec<f64>> {
        let v: Vec<f64> = attr?.split(':').map(|x| x.parse::<f64>().ok()).collect::<Option<_>>()?;
        (v.len() == self.controls.len()).then(|| v.iter().zip(&self.controls).map(|(v, c)| c.snap(*v)).collect())
    }

    /// The source evaluated for `values`: `{ let a = v; ...; expr }`.
    pub fn source(&self, values: &[f64]) -> String {
        let mut s = String::from("{ ");
        for (c, v) in self.controls.iter().zip(values) {
            s.push_str(&format!("let {} = {}; ", c.name, c.text(*v)));
        }
        s.push_str(&self.body);
        s.push_str(" }");
        s
    }
}

/// The `controls` attribute's value for `values`.
pub fn values_attr(m: &Manipulate, values: &[f64]) -> String {
    m.controls.iter().zip(values).map(|(c, v)| c.text(*v)).collect::<Vec<_>>().join(":")
}

impl Control {
    /// The resolution of positions: the step, else a thousandth of the range rounded down to a power of ten.
    pub fn resolution(&self) -> f64 {
        match self.step {
            Some(s) => s,
            None => 10f64.powf(((self.hi - self.lo) / 1000.0).log10().floor()),
        }
    }

    /// `v` within the range, on the steps (from `lo`), rounded to the resolution's digits.
    pub fn snap(&self, v: f64) -> f64 {
        let v = if v.is_finite() { v.clamp(self.lo, self.hi) } else { self.lo };
        let r = self.resolution();
        let v = match self.step {
            Some(s) => (self.lo + ((v - self.lo) / s).round() * s).min(self.hi),
            None => v,
        };
        let digits = (-r.log10().floor()).clamp(0.0, 15.0) as i32;
        let m = 10f64.powi(digits);
        ((v * m).round() / m).clamp(self.lo, self.hi)
    }

    /// The source text of value `v`: an integer for integer controls, else a machine real.
    pub fn text(&self, v: f64) -> String {
        if self.integer {
            return format!("{}", v.round() as i64);
        }
        let digits = (-self.resolution().log10().floor()).clamp(1.0, 15.0) as usize;
        let s = format!("{v:.digits$}");
        let s = s.trim_end_matches('0');
        let s = if s.ends_with('.') { format!("{s}0") } else { s.to_string() };
        if s == "-0.0" {
            "0.0".into()
        } else {
            s
        }
    }
}

impl Notebook {
    /// The controls of Output cell `id` and their positions now, when it shows a `manipulate` value.
    pub fn manipulate_of(&self, id: CellId) -> Option<(Manipulate, Vec<f64>)> {
        let c = self.cell(id)?;
        if c.style != Style::Output || c.failure {
            return None;
        }
        let m = Manipulate::parse(&c.text)?;
        let values = m.values_from(c.out_extra.get("controls")).unwrap_or_else(|| m.initial());
        Some((m, values))
    }

    /// Move control `k` of `manipulate` output `id` to `v`; `done` when the slider is let go. The position is kept
    /// on the output (not an undo unit) and evaluated: at once when nothing of this notebook's controls runs, else
    /// after the running evaluation (only the newest position waits; letting go interrupts a stale one).
    pub(crate) fn set_control(&mut self, id: CellId, k: usize, v: f64, done: bool) -> bool {
        let Some((m, mut values)) = self.manipulate_of(id) else { return false };
        let Some(c) = m.controls.get(k) else { return false };
        values[k] = c.snap(v);
        let attr = values_attr(&m, &values);
        if let Some(i) = self.index_of(id) {
            if self.cells[i].out_extra.get("controls") != Some(attr.as_str()) {
                self.cells[i].out_extra.set("controls", Some(attr));
                self.retouch(i);
                self.mark_modified();
            }
        }
        self.request_manipulate(id, values, done);
        true
    }

    /// Ask for the result of `manipulate` output `id` at `values`.
    pub(crate) fn request_manipulate(&mut self, id: CellId, values: Vec<f64>, done: bool) {
        if let Some(run) = &mut self.eval.manip {
            if run.cell == id && run.values == values {
                self.eval.manip_next = None;
                return;
            }
            self.eval.manip_next = Some((id, values));
            if done && !run.interrupted {
                run.interrupted = true;
                let req = run.req;
                self.eval.outbox.push(KernelRequest::Interrupt { id: req });
            }
            return;
        }
        self.start_manipulate(id, values);
    }

    pub(crate) fn start_manipulate(&mut self, id: CellId, values: Vec<f64>) {
        let Some((m, _)) = self.manipulate_of(id) else { return };
        let req = self.eval.next_req;
        self.eval.next_req += 1;
        self.eval.outbox.push(KernelRequest::Side { id: req, src: m.source(&values) });
        self.eval.manip = Some(ManipRun { cell: id, req, values, result: None, interrupted: false });
        if let Some(i) = self.index_of(id) {
            self.cells[i].eval = EvalMark::Running;
            self.retouch(i);
        }
    }

    /// The evaluation of a control ended: its result shows below the controls, the newest waiting position starts.
    pub(crate) fn finish_manipulate(&mut self, interrupted: bool) {
        let Some(run) = self.eval.manip.take() else { return };
        if let Some(i) = self.index_of(run.cell) {
            self.cells[i].eval = EvalMark::None;
            self.retouch(i);
        }
        if let (false, Some((text, failure))) = (interrupted, run.result) {
            self.show_manipulated(run.cell, &text, failure);
        }
        if let Some((id, values)) = self.eval.manip_next.take() {
            self.start_manipulate(id, values);
        }
    }

    /// Show `text` as the result of `manipulate` output `id` (in the Output cell after it, made when missing).
    fn show_manipulated(&mut self, id: CellId, text: &str, failure: bool) {
        let Some(i) = self.index_of(id) else { return };
        let next = self.cells.get(i + 1).filter(|c| c.style == Style::Output && c.out_extra.has("manipulated"));
        match next.map(|c| c.id) {
            Some(rid) => {
                self.set_text(rid, text.to_string(), false);
                let j = i + 1;
                self.cells[j].failure = failure;
                self.retouch(j);
            }
            None => {
                let mut c: Cell = self.new_cell(Style::Output, text);
                c.failure = failure;
                c.input_of = self.cells[i].input_of;
                c.out_extra.set("manipulated", None);
                self.insert_cells(i + 1, vec![c], false);
            }
        }
        self.mark_modified();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manipulate_values_read_with_their_controls() {
        let m = Manipulate::parse(
            "manipulate(plot(sin(a*x), [x, 0, 2*pi]), [a, 1, 5], [[b, 2], 0, 10, 0.5], [c, 0, 2*pi], [n, 1, 10, 1])",
        )
        .unwrap();
        assert_eq!(m.body, "plot(sin(a*x), [x, 0, 2*pi])");
        let names: Vec<&str> = m.controls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", "n"]);
        assert_eq!(m.initial(), [1.0, 2.0, 0.0, 1.0]);
        assert!((m.controls[2].hi - 2.0 * std::f64::consts::PI).abs() < 1e-12);
        assert!(m.controls[3].integer && !m.controls[1].integer);
        assert_eq!(m.controls[1].snap(3.3), 3.5);
        assert_eq!(m.controls[0].snap(9.0), 5.0);
        assert_eq!(m.controls[3].snap(4.4), 4.0);
        let values = [2.5, 3.5, 1.0, 7.0];
        assert_eq!(m.source(&values), "{ let a = 2.5; let b = 3.5; let c = 1.0; let n = 7; plot(sin(a*x), [x, 0, 2*pi]) }");
        assert_eq!(values_attr(&m, &values), "2.5:3.5:1.0:7");
        assert_eq!(m.values_from(Some("2.5:3.5:1.0:7")).unwrap(), values);
        assert!(m.values_from(Some("1:2")).is_none(), "the wrong count");
        // not manipulate values
        for bad in ["manipulate(x)", "manipulate(x, [a])", "manipulate(x, [a, 2, 1])", "manipulate(x, [f(a), 0, 1])", "plot(x)"] {
            assert!(Manipulate::parse(bad).is_none(), "{bad}");
        }
        // a continuous control: a thousandth of the range
        let c = &Manipulate::parse("manipulate(x, [t, 0, 1])").unwrap().controls[0];
        assert_eq!(c.snap(0.123456), 0.123);
        assert_eq!(c.text(0.5), "0.5");
    }

    #[test]
    fn constants_evaluate() {
        assert_eq!(constant("-1/2"), Some(-0.5));
        assert_eq!(constant("10^-3"), Some(0.001));
        assert_eq!(constant("2*pi"), Some(2.0 * std::f64::consts::PI));
        assert_eq!(constant("sqrt(4) + 1.5e1"), Some(17.0));
        assert_eq!(constant("2^3^2"), Some(512.0));
        assert!(constant("x + 1").is_none());
        assert!(constant("1/0").is_none());
    }
}
