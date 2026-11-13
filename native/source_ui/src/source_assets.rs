use external_cas_assets::PackageIndex;
use external_cas_fidelity::{
    GfxMovie,
    runtime_assets::{self, RuntimeFont},
    symbol_raster::AtlasPresentation,
};
use std::{collections::BTreeMap, io::Write, sync::atomic::AtomicBool};
use swf::{FontFlag, Point, PointDelta, ShapeRecord, StyleChangeData, Twips};
fn coord(v: i32, n: u16) -> Result<i32, String> {
    let scaled = i64::from(v) * 20480;
    if scaled % i64::from(n) != 0 {
        return Err("font coordinate cannot be represented exactly in DefineFont3 units".into());
    }
    i32::try_from(scaled / i64::from(n)).map_err(|_| "font scale overflow".into())
}
fn point(p: [i32; 2], n: u16) -> Result<Point<Twips>, String> {
    Ok(Point {
        x: Twips::new(coord(p[0], n)?),
        y: Twips::new(coord(p[1], n)?),
    })
}
fn delta(p: [i32; 2], n: u16) -> Result<PointDelta<Twips>, String> {
    Ok(PointDelta {
        dx: Twips::new(coord(p[0], n)?),
        dy: Twips::new(coord(p[1], n)?),
    })
}
fn font(f: &RuntimeFont, version: u8) -> Result<Vec<u8>, String> {
    let n = f.source.nominal_size;
    let mut glyphs = Vec::new();
    for g in &f.glyphs {
        let mut records = Vec::new();
        for c in &g.contours {
            records.push(ShapeRecord::StyleChange(Box::new(StyleChangeData {
                move_to: Some(point(c.move_to, n)?),
                fill_style_0: Some(1),
                fill_style_1: None,
                line_style: None,
                new_styles: None,
            })));
            for e in &c.edges {
                records.push(match e {
                    external_cas_fidelity::font::Edge::Line { delta: d } => {
                        ShapeRecord::StraightEdge {
                            delta: delta(*d, n)?,
                        }
                    }
                    external_cas_fidelity::font::Edge::Quadratic {
                        control_delta,
                        anchor_delta,
                    } => ShapeRecord::CurvedEdge {
                        control_delta: delta(*control_delta, n)?,
                        anchor_delta: delta(*anchor_delta, n)?,
                    },
                });
            }
        }
        let b = g.bounds;
        glyphs.push(swf::Glyph {
            shape_records: records,
            code: g.code,
            advance: u16::try_from(coord(i32::from(g.advance), n)?)
                .map_err(|_| "glyph advance extent")?,
            bounds: Some(swf::Rectangle {
                x_min: Twips::new(coord(b[0], n)?),
                y_min: Twips::new(coord(b[1], n)?),
                x_max: Twips::new(coord(b[2], n)?),
                y_max: Twips::new(coord(b[3], n)?),
            }),
        });
    }
    let mut flags = FontFlag::HAS_LAYOUT | FontFlag::HAS_WIDE_CODES;
    flags.set(FontFlag::IS_ITALIC, f.source.flags & 1 != 0);
    flags.set(FontFlag::IS_BOLD, f.source.flags & 2 != 0);
    let layout = swf::FontLayout {
        ascent: u16::try_from(coord(i32::from(f.source.ascent), n)?)
            .map_err(|_| "font ascent extent")?,
        descent: u16::try_from(coord(i32::from(f.source.descent), n)?)
            .map_err(|_| "font descent extent")?,
        leading: i16::try_from(coord(i32::from(f.source.leading), n)?)
            .map_err(|_| "font leading extent")?,
        kerning: f
            .kerning
            .iter()
            .map(|k| {
                Ok(swf::KerningRecord {
                    left_code: k.left,
                    right_code: k.right,
                    adjustment: Twips::new(coord(i32::from(k.advance), n)?),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    let tag = swf::Tag::DefineFont2(Box::new(swf::Font {
        version: 3,
        id: f.source.id,
        name: swf::SwfStr::from_utf8_str(&f.source.first_font_name),
        language: swf::Language::Unknown,
        layout: Some(layout),
        glyphs,
        flags,
    }));
    let mut out = Vec::new();
    swf::write::write_tag(&tag, version, &mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
fn tag(out: &mut Vec<u8>, code: u16, payload: &[u8]) {
    out.extend_from_slice(&((code << 6) | (payload.len().min(63) as u16)).to_le_bytes());
    if payload.len() >= 63 {
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(payload);
}
pub fn adapt(
    index: &PackageIndex,
    raw: &[u8],
    cancel: &AtomicBool,
) -> Result<
    (
        Vec<u8>,
        serde_json::Value,
        Vec<external_cas_fidelity::installed::SourcePin>,
    ),
    String,
> {
    let prepared = external_cas_fidelity::runtime::prepare_swf_movie(raw)?;
    let movie = GfxMovie::inspect(&prepared.runtime_bytes).map_err(|e| e.to_string())?;
    let assets = runtime_assets::load(
        index,
        &movie,
        AtlasPresentation::RedLuminanceGreenAlpha,
        cancel,
    )?;
    let mut replacements = BTreeMap::new();
    for f in &assets.fonts {
        replacements.insert(f.source.tag_index, font(f, movie.version)?);
    }
    for b in &assets.bitmaps {
        let mut argb = Vec::with_capacity(b.rgba.len());
        for p in b.rgba.chunks_exact(4) {
            let a = u16::from(p[3]);
            argb.extend_from_slice(&[
                p[3],
                ((u16::from(p[0]) * a + 127) / 255) as u8,
                ((u16::from(p[1]) * a + 127) / 255) as u8,
                ((u16::from(p[2]) * a + 127) / 255) as u8,
            ]);
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&argb).map_err(|e| e.to_string())?;
        let data = z.finish().map_err(|e| e.to_string())?;
        let t = swf::Tag::DefineBitsLossless(swf::DefineBitsLossless {
            version: 2,
            id: b.character_id,
            format: swf::BitmapFormat::Rgb32,
            width: b.dimensions[0] as u16,
            height: b.dimensions[1] as u16,
            data: std::borrow::Cow::Owned(data),
        });
        let mut out = Vec::new();
        swf::write::write_tag(&t, movie.version, &mut out).map_err(|e| e.to_string())?;
        replacements.insert(b.source.tag_index, out);
    }
    // The original first tag's source offset minus its variable header size is
    // derived from the prepared frame header, never guessed from tag bytes.
    let first = movie.tags.first().ok_or("empty movie")?;
    let header = first.payload.start - if first.payload.len() < 63 { 2 } else { 6 };
    let mut out = prepared.runtime_bytes[..header].to_vec();
    for (i, t) in movie.tags.iter().enumerate() {
        if let Some(r) = replacements.get(&i) {
            out.extend_from_slice(r);
        } else {
            tag(&mut out, t.code, movie.payload(t));
        }
    }
    out.extend_from_slice(&prepared.runtime_bytes[movie.trailing_bytes.clone()]);
    let len = u32::try_from(out.len()).map_err(|_| "adapted movie extent")?;
    out[4..8].copy_from_slice(&len.to_le_bytes());
    let report = serde_json::json!({"metadata_preparation":prepared.receipt,"source_fonts":assets.fonts.iter().map(|f|serde_json::json!({"header":f.source,"glyphs":f.glyphs.len(),"kerning":f.kerning.len()})).collect::<Vec<_>>(),"source_bitmaps":assets.bitmaps.iter().map(|b|serde_json::json!({"character_id":b.character_id,"source":b.source,"dimensions":b.dimensions,"atlas_pin":b.atlas_pin})).collect::<Vec<_>>(),"complete_scaleform_runtime":false});
    Ok((out, report, assets.atlas_pins))
}
