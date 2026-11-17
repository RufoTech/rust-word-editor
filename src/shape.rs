//! Floating shapes (rectangle, ellipse, arrow, text box) with inner rich text.
//! Each shape is a child window containing a RichEdit for the text run.

use crate::dropdown::ShapeKind;
use crate::editor;
use crate::theme::*;
use crate::util::{loword_signed, hiword_signed, pt_to_twips};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::Controls::EM_SETMARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const CLASS: PCWSTR = w!("RWE_Shape");
pub const WM_APP_SHAPE_NOTIFY: u32 = 0x8000 + 20; // forwarded editor notifications
pub const WM_APP_SHAPE_SELECT: u32 = 0x8000 + 21; // wParam=shape hwnd
pub const WM_APP_SHAPE_FOCUS_EDIT: u32 = 0x8000 + 22; // wParam=inner edit hwnd

const HANDLE: i32 = 7;
const MIN_SZ: i32 = 24;

#[derive(Clone)]
pub struct ShapeProps {
    pub kind: ShapeKind,
    pub fill: COLORREF,
    pub outline: COLORREF,
    pub outline_w: i32,     // px
    pub has_fill: bool,
    pub has_outline: bool,
    pub inset_l: i32,
    pub inset_t: i32,
    pub inset_r: i32,
    pub inset_b: i32, // px, scaled by zoom in layout
}

pub struct ShapeState {
    pub props: ShapeProps,
    pub edit: HWND,
    pub selected: bool,
    pub dragging: bool,
    pub resize_edge: u8, // bit0=left,1=right,2=top,3=bottom
    pub drag_start: POINT,
    pub orig_rect: RECT,
    pub zoom_pct: i32,
}

impl Default for ShapeProps {
    fn default() -> Self {
        ShapeProps {
            kind: ShapeKind::Rectangle,
            fill: PAGE_WHITE,
            outline: rgb(0x2B, 0x57, 0x9A),
            outline_w: 1,
            has_fill: true,
            has_outline: true,
            inset_l: 7,
            inset_t: 4,
            inset_r: 7,
            inset_b: 4,
        }
    }
}

pub fn register(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0 | CS_DBLCLKS.0),
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

pub fn create(
    parent: HWND,
    hinst: HINSTANCE,
    kind: ShapeKind,
    rect: RECT,
    zoom_pct: i32,
) -> HWND {
    let mut props = ShapeProps::default();
    props.kind = kind;
    create_with_props(parent, hinst, props, rect, zoom_pct)
}

pub fn create_with_props(
    parent: HWND,
    hinst: HINSTANCE,
    mut props: ShapeProps,
    rect: RECT,
    zoom_pct: i32,
) -> HWND {
    unsafe {
        match props.kind {
            ShapeKind::TextBox => {
                props.fill = PAGE_WHITE;
                props.inset_l = pt_to_twips(7.2) / 20; // ≈ px at 96dpi
                props.inset_r = props.inset_l;
                props.inset_t = 3;
                props.inset_b = 3;
            }
            ShapeKind::Line => {
                props.has_fill = false;
            }
            _ => {}
        }
        let st = Box::new(ShapeState {
            props,
            edit: HWND::default(),
            selected: true,
            dragging: false,
            resize_edge: 0,
            drag_start: POINT::default(),
            orig_rect: RECT::default(),
            zoom_pct,
        });
        // no WS_EX_TRANSPARENT: a transparent window lets clicks fall
        // through to the document, so the shape could never be focused,
        // moved, resized, or typed into
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            parent,
            HMENU::default(),
            hinst,
            Some(Box::into_raw(st) as _),
        )
        .unwrap_or_default();
        let s = state(hwnd);
        if !s.is_null() {
            // inner RichEdit
            let edit = editor::create_editor(hwnd, hinst, 9901).unwrap_or_default();
            SendMessageW(edit, EM_SETEVENTMASK, WPARAM(0), LPARAM(ENM_SELCHANGE as isize));
            // transparent-ish look: use shape fill as editor bg
            SendMessageW(edit, EM_SETBKGNDCOLOR, WPARAM(0), LPARAM((*s).props.fill.0 as isize));
            SendMessageW(edit, EM_SETMARGINS, WPARAM(EC_LEFTMARGIN as usize | EC_RIGHTMARGIN as usize), LPARAM(0));
            SendMessageW(edit, EM_SETTEXTMODE, WPARAM(TM_SINGLELEVELUNDO.0 as usize), LPARAM(0));
            (*s).edit = edit;
            update_region(hwnd);
            layout_edit(hwnd);
        }
        hwnd
    }
}

/// Clip the window (and its child editor) to the shape's geometry — keeps
/// ellipse/arrow text inside the outline like Word does.
unsafe fn update_region(hwnd: HWND) {
    let s = state(hwnd);
    if s.is_null() {
        return;
    }
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let rgn = match (*s).props.kind {
        ShapeKind::Ellipse => CreateEllipticRgn(0, 0, w, h),
        ShapeKind::RightArrow | ShapeKind::DownArrow => {
            let cx = w / 2;
            let cy = h / 2;
            let pts = if (*s).props.kind == ShapeKind::RightArrow {
                let sh = h / 2;
                let q = sh / 2;
                let head = (w / 4).max(8);
                vec![
                    POINT { x: 0, y: cy - q },
                    POINT { x: w - head, y: cy - q },
                    POINT { x: w - head, y: 0 },
                    POINT { x: w, y: cy },
                    POINT { x: w - head, y: h },
                    POINT { x: w - head, y: cy + q },
                    POINT { x: 0, y: cy + q },
                ]
            } else {
                let q = w / 4;
                let head = (h / 4).max(8);
                vec![
                    POINT { x: cx - q, y: 0 },
                    POINT { x: cx + q, y: 0 },
                    POINT { x: cx + q, y: h - head },
                    POINT { x: w, y: h - head },
                    POINT { x: cx, y: h },
                    POINT { x: 0, y: h - head },
                    POINT { x: cx - q, y: h - head },
                ]
            };
            CreatePolygonRgn(&pts, ALTERNATE)
        }
        _ => HRGN::default(),
    };
    if !rgn.0.is_null() {
        let _ = SetWindowRgn(hwnd, rgn, TRUE); // takes ownership
    }
}

unsafe fn state(hwnd: HWND) -> *mut ShapeState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ShapeState
}

fn scale(px: i32, zoom_pct: i32) -> i32 {
    px * zoom_pct / 100
}

unsafe fn layout_edit(hwnd: HWND) {
    let s = state(hwnd);
    if s.is_null() {
        return;
    }
    let st = &*s;
    if st.edit.0.is_null() {
        return;
    }
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let z = st.zoom_pct;
    // keep the inner editor inside the visible geometry for curved shapes
    let extra_x = match st.props.kind {
        ShapeKind::Ellipse => (rc.right - rc.left) / 8,
        _ => 0,
    };
    let extra_y = match st.props.kind {
        ShapeKind::Ellipse => (rc.bottom - rc.top) / 8,
        _ => 0,
    };
    let l = 2 + extra_x + scale(st.props.inset_l, z);
    let t = 2 + extra_y + scale(st.props.inset_t, z);
    let r = (rc.right - rc.left) - 2 - extra_x - scale(st.props.inset_r, z);
    let b = (rc.bottom - rc.top) - 2 - scale(st.props.inset_b, z);
    let _ = SetWindowPos(st.edit, HWND_TOP, l, t, (r - l).max(8), (b - t).max(8), SWP_NOACTIVATE);
}

fn handles(rc: &RECT) -> [RECT; 8] {
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let cx = [rc.left, rc.left + w / 2, rc.right];
    let cy = [rc.top, rc.top + h / 2, rc.bottom];
    let mut out = [RECT::default(); 8];
    let mut i = 0;
    for &y in &cy {
        for &x in &cx {
            if x == rc.left + w / 2 && y == rc.top + h / 2 {
                continue;
            }
            out[i] = RECT { left: x - HANDLE / 2, top: y - HANDLE / 2, right: x + HANDLE / 2, bottom: y + HANDLE / 2 };
            i += 1;
        }
    }
    out
}

fn edge_at(rc: &RECT, x: i32, y: i32) -> u8 {
    for (i, h) in handles(rc).iter().enumerate() {
        if x >= h.left && x <= h.right && y >= h.top && y <= h.bottom {
            // order: TL,T,TR, L,R, BL,B,BR
            return match i {
                0 => 1 | 4,
                1 => 4,
                2 => 2 | 4,
                3 => 1,
                4 => 2,
                5 => 1 | 8,
                6 => 8,
                7 => 2 | 8,
                _ => 0,
            };
        }
    }
    0
}

unsafe fn paint(hwnd: HWND) {
    let s = &*state(hwnd);
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;

    // erase with parent-gray (page area is painted by parent; use white here)
    let bb = CreateSolidBrush(PAGE_WHITE);
    FillRect(hdc, &rc, bb);
    let _ = DeleteObject(bb);

    let inner = RECT { left: 1, top: 1, right: w - 1, bottom: h - 1 };
    // fill
    if s.props.has_fill {
        let fb = CreateSolidBrush(s.props.fill);
        FillRect(hdc, &inner, fb);
        let _ = DeleteObject(fb);
    }
    // outline
    if s.props.has_outline && s.props.outline_w > 0 {
        let pen = CreatePen(PS_SOLID, s.props.outline_w.max(1), s.props.outline);
        let old = SelectObject(hdc, pen);
        let _ = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        draw_kind(hdc, &inner, s.props.kind);
        SelectObject(hdc, old);
        let _ = DeleteObject(pen);
    } else {
        // no outline: still need fill geometry for non-rect shapes
        if s.props.has_fill {
            // already filled the rect; for ellipse/arrow repaint shape mask
            // (kept simple: rectangular fill for MVP)
        }
    }

    // selection handles
    if s.selected {
        for hr in handles(&inner).iter() {
            let hb = CreateSolidBrush(PAGE_WHITE);
            FillRect(hdc, hr, hb);
            let _ = DeleteObject(hb);
            let pb = CreateSolidBrush(SELECTION_BORDER);
            FrameRect(hdc, hr, pb);
            let _ = DeleteObject(pb);
        }
    }
    let _ = EndPaint(hwnd, &ps);
}

fn draw_kind(hdc: HDC, r: &RECT, kind: ShapeKind) {
    unsafe {
        let cx = (r.left + r.right) / 2;
        let cy = (r.top + r.bottom) / 2;
        match kind {
            ShapeKind::TextBox | ShapeKind::Rectangle => {
                Rectangle(hdc, r.left, r.top, r.right, r.bottom);
            }
            ShapeKind::RoundedRect => {
                let rad = ((r.right - r.left).min(r.bottom - r.top) / 6).max(4);
                RoundRect(hdc, r.left, r.top, r.right, r.bottom, rad, rad);
            }
            ShapeKind::Ellipse => {
                Ellipse(hdc, r.left, r.top, r.right, r.bottom);
            }
            ShapeKind::RightArrow => {
                let sh = (r.bottom - r.top) / 2; // shaft half-height quarter
                let q = sh / 2;
                let head = ((r.right - r.left) / 4).max(8);
                let pts = [
                    POINT { x: r.left, y: cy - q },
                    POINT { x: r.right - head, y: cy - q },
                    POINT { x: r.right - head, y: r.top },
                    POINT { x: r.right, y: cy },
                    POINT { x: r.right - head, y: r.bottom },
                    POINT { x: r.right - head, y: cy + q },
                    POINT { x: r.left, y: cy + q },
                ];
                Polygon(hdc, &pts);
            }
            ShapeKind::DownArrow => {
                let q = (r.right - r.left) / 4;
                let head = ((r.bottom - r.top) / 4).max(8);
                let pts = [
                    POINT { x: cx - q, y: r.top },
                    POINT { x: cx + q, y: r.top },
                    POINT { x: cx + q, y: r.bottom - head },
                    POINT { x: r.right, y: r.bottom - head },
                    POINT { x: cx, y: r.bottom },
                    POINT { x: r.left, y: r.bottom - head },
                    POINT { x: cx - q, y: r.bottom - head },
                ];
                Polygon(hdc, &pts);
            }
            ShapeKind::Line => {
                MoveToEx(hdc, r.left, r.bottom, None);
                LineTo(hdc, r.right, r.top);
            }
        }
        let _ = cx;
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lp.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_SIZE => {
            update_region(hwnd);
            layout_edit(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(0),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_NOTIFY => {
            // forward inner-editor notifications to the main window
            let parent = GetParent(hwnd).unwrap_or_default();
            let s = state(hwnd);
            if !s.is_null() {
                SendMessageW(parent, WM_APP_SHAPE_FOCUS_EDIT, WPARAM((*s).edit.0 as usize), LPARAM(0));
            }
            SendMessageW(parent, msg, wp, lp)
        }
        WM_SETFOCUS | WM_LBUTTONDOWN => {
            let s = state(hwnd);
            if !s.is_null() {
                let st = &mut *s;
                if !st.selected {
                    st.selected = true;
                    let _ = InvalidateRect(hwnd, None, FALSE);
                }
                let parent = GetParent(hwnd).unwrap_or_default();
                SendMessageW(parent, WM_APP_SHAPE_SELECT, WPARAM(hwnd.0 as usize), LPARAM(0));

                if msg == WM_LBUTTONDOWN {
                    let x = loword_signed(lp.0 as isize);
                    let y = hiword_signed(lp.0 as isize);
                    let mut rc = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rc);
                    let edge = edge_at(&rc, x, y);
                    if edge != 0 {
                        st.resize_edge = edge;
                        st.dragging = true;
                        let _ = GetCursorPos(&mut st.drag_start);
                        let _ = GetWindowRect(hwnd, &mut st.orig_rect);
                        let _ = SetCapture(hwnd);
                    } else {
                        // check frame ring (not over inner edit)
                        let p = state(hwnd);
                        if !p.is_null() {
                            let mut er = RECT::default();
                            let _ = GetWindowRect((*p).edit, &mut er);
                            let mut pt = POINT { x, y };
                            let _ = ClientToScreen(hwnd, &mut pt);
                            if pt.x < er.left || pt.x > er.right || pt.y < er.top || pt.y > er.bottom {
                                st.resize_edge = 0;
                                st.dragging = true;
                                let _ = GetCursorPos(&mut st.drag_start);
                                let _ = GetWindowRect(hwnd, &mut st.orig_rect);
                                let _ = SetCapture(hwnd);
                            } else {
                                let _ = SetFocus((*p).edit);
                            }
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let s = state(hwnd);
            if !s.is_null() {
                let st = &mut *s;
                if st.dragging {
                    let mut cur = POINT::default();
                    let _ = GetCursorPos(&mut cur);
                    let dx = cur.x - st.drag_start.x;
                    let dy = cur.y - st.drag_start.y;
                    let mut r = st.orig_rect;
                    let parent = GetParent(hwnd).unwrap_or_default();
                    // convert to parent coords
                    let mut tl = POINT { x: r.left, y: r.top };
                    let mut br = POINT { x: r.right, y: r.bottom };
                    let _ = ScreenToClient(parent, &mut tl);
                    let _ = ScreenToClient(parent, &mut br);
                    r = RECT { left: tl.x, top: tl.y, right: br.x, bottom: br.y };
                    let e = st.resize_edge;
                    if e == 0 {
                        // move
                        r.left += dx;
                        r.right += dx;
                        r.top += dy;
                        r.bottom += dy;
                    } else {
                        if e & 1 != 0 {
                            r.left = (r.left + dx).min(r.right - MIN_SZ);
                        }
                        if e & 2 != 0 {
                            r.right = (r.right + dx).max(r.left + MIN_SZ);
                        }
                        if e & 4 != 0 {
                            r.top = (r.top + dy).min(r.bottom - MIN_SZ);
                        }
                        if e & 8 != 0 {
                            r.bottom = (r.bottom + dy).max(r.top + MIN_SZ);
                        }
                    }
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_TOP,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                } else {
                    // cursor feedback
                    let x = loword_signed(lp.0 as isize);
                    let y = hiword_signed(lp.0 as isize);
                    let mut rc = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rc);
                    let e = edge_at(&rc, x, y);
                    let cur = match e {
                        1 | 2 => LoadCursorW(None, IDC_SIZEWE).unwrap_or_default(),
                        4 | 8 => LoadCursorW(None, IDC_SIZENS).unwrap_or_default(),
                        5 | 10 => LoadCursorW(None, IDC_SIZENWSE).unwrap_or_default(),
                        6 | 9 => LoadCursorW(None, IDC_SIZENESW).unwrap_or_default(),
                        _ => HCURSOR::default(),
                    };
                    if !cur.0.is_null() {
                        let _ = SetCursor(cur);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let s = state(hwnd);
            if !s.is_null() && (*s).dragging {
                (*s).dragging = false;
                let _ = ReleaseCapture();
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // default cursor ok
            DefWindowProcW(hwnd, msg, wp, lp)
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

// ---- public mutators (called by app commands) ----

pub fn select(hwnd: HWND, on: bool) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).selected = on;
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

pub fn inner_edit(hwnd: HWND) -> HWND {
    unsafe {
        let s = state(hwnd);
        if s.is_null() {
            HWND::default()
        } else {
            (*s).edit
        }
    }
}

pub fn set_fill(hwnd: HWND, color: COLORREF, has_fill: bool) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).props.fill = color;
            (*s).props.has_fill = has_fill;
            SendMessageW((*s).edit, EM_SETBKGNDCOLOR, WPARAM(0), LPARAM(color.0 as isize));
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

pub fn set_outline(hwnd: HWND, color: COLORREF, width_px: i32, has_outline: bool) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).props.outline = color;
            (*s).props.outline_w = width_px;
            (*s).props.has_outline = has_outline;
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

/// Set inner text margins in points (Word dialog: Left/Right/Top/Bottom in inches;
/// pass 0 to zero them out).
pub fn set_insets(hwnd: HWND, l: i32, t: i32, r: i32, b: i32) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).props.inset_l = l;
            (*s).props.inset_t = t;
            (*s).props.inset_r = r;
            (*s).props.inset_b = b;
            layout_edit(hwnd);
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

pub fn get_props(hwnd: HWND) -> Option<ShapeProps> {
    unsafe {
        let s = state(hwnd);
        if s.is_null() {
            None
        } else {
            Some((*s).props.clone())
        }
    }
}

pub fn set_zoom(hwnd: HWND, zoom_pct: i32) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).zoom_pct = zoom_pct;
            layout_edit(hwnd);
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}
