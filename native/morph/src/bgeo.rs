// SPDX-License-Identifier: GPL-3.0-or-later
// C. Marinetti et al., SimRipper BGEO.cs/PreviewControl.LoadBGEOMorph.
use super::bytes::*;
use super::*;
use external_cas_assets::{Geometry, ResourceKey};
#[derive(Clone, Debug)]
pub struct BgeoLod {
    pub index_base: u32,
    pub vertices: u32,
    pub vectors: u32,
}
#[derive(Clone, Debug)]
pub struct BgeoBlend {
    pub position: bool,
    pub normal: bool,
    pub offset: i16,
    pub index: usize,
}
#[derive(Clone, Debug)]
pub struct Bgeo {
    pub version: u32,
    pub public_keys: Vec<ResourceKey>,
    pub lods: Vec<BgeoLod>,
    pub blends: Vec<BgeoBlend>,
    pub vectors: Vec<[f32; 3]>,
    pub raw: Vec<u8>,
    pub unparsed_suffix: Vec<u8>,
}
impl Bgeo {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let mut r = Reader::new(raw);
        let header = rcol(&mut r)?;
        if r.take(4)? != b"BGEO" {
            return Err(MorphError::Invalid("BGEO magic".into()));
        }
        let version = r.u32()?;
        if version != 0x600 {
            return Err(MorphError::Unsupported(format!("BGEO version {version}")));
        }
        let lod_count = r.count(12)?;
        let n = r.u32()? as usize;
        let vector_count = r.u32()? as usize;
        if !(1..=4).contains(&lod_count) {
            return Err(MorphError::Unsupported("BGEO LOD count".into()));
        }
        let mut lods = Vec::new();
        for _ in 0..lod_count {
            lods.push(BgeoLod {
                index_base: r.u32()?,
                vertices: r.u32()?,
                vectors: r.u32()?,
            });
        }
        if lods.iter().map(|l| l.vertices as usize).sum::<usize>() != n
            || n > r.raw.len().saturating_sub(r.at) / 2
        {
            return Err(MorphError::Invalid("BGEO vertex extent".into()));
        }
        let mut blends = Vec::new();
        let mut running = 0i64;
        let mut lod = 0;
        let mut previous = 0usize;
        for i in 0..n {
            if i == previous + lods[lod].vertices as usize {
                running += lods[lod].vectors as i64;
                previous += lods[lod].vertices as usize;
                lod += 1;
                if lod >= lods.len() {
                    return Err(MorphError::Invalid("BGEO LOD transition".into()));
                }
            }
            let raw = r.u16()? as i16;
            let offset = raw >> 2;
            let index = running + offset as i64;
            let position = raw & 1 != 0;
            let normal = raw & 2 != 0;
            if (position || normal)
                && (index < 0 || index as usize + usize::from(position && normal) >= vector_count)
            {
                return Err(MorphError::Invalid("BGEO delta vector index".into()));
            }
            blends.push(BgeoBlend {
                position,
                normal,
                offset,
                index: index.max(0) as usize,
            });
            running += offset as i64;
        }
        if vector_count > r.raw.len().saturating_sub(r.at) / 6 {
            return Err(MorphError::Truncated);
        }
        let mut vectors = Vec::new();
        for _ in 0..vector_count {
            let mut v = [0f32; 3];
            for c in &mut v {
                *c = ((r.u16()? ^ 0x8000) as i16) as f32 / 8000f32;
            }
            vectors.push(v);
        }
        Ok(Self {
            version,
            public_keys: header.public,
            lods,
            blends,
            vectors,
            unparsed_suffix: raw[r.at..].to_vec(),
            raw: raw.to_vec(),
        })
    }
}
pub fn apply_bgeo(
    geometry: &Geometry,
    morph: &Bgeo,
    lod: usize,
    weight: f32,
    copy_face_morphs: bool,
) -> Result<AppliedMorph> {
    if !weight.is_finite() {
        return Err(MorphError::Invalid("Non-finite BGEO weight".into()));
    }
    let mut result = AppliedMorph {
        geometry: geometry.clone(),
        stats: ApplyStats {
            vertices: geometry.vertices.len(),
            ..Default::default()
        },
    };
    if weight == 0.0 {
        return Ok(result);
    }
    if !morph.unparsed_suffix.is_empty() {
        return Err(MorphError::Unsupported(
            "BGEO trailing data retained but unqualified".into(),
        ));
    }
    let level = morph
        .lods
        .get(lod)
        .ok_or_else(|| MorphError::Invalid("BGEO LOD absent".into()))?;
    let first = morph.lods[..lod]
        .iter()
        .map(|l| l.vertices as usize)
        .sum::<usize>();
    let end = level
        .index_base
        .checked_add(level.vertices)
        .ok_or_else(|| MorphError::Invalid("BGEO ID overflow".into()))?;
    for v in &mut result.geometry.vertices {
        let id = v.vertex_id.ok_or_else(|| {
            MorphError::MissingMetadata("BGEO requires native GEOM vertex IDs".into())
        })?;
        if id < 0 || (id as u32) < level.index_base || (id as u32) >= end {
            continue;
        }
        result.stats.matched_vertices += 1;
        let factor = if copy_face_morphs {
            ((v.tag.ok_or_else(|| {
                MorphError::MissingMetadata("Copied face morph requires native tag".into())
            })? & 0x003f0000)
                >> 16) as f32
                / 63.0
        } else {
            1.0
        };
        let blend = &morph.blends[first + (id as u32 - level.index_base) as usize];
        let before_position = v.position;
        let before_normal = v.normal;
        if blend.position {
            for i in 0..3 {
                v.position[i] += morph.vectors[blend.index][i] * weight * factor;
            }
        }
        if blend.normal {
            for i in 0..3 {
                v.normal[i] +=
                    morph.vectors[blend.index + usize::from(blend.position)][i] * weight * factor;
            }
        }
        result.stats.changed_positions += usize::from(v.position != before_position);
        result.stats.changed_normals += usize::from(v.normal != before_normal);
        if v.position
            .iter()
            .chain(v.normal.iter())
            .any(|f| !f.is_finite())
        {
            return Err(MorphError::Invalid("Non-finite BGEO result".into()));
        }
    }
    Ok(result)
}
