//! Main window: page canvas, layout, command dispatch, dropdown builders,
//! backstage toggle, shape draw mode, file I/O hooks.

use crate::backstage;
use crate::commands::*;
use crate::docx;
use crate::dropdown::{self, DropIcon, DropItem, ShapeKind, WM_APP_DROPDOWN_PICK};
use crate::editor::{self, tom_document};
use crate::format::{self, FormatState};
use crate::ribbon;
use crate::shape::{self, WM_APP_SHAPE_FOCUS_EDIT, WM_APP_SHAPE_SELECT};
use crate::statusbar;
use crate::theme::*;
use crate::util::{loword_signed, hiword_signed, pt_to_twips, to_w};
use std::path::Path;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::UI::Controls::ICC_STANDARD_CLASSES;
use windows::Win32::UI::Controls::INITCOMMONCONTROLSEX;
use windows::Win32::UI::Controls::InitCommonControlsEx;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{SetWindowSubclass, DefSubclassProc, RemoveWindowSubclass};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Ole::CF_BITMAP;

const MK_LBUTTON_FLAG: u32 = 0x0001;
use windows::Win32::UI::WindowsAndMessaging::*;

const MAIN_CLASS: PCWSTR = w!("RWE_Main");

// page sizes in twips
const A4_W: i32 = 11906;
const A4_H: i32 = 16838;
const LETTER_W: i32 = 12240;
const LETTER_H: i32 = 15840;
const LEGAL_W: i32 = 12240;
const LEGAL_H: i32 = 20160;

#[derive(Clone)]
pub struct PageSetup {
    pub w_tw: i32,
    pub h_tw: i32,
    pub ml_tw: i32,
    pub mr_tw: i32,
    pub mt_tw: i32,
    pub mb_tw: i32,
    pub landscape: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            w_tw: A4_W,
            h_tw: A4_H,
            ml_tw: 1440,
            mr_tw: 1440,
            mt_tw: 1440,
            mb_tw: 1440,
            landscape: false,
        }
    }
}

pub struct App {
    pub hwnd: HWND,
    pub ribbon: HWND,
    pub editor: HWND,
    pub status: HWND,
    pub backstage: HWND,
    pub hinst: HINSTANCE,
    pub page: PageSetup,
    pub zoom_pct: i32,
    pub shapes: Vec<HWND>,
    pub focus_edit: HWND,       // editor hwnd that last had focus
    pub selected_shape: Option<HWND>,
    pub draw_mode: bool,
    pub drawing: bool,
    pub draw_kind: ShapeKind,
    pub draw_start: POINT,
    pub draw_cur: POINT,
    pub file_path: Option<String>,
    pub show_marks: bool,
    pub backstage_on: bool,
    pub custom_colors: [COLORREF; 16],
    pub last_font_color: COLORREF,
    pub last_highlight: COLORREF,
}

static mut APP_PTR: *mut App = std::ptr::null_mut();

pub fn app() -> &'static mut App {
    unsafe { &mut *APP_PTR }
}

pub fn register(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0 | CS_DBLCLKS.0),
            lpfnWndProc: Some(main_wndproc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            lpszClassName: MAIN_CLASS,
            lpszMenuName: PCWSTR::null(),
            ..Default::default()
        };
        RegisterClassW(&wc);

    }
}

pub fn run(hinst: HINSTANCE) -> windows::core::Result<()> {
    unsafe {
        let _ = OleInitialize(None);
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);

        editor::load_richedit()?;
        register(hinst);
        ribbon::register(hinst);
        statusbar::register(hinst);
        backstage::register(hinst);
        shape::register(hinst);
        dropdown::register(hinst);

        let app_box = Box::new(App {
            hwnd: HWND::default(),
            ribbon: HWND::default(),
            editor: HWND::default(),
            status: HWND::default(),
            backstage: HWND::default(),
            hinst,
            page: PageSetup::default(),
            zoom_pct: 100,
            shapes: Vec::new(),
            focus_edit: HWND::default(),
            selected_shape: None,
            draw_mode: false,
            drawing: false,
            draw_kind: ShapeKind::Rectangle,
            draw_start: POINT::default(),
            draw_cur: POINT::default(),
            file_path: None,
            show_marks: false,
            backstage_on: false,
            custom_colors: [COLORREF(0); 16],
            last_font_color: COLORREF(0),
            last_highlight: rgb(0xFF, 0xFF, 0x00),
        });
        APP_PTR = Box::into_raw(app_box);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            MAIN_CLASS,
            w!("Rust Word"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1180,
            820,
            HWND::default(),
            HMENU::default(),
            hinst,
            None,
        )?;
        app().hwnd = hwnd;
        app().ribbon = ribbon::create(hwnd, hinst);
        app().status = statusbar::create(hwnd, hinst);
        app().editor = editor::create_editor(hwnd, hinst, 9000)?;
        app().focus_edit = app().editor;
        app().backstage = backstage::create(hwnd, hinst);
        layout(hwnd);
        set_default_text();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}

fn set_default_text() {
    unsafe {
        let a = app();
        if let Some(doc) = tom_document(a.editor) {
            if let Ok(sel) = doc.GetSelection() {
                let _ = sel.SetText(&windows::core::BSTR::from(
                    "Type here. Rust Word Editor — Word-ə bənzər redaktor.\r",
                ));
            }
        }
    }
}

// ---------- layout ----------

fn tw_to_px(tw: i32, zoom: i32) -> i32 {
    // 96 dpi: 1440 twips = 1 inch = 96 px
    (tw as i64 * 96 * zoom as i64 / (1440 * 100)) as i32
}

fn page_content_rect(client_w: i32, client_h: i32, page: &PageSetup, zoom: i32) -> RECT {
    let pw = tw_to_px(page.w_tw, zoom);
    let ph = tw_to_px(page.h_tw, zoom);
    let ml = tw_to_px(page.ml_tw, zoom);
    let mt = tw_to_px(page.mt_tw, zoom);
    let mr = tw_to_px(page.mr_tw, zoom);
    let mb = tw_to_px(page.mb_tw, zoom);
    let x0 = (client_w - pw).max(0) / 2 + ml;
    // the page top starts below the ribbon — without the offset the first
    // line of the document renders underneath it and looks clipped
    let page_top = ribbon::TOTAL_H + 8;
    RECT {
        left: x0,
        top: page_top + mt,
        right: x0 + pw - ml - mr,
        bottom: page_top + ph - mb,
    }
}

fn page_rect(client_w: i32, _h: i32, page: &PageSetup, zoom: i32) -> RECT {
    let pw = tw_to_px(page.w_tw, zoom);
    let ph = tw_to_px(page.h_tw, zoom);
    let x0 = (client_w - pw).max(0) / 2;
    let page_top = ribbon::TOTAL_H + 8;
    RECT { left: x0, top: page_top, right: x0 + pw, bottom: page_top + ph }
}

pub fn layout(hwnd: HWND) {
    unsafe {
        let a = app();
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        let _ = SetWindowPos(a.ribbon, HWND_TOP, 0, 0, w, ribbon::TOTAL_H, SWP_NOACTIVATE);
        let _ = SetWindowPos(a.status, HWND_TOP, 0, h - statusbar::STATUS_H, w, statusbar::STATUS_H, SWP_NOACTIVATE);
        let mut er = page_content_rect(w, h, &a.page.clone(), a.zoom_pct);
        if er.bottom > h - statusbar::STATUS_H {
            er.bottom = h - statusbar::STATUS_H;
        }
        let _ = SetWindowPos(
            a.editor,
            HWND_TOP,
            er.left,
            er.top,
            er.right - er.left,
            er.bottom - er.top,
            SWP_NOACTIVATE,
        );
        // HWND_TOP on the editor also puts it above every shape — raise the
        // shapes back over it or they become unclickable after each layout
        for &s in &a.shapes {
            let _ = SetWindowPos(s, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        if a.backstage_on {
            // hiding the ribbon also hides its combo children — the only
            // bulletproof way to keep them off the backstage surface
            let _ = ShowWindow(a.ribbon, SW_HIDE);
            let _ = ShowWindow(a.editor, SW_HIDE);
            let _ = ShowWindow(a.status, SW_HIDE);
            for &s in &a.shapes {
                let _ = ShowWindow(s, SW_HIDE);
            }
            let _ = SetWindowPos(a.backstage, HWND_TOP, 0, 0, w, h, SWP_SHOWWINDOW | SWP_NOACTIVATE);
            let _ = InvalidateRect(a.backstage, None, TRUE);
            let _ = UpdateWindow(a.backstage);
        } else {
            let _ = ShowWindow(a.backstage, SW_HIDE);
            let _ = ShowWindow(a.ribbon, SW_SHOW);
            let _ = ShowWindow(a.editor, SW_SHOW);
            let _ = ShowWindow(a.status, SW_SHOW);
            for &s in &a.shapes {
                let _ = ShowWindow(s, SW_SHOW);
            }
        }
        let _ = InvalidateRect(hwnd, None, FALSE);
    }
}

fn clear_shapes(a: &mut App) {
    for sh in a.shapes.drain(..) {
        unsafe {
            let _ = DestroyWindow(sh);
        }
    }
    a.selected_shape = None;
    ribbon::set_shape_mode(a.ribbon, false);
}

/// Apply a font combo's current text — called directly by the ribbon on
/// Enter / list commit, bypassing the WM_COMMAND notification chain.
pub fn apply_combo(id: u32) {
    {
        let a = app();
        match id {
            IDC_FONT_NAME => {
                let name = ribbon::combo_text(a.ribbon, IDC_FONT_NAME);
                if !name.is_empty() {
                    if let Some(d) = focus_doc() {
                        format::set_font_name(&d, &name);
                    }
                }
            }
            IDC_FONT_SIZE => {
                let s = ribbon::combo_text(a.ribbon, IDC_FONT_SIZE);
                if let Some(v) = parse_f(&s) {
                    if let Some(d) = focus_doc() {
                        format::set_font_size(&d, v);
                    }
                }
            }
            _ => {}
        }
        sync_ribbon();
    }
}

// ---------- command helpers ----------

fn focus_doc() -> Option<windows::Win32::UI::Controls::RichEdit::ITextDocument> {
    let a = app();
    tom_document(a.focus_edit)
}

fn sync_ribbon() {
    unsafe {
        let a = app();
        if let Some(doc) = focus_doc() {
            let mut st = format::query_format_state(&doc);
            st.show_marks = a.show_marks;
            st.dirty = format::is_dirty(&doc);
            ribbon::sync_state(a.ribbon, &st);
            update_status();
        }
    }
}

fn update_status() {
    unsafe {
        let a = app();
        if let Some(doc) = focus_doc() {
            let text = format::whole_text(&doc);
            let words = text.split_whitespace().filter(|w| !w.is_empty()).count();
            statusbar::set_info(a.status, "Page 1 of 1", &format!("{} words", words));
        }
    }
}

fn select_shape(shape: Option<HWND>) {
    unsafe {
        let a = app();
        if let Some(old) = a.selected_shape {
            shape::select(old, false);
        }
        a.selected_shape = shape;
        if let Some(s) = shape {
            shape::select(s, true);
            a.focus_edit = shape::inner_edit(s);
        }
        ribbon::set_shape_mode(a.ribbon, shape.is_some());
    }
}

fn delete_shape(s: HWND) {
    unsafe {
        let a = app();
        a.shapes.retain(|&h| h != s);
        if a.selected_shape == Some(s) {
            select_shape(None);
        }
        let _ = DestroyWindow(s);
    }
}

// ---------- dropdown builders ----------

fn color_items() -> Vec<DropItem> {
    let mut v = Vec::new();
    for (i, c) in WORD_COLORS.iter().enumerate() {
        v.push(DropItem {
            label: String::new(),
            payload: c.0 as i64,
            icon: DropIcon::Color(*c),
            sublabel: String::new(),
        });
        // shades
        let _ = i;
    }
    v.push(DropItem {
        label: "More Colors…".into(),
        payload: -1,
        icon: DropIcon::None,
        sublabel: String::new(),
    });
    v
}

fn color_grid_plus_custom(tag: u32, anchor: RECT) {
    unsafe {
        let a = app();
        // 10 columns grid of theme colors + shades
        let mut items: Vec<DropItem> = Vec::new();
        for c in WORD_COLORS.iter() {
            for sh in THEME_SHADES.iter() {
                items.push(DropItem {
                    label: String::new(),
                    payload: shade(*c, *sh).0 as i64,
                    icon: DropIcon::Color(shade(*c, *sh)),
                    sublabel: String::new(),
                });
            }
        }
        items.push(DropItem { label: "More Colors…".into(), payload: -1, icon: DropIcon::None, sublabel: String::new() });
        dropdown::show(a.hwnd, a.hinst, tag, items, anchor.left, anchor.bottom, 10);
    }
}

fn show_dropdown(id: u32, anchor: RECT) {
    unsafe {
        let a = app();
        match id {
            IDM_FONT_COLOR | IDM_TEXT_HIGHLIGHT | IDM_SHADING | IDM_SHAPE_FILL | IDM_SHAPE_OUTLINE => {
                color_grid_plus_custom(id, anchor);
            }
            IDM_LINE_SPACING => {
                let items = vec![
                    DropItem { label: "1.0".into(), payload: pack_ls(0, 1.0), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "1.15".into(), payload: pack_ls(5, 1.15), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "1.5".into(), payload: pack_ls(1, 1.5), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "2.0".into(), payload: pack_ls(2, 2.0), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "2.5".into(), payload: pack_ls(5, 2.5), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "3.0".into(), payload: pack_ls(5, 3.0), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "At least 12pt".into(), payload: pack_ls(3, 12.0), icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Exactly 12pt".into(), payload: pack_ls(4, 12.0), icon: DropIcon::None, sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_CHAR_SPACING | IDM_WORD_SPACING => {
                let items = vec![
                    DropItem { label: "Normal (0 pt)".into(), payload: 0, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Expanded +0.5 pt".into(), payload: 5, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Expanded +1.0 pt".into(), payload: 10, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Expanded +2.0 pt".into(), payload: 20, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Expanded +5.0 pt".into(), payload: 50, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Condensed −0.5 pt".into(), payload: -5, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Condensed −1.0 pt".into(), payload: -10, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Condensed −2.0 pt".into(), payload: -20, icon: DropIcon::None, sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_SHAPES => {
                let items = vec![
                    DropItem { label: "Rectangle".into(), payload: 1, icon: DropIcon::Shape(ShapeKind::Rectangle), sublabel: String::new() },
                    DropItem { label: "Rounded Rectangle".into(), payload: 2, icon: DropIcon::Shape(ShapeKind::RoundedRect), sublabel: String::new() },
                    DropItem { label: "Ellipse".into(), payload: 3, icon: DropIcon::Shape(ShapeKind::Ellipse), sublabel: String::new() },
                    DropItem { label: "Right Arrow".into(), payload: 4, icon: DropIcon::Shape(ShapeKind::RightArrow), sublabel: String::new() },
                    DropItem { label: "Down Arrow".into(), payload: 5, icon: DropIcon::Shape(ShapeKind::DownArrow), sublabel: String::new() },
                    DropItem { label: "Line".into(), payload: 6, icon: DropIcon::Shape(ShapeKind::Line), sublabel: String::new() },
                    DropItem { label: "Text Box".into(), payload: 7, icon: DropIcon::Shape(ShapeKind::TextBox), sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 4);
            }
            IDM_MARGINS => {
                let items = vec![
                    DropItem { label: "Normal".into(), payload: 1, icon: DropIcon::None, sublabel: "2.54 cm all".into() },
                    DropItem { label: "Narrow".into(), payload: 2, icon: DropIcon::None, sublabel: "1.27 cm all".into() },
                    DropItem { label: "Moderate".into(), payload: 3, icon: DropIcon::None, sublabel: "1.91 / 2.54 cm".into() },
                    DropItem { label: "Wide".into(), payload: 4, icon: DropIcon::None, sublabel: "2.54 / 5.08 cm".into() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_ORIENTATION => {
                let items = vec![
                    DropItem { label: "Portrait".into(), payload: 1, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Landscape".into(), payload: 2, icon: DropIcon::None, sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_PAGE_SIZE => {
                let items = vec![
                    DropItem { label: "A4".into(), payload: 1, icon: DropIcon::None, sublabel: "21 × 29.7 cm".into() },
                    DropItem { label: "Letter".into(), payload: 2, icon: DropIcon::None, sublabel: "21.6 × 27.9 cm".into() },
                    DropItem { label: "Legal".into(), payload: 3, icon: DropIcon::None, sublabel: "21.6 × 35.6 cm".into() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_SYMBOL => {
                let syms = ["©", "®", "™", "€", "£", "¥", "§", "¶", "†", "‡", "•", "…", "‹", "›", "«", "»", "±", "×", "÷", "≈", "≠", "≤", "≥", "∞", "Ω", "α", "β", "π", "Σ", "√"];
                let items = syms
                    .iter()
                    .map(|s| DropItem { label: s.to_string(), payload: s.chars().next().unwrap() as i64, icon: DropIcon::None, sublabel: String::new() })
                    .collect();
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 5);
            }
            IDM_SHAPE_MARGIN_DLG => {
                let items = vec![
                    DropItem { label: "None (0)".into(), payload: 0, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Normal".into(), payload: 1, icon: DropIcon::None, sublabel: "0.25 / 0.13 cm".into() },
                    DropItem { label: "Narrow".into(), payload: 2, icon: DropIcon::None, sublabel: "0.1 / 0.05 cm".into() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_UNDERLINE => {
                let items = vec![
                    DropItem { label: "Underline".into(), payload: 1, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Double underline".into(), payload: 3, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Words only".into(), payload: 2, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "None".into(), payload: 0, icon: DropIcon::None, sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_BULLETS => {
                let items = vec![
                    DropItem { label: "• Bullet".into(), payload: 1, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "◦ Hollow bullet".into(), payload: 1, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "▪ Square bullet".into(), payload: 1, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "None".into(), payload: 0, icon: DropIcon::None, sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_BORDERS => {
                let items = vec![
                    DropItem { label: "Bottom border".into(), payload: 2, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Top border".into(), payload: 1, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Left border".into(), payload: 4, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Right border".into(), payload: 8, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "Box (all sides)".into(), payload: 15, icon: DropIcon::None, sublabel: String::new() },
                    DropItem { label: "No border".into(), payload: 0, icon: DropIcon::None, sublabel: String::new() },
                ];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            IDM_TEXT_EFFECTS | IDM_NUMBERING | IDM_MULTILEVEL => {
                let items = vec![DropItem {
                    label: "(not implemented yet)".into(),
                    payload: -1,
                    icon: DropIcon::None,
                    sublabel: String::new(),
                }];
                dropdown::show(a.hwnd, a.hinst, id, items, anchor.left, anchor.bottom, 0);
            }
            _ => {}
        }
    }
}

fn pack_ls(rule: i32, val: f32) -> i64 {
    ((rule as i64) << 32) | (val * 100.0) as i64
}
fn unpack_ls(p: i64) -> (i32, f32) {
    ((p >> 32) as i32, (p & 0xffff_ffff) as i32 as f32 / 100.0)
}

// ---------- file dialogs ----------

fn pick_open_path(hwnd: HWND) -> Option<String> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let filter = [
            COMDLG_FILTERSPEC { pszName: w!("Word Documents"), pszSpec: w!("*.docx;*.rtf;*.txt") },
            COMDLG_FILTERSPEC { pszName: w!("All Files"), pszSpec: w!("*.*") },
        ];
        let _ = dlg.SetFileTypes(&filter);
        dlg.Show(hwnd).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok()?;
        CoTaskMemFree(Some(p.0 as *const _));
        Some(s)
    }
}

fn pick_save_path(hwnd: HWND, cur: Option<&str>) -> Option<String> {
    unsafe {
        let dlg: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL).ok()?;
        let filter = [
            COMDLG_FILTERSPEC { pszName: w!("Word Document"), pszSpec: w!("*.docx") },
            COMDLG_FILTERSPEC { pszName: w!("Rich Text"), pszSpec: w!("*.rtf") },
            COMDLG_FILTERSPEC { pszName: w!("Plain Text"), pszSpec: w!("*.txt") },
        ];
        let _ = dlg.SetFileTypes(&filter);
        let _ = dlg.SetDefaultExtension(w!("docx"));
        if let Some(c) = cur {
            let name = Path::new(c).file_name().unwrap_or_default().to_string_lossy();
            let w = to_w(&name);
            let _ = dlg.SetFileName(PCWSTR(w.as_ptr()));
        }
        dlg.Show(hwnd).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok()?;
        CoTaskMemFree(Some(p.0 as *const _));
        Some(s)
    }
}

fn choose_color(hwnd: HWND, initial: COLORREF) -> Option<COLORREF> {
    unsafe {
        let a = app();
        let mut cc = CHOOSECOLORW {
            lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: hwnd,
            rgbResult: initial,
            lpCustColors: a.custom_colors.as_mut_ptr(),
            Flags: CC_RGBINIT | CC_FULLOPEN,
            ..Default::default()
        };
        if ChooseColorW(&mut cc).into() {
            Some(cc.rgbResult)
        } else {
            None
        }
    }
}

// ---------- command dispatch ----------

fn do_command(id: u32, anchor: RECT) {
    unsafe {
        let a = app();
        match id {
            IDM_FILE => {
                a.backstage_on = true;
                layout(a.hwnd);
            }
            IDM_BACK => {
                a.backstage_on = false;
                layout(a.hwnd);
            }
            IDM_EXIT => {
                let _ = PostMessageW(a.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            IDM_NEW => {
                if let Some(doc) = tom_document(a.editor) {
                    if let Ok(r) = doc.Range(0, editor::story_len(&doc)) {
                        let _ = r.SetText(&windows::core::BSTR::from(""));
                    }
                }
                clear_shapes(a);
                a.file_path = None;
                a.backstage_on = false;
                layout(a.hwnd);
            }
            IDM_OPEN => {
                if let Some(p) = pick_open_path(a.hwnd) {
                    match docx::load(&p, a.editor) {
                        Ok(()) => {
                            clear_shapes(a);
                            if let Some(ps) = docx::take_pending_page() {
                                a.page = ps;
                            }
                            for ps in docx::take_pending_shapes() {
                                let sh = shape::create_with_props(a.hwnd, a.hinst, ps.props, ps.rect, a.zoom_pct);
                                if !ps.text.is_empty() {
                                    if let Some(d) = tom_document(shape::inner_edit(sh)) {
                                        if let Ok(r) = d.Range(0, 0) {
                                            let _ = r.SetText(&windows::core::BSTR::from(ps.text.as_str()));
                                        }
                                    }
                                }
                                a.shapes.push(sh);
                            }
                            a.file_path = Some(p.clone());
                            backstage::set_doc_name(a.backstage, &p);
                        }
                        Err(e) => {
                            let msg = to_w(&format!("Open failed: {}", e));
                            MessageBoxW(a.hwnd, PCWSTR(msg.as_ptr()), w!("Rust Word"), MB_ICONERROR);
                        }
                    }
                }
                a.backstage_on = false;
                layout(a.hwnd);
            }
            IDM_SAVE | IDM_SAVE_AS => {
                let path = if id == IDM_SAVE { a.file_path.clone() } else { None };
                let chosen = match path {
                    Some(p) => Some(p),
                    None => pick_save_path(a.hwnd, a.file_path.as_deref()),
                };
                if let Some(p) = chosen {
                    match docx::save(&p, a.editor, &a.page, &a.shapes) {
                        Ok(()) => {
                            a.file_path = Some(p.clone());
                            backstage::set_doc_name(a.backstage, &p);
                            if let Some(d) = tom_document(a.editor) {
                                format::mark_clean(&d);
                            }
                        }
                        Err(e) => {
                            let msg = to_w(&format!("Save failed: {}", e));
                            MessageBoxW(a.hwnd, PCWSTR(msg.as_ptr()), w!("Rust Word"), MB_ICONERROR);
                        }
                    }
                }
                a.backstage_on = false;
                layout(a.hwnd);
            }
            IDM_UNDO => {
                SendMessageW(a.focus_edit, WM_UNDO, WPARAM(0), LPARAM(0));
            }
            IDM_REDO => {
                SendMessageW(a.focus_edit, EM_REDO, WPARAM(0), LPARAM(0));
            }
            IDM_CUT => {
                SendMessageW(a.focus_edit, WM_CUT, WPARAM(0), LPARAM(0));
            }
            IDM_COPY => {
                SendMessageW(a.focus_edit, WM_COPY, WPARAM(0), LPARAM(0));
            }
            IDM_PASTE => {
                SendMessageW(a.focus_edit, WM_PASTE, WPARAM(0), LPARAM(0));
            }
            IDM_SELECT_ALL => {
                SendMessageW(a.focus_edit, EM_SETSEL, WPARAM(0), LPARAM(-1));
            let _ = EM_SETSEL;
            }
            IDM_BOLD => {
                if let Some(d) = focus_doc() {
                    format::toggle_bold(&d);
                }
            }
            IDM_ITALIC => {
                if let Some(d) = focus_doc() {
                    format::toggle_italic(&d);
                }
            }
            IDM_UNDERLINE => {
                if let Some(d) = focus_doc() {
                    format::toggle_underline(&d);
                }
            }
            IDM_STRIKE => {
                if let Some(d) = focus_doc() {
                    format::toggle_strike(&d);
                }
            }
            IDM_SUBSCRIPT => {
                if let Some(d) = focus_doc() {
                    format::toggle_subscript(&d);
                }
            }
            IDM_SUPERSCRIPT => {
                if let Some(d) = focus_doc() {
                    format::toggle_superscript(&d);
                }
            }
            IDM_CLEAR_FORMAT => {
                if let Some(d) = focus_doc() {
                    format::clear_formatting(&d);
                }
            }
            IDM_FONT_GROW => {
                if let Some(d) = focus_doc() {
                    format::grow_font(&d);
                }
            }
            IDM_FONT_SHRINK => {
                if let Some(d) = focus_doc() {
                    format::shrink_font(&d);
                }
            }
            IDM_ALIGN_LEFT | IDM_ALIGN_CENTER | IDM_ALIGN_RIGHT | IDM_ALIGN_JUSTIFY => {
                if let Some(d) = focus_doc() {
                    let al = match id {
                        IDM_ALIGN_CENTER => 1,
                        IDM_ALIGN_RIGHT => 2,
                        IDM_ALIGN_JUSTIFY => 3,
                        _ => 0,
                    };
                    format::set_alignment(&d, al);
                }
            }
            IDM_INDENT_DEC => {
                if let Some(d) = focus_doc() {
                    format::change_indent(&d, -18.0);
                }
            }
            IDM_INDENT_INC => {
                if let Some(d) = focus_doc() {
                    format::change_indent(&d, 18.0);
                }
            }
            IDM_BULLETS => {
                if let Some(d) = focus_doc() {
                    format::toggle_bullets(&d);
                }
            }
            IDM_SHOW_MARKS => {
                a.show_marks = !a.show_marks;
                // RichEdit has no whitespace-marks mode; tracked for doc model only.
            }
            IDM_PAGE_BREAK => {
                if let Some(d) = focus_doc() {
                    format::insert_page_break(&d);
                }
            }
            IDM_TEXT_BOX => {
                a.draw_kind = ShapeKind::TextBox;
                begin_shape_draw();
            }
            IDM_DATE_TIME => {
                if let Some(d) = focus_doc() {
                    format::insert_text(&d, &now_string());
                }
            }
            IDM_SHAPE_DELETE => {
                if let Some(s) = a.selected_shape {
                    delete_shape(s);
                }
            }
            IDM_SHAPE_BRING_FWD => {
                if let Some(s) = a.selected_shape {
                    let _ = SetWindowPos(s, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
            IDM_SHAPE_SEND_BACK => {
                if let Some(s) = a.selected_shape {
                    let _ = SetWindowPos(s, a.editor, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
            IDM_ZOOM_IN => set_zoom((a.zoom_pct + 10).min(500)),
            IDM_ZOOM_OUT => set_zoom((a.zoom_pct - 10).max(10)),
            // dropdown-bearing commands
            IDM_FONT_COLOR | IDM_TEXT_HIGHLIGHT | IDM_SHADING | IDM_LINE_SPACING | IDM_CHAR_SPACING
            | IDM_WORD_SPACING | IDM_SHAPES | IDM_MARGINS | IDM_ORIENTATION | IDM_PAGE_SIZE
            | IDM_SYMBOL | IDM_SHAPE_FILL | IDM_SHAPE_OUTLINE | IDM_SHAPE_MARGIN_DLG | IDM_UNDERLINE
            | IDM_BULLETS | IDM_BORDERS | IDM_TEXT_EFFECTS | IDM_NUMBERING | IDM_MULTILEVEL => {
                show_dropdown(id, anchor);
            }
            IDM_PARA_DIALOG => {
                open_para_dialog(a.hwnd);
            }
            IDM_FONT_DIALOG => {
                open_font_dialog(a.hwnd);
            }
            IDM_PICTURE => {
                insert_picture(a.hwnd);
            }
            IDM_TABLE | IDM_HYPERLINK | IDM_SORT | IDM_COLUMNS | IDM_CHANGE_CASE => {
                // reserved
            }
            _ => {}
        }
        sync_ribbon();
    }
}

fn now_string() -> String {
    use std::time::SystemTime;
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let days = t / 86400;
    let mut z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let yy = if m <= 2 { y + 1 } else { y };
    z = 0;
    let _ = z;
    format!("{:04}-{:02}-{:02}", yy, m, d)
}

fn set_zoom(pct: i32) {
    unsafe {
        let a = app();
        a.zoom_pct = pct;
        editor::send(a.editor, EM_SETZOOM, pct as usize, 100isize);
        for &s in &a.shapes {
            shape::set_zoom(s, pct);
        }
        statusbar::set_zoom_display(a.status, pct);
        layout(a.hwnd);
    }
}

// ---------- simple input dialogs ----------

fn modal_dialog(
    hwnd: HWND,
    title: &str,
    fields: &[&str],
    initial: &[String],
) -> Option<Vec<String>> {
    // minimal modal: reuse a dialog box built at runtime
    // Simplest: sequential prompts via a tiny window is heavy; use one small window with edits.
    use std::cell::RefCell;
    thread_local! {
        static RES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    }
    unsafe {
        let wtitle = to_w(title);
        // Build controls directly on a popup window
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        const CLS: PCWSTR = w!("RWE_Dlg");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(dlg_wndproc),
            hInstance: HINSTANCE(hinst.0),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            lpszClassName: CLS,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let n = fields.len() as i32;
        let w = 340;
        let h = 40 + n * 30 + 50;
        let mut r = RECT { left: 0, top: 0, right: w, bottom: h };
        let _ = AdjustWindowRect(&mut r, WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU, FALSE);
        let mut wr = RECT::default();
        let _ = GetWindowRect(hwnd, &mut wr);
        let x = wr.left + (wr.right - wr.left - (r.right - r.left)) / 2;
        let y = wr.top + (wr.bottom - wr.top - (r.bottom - r.top)) / 2;

        struct Ctx {
            fields: Vec<String>,
            edits: Vec<HWND>,
            done: bool,
            ok: bool,
        }
        let mut ctx = Ctx { fields: fields.iter().map(|s| s.to_string()).collect(), edits: Vec::new(), done: false, ok: false };
        let dlg = CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
            CLS,
            PCWSTR(wtitle.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            x,
            y,
            r.right - r.left,
            r.bottom - r.top,
            hwnd,
            HMENU::default(),
            HINSTANCE(hinst.0),
            Some(&mut ctx as *mut _ as _),
        )
        .unwrap_or_default();
        let _ = EnableWindow(hwnd, false);
        for (i, f) in fields.iter().enumerate() {
            let ly = 14 + i as i32 * 30;
            let lf = to_w(f);
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                PCWSTR(lf.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                12,
                ly + 4,
                110,
                20,
                dlg,
                HMENU::default(),
                HINSTANCE(hinst.0),
                None,
            );
            let e = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                w!("EDIT"),
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                130,
                ly,
                190,
                22,
                dlg,
                HMENU((1000 + i as isize) as *mut core::ffi::c_void),
                HINSTANCE(hinst.0),
                None,
            )
            .unwrap_or_default();
            if let Some(init) = initial.get(i) {
                let wt = to_w(init);
                let _ = SetWindowTextW(e, PCWSTR(wt.as_ptr()));
            }
            ctx.edits.push(e);
        }
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("OK"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            130,
            40 + n * 30 - 8,
            80,
            26,
            dlg,
            HMENU(1 as _),
            HINSTANCE(hinst.0),
            None,
        );
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("Cancel"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            220,
            40 + n * 30 - 8,
            80,
            26,
            dlg,
            HMENU(2 as _),
            HINSTANCE(hinst.0),
            None,
        );

        // modal loop
        let mut msg = MSG::default();
        while !ctx.done && GetMessageW(&mut msg, None, 0, 0).into() {
            if msg.hwnd == dlg || IsChild(dlg, msg.hwnd).into() || msg.message == WM_KEYDOWN {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let _ = EnableWindow(hwnd, true);
        let _ = SetForegroundWindow(hwnd);
        let _ = DestroyWindow(dlg);
        if !ctx.ok {
            return None;
        }
        let mut out = Vec::new();
        for e in &ctx.edits {
            let len = GetWindowTextLengthW(*e) + 1;
            let mut buf = vec![0u16; len as usize];
            let n = GetWindowTextW(*e, &mut buf) as usize;
            out.push(String::from_utf16_lossy(&buf[..n]));
        }
        Some(out)
    }
}

struct CtxPub {
    fields: Vec<String>,
    edits: Vec<HWND>,
    done: bool,
    ok: bool,
}

unsafe extern "system" fn dlg_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lp.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_COMMAND => {
            let ctx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CtxPub;
            if !ctx.is_null() {
                let id = crate::util::loword(wp.0 as isize);
                if id == 1 {
                    (*ctx).ok = true;
                    (*ctx).done = true;
                } else if id == 2 {
                    (*ctx).done = true;
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let ctx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CtxPub;
            if !ctx.is_null() {
                (*ctx).done = true;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn parse_f(s: &str) -> Option<f32> {
    s.trim().replace(',', ".").parse::<f32>().ok()
}

fn open_para_dialog(hwnd: HWND) {
    if let Some(d) = focus_doc() {
        let st = format::query_format_state(&d);
        let vals = modal_dialog(
            hwnd,
            "Paragraph",
            &["Indent left (pt)", "Indent first line (pt)", "Indent right (pt)", "Space before (pt)", "Space after (pt)", "Line spacing (x)"],
            &[
                fmt1(st.left_indent),
                fmt1(st.first_indent),
                fmt1(st.right_indent),
                fmt1(st.space_before),
                fmt1(st.space_after),
                fmt1(st.line_spacing),
            ],
        );
        if let Some(v) = vals {
            let l = v.get(0).and_then(|s| parse_f(s)).unwrap_or(st.left_indent);
            let f = v.get(1).and_then(|s| parse_f(s)).unwrap_or(st.first_indent);
            let r = v.get(2).and_then(|s| parse_f(s)).unwrap_or(st.right_indent);
            format::set_indents(&d, f, l, r);
            if let Some(x) = v.get(3).and_then(|s| parse_f(s)) {
                format::set_space_before(&d, x);
            }
            if let Some(x) = v.get(4).and_then(|s| parse_f(s)) {
                format::set_space_after(&d, x);
            }
            if let Some(x) = v.get(5).and_then(|s| parse_f(s)) {
                format::set_line_spacing(&d, 5, x); // multiple
            }
        }
    }
}

fn open_font_dialog(hwnd: HWND) {
    if let Some(d) = focus_doc() {
        let st = format::query_format_state(&d);
        let vals = modal_dialog(
            hwnd,
            "Font",
            &["Font name", "Size (pt)", "Character spacing (pt)", "Offset super/sub (pt)"],
            &[st.font_name.clone(), fmt1(st.font_size_pt), fmt1(st.char_spacing), "0".into()],
        );
        if let Some(v) = vals {
            if let Some(n) = v.get(0) {
                if !n.trim().is_empty() {
                    format::set_font_name(&d, n.trim());
                }
            }
            if let Some(x) = v.get(1).and_then(|s| parse_f(s)) {
                format::set_font_size(&d, x);
            }
            if let Some(x) = v.get(2).and_then(|s| parse_f(s)) {
                format::set_char_spacing(&d, x);
            }
            if let Some(x) = v.get(3).and_then(|s| parse_f(s)) {
                unsafe {
                    if let Some(r) = editor::selection_range(&d) {
                        if let Ok(f) = r.GetFont() {
                            let _ = f.SetPosition(x);
                        }
                    }
                }
            }
        }
    }
}

fn fmt1(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i32)
    } else {
        format!("{:.1}", v)
    }
}

// ---------- shape draw mode ----------
// A layered/color-key overlay lets clicks fall through to the editor (Win32
// hit-tests only non-keyed pixels), so draw mode subclasses the editor itself
// and paints the rubber band with XOR on its DC.

const DRAW_SUBID: usize = 77;

fn begin_shape_draw() {
    unsafe {
        let a = app();
        a.draw_mode = true;
        a.drawing = false;
        let _ = SetWindowSubclass(a.editor, Some(draw_subproc), DRAW_SUBID, 0);
        let _ = SetCursor(LoadCursorW(None, IDC_CROSS).unwrap_or_default());
    }
}

fn norm_rect(s: POINT, c: POINT) -> RECT {
    RECT {
        left: s.x.min(c.x),
        top: s.y.min(c.y),
        right: s.x.max(c.x),
        bottom: s.y.max(c.y),
    }
}

fn draw_band(target: HWND, r: &RECT) {
    unsafe {
        let dc = GetDC(target);
        // R2_XORPEN + white pen inverts the screen: black on the white page
        // (R2_NOTXORPEN with white draws white on white — invisible)
        SetROP2(dc, R2_XORPEN);
        let pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FFFFFF));
        let old = SelectObject(dc, pen);
        let _ = SelectObject(dc, GetStockObject(NULL_BRUSH));
        let _ = Rectangle(dc, r.left, r.top, r.right, r.bottom);
        SelectObject(dc, old);
        let _ = DeleteObject(pen);
        let _ = ReleaseDC(target, dc);
    }
}

unsafe extern "system" fn draw_subproc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let a = app();
            a.drawing = true;
            a.draw_start = POINT { x: loword_signed(lp.0 as isize), y: hiword_signed(lp.0 as isize) };
            a.draw_cur = a.draw_start;
            let _ = SetCapture(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let a = app();
            if a.drawing {
                let old = norm_rect(a.draw_start, a.draw_cur);
                draw_band(hwnd, &old); // erase (XOR)
                a.draw_cur = POINT { x: loword_signed(lp.0 as isize), y: hiword_signed(lp.0 as isize) };
                let new = norm_rect(a.draw_start, a.draw_cur);
                draw_band(hwnd, &new); // draw
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let a = app();
            if a.drawing {
                let old = norm_rect(a.draw_start, a.draw_cur);
                draw_band(hwnd, &old); // erase
                a.drawing = false;
                a.draw_mode = false;
                let _ = ReleaseCapture();
                let _ = RemoveWindowSubclass(hwnd, Some(draw_subproc), DRAW_SUBID);
                // editor client coords -> main window client coords
                let mut pts = [a.draw_start, a.draw_cur];
                let _ = MapWindowPoints(hwnd, a.hwnd, &mut pts);
                let mut r = norm_rect(pts[0], pts[1]);
                if r.right - r.left < 8 || r.bottom - r.top < 8 {
                    r.right = r.left + 220;
                    r.bottom = r.top + 90;
                }
                let sh = shape::create(a.hwnd, a.hinst, a.draw_kind, r, a.zoom_pct);
                a.shapes.push(sh);
                select_shape(Some(sh));
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == VK_ESCAPE.0 as usize {
                let a = app();
                a.drawing = false;
                a.draw_mode = false;
                let _ = RemoveWindowSubclass(hwnd, Some(draw_subproc), DRAW_SUBID);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let _ = SetCursor(LoadCursorW(None, IDC_CROSS).unwrap_or_default());
            LRESULT(1)
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(draw_subproc), DRAW_SUBID);
            DefSubclassProc(hwnd, msg, wp, lp)
        }
        _ => DefSubclassProc(hwnd, msg, wp, lp),
    }
}

// ---------- picture insertion ----------

fn insert_picture(owner: HWND) {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).unwrap();
        let filter = [
            COMDLG_FILTERSPEC { pszName: w!("Bitmap Images"), pszSpec: w!("*.bmp") },
            COMDLG_FILTERSPEC { pszName: w!("All Files"), pszSpec: w!("*.*") },
        ];
        let _ = dlg.SetFileTypes(&filter);
        if dlg.Show(owner).is_err() {
            return;
        }
        let Ok(item) = dlg.GetResult() else { return };
        let Ok(p) = item.GetDisplayName(SIGDN_FILESYSPATH) else { return };
        let path = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        if !path.to_lowercase().ends_with(".bmp") {
            MessageBoxW(owner, w!("Only .bmp images are supported for now."), w!("Rust Word"), MB_ICONINFORMATION);
            return;
        }
        let wpath = to_w(&path);
        let hb = match LoadImageW(None, PCWSTR(wpath.as_ptr()), IMAGE_BITMAP, 0, 0, LR_LOADFROMFILE | LR_DEFAULTSIZE) {
            Ok(h) => h,
            Err(_) => {
                MessageBoxW(owner, w!("Could not load the image."), w!("Rust Word"), MB_ICONERROR);
                return;
            }
        };
        // RichEdit can't take an HBITMAP directly — put it on the clipboard and paste
        if OpenClipboard(owner).is_ok() {
            let _ = EmptyClipboard();
            let _ = SetClipboardData(CF_BITMAP.0 as u32, hb);
            let _ = CloseClipboard();
            SendMessageW(app().focus_edit, WM_PASTE, WPARAM(0), LPARAM(0));
        } else {
            MessageBoxW(owner, w!("Clipboard unavailable."), w!("Rust Word"), MB_ICONERROR);
        }
    }
}

// ---------- painting (page canvas) ----------

unsafe fn paint_main(hwnd: HWND) {
    let a = app();
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    // gray canvas below ribbon, above status
    let mut area = rc;
    area.top = ribbon::TOTAL_H;
    area.bottom -= statusbar::STATUS_H;
    let gb = CreateSolidBrush(PAGE_BG);
    FillRect(hdc, &area, gb);
    let _ = DeleteObject(gb);
    // white page
    let pr = page_rect(rc.right - rc.left, rc.bottom - rc.top, &a.page.clone(), a.zoom_pct);
    let wb = CreateSolidBrush(PAGE_WHITE);
    FillRect(hdc, &pr, wb);
    let _ = DeleteObject(wb);
    let pb = CreatePen(PS_SOLID, 1, rgb(0xC8, 0xC8, 0xC8));
    let old = SelectObject(hdc, pb);
    let _ = SelectObject(hdc, GetStockObject(NULL_BRUSH));
    Rectangle(hdc, pr.left, pr.top, pr.right, pr.bottom);
    SelectObject(hdc, old);
    let _ = DeleteObject(pb);
    let _ = EndPaint(hwnd, &ps);
}

// ---------- main wndproc ----------

unsafe extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_SIZE => {
            let a = app();
            if !a.ribbon.0.is_null() {
                layout(hwnd);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(0),
        WM_PAINT => {
            paint_main(hwnd);
            LRESULT(0)
        }
        WM_NOTIFY => {
            let a = app();
            if a.ribbon.0.is_null() {
                return LRESULT(0);
            }
            let hdr = lp.0 as *const NMHDR;
            if (*hdr).code == EN_SELCHANGE || (*hdr).code == EN_CHANGE {
                a.focus_edit = (*hdr).hwndFrom;
                sync_ribbon();
            }
            LRESULT(0)
        }
        WM_APP_SHAPE_SELECT => {
            select_shape(Some(HWND(wp.0 as _)));
            LRESULT(0)
        }
        WM_APP_SHAPE_FOCUS_EDIT => {
            let a = app();
            a.focus_edit = HWND(wp.0 as _);
            LRESULT(0)
        }
        WM_APP_DROPDOWN_PICK => {
            on_dropdown_pick(wp.0 as u32, lp.0 as i64);
            LRESULT(0)
        }
        WM_HSCROLL => {
            // zoom slider (forwarded by statusbar)
            let pos = statusbar::slider_pos(app().status);
            set_zoom(pos.clamp(10, 500));
            LRESULT(0)
        }
        WM_COMMAND => {
            let code = (wp.0 >> 16) as u32;
            let id = (wp.0 & 0xffff) as u32;
            let a = app();
            // combobox notifications
            if id == IDC_FONT_NAME && code == CBN_SELCHANGE && !ribbon::is_syncing(a.ribbon) {
                let name = ribbon::combo_text(a.ribbon, IDC_FONT_NAME);
                if !name.is_empty() {
                    if let Some(d) = focus_doc() {
                        format::set_font_name(&d, &name);
                    }
                }
                sync_ribbon();
                return LRESULT(0);
            }
            if id == IDC_FONT_SIZE && code == CBN_SELCHANGE && !ribbon::is_syncing(a.ribbon) {
                let s = ribbon::combo_text(a.ribbon, IDC_FONT_SIZE);
                if let Some(v) = parse_f(&s) {
                    if let Some(d) = focus_doc() {
                        format::set_font_size(&d, v);
                    }
                }
                sync_ribbon();
                return LRESULT(0);
            }
            // ribbon button: lParam carries anchor screen point for dropdowns
            let anchor = RECT {
                left: (lp.0 & 0xffffffff) as u32 as i32,
                top: 0,
                right: 0,
                bottom: ((lp.0 >> 32) & 0xffffffff) as u32 as i32,
            };
            do_command(id, anchor);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            LRESULT(0)
        }
        WM_CLOSE => {
            // check dirty
            let a = app();
            let mut allow = true;
            if let Some(d) = tom_document(a.editor) {
                if format::is_dirty(&d) {
                    let r = MessageBoxW(
                        hwnd,
                        w!("Save changes before closing?"),
                        w!("Rust Word"),
                        MB_YESNOCANCEL | MB_ICONQUESTION,
                    );
                    match r {
                        IDYES => do_command(IDM_SAVE, RECT::default()),
                        IDCANCEL => allow = false,
                        _ => {}
                    }
                }
            }
            if allow {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------- dropdown picks ----------

fn on_dropdown_pick(tag: u32, payload: i64) {
    unsafe {
        let a = app();
        match tag {
            IDM_FONT_COLOR => {
                if payload == -1 {
                    if let Some(c) = choose_color(a.hwnd, a.last_font_color) {
                        apply_font_color(c);
                    }
                } else {
                    apply_font_color(COLORREF(payload as u32));
                }
            }
            IDM_TEXT_HIGHLIGHT => {
                if payload == -1 {
                    if let Some(c) = choose_color(a.hwnd, a.last_highlight) {
                        apply_highlight(c);
                    }
                } else {
                    apply_highlight(COLORREF(payload as u32));
                }
            }
            IDM_SHADING => {
                if payload >= 0 {
                    set_para_shading(COLORREF(payload as u32));
                }
            }
            IDM_SHAPE_FILL => {
                if let Some(s) = a.selected_shape {
                    if payload == -1 {
                        if let Some(c) = choose_color(a.hwnd, PAGE_WHITE) {
                            shape::set_fill(s, c, true);
                        }
                    } else {
                        shape::set_fill(s, COLORREF(payload as u32), true);
                    }
                }
            }
            IDM_SHAPE_OUTLINE => {
                if let Some(s) = a.selected_shape {
                    if payload == -1 {
                        if let Some(c) = choose_color(a.hwnd, ACCENT) {
                            shape::set_outline(s, c, 1, true);
                        }
                    } else {
                        shape::set_outline(s, COLORREF(payload as u32), 1, true);
                    }
                }
            }
            IDM_SHAPE_MARGIN_DLG => {
                if let Some(s) = a.selected_shape {
                    let (l, t, r, b) = match payload {
                        0 => (0, 0, 0, 0),
                        2 => (4, 2, 4, 2),
                        _ => (7, 4, 7, 4),
                    };
                    shape::set_insets(s, l, t, r, b);
                }
            }
            IDM_LINE_SPACING => {
                let (rule, val) = unpack_ls(payload);
                if let Some(d) = focus_doc() {
                    format::set_line_spacing(&d, rule, val);
                }
            }
            IDM_CHAR_SPACING | IDM_WORD_SPACING => {
                if let Some(d) = focus_doc() {
                    format::set_char_spacing(&d, payload as f32 / 10.0);
                }
            }
            IDM_UNDERLINE => {
                if let Some(d) = focus_doc() {
                    if let Some(r) = editor::selection_range(&d) {
                        if let Ok(f) = r.GetFont() {
                            let _ = f.SetUnderline(payload as i32);
                        }
                    }
                }
            }
            IDM_BULLETS => {
                if let Some(d) = focus_doc() {
                    format::toggle_bullets(&d);
                }
            }
            IDM_BORDERS => {
                set_para_border(payload as u16);
            }
            IDM_SHAPES => {
                a.draw_kind = match payload {
                    1 => ShapeKind::Rectangle,
                    2 => ShapeKind::RoundedRect,
                    3 => ShapeKind::Ellipse,
                    4 => ShapeKind::RightArrow,
                    5 => ShapeKind::DownArrow,
                    6 => ShapeKind::Line,
                    _ => ShapeKind::TextBox,
                };
                begin_shape_draw();
            }
            IDM_MARGINS => {
                let (m, mt, mr, mb) = match payload {
                    2 => (720, 720, 720, 720),
                    3 => (1440, 1080, 1440, 1080),
                    4 => (2880, 1440, 2880, 1440),
                    _ => (1440, 1440, 1440, 1440),
                };
                let _ = m;
                a.page.ml_tw = match payload { 2 => 720, 3 => 1440, 4 => 2880, _ => 1440 };
                a.page.mt_tw = mt;
                a.page.mr_tw = mr;
                a.page.mb_tw = mb;
                layout(a.hwnd);
            }
            IDM_ORIENTATION => {
                if payload == 2 && !a.page.landscape {
                    a.page.landscape = true;
                    std::mem::swap(&mut a.page.w_tw, &mut a.page.h_tw);
                } else if payload == 1 && a.page.landscape {
                    a.page.landscape = false;
                    std::mem::swap(&mut a.page.w_tw, &mut a.page.h_tw);
                }
                layout(a.hwnd);
            }
            IDM_PAGE_SIZE => {
                let (w, h) = match payload {
                    2 => (LETTER_W, LETTER_H),
                    3 => (LEGAL_W, LEGAL_H),
                    _ => (A4_W, A4_H),
                };
                a.page.w_tw = w;
                a.page.h_tw = h;
                if a.page.landscape {
                    std::mem::swap(&mut a.page.w_tw, &mut a.page.h_tw);
                }
                layout(a.hwnd);
            }
            IDM_SYMBOL => {
                if payload > 0 {
                    if let Some(d) = focus_doc() {
                        let ch = char::from_u32(payload as u32).unwrap_or('?');
                        format::insert_text(&d, &ch.to_string());
                    }
                }
            }
            _ => {}
        }
        sync_ribbon();
    }
}

fn apply_font_color(c: COLORREF) {
    unsafe {
        let a = app();
        a.last_font_color = c;
        if let Some(d) = focus_doc() {
            format::set_font_color(&d, c);
        }
    }
}

fn apply_highlight(c: COLORREF) {
    unsafe {
        let a = app();
        a.last_highlight = c;
        if let Some(d) = focus_doc() {
            format::set_highlight(&d, c);
        }
    }
}

fn set_para_shading(c: COLORREF) {
    // Paragraph shading ≈ highlight in the RichEdit model.
    if let Some(d) = focus_doc() {
        format::set_highlight(&d, c);
    }
}

fn set_para_border(mask: u16) {
    let mut pf = editor::get_paraformat(app().focus_edit);
    pf.Base.dwMask = PFM_BORDER;
    pf.wBorders = PARAFORMAT_BORDERS(mask);
    pf.wBorderSpace = 4;
    pf.wBorderWidth = 6;
    editor::set_paraformat(app().focus_edit, &mut pf);
}
