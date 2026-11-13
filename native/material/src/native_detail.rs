// SPDX-License-Identifier: GPL-3.0-or-later
//! Native human detail naming inputs, including the actual trait-first Breast
//! policy. Numeric tuning IDs come from the selected source, never constants.
use crate::native_physique_source::NativePhysiqueTuningCandidate;
use external_cas_character::{CharacterPlan,ResourceObservation};
use external_cas_room::DataValue;
use external_cas_scene::FormSelection;
use external_cas_tray::NativeTraitIds;
use sha2::{Digest,Sha256};
use std::sync::Arc;
#[derive(Clone,Debug)]pub struct NativeSpecialTraitSettings {pub force_breasts:u64,pub force_breastless:u64,pub source:Arc<ResourceObservation>}
#[derive(Clone,Debug)]pub struct NativeHumanDetailContext {pub base_prefix:String,pub breast_prefix:Option<String>,pub breasts:bool,pub age_gender_word:u32,pub occult_word:u32,pub traits:NativeTraitIds,pub settings:NativeSpecialTraitSettings}
/// Exact low-byte result of native0033EEC0 with an already resolved tuning.
pub fn native_breast_detail_policy(word0:u32,word2:u32,traits:&[u64],force_breasts:u64,force_breastless:u64)->bool{
    if traits.contains(&force_breasts){true}else if traits.contains(&force_breastless){false}else{!((word2&0x20!=0)&&(word0&0x78!=0))&&word0&0x2000!=0}
}
pub fn native_special_trait_settings(tuning:&NativePhysiqueTuningCandidate)->Result<NativeSpecialTraitSettings,String>{
    let root=tuning.document.schemas.iter().filter(|s|s.name=="Client_CASTuning").collect::<Vec<_>>();if root.len()!=1||root[0].size!=68{return Err("Native CASTuning root schema changed".into())}let root_fields=root[0].columns.iter().filter(|c|c.name=="SpecialTraits").collect::<Vec<_>>();if root_fields.len()!=1||root_fields[0].offset!=36||root_fields[0].data_type!=13{return Err("Native CASTuning SpecialTraits reference schema changed".into())}
    let schema=tuning.document.schemas.iter().filter(|s|s.name=="Client_CASTuning.SpecialTraits").collect::<Vec<_>>();
    if schema.len()!=1||schema[0].size!=216{return Err("Native SpecialTraits schema identity/size changed".into())}
    for(name,offset)in[("ForceBreastsTrait",16),("ForceBreastlessTrait",24)]{let fields=schema[0].columns.iter().filter(|c|c.name==name).collect::<Vec<_>>();if fields.len()!=1||fields[0].offset!=offset||fields[0].data_type!=18{return Err(format!("Native SpecialTraits {name} schema changed"))}}
    let roots=tuning.document.instances.iter().filter(|(name,_)|name==&tuning.instance_name).collect::<Vec<_>>();if roots.len()!=1{return Err("Native selected CASTuning instance identity changed".into())}
    let traits=roots[0].1.field("SpecialTraits")?.ok_or("Native SpecialTraits object absent")?;
    if !matches!(traits,DataValue::Object{schema,..}if schema=="Client_CASTuning.SpecialTraits"){return Err("Native SpecialTraits object schema changed".into())}
    let id=|name:&str|->Result<u64,String>{match traits.field(name)?.ok_or_else(||format!("Native SpecialTraits {name} absent"))?{DataValue::Unsigned64(id)=>Ok(*id),_=>Err(format!("Native SpecialTraits {name} value type changed"))}};
    Ok(NativeSpecialTraitSettings{force_breasts:id("ForceBreastsTrait")?,force_breastless:id("ForceBreastlessTrait")?,source:tuning.source.clone()})
}
pub fn native_human_detail_context(plan:&CharacterPlan,tuning:&NativePhysiqueTuningCandidate)->Result<NativeHumanDetailContext,String>{
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("Native detail context belongs to stale Sim bytes".into())}
    if plan.native.extended_species!=Some(1)||!tuning.species.contains(&1){return Err("Native human detail context requires explicit selected human species/tuning".into())}
    let age=plan.native.age.ok_or("Native detail age absent")?;let gender=plan.native.gender.ok_or("Native detail gender absent")?;
    let age_char=match age{1=>'b',2=>'p',4=>'c',8=>'t',16=>'y',32=>'a',64=>'e',128=>'i',_=>return Err("Native detail age flags unresolved".into())};
    let occult=match plan.scene.resolved_form{Some(FormSelection::Base)=>1,Some(FormSelection::OccultIndex(i))=>plan.native.occult_forms.get(i).and_then(|f|f.occult_type).ok_or("Native detail selected-form occult absent")?,_=>return Err("Native detail selected form unresolved".into())};
    let word0=age|gender;let traits=plan.native.native_traits().map_err(|e|e.to_string())?.ok_or("Native trait wrapper absent; empty-vector initialization not qualified")?;let settings=native_special_trait_settings(tuning)?;
    let breasts=native_breast_detail_policy(word0,occult,&traits.ids,settings.force_breasts,settings.force_breastless);
    let mut sex=match gender&0x3000{0x1000=>'m',0x2000=>'f',_=>'u'};if age==128{sex='u'}else if age&6!=0{sex='m'}
    let detail_age=if occult&0x20!=0{'y'}else{age_char};let suffix=if occult==0x20{"Werewolf"}else{""};
    Ok(NativeHumanDetailContext{base_prefix:format!("{detail_age}{sex}{suffix}"),breast_prefix:(age&0x78!=0).then(||format!("{detail_age}{}{suffix}",if breasts{'f'}else{'m'})),breasts,age_gender_word:word0,occult_word:occult,traits,settings})
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn exact_native_trait_first_policy_matches_original_selector_low_byte(){
        #[derive(serde::Deserialize)]struct Vector{word0:u32,word2:u32,traits:Vec<u64>,force_breasts_trait:u64,force_breastless_trait:u64,expected:bool}
        #[derive(serde::Deserialize)]struct Vectors{cases:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-breast-policy-vectors.json")).unwrap();assert_eq!(vectors.cases.len(),632);
        for v in vectors.cases{assert_eq!(native_breast_detail_policy(v.word0,v.word2,&v.traits,v.force_breasts_trait,v.force_breastless_trait),v.expected);}
    }
}
