// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{Error, ReadLimits, thumbnail_decode::*};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, Rgba};
use sha2::{Digest, Sha256};
use std::{io::Cursor, sync::atomic::AtomicBool};
fn fixture(alpha: bool, dimensions: [u32; 2]) -> Vec<u8> {
    let color =
        ImageBuffer::<Rgb<u8>, _>::from_fn(2, 2, |x, y| Rgb([(x * 89) as u8, (y * 137) as u8, 41]));
    let mut jpg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 90)
        .encode_image(&DynamicImage::ImageRgb8(color))
        .unwrap();
    assert_eq!(&jpg[..6], &[0xff, 0xd8, 0xff, 0xe0, 0, 16]);
    if !alpha {
        return jpg;
    }
    let alpha = ImageBuffer::<Rgba<u8>, _>::from_fn(dimensions[0], dimensions[1], |x, y| {
        Rgba([((y * 2 + x) * 71) as u8, 234, 57, 0])
    });
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(alpha)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    let png = encoded.into_inner();
    let mut segment = vec![0xff, 0xe0];
    segment.extend(((png.len() + 10) as u16).to_be_bytes());
    segment.extend(b"ALFA");
    segment.extend((png.len() as u32).to_be_bytes());
    segment.extend(png);
    jpg.splice(20..20, segment);
    jpg
}
#[test]
fn alfa_red_channel_supplies_alpha_without_changing_jpeg_color() {
    let bytes = fixture(true, [2, 2]);
    let cancel = AtomicBool::new(false);
    let image = decode_pinned_thumbnail(
        &bytes,
        Sha256::digest(&bytes).into(),
        ReadLimits::default(),
        &cancel,
    )
    .unwrap();
    assert_eq!(image.alpha, ThumbnailAlphaMode::EmbeddedAlfaPngRed);
    assert_eq!([image.width, image.height], [2, 2]);
    assert_eq!(
        image.rgba.chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>(),
        vec![0, 71, 142, 213]
    );
    let color = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
        .unwrap()
        .into_rgba8();
    for (actual, expected) in image.rgba.chunks_exact(4).zip(color.chunks_exact(4)) {
        assert_eq!(&actual[..3], &expected[..3]);
    }
    let opaque = decode_thumbnail(&fixture(false, [2, 2]), ReadLimits::default(), &cancel).unwrap();
    assert_eq!(opaque.alpha, ThumbnailAlphaMode::OpaqueJpeg);
    assert!(opaque.rgba.chunks_exact(4).all(|p| p[3] == 255));
    // Original s4pi ToJFIF writes its own JFIFtail and later copies the
    // original eight-byteJFIFtail again after ALFA; restore the originalJPEG.
    let mut writer = bytes.clone();
    let end = 32 + u32::from_be_bytes(writer[28..32].try_into().unwrap()) as usize;
    let tail: Vec<u8> = writer[12..20].to_vec();
    writer.splice(end..end, tail);
    writer[12..20].copy_from_slice(&[1, 0, 0, 1, 0, 1, 0, 0]);
    let writer_decoded = decode_thumbnail(&writer, ReadLimits::default(), &cancel).unwrap();
    assert_eq!(writer_decoded.rgba, image.rgba);
    assert_eq!(
        external_cas_assets::thumbnail::image_info(&writer).dimensions,
        Some([2, 2])
    );
}
#[test]
fn invalid_alpha_is_explicit_instead_of_silently_becoming_opaque() {
    let cancel = AtomicBool::new(false);
    assert!(decode_thumbnail(&fixture(true, [1, 2]), ReadLimits::default(), &cancel).is_err());
    let mut wrong = fixture(true, [2, 2]);
    wrong.splice(20..20, [0xff, 0xe1, 0, 2]);
    assert!(matches!(
        decode_thumbnail(&wrong, ReadLimits::default(), &cancel),
        Err(Error::Unsupported(_))
    ));
    let mut bad = fixture(true, [2, 2]);
    bad[31] ^= 1;
    assert!(decode_thumbnail(&bad, ReadLimits::default(), &cancel).is_err());
    let mut short = fixture(true, [2, 2]);
    short.truncate(short.len() - 2);
    assert!(decode_thumbnail(&short, ReadLimits::default(), &cancel).is_err());
    assert!(matches!(
        decode_thumbnail(
            &fixture(true, [2, 2]),
            ReadLimits {
                max_resource_bytes: 16
            },
            &cancel
        ),
        Err(Error::Budget { .. })
    ));
    assert!(matches!(
        decode_pinned_thumbnail(
            &fixture(true, [2, 2]),
            [0; 32],
            ReadLimits::default(),
            &cancel
        ),
        Err(Error::Changed)
    ));
    assert!(matches!(
        decode_thumbnail(&[], ReadLimits::default(), &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
}
#[test]
fn png_alpha_is_preserved_and_animation_is_not_flattened() {
    let mut png = Cursor::new(Vec::new());
    let image = ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 3, Rgba([5, 13, 29, 71]));
    DynamicImage::ImageRgba8(image)
        .write_to(&mut png, ImageFormat::Png)
        .unwrap();
    let mut png = png.into_inner();
    let decoded = decode_thumbnail(&png, ReadLimits::default(), &AtomicBool::new(false)).unwrap();
    assert_eq!(decoded.alpha, ThumbnailAlphaMode::Png);
    assert_eq!(decoded.rgba, [5, 13, 29, 71].repeat(6));
    // The unsupported animation header is detected before codec CRC handling.
    png.splice(
        33..33,
        [
            0, 0, 0, 8, b'a', b'c', b'T', b'L', 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
    );
    assert!(matches!(
        decode_thumbnail(&png, ReadLimits::default(), &AtomicBool::new(false)),
        Err(Error::Unsupported(_))
    ));
}
