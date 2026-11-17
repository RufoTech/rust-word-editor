//! Bottom status bar: page/word info + zoom slider + zoom %.

use crate::commands::*;
use crate::theme::*;
use crate::util::{loword, to_w};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

const TBM_GETPOS: u32 = 0x0400;

pub const STATUS_H: i32 = 24;
const CLASS: PCWSTR = w!("RWE_StatusBar");

pub struct StatusState {
    pub pages: String,
    pub words: String,
    pub zoom_pct: i32,
    pub slider: HWND,
}

pub fn register(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0),
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

pub fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    unsafe {
        let st = Box::new(StatusState {
            pages: "Page 1 of 1".into(),
            words: "0 words".into(),
            zoom_pct: 100,
            slider: HWND::default(),
        });
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            0,
            0,
            100,
            STATUS_H,
            parent,
            HMENU::default(),
            hinst,
            Some(Box::into_raw(st) as _),
        )
        .unwrap_or_default()
    }
}

unsafe fn state(hwnd: HWND) -> *mut StatusState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut StatusState
}

fn ensure_slider(hwnd: HWND, w: i32) {
    unsafe {
        let s = state(hwnd);
        if s.is_null() || !(*s).slider.0.is_null() {
            return;
        }
        let parent = GetParent(hwnd).unwrap_or_default();
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let slider = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("msctls_trackbar32"),
            PCWSTR::null(),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | TBS_HORZ | TBS_NOTICKS | TBS_TOOLTIPS),
            w - 170,
            2,
            110,
            20,
            hwnd,
            HMENU(IDM_ZOOM_SLIDER as _),
            HINSTANCE(hinst.0),
            None,
        )
        .unwrap_or_default();
        SendMessageW(slider, TBM_SETRANGE, WPARAM(1), LPARAM((500 << 16 | 10) as isize));
        SendMessageW(slider, TBM_SETPOS, WPARAM(1), LPARAM((*s).zoom_pct as isize));
        (*s).slider = slider;
        let _ = parent;
    }
}

unsafe fn paint(hwnd: HWND) {
    let s = &*state(hwnd);
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = rc.right;
    let b = CreateSolidBrush(STATUS_BAR);
    FillRect(hdc, &rc, b);
    let _ = DeleteObject(b);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, STATUS_TEXT);
    let mut tr = RECT { left: 10, top: 0, right: 400, bottom: STATUS_H };
    let text = format!("{}  |  {}", s.pages, s.words);
    let mut buf = to_w(&text);
    buf.pop();
    DrawTextW(hdc, &mut buf, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    // zoom label
    let mut zr = RECT { left: w - 52, top: 0, right: w - 6, bottom: STATUS_H };
    let zt = format!("{}%", s.zoom_pct);
    let mut zb = to_w(&zt);
    zb.pop();
    DrawTextW(hdc, &mut zb, &mut zr, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lp.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_SIZE => {
            ensure_slider(hwnd, loword(lp.0 as isize));
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_HSCROLL => {
            // trackbar messages arrive here (slider's parent is statusbar); forward to main
            let parent = GetParent(hwnd).unwrap_or_default();
            SendMessageW(parent, WM_HSCROLL, wp, lp)
        }
        WM_NCDESTROY => {
            let s = state(hwnd);
            if !s.is_null() {
                let _ = Box::from_raw(s);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn set_info(hwnd: HWND, pages: &str, words: &str) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).pages = pages.into();
            (*s).words = words.into();
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

pub fn set_zoom_display(hwnd: HWND, pct: i32) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).zoom_pct = pct;
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

pub fn slider_pos(hwnd: HWND) -> i32 {
    unsafe {
        let s = state(hwnd);
        if s.is_null() || (*s).slider.0.is_null() {
            return 100;
        }
        SendMessageW((*s).slider, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as i32
    }
}
