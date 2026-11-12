// SPDX-License-Identifier: GPL-3.0-or-later
//! Controlled source-reference deformation inspection. Every applied, ineligible
//! or unresolved operation remains visible. This never sets native acceptance.
use crate::*;
use external_cas_morph::{
    apply_bgeo, apply_bond_mesh, apply_bond_rig, apply_dmap, ApplyStats, MorphData, MorphOperation,
};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug)]
pub enum ReferenceOperationState {
    Applied,
    ZeroWeight,
    IneligibleBySource,
    Unresolved(String),
}
#[derive(Clone, Debug)]
pub struct ReferenceOperationResult {
    pub operation_index: usize,
    pub key: ResourceKey,
    pub weight: Option<f32>,
    pub state: ReferenceOperationState,
    pub stats: Option<ApplyStats>,
}
#[derive(Clone, Debug)]
pub struct ReferenceMeshDeformation {
    pub part_index: usize,
    pub mesh_index: usize,
    pub key: ResourceKey,
    pub geometry: Arc<Geometry>,
    pub operations: Vec<ReferenceOperationResult>,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct ReferenceDeformation {
    pub native_sim_sha256: String,
    pub meshes: Vec<ReferenceMeshDeformation>,
    pub rig: Option<Rig>,
    pub tuning: ReferenceTuningSelection,
    pub issues: Vec<String>,
    pub vertex_operations: u64,
    pub native_qualified: bool,
}
#[derive(Clone, Debug)]
pub struct ReferenceDeformOptions {
    pub max_vertex_operations: u64,
}
impl Default for ReferenceDeformOptions {
    fn default() -> Self {
        Self {
            max_vertex_operations: 250_000_000,
        }
    }
}
/// Literal pinned source predicate, including its deliberately broad default.
pub fn reference_region_allows(body_type: u32, region: u32) -> bool {
    match region {
        9 => body_type == 60,
        31 => body_type == 61,
        _ => true,
    }
}
fn weight(operation: &MorphOperation, tuning: &ReferenceTuningSelection) -> Option<f32> {
    if let Some(w) = operation.weight {
        return Some(w);
    }
    let id = operation.modifier_key?;
    let native = operation.native_amount?;
    let scales = tuning.scales.as_ref()?;
    Some(
        native
            * scales
                .get(&id)
                .copied()
                .or(tuning.reference_default_scale)?,
    )
}
fn outcome(
    index: usize,
    operation: &MorphOperation,
    weight: Option<f32>,
    state: ReferenceOperationState,
    stats: Option<ApplyStats>,
) -> ReferenceOperationResult {
    ReferenceOperationResult {
        operation_index: index,
        key: operation.resource.key,
        weight,
        state,
        stats,
    }
}
/// Applies actual native resources to disposable runtime copies in the pinned
/// BGEO→DMAP→BOND stage order. Unsupported required inputs are never concealed.
/// Partial output is inspection evidence, not a complete Sim or a save format.
pub fn build_reference_deformation(
    plan: &CharacterPlan,
    options: &ReferenceDeformOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(CharacterProgress),
) -> Result<ReferenceDeformation> {
    check(cancel)?;
    if options.max_vertex_operations == 0 {
        return Err(CharacterError::Invalid("Zero deformation budget".into()));
    }
    if format!("{:x}", Sha256::digest(&plan.native.base.native.bytes))
        != plan.scene.native_sim_sha256
    {
        return Err(CharacterError::Invalid(
            "Character scene belongs to a different native Sim revision".into(),
        ));
    }
    let tuning = select_reference_tuning(plan);
    let mut out = ReferenceDeformation {
        native_sim_sha256: plan.scene.native_sim_sha256.clone(),
        meshes: vec![],
        rig: plan.rig.clone(),
        tuning,
        issues: vec![],
        vertex_operations: 0,
        native_qualified: false,
    };
    let Some(morph) = &plan.morph else {
        out.issues
            .push("Native morph dependency plan absent".into());
        return Ok(out);
    };
    if morph.native_sim_sha256 != plan.scene.native_sim_sha256 {
        return Err(CharacterError::Invalid(
            "Morph plan belongs to a different native Sim revision".into(),
        ));
    }
    let mut originals = vec![];
    let mut body_types = vec![];
    for (pi, part) in plan.parts.iter().enumerate() {
        for (mi, mesh) in part.meshes.iter().enumerate() {
            if let Some(geometry) = &mesh.native_rest {
                originals.push(
                    geometry
                        .vertices
                        .iter()
                        .map(|v| v.position)
                        .collect::<Vec<_>>(),
                );
                body_types.push(part.native_slot.body_type);
                let mut issues = vec![];
                if part
                    .casp
                    .as_ref()
                    .is_some_and(|c| ((c.gender as u32) << 8) & morph.frame == 0)
                {
                    issues
                        .push("Required opposite-frame clothing conversion is unqualified".into());
                }
                out.meshes.push(ReferenceMeshDeformation {
                    part_index: pi,
                    mesh_index: mi,
                    key: mesh.key,
                    geometry: geometry.clone(),
                    operations: vec![],
                    issues,
                });
            }
        }
    }
    let conversion_required = morph.gender == 0x2000
        && plan.parts.iter().any(|p| {
            [5, 6].contains(&p.native_slot.body_type)
                && p.casp
                    .as_ref()
                    .is_some_and(|c| ((c.gender as u32) << 8) == 0x1000)
        });
    let stages = ["BGEO", "DMAP", "BOND"];
    for (stage, kind) in stages.iter().enumerate() {
        for (oi, operation) in morph.operations.iter().enumerate() {
            check(cancel)?;
            let operation_stage = match operation.resource.key.type_id {
                0x067caa11 => 0,
                0xdb43e069 => 1,
                0x0355e0a6 => 2,
                _ => continue,
            };
            if operation_stage != stage {
                continue;
            }
            if matches!(&operation.data,Some(MorphData::Dmap(d))if d.kind==1) {
                continue;
            }
            progress(CharacterProgress {
                stage: Stage::Morphs,
                completed: oi,
                total: morph.operations.len(),
                detail: format!("Reference{kind} {}", operation.resource.key),
            });
            let amount = weight(operation, &out.tuning);
            let mut active_bond = false;
            for (index, mesh) in out.meshes.iter_mut().enumerate() {
                check(cancel)?;
                let mut state = None;
                if !reference_region_allows(body_types[index], operation.region) && stage == 1 {
                    state = Some(ReferenceOperationState::IneligibleBySource)
                }
                if amount == Some(0.) {
                    state = Some(ReferenceOperationState::ZeroWeight)
                } else if stage == 2 && amount.is_some_and(|w| w < 0.) {
                    state = Some(ReferenceOperationState::IneligibleBySource)
                }
                if state.is_none() && stage == 0 {
                    if mesh.geometry.vertices.iter().all(|v| v.vertex_id.is_none()) {
                        state = Some(
                            if mesh
                                .geometry
                                .vertices
                                .iter()
                                .any(|v| v.tag.is_some_and(|t| t & 0x00ff0000 != 0))
                            {
                                ReferenceOperationState::Unresolved("Native IDs absent on face-copy-tagged mesh; authoritative head-ID transfer has not been qualified".into())
                            } else {
                                ReferenceOperationState::IneligibleBySource
                            },
                        );
                    }
                }
                if state.is_none()
                    && stage == 1
                    && (mesh.geometry.vertices.iter().all(|v| v.tag.is_none())
                        || mesh.geometry.vertices.iter().all(|v| v.uvs.len() < 2))
                {
                    state = Some(ReferenceOperationState::IneligibleBySource)
                }
                if state.is_none() && amount.is_none() {
                    state = Some(ReferenceOperationState::Unresolved(
                        "Native amount/tuning scale unresolved; no weight invented".into(),
                    ))
                }
                if state.is_none() && stage == 2 && conversion_required {
                    state = Some(ReferenceOperationState::Unresolved(
                        "Required BOND-to-DMAP clothing conversion tuning is unqualified".into(),
                    ))
                }
                if state.is_none()
                    && out
                        .vertex_operations
                        .checked_add(mesh.geometry.vertices.len() as u64)
                        .is_none_or(|n| n > options.max_vertex_operations)
                {
                    state = Some(ReferenceOperationState::Unresolved(
                        "Reference deformation vertex-work budget exceeded".into(),
                    ))
                }
                if let Some(state) = state {
                    mesh.operations
                        .push(outcome(oi, operation, amount, state, None));
                    continue;
                }
                out.vertex_operations += mesh.geometry.vertices.len() as u64;
                let amount = amount.unwrap();
                let applied = match &operation.data {
                    Some(MorphData::Bgeo(b)) => apply_bgeo(
                        &mesh.geometry,
                        b,
                        plan.parts[mesh.part_index].selected_lod.unwrap_or(0) as usize,
                        amount,
                        false,
                    ),
                    Some(MorphData::Dmap(shape)) => {
                        let normals = morph
                            .operations
                            .iter()
                            .filter(|o| {
                                o.source == operation.source
                                    && o.native_amount.map(f32::to_bits)
                                        == operation.native_amount.map(f32::to_bits)
                            })
                            .find_map(|o| match &o.data {
                                Some(MorphData::Dmap(d)) if d.kind == 1 => {
                                    weight(o, &out.tuning).map(|w| (d, w))
                                }
                                _ => None,
                            });
                        apply_dmap(&mesh.geometry, &originals[index], shape, amount, normals)
                    }
                    Some(MorphData::Bond(b)) => match &out.rig {
                        Some(rig) => {
                            active_bond = true;
                            apply_bond_mesh(&mesh.geometry, rig, b, amount)
                        }
                        None => Err(external_cas_morph::MorphError::Unresolved(
                            "Native rig absent".into(),
                        )),
                    },
                    None => Err(external_cas_morph::MorphError::Unresolved(
                        operation
                            .resource
                            .error
                            .clone()
                            .unwrap_or("Morph resource decode unresolved".into()),
                    )),
                };
                match applied {
                    Ok(applied) => {
                        mesh.geometry = Arc::new(applied.geometry);
                        mesh.operations.push(outcome(
                            oi,
                            operation,
                            Some(amount),
                            ReferenceOperationState::Applied,
                            Some(applied.stats),
                        ));
                    }
                    Err(e) => mesh.operations.push(outcome(
                        oi,
                        operation,
                        Some(amount),
                        ReferenceOperationState::Unresolved(e.to_string()),
                        None,
                    )),
                }
            }
            if active_bond {
                if let (Some(MorphData::Bond(b)), Some(rig), Some(amount)) =
                    (&operation.data, &out.rig, amount)
                {
                    match apply_bond_rig(rig, b, amount) {
                        Ok(applied) => out.rig = Some(applied.rig),
                        Err(e) => out.issues.push(format!("Rig operation{oi}: {e}")),
                    }
                }
            }
        }
    }
    out.issues.extend(
        morph
            .issues
            .iter()
            .map(|i| format!("{}: {}", i.source, i.detail)),
    );
    out.issues.push("Controlled reference stages exclude unqualified head-ID transfer, automatic seam construction/matching, clothing conversions, pregnancy and final slot targets; native visual acceptance remains false".into());
    check(cancel)?;
    Ok(out)
}
