// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact native small-outfit insertion order. Region comparisons can depend on
//! the pair, so a generic language sort is not substituted for this algorithm.
use external_cas_assets::ResourceKey;
use external_cas_character::CharacterPart;
use std::collections::BTreeMap;
#[derive(Clone,Debug)]pub struct NativePartOrderRecord {pub part_index:usize,pub key:ResourceKey,pub body_type:u32,pub sort_layer:i32,pub descriptor_offset:i32,pub regions:BTreeMap<u32,f32>,pub overrides:BTreeMap<u32,f32>}
pub fn native_part_order_record(part_index:usize,part:&CharacterPart,descriptor_offset:i32)->Result<NativePartOrderRecord,String>{
    let casp=part.casp.as_ref().ok_or("Native ordering CASP unresolved")?;let source=part.source.as_ref().filter(|s|s.bytes.is_some()).ok_or("Native ordering CASP source unresolved")?;
    if part.region_source.as_ref().is_some_and(|s|s.bytes.is_none())||part.region_source.is_some()&&part.region_map.is_none(){return Err("Native ordering RMAP source/parser unresolved".into())}
    let mut regions=BTreeMap::new();if let Some(map)=&part.region_map{for block in &map.blocks{let priority=if block.meshes.is_empty(){-1.0}else{block.layer};if !priority.is_finite(){return Err("Native ordering RMAP nonfinite priority".into())}regions.insert(block.region,priority);}}
    let mut overrides=BTreeMap::new();for value in casp.region_overrides.as_ref().ok_or("Native ordering CASP override metadata absent")?{let priority=f32::from_bits(value.layer_bits);if !priority.is_finite(){return Err("Native ordering CASP nonfinite override".into())}overrides.insert(value.region as u32,priority);}
    Ok(NativePartOrderRecord{part_index,key:source.key,body_type:casp.body_type,sort_layer:casp.sort_layer,descriptor_offset,regions,overrides})
}
fn before(a:&NativePartOrderRecord,b:&NativePartOrderRecord)->bool{
    let hair_hat=(a.body_type==1&&b.body_type==2)||(a.body_type==2&&b.body_type==1);
    if !hair_hat{for region in(1..=25).rev(){if a.regions.get(&region).is_some_and(|v|*v> -1.0)&&b.regions.get(&region).is_some_and(|v|*v> -1.0){let av=a.overrides.get(&region).or_else(||a.regions.get(&region)).copied().unwrap_or(-2.0);let bv=b.overrides.get(&region).or_else(||b.regions.get(&region)).copied().unwrap_or(-2.0);if av!=bv{return av<bv}}}}
    let av=a.sort_layer.wrapping_add(a.descriptor_offset);let bv=b.sort_layer.wrapping_add(b.descriptor_offset);if av!=bv{return av<bv}
    [a.key.instance_id as u32,(a.key.instance_id>>32)as u32,a.key.group_id,a.key.type_id]>[b.key.instance_id as u32,(b.key.instance_id>>32)as u32,b.key.group_id,b.key.type_id]
}
/// Original019EB6E0 path, selected by its caller for at most28 records. The
/// descriptor offset is an explicit native input; no persisted-source join or
/// zero default is invented by this comparator.
pub fn native_small_outfit_order(records:&[NativePartOrderRecord])->Result<Vec<usize>,String>{
    if records.len()>28{return Err("Native outfit exceeds qualified insertion path; native large-sort path unresolved".into())}
    if records.iter().any(|r|r.regions.values().chain(r.overrides.values()).any(|v|!v.is_finite())){return Err("Native ordering nonfinite priority comparison unqualified".into())}
    let mut ordered:Vec<usize>=Vec::with_capacity(records.len());for(i,record)in records.iter().enumerate(){let mut at=ordered.len();while at>0&&before(record,&records[ordered[at-1]]){at-=1;}ordered.insert(at,i);}Ok(ordered.into_iter().map(|i|records[i].part_index).collect())
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn exact_native_small_sort_region_trees_and_record_moves_match_all_cases(){
        #[derive(serde::Deserialize)]struct Record{index:usize,instance:u64,#[serde(rename="type")]type_id:u32,group:u32,body_type:u32,sort_layer:i32,offset:i32,regions:BTreeMap<u32,f32>,overrides:BTreeMap<u32,f32>}
        #[derive(serde::Deserialize)]struct Vector{records:Vec<Record>,native_order:Vec<usize>}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-part-order-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),96);let mut count=0;
        for vector in vectors.vectors{count+=vector.records.len();let records=vector.records.into_iter().map(|r|NativePartOrderRecord{part_index:r.index,key:ResourceKey::new(r.type_id,r.group,r.instance),body_type:r.body_type,sort_layer:r.sort_layer,descriptor_offset:r.offset,regions:r.regions,overrides:r.overrides}).collect::<Vec<_>>();assert_eq!(native_small_outfit_order(&records).unwrap(),vector.native_order);}assert_eq!(count,624);
    }
}
