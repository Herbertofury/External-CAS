//! Rust-owned native morphology. Complete applicability is an explicit gate;
//! missing resources and unsupported adjustments are never silently discarded.
use std::fmt;
#[derive(Clone, Debug, PartialEq)]
pub enum MorphError {
    Cancelled,
    Truncated,
    Invalid(String),
    Unsupported(String),
    MissingMetadata(String),
    Unresolved(String),
}
impl fmt::Display for MorphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MorphError {}
pub type Result<T> = std::result::Result<T, MorphError>;
mod bgeo;
mod bond;
mod bond_mesh;
mod bytes;
mod dmap;
mod links;
mod plan;
pub use bgeo::{apply_bgeo, Bgeo, BgeoBlend, BgeoLod};
pub use bond::{apply_bond_rig, AppliedRig};
pub use bond_mesh::apply_bond_mesh;
pub use dmap::{apply_dmap, Dmap};
pub use links::{Bond, BoneAdjustment, Sculpt, SimModifier};
pub use plan::{
    build_morph_plan, build_morph_plan_cancellable, native_frame_gender, physique_prefix,
    ModifierScales, MorphData, MorphIssue, MorphOperation, MorphOptions, MorphPlan, MorphResource,
    PhysiqueWeight,
};
#[derive(Clone, Debug, Default)]
pub struct ApplyStats {
    pub vertices: usize,
    pub matched_vertices: usize,
    pub changed_positions: usize,
    pub changed_normals: usize,
    pub outside_map: usize,
}
#[derive(Clone, Debug)]
pub struct AppliedMorph {
    pub geometry: external_cas_assets::Geometry,
    pub stats: ApplyStats,
}
