// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{Error, ReadLimits, lrle::*};
use std::sync::atomic::AtomicBool;
fn fixture(kind: u8, width: u16, height: u16) -> Vec<u8> {
    let mut b = vec![0; 56];
    b[..8].copy_from_slice(b"LRLEV002");
    b[8..10].copy_from_slice(&width.to_le_bytes());
    b[10..12].copy_from_slice(&height.to_le_bytes());
    b[12..16].copy_from_slice(&9u32.to_le_bytes());
    let palette = if kind == 5 {
        vec![23; 130 * 4]
    } else {
        vec![1, 2, 3, 4, 51, 62, 73, 84]
    };
    let stream = match kind {
        0 => {
            let mut s = vec![67];
            s.extend(0..64);
            s
        }
        1 => vec![0x82, 1, 1],
        2 => {
            let mut s = vec![65];
            s.extend((0..16).map(|i| i % 2));
            s
        }
        3 => vec![0x86, 1, 51, 62, 73, 84],
        4 => vec![0x84, 1, 1, 0],
        _ => {
            let mut s = vec![65];
            for _ in 0..16 {
                s.extend([0x80, 1]);
            }
            s
        }
    };
    for i in 1..9 {
        b[16 + i * 4..20 + i * 4].copy_from_slice(&(stream.len() as u32).to_le_bytes());
    }
    b[52..56].copy_from_slice(&((palette.len() / 4) as u32).to_le_bytes());
    b.extend(palette);
    b.extend(stream);
    b
}
#[test]
fn every_lrle_command_preserves_pixels_alpha_and_source() {
    for kind in 0..6 {
        let b = fixture(kind, 4, 4);
        let old = b.clone();
        let image = decode_lrle_v2(&b, ReadLimits::default()).unwrap();
        assert_eq!(b, old);
        assert_eq!(image.rgba.len(), 64);
        assert_eq!((image.width, image.height), (4, 4));
        if kind == 1 || kind == 3 || kind == 4 {
            for p in image.rgba.chunks_exact(4) {
                assert_eq!(p, [73, 62, 51, 84]);
            }
        }
    }
    let b = fixture(1, 4, 4);
    for n in 0..b.len() {
        assert!(decode_lrle_v2(&b[..n], ReadLimits::default()).is_err());
    }
    let mut bad = b.clone();
    *bad.last_mut().unwrap() = 9;
    assert!(decode_lrle_v2(&bad, ReadLimits::default()).is_err());
    assert!(matches!(
        decode_lrle_v2(
            &b,
            ReadLimits {
                max_resource_bytes: 63
            }
        ),
        Err(Error::Budget { .. })
    ));
    assert!(matches!(
        decode_lrle_v2_cancellable(&b, ReadLimits::default(), &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
}
#[test]
fn tile_conversion_does_not_transpose_rows_or_blocks() {
    let mut b = vec![0; 24];
    b[..8].copy_from_slice(b"LRLEV002");
    b[8..10].copy_from_slice(&8u16.to_le_bytes());
    b[10..12].copy_from_slice(&4u16.to_le_bytes());
    b[12..16].copy_from_slice(&1u32.to_le_bytes());
    b.extend([0x83, 1]);
    for n in 0..32u8 {
        b.extend([n, 0, 0, 255]);
    }
    let image = decode_lrle_v2(&b, ReadLimits::default()).unwrap();
    for y in 0..4 {
        for x in 0..8 {
            let expected = (x / 4) * 16 + y * 4 + x % 4;
            assert_eq!(image.rgba[(y * 8 + x) * 4 + 2], expected as u8);
        }
    }
}

#[test]
fn legacy_lrle_runs_embedded_channels_and_failures() {
    fn image(stream: &[u8]) -> Vec<u8> {
        let mut b = vec![0; 20];
        b[..4].copy_from_slice(b"LRLE");
        b[8..10].copy_from_slice(&4u16.to_le_bytes());
        b[10..12].copy_from_slice(&4u16.to_le_bytes());
        b[12..16].copy_from_slice(&1u32.to_le_bytes());
        b.extend(stream);
        b
    }
    let mut literal = vec![65];
    literal.extend(0..64);
    let mut planar = vec![0x43, 0x81, 1];
    planar.extend(0..64);
    for stream in [
        vec![0x40],
        literal,
        vec![0x42, 1, 2, 3, 4],
        vec![0x43, 0x82, 2, 21],
        vec![0x43, 0x80, 2],
        planar,
    ] {
        let b = image(&stream);
        let old = b.clone();
        let decoded = decode_lrle(&b, ReadLimits::default()).unwrap();
        assert_eq!(decoded.rgba.len(), 64);
        assert_eq!(b, old);
        for n in 0..b.len() {
            assert!(decode_lrle(&b[..n], ReadLimits::default()).is_err());
        }
    }
    let repeated = decode_lrle(&image(&[0x42, 1, 2, 3, 4]), ReadLimits::default()).unwrap();
    assert_eq!(&repeated.rgba[..4], &[3, 2, 1, 4]);
    assert!(decode_lrle(&image(&[0x43, 0x81, 3]), ReadLimits::default()).is_err());
    assert!(decode_lrle(&image(&[0x00]), ReadLimits::default()).is_err());
    assert!(matches!(
        decode_lrle_cancellable(
            &image(&[0x40]),
            ReadLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(Error::Cancelled)
    ));
}
