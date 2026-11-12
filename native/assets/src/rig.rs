// SPDX-License-Identifier: GPL-3.0-or-later
// RIG 3.1 layout follows recovered rig.mjs / TS4CAS Tools RIG.cs.
use crate::{Bytes, Error, Geometry, Result};
#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub opposing_index: i32,
    pub parent_index: i32,
    pub hash: u32,
    pub flags: u32,
    pub position: [f32; 3],
    pub orientation: [f32; 4],
    pub scale: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct Chain {
    pub indices: Vec<i32>,
    pub pole: i32,
    pub slot: i32,
    pub root: i32,
}
#[derive(Clone, Debug)]
pub struct Rig {
    pub bones: Vec<Bone>,
    pub chains: Vec<Chain>,
}
impl Rig {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Bytes::new(data);
        let major = r.u32()?;
        let minor = r.u32()?;
        if major != 3 || minor != 1 {
            return Err(Error::unsupported(format!("RIG {major}.{minor}")));
        }
        let n = r.count(60)?;
        if n == 0 {
            return Err(Error::invalid("empty RIG"));
        }
        let mut bones = Vec::with_capacity(n);
        for i in 0..n {
            let position = [r.f32()?, r.f32()?, r.f32()?];
            let orientation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
            let scale = [r.f32()?, r.f32()?, r.f32()?];
            let len = r.u32()? as usize;
            let name = std::str::from_utf8(r.take(len)?)
                .map_err(|_| Error::unsupported("RIG name encoding"))?
                .to_owned();
            let opposing_index = r.i32()?;
            let parent_index = r.i32()?;
            let hash = r.u32()?;
            let flags = r.u32()?;
            if opposing_index < -1
                || opposing_index >= n as i32
                || parent_index < -1
                || parent_index >= n as i32
                || parent_index == i as i32
            {
                return Err(Error::invalid("RIG bone hierarchy"));
            }
            bones.push(Bone {
                name,
                opposing_index,
                parent_index,
                hash,
                flags,
                position,
                orientation,
                scale,
            });
        }
        // Validate all parent chains, including valid parents stored later in the file.
        let mut marks = vec![0u8; n];
        for start in 0..n {
            let mut chain = Vec::new();
            let mut current = Some(start);
            while let Some(i) = current {
                if marks[i] == 2 {
                    break;
                }
                if marks[i] == 1 {
                    return Err(Error::invalid("cyclic RIG hierarchy"));
                }
                marks[i] = 1;
                chain.push(i);
                current = (bones[i].parent_index >= 0).then_some(bones[i].parent_index as usize);
            }
            for i in chain {
                marks[i] = 2;
            }
        }
        let n = r.count(16)?;
        let mut chains = Vec::with_capacity(n);
        for _ in 0..n {
            let count = r.count(4)?;
            let mut indices = Vec::with_capacity(count);
            for _ in 0..count {
                indices.push(r.i32()?);
            }
            let pole = r.i32()?;
            let slot = r.i32()?;
            let root = r.i32()?;
            if indices
                .iter()
                .chain([&pole, &slot, &root])
                .any(|i| *i < -1 || *i >= bones.len() as i32)
            {
                return Err(Error::invalid("RIG chain index"));
            }
            chains.push(Chain {
                indices,
                pole,
                slot,
                root,
            });
        }
        if r.remaining() != 0 {
            return Err(Error::unsupported("trailing RIG data"));
        }
        Ok(Self { bones, chains })
    }
    /// No arbitrary skeleton is selected. Every used GEOM hash must map uniquely.
    pub fn bind_geometry(&self, geometry: &Geometry) -> Result<Vec<[u16; 4]>> {
        let mut out = Vec::with_capacity(geometry.vertices.len());
        for v in &geometry.vertices {
            if !v.has_skin {
                return Err(Error::unsupported("GEOM lacks skin weights"));
            }
            let mut joints = [0; 4];
            let mut total = 0u32;
            for i in 0..4 {
                if v.raw_weights[i] == 0 {
                    continue;
                }
                total += v.raw_weights[i] as u32;
                let hash = *geometry
                    .bone_palette
                    .get(v.joints[i] as usize)
                    .ok_or_else(|| Error::invalid("GEOM bone index outside palette"))?;
                let mut matches = self
                    .bones
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.hash == hash);
                let (j, _) = matches.next().ok_or_else(|| {
                    Error::unsupported(format!("RIG missing bone hash {hash:08X}"))
                })?;
                if matches.next().is_some() {
                    return Err(Error::unsupported(format!(
                        "RIG duplicate bone hash {hash:08X}"
                    )));
                }
                joints[i] = u16::try_from(j)
                    .map_err(|_| Error::unsupported("RIG exceeds renderer joint index width"))?;
            }
            if total == 0 {
                return Err(Error::invalid("GEOM vertex has no weighted influences"));
            }
            out.push(joints);
        }
        Ok(out)
    }
}
