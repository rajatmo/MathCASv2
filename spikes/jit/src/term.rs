//! Term-level evaluation: what v2 does without the compiler.
//!
//! The store is spike 1's hash-consed arena (`spikes/termstore/src/hc.rs`: 32-bit handles, immediate small
//! integers, one shared argument arena, a `hashbrown` table), extended with machine reals (interned, as decided in
//! spikes 1 and I3: a 64-bit value does not fit an immediate). A function is converted once to a term
//! (`block(set(v1, add(...)), while(...), ...)`) and evaluated by a strict evaluator that dispatches on the head
//! symbol, decodes number atoms, computes, and interns every intermediate value. Garbage made during one call is
//! dropped when the call returns (an ideal collector whose cost is the table removal).
//!
//! This is an *optimistic* stand-in for v2's evaluator: there is no rule lookup, no attribute or holding logic, no
//! canonical ordering of `plus`/`times` arguments and no normal-form stamping. Real symbolic evaluation is slower.
use crate::ir::{Bin, Elem, Func, Math, Ty, Un, E, S};
use crate::{Ctl, Status};
use hashbrown::HashTable;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Term(pub u32);

#[derive(Clone, Copy)]
enum Kind {
    Int(i64),
    Real(u64),
    Sym(u32),
    App { head: Term, start: u32, len: u32 },
}

#[derive(Clone, Copy)]
struct Node {
    kind: Kind,
    hash: u64,
}

const IMM: u32 = 1 << 31;
const IMM_MIN: i64 = -(1 << 30);
const IMM_MAX: i64 = (1 << 30) - 1;
const K: u64 = 0x9E37_79B9_7F4A_7C15;

fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

pub enum View<'a> {
    Int(i64),
    Real(f64),
    Sym(u32),
    App(Term, &'a [Term]),
}

#[derive(Default)]
pub struct Store {
    nodes: Vec<Node>,
    args: Vec<Term>,
    table: HashTable<u32>,
    names: Vec<String>,
}

impl Store {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn intern(&mut self, kind: Kind, hash: u64, new_args: &[Term]) -> Term {
        let nodes = &self.nodes;
        let args = &self.args;
        let eq = |&t: &u32| {
            let n = &nodes[t as usize];
            n.hash == hash
                && match (n.kind, kind) {
                    (Kind::Int(a), Kind::Int(b)) => a == b,
                    (Kind::Real(a), Kind::Real(b)) => a == b,
                    (Kind::Sym(a), Kind::Sym(b)) => a == b,
                    (Kind::App { head: h1, start, len }, Kind::App { head: h2, .. }) => {
                        h1 == h2 && &args[start as usize..(start + len) as usize] == new_args
                    }
                    _ => false,
                }
        };
        if let Some(&t) = self.table.find(hash, eq) {
            return Term(t);
        }
        let id = self.nodes.len() as u32;
        let kind = match kind {
            Kind::App { head, len, .. } => {
                let start = self.args.len() as u32;
                self.args.extend_from_slice(new_args);
                Kind::App { head, start, len }
            }
            k => k,
        };
        self.nodes.push(Node { kind, hash });
        let nodes = &self.nodes;
        self.table.insert_unique(hash, id, |&t| nodes[t as usize].hash);
        Term(id)
    }

    fn hash_of(&self, t: Term) -> u64 {
        if t.0 & IMM != 0 {
            mix(0x17, ((((t.0 << 1) as i32) >> 1) as i64) as u64)
        } else {
            self.nodes[t.0 as usize].hash
        }
    }

    pub fn int(&mut self, v: i64) -> Term {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & !IMM));
        }
        self.intern(Kind::Int(v), mix(0x17, v as u64), &[])
    }
    pub fn real(&mut self, x: f64) -> Term {
        self.intern(Kind::Real(x.to_bits()), mix(0x29, x.to_bits()), &[])
    }
    pub fn sym(&mut self, name: &str) -> Term {
        let i = match self.names.iter().position(|n| n == name) {
            Some(i) => i,
            None => {
                self.names.push(name.into());
                self.names.len() - 1
            }
        } as u32;
        self.intern(Kind::Sym(i), mix(0x51, crate::fp_words(name.bytes().map(u64::from))), &[])
    }
    pub fn app(&mut self, head: Term, args: &[Term]) -> Term {
        let mut h = mix(0x33, self.hash_of(head));
        for &a in args {
            h = mix(h, self.hash_of(a));
        }
        self.intern(Kind::App { head, start: 0, len: args.len() as u32 }, h, args)
    }
    pub fn view(&self, t: Term) -> View<'_> {
        if t.0 & IMM != 0 {
            return View::Int((((t.0 << 1) as i32) >> 1) as i64);
        }
        match self.nodes[t.0 as usize].kind {
            Kind::Int(v) => View::Int(v),
            Kind::Real(b) => View::Real(f64::from_bits(b)),
            Kind::Sym(i) => View::Sym(i),
            Kind::App { head, start, len } => View::App(head, &self.args[start as usize..(start + len) as usize]),
        }
    }

    /// Drop every term made after `mark` (they are garbage once a call has returned its result to the host).
    fn reset_to(&mut self, mark: (usize, usize)) {
        for id in mark.0..self.nodes.len() {
            let h = self.nodes[id].hash;
            if let Ok(e) = self.table.find_entry(h, |&t| t as usize == id) {
                e.remove();
            }
        }
        self.nodes.truncate(mark.0);
        self.args.truncate(mark.1);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum H {
    Block,
    Set,
    SetPart,
    If,
    While,
    For,
    Un(Un),
    Bin(Bin),
    PowI,
    Call(Math),
    Sel,
    Part,
    Len,
    Complex,
    List,
    True,
    False,
    Null,
    Var(u32),
}

/// A host value passed to or read from a term-level call.
#[derive(Clone, Debug, PartialEq)]
pub enum TVal {
    I(i64),
    F(f64),
    List(Vec<f64>),
}

#[derive(Clone, Copy)]
enum Num {
    I(i64),
    R(f64),
    C(f64, f64),
    B(bool),
}

pub struct TermFn {
    pub st: Store,
    /// head of each symbol, by symbol ordinal
    heads: Vec<H>,
    body: Term,
    params: Vec<u32>,
    env: Vec<Term>,
    /// initial value of every variable (zero of its type, as in the registers of the other back ends)
    init: Vec<Term>,
    t_true: Term,
    t_false: Term,
    t_null: Term,
    t_complex: Term,
    t_list: Term,
    mark: (usize, usize),
    pub steps: u64,
}

impl TermFn {
    pub fn new(func: &Func) -> Result<TermFn, String> {
        crate::ir::check(func)?;
        for t in &func.vars {
            if matches!(t, Ty::Arr(_, 2) | Ty::Arr(Elem::I64, _)) {
                return Err("term-level baseline supports 1-D real arrays only".into());
            }
        }
        let mut st = Store::default();
        let mut heads = vec![];
        let mut symh = |st: &mut Store, name: &str, h: H| {
            let t = st.sym(name);
            if let View::Sym(i) = st.view(t) {
                if heads.len() <= i as usize {
                    heads.resize(i as usize + 1, H::Null);
                }
                heads[i as usize] = h;
            }
            t
        };
        let mut c = Conv { syms: Default::default(), vars: vec![] };
        for (name, h) in [
            ("block", H::Block),
            ("set", H::Set),
            ("setpart", H::SetPart),
            ("if", H::If),
            ("while", H::While),
            ("for", H::For),
            ("powi", H::PowI),
            ("select", H::Sel),
            ("part", H::Part),
            ("length", H::Len),
            ("complex", H::Complex),
            ("list", H::List),
            ("True", H::True),
            ("False", H::False),
            ("Null", H::Null),
        ] {
            let t = symh(&mut st, name, h);
            c.syms.insert(name.to_string(), t);
        }
        for op in [Un::Neg, Un::Not, Un::Re, Un::Im, Un::Abs2, Un::Conj, Un::IToF, Un::FToI, Un::Abs, Un::Sqrt, Un::Floor] {
            let t = symh(&mut st, &format!("{op:?}").to_lowercase(), H::Un(op));
            c.syms.insert(format!("un:{op:?}"), t);
        }
        use Bin::*;
        for op in [Add, Sub, Mul, Div, Rem, And, Or, Xor, Shl, Shr, UShr, Lt, Le, Gt, Ge, Eq, Ne] {
            let t = symh(&mut st, &format!("{op:?}").to_lowercase(), H::Bin(op));
            c.syms.insert(format!("bin:{op:?}"), t);
        }
        for m in [Math::Sin, Math::Cos, Math::Exp, Math::Log, Math::Pow, Math::Fma] {
            let t = symh(&mut st, &format!("{m:?}").to_lowercase(), H::Call(m));
            c.syms.insert(format!("math:{m:?}"), t);
        }
        for k in 0..func.vars.len() {
            let t = symh(&mut st, &format!("v{k}"), H::Var(k as u32));
            c.vars.push(t);
        }
        let mut items: Vec<Term> = func.body.iter().map(|s| c.stmt(&mut st, s)).collect();
        items.push(match &func.ret {
            Some(e) => c.expr(&mut st, e),
            None => c.syms["Null"],
        });
        let blk = c.syms["block"];
        let body = st.app(blk, &items);
        let t_null = c.syms["Null"];
        let (zi, zf) = (st.int(0), st.real(0.0));
        let zc = st.app(c.syms["complex"], &[zf, zf]);
        let init: Vec<Term> = func
            .vars
            .iter()
            .map(|t| match t {
                Ty::I64 => zi,
                Ty::F64 => zf,
                Ty::C64 => zc,
                Ty::Bool => c.syms["False"],
                Ty::Arr(..) => t_null,
            })
            .collect();
        let mark = (st.nodes.len(), st.args.len());
        Ok(TermFn {
            heads,
            body,
            params: func.params.clone(),
            env: init.clone(),
            init,
            t_true: c.syms["True"],
            t_false: c.syms["False"],
            t_null,
            t_complex: c.syms["complex"],
            t_list: c.syms["list"],
            st,
            mark,
            steps: 0,
        })
    }

    /// Evaluate the function on host values; returns its result and the final values of its list parameters.
    pub fn call(&mut self, args: &[TVal], ctl: &Ctl) -> Result<(Option<TVal>, Vec<Vec<f64>>), Status> {
        self.st.reset_to(self.mark);
        self.env.copy_from_slice(&self.init);
        for (k, a) in args.iter().enumerate() {
            let t = match a {
                TVal::I(n) => self.st.int(*n),
                TVal::F(x) => self.st.real(*x),
                TVal::List(xs) => {
                    let el: Vec<Term> = xs.iter().map(|&x| self.st.real(x)).collect();
                    self.st.app(self.t_list, &el)
                }
            };
            self.env[self.params[k] as usize] = t;
        }
        let r = self.eval(self.body, ctl)?;
        let res = match self.st.view(r) {
            View::Int(n) => Some(TVal::I(n)),
            View::Real(x) => Some(TVal::F(x)),
            View::Sym(_) if r == self.t_true => Some(TVal::I(1)),
            View::Sym(_) if r == self.t_false => Some(TVal::I(0)),
            _ => None,
        };
        let mut lists = vec![];
        for &p in &self.params {
            let t = self.env[p as usize];
            if let View::App(h, els) = self.st.view(t) {
                if h == self.t_list {
                    let els = els.to_vec();
                    lists.push(els.iter().map(|&e| self.real_of(e)).collect());
                }
            }
        }
        Ok((res, lists))
    }

    fn real_of(&self, t: Term) -> f64 {
        match self.st.view(t) {
            View::Real(x) => x,
            _ => f64::NAN,
        }
    }

    fn head(&self, t: Term) -> H {
        match self.st.view(t) {
            View::Sym(i) => self.heads[i as usize],
            _ => H::Null,
        }
    }

    fn num(&self, t: Term) -> Num {
        match self.st.view(t) {
            View::Int(n) => Num::I(n),
            View::Real(x) => Num::R(x),
            View::Sym(_) => Num::B(t == self.t_true),
            View::App(_, a) => Num::C(self.real_of(a[0]), self.real_of(a[1])),
        }
    }

    fn mk(&mut self, n: Num) -> Term {
        match n {
            Num::I(v) => self.st.int(v),
            Num::R(x) => self.st.real(x),
            Num::B(b) => {
                if b {
                    self.t_true
                } else {
                    self.t_false
                }
            }
            Num::C(re, im) => {
                let (a, b) = (self.st.real(re), self.st.real(im));
                self.st.app(self.t_complex, &[a, b])
            }
        }
    }

    fn arg(&self, t: Term, k: usize) -> Term {
        match self.st.view(t) {
            View::App(_, a) => a[k],
            _ => unreachable!(),
        }
    }

    fn nargs(&self, t: Term) -> usize {
        match self.st.view(t) {
            View::App(_, a) => a.len(),
            _ => 0,
        }
    }

    fn var_of(&self, t: Term) -> usize {
        match self.head(t) {
            H::Var(k) => k as usize,
            _ => unreachable!("not a variable"),
        }
    }

    fn tick(&mut self, ctl: &Ctl) -> Result<(), Status> {
        self.steps += 1;
        if self.steps & 1023 == 0 {
            if ctl.fuel.fetch_sub(1024, Ordering::Relaxed) < 1024 {
                return Err(Status::Fuel);
            }
            if ctl.stop.load(Ordering::Relaxed) != 0 {
                return Err(Status::Interrupted);
            }
        }
        Ok(())
    }

    fn eval(&mut self, t: Term, ctl: &Ctl) -> Result<Term, Status> {
        self.tick(ctl)?;
        let (h, n) = match self.st.view(t) {
            View::Int(_) | View::Real(_) => return Ok(t),
            View::Sym(i) => {
                return Ok(match self.heads.get(i as usize) {
                    Some(H::Var(k)) => self.env[*k as usize],
                    _ => t,
                })
            }
            View::App(h, a) => (h, a.len()),
        };
        let head = self.head(h);
        match head {
            H::Block => {
                let mut r = self.t_null;
                for k in 0..n {
                    r = self.eval(self.arg(t, k), ctl)?;
                }
                Ok(r)
            }
            H::Set => {
                let x = self.var_of(self.arg(t, 0));
                let val = self.eval(self.arg(t, 1), ctl)?;
                self.env[x] = val;
                Ok(self.t_null)
            }
            H::SetPart => {
                let x = self.var_of(self.arg(t, 0));
                let idx = self.eval(self.arg(t, 1), ctl)?;
                let val = self.eval(self.arg(t, 2), ctl)?;
                let Num::I(ix) = self.num(idx) else { unreachable!() };
                let list = self.env[x];
                let mut els = match self.st.view(list) {
                    View::App(_, a) => a.to_vec(),
                    _ => unreachable!(),
                };
                if ix < 0 || ix as usize >= els.len() {
                    return Err(Status::Index);
                }
                els[ix as usize] = val;
                self.env[x] = self.st.app(self.t_list, &els);
                Ok(self.t_null)
            }
            H::If => {
                let c = self.eval(self.arg(t, 0), ctl)?;
                let branch = if c == self.t_true { 1 } else { 2 };
                self.eval(self.arg(t, branch), ctl)
            }
            H::While => {
                loop {
                    let c = self.eval(self.arg(t, 0), ctl)?;
                    if c != self.t_true {
                        break;
                    }
                    self.eval(self.arg(t, 1), ctl)?;
                }
                Ok(self.t_null)
            }
            H::For => {
                let x = self.var_of(self.arg(t, 0));
                let lo = self.eval(self.arg(t, 1), ctl)?;
                let hi = self.eval(self.arg(t, 2), ctl)?;
                self.env[x] = lo;
                loop {
                    let (Num::I(cur), Num::I(end)) = (self.num(self.env[x]), self.num(hi)) else { unreachable!() };
                    if cur >= end {
                        break;
                    }
                    self.eval(self.arg(t, 3), ctl)?;
                    let Num::I(cur) = self.num(self.env[x]) else { unreachable!() };
                    self.env[x] = self.st.int(cur.wrapping_add(1));
                    self.tick(ctl)?;
                }
                Ok(self.t_null)
            }
            H::Part => {
                let a = self.eval(self.arg(t, 0), ctl)?;
                let idx = self.eval(self.arg(t, 1), ctl)?;
                let Num::I(ix) = self.num(idx) else { unreachable!() };
                match self.st.view(a) {
                    View::App(_, els) if ix >= 0 && (ix as usize) < els.len() => Ok(els[ix as usize]),
                    _ => Err(Status::Index),
                }
            }
            H::Len => {
                let a = self.eval(self.arg(t, 0), ctl)?;
                let n = self.nargs(a) as i64;
                Ok(self.st.int(n))
            }
            H::Sel => {
                let c = self.eval(self.arg(t, 0), ctl)?;
                let a = self.eval(self.arg(t, 1), ctl)?;
                let b = self.eval(self.arg(t, 2), ctl)?;
                Ok(if c == self.t_true { a } else { b })
            }
            H::Complex => {
                let a = self.eval(self.arg(t, 0), ctl)?;
                let b = self.eval(self.arg(t, 1), ctl)?;
                Ok(self.st.app(self.t_complex, &[a, b]))
            }
            H::Un(op) => {
                let a = self.eval(self.arg(t, 0), ctl)?;
                let r = un(op, self.num(a));
                Ok(self.mk(r))
            }
            H::Bin(op) => {
                let a = self.eval(self.arg(t, 0), ctl)?;
                let b = self.eval(self.arg(t, 1), ctl)?;
                let r = bin(op, self.num(a), self.num(b))?;
                Ok(self.mk(r))
            }
            H::PowI => {
                let a = self.eval(self.arg(t, 0), ctl)?;
                let Num::I(e) = self.num(self.arg(t, 1)) else { unreachable!() };
                let r = powi(self.num(a), e as u32);
                Ok(self.mk(r))
            }
            H::Call(m) => {
                let mut x = [0.0f64; 3];
                for (k, slot) in x.iter_mut().enumerate().take(n) {
                    let a = self.eval(self.arg(t, k), ctl)?;
                    let Num::R(v) = self.num(a) else { unreachable!() };
                    *slot = v;
                }
                let r = match m {
                    Math::Sin => crate::libm::sin(x[0]),
                    Math::Cos => crate::libm::cos(x[0]),
                    Math::Exp => crate::libm::exp(x[0]),
                    Math::Log => crate::libm::log(x[0]),
                    Math::Pow => crate::libm::pow(x[0], x[1]),
                    Math::Fma => crate::libm::fma(x[0], x[1], x[2]),
                };
                Ok(self.st.real(r))
            }
            _ => Ok(t),
        }
    }
}

fn un(op: Un, a: Num) -> Num {
    use Num::*;
    match (op, a) {
        (Un::Neg, I(x)) => I(x.wrapping_neg()),
        (Un::Neg, R(x)) => R(-x),
        (Un::Neg, C(x, y)) => C(-x, -y),
        (Un::Not, B(b)) => B(!b),
        (Un::Not, I(x)) => I(!x),
        (Un::Re, C(x, _)) => R(x),
        (Un::Im, C(_, y)) => R(y),
        (Un::Abs2, C(x, y)) => R(x * x + y * y),
        (Un::Conj, C(x, y)) => C(x, -y),
        (Un::IToF, I(x)) => R(x as f64),
        (Un::FToI, R(x)) => I(x as i64),
        (Un::Abs, R(x)) => R(x.abs()),
        (Un::Sqrt, R(x)) => R(x.sqrt()),
        (Un::Floor, R(x)) => R(x.floor()),
        _ => unreachable!("type-checked"),
    }
}

fn cmul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn bin(op: Bin, a: Num, b: Num) -> Result<Num, Status> {
    use Num::*;
    Ok(match (a, b) {
        (I(x), I(y)) => match op {
            Bin::Add => I(x.wrapping_add(y)),
            Bin::Sub => I(x.wrapping_sub(y)),
            Bin::Mul => I(x.wrapping_mul(y)),
            Bin::Div | Bin::Rem if y == 0 => return Err(Status::DivZero),
            Bin::Div => I(x.wrapping_div(y)),
            Bin::Rem => I(x.wrapping_rem(y)),
            Bin::And => I(x & y),
            Bin::Or => I(x | y),
            Bin::Xor => I(x ^ y),
            Bin::Shl => I(x.wrapping_shl(y as u32)),
            Bin::Shr => I(x.wrapping_shr(y as u32)),
            Bin::UShr => I((x as u64).wrapping_shr(y as u32) as i64),
            Bin::Lt => B(x < y),
            Bin::Le => B(x <= y),
            Bin::Gt => B(x > y),
            Bin::Ge => B(x >= y),
            Bin::Eq => B(x == y),
            Bin::Ne => B(x != y),
            Bin::Cplx => unreachable!(),
        },
        (R(x), R(y)) => match op {
            Bin::Add => R(x + y),
            Bin::Sub => R(x - y),
            Bin::Mul => R(x * y),
            Bin::Div => R(x / y),
            Bin::Lt => B(x < y),
            Bin::Le => B(x <= y),
            Bin::Gt => B(x > y),
            Bin::Ge => B(x >= y),
            Bin::Eq => B(x == y),
            Bin::Ne => B(x != y),
            Bin::Cplx => C(x, y),
            _ => unreachable!(),
        },
        (B(x), B(y)) => match op {
            Bin::And => B(x & y),
            Bin::Or => B(x | y),
            Bin::Xor | Bin::Ne => B(x ^ y),
            Bin::Eq => B(x == y),
            _ => unreachable!(),
        },
        (C(ar, ai), C(br, bi)) => match op {
            Bin::Add => C(ar + br, ai + bi),
            Bin::Sub => C(ar - br, ai - bi),
            Bin::Mul => {
                let (r, i) = cmul((ar, ai), (br, bi));
                C(r, i)
            }
            Bin::Div => {
                let d = br * br + bi * bi;
                C((ar * br + ai * bi) / d, (ai * br - ar * bi) / d)
            }
            _ => unreachable!(),
        },
        _ => unreachable!("type-checked"),
    })
}

fn powi(a: Num, n: u32) -> Num {
    use Num::*;
    if n == 0 {
        return match a {
            I(_) => I(1),
            R(_) => R(1.0),
            _ => C(1.0, 0.0),
        };
    }
    let mul = |x: Num, y: Num| match (x, y) {
        (I(p), I(q)) => I(p.wrapping_mul(q)),
        (R(p), R(q)) => R(p * q),
        (C(a, b), C(c, d)) => {
            let (r, i) = cmul((a, b), (c, d));
            C(r, i)
        }
        _ => unreachable!(),
    };
    let mut r = a;
    let top = 31 - n.leading_zeros();
    for bit in (0..top).rev() {
        r = mul(r, r);
        if n >> bit & 1 == 1 {
            r = mul(r, a);
        }
    }
    r
}

struct Conv {
    syms: std::collections::HashMap<String, Term>,
    vars: Vec<Term>,
}

impl Conv {
    fn s(&self, k: &str) -> Term {
        self.syms[k]
    }
    fn expr(&mut self, st: &mut Store, e: &E) -> Term {
        match e {
            E::I(n) => st.int(*n),
            E::F(x) => st.real(*x),
            E::B(b) => self.s(if *b { "True" } else { "False" }),
            E::Var(x) => self.vars[*x as usize],
            E::Un(op, a) => {
                let a = self.expr(st, a);
                st.app(self.s(&format!("un:{op:?}")), &[a])
            }
            E::Bin(Bin::Cplx, a, b) => {
                let (a, b) = (self.expr(st, a), self.expr(st, b));
                st.app(self.s("complex"), &[a, b])
            }
            E::Bin(op, a, b) => {
                let (a, b) = (self.expr(st, a), self.expr(st, b));
                st.app(self.s(&format!("bin:{op:?}")), &[a, b])
            }
            E::PowI(a, n) => {
                let a = self.expr(st, a);
                let n = st.int(*n as i64);
                st.app(self.s("powi"), &[a, n])
            }
            E::Call(m, args) => {
                let a: Vec<Term> = args.iter().map(|x| self.expr(st, x)).collect();
                st.app(self.s(&format!("math:{m:?}")), &a)
            }
            E::Sel(c, a, b) => {
                let (c, a, b) = (self.expr(st, c), self.expr(st, a), self.expr(st, b));
                st.app(self.s("select"), &[c, a, b])
            }
            E::Load(a, idx) => {
                let ix = self.expr(st, &idx[0]);
                st.app(self.s("part"), &[self.vars[*a as usize], ix])
            }
            E::Len(a, _) => st.app(self.s("length"), &[self.vars[*a as usize]]),
        }
    }
    fn block(&mut self, st: &mut Store, b: &[S]) -> Term {
        let items: Vec<Term> = b.iter().map(|s| self.stmt(st, s)).collect();
        st.app(self.s("block"), &items)
    }
    fn stmt(&mut self, st: &mut Store, s: &S) -> Term {
        match s {
            S::Set(x, e) => {
                let e = self.expr(st, e);
                st.app(self.s("set"), &[self.vars[*x as usize], e])
            }
            S::Store(a, idx, e) => {
                let ix = self.expr(st, &idx[0]);
                let e = self.expr(st, e);
                st.app(self.s("setpart"), &[self.vars[*a as usize], ix, e])
            }
            S::If(c, t, e) => {
                let c = self.expr(st, c);
                let (t, e) = (self.block(st, t), self.block(st, e));
                st.app(self.s("if"), &[c, t, e])
            }
            S::While(c, b) => {
                let c = self.expr(st, c);
                let b = self.block(st, b);
                st.app(self.s("while"), &[c, b])
            }
            S::For(x, lo, hi, b) => {
                let (lo, hi) = (self.expr(st, lo), self.expr(st, hi));
                let b = self.block(st, b);
                st.app(self.s("for"), &[self.vars[*x as usize], lo, hi, b])
            }
        }
    }
}
