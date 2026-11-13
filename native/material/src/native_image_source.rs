// SPDX-License-Identifier: GPL-3.0-or-later
//! Native image-job resource selection before pixel preparation. Actual
//! RLEBlend probes LRLE with identical group/instance before its declared key;
//! an unresolved provider is not treated as absence and no fallback is authored.
use external_cas_assets::ResourceKey;
use external_cas_character::{ResourceObservation,ResourceResolver};
use external_cas_scene::EffectiveState;
use std::sync::Arc;
#[derive(Clone,Debug)]pub struct NativeImageJobSource {
    pub declared_key:ResourceKey,
    pub observations:Vec<Arc<ResourceObservation>>,
    pub selected:Option<Arc<ResourceObservation>>,
    pub selection_rule:String,
    pub issues:Vec<String>,
}
pub fn native_rle_blend_probe_key(declared:ResourceKey)->ResourceKey{ResourceKey::new(0x2bc04edf,declared.group_id,declared.instance_id)}
fn proven_absent(source:&ResourceObservation)->bool{source.candidates.is_empty()||source.effective_policy.as_ref().is_some_and(|p|p.state==EffectiveState::DeletedHighestPriority)}
fn select(declared_key:ResourceKey,probe:Arc<ResourceObservation>,declared:Option<Arc<ResourceObservation>>)->NativeImageJobSource{
    let mut observations=vec![probe.clone()];let mut issues=Vec::new();
    let(selected,selection_rule)=if probe.bytes.is_some(){(Some(probe),"Native RLEBlend19E3E90: existing same-group/instance LRLE replaces the declared image key".to_owned())}
    else if !proven_absent(&probe){issues.push("Native LRLE probe is unresolved; resource existence is not qualified, so declared-image fallback is not selected".into());(None,"Native LRLE probe unresolved".into())}
    else if let Some(source)=declared {observations.push(source.clone());if source.bytes.is_some(){(Some(source),"Native RLEBlend19E3E90: same-group/instance LRLE proven absent, declared key retained".into())}else{issues.push("Declared native image unresolved; native missing-resource substitution is not implemented".into());(None,"Declared native image unavailable".into())}}
    else {issues.push("Declared LRLE unavailable; native missing-resource substitution is not implemented".into());(None,"Declared LRLE unavailable".into())};
    NativeImageJobSource{declared_key,observations,selected,selection_rule,issues}
}
pub fn resolve_native_rle_blend_source(resolver:&mut ResourceResolver<'_,'_>,declared_key:ResourceKey)->NativeImageJobSource{
    let probe_key=native_rle_blend_probe_key(declared_key);let probe=resolver.get(probe_key);
    let declared=if probe_key!=declared_key&&probe.bytes.is_none()&&proven_absent(&probe){Some(resolver.get(declared_key))}else{None};
    select(declared_key,probe,declared)
}
#[cfg(test)]mod tests{
    use super::*;
    fn source(key:ResourceKey,present:bool)->Arc<ResourceObservation>{Arc::new(ResourceObservation{key,candidates:vec![],selected:None,selection:"synthetic source".into(),effective_policy:None,payload_sha256:None,bytes:present.then(||Arc::from([1u8].as_slice())),error:None})}
    #[test]fn actual_probe_preserves_full_key_and_never_authors_missing_source(){
        let key=ResourceKey::new(0x3453cf95,0x12345678,0xfedcba9876543210);let probe_key=native_rle_blend_probe_key(key);assert_eq!(probe_key,ResourceKey::new(0x2bc04edf,key.group_id,key.instance_id));
        let probe=source(probe_key,true);let selected=select(key,probe.clone(),None);assert!(Arc::ptr_eq(selected.selected.as_ref().unwrap(),&probe));
        let declared=source(key,true);let selected=select(key,source(probe_key,false),Some(declared.clone()));assert!(Arc::ptr_eq(selected.selected.as_ref().unwrap(),&declared));assert_eq!(selected.observations.len(),2);
        let missing=select(key,source(probe_key,false),Some(source(key,false)));assert!(missing.selected.is_none());assert!(!missing.issues.is_empty());
    }
}
