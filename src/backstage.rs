//! File backstage overlay: covers the client area with File commands.

use crate::commands::*;
use crate::theme::*;
use crate::util::{loword_signed, hiword_signed, to_w};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("RWE_Backstage");

struct Item {
    id: u32,
    label: &'static str,
}

const ITEMS: &[Item] = &[
    Item { id: IDM_BACK, label: "← Back" },
    Item { id: IDM_NEW, label: "New" },
    Item { id: IDM_OPEN, label: "Open" },
    Item { id: IDM_SAVE, label: "Save" },
    Item { id: IDM_SAVE_AS, label: "Save As" },
    Item { id: IDM_EXIT, label: "Exit" },
];

pub struct BackState {
    pub hot: isize,
    pub doc_name: String,
}

pub fn register(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0),
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

pub fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    unsafe {
        let st = Box::new(BackState { hot: -1, doc_name: "Document".into() });
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR::null(),
            WS_CHILD | WS_CLIPCHILDREN,
            0,
            0,
            0,
            0,
            parent,
            HMENU::default(),
            hinst,
            Some(Box::into_raw(st) as _),
        )
        .unwrap_or_default()
    }
}

unsafe fn state(hwnd: HWND) -> *mut BackState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut BackState
}

fn item_rect(i: usize) -> RECT {
    let idx = i as i32;
    if idx == 0 {
        // back arrow big tile
        RECT { left: 0, top: 0, right: 260, bottom: 64 }
    } else {
        RECT { left: 0, top: 64 + (idx - 1) * 44, right: 260, bottom: 64 + idx * 44 }
    }
}

fn hit(x: i32, y: i32) -> isize {
    for i in 0..ITEMS.len() {
        let r = item_rect(i);
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return i as isize;
        }
    }
    -1
}

unsafe fn paint(hwnd: HWND) {
    let s = &*state(hwnd);
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // left column — Word 365 backstage: deep blue nav, white items
    let col = RECT { left: 0, top: 0, right: 260, bottom: rc.bottom };
    let b = CreateSolidBrush(ACCENT);
    FillRect(hdc, &col, b);
    let _ = DeleteObject(b);
    // right side: white stays
    let body = RECT { left: 260, top: 0, right: rc.right, bottom: rc.bottom };
    let wb = CreateSolidBrush(PAGE_WHITE);
    FillRect(hdc, &body, wb);
    let _ = DeleteObject(wb);

    SetBkMode(hdc, TRANSPARENT);
    for (i, it) in ITEMS.iter().enumerate() {
        let r = item_rect(i);
        if s.hot == i as isize {
            let hb = CreateSolidBrush(rgb(0x2F, 0x6B, 0xCD)); // lighter blue hover on blue nav
            FillRect(hdc, &r, hb);
            let _ = DeleteObject(hb);
        }
        let mut tr = r;
        tr.left += 24;
        SetTextColor(hdc, PAGE_WHITE);
        let mut buf = to_w(it.label);
        buf.pop();
        DrawTextW(hdc, &mut buf, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    }
    // doc title on right
    let mut tr = RECT { left: 300, top: 24, right: rc.right - 40, bottom: 60 };
    SetTextColor(hdc, BTN_TEXT);
    let mut buf = to_w(&s.doc_name);
    buf.pop();
    DrawTextW(hdc, &mut buf, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    let _ = EndPaint(hwnd, &ps);
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
            let s = state(hwnd);
            if !s.is_null() {
                let h = hit(loword_signed(lp.0 as isize), hiword_signed(lp.0 as isize));
                if (*s).hot != h {
                    (*s).hot = h;
                    let _ = InvalidateRect(hwnd, None, FALSE);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let s = state(hwnd);
            if !s.is_null() {
                let h = hit(loword_signed(lp.0 as isize), hiword_signed(lp.0 as isize));
                if h >= 0 {
                    let it = &ITEMS[h as usize];
                    let parent = GetParent(hwnd).unwrap_or_default();
                    SendMessageW(parent, WM_COMMAND, WPARAM((BN_CLICKED as usize) << 16 | it.id as usize), LPARAM(0));
                }
            }
            LRESULT(0)
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

pub fn set_doc_name(hwnd: HWND, name: &str) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).doc_name = name.into();
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}
