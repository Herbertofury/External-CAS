// SPDX-License-Identifier: GPL-3.0-or-later
//! Native ordinary composite working storage. Compressed color and prepared
//! validity survive jobs; export does not recompress untouched source blocks.
use crate::{check,Error,Pixels,Result,native_canvas::NativeCasCanvasKind,native_blend::{NativeBlendAlpha,native_image_opacity_weight,blend_native_image_block},native_compressed_blend::{NativeCompressedImageBlock,blend_native_compressed_bgra_block_with_alpha},native_cpu_bc1::decode_native_four_color_bgra_block,native_lrle::NativeLrleMip,native_rle2::{NativeRle2Mip,NativeRle2Operation},native_shadow::{NativeShadowBlock,apply_native_shadow_bgra_block}};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Debug)]pub struct NativeEncodedPlane {pub width:u32,pub height:u32,pub dxgi_format:u32,pub row_pitch:usize,pub bytes:Vec<u8>}
#[derive(Clone,Debug)]pub struct NativeCompositeBlock {pub compressed_color:[u8;8],pub prepared_pixels:[[u8;4];16],pub prepared_valid:bool,pub alpha_valid:bool}
#[derive(Clone,Debug)]pub struct NativeCompositeCanvas {pub width:u32,pub height:u32,pub kind:NativeCasCanvasKind,blocks:Vec<NativeCompositeBlock>,tainted:bool}
fn canonical_color(mut block:[u8;8])->[u8;8]{let a=u16::from_le_bytes(block[..2].try_into().unwrap());let b=u16::from_le_bytes(block[2..4].try_into().unwrap());if b>=a{block[..2].copy_from_slice(&b.to_le_bytes());block[2..4].copy_from_slice(&a.to_le_bytes());let selectors=if a==b{0}else{u32::from_le_bytes(block[4..].try_into().unwrap())^0x55555555};block[4..].copy_from_slice(&selectors.to_le_bytes());}block}
impl NativeCompositeCanvas {
    pub fn new(width:u32,height:u32,kind:NativeCasCanvasKind,max_work_bytes:u64,cancel:&AtomicBool)->Result<Self>{
        check(cancel)?;if width==0||height==0{return Err(Error::Invalid("Native composite dimensions"))}let count=width.div_ceil(4)as u64*height.div_ceil(4)as u64;let bytes=count.checked_mul(std::mem::size_of::<NativeCompositeBlock>()as u64).ok_or(Error::Invalid("Native composite work extent overflow"))?;if bytes>max_work_bytes||count>usize::MAX as u64{return Err(Error::Budget{bytes,limit:max_work_bytes})}
        let blocks=vec![NativeCompositeBlock{compressed_color:[0;8],prepared_pixels:[[0;4];16],prepared_valid:false,alpha_valid:false};count as usize];check(cancel)?;Ok(Self{width,height,kind,blocks,tainted:false})
    }
    pub fn blocks(&self)->&[NativeCompositeBlock]{&self.blocks}
    pub fn is_tainted(&self)->bool{self.tainted}
    fn alpha(&self)->NativeBlendAlpha{if self.kind.native_flags()&2!=0{NativeBlendAlpha::ForceSourceAlphaOpaque}else{NativeBlendAlpha::InterpolateSourceAlpha}}
    fn prepare(&mut self,index:usize,cancel:&AtomicBool)->Result<[[u8;4];16]>{
        let block=&mut self.blocks[index];if !block.prepared_valid{let mut pixels=decode_native_four_color_bgra_block(&block.compressed_color,cancel)?;if self.kind.native_flags()&2!=0&&!block.alpha_valid{for pixel in &mut pixels{pixel[3]=0;}}block.prepared_pixels=pixels;block.prepared_valid=true;}Ok(block.prepared_pixels)
    }
    fn validate(&self,width:u32,height:u32,count:usize,cancel:&AtomicBool)->Result<()>{check(cancel)?;if self.tainted{return Err(Error::Invalid("Cancelled native composite must be discarded"))}if self.width!=width||self.height!=height||self.blocks.len()!=count{return Err(Error::Invalid("Native composite source extent mismatch"))}Ok(())}
    /// Source mip/order/operation are explicit. Cancellation marks the worker
    /// canvas unusable; callers discard it, preserving the original Sim/project.
    pub fn apply_rle2_mip(&mut self,mip:&NativeRle2Mip,operation:NativeRle2Operation,cancel:&AtomicBool)->Result<()>{
        self.validate(mip.width,mip.height,mip.blocks.len(),cancel)?;let opacity=match operation{NativeRle2Operation::UnshiftedOrdinaryBlend{opacity}=>{if native_image_opacity_weight(opacity)?==0{return Ok(())}Some(opacity)},NativeRle2Operation::Shadow=>None};let alpha=self.alpha();self.tainted=true;
        for(index,source)in mip.blocks.iter().enumerate(){check(cancel)?;if matches!(source,NativeCompressedImageBlock::Skip){continue}
            if let(Some(opacity),NativeCompressedImageBlock::Bc1(color))=(opacity,source){if native_image_opacity_weight(opacity)?==256{let block=&mut self.blocks[index];block.compressed_color=canonical_color(*color);block.prepared_valid=false;if self.kind.native_flags()&2!=0{block.alpha_valid=true;}continue}}
            let base=self.prepare(index,cancel)?;let output=if let Some(opacity)=opacity{blend_native_compressed_bgra_block_with_alpha(&base,source,opacity,alpha,cancel)?}else{let shadow=match source{NativeCompressedImageBlock::Bc1(b)=>NativeShadowBlock::Bc1(*b),NativeCompressedImageBlock::Bc3(b)=>NativeShadowBlock::Bc3Color(*b),NativeCompressedImageBlock::Skip=>unreachable!()};apply_native_shadow_bgra_block(&base,&shadow,cancel)?};self.blocks[index].prepared_pixels=output;
        }
        check(cancel)?;self.tainted=false;Ok(())
    }
    pub fn apply_lrle_mip_zero(&mut self,mip:&NativeLrleMip,opacity:f32,cancel:&AtomicBool)->Result<()>{
        self.validate(mip.pixels.width,mip.pixels.height,self.blocks.len(),cancel)?;if self.width%4!=0||self.height%4!=0{return Err(Error::Unsupported("Native LRLE composite requires complete4x4 blocks"))}let expected=self.width as u64*self.height as u64*4;if mip.pixels.rgba.len()as u64!=expected{return Err(Error::Invalid("Native LRLE prepared pixel extent"))}let weight=native_image_opacity_weight(opacity)?;if weight==0{return Ok(())}let across=self.width as usize/4;let alpha=self.alpha();self.tainted=true;
        for index in 0..self.blocks.len(){check(cancel)?;let mut source=[[0;4];16];for(y,row)in source.chunks_exact_mut(4).enumerate(){for(x,pixel)in row.iter_mut().enumerate(){let at=(((index/across)*4+y)*self.width as usize+(index%across)*4+x)*4;*pixel=[mip.pixels.rgba[at+2],mip.pixels.rgba[at+1],mip.pixels.rgba[at],mip.pixels.rgba[at+3]];}}
            if source.iter().all(|p|p[3]==0){continue}
            let base=self.prepare(index,cancel)?;let output=blend_native_image_block(&base,&source,opacity,alpha,cancel)?;let block=&mut self.blocks[index];block.prepared_pixels=output;if self.kind.native_flags()&2!=0{block.alpha_valid=weight==256&&source.iter().all(|p|p[3]==255);}
        }
        check(cancel)?;self.tainted=false;Ok(())
    }
    pub fn decoded_pixels(&self,max_bytes:u64,cancel:&AtomicBool)->Result<Pixels>{
        check(cancel)?;if self.tainted{return Err(Error::Invalid("Cancelled native composite cannot be read"))}let extent=self.width as u64*self.height as u64*4;if extent>max_bytes||extent>usize::MAX as u64{return Err(Error::Budget{bytes:extent,limit:max_bytes})}let mut rgba=vec![0;extent as usize];let across=self.width.div_ceil(4)as usize;
        for(index,block)in self.blocks.iter().enumerate(){check(cancel)?;let pixels=if block.prepared_valid{block.prepared_pixels}else{let mut pixels=decode_native_four_color_bgra_block(&block.compressed_color,cancel)?;if self.kind.native_flags()&2!=0&&!block.alpha_valid{for p in &mut pixels{p[3]=0;}}pixels};for(y,row)in pixels.chunks_exact(4).enumerate(){for(x,pixel)in row.iter().enumerate(){let(px,py)=((index%across)*4+x,(index/across)*4+y);if px<self.width as usize&&py<self.height as usize{let at=(py*self.width as usize+px)*4;rgba[at..at+4].copy_from_slice(&[pixel[2],pixel[1],pixel[0],pixel[3]]);}}}}
        Pixels::new(self.width,self.height,rgba,max_bytes)
    }
    /// Exact native output formats for the current explicit stage. Unprepared
    /// Cloth blocks preserve cached BC1 color and their native alpha state.
    pub fn export(&self,max_bytes:u64,cancel:&AtomicBool)->Result<NativeEncodedPlane>{
        check(cancel)?;if self.tainted{return Err(Error::Invalid("Cancelled native composite cannot be exported"))}if self.width%4!=0||self.height%4!=0{return Err(Error::Unsupported("Native composite export requires complete4x4 blocks"))}
        if self.kind==NativeCasCanvasKind::ClothWithAlphaGpu{let extent=self.blocks.len()as u64*16;if extent>max_bytes{return Err(Error::Budget{bytes:extent,limit:max_bytes})}let mut bytes=Vec::with_capacity(extent as usize);for block in &self.blocks{check(cancel)?;let encoded=if block.prepared_valid{crate::native_bc3::encode_native_cas_bc3_block(&block.prepared_pixels,cancel)?}else{crate::native_bc3::export_native_cas_unprepared_bc3_block(&block.compressed_color,block.alpha_valid,cancel)?};bytes.extend(encoded);}Ok(NativeEncodedPlane{width:self.width,height:self.height,dxgi_format:77,row_pitch:self.width as usize/4*16,bytes})}
        else{let pixels=self.decoded_pixels(max_bytes,cancel)?;let mut bytes=pixels.rgba;for p in bytes.chunks_exact_mut(4){p.swap(0,2);}Ok(NativeEncodedPlane{width:self.width,height:self.height,dxgi_format:87,row_pitch:self.width as usize*4,bytes})}
    }
}
#[cfg(test)]mod tests{
    use super::*;
    fn mip(blocks:Vec<NativeCompressedImageBlock>)->NativeRle2Mip{NativeRle2Mip{width:blocks.len()as u32*4,height:4,blocks,stream_ranges:std::array::from_fn(|_|0..0)}}
    #[test]fn retains_unprepared_source_color_across_noops_and_only_recompresses_prepared_blocks(){
        let cancel=AtomicBool::new(false);let mut canvas=NativeCompositeCanvas::new(8,4,NativeCasCanvasKind::ClothWithAlphaGpu,1024,&cancel).unwrap();let clear=canvas.export(1024,&cancel).unwrap();assert_eq!(clear.bytes,[0,5,0,0,0,0,0,0,0,0,0,0,0,0,0,0].repeat(2));
        let original=[0,0,255,255,0xe4,0xe4,0xe4,0xe4];canvas.apply_rle2_mip(&mip(vec![NativeCompressedImageBlock::Bc1(original),NativeCompressedImageBlock::Skip]),NativeRle2Operation::UnshiftedOrdinaryBlend{opacity:1.0},&cancel).unwrap();assert!(!canvas.blocks[0].prepared_valid);assert!(canvas.blocks[0].alpha_valid);assert_eq!(canvas.blocks[0].compressed_color,[255,255,0,0,0xb1,0xb1,0xb1,0xb1]);let exported=canvas.export(1024,&cancel).unwrap();assert_eq!(&exported.bytes[8..16],&canvas.blocks[0].compressed_color);assert_eq!(&exported.bytes[..8],&[0,5,255,255,255,255,255,255]);assert_eq!(&exported.bytes[16..],&clear.bytes[16..]);
        let before=exported.bytes;canvas.apply_rle2_mip(&mip(vec![NativeCompressedImageBlock::Skip,NativeCompressedImageBlock::Skip]),NativeRle2Operation::Shadow,&cancel).unwrap();assert_eq!(canvas.export(1024,&cancel).unwrap().bytes,before);
        let transparent=[0,0,255,255,255,255,255,255];canvas.apply_rle2_mip(&mip(vec![NativeCompressedImageBlock::Skip,NativeCompressedImageBlock::Bc1(transparent)]),NativeRle2Operation::Shadow,&cancel).unwrap();assert!(!canvas.blocks[0].prepared_valid);assert!(canvas.blocks[1].prepared_valid);assert_eq!(canvas.blocks[1].prepared_pixels,[[0;4];16]);
    }
    #[test]fn initialized_decoded_output_and_failure_preserve_meaningful_state(){let cancel=AtomicBool::new(false);for kind in[NativeCasCanvasKind::OverlayGpu,NativeCasCanvasKind::SkinToneAndMakeupGpu,NativeCasCanvasKind::ClothWithAlphaGpu]{let mut canvas=NativeCompositeCanvas::new(4,4,kind,1024,&cancel).unwrap();assert_eq!(canvas.decoded_pixels(64,&cancel).unwrap().rgba,kind.initial_bgra().repeat(16));assert!(canvas.apply_rle2_mip(&mip(vec![]),NativeRle2Operation::Shadow,&cancel).is_err());assert!(!canvas.is_tainted());assert!(canvas.export(1,&cancel).is_err());assert!(!canvas.is_tainted());}}
}
