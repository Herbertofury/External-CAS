// SPDX-License-Identifier: GPL-3.0-or-later
//! Read actual installed movie/atlas bytes through the existing pinned DBPF
//! reader. The caller selects the package provider; no global override guessing,
//! source writes, ActionScript execution, or extracted-asset disk cache occurs.
use crate::{fnv1_resource_name, ExternalImage, GfxMovie};
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Cursor, path::PathBuf};

pub const GFX_TYPE: u32 = 0x62ec_c59a;
pub const DDS_TYPE: u32 = 0x00b2_d882;
const MAX_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DECODED_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePin {
    pub provider_path: PathBuf,
    pub key: ResourceKey,
    pub ordinal: u32,
    pub index_sha256: String,
    pub payload_sha256: String,
}

/// Compare every accepted UI dependency against a previous observed session.
/// The caller obtains `observed` only by actual package reads on its worker.
/// Paths are duplicate-aware while accommodating ordinary/verbatim Windows
/// spelling. Missing/extra/duplicate resources and changed index/payload fail.
pub fn verify_pin_set(expected: &[SourcePin], observed: &[SourcePin]) -> Result<(), String> {
    if expected.is_empty() {
        return Err(
            "Saved installed UI has no verified source pins; select its source explicitly".into(),
        );
    }
    let identity = |pin: &SourcePin| (provider_identity(&pin.provider_path), pin.key, pin.ordinal);
    let mut expected_keys = BTreeSet::new();
    for pin in expected {
        if !expected_keys.insert(identity(pin)) {
            return Err("Saved installed UI contains duplicate dependency pins".into());
        }
    }
    let mut observed_keys = BTreeSet::new();
    for pin in observed {
        if !observed_keys.insert(identity(pin)) {
            return Err("Observed installed UI contains duplicate dependency pins".into());
        }
    }
    if expected_keys != observed_keys {
        return Err("Installed UI dependency providers or resources changed since the saved context; explicit source selection is required".into());
    }
    for old in expected {
        let new = observed
            .iter()
            .find(|pin| identity(pin) == identity(old))
            .ok_or("Missing installed UI dependency")?;
        if old.index_sha256 != new.index_sha256 || old.payload_sha256 != new.payload_sha256 {
            return Err(format!(
                "Installed UI source changed at {}; saved fonts/artwork were not restored",
                old.key
            ));
        }
    }
    Ok(())
}
fn provider_identity(path: &std::path::Path) -> String {
    let text = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if let Some(rest) = text.strip_prefix("//?/unc/") {
        format!("//{rest}")
    } else {
        text.strip_prefix("//?/").unwrap_or(&text).to_owned()
    }
}

#[derive(Debug)]
pub struct InstalledMovie {
    pub pin: SourcePin,
    pub source_bytes: Vec<u8>,
}
impl InstalledMovie {
    pub fn inspect(&self) -> Result<GfxMovie<'_>, String> {
        GfxMovie::inspect(&self.source_bytes).map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub struct InstalledAtlas {
    pub pin: SourcePin,
    pub resolved_name: String,
    pub dimensions: [u32; 2],
    /// `bc5-unorm-rg` retains literal channels; this alone does not establish
    /// the source UI renderer's luminance/color/alpha presentation semantics.
    pub pixel_encoding: String,
    /// Straight-alpha RGBA8 source pixels. No color-space conversion is assumed.
    pub rgba: Vec<u8>,
    pub source_bytes: Vec<u8>,
}

/// Bare library name from ImportAssets (remove the `.swf` suffix explicitly at
/// the call site). Conflicting matching ordinals fail rather than selecting one.
pub fn load_movie(index: &PackageIndex, bare_name: &str) -> Result<InstalledMovie, String> {
    let key = ResourceKey::new(GFX_TYPE, 0, fnv1_resource_name(bare_name));
    let (pin, source_bytes) = read_unique(index, key)?;
    GfxMovie::inspect(&source_bytes).map_err(|error| error.to_string())?;
    Ok(InstalledMovie { pin, source_bytes })
}

/// Resolve only name-derived DDS candidates in the explicitly selected package.
/// Both external filename and export name are retained; every matching provider
/// ordinal participates in ambiguity detection. No arbitrary first texture.
pub fn load_atlas(index: &PackageIndex, image: &ExternalImage) -> Result<InstalledAtlas, String> {
    if image.bitmap_format != 14 {
        return Err(format!(
            "external image format {} is preserved but not decoded",
            image.bitmap_format
        ));
    }
    let mut names = BTreeSet::new();
    for name in image
        .file_name
        .iter()
        .chain(std::iter::once(&image.export_name))
    {
        if !name.is_empty() {
            names.insert(name.to_ascii_lowercase());
            if let Some(stem) = name
                .strip_suffix(".dds")
                .or_else(|| name.strip_suffix(".DDS"))
            {
                names.insert(stem.to_ascii_lowercase());
            }
        }
    }
    let mut matches = Vec::new();
    let mut seen_keys = BTreeSet::new();
    for name in names {
        let instance = fnv1_resource_name(&name);
        for entry in index.entries.iter().filter(|entry| {
            entry.key.type_id == DDS_TYPE && entry.key.instance_id == instance && !entry.deleted()
        }) {
            if seen_keys.insert((entry.key, entry.ordinal)) {
                matches.push((name.clone(), entry.key, entry.ordinal));
            }
        }
    }
    // The installed UI has distinct resolution groups. Require dimensions from
    // the actual movie header; do not silently pick first/highest group number.
    let mut selected = Vec::new();
    for (name, key, _) in matches {
        let (pin, bytes) = read_unique(index, key)?;
        let header = crate::dds::header(&bytes)?;
        if header.dimensions == image.target_size.map(u32::from) {
            selected.push((name, pin, bytes));
        }
    }
    if selected.len() != 1 {
        return Err(format!("external image {:?} has {} name-derived DDS candidates matching source target dimensions {:?}; explicit resolution required",image.file_name,selected.len(),image.target_size));
    }
    let (resolved_name, pin, source_bytes) = selected.pop().unwrap();
    let header = crate::dds::header(&source_bytes)?;
    if matches!(&header.four_cc, b"ATI2" | b"BC5U") {
        let rgba = crate::dds::decode_bc5(&source_bytes)?;
        return Ok(InstalledAtlas {
            pin,
            resolved_name,
            dimensions: header.dimensions,
            pixel_encoding: "bc5-unorm-rg".into(),
            rgba,
            source_bytes,
        });
    }
    let mut reader =
        image::ImageReader::with_format(Cursor::new(&source_bytes), image::ImageFormat::Dds);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| format!("DDS atlas decoder: {error}"))?
        .into_rgba8();
    let dimensions = [decoded.width(), decoded.height()];
    if u64::from(dimensions[0]) * u64::from(dimensions[1]) * 4 > MAX_DECODED_BYTES {
        return Err("DDS atlas decoded size exceeds budget".into());
    }
    Ok(InstalledAtlas {
        pin,
        resolved_name,
        dimensions,
        pixel_encoding: "rgba8".into(),
        rgba: decoded.into_raw(),
        source_bytes,
    })
}

fn read_unique(index: &PackageIndex, key: ResourceKey) -> Result<(SourcePin, Vec<u8>), String> {
    let entries: Vec<_> = index
        .matching(key)
        .filter(|entry| !entry.deleted())
        .collect();
    if entries.len() != 1 {
        return Err(format!(
            "resource {key} has {} provider ordinals; explicit resolution required",
            entries.len()
        ));
    }
    let entry = entries[0];
    let bytes = index
        .read_resource(
            entry,
            ReadLimits {
                max_resource_bytes: MAX_RESOURCE_BYTES,
            },
        )
        .map_err(|error| error.to_string())?;
    let pin = SourcePin {
        provider_path: index.path.clone(),
        key,
        ordinal: entry.ordinal,
        index_sha256: hex(&index.index_sha256),
        payload_sha256: hex(&Sha256::digest(&bytes)),
    };
    Ok((pin, bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pin() -> SourcePin {
        SourcePin {
            provider_path: PathBuf::from(r"C:\Authorized\UI.package"),
            key: ResourceKey::new(GFX_TYPE, 0, 1),
            ordinal: 3,
            index_sha256: "11".repeat(32),
            payload_sha256: "22".repeat(32),
        }
    }
    #[test]
    fn pinned_restore_validates_all_resources_and_bytes() {
        let old = pin();
        let mut current = old.clone();
        current.provider_path = PathBuf::from(r"\\?\c:\authorized\UI.package");
        assert!(verify_pin_set(&[old.clone()], &[current.clone()]).is_ok());
        current.payload_sha256 = "33".repeat(32);
        assert!(verify_pin_set(&[old.clone()], &[current]).is_err());
        let mut changed = old.clone();
        changed.index_sha256 = "44".repeat(32);
        assert!(verify_pin_set(&[old.clone()], &[changed]).is_err());
        let mut changed = old.clone();
        changed.ordinal += 1;
        assert!(verify_pin_set(&[old.clone()], &[changed]).is_err());
        let mut changed = old.clone();
        changed.provider_path = PathBuf::from(r"C:\Different\UI.package");
        assert!(verify_pin_set(&[old.clone()], &[changed]).is_err());
        assert!(verify_pin_set(&[old.clone()], &[]).is_err());
        assert!(verify_pin_set(&[], &[old.clone()]).is_err());
        assert!(verify_pin_set(&[old.clone(), old.clone()], &[old.clone()]).is_err());
        assert!(verify_pin_set(&[old.clone()], &[old.clone(), old]).is_err());
    }
    #[test]
    fn source_pins_roundtrip_and_unc_identity() {
        let old = pin();
        let encoded = serde_json::to_vec(&old).unwrap();
        assert_eq!(serde_json::from_slice::<SourcePin>(&encoded).unwrap(), old);
        assert_eq!(
            provider_identity(std::path::Path::new(r"\\?\UNC\Server\Share\UI.package")),
            provider_identity(std::path::Path::new(r"\\server\share\UI.package"))
        );
    }
}
