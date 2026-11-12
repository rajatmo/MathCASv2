//! A tiny hand-written x86-64 encoder for one kernel: the Mandelbrot escape function
//! `escape(cr: f64, ci: f64, ctl: *const Ctl) -> i64` with the same operation order and the same poll (every 1024
//! iterations) as the IR, hand register-allocated. It shows what a careful code generator would emit, to judge
//! Cranelift's output. Windows x64 calling convention only (`cr` in xmm0, `ci` in xmm1, `ctl` in r8).
use crate::kernels::MANDEL_ITERS;

#[derive(Default)]
struct Asm {
    b: Vec<u8>,
    /// (position of a rel32 field, label)
    fix: Vec<(usize, usize)>,
    labels: Vec<Option<usize>>,
}

const XMM: u8 = 0;

impl Asm {
    fn bytes(&mut self, x: &[u8]) {
        self.b.extend_from_slice(x);
    }
    fn label(&mut self) -> usize {
        self.labels.push(None);
        self.labels.len() - 1
    }
    fn bind(&mut self, l: usize) {
        self.labels[l] = Some(self.b.len());
    }
    fn rel32(&mut self, l: usize) {
        self.fix.push((self.b.len(), l));
        self.bytes(&[0; 4]);
    }
    /// SSE2 op `prefix 0F opc` with register operands (xmm0..xmm7).
    fn sse(&mut self, prefix: u8, opc: u8, d: u8, s: u8) {
        self.bytes(&[prefix, 0x0F, opc, 0xC0 | (d << 3) | s]);
    }
    fn movapd(&mut self, d: u8, s: u8) {
        self.sse(0x66, 0x28, d, s)
    }
    fn mulsd(&mut self, d: u8, s: u8) {
        self.sse(0xF2, 0x59, d, s)
    }
    fn addsd(&mut self, d: u8, s: u8) {
        self.sse(0xF2, 0x58, d, s)
    }
    fn subsd(&mut self, d: u8, s: u8) {
        self.sse(0xF2, 0x5C, d, s)
    }
    fn xorpd(&mut self, d: u8, s: u8) {
        self.sse(0x66, 0x57, d, s)
    }
    fn ucomisd(&mut self, a: u8, b: u8) {
        self.sse(0x66, 0x2E, a, b)
    }
    fn jcc(&mut self, cc: u8, l: usize) {
        self.bytes(&[0x0F, 0x80 | cc]);
        self.rel32(l);
    }
    fn jmp(&mut self, l: usize) {
        self.bytes(&[0xE9]);
        self.rel32(l);
    }
    fn finish(mut self) -> Vec<u8> {
        for &(at, l) in &self.fix {
            let target = self.labels[l].expect("unbound label") as i64;
            let rel = (target - (at as i64 + 4)) as i32;
            self.b[at..at + 4].copy_from_slice(&rel.to_le_bytes());
        }
        self.b
    }
}

const JB: u8 = 0x2;
const JNZ: u8 = 0x5;
const JL: u8 = 0xC;
const JGE: u8 = 0xD;

/// Machine code of the escape function.
pub fn escape_code(poll_every: u32) -> Vec<u8> {
    let mut a = Asm::default();
    let (lp, done, fuel, intr, fail, c4) = (a.label(), a.label(), a.label(), a.label(), a.label(), a.label());
    let (cr, ci, zr, zi, t, u, s, four) = (XMM, 1, 2, 3, 4, 5, 6, 7);
    // prologue: xmm6 and xmm7 are callee-saved
    a.bytes(&[0x48, 0x83, 0xEC, 0x28]); // sub rsp, 40
    a.bytes(&[0xF3, 0x0F, 0x7F, 0x34, 0x24]); // movdqu [rsp], xmm6
    a.bytes(&[0xF3, 0x0F, 0x7F, 0x7C, 0x24, 0x10]); // movdqu [rsp+16], xmm7
    a.bytes(&[0x31, 0xC0]); // xor eax, eax        k = 0
    a.xorpd(zr, zr);
    a.xorpd(zi, zi);
    a.bytes(&[0xB9]); // mov ecx, poll_every
    a.bytes(&poll_every.to_le_bytes());
    a.bytes(&[0xF2, 0x0F, 0x10, 0x3D]); // movsd xmm7, [rip + c4]
    a.rel32(c4);
    a.bind(lp);
    a.bytes(&[0x48, 0x3D]); // cmp rax, 256
    a.bytes(&(MANDEL_ITERS as i32).to_le_bytes());
    a.jcc(JGE, done);
    a.movapd(t, zr);
    a.mulsd(t, zr); // t = zr*zr
    a.movapd(u, zi);
    a.mulsd(u, zi); // u = zi*zi
    a.movapd(s, t);
    a.addsd(s, u); // s = t + u
    a.ucomisd(four, s);
    a.jcc(JB, done); // 4 < s or unordered: escaped
    a.subsd(t, u);
    a.addsd(t, cr); // re = (t - u) + cr
    a.movapd(u, zr);
    a.mulsd(u, zi); // zr*zi
    a.movapd(s, zi);
    a.mulsd(s, zr); // zi*zr
    a.addsd(u, s);
    a.addsd(u, ci); // im = (zr*zi + zi*zr) + ci
    a.movapd(zr, t);
    a.movapd(zi, u);
    a.bytes(&[0x48, 0xFF, 0xC0]); // inc rax
    a.bytes(&[0xFF, 0xC9]); // dec ecx
    a.jcc(JNZ, lp);
    // poll: draw fuel, read the stop flag
    a.bytes(&[0xB9]);
    a.bytes(&poll_every.to_le_bytes()); // mov ecx, n
    a.bytes(&[0x48, 0xC7, 0xC2]);
    a.bytes(&(-(poll_every as i32)).to_le_bytes()); // mov rdx, -n
    a.bytes(&[0xF0, 0x49, 0x0F, 0xC1, 0x10]); // lock xadd [r8], rdx   (rdx = old fuel)
    a.bytes(&[0x48, 0x81, 0xFA]);
    a.bytes(&(poll_every as i32).to_le_bytes()); // cmp rdx, n
    a.jcc(JL, fuel);
    a.bytes(&[0x41, 0x8B, 0x50, 0x08]); // mov edx, [r8+8]
    a.bytes(&[0x85, 0xD2]); // test edx, edx
    a.jcc(JNZ, intr);
    a.jmp(lp);
    a.bind(done);
    a.bytes(&[0xF3, 0x0F, 0x6F, 0x34, 0x24]); // movdqu xmm6, [rsp]
    a.bytes(&[0xF3, 0x0F, 0x6F, 0x7C, 0x24, 0x10]); // movdqu xmm7, [rsp+16]
    a.bytes(&[0x48, 0x83, 0xC4, 0x28]); // add rsp, 40
    a.bytes(&[0xC3]); // ret
    a.bind(fuel);
    a.bytes(&[0xBA, 2, 0, 0, 0]); // mov edx, Status::Fuel
    a.jmp(fail);
    a.bind(intr);
    a.bytes(&[0xBA, 1, 0, 0, 0]); // mov edx, Status::Interrupted
    a.bind(fail);
    a.bytes(&[0x31, 0xC0]); // xor eax, eax (expected 0)
    a.bytes(&[0xF0, 0x41, 0x0F, 0xB1, 0x50, 0x0C]); // lock cmpxchg [r8+12], edx
    a.bytes(&[0x31, 0xC0]); // result 0
    a.jmp(done);
    while a.b.len() % 8 != 0 {
        a.bytes(&[0xCC]);
    }
    a.bind(c4);
    a.bytes(&4.0f64.to_le_bytes());
    a.finish()
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod mem {
    #[link(name = "kernel32")]
    extern "system" {
        fn VirtualAlloc(addr: *mut u8, size: usize, kind: u32, protect: u32) -> *mut u8;
        fn VirtualFree(addr: *mut u8, size: usize, kind: u32) -> i32;
    }
    pub struct Exec {
        pub ptr: *mut u8,
    }
    impl Exec {
        pub fn new(code: &[u8]) -> Option<Exec> {
            // SAFETY: plain allocation of fresh pages; we copy the code in before anyone calls it.
            unsafe {
                let p = VirtualAlloc(std::ptr::null_mut(), code.len().max(4096), 0x3000, 0x40);
                if p.is_null() {
                    return None;
                }
                std::ptr::copy_nonoverlapping(code.as_ptr(), p, code.len());
                Some(Exec { ptr: p })
            }
        }
    }
    impl Drop for Exec {
        fn drop(&mut self) {
            // SAFETY: `ptr` came from VirtualAlloc and is released once.
            unsafe {
                VirtualFree(self.ptr, 0, 0x8000);
            }
        }
    }
}

/// The hand-encoded escape function, ready to call (Windows x64 only).
pub struct HandEscape {
    #[cfg(all(windows, target_arch = "x86_64"))]
    mem: mem::Exec,
    pub code_bytes: usize,
}

// SAFETY: the code is immutable once written.
unsafe impl Send for HandEscape {}
unsafe impl Sync for HandEscape {}

impl HandEscape {
    #[cfg(all(windows, target_arch = "x86_64"))]
    pub fn new(poll_every: u32) -> Option<HandEscape> {
        let code = escape_code(poll_every);
        Some(HandEscape { code_bytes: code.len(), mem: mem::Exec::new(&code)? })
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    pub fn new(_poll_every: u32) -> Option<HandEscape> {
        None
    }
    pub fn func(&self) -> extern "C" fn(f64, f64, *const crate::Ctl) -> i64 {
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            // SAFETY: the buffer holds a complete function with this signature under the Windows x64 convention.
            unsafe { std::mem::transmute::<*mut u8, extern "C" fn(f64, f64, *const crate::Ctl) -> i64>(self.mem.ptr) }
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        unreachable!()
    }
}
