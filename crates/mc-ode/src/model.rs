//! The right-hand side `F(t, Y)` of the first-order system at machine reals, with its Jacobian `dF/dY` and time
//! derivative `dF/dt` for the stiff method: all three are expressions differentiated once by `d` and evaluated
//! through `mc_numfn::compile_expr` (compiled by `mc-jit` when the policy says, the interpreter for every refused
//! or declined call, decision NUM.2). Without a symbolic derivative the Jacobian is taken by forward differences.
use crate::system::System;
use crate::util::mentions;
use mc_core::{Heap, Store, T};
use mc_eval::{NativeCx, Res};
use mc_numfn::{NumFn, Policy, Var};

/// A function of `(t, y1, ..., yn)`.
type Fun<'h> = Box<dyn NumFn<'h, f64> + 'h>;

/// The right-hand side and its derivatives.
pub(crate) struct Model<'h> {
    /// the size of the state
    pub n: usize,
    f: Fun<'h>,
    /// what the Jacobian and dF/dt are made from, on the first use (`None` once made)
    pending: Option<Pending<'h>>,
    jac: Option<Fun<'h>>,
    ft: Option<Fun<'h>>,
    /// the right-hand side does not depend on t
    autonomous: bool,
    args: Vec<f64>,
    tmp: Vec<f64>,
    /// right-hand side evaluations (finite differences included)
    pub evals: u64,
    /// Jacobian evaluations
    pub jacobians: u64,
}

/// The variables of the compiled functions: `t`, then the state symbols.
pub(crate) fn vars(sys: &System<'_>) -> Vec<Var> {
    std::iter::once(sys.t).chain(sys.state.iter().copied()).map(Var::scalar).collect()
}

/// `d(f, spec)` through the evaluator; `None` when `d` cannot differentiate (the result still contains `d` or
/// `derivative`, or is a failure).
pub(crate) fn derivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, spec: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let r = cx.eval(h.call(h.sym("d"), &[f, spec]))?;
    if crate::util::is_failure(h, r) || mentions(cx, r, &[h.sym("d"), h.sym("derivative")])? {
        return Ok(None);
    }
    Ok(Some(r))
}

/// A function of the state's variables with `outputs` scalar outputs.
pub(crate) fn function<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    body: T<'h>,
    vars: &[Var],
    outputs: usize,
    policy: Policy,
) -> Res<Fun<'h>> {
    let opts = mc_numfn::Options { policy, numeric: true, outputs: Some(outputs) };
    mc_numfn::compile_expr(cx, body, vars, &opts)
}

/// The right-hand side and its variables, kept until the stiff method first needs the derivatives.
struct Pending<'h> {
    rhs: T<'h>,
    vars: Vec<Var>,
    policy: Policy,
}

impl<'h> Model<'h> {
    /// The model of `sys`, its right-hand side compiled as `policy` says.
    pub fn new(cx: &mut NativeCx<'_, '_, 'h>, sys: &System<'h>, policy: Policy) -> Res<Model<'h>> {
        let n = sys.n();
        let vars = vars(sys);
        let f = function(cx, sys.rhs, &vars, n, policy)?;
        let autonomous = !mentions(cx, sys.rhs, &[sys.t])?;
        Ok(Model {
            n,
            f,
            pending: Some(Pending { rhs: sys.rhs, vars, policy }),
            jac: None,
            ft: None,
            autonomous,
            args: vec![0.0; n + 1],
            tmp: vec![0.0; n],
            evals: 0,
            jacobians: 0,
        })
    }

    /// The Jacobian `d(F, [s1, ..., sn])` and `d(F, t)`, symbolically by `d` (once, when first needed); either is
    /// left out when `d` cannot differentiate.
    fn derivatives(&mut self, cx: &mut NativeCx<'_, '_, 'h>) -> Res<()> {
        let Some(p) = self.pending.take() else { return Ok(()) };
        let h: &'h Heap = cx.heap();
        let n = self.n;
        let states: Vec<T<'h>> = p.vars[1..].iter().map(|v| h.sym_term(v.sym)).collect();
        let spec = cx.list(&states);
        if let Some(j) = derivative(cx, p.rhs, spec)? {
            self.jac = Some(function(cx, j, &p.vars, n * n, p.policy)?);
        }
        if !self.autonomous {
            if let Some(d) = derivative(cx, p.rhs, h.sym_term(p.vars[0].sym))? {
                self.ft = Some(function(cx, d, &p.vars, n, p.policy)?);
            }
        }
        Ok(())
    }

    fn load(&mut self, t: f64, y: &[f64]) {
        self.args[0] = t;
        self.args[1..].copy_from_slice(y);
    }

    /// `F(t, y)` into `out`; `false` when a value is not a finite real.
    pub fn f(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: f64, y: &[f64], out: &mut [f64]) -> Res<bool> {
        self.evals += 1;
        self.load(t, y);
        self.f.eval(cx, &self.args, out)
    }

    /// The Jacobian `dF/dY` (row-major) and `dF/dt` at `(t, y)`, where `fy = F(t, y)`; `false` when a value is not
    /// a finite real.
    pub fn jacobian(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        t: f64,
        y: &[f64],
        fy: &[f64],
        jac: &mut [f64],
        ft: &mut [f64],
    ) -> Res<bool> {
        self.derivatives(cx)?;
        self.jacobians += 1;
        let n = self.n;
        self.load(t, y);
        let symbolic = match self.jac.as_mut() {
            Some(j) => j.eval(cx, &self.args, jac)?,
            None => false,
        };
        if !symbolic {
            // forward differences, column by column (Hairer's increments); also where the symbolic Jacobian is
            // not numeric (a derivative singular at the point)
            let mut yp = y.to_vec();
            let mut col = std::mem::take(&mut self.tmp);
            for k in 0..n {
                let delta = (f64::EPSILON * y[k].abs().max(1e-5)).sqrt();
                yp[k] = y[k] + delta;
                let ok = self.f(cx, t, &yp, &mut col)?;
                yp[k] = y[k];
                if !ok {
                    self.tmp = col;
                    return Ok(false);
                }
                for i in 0..n {
                    jac[i * n + k] = (col[i] - fy[i]) / delta;
                }
            }
            self.tmp = col;
        }
        if self.autonomous {
            ft.fill(0.0);
            return Ok(true);
        }
        self.load(t, y);
        let symbolic = match self.ft.as_mut() {
            Some(d) => d.eval(cx, &self.args, ft)?,
            None => false,
        };
        if symbolic {
            return Ok(true);
        }
        let delta = (f64::EPSILON * t.abs().max(1e-5)).sqrt();
        let mut col = std::mem::take(&mut self.tmp);
        let ok = self.f(cx, t + delta, y, &mut col)?;
        for i in 0..n {
            ft[i] = (col[i] - fy[i]) / delta;
        }
        self.tmp = col;
        Ok(ok)
    }
}
