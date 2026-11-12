// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual native-input atlas inspection. No embedded reference-tool mouth,
//! default skin or replacement pixel plane is accepted as user content.
use crate::{Pixels,color::{PackedColorShift,Axis},layers::over_in_place,skin::{human_detail_layers,soft_light_reference}};
use external_cas_assets::{Catalog,ReadLimits,ResourceKey};
use external_cas_character::{CharacterPlan,PartSelection,ResourceObservation,ResourceResolver};
use external_cas_scene::{FormSelection,ScopePolicy};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},sync::{Arc,atomic::{AtomicBool,Ordering}}};
#[derive(Clone,Debug)]
pub struct AtlasLayerObservation {
    pub role:String,pub key:ResourceKey,pub part_index:Option<usize>,pub sort_layer:Option<i32>,
    pub composition_method:Option<u8>,pub packed_color_shift:Option<u64>,
    pub width:Option<u32>,pub height:Option<u32>,pub opacity:f32,pub applied:bool,pub issues:Vec<String>,
}
#[derive(Clone,Debug)]
pub struct ReferenceAtlas {
    pub image:Pixels,pub source_pins:Vec<Arc<ResourceObservation>>,pub layers:Vec<AtlasLayerObservation>,
    pub issues:Vec<String>,pub native_qualified:bool,
}
#[derive(Clone,Debug)]
pub struct AtlasLimits {pub max_resource_bytes:u64,pub max_plane_bytes:u64,pub max_layers:usize}
impl Default for AtlasLimits {fn default()->Self{Self{max_resource_bytes:512*1024*1024,max_plane_bytes:256*1024*1024,max_layers:1024}}}
fn check(cancel:&AtomicBool)->std::result::Result<(),String>{if cancel.load(Ordering::Relaxed){Err("Atlas cancelled".into())}else{Ok(())}}
fn empty(width:u32,height:u32,limit:u64)->std::result::Result<Pixels,String>{
    let size=(width as u64).checked_mul(height as u64).and_then(|n|n.checked_mul(4)).ok_or("Atlas extent overflow")?;
    if size>limit{return Err("Atlas plane budget exceeded".into())}
    Pixels::new(width,height,vec![0;size as usize],limit).map_err(|e|e.to_string())
}
fn neutral()->PackedColorShift{PackedColorShift(0x4000000000000000)}
struct Inputs<'a,'b> {
    resolver:ResourceResolver<'a,'b>,pins:Vec<Arc<ResourceObservation>>,seen:BTreeSet<ResourceKey>,limit:u64,
}
impl<'a,'b> Inputs<'a,'b> {
    fn pin(&mut self,source:Arc<ResourceObservation>){if self.seen.insert(source.key){self.pins.push(source)}}
    fn texture(&mut self,key:ResourceKey,family:bool,cancel:&AtomicBool)->std::result::Result<(Pixels,Arc<ResourceObservation>,Vec<String>),String>{
        check(cancel)?;
        let keys=if family{vec![ResourceKey::new(0x2bc04edf,key.group_id,key.instance_id),ResourceKey::new(0x3453cf95,key.group_id,key.instance_id),ResourceKey::new(0x00b2d882,key.group_id,key.instance_id)]}else{vec![key]};
        let mut failures=vec![];
        for key in keys {
            let source=self.resolver.get(key);self.pin(source.clone());
            let Some(raw)=&source.bytes else{failures.push(format!("{key}: {}",source.error.as_deref().unwrap_or("unresolved")));continue};
            let decoded=if key.type_id==0x2bc04edf {
                external_cas_assets::lrle::decode_lrle_cancellable(raw,ReadLimits{max_resource_bytes:self.limit},cancel).map(|i|(i.width,i.height,i.rgba,vec![])).map_err(|e|e.to_string())
            }else{crate::texture::decode(raw).map(|i|(i.width,i.height,i.rgba,i.warnings))};
            match decoded {
                Ok((w,h,rgba,warnings))=>return Ok((Pixels::new(w,h,rgba,self.limit).map_err(|e|e.to_string())?,source,warnings)),
                Err(e)=>return Err(format!("Selected texture {key} decode failed: {e}")),
            }
        }
        Err(format!("Texture family unresolved: {}",failures.join("; ")))
    }
}
fn over(base:&mut Pixels,layer:&Pixels,opacity:f32,shift:PackedColorShift,cancel:&AtomicBool)->std::result::Result<(),String>{over_in_place(base,layer,opacity,shift,cancel).map_err(|e|e.to_string())}
fn observation(role:&str,key:ResourceKey,opacity:f32)->AtlasLayerObservation{AtlasLayerObservation{role:role.into(),key,part_index:None,sort_layer:None,composition_method:None,packed_color_shift:None,width:None,height:None,opacity,applied:false,issues:vec![]}}
fn add_layer(base:&mut Pixels,input:&mut Inputs<'_,'_>,record:&mut AtlasLayerObservation,family:bool,shift:PackedColorShift,cancel:&AtomicBool){
    match input.texture(record.key,family,cancel) {
        Ok((pixels,source,warnings))=>{
            record.key=source.key;record.width=Some(pixels.width);record.height=Some(pixels.height);record.issues.extend(warnings);
            match over(base,&pixels,record.opacity,shift,cancel){Ok(())=>record.applied=true,Err(e)=>record.issues.push(e)}
        }
        Err(e)=>record.issues.push(e),
    }
}
fn shadow(source:&Pixels)->Pixels {
    let mut result=source.clone();
    for (out,p) in result.rgba.chunks_exact_mut(4).zip(source.rgba.chunks_exact(4)){
        if p[3]>0 {let t=((p[0] as f64+p[2] as f64)/2.-150.)*3.5;let t=(t as i32).clamp(0,255);out.copy_from_slice(&[0,0,0,((255-t)/2)as u8]);}
    }result
}
fn sculpt_order(region:u32)->u8{match region{3=>1,6=>2,5=>3,4=>4,2=>5,1=>6,0=>7,_=>8}}
/// Default bounds; all missing content remains explicit. Pixel output is a
/// reference inspection result and never certifies native shader appearance.
pub fn build_reference_atlas<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool,progress:impl FnMut(String))->std::result::Result<ReferenceAtlas,String>{
    build_reference_atlas_with_limits(plan,catalog,policy,cancel,progress,&AtlasLimits::default())
}
pub fn build_reference_atlas_with_limits<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool,mut progress:impl FnMut(String),limits:&AtlasLimits)->std::result::Result<ReferenceAtlas,String>{
    check(cancel)?;
    if limits.max_resource_bytes==0||limits.max_plane_bytes==0||limits.max_layers==0{return Err("Atlas limits are zero".into())}
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("Atlas belongs to a stale native Sim revision".into())}
    let form=match plan.scene.resolved_form {Some(FormSelection::Base)=>&plan.native.base,Some(FormSelection::OccultIndex(i))=>plan.native.occult_forms.get(i).ok_or("Native form absent")?,_=>return Err("Native form unresolved".into())};
    if plan.native.extended_species.filter(|s|*s!=0).unwrap_or(1)!=1||form.occult_type==Some(32){return Err("Actual nonhuman/pelt material branch is not qualified".into())}
    let tone_plan=plan.scene.tone.as_ref().ok_or("Native TONE absent")?;
    let tone=tone_plan.parsed.as_ref().ok_or("Native TONE decode unresolved")?;
    let selected=tone_plan.selected_skin_set.ok_or("Native skin state unresolved")?;
    let set=tone.skin_sets.get(selected).ok_or("Native TONE skin-set index outside array")?;
    if set.texture_instance==0{return Err("Native TONE selected texture is zero; no replacement skin selected".into())}
    let mut choices=BTreeMap::new();
    if let Some(candidate)=&tone_plan.resource.selected{choices.insert(candidate.key,candidate.provider);}
    for source in plan.parts.iter().filter_map(|p|p.source.as_ref()).chain(plan.parts.iter().flat_map(|p|p.materials.iter()).filter_map(|d|d.selected.as_ref())).chain(plan.skin_materials.iter().filter_map(|d|d.selected.as_ref())) {
        if let Some(provider)=source.selected{choices.insert(source.key,provider);}
    }
    let resolver=ResourceResolver::new(catalog,policy,&choices,ReadLimits{max_resource_bytes:limits.max_resource_bytes},limits.max_resource_bytes,cancel);
    let mut input=Inputs{resolver,pins:vec![],seen:BTreeSet::new(),limit:limits.max_plane_bytes};
    let mut layers=vec![];let mut issues=vec![];
    if let Some(candidate)=&tone_plan.resource.selected {
        let source=input.resolver.get(candidate.key);
        if source.bytes.as_deref()!=tone_plan.raw.as_deref(){return Err("Native TONE source changed or differs from the character plan".into())}
        input.pin(source);
    }
    for part in &plan.parts {if let Some(source)=&part.source{input.pin(source.clone())}}
    progress("Decoding actual native TONE texture".into());
    let base_set=tone.skin_sets.first().ok_or("TONE base skin-set absent")?;
    let key=ResourceKey::new(0x2bc04edf,0,base_set.texture_instance);
    let (mut skin,source,warnings)=input.texture(key,true,cancel)?;issues.extend(warnings);
    let mut native_skin=observation("native-tone-base",source.key,1.);native_skin.width=Some(skin.width);native_skin.height=Some(skin.height);native_skin.applied=true;layers.push(native_skin);
    if skin.width!=1024||skin.height!=2048{issues.push("Native skin has a nonstandard human atlas size; placement is source-dependent".into())}
    if selected>0 {
        let mut tan=observation("native-tan-texture",ResourceKey::new(0x2bc04edf,0,set.texture_instance),1.);add_layer(&mut skin,&mut input,&mut tan,true,neutral(),cancel);layers.push(tan);
        if set.overlay_instance!=0 {let mut mask=observation("native-tan-overlay-mask",ResourceKey::new(0x2bc04edf,0,set.overlay_instance),0.15);add_layer(&mut skin,&mut input,&mut mask,true,neutral(),cancel);layers.push(mask);}
        issues.push("Native tan-line mask extraction and reference UI tan-line choice remain unqualified".into());
    }
    let width=skin.width;let height=skin.height;
    let physique=form.physique.as_ref().ok_or("Native physique missing; no neutral detail preset selected")?.split(',').filter(|s|!s.is_empty()).map(|s|s.trim().parse::<f32>().map_err(|_|"Native physique detail number".to_string())).collect::<std::result::Result<Vec<_>,_>>()?;
    if physique.len()<4{return Err("Native physique lacks four source detail weights".into())}
    let detail_layers=human_detail_layers(plan.native.age.ok_or("Native age absent")?,plan.native.gender.ok_or("Native sex absent")?,physique[..4].try_into().unwrap()).map_err(|e|e.to_string())?;
    progress("Compositing actual age, sex and physique detail textures".into());
    let mut details=empty(width,height,limits.max_plane_bytes)?;
    for layer in detail_layers {check(cancel)?;let mut record=observation(layer.role,ResourceKey::new(0x2bc04edf,0,layer.instance),layer.opacity);add_layer(&mut details,&mut input,&mut record,true,neutral(),cancel);if !record.applied{return Err(format!("Required native detail layer {} unresolved: {:?}",record.key,record.issues))}layers.push(record);}
    let mut sculpt_plane=empty(width,height,limits.max_plane_bytes)?;
    let mut sculpts=BTreeMap::new();
    if let Some(morph)=&plan.morph {for source in &morph.link_resources {if source.key.type_id==0x9d1ab874 {if let Some(raw)=&source.raw {match external_cas_morph::Sculpt::parse(raw){Ok(s)=>{sculpts.insert(s.region,s);},Err(e)=>issues.push(format!("SCULPT texture fields unresolved: {e}"))}}}}}
    let mut sculpts=sculpts.into_values().collect::<Vec<_>>();sculpts.sort_by_key(|s|sculpt_order(s.region));
    for sculpt in sculpts {if sculpt.texture.instance_id!=0 {let mut record=observation("native-sculpt-texture",sculpt.texture,1.);add_layer(&mut sculpt_plane,&mut input,&mut record,true,neutral(),cancel);layers.push(record);}}
    over(&mut sculpt_plane,&details,1.,neutral(),cancel)?;details=sculpt_plane;
    let mut outfit_overlay=empty(width,height,limits.max_plane_bytes)?;
    let mut overlay=empty(width,height,limits.max_plane_bytes)?;
    let shoes=plan.parts.iter().rev().find(|p|p.native_slot.body_type==8).and_then(|p|p.region_map.as_ref());
    let shoes_on_top=shoes.is_some_and(|s|plan.parts.iter().filter(|p|[5,7].contains(&p.native_slot.body_type)).filter_map(|p|p.region_map.as_ref()).any(|r|r.max_layer(3)<s.max_layer(3)||r.max_layer(2)<s.max_layer(2)));
    let mut parts=plan.parts.iter().enumerate().filter(|(_,p)|!matches!(p.reference_selection,PartSelection::ExcludedByReference{..})).collect::<Vec<_>>();
    parts.sort_by(|(ai,a),(bi,b)|{let ac=a.casp.as_ref();let bc=b.casp.as_ref();let av=ac.map(|c|c.composition_method).unwrap_or(0);let bv=bc.map(|c|c.composition_method).unwrap_or(0);let al=if a.native_slot.body_type==8&&shoes_on_top{17000}else{ac.map(|c|c.sort_layer).unwrap_or(0)};let bl=if b.native_slot.body_type==8&&shoes_on_top{17000}else{bc.map(|c|c.sort_layer).unwrap_or(0)};bv.cmp(&av).then(al.cmp(&bl)).then(ai.cmp(bi))});
    progress("Layering actual current-outfit textures and native color shifts".into());
    let mut previous_sort=None;
    for (pi,part) in parts {
        check(cancel)?;
        let Some(casp)=&part.casp else{issues.push(format!("Outfit part{pi} has no resolved CASP"));continue};
        let sort=if casp.body_type==8&&shoes_on_top{17000}else{casp.sort_layer};
        if previous_sort==Some((casp.composition_method,sort)){issues.push(format!("Equal composition/layer order at part{pi}; native tie ordering remains unqualified"))}previous_sort=Some((casp.composition_method,sort));
        let raw_shift=part.native_slot.color_shift;let shift=PackedColorShift(raw_shift.unwrap_or(0x4000000000000000));
        if raw_shift.is_none(){issues.push(format!("Part{pi} has no serialized shift; neutral follows pinned reference display rule and is never written"))}
        for dependency in part.materials.iter().filter(|d|d.role=="shadow").chain(part.materials.iter().filter(|d|d.role=="diffuse")) {
            if layers.len()>=limits.max_layers{return Err("Atlas layer count exceeds bound".into())}
            let key=dependency.selected.as_ref().map(|s|s.key).unwrap_or(dependency.declared_key);
            let mut record=observation(&format!("outfit-{}",dependency.role),key,1.);record.part_index=Some(pi);record.sort_layer=Some(sort);record.composition_method=Some(casp.composition_method);record.packed_color_shift=raw_shift;
            match input.texture(key,dependency.role=="diffuse",cancel) {
                Ok((image,source,warnings))=>{
                    record.key=source.key;record.width=Some(image.width);record.height=Some(image.height);record.issues.extend(warnings);
                    if dependency.role=="shadow" {match over(&mut overlay,&shadow(&image),1.,neutral(),cancel){Ok(())=>record.applied=true,Err(e)=>record.issues.push(e)}}
                    else if casp.composition_method==3 {match over(&mut outfit_overlay,&image,1.,neutral(),cancel){Ok(())=>record.applied=true,Err(e)=>record.issues.push(e)}}
                    else {
                        record.opacity=shift.value(Axis::Opacity)*match casp.composition_method {2=>base_set.makeup_opacity,4=>base_set.makeup_opacity_2,_=>1.};
                        match over(&mut overlay,&image,record.opacity,shift,cancel){Ok(())=>record.applied=true,Err(e)=>record.issues.push(e)}
                        if casp.composition_method==1{record.issues.push("Composition1 remains the source straight-overlay preview; native hard-light semantics unqualified".into())}
                    }
                }
                Err(e)=>record.issues.push(e),
            }
            layers.push(record);
        }
        if part.materials.iter().any(|m|!["shadow","diffuse"].contains(&m.role.as_str())){issues.push(format!("Part{pi} normal/specular/emission/auxiliary material inputs remain separate renderer gates"))}
        if casp.texture_space!=0{issues.push(format!("Part{pi} texture-space{} placement unqualified",casp.texture_space))}
    }
    over(&mut details,&outfit_overlay,1.,neutral(),cancel)?;
    let alpha=skin.rgba.chunks_exact(4).map(|p|p[3]).collect::<Vec<_>>();for p in skin.rgba.chunks_exact_mut(4){p[3]=255;}
    progress("Blending native skin and detail planes".into());
    let mut final_skin=soft_light_reference(&skin,&details,tone.opacity as f64/100.,limits.max_plane_bytes,cancel).map_err(|e|e.to_string())?;
    for (p,a) in final_skin.rgba.chunks_exact_mut(4).zip(alpha){p[3]=a;}
    if tone_plan.value_shift.is_some_and(|v|v.abs()>0.001){issues.push("Native skin value shift is retained but this float-domain pixel transform is unqualified".into())}
    if tone.saturation>0{issues.push("Native TONE hue/saturation overlay semantics and reference UI branch are unqualified; no invented hue plane applied".into())}
    for dependency in plan.skin_materials.iter().filter(|d|d.role=="age/gender TONE overlay") {let mut record=observation("native-age-sex-tone-overlay",dependency.selected.as_ref().map(|s|s.key).unwrap_or(dependency.declared_key),1.);add_layer(&mut final_skin,&mut input,&mut record,true,neutral(),cancel);layers.push(record);issues.push("TONE age/sex overlay selector differs between reference source branches; retained native selection requires qualification".into());}
    let cutoff=height/4;for p in final_skin.rgba[..(cutoff as usize*width as usize*4)].chunks_exact_mut(4){p.copy_from_slice(&[0;4]);}
    over(&mut final_skin,&overlay,1.,neutral(),cancel)?;
    for layer in &layers {issues.extend(layer.issues.iter().map(|e|format!("{} {}: {e}",layer.role,layer.key)));}
    issues.push("Encoded-RGB source-over rounding, shader families, scalar skin branch, tan/mouth placement and native visible parity remain unqualified; no reference-tool authored mouth/default textures were inserted".into());
    check(cancel)?;Ok(ReferenceAtlas{image:final_skin,source_pins:input.pins,layers,issues,native_qualified:false})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_uses_original_red_blue_average_and_preserves_transparent_values(){
        let source=Pixels::new(4,1,vec![0,200,0,255,255,0,255,255,150,0,150,255,9,8,7,0],1024).unwrap();
        let result=shadow(&source);
        assert_eq!(result.rgba,vec![0,0,0,127,0,0,0,0,0,0,0,127,9,8,7,0]);
        assert_eq!(source.rgba[0..4],[0,200,0,255]);
    }
    #[test]
    fn no_replacement_atlas_is_built_for_cancelled_or_unresolved_native_input(){
        let native=serde_json::from_value(serde_json::json!({"sim_index":0,"sim_id":1,"base":{"outfits":[],"native":{"bytes":[],"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","fields":[]}},"occult_forms":[]})).unwrap();
        let catalog=Catalog::default();let plan=external_cas_character::assemble_character(&native,&catalog,None,&Default::default()).unwrap();
        assert!(build_reference_atlas(&plan,&catalog,None,&AtomicBool::new(false),|_|{}).unwrap_err().contains("TONE"));
        assert!(build_reference_atlas(&plan,&catalog,None,&AtomicBool::new(true),|_|{}).unwrap_err().contains("cancelled"));
    }
}
