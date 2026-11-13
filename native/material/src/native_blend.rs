// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently expressed native image-job blend. This operates on encoded
//! channel bytes, without the reference atlas's gamma-space conversion. Input
//! preparation, layer order, masks and native color adjustments stay separate.
use crate::{check,Error,Pixels,Result};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum NativeBlendAlpha {InterpolateSourceAlpha,ForceSourceAlphaOpaque}
/// Native image factory clamps its opacity before this conversion. An explicit
/// caller must provide the same finite0..1 domain instead of silently clamping.
pub fn native_image_opacity_weight(opacity:f32)->Result<u16>{
    if !opacity.is_finite()||!(0.0..=1.0).contains(&opacity){return Err(Error::Invalid("Native image opacity outside finite0..1"))}
    Ok((opacity*256.0+0.5)as u16)
}
fn blend(base:[u8;4],source:[u8;4],opacity_weight:u16,alpha:NativeBlendAlpha)->[u8;4]{
    let source_alpha=source[3]as u32;
    let weight=if opacity_weight==256{source_alpha+(source_alpha>>7)}else{(source_alpha*opacity_weight as u32*0x8080+0x400000)>>23}as i32;
    std::array::from_fn(|c|{let old=base[c]as i32;let new=if c==3&&alpha==NativeBlendAlpha::ForceSourceAlphaOpaque{255}else{source[c]as i32};(old+(((new-old)*weight+128)>>8))as u8})
}
/// Exact prepared4×4 block operation used by raw-image jobs. RGB and BGRA
/// channel layouts both obey this independent-channel rule; alpha is byte3.
pub fn blend_native_image_block(base:&[[u8;4];16],source:&[[u8;4];16],opacity:f32,alpha:NativeBlendAlpha,cancel:&AtomicBool)->Result<[[u8;4];16]>{
    check(cancel)?;let weight=native_image_opacity_weight(opacity)?;let output=std::array::from_fn(|i|blend(base[i],source[i],weight,alpha));check(cancel)?;Ok(output)
}
/// Exact same-size prepared RGBA plane blend, cancellable between rows. This
/// does not resize, select/compress sources, apply HSL or infer native flags.
pub fn blend_native_image_in_place(base:&mut Pixels,source:&Pixels,opacity:f32,alpha:NativeBlendAlpha,cancel:&AtomicBool)->Result<()>{
    check(cancel)?;let weight=native_image_opacity_weight(opacity)?;
    let bytes=(base.width as u64).checked_mul(base.height as u64).and_then(|v|v.checked_mul(4)).ok_or(Error::Invalid("Native image dimensions overflow"))?;
    if base.width==0||base.height==0||base.width!=source.width||base.height!=source.height||base.rgba.len()as u64!=bytes||source.rgba.len()!=base.rgba.len(){return Err(Error::Invalid("Native image plane extent mismatch"))}
    for(row,(old,new))in base.rgba.chunks_exact_mut(base.width as usize*4).zip(source.rgba.chunks_exact(source.width as usize*4)).enumerate(){if row%8==0{check(cancel)?}for(old,new)in old.chunks_exact_mut(4).zip(new.chunks_exact(4)){let result=blend((&*old).try_into().unwrap(),new.try_into().unwrap(),weight,alpha);old.copy_from_slice(&result);}}
    check(cancel)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn alpha_modes_opacity_validation_cancellation_and_source_preservation(){
        let cancel=AtomicBool::new(false);let base=[[10,20,30,40];16];let source=[[250,100,0,128];16];let original=source;
        let ordinary=blend_native_image_block(&base,&source,1.0,NativeBlendAlpha::InterpolateSourceAlpha,&cancel).unwrap();let opaque=blend_native_image_block(&base,&source,1.0,NativeBlendAlpha::ForceSourceAlphaOpaque,&cancel).unwrap();assert_eq!(ordinary[0],[131,60,15,84]);assert_eq!(opaque[0],[131,60,15,148]);assert_eq!(source,original);
        assert_eq!(blend_native_image_block(&base,&source,0.0,NativeBlendAlpha::ForceSourceAlphaOpaque,&cancel).unwrap(),base);
        for invalid in [f32::NAN,f32::INFINITY,-0.01,1.01]{assert!(native_image_opacity_weight(invalid).is_err());}
        assert!(blend_native_image_block(&base,&source,1.0,NativeBlendAlpha::InterpolateSourceAlpha,&AtomicBool::new(true)).is_err());
        let mut image=Pixels::new(4,4,base.concat(),1024).unwrap();let layer=Pixels::new(4,4,source.concat(),1024).unwrap();blend_native_image_in_place(&mut image,&layer,1.0,NativeBlendAlpha::ForceSourceAlphaOpaque,&cancel).unwrap();assert_eq!(image.rgba,opaque.concat());assert_eq!(layer.rgba,source.concat());
    }
    #[test]fn actual_native_raw_image_jobs_match_all_synthetic_bytes(){
        use sha2::{Digest,Sha256};
        #[derive(serde::Deserialize)]struct Input{base:[[u8;4];16],source:[[u8;4];16],opacity:f32,force_source_alpha:bool}
        #[derive(serde::Deserialize)]struct Vector{input:Input,weight:u16,bgra_sha256:String}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-blend-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),256);let cancel=AtomicBool::new(false);
        for vector in vectors.vectors{let input=vector.input;assert_eq!(native_image_opacity_weight(input.opacity).unwrap(),vector.weight);let alpha=if input.force_source_alpha{NativeBlendAlpha::ForceSourceAlphaOpaque}else{NativeBlendAlpha::InterpolateSourceAlpha};let result=blend_native_image_block(&input.base,&input.source,input.opacity,alpha,&cancel).unwrap();assert_eq!(format!("{:x}",Sha256::digest(result.concat())),vector.bgra_sha256);}
    }
}
