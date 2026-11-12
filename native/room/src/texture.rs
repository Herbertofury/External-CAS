// SPDX-License-Identifier: GPL-3.0-or-later
use crate::Result;
pub fn decode_room_texture(source: &[u8]) -> Result<(u32, u32, Vec<u8>, String)> {
    if source.len() < 128 || source.len() > 64 * 1024 * 1024 || source.get(..4) != Some(b"DDS ") {
        return Err("room texture is not bounded DDS".into());
    }
    let word = |at| u32::from_le_bytes(source[at..at + 4].try_into().unwrap());
    if word(4) != 124 || word(76) != 32 {
        return Err("unsupported room DDS header".into());
    }
    let width = word(16) as usize;
    let height = word(12) as usize;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || width * height > 32 * 1024 * 1024
    {
        return Err("room DDS pixel budget".into());
    }
    if word(24) > 1 || word(112) & 0x20fe00 != 0 {
        return Err("room cubemap/volume DDS is unsupported".into());
    }
    if word(80) == 0x41
        && word(84) == 0
        && word(88) == 32
        && word(92) == 0x00ff0000
        && word(96) == 0x0000ff00
        && word(100) == 0x000000ff
        && word(104) == 0xff000000
    {
        let pitch = word(20) as usize;
        if word(8) & 8 == 0
            || pitch < width * 4
            || pitch > width * 4 + 65536
            || 128usize
                .checked_add(pitch.checked_mul(height).ok_or("room DDS pitch overflow")?)
                .is_none_or(|end| end > source.len())
        {
            return Err("room DDS pitch outside source".into());
        }
        let mut rgba = vec![0; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let at = 128 + y * pitch + x * 4;
                let to = (y * width + x) * 4;
                rgba[to..to + 4].copy_from_slice(&[
                    source[at + 2],
                    source[at + 1],
                    source[at],
                    source[at + 3],
                ]);
            }
        }
        return Ok((
            width as u32,
            height as u32,
            rgba,
            "source BGRA8 DDS first mip".into(),
        ));
    }
    let mut data = source.to_vec();
    let kind = &source[84..88];
    let conversion = if matches!(kind, b"DST1" | b"DST5") {
        let block = if kind == b"DST1" { 8 } else { 16 };
        let length = source.len() - 128;
        if length == 0 || length % block != 0 {
            return Err("DST stream extent".into());
        }
        let count = length / block;
        let (offsets, lengths): (&[usize], &[usize]) = if block == 8 {
            (&[0, count * 4], &[4, 4])
        } else {
            (&[0, count * 6, count * 2, count * 12], &[2, 6, 4, 4])
        };
        for i in 0..count {
            let mut to = 128 + i * block;
            for (&offset, &length) in offsets.iter().zip(lengths) {
                let from = 128 + offset + i * length;
                data[to..to + length].copy_from_slice(&source[from..from + length]);
                to += length;
            }
        }
        data[84..88].copy_from_slice(if block == 8 { b"DXT1" } else { b"DXT5" });
        "source-preserving DST unshuffle"
    } else {
        "standard DDS"
    };
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Dds);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("room DDS decoder: {e}"))?
        .into_rgba8();
    Ok((
        image.width(),
        image.height(),
        image.into_raw(),
        conversion.into(),
    ))
}
