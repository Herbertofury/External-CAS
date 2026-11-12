// SPDX-License-Identifier: GPL-3.0-or-later
// LRLE V002 format: recovered lrle-v2-local.mjs and SimRipper LRLE.cs.
// Original encoded bytes are never modified. Only mip zero is decoded.
use crate::{Bytes, Error, ReadLimits, Result};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    pub rgba: Vec<u8>,
}
pub fn decode_lrle(data: &[u8], limits: ReadLimits) -> Result<DecodedImage> {
    decode_lrle_cancellable(data, limits, &AtomicBool::new(false))
}
pub fn decode_lrle_cancellable(
    data: &[u8],
    limits: ReadLimits,
    cancel: &AtomicBool,
) -> Result<DecodedImage> {
    match data.get(4..8) {
        Some(b"V002") => decode_lrle_v2_cancellable(data, limits, cancel),
        Some([0, 0, 0, 0]) => decode_lrle_v1(data, limits, cancel),
        _ => Err(Error::unsupported("LRLE version")),
    }
}
fn decode_lrle_v1(data: &[u8], limits: ReadLimits, cancel: &AtomicBool) -> Result<DecodedImage> {
    limits.check(data.len() as u64)?;
    let mut r = Bytes::new(data);
    if r.take(4)? != b"LRLE" || r.u32()? != 0 {
        return Err(Error::invalid("legacy LRLE header"));
    }
    let width = r.u16()? as usize;
    let height = r.u16()? as usize;
    let mips = r.u32()?;
    if width == 0
        || height == 0
        || width % 4 != 0
        || height % 4 != 0
        || mips == 0
        || mips as usize > r.remaining() / 4
    {
        return Err(Error::unsupported("legacy LRLE dimensions or mip layout"));
    }
    let image_bytes = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| Error::invalid("legacy LRLE image size"))?;
    limits.check(image_bytes as u64)?;
    let (second, last) = mip_offsets(&mut r, mips, cancel)?;
    if last > r.remaining() {
        return Err(Error::invalid("legacy LRLE mip offsets"));
    }
    r.end = r.at + second.unwrap_or(r.remaining());
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(image_bytes)
        .map_err(|_| Error::invalid("LRLE pixel allocation"))?;
    rgba.resize(image_bytes, 0);
    let pixel_count = width * height;
    let mut written = 0usize;
    while r.remaining() > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let opcode = r.data[r.at];
        let kind = opcode & 3;
        let n = if kind == 1 || kind == 3 {
            r.u8()? as usize >> 2
        } else {
            variable(&mut r, 2)? as usize
        };
        if n == 0 || n > pixel_count - written {
            return Err(Error::invalid("legacy LRLE run extent"));
        }
        match kind {
            0 => {
                for _ in 0..n {
                    check_pixel_cancel(written, cancel)?;
                    put_pixel(&mut rgba, width, written, [0; 4]);
                    written += 1;
                }
            }
            1 => {
                let raw = r.take(n * 4)?;
                for b in raw.chunks_exact(4) {
                    put_pixel(&mut rgba, width, written, b.try_into().unwrap());
                    written += 1;
                }
            }
            2 => {
                let bgra: [u8; 4] = r.take(4)?.try_into().unwrap();
                for _ in 0..n {
                    check_pixel_cancel(written, cancel)?;
                    put_pixel(&mut rgba, width, written, bgra);
                    written += 1;
                }
            }
            3 => {
                let channels = embedded_rle(&mut r, n * 4)?;
                for i in 0..n {
                    put_pixel(
                        &mut rgba,
                        width,
                        written,
                        [
                            channels[i],
                            channels[i + n],
                            channels[i + 2 * n],
                            channels[i + 3 * n],
                        ],
                    );
                    written += 1;
                }
            }
            _ => unreachable!(),
        }
    }
    if written != pixel_count {
        return Err(Error::invalid("incomplete legacy LRLE mip"));
    }
    Ok(DecodedImage {
        width: width as u32,
        height: height as u32,
        mips,
        rgba,
    })
}
fn embedded_rle(r: &mut Bytes<'_>, required: usize) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(required);
    while result.len() < required {
        let opcode = r.u8()?;
        let bits = if opcode & 1 != 0 { 1 } else { 2 };
        let mut n = ((opcode & 0x7f) >> bits) as usize;
        if opcode & 0x80 != 0 {
            n += (r.u8()? as usize) << (7 - bits);
        }
        if n == 0 || n > required - result.len() {
            return Err(Error::invalid("legacy LRLE embedded run extent"));
        }
        if opcode & 1 != 0 {
            result.extend_from_slice(r.take(n)?);
        } else if opcode & 2 != 0 {
            let value = r.u8()?;
            result.resize(result.len() + n, value);
        } else {
            result.resize(result.len() + n, 0);
        }
    }
    Ok(result)
}
pub fn decode_lrle_v2(data: &[u8], limits: ReadLimits) -> Result<DecodedImage> {
    decode_lrle_v2_cancellable(data, limits, &AtomicBool::new(false))
}
pub fn decode_lrle_v2_cancellable(
    data: &[u8],
    limits: ReadLimits,
    cancel: &AtomicBool,
) -> Result<DecodedImage> {
    limits.check(data.len() as u64)?;
    let mut r = Bytes::new(data);
    if r.take(4)? != b"LRLE" {
        return Err(Error::invalid("LRLE magic"));
    }
    if r.take(4)? != b"V002" {
        return Err(Error::unsupported("LRLE version (requires V002)"));
    }
    let width = r.u16()? as usize;
    let height = r.u16()? as usize;
    let mips = r.u32()?;
    if width == 0
        || height == 0
        || width % 4 != 0
        || height % 4 != 0
        || mips == 0
        || mips as usize > r.remaining() / 4
    {
        return Err(Error::unsupported("LRLE dimensions or mip layout"));
    }
    let image_bytes = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| Error::invalid("LRLE image size"))?;
    limits.check(image_bytes as u64)?;
    let (second, last) = mip_offsets(&mut r, mips, cancel)?;
    let colors = r.count(4)?;
    let palette = r.take(colors * 4)?;
    let data_at = r.at;
    if last > r.remaining() {
        return Err(Error::invalid("LRLE mip extent"));
    }
    r.end = data_at + second.unwrap_or(r.remaining());
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(image_bytes)
        .map_err(|_| Error::invalid("LRLE pixel allocation"))?;
    rgba.resize(image_bytes, 0);
    let mut written = 0usize;
    let pixel_count = width * height;
    let color = |index: usize| -> Result<[u8; 4]> {
        palette
            .get(index * 4..index * 4 + 4)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| Error::invalid("LRLE palette index"))
    };
    while r.remaining() > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let opcode = r.data[r.at];
        let shift = if opcode & 3 == 3 || opcode & 3 == 1 {
            2
        } else {
            3
        };
        let n = variable(&mut r, shift)? as usize;
        if n == 0 || n > pixel_count - written {
            return Err(Error::invalid("LRLE run extent"));
        }
        if opcode & 3 == 3 {
            let bytes = r.take(
                n.checked_mul(4)
                    .ok_or_else(|| Error::invalid("LRLE literal size"))?,
            )?;
            for b in bytes.chunks_exact(4) {
                check_pixel_cancel(written, cancel)?;
                put_pixel(&mut rgba, width, written, b.try_into().unwrap());
                written += 1;
            }
        } else if opcode & 7 == 6 {
            let b: [u8; 4] = r.take(4)?.try_into().unwrap();
            for _ in 0..n {
                check_pixel_cancel(written, cancel)?;
                put_pixel(&mut rgba, width, written, b);
                written += 1;
            }
        } else if opcode & 3 == 1 {
            for _ in 0..n {
                check_pixel_cancel(written, cancel)?;
                let b = color(variable(&mut r, 0)? as usize)?;
                put_pixel(&mut rgba, width, written, b);
                written += 1;
            }
        } else if opcode & 7 == 2 || opcode & 7 == 4 {
            let index = if opcode & 7 == 2 {
                r.u8()? as usize
            } else {
                r.u16()? as usize
            };
            let b = color(index)?;
            for _ in 0..n {
                check_pixel_cancel(written, cancel)?;
                put_pixel(&mut rgba, width, written, b);
                written += 1;
            }
        } else {
            return Err(Error::unsupported(format!("LRLE opcode {opcode:#04X}")));
        }
    }
    if written != pixel_count {
        return Err(Error::invalid("incomplete LRLE mip"));
    }
    Ok(DecodedImage {
        width: width as u32,
        height: height as u32,
        mips,
        rgba,
    })
}
fn mip_offsets(
    r: &mut Bytes<'_>,
    mips: u32,
    cancel: &AtomicBool,
) -> Result<(Option<usize>, usize)> {
    let mut second = None;
    let mut last = 0;
    for i in 0..mips {
        if i % 1024 == 0 && cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let offset = r.u32()? as usize;
        if (i == 0 && offset != 0) || offset < last {
            return Err(Error::invalid("LRLE mip offsets"));
        }
        if i == 1 {
            second = Some(offset);
        }
        last = offset;
    }
    Ok((second, last))
}
fn variable(r: &mut Bytes<'_>, first_bits: u32) -> Result<u32> {
    let mut b = r.u8()?;
    let mut value = ((b & 127) >> first_bits) as u64;
    let mut shift = 7 - first_bits;
    let mut steps = 1;
    while b & 128 != 0 {
        if steps == 5 {
            return Err(Error::invalid("LRLE variable integer overflow"));
        }
        b = r.u8()?;
        value += ((b & 127) as u64) << shift;
        shift += 7;
        steps += 1;
    }
    u32::try_from(value).map_err(|_| Error::invalid("LRLE variable integer overflow"))
}
#[inline]
fn check_pixel_cancel(written: usize, cancel: &AtomicBool) -> Result<()> {
    if written & 0x3fff == 0 && cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn put_pixel(rgba: &mut [u8], width: usize, index: usize, bgra: [u8; 4]) {
    let block = index / 16;
    let in_block = index % 16;
    let blocks_per_row = width / 4;
    let x = (block % blocks_per_row) * 4 + in_block % 4;
    let y = (block / blocks_per_row) * 4 + in_block / 4;
    let at = (y * width + x) * 4;
    rgba[at..at + 4].copy_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
}
