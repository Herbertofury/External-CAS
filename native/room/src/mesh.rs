// SPDX-License-Identifier: GPL-3.0-or-later
use crate::*;
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct VertexLayout {
    pub usage: u8,
    pub usage_index: u8,
    pub format: u8,
    pub offset: u8,
    pub bytes: Option<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct VertexFormat {
    pub version: u32,
    pub stride: usize,
    pub extended: u32,
    pub layouts: Vec<VertexLayout>,
}
pub fn parse_vertex_format(data: &[u8], chunk: &RcolChunk) -> Result<VertexFormat> {
    let mut r = Bytes::region(data, chunk.extent.clone())?;
    if r.take(4)? != b"VRTF" {
        return Err("expected VRTF".into());
    }
    let version = r.u32()?;
    let stride = r.i32()?;
    let n = r.count(4)?;
    let extended = r.u32()?;
    if stride <= 0 || stride > 256 {
        return Err("unsupported vertex stride".into());
    }
    let mut layouts = Vec::new();
    for _ in 0..n {
        let usage = r.u8()?;
        let usage_index = r.u8()?;
        let format = r.u8()?;
        let offset = r.u8()?;
        let size = match format {
            0 | 4 | 5 | 6 | 8 | 11 => Some(4),
            1 | 7 | 10 | 12 | 255 => Some(8),
            2 => Some(12),
            3 => Some(16),
            _ => None,
        };
        if usize::from(offset) >= stride as usize
            || size.is_some_and(|size| usize::from(offset) + size > stride as usize)
        {
            return Err("vertex field outside stride".into());
        }
        layouts.push(VertexLayout {
            usage,
            usage_index,
            format,
            offset,
            bytes: size,
        });
    }
    Ok(VertexFormat {
        version,
        stride: stride as usize,
        extended,
        layouts,
    })
}
#[derive(Debug, Clone, Serialize)]
pub struct RoomVertex {
    pub position_short4: [i16; 4],
    pub normal_ubyte4n: Option<[u8; 4]>,
    pub tangent_ubyte4n: Option<[u8; 4]>,
    pub uv_short2: Vec<(u8, [i16; 2])>,
    pub color_ubyte4: Option<[u8; 4]>,
    pub blend_indices: Option<[u8; 4]>,
    pub blend_weights_ubyte4n: Option<[u8; 4]>,
}
#[derive(Debug, Clone, Serialize)]
pub enum RoomSkinningStatus {
    Unskinned,
    UnqualifiedBindings,
    DiagnosticSourceBindSpace { reason: String },
}
#[derive(Debug, Clone, Serialize)]
pub struct MeshGeometry {
    pub format: VertexFormat,
    pub source_vertex_bytes: Vec<u8>,
    pub indices: Vec<u32>,
    pub source_vertices: Vec<RoomVertex>,
    pub source_tool_positions: Vec<[f32; 3]>,
    pub source_tool_normals: Vec<[f32; 3]>,
    pub source_tool_uv0: Vec<[f32; 2]>,
    pub skinning_status: RoomSkinningStatus,
}
pub fn extract_mesh(
    data: &[u8],
    record: &MeshRecord,
    format: VertexFormat,
    vbuf: &RcolChunk,
    ibuf: &RcolChunk,
    max_vertices: usize,
    max_triangles: usize,
) -> Result<MeshGeometry> {
    extract_mesh_cancellable(
        data,
        record,
        format,
        vbuf,
        ibuf,
        max_vertices,
        max_triangles,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
pub fn extract_mesh_cancellable(
    data: &[u8],
    record: &MeshRecord,
    format: VertexFormat,
    vbuf: &RcolChunk,
    ibuf: &RcolChunk,
    max_vertices: usize,
    max_triangles: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<MeshGeometry> {
    let cancelled = || {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            Err("room mesh decode cancelled".to_string())
        } else {
            Ok(())
        }
    };
    cancelled()?;
    if record.primitive_type != 3 {
        return Err(format!(
            "unsupported room primitive {}",
            record.primitive_type
        ));
    }
    if record.vertex_count < 0 || record.start_index < 0 || record.primitive_count < 0 {
        return Err("negative room mesh range".into());
    }
    let count = record.vertex_count as usize;
    let triangles = record.primitive_count as usize;
    if count > max_vertices || triangles > max_triangles {
        return Err("room mesh budget".into());
    }
    if record.start_vertex != 0 || record.minimum_vertex != 0 {
        return Err(
            "nonzero room base/minimum vertex requires explicit native qualification".into(),
        );
    }
    if format
        .layouts
        .iter()
        .any(|layout| layout.bytes.is_none() || layout.usage > 6)
    {
        return Err("unsupported/skinned room vertex attributes remain in source bytes".into());
    }
    let mut vr = Bytes::region(data, vbuf.extent.clone())?;
    if vr.take(4)? != b"VBUF" {
        return Err("expected VBUF".into());
    }
    vr.u32()?;
    let vflags = vr.u32()?;
    let swizzle = vr.u32()?;
    if vflags & 6 != 0 || vflags & !7 != 0 {
        return Err(format!(
            "unsupported VBUF flags {vflags:08X}, swizzle {swizzle:08X}"
        ));
    }
    let offset = record.stream_offset as usize;
    let bytes = count
        .checked_mul(format.stride)
        .ok_or("vertex extent overflow")?;
    vr.take(offset)?;
    let source_vertex_bytes = vr.take(bytes)?.to_vec();
    let mut ir = Bytes::region(data, ibuf.extent.clone())?;
    if ir.take(4)? != b"IBUF" {
        return Err("expected IBUF".into());
    }
    ir.u32()?;
    let iflags = ir.u32()?;
    ir.u32()?;
    if iflags & !7 != 0 || iflags & 4 != 0 {
        return Err(format!("unsupported IBUF flags {iflags:08X}"));
    }
    let width = if iflags & 2 != 0 { 4 } else { 2 };
    if ir.remaining() % width != 0 {
        return Err("IBUF width extent".into());
    }
    let start = record.start_index as usize;
    let end = start
        .checked_add(triangles.checked_mul(3).ok_or("index count overflow")?)
        .ok_or("index range overflow")?;
    if end > ir.remaining() / width {
        return Err("room indices outside IBUF".into());
    }
    let mut prior = 0i32;
    let mut indices = Vec::new();
    for index in 0..end {
        if index % 1024 == 0 {
            cancelled()?;
        }
        let delta = if width == 4 {
            ir.i32()?
        } else {
            ir.u16()? as i16 as i32
        };
        let value = if iflags & 1 != 0 {
            prior
                .checked_add(delta)
                .ok_or("differenced room index overflow")?
        } else {
            delta
        };
        prior = value;
        if index >= start {
            if value < 0 || value as usize >= count {
                return Err("room index outside local vertex range".into());
            }
            indices.push(value as u32);
        }
    }
    let select = |usage, index| {
        format
            .layouts
            .iter()
            .filter(|l| l.usage == usage && l.usage_index == index)
            .collect::<Vec<_>>()
    };
    let positions = select(0, 0);
    if positions.len() != 1 || positions[0].format != 7 {
        return Err("unsupported room position declaration".into());
    }
    let position_offset = positions[0].offset as usize;
    let normals = select(1, 0);
    let normal_offset = if normals.len() == 1 && normals[0].format == 8 {
        Some(normals[0].offset as usize)
    } else if normals.is_empty() {
        None
    } else {
        return Err("unsupported room normal declaration".into());
    };
    let tangents = select(5, 0);
    let tangent_offset = if tangents.len() == 1 && tangents[0].format == 8 {
        Some(tangents[0].offset as usize)
    } else if tangents.is_empty() {
        None
    } else {
        return Err("unsupported room tangent declaration".into());
    };
    let colors = select(6, 0);
    let color_offset = if colors.len() == 1 && matches!(colors[0].format, 4 | 5 | 8) {
        Some(colors[0].offset as usize)
    } else {
        None
    };
    let uv_layouts: Vec<_> = format.layouts.iter().filter(|l| l.usage == 2).collect();
    let blends = select(3, 0);
    let weights = select(4, 0);
    let blend_offsets = if blends.is_empty() && weights.is_empty() {
        None
    } else if blends.len() == 1
        && blends[0].format == 4
        && weights.len() == 1
        && weights[0].format == 8
    {
        Some((blends[0].offset as usize, weights[0].offset as usize))
    } else {
        return Err("unsupported room blend declaration".into());
    };
    if uv_layouts.iter().any(|l| l.format != 6) {
        return Err("unsupported room UV declaration".into());
    }
    let mut source_vertices = Vec::new();
    let mut source_tool_positions = Vec::new();
    let mut source_tool_normals = Vec::new();
    let mut source_tool_uv0 = Vec::new();
    for (index, vertex) in source_vertex_bytes.chunks_exact(format.stride).enumerate() {
        if index % 1024 == 0 {
            cancelled()?;
        }
        let position_short4 = std::array::from_fn(|i| {
            i16::from_le_bytes(
                vertex[position_offset + i * 2..position_offset + i * 2 + 2]
                    .try_into()
                    .unwrap(),
            )
        });
        let normal_ubyte4n = normal_offset.map(|at| vertex[at..at + 4].try_into().unwrap());
        let tangent_ubyte4n = tangent_offset.map(|at| vertex[at..at + 4].try_into().unwrap());
        let color_ubyte4 = color_offset.map(|at| vertex[at..at + 4].try_into().unwrap());
        let uv_short2 = uv_layouts
            .iter()
            .map(|layout| {
                let at = layout.offset as usize;
                (
                    layout.usage_index,
                    [
                        i16::from_le_bytes(vertex[at..at + 2].try_into().unwrap()),
                        i16::from_le_bytes(vertex[at + 2..at + 4].try_into().unwrap()),
                    ],
                )
            })
            .collect::<Vec<_>>();
        let scalar = if position_short4[3] == 0 {
            32767.
        } else {
            f32::from(position_short4[3])
        };
        source_tool_positions.push(std::array::from_fn(|i| {
            f32::from(position_short4[i]) / scalar
        }));
        source_tool_normals.push(
            normal_ubyte4n
                .map(|n: [u8; 4]| std::array::from_fn(|i| ((f32::from(n[i]) / 255.) * 2.) - 1.))
                .unwrap_or([0.; 3]),
        );
        source_tool_uv0.push(
            uv_short2
                .iter()
                .find(|(usage, _)| *usage == 0)
                .map(|(_, uv)| uv.map(|v| f32::from(v) / 32767.))
                .unwrap_or([0.; 2]),
        );
        source_vertices.push(RoomVertex {
            position_short4,
            normal_ubyte4n,
            tangent_ubyte4n,
            uv_short2,
            color_ubyte4,
            blend_indices: blend_offsets.map(|(a, _)| vertex[a..a + 4].try_into().unwrap()),
            blend_weights_ubyte4n: blend_offsets.map(|(_, a)| vertex[a..a + 4].try_into().unwrap()),
        });
    }
    Ok(MeshGeometry {
        format,
        source_vertex_bytes,
        indices,
        source_vertices,
        source_tool_positions,
        source_tool_normals,
        source_tool_uv0,
        skinning_status: if blend_offsets.is_some() {
            RoomSkinningStatus::UnqualifiedBindings
        } else {
            RoomSkinningStatus::Unskinned
        },
    })
}
