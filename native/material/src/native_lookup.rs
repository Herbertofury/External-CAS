// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently expressed numerical contract of the installed skin-detail
//! lookup generator. This is not the native selector or a replacement LUT.
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
/// Explicit generation only: native callers have a separate zero-input/default
/// resource branch controlled by an engine setting. No missing value is given
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
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn generator_is_explicit_cancellable_and_native_alpha_stays_zero(){
        assert!(generate_native_skin_detail_lookup(0,&AtomicBool::new(true)).is_err());
        let lookup=generate_native_skin_detail_lookup((10<<16)|20,&AtomicBool::new(false)).unwrap();assert_eq!((lookup.width,lookup.height),(256,1));assert!(lookup.bgra.chunks_exact(4).all(|p|p[3]==0));assert_eq!(&lookup.bgra[128*4..129*4],&[101,102,153,0]);
        let zero=generate_native_skin_detail_lookup(0,&AtomicBool::new(false)).unwrap();assert_eq!(&zero.bgra[255*4..256*4],&[254,254,254,0]);
        let outside=generate_native_skin_detail_lookup(0xffffffff,&AtomicBool::new(false)).unwrap();assert!(outside.bgra.iter().all(|b|*b==0));
    }
}
