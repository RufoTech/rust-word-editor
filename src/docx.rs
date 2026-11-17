//! DOCX (OOXML) read/write — Word-compatible subset.
//! Read: paragraphs, run properties (font/size/color/b/i/u/s/spacing/position),
//! paragraph props (align, spacing, indents, bullets), section setup.
//! Write: same subset + shapes as wps text boxes.

use crate::app::PageSetup;
use crate::editor::{self, tom_document};
use crate::util::{pt_to_hp, pt_to_twips, twips_to_pt};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::fmt::Write as _;
use std::io::{Read, Write};
use windows::core::BSTR;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::RichEdit::*;

// ---------- XML helpers ----------

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn hex(color: i32) -> String {
    let c = color as u32;
    format!("{:02X}{:02X}{:02X}", c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF)
}

fn attr<'a>(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .map(|a| String::from_utf8_lossy(&a.value).to_string())
}

fn wval<'a>(e: &BytesStart) -> Option<String> {
    attr(e, b"w:val")
}

// ---------- WRITE ----------

#[derive(Default)]
struct RunFmt {
    bold: bool,
    italic: bool,
    underline: i32,
    strike: bool,
    size_pt: f32,
    name: String,
    color: i32,
    back: i32,
    spacing_pt: f32,
    pos_pt: f32,
}

#[derive(Default)]
struct ParaFmt {
    align: i32,
    space_before: f32,
    space_after: f32,
    ls_rule: i32,
    ls_val: f32,
    first: f32,
    left: f32,
    right: f32,
    list_type: i32,
}

fn para_xml(p: &ParaFmt, out: &mut String) {
    let mut pr = String::new();
    if p.list_type != 0 {
        pr.push_str("<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>");
    }
    if p.space_before != 0.0 || p.space_after != 0.0 || p.ls_rule != 0 || p.ls_val != 1.0 {
        let (line, rule) = match p.ls_rule {
            1 => (360, "auto"),   // 1.5 lines
            2 => (480, "auto"),   // double
            3 => (pt_to_twips(p.ls_val), "atLeast"),
            4 => (pt_to_twips(p.ls_val), "exact"),
            5 => (((p.ls_val * 240.0) as i32), "auto"),
            _ => (240, "auto"),
        };
        let _ = write!(
            pr,
            "<w:spacing w:before=\"{}\" w:after=\"{}\" w:line=\"{}\" w:lineRule=\"{}\"/>",
            pt_to_twips(p.space_before),
            pt_to_twips(p.space_after),
            line,
            rule
        );
    }
    if p.left != 0.0 || p.right != 0.0 || p.first != 0.0 {
        let _ = write!(
            pr,
            "<w:ind w:left=\"{}\" w:right=\"{}\" w:firstLine=\"{}\"/>",
            pt_to_twips(p.left),
            pt_to_twips(p.right),
            pt_to_twips(p.first)
        );
    }
    if p.align != 0 {
        let a = match p.align {
            1 => "center",
            2 => "right",
            3 => "both",
            _ => "left",
        };
        let _ = write!(pr, "<w:jc w:val=\"{}\"/>", a);
    }
    if !pr.is_empty() {
        let _ = write!(out, "<w:pPr>{}</w:pPr>", pr);
    }
}

fn run_xml(r: &RunFmt, text: &str, out: &mut String) {
    let mut rp = String::new();
    if r.bold {
        rp.push_str("<w:b/>");
    }
    if r.italic {
        rp.push_str("<w:i/>");
    }
    if r.underline != 0 {
        let _ = write!(rp, "<w:u w:val=\"single\"/>");
    }
    if r.strike {
        rp.push_str("<w:strike/>");
    }
    if !r.name.is_empty() {
        let _ = write!(rp, "<w:rFonts w:ascii=\"{0}\" w:hAnsi=\"{0}\" w:cs=\"{0}\"/>", r.name);
    }
    if r.color != 0 && r.color != -16777216 && r.color != -9999997 {
        let _ = write!(rp, "<w:color w:val=\"{}\"/>", hex(r.color));
    }
    if r.size_pt != 0.0 {
        let _ = write!(rp, "<w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/>", pt_to_hp(r.size_pt), pt_to_hp(r.size_pt));
    }
    if r.spacing_pt != 0.0 {
        let _ = write!(rp, "<w:spacing w:val=\"{}\"/>", pt_to_twips(r.spacing_pt));
    }
    if r.pos_pt != 0.0 {
        let _ = write!(rp, "<w:position w:val=\"{}\"/>", pt_to_hp(r.pos_pt));
    }
    // back < 0 means tomAutoColor (-9999997) — no highlight
    if r.back > 0 {
        let _ = write!(rp, "<w:shd w:val=\"clear\" w:fill=\"{}\"/>", hex(r.back));
    }
    out.push_str("<w:r>");
    if !rp.is_empty() {
        let _ = write!(out, "<w:rPr>{}</w:rPr>", rp);
    }
    let _ = write!(out, "<w:t xml:space=\"preserve\">");
    esc(text, out);
    out.push_str("</w:t></w:r>");
}

fn read_run_fmt(r: &ITextRange) -> RunFmt {
    let mut f = RunFmt::default();
    unsafe {
        if let Ok(font) = r.GetFont() {
            f.bold = font.GetBold().unwrap_or(0) == -1;
            f.italic = font.GetItalic().unwrap_or(0) == -1;
            f.underline = font.GetUnderline().unwrap_or(0);
            f.strike = font.GetStrikeThrough().unwrap_or(0) == -1;
            f.size_pt = font.GetSize().unwrap_or(11.0);
            f.name = font.GetName().map(|b| b.to_string()).unwrap_or_default();
            f.color = font.GetForeColor().unwrap_or(0);
            f.back = font.GetBackColor().unwrap_or(0);
            f.spacing_pt = font.GetSpacing().unwrap_or(0.0);
            f.pos_pt = font.GetPosition().unwrap_or(0.0);
        }
    }
    f
}

fn read_para_fmt(r: &ITextRange) -> ParaFmt {
    let mut p = ParaFmt::default();
    unsafe {
        if let Ok(par) = r.GetPara() {
            p.align = par.GetAlignment().unwrap_or(0);
            p.space_before = par.GetSpaceBefore().unwrap_or(0.0);
            p.space_after = par.GetSpaceAfter().unwrap_or(0.0);
            p.ls_rule = par.GetLineSpacingRule().unwrap_or(0);
            p.ls_val = par.GetLineSpacing().unwrap_or(1.0);
            p.first = par.GetFirstLineIndent().unwrap_or(0.0);
            p.left = par.GetLeftIndent().unwrap_or(0.0);
            p.right = par.GetRightIndent().unwrap_or(0.0);
            p.list_type = par.GetListType().unwrap_or(0);
        }
    }
    p
}

fn document_xml(editor: HWND, page: &PageSetup, shapes: &[HWND]) -> String {
    let mut body = String::new();
    if let Some(doc) = tom_document(editor) {
        unsafe {
            let story = editor::story_len(&doc);
            let mut pos = 0i32;
            while pos < story {
                // find end of this paragraph
                let pr = doc.Range(pos, pos).ok();
                let para_end = match &pr {
                    Some(r) => {
                        let _ = r.MoveEnd(editor::TOM_PARA, 1);
                        r.GetEnd().unwrap_or(story)
                    }
                    None => story,
                };
                let para_range = doc.Range(pos, para_end).ok();
                if let Some(para_range) = para_range {
                    let pfmt = read_para_fmt(&para_range);
                    body.push_str("<w:p>");
                    para_xml(&pfmt, &mut body);
                    // iterate char-format runs inside paragraph
                    let mut rp = pos;
                    while rp < para_end {
                        let r0 = doc.Range(rp, para_end).ok();
                        let next = match &r0 {
                            Some(rr) => {
                                let _ = rr.MoveEnd(editor::TOM_CHARFMT, 1);
                                // charfmt units may extend past the paragraph mark — clamp
                                rr.GetEnd().unwrap_or(para_end).min(para_end)
                            }
                            None => para_end,
                        };
                        if let Some(rr) = doc.Range(rp, next).ok() {
                            let txt = rr.GetText().unwrap_or_default().to_string();
                            let clean = txt.trim_end_matches(['\r', '\n']);
                            if !clean.is_empty() {
                                let rf = read_run_fmt(&rr);
                                run_xml(&rf, clean, &mut body);
                            }
                        }
                        rp = next.max(rp + 1);
                    }
                    body.push_str("</w:p>");
                }
                pos = para_end.max(pos + 1);
            }
        }
    }
    if body.is_empty() {
        body.push_str("<w:p/>");
    }

    // shapes → anchored text boxes
    for (i, s) in shapes.iter().enumerate() {
        if let Some(sp) = crate::shape::get_props(*s) {
            let mut wr = windows::Win32::Foundation::RECT::default();
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(*s, &mut wr);
            }
            let txt = shape_text(*s);
            let wpx = (wr.right - wr.left).max(1) as i64;
            let hpx = (wr.bottom - wr.top).max(1) as i64;
            let cx = wpx * 914400 / 96;
            let cy = hpx * 914400 / 96;
            let geom = match sp.kind {
                crate::dropdown::ShapeKind::RoundedRect => "roundRect",
                crate::dropdown::ShapeKind::Ellipse => "ellipse",
                crate::dropdown::ShapeKind::RightArrow => "rightArrow",
                crate::dropdown::ShapeKind::DownArrow => "downArrow",
                _ => "rect",
            };
            let fill = if sp.has_fill { format!("<a:solidFill><a:srgbClr val=\"{:06X}\"/></a:solidFill>", sp.fill.0 & 0xFFFFFF) } else { "<a:noFill/>".into() };
            let _ = write!(
                body,
                "<w:p><w:r><w:drawing>\
<wp:anchor behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\" relativeHeight=\"{z}\" simplePos=\"0\">\
<wp:simplePos x=\"0\" y=\"0\"/>\
<wp:positionH relativeFrom=\"page\"><wp:posOffset>{x}</wp:posOffset></wp:positionH>\
<wp:positionV relativeFrom=\"page\"><wp:posOffset>{y}</wp:posOffset></wp:positionV>\
<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>\
<wp:wrapNone/><wp:docPr id=\"{id}\" name=\"Shape {id}\"/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
<a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
<wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr>\
<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
<a:prstGeom prst=\"{geom}\"><a:avLst/></a:prstGeom>{fill}</wps:spPr>\
<wps:txbx><w:txbxContent><w:p><w:r><w:t xml:space=\"preserve\">{txt}</w:t></w:r></w:p></w:txbxContent></wps:txbx>\
<wps:bodyPr lIns=\"{li}\" tIns=\"{ti}\" rIns=\"{ri}\" bIns=\"{bi}\"/></wps:wsp>\
</a:graphicData></a:graphic></wp:anchor></w:r></w:p>",
                z = 251659264 + i,
                x = wr.left as i64 * 914400 / 96,
                y = wr.top as i64 * 914400 / 96,
                cx = cx,
                cy = cy,
                id = i + 100,
                geom = geom,
                fill = fill,
                txt = {
                    let mut t = String::new();
                    esc(&txt, &mut t);
                    t
                },
                li = sp.inset_l as i64 * 914400 / 96,
                ti = sp.inset_t as i64 * 914400 / 96,
                ri = sp.inset_r as i64 * 914400 / 96,
                bi = sp.inset_b as i64 * 914400 / 96,
            );
        }
    }

    // NOTE: a `\` continuation in a Rust string literal strips the newline AND
    // all leading whitespace of the next line — each segment must therefore end
    // with an explicit space before the `\`.
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:wpc=\"http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas\" \
xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
xmlns:o=\"urn:schemas-microsoft-com:office:office\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" \
xmlns:v=\"urn:schemas-microsoft-com:vml\" \
xmlns:wp14=\"http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
xmlns:w10=\"urn:schemas-microsoft-com:office:word\" \
xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
xmlns:wpg=\"http://schemas.microsoft.com/office/word/2010/wordprocessingGroup\" \
xmlns:wpi=\"http://schemas.microsoft.com/office/word/2010/wordprocessingInk\" \
xmlns:wne=\"http://schemas.microsoft.com/office/word/2006/wordml\" \
xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\" \
mc:Ignorable=\"w14 wp14\">\
<w:body>{body}\
<w:sectPr>\
<w:pgSz w:w=\"{pw}\" w:h=\"{ph}\"{orient}/>\
<w:pgMar w:top=\"{mt}\" w:right=\"{mr}\" w:bottom=\"{mb}\" w:left=\"{ml}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>\
</w:sectPr></w:body></w:document>",
        body = body,
        pw = page.w_tw,
        ph = page.h_tw,
        orient = if page.landscape { " w:orient=\"landscape\"" } else { "" },
        mt = page.mt_tw,
        mr = page.mr_tw,
        mb = page.mb_tw,
        ml = page.ml_tw,
    )
}

fn shape_text(s: HWND) -> String {
    let e = crate::shape::inner_edit(s);
    if e.0.is_null() {
        return String::new();
    }
    if let Some(d) = tom_document(e) {
        unsafe {
            if let Ok(r) = d.Range(0, editor::story_len(&d)) {
                return r.GetText().unwrap_or_default().to_string().trim_end_matches('\r').to_string();
            }
        }
    }
    String::new()
}

const CT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>"#;

const RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const CORE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<dc:title>Document</dc:title><dc:creator>Rust Word</dc:creator></cp:coreProperties>"#;

const APP_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Rust Word</Application></Properties>"#;

pub fn save(path: &str, editor: HWND, page: &PageSetup, shapes: &[HWND]) -> Result<(), String> {
    if path.to_lowercase().ends_with(".txt") {
        if let Some(d) = tom_document(editor) {
            let t = crate::format::whole_text(&d);
            return std::fs::write(path, t.replace('\r', "\r\n")).map_err(|e| e.to_string());
        }
        return Err("no doc".into());
    }
    if path.to_lowercase().ends_with(".rtf") {
        return save_rtf(path, editor);
    }
    let xml = document_xml(editor, page, shapes);
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("[Content_Types].xml", opts).map_err(|e| e.to_string())?;
    zw.write_all(CT_XML.as_bytes()).map_err(|e| e.to_string())?;
    zw.start_file("_rels/.rels", opts).map_err(|e| e.to_string())?;
    zw.write_all(RELS_XML.as_bytes()).map_err(|e| e.to_string())?;
    zw.start_file("word/document.xml", opts).map_err(|e| e.to_string())?;
    zw.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
    zw.start_file("docProps/core.xml", opts).map_err(|e| e.to_string())?;
    zw.write_all(CORE_XML.as_bytes()).map_err(|e| e.to_string())?;
    zw.start_file("docProps/app.xml", opts).map_err(|e| e.to_string())?;
    zw.write_all(APP_XML.as_bytes()).map_err(|e| e.to_string())?;
    zw.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn save_rtf(path: &str, editor: HWND) -> Result<(), String> {
    unsafe {
        let mut es = EDITSTREAM {
            dwCookie: 0,
            dwError: 0,
            pfnCallback: Some(rtf_out_cb),
        };
        let mut buf: Vec<u8> = Vec::new();
        es.dwCookie = &mut buf as *mut _ as usize;
        SendMessageW(
            editor,
            EM_STREAMOUT,
            WPARAM((SF_RTF | SFF_PLAINRTF) as usize),
            LPARAM(&mut es as *mut _ as isize),
        );
        std::fs::write(path, buf).map_err(|e| e.to_string())
    }
}

unsafe extern "system" fn rtf_out_cb(cookie: usize, pb: *mut u8, cb: i32, pcb: *mut i32) -> u32 {
    let buf = &mut *(cookie as *mut Vec<u8>);
    buf.extend_from_slice(std::slice::from_raw_parts(pb, cb as usize));
    *pcb = cb;
    0
}

// ---------- READ ----------

struct Ctx {
    doc: ITextDocument,
    end: i32,
}

impl Ctx {
    fn insert(&mut self, text: &str) -> i32 {
        unsafe {
            if let Ok(r) = self.doc.Range(self.end, self.end) {
                let _ = r.SetText(&BSTR::from(text));
                self.end += text.encode_utf16().count() as i32;
            }
            self.end
        }
    }

    fn apply_run(&mut self, start: i32, f: &RunFmt) {
        unsafe {
            if let Ok(r) = self.doc.Range(start, self.end) {
                if let Ok(font) = r.GetFont() {
                    let _ = font.SetBold(if f.bold { -1 } else { 0 });
                    let _ = font.SetItalic(if f.italic { -1 } else { 0 });
                    if f.underline != 0 {
                        let _ = font.SetUnderline(f.underline);
                    }
                    let _ = font.SetStrikeThrough(if f.strike { -1 } else { 0 });
                    if f.size_pt != 0.0 {
                        let _ = font.SetSize(f.size_pt);
                    }
                    if !f.name.is_empty() {
                        let _ = font.SetName(&BSTR::from(f.name.as_str()));
                    }
                    if f.color != 0 {
                        let _ = font.SetForeColor(f.color);
                    }
                    if f.back != 0 {
                        let _ = font.SetBackColor(f.back);
                    }
                    if f.spacing_pt != 0.0 {
                        let _ = font.SetSpacing(f.spacing_pt);
                    }
                    if f.pos_pt != 0.0 {
                        let _ = font.SetPosition(f.pos_pt);
                    }
                }
            }
        }
    }

    fn apply_para(&mut self, start: i32, p: &ParaFmt) {
        unsafe {
            if let Ok(r) = self.doc.Range(start, self.end) {
                if let Ok(par) = r.GetPara() {
                    let _ = par.SetAlignment(p.align);
                    let _ = par.SetSpaceBefore(p.space_before);
                    let _ = par.SetSpaceAfter(p.space_after);
                    let _ = par.SetLineSpacing(p.ls_rule, p.ls_val);
                    let _ = par.SetIndents(p.first, p.left, p.right);
                    if p.list_type != 0 {
                        let _ = par.SetListType(1);
                    }
                }
            }
        }
    }
}

fn parse_rpr(e: &BytesStart, f: &mut RunFmt) {
    let local = e.local_name();
    match local.as_ref() {
        b"b" => {
            f.bold = !matches!(wval(e).as_deref(), Some("0") | Some("false") | Some("off"));
        }
        b"i" => {
            f.italic = !matches!(wval(e).as_deref(), Some("0") | Some("false") | Some("off"));
        }
        b"strike" | b"dstrike" => {
            f.strike = true;
        }
        b"u" => {
            f.underline = match wval(e).as_deref() {
                Some("none") | Some("0") => 0,
                Some("double") => 3,
                Some("words") => 2,
                _ => 1,
            };
        }
        b"sz" => {
            if let Some(v) = wval(e).and_then(|s| s.parse::<i32>().ok()) {
                f.size_pt = crate::util::hp_to_pt(v);
            }
        }
        b"rFonts" => {
            if let Some(n) = attr(e, b"w:ascii") {
                f.name = n;
            }
        }
        b"color" => {
            if let Some(v) = wval(e) {
                if let Ok(c) = u32::from_str_radix(&v, 16) {
                    f.color = windows::Win32::Foundation::COLORREF(c).0 as i32;
                    // docx is RGB hex; convert to COLORREF (BGR)
                    f.color = ((c & 0xFF) << 16 | (c & 0xFF00) | ((c >> 16) & 0xFF)) as i32;
                }
            }
        }
        b"spacing" => {
            if let Some(v) = wval(e).and_then(|s| s.parse::<i32>().ok()) {
                f.spacing_pt = twips_to_pt(v);
            }
        }
        b"position" => {
            if let Some(v) = wval(e).and_then(|s| s.parse::<i32>().ok()) {
                f.pos_pt = crate::util::hp_to_pt(v);
            }
        }
        b"shd" => {
            if let Some(v) = attr(e, b"w:fill") {
                if let Ok(c) = u32::from_str_radix(&v, 16) {
                    f.back = ((c & 0xFF) << 16 | (c & 0xFF00) | ((c >> 16) & 0xFF)) as i32;
                }
            }
        }
        b"highlight" => {
            if let Some(v) = wval(e) {
                let c = match v.as_str() {
                    "yellow" => 0x00FFFF,
                    "green" => 0x00FF00,
                    "cyan" => 0xFFFF00,
                    "magenta" => 0xFF00FF,
                    "red" => 0x0000FF,
                    "blue" => 0xFF0000,
                    _ => 0x00FFFF,
                };
                f.back = c;
            }
        }
        _ => {}
    }
}

fn parse_ppr(e: &BytesStart, p: &mut ParaFmt) {
    let local = e.local_name();
    match local.as_ref() {
        b"jc" => {
            p.align = match wval(e).as_deref() {
                Some("center") => 1,
                Some("right") | Some("end") => 2,
                Some("both") | Some("distribute") => 3,
                _ => 0,
            };
        }
        b"spacing" => {
            if let Some(v) = attr(e, b"w:before").and_then(|s| s.parse::<i32>().ok()) {
                p.space_before = twips_to_pt(v);
            }
            if let Some(v) = attr(e, b"w:after").and_then(|s| s.parse::<i32>().ok()) {
                p.space_after = twips_to_pt(v);
            }
            if let Some(v) = attr(e, b"w:line").and_then(|s| s.parse::<i32>().ok()) {
                let rule = attr(e, b"w:lineRule").unwrap_or_else(|| "auto".into());
                match rule.as_str() {
                    "atLeast" => {
                        p.ls_rule = 3;
                        p.ls_val = twips_to_pt(v);
                    }
                    "exact" => {
                        p.ls_rule = 4;
                        p.ls_val = twips_to_pt(v);
                    }
                    _ => {
                        p.ls_rule = 5;
                        p.ls_val = v as f32 / 240.0;
                    }
                }
            }
        }
        b"ind" => {
            if let Some(v) = attr(e, b"w:left").or_else(|| attr(e, b"w:start")).and_then(|s| s.parse::<i32>().ok()) {
                p.left = twips_to_pt(v);
            }
            if let Some(v) = attr(e, b"w:right").or_else(|| attr(e, b"w:end")).and_then(|s| s.parse::<i32>().ok()) {
                p.right = twips_to_pt(v);
            }
            if let Some(v) = attr(e, b"w:firstLine").and_then(|s| s.parse::<i32>().ok()) {
                p.first = twips_to_pt(v);
            }
            if let Some(v) = attr(e, b"w:hanging").and_then(|s| s.parse::<i32>().ok()) {
                p.first = -twips_to_pt(v);
            }
        }
        b"numPr" => {
            p.list_type = 1;
        }
        _ => {}
    }
}

/// Load a .docx / .txt into the editor.
pub fn load(path: &str, editor: HWND) -> Result<(), String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".txt") {
        let t = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if let Some(d) = tom_document(editor) {
            unsafe {
                if let Ok(r) = d.Range(0, editor::story_len(&d)) {
                    let _ = r.SetText(&BSTR::from(t.replace('\n', "\r").as_str()));
                }
            }
        }
        return Ok(());
    }
    if lower.ends_with(".rtf") {
        return load_rtf(path, editor);
    }

    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut za = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut xml = String::new();
    za.by_name("word/document.xml")
        .map_err(|_| "not a docx (missing word/document.xml)".to_string())?
        .read_to_string(&mut xml)
        .map_err(|e| e.to_string())?;

    let doc = tom_document(editor).ok_or("no doc interface")?;
    unsafe {
        if let Ok(r) = doc.Range(0, editor::story_len(&doc)) {
            let _ = r.SetText(&BSTR::from(""));
        }
    }
    let mut ctx = Ctx { doc, end: 0 };

    let mut reader = Reader::from_str(&xml);
    let mut rf = RunFmt::default();
    let mut pf = ParaFmt::default();
    let mut para_start = 0i32;
    let mut run_start = 0i32;
    let mut in_run = false;
    let mut in_p = false;
    let mut in_t = false;
    let mut in_rpr = false;
    let mut in_ppr = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                match e.local_name().as_ref() {
                    b"p" => {
                        in_p = true;
                        pf = ParaFmt::default();
                        para_start = ctx.end;
                    }
                    b"pPr" if in_p => in_ppr = true,
                    b"r" => {
                        in_run = true;
                        rf = RunFmt::default();
                        run_start = ctx.end;
                    }
                    b"rPr" if in_run => in_rpr = true,
                    b"t" => in_t = true,
                    b"br" | b"cr" => {
                        ctx.insert("\n");
                    }
                    b"tab" => {
                        ctx.insert("\t");
                    }
                    _ => {
                        if in_rpr {
                            parse_rpr(&e, &mut rf);
                        } else if in_ppr {
                            parse_ppr(&e, &mut pf);
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if in_rpr {
                    parse_rpr(&e, &mut rf);
                } else if in_ppr {
                    parse_ppr(&e, &mut pf);
                } else {
                    match e.local_name().as_ref() {
                        b"br" | b"cr" => {
                            ctx.insert("\n");
                        }
                        b"tab" => {
                            ctx.insert("\t");
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if in_t && in_run {
                    let t = e.unescape().unwrap_or_default().to_string();
                    ctx.insert(&t);
                }
            }
            Ok(Event::End(e)) => {
                match e.local_name().as_ref() {
                    b"t" => in_t = false,
                    b"rPr" => in_rpr = false,
                    b"pPr" => in_ppr = false,
                    b"r" => {
                        ctx.apply_run(run_start, &rf);
                        in_run = false;
                    }
                    b"p" => {
                        // end paragraph
                        ctx.insert("\r");
                        ctx.apply_para(para_start, &pf);
                        in_p = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("xml error: {}", e)),
            _ => {}
        }
    }
    unsafe {
        let _ = ctx.doc.SetSaved(tomConstants(editor::TOM_TRUE));
    }
    Ok(())
}

fn load_rtf(path: &str, editor: HWND) -> Result<(), String> {
    let buf = std::fs::read(path).map_err(|e| e.to_string())?;
    unsafe {
        struct Holder(Vec<u8>, usize);
        unsafe extern "system" fn cb(cookie: usize, pb: *mut u8, cb: i32, pcb: *mut i32) -> u32 {
            let h = &mut *(cookie as *mut Holder);
            let avail = h.0.len() - h.1;
            let n = (cb as usize).min(avail);
            std::ptr::copy_nonoverlapping(h.0[h.1..h.1 + n].as_ptr(), pb, n);
            h.1 += n;
            *pcb = n as i32;
            (n == 0) as u32
        }
        let mut h = Holder(buf, 0);
        let mut es = EDITSTREAM {
            dwCookie: &mut h as *mut _ as usize,
            dwError: 0,
            pfnCallback: Some(cb),
        };
        SendMessageW(editor, EM_STREAMIN, WPARAM((SF_RTF | SFF_PLAINRTF) as usize), LPARAM(&mut es as *mut _ as isize));
    }
    Ok(())
}
