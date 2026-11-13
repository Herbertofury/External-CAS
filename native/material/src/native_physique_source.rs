// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-pinned native CASTuning parameter selection. The native SimData
//! schema supplies species/occult and all physique floats; record names never
//! substitute for context, and unknown fields remain in the original document.
use crate::native_physique::{plan_native_physique_texture_weights,NativePhysiqueTextureParameters,NativePhysiqueTexturePlan};
use external_cas_assets::{Catalog,ReadLimits,ResourceKey};
use external_cas_character::{CharacterPlan,ResourceObservation,ResourceResolver};
use external_cas_room::{parse_simdata,DataDocument,DataValue};
use external_cas_scene::{FormSelection,ScopePolicy,EffectiveState};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,sync::{Arc,atomic::{AtomicBool,Ordering}}};
#[derive(Clone,Debug)]
pub struct NativePhysiqueTuningCandidate {
    pub source:Arc<ResourceObservation>,pub instance_name:String,pub species:Vec<u32>,pub occult:Vec<u32>,
    pub max_weights:[f32;4],pub parameters:NativePhysiqueTextureParameters,pub document:Arc<DataDocument>,
}
#[derive(Clone,Debug)]
pub struct NativePhysiqueSourcePlan {pub tuning:NativePhysiqueTuningCandidate,pub weights:NativePhysiqueTexturePlan,pub selection_rule:String}
#[derive(Clone,Debug,Default)]
pub struct NativePhysiqueSourceResolution {
    pub selected:Option<NativePhysiqueSourcePlan>,pub candidates:Vec<NativePhysiqueTuningCandidate>,
    pub inactive_tombstones:Vec<ResourceKey>,
    pub source_pins:Vec<Arc<ResourceObservation>>,pub issues:Vec<String>,
}
fn field<'a>(object:&'a DataValue,name:&str)->Result<&'a DataValue,String>{object.field(name)?.ok_or_else(||format!("Required native CASTuning field {name} absent"))}
fn number(object:&DataValue,name:&str)->Result<f32,String>{let value=f32::from_bits(field(object,name)?.float_bits()?);if !value.is_finite(){return Err(format!("Native CASTuning field {name} is nonfinite"))}Ok(value)}
fn enums(object:&DataValue,name:&str)->Result<Vec<u32>,String>{
    let DataValue::Vector(values)=field(object,name)? else{return Err(format!("Native CASTuning {name} is not a vector"))};
    values.iter().map(|v|match v{DataValue::Signed64(v)=>u32::try_from(*v).map_err(|_|format!("Native CASTuning {name} enum outside u32")),DataValue::Unsigned32(v)=>Ok(*v),_=>Err(format!("Native CASTuning {name} enum representation is unsupported"))}).collect()
}
/// Current native loader reads this named48-byte schema through fixed offsets.
/// An altered schema must not silently reuse its field interpretation.
fn stored_weights(raw:&str)->Result<[f32;9],String>{
    let mut terms=raw.split(',').collect::<Vec<_>>();if terms.last()==Some(&""){terms.pop();}
    if terms.len()!=9||terms.iter().any(|s|s.trim().is_empty()){return Err("Native texture builder requires nine explicit physique terms and at most one trailing delimiter".into())}
    terms.into_iter().map(|v|v.trim().parse::<f32>().map_err(|_|"Native physique number".to_string())).collect::<Result<Vec<_>,_>>()?.try_into().map_err(|_|"Native physique term count".into())
}
fn schema(document:&DataDocument)->Result<(),String>{
    let roots=document.schemas.iter().filter(|s|s.name=="Client_CASTuning").collect::<Vec<_>>();if roots.len()!=1||roots[0].size!=68{return Err("Native CASTuning root schema identity/extent mismatch".into())}
    for(name,offset,data_type)in[("Physique",12,13),("Species",52,14),("Occult",60,14)]{let columns=roots[0].columns.iter().filter(|c|c.name==name).collect::<Vec<_>>();if columns.len()!=1||columns[0].offset!=offset||columns[0].data_type!=data_type{return Err(format!("Native CASTuning root field {name} schema mismatch"))}}
    let named=document.schemas.iter().filter(|s|s.name=="Client_CASTuning.Physique").collect::<Vec<_>>();if named.len()!=1||named[0].size!=48{return Err("Native CASTuning Physique schema identity/extent mismatch".into())}let schema=named[0];
    for(name,offset)in [("MaxHeavy",0),("MaxLean",4),("MaxFit",8),("MaxBony",12),("HeavyTextureFadeOutStart",16),("HeavyTextureFadeOutEnd",20),("FitTextureFadeOutStart",24),("FitTextureFadeOutEnd",28),("EaseInExponentHeavy",32),("EaseInExponentLean",36),("EaseInExponentFit",40),("EaseInExponentBony",44)]{
        let columns=schema.columns.iter().filter(|c|c.name==name).collect::<Vec<_>>();if columns.len()!=1||columns[0].offset!=offset||columns[0].data_type!=10{return Err(format!("Native CASTuning Physique field {name} schema mismatch"))}
    }Ok(())
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn enum_and_numeric_fields_reject_opaque_duplicates_and_nonfinite(){
        let object=DataValue::Object{schema:"controlled".into(),schema_hash:0,fields:vec![("Species".into(),DataValue::Vector(vec![DataValue::Signed64(1)])),("f".into(),DataValue::FloatBits(1.0_f32.to_bits())),("opaque".into(),DataValue::Opaque{data_type:6,source_offset:123,bytes:vec![1,2,3,4]})]};assert_eq!(enums(&object,"Species").unwrap(),vec![1]);assert_eq!(number(&object,"f").unwrap(),1.0);assert!(number(&object,"opaque").is_err());assert!(field(&object,"missing").is_err());
        let duplicate=DataValue::Object{schema:"controlled".into(),schema_hash:0,fields:vec![("f".into(),DataValue::FloatBits(0)),("f".into(),DataValue::FloatBits(f32::NAN.to_bits()))]};assert!(number(&duplicate,"f").is_err());let nonfinite=DataValue::Object{schema:"controlled".into(),schema_hash:0,fields:vec![("f".into(),DataValue::FloatBits(f32::NAN.to_bits()))]};assert!(number(&nonfinite,"f").is_err());
    }
    #[test]fn incomplete_schema_is_rejected_without_a_default(){let document=DataDocument{version:0,schemas:vec![],instances:vec![],issues:vec![]};assert!(schema(&document).is_err());}
}
#[cfg(test)]mod native_weights_validation{
    use super::*;
    #[test]fn empty_terms_do_not_shift_native_axis_identity(){assert_eq!(stored_weights("1,2,3,4,5,6,7,8,9,").unwrap(),[1.,2.,3.,4.,5.,6.,7.,8.,9.]);assert_eq!(stored_weights("1,2,3,4,5,6,7,8,9").unwrap(),[1.,2.,3.,4.,5.,6.,7.,8.,9.]);for malformed in ["1,,2,3,4,5,6,7,8,9","1,2,3,4,5,6,7,8,9,,","","0,0,0,0"]{assert!(stored_weights(malformed).is_err());}}
    fn context(species:Vec<u32>,occult:Vec<u32>)->NativePhysiqueTuningCandidate{
        NativePhysiqueTuningCandidate{source:Arc::new(ResourceObservation{key:ResourceKey::new(0x545ac67a,0x4b5265,1),candidates:vec![],selected:None,selection:"controlled numeric context".into(),effective_policy:None,payload_sha256:None,bytes:None,error:None}),instance_name:"name does not determine context".into(),species,occult,max_weights:[0.75;4],parameters:NativePhysiqueTextureParameters{heavy_fade:[0.3,0.75],fit_fade:[0.3,0.75],ease_exponents:[1.0,0.85,1.0,0.7]},document:Arc::new(DataDocument{version:0,schemas:vec![],instances:vec![],issues:vec![]})}
    }
    #[test]fn exact_occult_context_precedes_default_and_ambiguous_contexts_are_rejected(){
        let candidates=vec![context(vec![1],vec![]),context(vec![1],vec![4]),context(vec![1],vec![8])];assert_eq!(select(&candidates,1,1).unwrap(),0);assert_eq!(select(&candidates,1,4).unwrap(),1);assert_eq!(select(&candidates,1,8).unwrap(),2);assert!(select(&candidates,1,12).is_err());assert!(select(&candidates,2,1).is_err());assert!(select(&[context(vec![1],vec![]),context(vec![1],vec![])],1,1).is_err());
    }
}
fn candidates(source:Arc<ResourceObservation>,document:DataDocument)->Result<Vec<NativePhysiqueTuningCandidate>,String>{
    schema(&document)?;let document=Arc::new(document);let mut result=vec![];
    for(name,object)in &document.instances{
        let DataValue::Object{schema,..}=object else{continue};if schema!="Client_CASTuning"{continue}
        let physique=field(object,"Physique")?;let max_weights=[number(physique,"MaxHeavy")?,number(physique,"MaxLean")?,number(physique,"MaxFit")?,number(physique,"MaxBony")?];
        let parameters=NativePhysiqueTextureParameters{heavy_fade:[number(physique,"HeavyTextureFadeOutStart")?,number(physique,"HeavyTextureFadeOutEnd")?],fit_fade:[number(physique,"FitTextureFadeOutStart")?,number(physique,"FitTextureFadeOutEnd")?],ease_exponents:[number(physique,"EaseInExponentHeavy")?,number(physique,"EaseInExponentLean")?,number(physique,"EaseInExponentFit")?,number(physique,"EaseInExponentBony")?]};
        result.push(NativePhysiqueTuningCandidate{source:source.clone(),instance_name:name.clone(),species:enums(object,"Species")?,occult:enums(object,"Occult")?,max_weights,parameters,document:document.clone()});
    }
    if result.is_empty(){return Err("CASTuning group resource has no native Client_CASTuning instance".into())}Ok(result)
}
/// Numeric context join from native32A2F0, with strict ambiguity handling.
/// The native human fallback for other species is deliberately not activated
/// by this parameter resolver; missing same-species context stays explicit.
fn select(candidates:&[NativePhysiqueTuningCandidate],species:u32,occult:u32)->Result<usize,String>{
    let same=candidates.iter().enumerate().filter(|(_,c)|c.species.contains(&species)).collect::<Vec<_>>();
    let exact=same.iter().filter(|(_,c)|{let mask=c.occult.iter().fold(0,|a,v|a|v);mask&occult!=0||(occult<2&&(mask==0||mask&1!=0))}).map(|(i,_)|*i).collect::<Vec<_>>();
    let matching=if exact.is_empty(){same.iter().filter(|(_,c)|c.occult.is_empty()||c.occult.iter().any(|v|v&1!=0)).map(|(i,_)|*i).collect::<Vec<_>>()}else{exact};
    match matching.as_slice(){[i]=>Ok(*i),[]=>Err("No source-qualified same-species native CASTuning context".into()),_=>Err("Multiple native CASTuning contexts match; native load-order tie is unresolved".into())}
}
pub fn resolve_native_physique_texture_plan<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool)->Result<NativePhysiqueSourceResolution,String>{
    if cancel.load(Ordering::Relaxed){return Err("Native physique selection cancelled".into())}
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("Native physique plan belongs to a stale Sim revision".into())}
    let species=plan.native.extended_species.filter(|v|*v!=0).ok_or("Explicit native species absent")?;
    // The selected top-level SimData appearance is the native base form; its
    // wrapper has no occult_type field. Stored occult-form records carry their
    // own required type. This uses selected-form identity, never live-current.
    let(form,occult)=match plan.scene.resolved_form{Some(FormSelection::Base)=>(&plan.native.base,1),Some(FormSelection::OccultIndex(i))=>{let form=plan.native.occult_forms.get(i).ok_or("Native selected form absent")?;(form,form.occult_type.ok_or("Native selected-form occult context absent")?)},_=>return Err("Native selected form unresolved".into())};
    let raw_weights=form.physique.as_ref().ok_or("Native selected-form physique absent")?;let weights=stored_weights(raw_weights)?;
    let mut keys=catalog.by_key.keys().filter(|k|k.type_id==0x545ac67a&&k.group_id==0x004b5265).copied().collect::<Vec<_>>();keys.sort();if keys.len()>128{return Err("Native CASTuning source count exceeds128 budget".into())}
    let choices=BTreeMap::new();let mut resolver=ResourceResolver::new(catalog,policy,&choices,ReadLimits{max_resource_bytes:16*1024*1024},64*1024*1024,cancel);let mut result=NativePhysiqueSourceResolution::default();let mut unknown=false;
    for key in keys{
        if cancel.load(Ordering::Relaxed){return Err("Native physique selection cancelled".into())}let source=resolver.get(key);result.source_pins.push(source.clone());
        if source.effective_policy.as_ref().is_some_and(|p|p.state==EffectiveState::DeletedHighestPriority){result.inactive_tombstones.push(key);continue}
        let parsed=source.bytes.as_ref().ok_or_else(||source.error.clone().unwrap_or_else(||"CASTuning source unresolved".into())).and_then(|raw|parse_simdata(raw,cancel)).and_then(|doc|candidates(source.clone(),doc));
        match parsed{Ok(candidates)=>result.candidates.extend(candidates),Err(e)=>{unknown=true;result.issues.push(format!("{key}: {e}"));}}
    }
    if unknown{result.issues.push("An unparsed CASTuning candidate could affect context selection; no tuning selected".into());return Ok(result)}
    match select(&result.candidates,species,occult){
        Ok(index)=>{let tuning=result.candidates[index].clone();let planned=plan_native_physique_texture_weights(weights,&tuning.parameters,cancel).map_err(|e|e.to_string())?;if !planned.unnamed_axes.is_empty(){result.issues.push(format!("Native unnamed physique axes retained and affect normalization: {:?}",planned.unnamed_axes));}result.selected=Some(NativePhysiqueSourcePlan{tuning,weights:planned,selection_rule:format!("Exact native SimData Species={species}, selected-form Occult={occult}; no record-name or human-species fallback")});}
        Err(e)=>result.issues.push(e),
    }Ok(result)
}
