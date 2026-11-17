//! Small helpers: wide strings, loword/hiword, signed/unsigned extraction.

pub fn to_w(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

#[inline]
pub fn loword(l: isize) -> i32 {
    (l & 0xffff) as i32
}

#[inline]
pub fn hiword(l: isize) -> i32 {
    ((l >> 16) & 0xffff) as i32
}

#[inline]
pub fn loword_signed(l: isize) -> i32 {
    ((l & 0xffff) as u16) as i16 as i32
}

#[inline]
pub fn hiword_signed(l: isize) -> i32 {
    (((l >> 16) & 0xffff) as u16) as i16 as i32
}

/// twips -> points
#[inline]
pub fn twips_to_pt(tw: i32) -> f32 {
    tw as f32 / 20.0
}

/// points -> twips
#[inline]
pub fn pt_to_twips(pt: f32) -> i32 {
    (pt * 20.0).round() as i32
}

/// POINTS -> twips (Word/OOXML also uses "half-points" for fonts)
#[inline]
pub fn hp_to_pt(hp: i32) -> f32 {
    hp as f32 / 2.0
}

#[inline]
pub fn pt_to_hp(pt: f32) -> i32 {
    (pt * 2.0).round() as i32
}
