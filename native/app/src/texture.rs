//! The Rust-owned material core shares the qualified decoder with the viewport.
pub use external_cas_material::texture::*;

use external_cas_assets::{ReadLimits, ResourceKey, lrle::decode_lrle_cancellable};
use std::sync::atomic::AtomicBool;

/// Decode the selected resource's format, independent of whether its TGI was
/// declared by the CASP or chosen as one documented same-identity alternative.
pub(crate) fn decode_diffuse(
    key: ResourceKey,
    raw: &[u8],
    cancel: &AtomicBool,
) -> Result<Texture, String> {
    if key.type_id != 0x2bc0_4edf {
        return decode(raw);
    }
    let image = decode_lrle_cancellable(raw, ReadLimits::default(), cancel)
        .map_err(|error| error.to_string())?;
    Ok(Texture {
        width: image.width,
        height: image.height,
        rgba: image.rgba,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_lrle() -> Vec<u8> {
        // Original generated 4×4 mip, one repeated opaque nonuniform color.
        let mut raw = vec![0; 20];
        raw[..4].copy_from_slice(b"LRLE");
        raw[8..10].copy_from_slice(&4u16.to_le_bytes());
        raw[10..12].copy_from_slice(&4u16.to_le_bytes());
        raw[12..16].copy_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&[0x42, 19, 73, 211, 255]);
        raw
    }

    #[test]
    fn declared_lrle_uses_the_native_decoder_and_keeps_source_bytes() {
        let raw = legacy_lrle();
        let original = raw.clone();
        let key = ResourceKey::new(0x2bc0_4edf, 9, 11);
        let decoded = decode_diffuse(key, &raw, &AtomicBool::new(false)).unwrap();
        assert_eq!((decoded.width, decoded.height), (4, 4));
        assert_eq!(decoded.rgba, [211, 73, 19, 255].repeat(16));
        assert_eq!(raw, original);
    }

    #[test]
    fn selected_lrle_rejects_malformed_payload_and_honors_cancellation() {
        let raw = legacy_lrle();
        let key = ResourceKey::new(0x2bc0_4edf, 9, 11);
        let cancel = AtomicBool::new(false);
        assert!(decode_diffuse(key, &raw[..24], &cancel).is_err());
        assert!(decode_diffuse(key, &raw, &AtomicBool::new(true)).is_err());
    }
}
