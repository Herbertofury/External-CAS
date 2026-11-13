//! Original VS924-compatible attribute storage. Native deformation eligibility,
//! grouping, stage choice and final material assembly remain caller contracts.
use super::*;
use external_cas_animation::Rig;
use external_cas_assets::{Geometry, gpu_vertex::*};
use std::collections::{BTreeMap, BTreeSet};

pub const CAS_NORMAL_SKINNING_VERTEX_SHA256: &str =
    "0c09f08016b8b2fd008624108d6c7b986e9ed798542b6ab89d08247d220a7b1e";
#[derive(Clone, Copy, Debug)]
pub enum PositionPacking {
    Initial,
    PostDeformation,
}
#[derive(Clone, Copy, Debug)]
pub enum NormalPacking {
    Initial,
    PostDeformation,
}
#[derive(Clone, Copy, Debug)]
pub enum TangentPacking {
    Initial,
    SeamAverage,
}
/// No default: the caller must identify which source stage supplied each field.
/// Post-deformation inputs must already have their source normalization applied.
#[derive(Clone, Copy, Debug)]
pub struct CasVertexPacking {
    pub position: PositionPacking,
    pub normal: NormalPacking,
    pub tangent: TangentPacking,
}
pub struct EncodedCasMesh {
    pub bytes: Vec<u8>,
    pub indices: Vec<u32>,
    /// Matrix rows must be uploaded in THIS order, without weight normalization.
    pub rig_bone_indices: Vec<usize>,
    pub palette_hashes: Vec<u32>,
    pub vertex_count: usize,
    pub packing: CasVertexPacking,
}
impl EncodedCasMesh {
    pub const STRIDE: u32 = 36;
    pub fn declaration() -> [NativeVertexElement; 8] {
        [
            (VertexSemantic::Position, 0, 7, 0),
            (VertexSemantic::Normal, 0, 5, 8),
            (VertexSemantic::TexCoord, 0, 6, 12),
            (VertexSemantic::TexCoord, 1, 6, 16),
            (VertexSemantic::Color, 1, 5, 20),
            (VertexSemantic::BlendIndices, 0, 4, 24),
            (VertexSemantic::BlendWeight, 0, 5, 28),
            (VertexSemantic::Tangent, 0, 5, 32),
        ]
        .map(|(semantic, semantic_index, native_format, byte_offset)| {
            NativeVertexElement {
                semantic,
                semantic_index,
                native_format,
                slot: 0,
                byte_offset,
            }
        })
    }
}
/// Retains authored fields in `geometry`; this creates only a GPU packet.
/// A compact used-hash palette is a Rust storage choice, not a claim to match
/// the engine's material/batch grouping order. Each hash keeps its real matrix.
pub fn encode_cas_normal_skinning_mesh(
    geometry: &Geometry,
    rig: &Rig,
    native_body_type: u32,
    packing: CasVertexPacking,
) -> Result<EncodedCasMesh> {
    if !matches!(geometry.shader, 0x548394b9 | 0xdf0863b8) {
        return Err("This original vertex contract is restricted to SimSkin/CAS family".into());
    }
    if geometry.vertices.is_empty()
        || geometry.vertices.len() > 65535
        || geometry.indices.is_empty()
        || geometry.indices.len() % 3 != 0
        || geometry
            .indices
            .iter()
            .any(|&i| i as usize >= geometry.vertices.len())
    {
        return Err("Original CAS geometry vertex/index extent invalid".into());
    }
    if geometry.palette_error.is_some() || !geometry.unparsed_vertex_usages.is_empty() {
        return Err("Original CAS geometry palette/attribute decoding is incomplete".into());
    }
    // This source body code rewrites tag alpha using merged-group context.
    if native_body_type == 0x72 {
        return Err("Source body0x72 requires qualified merged tag-alpha context".into());
    }
    let mut used = BTreeSet::new();
    for vertex in &geometry.vertices {
        if !vertex.has_skin {
            return Err("A nonskinned vertex cannot use original VS924".into());
        }
        used.extend(vertex.joints);
    }
    let mut by_hash = BTreeMap::new();
    let mut remap = BTreeMap::new();
    let mut rig_bone_indices = Vec::new();
    let mut palette_hashes = Vec::new();
    for source_index in used {
        let hash = *geometry
            .bone_palette
            .get(source_index as usize)
            .ok_or("Source joint exceeds GEOM palette")?;
        let index = if let Some(&index) = by_hash.get(&hash) {
            index
        } else {
            if palette_hashes.len() >= 60 {
                return Err("Original CAS shader supports at most60 used palette hashes; no joints truncated".into());
            }
            let bone = rig.bone_index(hash).map_err(|e| e.to_string())?;
            let index = palette_hashes.len() as u8;
            by_hash.insert(hash, index);
            palette_hashes.push(hash);
            rig_bone_indices.push(bone);
            index
        };
        remap.insert(source_index, index);
    }
    let mut bytes = Vec::with_capacity(geometry.vertices.len() * EncodedCasMesh::STRIDE as usize);
    for vertex in &geometry.vertices {
        let position = match packing.position {
            PositionPacking::Initial => pack_initial_position(vertex.position),
            PositionPacking::PostDeformation => pack_post_deformation_position(vertex.position),
        }
        .map_err(|e| e.to_string())?;
        for word in position {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        // Position.W and normal.W are not read by this source vertex program.
        bytes.extend_from_slice(&0i16.to_le_bytes());
        let normal = match packing.normal {
            NormalPacking::Initial => pack_initial_normal(vertex.normal),
            NormalPacking::PostDeformation => pack_post_deformation_normal(vertex.normal),
        }
        .map_err(|e| e.to_string())?;
        bytes.extend_from_slice(&normal);
        bytes.push(0);
        for occurrence in 0..2 {
            let uv = *vertex.uvs.get(occurrence).ok_or(
                "Original VS924 requires both authored UV sets; no duplicate/default stream",
            )?;
            for word in pack_initial_uv(uv, occurrence as u32).map_err(|e| e.to_string())? {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        let tag = vertex
            .tag
            .ok_or("Original COLOR1 alpha is unavailable; authored tag retained by caller")?;
        // VS924 reads COLOR1.W only. The original tag RGB remains in Geometry;
        // this packet does not claim to implement its generic robe/tag rewriting.
        bytes.extend_from_slice(&tag.to_le_bytes());
        for joint in vertex.joints {
            bytes.push(remap[&joint]);
        }
        for (value, raw) in vertex.weights.iter().zip(vertex.raw_weights) {
            if value.to_bits() != (raw as f32 / 255.).to_bits() {
                return Err("Edited float bone weights do not match retained source bytes".into());
            }
        }
        bytes.extend_from_slice(&vertex.raw_weights);
        let tangent = vertex
            .tangent
            .ok_or("Original normal-enabled vertex stage requires authored tangent")?;
        match packing.tangent {
            TangentPacking::Initial => {
                bytes.extend_from_slice(&pack_initial_tangent(tangent).map_err(|e| e.to_string())?)
            }
            TangentPacking::SeamAverage => {
                bytes.extend_from_slice(
                    &pack_seam_average_tangent(tangent).map_err(|e| e.to_string())?,
                );
                bytes.push(0x80);
            }
        }
    }
    Ok(EncodedCasMesh {
        bytes,
        indices: geometry.indices.clone(),
        rig_bone_indices,
        palette_hashes,
        vertex_count: geometry.vertices.len(),
        packing,
    })
}
impl OriginalShaderBridge {
    pub fn create_encoded_cas_mesh(
        &self,
        program: &OriginalProgram,
        packet: &EncodedCasMesh,
    ) -> Result<OriginalMesh> {
        if program.vertex.source_sha256 != CAS_NORMAL_SKINNING_VERTEX_SHA256 {
            return Err("Encoded CAS packet belongs to another original vertex program".into());
        }
        self.create_mesh(
            program,
            &EncodedCasMesh::declaration(),
            EncodedCasMesh::STRIDE,
            &packet.bytes,
            &packet.indices,
        )
    }
}
