// SPDX-License-Identifier: GPL-3.0-or-later
//! Recovered exact first SimRipper SkinBlender branch, commit862a32949e5156b371e2d2f83de4e37e0bb1afcc.
//! Caller supplies actual tone/detail pixels. Native selection/layering, hue,
//! tan, sculpts and game shader equivalence are separately qualified boundaries.
use crate::{Error,Pixels,Result,check};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Debug)]
pub struct DetailLayer{pub instance:u64,pub opacity:f32,pub role:&'static str}
/// Actual native detail identifiers following the retained source selection.
/// Infant source texture selection is unresolved; no toddler body/skin is used.
pub fn human_detail_layers(age:u32,gender:u32,physique:[f32;4])->Result<Vec<DetailLayer>>{
    let mut index=match age{1=>0,2=>1,4=>2,8=>3,16=>4,32=>5,64=>6,_=>return Err(Error::Unsupported("Native age has no qualified human detail texture selection"))};
    if ![0x1000,0x2000].contains(&gender){return Err(Error::Unsupported("Native sex is ambiguous for human detail textures"));}
    if physique.iter().any(|v|!v.is_finite()||*v<0.||*v>1.){return Err(Error::Unsupported("Native physique detail weights are outside the retained reference domain"));}
    let adult=age>=8;if adult&&gender==0x2000{index+=8;}
    let table=&crate::skin_keys::HUMAN_DETAIL_INSTANCES;
    let mut layers=vec![DetailLayer{instance:table[index][0],opacity:1.,role:"native-neutral-detail"}];
    if adult{layers.push(DetailLayer{instance:table[index+4][0],opacity:1.,role:"native-age-sex-detail-overlay"});}
    for(i,&opacity)in physique.iter().enumerate(){
        if opacity<=0.||(!adult&&!matches!(i,0|2)){continue;}
        if table[index][i+1]!=0{layers.push(DetailLayer{instance:table[index][i+1],opacity,role:"native-physique-detail"});}
        if adult&&table[index+4][i+1]!=0{layers.push(DetailLayer{instance:table[index+4][i+1],opacity,role:"native-physique-detail-overlay"});}
    }
    Ok(layers)
}
pub fn soft_light_reference(tone:&Pixels,detail:&Pixels,opacity:f64,limit:u64,cancel:&AtomicBool)->Result<Pixels>{
    check(cancel)?;
    if !opacity.is_finite()||!(0.0..=1.0).contains(&opacity){return Err(Error::Invalid("Skin opacity"));}
    if tone.width!=detail.width||tone.height!=detail.height||tone.rgba.len()!=detail.rgba.len(){return Err(Error::Unsupported("Skin layer dimensions must already match"));}
    if tone.rgba.chunks_exact(4).any(|p|p[3]!=255){return Err(Error::Unsupported("Reference skin branch requires opaque tone"));}
    if tone.rgba.len()as u64>limit{return Err(Error::Budget{bytes:tone.rgba.len()as u64,limit});}
    let mut rgba=vec![255;tone.rgba.len()];let alpha=opacity as f32;
    for(i,((base,shade),out))in tone.rgba.chunks_exact(4).zip(detail.rgba.chunks_exact(4)).zip(rgba.chunks_exact_mut(4)).enumerate(){
        if i%4096==0{check(cancel)?;}
        for k in 0..3{
            let d=shade[k]as f32;let c=base[k]as f32;
            let mut v=((1.-2.*(d/255.))*(c/255.)*(c/255.)+2.*(d/255.)*(c/255.))*255.;
            v=((v as f64*1.2_f64).min(255.))as f32;
            let second=if v>128.{255.-((255.-2.*(d-128.))*(255.-v)/256.)}else{(2.*d*v)/256.};
            v=second*alpha+v*(1.-alpha);v=(((v/255.-0.75)*1.1)+0.75)*255.;out[k]=v.clamp(0.,255.)as u8;
        }
    }
    check(cancel)?;Pixels::new(tone.width,tone.height,rgba,limit)
}
