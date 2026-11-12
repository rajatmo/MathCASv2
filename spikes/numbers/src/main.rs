//! Numbers spike driver. Prints markdown tables for: multiplication scaling and agreement (mul), crossover sweeps
//! (cross), ball enclosure and overhead (ball), constants N1/N2 (const) and adaptive precision N11 (adapt).
//!
//! usage: numbers [--quick | --full] [part...]
//!
//! --quick (default) runs small sizes in seconds on a busy machine; --full runs the benchmark sizes (10^7-digit
//! products, 10^6-digit constants) with repeated runs, for an idle machine. Build with `--features v1` to include
//! v1's big numbers.
use numbers::adaptive::{self, n_eval};
use numbers::ball::{enclosure_trials, Ball, Float, Mag};
use numbers::consts::{self, bits_for_digits, nb_from_limbs, nb_limbs};
use numbers::int::Int;
use numbers::nat::{self, Algo, Nat};
use numbers::{fingerprint, Rng, STOP};
use num_bigint::BigInt as NbInt;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- peak heap (as in spike 1)

struct Counting;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let c = CUR.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(c, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            if n > l.size() {
                let c = CUR.fetch_add(n - l.size(), Ordering::Relaxed) + (n - l.size());
                PEAK.fetch_max(c, Ordering::Relaxed);
            } else {
                CUR.fetch_sub(l.size() - n, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static A: Counting = Counting;

// ---------------------------------------------------------------- timing, limits

#[derive(Clone, Copy)]
struct Mode {
    full: bool,
}

struct Timing {
    median: f64,
    min: f64,
    runs: usize,
    peak_mb: f64,
    limit: bool,
}

/// Case time limit (benchmark protocol section 1).
const LIMIT: Duration = Duration::from_secs(60);

/// Sets the stop flags (this crate's, and v1's when compiled in) if the case runs past the limit.
struct Watchdog {
    tx: Option<mpsc::Sender<()>>,
    h: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    fn start() -> Watchdog {
        let (tx, rx) = mpsc::channel::<()>();
        let v1flag = v1::flag();
        let h = std::thread::spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(LIMIT) {
                STOP.store(true, Ordering::Relaxed);
                v1::raise(&v1flag);
            }
        });
        Watchdog { tx: Some(tx), h: Some(h) }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

/// Runs f under the protocol: in --full, a warm-up and at least 5 runs (or about 2 s) for cheap cases, 3 runs up to
/// 5 s per run, a single run beyond; in --quick, 5 runs below 20 ms, else one. Returns the median and the last value.
fn measure<T>(mode: Mode, mut f: impl FnMut() -> T) -> (Timing, T) {
    STOP.store(false, Ordering::Relaxed);
    v1::reset();
    let base = CUR.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let wd = Watchdog::start();
    let t0 = Instant::now();
    let mut out = f();
    let first = t0.elapsed().as_secs_f64();
    let mut times = vec![first];
    let extra = if mode.full {
        if first < 0.4 {
            times.clear(); // the first run was the warm-up
            ((2.0 / first.max(1e-6)) as usize).clamp(5, 50)
        } else if first < 5.0 {
            2
        } else {
            0
        }
    } else if first < 0.02 {
        4
    } else {
        0
    };
    for _ in 0..extra {
        if numbers::stopped() {
            break;
        }
        let t = Instant::now();
        out = f();
        times.push(t.elapsed().as_secs_f64());
    }
    drop(wd);
    let limit = numbers::stopped();
    STOP.store(false, Ordering::Relaxed);
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base) as f64 / 1e6;
    (Timing { median: times[times.len() / 2], min: times[0], runs: times.len(), peak_mb: peak, limit }, out)
}

/// Seconds per call of a cheap operation: one warm-up call, a calibration call, then the median of 3 (quick) or 7
/// (full) samples of enough calls for about 10 ms (quick) or 30 ms (full) each.
fn per_call<T>(mode: Mode, f: &dyn Fn() -> T) -> f64 {
    std::hint::black_box(f());
    let t = Instant::now();
    std::hint::black_box(f());
    let one = t.elapsed().as_secs_f64().max(1e-8);
    let budget = if mode.full { 0.03 } else { 0.01 };
    let reps = ((budget / one) as usize).clamp(1, 1_000_000);
    let mut v: Vec<f64> = (0..if mode.full { 7 } else { 3 })
        .map(|_| {
            let t = Instant::now();
            for _ in 0..reps {
                std::hint::black_box(f());
            }
            t.elapsed().as_secs_f64() / reps as f64
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn ms(s: f64) -> String {
    let v = s * 1e3;
    if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else if v >= 1.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.3}")
    }
}

fn time_cell(t: &Timing) -> String {
    if t.limit {
        "limit".to_string()
    } else {
        ms(t.median)
    }
}

fn rand_bits(rng: &mut Rng, bits: u64) -> Nat {
    let limbs = bits.div_ceil(64) as usize;
    let mut v: Vec<u64> = (0..limbs).map(|_| rng.next_u64()).collect();
    let top = bits - 64 * (limbs as u64 - 1); // 1..=64 bits in the top limb
    if top < 64 {
        v[limbs - 1] &= (1u64 << top) - 1;
    }
    v[limbs - 1] |= 1u64 << (top - 1);
    Nat::from_limbs(v)
}

// ---------------------------------------------------------------- v1 adapter

#[cfg(feature = "v1")]
mod v1 {
    use numbers::bigfloat::BigFloat;
    use numbers::bigint::BigInt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    pub type Flag = Arc<AtomicBool>;
    pub const ENABLED: bool = true;

    thread_local! {
        static FLAG: Flag = Arc::new(AtomicBool::new(false));
    }
    pub fn flag() -> Flag {
        FLAG.with(|f| {
            numbers::interrupt::install(f);
            f.clone()
        })
    }
    pub fn raise(f: &Flag) {
        f.store(true, Ordering::Relaxed);
    }
    pub fn reset() {
        FLAG.with(|f| f.store(false, Ordering::Relaxed));
    }

    /// From little-endian 64-bit limbs by divide and conquer over shifts and additions (v1's decimal parser is
    /// quadratic, and v1 exposes no limb constructor).
    pub fn from_limbs(l: &[u64]) -> BigInt {
        if l.len() <= 1 {
            return BigInt::from_u64(l.first().copied().unwrap_or(0));
        }
        let h = l.len() / 2;
        from_limbs(&l[h..]).shl_bits(64 * h as u64).add(&from_limbs(&l[..h]))
    }
    /// To little-endian 64-bit limbs through v1's base-2^16 digits.
    pub fn limbs(x: &BigInt) -> Vec<u64> {
        let d = x.to_digits(1 << 16); // most significant first
        let mut out = vec![0u64; d.len().div_ceil(4)];
        for (i, &v) in d.iter().rev().enumerate() {
            out[i / 4] |= (v as u64) << (16 * (i % 4));
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }
    pub fn mul(a: &BigInt, b: &BigInt) -> BigInt {
        a.mul(b)
    }
    pub fn float(x: &numbers::ball::Float, prec: u32) -> BigFloat {
        BigFloat { neg: x.neg, mant: from_limbs(&x.man.limbs), exp: x.exp, prec }
    }
    /// The first d digits of pi with v1's own algorithm and conversion.
    pub fn pi_digits(d: u64) -> String {
        let prec = numbers::consts::bits_for_digits(d + 12) + 16;
        let (_, s, _) = BigFloat::pi(prec as u32).to_decimal(d as usize + 12);
        s[..d as usize].to_string()
    }
    /// e to off + k + 12 digits (exp(1) in v1), converted in full, then the k digits after position off.
    pub fn e_digits_at(off: u64, k: u64) -> String {
        let n = off + k + 13;
        let prec = numbers::consts::bits_for_digits(n) + 16;
        let (_, s, _) = BigFloat::e(prec as u32).to_decimal(n as usize);
        s[off as usize + 1..(off + 1 + k) as usize].to_string()
    }
    pub use numbers::bigfloat::BigFloat as Float;
}

#[cfg(not(feature = "v1"))]
mod v1 {
    pub struct Flag;
    pub const ENABLED: bool = false;
    pub fn flag() -> Flag {
        Flag
    }
    pub fn raise(_: &Flag) {}
    pub fn reset() {}
}

// ---------------------------------------------------------------- part 1: multiplication

fn part_mul(mode: Mode) {
    println!("## Multiplication of two n-digit integers\n");
    println!("Times in ms (median); peak heap in MB above the inputs; check = fingerprint equal to own/auto.\n");
    println!("| n digits | implementation | median | min | runs | peak heap | check |");
    println!("|---|---|---|---|---|---|---|");
    let sizes: &[u64] = if mode.full { &[1_000, 10_000, 100_000, 1_000_000, 10_000_000] } else { &[1_000, 10_000, 100_000] };
    let mut rng = Rng(42);
    for &n in sizes {
        let bits = bits_for_digits(n);
        let a = rand_bits(&mut rng, bits);
        let b = rand_bits(&mut rng, bits);
        let row = |name: &str, t: &Timing, fp: Option<u64>, want: u64| {
            let check = match fp {
                None if t.limit => "limit",
                None => "-",
                Some(f) if f == want => "ok",
                Some(_) => "MISMATCH",
            };
            println!(
                "| {n} | {name} | {} | {} | {} | {:.1} | {check} |",
                time_cell(t),
                ms(t.min),
                t.runs,
                t.peak_mb
            );
        };
        let (t, r) = measure(mode, || a.mul(&b));
        let want = fingerprint(&r.limbs);
        drop(r);
        row("own auto", &t, Some(want), want);
        let algos: Vec<(&str, Algo, u64)> = vec![
            ("own schoolbook", Algo::school(), 1_000_000),
            ("own Karatsuba (no NTT)", Algo::kara_only(), 10_000_000),
            ("own NTT (top level)", Algo::ntt_only(), 10_000_000),
        ];
        for (name, al, max_n) in algos {
            if n > max_n || (!mode.full && n > 100_000) {
                continue;
            }
            let (t, r) = measure(mode, || a.mul_with(&b, al));
            row(name, &t, (!t.limit).then(|| fingerprint(&r.limbs)), want);
        }
        let (na, nb) = (nb_from_limbs(&a.limbs), nb_from_limbs(&b.limbs));
        let (t, r) = measure(mode, || &na * &nb);
        row("num-bigint 0.4", &t, (!t.limit).then(|| fingerprint(&nb_limbs(&r))), want);
        drop((na, nb, r));
        #[cfg(feature = "v1")]
        {
            let (va, vb) = (v1::from_limbs(&a.limbs), v1::from_limbs(&b.limbs));
            let (t, r) = measure(mode, || v1::mul(&va, &vb));
            row("v1 kernel", &t, (!t.limit).then(|| fingerprint(&v1::limbs(&r))), want);
        }
    }
    println!();
}

// ---------------------------------------------------------------- part 1b: crossovers

fn part_cross(mode: Mode) {
    println!("## Crossovers\n");
    println!(
        "Compiled defaults: Karatsuba from {} limbs, NTT from {} limbs (64-bit limbs; 1 limb = 19.3 digits).\n",
        nat::KARA_THRESH.load(Ordering::Relaxed),
        nat::NTT_THRESH.load(Ordering::Relaxed)
    );
    let mut rng = Rng(7);
    // schoolbook -> Karatsuba: total time of balanced products at several sizes for each threshold
    println!("### Karatsuba threshold (sum of times for 64, 128, 256, 512, 1024-limb products, us)\n");
    println!("| threshold (limbs) | time |");
    println!("|---|---|");
    let sizes = [64usize, 128, 256, 512, 1024];
    let ops: Vec<(Nat, Nat)> = sizes
        .iter()
        .map(|&l| (rand_bits(&mut rng, 64 * l as u64), rand_bits(&mut rng, 64 * l as u64)))
        .collect();
    let mut best = (f64::MAX, 0);
    for th in [8usize, 12, 16, 20, 24, 32, 40, 48, 64, 96] {
        let al = Algo { kara: th, ntt: usize::MAX };
        let tot: f64 = ops.iter().map(|(a, b)| per_call(mode, &|| a.mul_with(b, al))).sum();
        if tot < best.0 {
            best = (tot, th);
        }
        println!("| {th} | {:.1} |", tot * 1e6);
    }
    println!("\nBest Karatsuba threshold: **{} limbs**.\n", best.1);

    println!("### Karatsuba against NTT (balanced products, us per product)\n");
    println!("| limbs | digits | Karatsuba | NTT | num-bigint | NTT / Karatsuba |");
    println!("|---|---|---|---|---|---|");
    let mut sizes: Vec<usize> = vec![256, 384, 512, 640, 768, 1024, 1280, 1536, 2048, 2560, 3072, 4096, 6144, 8192];
    if mode.full {
        sizes.extend([12288, 16384, 32768]);
    }
    let mut first_win_after_last_loss = None;
    for &l in &sizes {
        let a = rand_bits(&mut rng, 64 * l as u64);
        let b = rand_bits(&mut rng, 64 * l as u64);
        let tk = per_call(mode, &|| a.mul_with(&b, Algo::kara_only()));
        let tn = per_call(mode, &|| a.mul_with(&b, Algo::ntt_only()));
        let (na, nb) = (nb_from_limbs(&a.limbs), nb_from_limbs(&b.limbs));
        let tb = per_call(mode, &|| &na * &nb);
        if tn < tk {
            first_win_after_last_loss.get_or_insert(l);
        } else {
            first_win_after_last_loss = None;
        }
        println!(
            "| {l} | {} | {:.1} | {:.1} | {:.1} | {:.2} |",
            (l as f64 * 64.0 * std::f64::consts::LOG10_2) as u64,
            tk * 1e6,
            tn * 1e6,
            tb * 1e6,
            tn / tk
        );
    }
    match first_win_after_last_loss {
        Some(l) => println!("\nNTT faster from **{l} limbs** on (at every measured size from there).\n"),
        None => println!("\nNTT not consistently faster in the measured range.\n"),
    }
}

// ---------------------------------------------------------------- part 2: balls

fn rand_ball_p(rng: &mut Rng, p: u64) -> Ball {
    let mid = Float::new(false, rand_bits(rng, p), -(p as i64) + 1);
    Ball { mid: mid.clone(), rad: Mag::pow2(mid.top() - p as i64 - 3) }
}

fn part_ball(mode: Mode) {
    println!("## Ball arithmetic\n");
    let n = if mode.full { 30_000 } else { 3_000 };
    let (t, tr) = measure(Mode { full: false }, || enclosure_trials(2024, n));
    println!(
        "Enclosure: {n} random cases (precisions 24 to 700 bits, exact and wide radii), {} exact checks of \
         add/sub/mul/div/sqrt/compare at the inputs' end, middle and interior points, plus {} chains of 30 operations: \
         **{} failures**; {} undefined results (divisor containing zero, sqrt of a non-positive ball). {} ms.\n",
        tr.checks,
        n / 10,
        tr.failures,
        tr.undefined,
        ms(t.median)
    );
    println!("Overhead of radius bookkeeping: ns per operation (median), bare = the same rounded midpoint operation.\n");
    print!("| digits | bits | op | bare | ball | overhead |");
    if v1::ENABLED {
        print!(" v1 BigFloat |");
    }
    println!();
    print!("|---|---|---|---|---|---|");
    if v1::ENABLED {
        print!("---|");
    }
    println!();
    let digits: &[u64] = &[100, 1_000, 10_000, 100_000];
    let mut rng = Rng(99);
    for &d in digits {
        let p = bits_for_digits(d);
        let a = rand_ball_p(&mut rng, p);
        let b = rand_ball_p(&mut rng, p);
        // repetitions for about 20 ms (quick) or 100 ms (full) per sample
        let budget = if mode.full { 0.1 } else { 0.02 };
        // Calls per sample, from one timed call; then interleaved samples of the given closures (warmed up first:
        // whichever runs first after a size change otherwise pays for the allocator's growth), median of each.
        let samples = if mode.full { 7 } else { 3 };
        let per_n = |fs: &[&dyn Fn()]| -> Vec<f64> {
            for f in fs {
                f();
            }
            let t = Instant::now();
            fs[0]();
            let one = t.elapsed().as_secs_f64().max(1e-8);
            let reps = ((budget / one) as usize).clamp(1, 1_000_000);
            for f in fs {
                f();
            }
            let mut v: Vec<Vec<f64>> = vec![vec![]; fs.len()];
            for _ in 0..samples {
                for (i, f) in fs.iter().enumerate() {
                    let t = Instant::now();
                    for _ in 0..reps {
                        f();
                    }
                    v[i].push(t.elapsed().as_secs_f64() / reps as f64 * 1e9);
                }
            }
            v.into_iter()
                .map(|mut x| {
                    x.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    x[x.len() / 2]
                })
                .collect()
        };
        #[cfg(feature = "v1")]
        let per = |f: &dyn Fn()| per_n(&[f])[0];
        type Op<'a> = (&'a str, Box<dyn Fn() + 'a>, Box<dyn Fn() + 'a>);
        let ops: Vec<Op> = vec![
            ("add", Box::new(|| drop(std::hint::black_box(a.mid.add(&b.mid, p)))), Box::new(|| drop(std::hint::black_box(a.add(&b, p))))),
            ("mul", Box::new(|| drop(std::hint::black_box(a.mid.mul(&b.mid, p)))), Box::new(|| drop(std::hint::black_box(a.mul(&b, p))))),
            ("div", Box::new(|| drop(std::hint::black_box(a.mid.div(&b.mid, p)))), Box::new(|| drop(std::hint::black_box(a.div(&b, p))))),
            ("sqrt", Box::new(|| drop(std::hint::black_box(a.mid.sqrt(p)))), Box::new(|| drop(std::hint::black_box(a.sqrt(p))))),
        ];
        #[cfg(feature = "v1")]
        let (va, vb) = (v1::float(&a.mid, p as u32), v1::float(&b.mid, p as u32));
        for (name, bare, ball) in &ops {
            let t = per_n(&[bare.as_ref(), ball.as_ref()]);
            let (tb, tl) = (t[0], t[1]);
            print!("| {d} | {p} | {name} | {tb:.0} | {tl:.0} | {:+.1}% |", (tl / tb - 1.0) * 100.0);
            #[cfg(feature = "v1")]
            {
                let tv = match *name {
                    "add" => per(&|| drop(std::hint::black_box(va.add(&vb)))),
                    "mul" => per(&|| drop(std::hint::black_box(va.mul(&vb)))),
                    "div" => per(&|| drop(std::hint::black_box(va.div(&vb)))),
                    _ => per(&|| drop(std::hint::black_box(va.sqrt()))),
                };
                print!(" {tv:.0} |");
            }
            println!();
        }
    }
    println!();
}

// ---------------------------------------------------------------- part 3: constants

/// Digit counts of the first 10^6 decimals of pi (after "3."), a published table; the counts sum to 10^6.
const PI_1M_COUNTS: [u32; 10] = [99959, 99758, 100026, 100229, 100230, 100359, 99548, 99800, 99985, 100106];
const PI50: &str = "31415926535897932384626433832795028841971693993751";
const E50: &str = "27182818284590452353602874713526624977572470936999";

fn part_const(mode: Mode) {
    println!("## Constants (N1, N2)\n");
    let sizes: &[u64] = if mode.full { &[10_000, 100_000, 1_000_000] } else { &[10_000, 100_000] };
    println!("### N1: digits of pi (Chudnovsky, binary splitting)\n");
    println!("binary = pi 2^p as an integer; total = including the decimal conversion. Times in ms.\n");
    println!("| digits | implementation | binary | total | peak heap | digits agree |");
    println!("|---|---|---|---|---|---|");
    for &d in sizes {
        let (tb, _) = measure(mode, || consts::pi_fixed::<Int>(bits_for_digits(d + 12) + 16));
        let (tt, own) = measure(mode, || consts::pi_digits::<Int>(d));
        println!("| {d} | own | {} | {} | {:.1} | reference |", time_cell(&tb), time_cell(&tt), tt.peak_mb);
        let mut checks = vec![];
        checks.push(("leading 50 digits", own[..50] == *PI50));
        checks.push(("six nines at 762", &own[762..768] == "999999"));
        let (_, longer) = measure(Mode { full: false }, || consts::pi_digits::<Int>(d + 100));
        checks.push(("self-consistent with d + 100", longer[..d as usize] == own));
        if d == 1_000_000 {
            let mut c = [0u32; 10];
            for ch in own[1..].bytes() {
                c[(ch - b'0') as usize] += 1;
            }
            // own has 10^6 digits including the leading 3: 999,999 decimals, plus the 10^6-th from the longer run
            c[(longer.as_bytes()[d as usize] - b'0') as usize] += 1;
            checks.push(("digit counts of the first 10^6 decimals", c == PI_1M_COUNTS));
        }
        if d <= if mode.full { 100_000 } else { 10_000 } {
            let (tm, m) = measure(Mode { full: false }, || consts::pi_digits_machin(d));
            checks.push(("Machin's formula", m == own));
            println!("| {d} | own, Machin (check only) | | {} | | {} |", time_cell(&tm), m == own);
        }
        if d <= 100_000 || mode.full {
            let (tn, nb) = measure(mode, || consts::pi_digits::<NbInt>(d));
            let ok = !tn.limit && nb == own;
            println!("| {d} | num-bigint 0.4, same algorithm | | {} | {:.1} | {} |", time_cell(&tn), tn.peak_mb, ok);
        }
        // v1 at 10^6 digits runs past the case limit, and its quadratic division and conversion do not poll the
        // interrupt flag, so it would hold the run for minutes: it is measured only by the explicit `v1big` probe
        #[cfg(feature = "v1")]
        if d >= 1_000_000 {
            println!("| {d} | v1 kernel (BigFloat::pi, to_decimal) | | > 60000 (limit; `v1big`) | | - |");
        } else {
            let (tv, v) = measure(mode, || v1::pi_digits(d));
            let ok = !tv.limit && v == own;
            let tvb = if d <= 100_000 {
                let (tvb, _) = measure(mode, || v1::Float::pi(bits_for_digits(d + 12) as u32 + 16));
                time_cell(&tvb)
            } else {
                "-".into()
            };
            println!("| {d} | v1 kernel (BigFloat::pi, to_decimal) | {tvb} | {} | {:.1} | {} |", time_cell(&tv), tv.peak_mb, ok);
        }
        let all = checks.iter().all(|c| c.1);
        let list: Vec<String> = checks.iter().map(|(n, ok)| format!("{n}: {}", if *ok { "ok" } else { "FAIL" })).collect();
        println!("| {d} | checks | | | | {} ({}) |", if all { "ok" } else { "FAIL" }, list.join("; "));
    }
    println!();

    println!("### N2: 20 digits of e starting after position off\n");
    println!("targeted = ((num 10^off) mod den) 10^k / den on the binary-splitting fraction, no full conversion; full = all digits converted. Times in ms.\n");
    println!("| off | implementation | time | peak heap | digits | agree |");
    println!("|---|---|---|---|---|---|");
    for &off in sizes {
        let k = 20;
        let (tt, own) = measure(mode, || consts::e_digits_at::<Int>(off, k));
        println!("| {off} | own, targeted | {} | {:.1} | {own} | reference |", time_cell(&tt), tt.peak_mb);
        let (tf, full) = measure(mode, || consts::e_digits::<Int>(off + k + 1));
        let fs = &full[off as usize + 1..];
        println!("| {off} | own, full conversion | {} | {:.1} | | {} |", time_cell(&tf), tf.peak_mb, fs == own);
        let lead_ok = full[..50] == *E50;
        let (tc, cf) = measure(Mode { full: false }, || consts::e_digits_cf(off + k + 1));
        println!(
            "| {off} | own, continued fraction (check only) | {} | | | {} |",
            time_cell(&tc),
            cf[off as usize + 1..] == own && lead_ok
        );
        if off <= 100_000 || mode.full {
            let (tn, nb) = measure(mode, || consts::e_digits_at::<NbInt>(off, k));
            println!("| {off} | num-bigint 0.4, targeted | {} | {:.1} | | {} |", time_cell(&tn), tn.peak_mb, !tn.limit && nb == own);
        }
        #[cfg(feature = "v1")]
        if off >= 1_000_000 {
            println!("| {off} | v1 kernel (BigFloat::e, to_decimal) | > 60000 (limit; `v1big`) | | | - |");
        } else {
            let (tv, v) = measure(mode, || v1::e_digits_at(off, k));
            println!("| {off} | v1 kernel (BigFloat::e, to_decimal) | {} | {:.1} | | {} |", time_cell(&tv), tv.peak_mb, !tv.limit && v == own);
        }
    }
    println!();
}

/// Where pi's time goes: the series, the square root, the final product and division, the conversion.
fn part_pibreak(mode: Mode) {
    println!("## Cost breakdown of pi (ms)
");
    println!("| digits | backend | series Q, T | sqrt(10005) | 426880 s Q | / T | to decimal (incl. 10^d product) |");
    println!("|---|---|---|---|---|---|---|");
    let sizes: &[u64] = if mode.full { &[10_000, 100_000, 1_000_000] } else { &[10_000, 100_000] };
    fn row<B: consts::Backend>(mode: Mode, d: u64) {
        let bits = bits_for_digits(d + 12) + 16;
        let (t1, (q, t)) = measure(mode, || consts::chud_qt::<B>(bits));
        let (t2, s) = measure(mode, || B::from_u64(10005).shl(2 * bits).isqrt());
        let (t3, num) = measure(mode, || s.mul(&q).mul_u64(426_880));
        let (t4, x) = measure(mode, || num.div_floor(&t));
        let (t5, _) = measure(mode, || x.mul(&B::pow10(d + 11)).div_floor(&B::from_u64(1).shl(bits)).to_decimal());
        println!(
            "| {d} | {} | {} | {} | {} | {} | {} |",
            B::NAME,
            time_cell(&t1),
            time_cell(&t2),
            time_cell(&t3),
            time_cell(&t4),
            time_cell(&t5)
        );
    }
    for &d in sizes {
        row::<Int>(mode, d);
        row::<NbInt>(mode, d);
    }
    println!();
}

// ---------------------------------------------------------------- part 4: adaptive precision

fn part_adapt(mode: Mode) {
    println!("## N11: adaptive precision, n(e, 50)\n");
    println!("Start at 50 log2(10) + 16 = 183 bits, double until the ball has 50 correct digits (cap 2^14 bits). fixed = one evaluation at the final precision.\n");
    println!("| id | expression | rounds | final bits | reached | encloses exact | time (ms) | fixed (ms) | value (50 digits) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for (id, desc, f) in adaptive::expressions() {
        let (t, o) = measure(mode, || n_eval(&f, 50, 1 << 14));
        let (tf, _) = measure(mode, || f(o.final_bits));
        let ball = o.ball.clone();
        let encl = ball.as_ref().is_some_and(|b| adaptive::check(id, b));
        let value = match &ball {
            Some(b) if o.reached => {
                let (s, e) = b.mid.to_sci(50);
                format!("{}{}.{}e{e}", if b.mid.neg { "-" } else { "" }, &s[..1], &s[1..])
            }
            Some(b) => format!("failure(precision_not_reached): radius 2^{:.0}", b.rad.log2()),
            None => "undefined".into(),
        };
        println!(
            "| {id} | {desc} | {} | {} | {} | {} | {} | {} | {value} |",
            o.rounds,
            o.final_bits,
            o.reached,
            encl,
            time_cell(&t),
            time_cell(&tf)
        );
    }
    // the first rounds of the hard cases: how wide the honest balls are
    println!("\nCorrect bits by round (relative to the midpoint; 0 = the ball contains zero; - = undefined):\n");
    println!("| id | 183 | 366 | 732 | 1464 |");
    println!("|---|---|---|---|---|");
    for (id, _, f) in adaptive::expressions() {
        let cells: Vec<String> = [183u64, 366, 732, 1464]
            .iter()
            .map(|&p| match f(p) {
                Some(b) if b.contains_zero() => "0".into(),
                Some(b) => format!("{:.0}", b.rel_bits().min(1e9)),
                None => "-".into(),
            })
            .collect();
        println!("| {id} | {} |", cells.join(" | "));
    }
    println!();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = Mode { full: args.iter().any(|a| a == "--full") };
    let parts: Vec<&str> = args.iter().filter(|a| !a.starts_with("--")).map(|s| s.as_str()).collect();
    let want = |p: &str| parts.is_empty() || parts.contains(&p);
    println!("# Numbers spike: {} run\n", if mode.full { "full" } else { "quick" });
    println!(
        "Machine: {} ({} hardware threads), {}; v1 comparison {}.\n",
        std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown CPU".into()),
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        std::env::consts::OS,
        if v1::ENABLED { "on" } else { "off (build with --features v1)" }
    );
    // the stack for deep recursion is not needed: every recursion here halves its input
    let t0 = Instant::now();
    if want("mul") {
        part_mul(mode);
    }
    if want("cross") {
        part_cross(mode);
    }
    if want("ball") {
        part_ball(mode);
    }
    if want("const") {
        part_const(mode);
    }
    if parts.contains(&"pibreak") || (parts.is_empty() && mode.full) {
        part_pibreak(mode);
    }
    #[cfg(feature = "v1")]
    if parts.contains(&"v1big") {
        // v1 alone at 10^6 digits. Both exceed the 60 s limit; the watchdog raises v1's flag, but v1's quadratic
        // loops do not poll it, so this probe can run for several minutes
        let (t, _) = measure(Mode { full: false }, || v1::pi_digits(1_000_000));
        println!("v1 pi 10^6 digits: {} ms", time_cell(&t));
        let (t, _) = measure(Mode { full: false }, || v1::e_digits_at(1_000_000, 20));
        println!("v1 e at offset 10^6: {} ms", time_cell(&t));
    }
    if want("adapt") {
        part_adapt(mode);
    }
    println!("Total wall time: {:.1} s", t0.elapsed().as_secs_f64());
}
