// SPDX-License-Identifier: GPL-3.0-or-later
// RIG v3.1 layout ported from the recovered TS4CAS Tools-derived rig.mjs.
use crate::{Matrix4, Result, fingerprint, invalid, reader::Reader, unsupported};

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub opposing_index: i32,
    pub parent_index: i32,
    pub hash: u32,
    pub flags: u32,
    pub position: [f32; 3],
    /// XYZW quaternion, preserved exactly as stored.
    pub orientation: [f32; 4],
    pub scale: [f32; 3],
}
#[derive(Debug, Clone)]
pub struct IkChain {
    pub indices: Vec<i32>,
    pub pole: i32,
    pub slot: i32,
    pub root: i32,
}

#[derive(Debug, Clone)]
pub struct Rig {
    pub version: u32,
    pub minor_version: u32,
    pub bones: Vec<Bone>,
    pub chains: Vec<IkChain>,
    pub trailing: Vec<u8>,
    pub original: Vec<u8>,
    pub sha256: [u8; 32],
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BindPose {
    pub rig_sha256: [u8; 32],
    pub local: Vec<Matrix4>,
    pub global: Vec<Matrix4>,
    pub inverse: Vec<Matrix4>,
    /// Parent-before-child evaluation order; file order need not be topological.
    pub evaluation_order: Vec<usize>,
    pub caller_supplied_globals: bool,
}

impl Rig {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(invalid(0, "rig exceeds 16 MiB input budget"));
        }
        let mut r = Reader::new(bytes);
        let version = r.u32()?;
        let minor_version = r.u32()?;
        let mut rig = Self {
            version,
            minor_version,
            bones: Vec::new(),
            chains: Vec::new(),
            trailing: Vec::new(),
            original: bytes.to_vec(),
            sha256: fingerprint(bytes),
            limitations: Vec::new(),
        };
        if version != 3 || minor_version != 1 {
            rig.limitations.push(format!(
                "unsupported rig version {version}.{minor_version}; original bytes retained"
            ));
            rig.trailing = bytes[r.at..].to_vec();
            return Ok(rig);
        }
        let count = r.u32()? as usize;
        if count == 0 || count > 4096 {
            return Err(invalid(r.at - 4, "invalid rig bone count"));
        }
        for index in 0..count {
            let position = r.vector()?;
            let orientation = r.vector()?;
            let scale = r.vector()?;
            let n = r.i32()?;
            if !(0..=1024).contains(&n) {
                return Err(invalid(r.at - 4, "invalid bone name extent"));
            }
            let name_bytes = r.take(n as usize)?.to_vec();
            if name_bytes.iter().any(|b| !(32..=126).contains(b)) {
                rig.limitations.push(format!(
                    "bone {index} has non-ASCII name bytes; retained losslessly"
                ));
            }
            let name = String::from_utf8_lossy(&name_bytes).into_owned();
            let opposing_index = r.i32()?;
            let parent_index = r.i32()?;
            let hash = r.u32()?;
            let flags = r.u32()?;
            if opposing_index < -1
                || opposing_index >= count as i32
                || parent_index < -1
                || parent_index >= count as i32
                || parent_index == index as i32
            {
                return Err(invalid(r.at - 16, "invalid rig hierarchy index"));
            }
            rig.bones.push(Bone {
                name,
                name_bytes,
                opposing_index,
                parent_index,
                hash,
                flags,
                position,
                orientation,
                scale,
            });
        }
        let chain_count = r.u32()? as usize;
        if chain_count > 4096 {
            return Err(invalid(r.at - 4, "rig chain count exceeds budget"));
        }
        for _ in 0..chain_count {
            let n = r.u32()? as usize;
            if n > count {
                return Err(invalid(r.at - 4, "invalid IK chain length"));
            }
            let mut indices = Vec::with_capacity(n);
            for _ in 0..n {
                indices.push(r.i32()?);
            }
            let pole = r.i32()?;
            let slot = r.i32()?;
            let root = r.i32()?;
            if indices
                .iter()
                .chain([&pole, &slot, &root])
                .any(|v| *v < -1 || *v >= count as i32)
            {
                return Err(invalid(r.at, "invalid IK chain bone index"));
            }
            rig.chains.push(IkChain {
                indices,
                pole,
                slot,
                root,
            });
        }
        rig.trailing = bytes[r.at..].to_vec();
        if !rig.trailing.is_empty() {
            rig.limitations
                .push("unknown trailing rig bytes retained; mutation unavailable".into());
        }
        rig.evaluation_order()?;
        Ok(rig)
    }

    pub fn evaluation_order(&self) -> Result<Vec<usize>> {
        if self.version != 3 || self.minor_version != 1 || self.bones.is_empty() {
            return Err(unsupported("unsupported or empty rig"));
        }
        // Iterative parent walk avoids attacker-controlled recursion depth.
        let mut state = vec![0u8; self.bones.len()];
        let mut order = Vec::with_capacity(state.len());
        for start in 0..state.len() {
            if state[start] == 2 {
                continue;
            }
            let mut pending = Vec::new();
            let mut next = Some(start);
            while let Some(index) = next {
                if state[index] == 2 {
                    break;
                }
                if state[index] == 1 {
                    return Err(invalid(index, "cyclic rig hierarchy"));
                }
                state[index] = 1;
                pending.push(index);
                let parent = self.bones[index].parent_index;
                if parent < -1 || parent >= state.len() as i32 {
                    return Err(invalid(index, "invalid rig parent"));
                }
                next = (parent >= 0).then_some(parent as usize);
            }
            for index in pending.into_iter().rev() {
                state[index] = 2;
                order.push(index);
            }
        }
        Ok(order)
    }

    pub fn bind_pose(&self) -> Result<BindPose> {
        let order = self.evaluation_order()?;
        let local = self
            .bones
            .iter()
            .map(|b| Matrix4::from_trs(b.position, b.orientation, b.scale))
            .collect::<Result<Vec<_>>>()?;
        let mut global = vec![Matrix4::IDENTITY; local.len()];
        for &index in &order {
            let parent = self.bones[index].parent_index;
            global[index] = if parent < 0 {
                local[index]
            } else {
                global[parent as usize].multiply(local[index])
            };
        }
        let inverse = global
            .iter()
            .map(|m| m.inverse_affine())
            .collect::<Result<Vec<_>>>()?;
        Ok(BindPose {
            rig_sha256: self.sha256,
            local,
            global,
            inverse,
            evaluation_order: order,
            caller_supplied_globals: false,
        })
    }

    /// Accept a previously verified source-tool bind in file bone order.
    /// This does not attest to the tool/rig selection or convert mesh coordinates.
    pub fn bind_pose_from_globals(&self, global: &[Matrix4]) -> Result<BindPose> {
        let order = self.evaluation_order()?;
        if global.len() != self.bones.len() {
            return Err(unsupported("bind bone count mismatch"));
        }
        let inverse = global
            .iter()
            .map(|m| m.inverse_affine())
            .collect::<Result<Vec<_>>>()?;
        let local = self
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if b.parent_index < 0 {
                    global[i]
                } else {
                    inverse[b.parent_index as usize].multiply(global[i])
                }
            })
            .collect();
        Ok(BindPose {
            rig_sha256: self.sha256,
            local,
            global: global.to_vec(),
            inverse,
            evaluation_order: order,
            caller_supplied_globals: true,
        })
    }

    /// Exact hash binding. Missing and duplicate hashes fail instead of choosing.
    pub fn bone_index(&self, hash: u32) -> Result<usize> {
        let mut matches = self
            .bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.hash == hash);
        let index = matches
            .next()
            .ok_or_else(|| unsupported(format!("bone hash {hash:08X} absent from rig")))?
            .0;
        if matches.next().is_some() {
            return Err(unsupported(format!("bone hash {hash:08X} is ambiguous")));
        }
        Ok(index)
    }
}
