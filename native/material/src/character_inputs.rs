// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-preserving material inputs. Decoding an authored CASP image does not
//! qualify its engine composite, sampler, SRV space or native shader binding.
use crate::{Pixels,texture};
use external_cas_assets::{Catalog,ReadLimits,ResourceKey};
use external_cas_character::{CharacterPlan,PartSelection,ResourceObservation,ResourceResolver};
use external_cas_scene::ScopePolicy;
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},sync::{Arc,atomic::{AtomicBool,Ordering}}};

#[derive(Clone,Debug)]
pub struct MaterialInputLimits {
    pub max_source_bytes:u64,
    pub max_decoded_bytes:u64,
    pub max_planes:usize,
}
impl Default for MaterialInputLimits {fn default()->Self{Self{max_source_bytes:512*1024*1024,max_decoded_bytes:512*1024*1024,max_planes:1024}}}
#[derive(Clone,Debug)]
pub struct DecodedMaterialPlane {
    pub source:Arc<ResourceObservation>,
    /// Mip zero, encoded channel values. Normal blue/alpha stay unchanged.
    pub pixels:Pixels,
    pub warnings:Vec<String>,
    /// Original RLES bytes remain in source; these auxiliary streams are not
    /// reconstructed from the displayable RGBA image.
    pub uninterpreted_auxiliary_streams:bool,
}
#[derive(Clone,Debug)]
pub struct MaterialPlaneInput {
    pub role:String,
    pub declared_key:ResourceKey,
    pub source:Arc<ResourceObservation>,
    pub decoded:Option<Arc<DecodedMaterialPlane>>,
    pub selection_rule:String,
    pub issues:Vec<String>,
}
#[derive(Clone,Debug)]
pub struct PartMaterialInputs {
    pub part_index:usize,
    pub body_type:u32,
    pub texture_space:Option<u8>,
    pub composition_method:Option<u8>,
    pub sort_layer:Option<i32>,
    pub packed_color_shift:Option<u64>,
    pub mesh_shader_hashes:Vec<u32>,
    pub meshes:Vec<MeshMaterialInputs>,
    pub source:Option<Arc<ResourceObservation>>,
    pub excluded_by_reference:bool,
    pub planes:Vec<MaterialPlaneInput>,
    pub issues:Vec<String>,
}
#[derive(Clone,Debug)]
pub struct MeshMaterialInputs {
    pub mesh_index:usize,
    pub key:ResourceKey,
    pub shader_hash:Option<u32>,
    pub source:Arc<ResourceObservation>,
    pub material:Option<Arc<crate::geom_material::GeomMaterial>>,
    pub reference_normal_key:Option<ResourceKey>,
    pub reference_emission_key:Option<ResourceKey>,
    pub issues:Vec<String>,
}
#[derive(Clone,Debug)]
pub struct UnresolvedGlobalBinding {
    pub parameter_name:&'static str,
    pub parameter_hash:u32,
    pub sampling_contract:&'static str,
    pub reason:String,
}
#[derive(Clone,Debug)]
pub struct CharacterMaterialInputs {
    pub parts:Vec<PartMaterialInputs>,
    pub unresolved_global_bindings:Vec<UnresolvedGlobalBinding>,
    pub source_pins:Vec<Arc<ResourceObservation>>,
    pub decoded_bytes:u64,
    pub issues:Vec<String>,
    pub native_qualified:bool,
}
fn check(cancel:&AtomicBool)->Result<(),String>{if cancel.load(Ordering::Relaxed){Err("Material input resolution cancelled".into())}else{Ok(())}}
fn globals()->Vec<UnresolvedGlobalBinding>{vec![
    UnresolvedGlobalBinding{parameter_name:"SkinDetailHSL",parameter_hash:0x3c410f96,sampling_contract:"CAS PS3038/3049: (OverlayTexture.GREEN, 0)",reason:"Installed engine construction/resource selector has not been qualified; no substitute LUT is created".into()},
    UnresolvedGlobalBinding{parameter_name:"CASAmbientIntensityTexture",parameter_hash:0xe02df0f4,sampling_contract:"CAS PS3038/3049: source UV1; GREEN rim scale, ALPHA ambient interpolation",reason:"Installed engine resource selector has not been qualified; no constant or replacement ambient plane is created".into()},
]}
fn extent(raw:&[u8])->Result<(u32,u32),String>{
    let u16at=|at|->Result<u32,String>{Ok(u16::from_le_bytes(raw.get(at..at+2).ok_or("Truncated image extent")?.try_into().unwrap())as u32)};
    let u32at=|at|->Result<u32,String>{Ok(u32::from_le_bytes(raw.get(at..at+4).ok_or("Truncated image extent")?.try_into().unwrap()))};
    let dimensions=if raw.get(..4)==Some(b"LRLE") {(u16at(8)?,u16at(10)?)}
        else if raw.get(..4)==Some(b"DXT5")&&matches!(raw.get(4..8),Some(b"RLE2"|b"RLES")){(u16at(8)?,u16at(10)?)}
        else if raw.get(..4)==Some(b"DDS "){(u32at(16)?,u32at(12)?)}
        else {image::ImageReader::new(std::io::Cursor::new(raw)).with_guessed_format().map_err(|e|e.to_string())?.into_dimensions().map_err(|e|e.to_string())?};
    if dimensions.0==0||dimensions.1==0{return Err("Zero image extent".into())}Ok(dimensions)
}
fn decode_source(source:Arc<ResourceObservation>,remaining:u64,cancel:&AtomicBool)->Result<DecodedMaterialPlane,String>{
    check(cancel)?;let raw=source.bytes.as_ref().ok_or_else(||source.error.clone().unwrap_or("Material image unresolved".into()))?;
    let (width,height)=extent(raw)?;let bytes=(width as u64).checked_mul(height as u64).and_then(|n|n.checked_mul(4)).ok_or("Material image extent overflow")?;
    if bytes>remaining{return Err(format!("Material decoded-plane budget: requires{bytes}, remaining{remaining}"))}
    let auxiliary=raw.get(4..8)==Some(b"RLES");
    let (w,h,rgba,warnings)=if source.key.type_id==0x2bc04edf {
        let image=external_cas_assets::lrle::decode_lrle_cancellable(raw,ReadLimits{max_resource_bytes:remaining.max(raw.len()as u64)},cancel).map_err(|e|e.to_string())?;
        (image.width,image.height,image.rgba,vec![])
    }else{let image=texture::decode(raw)?;(image.width,image.height,image.rgba,image.warnings)};
    if (w,h)!=(width,height){return Err("Decoded image dimensions differ from preflight".into())}
    check(cancel)?;Ok(DecodedMaterialPlane{source,pixels:Pixels::new(w,h,rgba,remaining).map_err(|e|e.to_string())?,warnings,uninterpreted_auxiliary_streams:auxiliary})
}
pub fn resolve_character_material_inputs<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool,progress:impl FnMut(String))->Result<CharacterMaterialInputs,String>{
    resolve_character_material_inputs_with_limits(plan,catalog,policy,cancel,progress,&MaterialInputLimits::default())
}
/// Collects all declared part-image diagnostics instead of stopping at the first
/// missing image. Only cancellation, invalid budgets and stale native state
/// abort the observation. Original images and packed color shifts are not edited.
pub fn resolve_character_material_inputs_with_limits<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool,mut progress:impl FnMut(String),limits:&MaterialInputLimits)->Result<CharacterMaterialInputs,String>{
    check(cancel)?;if limits.max_source_bytes==0||limits.max_decoded_bytes==0||limits.max_planes==0{return Err("Material input limits are zero".into())}
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("Material inputs belong to a stale native Sim revision".into())}
    let mut choices=BTreeMap::new();
    for source in plan.parts.iter().filter_map(|p|p.source.as_ref()).chain(plan.parts.iter().flat_map(|p|&p.materials).filter_map(|m|m.selected.as_ref())){if let Some(provider)=source.selected{choices.insert(source.key,provider);}}
    let mut resolver=ResourceResolver::new(catalog,policy,&choices,ReadLimits{max_resource_bytes:limits.max_source_bytes},limits.max_source_bytes,cancel);
    let mut output=CharacterMaterialInputs{parts:vec![],unresolved_global_bindings:globals(),source_pins:vec![],decoded_bytes:0,issues:vec!["Authored CASP images remain separate; native composite construction, per-mesh MTNF overrides, SRV/sampler spaces and shader permutation selection remain unqualified".into()],native_qualified:false};
    let mut seen=BTreeSet::new();let mut decoded:BTreeMap<ResourceKey,Result<Arc<DecodedMaterialPlane>,String>>=BTreeMap::new();let mut count=0;
    for (part_index,part) in plan.parts.iter().enumerate(){
        check(cancel)?;progress(format!("Resolving actual CASP material inputs {}/{}",part_index+1,plan.parts.len()));
        let casp=part.casp.as_ref();let hashes=part.meshes.iter().filter_map(|m|m.native_rest.as_ref().map(|g|g.shader)).collect::<BTreeSet<_>>().into_iter().collect();
        let mut current=PartMaterialInputs{part_index,body_type:part.native_slot.body_type,texture_space:casp.map(|p|p.texture_space),composition_method:casp.map(|p|p.composition_method),sort_layer:casp.map(|p|p.sort_layer),packed_color_shift:part.native_slot.color_shift,mesh_shader_hashes:hashes,meshes:vec![],source:part.source.clone(),excluded_by_reference:matches!(part.reference_selection,PartSelection::ExcludedByReference{..}),planes:vec![],issues:vec![]};
        if let Some(source)=&part.source{if seen.insert(source.key){output.source_pins.push(source.clone())}}
        for (mesh_index,mesh) in part.meshes.iter().enumerate(){
            if seen.insert(mesh.key){output.source_pins.push(mesh.source.clone())}
            let mut observed=MeshMaterialInputs{mesh_index,key:mesh.key,shader_hash:mesh.native_rest.as_ref().map(|g|g.shader),source:mesh.source.clone(),material:None,reference_normal_key:None,reference_emission_key:None,issues:vec![]};
            if let Some(geometry)=&mesh.native_rest {
                if mesh.source.bytes.as_deref()!=Some(geometry.original.as_slice()){observed.issues.push("GEOM parsed bytes differ from immutable source pin; material parse refused".into())}
                else if !geometry.material.is_empty(){match crate::geom_material::GeomMaterial::parse(&geometry.material){
                    Ok(material)=>{
                        observed.issues.extend(material.issues.iter().cloned());
                        for (hash,target) in [(0x6e56548a,&mut observed.reference_normal_key),(0xf303d152,&mut observed.reference_emission_key)]{match material.reference_texture_key(hash,&geometry.references){Ok(key)=>*target=key,Err(e)=>observed.issues.push(e)}}
                        if observed.reference_normal_key.is_some()&&observed.reference_normal_key!=casp.and_then(|c|c.normal){observed.issues.push("Source-tool GEOM normal reference differs from declared CASP normal; native precedence is unqualified".into())}
                        observed.material=Some(Arc::new(material));
                    },Err(error)=>observed.issues.push(error),
                }}
            }else{observed.issues.push("Native GEOM unavailable; no material defaults synthesized".into())}
            current.meshes.push(observed);
        }
        let Some(casp)=casp else{current.issues.push("Native CASP unresolved; no replacement material inputs selected".into());output.parts.push(current);continue};
        for reference in casp.roles.iter().filter(|r|["diffuse","shadow","normal","specular","emission","color-shift-mask","new-image"].contains(&r.role)&&!r.key.is_empty()){
            check(cancel)?;count+=1;
            let dependency=part.materials.iter().find(|m|m.role==reference.role&&m.declared_key==reference.key);
            let expected=dependency.and_then(|m|m.selected.as_ref());let key=expected.map(|s|s.key).unwrap_or(reference.key);let source=resolver.get(key);
            if seen.insert(key){output.source_pins.push(source.clone())}
            let mut entry=MaterialPlaneInput{role:reference.role.into(),declared_key:reference.key,source:source.clone(),decoded:None,selection_rule:dependency.map(|d|d.selection_rule.clone()).unwrap_or("declared full TGI; unknown roles remain uninterpreted".into()),issues:vec![]};
            let changed=expected.is_some_and(|old|old.payload_sha256!=source.payload_sha256||old.bytes.as_deref()!=source.bytes.as_deref());
            if changed{entry.issues.push("Material source differs from the immutable character plan; no changed-source image accepted".into())}
            else if count>limits.max_planes{entry.issues.push("Declared material-plane count exceeds budget; observation retained without decode".into())}
            else {
                if !decoded.contains_key(&key){let result=decode_source(source,limits.max_decoded_bytes.saturating_sub(output.decoded_bytes),cancel).map(Arc::new);if let Ok(image)=&result{output.decoded_bytes+=image.pixels.rgba.len()as u64;}decoded.insert(key,result);}
                match &decoded[&key]{Ok(image)=>{entry.issues.extend(image.warnings.iter().cloned());entry.decoded=Some(image.clone());},Err(error)=>entry.issues.push(error.clone())}
            }
            current.planes.push(entry);
        }
        output.parts.push(current);
    }
    check(cancel)?;Ok(output)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_plane_extent_preflight_has_no_channel_reinterpretation(){
        let mut dds=vec![0;128];dds[..4].copy_from_slice(b"DDS ");dds[12..16].copy_from_slice(&2048u32.to_le_bytes());dds[16..20].copy_from_slice(&1024u32.to_le_bytes());assert_eq!(extent(&dds).unwrap(),(1024,2048));
        for size in 0..20{assert!(extent(&dds[..size]).is_err());}
        let mut rle=vec![0;12];rle[..8].copy_from_slice(b"DXT5RLES");rle[8..10].copy_from_slice(&8u16.to_le_bytes());rle[10..12].copy_from_slice(&4u16.to_le_bytes());assert_eq!(extent(&rle).unwrap(),(8,4));
        let bindings=globals();assert_eq!(bindings.len(),2);assert_eq!(bindings[0].parameter_hash,0x3c410f96);assert_eq!(bindings[1].parameter_hash,0xe02df0f4);
    }
    #[test]fn unresolved_actual_inputs_remain_diagnostics_without_authoring_images(){
        let native=serde_json::from_value(serde_json::json!({"sim_index":0,"sim_id":1,"base":{"outfits":[],"native":{"bytes":[],"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","fields":[]}},"occult_forms":[]})).unwrap();
        let catalog=Catalog::default();let mut plan=external_cas_character::assemble_character(&native,&catalog,None,&Default::default()).unwrap();
        let inputs=resolve_character_material_inputs(&plan,&catalog,None,&AtomicBool::new(false),|_|{}).unwrap();assert!(inputs.parts.is_empty());assert_eq!(inputs.decoded_bytes,0);assert_eq!(inputs.unresolved_global_bindings.len(),2);assert!(!inputs.native_qualified);
        assert!(resolve_character_material_inputs(&plan,&catalog,None,&AtomicBool::new(true),|_|{}).is_err());plan.scene.native_sim_sha256="stale".into();assert!(resolve_character_material_inputs(&plan,&catalog,None,&AtomicBool::new(false),|_|{}).unwrap_err().contains("stale"));
    }
    #[test]fn decoding_preserves_blue_alpha_source_pin_and_checks_memory_before_allocation(){
        let rgba=vec![5,6,132,255,200,100,33,17];let mut raw=Vec::new();image::write_buffer_with_format(&mut std::io::Cursor::new(&mut raw),&rgba,1,2,image::ColorType::Rgba8,image::ImageFormat::Png).unwrap();
        let source=Arc::new(ResourceObservation{key:ResourceKey::new(0x00b2d882,0,1),candidates:vec![],selected:None,selection:"fabricated test image".into(),effective_policy:None,payload_sha256:Some(Sha256::digest(&raw).into()),bytes:Some(Arc::from(raw.clone())),error:None});
        assert!(decode_source(source.clone(),7,&AtomicBool::new(false)).unwrap_err().contains("budget"));
        let image=decode_source(source.clone(),8,&AtomicBool::new(false)).unwrap();assert_eq!(image.pixels.rgba,rgba);assert!(Arc::ptr_eq(&image.source,&source));assert_eq!(source.bytes.as_deref(),Some(raw.as_slice()));assert!(decode_source(source,8,&AtomicBool::new(true)).is_err());
    }
}
