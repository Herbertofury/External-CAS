// SPDX-License-Identifier: GPL-3.0-or-later
//! Native unshifted compressed image-job arithmetic on an explicitly prepared
//! destination. This does not select resources, flags, mips, masks or canvases.
use crate::{check,Result,native_blend::{native_image_opacity_weight,NativeBlendAlpha},native_cpu_bc1::{decode_native_cpu_skin_bgra_block,decode_native_four_color_bgra_block}};
use std::sync::atomic::AtomicBool;

#[derive(Clone,Debug,PartialEq,Eq)]
pub enum NativeCompressedImageBlock {Skip,Bc1([u8;8]),Bc3([u8;16])}
fn alpha_weights(block:&[u8;16],weight:u16)->[u16;16]{
    let(a,b)=(block[0]as u32,block[1]as u32);let weight=weight as u32;
    let(x,y,shift)=if weight==256{(a*0x10101+0x8000,b*0x10101+0x8000,16)}else{(a*weight*0x8080+0x4000,b*weight*0x8080+0x4000,23)};
    let mut palette=[x>>shift,y>>shift,0,0,0,0,0,weight];
    if a>b {let step=(x-y)/7;for i in 1..=6 {palette[i+1]=(x-step*i as u32)>>shift;}}
    else {let step=(y-x)/5;for i in 1..=4 {palette[i+1]=(x+step*i as u32)>>shift;}}
    let bits=block[2..8].iter().enumerate().fold(0u64,|bits,(i,&value)|bits|((value as u64)<<(8*i)));
    std::array::from_fn(|i|palette[((bits>>(3*i))&7)as usize]as u16)
}
/// The native ordinary prepared-cache branch (context flag bit1 clear).
/// BC3 alpha is expanded directly to blend weights before interpolation, rather
/// than decoding8-bit alpha then multiplying opacity. Output alpha blends
/// toward255 for BC3; partial BC1 keeps its native transparent-color branch.
/// At full opacity native BC1 is canonicalized into an opaque four-color block.
pub fn blend_native_compressed_bgra_block(base:&[[u8;4];16],block:&NativeCompressedImageBlock,opacity:f32,cancel:&AtomicBool)->Result<[[u8;4];16]>{
    blend_native_compressed_bgra_block_with_alpha(base,block,opacity,NativeBlendAlpha::InterpolateSourceAlpha,cancel)
}
/// Explicit native ordinary/context-bit1 alpha behavior. The source alpha
/// still determines BC3 blend weights even when output blends toward opaque.
pub fn blend_native_compressed_bgra_block_with_alpha(base:&[[u8;4];16],block:&NativeCompressedImageBlock,opacity:f32,alpha:NativeBlendAlpha,cancel:&AtomicBool)->Result<[[u8;4];16]>{
    check(cancel)?;let weight=native_image_opacity_weight(opacity)?;
    if weight==0||matches!(block,NativeCompressedImageBlock::Skip){return Ok(*base)}
    let(colors,weights)=match block {
        NativeCompressedImageBlock::Skip=>unreachable!(),
        NativeCompressedImageBlock::Bc1(block)=>{
            let colors=if weight==256{decode_native_four_color_bgra_block(block,cancel)?}else{decode_native_cpu_skin_bgra_block(block,cancel)?};
            (colors,[weight;16])
        },
        NativeCompressedImageBlock::Bc3(block)=>(decode_native_four_color_bgra_block(block[8..16].try_into().unwrap(),cancel)?,alpha_weights(block,weight)),
    };
    let output=std::array::from_fn(|i|std::array::from_fn(|c|{let old=base[i][c]as i32;let new=if c==3&&alpha==NativeBlendAlpha::ForceSourceAlphaOpaque{255}else{colors[i][c]as i32};(old+(((new-old)*weights[i]as i32+128)>>8))as u8}));
    check(cancel)?;Ok(output)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn source_alpha_rounding_opacity_boundaries_and_cancellation(){
        let cancel=AtomicBool::new(false);let base=[[17,33,65,129];16];let original=base;
        let source=NativeCompressedImageBlock::Bc3([0,255,255,255,255,255,255,255,255,255,255,255,0,0,0,0]);
        assert_eq!(blend_native_compressed_bgra_block(&base,&source,0.0,&cancel).unwrap(),base);
        assert_eq!(blend_native_compressed_bgra_block(&base,&source,1.0,&cancel).unwrap(),[[255;4];16]);
        assert_eq!(blend_native_compressed_bgra_block(&base,&NativeCompressedImageBlock::Skip,0.5,&cancel).unwrap(),base);
        assert_eq!(base,original);for opacity in [-0.1,1.1,f32::NAN,f32::INFINITY]{assert!(blend_native_compressed_bgra_block(&base,&source,opacity,&cancel).is_err());}
        assert!(blend_native_compressed_bgra_block(&base,&source,0.5,&AtomicBool::new(true)).is_err());
    }
    #[test]fn original_native_compressed_blend_matches_all_synthetic_bytes(){
        use sha2::{Digest,Sha256};
        #[derive(serde::Deserialize)]struct Vector{mode:String,opacity:f32,base_bgra:[[u8;4];16],source_block:Vec<u8>,bgra_sha256:String}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-compressed-blend-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),768);let cancel=AtomicBool::new(false);
        for vector in vectors.vectors {let block=match vector.mode.as_str(){"bc1"=>NativeCompressedImageBlock::Bc1(vector.source_block.try_into().unwrap()),"bc3"=>NativeCompressedImageBlock::Bc3(vector.source_block.try_into().unwrap()),_=>panic!("Unknown oracle mode")};let result=blend_native_compressed_bgra_block(&vector.base_bgra,&block,vector.opacity,&cancel).unwrap();assert_eq!(format!("{:x}",Sha256::digest(result.concat())),vector.bgra_sha256);}
    }
}
