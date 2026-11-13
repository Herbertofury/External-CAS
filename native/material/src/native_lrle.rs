// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit native image-job preparation. Legacy LRLE and V002 palette paths
//! are qualified separately from V002 literal kind3, which remains gated.
use crate::{check,Error,Pixels,Result,native_blend::{NativeBlendAlpha,blend_native_image_in_place}};
use external_cas_assets::ReadLimits;
use sha2::{Digest,Sha256};
use std::sync::{Arc,atomic::AtomicBool};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum NativeLrleProfile {Legacy,V002Palette}
#[derive(Clone,Debug)]pub struct NativeLrleMip {pub source:Arc<[u8]>,pub source_sha256:[u8;32],pub pixels:Pixels,pub profile:NativeLrleProfile}
fn u32le(bytes:&[u8],at:usize)->Result<usize>{Ok(u32::from_le_bytes(bytes.get(at..at+4).ok_or(Error::Invalid("Truncated native LRLE"))?.try_into().unwrap())as usize)}
fn variable(bytes:&[u8],at:&mut usize,end:usize)->Result<u32>{let mut result=0u64;let mut shift=0;loop{let value=*bytes.get(*at).filter(|_|*at<end).ok_or(Error::Invalid("Truncated native LRLE variable"))?;*at+=1;result|=((value&127)as u64)<<shift;if value&128==0{break}shift+=7;if shift>=35{return Err(Error::Invalid("Native LRLE variable overflow"))}}u32::try_from(result).map_err(|_|Error::Invalid("Native LRLE variable overflow"))}
fn advance(at:&mut usize,n:usize,end:usize)->Result<()>{*at=at.checked_add(n).filter(|&v|v<=end).ok_or(Error::Invalid("Native LRLE stream extent"))?;Ok(())}
fn profile(bytes:&[u8],cancel:&AtomicBool)->Result<NativeLrleProfile>{
    if bytes.get(..4)!=Some(b"LRLE"){return Err(Error::Invalid("Native LRLE magic"))}
    if bytes.get(4..8)==Some(&[0,0,0,0]){return Ok(NativeLrleProfile::Legacy)}
    if bytes.get(4..8)!=Some(b"V002"){return Err(Error::Unsupported("Native LRLE version"))}
    let width=u16::from_le_bytes(bytes.get(8..10).ok_or(Error::Invalid("Native LRLE width"))?.try_into().unwrap())as usize;
    let height=u16::from_le_bytes(bytes.get(10..12).ok_or(Error::Invalid("Native LRLE height"))?.try_into().unwrap())as usize;
    let mips=u32le(bytes,12)?;if width==0||height==0||mips==0||mips>16{return Err(Error::Invalid("Native LRLE mip-zero dimensions/count"))}
    let table_end=16+mips*4;let colors=u32le(bytes,table_end)?;let data_at=table_end.checked_add(4).and_then(|v|colors.checked_mul(4).and_then(|n|v.checked_add(n))).filter(|&v|v<=bytes.len()).ok_or(Error::Invalid("Native LRLE palette extent"))?;
    let offsets=(0..mips).map(|i|u32le(bytes,16+i*4)).collect::<Result<Vec<_>>>()?;
    if offsets[0]!=0||offsets.windows(2).any(|p|p[0]>p[1])||offsets.last().is_some_and(|&v|v>bytes.len()-data_at){return Err(Error::Invalid("Native LRLE mip offsets"))}
    let mut at=data_at;let end=if mips>1{data_at+offsets[1]}else{bytes.len()};let mut pixels=0usize;let count=width.checked_mul(height).ok_or(Error::Invalid("Native LRLE pixel extent"))?;
    while at<end{
        check(cancel)?;let opcode=bytes[at];let encoded=variable(bytes,&mut at,end)?;
        if opcode&3==3{return Err(Error::Unsupported("Native V002 literal kind3 differs from maintained decoder; source retained"))}
        let run=(encoded>>if opcode&3==1{2}else{3})as usize;
        if run==0||pixels.checked_add(run).is_none_or(|n|n>count){return Err(Error::Invalid("Native LRLE pixel run"))}
        if opcode&7==6{advance(&mut at,4,end)?;}
        else if opcode&3==1{for _ in 0..run{if variable(bytes,&mut at,end)?as usize>=colors{return Err(Error::Invalid("Native LRLE palette index"))}}}
        else if opcode&7==2{let index=*bytes.get(at).filter(|_|at<end).ok_or(Error::Invalid("Native LRLE palette byte"))?as usize;advance(&mut at,1,end)?;if index>=colors{return Err(Error::Invalid("Native LRLE palette index"))}}
        else if opcode&7==4{let index=u16::from_le_bytes(bytes.get(at..at+2).filter(|_|at+2<=end).ok_or(Error::Invalid("Native LRLE palette word"))?.try_into().unwrap())as usize;advance(&mut at,2,end)?;if index>=colors{return Err(Error::Invalid("Native LRLE palette index"))}}
        else{return Err(Error::Unsupported("Native LRLE opcode"))}
        pixels+=run;
    }
    if pixels!=count{return Err(Error::Invalid("Native LRLE incomplete mip zero"))}Ok(NativeLrleProfile::V002Palette)
}
pub fn prepare_native_lrle_mip_zero(source:Arc<[u8]>,max_bytes:u64,cancel:&AtomicBool)->Result<NativeLrleMip>{
    check(cancel)?;if source.len()as u64>max_bytes{return Err(Error::Budget{bytes:source.len()as u64,limit:max_bytes})}
    let profile=profile(&source,cancel)?;
    let decoded=external_cas_assets::lrle::decode_lrle_cancellable(&source,ReadLimits{max_resource_bytes:max_bytes},cancel).map_err(|error|match error{external_cas_assets::Error::Cancelled=>Error::Cancelled,_=>Error::Invalid("Native LRLE decoded extent/stream")})?;
    let pixels=Pixels::new(decoded.width,decoded.height,decoded.rgba,max_bytes)?;check(cancel)?;
    Ok(NativeLrleMip{source_sha256:Sha256::digest(&source).into(),source,pixels,profile})
}
/// Explicit existing destination, opacity and native alpha mode. Returns a new
/// plane and preserves both source and destination when preparation fails.
pub fn blend_native_lrle_mip(base:&Pixels,mip:&NativeLrleMip,opacity:f32,alpha:NativeBlendAlpha,max_output_bytes:u64,cancel:&AtomicBool)->Result<Pixels>{
    check(cancel)?;if base.rgba.len()as u64>max_output_bytes{return Err(Error::Budget{bytes:base.rgba.len()as u64,limit:max_output_bytes})}
    let mut output=base.clone();blend_native_image_in_place(&mut output,&mip.pixels,opacity,alpha,cancel)?;Ok(output)
}
#[cfg(test)]mod tests{
    use super::*;
    fn fixture()->Vec<u8>{let mut out=b"LRLEV002".to_vec();out.extend(4u16.to_le_bytes());out.extend(4u16.to_le_bytes());out.extend(1u32.to_le_bytes());out.extend(0u32.to_le_bytes());out.extend(1u32.to_le_bytes());out.extend([3,5,7,255]);out.extend([0x82,1,0]);out}
    #[test]fn source_preservation_palette_profile_and_unsupported_literals_are_explicit(){
        let cancel=AtomicBool::new(false);let source:Arc<[u8]>=fixture().into();let mip=prepare_native_lrle_mip_zero(source.clone(),1024,&cancel).unwrap();assert!(Arc::ptr_eq(&source,&mip.source));assert_eq!(mip.profile,NativeLrleProfile::V002Palette);assert_eq!(mip.pixels.rgba,[7,5,3,255].repeat(16));assert_eq!(mip.source_sha256,<[u8;32]>::from(Sha256::digest(&source)));
        let base=Pixels::new(4,4,vec![0;64],64).unwrap();let output=blend_native_lrle_mip(&base,&mip,1.0,NativeBlendAlpha::InterpolateSourceAlpha,64,&cancel).unwrap();assert_eq!(output.rgba,mip.pixels.rgba);assert_eq!(base.rgba,vec![0;64]);assert_eq!(mip.source.as_ref(),fixture());
        let mut literal=fixture();literal.truncate(28);literal.push(67);literal.extend([3,5,7,255].repeat(16));assert!(matches!(prepare_native_lrle_mip_zero(literal.into(),1024,&cancel),Err(Error::Unsupported(_))));
        assert!(prepare_native_lrle_mip_zero(source.clone(),1,&cancel).is_err());assert!(prepare_native_lrle_mip_zero(source,1024,&AtomicBool::new(true)).is_err());
    }
    #[test]fn truncation_palette_errors_and_extra_pixels_cannot_become_native_sources(){
        let cancel=AtomicBool::new(false);let bytes=fixture();for end in 0..bytes.len(){assert!(prepare_native_lrle_mip_zero(Arc::from(&bytes[..end]),1024,&cancel).is_err());}
        let mut bad=bytes.clone();*bad.last_mut().unwrap()=1;assert!(prepare_native_lrle_mip_zero(bad.into(),1024,&cancel).is_err());
        let mut extra=bytes;extra.extend([14,3,5,7,255]);assert!(prepare_native_lrle_mip_zero(extra.into(),1024,&cancel).is_err());
    }
}
