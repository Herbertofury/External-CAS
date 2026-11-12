// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Scaleform compacted-font outline reader, based on the GPL FFDec format
//! reader (see README). This reuses installed font curves; it does not substitute
//! a system font or execute game code. Outline coordinates remain source Y-down.
use crate::{CompactedFontHeader, GfxMovie, InspectError, Reader, Result};
use serde::Serialize;
use std::ops::Range;

#[derive(Debug, Clone, Serialize)]
pub struct GlyphInfo {
    pub code: u16,
    pub advance: i16,
    pub source_offset: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct KerningPair {
    pub left: u16,
    pub right: u16,
    pub advance: i16,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Edge {
    Line {
        delta: [i32; 2],
    },
    Quadratic {
        control_delta: [i32; 2],
        anchor_delta: [i32; 2],
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct Contour {
    pub move_to: [i32; 2],
    pub edges: Vec<Edge>,
}
#[derive(Debug, Clone, Serialize)]
pub struct GlyphOutline {
    pub code: u16,
    pub advance: i16,
    /// Source order xmin, ymin, xmax, ymax, in nominal font units (Y-down).
    pub bounds: [i32; 4],
    pub contours: Vec<Contour>,
}

pub struct CompactedFont<'a> {
    pub header: &'a CompactedFontHeader,
    pub glyphs: Vec<GlyphInfo>,
    pub kerning: Vec<KerningPair>,
    raw: &'a [u8],
    font_base: usize,
    outline_range: Range<usize>,
    /// Extra FontTypes/unknown trailing bytes remain preserved in the movie.
    pub unparsed: Range<usize>,
}
impl<'a> CompactedFont<'a> {
    pub fn open(movie: &'a GfxMovie<'a>, header: &'a CompactedFontHeader) -> Result<Self> {
        let tag = movie
            .tags
            .get(header.tag_index)
            .ok_or(InspectError::Invalid {
                at: 0,
                message: "font tag index",
            })?;
        if tag.code != 1005
            || header.outline_bytes.start < tag.payload.start
            || header.outline_bytes.end > tag.payload.end
        {
            return Err(InspectError::Invalid {
                at: tag.payload.start,
                message: "font header range",
            });
        }
        let raw = movie.raw();
        let font_base = tag.payload.start + 2;
        let mut r = Reader::new(
            &raw[header.outline_bytes.end..tag.payload.end],
            header.outline_bytes.end,
        );
        let count = header.declared_glyph_count as usize;
        if count > r.remaining() / 8 {
            return Err(r.invalid("font glyph count exceeds tag"));
        }
        let mut glyphs = Vec::with_capacity(count);
        for _ in 0..count {
            let code = r.u16()?;
            let advance = r.u16()? as i16;
            let offset = (r.u32()? as usize)
                .checked_add(font_base)
                .ok_or_else(|| r.invalid("glyph offset overflow"))?;
            if !header.outline_bytes.contains(&offset) {
                return Err(r.invalid("glyph offset outside outline bytes"));
            }
            glyphs.push(GlyphInfo {
                code,
                advance,
                source_offset: offset,
            });
        }
        let kern_count = ui30(&mut r)? as usize;
        if kern_count > r.remaining() / 6 {
            return Err(r.invalid("kerning count exceeds tag"));
        }
        let mut kerning = Vec::with_capacity(kern_count);
        for _ in 0..kern_count {
            kerning.push(KerningPair {
                left: r.u16()?,
                right: r.u16()?,
                advance: r.u16()? as i16,
            });
        }
        Ok(Self {
            header,
            glyphs,
            kerning,
            raw,
            font_base,
            outline_range: header.outline_bytes.clone(),
            unparsed: r.absolute()..tag.payload.end,
        })
    }
    /// Decode a requested character only. The caller sets a total edge budget;
    /// outline/contour references remain bounded to this FontType's source bytes.
    pub fn glyph(&self, code: u16, max_edges: usize) -> Result<Option<GlyphOutline>> {
        let matching: Vec<_> = self.glyphs.iter().filter(|g| g.code == code).collect();
        if matching.is_empty() {
            return Ok(None);
        }
        if matching.len() != 1 {
            return Err(InspectError::Invalid {
                at: self.font_base,
                message: "duplicate font character code",
            });
        }
        self.glyph_info(matching[0], max_edges).map(Some)
    }
    pub fn glyph_at(&self, index: usize, max_edges: usize) -> Result<GlyphOutline> {
        let info = self.glyphs.get(index).ok_or(InspectError::Invalid {
            at: self.font_base,
            message: "glyph index",
        })?;
        self.glyph_info(info, max_edges)
    }
    fn glyph_info(&self, info: &GlyphInfo, max_edges: usize) -> Result<GlyphOutline> {
        let mut r = Reader::new(
            &self.raw[info.source_offset..self.outline_range.end],
            info.source_offset,
        );
        let bounds = [si15(&mut r)?, si15(&mut r)?, si15(&mut r)?, si15(&mut r)?];
        let count = ui15(&mut r)? as usize;
        if count > r.remaining() / 3 {
            return Err(r.invalid("contour count exceeds glyph extent"));
        }
        let mut contours = Vec::with_capacity(count);
        let mut used_edges: usize = 0;
        for _ in 0..count {
            let move_to = [si15(&mut r)?, si15(&mut r)?];
            let encoded = ui30(&mut r)?;
            let mut referenced;
            let edge_reader = if encoded & 1 != 0 {
                let offset = self
                    .font_base
                    .checked_add((encoded >> 1) as usize)
                    .ok_or_else(|| r.invalid("contour reference overflow"))?;
                if !self.outline_range.contains(&offset) {
                    return Err(r.invalid("contour reference outside font outlines"));
                }
                referenced = Reader::new(&self.raw[offset..self.outline_range.end], offset);
                let count = ui30(&mut referenced)?;
                if count & 1 != 0 {
                    return Err(referenced.invalid("contour reference does not name direct edges"));
                }
                (count >> 1, &mut referenced)
            } else {
                (encoded >> 1, &mut r)
            };
            let count = edge_reader.0 as usize;
            let r_edges = edge_reader.1;
            used_edges = used_edges
                .checked_add(count)
                .ok_or_else(|| r_edges.invalid("edge count overflow"))?;
            if used_edges > max_edges {
                return Err(InspectError::Budget {
                    bytes: used_edges,
                    limit: max_edges,
                });
            }
            if count > r_edges.remaining() / 2 {
                return Err(r_edges.invalid("edge count exceeds outline extent"));
            }
            let edges = (0..count)
                .map(|_| read_edge(r_edges))
                .collect::<Result<Vec<_>>>()?;
            contours.push(Contour { move_to, edges });
        }
        Ok(GlyphOutline {
            code: info.code,
            advance: info.advance,
            bounds,
            contours,
        })
    }
}

fn ui15(r: &mut Reader<'_>) -> Result<u16> {
    let a = r.u8()?;
    Ok(if a & 1 == 0 {
        u16::from(a >> 1)
    } else {
        u16::from(a >> 1) | (u16::from(r.u8()?) << 7)
    })
}
fn si15(r: &mut Reader<'_>) -> Result<i32> {
    let a = r.u8()?;
    Ok(if a & 1 == 0 {
        i32::from(a as i8) >> 1
    } else {
        i32::from(a >> 1) | (i32::from(r.u8()? as i8) << 7)
    })
}
fn ui30(r: &mut Reader<'_>) -> Result<u32> {
    let a = r.u8()?;
    let mut v = u32::from(a >> 2);
    for i in 0..a & 3 {
        v |= u32::from(r.u8()?) << (6 + 8 * i);
    }
    Ok(v)
}

fn read_edge(r: &mut Reader<'_>) -> Result<Edge> {
    let first = r.u8()?;
    let code = first & 15;
    // Four opcode bits then contiguous little-endian signed components.
    let (width, count) = match code {
        0 | 2 => (12, 1),
        1 | 3 => (20, 1),
        4..=7 => (6 + 4 * (code - 4), 2),
        8..=15 => (5 + 2 * (code - 8), 4),
        _ => unreachable!(),
    };
    let bytes = (4 + usize::from(width) * count).div_ceil(8);
    let mut packed = vec![first];
    packed.extend_from_slice(r.take(bytes - 1)?);
    let mut at = 4;
    let mut values = [0; 4];
    for value in values.iter_mut().take(count) {
        let mut bits = 0u32;
        for i in 0..width {
            let bit = (packed[at / 8] >> (at % 8)) & 1;
            bits |= u32::from(bit) << i;
            at += 1;
        }
        *value = ((bits << (32 - width)) as i32) >> (32 - width);
    }
    Ok(match code {
        0 | 1 => Edge::Line {
            delta: [values[0], 0],
        },
        2 | 3 => Edge::Line {
            delta: [0, values[0]],
        },
        4..=7 => Edge::Line {
            delta: [values[0], values[1]],
        },
        _ => Edge::Quadratic {
            control_delta: [values[0], values[1]],
            anchor_delta: [values[2], values[3]],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edge_bytes(code: u8, width: u8, values: &[i32]) -> Vec<u8> {
        let mut out = vec![0u8; (4 + usize::from(width) * values.len()).div_ceil(8)];
        out[0] = code;
        let mut at = 4;
        for value in values {
            for i in 0..width {
                out[at / 8] |= (((value >> i) & 1) as u8) << (at % 8);
                at += 1;
            }
        }
        out
    }
    #[test]
    fn all_edge_encodings_sign_extend_and_preserve_curves() {
        for code in 0..16 {
            let (width, count) = match code {
                0 | 2 => (12, 1),
                1 | 3 => (20, 1),
                4..=7 => (6 + 4 * (code - 4), 2),
                _ => (5 + 2 * (code - 8), 4),
            };
            let source = [-(1 << (width - 1)), (1 << (width - 1)) - 1, -1, 1];
            let bytes = edge_bytes(code, width, &source[..count]);
            let mut r = Reader::new(&bytes, 0);
            let edge = read_edge(&mut r).unwrap();
            assert_eq!(r.remaining(), 0);
            let expected = match code {
                0 | 1 => Edge::Line {
                    delta: [source[0], 0],
                },
                2 | 3 => Edge::Line {
                    delta: [0, source[0]],
                },
                4..=7 => Edge::Line {
                    delta: [source[0], source[1]],
                },
                _ => Edge::Quadratic {
                    control_delta: [source[0], source[1]],
                    anchor_delta: [source[2], source[3]],
                },
            };
            assert_eq!(edge, expected, "code={code}");
            for cut in 0..bytes.len() {
                assert!(read_edge(&mut Reader::new(&bytes[..cut], 0)).is_err());
            }
        }
    }
    #[test]
    fn packed_signed_and_unsigned_counts() {
        for (bytes, expected) in [
            (vec![0xfe], -1),
            (vec![0x80], -64),
            (vec![0x7e], 63),
            (vec![0x01, 0x80], -16384),
            (vec![0xff, 0x7f], 16383),
        ] {
            assert_eq!(si15(&mut Reader::new(&bytes, 0)).unwrap(), expected);
        }
        assert_eq!(ui15(&mut Reader::new(&[255, 255], 0)).unwrap(), 32767);
        assert_eq!(
            ui30(&mut Reader::new(&[255, 255, 255, 255], 0)).unwrap(),
            (1 << 30) - 1
        );
    }
}
