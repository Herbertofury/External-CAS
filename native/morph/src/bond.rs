// SPDX-License-Identifier: GPL-3.0-or-later
// RIG.BoneMorpher/ScaleBone and Geometry.Quaternion arithmetic from pinned
// SimRipper. This updates a runtime rig copy; it is not mesh skin deformation.
use super::*;
use external_cas_assets::Rig;
use std::collections::BTreeMap;
pub(crate) type Q = [f64; 4];
pub(crate) fn balanced(input: [f32; 4]) -> Result<Q> {
    let mut q = input.map(f64::from);
    if q[0] == 0.0 && q[1] == 0.0 && q[2] == 0.0 && (q[3] == 0.0 || q[3] == 1.0) {
        return Ok([0.0, 0.0, 0.0, 1.0]);
    }
    let magnitude = q.iter().map(|v| v * v).sum::<f64>();
    if (magnitude - 1.0).abs() > 0.000001 {
        let m = q[0] * q[0] - q[1] * q[1] - q[2] * q[2];
        if m <= 1.0 {
            q[3] = (1.0 - m).sqrt();
        } else {
            let n = magnitude.sqrt();
            if n == 0.0 {
                return Err(MorphError::Invalid("Zero quaternion magnitude".into()));
            }
            q = q.map(|v| (v / n) as f32 as f64);
        }
    }
    if q.iter().any(|v| !v.is_finite()) {
        return Err(MorphError::Invalid(
            "Nonfinite balanced BOND quaternion".into(),
        ));
    }
    Ok(q)
}
pub(crate) fn weighted(q: Q, w: f32) -> Result<Q> {
    let q = [q[0] * w as f64, q[1] * w as f64, q[2] * w as f64, q[3]];
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !n.is_finite() || n == 0.0 {
        return Err(MorphError::Invalid(
            "Invalid weighted BOND quaternion".into(),
        ));
    }
    Ok(q.map(|v| (v / n) as f32 as f64))
}
fn mul(q: Q, r: Q) -> [f32; 4] {
    [
        (r[3] * q[0] + r[0] * q[3] - r[1] * q[2] + r[2] * q[1]) as f32,
        (r[3] * q[1] + r[0] * q[2] + r[1] * q[3] - r[2] * q[0]) as f32,
        (r[3] * q[2] - r[0] * q[1] + r[1] * q[0] + r[2] * q[3]) as f32,
        (r[3] * q[3] - r[0] * q[0] - r[1] * q[1] - r[2] * q[2]) as f32,
    ]
}
#[derive(Clone, Debug)]
pub struct AppliedRig {
    pub rig: Rig,
    pub adjustments: usize,
    pub descendant_position_updates: usize,
}
/// Rig-local part only. The caller must separately perform qualified BOND mesh
/// deformation; replacing a rig alone cannot stand in for that operation.
pub fn apply_bond_rig(original: &Rig, bond: &Bond, weight: f32) -> Result<AppliedRig> {
    if !weight.is_finite() {
        return Err(MorphError::Invalid("Nonfinite BOND weight".into()));
    }
    if !bond.unparsed_suffix.is_empty() {
        return Err(MorphError::Unsupported(
            "BOND opaque suffix retained".into(),
        ));
    }
    let mut by_hash = BTreeMap::new();
    for (i, b) in original.bones.iter().enumerate() {
        if by_hash.insert(b.hash, i).is_some() {
            return Err(MorphError::Unresolved("Duplicate RIG bone hash".into()));
        }
        if b.parent_index >= i as i32 || b.parent_index < -1 {
            return Err(MorphError::Unsupported(
                "BOND source requires parent-before-child RIG order".into(),
            ));
        }
        if b.position
            .iter()
            .chain(b.orientation.iter())
            .chain(b.scale.iter())
            .any(|v| !v.is_finite())
        {
            return Err(MorphError::Invalid("Nonfinite native RIG transform".into()));
        }
    }
    let mut result = AppliedRig {
        rig: original.clone(),
        adjustments: 0,
        descendant_position_updates: 0,
    };
    let mut local = original
        .bones
        .iter()
        .map(|b| balanced(b.orientation))
        .collect::<Result<Vec<_>>>()?;
    if weight != 0.0 {
        for d in &bond.adjustments {
            let index = *by_hash.get(&d.slot_hash).ok_or_else(|| {
                MorphError::MissingMetadata(format!(
                    "BOND bone {:08X} absent from native RIG",
                    d.slot_hash
                ))
            })?;
            let rotation = weighted(balanced(d.quaternion)?, weight)?;
            let bone = &mut result.rig.bones[index];
            for k in 0..3 {
                bone.position[k] += d.offset[k] * weight;
            }
            bone.orientation = mul(local[index], rotation);
            let mut descendants = vec![false; result.rig.bones.len()];
            for i in index + 1..result.rig.bones.len() {
                let parent = result.rig.bones[i].parent_index;
                descendants[i] =
                    parent == index as i32 || (parent >= 0 && descendants[parent as usize]);
                if descendants[i] {
                    for k in 0..3 {
                        result.rig.bones[i].position[k] *= d.scale[k] * weight + 1.0;
                    }
                    result.descendant_position_updates += 1;
                }
            }
            for (i, l) in local.iter_mut().enumerate().skip(index) {
                *l = balanced(result.rig.bones[i].orientation)?;
            }
            result.adjustments += 1;
        }
    }
    for (b, l) in result.rig.bones.iter_mut().zip(local) {
        b.orientation = l.map(|v| v as f32);
        if b.position
            .iter()
            .chain(b.orientation.iter())
            .any(|v| !v.is_finite())
        {
            return Err(MorphError::Invalid("Nonfinite BOND rig result".into()));
        }
    }
    Ok(result)
}
