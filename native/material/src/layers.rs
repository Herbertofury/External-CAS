// SPDX-License-Identifier: GPL-3.0-or-later
//! Native-input atlas layers in encoded RGB. Source-over precision is an
//! explicit renderer boundary; it is not silently called the game shader.
use crate::{Pixels,Error,Result,check,color::{PackedColorShift,shift_reference_rgb}};
use std::sync::atomic::AtomicBool;
pub fn over_in_place(base:&mut Pixels,layer:&Pixels,opacity:f32,shift:PackedColorShift,cancel:&AtomicBool)->Result<()>{
    if base.width!=layer.width||base.height!=layer.height||base.rgba.len()!=layer.rgba.len(){return Err(Error::Unsupported("Atlas layer sizes differ; native placement/resampling is required"));}
    if !opacity.is_finite()||!(0.0..=1.0).contains(&opacity)||!shift.preview_supported(){return Err(Error::Unsupported("Native atlas opacity/color domain is unqualified"));}
    let transform=shift.value(crate::color::Axis::Hue)!=0.||shift.value(crate::color::Axis::Saturation)!=0.||shift.value(crate::color::Axis::Brightness)!=0.;
    for(i,(dest,source))in base.rgba.chunks_exact_mut(4).zip(layer.rgba.chunks_exact(4)).enumerate(){
        if i%4096==0{check(cancel)?;}if source[3]==0{continue;}
        let alpha=source[3]as f64/255.*opacity as f64;let dest_alpha=dest[3]as f64/255.;let out_alpha=alpha+dest_alpha*(1.-alpha);if out_alpha==0.{continue;}
        let rgb=if transform{shift_reference_rgb([source[0],source[1],source[2]],shift)?}else{[source[0],source[1],source[2]]};
        for channel in 0..3{dest[channel]=((rgb[channel]as f64*alpha+dest[channel]as f64*dest_alpha*(1.-alpha))/out_alpha).clamp(0.,255.).round_ties_even()as u8;}
        dest[3]=(out_alpha*255.).clamp(0.,255.).round_ties_even()as u8;
    }
    check(cancel)
}
