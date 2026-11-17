//! Generic painted dropdown panel (menus, color grids, shape gallery).
//! On selection posts WM_APP_DROPDOWN_PICK to the main window:
//!   wParam = menu tag (the command id that opened it), lParam = payload.

use crate::theme::*;
use crate::util::to_w;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, SetCapture, ReleaseCapture};

pub const WM_APP_DROPDOWN_PICK: u32 = 0x8000 + 40;

#[derive(Clone)]
pub struct DropItem {
    pub label: String,
    pub payload: i64,
    pub icon: DropIcon,
    pub sublabel: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DropIcon {
    None,
    Color(COLORREF),
    Shape(ShapeKind),
    Check(bool),
}

#[derive(Clone, Copy, PartialEq)]
pub enum ShapeKind {
    TextBox,
    Rectangle,
    RoundedRect,
    Ellipse,
    RightArrow,
    DownArrow,
    Line,
}

pub struct DropState {
    pub items: Vec<DropItem>,
    pub tag: u32,
    pub hot: isize,
    pub cols: usize, // >1 means grid layout
    pub cell: i32,   // cell size for grid
    pub owner: HWND,
    pub is_grid: bool,
}

unsafe fn st(hwnd: HWND) -> *mut DropState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DropState
}

const CLASS: PCWSTR = w!("RWE_Dropdown");

pub fn register(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_DROPSHADOW.0 | CS_HREDRAW.0 | CS_VREDRAW.0),
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

/// Show dropdown at screen point (x,y). `grid` items lay out in `cols` columns.
pub fn show(
    owner: HWND,
    hinst: HINSTANCE,
    tag: u32,
    items: Vec<DropItem>,
    x: i32,
    y: i32,
    cols: usize,
) -> HWND {
    unsafe {
        let is_grid = cols > 1;
        let (w, h) = measure(&items, cols);
        let mut px = x;
        let mut py = y;
        let mut sm = POINT::default();
        let _ = GetSystemMetrics(SM_CXSCREEN);
        let cx = GetSystemMetrics(SM_CXSCREEN);
        let cy = GetSystemMetrics(SM_CYSCREEN);
        let _ = &mut sm;
        if px + w > cx {
            px = cx - w - 4;
        }
        if py + h > cy {
            py = y - h - 4;
        }
        let stb = Box::new(DropState {
            items,
            tag,
            hot: -1,
            cols: cols.max(1),
            cell: 28,
            owner,
            is_grid,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS,
            PCWSTR::null(),
            WS_POPUP | WS_VISIBLE,
            px,
            py,
            w,
            h,
            owner,
            HMENU::default(),
            hinst,
            Some(Box::into_raw(stb) as _),
        )
        .unwrap_or_default();
        let _ = SetCapture(hwnd);
        hwnd
    }
}

fn measure(items: &[DropItem], cols: usize) -> (i32, i32) {
    if cols > 1 {
        let rows = items.len().div_ceil(cols) as i32;
        (cols as i32 * 28 + 8, rows * 28 + 8)
    } else {
        let mut w: i32 = 150;
        for it in items {
            let est = 30 + it.label.len() as i32 * 7 + it.sublabel.len() as i32 * 6;
            w = w.max(est.min(420));
        }
        (w + 8, items.len() as i32 * 24 + 8)
    }
}

fn item_rect(st: &DropState, i: usize) -> RECT {
    let idx = i as i32;
    if st.is_grid {
        let col = idx % st.cols as i32;
        let row = idx / st.cols as i32;
        RECT {
            left: 4 + col * st.cell,
            top: 4 + row * st.cell,
            right: 4 + col * st.cell + st.cell,
            bottom: 4 + row * st.cell + st.cell,
        }
    } else {
        RECT {
            left: 4,
            top: 4 + idx * 24,
            right: 1000,
            bottom: 4 + idx * 24 + 24,
        }
    }
}

fn hit(st: &DropState, x: i32, y: i32) -> isize {
    for (i, _) in st.items.iter().enumerate() {
        let r = item_rect(st, i);
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return i as isize;
        }
    }
    -1
}

fn draw_shape_icon(hdc: HDC, r: &RECT, kind: ShapeKind, c: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 2, c);
        let old = SelectObject(hdc, pen);
        let _ = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let cx = (r.left + r.right) / 2;
        let cy = (r.top + r.bottom) / 2;
        match kind {
            ShapeKind::Rectangle | ShapeKind::TextBox => {
                Rectangle(hdc, r.left + 3, cy - 7, r.right - 3, cy + 7);
            }
            ShapeKind::RoundedRect => {
                RoundRect(hdc, r.left + 3, cy - 7, r.right - 3, cy + 7, 8, 8);
            }
            ShapeKind::Ellipse => {
                Ellipse(hdc, r.left + 3, cy - 7, r.right - 3, cy + 7);
            }
            ShapeKind::RightArrow => {
                let pts = [
                    POINT { x: r.left + 3, y: cy - 5 },
                    POINT { x: cx + 2, y: cy - 5 },
                    POINT { x: cx + 2, y: cy - 8 },
                    POINT { x: r.right - 3, y: cy },
                    POINT { x: cx + 2, y: cy + 8 },
                    POINT { x: cx + 2, y: cy + 5 },
                    POINT { x: r.left + 3, y: cy + 5 },
                ];
                Polygon(hdc, &pts);
            }
            ShapeKind::DownArrow => {
                let pts = [
                    POINT { x: cx - 5, y: r.top + 4 },
                    POINT { x: cx + 5, y: r.top + 4 },
                    POINT { x: cx + 5, y: cy + 1 },
                    POINT { x: cx + 8, y: cy + 1 },
                    POINT { x: cx, y: r.bottom - 4 },
                    POINT { x: cx - 8, y: cy + 1 },
                    POINT { x: cx - 5, y: cy + 1 },
                ];
                Polygon(hdc, &pts);
            }
            ShapeKind::Line => {
                MoveToEx(hdc, r.left + 3, r.bottom - 5, None);
                LineTo(hdc, r.right - 3, r.top + 5);
            }
        }
        SelectObject(hdc, old);
        let _ = DeleteObject(pen);
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let s = &*st(hwnd);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    FillRect(hdc, &rc, HBRUSH(GetStockObject(WHITE_BRUSH).0));

    let brush = CreateSolidBrush(rgb(0xEE, 0xEE, 0xEE));
    FrameRect(hdc, &rc, brush);
    let _ = DeleteObject(brush);

    SetBkMode(hdc, TRANSPARENT);
    for (i, it) in s.items.iter().enumerate() {
        let r = item_rect(s, i);
        if s.hot == i as isize {
            let hb = CreateSolidBrush(BTN_HOT);
            FillRect(hdc, &r, hb);
            let _ = DeleteObject(hb);
        }
        match it.icon {
            DropIcon::Color(c) => {
                let mut cr = r;
                cr.left += 6;
                cr.right = cr.left + 16;
                cr.top += 4;
                cr.bottom -= 4;
                let b = CreateSolidBrush(c);
                FillRect(hdc, &cr, b);
                let _ = DeleteObject(b);
                let pb = CreateSolidBrush(rgb(0xA0, 0xA0, 0xA0));
                FrameRect(hdc, &cr, pb);
                let _ = DeleteObject(pb);
            }
            DropIcon::Shape(k) => draw_shape_icon(hdc, &r, k, rgb(0x2B, 0x57, 0x9A)),
            _ => {}
        }
        if !it.label.is_empty() {
            let mut tr = r;
            tr.left += 26;
            tr.top += 4;
            SetTextColor(hdc, BTN_TEXT);
            let mut buf = to_w(&it.label);
            buf.pop();
            DrawTextW(hdc, &mut buf, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
        }
        if !it.sublabel.is_empty() {
            let mut tr = r;
            tr.right -= 8;
            tr.top += 4;
            SetTextColor(hdc, GROUP_LABEL);
            let mut buf = to_w(&it.sublabel);
            buf.pop();
            DrawTextW(hdc, &mut buf, &mut tr, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
        }
    }
    let _ = EndPaint(hwnd, &ps);
}

fn close(hwnd: HWND) {
    unsafe {
        let s = st(hwnd);
        if !s.is_null() {
            let _ = Box::from_raw(s);
        }
        let _ = DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lp.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let s = st(hwnd);
            if !s.is_null() {
                let i = hit(&*s, crate::util::loword_signed(lp.0 as isize), crate::util::hiword_signed(lp.0 as isize));
                if (*s).hot != i {
                    (*s).hot = i;
                    let _ = InvalidateRect(hwnd, None, FALSE);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let s = st(hwnd);
            if !s.is_null() {
                let i = hit(&*s, crate::util::loword_signed(lp.0 as isize), crate::util::hiword_signed(lp.0 as isize));
                let owner = (*s).owner;
                if i >= 0 {
                    let st = &*s;
                    let it = &st.items[i as usize];
                    SendMessageW(
                        owner,
                        WM_APP_DROPDOWN_PICK,
                        WPARAM((*s).tag as usize),
                        LPARAM(it.payload as isize),
                    );
                }
                let _ = ReleaseCapture();
                close(hwnd);
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // clicked elsewhere
            close(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == VK_ESCAPE.0 as usize {
                let _ = ReleaseCapture();
                close(hwnd);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
