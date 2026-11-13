// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded source-preserving RLE2 stream preparation for the independently
//! qualified native image/shadow operations. No RLES auxiliary stream is lost.
use crate::{check,Error,Pixels,Result,native_blend::native_image_opacity_weight,native_compressed_blend::{NativeCompressedImageBlock,blend_native_compressed_bgra_block},native_shadow::{NativeShadowBlock,apply_native_shadow_bgra_block}};
use std::{ops::Range,sync::{Arc,atomic::AtomicBool}};

#[derive(Clone,Debug)]pub struct NativeRle2Limits {pub max_source_bytes:u64,pub max_prepared_bytes:u64}
impl Default for NativeRle2Limits {fn default()->Self{Self{max_source_bytes:256*1024*1024,max_prepared_bytes:256*1024*1024}}}
#[derive(Clone,Debug)]pub struct NativeRle2Mip {pub width:u32,pub height:u32,pub blocks:Vec<NativeCompressedImageBlock>,pub stream_ranges:[Range<usize>;5]}
#[derive(Clone,Debug)]pub struct NativeRle2Image {pub width:u32,pub height:u32,pub original:Arc<[u8]>,mips:Vec<NativeRle2Mip>}
fn u16le(bytes:&[u8],at:usize)->Result<u16>{Ok(u16::from_le_bytes(bytes.get(at..at+2).ok_or(Error::Invalid("Truncated native RLE2"))?.try_into().unwrap()))}
fn u32le(bytes:&[u8],at:usize)->Result<usize>{Ok(u32::from_le_bytes(bytes.get(at..at+4).ok_or(Error::Invalid("Truncated native RLE2"))?.try_into().unwrap())as usize)}
fn stream<const N:usize>(bytes:&[u8],at:&mut usize,end:usize)->Result<[u8;N]>{let next=at.checked_add(N).filter(|&n|n<=end).ok_or(Error::Invalid("Truncated native RLE2 stream"))?;let value=bytes.get(*at..next).ok_or(Error::Invalid("Native RLE2 stream extent"))?.try_into().unwrap();*at=next;Ok(value)}
impl NativeRle2Image {
    pub fn parse(bytes:Arc<[u8]>,limits:&NativeRle2Limits,cancel:&AtomicBool)->Result<Self>{
        check(cancel)?;if bytes.len()as u64>limits.max_source_bytes{return Err(Error::Budget{bytes:bytes.len()as u64,limit:limits.max_source_bytes})}
        if bytes.get(..8)!=Some(b"DXT5RLE2"){return Err(Error::Unsupported("Native RLE2 requires exact DXT5/RLE2 header; RLES remains opaque"))}
        let width=u16le(&bytes,8)?as u32;let height=u16le(&bytes,10)?as u32;let mip_count=u16le(&bytes,12)?as usize;
        if u16le(&bytes,14)?!=0{return Err(Error::Unsupported("Native RLE2 reserved header must be zero"))}
        if width==0||height==0||mip_count==0||mip_count>16{return Err(Error::Invalid("Native RLE2 dimensions/mip count"))}
        let header_end=16+mip_count*20;if header_end>bytes.len(){return Err(Error::Invalid("Truncated native RLE2 mip table"))}
        let mut headers=Vec::with_capacity(mip_count+1);
        for mip in 0..mip_count{let mut header=[0;5];for(i,offset)in header.iter_mut().enumerate(){*offset=u32le(&bytes,16+mip*20+i*4)?;}headers.push(header);}
        // Native loader19E9E50 appends the first mip's next four stream starts
        // and payload end as the terminal five-pointer record.
        headers.push([headers[0][1],headers[0][2],headers[0][3],headers[0][4],bytes.len()]);
        if headers[0][0]!=header_end{return Err(Error::Unsupported("Native RLE2 leading source padding is not qualified"))}
        let mut mips=Vec::with_capacity(mip_count);let mut prepared=0u64;
        for mip in 0..mip_count{
            check(cancel)?;let mut at=headers[mip];let end=headers[mip+1];
            for i in 0..5 {if at[i]<header_end||at[i]>end[i]||end[i]>bytes.len(){return Err(Error::Invalid("Native RLE2 stream offset extent"))}}
            if (end[0]-at[0])%2!=0{return Err(Error::Invalid("Native RLE2 odd command stream"))}
            let w=(width>>mip).max(1);let h=(height>>mip).max(1);let count=w.div_ceil(4)as usize*h.div_ceil(4)as usize;
            prepared=prepared.checked_add(count as u64*std::mem::size_of::<NativeCompressedImageBlock>()as u64).ok_or(Error::Invalid("Native RLE2 prepared extent overflow"))?;
            if prepared>limits.max_prepared_bytes{return Err(Error::Budget{bytes:prepared,limit:limits.max_prepared_bytes})}
            let mut blocks=Vec::with_capacity(count);
            while at[0]<end[0]{
                check(cancel)?;let command=u16::from_le_bytes(stream::<2>(&bytes,&mut at[0],end[0])?);let op=command&3;let run=(command>>2)as usize;
                if op==3||blocks.len().checked_add(run).is_none_or(|n|n>count){return Err(Error::Invalid("Native RLE2 command run/operation"))}
                for _ in 0..run {
                    let block=match op {
                        0=>NativeCompressedImageBlock::Skip,
                        1=>{let mut block=[0;16];block[..2].copy_from_slice(&stream::<2>(&bytes,&mut at[3],end[3])?);block[2..8].copy_from_slice(&stream::<6>(&bytes,&mut at[4],end[4])?);block[8..12].copy_from_slice(&stream::<4>(&bytes,&mut at[1],end[1])?);block[12..].copy_from_slice(&stream::<4>(&bytes,&mut at[2],end[2])?);NativeCompressedImageBlock::Bc3(block)},
                        2=>{let mut block=[0;8];block[..4].copy_from_slice(&stream::<4>(&bytes,&mut at[1],end[1])?);block[4..].copy_from_slice(&stream::<4>(&bytes,&mut at[2],end[2])?);NativeCompressedImageBlock::Bc1(block)},
                        _=>unreachable!(),
                    };blocks.push(block);
                }
            }
            if blocks.len()!=count||at!=end{return Err(Error::Invalid("Native RLE2 incomplete mip/unused stream data"))}
            mips.push(NativeRle2Mip{width:w,height:h,blocks,stream_ranges:std::array::from_fn(|i|headers[mip][i]..end[i])});
        }
        check(cancel)?;Ok(Self{width,height,original:bytes,mips})
    }
    pub fn mips(&self)->&[NativeRle2Mip]{&self.mips}
}
#[derive(Clone,Copy,Debug,PartialEq)]pub enum NativeRle2Operation {UnshiftedOrdinaryBlend{opacity:f32},Shadow}
/// Produce a new plane from explicit existing RGBA pixels; failure/cancellation
/// never changes the caller's destination. No canvas, mip, flags or order is
/// chosen. The compressed source and prepared mip remain unchanged.
pub fn apply_native_rle2_mip(base:&Pixels,mip:&NativeRle2Mip,operation:NativeRle2Operation,max_output_bytes:u64,cancel:&AtomicBool)->Result<Pixels>{
    check(cancel)?;if let NativeRle2Operation::UnshiftedOrdinaryBlend{opacity}=operation{native_image_opacity_weight(opacity)?;}
    let extent=(base.width as u64).checked_mul(base.height as u64).and_then(|n|n.checked_mul(4)).ok_or(Error::Invalid("Native RLE2 destination overflow"))?;
    if extent>max_output_bytes{return Err(Error::Budget{bytes:extent,limit:max_output_bytes})}
    let across=base.width.div_ceil(4)as usize;let count=across.checked_mul(base.height.div_ceil(4)as usize).ok_or(Error::Invalid("Native RLE2 destination block overflow"))?;
    if base.width==0||base.height==0||base.width!=mip.width||base.height!=mip.height||base.rgba.len()as u64!=extent||mip.blocks.len()!=count{return Err(Error::Invalid("Native RLE2 prepared destination extent mismatch"))}
    let mut output=base.clone();
    for(index,source)in mip.blocks.iter().enumerate(){
        check(cancel)?;if matches!(source,NativeCompressedImageBlock::Skip){continue}
        let mut block=[[0;4];16];let ox=index%across*4;let oy=index/across*4;
        for(y,row)in block.chunks_exact_mut(4).enumerate(){for(x,pixel)in row.iter_mut().enumerate(){let(px,py)=(ox+x,oy+y);if px<base.width as usize&&py<base.height as usize{let at=(py*base.width as usize+px)*4;*pixel=[base.rgba[at+2],base.rgba[at+1],base.rgba[at],base.rgba[at+3]];}}}
        let result=match operation {
            NativeRle2Operation::UnshiftedOrdinaryBlend{opacity}=>blend_native_compressed_bgra_block(&block,source,opacity,cancel)?,
            NativeRle2Operation::Shadow=>{let source=match source{NativeCompressedImageBlock::Skip=>unreachable!(),NativeCompressedImageBlock::Bc1(b)=>NativeShadowBlock::Bc1(*b),NativeCompressedImageBlock::Bc3(b)=>NativeShadowBlock::Bc3Color(*b)};apply_native_shadow_bgra_block(&block,&source,cancel)?},
        };
        for(y,row)in result.chunks_exact(4).enumerate(){for(x,pixel)in row.iter().enumerate(){let(px,py)=(ox+x,oy+y);if px<base.width as usize&&py<base.height as usize{let at=(py*base.width as usize+px)*4;output.rgba[at..at+4].copy_from_slice(&[pixel[2],pixel[1],pixel[0],pixel[3]]);}}}
    }
    check(cancel)?;Ok(output)
}

#[cfg(test)]mod tests{
    use super::*;
    fn fixture()->Vec<u8>{
        let mut out=b"DXT5RLE2".to_vec();out.extend(4u16.to_le_bytes());out.extend(4u16.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(0u16.to_le_bytes());
        for offset in [36u32,38,42,46,48]{out.extend(offset.to_le_bytes());}
        out.extend(5u16.to_le_bytes());out.extend([255,255,255,255,0,0,0,0,0,255,255,255,255,255,255,255]);out
    }
    #[test]fn exact_split_streams_original_bytes_and_cancellable_copy_operation(){
        let cancel=AtomicBool::new(false);let bytes:Arc<[u8]>=fixture().into();let image=NativeRle2Image::parse(bytes.clone(),&Default::default(),&cancel).unwrap();assert!(Arc::ptr_eq(&bytes,&image.original));assert_eq!(image.mips.len(),1);assert_eq!(image.mips[0].blocks,vec![NativeCompressedImageBlock::Bc3([0,255,255,255,255,255,255,255,255,255,255,255,0,0,0,0])]);
        let base=Pixels::new(4,4,vec![10;64],64).unwrap();let original=base.rgba.clone();let result=apply_native_rle2_mip(&base,&image.mips[0],NativeRle2Operation::UnshiftedOrdinaryBlend{opacity:1.0},64,&cancel).unwrap();assert_eq!(result.rgba,vec![255;64]);assert_eq!(base.rgba,original);assert_eq!(image.original.as_ref(),fixture());
        let shadow=apply_native_rle2_mip(&base,&image.mips[0],NativeRle2Operation::Shadow,64,&cancel).unwrap();assert_eq!(shadow.rgba,base.rgba);
        assert!(apply_native_rle2_mip(&base,&image.mips[0],NativeRle2Operation::Shadow,63,&cancel).is_err());assert!(apply_native_rle2_mip(&base,&image.mips[0],NativeRle2Operation::Shadow,64,&AtomicBool::new(true)).is_err());assert_eq!(base.rgba,original);
    }
    #[test]fn rejects_truncation_overlaps_bad_runs_header_and_budget_before_composition(){
        let cancel=AtomicBool::new(false);let bytes=fixture();for end in 0..bytes.len(){assert!(NativeRle2Image::parse(Arc::from(&bytes[..end]),&Default::default(),&cancel).is_err(),"prefix{end}");}
        for(at,value)in [(14,1u8),(36,7),(37,1),(16,35),(20,37)]{let mut broken=bytes.clone();broken[at]=value;assert!(NativeRle2Image::parse(broken.into(),&Default::default(),&cancel).is_err());}
        assert!(NativeRle2Image::parse(bytes.clone().into(),&NativeRle2Limits{max_source_bytes:1,max_prepared_bytes:1024},&cancel).is_err());assert!(NativeRle2Image::parse(bytes.into(),&NativeRle2Limits{max_source_bytes:1024,max_prepared_bytes:0},&cancel).is_err());
    }
}
