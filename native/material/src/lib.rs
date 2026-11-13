// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-preserving native color shifts and material composition. Pixel ports
//! retain their reference-tool boundary; game-render parity requires comparison.
pub mod color;
pub mod skin;
pub mod texture;
pub mod shader;
pub mod cas_lighting;
pub mod layers;
pub mod character_atlas;
pub use character_atlas::{build_reference_atlas,build_reference_atlas_with_limits,ReferenceAtlas,AtlasLayerObservation,AtlasLimits};
pub mod character_inputs;
pub use character_inputs::{resolve_character_material_inputs,resolve_character_material_inputs_with_limits,CharacterMaterialInputs,PartMaterialInputs,MeshMaterialInputs,MaterialPlaneInput,DecodedMaterialPlane,UnresolvedGlobalBinding,MaterialInputLimits};
pub mod geom_material;
pub mod native_lookup;
pub mod native_globals;
pub mod native_cpu_skin;
pub mod native_cpu_bc1;
pub mod native_blend;
pub mod native_shadow;
pub mod native_compressed_blend;
pub mod native_rle2;
pub mod native_image_source;
pub mod native_lrle;
pub mod native_canvas;
pub mod native_bc3;
pub mod native_detail;
pub mod native_composite;
pub mod native_order;
pub mod native_physique;
pub mod native_physique_source;
mod skin_keys;
use std::{fmt,sync::atomic::{AtomicBool,Ordering}};
#[derive(Debug,Clone,PartialEq)]
pub enum Error { Invalid(&'static str), Unsupported(&'static str), Cancelled, Budget{bytes:u64,limit:u64} }
impl fmt::Display for Error{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{write!(f,"{self:?}")}}
impl std::error::Error for Error{}
pub type Result<T>=std::result::Result<T,Error>;
pub(crate) fn check(cancel:&AtomicBool)->Result<()>{if cancel.load(Ordering::Relaxed){Err(Error::Cancelled)}else{Ok(())}}
#[derive(Clone,Debug)]
pub struct Pixels{pub width:u32,pub height:u32,pub rgba:Vec<u8>}
impl Pixels{
    pub fn new(width:u32,height:u32,rgba:Vec<u8>,limit:u64)->Result<Self>{
        let bytes=(width as u64).checked_mul(height as u64).and_then(|n|n.checked_mul(4)).ok_or(Error::Invalid("Image dimensions overflow"))?;
        if width==0||height==0||rgba.len()as u64!=bytes{return Err(Error::Invalid("Image extent"));}
        if bytes>limit{return Err(Error::Budget{bytes,limit});}
        Ok(Self{width,height,rgba})
    }
}
