// SPDX-License-Identifier: GPL-3.0-or-later
//! Native fixed-resource and generated lookup contracts. The ambient context
//! is the same explicit native species value written by the CAS model builder.
use crate::{character_inputs::{decode_source,DecodedMaterialPlane},native_lookup::{generate_native_skin_detail_lookup,generate_native_renderer_default_lookup,NativeSkinDetailLookup}};
use external_cas_assets::{Catalog,ReadLimits,ResourceKey};
use external_cas_character::{CharacterPlan,ResourceObservation,ResourceResolver};
use external_cas_scene::ScopePolicy;
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,sync::{Arc,atomic::{AtomicBool,Ordering}}};
#[derive(Clone,Debug)]
pub struct NativeAmbientInput {pub context:u32,pub image:Arc<DecodedMaterialPlane>}
#[derive(Clone,Debug,Default)]
pub struct NativeGlobalMaterialInputs {
    pub ambient:Option<NativeAmbientInput>,
    pub tone_lookup:Option<NativeToneLookup>,
    pub issues:Vec<String>,
}
#[derive(Clone,Debug)]
pub struct NativeToneLookup {
    pub source:Arc<ResourceObservation>,
    pub lookup:NativeSkinDetailLookup,
    /// Proven texture creation format, DXGI_FORMAT_B8G8R8A8_UNORM.
    pub dxgi_resource_format:u32,
    pub selection:NativeToneLookupSelection,
    pub issues:Vec<String>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum NativeToneLookupSelection {GeneratedPackedTone,InitializedRendererDefaultGrayscale}
/// Actual initialized-CAS renderer branch: zero selects its default-table
/// resource, nonzero runs the explicit native packed-HueSat generator.
pub fn native_tone_lookup_for_initialized_renderer(packed:u32,cancel:&AtomicBool)->Result<(NativeSkinDetailLookup,NativeToneLookupSelection),String>{
    let selection=if packed==0{NativeToneLookupSelection::InitializedRendererDefaultGrayscale}else{NativeToneLookupSelection::GeneratedPackedTone};
    let lookup=if packed==0{generate_native_renderer_default_lookup(cancel)}else{generate_native_skin_detail_lookup(packed,cancel)}.map_err(|e|e.to_string())?;
    Ok((lookup,selection))
}
/// Exact integer getter contract, not an inferred species/age default.
pub fn native_ambient_key(context:u32)->Option<ResourceKey>{
    let instance=match context{1=>0x523358fb929bbd37,2|4=>0xf63401c193ac863e,3=>0xf9d2e9e815c80c9b,5=>0xc399c6e9bac26732,6=>0xf04f38c823ab66c2,_=>return None};
    Some(ResourceKey::new(0x00b2d882,0,instance))
}
pub fn resolve_native_ambient_context<'a>(context:u32,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool)->Result<NativeAmbientInput,String>{
    resolve_native_ambient_context_with_budget(context,catalog,policy,cancel,64*1024*1024,256*1024*1024)
}
pub(crate) fn resolve_native_ambient_context_with_budget<'a>(context:u32,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool,source_budget:u64,plane_budget:u64)->Result<NativeAmbientInput,String>{
    if cancel.load(Ordering::Relaxed){return Err("Native ambient resolution cancelled".into())}
    let key=native_ambient_key(context).ok_or("Native ambient getter returned an empty key for the explicit context")?;
    let choices=BTreeMap::new();let mut resolver=ResourceResolver::new(catalog,policy,&choices,ReadLimits{max_resource_bytes:source_budget},source_budget,cancel);
    let source=resolver.get(key);let image=Arc::new(decode_source(source,plane_budget,cancel)?);Ok(NativeAmbientInput{context,image})
}
/// The native CAS setup writes its model record's species into the remapper's
/// context before calling the fixed-resource getter. Missing/zero/unknown
/// serialized values remain errors instead of adopting a human default.
pub fn native_ambient_context(plan:&CharacterPlan)->Result<u32,String>{
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("Native ambient belongs to a stale Sim revision".into())}
    let species=plan.native.extended_species.ok_or("Explicit native ExtendedSpecies absent; no ambient context default selected")?;
    if native_ambient_key(species).is_none(){return Err("Unknown native species ambient context; original state retained".into())}Ok(species)
}
pub fn resolve_native_ambient<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool)->Result<NativeAmbientInput,String>{
    resolve_native_ambient_context(native_ambient_context(plan)?,catalog,policy,cancel)
}
/// Current v12 source dataflow is proven: serialized saturation:u16 followed
/// by hue:u16 is read as one native little-endian u32, then copied unchanged
/// through the compositor into the exact generated lookup. Zero selects the
/// proven initialized-renderer default-table grayscale construction.
pub fn resolve_native_tone_lookup<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool)->Result<NativeToneLookup,String>{
    if cancel.load(Ordering::Relaxed){return Err("Native tone lookup cancelled".into())}
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("Native lookup belongs to a stale Sim revision".into())}
    let planned=plan.scene.tone.as_ref().ok_or("Native TONE absent")?;let tone=planned.parsed.as_ref().ok_or("Native TONE not parsed")?;
    if tone.version!=12{return Err("Native lookup scene integration is currently qualified only for TONE v12; original source retained".into())}
    let packed=((tone.hue as u32)<<16)|tone.saturation as u32;
    let selected=planned.resource.selected.as_ref().ok_or("Native TONE provider unresolved")?;
    let choices=BTreeMap::from([(selected.key,selected.provider)]);let mut resolver=ResourceResolver::new(catalog,policy,&choices,ReadLimits{max_resource_bytes:1024*1024},1024*1024,cancel);let source=resolver.get(selected.key);
    if source.bytes.as_deref()!=planned.raw.as_deref()||source.bytes.as_deref()!=Some(tone.raw.as_slice()){return Err("Native TONE lookup source changed or differs from parsed plan".into())}
    let(lookup,selection)=native_tone_lookup_for_initialized_renderer(packed,cancel)?;
    Ok(NativeToneLookup{source,lookup,selection,dxgi_resource_format:87,issues:vec!["Resource format and generated bytes are qualified; live sampler/permutation selection and complete native composite remain separate gates".into()]})
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn exact_native_context_mapping_has_no_default_or_species_guess(){
        assert!(native_ambient_key(0).is_none());assert!(native_ambient_key(7).is_none());assert!(native_ambient_key(u32::MAX).is_none());
        assert_eq!(native_ambient_key(1),Some(ResourceKey::new(0xb2d882,0,0x523358fb929bbd37)));assert_eq!(native_ambient_key(2),native_ambient_key(4));assert_ne!(native_ambient_key(2),native_ambient_key(3));
        assert!(resolve_native_ambient_context(1,&Catalog::default(),None,&AtomicBool::new(true)).unwrap_err().contains("cancelled"));assert!(resolve_native_ambient_context(1,&Catalog::default(),None,&AtomicBool::new(false)).is_err());
    }
    #[test]fn initialized_renderer_selects_true_zero_default_without_changing_nonzero(){
        let cancel=AtomicBool::new(false);let(zero,selection)=native_tone_lookup_for_initialized_renderer(0,&cancel).unwrap();assert_eq!(selection,NativeToneLookupSelection::InitializedRendererDefaultGrayscale);assert_eq!(&zero.bgra[1020..],[255;4]);
        let(nonzero,selection)=native_tone_lookup_for_initialized_renderer(0x000a0014,&cancel).unwrap();assert_eq!(selection,NativeToneLookupSelection::GeneratedPackedTone);assert_eq!(nonzero.bgra,generate_native_skin_detail_lookup(0x000a0014,&cancel).unwrap().bgra);
        assert!(native_tone_lookup_for_initialized_renderer(0,&AtomicBool::new(true)).is_err());
    }
}
