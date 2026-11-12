// SPDX-License-Identifier: GPL-3.0-or-later
//! Native THUM JPEG/PNG pixels, including s4pi's embedded ALFA PNG red plane.
//! Layout basis: Sims4Tools ThumbnailResource.cs, fff19365a12711879bad26481a393a6fbc62c465.
//! Codec work is bounded but not interruptible internally; call from a worker.
use crate::{Error, ReadLimits, Result};
use image::{ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    io::Cursor,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbnailAlphaMode {
    OpaqueJpeg,
    EmbeddedAlfaPngRed,
    Png,
}
#[derive(Clone, Debug)]
pub struct DecodedThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub alpha: ThumbnailAlphaMode,
    pub payload_sha256: [u8; 32],
}

/// Decode the original pinned bytes. EXIF orientation and ICC transforms are
/// deliberately not applied; native thumbnail selection remains separate.
pub fn decode_pinned_thumbnail(
    bytes: &[u8],
    expected_sha256: [u8; 32],
    limits: ReadLimits,
    cancel: &AtomicBool,
) -> Result<DecodedThumbnail> {
    cancelled(cancel)?;
    if <[u8; 32]>::from(Sha256::digest(bytes)) != expected_sha256 {
        return Err(Error::Changed);
    }
    decode_thumbnail(bytes, limits, cancel)
}
/// The resource budget applies to input, retained RGBA, and codec allocation
/// requests. The image crate's allocation limit is best effort, not a strict
/// bound on process peak memory. Strict dimensions are set before decoding.
pub fn decode_thumbnail(
    bytes: &[u8],
    limits: ReadLimits,
    cancel: &AtomicBool,
) -> Result<DecodedThumbnail> {
    cancelled(cancel)?;
    budget(bytes.len() as u64, limits)?;
    let (color_bytes, alpha_bytes) = if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg_color_and_alpha(bytes, cancel)?
    } else {
        (Cow::Borrowed(bytes), None)
    };
    let info = super::thumbnail::image_info(&color_bytes);
    let dimensions = info.dimensions.ok_or_else(|| {
        Error::unsupported(
            info.header_error
                .unwrap_or_else(|| "Thumbnail dimensions unavailable".into()),
        )
    })?;
    let (format, alpha_bytes, alpha) = match info.format {
        super::thumbnail::ThumbnailFormat::Jpeg => (
            ImageFormat::Jpeg,
            alpha_bytes,
            if alpha_bytes.is_some() {
                ThumbnailAlphaMode::EmbeddedAlfaPngRed
            } else {
                ThumbnailAlphaMode::OpaqueJpeg
            },
        ),
        super::thumbnail::ThumbnailFormat::Png => {
            validate_static_png(bytes, cancel)?;
            (ImageFormat::Png, None, ThumbnailAlphaMode::Png)
        }
        _ => {
            return Err(Error::unsupported(
                "Unrecognized thumbnail image; original bytes retained",
            ));
        }
    };
    let rgba_size = (dimensions[0] as u64)
        .checked_mul(dimensions[1] as u64)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| Error::invalid("Thumbnail output size overflow"))?;
    budget(
        rgba_size
            .checked_mul(if alpha_bytes.is_some() { 2 } else { 1 })
            .ok_or_else(|| Error::invalid("Thumbnail alpha size overflow"))?,
        limits,
    )?;
    let mut color = decode_image(&color_bytes, format, dimensions, limits)?;
    cancelled(cancel)?;
    if let Some(alpha_bytes) = alpha_bytes {
        validate_static_png(alpha_bytes, cancel)?;
        let alpha_info = super::thumbnail::image_info(alpha_bytes);
        if alpha_info.dimensions != Some(dimensions) {
            return Err(Error::invalid(
                "Thumbnail ALFA PNG dimensions differ from color image",
            ));
        }
        let plane = decode_image(alpha_bytes, ImageFormat::Png, dimensions, limits)?;
        cancelled(cancel)?;
        for (i, (color, plane)) in color
            .chunks_exact_mut(4)
            .zip(plane.chunks_exact(4))
            .enumerate()
        {
            if i % 4096 == 0 {
                cancelled(cancel)?;
            }
            color[3] = plane[0];
        }
    }
    cancelled(cancel)?;
    Ok(DecodedThumbnail {
        width: dimensions[0],
        height: dimensions[1],
        rgba: color,
        alpha,
        payload_sha256: Sha256::digest(bytes).into(),
    })
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn budget(requested: u64, limits: ReadLimits) -> Result<()> {
    if requested > limits.max_resource_bytes {
        Err(Error::Budget {
            requested,
            limit: limits.max_resource_bytes,
        })
    } else {
        Ok(())
    }
}
fn decode_image(
    bytes: &[u8],
    format: ImageFormat,
    dimensions: [u32; 2],
    limits: ReadLimits,
) -> Result<Vec<u8>> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut bound = image::Limits::default();
    bound.max_image_width = Some(dimensions[0]);
    bound.max_image_height = Some(dimensions[1]);
    bound.max_alloc = Some(limits.max_resource_bytes);
    reader.limits(bound);
    let image = reader
        .decode()
        .map_err(|e| Error::invalid(format!("Thumbnail pixel decoding: {e}")))?;
    if [image.width(), image.height()] != dimensions {
        return Err(Error::invalid(
            "Thumbnail decoder dimensions differ from inspected header",
        ));
    }
    Ok(image.into_rgba8().into_raw())
}
fn validate_static_png(bytes: &[u8], cancel: &AtomicBool) -> Result<()> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::invalid("Thumbnail ALFA image is not PNG"));
    }
    let mut at = 8usize;
    let mut ended = false;
    while at < bytes.len() {
        cancelled(cancel)?;
        let header = bytes
            .get(at..at + 8)
            .ok_or_else(|| Error::invalid("PNG chunk header"))?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .filter(|n| *n <= bytes.len())
            .ok_or_else(|| Error::invalid("PNG chunk extent"))?;
        if &header[4..8] == b"acTL" {
            return Err(Error::unsupported(
                "Animated thumbnail PNG is retained but not flattened silently",
            ));
        }
        if &header[4..8] == b"IEND" {
            if length != 0 || end != bytes.len() {
                return Err(Error::invalid("PNG trailing bytes or invalid IEND"));
            }
            ended = true;
            break;
        }
        at = end;
    }
    if !ended {
        return Err(Error::invalid("PNG end chunk missing"));
    }
    Ok(())
}
/// s4pi's writer copied the original JPEG's first12 bytes, emitted a synthetic
/// 8-byteJFIFtail + ALFA, then copied the original JPEG from byte12. Restore
/// that exact writer structure; do not skip arbitrary invalid JPEG bytes.
pub(crate) fn jpeg_color_and_alpha<'a>(
    bytes: &'a [u8],
    cancel: &AtomicBool,
) -> Result<(Cow<'a, [u8]>, Option<&'a [u8]>)> {
    if bytes.get(20..22) != Some(&[0xff, 0xe0]) || bytes.get(24..28) != Some(b"ALFA") {
        let alpha = jpeg_alpha(bytes, cancel)?;
        return Ok((Cow::Borrowed(bytes), alpha));
    }
    let size = u32::from_be_bytes(
        bytes
            .get(28..32)
            .ok_or_else(|| Error::invalid("Thumbnail ALFA size"))?
            .try_into()
            .unwrap(),
    ) as usize;
    let end = 32usize
        .checked_add(size)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| Error::invalid("Thumbnail ALFA extent"))?;
    let length = u16::from_be_bytes(bytes[22..24].try_into().unwrap()) as usize;
    if size < 8 || size.checked_add(10) != Some(length) {
        return Err(Error::invalid("Thumbnail ALFA segment length"));
    }
    let alpha = &bytes[32..end];
    if !alpha.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::invalid("Thumbnail ALFA is not PNG"));
    }
    let prefix_end = if bytes.get(end) == Some(&0xff) {
        20
    } else {
        let jfif = &[0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];
        let tail = bytes
            .get(end..end + 8)
            .ok_or_else(|| Error::invalid("s4pi JFIF tail extent"))?;
        if bytes.get(..12) != Some(jfif)
            || bytes.get(12..20) != Some(&[1, 0, 0, 1, 0, 1, 0, 0])
            || tail[0] > 2
            || tail[1] > 2
            || tail[6] != 0
            || tail[7] != 0
            || bytes.get(end + 8) != Some(&0xff)
        {
            return Err(Error::unsupported(
                "Unsupported thumbnail ALFA/JFIF writer layout; original bytes retained",
            ));
        }
        12
    };
    let mut color = Vec::with_capacity(prefix_end + bytes.len() - end);
    color.extend_from_slice(&bytes[..prefix_end]);
    color.extend_from_slice(&bytes[end..]);
    if jpeg_alpha(&color, cancel)?.is_some() {
        return Err(Error::unsupported("Multiple thumbnail ALFA images"));
    }
    Ok((Cow::Owned(color), Some(alpha)))
}
/// Walk all JPEG segments, including between progressive scans. ALFA outside
/// the documented APP0 at byte20 is explicitly unsupported, never opaque.
fn jpeg_alpha<'a>(bytes: &'a [u8], cancel: &AtomicBool) -> Result<Option<&'a [u8]>> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(Error::invalid("JPEG start marker"));
    }
    let mut at = 2;
    let mut alpha = None;
    let mut in_scan = false;
    let mut had_scan = false;
    while at < bytes.len() {
        cancelled(cancel)?;
        if in_scan {
            while at < bytes.len() && bytes[at] != 0xff {
                at += 1;
                if at % 4096 == 0 {
                    cancelled(cancel)?;
                }
            }
            if at >= bytes.len() {
                return Err(Error::invalid("JPEG scan has no end marker"));
            }
        }
        let start = at;
        if bytes[at] != 0xff {
            return Err(Error::invalid("JPEG marker boundary"));
        }
        while at < bytes.len() && bytes[at] == 0xff {
            at += 1;
        }
        let marker = *bytes
            .get(at)
            .ok_or_else(|| Error::invalid("JPEG marker extent"))?;
        at += 1;
        if in_scan && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
            continue;
        }
        in_scan = false;
        if marker == 0xd9 {
            if !had_scan || at != bytes.len() {
                return Err(Error::invalid("JPEG trailing bytes or missing scan"));
            }
            return Ok(alpha);
        }
        if marker == 0x01 {
            continue;
        }
        if marker == 0 || marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
            return Err(Error::unsupported("Unsupported JPEG marker layout"));
        }
        let length = u16::from_be_bytes(
            bytes
                .get(at..at + 2)
                .ok_or_else(|| Error::invalid("JPEG segment length"))?
                .try_into()
                .unwrap(),
        ) as usize;
        let end = at
            .checked_add(length)
            .filter(|end| length >= 2 && *end <= bytes.len())
            .ok_or_else(|| Error::invalid("JPEG segment extent"))?;
        let data = at + 2;
        if (0xe0..=0xef).contains(&marker) && end - data >= 4 {
            let tag = &bytes[data..data + 4];
            if tag == b"ALFA" {
                if alpha.is_some() || marker != 0xe0 || start != 20 || data != 24 || end - data < 8
                {
                    return Err(Error::unsupported(
                        "Unsupported thumbnail ALFA layout; original bytes retained",
                    ));
                }
                let size =
                    u32::from_be_bytes(bytes[data + 4..data + 8].try_into().unwrap()) as usize;
                if size < 8 || length != size + 10 || data + 8 + size != end {
                    return Err(Error::invalid("Thumbnail ALFA segment length"));
                }
                let embedded = &bytes[data + 8..end];
                if !embedded.starts_with(b"\x89PNG\r\n\x1a\n") {
                    return Err(Error::invalid("Thumbnail ALFA payload is not PNG"));
                }
                alpha = Some(embedded);
            } else if tag == b"ALFJ" || tag == b"ALFX" {
                return Err(Error::unsupported(
                    "JPEG-encoded or segmented alpha is not this THUM ALFA PNG format",
                ));
            }
        }
        at = end;
        if marker == 0xda {
            in_scan = true;
            had_scan = true;
        }
    }
    Err(Error::invalid("JPEG end marker missing"))
}
