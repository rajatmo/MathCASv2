//! The text pointer (the I-beam over text), made by the window itself.
//!
//! The system's I-beam on Windows takes its colour from what is under it by inverting, and on some systems (a
//! display in HDR mode, some graphics drivers) that inversion is lost over a window drawn by the GPU: the pointer
//! stays white and cannot be seen on a light page. So on Windows the window gives the system its own I-beam, a
//! colour pointer in the theme's text colour with a halo of the page colour, which is visible on any page in both
//! themes. It is a real system pointer (the system moves it, not the window's frames), so it follows the mouse
//! as fast as any other. Other systems draw their I-beam in fixed colours and keep it.
use eframe::egui::{self, Color32, CursorIcon};

/// Half the height of the I-beam and half the width of its serifs, in points.
const HALF_HEIGHT: f32 = 8.0;
const SERIF: f32 = 2.5;

/// Call at the end of a frame: when the frame asked for the text pointer, show the window's own I-beam in
/// `ink` with a halo in `page`. Nothing happens for touch input (no pointer to show).
pub fn paint_text_pointer(ctx: &egui::Context, ink: Color32, page: Color32) {
    let wanted = ctx.output(|o| o.cursor_icon) == CursorIcon::Text && !ctx.input(|i| i.any_touches());
    #[cfg(windows)]
    {
        if windows::show(wanted, ctx.pixels_per_point(), ink, page) {
            // the window system keeps the arrow as its choice; the I-beam is set over it while text is under
            // the pointer
            if wanted {
                ctx.set_cursor_icon(CursorIcon::Default);
            }
            return;
        }
        painted(ctx, wanted, ink, page);
    }
    #[cfg(not(windows))]
    {
        let _ = (wanted, ink, page, painted);
    }
}

/// The I-beam painted into the frame (when the system pointer cannot be replaced): it follows the mouse a frame
/// late.
fn painted(ctx: &egui::Context, wanted: bool, ink: Color32, page: Color32) {
    use egui::{pos2, Id, LayerId, Order, Stroke};
    if !wanted {
        return;
    }
    let Some(p) = ctx.input(|i| i.pointer.latest_pos()) else { return };
    ctx.set_cursor_icon(CursorIcon::None);
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("text-pointer")));
    let ppp = ctx.pixels_per_point();
    // whole pixels, the stem through a pixel's centre: a crisp one-pixel line at every scale
    let snap = |v: f32| ((v * ppp).floor() + 0.5) / ppp;
    let (x, y) = (snap(p.x), snap(p.y));
    let (top, bottom) = (snap(y - HALF_HEIGHT), snap(y + HALF_HEIGHT));
    let lines = [
        [pos2(x, top), pos2(x, bottom)],
        [pos2(x - SERIF, top), pos2(x + SERIF, top)],
        [pos2(x - SERIF, bottom), pos2(x + SERIF, bottom)],
    ];
    let thin = (1.0 / ppp).max(1.0);
    for l in lines {
        painter.line_segment(l, Stroke::new(thin + 2.0, page));
    }
    for l in lines {
        painter.line_segment(l, Stroke::new(thin, ink));
    }
}

/// The pixels of the I-beam at `scale` pixels per point: (width, height, hot spot x, hot spot y, BGRA rows from
/// the top). Ink where the stem and the two serifs are, the page colour one line around them, clear elsewhere.
#[cfg(any(windows, test))]
fn image(scale: f32, ink: Color32, page: Color32) -> (usize, usize, usize, usize, Vec<u8>) {
    let px = |points: f32| (points * scale).round().max(1.0) as usize;
    let (line, halo) = (px(1.0), px(1.0));
    let (half_h, serif) = (px(HALF_HEIGHT), px(SERIF));
    let (w, h) = (2 * (serif + halo) + line, 2 * (half_h + halo) + line);
    let (cx, cy) = (serif + halo, half_h + halo);
    // the ink as rectangles (x0, y0, x1, y1), ends excluded: the stem and the two serifs
    let ink_rects = [
        (cx, halo, cx + line, h - halo),
        (halo, halo, w - halo, halo + line),
        (halo, h - halo - line, w - halo, h - halo),
    ];
    let inside = |x: usize, y: usize, grow: usize| {
        ink_rects.iter().any(|&(x0, y0, x1, y1)| x + grow >= x0 && x < x1 + grow && y + grow >= y0 && y < y1 + grow)
    };
    let mut bits = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let c = if inside(x, y, 0) {
                ink
            } else if inside(x, y, halo) {
                page
            } else {
                continue;
            };
            bits[(y * w + x) * 4..][..4].copy_from_slice(&[c.b(), c.g(), c.r(), 255]);
        }
    }
    (w, h, cx + line / 2, cy + line / 2, bits)
}

#[cfg(windows)]
mod windows {
    //! The I-beam as a system pointer: a colour cursor made from [`super::image`], set by a hook on the window's
    //! "which pointer?" message while text is under the pointer.
    use super::Color32;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::Mutex;

    type Hwnd = isize;
    type Proc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;

    #[repr(C)]
    struct IconInfo {
        icon: i32,
        x: u32,
        y: u32,
        mask: isize,
        colour: isize,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetCursor(cursor: isize) -> isize;
        fn LoadCursorW(instance: isize, name: *const u16) -> isize;
        fn CreateIconIndirect(info: *const IconInfo) -> isize;
        fn DestroyIcon(icon: isize) -> i32;
        fn EnumThreadWindows(thread: u32, each: unsafe extern "system" fn(Hwnd, isize) -> i32, data: isize) -> i32;
        fn IsWindowVisible(window: Hwnd) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateBitmap(width: i32, height: i32, planes: u32, bits_per_pixel: u32, bits: *const c_void) -> isize;
        fn DeleteObject(object: isize) -> i32;
    }
    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(window: Hwnd, hook: Proc, id: usize, data: usize) -> i32;
        fn DefSubclassProc(window: Hwnd, message: u32, w: usize, l: isize) -> isize;
    }

    const WM_SETCURSOR: u32 = 0x0020;
    const HTCLIENT: isize = 1;
    const IDC_ARROW: *const u16 = 32512 as *const u16;
    const HOOK_ID: usize = 0x4d43_5054;

    /// Text is under the pointer (set each frame), and the I-beam to show then (0: none made).
    static WANTED: AtomicBool = AtomicBool::new(false);
    static CURSOR: AtomicIsize = AtomicIsize::new(0);
    /// What the I-beam was made for: pixels per point in hundredths, ink and page colours.
    type MadeFor = (u32, [u8; 4], [u8; 4]);
    /// The windows that have the hook, and what the I-beam was made for.
    static STATE: Mutex<(Vec<Hwnd>, Option<MadeFor>)> = Mutex::new((Vec::new(), None));

    unsafe extern "system" fn hook(window: Hwnd, message: u32, w: usize, l: isize, _id: usize, _data: usize) -> isize {
        if message == WM_SETCURSOR && (l & 0xffff) == HTCLIENT && WANTED.load(Ordering::Relaxed) {
            let cursor = CURSOR.load(Ordering::Relaxed);
            if cursor != 0 {
                // SAFETY: a cursor handle this module made and has not destroyed
                unsafe { SetCursor(cursor) };
                return 1;
            }
        }
        // SAFETY: the next handler of a window this hook was installed on
        unsafe { DefSubclassProc(window, message, w, l) }
    }

    unsafe extern "system" fn visible(window: Hwnd, data: isize) -> i32 {
        // SAFETY: `data` is the `Vec` that `show` passes for the length of the enumeration
        let found = unsafe { &mut *(data as *mut Vec<Hwnd>) };
        // SAFETY: a window handle the system just gave
        if unsafe { IsWindowVisible(window) } != 0 {
            found.push(window);
        }
        1
    }

    /// Make the I-beam for this scale and these colours.
    fn make(scale: f32, ink: Color32, page: Color32) -> isize {
        let (w, h, x, y, bits) = super::image(scale, ink, page);
        let mask = vec![0u8; w.div_ceil(16) * 2 * h];
        // SAFETY: both buffers hold the rows the sizes name (32-bit rows; 1-bit rows padded to 16 bits); the
        // bitmaps are copied into the cursor and deleted
        unsafe {
            let colour = CreateBitmap(w as i32, h as i32, 1, 32, bits.as_ptr().cast());
            let mask = CreateBitmap(w as i32, h as i32, 1, 1, mask.as_ptr().cast());
            let info = IconInfo { icon: 0, x: x as u32, y: y as u32, mask, colour };
            let cursor = if colour != 0 && mask != 0 { CreateIconIndirect(&info) } else { 0 };
            DeleteObject(colour);
            DeleteObject(mask);
            cursor
        }
    }

    /// Show the window's I-beam while `wanted`; false when it cannot be made (the caller paints one).
    /// Called on the thread that owns the window.
    pub fn show(wanted: bool, scale: f32, ink: Color32, page: Color32) -> bool {
        let Ok(mut state) = STATE.lock() else { return false };
        // the hook, on every visible window of this thread that does not have it yet
        let mut found: Vec<Hwnd> = Vec::new();
        // SAFETY: the callback only pushes into `found`, which outlives the call
        unsafe { EnumThreadWindows(GetCurrentThreadId(), visible, &mut found as *mut Vec<Hwnd> as isize) };
        for window in found {
            if !state.0.contains(&window) {
                // SAFETY: a window of this thread; the hook is a function of this program, which lives as long
                if unsafe { SetWindowSubclass(window, hook, HOOK_ID, 0) } != 0 {
                    state.0.push(window);
                }
            }
        }
        if state.0.is_empty() {
            return false;
        }
        let key = ((scale * 100.0).round() as u32, ink.to_array(), page.to_array());
        if state.1 != Some(key) {
            let cursor = make(scale, ink, page);
            if cursor == 0 {
                return false;
            }
            let old = CURSOR.swap(cursor, Ordering::Relaxed);
            state.1 = Some(key);
            if old != 0 {
                // SAFETY: the pointer made before, no longer the one shown after the next pointer message
                unsafe {
                    if WANTED.load(Ordering::Relaxed) {
                        SetCursor(cursor);
                    }
                    DestroyIcon(old);
                }
            }
        }
        // at a change the pointer is set at once: the next "which pointer?" message comes only with a mouse move
        if WANTED.swap(wanted, Ordering::Relaxed) != wanted {
            // SAFETY: a cursor made above, or the system's arrow
            unsafe { SetCursor(if wanted { CURSOR.load(Ordering::Relaxed) } else { LoadCursorW(0, IDC_ARROW) }) };
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_i_beam_has_ink_inside_a_halo() {
        let (ink, page) = (Color32::from_rgb(10, 20, 30), Color32::from_rgb(250, 250, 250));
        for scale in [1.0, 1.5, 2.0, 3.0] {
            let (w, h, x, y, bits) = image(scale, ink, page);
            assert_eq!(bits.len(), w * h * 4);
            let at = |x: usize, y: usize| &bits[(y * w + x) * 4..][..4];
            // the hot spot is ink, the corners of the picture are halo, beside the stem at mid height it is clear
            assert_eq!(at(x, y), [30, 20, 10, 255], "{scale}");
            assert_eq!(at(0, 0), [250, 250, 250, 255], "{scale}");
            assert_eq!(at(0, y)[3], 0, "{scale}");
            // every ink pixel has no clear neighbour
            for py in 0..h {
                for px in 0..w {
                    if at(px, py) == [30, 20, 10, 255] {
                        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                            let (nx, ny) = (px as i32 + dx, py as i32 + dy);
                            assert!(nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h, "{scale}");
                            assert_eq!(at(nx as usize, ny as usize)[3], 255, "{scale}");
                        }
                    }
                }
            }
        }
    }
}
