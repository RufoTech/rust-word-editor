//! Word-like color theme (Office 2016+ light theme approximation).

use windows::Win32::Foundation::COLORREF;

pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

// Word 2016+ "Colorful" theme
pub const TITLE_BAR: COLORREF = rgb(0xB9, 0xD3, 0xE8); // app title / quick access strip
pub const TAB_STRIP: COLORREF = rgb(0xF3, 0xF3, 0xF3); // tab row background
pub const TAB_ACTIVE: COLORREF = rgb(0xFF, 0xFF, 0xFF); // selected tab
pub const TAB_TEXT: COLORREF = rgb(0x26, 0x26, 0x26);
pub const TAB_TEXT_HOT: COLORREF = rgb(0x2B, 0x57, 0x9A);
pub const FILE_TAB: COLORREF = rgb(0x2B, 0x57, 0x9A); // File button blue
pub const FILE_TAB_TEXT: COLORREF = rgb(0xFF, 0xFF, 0xFF);

pub const RIBBON_BG: COLORREF = rgb(0xF5, 0xF6, 0xF7); // ribbon body
pub const GROUP_SEP: COLORREF = rgb(0xD4, 0xD4, 0xD4);
pub const GROUP_LABEL: COLORREF = rgb(0x66, 0x66, 0x66);
pub const BTN_HOT: COLORREF = rgb(0xDC, 0xE8, 0xF8); // hover
pub const BTN_HOT_BORDER: COLORREF = rgb(0x70, 0xA3, 0xD8);
pub const BTN_CHECKED: COLORREF = rgb(0xB4, 0xD2, 0xF0); // toggled
pub const BTN_CHECKED_BORDER: COLORREF = rgb(0x4D, 0x8A, 0xC8);
pub const BTN_TEXT: COLORREF = rgb(0x26, 0x26, 0x26);
pub const BTN_DISABLED_TEXT: COLORREF = rgb(0xA0, 0xA0, 0xA0);

pub const PAGE_BG: COLORREF = rgb(0xEE, 0xF1, 0xF5); // gray canvas around page
pub const PAGE_WHITE: COLORREF = rgb(0xFF, 0xFF, 0xFF);
pub const STATUS_BAR: COLORREF = rgb(0xF3, 0xF3, 0xF3);
pub const STATUS_TEXT: COLORREF = rgb(0x44, 0x44, 0x44);

pub const ACCENT: COLORREF = rgb(0x2B, 0x57, 0x9A);
pub const SELECTION_BORDER: COLORREF = rgb(0x2B, 0x57, 0x9A);

// Word standard color palette rows used by the color popup.
pub const WORD_COLORS: [COLORREF; 10] = [
    rgb(0x00, 0x00, 0x00),
    rgb(0x44, 0x54, 0x6A),
    rgb(0xE7, 0xE6, 0xE6),
    rgb(0xFF, 0xFF, 0xFF),
    rgb(0x2E, 0x74, 0xB5),
    rgb(0x70, 0xAD, 0x47),
    rgb(0xFF, 0xC0, 0x00),
    rgb(0xED, 0x7D, 0x31),
    rgb(0xFF, 0x00, 0x00),
    rgb(0xC0, 0x00, 0x00),
];

pub const THEME_SHADES: [u8; 5] = [80, 60, 40, 20, 0]; // light->dark intensity

pub fn shade(c: COLORREF, pct: u8) -> COLORREF {
    // pct: 0 = color, 100 = white
    let f = pct as f32 / 100.0;
    let ch = |v: u32| -> u32 { (v as f32 + (255.0 - v as f32) * f) as u32 };
    let r = ch(c.0 & 0xFF);
    let g = ch((c.0 >> 8) & 0xFF);
    let b = ch((c.0 >> 16) & 0xFF);
    COLORREF(r | (g << 8) | (b << 16))
}
