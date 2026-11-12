// SPDX-License-Identifier: GPL-3.0-or-later
//! Native OutfitData.part_shifts lanes, from pinned SimRipper ImageStack.
//! tyjokr's supplied Nov5 UI is the local interaction/availability reference.
use crate::{Error,Result};
#[derive(Clone,Copy,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(transparent)]
pub struct PackedColorShift(pub u64);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Axis{Brightness,Saturation,Hue,Opacity}
impl Axis{fn shift(self)->u32{match self{Self::Brightness=>0,Self::Saturation=>16,Self::Hue=>32,Self::Opacity=>48}}}
impl Default for PackedColorShift{fn default()->Self{Self::NEUTRAL}}
impl PackedColorShift{
    pub const NEUTRAL:Self=Self(0x4000_0000_0000_0000);
    pub fn raw_lane(self,axis:Axis)->i16{(self.0>>axis.shift())as u16 as i16}
    pub fn value(self,axis:Axis)->f32{self.raw_lane(axis)as f32/16384.}
    pub fn replace_lane(self,axis:Axis,lane:i16)->Self{let shift=axis.shift();Self((self.0&!(0xffff_u64<<shift))|((lane as u16 as u64)<<shift))}
    /// Quantize only the explicitly edited lane. Other native bits are exact.
    /// Ties round to even; this policy is explicit until engine quantization is
    /// independently qualified. Already serialized lanes are never requantized.
    pub fn replace_value(self,axis:Axis,value:f32)->Result<Self>{
        if !value.is_finite()||!(-2.0..=32767.0/16384.0).contains(&value){return Err(Error::Invalid("Color shift outside signed Q2.14 lane"));}
        Ok(self.replace_lane(axis,(value*16384.).round_ties_even()as i16))
    }
    pub fn hue_saturation_value_opacity(self)->[f32;4]{[self.value(Axis::Hue),self.value(Axis::Saturation),self.value(Axis::Brightness),self.value(Axis::Opacity)]}
    pub fn preview_supported(self)->bool{
        let[h,s,v,o]=self.hue_saturation_value_opacity();
        (-1.0..=1.0).contains(&h)&&(-1.0..=1.0).contains(&s)&&(-1.0..=1.0).contains(&v)&&(0.0..=1.0).contains(&o)
    }
}
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct SliderRange{pub minimum:f32,pub maximum:f32,pub step:f32,pub neutral:f32}
impl SliderRange{
    pub fn tyjokr_fallback(axis:Axis)->Self{match axis{Axis::Opacity=>Self{minimum:0.2,maximum:1.,step:0.05,neutral:1.},_=>Self{minimum:-0.5,maximum:0.5,step:0.05,neutral:0.}}}
    pub fn valid(self)->bool{self.minimum.is_finite()&&self.maximum.is_finite()&&self.step.is_finite()&&self.neutral.is_finite()&&self.minimum<self.maximum&&self.step>0.&&self.maximum!=0.&&self.neutral>=self.minimum&&self.neutral<=self.maximum}
}
/// Pinned reference-tool HSV transform in encoded RGB space. This is not a
/// proof of the game's shader/mask/auxiliary behavior. Alpha is handled by the
/// actual selected material; a transparent texture is never synthesized.
pub fn shift_reference_rgb(rgb:[u8;3],shift:PackedColorShift)->Result<[u8;3]>{
    if !shift.preview_supported(){return Err(Error::Unsupported("Unqualified native color shift range"));}
    let[dh,ds,dv,_]=shift.hue_saturation_value_opacity();
    if dh==0.&&ds==0.&&dv==0.{return Ok(rgb);}
    let[r,g,b]=rgb.map(|v|v as f64/255.);
    let minimum=r.min(g).min(b);let maximum=r.max(g).max(b);let delta=maximum-minimum;
    let hue=if delta==0.{0.}else if r==maximum{60.*((g-b)/delta)+if g<b{360.}else{0.}}else if g==maximum{60.*((b-r)/delta)+120.}else{60.*((r-g)/delta)+240.};
    let mut h=(hue as f32)/360.+dh;
    while h>1.{h-=1.;}while h<0.{h+=1.;}
    let s=((if maximum==0.{0.}else{(delta/maximum)as f32})+ds).clamp(0.,1.);
    let v=((maximum as f32)+dv).clamp(0.,1.);
    let mut degrees=(h*360.)as f64;while degrees>=360.{degrees-=360.;}
    let result=if v<=0.{[0.;3]}else if s<=0.{[v as f64;3]}else{
        let sector=degrees/60.;let index=sector.floor()as i32;let fraction=sector-index as f64;
        let v=v as f64;let s=s as f64;let low=v*(1.-s);let falling=v*(1.-s*fraction);let rising=v*(1.-s*(1.-fraction));
        match index{0=>[v,rising,low],1=>[falling,v,low],2=>[low,v,rising],3=>[low,falling,v],4=>[rising,low,v],5=>[v,low,falling],_=>[v;3]}
    };
    Ok(result.map(|v|(v*255.).clamp(0.,255.)as u8))
}
