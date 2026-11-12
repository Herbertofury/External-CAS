// SPDX-License-Identifier: GPL-3.0-or-later
//! In-memory TrueType adapter for installed compacted font curves. All source
//! glyphs and horizontal kerning are retained. No font is installed, written,
//! bundled or assigned inferred redistribution/embedding permission flags.
use crate::font::{CompactedFont, Edge, GlyphOutline};
use std::collections::BTreeMap;

const MAX_POINTS: usize = 3_000_000;
#[derive(Debug)]
pub struct ConvertedFont {
    pub bytes: Vec<u8>,
    pub glyph_count: usize,
    pub kerning_pairs: usize,
}
fn u16be(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn i16be(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn u32be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn i16_checked(value: i32) -> Result<i16, String> {
    i16::try_from(value).map_err(|_| "font coordinate exceeds TrueType i16 range".into())
}
fn flipped(value: i32) -> Result<i16, String> {
    i16_checked(
        value
            .checked_neg()
            .ok_or("font coordinate negation overflow")?,
    )
}
fn padded(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
fn sum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |value, part| {
        let mut word = [0; 4];
        word[..part.len()].copy_from_slice(part);
        value.wrapping_add(u32::from_be_bytes(word))
    })
}

/// Convert the source's first FontType into a standard, hint-free TrueType face.
/// Compacted source curves have no TrueType hint instructions to preserve.
pub fn convert(font: &CompactedFont<'_>) -> Result<ConvertedFont, String> {
    let count = font.glyphs.len();
    if count >= u16::MAX as usize {
        return Err("too many source glyphs for TrueType".into());
    }
    if !(16..=16384).contains(&font.header.nominal_size) {
        return Err("unsupported source font nominal size".into());
    }
    if !font.unparsed.is_empty() {
        return Err(
            "font contains additional FontTypes/unknown suffix; conversion would be incomplete"
                .into(),
        );
    }
    let mut codes = BTreeMap::new();
    for (index, glyph) in font.glyphs.iter().enumerate() {
        if codes.insert(glyph.code, (index + 1) as u16).is_some() {
            return Err("duplicate source glyph code".into());
        }
        if glyph.advance < 0 {
            return Err("negative source glyph advance unsupported by TrueType adapter".into());
        }
    }
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    let mut hmtx = Vec::new();
    // Required glyph zero: an empty missing glyph, separate from every source code.
    u32be(&mut loca, 0);
    u32be(&mut loca, 0);
    u16be(&mut hmtx, 0);
    i16be(&mut hmtx, 0);
    let mut global = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
    let mut max_points = 0;
    let mut max_contours = 0;
    let mut total_points = 0;
    let mut advance_max = 0;
    let mut min_lsb = i16::MAX;
    let mut min_rsb = i16::MAX;
    let mut max_extent = i16::MIN;
    for index in 0..count {
        let glyph = font.glyph_at(index, 100_000).map_err(|e| e.to_string())?;
        let (bytes, bounds, points, contours) = simple_glyph(&glyph)?;
        total_points += points;
        if total_points > MAX_POINTS {
            return Err("font point budget exceeded".into());
        }
        max_points = max_points.max(points);
        max_contours = max_contours.max(contours);
        if points > 0 {
            for i in 0..2 {
                global[i] = global[i].min(bounds[i]);
                global[i + 2] = global[i + 2].max(bounds[i + 2]);
            }
        }
        let advance = glyph.advance as u16;
        advance_max = advance_max.max(advance);
        min_lsb = min_lsb.min(bounds[0]);
        min_rsb = min_rsb.min(i16_checked(i32::from(advance) - i32::from(bounds[2]))?);
        max_extent = max_extent.max(bounds[2]);
        glyf.extend_from_slice(&bytes);
        padded(&mut glyf);
        u32be(&mut loca, glyf.len() as u32);
        u16be(&mut hmtx, advance);
        i16be(&mut hmtx, bounds[0]);
    }
    if global[0] == i16::MAX {
        global = [0; 4];
    }
    if count == 0 {
        min_lsb = 0;
        min_rsb = 0;
        max_extent = 0;
    }
    let mut head = Vec::new();
    u32be(&mut head, 0x00010000);
    u32be(&mut head, 0x00010000);
    u32be(&mut head, 0);
    u32be(&mut head, 0x5f0f3cf5);
    u16be(&mut head, 0);
    u16be(&mut head, font.header.nominal_size);
    head.extend_from_slice(&[0; 16]);
    for value in global {
        i16be(&mut head, value);
    }
    u16be(&mut head, if font.header.flags & 2 != 0 { 1 } else { 0 });
    u16be(&mut head, 8);
    i16be(&mut head, 2);
    i16be(&mut head, 1);
    i16be(&mut head, 0);
    let mut hhea = Vec::new();
    u32be(&mut hhea, 0x00010000);
    i16be(&mut hhea, font.header.ascent);
    i16be(&mut hhea, flipped(i32::from(font.header.descent))?);
    i16be(&mut hhea, font.header.leading);
    u16be(&mut hhea, advance_max);
    i16be(&mut hhea, min_lsb);
    i16be(&mut hhea, min_rsb);
    i16be(&mut hhea, max_extent);
    i16be(&mut hhea, 1);
    i16be(&mut hhea, 0);
    i16be(&mut hhea, 0);
    hhea.extend_from_slice(&[0; 8]);
    i16be(&mut hhea, 0);
    u16be(&mut hhea, (count + 1) as u16);
    let mut maxp = Vec::new();
    u32be(&mut maxp, 0x00010000);
    u16be(&mut maxp, (count + 1) as u16);
    u16be(
        &mut maxp,
        u16::try_from(max_points).map_err(|_| "glyph point count exceeds u16")?,
    );
    u16be(
        &mut maxp,
        u16::try_from(max_contours).map_err(|_| "glyph contour count exceeds u16")?,
    );
    for value in [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0] {
        u16be(&mut maxp, value);
    }
    let mut sub = Vec::new();
    u16be(&mut sub, 12);
    u16be(&mut sub, 0);
    u32be(&mut sub, (16 + 12 * codes.len()) as u32);
    u32be(&mut sub, 0);
    u32be(&mut sub, codes.len() as u32);
    for (code, id) in &codes {
        u32be(&mut sub, u32::from(*code));
        u32be(&mut sub, u32::from(*code));
        u32be(&mut sub, u32::from(*id));
    }
    let mut cmap = Vec::new();
    u16be(&mut cmap, 0);
    u16be(&mut cmap, 1);
    u16be(&mut cmap, 3);
    u16be(&mut cmap, 10);
    u32be(&mut cmap, 12);
    cmap.extend_from_slice(&sub);
    let mut post = vec![0; 32];
    post[..4].copy_from_slice(&0x00030000u32.to_be_bytes());
    let mut tables = BTreeMap::from([
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"hmtx", hmtx),
        (*b"glyf", glyf),
        (*b"loca", loca),
        (*b"cmap", cmap),
        (*b"post", post),
        (*b"name", name_table(&font.header.first_font_name)?),
    ]);
    let mut pairs = BTreeMap::new();
    for pair in &font.kerning {
        let left = *codes
            .get(&pair.left)
            .ok_or("source kerning names missing glyph")?;
        let right = *codes
            .get(&pair.right)
            .ok_or("source kerning names missing glyph")?;
        if pairs.insert((left, right), pair.advance).is_some() {
            return Err("duplicate source kerning pair".into());
        }
    }
    if !pairs.is_empty() {
        let len = 14 + pairs.len() * 6;
        if len > u16::MAX as usize {
            return Err("kerning table exceeds classic TrueType extent".into());
        }
        let n = pairs.len() as u16;
        let power = 1u16 << (15 - n.leading_zeros() as u16);
        let mut kern = Vec::new();
        u16be(&mut kern, 0);
        u16be(&mut kern, 1);
        u16be(&mut kern, 0);
        u16be(&mut kern, len as u16);
        u16be(&mut kern, 1);
        u16be(&mut kern, n);
        u16be(&mut kern, power * 6);
        u16be(&mut kern, power.trailing_zeros() as u16);
        u16be(&mut kern, n * 6 - power * 6);
        for ((left, right), advance) in &pairs {
            u16be(&mut kern, *left);
            u16be(&mut kern, *right);
            i16be(&mut kern, *advance);
        }
        tables.insert(*b"kern", kern);
    }
    let n = tables.len() as u16;
    let power = 1u16 << (15 - n.leading_zeros() as u16);
    let mut bytes = Vec::new();
    u32be(&mut bytes, 0x00010000);
    u16be(&mut bytes, n);
    u16be(&mut bytes, power * 16);
    u16be(&mut bytes, power.trailing_zeros() as u16);
    u16be(&mut bytes, n * 16 - power * 16);
    let mut offset = 12 + tables.len() * 16;
    let mut head_offset = 0;
    for (tag, data) in &tables {
        bytes.extend_from_slice(tag);
        u32be(&mut bytes, sum(data));
        u32be(&mut bytes, offset as u32);
        u32be(&mut bytes, data.len() as u32);
        if tag == b"head" {
            head_offset = offset;
        }
        offset += (data.len() + 3) & !3;
    }
    for data in tables.values() {
        bytes.extend_from_slice(data);
        padded(&mut bytes);
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(sum(&bytes));
    bytes[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(ConvertedFont {
        bytes,
        glyph_count: count,
        kerning_pairs: pairs.len(),
    })
}

fn simple_glyph(glyph: &GlyphOutline) -> Result<(Vec<u8>, [i16; 4], usize, usize), String> {
    let mut points = Vec::new();
    let mut ends = Vec::new();
    for contour in &glyph.contours {
        let start = points.len();
        let mut pen = contour.move_to;
        points.push((i16_checked(pen[0])?, flipped(pen[1])?, true));
        for edge in &contour.edges {
            match edge {
                Edge::Line { delta } => {
                    for i in 0..2 {
                        pen[i] = pen[i]
                            .checked_add(delta[i])
                            .ok_or("font coordinate overflow")?;
                    }
                }
                Edge::Quadratic {
                    control_delta,
                    anchor_delta,
                } => {
                    for i in 0..2 {
                        pen[i] = pen[i]
                            .checked_add(control_delta[i])
                            .ok_or("font coordinate overflow")?;
                    }
                    points.push((i16_checked(pen[0])?, flipped(pen[1])?, false));
                    for i in 0..2 {
                        pen[i] = pen[i]
                            .checked_add(anchor_delta[i])
                            .ok_or("font coordinate overflow")?;
                    }
                }
            }
            points.push((i16_checked(pen[0])?, flipped(pen[1])?, true));
        }
        if points.len() > start + 1 && points.last() == points.get(start) {
            points.pop();
        }
        if points.len() == start + 1 {
            points.pop();
            continue;
        }
        ends.push(
            u16::try_from(points.len() - 1).map_err(|_| "glyph points exceed TrueType extent")?,
        );
    }
    let mut bounds = [0; 4];
    if !points.is_empty() {
        bounds = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        for &(x, y, _) in &points {
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
        }
    }
    let mut bytes = Vec::new();
    i16be(
        &mut bytes,
        i16::try_from(ends.len()).map_err(|_| "glyph contours exceed TrueType extent")?,
    );
    for value in bounds {
        i16be(&mut bytes, value);
    }
    for end in &ends {
        u16be(&mut bytes, *end);
    }
    u16be(&mut bytes, 0);
    for &(_, _, on_curve) in &points {
        bytes.push(u8::from(on_curve));
    }
    let mut previous = 0i32;
    for &(x, _, _) in &points {
        i16be(&mut bytes, i16_checked(i32::from(x) - previous)?);
        previous = i32::from(x);
    }
    let mut previous = 0i32;
    for &(_, y, _) in &points {
        i16be(&mut bytes, i16_checked(i32::from(y) - previous)?);
        previous = i32::from(y);
    }
    Ok((bytes, bounds, points.len(), ends.len()))
}
fn name_table(name: &str) -> Result<Vec<u8>, String> {
    let records = [
        (1, name.to_owned()),
        (2, "Regular".into()),
        (4, name.to_owned()),
        (6, "ExternalCASLocalRuntimeFont".into()),
    ];
    let mut bytes = Vec::new();
    u16be(&mut bytes, 0);
    u16be(&mut bytes, records.len() as u16);
    u16be(&mut bytes, (6 + records.len() * 12) as u16);
    let mut strings = Vec::new();
    for (id, text) in records {
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        u16be(&mut bytes, 3);
        u16be(&mut bytes, 1);
        u16be(&mut bytes, 0x0409);
        u16be(&mut bytes, id);
        u16be(
            &mut bytes,
            u16::try_from(encoded.len()).map_err(|_| "font name too long")?,
        );
        u16be(
            &mut bytes,
            u16::try_from(strings.len()).map_err(|_| "font names too long")?,
        );
        strings.extend_from_slice(&encoded);
    }
    bytes.extend_from_slice(&strings);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quadratic_curve_keeps_control_point_and_y_conversion() {
        let glyph = GlyphOutline {
            code: 65,
            advance: 100,
            bounds: [0, -20, 100, 100],
            contours: vec![crate::font::Contour {
                move_to: [10, 20],
                edges: vec![
                    Edge::Quadratic {
                        control_delta: [20, -10],
                        anchor_delta: [40, 30],
                    },
                    Edge::Line { delta: [-60, -20] },
                ],
            }],
        };
        let (bytes, bounds, points, contours) = simple_glyph(&glyph).unwrap();
        assert_eq!(points, 3);
        assert_eq!(contours, 1);
        assert_eq!(bounds, [10, -40, 70, -10]);
        assert_eq!(&bytes[14..17], &[1, 0, 1]);
    }
    #[test]
    fn checksum_uses_padded_big_endian_words() {
        assert_eq!(sum(&[1, 2, 3]), 0x01020300);
    }
}
