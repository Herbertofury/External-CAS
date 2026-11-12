// SPDX-License-Identifier: GPL-3.0-or-later
//! RCOL/MODL/MLOD/MATD layouts checked against recovered GPL Sims4Tools readers.
use crate::{Bytes, Result};
use external_cas_assets::ResourceKey;
use serde::Serialize;
use std::ops::Range;
#[derive(Debug, Clone, Serialize)]
pub struct RcolChunk {
    pub key: ResourceKey,
    pub extent: Range<usize>,
    pub declared_offset: u32,
    pub declared_size: i32,
    pub tag: [u8; 4],
}
#[derive(Debug, Clone, Serialize)]
pub struct Rcol {
    pub version: u32,
    pub public_count: usize,
    pub unused: u32,
    pub chunks: Vec<RcolChunk>,
    pub externals: Vec<ResourceKey>,
}
#[derive(Debug, Clone, Serialize)]
pub enum ChunkReference {
    Null,
    Internal { index: usize, key: ResourceKey },
    External { index: usize, key: ResourceKey },
    Unsupported { kind: u32, index: u32 },
}
fn key(r: &mut Bytes<'_>) -> Result<ResourceKey> {
    let instance = r.u64()?;
    let ty = r.u32()?;
    let group = r.u32()?;
    Ok(ResourceKey::new(ty, group, instance))
}
impl Rcol {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() > 32 * 1024 * 1024 {
            return Err("RCOL payload budget".into());
        }
        let mut r = Bytes::new(data);
        let version = r.u32()?;
        if version != 3 {
            return Err(format!("unsupported RCOL version {version}"));
        }
        let public = r.i32()?;
        let unused = r.u32()?;
        let external = r.i32()?;
        let internal = r.i32()?;
        if public < 0 || external < 0 || internal < 1 || public > internal {
            return Err("invalid RCOL counts".into());
        }
        let (public_count, external, internal) =
            (public as usize, external as usize, internal as usize);
        let table_end = 20usize
            .checked_add(
                (internal + external)
                    .checked_mul(16)
                    .ok_or("RCOL table overflow")?,
            )
            .and_then(|v| v.checked_add(internal * 8))
            .filter(|end| *end <= data.len())
            .ok_or("RCOL table outside payload")?;
        let keys = (0..internal)
            .map(|_| key(&mut r))
            .collect::<Result<Vec<_>>>()?;
        let externals = (0..external)
            .map(|_| key(&mut r))
            .collect::<Result<Vec<_>>>()?;
        let mut chunks = Vec::new();
        for key in keys {
            let declared_offset = r.u32()?;
            let declared_size = r.i32()?;
            let (start, size) = if internal == 1 {
                (table_end, data.len() - table_end)
            } else {
                if declared_size < 0 {
                    return Err("negative RCOL chunk size".into());
                }
                (declared_offset as usize, declared_size as usize)
            };
            let end = start
                .checked_add(size)
                .filter(|end| *end <= data.len())
                .ok_or("RCOL chunk outside payload")?;
            if start < table_end || size < 4 {
                return Err("RCOL chunk intersects table or lacks tag".into());
            }
            chunks.push(RcolChunk {
                key,
                extent: start..end,
                declared_offset,
                declared_size,
                tag: data[start..start + 4].try_into().unwrap(),
            });
        }
        Ok(Self {
            version,
            public_count,
            unused,
            chunks,
            externals,
        })
    }
    pub fn reference(&self, value: u32) -> ChunkReference {
        if value == 0 {
            return ChunkReference::Null;
        }
        let kind = value >> 28;
        let Some(index) = (value & 0xfffffff).checked_sub(1) else {
            return ChunkReference::Unsupported {
                kind,
                index: u32::MAX,
            };
        };
        let i = index as usize;
        match kind {
            0 if i < self.public_count => ChunkReference::Internal {
                index: i,
                key: self.chunks[i].key,
            },
            1 if i + self.public_count < self.chunks.len() => {
                let index = i + self.public_count;
                ChunkReference::Internal {
                    index,
                    key: self.chunks[index].key,
                }
            }
            3 if i < self.externals.len() => ChunkReference::External {
                index: i,
                key: self.externals[i],
            },
            _ => ChunkReference::Unsupported { kind, index },
        }
    }
    pub fn internal(&self, value: u32) -> Result<&RcolChunk> {
        match self.reference(value) {
            ChunkReference::Internal { index, .. } => Ok(&self.chunks[index]),
            other => Err(format!("required room chunk is {other:?}")),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ModelLod {
    pub raw_reference: u32,
    pub flags: u32,
    pub id: u32,
    pub minimum: f32,
    pub maximum: f32,
    pub target: ChunkReference,
}
#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub version: u32,
    pub bounds: [f32; 6],
    pub extra_bounds: Vec<[f32; 6]>,
    pub fade: Option<(u32, f32)>,
    pub unknown_prefix: Vec<u8>,
    pub lods: Vec<ModelLod>,
    pub unparsed: Range<usize>,
}
pub fn parse_model(data: &[u8], rcol: &Rcol, chunk: &RcolChunk) -> Result<Model> {
    let mut r = Bytes::region(data, chunk.extent.clone())?;
    if r.take(4)? != b"MODL" {
        return Err("expected MODL".into());
    }
    let version = r.u32()?;
    let n = r.count(20)?;
    let mut bounds = [0.; 6];
    for v in &mut bounds {
        *v = r.f32()?;
    }
    let mut extra_bounds = Vec::new();
    let mut fade = None;
    let mut unknown_prefix = Vec::new();
    if (258..0x300).contains(&version) {
        let n = r.count(24)?;
        for _ in 0..n {
            let mut bound = [0.; 6];
            for v in &mut bound {
                *v = r.f32()?;
            }
            extra_bounds.push(bound);
        }
        fade = Some((r.u32()?, r.f32()?));
    } else if version >= 0x300 {
        unknown_prefix = r.take(20)?.to_vec();
    }
    let mut lods = Vec::new();
    for _ in 0..n {
        let raw_reference = r.u32()?;
        lods.push(ModelLod {
            raw_reference,
            flags: r.u32()?,
            id: r.u32()?,
            minimum: r.f32()?,
            maximum: r.f32()?,
            target: rcol.reference(raw_reference),
        });
    }
    Ok(Model {
        version,
        bounds,
        extra_bounds,
        fade,
        unknown_prefix,
        lods,
        unparsed: r.absolute()..chunk.extent.end,
    })
}
#[derive(Debug, Clone, Serialize)]
pub struct MeshRecord {
    pub extent: Range<usize>,
    pub name: u32,
    pub material: u32,
    pub vertex_format: u32,
    pub vertex_buffer: u32,
    pub index_buffer: u32,
    pub primitive_type: u8,
    pub flags: u32,
    pub stream_offset: u32,
    pub start_vertex: i32,
    pub start_index: i32,
    pub minimum_vertex: i32,
    pub vertex_count: i32,
    pub primitive_count: i32,
    pub parameter_material: Option<u32>,
    pub joint_count: Option<usize>,
    pub skin_controller: Option<u32>,
    pub joint_references: Vec<u32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModelLodChunk {
    pub version: u32,
    pub meshes: Vec<MeshRecord>,
    pub unparsed: Range<usize>,
}
pub fn parse_mlod(data: &[u8], chunk: &RcolChunk) -> Result<ModelLodChunk> {
    let mut r = Bytes::region(data, chunk.extent.clone())?;
    if r.take(4)? != b"MLOD" {
        return Err("expected MLOD".into());
    }
    let version = r.u32()?;
    let n = r.count(52)?;
    let mut meshes = Vec::new();
    for _ in 0..n {
        let size = r.u32()? as usize;
        let start = r.absolute();
        let bytes = r.take(size)?;
        let mut m = Bytes::new(bytes);
        if size < 48 {
            return Err("short MLOD mesh".into());
        }
        let name = m.u32()?;
        let material = m.u32()?;
        let vertex_format = m.u32()?;
        let vertex_buffer = m.u32()?;
        let index_buffer = m.u32()?;
        let primitive = m.u32()?;
        let stream_offset = m.u32()?;
        let start_vertex = m.i32()?;
        let start_index = m.i32()?;
        let minimum_vertex = m.i32()?;
        let vertex_count = m.i32()?;
        let primitive_count = m.i32()?;
        let (parameter_material, joint_count, skin_controller, joint_references) = if size >= 84 {
            m.at = 72;
            let skin = m.u32()?;
            let joints = m.count(4)?;
            let references = (0..joints).map(|_| m.u32()).collect::<Result<Vec<_>>>()?;
            (Some(m.u32()?), Some(joints), Some(skin), references)
        } else {
            (None, None, None, Vec::new())
        };
        meshes.push(MeshRecord {
            extent: start..start + size,
            name,
            material,
            vertex_format,
            vertex_buffer,
            index_buffer,
            primitive_type: (primitive & 255) as u8,
            flags: primitive >> 8,
            stream_offset,
            start_vertex,
            start_index,
            minimum_vertex,
            vertex_count,
            primitive_count,
            parameter_material,
            joint_count,
            skin_controller,
            joint_references,
        });
    }
    Ok(ModelLodChunk {
        version,
        meshes,
        unparsed: r.absolute()..chunk.extent.end,
    })
}
#[derive(Debug, Clone, Serialize)]
pub struct MaterialField {
    pub field: u32,
    pub kind: u32,
    pub words: i32,
    pub extent: Range<usize>,
    pub bytes: Vec<u8>,
    pub texture_key: Option<ResourceKey>,
    pub float_bits: Option<Vec<u32>>,
}
impl MaterialField {
    pub fn floats(&self) -> Option<Vec<f32>> {
        self.float_bits
            .as_ref()
            .map(|bits| bits.iter().copied().map(f32::from_bits).collect())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Material {
    pub version: u32,
    pub shader: u32,
    pub fields: Vec<MaterialField>,
    pub unparsed: Range<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MaterialSetEntry {
    pub reference: u32,
    pub state: u32,
    pub variant: Option<u32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MaterialSet {
    pub version: u32,
    pub name_hash: u32,
    pub raw_index: u32,
    pub entries: Vec<MaterialSetEntry>,
}
pub fn parse_material_set(data: &[u8], chunk: &RcolChunk) -> Result<MaterialSet> {
    let mut r = Bytes::region(data, chunk.extent.clone())?;
    if r.take(4)? != b"MTST" {
        return Err("expected MTST".into());
    }
    let version = r.u32()?;
    let name_hash = r.u32()?;
    let raw_index = r.u32()?;
    let n = r.count(if version < 0x300 { 8 } else { 12 })?;
    let mut entries = Vec::new();
    for _ in 0..n {
        entries.push(MaterialSetEntry {
            reference: r.u32()?,
            state: r.u32()?,
            variant: if version < 0x300 {
                None
            } else {
                Some(r.u32()?)
            },
        });
    }
    if r.remaining() != 0 {
        return Err("unqualified MTST trailing data".into());
    }
    Ok(MaterialSet {
        version,
        name_hash,
        raw_index,
        entries,
    })
}
impl Material {
    pub fn float_values(&self, field: u32) -> Result<Option<Vec<f32>>> {
        let found: Vec<_> = self.fields.iter().filter(|f| f.field == field).collect();
        if found.len() > 1 {
            return Err(format!("ambiguous room material field {field:08X}"));
        }
        let Some(value) = found.first() else {
            return Ok(None);
        };
        let Some(values) = value.floats() else {
            return Err(format!(
                "room material field {field:08X} is not float encoded"
            ));
        };
        if !values.iter().all(|v| v.is_finite()) {
            return Err("nonfinite room material transform; exact bits retained".into());
        }
        Ok(Some(values))
    }
}
pub fn parse_material(data: &[u8], chunk: &RcolChunk) -> Result<Material> {
    let bytes = data
        .get(chunk.extent.clone())
        .ok_or("MATD outside payload")?;
    let mut r = Bytes::new(bytes);
    if r.take(4)? != b"MATD" {
        return Err("expected MATD".into());
    }
    let version = r.u32()?;
    r.u32()?;
    let shader = r.u32()?;
    let length = r.u32()? as usize;
    let start: usize = if version < 0x103 { 20 } else { 28 };
    let end = start
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or("MTRL outside MATD")?;
    let mut m = Bytes::new(&bytes[start..end]);
    if m.take(4)? != b"MTRL" {
        return Err("expected MTRL".into());
    }
    m.take(8)?;
    let n = m.count(16)?;
    let table_end = 16 + n * 16;
    let mut fields = Vec::new();
    for _ in 0..n {
        let field = m.u32()?;
        let kind = m.u32()?;
        let words = m.i32()?;
        let offset = m.u32()? as usize;
        if words < 0 || offset < table_end {
            return Err("invalid MTRL field extent".into());
        }
        let len = (words as usize)
            .checked_mul(4)
            .ok_or("MTRL extent overflow")?;
        let finish = offset
            .checked_add(len)
            .filter(|finish| *finish <= length)
            .ok_or("MTRL data outside chunk")?;
        let raw = &bytes[start + offset..start + finish];
        let texture_key =
            if (kind == 4 && (words == 4 || words == 5)) || (kind == 0x10004 && words == 4) {
                Some(key(&mut Bytes::new(raw))?)
            } else {
                None
            };
        let float_bits = if kind == 1 {
            Some(
                raw.chunks_exact(4)
                    .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
                    .collect(),
            )
        } else {
            None
        };
        fields.push(MaterialField {
            field,
            kind,
            words,
            extent: chunk.extent.start + start + offset..chunk.extent.start + start + finish,
            bytes: raw.to_vec(),
            texture_key,
            float_bits,
        });
    }
    Ok(Material {
        version,
        shader,
        fields,
        unparsed: chunk.extent.start + end..chunk.extent.end,
    })
}
