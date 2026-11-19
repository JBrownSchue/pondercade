//! Windows-only fix for the window growing when it is dragged between monitors with
//! different scaling (e.g. 125% ↔ 100%).
//!
//! While the user drags the title bar, Windows runs a modal move loop that keeps proposing the
//! window rect it tracked when the drag started. When the window crosses onto a monitor with a
//! different DPI, winit rescales the window to keep its logical size, but the move loop then
//! re-applies its stale pixel size. winit already counts the new scale factor, so on the next
//! crossing it scales that stale (too large) size up again: the window grows by the DPI ratio on
//! every crossing, without bound, and stops following the cursor.
//!
//! We subclass the window and, only while it is being *moved* (not resized), rewrite the size in
//! every `WM_WINDOWPOSCHANGING` to "logical size at drag start × current DPI". Proposals that
//! aren't a uniform DPI-style rescale (e.g. Aero Snap) are left alone.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetWindowLongW, SWP_NOSIZE, WINDOWPOS, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING,
    WM_SIZING, WM_WINDOWPOSCHANGING,
};

static IN_LOOP: AtomicBool = AtomicBool::new(false);
static MOVING: AtomicBool = AtomicBool::new(false);
static SIZING: AtomicBool = AtomicBool::new(false);
/// Client size in logical units (96-DPI pixels) when the drag started, as two f32 bit patterns.
static LOGICAL: AtomicU64 = AtomicU64::new(0);

/// Installs the fix on the app's main window.
pub fn install(window: &impl raw_window_handle::HasWindowHandle) {
    let Ok(handle) = window.window_handle() else { return };
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else { return };
    let hwnd = h.hwnd.get() as HWND;
    unsafe {
        SetWindowSubclass(hwnd, Some(subclass_proc), 0x504f_4e44, 0);
    }
}

fn dpi_of(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        d => d,
    }
}

/// Outer (frame-inclusive) size for the drag-start logical client size at `dpi`.
fn outer_size(hwnd: HWND, dpi: u32) -> Option<(i32, i32)> {
    let bits = LOGICAL.load(Relaxed);
    let (lw, lh) = (f32::from_bits((bits >> 32) as u32), f32::from_bits(bits as u32));
    if lw <= 0.0 || lh <= 0.0 {
        return None;
    }
    let scale = dpi as f32 / 96.0;
    let mut r = RECT { left: 0, top: 0, right: (lw * scale).round() as i32, bottom: (lh * scale).round() as i32 };
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        AdjustWindowRectExForDpi(&mut r, style, 0, ex, dpi);
    }
    Some((r.right - r.left, r.bottom - r.top))
}

unsafe extern "system" fn subclass_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_ENTERSIZEMOVE => {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            unsafe { GetClientRect(hwnd, &mut r) };
            let scale = dpi_of(hwnd) as f32 / 96.0;
            let (lw, lh) = ((r.right - r.left) as f32 / scale, (r.bottom - r.top) as f32 / scale);
            LOGICAL.store(((lw.to_bits() as u64) << 32) | lh.to_bits() as u64, Relaxed);
            IN_LOOP.store(true, Relaxed);
            MOVING.store(false, Relaxed);
            SIZING.store(false, Relaxed);
        }
        WM_EXITSIZEMOVE => IN_LOOP.store(false, Relaxed),
        WM_MOVING => MOVING.store(true, Relaxed),
        WM_SIZING => SIZING.store(true, Relaxed),
        WM_WINDOWPOSCHANGING if IN_LOOP.load(Relaxed) && MOVING.load(Relaxed) && !SIZING.load(Relaxed) && lparam != 0 => {
            let wp = unsafe { &mut *(lparam as *mut WINDOWPOS) };
            if wp.flags & SWP_NOSIZE == 0 && wp.cx > 0 && wp.cy > 0 {
                if let Some((w, h)) = outer_size(hwnd, dpi_of(hwnd)) {
                    // Only correct DPI-style rescales (same factor on both axes); leave e.g. Snap alone.
                    let (fx, fy) = (wp.cx as f32 / w as f32, wp.cy as f32 / h as f32);
                    if (fx - fy).abs() < 0.03 * fx && (wp.cx != w || wp.cy != h) {
                        wp.cx = w;
                        wp.cy = h;
                    }
                }
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
