//! Word-like ribbon: tab strip + groups of buttons + embedded font controls.
//! Communicates with the app through WM_COMMAND on the main window.

use crate::commands::*;
use crate::format::FormatState;
use crate::theme::*;
use crate::util::{loword_signed, hiword_signed, to_w};
use std::cell::Cell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Shell::{SetWindowSubclass, DefSubclassProc};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const TAB_STRIP_H: i32 = 32;
pub const RIBBON_H: i32 = 96;
pub const TOTAL_H: i32 = TAB_STRIP_H + RIBBON_H;
const GROUP_LABEL_H: i32 = 16;

const CLASS: PCWSTR = w!("RWE_Ribbon");

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Large, // full-height button, glyph over label
    Small, // 3-per-column glyph+label
    Combo, // embedded control
    Sep,
}

#[derive(Clone)]
struct Item {
    id: u32,
    glyph: &'static str,
    label: &'static str,
    style: Style,
    dropdown: bool,
    toggle: bool,
    combo: Option<ComboKind>,
}

#[derive(Clone, Copy, PartialEq)]
enum ComboKind {
    FontName,
    FontSize,
}

struct Group {
    label: &'static str,
    items: Vec<Item>,
}

struct Tab {
    name: &'static str,
    contextual: bool,
    groups: Vec<Group>,
}

fn bi(id: u32, glyph: &'static str, label: &'static str) -> Item {
    Item { id, glyph, label, style: Style::Small, dropdown: false, toggle: false, combo: None }
}
fn big(id: u32, glyph: &'static str, label: &'static str) -> Item {
    Item { id, glyph, label, style: Style::Large, dropdown: false, toggle: false, combo: None }
}
fn tgl(id: u32, glyph: &'static str, label: &'static str) -> Item {
    Item { id, glyph, label, style: Style::Small, dropdown: false, toggle: true, combo: None }
}
fn dd(id: u32, glyph: &'static str, label: &'static str) -> Item {
    Item { id, glyph, label, style: Style::Small, dropdown: true, toggle: false, combo: None }
}
fn dd_big(id: u32, glyph: &'static str, label: &'static str) -> Item {
    Item { id, glyph, label, style: Style::Large, dropdown: true, toggle: false, combo: None }
}
fn tgl_dd(id: u32, glyph: &'static str, label: &'static str) -> Item {
    Item { id, glyph, label, style: Style::Small, dropdown: true, toggle: true, combo: None }
}
fn combo(id: u32, kind: ComboKind, w_hint: &'static str) -> Item {
    Item { id, glyph: "", label: w_hint, style: Style::Combo, dropdown: false, toggle: false, combo: Some(kind) }
}
fn sep() -> Item {
    Item { id: 0, glyph: "", label: "", style: Style::Sep, dropdown: false, toggle: false, combo: None }
}

fn build_tabs() -> Vec<Tab> {
    vec![
        Tab {
            name: "Home",
            contextual: false,
            groups: vec![
                Group {
                    label: "Clipboard",
                    items: vec![
                        big(IDM_PASTE, "P", "Paste"),
                        bi(IDM_CUT, "X", "Cut"),
                        bi(IDM_COPY, "C", "Copy"),
                    ],
                },
                Group {
                    label: "Font",
                    items: vec![
                        combo(IDC_FONT_NAME, ComboKind::FontName, "fontname"),
                        combo(IDC_FONT_SIZE, ComboKind::FontSize, "size"),
                        sep(),
                        bi(IDM_FONT_GROW, "A+", ""),
                        bi(IDM_FONT_SHRINK, "A-", ""),
                        bi(IDM_CLEAR_FORMAT, "Ax", ""),
                        sep(),
                        tgl(IDM_BOLD, "B", ""),
                        tgl(IDM_ITALIC, "I", ""),
                        tgl_dd(IDM_UNDERLINE, "U", ""),
                        tgl(IDM_STRIKE, "S", ""),
                        tgl(IDM_SUBSCRIPT, "X2", ""),
                        tgl(IDM_SUPERSCRIPT, "X²", ""),
                        dd(IDM_TEXT_EFFECTS, "A~", ""),
                        dd(IDM_TEXT_HIGHLIGHT, "ab▮", ""),
                        dd(IDM_FONT_COLOR, "A▮", ""),
                        bi(IDM_FONT_DIALOG, "↘", ""),
                    ],
                },
                Group {
                    label: "Paragraph",
                    items: vec![
                        tgl_dd(IDM_BULLETS, "•≡", ""),
                        tgl_dd(IDM_NUMBERING, "1≡", ""),
                        bi(IDM_INDENT_DEC, "◀≡", ""),
                        bi(IDM_INDENT_INC, "▶≡", ""),
                        tgl(IDM_SHOW_MARKS, "¶", ""),
                        sep(),
                        tgl(IDM_ALIGN_LEFT, "☰", ""),
                        tgl(IDM_ALIGN_CENTER, "☰", ""),
                        tgl(IDM_ALIGN_RIGHT, "☰", ""),
                        tgl(IDM_ALIGN_JUSTIFY, "☰", ""),
                        dd(IDM_LINE_SPACING, "↕≡", ""),
                        dd(IDM_SHADING, "▨", ""),
                        dd(IDM_BORDERS, "▢", ""),
                        dd(IDM_WORD_SPACING, "a_b", ""),
                        bi(IDM_PARA_DIALOG, "↘", ""),
                    ],
                },
                Group {
                    label: "Editing",
                    items: vec![bi(IDM_SELECT_ALL, "⇲", "Select All")],
                },
            ],
        },
        Tab {
            name: "Insert",
            contextual: false,
            groups: vec![
                Group {
                    label: "Pages",
                    items: vec![big(IDM_PAGE_BREAK, "▤", "Page Break")],
                },
                Group {
                    label: "Illustrations",
                    items: vec![
                        dd_big(IDM_SHAPES, "⬠", "Shapes"),
                        big(IDM_PICTURE, "🖼", "Picture"),
                    ],
                },
                Group {
                    label: "Text",
                    items: vec![
                        big(IDM_TEXT_BOX, "▭", "Text Box"),
                        bi(IDM_DATE_TIME, "🕐", "Date & Time"),
                        bi(IDM_SYMBOL, "Ω", "Symbol"),
                    ],
                },
            ],
        },
        Tab {
            name: "Layout",
            contextual: false,
            groups: vec![
                Group {
                    label: "Page Setup",
                    items: vec![
                        dd(IDM_MARGINS, "▤", "Margins"),
                        dd(IDM_ORIENTATION, "▯", "Orientation"),
                        dd(IDM_PAGE_SIZE, "⬚", "Size"),
                    ],
                },
                Group {
                    label: "Paragraph",
                    items: vec![
                        bi(IDM_INDENT_DEC, "◀≡", "Decrease Indent"),
                        bi(IDM_INDENT_INC, "▶≡", "Increase Indent"),
                        bi(IDM_PARA_DIALOG, "↘", "Spacing…"),
                    ],
                },
            ],
        },
        Tab {
            name: "Shape Format",
            contextual: true,
            groups: vec![
                Group {
                    label: "Shape Styles",
                    items: vec![
                        dd(IDM_SHAPE_FILL, "⬒", "Shape Fill"),
                        dd(IDM_SHAPE_OUTLINE, "◻", "Shape Outline"),
                        bi(IDM_SHAPE_MARGIN_DLG, "⇹", "Text Margins"),
                    ],
                },
                Group {
                    label: "Arrange",
                    items: vec![
                        bi(IDM_SHAPE_BRING_FWD, "⬆", "Bring Forward"),
                        bi(IDM_SHAPE_SEND_BACK, "⬇", "Send Backward"),
                        bi(IDM_SHAPE_DELETE, "✕", "Delete"),
                    ],
                },
            ],
        },
    ]
}

#[derive(Clone, Copy)]
struct BtnSpot {
    rect: RECT,
    tab: usize,
    group: usize,
    item: usize,
    split: bool, // the ▼ part of a dropdown button
}

pub struct RibbonState {
    tabs: Vec<Tab>,
    active: usize,
    shape_mode: bool,
    spots: Vec<BtnSpot>,
    hot: isize,
    pressed: isize,
    width: i32,
    font_combo: Cell<HWND>,
    size_combo: Cell<HWND>,
    syncing: bool,
    pub st: FormatState,
}

pub fn register(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0),
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

pub fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    unsafe {
        let st = Box::new(RibbonState {
            tabs: build_tabs(),
            active: 0,
            shape_mode: false,
            spots: Vec::new(),
            hot: -1,
            pressed: -1,
            width: 0,
            font_combo: Cell::new(HWND::default()),
            size_combo: Cell::new(HWND::default()),
            syncing: false,
            st: FormatState::default(),
        });
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            0,
            0,
            100,
            TOTAL_H,
            parent,
            HMENU::default(),
            hinst,
            Some(Box::into_raw(st) as _),
        )
        .unwrap_or_default()
    }
}

unsafe fn state(hwnd: HWND) -> *mut RibbonState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RibbonState
}

// ---------- helpers ----------

fn visible_tabs<'a>(s: &'a RibbonState) -> Vec<(usize, &'a Tab)> {
    s.tabs
        .iter()
        .enumerate()
        .filter(|(i, t)| !t.contextual || s.shape_mode && *i >= 0 || !t.contextual)
        .filter(|(_, t)| !t.contextual || s.shape_mode)
        .collect()
}

fn is_checked(id: u32, st: &FormatState) -> bool {
    match id {
        IDM_BOLD => st.bold,
        IDM_ITALIC => st.italic,
        IDM_UNDERLINE => st.underline,
        IDM_STRIKE => st.strike,
        IDM_SUBSCRIPT => st.subscript,
        IDM_SUPERSCRIPT => st.superscript,
        IDM_ALIGN_LEFT => st.alignment == 0,
        IDM_ALIGN_CENTER => st.alignment == 1,
        IDM_ALIGN_RIGHT => st.alignment == 2,
        IDM_ALIGN_JUSTIFY => st.alignment == 3,
        IDM_BULLETS => st.list_type != 0,
        IDM_SHOW_MARKS => st.show_marks,
        _ => false,
    }
}

fn enum_fonts(list: &mut Vec<String>) {
    unsafe {
        extern "system" fn cb(lf: *const LOGFONTW, _tm: *const TEXTMETRICW, _t: u32, lp: LPARAM) -> i32 {
            unsafe {
                let name: &[u16] = {
                    let raw = &(*lf).lfFaceName;
                    let n = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                    &raw[..n]
                };
                let s = String::from_utf16_lossy(name);
                if !s.starts_with('@') {
                    let v = &mut *(lp.0 as *mut Vec<String>);
                    if !v.contains(&s) {
                        v.push(s);
                    }
                }
            }
            1
        }
        let dc = GetDC(None);
        let mut lf = LOGFONTW::default();
        lf.lfCharSet = DEFAULT_CHARSET;
        EnumFontFamiliesExW(dc, &lf, Some(cb), LPARAM(list as *mut _ as isize), 0);
        let _ = ReleaseDC(None, dc);
        list.sort();
    }
}

fn ensure_combos(hwnd: HWND, s: &RibbonState, hinst: HINSTANCE) {
    unsafe {
        if s.font_combo.get().0.is_null() {
            let cb = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("COMBOBOX"),
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWN as u32),
                0,
                0,
                170,
                200,
                hwnd,
                HMENU(IDC_FONT_NAME as _),
                hinst,
                None,
            )
            .unwrap_or_default();
            s.font_combo.set(cb);
            let mut fonts = Vec::new();
            enum_fonts(&mut fonts);
            for f in fonts {
                let w = to_w(&f);
                SendMessageW(cb, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
            }
            subclass_combo(cb, IDC_FONT_NAME);
        }
        if s.size_combo.get().0.is_null() {
            let cb = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("COMBOBOX"),
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWN as u32),
                0,
                0,
                52,
                300,
                hwnd,
                HMENU(IDC_FONT_SIZE as _),
                hinst,
                None,
            )
            .unwrap_or_default();
            s.size_combo.set(cb);
            for sz in [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72] {
                let w = to_w(&sz.to_string());
                SendMessageW(cb, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
            }
            subclass_combo(cb, IDC_FONT_SIZE);
        }
    }
}

static mut NEXT_SUBCLASS: usize = 1;

fn subclass_combo(cb: HWND, id: u32) {
    unsafe {
        extern "system" fn proc(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _id: usize,
            data: usize,
        ) -> LRESULT {
            unsafe {
                if msg == WM_KEYDOWN && wp.0 == VK_RETURN.0 as usize {
                    let parent = GetParent(hwnd).unwrap_or_default();
                    // forward as CBN_SELCHANGE so the app applies the typed value
                    SendMessageW(
                        GetParent(parent).unwrap_or(parent),
                        WM_COMMAND,
                        WPARAM(((CBN_SELCHANGE as usize) << 16) | data),
                        LPARAM(hwnd.0 as isize),
                    );
                    return LRESULT(0);
                }
                DefSubclassProc(hwnd, msg, wp, lp)
            }
        }
        let _ = SetWindowSubclass(cb, Some(proc), NEXT_SUBCLASS, id as usize);
        NEXT_SUBCLASS += 1;
    }
}

// ---------- layout & painting ----------

fn layout_tab_strip(s: &RibbonState, w: i32) -> Vec<(RECT, usize)> {
    // File button + tab rects
    let mut v = Vec::new();
    v.push((RECT { left: 6, top: 4, right: 56, bottom: TAB_STRIP_H - 2 }, usize::MAX)); // File
    let mut x = 60;
    for (i, t) in s.tabs.iter().enumerate() {
        if t.contextual && !s.shape_mode {
            continue;
        }
        let tw = t.name.len() as i32 * 7 + 22;
        v.push((
            RECT { left: x, top: 6, right: x + tw, bottom: TAB_STRIP_H },
            i,
        ));
        x += tw + 2;
    }
    let _ = w;
    v
}

fn layout_body(s: &mut RibbonState, w: i32) {
    s.spots.clear();
    let tab = match s.tabs.get(s.active) {
        Some(t) => t,
        None => return,
    };
    if tab.contextual && !s.shape_mode {
        return;
    }
    let mut x = 8;
    let body_top = TAB_STRIP_H;
    let body_h = RIBBON_H - GROUP_LABEL_H;
    for (gi, g) in tab.groups.iter().enumerate() {
        let gx = x;
        let mut col_x = gx;
        let mut row = 0;
        for (ii, it) in g.items.iter().enumerate() {
            match it.style {
                Style::Large => {
                    let r = RECT {
                        left: col_x,
                        top: body_top + 2,
                        right: col_x + 46,
                        bottom: body_top + body_h - 2,
                    };
                    s.spots.push(BtnSpot { rect: r, tab: s.active, group: gi, item: ii, split: false });
                    col_x += 48;
                }
                Style::Small => {
                    if row == 3 {
                        row = 0;
                        col_x += 30;
                    }
                    let bw = if it.label.is_empty() { 26 } else { 26 + it.label.len() as i32 * 6 };
                    let mut r = RECT {
                        left: col_x,
                        top: body_top + 2 + row * 25,
                        right: col_x + bw + 8,
                        bottom: body_top + 2 + row * 25 + 23,
                    };
                    if it.dropdown {
                        r.right += 10;
                    }
                    s.spots.push(BtnSpot { rect: r, tab: s.active, group: gi, item: ii, split: false });
                    if it.dropdown {
                        let mut sr = r;
                        sr.left = r.right - 12;
                        s.spots.push(BtnSpot { rect: sr, tab: s.active, group: gi, item: ii, split: true });
                    }
                    row += 1;
                }
                Style::Combo | Style::Sep => {}
            }
        }
        // combos/seps are laid out after buttons: put combos at group start? Word has
        // font combos on first row; we position them on reserved spots during paint.
        // compute group width:
        col_x += 4;
        let mut max_right = col_x;
        for (ii, it) in g.items.iter().enumerate() {
            match it.combo {
                Some(ComboKind::FontName) => {
                    s.spots.push(BtnSpot {
                        rect: RECT { left: gx, top: body_top + 2, right: gx + 176, bottom: body_top + 25 },
                        tab: s.active, group: gi, item: ii, split: false,
                    });
                }
                Some(ComboKind::FontSize) => {
                    s.spots.push(BtnSpot {
                        rect: RECT { left: gx + 178, top: body_top + 2, right: gx + 236, bottom: body_top + 25 },
                        tab: s.active, group: gi, item: ii, split: false,
                    });
                }
                None => {}
            }
            let _ = it;
        }
        for sp in s.spots.iter() {
            if sp.group == gi && sp.tab == s.active {
                max_right = max_right.max(sp.rect.right);
            }
        }
        x = max_right + 10;
        let _ = w;
    }
}

unsafe fn draw_text(hdc: HDC, s: &str, r: &mut RECT, fmt: DRAW_TEXT_FORMAT, color: COLORREF) {
    SetTextColor(hdc, color);
    let mut buf = to_w(s);
    let n = buf.len().saturating_sub(1);
    DrawTextW(hdc, &mut buf[..n], r, fmt);
}

unsafe fn paint(hwnd: HWND) {
    let s = &mut *state(hwnd);
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    if w != s.width {
        s.width = w;
    }
    layout_body(s, w);
    ensure_combos(hwnd, s, GetModuleHandleW(None).unwrap_or_default().into());

    SetBkMode(hdc, TRANSPARENT);

    // --- tab strip ---
    let strip = RECT { left: 0, top: 0, right: w, bottom: TAB_STRIP_H };
    let b = CreateSolidBrush(TITLE_BAR);
    FillRect(hdc, &strip, b);
    let _ = DeleteObject(b);

    for (r, idx) in layout_tab_strip(s, w) {
        if idx == usize::MAX {
            let fb = CreateSolidBrush(FILE_TAB);
            FillRect(hdc, &r, fb);
            let _ = DeleteObject(fb);
            let mut tr = r;
            draw_text(hdc, "File", &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE, FILE_TAB_TEXT);
        } else {
            let t = &s.tabs[idx];
            if idx == s.active {
                let ab = CreateSolidBrush(TAB_ACTIVE);
                FillRect(hdc, &r, ab);
                let _ = DeleteObject(ab);
            } else if t.contextual {
                let cb = CreateSolidBrush(rgb(0xF8, 0xE5, 0xB0));
                FillRect(hdc, &r, cb);
                let _ = DeleteObject(cb);
            }
            let mut tr = r;
            draw_text(hdc, t.name, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE, if t.contextual { TAB_TEXT_HOT } else { TAB_TEXT });
        }
    }

    // --- ribbon body ---
    let body = RECT { left: 0, top: TAB_STRIP_H, right: w, bottom: TOTAL_H };
    let b = CreateSolidBrush(RIBBON_BG);
    FillRect(hdc, &body, b);
    let _ = DeleteObject(b);

    if let Some(tab) = s.tabs.get(s.active) {
        if !tab.contextual || s.shape_mode {
            // group separators + labels
            let mut gi_right: Vec<(i32, &str, i32, i32)> = Vec::new();
            for gi in 0..tab.groups.len() {
                let mut min_x = i32::MAX;
                let mut max_x = 0;
                for sp in s.spots.iter() {
                    if sp.group == gi && sp.tab == s.active {
                        min_x = min_x.min(sp.rect.left);
                        max_x = max_x.max(sp.rect.right);
                    }
                }
                if max_x > 0 {
                    gi_right.push((max_x + 6, tab.groups[gi].label, min_x, max_x));
                }
            }
            let pen = CreatePen(PS_SOLID, 1, GROUP_SEP);
            let old = SelectObject(hdc, pen);
            for (x, _, _, _) in &gi_right {
                MoveToEx(hdc, *x, TAB_STRIP_H + 4, None);
                LineTo(hdc, *x, TOTAL_H - GROUP_LABEL_H - 4);
            }
            SelectObject(hdc, old);
            let _ = DeleteObject(pen);
            for (_, label, min_x, max_x) in gi_right {
                let mut lr = RECT {
                    left: min_x,
                    top: TOTAL_H - GROUP_LABEL_H,
                    right: max_x + 6,
                    bottom: TOTAL_H - 1,
                };
                draw_text(hdc, label, &mut lr, DT_CENTER | DT_VCENTER | DT_SINGLELINE, GROUP_LABEL);
            }
        }
    }

    // --- buttons ---
    let spots = s.spots.clone_rects();
    for (i, sp) in spots.iter().enumerate() {
        let it = &s.tabs[sp.tab].groups[sp.group].items[sp.item];
        match it.style {
            Style::Combo => {
                // move/resize the real control
                let cb = match it.combo {
                    Some(ComboKind::FontName) => s.font_combo.get(),
                    Some(ComboKind::FontSize) => s.size_combo.get(),
                    None => HWND::default(),
                };
                if !cb.0.is_null() {
                    let _ = SetWindowPos(
                        cb,
                        HWND_TOP,
                        sp.rect.left,
                        sp.rect.top,
                        sp.rect.right - sp.rect.left,
                        sp.rect.bottom - sp.rect.top,
                        SWP_NOACTIVATE,
                    );
                }
            }
            Style::Sep => {}
            _ => {
                if sp.split {
                    // only dropdown arrow zone
                    if s.hot == i as isize || s.pressed == i as isize {
                        let hb = CreateSolidBrush(BTN_HOT);
                        FillRect(hdc, &sp.rect, hb);
                        let _ = DeleteObject(hb);
                    }
                    let mut tr = sp.rect;
                    draw_text(hdc, "▾", &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE, BTN_TEXT);
                } else {
                    let checked = it.toggle && is_checked(it.id, &s.st);
                    if checked {
                        let cb = CreateSolidBrush(BTN_CHECKED);
                        FillRect(hdc, &sp.rect, cb);
                        let _ = DeleteObject(cb);
                        let pb = CreateSolidBrush(BTN_CHECKED_BORDER);
                        FrameRect(hdc, &sp.rect, pb);
                        let _ = DeleteObject(pb);
                    } else if s.hot == i as isize || s.pressed == i as isize {
                        let hb = CreateSolidBrush(BTN_HOT);
                        FillRect(hdc, &sp.rect, hb);
                        let _ = DeleteObject(hb);
                        let pb = CreateSolidBrush(BTN_HOT_BORDER);
                        FrameRect(hdc, &sp.rect, pb);
                        let _ = DeleteObject(pb);
                    }
                    match it.style {
                        Style::Large => {
                            let mut gr = sp.rect;
                            gr.top += 8;
                            gr.bottom = gr.top + 30;
                            draw_text(hdc, it.glyph, &mut gr, DT_CENTER | DT_VCENTER | DT_SINGLELINE, ACCENT);
                            let mut lr = sp.rect;
                            lr.top = gr.bottom + 4;
                            draw_text(hdc, it.label, &mut lr, DT_CENTER | DT_TOP | DT_WORDBREAK, BTN_TEXT);
                            if it.dropdown {
                                let mut ar = sp.rect;
                                ar.top = ar.bottom - 14;
                                draw_text(hdc, "▾", &mut ar, DT_CENTER | DT_VCENTER | DT_SINGLELINE, BTN_TEXT);
                            }
                        }
                        _ => {
                            let mut r = sp.rect;
                            r.left += 4;
                            let mut gr = r;
                            gr.right = gr.left + 20;
                            draw_text(hdc, it.glyph, &mut gr, DT_CENTER | DT_VCENTER | DT_SINGLELINE, ACCENT);
                            let mut lr = r;
                            lr.left += 20;
                            lr.right -= if it.dropdown { 12 } else { 2 };
                            draw_text(hdc, it.label, &mut lr, DT_LEFT | DT_VCENTER | DT_SINGLELINE, BTN_TEXT);
                            if it.dropdown {
                                let mut ar = r;
                                ar.left = ar.right - 10;
                                draw_text(hdc, "▾", &mut ar, DT_CENTER | DT_VCENTER | DT_SINGLELINE, BTN_TEXT);
                            }
                        }
                    }
                }
            }
        }
    }
    let _ = EndPaint(hwnd, &ps);
}

// small helper since BtnSpot holds RECT (Copy)
trait CloneRects {
    fn clone_rects(&self) -> Vec<BtnSpot>;
}
impl CloneRects for Vec<BtnSpot> {
    fn clone_rects(&self) -> Vec<BtnSpot> {
        self.iter()
            .map(|b| BtnSpot { rect: b.rect, tab: b.tab, group: b.group, item: b.item, split: b.split })
            .collect()
    }
}

fn hit_spot(s: &RibbonState, x: i32, y: i32) -> isize {
    for (i, sp) in s.spots.iter().enumerate() {
        if sp.tab != s.active {
            continue;
        }
        let it = &s.tabs[sp.tab].groups[sp.group].items[sp.item];
        if it.style == Style::Combo {
            continue;
        }
        let r = &sp.rect;
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return i as isize;
        }
    }
    -1
}

fn hit_tab(s: &RibbonState, x: i32, y: i32, w: i32) -> isize {
    for (i, (r, _)) in layout_tab_strip(s, w).iter().enumerate() {
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return i as isize;
        }
    }
    -1
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lp.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            let s = state(hwnd);
            if !s.is_null() {
                let _ = InvalidateRect(hwnd, None, FALSE);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let s = state(hwnd);
            if !s.is_null() {
                let st = &mut *s;
                let x = loword_signed(lp.0 as isize);
                let y = hiword_signed(lp.0 as isize);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let h = if y < TAB_STRIP_H {
                    hit_tab(st, x, y, rc.right)
                } else {
                    hit_spot(st, x, y)
                };
                if st.hot != h {
                    st.hot = h;
                    let _ = InvalidateRect(hwnd, None, FALSE);
                }
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let s = state(hwnd);
            if !s.is_null() {
                (*s).hot = -1;
                let _ = InvalidateRect(hwnd, None, FALSE);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let s = state(hwnd);
            if !s.is_null() {
                let st = &mut *s;
                let x = loword_signed(lp.0 as isize);
                let y = hiword_signed(lp.0 as isize);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                if y < TAB_STRIP_H {
                    let i = hit_tab(st, x, y, rc.right);
                    if i >= 0 {
                        let (r, idx) = layout_tab_strip(st, rc.right)[i as usize];
                        let _ = r;
                        if idx == usize::MAX {
                            // File → backstage
                            SendMessageW(
                                GetParent(hwnd).unwrap_or_default(),
                                WM_COMMAND,
                                WPARAM((BN_CLICKED as usize) << 16 | (IDM_FILE as usize & 0xffff)),
                                LPARAM(0),
                            );
                        } else {
                            st.active = idx;
                            st.hot = -1;
                            let _ = InvalidateRect(hwnd, None, FALSE);
                        }
                    }
                } else {
                    let i = hit_spot(st, x, y);
                    if i >= 0 {
                        st.pressed = i;
                        let sp = st.spots[i as usize];
                        let it = &st.tabs[sp.tab].groups[sp.group].items[sp.item];
                        let mut scr = sp.rect;
                        let mut p1 = POINT { x: scr.left, y: scr.bottom };
                        let _ = ClientToScreen(hwnd, &mut p1);
                        scr.left = p1.x;
                        scr.bottom = p1.y;
                        SendMessageW(
                            GetParent(hwnd).unwrap_or_default(),
                            WM_COMMAND,
                            WPARAM((BN_CLICKED as usize) << 16 | (it.id as usize & 0xffff)),
                            LPARAM((scr.bottom as isize) << 32 | (scr.left as isize & 0xffffffff)),
                        );
                        st.pressed = -1;
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            // combobox notifications land on the ribbon (their parent): forward to main
            let parent = GetParent(hwnd).unwrap_or_default();
            SendMessageW(parent, msg, wp, lp)
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

// ---- public API used by app ----

pub fn set_active_tab(hwnd: HWND, idx: usize) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            (*s).active = idx;
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    }
}

/// Index of the Shape Format tab (4)
pub const SHAPE_TAB: usize = 3;

pub fn set_shape_mode(hwnd: HWND, on: bool) {
    unsafe {
        let s = state(hwnd);
        if !s.is_null() {
            let st = &mut *s;
            if st.shape_mode != on {
                st.shape_mode = on;
                if on {
                    st.active = SHAPE_TAB;
                } else if st.tabs.get(st.active).map(|t| t.contextual).unwrap_or(false) {
                    st.active = 0;
                }
                let _ = InvalidateRect(hwnd, None, FALSE);
            }
        }
    }
}

pub fn sync_state(hwnd: HWND, st: &FormatState) {
    unsafe {
        let s = state(hwnd);
        if s.is_null() {
            return;
        }
        let rs = &mut *s;
        rs.syncing = true;
        rs.st = st.clone();
        let cb = rs.font_combo.get();
        if !cb.0.is_null() {
            let w = to_w(&st.font_name);
            SetWindowTextW(cb, PCWSTR(w.as_ptr()));
        }
        let sb = rs.size_combo.get();
        if !sb.0.is_null() {
            let txt = if st.font_size_pt.fract() == 0.0 {
                format!("{}", st.font_size_pt as i32)
            } else {
                format!("{:.1}", st.font_size_pt)
            };
            let w = to_w(&txt);
            SetWindowTextW(sb, PCWSTR(w.as_ptr()));
        }
        rs.syncing = false;
        let _ = InvalidateRect(hwnd, None, FALSE);
    }
}

pub fn is_syncing(hwnd: HWND) -> bool {
    unsafe {
        let s = state(hwnd);
        !s.is_null() && (*s).syncing
    }
}

pub fn combo_text(hwnd: HWND, id: u32) -> String {
    unsafe {
        let s = state(hwnd);
        if s.is_null() {
            return String::new();
        }
        let cb = match id {
            IDC_FONT_NAME => (*s).font_combo.get(),
            IDC_FONT_SIZE => (*s).size_combo.get(),
            _ => HWND::default(),
        };
        if cb.0.is_null() {
            return String::new();
        }
        let len = GetWindowTextLengthW(cb) + 1;
        let mut buf = vec![0u16; len as usize];
        let n = GetWindowTextW(cb, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}
