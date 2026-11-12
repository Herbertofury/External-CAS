// SPDX-License-Identifier: GPL-3.0-or-later
//! SKIN hash/inverse-bind framing follows Sims4Tools SKIN.cs (GPL-3.0+).
use crate::*;
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct RoomSkinBone {
    pub hash: u32,
    pub inverse_bind_bits: [u32; 12],
}
#[derive(Debug, Clone, Serialize)]
pub struct RoomSkin {
    pub version: u32,
    pub bones: Vec<RoomSkinBone>,
}
pub fn parse_room_skin(data: &[u8], chunk: &RcolChunk) -> Result<RoomSkin> {
    let mut r = Bytes::region(data, chunk.extent.clone())?;
    if r.take(4)? != b"SKIN" {
        return Err("expected SKIN".into());
    }
    let version = r.u32()?;
    if version != 1 {
        return Err(format!("unsupported room SKIN version {version}"));
    }
    let n = r.count(52)?;
    if n > 256 {
        return Err("room SKIN bone budget".into());
    }
    let hashes = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
    let mut bones = Vec::new();
    for hash in hashes {
        let mut inverse_bind_bits = [0; 12];
        for v in &mut inverse_bind_bits {
            *v = r.u32()?;
        }
        bones.push(RoomSkinBone {
            hash,
            inverse_bind_bits,
        });
    }
    if r.remaining() != 0 {
        return Err("unqualified room SKIN trailing bytes".into());
    }
    Ok(RoomSkin { version, bones })
}
/// Observe static binding prerequisites without inventing the runtime palette.
/// Even identity inverse bind plus a null Rig link cannot establish shader
/// palette-index semantics or the actual global bone transform. Only unskinned
/// data currently passes this normal-path gate.
pub fn qualify_static_skinning(
    geometry: &mut MeshGeometry,
    record: &MeshRecord,
    skin: Option<&RoomSkin>,
    explicit_null_object_rig: bool,
) -> Result<()> {
    if matches!(geometry.skinning_status, RoomSkinningStatus::Unskinned) {
        return Ok(());
    }
    if !explicit_null_object_rig {
        return Err(
            "room skin binding requires observed object rig/pose; explicit null Rig link absent"
                .into(),
        );
    }
    let skin = skin.ok_or("room blend attributes lack SKIN binding")?;
    let mut active = None;
    for vertex in &geometry.source_vertices {
        let indices = vertex.blend_indices.ok_or("room blend indices absent")?;
        let weights = vertex
            .blend_weights_ubyte4n
            .ok_or("room blend weights absent")?;
        if weights.iter().map(|v| u16::from(*v)).sum::<u16>() != 255 {
            return Err("room static blend weights are not exactly normalized".into());
        }
        for i in 0..4 {
            if weights[i] == 0 {
                continue;
            }
            let hash = *record
                .joint_references
                .get(indices[i] as usize)
                .ok_or("active room blend index outside joint references")?;
            if active.is_some_and(|h| h != hash) {
                return Err("mixed room skin bone hashes require actual rig/pose".into());
            }
            let found = skin
                .bones
                .iter()
                .filter(|b| b.hash == hash)
                .collect::<Vec<_>>();
            if found.len() != 1 {
                return Err("active room SKIN bone hash absent/ambiguous".into());
            }
            let identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.];
            if found[0]
                .inverse_bind_bits
                .iter()
                .zip(identity)
                .any(|(a, b)| f32::from_bits(*a) != b)
            {
                return Err(
                    "active room inverse bind is not identity; actual rig/pose required".into(),
                );
            }
            active = Some(hash);
        }
    }
    let hash = active.ok_or("empty room skin weights")?;
    Err(format!(
        "source identity inverse bind for hash {hash:08X} observed, but null object Rig does not establish actual shader palette mapping/global bone transform"
    ))
}
