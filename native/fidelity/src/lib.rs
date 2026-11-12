// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only native inspection of authorized, locally installed CAS UI assets.
//!
//! This is a metadata adapter, not an ActionScript runtime, font rasterizer,
//! display-list renderer or claim of visual parity. Every source byte is retained
//! by the returned movie; unsupported tag bodies remain available by range.
//! Format references and limits are documented in README.md.

use serde::Serialize;
use std::{fmt, ops::Range};
pub mod color_ui;
pub mod dds;
pub mod font;
pub mod installed;
pub mod runtime;
pub mod runtime_assets;
pub mod symbol_raster;
pub mod timeline;
pub mod truetype;
pub mod ui_layout;

const MAX_MOVIE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectError {
    Truncated { at: usize },
    Invalid { at: usize, message: &'static str },
    UnsupportedCompression,
    Budget { bytes: usize, limit: usize },
}
impl fmt::Display for InspectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { at } => write!(f, "truncated GFX metadata at byte {at}"),
            Self::Invalid { at, message } => {
                write!(f, "invalid GFX metadata at byte {at}: {message}")
            }
            Self::UnsupportedCompression => write!(
                f,
                "compressed GFX/SWF body is unsupported; provide a decoded movie"
            ),
            Self::Budget { bytes, limit } => {
                write!(f, "GFX movie has {bytes} bytes; limit is {limit}")
            }
        }
    }
}
impl std::error::Error for InspectError {}
type Result<T> = std::result::Result<T, InspectError>;

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub code: u16,
    pub payload: Range<usize>,
    /// Unparsed, preserved bytes after the inspected prefix, if any.
    pub unparsed: Range<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NamedCharacter {
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub movie: String,
    pub characters: Vec<NamedCharacter>,
    pub tag_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalImage {
    pub id: u16,
    /// Only present in DefineExternalImage2; its semantics are preserved raw.
    pub id_type: Option<u16>,
    pub bitmap_format: u16,
    pub target_size: [u16; 2],
    pub export_name: String,
    /// Short-form tag 1001 can omit the filename. Do not invent one.
    pub file_name: Option<String>,
    pub tag_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubImage {
    pub character_id: u16,
    pub image_id: u16,
    pub pixel_rect: [u16; 4],
    pub tag_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExporterInfo {
    pub version: u16,
    pub flags: Option<u32>,
    pub bitmap_format: u16,
    pub prefix: String,
    pub movie_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactedFontHeader {
    pub id: u16,
    /// First FontType in this tag only. Remaining FontTypes/outlines are raw.
    pub first_font_name: String,
    pub flags: u16,
    pub nominal_size: u16,
    pub ascent: i16,
    pub descent: i16,
    pub leading: i16,
    pub declared_glyph_count: u32,
    pub outline_bytes: Range<usize>,
    pub tag_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GfxMovie<'a> {
    #[serde(skip)]
    raw: &'a [u8],
    pub signature: String,
    pub version: u8,
    /// [xmin, xmax, ymin, ymax] in 1/20 pixel units, including signed origins.
    pub stage_twips: [i32; 4],
    pub frame_rate: f64,
    pub frame_count: u16,
    pub tags: Vec<Tag>,
    pub imports: Vec<Import>,
    pub exports: Vec<NamedCharacter>,
    pub symbol_classes: Vec<NamedCharacter>,
    pub external_images: Vec<ExternalImage>,
    pub sub_images: Vec<SubImage>,
    pub compacted_font_headers: Vec<CompactedFontHeader>,
    pub exporter_info: Vec<ExporterInfo>,
    pub trailing_bytes: Range<usize>,
    /// Explicitly true even if a particular movie contains no script tags.
    pub metadata_only: bool,
}

impl<'a> GfxMovie<'a> {
    pub fn inspect(raw: &'a [u8]) -> Result<Self> {
        if raw.len() > MAX_MOVIE_BYTES {
            return Err(InspectError::Budget {
                bytes: raw.len(),
                limit: MAX_MOVIE_BYTES,
            });
        }
        let mut r = Reader::new(raw, 0);
        let signature = r.take(3)?;
        if matches!(signature, b"CFX" | b"CWS" | b"ZWS") {
            return Err(InspectError::UnsupportedCompression);
        }
        if !matches!(signature, b"GFX" | b"FWS") {
            return Err(r.invalid("expected GFX or FWS signature"));
        }
        let version = r.u8()?;
        let declared = r.u32()? as usize;
        if declared != raw.len() {
            return Err(r.invalid("declared movie size differs from payload"));
        }
        let rect_start = r.at;
        let mut bits = Bits {
            raw,
            at: rect_start * 8,
        };
        let width = bits.unsigned(5)? as u8;
        if width == 0 || width > 31 {
            return Err(r.invalid("invalid RECT bit width"));
        }
        let mut stage_twips = [0; 4];
        for component in &mut stage_twips {
            *component = bits.signed(width)?;
        }
        r.at = bits.at.div_ceil(8);
        if stage_twips[1] < stage_twips[0] || stage_twips[3] < stage_twips[2] {
            return Err(r.invalid("negative stage extent"));
        }
        let frame_rate = f64::from(r.u16()?) / 256.;
        let frame_count = r.u16()?;
        let mut movie = Self {
            raw,
            signature: String::from_utf8(signature.to_vec()).unwrap(),
            version,
            stage_twips,
            frame_rate,
            frame_count,
            tags: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            symbol_classes: Vec::new(),
            external_images: Vec::new(),
            sub_images: Vec::new(),
            compacted_font_headers: Vec::new(),
            exporter_info: Vec::new(),
            trailing_bytes: raw.len()..raw.len(),
            metadata_only: true,
        };
        loop {
            let head = r.u16()?;
            let code = head >> 6;
            let short_len = (head & 63) as usize;
            let len = if short_len == 63 {
                r.u32()? as usize
            } else {
                short_len
            };
            let start = r.absolute();
            let bytes = r.take(len)?;
            let payload = start..start + len;
            let tag_index = movie.tags.len();
            let mut body = Reader::new(bytes, start);
            match code {
                0 => {
                    if len != 0 {
                        return Err(body.invalid("End tag has a body"));
                    }
                }
                56 => movie.exports.extend(body.named_characters()?),
                76 => movie.symbol_classes.extend(body.named_characters()?),
                57 | 71 => {
                    let url = body.c_string()?;
                    if code == 71 {
                        if body.take(2)? != [1, 0] {
                            return Err(body.invalid("unsupported ImportAssets2 reserved values"));
                        }
                    }
                    movie.imports.push(Import {
                        movie: url,
                        characters: body.named_characters()?,
                        tag_index,
                    });
                }
                1001 | 1009 => {
                    let id = body.u16()?;
                    let id_type = if code == 1009 {
                        Some(body.u16()?)
                    } else {
                        None
                    };
                    let bitmap_format = body.u16()?;
                    let target_size = [body.u16()?, body.u16()?];
                    let export_name = body.net_string()?;
                    let file_name = if code == 1009 || body.remaining() > 0 {
                        Some(body.net_string()?)
                    } else {
                        None
                    };
                    movie.external_images.push(ExternalImage {
                        id,
                        id_type,
                        bitmap_format,
                        target_size,
                        export_name,
                        file_name,
                        tag_index,
                    });
                }
                1008 => {
                    movie.sub_images.push(SubImage {
                        character_id: body.u16()?,
                        image_id: body.u16()?,
                        pixel_rect: [body.u16()?, body.u16()?, body.u16()?, body.u16()?],
                        tag_index,
                    });
                }
                1005 => {
                    let id = body.u16()?;
                    let first_font_name = body.c_string()?;
                    let flags = body.u16()?;
                    let nominal_size = body.u16()?;
                    let ascent = body.u16()? as i16;
                    let descent = body.u16()? as i16;
                    let leading = body.u16()? as i16;
                    let declared_glyph_count = body.u32()?;
                    let outline_len = body.u32()? as usize;
                    let start = body.absolute();
                    body.take(outline_len)?;
                    movie.compacted_font_headers.push(CompactedFontHeader {
                        id,
                        first_font_name,
                        flags,
                        nominal_size,
                        ascent,
                        descent,
                        leading,
                        declared_glyph_count,
                        outline_bytes: start..start + outline_len,
                        tag_index,
                    });
                }
                1000 => {
                    let version = body.u16()?;
                    let flags = if version >= 266 {
                        Some(body.u32()?)
                    } else {
                        None
                    };
                    let bitmap_format = body.u16()?;
                    let prefix = body.net_string()?;
                    let movie_name = body.net_string()?;
                    movie.exporter_info.push(ExporterInfo {
                        version,
                        flags,
                        bitmap_format,
                        prefix,
                        movie_name,
                    });
                }
                _ => {}
            }
            movie.tags.push(Tag {
                code,
                payload: payload.clone(),
                unparsed: body.absolute()..payload.end,
            });
            if code == 0 {
                movie.trailing_bytes = r.absolute()..raw.len();
                break;
            }
        }
        Ok(movie)
    }

    pub fn raw(&self) -> &'a [u8] {
        self.raw
    }
    pub fn payload(&self, tag: &Tag) -> &'a [u8] {
        &self.raw[tag.payload.clone()]
    }
    pub fn stage_pixels(&self) -> [f64; 2] {
        [
            (f64::from(self.stage_twips[1]) - f64::from(self.stage_twips[0])) / 20.,
            (f64::from(self.stage_twips[3]) - f64::from(self.stage_twips[2])) / 20.,
        ]
    }
}

/// Sims resource-name candidate from the actual library import names. Resolution
/// still requires matching the full TGI, provider ordinal and selected bytes.
pub fn fnv1_resource_name(name: &str) -> u64 {
    name.to_ascii_lowercase()
        .bytes()
        .fold(0xcbf29ce484222325, |hash, byte| {
            hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
        })
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    base: usize,
}
impl<'a> Reader<'a> {
    fn new(data: &'a [u8], base: usize) -> Self {
        Self { data, at: 0, base }
    }
    fn absolute(&self) -> usize {
        self.base + self.at
    }
    fn invalid(&self, message: &'static str) -> InspectError {
        InspectError::Invalid {
            at: self.absolute(),
            message,
        }
    }
    fn remaining(&self) -> usize {
        self.data.len() - self.at
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.data.len())
            .ok_or(InspectError::Truncated {
                at: self.absolute(),
            })?;
        let result = &self.data[self.at..end];
        self.at = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn utf8(&self, bytes: &[u8]) -> Result<String> {
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.invalid("non-UTF8 name; source bytes retained by caller"))
    }
    fn c_string(&mut self) -> Result<String> {
        let count = self.data[self.at..]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(InspectError::Truncated {
                at: self.absolute(),
            })?;
        let bytes = self.take(count)?;
        self.take(1)?;
        self.utf8(bytes)
    }
    fn net_string(&mut self) -> Result<String> {
        let count = self.u8()? as usize;
        let bytes = self.take(count)?;
        self.utf8(bytes)
    }
    fn named_characters(&mut self) -> Result<Vec<NamedCharacter>> {
        let count = self.u16()? as usize;
        if count > self.remaining() / 3 {
            return Err(self.invalid("character count exceeds body"));
        }
        (0..count)
            .map(|_| {
                Ok(NamedCharacter {
                    id: self.u16()?,
                    name: self.c_string()?,
                })
            })
            .collect()
    }
}
struct Bits<'a> {
    raw: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn unsigned(&mut self, width: u8) -> Result<u32> {
        let mut result = 0;
        for _ in 0..width {
            let byte = self
                .raw
                .get(self.at / 8)
                .ok_or(InspectError::Truncated { at: self.at / 8 })?;
            result = (result << 1) | u32::from((byte >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(result)
    }
    fn signed(&mut self, width: u8) -> Result<i32> {
        Ok(((self.unsigned(width)? << (32 - width)) as i32) >> (32 - width))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag(out: &mut Vec<u8>, code: u16, body: &[u8]) {
        out.extend_from_slice(&((code << 6) | 63).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
    }
    fn movie(tags: &[u8]) -> Vec<u8> {
        let mut out = b"GFX\x0f\0\0\0\0".to_vec();
        // RECT width=1, four zero components, padded to byte boundary.
        out.extend_from_slice(&[8, 0, 0, 30, 1, 0]);
        out.extend_from_slice(tags);
        out.extend_from_slice(&[0, 0]);
        let len = out.len() as u32;
        out[4..8].copy_from_slice(&len.to_le_bytes());
        out
    }
    #[test]
    fn imports_fonts_atlas_and_unknown_ranges_are_preserved() {
        let mut tags = Vec::new();
        tag(
            &mut tags,
            71,
            b"gfxfontlib.swf\0\x01\0\x01\0\x01\0HeadlineFont\0",
        );
        tag(
            &mut tags,
            1001,
            b"\x09\0\x0e\0\0\x04\0\x02\x05atlas\x09atlas.dds",
        );
        let mut font = b"\x03\0UISans\0".to_vec();
        for value in [0u16, 1024, 900, 200, 0] {
            font.extend_from_slice(&value.to_le_bytes());
        }
        font.extend_from_slice(&2u32.to_le_bytes());
        font.extend_from_slice(&3u32.to_le_bytes());
        font.extend_from_slice(&[1, 2, 3, 9, 8, 7]);
        tag(&mut tags, 1005, &font);
        tag(&mut tags, 999, &[8, 9, 10]);
        let bytes = movie(&tags);
        let m = GfxMovie::inspect(&bytes).unwrap();
        assert_eq!(m.raw(), bytes);
        assert_eq!(m.frame_rate, 30.);
        assert_eq!(m.imports[0].characters[0].name, "HeadlineFont");
        assert_eq!(m.external_images[0].file_name.as_deref(), Some("atlas.dds"));
        assert_eq!(m.external_images[0].target_size, [1024, 512]);
        assert_eq!(m.compacted_font_headers[0].declared_glyph_count, 2);
        assert_eq!(&bytes[m.tags[2].unparsed.clone()], &[9, 8, 7]);
        assert_eq!(m.payload(&m.tags[3]), &[8, 9, 10]);
        assert!(m.metadata_only);
    }
    #[test]
    fn every_truncation_fails_without_panicking() {
        let mut tags = Vec::new();
        tag(
            &mut tags,
            1009,
            b"\x01\0\0\0\x0e\0\0\x04\0\x04\x01a\x05a.dds",
        );
        let bytes = movie(&tags);
        for cut in 0..bytes.len() {
            let mut truncated = bytes[..cut].to_vec();
            if cut >= 8 {
                truncated[4..8].copy_from_slice(&(cut as u32).to_le_bytes());
            }
            assert!(GfxMovie::inspect(&truncated).is_err(), "cut={cut}");
        }
    }
    #[test]
    fn refuses_bad_lengths_counts_and_internal_compression() {
        assert_eq!(
            GfxMovie::inspect(b"CFX").unwrap_err(),
            InspectError::UnsupportedCompression
        );
        let mut tags = Vec::new();
        tag(&mut tags, 56, &[255, 255]);
        assert!(GfxMovie::inspect(&movie(&tags)).is_err());
        let mut bytes = movie(&[]);
        bytes[4] = 0;
        assert!(GfxMovie::inspect(&bytes).is_err());
        let mut tags = Vec::new();
        tag(&mut tags, 1005, b"\x01\0name\0");
        assert!(GfxMovie::inspect(&movie(&tags)).is_err());
    }
    #[test]
    fn preserves_trailing_bytes_and_long_unknown_tag() {
        let mut tags = Vec::new();
        tag(&mut tags, 999, &[42; 100]);
        let mut bytes = movie(&tags);
        bytes.extend_from_slice(&[1, 2, 3]);
        let len = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&len.to_le_bytes());
        let m = GfxMovie::inspect(&bytes).unwrap();
        assert_eq!(m.payload(&m.tags[0]), &[42; 100]);
        assert_eq!(&bytes[m.trailing_bytes], &[1, 2, 3]);
    }
    #[test]
    fn known_bare_name_matches_case_and_extension_boundary() {
        assert_eq!(fnv1_resource_name("gfxfontlib"), 0x5c7cb2846f0fca7a);
        assert_eq!(
            fnv1_resource_name("CASLibrary"),
            fnv1_resource_name("caslibrary")
        );
        assert_ne!(
            fnv1_resource_name("caslibrary.swf"),
            fnv1_resource_name("caslibrary")
        );
    }
}
