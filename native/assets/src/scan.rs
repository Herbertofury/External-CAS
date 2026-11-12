// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Error, PackageIndex, ResourceEntry, ResourceKey, Result};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Default)]
pub struct ScanProgress {
    pub directories: u64,
    pub files: u64,
    pub packages: u64,
    pub resources: u64,
    pub skipped_links: u64,
    pub current: PathBuf,
}
#[derive(Clone, Debug)]
pub struct ScanIssue {
    pub path: PathBuf,
    pub error: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Provider {
    pub package_index: usize,
    pub entry_index: usize,
}
/// Full TGI candidates for an instance-only native outfit reference. A matching
/// instance does not establish a resource group or package load precedence.
#[derive(Clone, Debug)]
pub struct CaspCandidate {
    pub key: ResourceKey,
    pub providers: Vec<Provider>,
}
#[derive(Clone, Debug)]
pub struct TextureCandidate {
    pub key: ResourceKey,
    pub kind: &'static str,
    pub providers: Vec<Provider>,
}
#[derive(Default)]
pub struct Catalog {
    pub packages: Vec<PackageIndex>,
    pub by_key: HashMap<ResourceKey, Vec<Provider>>,
    pub issues: Vec<ScanIssue>,
    pub progress: ScanProgress,
    pub casp_instances: HashMap<u64, Vec<ResourceKey>>,
    pub thumbnail_instances: HashMap<u64, Vec<ResourceKey>>,
    /// False only for manually assembled catalogs. A complete empty index must
    /// stay distinguishable from an unbuilt one to avoid whole-library scans.
    pub instance_indexes_ready: bool,
}
impl Catalog {
    pub fn rebuild_instance_indexes(&mut self) {
        self.casp_instances.clear();
        self.thumbnail_instances.clear();
        for key in self.by_key.keys() {
            if key.type_id == crate::dbpf::CASP {
                self.casp_instances
                    .entry(key.instance_id)
                    .or_default()
                    .push(*key);
            } else if key.type_id == crate::dbpf::THUMBNAIL {
                self.thumbnail_instances
                    .entry(key.instance_id)
                    .or_default()
                    .push(*key);
            }
        }
        self.instance_indexes_ready = true;
    }
    /// Report the exact key and specific same-group/instance texture-family
    /// alternatives documented by SimRipper. No priority or winner is inferred.
    pub fn texture_candidates(
        &self,
        declared: ResourceKey,
        include_dds: bool,
    ) -> Vec<TextureCandidate> {
        let mut keys = vec![(declared, "exact")];
        if [0x2bc0_4edf, 0x3453_cf95, 0x00b2_d882, 0xb6c8_b6a0].contains(&declared.type_id) {
            for (type_id, kind) in [
                (0x2bc0_4edf, "reference-lrle-alternative"),
                (0x3453_cf95, "reference-rle2-alternative"),
                (0x00b2_d882, "reference-dst-alternative"),
                (0xb6c8_b6a0, "reference-dds-alternative"),
            ] {
                if type_id != declared.type_id && (include_dds || type_id != 0xb6c8_b6a0) {
                    keys.push((
                        ResourceKey::new(type_id, declared.group_id, declared.instance_id),
                        kind,
                    ));
                }
            }
        }
        keys.into_iter()
            .map(|(key, kind)| TextureCandidate {
                key,
                kind,
                providers: self.providers(key).to_vec(),
            })
            .collect()
    }
    pub fn casp_candidates(&self, instance_id: u64) -> Vec<CaspCandidate> {
        let keys = if self.instance_indexes_ready {
            self.casp_instances
                .get(&instance_id)
                .cloned()
                .unwrap_or_default()
        } else {
            self.by_key
                .keys()
                .filter(|key| key.type_id == crate::dbpf::CASP && key.instance_id == instance_id)
                .copied()
                .collect()
        };
        let mut candidates = keys
            .into_iter()
            .map(|key| CaspCandidate {
                key,
                providers: self.providers(key).to_vec(),
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|c| c.key);
        candidates
    }
    pub fn unique_casp_instance(&self, instance_id: u64) -> Result<(ResourceKey, Provider)> {
        let candidates = self.casp_candidates(instance_id);
        match candidates.as_slice() {
            [candidate] if candidate.providers.len() == 1 => {
                Ok((candidate.key, candidate.providers[0]))
            }
            [] => Err(Error::invalid(format!(
                "missing CASP instance {instance_id:016X}"
            ))),
            [candidate] => Err(Error::unsupported(format!(
                "CASP instance {instance_id:016X} has {} providers for {}; select a source explicitly",
                candidate.providers.len(),
                candidate.key
            ))),
            _ => Err(Error::unsupported(format!(
                "CASP instance {instance_id:016X} matches {} full TGI keys; select a group and source explicitly",
                candidates.len()
            ))),
        }
    }
    /// Roots may be files or folders. Files/indexes are visited incrementally.
    /// Every duplicate, tombstone and raw storage marker remains represented.
    /// No game load order is inferred from directory order.
    pub fn scan_roots(
        roots: &[PathBuf],
        cancel: &AtomicBool,
        mut report: impl FnMut(&ScanProgress),
    ) -> Result<Self> {
        let mut out = Self {
            instance_indexes_ready: true,
            ..Self::default()
        };
        let mut seen = HashSet::new();
        let mut pending = roots.to_vec();
        while let Some(path) = pending.pop() {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let actual = match path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    out.issues.push(ScanIssue {
                        path,
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            if !seen.insert(actual.clone()) {
                continue;
            }
            let metadata = match fs::metadata(&actual) {
                Ok(m) => m,
                Err(e) => {
                    out.issues.push(ScanIssue {
                        path,
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            out.progress.current = path.clone();
            if metadata.is_file() {
                out.scan_file(&actual, cancel)?;
                report(&out.progress);
                continue;
            }
            if !metadata.is_dir() {
                continue;
            }
            out.progress.directories += 1;
            report(&out.progress);
            let children = match fs::read_dir(&actual) {
                Ok(r) => r,
                Err(e) => {
                    out.issues.push(ScanIssue {
                        path,
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            for item in children {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                let entry = match item {
                    Ok(e) => e,
                    Err(e) => {
                        out.issues.push(ScanIssue {
                            path: actual.clone(),
                            error: e.to_string(),
                        });
                        continue;
                    }
                };
                let ty = match entry.file_type() {
                    Ok(t) => t,
                    Err(e) => {
                        out.issues.push(ScanIssue {
                            path: entry.path(),
                            error: e.to_string(),
                        });
                        continue;
                    }
                };
                // Do not silently leave an explicitly selected scope through child links.
                if ty.is_symlink() {
                    out.progress.skipped_links += 1;
                    continue;
                }
                if ty.is_dir() {
                    pending.push(entry.path());
                } else if ty.is_file() {
                    out.progress.current = entry.path();
                    out.scan_file(&entry.path(), cancel)?;
                    if out.progress.files % 64 == 0 {
                        report(&out.progress);
                    }
                }
            }
            report(&out.progress);
        }
        Ok(out)
    }
    fn scan_file(&mut self, path: &Path, cancel: &AtomicBool) -> Result<()> {
        self.progress.files += 1;
        if !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("package"))
        {
            return Ok(());
        }
        match PackageIndex::open_cancellable(path, cancel) {
            Ok(package) => {
                let package_index = self.packages.len();
                for (entry_index, entry) in package.entries.iter().enumerate() {
                    let providers = self.by_key.entry(entry.key).or_default();
                    let first = providers.is_empty();
                    providers.push(Provider {
                        package_index,
                        entry_index,
                    });
                    if first {
                        if entry.key.type_id == crate::dbpf::CASP {
                            self.casp_instances
                                .entry(entry.key.instance_id)
                                .or_default()
                                .push(entry.key);
                        } else if entry.key.type_id == crate::dbpf::THUMBNAIL {
                            self.thumbnail_instances
                                .entry(entry.key.instance_id)
                                .or_default()
                                .push(entry.key);
                        }
                    }
                }
                self.progress.resources += package.entries.len() as u64;
                self.progress.packages += 1;
                self.packages.push(package);
            }
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(e) => self.issues.push(ScanIssue {
                path: path.to_path_buf(),
                error: e.to_string(),
            }),
        }
        Ok(())
    }
    pub fn providers(&self, key: ResourceKey) -> &[Provider] {
        self.by_key.get(&key).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn provider(&self, p: Provider) -> Option<(&PackageIndex, &ResourceEntry)> {
        let package = self.packages.get(p.package_index)?;
        Some((package, package.entries.get(p.entry_index)?))
    }
    pub fn unique(&self, key: ResourceKey) -> Result<(&PackageIndex, &ResourceEntry)> {
        match self.providers(key) {
            [p] => self
                .provider(*p)
                .ok_or_else(|| Error::invalid("catalog provider")),
            [] => Err(Error::invalid(format!("missing resource {key}"))),
            p => Err(Error::unsupported(format!(
                "ambiguous resource {key}: {} providers; select explicitly",
                p.len()
            ))),
        }
    }
}
