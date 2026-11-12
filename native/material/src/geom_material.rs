// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded GEOM MTNF observations. Framing follows pinned s4pi ShaderData;
//! normal/emission index interpretation follows SimRipper GEOM.MTNF only.
use external_cas_assets::ResourceKey;
#[derive(Clone,Debug)]
pub struct GeomMaterialParameter {pub hash:u32,pub data_type:u32,pub count:u32,pub offset:u32,pub words:Vec<u32>}
#[derive(Clone,Debug)]
pub struct GeomMaterial {pub magic:[u8;4],pub marker:u32,pub declared_data_length:u32,pub parameters:Vec<GeomMaterialParameter>,pub original:Vec<u8>,pub issues:Vec<String>}
impl GeomMaterial {
    pub fn parse(bytes:&[u8])->Result<Self,String>{
        if bytes.len()<16||bytes.len()>1024*1024{return Err("Bounded MTNF input extent".into())}
        let word=|at:usize|->Result<u32,String>{Ok(u32::from_le_bytes(bytes.get(at..at+4).ok_or("Truncated MTNF word")?.try_into().unwrap()))};
        let magic=bytes[..4].try_into().unwrap();if !matches!(&magic,b"MTNF"|b"MTRL"){return Err("Unsupported GEOM material magic".into())}
        let marker=word(4)?;let declared_data_length=word(8)?;let count=word(12)?as usize;
        if count>1024{return Err("MTNF parameter-count budget".into())}
        let start=16+count*16;let end=start.checked_add(declared_data_length as usize).ok_or("MTNF data extent overflow")?;if end>bytes.len(){return Err("Truncated MTNF declared data".into())}
        let mut parameters=vec![];let mut issues=vec![];let mut declared_words=0usize;
        for index in 0..count {
            let at=16+index*16;let hash=word(at)?;let data_type=word(at+4)?;let count=word(at+8)?;let offset=word(at+12)?;
            let length=(count as usize).checked_mul(4).ok_or("MTNF parameter extent overflow")?;let to=(offset as usize).checked_add(length).ok_or("MTNF parameter offset overflow")?;
            if (offset as usize)<start||to>end{return Err("MTNF parameter pointer outside declared data".into())}
            let words=bytes[offset as usize..to].chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect();declared_words=declared_words.checked_add(count as usize).ok_or("MTNF parameter count overflow")?;
            if !matches!(data_type,1|2|4|0x10004){issues.push(format!("Parameter {hash:08X} has opaque type {data_type:08X}"))}
            parameters.push(GeomMaterialParameter{hash,data_type,count,offset,words});
        }
        if declared_words.checked_mul(4)!=Some(declared_data_length as usize){issues.push("Declared data length differs from sum of parameter words; all original framing retained".into())}
        if end!=bytes.len(){issues.push("Trailing material bytes retained".into())}
        let mut seen=std::collections::BTreeSet::new();for parameter in &parameters{if !seen.insert(parameter.hash){issues.push(format!("Duplicate parameter {:08X} retained; typed lookup is ambiguous",parameter.hash))}}
        Ok(Self{magic,marker,declared_data_length,parameters,original:bytes.to_vec(),issues})
    }
    pub fn unique_parameter(&self,hash:u32)->Result<Option<&GeomMaterialParameter>,String>{let mut matching=self.parameters.iter().filter(|p|p.hash==hash);let value=matching.next();if matching.next().is_some(){return Err(format!("Duplicate material parameter {hash:08X}"))}Ok(value)}
    /// Stored float values only. Missing parameters stay None; no shader
    /// default, UV transform or color-space conversion is synthesized.
    pub fn float_values(&self,hash:u32)->Result<Option<Vec<f32>>,String>{
        let Some(parameter)=self.unique_parameter(hash)? else{return Ok(None)};
        if parameter.data_type!=1||!(1..=4).contains(&parameter.count){return Err(format!("Parameter {hash:08X} does not have qualified float framing"))}
        let values=parameter.words.iter().copied().map(f32::from_bits).collect::<Vec<_>>();if values.iter().any(|v|!v.is_finite()){return Err(format!("Parameter {hash:08X} contains nonfinite float data; raw bits retained"))}Ok(Some(values))
    }
    /// Exact source-tool interpretation for its two explicit index getters.
    /// This is not a universal meaning of DataType 0x10004: other containers
    /// use inline keys. Ambiguous parameters and nonzero reserved words fail.
    pub fn reference_texture_key(&self,hash:u32,references:&[ResourceKey])->Result<Option<ResourceKey>,String>{
        if ![0x6e56548a,0xf303d152].contains(&hash){return Err("No proven GEOM source-tool index getter for requested parameter".into())}
        let Some(parameter)=self.unique_parameter(hash)? else{return Ok(None)};
        if !matches!(parameter.data_type,4|0x10004)||parameter.count!=4||parameter.words[1..].iter().any(|v|*v!=0){return Err("GEOM source-tool texture-index framing is unqualified".into())}
        let index=parameter.words[0]as usize;let key=*references.get(index).ok_or("GEOM source-tool texture index outside TGI list")?;if key.is_empty(){return Ok(None)}Ok(Some(key))
    }
}
#[cfg(test)]mod tests{
    use super::*;
    fn fixture()->Vec<u8>{let mut bytes=Vec::from(*b"MTNF");for v in [0u32,20,2,0x6e56548a,0x10004,4,48,0x3c45e334,1,1,64,1,0,0,0,0x3f800000]{bytes.extend(v.to_le_bytes())}bytes}
    #[test]fn material_offsets_raw_bits_unknowns_and_all_prefixes(){
        let bytes=fixture();let material=GeomMaterial::parse(&bytes).unwrap();assert_eq!(material.original,bytes);assert_eq!(material.float_values(0x3c45e334).unwrap(),Some(vec![1.]));let keys=[ResourceKey::new(1,0,1),ResourceKey::new(2,0,2)];assert_eq!(material.reference_texture_key(0x6e56548a,&keys).unwrap(),Some(keys[1]));
        for prefix in 0..bytes.len(){assert!(GeomMaterial::parse(&bytes[..prefix]).is_err(),"prefix{prefix}")}
        let mut bad=bytes.clone();bad[28..32].copy_from_slice(&4u32.to_le_bytes());assert!(GeomMaterial::parse(&bad).is_err());
        let mut nan=bytes.clone();nan[64..68].copy_from_slice(&0x7fc01234u32.to_le_bytes());let material=GeomMaterial::parse(&nan).unwrap();assert_eq!(material.original,nan);assert!(material.float_values(0x3c45e334).is_err());
        let mut unknown=bytes;unknown[20..24].copy_from_slice(&0xdeadbeefu32.to_le_bytes());let material=GeomMaterial::parse(&unknown).unwrap();assert!(!material.issues.is_empty());assert!(material.reference_texture_key(0x6e56548a,&keys).is_err());
    }
    #[test]fn duplicate_hashes_and_inline_key_ambiguity_are_not_arbitrarily_resolved(){
        let mut bytes=fixture();bytes[32..36].copy_from_slice(&0x6e56548au32.to_le_bytes());let material=GeomMaterial::parse(&bytes).unwrap();assert!(material.unique_parameter(0x6e56548a).is_err());assert_eq!(material.parameters.len(),2);
        let mut bytes=fixture();bytes[52..56].copy_from_slice(&123u32.to_le_bytes());let material=GeomMaterial::parse(&bytes).unwrap();assert!(material.reference_texture_key(0x6e56548a,&[ResourceKey::new(1,2,3),ResourceKey::new(4,5,6)]).is_err());
    }
}
