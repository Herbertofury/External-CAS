// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit offline Mods scope. Only configured package files are observed;
//! scripts, DLLs, game processes and profile writes are never involved.
use crate::*;
use external_cas_assets::{Catalog, scan::Provider};
use external_cas_scene::{ConfigObservation, PackagePriority, PriorityRule};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModsActivation {
    RequireObservedEnabled,
    ExplicitOfflinePreview,
}
#[derive(Clone)]
pub struct RoomModsRequest {
    pub root: PathBuf,
    pub options_path: Option<PathBuf>,
    pub activation: ModsActivation,
    /// An explicit subset is useful for a selected replacement, but it is
    /// reported as incomplete coverage even when every selected source exists.
    pub candidate_paths: Option<Vec<PathBuf>>,
    /// Optional already-indexed sources. Every accepted payload is revalidated
    /// against its actual source by PackageReader before it can be published.
    pub catalog: Option<Arc<Catalog>>,
}
impl std::fmt::Debug for RoomModsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomModsRequest")
            .field("root", &self.root)
            .field("options_path", &self.options_path)
            .field("activation", &self.activation)
            .field("candidate_paths", &self.candidate_paths)
            .field(
                "cached_packages",
                &self.catalog.as_ref().map(|c| c.packages.len()),
            )
            .finish()
    }
}
impl RoomModsRequest {
    pub fn configured(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            options_path: root.parent().map(|p| p.join("Options.ini")),
            root,
            activation: ModsActivation::RequireObservedEnabled,
            candidate_paths: None,
            catalog: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RoomModsDiscovery {
    pub request: RoomModsRequest,
    pub packages: Vec<PackagePriority>,
    pub observations: Vec<ConfigObservation>,
    pub issues: Vec<RoomIssue>,
    pub complete: bool,
    pub enabled_observed: Option<bool>,
}
fn digest(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("Mods discovery cancelled".into())
    } else {
        Ok(())
    }
}
fn plain(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        if p.as_os_str().is_empty() {
            continue;
        }
        plain_single(p)?;
    }
    Ok(())
}
fn plain_single(p: &Path) -> Result<()> {
    let m = fs::symlink_metadata(p).map_err(|e| e.to_string())?;
    if m.file_type().is_symlink() {
        return Err("linked Mods source is unsupported".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("reparse Mods source is unsupported".into());
        }
    }
    Ok(())
}
fn config(path: &Path) -> Result<(String, ConfigObservation)> {
    plain(path)?;
    let before = fs::metadata(path).map_err(|e| e.to_string())?;
    if !before.is_file() || before.len() > 1024 * 1024 {
        return Err("Mods config byte extent".into());
    }
    let b = fs::read(path).map_err(|e| e.to_string())?;
    let after = fs::metadata(path).map_err(|e| e.to_string())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || b.len() as u64 != after.len()
    {
        return Err("Mods config changed during read".into());
    }
    let text = String::from_utf8(b.clone()).map_err(|_| "Mods config is not UTF-8")?;
    Ok((
        text.trim_start_matches('\u{feff}').into(),
        ConfigObservation {
            path: fs::canonicalize(path).map_err(|e| e.to_string())?,
            sha256: digest(&b),
            bytes: b.len(),
        },
    ))
}
fn glob(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while j < n.len() {
        if i < p.len() && (p[i] == '?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            retry = j;
        } else if let Some(s) = star {
            retry += 1;
            j = retry;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}
fn components(pattern: &str) -> Vec<&str> {
    pattern.split('/').collect()
}
fn matches(pattern: &str, path: &[String]) -> bool {
    let p = components(pattern);
    p.len() == path.len() && p.iter().zip(path).all(|(p, n)| glob(p, n))
}
fn prefixes(pattern: &str, path: &[String]) -> bool {
    let p = components(pattern);
    p.len() > path.len() && p.iter().zip(path).all(|(p, n)| glob(p, n))
}
fn rules(text: &str, observation: &ConfigObservation) -> Result<Vec<PriorityRule>> {
    let mut priority = None;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap();
        let args = parts.next().unwrap_or("").trim();
        if command.eq_ignore_ascii_case("Priority") {
            priority = Some(
                args.parse::<i32>()
                    .map_err(|_| format!("invalid Mods priority on line {}", i + 1))?,
            );
        } else if command.eq_ignore_ascii_case("PackedFile") {
            let pattern = args.trim_matches('"').replace('\\', "/");
            let p = components(&pattern);
            if p.is_empty()
                || p.len() > 64
                || p.iter()
                    .any(|s| s.is_empty() || *s == "." || *s == ".." || s.contains(':'))
                || !pattern.to_ascii_lowercase().ends_with(".package")
            {
                return Err(format!(
                    "unsupported Mods PackedFile pattern on line {}",
                    i + 1
                ));
            }
            out.push(PriorityRule {
                config_path: observation.path.clone(),
                config_sha256: observation.sha256.clone(),
                line: i + 1,
                pattern,
                priority: priority.ok_or("Mods PackedFile lacks explicit priority")?,
            });
        } else {
            return Err(format!(
                "unsupported Mods config directive {command} on line {}",
                i + 1
            ));
        }
    }
    if out.is_empty() {
        return Err("Mods config has no qualified package rules".into());
    }
    Ok(out)
}
fn enabled(text: &str) -> Result<Option<bool>> {
    let mut value = None;
    let mut section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.eq_ignore_ascii_case("[options]");
            continue;
        }
        if section {
            if let Some((key, v)) = line.split_once('=') {
                if key.trim().eq_ignore_ascii_case("modsdisabled") {
                    if value.is_some() {
                        return Err("duplicate Options.ini modsdisabled".into());
                    }
                    value = Some(match v.trim() {
                        "0" => true,
                        "1" => false,
                        _ => return Err("invalid Options.ini modsdisabled".into()),
                    });
                }
            }
        }
    }
    Ok(value)
}
pub fn discover_mods(request: &RoomModsRequest, cancel: &AtomicBool) -> Result<RoomModsDiscovery> {
    check(cancel)?;
    if !request.root.is_absolute() {
        return Err("explicit absolute Mods root required".into());
    }
    plain(&request.root)?;
    let root = fs::canonicalize(&request.root).map_err(|e| e.to_string())?;
    let (text, observation) = config(&root.join("Resource.cfg"))?;
    let rs = rules(&text, &observation)?;
    let mut observations = vec![observation];
    let mut issues = Vec::new();
    let mut complete = request.candidate_paths.is_none();
    let enabled_observed = if let Some(path) = &request.options_path {
        let (t, o) = config(path)?;
        observations.push(o);
        enabled(&t)?
    } else {
        None
    };
    if request.activation == ModsActivation::RequireObservedEnabled
        && enabled_observed != Some(true)
    {
        return Err(
            "Mods scope requires Options.ini modsdisabled=0 or explicit offline-preview activation"
                .into(),
        );
    }
    if request.activation == ModsActivation::ExplicitOfflinePreview {
        issues.push(RoomIssue{stage:"mods-activation".into(),detail:"Caller explicitly selected offline package preview; this does not establish active game mod state".into(),required_for_parity:true});
        complete = false;
    }
    let mut paths = Vec::new();
    if let Some(selected) = &request.candidate_paths {
        issues.push(RoomIssue{stage:"mods-coverage".into(),detail:"Explicit replacement package subset selected; other configured Mods candidates have not been exhaustively indexed".into(),required_for_parity:true});
        for p in selected {
            check(cancel)?;
            plain(p)?;
            let p = fs::canonicalize(p).map_err(|e| e.to_string())?;
            if !p.starts_with(&root) {
                return Err("selected Mods package escapes explicit root".into());
            }
            paths.push(p);
        }
    } else {
        let mut pending = vec![(root.clone(), Vec::<String>::new())];
        while let Some((dir, prefix)) = pending.pop() {
            check(cancel)?;
            for e in fs::read_dir(dir).map_err(|e| e.to_string())? {
                check(cancel)?;
                let e = e.map_err(|e| e.to_string())?;
                let mut parts = prefix.clone();
                parts.push(
                    e.file_name()
                        .to_str()
                        .ok_or("non-Unicode Mods filename")?
                        .into(),
                );
                let ty = e.file_type().map_err(|e| e.to_string())?;
                if ty.is_symlink()
                    && rs
                        .iter()
                        .any(|r| prefixes(&r.pattern, &parts) || matches(&r.pattern, &parts))
                {
                    return Err("configured linked Mods source is unsupported".into());
                }
                if ty.is_dir() && rs.iter().any(|r| prefixes(&r.pattern, &parts)) {
                    plain_single(&e.path())?;
                    pending.push((e.path(), parts));
                } else if ty.is_file() && rs.iter().any(|r| matches(&r.pattern, &parts)) {
                    plain_single(&e.path())?;
                    paths.push(fs::canonicalize(e.path()).map_err(|e| e.to_string())?);
                }
            }
        }
    }
    let mut packages = Vec::new();
    for path in paths.into_iter().collect::<BTreeSet<_>>() {
        let parts = path
            .strip_prefix(&root)
            .map_err(|_| "Mods package escape")?
            .components()
            .map(|c| {
                c.as_os_str()
                    .to_str()
                    .map(String::from)
                    .ok_or("non-Unicode Mods component")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let matching: Vec<_> = rs
            .iter()
            .filter(|r| matches(&r.pattern, &parts))
            .cloned()
            .collect();
        if matching.is_empty() {
            return Err(format!(
                "selected package is not configured by Mods Resource.cfg: {}",
                path.display()
            ));
        }
        packages.push(PackagePriority {
            path,
            priority: matching.iter().map(|r| r.priority).max().unwrap(),
            rules: matching,
        });
    }
    Ok(RoomModsDiscovery {
        request: request.clone(),
        packages,
        observations,
        issues,
        complete,
        enabled_observed,
    })
}
impl RoomModsDiscovery {
    pub fn priority(&self, path: &Path) -> Option<i32> {
        self.packages
            .binary_search_by(|p| p.path.as_path().cmp(path))
            .ok()
            .map(|i| self.packages[i].priority)
    }
    pub fn revalidate(&self, cancel: &AtomicBool) -> Result<()> {
        let current = discover_mods(&self.request, cancel)?;
        if self
            .observations
            .iter()
            .map(|o| (&o.path, &o.sha256))
            .ne(current.observations.iter().map(|o| (&o.path, &o.sha256)))
            || self
                .packages
                .iter()
                .map(|p| (&p.path, p.priority))
                .ne(current.packages.iter().map(|p| (&p.path, p.priority)))
        {
            return Err(
                "configured Mods sources or settings changed during room/tuning load".into(),
            );
        }
        Ok(())
    }
    pub(crate) fn revalidate_catalog(&self, catalog: &Catalog, cancel: &AtomicBool) -> Result<()> {
        for package in &catalog.packages {
            check(cancel)?;
            if self.priority(&package.path).is_some() {
                package
                    .revalidate(cancel)
                    .map_err(|e| format!("configured Mods index changed during load: {e}"))?;
            }
        }
        Ok(())
    }
}
pub(crate) fn catalog_with_mods(
    mut paths: Vec<PathBuf>,
    mods: Option<&RoomModsDiscovery>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&external_cas_assets::scan::ScanProgress),
) -> Result<Catalog> {
    let mut cached = BTreeMap::new();
    if let Some(mods) = mods {
        let known = mods
            .request
            .catalog
            .as_ref()
            .map(|c| {
                c.packages
                    .iter()
                    .map(|p| (&p.path, p))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        for p in &mods.packages {
            if let Some(index) = known.get(&p.path) {
                cached.insert(p.path.clone(), (*index).clone());
            } else {
                paths.push(p.path.clone());
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths.retain(|p| !cached.contains_key(p));
    let mut catalog =
        Catalog::scan_roots(&paths, cancel, |s| progress(s)).map_err(|e| e.to_string())?;
    for (_, index) in cached {
        check(cancel)?;
        index
            .revalidate(cancel)
            .map_err(|e| format!("cached Mods index changed: {e}"))?;
        let package_index = catalog.packages.len();
        for (entry_index, entry) in index.entries.iter().enumerate() {
            catalog.by_key.entry(entry.key).or_default().push(Provider {
                package_index,
                entry_index,
            });
        }
        catalog.progress.resources += index.entries.len() as u64;
        catalog.progress.packages += 1;
        catalog.packages.push(index);
    }
    catalog.rebuild_instance_indexes();
    Ok(catalog)
}
pub fn index_configured_mods(
    discovery: &RoomModsDiscovery,
    cancel: &AtomicBool,
    progress: impl FnMut(&external_cas_assets::scan::ScanProgress),
) -> Result<Catalog> {
    catalog_with_mods(Vec::new(), Some(discovery), cancel, progress)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_globs_preserve_depth_unicode_and_priority_evidence() {
        assert!(matches(
            "*/*.package",
            &["Créateur".into(), "Room.package".into()]
        ));
        assert!(!matches(
            "*.package",
            &["Folder".into(), "Room.package".into()]
        ));
        assert!(!prefixes("*.package", &["Folder".into()]));
        let o = ConfigObservation {
            path: "Resource.cfg".into(),
            sha256: "0".repeat(64),
            bytes: 0,
        };
        let r = rules(
            "Priority 500\nPackedFile */*.package\nPriority 600\nPackedFile Overrides/*.package",
            &o,
        )
        .unwrap();
        let path = vec!["Overrides".into(), "Room.package".into()];
        assert_eq!(
            r.iter()
                .filter(|r| matches(&r.pattern, &path))
                .map(|r| r.priority)
                .max(),
            Some(600)
        );
        assert!(rules("Priority 500\nSelect X\nPackedFile *.package", &o).is_err());
        assert!(rules("Priority 500\nPackedFile ../*.package", &o).is_err());
    }
    #[test]
    fn disabled_missing_duplicate_settings_are_explicit() {
        assert_eq!(enabled("[options]\nmodsdisabled = 0").unwrap(), Some(true));
        assert_eq!(enabled("[options]\nmodsdisabled = 1").unwrap(), Some(false));
        assert_eq!(enabled("[options]\nother=1").unwrap(), None);
        assert!(enabled("[options]\nmodsdisabled=0\nmodsdisabled=1").is_err());
    }
    #[test]
    fn configured_discovery_pins_depth_settings_subset_and_revalidation() {
        let root = std::env::temp_dir().join(format!(
            "external-cas-room-mods-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let mods = root.join("Mods");
        fs::create_dir(&mods).unwrap();
        let nested = mods.join("Créateur");
        fs::create_dir(&nested).unwrap();
        let config_path = mods.join("Resource.cfg");
        let options = root.join("Options.ini");
        let a = mods.join("A.package");
        let b = nested.join("B.package");
        let excluded = nested.join("ignored.ts4script");
        for p in [&a, &b, &excluded] {
            fs::write(p, b"fixture only").unwrap();
        }
        fs::write(
            &config_path,
            "Priority 500\nPackedFile *.package\nPackedFile */*.package",
        )
        .unwrap();
        fs::write(&options, "[options]\nmodsdisabled=0").unwrap();
        let request = RoomModsRequest::configured(&mods);
        let d = discover_mods(&request, &AtomicBool::new(false)).unwrap();
        assert_eq!(d.packages.len(), 2);
        assert!(d.complete);
        assert_eq!(d.observations.len(), 2);
        d.revalidate(&AtomicBool::new(false)).unwrap();
        let mut subset = request.clone();
        subset.candidate_paths = Some(vec![a.clone()]);
        let selected = discover_mods(&subset, &AtomicBool::new(false)).unwrap();
        assert_eq!(selected.packages.len(), 1);
        assert!(!selected.complete);
        subset.candidate_paths = Some(vec![options.clone()]);
        assert!(discover_mods(&subset, &AtomicBool::new(false)).is_err());
        fs::write(
            &config_path,
            "Priority 600\nPackedFile *.package\nPackedFile */*.package",
        )
        .unwrap();
        assert!(d.revalidate(&AtomicBool::new(false)).is_err());
        for p in [&a, &b, &excluded, &config_path, &options] {
            fs::remove_file(p).unwrap();
        }
        fs::remove_dir(nested).unwrap();
        fs::remove_dir(mods).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
