// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently expressed fixed-point contract of the native CPU skin/detail
//! kernel. Layer selection, native compressed-block preparation and placement
//! are separate gates; this module never invents any missing input image.
use crate::{check,Error,Result,native_lookup::{generate_native_skin_detail_lookup,generate_native_renderer_default_lookup,NativeSkinDetailLookup}};
use std::sync::atomic::AtomicBool;
/// The native CPU operator has its own explicit packed-zero path: all four
/// bytes in entry i equal i. Nonzero inputs use the same numeric HSL generator.
/// This does not perform GPU resource selection.
pub fn generate_native_cpu_skin_lookup(packed:u32,cancel:&AtomicBool)->Result<NativeSkinDetailLookup>{
    check(cancel)?;if packed!=0{return generate_native_skin_detail_lookup(packed,cancel)}
    generate_native_renderer_default_lookup(cancel)
}
fn product(a:u8,b:u8)->i32{let value=a as i32*b as i32+127;(value+(value>>8))>>8}
fn overlay(base:u8,detail:u8)->u8{if base<128{(2*product(base,detail))as u8}else{(255-2*product(255-base,255-detail))as u8}}
fn channel(skin:u8,tint:u8,detail:u8,weight:u8)->u8{
    let adjusted=(skin as i32+((product(skin,tint)-skin as i32)>>1))as u8;
    let first=overlay(adjusted,detail);if weight==0{return first}
    let second=overlay(first,detail);if weight>=128{return second}
    (first as i32+(((second as i32-first as i32)*weight as i32)>>7))as u8
}
/// Native code narrows opacity to its low byte, converts percent to a 0..128
/// fixed-point weight using single-precision math and rounds with +0.5 before
/// truncation. The final byte conversion is deliberately not saturation.
pub fn native_cpu_skin_weight(opacity:u32)->u8{
    ((((opacity&255)<<7)as f32*f32::from_bits(0x3c23d70a)+0.5)as i32)as u8
}
/// Exact 4×4 numeric kernel, using explicit prepared detail-green and skin BGRA
/// inputs. Native skin alpha is ignored by this operation; output alpha is255.
pub fn compose_native_cpu_skin_block(detail_green:&[u8;16],skin_bgra:&[[u8;4];16],lookup:&NativeSkinDetailLookup,opacity:u32,cancel:&AtomicBool)->Result<[[u8;4];16]>{
    check(cancel)?;if lookup.width!=256||lookup.height!=1||lookup.bgra.len()!=1024{return Err(Error::Invalid("Native CPU skin lookup extent"))}
    let weight=native_cpu_skin_weight(opacity);let mut output=[[0;4];16];
    for index in 0..16{let detail=detail_green[index];let tint=&lookup.bgra[detail as usize*4..detail as usize*4+4];for c in 0..3{output[index][c]=channel(skin_bgra[index][c],tint[c],detail,weight)}output[index][3]=255;}
    check(cancel)?;Ok(output)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn cpu_zero_lookup_and_fixed_point_alpha_are_explicit(){
        let cancel=AtomicBool::new(false);let lookup=generate_native_cpu_skin_lookup(0,&cancel).unwrap();assert_eq!(&lookup.bgra[1020..],[255;4]);assert_eq!(&lookup.bgra[512..516],[128;4]);assert_ne!(lookup.bgra,generate_native_skin_detail_lookup(0,&cancel).unwrap().bgra);
        assert_eq!(native_cpu_skin_weight(0),0);assert_eq!(native_cpu_skin_weight(85),109);assert_eq!(native_cpu_skin_weight(100),128);assert_eq!(native_cpu_skin_weight(255),70);
        let skin=[[17,128,244,0];16];let original=skin;let result=compose_native_cpu_skin_block(&[128;16],&skin,&lookup,85,&cancel).unwrap();assert!(result.iter().all(|p|p[3]==255));assert_eq!(skin,original);assert!(compose_native_cpu_skin_block(&[128;16],&skin,&lookup,85,&AtomicBool::new(true)).is_err());
    }
    #[test]fn all_native_emulated_cpu_blocks_match_exactly(){
        use sha2::{Digest,Sha256};
        #[derive(serde::Deserialize)]struct Input{packed:u32,opacity:u32,detail:[u8;16],skin:[[u8;4];16]}
        #[derive(serde::Deserialize)]struct Vector{input:Input,bgra_sha256:String,weight:u8}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-cpu-skin-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),260);
        let cancel=AtomicBool::new(false);for vector in vectors.vectors{let lookup=generate_native_cpu_skin_lookup(vector.input.packed,&cancel).unwrap();let output=compose_native_cpu_skin_block(&vector.input.detail,&vector.input.skin,&lookup,vector.input.opacity,&cancel).unwrap();assert_eq!(native_cpu_skin_weight(vector.input.opacity),vector.weight);assert_eq!(format!("{:x}",Sha256::digest(output.concat())),vector.bgra_sha256);}
    }
}
