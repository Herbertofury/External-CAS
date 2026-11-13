// SPDX-License-Identifier: GPL-3.0-or-later
//! Native CAS ordinary composite canvas. The source dispatcher selects these
//! flags; the alternate specular working context is not substituted here.
use crate::{check,Error,Pixels,Result};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum NativeCasCanvasKind {OverlayGpu,SkinToneAndMakeupGpu,ClothWithAlphaGpu,OverlayCpu}
impl NativeCasCanvasKind {
    pub fn native_flags(self)->u32{match self{Self::OverlayGpu=>1,Self::SkinToneAndMakeupGpu=>0x21,Self::ClothWithAlphaGpu|Self::OverlayCpu=>3}}
    pub fn initial_bgra(self)->[u8;4]{[0,0,0,if self.native_flags()&2!=0{0}else{255}]}
}
#[derive(Clone,Debug)]pub struct NativeCasCanvas {pub kind:NativeCasCanvasKind,pub native_flags:u32,pub blocks_across:u32,pub blocks_down:u32,pub pixels:Pixels}
/// Native initializer zeroes compressed color storage and validity. Its actual
/// lazy decoder makes bit1-clear pixels opaque black, or bit1-set pixels black
/// with zero alpha. Pixel-cache allocation itself is never treated as cleared.
pub fn new_native_cas_canvas(width:u32,height:u32,kind:NativeCasCanvasKind,max_bytes:u64,cancel:&AtomicBool)->Result<NativeCasCanvas>{
    check(cancel)?;if width==0||height==0{return Err(Error::Invalid("Native CAS canvas dimensions"))}
    let bytes=(width as u64).checked_mul(height as u64).and_then(|n|n.checked_mul(4)).ok_or(Error::Invalid("Native CAS canvas extent overflow"))?;
    if bytes>max_bytes||bytes>usize::MAX as u64{return Err(Error::Budget{bytes,limit:max_bytes.min(usize::MAX as u64)})}
    let mut rgba=vec![0;bytes as usize];let initial=kind.initial_bgra();if initial[3]!=0 {for(index,pixel)in rgba.chunks_exact_mut(4).enumerate(){if index%16384==0{check(cancel)?}pixel[3]=initial[3];}}
    let pixels=Pixels::new(width,height,rgba,max_bytes)?;check(cancel)?;
    Ok(NativeCasCanvas{kind,native_flags:kind.native_flags(),blocks_across:width.div_ceil(4),blocks_down:height.div_ceil(4),pixels})
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_stage_flags_match_actual_initialized_and_lazily_read_cache(){
        use sha2::{Digest,Sha256};
        #[derive(serde::Deserialize)]struct Vector{flags:u32,width:u32,height:u32,blocks:usize,initial_bgra:[u8;4],prepared_sha256:String}
        #[derive(serde::Deserialize)]struct Vectors{vectors:Vec<Vector>}
        let vectors:Vectors=serde_json::from_str(include_str!("../evidence/native-canvas-vectors.json")).unwrap();assert_eq!(vectors.vectors.len(),30);let cancel=AtomicBool::new(false);
        for vector in vectors.vectors{assert_eq!(vector.initial_bgra,[0,0,0,if vector.flags&2!=0{0}else{255}]);assert_eq!(format!("{:x}",Sha256::digest(vector.initial_bgra.repeat(vector.blocks*16))),vector.prepared_sha256);for kind in [NativeCasCanvasKind::OverlayGpu,NativeCasCanvasKind::SkinToneAndMakeupGpu,NativeCasCanvasKind::ClothWithAlphaGpu,NativeCasCanvasKind::OverlayCpu]{if kind.native_flags()==vector.flags{let canvas=new_native_cas_canvas(vector.width,vector.height,kind,1024*1024,&cancel).unwrap();assert_eq!(canvas.pixels.rgba,vector.initial_bgra.repeat(vector.width as usize*vector.height as usize));assert_eq!(canvas.blocks_across as usize*canvas.blocks_down as usize,vector.blocks);}}}
        assert!(new_native_cas_canvas(1024,2048,NativeCasCanvasKind::OverlayGpu,8,&cancel).is_err());assert!(new_native_cas_canvas(0,4,NativeCasCanvasKind::OverlayGpu,1024,&cancel).is_err());assert!(new_native_cas_canvas(4,4,NativeCasCanvasKind::OverlayGpu,1024,&AtomicBool::new(true)).is_err());
    }
}
