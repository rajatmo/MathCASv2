//! The integration loop of the machine methods: step size selection up to the end point, the method choice and
//! the automatic switch to the stiff method, events, the step limit, and fuel and interrupt polling (one unit per
//! attempted step besides the evaluations' own).
use crate::dense::{self, Steps};
use crate::dopri::Dopri;
use crate::events::Events;
use crate::model::Model;
use crate::options::{EventAction, Method, Tolerance};
use crate::rodas::Rodas;
use crate::step::{initial_step, Attempt, Stepper};
use mc_eval::{NativeCx, Res};

/// The settings of one integration.
pub(crate) struct Settings {
    /// `Auto`, `Rk45` or `Stiff`
    pub method: Method,
    /// the error tolerances
    pub tol: Tolerance,
    /// the largest step size (unbounded: the length of the interval)
    pub hmax: Option<f64>,
    /// the step limit
    pub max_steps: usize,
}

/// How an integration ended.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum End {
    /// at the end point
    Done,
    /// an event stopped it
    Event,
    /// the step limit was reached
    MaxSteps,
    /// the step size fell below the resolution of the time (a singularity, or a tolerance too tight)
    StepSize,
    /// the right-hand side (or an event function) is not a finite real at this time and state
    NotNumeric(f64, Vec<f64>),
}

/// The result of one integration.
pub(crate) struct Run {
    /// the accepted steps in the order taken
    pub steps: Steps,
    /// the time reached
    pub t: f64,
    /// how it ended
    pub end: End,
    /// rejected attempts
    pub rejected: u64,
    /// where the automatic method switched to the stiff method
    pub switched: Option<f64>,
}

/// Halvings of the step size tried when a stage is not numeric (it may have left the domain of F).
const NOT_NUMERIC_RETRIES: u32 = 8;

/// Integrate `m` from `(t0, y0)` to `t1` (in either direction).
pub(crate) fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    m: &mut Model<'h>,
    mut events: Option<&mut Events<'h>>,
    t0: f64,
    y0: &[f64],
    t1: f64,
    s: &Settings,
) -> Res<Run> {
    let n = m.n;
    let mut run = Run { steps: Steps::default(), t: t0, end: End::Done, rejected: 0, switched: None };
    let mut y = y0.to_vec();
    if t1 == t0 {
        return Ok(run);
    }
    let dir = (t1 - t0).signum();
    let mut stepper: Box<dyn Stepper<'h>> =
        if s.method == Method::Stiff { Box::new(Rodas::new(n)) } else { Box::new(Dopri::new(n)) };
    let mut stiff = s.method == Method::Stiff;
    if !stepper.start(cx, m, t0, &y)? {
        run.end = End::NotNumeric(t0, y);
        return Ok(run);
    }
    if let Some(ev) = events.as_deref_mut() {
        if !ev.start(cx, t0, &y)? {
            run.end = End::NotNumeric(t0, y);
            return Ok(run);
        }
    }
    let hmax = s.hmax.unwrap_or(f64::INFINITY).min((t1 - t0).abs());
    let f0 = stepper.f0().to_vec();
    let mut h = initial_step(cx, m, t0, &y, &f0, stepper.order(), dir, hmax, &s.tol)?;
    let mut t = t0;
    let mut retries = 0;
    let mut coefs = Vec::with_capacity(dense::COEFS * n);
    loop {
        cx.tick(1)?;
        if run.steps.len() >= s.max_steps {
            run.end = End::MaxSteps;
            break;
        }
        let rest = t1 - t;
        let last = (1.01 * h).abs() >= rest.abs();
        if last {
            h = rest;
        }
        if h == 0.0 || h.abs() <= 16.0 * f64::EPSILON * t.abs() {
            run.end = End::StepSize;
            break;
        }
        match stepper.attempt(cx, m, t, &y, h, &s.tol)? {
            Attempt::Rejected { h_next } => {
                run.rejected += 1;
                h = h_next;
            }
            Attempt::NotNumeric(tp, yp) => {
                if retries >= NOT_NUMERIC_RETRIES {
                    run.end = End::NotNumeric(tp, yp);
                    break;
                }
                retries += 1;
                run.rejected += 1;
                h *= 0.25;
            }
            Attempt::Accepted { h_next, stiff: found_stiff } => {
                retries = 0;
                coefs.clear();
                stepper.accept(&mut y, h, &mut coefs);
                run.steps.push(t, h, &coefs);
                if let Some(ev) = events.as_deref_mut() {
                    match ev.check(cx, t, h, &coefs, &y)? {
                        Err(tp) => {
                            run.end = End::NotNumeric(tp, y);
                            break;
                        }
                        Ok(hits) => {
                            if ev.action == EventAction::Stop {
                                if let Some(hit) = hits.first() {
                                    run.steps.truncate_last(n, hit.theta);
                                    run.t = t + hit.theta * h;
                                    run.end = End::Event;
                                    return Ok(run);
                                }
                            }
                        }
                    }
                }
                t = if last { t1 } else { t + h };
                run.t = t;
                if last {
                    break;
                }
                if found_stiff && !stiff && s.method == Method::Auto {
                    // stiffness detected: the rest with the stiff method (decision N3.8)
                    stiff = true;
                    run.switched = Some(t);
                    stepper = Box::new(Rodas::new(n));
                }
                if !stepper.start(cx, m, t, &y)? {
                    run.end = End::NotNumeric(t, y);
                    break;
                }
                h = dir * h_next.abs().min(hmax);
            }
        }
    }
    Ok(run)
}
