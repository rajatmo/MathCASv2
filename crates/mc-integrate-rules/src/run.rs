//! The interpreter: `int(u)` by linearity, the caller's quick methods, then the first rule whose pattern and
//! condition match; right sides are built by canonical arithmetic and their `int(...)` calls recursed into.
//! Bounded: depth, number of rule applications, a deadline, the evaluator's fuel; a cycle (an integrand met again
//! while it is being integrated) fails that branch.
use crate::cond::{function, holds, value};
use crate::syntax::N;
use crate::table::{bit, table, Rule, Table, RUN_ADD, SINGLE};
use crate::util::{call_of, free_of, replace};
use mc_api::{Store, View, T};
use mc_arith::Arith;
use mc_core::{Heap, Raw};
use mc_eval::{pat, Binds, NativeCx, Pat, Pattern, Res, Val};
use rustc_hash::FxHashMap;
use std::time::{Duration, Instant};

/// The quick methods of the caller, tried on every integrand before the rules (`(cx, u, x)`).
pub type Fallback<'f, 'h> = dyn FnMut(&mut NativeCx<'_, '_, 'h>, T<'h>, T<'h>) -> Res<Option<T<'h>>> + 'f;

/// Whether the product logarithm substitution runs (it was off until `mc-special`'s `product_log` balls stopped
/// aborting above about 1000 bits, round 18).
const PRODUCT_LOG: bool = true;

/// The size (nodes) up to which the substitutions and expansions after the rules are tried.
const METHOD_SIZE: usize = 400;

/// Limits of one call.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// wall-clock time
    pub time: Duration,
    /// nesting of `int`
    pub depth: u32,
    /// rule applications
    pub steps: u32,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { time: Duration::from_millis(1500), depth: 24, steps: 400 }
    }
}

/// Counters of one call.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// integrands seen by `int` (memo hits included)
    pub integrands: u64,
    /// candidate rules after dispatch
    pub candidates: u64,
    /// rules whose pattern and condition matched
    pub matched: u64,
    /// time spent in dispatch (mask and candidate list)
    pub dispatch: Duration,
    /// time spent matching
    pub matching: Duration,
    /// a limit was reached
    pub exhausted: bool,
}

/// One call's state.
pub struct Run<'a, 'f, 'c, 'k, 'h> {
    cx: &'a mut NativeCx<'c, 'k, 'h>,
    ar: Arith<'h, Heap>,
    x: T<'h>,
    fallback: &'a mut Fallback<'f, 'h>,
    table: &'static Table,
    deadline: Instant,
    limits: Limits,
    depth: u32,
    steps: u32,
    memo: FxHashMap<T<'h>, Option<T<'h>>>,
    pats: FxHashMap<usize, Option<Pattern>>,
    cands: Vec<Vec<usize>>,
    /// the rules applied on the way to the answer, outermost first (file, line, section)
    pub fired: Vec<usize>,
    /// counters
    pub stats: Stats,
    trace: bool,
}

impl<'a, 'f, 'c, 'k, 'h> Run<'a, 'f, 'c, 'k, 'h> {
    /// A run for integrands in `x`.
    pub fn new(
        cx: &'a mut NativeCx<'c, 'k, 'h>,
        x: T<'h>,
        limits: Limits,
        fallback: &'a mut Fallback<'f, 'h>,
    ) -> Result<Run<'a, 'f, 'c, 'k, 'h>, &'static str> {
        let table = table()?;
        let ar = Arith::new(cx.heap()).with_stop(cx.stop_flag().clone());
        Ok(Run {
            cx,
            ar,
            x,
            fallback,
            table,
            deadline: Instant::now() + limits.time,
            limits,
            depth: 0,
            steps: 0,
            memo: FxHashMap::default(),
            pats: FxHashMap::default(),
            cands: vec![],
            fired: vec![],
            stats: Stats::default(),
            trace: std::env::var_os("MC_RULES_TRACE").is_some(),
        })
    }

    /// The heap.
    pub fn heap(&self) -> &'h Heap {
        self.cx.heap()
    }
    /// The variable of integration.
    pub fn x(&self) -> T<'h> {
        self.x
    }
    /// The canonical arithmetic.
    pub fn arith(&self) -> &Arith<'h, Heap> {
        &self.ar
    }
    /// Evaluate a term.
    pub fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.cx.eval(t)
    }

    /// The rule with number `id`.
    pub fn rule(&self, id: usize) -> &'static Rule {
        &self.table.rules[id]
    }

    /// An antiderivative of `u`, or `None`.
    pub fn int(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        self.cx.tick(4)?;
        self.stats.integrands += 1;
        if self.depth >= self.limits.depth || self.steps >= self.limits.steps || Instant::now() > self.deadline {
            self.stats.exhausted = true;
            return Ok(None);
        }
        let u = self.cx.eval(u)?;
        if self.trace {
            eprintln!("{:>w$}int {}", "", crate::util::show(self.cx.heap(), u, 300), w = 2 * self.depth as usize);
        }
        if let Some(r) = self.memo.get(&u) {
            return Ok(*r);
        }
        self.memo.insert(u, None);
        self.depth += 1;
        let fired = self.fired.len();
        let r = self.int_new(u);
        self.depth -= 1;
        let r = r?;
        if r.is_none() {
            self.fired.truncate(fired);
        }
        self.memo.insert(u, r);
        Ok(r)
    }

    fn int_new(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        let h = self.cx.heap();
        let x = self.x;
        if free_of(h, u, x) {
            return Ok(Some(self.ar.mul(&[u, x])));
        }
        if let Some((s, args)) = call_of(h, u) {
            let name = h.sym_name(s);
            if name == mc_api::names::ADD {
                let mut out = Vec::with_capacity(args.len());
                for &a in args {
                    match self.int(a)? {
                        Some(r) => out.push(r),
                        None => return Ok(None),
                    }
                }
                return Ok(Some(self.ar.add(&out)));
            }
            if name == mc_api::names::MUL {
                let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| free_of(h, a, x));
                if !free.is_empty() {
                    let rest = self.ar.mul(&dep);
                    return Ok(self.int(rest)?.map(|r| self.ar.mul(&[self.ar.mul(&free), r])));
                }
            }
        }
        // fractional powers of products: a piecewise constant factor out (b sec)^(1/2) = F sec^(1/2)
        if let Some((f, q)) = crate::methods::unnest(&self.ar, u, x) {
            return Ok(self.int(q)?.map(|r| self.ar.mul(&[f, r])));
        }
        // the caller's quick methods (they already failed on the integrand the run started from)
        if self.depth > 1 {
            if let Some(r) = (self.fallback)(self.cx, u, x)? {
                return Ok(Some(r));
            }
        }
        let t0 = Instant::now();
        let mask = mask_of(h, u, x);
        let mut cands = self.cands.pop().unwrap_or_default();
        self.table.candidates(mask, &mut cands);
        self.stats.dispatch += t0.elapsed();
        self.stats.candidates += cands.len() as u64;
        let mut answer = None;
        for &id in &cands {
            if Instant::now() > self.deadline {
                self.stats.exhausted = true;
                break;
            }
            let t1 = Instant::now();
            let vals = self.try_rule(id, u)?;
            self.stats.matching += t1.elapsed();
            let Some(vals) = vals else { continue };
            self.stats.matched += 1;
            self.steps += 1;
            let at = self.fired.len();
            self.fired.push(id);
            let rule = self.rule(id);
            if self.trace {
                eprintln!(
                    "{:>w$}{} {} line {}",
                    "",
                    rule.file,
                    rule.text.section,
                    rule.text.line,
                    w = 2 * self.depth as usize
                );
            }
            if let Some(r) = self.rhs(&rule.text.rhs, &vals)? {
                answer = Some(r);
                break;
            }
            self.fired.truncate(at);
            if self.steps >= self.limits.steps {
                break;
            }
        }
        self.cands.push(cands);
        // the substitutions and expansions below rewrite the whole integrand: only for integrands of moderate size
        let small = crate::util::size(h, u, METHOD_SIZE) < METHOD_SIZE;
        if small && answer.is_none() && !self.stats.exhausted {
            answer = self.trig_rational(u)?;
        }
        if small && answer.is_none() && !self.stats.exhausted {
            answer = self.linear_radical(u)?;
        }
        if small && answer.is_none() && !self.stats.exhausted {
            if let Some(v) = self.sines_and_cosines(u)? {
                answer = self.int(v)?;
            }
        }
        if PRODUCT_LOG && answer.is_none() && !self.stats.exhausted {
            answer = self.product_log(u)?;
        }
        if small && answer.is_none() && !self.stats.exhausted {
            answer = self.expanded(u)?;
        }
        let exp_log = bit("exp") | bit("log") | bit("sin") | bit("cos") | bit("sinh") | bit("cosh");
        if answer.is_none() && !self.stats.exhausted && self.depth >= 2 && mask & !exp_log == 0 {
            answer = self.engine(u)?;
        }
        Ok(answer)
    }

    /// The bindings of rule `id` against `u` (its condition included), if it matches.
    fn try_rule(&mut self, id: usize, u: T<'h>) -> Res<Option<Vec<T<'h>>>> {
        if !self.pats.contains_key(&id) {
            let p = compile(self.cx, self.rule(id), self.x);
            self.pats.insert(id, p);
        }
        let Some(p) = self.pats.get(&id).and_then(|p| p.as_ref()) else {
            return Ok(None);
        };
        let mut b = Binds::default();
        if !self.cx.match_binds(p, u, &mut b)? {
            return Ok(None);
        }
        let rule = self.rule(id);
        let n = rule.text.vars.len();
        let mut vals = Vec::with_capacity(n);
        for i in 0..n {
            match b.get(i as u16) {
                Val::One(t) => vals.push(t),
                Val::Seq(o, len) => vals.push(joined(&self.ar, rule.runs[i], b.run(o, len))),
                Val::Unbound => return Ok(None),
            }
        }
        Ok(Some(vals))
    }

    /// Build a right side; `None` when an `int` in it fails.
    fn rhs(&mut self, n: &N, vals: &[T<'h>]) -> Res<Option<T<'h>>> {
        let h = self.cx.heap();
        Ok(Some(match n {
            N::Add(v) | N::Mul(v) => {
                let mut a = Vec::with_capacity(v.len());
                for t in v {
                    match self.rhs(t, vals)? {
                        Some(r) => a.push(r),
                        None => return Ok(None),
                    }
                }
                if matches!(n, N::Add(_)) {
                    self.ar.add(&a)
                } else {
                    self.ar.mul(&a)
                }
            }
            N::Pow(b, e) => {
                let (Some(b), Some(e)) = (self.rhs(b, vals)?, self.rhs(e, vals)?) else {
                    return Ok(None);
                };
                self.ar.pow(b, e)
            }
            N::Call(f, args) => {
                let mut a = Vec::with_capacity(args.len());
                for t in args {
                    match self.rhs(t, vals)? {
                        Some(r) => a.push(r),
                        None => return Ok(None),
                    }
                }
                match (&**f, a.as_slice()) {
                    ("int", [u]) => return self.int(*u),
                    // subst(r, g): r (in x) with x replaced by g
                    ("subst", [r, g]) => {
                        let t = replace(&self.ar, *r, self.x, *g);
                        self.cx.eval(t)?
                    }
                    (f, [u]) if crate::table::EVAL_UTILITIES.contains(&f) => self.cx.eval(h.call(h.sym(f), &[*u]))?,
                    _ => match function(&self.ar, f, &a) {
                        Some(t) => t,
                        None => return Ok(None),
                    },
                }
            }
            _ => match value(&self.ar, n, vals, self.x) {
                Some(t) => t,
                None => return Ok(None),
            },
        }))
    }
}

impl<'h> Run<'_, '_, '_, '_, 'h> {
    /// The tower engine on `u` (an exponential-logarithmic integrand a rule or a substitution led to), with a short
    /// budget inside the run's deadline: the decision procedure, then the ansatz.
    pub(crate) fn engine(&mut self, u: T<'h>) -> Res<Option<T<'h>>> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        let h = self.cx.heap();
        let e = h.sym_term(h.sym("e"));
        let exp = h.sym("exp");
        let pow = h.sym(mc_api::names::POW);
        // e^w as exp(w), the form the engine reads
        let v = crate::methods::map_calls(&self.ar, u, &mut |name, args| match (name, args) {
            (mc_api::names::POW, [b, w]) if *b == e => Some(h.call(exp, &[*w])),
            _ => None,
        });
        let _ = pow;
        let x = self.x;
        if self.trace {
            eprintln!("engine left {:?} on {}", left, crate::util::show(h, v, 200));
        }
        let ar = Arith::new(h);
        match mc_integrate::risch(self.cx, &ar, v, x, left.min(Duration::from_millis(300)))? {
            mc_integrate::Outcome::Found { part, rest } => {
                if self.trace {
                    eprintln!(
                        "engine found {} rest {}",
                        crate::util::show(h, part, 200),
                        crate::util::show(h, rest, 200)
                    );
                }
                if crate::cond::is_zero(&ar, rest) {
                    return Ok(Some(part));
                }
                // the rational rest goes to the quick methods (as in the portfolio)
                return Ok((self.fallback)(self.cx, rest, x)?.map(|r| ar.add(&[part, r])));
            }
            mc_integrate::Outcome::NonElementary(_) | mc_integrate::Outcome::Unknown => {}
        }
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        mc_integrate::norman(self.cx, &ar, v, x, left.min(Duration::from_millis(400)), true)
    }
}

/// The dispatch bits of the functions in the subterms of `u` that depend on `x` (`exp` for `e^w`).
pub fn mask_of<'h>(h: &'h Heap, u: T<'h>, x: T<'h>) -> u64 {
    let mut m = 0u64;
    let mut stack = vec![u];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(t) = stack.pop() {
        let View::App { head, args } = h.view(t) else {
            continue;
        };
        if !seen.insert(t) || free_of(h, t, x) {
            continue;
        }
        if let View::Sym(s) = h.view(head) {
            let name = h.sym_name(s);
            match name.as_str() {
                mc_api::names::ADD | mc_api::names::MUL => {}
                mc_api::names::POW => {
                    if matches!(h.view(args[0]), View::Sym(b) if h.sym_name(b) == "e") {
                        m |= bit("exp");
                    }
                }
                other => m |= bit(other),
            }
        }
        stack.extend_from_slice(args);
    }
    m
}

/// The compiled pattern of a rule for this heap and `x`: the condition wraps the whole pattern, so the matcher
/// backtracks into the next match when it fails. `None` if it does not compile (a bad rule; the table tests catch
/// these).
pub fn compile<'h>(cx: &mut NativeCx<'_, '_, 'h>, rule: &'static Rule, x: T<'h>) -> Option<Pattern> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let p = lower(&ar, &rule.text.lhs, x, None, &rule.runs)?;
    let p = match &rule.text.cond {
        Some(c) => {
            let nv = rule.text.vars.len();
            let vars: Vec<u16> = (0..nv as u16).collect();
            let rx = Raw::new(x);
            let runs = &rule.runs;
            pat::cond(p, &vars, move |cc| {
                let h = cc.heap();
                let ar = Arith::new(h);
                let vals: Vec<T<'_>> = (0..nv)
                    .map(|i| match cc.binding(i) {
                        Val::Seq(o, n) => joined(&ar, runs[i], cc.run(o, n)),
                        Val::One(t) => t,
                        Val::Unbound => h.int(0),
                    })
                    .collect();
                Ok(holds(&ar, c, &vals, rx.get(h)))
            })
        }
        None => p,
    };
    cx.compile_pattern(p).ok()
}

/// The term a run of arguments stands for: their sum or product.
fn joined<'h>(ar: &Arith<'h, Heap>, kind: u8, run: &[T<'h>]) -> T<'h> {
    if kind == RUN_ADD {
        ar.add(run)
    } else {
        ar.mul(run)
    }
}

fn has_vars(n: &N) -> bool {
    match n {
        N::X | N::Var(_) | N::Opt(_) => true,
        N::Add(v) | N::Mul(v) | N::Call(_, v) | N::And(v) | N::Or(v) => v.iter().any(has_vars),
        N::Pow(a, b) | N::Cmp(_, a, b) => has_vars(a) || has_vars(b),
        N::Not(a) => has_vars(a),
        _ => false,
    }
}

/// A left side as a pattern: `x` literal, `a_` a blank, `a_.` an optional argument with the unit of its place.
fn lower<'h>(ar: &Arith<'h, Heap>, n: &N, x: T<'h>, unit: Option<i64>, runs: &[u8]) -> Option<Pat> {
    let h = ar.store();
    if !has_vars(n) {
        return Some(pat::lit(value(ar, n, &[], x)?));
    }
    let one = |i: u16| {
        if runs[i as usize] == SINGLE {
            pat::var(i)
        } else {
            Pat::Seq { var: Some(i), min: 1, ty: None, mark: None }
        }
    };
    Some(match n {
        N::X => pat::lit(x),
        N::Var(i) => one(*i),
        N::Opt(i) => Pat::Opt { p: Box::new(one(*i)), var: *i, default: Raw::new(h.int(unit?)) },
        N::Add(v) => {
            let a: Option<Vec<Pat>> = v.iter().map(|t| lower(ar, t, x, Some(0), runs)).collect();
            pat::call(h, h.sym(mc_api::names::ADD), a?)
        }
        N::Mul(v) => {
            let a: Option<Vec<Pat>> = v.iter().map(|t| lower(ar, t, x, Some(1), runs)).collect();
            pat::call(h, h.sym(mc_api::names::MUL), a?)
        }
        N::Pow(b, e) => {
            pat::call(h, h.sym(mc_api::names::POW), vec![lower(ar, b, x, None, runs)?, lower(ar, e, x, Some(1), runs)?])
        }
        N::Call(f, args) if &**f == "sqrt" && args.len() == 1 => pat::call(
            h,
            h.sym(mc_api::names::POW),
            vec![lower(ar, &args[0], x, None, runs)?, pat::lit(ar.rational(h.int(1), h.int(2)))],
        ),
        N::Call(f, args) if &**f == "exp" && args.len() == 1 => pat::call(
            h,
            h.sym(mc_api::names::POW),
            vec![pat::lit(h.sym_term(h.sym("e"))), lower(ar, &args[0], x, Some(1), runs)?],
        ),
        N::Call(f, args) => {
            let a: Option<Vec<Pat>> = args.iter().map(|t| lower(ar, t, x, None, runs)).collect();
            pat::call(h, h.sym(f), a?)
        }
        _ => return None,
    })
}
