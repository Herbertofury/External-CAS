// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-derived CAS gallery thumbnails. No images or derivatives are stored.
use crate::{
    CasPart, Catalog, Error, ReadLimits, ResourceKey, Result,
    dbpf::{CASP, THUMBNAIL},
    scan::Provider,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbnailModelHint {
    FemaleModel,
    MaleModel,
    Unknown,
}
pub fn model_hint(group: u32) -> ThumbnailModelHint {
    match group {
        0x00000002 => ThumbnailModelHint::FemaleModel,
        0x00000102 => ThumbnailModelHint::MaleModel,
        _ => ThumbnailModelHint::Unknown,
    }
}
#[derive(Clone, Debug)]
pub struct ThumbnailCandidate {
    pub key: ResourceKey,
    pub providers: Vec<Provider>,
    pub model_hint: ThumbnailModelHint,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbnailFormat {
    Jpeg,
    Png,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct ThumbnailImageInfo {
    pub format: ThumbnailFormat,
    pub dimensions: Option<[u32; 2]>,
    pub header_error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ThumbnailPayload {
    pub key: ResourceKey,
    pub provider: Provider,
    pub source_index_sha256: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub bytes: Vec<u8>,
    pub info: ThumbnailImageInfo,
}
#[derive(Clone, Debug)]
pub struct ThumbnailFailure {
    pub key: ResourceKey,
    pub provider: Provider,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct ThumbnailEquivalence {
    pub payload_sha256: [u8; 32],
    pub payload_indices: Vec<usize>,
}
#[derive(Clone, Debug, Default)]
pub struct ThumbnailInspection {
    pub complete: bool,
    pub payloads: Vec<ThumbnailPayload>,
    pub failures: Vec<ThumbnailFailure>,
    pub equivalences: Vec<ThumbnailEquivalence>,
}
impl ThumbnailInspection {
    /// Selecting an identical byte image does not choose game load precedence.
    pub fn equivalent_payload(&self) -> Option<&ThumbnailPayload> {
        if !self.complete || self.equivalences.len() != 1 {
            return None;
        }
        self.equivalences[0]
            .payload_indices
            .first()
            .and_then(|i| self.payloads.get(*i))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbnailRelationship {
    DeclaredParentDesign,
    SharedOutfitGeometryDesign,
}
#[derive(Clone, Debug)]
pub struct ThumbnailCaspSource {
    pub key: ResourceKey,
    pub provider: Provider,
    pub payload_sha256: Option<[u8; 32]>,
}
#[derive(Clone, Debug)]
pub struct DesignThumbnailCandidate {
    pub casp_key: ResourceKey,
    pub relationship: ThumbnailRelationship,
    pub casp_sources: Vec<ThumbnailCaspSource>,
    pub candidates: Vec<ThumbnailCandidate>,
}
#[derive(Clone, Debug, Default)]
pub struct ThumbnailChoices {
    pub exact: Vec<ThumbnailCandidate>,
    pub design: Vec<DesignThumbnailCandidate>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DesignKey {
    outfit_id: u32,
    body_type: u32,
    age: u8,
    gender: u8,
    species: Option<u32>,
    texture_space: u8,
    lod0: Vec<ResourceKey>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Registration {
    key: ResourceKey,
    payload_sha256: [u8; 32],
    parent: Option<ResourceKey>,
    design: Option<DesignKey>,
}
/// Build alongside parsed/CACHED CASP metadata, never by decoding every package
/// for a visible card. Keys use full LOD0 refs and source body context; name
/// heuristics and unscoped cross-package design substitution are not used.
#[derive(Default)]
pub struct ThumbnailDesignIndex {
    registrations: HashMap<Provider, Registration>,
    designs: HashMap<DesignKey, Vec<Provider>>,
}
impl ThumbnailDesignIndex {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(
        &mut self,
        key: ResourceKey,
        provider: Provider,
        payload_sha256: [u8; 32],
        part: &CasPart,
    ) -> Result<()> {
        if key.type_id != CASP {
            return Err(Error::invalid("thumbnail registration is not a CASP"));
        }
        let parent = part
            .roles
            .iter()
            .find(|r| r.role == "parent" && !r.key.is_empty())
            .map(|r| r.key);
        let design = if part.outfit_id != 0 && !part.lod0_meshes.is_empty() {
            Some(DesignKey {
                outfit_id: part.outfit_id,
                body_type: part.body_type,
                age: part.age,
                gender: part.gender,
                species: part.species,
                texture_space: part.texture_space,
                lod0: part.lod0_meshes.clone(),
            })
        } else {
            None
        };
        let registration = Registration {
            key,
            payload_sha256,
            parent,
            design: design.clone(),
        };
        if let Some(old) = self.registrations.get(&provider) {
            if old == &registration {
                return Ok(());
            }
            return Err(Error::invalid(
                "CASP provider registered with conflicting thumbnail metadata",
            ));
        }
        if let Some(design) = design {
            self.designs.entry(design).or_default().push(provider);
        }
        self.registrations.insert(provider, registration);
        Ok(())
    }
    pub fn choices(&self, catalog: &Catalog, selected: Provider) -> Result<ThumbnailChoices> {
        let selected_registration = self
            .registrations
            .get(&selected)
            .ok_or_else(|| Error::invalid("CASP has no registered thumbnail metadata"))?;
        let mut choices = ThumbnailChoices {
            exact: catalog.thumbnail_candidates(selected_registration.key.instance_id),
            ..ThumbnailChoices::default()
        };
        if let Some(parent) = selected_registration.parent {
            if parent.type_id != CASP {
                choices.notes.push(format!(
                    "Declared parent has unsupported resource type {:08X}",
                    parent.type_id
                ));
            } else if parent.instance_id == selected_registration.key.instance_id {
                choices.notes.push("Declared parent shares the selected instance; exact candidates already retain those thumbnails".into());
            } else {
                let sources = catalog
                    .providers(parent)
                    .iter()
                    .map(|p| ThumbnailCaspSource {
                        key: parent,
                        provider: *p,
                        payload_sha256: self
                            .registrations
                            .get(p)
                            .filter(|r| r.key == parent)
                            .map(|r| r.payload_sha256),
                    })
                    .collect::<Vec<_>>();
                if sources.is_empty() {
                    choices.notes.push("Declared parent CASP is missing in the selected source set; any parent image is design-only".into());
                }
                let candidates = catalog.thumbnail_candidates(parent.instance_id);
                if !candidates.is_empty() {
                    choices.design.push(DesignThumbnailCandidate {
                        casp_key: parent,
                        relationship: ThumbnailRelationship::DeclaredParentDesign,
                        casp_sources: sources,
                        candidates,
                    });
                }
            }
        }
        if let Some(design) = &selected_registration.design {
            let mut related = BTreeMap::<ResourceKey, Vec<ThumbnailCaspSource>>::new();
            if let Some(members) = self.designs.get(design) {
                for member in members {
                    if member.package_index != selected.package_index {
                        continue;
                    }
                    let registered = &self.registrations[member];
                    if registered.key.instance_id == selected_registration.key.instance_id {
                        continue;
                    }
                    related
                        .entry(registered.key)
                        .or_default()
                        .push(ThumbnailCaspSource {
                            key: registered.key,
                            provider: *member,
                            payload_sha256: Some(registered.payload_sha256),
                        });
                }
            }
            for (key, casp_sources) in related {
                if choices.design.iter().any(|d| d.casp_key == key) {
                    continue;
                }
                let candidates = catalog.thumbnail_candidates(key.instance_id);
                if !candidates.is_empty() {
                    choices.design.push(DesignThumbnailCandidate {
                        casp_key: key,
                        relationship: ThumbnailRelationship::SharedOutfitGeometryDesign,
                        casp_sources,
                        candidates,
                    });
                }
            }
        }
        Ok(choices)
    }
}

impl Catalog {
    /// O(number of matching full keys/providers) after discovery. The fallback
    /// exists only for manually assembled catalogs whose indexes are not ready.
    pub fn thumbnail_candidates(&self, instance: u64) -> Vec<ThumbnailCandidate> {
        let mut keys = if self.instance_indexes_ready {
            self.thumbnail_instances
                .get(&instance)
                .cloned()
                .unwrap_or_default()
        } else {
            self.by_key
                .keys()
                .filter(|k| k.type_id == THUMBNAIL && k.instance_id == instance)
                .copied()
                .collect()
        };
        keys.sort_unstable();
        keys.into_iter()
            .map(|key| ThumbnailCandidate {
                key,
                providers: self.providers(key).to_vec(),
                model_hint: model_hint(key.group_id),
            })
            .collect()
    }
    /// Read each provider once, sharing one validated reader per package. The
    /// supplied byte budget bounds each blob and total retained source bytes in
    /// this inspection. Budget failures are explicit; discovery is never capped.
    pub fn inspect_thumbnails(
        &self,
        candidates: &[ThumbnailCandidate],
        limits: ReadLimits,
        cancel: &AtomicBool,
    ) -> Result<ThumbnailInspection> {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let mut result = ThumbnailInspection::default();
        let mut pending = BTreeMap::<usize, Vec<(ResourceKey, Provider)>>::new();
        let mut seen = HashSet::new();
        for candidate in candidates {
            for provider in &candidate.providers {
                if seen.insert((candidate.key, *provider)) {
                    pending
                        .entry(provider.package_index)
                        .or_default()
                        .push((candidate.key, *provider));
                }
            }
        }
        let mut retained = 0u64;
        let mut used = Vec::new();
        for (package_index, requests) in pending {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let Some(package) = self.packages.get(package_index) else {
                for (key, provider) in requests {
                    result.failures.push(ThumbnailFailure {
                        key,
                        provider,
                        message: "Thumbnail package handle is unavailable".into(),
                    });
                }
                continue;
            };
            let mut reader = match package.reader_cancellable(cancel) {
                Ok(r) => r,
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error) => {
                    for (key, provider) in requests {
                        result.failures.push(ThumbnailFailure {
                            key,
                            provider,
                            message: error.to_string(),
                        });
                    }
                    continue;
                }
            };
            used.push(package_index);
            for (key, provider) in requests {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                let Some(entry) = package
                    .entries
                    .get(provider.entry_index)
                    .filter(|entry| entry.key == key && key.type_id == THUMBNAIL)
                else {
                    result.failures.push(ThumbnailFailure {
                        key,
                        provider,
                        message: "Thumbnail locator is stale or mismatched".into(),
                    });
                    continue;
                };
                if retained
                    .checked_add(entry.memory_size as u64)
                    .is_none_or(|n| n > limits.max_resource_bytes)
                {
                    result.failures.push(ThumbnailFailure {
                        key,
                        provider,
                        message: Error::Budget {
                            requested: retained.saturating_add(entry.memory_size as u64),
                            limit: limits.max_resource_bytes,
                        }
                        .to_string(),
                    });
                    continue;
                }
                match reader.read_resource_cancellable(entry, limits, cancel) {
                    Ok(bytes) => {
                        retained += bytes.len() as u64;
                        let payload_sha256 = Sha256::digest(&bytes).into();
                        let info = image_info(&bytes);
                        result.payloads.push(ThumbnailPayload {
                            key,
                            provider,
                            source_index_sha256: package.index_sha256,
                            payload_sha256,
                            bytes,
                            info,
                        });
                    }
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(error) => result.failures.push(ThumbnailFailure {
                        key,
                        provider,
                        message: error.to_string(),
                    }),
                }
            }
        }
        for package_index in used {
            if let Err(error) = self.packages[package_index].revalidate(cancel) {
                if matches!(error, Error::Cancelled) {
                    return Err(Error::Cancelled);
                }
                for payload in result
                    .payloads
                    .iter()
                    .filter(|p| p.provider.package_index == package_index)
                {
                    result.failures.push(ThumbnailFailure {
                        key: payload.key,
                        provider: payload.provider,
                        message: format!("Source changed after thumbnail observation: {error}"),
                    });
                }
            }
        }
        for (index, payload) in result.payloads.iter().enumerate() {
            let equivalent = result.equivalences.iter_mut().find(|e| {
                e.payload_sha256 == payload.payload_sha256
                    && result.payloads[e.payload_indices[0]].bytes == payload.bytes
            });
            if let Some(equivalent) = equivalent {
                equivalent.payload_indices.push(index);
            } else {
                result.equivalences.push(ThumbnailEquivalence {
                    payload_sha256: payload.payload_sha256,
                    payload_indices: vec![index],
                });
            }
        }
        result.complete = result.failures.is_empty();
        Ok(result)
    }
}

pub fn image_info(bytes: &[u8]) -> ThumbnailImageInfo {
    if bytes.starts_with(&[0xff, 0xd8]) {
        let dimensions = jpeg_dimensions(bytes).or_else(|_| {
            crate::thumbnail_decode::jpeg_color_and_alpha(bytes, &AtomicBool::new(false))
                .and_then(|(color, _)| jpeg_dimensions(&color))
        });
        match dimensions {
            Ok(size) => ThumbnailImageInfo {
                format: ThumbnailFormat::Jpeg,
                dimensions: Some(size),
                header_error: None,
            },
            Err(error) => ThumbnailImageInfo {
                format: ThumbnailFormat::Jpeg,
                dimensions: None,
                header_error: Some(error.to_string()),
            },
        }
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let dimensions = if bytes.len() >= 24
            && &bytes[12..16] == b"IHDR"
            && u32::from_be_bytes(bytes[8..12].try_into().unwrap()) == 13
        {
            let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            (width != 0 && height != 0).then_some([width, height])
        } else {
            None
        };
        ThumbnailImageInfo {
            format: ThumbnailFormat::Png,
            dimensions,
            header_error: dimensions.is_none().then(|| {
                "Invalid PNG dimension header; pixel decoder must verify the complete image".into()
            }),
        }
    } else {
        ThumbnailImageInfo {
            format: ThumbnailFormat::Unknown,
            dimensions: None,
            header_error: Some(
                "Unrecognized thumbnail encoding; original source bytes retained".into(),
            ),
        }
    }
}
fn jpeg_dimensions(bytes: &[u8]) -> Result<[u32; 2]> {
    let mut at = 2;
    while at < bytes.len() {
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
        if matches!(marker, 0xd9 | 0xda) {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        let length = u16::from_be_bytes(
            bytes
                .get(at..at + 2)
                .ok_or_else(|| Error::invalid("JPEG segment extent"))?
                .try_into()
                .unwrap(),
        ) as usize;
        if length < 2 || at.checked_add(length).is_none_or(|end| end > bytes.len()) {
            return Err(Error::invalid("JPEG segment length"));
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if length < 8 {
                return Err(Error::invalid("JPEG frame header"));
            }
            let height = u16::from_be_bytes(bytes[at + 3..at + 5].try_into().unwrap()) as u32;
            let width = u16::from_be_bytes(bytes[at + 5..at + 7].try_into().unwrap()) as u32;
            if width == 0 || height == 0 {
                return Err(Error::invalid("JPEG dimensions"));
            }
            return Ok([width, height]);
        }
        at += length;
    }
    Err(Error::unsupported("JPEG frame dimensions are unavailable"))
}
