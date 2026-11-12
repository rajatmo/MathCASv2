//! Products and constructors: `dot`, `cross`, `kronecker_product`, `trace`, `identity_matrix`, `diagonal_matrix`,
//! `norm`, and the vector calculus `grad`, `div`, `curl`, `laplacian` (terms of `d` that the evaluator evaluates,
//! so this crate does not link the calculus).
use crate::decomp;
use crate::dense::{self, Mat};
use crate::natives::Out;
use crate::scalar::Scalar;
use crate::systems::{num_mul, num_terms, square_arg, take, tensor_arg};
use crate::tensor::{Arr, Domain, Env, Shape};
use mc_api::{names, Store, View};
use mc_core::T;
use mc_poly::work::{Halt, Meter, WorkResult};

/// The most entries of a matrix the constructors build (the lists library's limit on built lists): beyond it they
/// give `failure(limit, memory)` instead of allocating gigabytes.
const MAX_ENTRIES: usize = 1 << 26;

/// `identity_matrix(n)`: the n x n identity of exact integers; `identity_matrix([m, n])` the m x n one;
/// `working_precision: d` ones of d digits.
pub fn identity_matrix<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    let all = args;
    let (args, opts) = options(env, args);
    let Some(opts) = opts else { return Ok(Out::Decline) };
    let [n] = args else { return Ok(Out::Decline) };
    // n, or [rows, cols] for a rectangular matrix with ones on the main diagonal
    let (r, c) = match env.as_list(*n) {
        Some(&[r, c]) => match (env.int(r), env.int(c)) {
            (Some(r), Some(c)) => (r, c),
            _ => return Ok(Out::Done(env.bad_argument(1))),
        },
        _ => match env.int(*n) {
            Some(n) => (n, n),
            None => return Ok(Out::Done(env.bad_argument(1))),
        },
    };
    if !(0..=1 << 14).contains(&r) || !(0..=1 << 14).contains(&c) {
        return Ok(Out::Done(env.bad_argument(1)));
    }
    let (r, c) = (r as usize, c as usize);
    if r == 0 {
        return Ok(Out::Done(env.list(&[])));
    }
    if r * c > MAX_ENTRIES {
        // with a target structure the call stays (a structured form is not built), else the limit failure
        if args.len() < all.len() {
            return Ok(Out::Decline);
        }
        return Err(Halt::Memory);
    }
    if let Some(bits) = opts.precision_bits {
        // working_precision: d: ones of d digits, exact zeros
        let one = env.num(&mc_arith::Num::int(1).to_kind(mc_arith::Kind::Ball, bits));
        let zero = env.h().int(0);
        let mut out = vec![zero; r * c];
        for i in 0..r.min(c) {
            out[i * c + i] = one;
        }
        return Ok(Out::Done(env.matrix(r, c, &out)));
    }
    let mut a = vec![0i64; r * c];
    for i in 0..r.min(c) {
        a[i * c + i] = 1;
    }
    Ok(Out::Done(env.h().int_array(&[r, c], &a)))
}

/// The options of the matrix constructors: `target_structure: "Dense" | "Sparse" | automatic` (all give the
/// dense list form; `"Structured"` and other values are declined) and `working_precision: d`.
pub(crate) struct CtorOptions {
    pub(crate) precision_bits: Option<u64>,
}

/// The arguments without a trailing `options(...)`, and the options (`None`: an option this function declines).
pub(crate) fn options<'a, 'h>(env: &Env<'h>, args: &'a [T<'h>]) -> (&'a [T<'h>], Option<CtorOptions>) {
    let h = env.h();
    let mut o = CtorOptions { precision_bits: None };
    let Some((&last, rest)) = args.split_last() else { return (args, Some(o)) };
    let View::App { head, args: rules } = h.view(last) else { return (args, Some(o)) };
    if head != env.sym("options") {
        return (args, Some(o));
    }
    for &r in rules {
        let View::App { args: &[k, v], .. } = h.view(r) else { return (rest, None) };
        let View::Sym(name) = h.view(k) else { return (rest, None) };
        match h.sym_name(name).as_str() {
            "target_structure" => {
                let ok = match h.view(v) {
                    View::Str(s) => matches!(s, "Dense" | "Sparse"),
                    View::Sym(s) => matches!(h.sym_name(s).as_str(), "automatic" | "auto"),
                    _ => false,
                };
                if !ok {
                    return (rest, None);
                }
            }
            "working_precision" => match env.int(v) {
                Some(d @ 1..=100_000) => o.precision_bits = Some(mc_arith::text::digits_to_bits(d as u64)),
                _ => return (rest, None),
            },
            _ => return (rest, None),
        }
    }
    (rest, Some(o))
}

/// `diagonal_matrix(v)`, `diagonal_matrix(v, k)`, `diagonal_matrix(v, k, n)`, `diagonal_matrix(v, k, [m, n])`: the
/// matrix with `v` on its k-th diagonal (above the main one for k > 0, below for k < 0) and exact zeros elsewhere;
/// square of the size v needs unless a size is given (v then cut or padded with zeros).
pub fn diagonal_matrix<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    let (args, opts) = options(env, args);
    if opts.is_none_or(|o| o.precision_bits.is_some()) {
        return Ok(Out::Decline);
    }
    let (v, k, size) = match args {
        [v] => (*v, 0, None),
        [v, k] | [v, k, _] => match env.int(*k) {
            Some(k) if k.unsigned_abs() <= 1 << 14 => (*v, k, args.get(2).copied()),
            _ => return Ok(Out::Done(env.bad_argument(2))),
        },
        _ => return Ok(Out::Decline),
    };
    let a = take!(tensor_arg(env, &[v], 0));
    if a.is_matrix() {
        return Ok(Out::Done(env.bad_argument(1)));
    }
    let xs = a.terms(env);
    let natural = xs.len() + k.unsigned_abs() as usize;
    let (rows, cols) = match size {
        None => (natural, natural),
        Some(s) => match env.as_list(s) {
            Some(&[m, n]) => match (env.int(m), env.int(n)) {
                (Some(m @ 0..), Some(n @ 0..)) => (m as usize, n as usize),
                _ => return Ok(Out::Done(env.bad_argument(3))),
            },
            _ => match env.int(s) {
                Some(n @ 0..) => (n as usize, n as usize),
                _ => return Ok(Out::Done(env.bad_argument(3))),
            },
        },
    };
    if rows.checked_mul(cols).is_none_or(|c| c > MAX_ENTRIES) {
        return Err(Halt::Memory);
    }
    let zero = env.h().int(0);
    let mut out = vec![zero; rows * cols];
    for (i, &x) in xs.iter().enumerate() {
        let (r, c) = if k >= 0 { (i, i + k as usize) } else { (i + k.unsigned_abs() as usize, i) };
        if r < rows && c < cols {
            out[r * cols + c] = x;
        }
    }
    Ok(Out::Done(env.matrix(rows, cols, &out)))
}

/// The product of two tensors as a term, or the reply refusing them.
fn dot2<'h>(env: &Env<'h>, a: &Arr<'h>, b: &Arr<'h>, m: &mut Meter) -> WorkResult<Result<T<'h>, Out<'h>>> {
    // a vector on the left is a row, on the right a column
    let (r, n) = (a.rows(), a.cols());
    let (n2, c) = match b.shape {
        Shape::Vector(k) => (k, 1),
        Shape::Matrix(k, c) => (k, c),
    };
    if n != n2 {
        return Ok(Err(Out::Done(env.failure("shape_mismatch", &[]))));
    }
    let out_shape = match (a.shape, b.shape) {
        (Shape::Vector(_), Shape::Vector(_)) => None,
        (Shape::Vector(_), Shape::Matrix(..)) => Some((false, c)),
        (Shape::Matrix(..), Shape::Vector(_)) => Some((false, r)),
        (Shape::Matrix(..), Shape::Matrix(..)) => Some((true, 0)),
    };
    let finish = |terms: Vec<T<'h>>| -> T<'h> {
        match out_shape {
            None => terms[0],
            Some((false, _)) => env.list(&terms),
            Some((true, _)) => env.matrix(r, c, &terms),
        }
    };
    let (br, bc) = (n2, c);
    Ok(Ok(match a.domain.join(b.domain) {
        Domain::Real => {
            let am = a.reals(env);
            let am = Mat { rows: r, cols: n, a: am.a };
            let bmat = Mat { rows: br, cols: bc, a: b.reals(env).a };
            let p = dense::mul(&am, &bmat, m)?;
            match out_shape {
                None => env.h().real(p.a[0]),
                Some((false, _)) => env.real_vector(&p.a),
                Some((true, _)) => env.real_matrix(&p),
            }
        }
        Domain::Complex => {
            let am = Mat { rows: r, cols: n, a: a.complex(env).a };
            let bmat = Mat { rows: br, cols: bc, a: b.complex(env).a };
            let p = dense::mul(&am, &bmat, m)?;
            finish(p.a.iter().map(|&z| env.cx(z)).collect())
        }
        Domain::Rational | Domain::ExactComplex | Domain::Ball => {
            let p = num_mul(&a.nums(env), &b.nums(env), r, n, bc, m)?;
            finish(num_terms(env, &p))
        }
        Domain::Symbolic => {
            let (at, bt) = (a.terms(env), b.terms(env));
            let mut out = Vec::with_capacity(r * bc);
            for i in 0..r {
                m.step((n * bc) as u64 + 1)?;
                for j in 0..bc {
                    let prods: Vec<T<'h>> = (0..n).map(|k| env.ar.mul(&[at[i * n + k], bt[k * bc + j]])).collect();
                    out.push(env.ar.add(&prods));
                }
            }
            finish(out)
        }
    }))
}

/// `dot(a, b, ...)`: the product of vectors and matrices (scalar product of two vectors, matrix times vector,
/// vector times matrix, matrix product), folded from the left. Entries are not conjugated.
pub fn dot<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if args.is_empty() {
        return Ok(Out::Decline);
    }
    let mut acc = take!(tensor_arg(env, args, 0));
    let mut val = args[0];
    for k in 1..args.len() {
        let b = take!(tensor_arg(env, args, k));
        val = match dot2(env, &acc, &b, m)? {
            Ok(t) => t,
            Err(out) => return Ok(out),
        };
        if k + 1 < args.len() {
            acc = match env.read(val) {
                Some(a) => a,
                // a scalar product in the middle of a chain
                None => return Ok(Out::Done(env.failure("shape_mismatch", &[]))),
            };
        }
    }
    Ok(Out::Done(val))
}

/// `cross(u, v)`: the cross product of two vectors of length 3.
pub fn cross<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    if args.len() != 2 {
        return Ok(Out::Decline);
    }
    let u = take!(tensor_arg(env, args, 0));
    let v = take!(tensor_arg(env, args, 1));
    for (k, x) in [&u, &v].iter().enumerate() {
        if x.is_matrix() || x.cols() != 3 {
            return Ok(Out::Done(env.bad_argument(k + 1)));
        }
    }
    if u.domain.join(v.domain) == Domain::Real {
        let (a, b) = (u.reals(env).a, v.reals(env).a);
        let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        return Ok(Out::Done(env.real_vector(&c)));
    }
    let (a, b) = (u.terms(env), v.terms(env));
    let ar = &env.ar;
    let comp = |i: usize, j: usize| ar.sub(ar.mul(&[a[i], b[j]]), ar.mul(&[a[j], b[i]]));
    Ok(Out::Done(env.list(&[comp(1, 2), comp(2, 0), comp(0, 1)])))
}

/// `kronecker_product(a, b, ...)`: the Kronecker product of matrices (or of vectors), folded from the left.
pub fn kronecker_product<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if args.is_empty() {
        return Ok(Out::Decline);
    }
    let mut acc = take!(tensor_arg(env, args, 0));
    let mut val = args[0];
    for k in 1..args.len() {
        let b = take!(tensor_arg(env, args, k));
        if acc.is_matrix() != b.is_matrix() {
            return Ok(Out::Done(env.failure("shape_mismatch", &[])));
        }
        let (ra, ca, rb, cb) = (acc.rows(), acc.cols(), b.rows(), b.cols());
        let (r, c) = (ra * rb, ca * cb);
        if r.checked_mul(c).is_none_or(|e| e > MAX_ENTRIES) {
            return Err(Halt::Memory);
        }
        m.step((r * c) as u64 + 1)?;
        let index = |i: usize, j: usize| ((i / rb) * ca + j / cb, (i % rb) * cb + j % cb);
        val = if acc.domain.join(b.domain) == Domain::Real {
            let (x, y) = (acc.reals(env).a, b.reals(env).a);
            let out: Vec<f64> = (0..r).flat_map(|i| (0..c).map(move |j| (i, j))).map(|(i, j)| {
                let (p, q) = index(i, j);
                x[p] * y[q]
            }).collect();
            if acc.is_matrix() { env.real_matrix(&Mat { rows: r, cols: c, a: out }) } else { env.real_vector(&out) }
        } else {
            let (x, y) = (acc.terms(env), b.terms(env));
            let out: Vec<T<'h>> = (0..r).flat_map(|i| (0..c).map(move |j| (i, j))).map(|(i, j)| {
                let (p, q) = index(i, j);
                env.ar.mul(&[x[p], y[q]])
            }).collect();
            if acc.is_matrix() { env.matrix(r, c, &out) } else { env.list(&out) }
        };
        if k + 1 < args.len() {
            // invariant: the product of two tensors read above, built as a matrix or a list of the right shape
            acc = env.read(val).expect("a product of tensors is a tensor");
        }
    }
    Ok(Out::Done(val))
}

/// `trace(a)`, `trace(a, f)`, `trace(a, f, n)`: `f` (default `add`) applied to the entries `a[i][i]...[i]` down the
/// first `n` levels (default: all of them) of an array, as many of them as the smallest of those dimensions: the
/// sum of the diagonal of a matrix (square or not), the sum of a vector.
pub fn trace<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    if let [_] = args {
        // a square matrix: the machine path
        if let Ok(a) = square_arg(env, args) {
            let n = a.rows();
            if a.domain == Domain::Real {
                let x = a.reals(env);
                return Ok(Out::Done(env.h().real((0..n).map(|i| x.at(i, i)).sum())));
            }
            let t = a.terms(env);
            let diag: Vec<T<'h>> = (0..n).map(|i| t[i * n + i]).collect();
            return Ok(Out::Done(env.ar.add(&diag)));
        }
    }
    let (x, f, level) = match args {
        [x] => (*x, None, None),
        [x, f] => (*x, Some(*f), None),
        [x, f, n] => match env.int(*n) {
            Some(n) if n >= 1 => (*x, Some(*f), Some(n as usize)),
            _ => return Ok(Out::Done(env.bad_argument(3))),
        },
        _ => return Ok(Out::Decline),
    };
    // the dimensions down the nested lists, as far as they are rectangular
    let mut dims = vec![];
    let mut probe = x;
    while let Some(items) = env.as_list(probe) {
        if items.is_empty() {
            break;
        }
        dims.push(items.len());
        probe = items[0];
    }
    if dims.is_empty() {
        return Ok(Out::Decline);
    }
    let level = level.unwrap_or(dims.len());
    if level > dims.len() {
        return Ok(Out::Done(env.bad_argument(3)));
    }
    let count = dims[..level].iter().copied().min().unwrap_or(0);
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let mut t = x;
        for _ in 0..level {
            match env.as_list(t).and_then(|items| items.get(i)) {
                Some(&u) => t = u,
                None => return Ok(Out::Decline),
            }
        }
        entries.push(t);
    }
    let is_list_head = |f: T<'h>| matches!(env.h().view(f), View::Sym(s) if env.h().sym_name(s) == "list");
    Ok(match f {
        None => Out::Again(env.ar.add(&entries)),
        Some(f) if is_list_head(f) => Out::Done(env.list(&entries)),
        Some(f) => Out::Again(env.h().app(f, &entries)),
    })
}

// The order of a norm.
#[derive(Clone, Copy, PartialEq)]
enum Order {
    P(i64),
    Inf,
    Frobenius,
}

/// `norm(v)`, `norm(v, p)` (p a positive integer or `inf`); `norm(m)` (the 2-norm, the largest singular value),
/// `norm(m, 1)`, `norm(m, inf)`, `norm(m, "frobenius")`.
pub fn norm<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    // a scalar (a number, a quantity, an expression): its absolute value
    if let [x] = args {
        if env.as_list(*x).is_none() && env.h().packed(*x).is_none() {
            return Ok(Out::Again(env.call("abs", &[*x])));
        }
    }
    // the p-norm of a vector for any real p >= 1 or a symbolic p: (sum |x_k|^p)^(1/p) (round 37)
    if let [_, p] = args {
        let general = match env.h().view(*p) {
            View::Real(x) => x.is_finite() && x >= 1.0,
            View::Rational { .. } => env.ar.num(*p).is_some_and(|q| q.as_real().is_some_and(|r| r.to_f64() >= 1.0)),
            View::Sym(s) => env.h().sym_name(s) != "inf",
            View::App { .. } => true,
            _ => false,
        };
        if general {
            let a = take!(tensor_arg(env, args, 0));
            if a.is_matrix() {
                return Ok(Out::Decline);
            }
            let h = env.h();
            let xs = a.terms(env);
            let sum = env.call(names::ADD, &xs.iter().map(|&x| env.call(names::POW, &[env.call("abs", &[x]), *p])).collect::<Vec<_>>());
            return Ok(Out::Again(env.call(names::POW, &[sum, env.call(names::POW, &[*p, h.int(-1)])])));
        }
    }
    let order = match args {
        [_] => Order::P(2),
        [_, p] => match env.h().view(*p) {
            View::Int(k) if k >= 1 => Order::P(k),
            View::Sym(s) if env.h().sym_name(s) == "inf" => Order::Inf,
            // the Frobenius norm, by its name in any letter case
            View::Str(s) if s.eq_ignore_ascii_case("frobenius") => Order::Frobenius,
            _ => return Ok(Out::Done(env.bad_argument(2))),
        },
        _ => return Ok(Out::Decline),
    };
    let a = take!(tensor_arg(env, args, 0));
    let machine = a.domain.machine();
    let abs = |x: T<'h>| env.call("abs", &[x]);
    let h = env.h();
    if !a.is_matrix() || order == Order::Frobenius {
        let order = if order == Order::Frobenius { Order::P(2) } else { order };
        if machine {
            let v = a.complex(env).a;
            let r = match order {
                Order::Inf => v.iter().fold(0.0f64, |s, z| s.max(z.abs())),
                Order::P(2) => Mat { rows: 1, cols: v.len(), a: v }.frobenius(),
                Order::P(p) => v.iter().map(|z| z.abs().powi(p as i32)).sum::<f64>().powf(1.0 / p as f64),
                // invariant: Frobenius was mapped to P(2) above
                Order::Frobenius => unreachable!("mapped to 2"),
            };
            return Ok(Out::Done(h.real(r)));
        }
        let xs = a.terms(env);
        let t = match order {
            Order::Inf => env.call("max", &xs.iter().map(|&x| abs(x)).collect::<Vec<_>>()),
            Order::P(p) => {
                let pt = h.int(p);
                let sum = env.call(names::ADD, &xs.iter().map(|&x| env.call(names::POW, &[abs(x), pt])).collect::<Vec<_>>());
                env.call(names::POW, &[sum, env.ar.rational(h.int(1), pt)])
            }
            // invariant: Frobenius was mapped to P(2) above
            Order::Frobenius => unreachable!("mapped to 2"),
        };
        return Ok(Out::Again(t));
    }
    let (r, c) = (a.rows(), a.cols());
    match order {
        Order::P(2) => {
            if machine {
                let (_, s, _) = dense::svd(&a.complex(env), m)?;
                return Ok(Out::Done(h.real(s.first().copied().unwrap_or(0.0))));
            }
            decomp::largest_singular_value(env, &a, m)
        }
        Order::P(1) | Order::Inf => {
            let by_rows = order == Order::Inf;
            let (outer, inner) = if by_rows { (r, c) } else { (c, r) };
            let at = |i: usize, j: usize| if by_rows { i * c + j } else { j * c + i };
            if machine {
                let v = a.complex(env).a;
                let best = (0..outer).map(|i| (0..inner).map(|j| v[at(i, j)].abs()).sum::<f64>()).fold(0.0f64, f64::max);
                return Ok(Out::Done(h.real(best)));
            }
            let xs = a.terms(env);
            let sums: Vec<T<'h>> = (0..outer).map(|i| env.call(names::ADD, &(0..inner).map(|j| abs(xs[at(i, j)])).collect::<Vec<_>>())).collect();
            Ok(Out::Again(env.call("max", &sums)))
        }
        _ => Ok(Out::Done(env.bad_argument(2))),
    }
}

/// The variables argument (a list).
fn vars_arg<'h>(env: &Env<'h>, args: &[T<'h>]) -> Result<&'h [T<'h>], Out<'h>> {
    env.as_list(args[1]).ok_or_else(|| Out::Done(env.bad_argument(2)))
}

/// `d(f, x)`.
fn d<'h>(env: &Env<'h>, f: T<'h>, x: T<'h>) -> T<'h> {
    env.call("d", &[f, x])
}

/// `grad(f, [x1, ..., xn])`: the gradient `[d(f, x1), ...]`; for a list `f`, the Jacobian matrix.
pub fn grad<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    if args.len() != 2 {
        return Ok(Out::Decline);
    }
    let vars = take!(vars_arg(env, args));
    let one = |f: T<'h>| env.list(&vars.iter().map(|&x| d(env, f, x)).collect::<Vec<_>>());
    Ok(Out::Again(match env.as_list(args[0]) {
        Some(fs) => env.list(&fs.iter().map(|&f| one(f)).collect::<Vec<_>>()),
        None => one(args[0]),
    }))
}

/// `div(v, [x1, ..., xn])`: the divergence `d(v1, x1) + ... + d(vn, xn)`.
pub fn div<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    if args.len() != 2 {
        return Ok(Out::Decline);
    }
    let vars = take!(vars_arg(env, args));
    let Some(v) = env.as_list(args[0]) else { return Ok(Out::Done(env.bad_argument(1))) };
    if v.len() != vars.len() {
        return Ok(Out::Done(env.failure("shape_mismatch", &[])));
    }
    let terms: Vec<T<'h>> = v.iter().zip(vars).map(|(&f, &x)| d(env, f, x)).collect();
    Ok(Out::Again(env.call(names::ADD, &terms)))
}

/// `curl(v, [x, y, z])`: the curl of a field in three dimensions; `curl([v1, v2], [x, y])` the scalar curl
/// `d(v2, x) - d(v1, y)`.
pub fn curl<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    if args.len() != 2 {
        return Ok(Out::Decline);
    }
    let vars = take!(vars_arg(env, args));
    let Some(v) = env.as_list(args[0]) else { return Ok(Out::Done(env.bad_argument(1))) };
    if v.len() != vars.len() || !(2..=3).contains(&v.len()) {
        if v.len() == vars.len() && (4..=7).contains(&v.len()) && v.iter().all(|&x| env.as_list(x).is_none()) {
            return Ok(Out::Again(tensor_curl(env, args[0], vars, 1)));
        }
        return Ok(Out::Done(env.failure("shape_mismatch", &[])));
    }
    // an array of rank k >= 2 (every level of length n): the rank n - k - 1 curl
    let n = vars.len();
    let mut rank = 0;
    let mut level = vec![args[0]];
    while level.iter().all(|&x| env.as_list(x).is_some_and(|l| l.len() == n)) {
        rank += 1;
        level = level.iter().flat_map(|&x| env.as_list(x).unwrap_or(&[]).to_vec()).collect();
    }
    if rank >= 2 {
        if rank >= n {
            return Ok(Out::Done(env.failure("shape_mismatch", &[])));
        }
        return Ok(Out::Again(tensor_curl(env, args[0], vars, rank)));
    }
    let minus_one = env.h().int(-1);
    let diff = |a: T<'h>, b: T<'h>| env.call(names::ADD, &[a, env.call(names::MUL, &[minus_one, b])]);
    let t = if v.len() == 2 {
        diff(d(env, v[1], vars[0]), d(env, v[0], vars[1]))
    } else {
        let (x, y, z) = (vars[0], vars[1], vars[2]);
        env.list(&[
            diff(d(env, v[2], y), d(env, v[1], z)),
            diff(d(env, v[0], z), d(env, v[2], x)),
            diff(d(env, v[1], x), d(env, v[0], y)),
        ])
    };
    Ok(Out::Again(t))
}

/// The curl of an array `a` of rank k < n in n variables (round 36): the array of rank n - k - 1 with entries
/// (1/k!) sum eps(i1, ..., in) d(a[i2, ..., i(k+1)], x(i1)) over the free indices i(k+2), ..., in (the Hodge dual
/// of the exterior derivative of a as a k-form; for k = 1, n = 3 the usual curl, for k = n - 1 a scalar).
fn tensor_curl<'h>(env: &Env<'h>, a: T<'h>, vars: &[T<'h>], k: usize) -> T<'h> {
    let n = vars.len();
    let h = env.h();
    let free = n - k - 1;
    let entry = |idx: &[usize]| idx.iter().fold(a, |t, &i| env.as_list(t).map_or(t, |l| l[i]));
    // the terms of each entry of the result, the entries in row-major order of the free indices
    let mut terms: Vec<Vec<T<'h>>> = vec![vec![]; n.pow(free as u32)];
    let mut perm: Vec<usize> = (0..n).collect();
    // Heap's algorithm over the permutations, with their signs
    let mut c = vec![0usize; n];
    let mut sign = 1i64;
    let mut visit = |perm: &[usize], sign: i64| {
        let at = perm[k + 1..].iter().fold(0, |acc, &i| acc * n + i);
        let dt = d(env, entry(&perm[1..=k]), vars[perm[0]]);
        terms[at].push(if sign > 0 { dt } else { env.call(names::MUL, &[h.int(-1), dt]) });
    };
    visit(&perm, sign);
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 {
                perm.swap(0, i);
            } else {
                perm.swap(c[i], i);
            }
            sign = -sign;
            visit(&perm, sign);
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    let fact: i64 = (1..=k as i64).product();
    let scale = env.ar.rational(h.int(1), h.int(fact));
    let flat: Vec<T<'h>> = terms.iter().map(|ts| env.call(names::MUL, &[scale, env.call(names::ADD, ts)])).collect();
    // nest the entries into the rank n - k - 1 array
    let mut level = flat;
    for _ in 0..free {
        level = level.chunks(n).map(|c| env.list(c)).collect();
    }
    level[0]
}

/// `laplacian(f, [x1, ..., xn])`: the sum of the second derivatives; for a list `f`, the list of Laplacians.
pub fn laplacian<'h>(env: &Env<'h>, args: &[T<'h>]) -> WorkResult<Out<'h>> {
    if args.len() != 2 {
        return Ok(Out::Decline);
    }
    let vars = take!(vars_arg(env, args));
    let one = |f: T<'h>| env.call(names::ADD, &vars.iter().map(|&x| d(env, d(env, f, x), x)).collect::<Vec<_>>());
    Ok(Out::Again(match env.as_list(args[0]) {
        Some(fs) => env.list(&fs.iter().map(|&f| one(f)).collect::<Vec<_>>()),
        None => one(args[0]),
    }))
}
