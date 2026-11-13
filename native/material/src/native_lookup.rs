// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently expressed numerical contract of the installed skin-detail
//! lookup generators. Source selection is retained separately in native_globals.
use crate::{check,Result};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Debug)]
pub struct NativeSkinDetailLookup {
    pub packed_input:u32,
    pub width:u32,
    pub height:u32,
    /// Exact native staging-buffer order. Callers must qualify the native
    /// texture-format/SRV mapping before binding these bytes to a shader.
    pub bgra:Vec<u8>,
}
fn byte(value:f32)->u8{(value as i32)as u8}
/// Explicit generation only: initialized native renderers have a separate
/// zero-input/default resource branch. No missing value is given
/// an invented hue, saturation, texture or default resource here.
pub fn generate_native_skin_detail_lookup(packed:u32,cancel:&AtomicBool)->Result<NativeSkinDetailLookup>{
    check(cancel)?;
    let hue=((packed>>16)as f32)*f32::from_bits(0x3b360b61);
    let saturation=((packed&0xffff)as f32)*f32::from_bits(0x3c23d70a);
    let hue_times_six=hue*6.;let sector_position=hue*hue_times_six;
    let sector=sector_position as i32;let fraction=sector_position-sector as f32;
    let mut bgra=vec![0;256*4];
    for index in 0..256 {
        if index%32==0{check(cancel)?}
        let lightness=index as f32*f32::from_bits(0x3b800000);
        let high=if lightness>0.5{(saturation+lightness)-saturation*lightness}else{(saturation+1.)*lightness};
        if high==0.{continue}
        let low=(lightness+lightness)-high;
        // Keep the native single-precision operations, including the divide
        // followed by a multiply. Algebraic cancellation changes some bytes.
        let chroma_fraction=((high-low)/high)*high*fraction;
        let low_scaled=low*255.;
        let peak=byte(high*255.);let bottom=byte(low_scaled);
        let rising=byte(chroma_fraction*255.+low_scaled);
        let falling=byte((high-chroma_fraction)*255.);
        let rgb=match sector{0=>[peak,rising,bottom],1=>[falling,peak,bottom],2=>[bottom,peak,rising],3=>[bottom,falling,peak],4=>[rising,bottom,peak],5=>[peak,bottom,falling],_=>[0;3]};
        bgra[index*4..index*4+3].copy_from_slice(&[rgb[2],rgb[1],rgb[0]]);
    }
    check(cancel)?;Ok(NativeSkinDetailLookup{packed_input:packed,width:256,height:1,bgra})
}
/// Exact default table construction used by the initialized native renderer
/// (12591C0 -> 19BFC80 -> table+80). The CPU zero-input setter independently
/// calls the same grayscale generator. This is distinct from explicitly
/// running the HSL generator with zero, whose final RGB entry is254.
pub fn generate_native_renderer_default_lookup(cancel:&AtomicBool)->Result<NativeSkinDetailLookup>{
    check(cancel)?;let bgra=(0..256).flat_map(|index|[index as u8;4]).collect();check(cancel)?;
    Ok(NativeSkinDetailLookup{packed_input:0,width:256,height:1,bgra})
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn generator_is_explicit_cancellable_and_native_alpha_stays_zero(){
        assert!(generate_native_skin_detail_lookup(0,&AtomicBool::new(true)).is_err());
        let lookup=generate_native_skin_detail_lookup((10<<16)|20,&AtomicBool::new(false)).unwrap();assert_eq!((lookup.width,lookup.height),(256,1));assert!(lookup.bgra.chunks_exact(4).all(|p|p[3]==0));assert_eq!(&lookup.bgra[128*4..129*4],&[101,102,153,0]);
        let zero=generate_native_skin_detail_lookup(0,&AtomicBool::new(false)).unwrap();assert_eq!(&zero.bgra[255*4..256*4],&[254,254,254,0]);
        let outside=generate_native_skin_detail_lookup(0xffffffff,&AtomicBool::new(false)).unwrap();assert!(outside.bgra.iter().all(|b|*b==0));
    }
    #[test]fn native_emulated_cases_match_all_bytes_across_sector_and_integer_boundaries(){
        use sha2::{Digest,Sha256};
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../evidence/native-lookup-vectors.json")).unwrap();let vectors=vectors["vectors"].as_array().unwrap();assert_eq!(vectors.len(),147);
        for vector in vectors{let packed=vector["packed_input"].as_u64().unwrap()as u32;let actual=generate_native_skin_detail_lookup(packed,&AtomicBool::new(false)).unwrap();assert_eq!(format!("{:x}",Sha256::digest(actual.bgra)),vector["bgra_sha256"].as_str().unwrap(),"packed input{packed:08X}");}
    }
    #[test]fn initialized_renderer_default_uses_exact_native_grayscale_including_alpha(){
        use sha2::{Digest,Sha256};
        let cancel=AtomicBool::new(false);let default=generate_native_renderer_default_lookup(&cancel).unwrap();
        for(index,pixel)in default.bgra.chunks_exact(4).enumerate(){assert_eq!(pixel,&[index as u8;4]);}
        // Independent actual native19BFC80 execution, shared with the CPU-zero oracle.
        assert_eq!(format!("{:x}",Sha256::digest(&default.bgra)),"83a446ee1b8a6bd3a43e706b334d3566afab316a56f81c79e07434f8c8205277");
        assert_ne!(default.bgra,generate_native_skin_detail_lookup(0,&cancel).unwrap().bgra);
        assert!(generate_native_renderer_default_lookup(&AtomicBool::new(true)).is_err());
    }
}
