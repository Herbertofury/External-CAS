// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded first-mip ATI2/BC5 UNORM decoder. Both channels are retained literally;
//! color/alpha semantics are a separate caller-selected presentation operation.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub dimensions: [u32; 2],
    pub four_cc: [u8; 4],
    pub mip_levels: u32,
}
pub fn header(bytes: &[u8]) -> Result<Header, String> {
    if bytes.len() < 128 || bytes.get(..4) != Some(b"DDS ") {
        return Err("truncated or invalid DDS header".into());
    }
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    if word(4) != 124 || word(76) != 32 {
        return Err("unsupported DDS header layout".into());
    }
    let dimensions = [word(16), word(12)];
    if dimensions.iter().any(|n| *n == 0 || *n > 8192)
        || u64::from(dimensions[0]) * u64::from(dimensions[1]) > 32 * 1024 * 1024
    {
        return Err("DDS dimensions exceed decode budget".into());
    }
    if word(112) != 0 || word(24) > 1 {
        return Err("DDS cubemap/volume UI atlas is unsupported".into());
    }
    let mip_levels = word(28).max(1);
    if mip_levels > 14 {
        return Err("DDS mip count exceeds supported extent".into());
    }
    Ok(Header {
        dimensions,
        four_cc: bytes[84..88].try_into().unwrap(),
        mip_levels,
    })
}

/// RGBA8 with source R/G, zero B and opaque A. No normal reconstruction,
/// luminance swizzle, sRGB conversion or alpha assignment is silently applied.
pub fn decode_bc5(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let h = header(bytes)?;
    if &h.four_cc != b"ATI2" && &h.four_cc != b"BC5U" {
        return Err("DDS is not unsigned two-channel BC5".into());
    }
    let width = h.dimensions[0] as usize;
    let height = h.dimensions[1] as usize;
    let blocks_x = width.div_ceil(4);
    let blocks_y = height.div_ceil(4);
    let mut needed = 128;
    for mip in 0..h.mip_levels {
        needed += (width >> mip).max(1).div_ceil(4) * (height >> mip).max(1).div_ceil(4) * 16;
    }
    if needed > bytes.len() {
        return Err("truncated BC5 mip stream".into());
    }
    let mut rgba = vec![0; width * height * 4];
    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            let at = 128 + (block_y * blocks_x + block_x) * 16;
            let red = channel(&bytes[at..at + 8]);
            let green = channel(&bytes[at + 8..at + 16]);
            for y in 0..4 {
                for x in 0..4 {
                    let px = block_x * 4 + x;
                    let py = block_y * 4 + y;
                    if px >= width || py >= height {
                        continue;
                    }
                    let dst = (py * width + px) * 4;
                    rgba[dst..dst + 4].copy_from_slice(&[red[y * 4 + x], green[y * 4 + x], 0, 255]);
                }
            }
        }
    }
    Ok(rgba)
}
fn channel(block: &[u8]) -> [u8; 16] {
    let a = u16::from(block[0]);
    let b = u16::from(block[1]);
    let mut table = [0u8; 8];
    table[0] = a as u8;
    table[1] = b as u8;
    if a > b {
        for i in 1..=6 {
            table[i + 1] = (((7 - i) as u16 * a + i as u16 * b) / 7) as u8;
        }
    } else {
        for i in 1..=4 {
            table[i + 1] = (((5 - i) as u16 * a + i as u16 * b) / 5) as u8;
        }
        table[6] = 0;
        table[7] = 255;
    }
    let mut bits = 0u64;
    for (i, byte) in block[2..8].iter().enumerate() {
        bits |= u64::from(*byte) << (i * 8);
    }
    let mut result = [0; 16];
    for pixel in &mut result {
        *pixel = table[(bits & 7) as usize];
        bits >>= 3;
    }
    result
}

/// Explicit UI luminance/alpha interpretation. Callers must validate this
/// presentation against the installed source symbol; BC5 itself gives no alpha.
pub fn red_luminance_green_alpha(rg: &[u8]) -> Result<Vec<u8>, String> {
    if rg.len() % 4 != 0 {
        return Err("invalid RGBA channel extent".into());
    }
    Ok(rg
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[0], p[0], p[1]])
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data(width: u32, height: u32) -> Vec<u8> {
        let mut data = vec![0; 144];
        data[..4].copy_from_slice(b"DDS ");
        for (at, value) in [(4, 124u32), (12, height), (16, width), (28, 1), (76, 32)] {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[84..88].copy_from_slice(b"ATI2");
        data[128] = 255;
        data[129] = 0;
        data[136] = 80;
        data[137] = 100;
        data
    }
    #[test]
    fn literal_channels_and_partial_blocks() {
        let pixels = decode_bc5(&data(3, 2)).unwrap();
        assert_eq!(pixels.len(), 24);
        assert!(pixels.chunks_exact(4).all(|p| p == [255, 80, 0, 255]));
        assert_eq!(
            &red_luminance_green_alpha(&pixels).unwrap()[..4],
            &[255, 255, 255, 80]
        );
    }
    #[test]
    fn both_interpolation_branches_and_indices() {
        let mut block = [0u8; 8];
        block[..2].copy_from_slice(&[255, 0]);
        block[2] = 2;
        assert_eq!(channel(&block)[0], 218);
        block[..2].copy_from_slice(&[0, 100]);
        block[2] = 7;
        assert_eq!(channel(&block)[0], 255);
        block[2] = 6;
        assert_eq!(channel(&block)[0], 0);
    }
    #[test]
    fn truncation_and_bad_dimensions_fail() {
        let bytes = data(4, 4);
        for cut in 0..bytes.len() {
            assert!(decode_bc5(&bytes[..cut]).is_err());
        }
        assert!(decode_bc5(&data(0, 1)).is_err());
    }
}
