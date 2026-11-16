//! How much stack the calling thread has left.
//!
//! The recursive phases (parsing, lowering, typesetting) need a large stack and used to start a thread with one for
//! every call: two or three thread starts per evaluated cell. A thread started with a large reserved stack (the
//! session's workers, the kernel's engine thread) declares it here, and those phases run directly on it while
//! enough of it is free ([`remaining`]); on any other thread they still start their own.
use std::cell::Cell;

thread_local! {
    /// (address near the top of the stack when it was declared, bytes reserved); (0, 0) when not declared
    static DECLARED: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

/// Declare that the calling thread's stack has `bytes` reserved. Call it at the start of the thread, before any
/// deep call: the space used is counted from here.
pub fn declare(bytes: usize) {
    let here = position();
    DECLARED.with(|d| d.set((here, bytes)));
}

/// The bytes of stack still free below the caller on a thread that declared its stack (stacks grow downwards on
/// every supported target); 0 on a thread that did not.
pub fn remaining() -> usize {
    let (top, bytes) = DECLARED.with(Cell::get);
    if bytes == 0 {
        return 0;
    }
    let used = top.saturating_sub(position());
    bytes.saturating_sub(used)
}

/// An address in the caller's frame region.
#[inline(never)]
fn position() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undeclared_threads_have_none() {
        std::thread::spawn(|| assert_eq!(remaining(), 0)).join().unwrap();
    }

    #[test]
    fn declared_stack_shrinks_with_depth() {
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(|| {
                declare(8 << 20);
                let top = remaining();
                assert!(top > (8 << 20) - 4096 && top <= 8 << 20, "{top}");
                fn deep(n: u32) -> usize {
                    let pad = [n as u8; 512];
                    std::hint::black_box(&pad);
                    if n == 0 {
                        remaining()
                    } else {
                        deep(n - 1)
                    }
                }
                let low = deep(100);
                assert!(low < top - 100 * 512, "{low} {top}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
