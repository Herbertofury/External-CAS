// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently expressed CAS shadow image operation. The caller supplies
//! already selected, explicitly present compressed blocks and prepared pixels.
//! Canvas initialization, resource/mip selection and layer order are separate.
use crate::{check,Error,Pixels,Result,native_cpu_bc1::{decode_native_cpu_skin_bgra_block,decode_native_four_color_bgra_block}};
use std::sync::atomic::AtomicBool;

#[derive(Clone,Debug,PartialEq,Eq)]
pub enum NativeShadowBlock {
    /// Native RLE command zero advances without changing existing pixels.
    Skip,
    /// Native RLE command two uses BC1 transparency and endpoint ordering.
    Bc1([u8;8]),
    /// Native RLE command one ignores these alpha bytes and always uses the
    /// four-color palette from bytes8..16, even when color endpoints are equal.
    Bc3Color([u8;16]),
}
fn colors(block:&NativeShadowBlock,cancel:&AtomicBool)->Result<Option<[[u8;4];16]>>{
    check(cancel)?;
    Ok(match block {
        NativeShadowBlock::Skip=>None,
        NativeShadowBlock::Bc1(block)=>Some(decode_native_cpu_skin_bgra_block(block,cancel)?),
        NativeShadowBlock::Bc3Color(block)=>{
            Some(decode_native_four_color_bgra_block(block[8..16].try_into().unwrap(),cancel)?)
        }
    })
}
fn multiply(mut base:[u8;4],shadow:[u8;4])->[u8;4]{
    if shadow[3]!=0 {for c in 0..3 {base[c]=((base[c]as u16*shadow[c]as u16+128)>>8)as u8;}}
    base
}
/// Exact native prepared4x4 shadow operation in BGRA channel order. Source
/// block alpha does not become output alpha; destination alpha is preserved.
pub fn apply_native_shadow_bgra_block(base:&[[u8;4];16],block:&NativeShadowBlock,cancel:&AtomicBool)->Result<[[u8;4];16]>{
    let decoded=colors(block,cancel)?;
    let output=match decoded {None=>*base,Some(shadow)=>std::array::from_fn(|i|multiply(base[i],shadow[i]))};
    check(cancel)?;Ok(output)
}
/// Place explicitly selected row-major4x4 blocks into an existing RGBA plane.
/// Partial edge blocks are clipped to that plane. This only performs the
/// independently qualified block arithmetic; no native canvas is synthesized.
pub fn apply_native_shadow_blocks_in_place(base:&mut Pixels,blocks:&[NativeShadowBlock],cancel:&AtomicBool)->Result<()>{
    check(cancel)?;
    let bytes=(base.width as u64).checked_mul(base.height as u64).and_then(|n|n.checked_mul(4)).ok_or(Error::Invalid("Native shadow extent overflow"))?;
    let across=base.width.div_ceil(4)as usize;
    let required=across.checked_mul(base.height.div_ceil(4)as usize).ok_or(Error::Invalid("Native shadow block extent overflow"))?;
    if base.width==0||base.height==0||base.rgba.len()as u64!=bytes||blocks.len()!=required{return Err(Error::Invalid("Native shadow prepared extent mismatch"))}
    for(index,block)in blocks.iter().enumerate(){
        let Some(decoded)=colors(block,cancel)?else{continue};
        for(y,row)in decoded.chunks_exact(4).enumerate(){let py=(index/across)*4+y;if py>=base.height as usize{break}
            for(x,&bgra)in row.iter().enumerate(){let px=(index%across)*4+x;if px>=base.width as usize{break}
                let at=(py*base.width as usize+px)*4;
                let old=base.rgba[at..at+4].try_into().unwrap();
                let result=multiply(old,[bgra[2],bgra[1],bgra[0],bgra[3]]);
                base.rgba[at..at+4].copy_from_slice(&result);
            }
        }
    }
    check(cancel)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn preserves_alpha_transparent_pixels_sources_and_rejects_bad_extents(){
        let cancel=AtomicBool::new(false);let base=[[255,128,64,37];16];let original=base;
        let white=NativeShadowBlock::Bc1([255,255,255,255,0,0,0,0]);
        assert_eq!(apply_native_shadow_bgra_block(&base,&white,&cancel).unwrap(),[[254,128,64,37];16]);
        let transparent=NativeShadowBlock::Bc1([0,0,255,255,255,255,255,255]);
        assert_eq!(apply_native_shadow_bgra_block(&base,&transparent,&cancel).unwrap(),base);
        let ignored_alpha=NativeShadowBlock::Bc3Color([0,0,0,0,0,0,0,0,255,255,255,255,255,255,255,255]);
        assert_eq!(apply_native_shadow_bgra_block(&base,&ignored_alpha,&cancel).unwrap(),[[254,128,64,37];16]);
        assert_eq!(base,original);assert_eq!(apply_native_shadow_bgra_block(&base,&NativeShadowBlock::Skip,&cancel).unwrap(),base);
        assert!(apply_native_shadow_bgra_block(&base,&white,&AtomicBool::new(true)).is_err());
        let mut image=Pixels::new(1,1,vec![64,128,255,37],4).unwrap();
        assert!(apply_native_shadow_blocks_in_place(&mut image,&[],&cancel).is_err());assert_eq!(image.rgba,vec![64,128,255,37]);
        apply_native_shadow_blocks_in_place(&mut image,&[white],&cancel).unwrap();assert_eq!(image.rgba,vec![64,128,254,37]);
    }
    #[test]fn original_native_shadow_matches_all_synthetic_compressed_blocks(){
        use sha2::{Digest,Sha256};
        #[derive(serde::Deserialize)]struct Vector{mode:String,base_bgra:[[u8;4];16],source_block:Vec<u8>,bgra_sha256:String}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-shadow-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),512);let cancel=AtomicBool::new(false);
        for vector in vectors.vectors{let block=match vector.mode.as_str(){"bc1"=>NativeShadowBlock::Bc1(vector.source_block.try_into().unwrap()),"bc3-color"=>NativeShadowBlock::Bc3Color(vector.source_block.try_into().unwrap()),_=>panic!("Unknown oracle mode")};let output=apply_native_shadow_bgra_block(&vector.base_bgra,&block,&cancel).unwrap();assert_eq!(format!("{:x}",Sha256::digest(output.concat())),vector.bgra_sha256);}
    }
}
