//! RichEdit (msftedit.dll) wrapper: the editing engine.
//! Formatting goes through TOM (ITextDocument/ITextRange/ITextFont/ITextPara),
//! which maps 1:1 onto Word's character/paragraph model.

use std::ptr;
use windows::core::{w, BSTR, Interface, IUnknown, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const RICHEDIT_CLASS: PCWSTR = w!("RichEdit50W");

// ---- TOM constants (tom.h) ----
pub const TOM_TRUE: i32 = -1;
pub const TOM_FALSE: i32 = 0;
pub const TOM_TOGGLE: i32 = 9999999;
pub const TOM_AUTOCOLOR: i32 = -9999997;

pub const TOM_CHAR: i32 = 1;
pub const TOM_WORD: i32 = 2;
pub const TOM_PARA: i32 = 4;
pub const TOM_STORY: i32 = 6;
pub const TOM_CHARFMT: i32 = 13;
pub const TOM_PARAFMT: i32 = 14;

pub const TOM_ALIGN_LEFT: i32 = 0;
pub const TOM_ALIGN_CENTER: i32 = 1;
pub const TOM_ALIGN_RIGHT: i32 = 2;
pub const TOM_ALIGN_JUSTIFY: i32 = 3;

pub const TOM_LINES_SINGLE: i32 = 0;
pub const TOM_LINES_1_5: i32 = 1;
pub const TOM_LINES_2: i32 = 2;
pub const TOM_LINES_ATLEAST: i32 = 3;
pub const TOM_LINES_EXACT: i32 = 4;
pub const TOM_LINES_MULTIPLE: i32 = 5;

pub const TOM_UNDERLINE_NONE: i32 = 0;
pub const TOM_UNDERLINE_SINGLE: i32 = 1;
pub const TOM_UNDERLINE_WORDS: i32 = 2;
pub const TOM_UNDERLINE_DOUBLE: i32 = 3;

pub const TOM_LIST_NONE: i32 = 0;
pub const TOM_LIST_BULLET: i32 = 1;

pub const SCF_SELECTION: usize = 1;
pub const SCF_ALL: usize = 4;

static mut RICHEDIT_LIB: HMODULE = HMODULE(std::ptr::null_mut());

pub fn load_richedit() -> windows::core::Result<()> {
    unsafe {
        RICHEDIT_LIB = LoadLibraryW(w!("msftedit.dll"))?;
    }
    Ok(())
}

/// Create the RichEdit editing window.
/// `page_w`/`page_h`/`margins` are in twips and re-apply on layout.
pub fn create_editor(parent: HWND, hinst: HINSTANCE, id: u32) -> windows::core::Result<HWND> {
    unsafe {
        let style = WINDOW_STYLE(
            WS_CHILD.0
                | WS_VISIBLE.0
                | WS_VSCROLL.0
                | WS_CLIPCHILDREN.0
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_WANTRETURN as u32,
        );
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            RICHEDIT_CLASS,
            PCWSTR::null(),
            style,
            0,
            0,
            100,
            100,
            parent,
            HMENU(id as _),
            hinst,
            None,
        )?;
        // RichEdit 4.x: notifications — only sel/change; key/mouse events are
        // not consumed anywhere and would flood EN_MSGFILTER per keystroke
        SendMessageW(
            hwnd,
            EM_SETEVENTMASK,
            WPARAM(0),
            LPARAM((ENM_SELCHANGE | ENM_CHANGE) as isize),
        );
        // NOTE: SES_EMULATESYSEDIT must NOT be set — on real msftedit it makes
        // the control behave like a plain EDIT and formatting stops applying
        // (Wine stubs the message, which is why the bug only shows on Windows)
        SendMessageW(hwnd, EM_SETTYPOGRAPHYOPTIONS, WPARAM(1), LPARAM(1));
        Ok(hwnd)
    }
}

// ---------- low-level helpers ----------

pub fn send(hwnd: HWND, msg: u32, w: usize, l: isize) -> LRESULT {
    unsafe { SendMessageW(hwnd, msg, WPARAM(w), LPARAM(l)) }
}

pub fn get_charformat(hwnd: HWND) -> CHARFORMAT2W {
    unsafe {
        let mut cf = CHARFORMAT2W {
            Base: CHARFORMATW {
                cbSize: std::mem::size_of::<CHARFORMAT2W>() as u32,
                dwMask: CFM_MASK(u32::MAX),
                ..Default::default()
            },
            ..Default::default()
        };
        SendMessageW(
            hwnd,
            EM_GETCHARFORMAT,
            WPARAM(SCF_SELECTION),
            LPARAM(&mut cf as *mut _ as isize),
        );
        cf
    }
}

pub fn set_charformat(hwnd: HWND, cf: &mut CHARFORMAT2W, scope: usize) {
    unsafe {
        SendMessageW(
            hwnd,
            EM_SETCHARFORMAT,
            WPARAM(scope),
            LPARAM(cf as *const _ as isize),
        );
    }
}

pub fn get_paraformat(hwnd: HWND) -> PARAFORMAT2 {
    unsafe {
        let mut pf = PARAFORMAT2 {
            Base: PARAFORMAT {
                cbSize: std::mem::size_of::<PARAFORMAT2>() as u32,
                dwMask: PARAFORMAT_MASK(u32::MAX),
                ..Default::default()
            },
            ..Default::default()
        };
        SendMessageW(hwnd, EM_GETPARAFORMAT, WPARAM(0), LPARAM(&mut pf as *mut _ as isize));
        pf
    }
}

pub fn set_paraformat(hwnd: HWND, pf: &mut PARAFORMAT2) {
    unsafe {
        SendMessageW(hwnd, EM_SETPARAFORMAT, WPARAM(0), LPARAM(pf as *const _ as isize));
    }
}

// ---------- TOM document ----------

pub fn tom_document(hwnd: HWND) -> Option<ITextDocument> {
    unsafe {
        // EM_GETOLEINTERFACE fills a pointer-sized out arg
        let mut unk: *mut core::ffi::c_void = ptr::null_mut();
        let ok = SendMessageW(
            hwnd,
            EM_GETOLEINTERFACE,
            WPARAM(0),
            LPARAM(&mut unk as *mut _ as isize),
        );
        if ok.0 == 0 || unk.is_null() {
            return None;
        }
        let unknown: IUnknown = IUnknown::from_raw(unk);
        unknown.cast::<ITextDocument>().ok()
    }
}

pub fn selection_range(doc: &ITextDocument) -> Option<ITextRange> {
    unsafe { doc.GetSelection().ok().and_then(|s| s.cast::<ITextRange>().ok()) }
}

pub fn story_len(doc: &ITextDocument) -> i32 {
    unsafe {
        doc.Range(0, 0)
            .ok()
            .and_then(|r| r.GetStoryLength().ok())
            .unwrap_or(0)
    }
}

/// Range over the whole story.
pub fn story_range(doc: &ITextDocument) -> Option<ITextRange> {
    unsafe { doc.Range(0, story_len(doc)).ok() }
}
