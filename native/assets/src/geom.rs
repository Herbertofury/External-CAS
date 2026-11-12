// SPDX-License-Identifier: GPL-3.0-or-later
// Port of recovered GEOM/palette readers; see README for upstream provenance.
use crate::{Bytes, Error, ResourceKey, Result};
#[derive(Clone, Debug)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
    pub raw_weights: [u8; 4],
    pub uvs: Vec<[f32; 2]>,
    pub has_skin: bool,
    pub vertex_id: Option<i32>,
    pub tag: Option<u32>,
    pub tangent: Option<[f32; 3]>,
}
#[derive(Clone, Debug)]
pub struct UvStitch {
    pub vertex_index: u32,
    pub coordinates: Vec<[f32; 2]>,
}
#[derive(Clone, Debug)]
pub struct SeamStitch {
    pub vertex_index: u32,
    pub vertex_id: u16,
}
#[derive(Clone, Debug)]
pub struct Geometry {
    pub version: u32,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bone_palette: Vec<u32>,
    pub palette_error: Option<String>,
    pub shader: u32,
    pub merge_group: u32,
    pub sort_order: u32,
    pub references: Vec<ResourceKey>,
    pub material: Vec<u8>,
    pub unparsed_vertex_usages: Vec<u32>,
    pub uv_stitches: Option<Vec<UvStitch>>,
    pub seam_stitches: Option<Vec<SeamStitch>>,
    /// Immutable original bytes remain available for every unparsed field.
    pub original: Vec<u8>,
}
impl Geometry {
    /// SimRipper GetStitchUVs returns the FIRST matching UVStitch record. It
    /// does not combine duplicate records or derive UVs from SeamStitch tags.
    pub fn stitch_uvs(&self, vertex_index: usize) -> Result<&[[f32; 2]]> {
        if vertex_index >= self.vertices.len() {
            return Err(Error::invalid("stitch vertex outside geometry"));
        }
        let records = self
            .uv_stitches
            .as_ref()
            .ok_or_else(|| Error::unsupported("GEOM stitch tail is unavailable"))?;
        Ok(records
            .iter()
            .find(|s| s.vertex_index as usize == vertex_index)
            .map(|s| s.coordinates.as_slice())
            .unwrap_or(&[]))
    }
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Bytes::new(data);
        if r.u32()? != 3 {
            return Err(Error::unsupported("GEOM RCOL wrapper"));
        }
        r.skip(8)?;
        let external = r.u32()? as usize;
        let internal = r.u32()? as usize;
        if internal != 1 {
            return Err(Error::unsupported("multi-chunk GEOM RCOL"));
        }
        let n = external
            .checked_add(internal)
            .and_then(|n| n.checked_mul(16))
            .ok_or_else(|| Error::invalid("GEOM key count"))?;
        r.skip(n)?;
        let offset = r.u32()? as usize;
        let size = r.u32()? as usize;
        if offset != r.at || size < 24 || offset.checked_add(size).is_none_or(|n| n > data.len()) {
            return Err(Error::invalid("GEOM chunk extent"));
        }
        r.end = offset + size;
        if r.take(4)? != b"GEOM" {
            return Err(Error::invalid("GEOM magic"));
        }
        let version = r.u32()?;
        if !(12..=15).contains(&version) {
            return Err(Error::unsupported(format!("GEOM version {version}")));
        }
        let tgi_field = r.at;
        let relative = r.u32()? as usize;
        let _tgi_size = r.u32()?;
        let shader = r.u32()?;
        let material = if shader != 0 {
            let size = r.u32()? as usize;
            r.take(size)?.to_vec()
        } else {
            Vec::new()
        };
        let merge_group = r.u32()?;
        let sort_order = r.u32()?;
        let vertex_count = r.u32()? as usize;
        let format_count = r.count(9)?;
        if vertex_count == 0 || format_count == 0 {
            return Err(Error::invalid("GEOM empty vertex declaration"));
        }
        let mut formats = Vec::with_capacity(format_count);
        let mut stride = 0usize;
        let mut unparsed_vertex_usages = Vec::new();
        for _ in 0..format_count {
            let usage = r.u32()?;
            let subtype = r.u32()?;
            let size = r.u8()? as usize;
            if size == 0 {
                return Err(Error::invalid("zero GEOM attribute size"));
            }
            if (matches!(usage, 1 | 2 | 6) && size != 12)
                || (usage == 3 && size != 8)
                || (matches!(usage, 4 | 5 | 7 | 10) && size != 4)
                || (usage == 5 && subtype != 2)
            {
                return Err(Error::unsupported("GEOM vertex declaration"));
            }
            stride = stride
                .checked_add(size)
                .ok_or_else(|| Error::invalid("GEOM stride overflow"))?;
            if !matches!(usage, 1..=7 | 10) {
                unparsed_vertex_usages.push(usage);
            }
            formats.push((usage, subtype, size));
        }
        for usage in [1, 2] {
            if formats.iter().filter(|f| f.0 == usage).count() != 1 {
                return Err(Error::unsupported(
                    "GEOM missing/duplicate position or normal",
                ));
            }
        }
        if !formats.iter().any(|f| f.0 == 3)
            || formats.iter().filter(|f| f.0 == 4).count() > 1
            || formats.iter().filter(|f| f.0 == 5).count() > 1
        {
            return Err(Error::unsupported("GEOM UV/skin declaration"));
        }
        if vertex_count > r.remaining() / stride {
            return Err(Error::invalid("GEOM vertex count exceeds extent"));
        }
        let mut vertices = Vec::new();
        vertices
            .try_reserve(vertex_count)
            .map_err(|_| Error::invalid("GEOM vertex allocation"))?;
        for _ in 0..vertex_count {
            let mut v = Vertex {
                position: [0.; 3],
                normal: [0.; 3],
                uv: [0.; 2],
                joints: [0; 4],
                weights: [0.; 4],
                raw_weights: [0; 4],
                uvs: Vec::new(),
                has_skin: false,
                vertex_id: None,
                tag: None,
                tangent: None,
            };
            let mut joints = false;
            let mut weights = false;
            for &(usage, _, size) in &formats {
                match usage {
                    1 => v.position = [r.f32()?, r.f32()?, r.f32()?],
                    2 => v.normal = [r.f32()?, r.f32()?, r.f32()?],
                    3 => v.uvs.push([r.f32()?, r.f32()?]),
                    4 => {
                        let b = r.take(4)?;
                        for (i, &n) in b.iter().enumerate() {
                            v.joints[i] = n as u16;
                        }
                        joints = true;
                    }
                    5 => {
                        v.raw_weights = r.take(4)?.try_into().unwrap();
                        for i in 0..4 {
                            v.weights[i] = v.raw_weights[i] as f32 / 255.;
                        }
                        weights = true;
                    }
                    6 => v.tangent = Some([r.f32()?, r.f32()?, r.f32()?]),
                    7 => v.tag = Some(r.u32()?),
                    10 => v.vertex_id = Some(r.i32()?),
                    _ => r.skip(size)?,
                }
            }
            v.uv = v.uvs[0];
            v.has_skin = joints && weights;
            vertices.push(v);
        }
        let groups = r.u32()?;
        let index_size = r.u8()? as usize;
        let index_count = r.u32()? as usize;
        if groups != 1 || ![2, 4].contains(&index_size) {
            return Err(Error::unsupported("GEOM index layout"));
        }
        if index_count == 0 || index_count % 3 != 0 || index_count > r.remaining() / index_size {
            return Err(Error::invalid("GEOM triangle index extent"));
        }
        let mut indices = Vec::with_capacity(index_count);
        for _ in 0..index_count {
            let n = if index_size == 2 {
                r.u16()? as u32
            } else {
                r.u32()?
            };
            if n as usize >= vertex_count {
                return Err(Error::invalid("GEOM triangle index outside vertices"));
            }
            indices.push(n);
        }
        let tgi_at = tgi_field
            .checked_add(4)
            .and_then(|n| n.checked_add(relative))
            .ok_or_else(|| Error::invalid("GEOM reference offset"))?;
        if tgi_at < r.at || tgi_at > r.end {
            return Err(Error::invalid("GEOM reference table overlap"));
        }
        let palette = parse_palette(data, r.at, tgi_at, version, vertex_count);
        let (bone_palette, uv_stitches, seam_stitches, palette_error) = match palette {
            Ok(tail) => (
                tail.bone_palette,
                Some(tail.uv_stitches),
                Some(tail.seam_stitches),
                None,
            ),
            Err(e) => (Vec::new(), None, None, Some(e.to_string())),
        };
        let mut t = Bytes {
            data,
            at: tgi_at,
            end: r.end,
        };
        let count = t.count(16)?;
        let mut references = Vec::with_capacity(count);
        for _ in 0..count {
            let ty = t.u32()?;
            let group = t.u32()?;
            let instance = t.u64()?;
            references.push(ResourceKey::new(ty, group, instance));
        }
        if t.remaining() != 0 {
            return Err(Error::unsupported("trailing GEOM reference bytes"));
        }
        Ok(Self {
            version,
            vertices,
            indices,
            bone_palette,
            palette_error,
            shader,
            merge_group,
            sort_order,
            references,
            material,
            unparsed_vertex_usages,
            uv_stitches,
            seam_stitches,
            original: data.to_vec(),
        })
    }
}
struct GeometryTail {
    bone_palette: Vec<u32>,
    uv_stitches: Vec<UvStitch>,
    seam_stitches: Vec<SeamStitch>,
}
fn parse_palette(
    data: &[u8],
    at: usize,
    end: usize,
    version: u32,
    vertex_count: usize,
) -> Result<GeometryTail> {
    let mut r = Bytes { data, at, end };
    let stitches = r.count(8)?;
    let mut uv_stitches = Vec::with_capacity(stitches);
    for _ in 0..stitches {
        let vertex_index = r.u32()?;
        if vertex_index as usize >= vertex_count {
            return Err(Error::invalid("GEOM UV stitch vertex outside geometry"));
        }
        let pairs = r.count(8)?;
        let mut coordinates = Vec::with_capacity(pairs);
        for _ in 0..pairs {
            coordinates.push([r.f32()?, r.f32()?]);
        }
        uv_stitches.push(UvStitch {
            vertex_index,
            coordinates,
        });
    }
    let mut seam_stitches = Vec::new();
    if version >= 13 {
        let seams = r.count(6)?;
        seam_stitches
            .try_reserve(seams)
            .map_err(|_| Error::invalid("GEOM seam allocation"))?;
        for _ in 0..seams {
            let vertex_index = r.u32()?;
            let vertex_id = r.u16()?;
            if vertex_index as usize >= vertex_count {
                return Err(Error::invalid("GEOM seam vertex outside geometry"));
            }
            seam_stitches.push(SeamStitch {
                vertex_index,
                vertex_id,
            });
        }
    }
    let intersections = r.count(if version >= 14 { 66 } else { 63 })?;
    r.skip(intersections * if version >= 14 { 66 } else { 63 })?;
    let bones = r.count(4)?;
    let mut hashes = Vec::with_capacity(bones);
    for _ in 0..bones {
        hashes.push(r.u32()?);
    }
    if version >= 15 {
        let states = r.count(20)?;
        r.skip(states * 20)?;
    }
    if r.remaining() != 0 {
        return Err(Error::unsupported("GEOM palette tail"));
    }
    Ok(GeometryTail {
        bone_palette: hashes,
        uv_stitches,
        seam_stitches,
    })
}
