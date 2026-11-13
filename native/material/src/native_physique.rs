// SPDX-License-Identifier: GPL-3.0-or-later
//! Native physique texture-job weights. The caller supplies explicit observed
//! tuning; no neutral physique, omitted field or source context is invented.
use crate::{check,Error,Result};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Debug)]
pub struct NativePhysiqueTextureParameters {
    pub heavy_fade:[f32;2],
    pub fit_fade:[f32;2],
    /// Native schema order: Heavy, Lean, Fit, Bony. The raw weight vector's
    /// first four entries instead use Heavy, Fit, Lean, Bony.
    pub ease_exponents:[f32;4],
}
#[derive(Clone,Debug)]
pub struct NativePhysiqueTextureWeight {
    pub axis:usize,pub original:f32,pub suppressed:f32,pub normalized:f32,pub opacity:f32,
}
#[derive(Clone,Debug)]
pub struct NativePhysiqueTexturePlan {
    pub original_weights:[f32;9],
    pub parameters:NativePhysiqueTextureParameters,
    /// Native descending pre-easing weight order, retaining ties in input order.
    pub ordered:Vec<NativePhysiqueTextureWeight>,
    /// Slots5..8 affect normalization but the native preset-name emitter has
    /// no image-name branch for them. Their actual weights remain retained.
    pub unnamed_axes:Vec<usize>,
}
fn fade(pair:[f32;2])->Result<[f32;2]>{
    if pair.iter().any(|v|!v.is_finite()){return Err(Error::Invalid("Nonfinite native physique fade"))}
    let start=pair[0];let end=if pair[1]<=start{start+f32::from_bits(0x3c23d70a)}else{pair[1]};let slope=1.0/(end-start);let offset=-(start*slope);
    if !slope.is_finite()||!offset.is_finite(){return Err(Error::Invalid("Native physique fade coefficients are nonfinite"))}Ok([slope,offset])
}
pub fn plan_native_physique_texture_weights(weights:[f32;9],parameters:&NativePhysiqueTextureParameters,cancel:&AtomicBool)->Result<NativePhysiqueTexturePlan>{
    check(cancel)?;
    if weights.iter().chain(parameters.ease_exponents.iter()).any(|v|!v.is_finite()){return Err(Error::Invalid("Nonfinite native physique weight or easing"))}
    let heavy=fade(parameters.heavy_fade)?;let fit=fade(parameters.fit_fade)?;let mut ordered=vec![];let mut total=0.0_f32;
    for(axis,&original)in weights.iter().enumerate(){
        if original<=0.0{continue}
        let mut suppressed=original;
        if matches!(axis,1|3){let factor=(weights[0]*heavy[0]+heavy[1]).clamp(0.0,1.0);suppressed=(-suppressed)*factor+suppressed;}
        else if axis==2{let factor=(weights[1]*fit[0]+fit[1]).clamp(0.0,1.0);suppressed=(-suppressed)*factor+suppressed;}
        if suppressed>0.0{total+=suppressed;ordered.push(NativePhysiqueTextureWeight{axis,original,suppressed,normalized:suppressed,opacity:0.0});}
    }
    if !total.is_finite(){return Err(Error::Invalid("Native physique weight total is nonfinite"))}
    ordered.sort_by(|a,b|b.suppressed.total_cmp(&a.suppressed));
    for weight in &mut ordered{
        if total>1.0{weight.normalized=(1.0/total)*weight.suppressed;}
        let exponent=match weight.axis{0=>parameters.ease_exponents[0],1=>parameters.ease_exponents[2],2=>parameters.ease_exponents[1],3=>parameters.ease_exponents[3],_=>1.0};
        weight.opacity=1.0-(1.0-weight.normalized).powf(exponent);
        if !weight.opacity.is_finite(){return Err(Error::Invalid("Native physique eased opacity is nonfinite"))}
    }
    let unnamed_axes=ordered.iter().filter(|v|v.axis>=5).map(|v|v.axis).collect();check(cancel)?;
    Ok(NativePhysiqueTexturePlan{original_weights:weights,parameters:parameters.clone(),ordered,unnamed_axes})
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_order_and_weight_bits_match_controlled_original_builder(){
        #[derive(serde::Deserialize)]struct Input{weights:[f32;9],heavy_fade:[f32;2],fit_fade:[f32;2],ease_exponents:[f32;4]}
        #[derive(serde::Deserialize)]struct Job{axis:usize,opacity_bits:u32}
        #[derive(serde::Deserialize)]struct Vector{input:Input,emitted:Vec<Job>}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-physique-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),64);let cancel=AtomicBool::new(false);
        for vector in vectors.vectors{let i=vector.input;let parameters=NativePhysiqueTextureParameters{heavy_fade:i.heavy_fade,fit_fade:i.fit_fade,ease_exponents:i.ease_exponents};let result=plan_native_physique_texture_weights(i.weights,&parameters,&cancel).unwrap();assert_eq!(result.ordered.len(),vector.emitted.len());for(actual,expected)in result.ordered.iter().zip(vector.emitted){assert_eq!(actual.axis,expected.axis);assert_eq!(actual.opacity.to_bits(),expected.opacity_bits);}assert_eq!(result.original_weights,i.weights);}
    }
    #[test]fn unsupported_values_and_unnamed_axes_remain_explicit(){
        let p=NativePhysiqueTextureParameters{heavy_fade:[0.3,0.75],fit_fade:[0.3,0.75],ease_exponents:[1.0,0.85,1.0,0.7]};let mut weights=[0.0;9];weights[8]=1.0;let result=plan_native_physique_texture_weights(weights,&p,&AtomicBool::new(false)).unwrap();assert_eq!(result.unnamed_axes,vec![8]);assert_eq!(result.ordered[0].opacity,1.0);
        weights[0]=f32::NAN;assert!(plan_native_physique_texture_weights(weights,&p,&AtomicBool::new(false)).is_err());assert!(plan_native_physique_texture_weights([0.0;9],&p,&AtomicBool::new(true)).is_err());
    }
}
