// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-owned GFx bitmap/font data for a host interpreter adapter. This does
//! not implement the Scaleform VM extensions or the game's service backend.
use crate::{
    CompactedFontHeader, GfxMovie, SubImage,
    font::{CompactedFont, GlyphOutline, KerningPair},
    installed::{InstalledAtlas, SourcePin, load_atlas},
    symbol_raster::AtlasPresentation,
};
use external_cas_assets::PackageIndex;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug)]
pub struct RuntimeFont {
    pub source: CompactedFontHeader,
    /// Exact original SymbolClass aliases; these are not system font names.
    pub aliases: Vec<String>,
    /// Original nominal units and Y-down curves. No rounding/rescaling occurs.
    pub glyphs: Vec<GlyphOutline>,
    pub kerning: Vec<KerningPair>,
}
#[derive(Debug)]
pub struct RuntimeBitmap {
    pub character_id: u16,
    pub source: SubImage,
    pub atlas_pin: SourcePin,
    pub dimensions: [u32; 2],
    /// Straight-alpha source pixels, with an explicitly chosen presentation.
    pub rgba: Vec<u8>,
}
#[derive(Debug, Default)]
pub struct RuntimeAssets {
    pub fonts: Vec<RuntimeFont>,
    pub bitmaps: Vec<RuntimeBitmap>,
    pub atlas_pins: Vec<SourcePin>,
}
const MAX_BITMAP_BYTES: usize = 128 * 1024 * 1024;
const MAX_FONT_EDGES: usize = 3_000_000;

/// Resolve only the explicitly selected package's actual atlas resources. All
/// images/fonts must qualify before publication; callers keep the original
/// movie and pins. Invoke off the UI thread, and retain that source movie.
pub fn load(
    index: &PackageIndex,
    movie: &GfxMovie<'_>,
    presentation: AtlasPresentation,
    cancel: &AtomicBool,
) -> Result<RuntimeAssets, String> {
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err("source UI asset load cancelled".to_string())
        } else {
            Ok(())
        }
    };
    let mut result = RuntimeAssets::default();
    let mut atlases = BTreeMap::new();
    for image in &movie.external_images {
        check()?;
        if atlases.contains_key(&image.id) {
            return Err("duplicate source external image ID".into());
        }
        let atlas = load_atlas(index, image)?;
        result.atlas_pins.push(atlas.pin.clone());
        atlases.insert(image.id, atlas);
    }
    let mut bytes = 0usize;
    let mut ids = std::collections::BTreeSet::new();
    for image in &movie.sub_images {
        check()?;
        if !ids.insert(image.character_id) {
            return Err("duplicate source sub-image character ID".into());
        }
        let atlas = atlases.get(&image.image_id).ok_or(
            "sub-image has no local source atlas; cross-movie atlas binding is unqualified",
        )?;
        let bitmap = crop(image, atlas, presentation)?;
        bytes = bytes
            .checked_add(bitmap.rgba.len())
            .ok_or("source UI bitmap byte overflow")?;
        if bytes > MAX_BITMAP_BYTES {
            return Err("source UI bitmap byte budget exceeded".into());
        }
        result.bitmaps.push(bitmap);
    }
    let mut edges = 0usize;
    for header in &movie.compacted_font_headers {
        check()?;
        let font = CompactedFont::open(movie, header).map_err(|e| e.to_string())?;
        if !font.unparsed.is_empty() {
            return Err(
                "additional compacted FontTypes/unknown suffix are preserved but not adapted"
                    .into(),
            );
        }
        if header.nominal_size == 0 {
            return Err("source font nominal size is zero".into());
        }
        let mut glyphs = Vec::with_capacity(font.glyphs.len());
        let mut codes = std::collections::BTreeSet::new();
        for i in 0..font.glyphs.len() {
            check()?;
            let glyph = font
                .glyph_at(i, MAX_FONT_EDGES - edges)
                .map_err(|e| e.to_string())?;
            if !codes.insert(glyph.code) {
                return Err("duplicate source glyph code".into());
            }
            edges += glyph.contours.iter().map(|c| c.edges.len()).sum::<usize>();
            if edges > MAX_FONT_EDGES {
                return Err("source UI font edge budget exceeded".into());
            }
            glyphs.push(glyph);
        }
        let aliases = movie
            .symbol_classes
            .iter()
            .filter(|s| s.id == header.id)
            .map(|s| s.name.clone())
            .collect();
        result.fonts.push(RuntimeFont {
            source: header.clone(),
            aliases,
            glyphs,
            kerning: font.kerning,
        });
    }
    check()?;
    Ok(result)
}

fn crop(
    source: &SubImage,
    atlas: &InstalledAtlas,
    presentation: AtlasPresentation,
) -> Result<RuntimeBitmap, String> {
    let [x0, y0, x1, y1] = source.pixel_rect.map(u32::from);
    let [aw, ah] = atlas.dimensions;
    if x0 >= x1 || y0 >= y1 || x1 > aw || y1 > ah {
        return Err("sub-image rectangle is outside its source atlas".into());
    }
    let expected = u64::from(aw) * u64::from(ah) * 4;
    if expected > MAX_BITMAP_BYTES as u64 || atlas.rgba.len() as u64 != expected {
        return Err("source atlas pixel extent/budget".into());
    }
    let bc5 = atlas.pixel_encoding == "bc5-unorm-rg";
    if bc5 && !matches!(presentation, AtlasPresentation::RedLuminanceGreenAlpha) {
        return Err("source BC5 UI atlas requires explicit channel presentation".into());
    }
    if !bc5 && atlas.pixel_encoding != "rgba8" {
        return Err("unqualified source atlas pixel encoding".into());
    }
    let width = x1 - x0;
    let height = y1 - y0;
    let size = u64::from(width) * u64::from(height) * 4;
    if size > MAX_BITMAP_BYTES as u64 {
        return Err("source sub-image bitmap budget".into());
    }
    let mut rgba = Vec::with_capacity(size as usize);
    for y in y0..y1 {
        let start = ((u64::from(y) * u64::from(aw) + u64::from(x0)) * 4) as usize;
        let end = start + width as usize * 4;
        if bc5 {
            for pixel in atlas.rgba[start..end].chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        } else {
            rgba.extend_from_slice(&atlas.rgba[start..end]);
        }
    }
    Ok(RuntimeBitmap {
        character_id: source.character_id,
        source: source.clone(),
        atlas_pin: atlas.pin.clone(),
        dimensions: [width, height],
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn atlas() -> InstalledAtlas {
        InstalledAtlas {
            pin: SourcePin {
                provider_path: "explicit-test.package".into(),
                key: external_cas_assets::ResourceKey::new(1, 2, 3),
                ordinal: 4,
                index_sha256: "test-index".into(),
                payload_sha256: "test-payload".into(),
            },
            resolved_name: "test".into(),
            dimensions: [2, 1],
            pixel_encoding: "bc5-unorm-rg".into(),
            rgba: vec![11, 22, 0, 255, 33, 44, 0, 255],
            source_bytes: vec![],
        }
    }
    #[test]
    fn exact_source_crop_requires_channel_policy_and_preserves_straight_values() {
        let a = atlas();
        let s = SubImage {
            character_id: 7,
            image_id: 9,
            pixel_rect: [1, 0, 2, 1],
            tag_index: 5,
        };
        assert!(crop(&s, &a, AtlasPresentation::LiteralRgba).is_err());
        let b = crop(&s, &a, AtlasPresentation::RedLuminanceGreenAlpha).unwrap();
        assert_eq!(b.rgba, [33, 33, 33, 44]);
        assert_eq!(b.character_id, 7);
        assert_eq!(b.atlas_pin, a.pin);
        assert_eq!(a.rgba, [11, 22, 0, 255, 33, 44, 0, 255]);
    }
    #[test]
    fn malformed_crop_extent_and_unknown_encoding_are_refused() {
        let mut a = atlas();
        let mut s = SubImage {
            character_id: 7,
            image_id: 9,
            pixel_rect: [0, 0, 3, 1],
            tag_index: 5,
        };
        assert!(crop(&s, &a, AtlasPresentation::RedLuminanceGreenAlpha).is_err());
        s.pixel_rect = [0, 0, 2, 1];
        a.rgba.pop();
        assert!(crop(&s, &a, AtlasPresentation::RedLuminanceGreenAlpha).is_err());
        a = atlas();
        a.pixel_encoding = "unknown".into();
        assert!(crop(&s, &a, AtlasPresentation::LiteralRgba).is_err());
    }
}
