// SPDX-License-Identifier: GPL-3.0-or-later
// SimRipper LoadBONDMorph and GEOM.BoneMorpher; source-tool arithmetic.
use super::bond::{balanced, weighted, Q};
use super::*;
use external_cas_assets::{Geometry, Rig};
type M = [[f64; 4]; 4];
fn qmul(q: Q, r: Q) -> Q {
    [
        r[3] * q[0] + r[0] * q[3] - r[1] * q[2] + r[2] * q[1],
        r[3] * q[1] + r[0] * q[2] + r[1] * q[3] - r[2] * q[0],
        r[3] * q[2] - r[0] * q[1] + r[1] * q[0] + r[2] * q[3],
        r[3] * q[3] - r[0] * q[0] - r[1] * q[1] - r[2] * q[2],
    ]
}
fn conjugate(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}
fn matrix(q: Q, p: [f32; 3], s: [f32; 3]) -> M {
    let [x, y, z, w] = q;
    [
        [
            s[0] as f64 - 2.0 * y * y - 2.0 * z * z,
            2.0 * x * y - 2.0 * z * w,
            2.0 * x * z + 2.0 * y * w,
            p[0] as f64,
        ],
        [
            2.0 * x * y + 2.0 * z * w,
            s[1] as f64 - 2.0 * x * x - 2.0 * z * z,
            2.0 * y * z - 2.0 * x * w,
            p[1] as f64,
        ],
        [
            2.0 * x * z - 2.0 * y * w,
            2.0 * y * z + 2.0 * x * w,
            s[2] as f64 - 2.0 * x * x - 2.0 * y * y,
            p[2] as f64,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
fn mm(a: M, b: M) -> M {
    let mut out = [[0.0; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            for i in 0..4 {
                out[r][c] += a[r][i] * b[i][c];
            }
        }
    }
    out
}
fn mv(m: M, v: [f32; 3]) -> [f32; 3] {
    let v = [v[0] as f64, v[1] as f64, v[2] as f64, 1.0];
    let mut out = [0.0f64; 3];
    for i in 0..4 {
        for k in 0..3 {
            out[k] += m[k][i] * v[i];
        }
    }
    out.map(|v| v as f32)
}
fn magnitude(v: [f32; 3]) -> f32 {
    ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) as f64).sqrt() as f32
}
fn normal_matrix(m: M) -> Result<M> {
    let det =
        -m[0][2] * m[1][1] * m[2][0] + m[0][1] * m[1][2] * m[2][0] + m[0][2] * m[1][0] * m[2][1]
            - m[0][0] * m[1][2] * m[2][1]
            - m[0][1] * m[1][0] * m[2][2]
            + m[0][0] * m[1][1] * m[2][2];
    if !det.is_finite() || det == 0.0 {
        return Err(MorphError::Invalid("Singular BOND normal transform".into()));
    }
    let d = 1.0 / det;
    let inv = [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
        ],
    ];
    let mut result = [[0.0; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            result[r][c] = inv[c][r];
        }
    }
    result[3][3] = 1.0;
    Ok(result)
}
/// Deforms a mesh using the rig before this BOND. Position deltas accumulate
/// across adjustments; normals update in adjustment order. Inputs stay intact.
pub fn apply_bond_mesh(
    geometry: &Geometry,
    rig: &Rig,
    bond: &Bond,
    weight: f32,
) -> Result<AppliedMorph> {
    let mut result = AppliedMorph {
        geometry: geometry.clone(),
        stats: ApplyStats {
            vertices: geometry.vertices.len(),
            ..Default::default()
        },
    };
    if !weight.is_finite() {
        return Err(MorphError::Invalid("Nonfinite BOND weight".into()));
    }
    if weight == 0.0 {
        return Ok(result);
    }
    let _ = apply_bond_rig(rig, bond, weight)?;
    if geometry.palette_error.is_some() {
        return Err(MorphError::MissingMetadata(
            "BOND needs a qualified native GEOM palette".into(),
        ));
    }
    let mut globals = vec![];
    let mut rotations = vec![];
    for b in &rig.bones {
        let local = balanced(b.orientation)?;
        let m = matrix(local, b.position, b.scale);
        let (m, q) = if b.parent_index >= 0 {
            (
                mm(globals[b.parent_index as usize], m),
                qmul(rotations[b.parent_index as usize], local),
            )
        } else {
            (m, local)
        };
        globals.push(m);
        rotations.push(q);
    }
    let mut bone_indices = std::collections::BTreeMap::new();
    for (i, b) in rig.bones.iter().enumerate() {
        bone_indices.insert(b.hash, i);
    }
    let mut palette = vec![];
    for hash in &geometry.bone_palette {
        palette.push(*bone_indices.get(hash).ok_or_else(|| {
            MorphError::MissingMetadata(format!("GEOM bone {hash:08X} absent from native RIG"))
        })?);
    }
    let mut deltas = vec![[0.0f32; 3]; geometry.vertices.len()];
    let mut affected = vec![false; geometry.vertices.len()];
    for d in &bond.adjustments {
        let index = bone_indices[&d.slot_hash];
        let mut descendants = vec![false; rig.bones.len()];
        descendants[index] = true;
        for i in index + 1..rig.bones.len() {
            let parent = rig.bones[i].parent_index;
            if parent >= 0 && descendants[parent as usize] {
                descendants[i] = true;
            }
        }
        let pivot = mv(globals[index], [0.0; 3]);
        let q = if rig.bones[index].parent_index >= 0 {
            rotations[rig.bones[index].parent_index as usize].map(|v| v as f32 as f64)
        } else {
            rotations[index]
        };
        let rotation = qmul(qmul(q, balanced(d.quaternion)?), conjugate(q));
        let offset = qmul(
            qmul(
                q,
                [
                    d.offset[0] as f64,
                    d.offset[1] as f64,
                    d.offset[2] as f64,
                    0.0,
                ],
            ),
            conjugate(q),
        );
        let rm = matrix(q, [0.0; 3], [1.0; 3]);
        let mut world_scale = [0.0; 3];
        for r in 0..3 {
            let row = [
                (rm[r][0] * (d.scale[0] + 1.0) as f64) as f32,
                (rm[r][1] * (d.scale[1] + 1.0) as f64) as f32,
                (rm[r][2] * (d.scale[2] + 1.0) as f64) as f32,
            ];
            world_scale[r] = magnitude(row) - 1.0;
        }
        for (i, v) in result.geometry.vertices.iter_mut().enumerate() {
            if !v.has_skin {
                return Err(MorphError::MissingMetadata(
                    "BOND mesh requires native skin bytes".into(),
                ));
            }
            let mut bone_weight = 0.0f32;
            for k in 0..4 {
                if v.raw_weights[k] > 0 {
                    let p = *palette.get(v.joints[k] as usize).ok_or_else(|| {
                        MorphError::MissingMetadata("Native GEOM palette index out of range".into())
                    })?;
                    if descendants[p] {
                        bone_weight += v.raw_weights[k] as f32 / 255.0;
                    }
                }
            }
            if bone_weight == 0.0 {
                continue;
            }
            let amount = bone_weight * weight;
            let transform = matrix(
                weighted(rotation, amount)?,
                [
                    offset[0] as f32 * amount,
                    offset[1] as f32 * amount,
                    offset[2] as f32 * amount,
                ],
                world_scale.map(|s| s * amount + 1.0),
            );
            let before = geometry.vertices[i].position;
            let local = [
                before[0] - pivot[0],
                before[1] - pivot[1],
                before[2] - pivot[2],
            ];
            let transformed = mv(transform, local);
            for k in 0..3 {
                deltas[i][k] += (transformed[k] + pivot[k]) - before[k];
            }
            let mut n = mv(normal_matrix(transform)?, v.normal);
            let m = magnitude(n);
            if m != 0.0 {
                let inv = 1.0 / m;
                for x in &mut n {
                    *x *= inv;
                }
            }
            v.normal = n;
            affected[i] = true;
        }
    }
    for (i, v) in result.geometry.vertices.iter_mut().enumerate() {
        if affected[i] {
            for k in 0..3 {
                v.position[k] += deltas[i][k];
            }
            result.stats.matched_vertices += 1;
            result.stats.changed_positions +=
                usize::from(v.position != geometry.vertices[i].position);
            result.stats.changed_normals += usize::from(v.normal != geometry.vertices[i].normal);
        }
        if v.position
            .iter()
            .chain(v.normal.iter())
            .any(|v| !v.is_finite())
        {
            return Err(MorphError::Invalid("Nonfinite BOND mesh result".into()));
        }
    }
    Ok(result)
}
