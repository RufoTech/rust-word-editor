//! PDF export.
//!
//! Primary path: print the document to "Microsoft Print to PDF" (Windows 10+)
//! using EM_FORMATRANGE, then overlay shapes with plain GDI drawing so text
//! boxes and their inner text land on the same page positions.
//! Fallback: if the printer does not exist, write a minimal PDF ourselves
//! (Helvetica-family base fonts, Latin text, basic bold/italic/size/color).

use crate::app::PageSetup;
use crate::dropdown::ShapeKind;
use crate::editor::{self, tom_document};
use crate::shape;
use crate::util::to_w;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Export the whole document (text + shapes) to `path`.
/// `editor` = main RichEdit, `shapes` = shape window handles, `page` = page setup
/// in twips, `page_px` = on-screen page content rect + zoom for shape mapping.
pub fn export(
    editor: HWND,
    shapes: &[HWND],
    page: &PageSetup,
    path: &str,
    page_content_px: RECT,
    zoom_pct: i32,
) -> Result<(), String> {
    if print_to_pdf(editor, shapes, page, path, page_content_px, zoom_pct).is_ok() {
        return Ok(());
    }
    simple_pdf(editor, shapes, page, path, page_content_px, zoom_pct)
}

// ---------- path 1: Microsoft Print to PDF ----------

fn print_to_pdf(
    editor: HWND,
    shapes: &[HWND],
    page: &PageSetup,
    path: &str,
    page_content_px: RECT,
    zoom_pct: i32,
) -> Result<(), String> {
    unsafe {
        // lpszOutput: passing a file path makes the driver write there directly
        // instead of prompting (works with the PORTPROMPT: port).
        let pw = to_w(path);
        let dc = CreateDCW(
            w!("Microsoft Print to PDF"),
            w!("Microsoft Print to PDF"),
            PCWSTR(pw.as_ptr()),
            None,
        );
        if dc.is_invalid() {
            return Err("printer not found".into());
        }

        let doc_name = to_w("Rust Word — PDF export");
        let mut di = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(doc_name.as_ptr()),
            lpszOutput: PCWSTR::null(),
            lpszDatatype: PCWSTR::null(),
            fwType: 0,
        };
        if StartDocW(dc, &di) <= 0 {
            let _ = DeleteDC(dc);
            return Err("StartDoc failed".into());
        }

        // FORMATRANGE rects are in twips; the control maps them to the printer DC
        let rc_content = RECT {
            left: page.ml_tw,
            top: page.mt_tw,
            right: page.w_tw - page.mr_tw,
            bottom: page.h_tw - page.mb_tw,
        };
        let rc_page = RECT { left: 0, top: 0, right: page.w_tw, bottom: page.h_tw };
        let story = tom_document(editor).map(|d| editor::story_len(&d)).unwrap_or(0);
        let dpi = GetDeviceCaps(dc, LOGPIXELSY);

        let mut cp = 0i32;
        let mut first = true;
        loop {
            if StartPage(dc) <= 0 {
                break;
            }
            let mut fr = FORMATRANGE {
                hdc: dc,
                hdcTarget: dc,
                rc: rc_content,
                rcPage: rc_page,
                chrg: CHARRANGE { cpMin: cp, cpMax: -1 },
            };
            let next = SendMessageW(
                editor,
                EM_FORMATRANGE,
                WPARAM(1),
                LPARAM(&mut fr as *mut _ as isize),
            )
            .0 as i32;
            draw_shapes_gdi(dc, shapes, page, page_content_px, zoom_pct, dpi);
            let _ = EndPage(dc);
            if next < 0 || next >= story || (!first && next <= cp) {
                break;
            }
            first = false;
            cp = next;
        }
        let _ = EndDoc(dc);
        let _ = DeleteDC(dc);
        Ok(())
    }
}

/// Map a shape's client-px rect to page twips: offset from page content rect,
/// scaled by zoom, converted px→twips (96dpi → *1440/96).
fn shape_twips(rect: RECT, page_content_px: RECT, zoom_pct: i32, ml_tw: i32, mt_tw: i32) -> RECT {
    let px_to_tw = |px: i64| -> i32 { (px * 1440 * 100 / (96 * zoom_pct.max(1) as i64)) as i32 };
    RECT {
        left: ml_tw + px_to_tw((rect.left - page_content_px.left) as i64),
        top: mt_tw + px_to_tw((rect.top - page_content_px.top) as i64),
        right: ml_tw + px_to_tw((rect.right - page_content_px.left) as i64),
        bottom: mt_tw + px_to_tw((rect.bottom - page_content_px.top) as i64),
    }
}

fn draw_shapes_gdi(dc: HDC, shapes: &[HWND], page: &PageSetup, page_content_px: RECT, zoom_pct: i32, dpi: i32) {
    let tw2px = |tw: i32| tw * dpi / 1440;
    unsafe {
        for &s in shapes {
            let Some(sp) = shape::get_props(s) else { continue };
            let mut wr = RECT::default();
            if GetWindowRect(s, &mut wr).is_err() {
                continue;
            }
            // client px → page twips → printer px
            let t = shape_twips(wr, page_content_px, zoom_pct, page.ml_tw, page.mt_tw);
            let (l, tp, r, b) = (tw2px(t.left), tw2px(t.top), tw2px(t.right), tw2px(t.bottom));

            let pen = CreatePen(PS_SOLID, (sp.outline_w * dpi / 96).max(1), sp.outline);
            let brush = if !sp.has_fill {
                HBRUSH(GetStockObject(NULL_BRUSH).0)
            } else {
                CreateSolidBrush(sp.fill)
            };
            let op = SelectObject(dc, pen);
            let ob = SelectObject(dc, brush);
            match sp.kind {
                ShapeKind::Ellipse => {
                    let _ = Ellipse(dc, l, tp, r, b);
                }
                ShapeKind::RoundedRect => {
                    let _ = RoundRect(dc, l, tp, r, b, 120, 120);
                }
                ShapeKind::Line => {
                    let _ = MoveToEx(dc, l, tp, None);
                    let _ = LineTo(dc, r, b);
                }
                _ => {
                    let _ = Rectangle(dc, l, tp, r, b);
                }
            }
            SelectObject(dc, ob);
            SelectObject(dc, op);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);

            // inner text
            let ie = shape::inner_edit(s);
            if let Some(d) = tom_document(ie) {
                if let Some(range) = editor::story_range(&d) {
                    if let Ok(txt) = range.GetText() {
                        let text = txt.to_string();
                        if !text.trim().is_empty() {
                            let (name, pt) = inner_font(&d);
                            let fname = to_w(&name);
                            let mut lf = LOGFONTW::default();
                            lf.lfHeight = -(pt * dpi as f32 / 72.0) as i32;
                            lf.lfCharSet = DEFAULT_CHARSET;
                            let n = fname.len().min(31);
                            lf.lfFaceName[..n].copy_from_slice(&fname[..n]);
                            let font = CreateFontIndirectW(&lf);
                            let of = SelectObject(dc, font);
                            SetBkMode(dc, TRANSPARENT);
                            let ins = |px: i32| px * dpi / 96;
                            let mut tr = RECT {
                                left: l + ins(sp.inset_l),
                                top: tp + ins(sp.inset_t),
                                right: r - ins(sp.inset_r),
                                bottom: b - ins(sp.inset_b),
                            };
                            let mut buf = to_w(text.trim_end_matches(['\r', '\n']));
                            DrawTextW(dc, &mut buf, &mut tr, DT_LEFT | DT_TOP | DT_WORDBREAK);
                            SelectObject(dc, of);
                            let _ = DeleteObject(font);
                        }
                    }
                }
            }
        }
    }
}

fn inner_font(doc: &windows::Win32::UI::Controls::RichEdit::ITextDocument) -> (String, f32) {
    unsafe {
        if let Ok(r) = doc.Range(0, 0) {
            if let Ok(f) = r.GetFont() {
                let name = f.GetName().map(|b| b.to_string()).unwrap_or_else(|_| "Calibri".into());
                let pt = f.GetSize().unwrap_or(11.0).max(6.0);
                return (name, pt);
            }
        }
    }
    ("Calibri".into(), 11.0)
}

// ---------- path 2: hand-written minimal PDF ----------

/// Map any char to WinAnsi-encodable bytes; unrepresentable → nearest ASCII.
fn pdf_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = match c {
            'ə' | 'Ə' => 'e',
            'ğ' | 'Ğ' => 'g',
            'ı' => 'i',
            'İ' => 'I',
            'ş' | 'Ş' => 's',
            'ç' | 'Ç' => 'c',
            'ö' | 'Ö' => 'o',
            'ü' | 'Ü' => 'u',
            '–' | '—' => '-',
            '‘' | '’' => '\'',
            '“' | '”' => '"',
            _ => c,
        };
        if (c as u32) < 128 || (c as u32) < 256 {
            match c {
                '(' | ')' | '\\' => {
                    out.push('\\');
                    out.push(c);
                }
                _ => out.push(c),
            }
        } else {
            out.push('?');
        }
    }
    out
}

struct RunTxt {
    text: String,
    bold: bool,
    italic: bool,
    size: f32,
    color: u32,
}

fn collect_runs(editor: HWND) -> Vec<Vec<RunTxt>> {
    let mut paras: Vec<Vec<RunTxt>> = Vec::new();
    if let Some(doc) = tom_document(editor) {
        unsafe {
            let story = editor::story_len(&doc);
            let mut pos = 0i32;
            while pos < story {
                let pr = doc.Range(pos, pos).ok();
                let para_end = match &pr {
                    Some(r) => {
                        let _ = r.MoveEnd(editor::TOM_PARA, 1);
                        r.GetEnd().unwrap_or(story)
                    }
                    None => story,
                };
                let mut runs = Vec::new();
                let mut rp = pos;
                while rp < para_end {
                    let r0 = doc.Range(rp, para_end).ok();
                    let next = match &r0 {
                        Some(rr) => {
                            let _ = rr.MoveEnd(editor::TOM_CHARFMT, 1);
                            rr.GetEnd().unwrap_or(para_end).min(para_end)
                        }
                        None => para_end,
                    };
                    if let Some(rr) = doc.Range(rp, next).ok() {
                        let txt = rr.GetText().unwrap_or_default().to_string();
                        let clean = txt.trim_end_matches(['\r', '\n']);
                        if !clean.is_empty() {
                            let mut rt = RunTxt {
                                text: clean.to_string(),
                                bold: false,
                                italic: false,
                                size: 11.0,
                                color: 0,
                            };
                            if let Ok(f) = rr.GetFont() {
                                rt.bold = f.GetBold().unwrap_or(0) != 0;
                                rt.italic = f.GetItalic().unwrap_or(0) != 0;
                                rt.size = f.GetSize().unwrap_or(11.0);
                                rt.color = f.GetForeColor().unwrap_or(0) as u32;
                            }
                            runs.push(rt);
                        }
                    }
                    rp = next.max(rp + 1);
                }
                paras.push(runs);
                pos = para_end.max(pos + 1);
            }
        }
    }
    paras
}

/// rough average char width factor per font family (pt fraction)
fn char_factor(bold: bool, italic: bool) -> f32 {
    if bold { 0.56 } else if italic { 0.52 } else { 0.50 }
}

fn simple_pdf(
    editor: HWND,
    shapes: &[HWND],
    page: &PageSetup,
    path: &str,
    page_content_px: RECT,
    zoom_pct: i32,
) -> Result<(), String> {
    let tw2pt = |tw: i32| tw as f32 / 20.0; // twips → pdf points (72/inch)
    let pw = tw2pt(page.w_tw);
    let ph = tw2pt(page.h_tw);
    let ml = tw2pt(page.ml_tw);
    let mt = tw2pt(page.mt_tw);
    let mr = tw2pt(page.mr_tw);
    let mb = tw2pt(page.mb_tw);

    let paras = collect_runs(editor);

    // wrap text into (x-offset words) lines — simple greedy wrap
    struct Line {
        segs: Vec<(String, f32, usize, u32)>, // text, size, font idx, color
        height: f32,
    }
    let content_w = pw - ml - mr;
    let mut lines: Vec<Line> = Vec::new();
    for runs in &paras {
        if runs.is_empty() {
            lines.push(Line { segs: vec![], height: 14.0 });
            continue;
        }
        let mut cur = Line { segs: vec![], height: 0.0 };
        let mut cur_w = 0f32;
        for r in runs {
            let fidx = if r.bold && r.italic {
                4
            } else if r.bold {
                2
            } else if r.italic {
                3
            } else {
                1
            };
            for word in r.text.split(' ') {
                if word.is_empty() {
                    continue;
                }
                let ww = word.len() as f32 * r.size * char_factor(r.bold, r.italic) + r.size * 0.5;
                if cur_w + ww > content_w && !cur.segs.is_empty() {
                    lines.push(std::mem::replace(&mut cur, Line { segs: vec![], height: 0.0 }));
                    cur_w = 0.0;
                }
                cur.segs.push((format!("{} ", word), r.size, fidx, r.color));
                cur.height = cur.height.max(r.size * 1.3);
                cur_w += ww;
            }
        }
        if cur.height == 0.0 {
            cur.height = 14.0;
        }
        lines.push(cur);
    }

    let col = |c: u32, op: &str| {
        format!(
            "{:.2} {:.2} {:.2} {}",
            (c & 0xFF) as f32 / 255.0,
            ((c >> 8) & 0xFF) as f32 / 255.0,
            ((c >> 16) & 0xFF) as f32 / 255.0,
            op
        )
    };

    // shapes → (kind, rect in points, inner text, colors)
    struct Sh {
        kind: ShapeKind,
        l: f32,
        t: f32,
        r: f32,
        b: f32,
        line: u32,
        fill: u32,
        has_fill: bool,
        has_outline: bool,
        text: Vec<Vec<RunTxt>>,
        ins_l: f32,
        ins_t: f32,
        ins_r: f32,
        ins_b: f32,
    }
    let px2pt = |px: i64| -> f32 { (px * 72 * 100 / (96 * zoom_pct.max(1) as i64)) as f32 };
    let mut shs: Vec<Sh> = Vec::new();
    unsafe {
        for &s in shapes {
            let Some(sp) = shape::get_props(s) else { continue };
            let mut wr = RECT::default();
            if GetWindowRect(s, &mut wr).is_err() {
                continue;
            }
            let mut texts = Vec::new();
            if let Some(d) = tom_document(shape::inner_edit(s)) {
                texts = shape_text_runs(&d);
            }
            shs.push(Sh {
                kind: sp.kind,
                l: ml + px2pt((wr.left - page_content_px.left) as i64),
                t: mt + px2pt((wr.top - page_content_px.top) as i64),
                r: ml + px2pt((wr.right - page_content_px.left) as i64),
                b: mt + px2pt((wr.bottom - page_content_px.top) as i64),
                line: sp.outline.0,
                fill: sp.fill.0,
                has_fill: sp.has_fill,
                has_outline: sp.has_outline,
                text: texts,
                ins_l: sp.inset_l as f32,
                ins_t: sp.inset_t as f32,
                ins_r: sp.inset_r as f32,
                ins_b: sp.inset_b as f32,
            });
        }
    }

    // paginate lines
    let mut pages: Vec<String> = Vec::new();
    let mut content = String::new();
    let mut y = mt;
    for line in &lines {
        if y + line.height > ph - mb && !content.is_empty() {
            pages.push(std::mem::take(&mut content));
            y = mt;
        }
        let mut x = ml;
        if line.segs.is_empty() {
            y += line.height;
            continue;
        }
        for (txt, sz, fidx, c) in &line.segs {
            content.push_str(&format!(
                "BT /F{} {:.1} Tf {} {:.1} {:.1} Td ({}) Tj ET\n",
                fidx,
                sz,
                col(*c, "rg"),
                x,
                ph - y - sz,
                pdf_escape(txt)
            ));
            x += txt.len() as f32 * sz * 0.5;
        }
        y += line.height;
    }
    // shapes

    for sh in &shs {
        let top = ph - sh.t;
        let bot = ph - sh.b;
        if sh.has_fill {
            content.push_str(&col(sh.fill, "rg"));
            match sh.kind {
                ShapeKind::Ellipse => content.push_str(&ellipse_path(sh.l, top, sh.r, bot, true)),
                _ => content.push_str(&format!("{:.1} {:.1} {:.1} {:.1} re f\n", sh.l, bot, sh.r - sh.l, top - bot)),
            }
        }
        if sh.has_outline {
            content.push_str(&col(sh.line, "RG"));
            content.push_str("1 w\n");
            match sh.kind {
                ShapeKind::Ellipse => content.push_str(&ellipse_path(sh.l, top, sh.r, bot, false)),
                ShapeKind::Line => content.push_str(&format!("{:.1} {:.1} m {:.1} {:.1} l S\n", sh.l, top, sh.r, bot)),
                _ => content.push_str(&format!("{:.1} {:.1} {:.1} {:.1} re S\n", sh.l, bot, sh.r - sh.l, top - bot)),
            }
        }
        let mut sy = sh.t + sh.ins_t;
        for runs in &sh.text {
            for r in runs {
                if sy > sh.b - sh.ins_b {
                    break;
                }
                let fidx = if r.bold { 2 } else { 1 };
                content.push_str(&format!(
                    "BT /F{} {:.1} Tf {:.1} {:.1} Td ({}) Tj ET\n",
                    fidx,
                    r.size.min(14.0),
                    sh.l + sh.ins_l,
                    ph - sy - r.size,
                    pdf_escape(&r.text)
                ));
                sy += r.size * 1.3;
            }
            if runs.is_empty() {
                sy += 14.0;
            }
        }
    }
    if !content.is_empty() {
        pages.push(content);
    }
    if pages.is_empty() {
        pages.push(String::new());
    }

    // ---- write objects ----
    let catalog_id = 1usize;
    let pages_id = 2usize;
    let mut next = 3usize;
    let font_ids: Vec<usize> = (0..6).map(|i| next + i).collect();
    next += 6;
    let content_ids: Vec<usize> = (0..pages.len()).map(|i| next + i).collect();
    next += pages.len();
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| next + i).collect();

    // rebuild buffer properly
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs: Vec<usize> = Vec::new();
    let mut push_obj = |id: usize, body: String, out: &mut Vec<u8>, offs: &mut Vec<usize>| {
        while offs.len() < id {
            offs.push(0);
        }
        offs[id - 1] = out.len();
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", id, body).as_bytes());
    };
    let kids = page_ids.iter().map(|i| format!("{} 0 R", i)).collect::<Vec<_>>().join(" ");
    push_obj(catalog_id, format!("<< /Type /Catalog /Pages {} 0 R >>", pages_id), &mut out, &mut offs);
    push_obj(pages_id, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids, page_ids.len()), &mut out, &mut offs);
    let fonts = ["Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique", "Times-Roman", "Courier"];
    for (i, f) in fonts.iter().enumerate() {
        push_obj(font_ids[i], format!("<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>", f), &mut out, &mut offs);
    }
    for (i, c) in pages.iter().enumerate() {
        push_obj(
            content_ids[i],
            format!("<< /Length {} >>\nstream\n{}\nendstream", c.len(), c),
            &mut out,
            &mut offs,
        );
    }
    let font_res = font_ids
        .iter()
        .enumerate()
        .map(|(i, id)| format!("/F{} {} 0 R", i + 1, id))
        .collect::<Vec<_>>()
        .join(" ");
    for (i, pid) in page_ids.iter().enumerate() {
        push_obj(
            *pid,
            format!(
                "<< /Type /Page /Parent {} 0 R /MediaBox [0 0 {:.1} {:.1}] /Contents {} 0 R /Resources << /Font << {} >> >> >>",
                pages_id, pw, ph, content_ids[i], font_res
            ),
            &mut out,
            &mut offs,
        );
    }
    let xref_pos = out.len();
    let total = page_ids.last().copied().unwrap_or(2);
    out.extend_from_slice(format!("xref\n0 {}\n", total + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for i in 1..=total {
        out.extend_from_slice(format!("{:010} 00000 n \n", offs[i - 1]).as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{}\n%%%%EOF\n", total + 1, catalog_id, xref_pos).as_bytes(),
    );
    std::fs::write(path, &out).map_err(|e| e.to_string())?;
    Ok(())
}

fn shape_text_runs(
    doc: &windows::Win32::UI::Controls::RichEdit::ITextDocument,
) -> Vec<Vec<RunTxt>> {
    let mut paras: Vec<Vec<RunTxt>> = Vec::new();
    unsafe {
        let story = editor::story_len(doc);
        let mut pos = 0i32;
        while pos < story {
            let pr = doc.Range(pos, pos).ok();
            let para_end = match &pr {
                Some(r) => {
                    let _ = r.MoveEnd(editor::TOM_PARA, 1);
                    r.GetEnd().unwrap_or(story)
                }
                None => story,
            };
            let mut runs = Vec::new();
            if let Some(rr) = doc.Range(pos, para_end).ok() {
                let txt = rr.GetText().unwrap_or_default().to_string();
                let clean = txt.trim_end_matches(['\r', '\n']);
                if !clean.is_empty() {
                    let mut rt = RunTxt { text: clean.to_string(), bold: false, italic: false, size: 11.0, color: 0 };
                    if let Ok(f) = rr.GetFont() {
                        rt.bold = f.GetBold().unwrap_or(0) != 0;
                        rt.italic = f.GetItalic().unwrap_or(0) != 0;
                        rt.size = f.GetSize().unwrap_or(11.0);
                    }
                    runs.push(rt);
                }
            }
            paras.push(runs);
            pos = para_end.max(pos + 1);
        }
    }
    paras
}

/// PDF path ops for an ellipse inscribed in (l,top)-(r,bot) — y already flipped.
fn ellipse_path(l: f32, top: f32, r: f32, bot: f32, fill: bool) -> String {
    let cx = (l + r) / 2.0;
    let cy = (top + bot) / 2.0;
    let rx = (r - l) / 2.0;
    let ry = (top - bot) / 2.0;
    let k = 0.5523; // bezier circle approx
    let mut s = String::new();
    s.push_str(&format!("{:.1} {:.1} m\n", cx + rx, cy));
    s.push_str(&format!(
        "{:.1} {:.1} {:.1} {:.1} {:.1} {:.1} c\n",
        cx + rx, cy + ry * k, cx + rx * k, cy + ry, cx, cy + ry
    ));
    s.push_str(&format!(
        "{:.1} {:.1} {:.1} {:.1} {:.1} {:.1} c\n",
        cx - rx * k, cy + ry, cx - rx, cy + ry * k, cx - rx, cy
    ));
    s.push_str(&format!(
        "{:.1} {:.1} {:.1} {:.1} {:.1} {:.1} c\n",
        cx - rx, cy - ry * k, cx - rx * k, cy - ry, cx, cy - ry
    ));
    s.push_str(&format!(
        "{:.1} {:.1} {:.1} {:.1} {:.1} {:.1} c\n",
        cx + rx * k, cy - ry, cx + rx, cy - ry * k, cx + rx, cy
    ));
    s.push_str(if fill { "f\n" } else { "S\n" });
    s
}


