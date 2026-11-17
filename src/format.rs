//! High-level formatting operations on the editor via TOM + EM_* messages.

use windows::core::{BSTR, HSTRING, Interface};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::editor::*;

// ---------- selection char ops ----------

fn sel(doc: &ITextDocument) -> Option<ITextRange> {
    unsafe { doc.GetSelection().ok().and_then(|s| s.cast::<ITextRange>().ok()) }
}

fn toggle_font_val(cur: i32) -> i32 {
    if cur == TOM_TRUE {
        TOM_FALSE
    } else {
        TOM_TRUE
    }
}

pub fn toggle_bold(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetBold().unwrap_or(TOM_FALSE);
                let _ = f.SetBold(toggle_font_val(cur));
            }
        }
    }
}

pub fn toggle_italic(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetItalic().unwrap_or(TOM_FALSE);
                let _ = f.SetItalic(toggle_font_val(cur));
            }
        }
    }
}

pub fn toggle_underline(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetUnderline().unwrap_or(TOM_UNDERLINE_NONE);
                let next = if cur == TOM_UNDERLINE_NONE {
                    TOM_UNDERLINE_SINGLE
                } else {
                    TOM_UNDERLINE_NONE
                };
                let _ = f.SetUnderline(next);
            }
        }
    }
}

pub fn toggle_strike(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetStrikeThrough().unwrap_or(TOM_FALSE);
                let _ = f.SetStrikeThrough(toggle_font_val(cur));
            }
        }
    }
}

fn toggle_offset(r: &ITextRange, positive: bool) {
    unsafe {
        if let Ok(f) = r.GetFont() {
            let pos = f.GetPosition().unwrap_or(0.0);
            let target = if (positive && pos > 0.0) || (!positive && pos < 0.0) {
                0.0
            } else if positive {
                6.0
            } else {
                -3.0
            };
            let _ = f.SetPosition(target);
        }
    }
}

pub fn toggle_superscript(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        toggle_offset(&r, true);
    }
}

pub fn toggle_subscript(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        toggle_offset(&r, false);
    }
}

pub fn set_font_name(doc: &ITextDocument, name: &str) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let _ = f.SetName(&BSTR::from(name));
            }
        }
    }
}

pub fn set_font_size(doc: &ITextDocument, pt: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let _ = f.SetSize(pt);
            }
        }
    }
}

pub fn grow_font(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetSize().unwrap_or(11.0);
                let next = next_size(cur, true);
                let _ = f.SetSize(next);
            }
        }
    }
}

pub fn shrink_font(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetSize().unwrap_or(11.0);
                let next = next_size(cur, false);
                let _ = f.SetSize(next);
            }
        }
    }
}

const WORD_SIZES: [f32; 25] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0,
    72.0, 80.0, 88.0, 96.0, 100.0, 110.0, 120.0, 130.0, 140.0, 150.0,
];

fn next_size(cur: f32, up: bool) -> f32 {
    if up {
        WORD_SIZES.iter().copied().find(|s| *s > cur).unwrap_or(150.0)
    } else {
        WORD_SIZES.iter().copied().rev().find(|s| *s < cur).unwrap_or(8.0)
    }
}

pub fn set_font_color(doc: &ITextDocument, color: COLORREF) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let _ = f.SetForeColor(color.0 as i32);
            }
        }
    }
}

pub fn set_highlight(doc: &ITextDocument, color: COLORREF) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let _ = f.SetBackColor(color.0 as i32);
            }
        }
    }
}

/// Character spacing in points (positive = expanded, negative = condensed).
pub fn set_char_spacing(doc: &ITextDocument, pt: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let _ = f.SetSpacing(pt);
            }
        }
    }
}

pub fn toggle_all_caps(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let cur = f.GetAllCaps().unwrap_or(TOM_FALSE);
                let _ = f.SetAllCaps(toggle_font_val(cur));
            }
        }
    }
}

pub fn clear_formatting(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                let _ = f.Reset(tomConstants(TOM_TRUE));
            }
            if let Ok(p) = r.GetPara() {
                let _ = p.Reset(TOM_TRUE);
            }
        }
    }
}

// ---------- paragraph ops ----------

pub fn set_alignment(doc: &ITextDocument, align: i32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let _ = p.SetAlignment(align);
            }
        }
    }
}

/// rule: TOM_LINES_* ; val: multiplier or points depending on rule
/// For tomLinesMultiple: val is multiple (e.g. 1.5). For AtLeast/Exact: val in pt.
pub fn set_line_spacing(doc: &ITextDocument, rule: i32, val: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let _ = p.SetLineSpacing(rule, val);
            }
        }
    }
}

pub fn set_space_before(doc: &ITextDocument, pt: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let _ = p.SetSpaceBefore(pt);
            }
        }
    }
}

pub fn set_space_after(doc: &ITextDocument, pt: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let _ = p.SetSpaceAfter(pt);
            }
        }
    }
}

pub fn set_indents(doc: &ITextDocument, first: f32, left: f32, right: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let _ = p.SetIndents(first, left, right);
            }
        }
    }
}

pub fn change_indent(doc: &ITextDocument, delta_pt: f32) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let cur = p.GetLeftIndent().unwrap_or(0.0);
                let first = p.GetFirstLineIndent().unwrap_or(0.0);
                let right = p.GetRightIndent().unwrap_or(0.0);
                let next = (cur + delta_pt).max(0.0);
                let _ = p.SetIndents(first, next, right);
            }
        }
    }
}

pub fn toggle_bullets(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(p) = r.GetPara() {
                let cur = p.GetListType().unwrap_or(TOM_LIST_NONE);
                let next = if cur == TOM_LIST_NONE {
                    TOM_LIST_BULLET
                } else {
                    TOM_LIST_NONE
                };
                let _ = p.SetListType(next);
            }
        }
    }
}

// ---------- snapshot of selection format (for ribbon state) ----------

#[derive(Default, Clone)]
pub struct FormatState {
    pub font_name: String,
    pub font_size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub fore_color: i32,
    pub back_color: i32,
    pub char_spacing: f32,
    pub alignment: i32,
    pub space_before: f32,
    pub space_after: f32,
    pub line_spacing_rule: i32,
    pub line_spacing: f32,
    pub left_indent: f32,
    pub first_indent: f32,
    pub right_indent: f32,
    pub list_type: i32,
    pub dirty: bool,
    pub show_marks: bool,
}

pub fn query_format_state(doc: &ITextDocument) -> FormatState {
    let mut st = FormatState {
        font_size_pt: 11.0,
        alignment: TOM_ALIGN_LEFT,
        ..Default::default()
    };
    if let Some(r) = sel(doc) {
        unsafe {
            if let Ok(f) = r.GetFont() {
                st.font_name = f.GetName().map(|b| b.to_string()).unwrap_or_else(|_| "Calibri".into());
                st.font_size_pt = f.GetSize().unwrap_or(11.0);
                st.bold = f.GetBold().unwrap_or(TOM_FALSE) == TOM_TRUE;
                st.italic = f.GetItalic().unwrap_or(TOM_FALSE) == TOM_TRUE;
                st.underline = f.GetUnderline().unwrap_or(TOM_UNDERLINE_NONE) != TOM_UNDERLINE_NONE;
                st.strike = f.GetStrikeThrough().unwrap_or(TOM_FALSE) == TOM_TRUE;
                st.fore_color = f.GetForeColor().unwrap_or(0);
                st.back_color = f.GetBackColor().unwrap_or(0);
                st.char_spacing = f.GetSpacing().unwrap_or(0.0);
                let pos = f.GetPosition().unwrap_or(0.0);
                st.superscript = pos > 0.0;
                st.subscript = pos < 0.0;
            }
            if let Ok(p) = r.GetPara() {
                st.alignment = p.GetAlignment().unwrap_or(TOM_ALIGN_LEFT);
                st.space_before = p.GetSpaceBefore().unwrap_or(0.0);
                st.space_after = p.GetSpaceAfter().unwrap_or(0.0);
                st.line_spacing_rule = p.GetLineSpacingRule().unwrap_or(TOM_LINES_SINGLE);
                st.line_spacing = p.GetLineSpacing().unwrap_or(1.0);
                st.left_indent = p.GetLeftIndent().unwrap_or(0.0);
                st.first_indent = p.GetFirstLineIndent().unwrap_or(0.0);
                st.right_indent = p.GetRightIndent().unwrap_or(0.0);
                st.list_type = p.GetListType().unwrap_or(TOM_LIST_NONE);
            }
        }
    }
    st
}

// ---------- misc ----------

pub fn set_zoom(hwnd: HWND, numerator: usize, denominator: usize) {
    send(
        hwnd,
        EM_SETZOOM,
        numerator,
        denominator as isize,
    );
}

pub fn get_zoom(hwnd: HWND) -> (usize, usize) {
    let mut num: i32 = 100;
    let mut den: i32 = 100;
    unsafe {
        SendMessageW(
            hwnd,
            EM_GETZOOM,
            WPARAM(&mut num as *mut _ as usize),
            LPARAM(&mut den as *mut _ as isize),
        );
    }
    (num.max(1) as usize, den.max(1) as usize)
}

pub fn insert_page_break(doc: &ITextDocument) {
    if let Some(r) = sel(doc) {
        unsafe {
            let _ = r.SetText(&BSTR::from("\u{000C}"));
        }
    }
}

pub fn insert_text(doc: &ITextDocument, text: &str) {
    if let Some(r) = sel(doc) {
        unsafe {
            let _ = r.SetText(&BSTR::from(text));
        }
    }
}

pub fn whole_text(doc: &ITextDocument) -> String {
    unsafe {
        match story_range(doc).and_then(|r| r.GetText().ok()) {
            Some(b) => b.to_string(),
            None => String::new(),
        }
    }
}

pub fn mark_clean(doc: &ITextDocument) {
    unsafe {
        let _ = doc.SetSaved(tomConstants(TOM_TRUE));
    }
}

pub fn is_dirty(doc: &ITextDocument) -> bool {
    unsafe { doc.GetSaved().unwrap_or(TOM_FALSE) == TOM_FALSE }
}
