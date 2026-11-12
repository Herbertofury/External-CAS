// SPDX-License-Identifier: GPL-3.0-or-later
// SimRipper DMAP.cs and PreviewControl.LoadDMapMorph, original float semantics.
use super::bytes::*;
use super::*;
use external_cas_assets::Geometry;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct Dmap {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub age_gender: u32,
    pub species: Option<u32>,
    pub physique: u8,
    pub kind: u8,
    pub min_col: u32,
    pub max_col: u32,
    pub min_row: u32,
    pub max_row: u32,
    pub robe_channel: u8,
    pub skin_min: f32,
    pub skin_delta: f32,
    pub robe_min: f32,
    pub robe_delta: f32,
    pub skin: Vec<[f32; 3]>,
    pub robe: Vec<[f32; 3]>,
    pub raw: Vec<u8>,
    pub has_data: bool,
}
impl Dmap {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        Self::parse_with_pixel_budget(raw, 4_194_304)
    }
    pub fn parse_with_pixel_budget(raw: &[u8], pixel_budget: usize) -> Result<Self> {
        let mut r = Reader::new(raw);
        let version = r.u32()?;
        if ![5, 6, 7].contains(&version) {
            return Err(MorphError::Unsupported(format!("DMAP version {version}")));
        }
        let doubled = r.u32()?;
        let height = r.u32()?;
        let age_gender = r.u32()?;
        let species = if version > 5 { Some(r.u32()?) } else { None };
        let physique = r.u8()?;
        let kind = r.u8()?;
        if doubled == 0 || doubled % 2 != 0 || height == 0 || kind > 1 {
            return Err(MorphError::Invalid("DMAP dimensions or kind".into()));
        }
        let width = doubled / 2;
        let min_col = r.u32()?;
        let max_col = r.u32()?;
        let min_row = r.u32()?;
        let max_row = r.u32()?;
        let robe_channel = r.u8()?;
        if robe_channel > 2 {
            return Err(MorphError::Unsupported("DMAP robe channel".into()));
        }
        let (mut skin_min, mut skin_delta) = if kind == 0 { (-0.2, 0.4) } else { (-0.75, 1.5) };
        let (mut robe_min, mut robe_delta) = (skin_min, skin_delta);
        if version > 6 {
            skin_min = r.f32()?;
            skin_delta = r.f32()?;
            if robe_channel == 0 {
                robe_min = r.f32()?;
                robe_delta = r.f32()?;
            } else {
                robe_min = skin_min;
                robe_delta = skin_delta;
            }
        }
        let total = r.u32()? as usize;
        if total != raw.len() - r.at {
            return Err(MorphError::Invalid("DMAP scanline byte extent".into()));
        }
        let mut out = Self {
            version,
            width,
            height,
            age_gender,
            species,
            physique,
            kind,
            min_col,
            max_col,
            min_row,
            max_row,
            robe_channel,
            skin_min,
            skin_delta,
            robe_min,
            robe_delta,
            skin: Vec::new(),
            robe: Vec::new(),
            raw: raw.to_vec(),
            has_data: total > 0 && max_col != 0,
        };
        if total == 0 {
            if max_col != 0 {
                return Err(MorphError::Invalid(
                    "Empty DMAP declares a nonempty crop".into(),
                ));
            }
            return Ok(out);
        }
        if min_col > max_col || max_col >= width || min_row > max_row || max_row >= height {
            return Err(MorphError::Invalid("DMAP crop outside dimensions".into()));
        }
        let w = (max_col - min_col + 1) as usize;
        let h = (max_row - min_row + 1) as usize;
        let pixels = w
            .checked_mul(h)
            .filter(|&n| n <= pixel_budget)
            .ok_or_else(|| MorphError::Unsupported("DMAP decoded pixel budget".into()))?;
        let mut skin = vec![[128u8; 3]; pixels];
        let mut robe = vec![[128u8; 3]; pixels];
        for y in 0..h {
            let start = r.at;
            let size = r.u16()? as usize;
            let mode = r.u8()?;
            let end = start
                .checked_add(size)
                .filter(|&e| e <= raw.len())
                .ok_or(MorphError::Truncated)?;
            if size < 3 {
                return Err(MorphError::Invalid("DMAP row size".into()));
            }
            if mode == 2 {
                if size != 3 {
                    return Err(MorphError::Invalid("DMAP empty row extent".into()));
                }
                continue;
            }
            if mode > 1 {
                return Err(MorphError::Unsupported("DMAP scan compression".into()));
            }
            let channel = r.u8()?;
            if channel > 2 {
                return Err(MorphError::Unsupported("DMAP scan robe channel".into()));
            }
            let stride = if channel == 0 { 6 } else { 3 };
            let write =
                |x: usize, value: &[u8], skin: &mut Vec<[u8; 3]>, robe: &mut Vec<[u8; 3]>| {
                    skin[y * w + x].copy_from_slice(&value[..3]);
                    match channel {
                        0 => robe[y * w + x].copy_from_slice(&value[3..6]),
                        1 => {}
                        2 => robe[y * w + x].copy_from_slice(&value[..3]),
                        _ => unreachable!(),
                    }
                };
            if mode == 0 {
                if size != 4 + stride * w {
                    return Err(MorphError::Invalid("DMAP literal row extent".into()));
                }
                for x in 0..w {
                    write(x, r.take(stride)?, &mut skin, &mut robe);
                }
            } else {
                let n = r.u8()? as usize;
                if n < 2 || 5 + 4 * n > size {
                    return Err(MorphError::Invalid("DMAP RLE index extent".into()));
                }
                let mut pixel_indexes = Vec::new();
                let mut data_indexes = Vec::new();
                for _ in 0..n {
                    pixel_indexes.push(r.u16()? as usize);
                }
                for _ in 0..n {
                    data_indexes.push(r.u16()? as usize);
                }
                if end < r.at || (end - r.at) % (stride + 1) != 0 {
                    return Err(MorphError::Invalid("DMAP RLE run extent".into()));
                }
                let mut runs = Vec::new();
                let mut run_starts = Vec::new();
                let mut cursor = 0;
                while r.at < end {
                    let length = r.u8()? as usize;
                    if length == 0 || cursor + length > w {
                        return Err(MorphError::Invalid("DMAP RLE run coverage".into()));
                    }
                    run_starts.push(cursor);
                    runs.push((length, r.take(stride)?.to_vec()));
                    cursor += length;
                }
                if cursor != w {
                    return Err(MorphError::Invalid("DMAP incomplete RLE row".into()));
                }
                for i in 0..n {
                    if !((data_indexes[i] < runs.len()
                        && pixel_indexes[i] == run_starts[data_indexes[i]])
                        || (data_indexes[i] == runs.len() && pixel_indexes[i] == w))
                    {
                        return Err(MorphError::Invalid("DMAP RLE anchor differs".into()));
                    }
                }
                let step = 1 + w / (n - 1);
                for x in 0..w {
                    let anchor = x / step;
                    if anchor >= n {
                        return Err(MorphError::Invalid("DMAP RLE lookup index".into()));
                    }
                    let mut run = data_indexes[anchor];
                    let mut pixel = pixel_indexes[anchor];
                    if pixel > x {
                        return Err(MorphError::Invalid("DMAP RLE anchor exceeds pixel".into()));
                    }
                    while run < runs.len() && x >= pixel + runs[run].0 {
                        pixel += runs[run].0;
                        run += 1;
                    }
                    let value = &runs
                        .get(run)
                        .ok_or_else(|| MorphError::Invalid("DMAP RLE lookup outside runs".into()))?
                        .1;
                    write(x, value, &mut skin, &mut robe);
                }
            }
            if r.at != end {
                return Err(MorphError::Invalid("DMAP row boundary".into()));
            }
        }
        if r.at != raw.len() {
            return Err(MorphError::Invalid("DMAP trailing bytes".into()));
        }
        let convert = |pixels: Vec<[u8; 3]>, min: f32, delta: f32| {
            pixels
                .into_iter()
                .map(|pixel| {
                    pixel.map(|v| {
                        if v == 128 {
                            0.0
                        } else {
                            ((v as f64 * delta as f64 / 255.0) + min as f64) as f32
                        }
                    })
                })
                .collect()
        };
        out.skin = convert(skin, skin_min, skin_delta);
        out.robe = convert(robe, robe_min, robe_delta);
        Ok(out)
    }
    pub fn sample(&self, x: i32, y: i32, mirror_x: bool, robe_blend: u8) -> [f32; 3] {
        if !self.has_data || y < 0 || x == i32::MIN {
            return [0.0; 3];
        }
        let w = (self.max_col - self.min_col + 1) as usize;
        let h = (self.max_row - self.min_row + 1) as usize;
        let ux = x.unsigned_abs() as usize;
        if ux >= w || y as usize >= h {
            return [0.0; 3];
        }
        let i = y as usize * w + ux;
        let blend = robe_blend as f32 / 63.0;
        let mut result = [0.0; 3];
        for k in 0..3 {
            let mut s = self.skin[i][k];
            let mut r = self.robe[i][k];
            if k == 0 && (mirror_x || x < 0) {
                s = -s;
                r = -r;
            }
            result[k] = (s * (1.0 - blend)) + (r * blend);
        }
        result
    }
}
pub fn apply_dmap(
    geometry: &Geometry,
    original_positions: &[[f32; 3]],
    shape: &Dmap,
    shape_weight: f32,
    normal: Option<(&Dmap, f32)>,
) -> Result<AppliedMorph> {
    if !shape_weight.is_finite() || normal.map(|(_, w)| !w.is_finite()).unwrap_or(false) {
        return Err(MorphError::Invalid("Non-finite DMAP weight".into()));
    }
    let mut result = AppliedMorph {
        geometry: geometry.clone(),
        stats: ApplyStats {
            vertices: geometry.vertices.len(),
            ..Default::default()
        },
    };
    if shape_weight == 0.0 || !shape.has_data {
        return Ok(result);
    }
    if shape.kind != 0 {
        return Err(MorphError::Invalid(
            "Position DMAP is not a shape map".into(),
        ));
    }
    if original_positions.len() != geometry.vertices.len() {
        return Err(MorphError::MissingMetadata(
            "DMAP original-position count differs".into(),
        ));
    }
    if let Some((n, _)) = normal {
        if n.kind != 1 {
            return Err(MorphError::Invalid("Normal DMAP has shape kind".into()));
        }
    }
    let stitches = geometry.uv_stitches.as_ref().ok_or_else(|| {
        MorphError::MissingMetadata("DMAP requires a parsed UV-stitch tail".into())
    })?;
    let mut first_stitch = BTreeMap::new();
    for s in stitches {
        if s.vertex_index as usize >= geometry.vertices.len() {
            return Err(MorphError::Invalid("UV stitch vertex outside mesh".into()));
        }
        first_stitch
            .entry(s.vertex_index as usize)
            .or_insert(&s.coordinates);
    }
    for (i, v) in result.geometry.vertices.iter_mut().enumerate() {
        let tag = v.tag.ok_or_else(|| {
            MorphError::MissingMetadata("DMAP requires native vertex tags".into())
        })?;
        let uv1 = *v
            .uvs
            .get(1)
            .ok_or_else(|| MorphError::MissingMetadata("DMAP requires UV set1".into()))?;
        let uv = first_stitch
            .get(&i)
            .and_then(|s| s.first())
            .copied()
            .unwrap_or(uv1);
        if uv
            .iter()
            .chain(original_positions[i].iter())
            .any(|f| !f.is_finite())
        {
            return Err(MorphError::Invalid("Non-finite DMAP sampling input".into()));
        }
        let px = (shape.width as f32 * uv[0]).abs() - shape.min_col as f32 - 0.5;
        let py = shape.height as f32 * uv[1] - shape.min_row as f32 - 0.5;
        if [px, py]
            .iter()
            .any(|v| !v.is_finite() || *v < i32::MIN as f32 || *v >= i32::MAX as f32)
        {
            return Err(MorphError::Unsupported(
                "DMAP projected coordinate is outside source integer range".into(),
            ));
        }
        let x = px as i32;
        let mut y = py as i32;
        if y > (shape.max_row - shape.min_row) as i32 {
            y = ((shape.max_row - shape.min_row) as f32 - 0.5) as i32;
        }
        if x < 0
            || x > (shape.max_col - shape.min_col) as i32
            || y < 0
            || y > (shape.max_row - shape.min_row) as i32
        {
            result.stats.outside_map += 1;
            continue;
        }
        result.stats.matched_vertices += 1;
        let delta = shape.sample(x, y, original_positions[i][0] < 0.0, (tag & 63) as u8);
        if delta.iter().all(|v| v.abs() < 0.00005) {
            continue;
        }
        let weight = (((tag & 0xff00) >> 8) as f32 / 64.0).min(1.0);
        let before_position = v.position;
        let before_normal = v.normal;
        for k in 0..3 {
            v.position[k] -= delta[k] * shape_weight * weight;
        }
        if let Some((n, nw)) = normal {
            let delta = n.sample(x, y, original_positions[i][0] < 0.0, (tag & 63) as u8);
            for k in 0..3 {
                v.normal[k] -= delta[k] * nw * weight;
            }
        }
        result.stats.changed_positions += usize::from(v.position != before_position);
        result.stats.changed_normals += usize::from(v.normal != before_normal);
        if v.position
            .iter()
            .chain(v.normal.iter())
            .any(|f| !f.is_finite())
        {
            return Err(MorphError::Invalid("Non-finite DMAP result".into()));
        }
    }
    Ok(result)
}
