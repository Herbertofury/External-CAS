// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit compressed-block preparation used by the native CPU SkinDetail
//! operation. This is not a universal GPU BC1 decoder or a layer selector.
use crate::{check,Result,native_lookup::NativeSkinDetailLookup,native_cpu_skin::compose_native_cpu_skin_block};
use std::sync::atomic::AtomicBool;
fn endpoints(block:&[u8;8])->(u16,u16,u32){(u16::from_le_bytes([block[0],block[1]]),u16::from_le_bytes([block[2],block[3]]),u32::from_le_bytes(block[4..8].try_into().unwrap()))}
fn green(value:u16)->u8{let g=((value>>5)&63)as u8;(g<<2)|(g>>4)}
fn bgra(value:u16)->[u8;4]{let b=(value&31)as u8;let r=(value>>11)as u8;[(b<<3)|(b>>2),green(value),(r<<3)|(r>>2),255]}
fn weighted(a:u8,b:u8)->u8{((a as u32*171+b as u32*85+128)>>8)as u8}
fn midpoint(a:u8,b:u8)->u8{(a|b)-((a^b)>>1)}
pub(crate) fn decode_native_four_color_bgra_block(block:&[u8;8],cancel:&AtomicBool)->Result<[[u8;4];16]>{
    check(cancel)?;let(a,b,selectors)=endpoints(block);let a=bgra(a);let b=bgra(b);
    let palette=[a,b,std::array::from_fn(|c|weighted(a[c],b[c])),std::array::from_fn(|c|weighted(b[c],a[c]))];
    Ok(std::array::from_fn(|i|palette[((selectors>>(i*2))&3)as usize]))
}
/// The CPU operation observes only the detail block's green channel. Its
/// four-color interpolation is171/85 with +128 before shift8; its three-color
/// midpoint rounds upward. Selector3 is zero in the transparent branch.
pub fn decode_native_cpu_detail_green_block(block:&[u8;8],cancel:&AtomicBool)->Result<[u8;16]>{
    check(cancel)?;let(a,b,selectors)=endpoints(block);let a_green=green(a);let b_green=green(b);
    let palette=if a>b{[a_green,b_green,weighted(a_green,b_green),weighted(b_green,a_green)]}else{[a_green,b_green,midpoint(a_green,b_green),0]};
    Ok(std::array::from_fn(|i|palette[((selectors>>(i*2))&3)as usize]))
}
/// Exact native CPU full-color block preparation. Native block alpha is255
/// except selector3 in the c0<=c1 branch, where the full pixel is zero.
pub fn decode_native_cpu_skin_bgra_block(block:&[u8;8],cancel:&AtomicBool)->Result<[[u8;4];16]>{
    check(cancel)?;let(a,b,selectors)=endpoints(block);let first=bgra(a);let second=bgra(b);
    let third=std::array::from_fn(|c|if a>b{weighted(first[c],second[c])}else{midpoint(first[c],second[c])});
    let fourth=if a>b{std::array::from_fn(|c|weighted(second[c],first[c]))}else{[0;4]};
    let palette=[first,second,third,fourth];Ok(std::array::from_fn(|i|palette[((selectors>>(i*2))&3)as usize]))
}
/// One explicitly present native compressed detail+skin block, with no cache
/// flags, missing-content defaults, placement, resampling or layer ordering.
pub fn compose_native_cpu_skin_bc1_block(detail:&[u8;8],skin:&[u8;8],lookup:&NativeSkinDetailLookup,opacity:u32,cancel:&AtomicBool)->Result<[[u8;4];16]>{
    let detail=decode_native_cpu_detail_green_block(detail,cancel)?;let skin=decode_native_cpu_skin_bgra_block(skin,cancel)?;
    compose_native_cpu_skin_block(&detail,&skin,lookup,opacity,cancel)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_midpoint_rounds_up_and_inputs_remain_unchanged(){
        let cancel=AtomicBool::new(false);let block=[0,0,0,2,0xaa,0xaa,0xaa,0xaa];let original=block;
        assert_eq!(decode_native_cpu_detail_green_block(&block,&cancel).unwrap(),[33;16]);
        assert_eq!(decode_native_cpu_skin_bgra_block(&block,&cancel).unwrap(),[[0,33,0,255];16]);assert_eq!(block,original);
        let transparent=[0,0,0x20,0,255,255,255,255];assert_eq!(decode_native_cpu_detail_green_block(&transparent,&cancel).unwrap(),[0;16]);assert_eq!(decode_native_cpu_skin_bgra_block(&transparent,&cancel).unwrap(),[[0;4];16]);
        assert!(decode_native_cpu_detail_green_block(&block,&AtomicBool::new(true)).is_err());assert!(decode_native_cpu_skin_bgra_block(&block,&AtomicBool::new(true)).is_err());
    }
    #[test]fn original_native_compressed_block_preparation_and_composition_match_all_bytes(){
        use sha2::{Digest,Sha256};
        #[derive(serde::Deserialize)]struct Input{detail:[u8;8],skin:[u8;8],packed:u32,opacity:u32}
        #[derive(serde::Deserialize)]struct Vector{input:Input,detail_sha256:String,skin_bgra_sha256:String,composed_bgra_sha256:String}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-cpu-bc1-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),256);let cancel=AtomicBool::new(false);
        for vector in vectors.vectors{
            let input=vector.input;let detail=decode_native_cpu_detail_green_block(&input.detail,&cancel).unwrap();let skin=decode_native_cpu_skin_bgra_block(&input.skin,&cancel).unwrap();let lookup=crate::native_cpu_skin::generate_native_cpu_skin_lookup(input.packed,&cancel).unwrap();let composed=compose_native_cpu_skin_bc1_block(&input.detail,&input.skin,&lookup,input.opacity,&cancel).unwrap();
            assert_eq!(format!("{:x}",Sha256::digest(detail)),vector.detail_sha256);assert_eq!(format!("{:x}",Sha256::digest(skin.concat())),vector.skin_bgra_sha256);assert_eq!(format!("{:x}",Sha256::digest(composed.concat())),vector.composed_bgra_sha256);
        }
    }
}
