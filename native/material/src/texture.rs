// SPDX-License-Identifier: GPL-3.0-or-later
// Ported from the recovered rle2.mjs format research. See THIRD_PARTY_NOTICES.md.
// Material auxiliary streams are reported, never guessed or written back.
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub warnings: Vec<String>,
}
const LIMIT: usize = 256 * 1024 * 1024;
fn u16le(b: &[u8], at: usize) -> Result<usize, String> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2)
            .ok_or("Truncated texture")?
            .try_into()
            .unwrap(),
    ) as usize)
}
fn u32le(b: &[u8], at: usize) -> Result<usize, String> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or("Truncated texture")?
            .try_into()
            .unwrap(),
    ) as usize)
}
pub fn decode(data: &[u8]) -> Result<Texture, String> {
    if data.get(..4) == Some(b"DXT5") && matches!(data.get(4..8), Some(b"RLE2" | b"RLES")) {
        return decode_rle(data);
    }
    if data.get(..4) == Some(b"DDS ") {
        return decode_dds(data);
    }
    let image = image::load_from_memory(data)
        .map_err(|e| format!("Unsupported image or texture: {e}"))?
        .into_rgba8();
    Ok(Texture {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        warnings: Vec::new(),
    })
}

/// Restore the complete shuffled block stream, including all mip levels.
/// The original source is never changed. No normal-channel remapping occurs.
/// Port of s4pi DSTResource.Unshuffle (GPL-3.0-or-later; pinned in README).
pub fn unshuffle_dst(data: &[u8]) -> Result<Option<Vec<u8>>, String> {
    if data.len() < 128 || data.len() > LIMIT || data.get(..4) != Some(b"DDS ") {
        return Err("Invalid bounded DDS input".into());
    }
    if u32le(data, 4)? != 124 || u32le(data, 76)? != 32 {
        return Err("Invalid DDS header".into());
    }
    let block = match &data[84..88] {
        b"DST1" => 8,
        b"DST5" => 16,
        b"DST3" => return Err("DST3 layout is unqualified; original source retained".into()),
        _ => return Ok(None),
    };
    if u32le(data, 80)? & 4 == 0 || u32le(data, 24)? > 1 || u32le(data, 112)? & 0x20fe00 != 0 {
        return Err("Unsupported DST pixel flags, cubemap or volume".into());
    }
    let width = u32le(data, 16)?;
    let height = u32le(data, 12)?;
    if width == 0 || height == 0 || width > 16384 || height > 16384 || width * height > LIMIT / 4 {
        return Err("DDS pixel budget".into());
    }
    let mips = u32le(data, 28)?.max(1);
    let maximum_mips = usize::BITS as usize - width.max(height).leading_zeros() as usize;
    if mips > maximum_mips {
        return Err("DDS mip count exceeds dimensions".into());
    }
    let mut expected = 0usize;
    for mip in 0..mips {
        expected = expected
            .checked_add(
                (width >> mip).max(1).div_ceil(4) * (height >> mip).max(1).div_ceil(4) * block,
            )
            .ok_or("DST mip extent overflow")?;
    }
    let length = data.len() - 128;
    if length != expected || length % block != 0 {
        return Err("DST complete mip extent mismatch".into());
    }
    let count = length / block;
    let (offsets, lengths): (&[usize], &[usize]) = if block == 8 {
        (&[0, count * 4], &[4, 4])
    } else {
        (&[0, count * 6, count * 2, count * 12], &[2, 6, 4, 4])
    };
    let mut output = data.to_vec();
    for i in 0..count {
        let mut to = 128 + i * block;
        for (&offset, &length) in offsets.iter().zip(lengths) {
            let from = 128 + offset + i * length;
            output[to..to + length].copy_from_slice(&data[from..from + length]);
            to += length;
        }
    }
    output[84..88].copy_from_slice(if block == 8 { b"DXT1" } else { b"DXT5" });
    Ok(Some(output))
}

fn decode_dds(data: &[u8]) -> Result<Texture, String> {
    let restored = unshuffle_dst(data)?;
    let decoded_source = restored.as_deref().unwrap_or(data);
    if restored.is_some() && data.get(84..88) == Some(b"DST5") {
        // Keep the already qualified native BC3 endpoint/interpolation policy
        // used by RLE2. Independent Pillow matches all five body normals byte
        // for byte; image's DXT path uses a different rounding convention.
        let width = u32le(data, 16)?;
        let height = u32le(data, 12)?;
        let end = 128 + width.div_ceil(4) * height.div_ceil(4) * 16;
        return Ok(Texture {
            width: width as u32,
            height: height as u32,
            rgba: decode_bc3(width, height, &decoded_source[128..end])?,
            warnings: Vec::new(),
        });
    }
    let mut reader = image::ImageReader::with_format(
        std::io::Cursor::new(decoded_source),
        image::ImageFormat::Dds,
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(LIMIT as u64);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("Unsupported DDS texture: {e}"))?
        .into_rgba8();
    Ok(Texture {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        warnings: Vec::new(),
    })
}
fn decode_rle(data: &[u8]) -> Result<Texture, String> {
    if data.len() < 36 || data.len() > LIMIT {
        return Err("Invalid RLE input extent".into());
    }
    let rles = &data[4..8] == b"RLES";
    let streams = if rles { 6 } else { 5 };
    let width = u16le(data, 8)?;
    let height = u16le(data, 10)?;
    let mips = u16le(data, 12)?;
    if width == 0 || height == 0 || width * height > LIMIT / 4 || mips == 0 || mips > 16 {
        return Err("Invalid RLE dimensions".into());
    }
    let header_end = 16 + mips * streams * 4;
    if header_end > data.len() {
        return Err("Truncated RLE headers".into());
    }
    let mut headers = Vec::with_capacity(mips + 1);
    for mip in 0..mips {
        let mut offsets = Vec::with_capacity(streams);
        for stream in 0..streams {
            offsets.push(u32le(data, 16 + (mip * streams + stream) * 4)?);
        }
        headers.push(offsets);
    }
    let mut sentinel = headers[0][1..].to_vec();
    sentinel.push(data.len());
    headers.push(sentinel);
    let mut first = None;
    let mut decoded_bytes = 0usize;
    for mip in 0..mips {
        let mut offsets = headers[mip].clone();
        let end = &headers[mip + 1];
        for stream in 0..streams {
            if offsets[stream] < header_end
                || offsets[stream] > end[stream]
                || end[stream] > data.len()
            {
                return Err("Invalid RLE stream extent".into());
            }
        }
        let w = (width >> mip).max(1);
        let h = (height >> mip).max(1);
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        decoded_bytes = decoded_bytes
            .checked_add(blocks * 16)
            .ok_or("RLE size overflow")?;
        if decoded_bytes > LIMIT || (end[0] - offsets[0]) % 2 != 0 {
            return Err("RLE decoded size or command extent".into());
        }
        let mut bc3 = vec![0u8; blocks * 16];
        let mut count = 0;
        while offsets[0] < end[0] {
            let command = u16le(data, offsets[0])?;
            offsets[0] += 2;
            let op = command & 3;
            let run = command >> 2;
            if op == 3 || count + run > blocks {
                return Err("Unsupported RLE command".into());
            }
            for _ in 0..run {
                let dst = count * 16;
                count += 1;
                match op {
                    0 => {
                        bc3[dst..dst + 8].copy_from_slice(&[0, 5, 0, 0, 0, 0, 0, 0]);
                        bc3[dst + 8..dst + 16].copy_from_slice(&[0, 5, 0, 0, 0, 0, 0, 0]);
                    }
                    1 => {
                        copy_stream(data, &mut offsets, end, 3, 2, &mut bc3[dst..dst + 2])?;
                        copy_stream(data, &mut offsets, end, 4, 6, &mut bc3[dst + 2..dst + 8])?;
                        copy_stream(data, &mut offsets, end, 1, 4, &mut bc3[dst + 8..dst + 12])?;
                        copy_stream(data, &mut offsets, end, 2, 4, &mut bc3[dst + 12..dst + 16])?;
                    }
                    2 => {
                        bc3[dst..dst + 8].copy_from_slice(&[0, 5, 255, 255, 255, 255, 255, 255]);
                        copy_stream(data, &mut offsets, end, 1, 4, &mut bc3[dst + 8..dst + 12])?;
                        copy_stream(data, &mut offsets, end, 2, 4, &mut bc3[dst + 12..dst + 16])?;
                    }
                    _ => unreachable!(),
                }
            }
        }
        let required = if rles { 3 } else { 5 };
        if count != blocks || (0..required).any(|i| offsets[i] != end[i]) {
            return Err("Incomplete RLE mip".into());
        }
        if rles {
            for i in 3..streams {
                decoded_bytes = decoded_bytes
                    .checked_add(end[i] - offsets[i])
                    .ok_or("RLES size overflow")?;
            }
        }
        if decoded_bytes > LIMIT {
            return Err("RLES decoded size limit".into());
        }
        if mip == 0 {
            first = Some(decode_bc3(w, h, &bc3)?);
        }
    }
    Ok(Texture {
        width: width as u32,
        height: height as u32,
        rgba: first.unwrap(),
        warnings: if rles {
            vec!["RLES auxiliary material streams are preserved in the source; their appearance is not yet interpreted.".into()]
        } else {
            Vec::new()
        },
    })
}
fn copy_stream(
    data: &[u8],
    offsets: &mut [usize],
    end: &[usize],
    stream: usize,
    len: usize,
    out: &mut [u8],
) -> Result<(), String> {
    let next = offsets[stream]
        .checked_add(len)
        .ok_or("RLE stream overflow")?;
    if next > end[stream] {
        return Err("Truncated RLE stream".into());
    }
    out.copy_from_slice(&data[offsets[stream]..next]);
    offsets[stream] = next;
    Ok(())
}
fn decode_bc3(width: usize, height: usize, blocks: &[u8]) -> Result<Vec<u8>, String> {
    let across = width.div_ceil(4);
    if blocks.len() != across * height.div_ceil(4) * 16 || width * height > LIMIT / 4 {
        return Err("BC3 extent".into());
    }
    let rgb = |v: usize| -> [u8; 3] {
        let r = (v >> 11) & 31;
        let g = (v >> 5) & 63;
        let b = v & 31;
        [
            ((r << 3) | (r >> 2)) as u8,
            ((g << 2) | (g >> 4)) as u8,
            ((b << 3) | (b >> 2)) as u8,
        ]
    };
    let mut rgba = vec![0; width * height * 4];
    for (block_index, block) in blocks.chunks_exact(16).enumerate() {
        let a0 = block[0] as u32;
        let a1 = block[1] as u32;
        let mut alphas = [a0, a1, 0, 0, 0, 0, 0, 255];
        if a0 > a1 {
            for k in 1..=6 {
                alphas[k + 1] = ((7 - k) as u32 * a0 + k as u32 * a1) / 7;
            }
        } else {
            for k in 1..=4 {
                alphas[k + 1] = ((5 - k) as u32 * a0 + k as u32 * a1) / 5;
            }
        }
        let mut alpha_bits = 0u64;
        for (i, &b) in block[2..8].iter().enumerate() {
            alpha_bits |= (b as u64) << (i * 8);
        }
        let c0 = rgb(u16le(block, 8)?);
        let c1 = rgb(u16le(block, 10)?);
        let colors = [
            c0,
            c1,
            std::array::from_fn(|k| ((2 * c0[k] as u16 + c1[k] as u16) / 3) as u8),
            std::array::from_fn(|k| ((c0[k] as u16 + 2 * c1[k] as u16) / 3) as u8),
        ];
        let bits = u32le(block, 12)? as u32;
        for y in 0..4 {
            for x in 0..4 {
                let px = (block_index % across) * 4 + x;
                let py = (block_index / across) * 4 + y;
                if px >= width || py >= height {
                    continue;
                }
                let bit = y * 4 + x;
                let color = colors[((bits >> (2 * bit)) & 3) as usize];
                let at = (py * width + px) * 4;
                rgba[at..at + 3].copy_from_slice(&color);
                rgba[at + 3] = alphas[((alpha_bits >> (3 * bit)) & 7) as usize] as u8;
            }
        }
    }
    Ok(rgba)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn opaque_red() -> Vec<u8> {
        let mut data = b"DXT5RLE2".to_vec();
        data.extend(4u16.to_le_bytes());
        data.extend(4u16.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(0u16.to_le_bytes());
        for offset in [36u32, 38, 42, 46, 46] {
            data.extend(offset.to_le_bytes());
        }
        data.extend(6u16.to_le_bytes());
        data.extend(0xf800u16.to_le_bytes());
        data.extend(0xf800u16.to_le_bytes());
        data.extend([0u8; 4]);
        data
    }
    #[test]
    fn dst_mip_stream_order_bounds_and_original_are_preserved() {
        for (block, fourcc) in [(8, b"DST1"), (16, b"DST5")] {
            let count = 4; // 8x4, 4x2 and 2x1 mips: 2+1+1 blocks.
            let mut header = vec![0u8; 128];
            header[..4].copy_from_slice(b"DDS ");
            for (at, value) in [
                (4, 124u32),
                (8, 0xa1007),
                (12, 4),
                (16, 8),
                (28, 3),
                (76, 32),
                (80, 4),
                (108, 0x401008),
            ] {
                header[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
            header[84..88].copy_from_slice(fourcc);
            let blocks = (0..count * block)
                .map(|i| (i * 7) as u8)
                .collect::<Vec<_>>();
            let lengths: &[usize] = if block == 8 { &[4, 4] } else { &[2, 6, 4, 4] };
            let orders: &[usize] = if block == 8 { &[0, 1] } else { &[0, 2, 1, 3] };
            let offsets = lengths
                .iter()
                .scan(0, |at, &length| {
                    let old = *at;
                    *at += length;
                    Some(old)
                })
                .collect::<Vec<_>>();
            let mut shuffled = header.clone();
            for &stream in orders {
                for i in 0..count {
                    shuffled.extend_from_slice(
                        &blocks[i * block + offsets[stream]
                            ..i * block + offsets[stream] + lengths[stream]],
                    );
                }
            }
            let original = shuffled.clone();
            let restored = unshuffle_dst(&shuffled).unwrap().unwrap();
            assert_eq!(&restored[128..], &blocks);
            assert_eq!(shuffled, original);
            assert_eq!(
                &restored[84..88],
                if block == 8 { b"DXT1" } else { b"DXT5" }
            );
            for n in 0..shuffled.len() {
                assert!(unshuffle_dst(&shuffled[..n]).is_err());
            }
            let mut extra = shuffled.clone();
            extra.push(0);
            assert!(unshuffle_dst(&extra).is_err());
            let mut bad = shuffled.clone();
            bad[112..116].copy_from_slice(&0x200u32.to_le_bytes());
            assert!(unshuffle_dst(&bad).is_err());
            let mut bad = shuffled.clone();
            bad[28..32].copy_from_slice(&20u32.to_le_bytes());
            assert!(unshuffle_dst(&bad).is_err());
        }
    }

    #[test]
    #[ignore = "requires explicit private CAS_NORMAL_QUALIFICATION manifest and independent original s4pi/Pillow outputs"]
    fn installed_normal_textures_match_original_s4pi_and_pillow() {
        use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
        use sha2::{Digest, Sha256};
        use std::sync::atomic::AtomicBool;
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var_os("CAS_NORMAL_QUALIFICATION").expect("private manifest"))
                .unwrap(),
        )
        .unwrap();
        let samples = manifest["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 5);
        for sample in samples {
            let index = PackageIndex::open(sample["source_path"].as_str().unwrap()).unwrap();
            let key = sample["key"]
                .as_str()
                .unwrap()
                .parse::<ResourceKey>()
                .unwrap();
            let entries = index.matching(key).collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            let hash = sample["payload_sha256"].as_str().unwrap();
            let raw = index
                .read_resource(entries[0], ReadLimits::default())
                .unwrap();
            assert_eq!(format!("{:x}", Sha256::digest(&raw)), hash);
            let restored = unshuffle_dst(&raw).unwrap().unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&restored)),
                sample["oracle_dds_sha256"].as_str().unwrap()
            );
            let image = decode(&raw).unwrap();
            assert_eq!(image.width, sample["width"].as_u64().unwrap() as u32);
            assert_eq!(image.height, sample["height"].as_u64().unwrap() as u32);
            let expected = std::fs::read(sample["oracle_rgba"].as_str().unwrap()).unwrap();
            assert_eq!(image.rgba.len(), expected.len());
            let max_difference = image
                .rgba
                .iter()
                .zip(&expected)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap();
            eprintln!(
                "{key} full-mip restoration exact; RGBA/Pillow max={max_difference}; rgba_sha256={:x}",
                Sha256::digest(&image.rgba)
            );
            assert_eq!(max_difference, 0, "pixel differences for {key}");
            index.revalidate(&AtomicBool::new(false)).unwrap();
        }
    }

    #[test]
    fn opaque_bc3_and_rle_rounding() {
        let decoded = decode(&opaque_red()).unwrap();
        assert_eq!((decoded.width, decoded.height), (4, 4));
        assert!(decoded.rgba.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    }
    #[test]
    fn all_rle_truncations_rejected() {
        let data = opaque_red();
        for n in 0..data.len() {
            assert!(decode(&data[..n]).is_err(), "accepted {n}");
        }
        let mut data = data;
        data[36] = 7;
        assert!(decode(&data).is_err());
    }
    #[test]
    #[ignore = "requires explicit private installed-source paths; read-only"]
    fn real_installed_rle_pixels_match_recovered_oracle() {
        use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
        use sha2::{Digest, Sha256};
        let inputs: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var_os("CAS_CC_SAMPLES").expect("CAS_CC_SAMPLES")).unwrap(),
        )
        .unwrap();
        let evidence: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                std::env::var_os("CAS_CC_TEXTURE_EVIDENCE").expect("CAS_CC_TEXTURE_EVIDENCE"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut compared = 0;
        for sample in evidence["samples"].as_array().unwrap() {
            let records = sample["texturePixels"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["format"].as_str().unwrap().contains("BC3"))
                .collect::<Vec<_>>();
            if records.is_empty() {
                continue;
            }
            let label = sample["category"].as_str().unwrap();
            let path = inputs["samples"][label].as_str().unwrap();
            let package = PackageIndex::open(path).unwrap();
            let mut reader = package.reader().unwrap();
            for record in records {
                let key: ResourceKey = record["key"].as_str().unwrap().parse().unwrap();
                let entries = package.matching(key).collect::<Vec<_>>();
                assert_eq!(entries.len(), 1);
                let raw = reader
                    .read_resource(entries[0], ReadLimits::default())
                    .unwrap();
                assert_eq!(
                    format!("{:x}", Sha256::digest(&raw)),
                    record["payloadHash"].as_str().unwrap()
                );
                let decoded = decode(&raw).unwrap();
                assert_eq!(decoded.width, record["width"].as_u64().unwrap() as u32);
                assert_eq!(decoded.height, record["height"].as_u64().unwrap() as u32);
                assert_eq!(
                    format!("{:x}", Sha256::digest(&decoded.rgba)),
                    record["rgbaHash"].as_str().unwrap()
                );
                compared += 1;
            }
        }
        assert_eq!(
            compared, 5,
            "expected the explicitly qualified RLE2/RLES sample scope"
        );
    }
}
