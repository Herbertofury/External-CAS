//! Explicit numeric priorities observed in installed-game resource configs.
//! Equal-priority, unconfigured and custom-content ties remain unresolved.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};
const MAX_CONFIG: usize = 1024 * 1024;
#[derive(Clone, Debug)]
pub enum ScopeError {
    Io(String),
    Invalid(String),
    Changed(String),
}
impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ScopeError {}
#[derive(Clone, Debug)]
pub struct ConfigObservation {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: usize,
}
#[derive(Clone, Debug)]
pub struct PriorityRule {
    pub config_path: PathBuf,
    pub config_sha256: String,
    pub line: usize,
    pub pattern: String,
    pub priority: i32,
}
#[derive(Clone, Debug)]
pub struct PackagePriority {
    pub path: PathBuf,
    pub priority: i32,
    pub rules: Vec<PriorityRule>,
}
#[derive(Clone, Debug)]
pub struct ScopeIssue {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub struct GameInstallScope {
    pub root: PathBuf,
    pub observed_game_version: Option<String>,
    pub version_observation: Option<ConfigObservation>,
    pub configs: Vec<ConfigObservation>,
    pub packages: Vec<PackagePriority>,
    pub issues: Vec<ScopeIssue>,
}
#[derive(Clone, Debug)]
pub struct ScopeCoverage {
    pub configured_packages: usize,
    pub indexed_known_packages: usize,
    pub unindexed_paths: Vec<PathBuf>,
    pub unscoped_catalog_paths: Vec<PathBuf>,
    pub active_pack_entitlements_verified: bool,
    pub unqualified_config_directives: usize,
}
impl ScopeCoverage {
    pub fn configured_scope_complete(&self) -> bool {
        self.unindexed_paths.is_empty()
            && self.unscoped_catalog_paths.is_empty()
            && self.unqualified_config_directives == 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectiveState {
    Missing,
    UniqueProvider,
    UniqueHighestPriority,
    TiedHighestPriority,
    UnconfiguredProvider,
    DeletedHighestPriority,
    InvalidProvider,
}
#[derive(Clone, Debug)]
pub struct RankedProvider {
    pub provider: Provider,
    pub priority: Option<i32>,
    pub package_path: Option<PathBuf>,
    pub evidence: Vec<PriorityRule>,
    pub deleted: bool,
}
#[derive(Clone, Debug)]
pub struct EffectiveResolution {
    pub key: ResourceKey,
    pub state: EffectiveState,
    pub selected: Option<Provider>,
    pub candidates: Vec<RankedProvider>,
    pub configured_scope_complete: bool,
}
pub struct ScopePolicy<'a> {
    scope: &'a GameInstallScope,
    catalog: &'a Catalog,
    priorities: Vec<Option<&'a PackagePriority>>,
    pub coverage: ScopeCoverage,
}
#[derive(Clone, Debug)]
pub struct PolicyChoices {
    pub choices: BTreeMap<ResourceKey, Provider>,
    pub decisions: Vec<EffectiveResolution>,
    pub coverage: ScopeCoverage,
    pub observed_game_version: Option<String>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn plain(path: &Path) -> Result<(), ScopeError> {
    for p in path.ancestors() {
        if p.as_os_str().is_empty() {
            continue;
        }
        let md = fs::symlink_metadata(p).map_err(|e| ScopeError::Io(e.to_string()))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if md.file_attributes() & 0x400 != 0 {
                return Err(ScopeError::Invalid(
                    "Reparse config/package paths are unsupported".into(),
                ));
            }
        }
        if md.file_type().is_symlink() {
            return Err(ScopeError::Invalid(
                "Linked config/package paths are unsupported".into(),
            ));
        }
    }
    Ok(())
}
fn read_config(path: &Path) -> Result<(String, ConfigObservation), ScopeError> {
    plain(path)?;
    let before = fs::metadata(path).map_err(|e| ScopeError::Io(e.to_string()))?;
    if !before.is_file() || before.len() > MAX_CONFIG as u64 {
        return Err(ScopeError::Invalid("Config extent exceeds bound".into()));
    }
    let bytes = fs::read(path).map_err(|e| ScopeError::Io(e.to_string()))?;
    let after = fs::metadata(path).map_err(|e| ScopeError::Io(e.to_string()))?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() as u64 != after.len()
    {
        return Err(ScopeError::Changed(path.display().to_string()));
    }
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| ScopeError::Invalid("Resource config is not UTF-8".into()))?;
    Ok((
        text.trim_start_matches('\u{feff}').to_string(),
        ConfigObservation {
            path: fs::canonicalize(path).map_err(|e| ScopeError::Io(e.to_string()))?,
            sha256: digest(&bytes),
            bytes: bytes.len(),
        },
    ))
}
fn is_pack_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 4
        && name.is_ascii()
        && ["EP", "GP", "SP", "FP"]
            .iter()
            .any(|p| name[..2].eq_ignore_ascii_case(p))
        && b[2..].iter().all(u8::is_ascii_digit)
}
fn glob(pattern: &str, name: &str) -> bool {
    if !pattern.is_ascii() || !name.is_ascii() {
        return false;
    }
    let p = pattern.to_ascii_lowercase().into_bytes();
    let n = name.to_ascii_lowercase().into_bytes();
    let (mut i, mut j, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    while j < n.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
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
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}
struct CompiledRules {
    rules: Vec<PriorityRule>,
    blocked_patterns: Vec<String>,
}
fn parse_rules(
    text: &str,
    observation: &ConfigObservation,
    issues: &mut Vec<ScopeIssue>,
) -> CompiledRules {
    let mut rules = Vec::new();
    let mut priority = None;
    let mut conditional = 0usize;
    let mut valid = true;
    let mut blocked_patterns = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let command = parts.next().unwrap();
        let args = parts.collect::<Vec<_>>();
        if command.eq_ignore_ascii_case("Select") {
            conditional += 1;
            issues.push(ScopeIssue {
                path: observation.path.clone(),
                line: Some(index + 1),
                detail: format!(
                    "Conditional config retained but not evaluated: {}",
                    args.join(" ")
                ),
            });
            continue;
        }
        if command.eq_ignore_ascii_case("End") {
            if conditional == 0 {
                valid = false;
                issues.push(ScopeIssue {
                    path: observation.path.clone(),
                    line: Some(index + 1),
                    detail: "Unmatched End directive".into(),
                });
            } else {
                conditional -= 1;
            }
            continue;
        }
        if conditional > 0 {
            if command.eq_ignore_ascii_case("PackedFile") && args.len() == 1 {
                blocked_patterns.push(args[0].to_string());
            }
            continue;
        }
        if command.eq_ignore_ascii_case("Priority") && args.len() == 1 {
            match args[0].parse::<i32>() {
                Ok(p) => priority = Some(p),
                Err(_) => {
                    valid = false;
                    priority = None;
                    issues.push(ScopeIssue {
                        path: observation.path.clone(),
                        line: Some(index + 1),
                        detail: "Invalid numeric Priority".into(),
                    });
                }
            }
            continue;
        }
        if command.eq_ignore_ascii_case("PackedFile") && args.len() == 1 {
            let pattern = args[0];
            if pattern.chars().any(|c| matches!(c, '\\' | '/' | ':'))
                || pattern.contains("..")
                || !pattern.is_ascii()
            {
                valid = false;
                issues.push(ScopeIssue { path:observation.path.clone(),line:Some(index+1),detail:"Only local ASCII filename PackedFile rules are qualified in installed-game scope".into() });
                continue;
            }
            if let Some(priority) = priority {
                rules.push(PriorityRule {
                    config_path: observation.path.clone(),
                    config_sha256: observation.sha256.clone(),
                    line: index + 1,
                    pattern: pattern.into(),
                    priority,
                });
            } else {
                valid = false;
                issues.push(ScopeIssue {
                    path: observation.path.clone(),
                    line: Some(index + 1),
                    detail: "PackedFile has no explicit numeric Priority".into(),
                });
            }
            continue;
        }
        valid = false;
        issues.push(ScopeIssue {
            path: observation.path.clone(),
            line: Some(index + 1),
            detail: format!("Unqualified directive: {command}"),
        });
    }
    if conditional != 0 {
        valid = false;
        issues.push(ScopeIssue {
            path: observation.path.clone(),
            line: None,
            detail: "Unterminated conditional config block".into(),
        });
    }
    CompiledRules {
        rules: if valid { rules } else { Vec::new() },
        blocked_patterns,
    }
}
impl GameInstallScope {
    /// Observe config and list packages; no package payload or game is opened.
    pub fn discover(root: &Path) -> Result<Self, ScopeError> {
        if !root.is_absolute() {
            return Err(ScopeError::Invalid(
                "Game install root must be absolute".into(),
            ));
        }
        plain(root)?;
        let root = fs::canonicalize(root).map_err(|e| ScopeError::Io(e.to_string()))?;
        let mut result = Self {
            root: root.clone(),
            observed_game_version: None,
            version_observation: None,
            configs: Vec::new(),
            packages: Vec::new(),
            issues: Vec::new(),
        };
        let version = root.join("Game").join("Bin").join("Default.ini");
        if version.is_file() {
            let (text, observation) = read_config(&version)?;
            let mut in_version = false;
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with('[') {
                    in_version = line.eq_ignore_ascii_case("[Version]");
                    continue;
                }
                if in_version {
                    if let Some((k, v)) = line.split_once('=') {
                        if k.trim().eq_ignore_ascii_case("gameversion") {
                            result.observed_game_version = Some(v.trim().to_string());
                        }
                    }
                }
            }
            result.version_observation = Some(observation);
        }
        let mut configs = Vec::new();
        for dir in [
            root.join("Data").join("Client"),
            root.join("Data").join("Simulation"),
        ] {
            let p = dir.join("Resource.cfg");
            if p.is_file() {
                configs.push(p);
            } else {
                return Err(ScopeError::Invalid(format!(
                    "Missing base resource config {}",
                    p.display()
                )));
            }
        }
        for base in [root.clone(), root.join("Delta")] {
            if !base.is_dir() {
                continue;
            }
            for entry in fs::read_dir(&base).map_err(|e| ScopeError::Io(e.to_string()))? {
                let entry = entry.map_err(|e| ScopeError::Io(e.to_string()))?;
                let name = entry.file_name().to_string_lossy().to_string();
                if !is_pack_name(&name)
                    || !entry
                        .file_type()
                        .map_err(|e| ScopeError::Io(e.to_string()))?
                        .is_dir()
                {
                    continue;
                }
                for file in ["ResourceClient.cfg", "ResourceSimulation.cfg"] {
                    let p = entry.path().join(file);
                    if p.is_file() {
                        configs.push(p);
                    }
                }
            }
        }
        configs.sort();
        let mut packages: BTreeMap<PathBuf, PackagePriority> = BTreeMap::new();
        for config in configs {
            let (text, observation) = read_config(&config)?;
            let compiled = parse_rules(&text, &observation, &mut result.issues);
            let rules = compiled.rules;
            let directory = config.parent().unwrap();
            for entry in fs::read_dir(directory).map_err(|e| ScopeError::Io(e.to_string()))? {
                let entry = entry.map_err(|e| ScopeError::Io(e.to_string()))?;
                let ty = entry
                    .file_type()
                    .map_err(|e| ScopeError::Io(e.to_string()))?;
                if !ty.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if compiled.blocked_patterns.iter().any(|p| glob(p, &name)) {
                    continue;
                }
                let matching = rules
                    .iter()
                    .filter(|r| glob(&r.pattern, &name))
                    .cloned()
                    .collect::<Vec<_>>();
                if matching.is_empty() {
                    continue;
                }
                plain(&entry.path())?;
                let path =
                    fs::canonicalize(entry.path()).map_err(|e| ScopeError::Io(e.to_string()))?;
                if !path.starts_with(&root) {
                    return Err(ScopeError::Invalid(
                        "Configured package escapes install scope".into(),
                    ));
                }
                let priority = matching.iter().map(|r| r.priority).max().unwrap();
                packages
                    .entry(path.clone())
                    .and_modify(|p| {
                        p.priority = p.priority.max(priority);
                        p.rules.extend(matching.clone());
                    })
                    .or_insert(PackagePriority {
                        path,
                        priority,
                        rules: matching,
                    });
            }
            result.configs.push(observation);
        }
        result.packages = packages.into_values().collect();
        Ok(result)
    }
    pub fn package_paths(&self) -> Vec<PathBuf> {
        self.packages.iter().map(|p| p.path.clone()).collect()
    }
    pub fn revalidate(&self) -> Result<(), ScopeError> {
        let current = Self::discover(&self.root)?;
        let same_configs = self.configs.len() == current.configs.len()
            && self
                .configs
                .iter()
                .zip(&current.configs)
                .all(|(a, b)| a.path == b.path && a.sha256 == b.sha256);
        let same_packages = self.packages.len() == current.packages.len()
            && self
                .packages
                .iter()
                .zip(&current.packages)
                .all(|(a, b)| a.path == b.path && a.priority == b.priority);
        let same_version = self.version_observation.as_ref().map(|v| &v.sha256)
            == current.version_observation.as_ref().map(|v| &v.sha256);
        if !same_configs || !same_packages || !same_version {
            return Err(ScopeError::Changed(
                "Install configuration, game version or configured package inventory changed"
                    .into(),
            ));
        }
        Ok(())
    }
    pub fn prepare<'a>(&'a self, catalog: &'a Catalog) -> Result<ScopePolicy<'a>, ScopeError> {
        self.revalidate()?;
        let by_path = self
            .packages
            .iter()
            .map(|p| (&p.path, p))
            .collect::<BTreeMap<_, _>>();
        let mut priorities = Vec::new();
        let mut indexed = BTreeMap::new();
        let mut unscoped = Vec::new();
        for package in &catalog.packages {
            let path =
                fs::canonicalize(&package.path).map_err(|e| ScopeError::Io(e.to_string()))?;
            let priority = by_path.get(&path).copied();
            if priority.is_some() {
                indexed.insert(path, ());
            } else {
                unscoped.push(path);
            }
            priorities.push(priority);
        }
        let unindexed = self
            .packages
            .iter()
            .filter(|p| !indexed.contains_key(&p.path))
            .map(|p| p.path.clone())
            .collect::<Vec<_>>();
        Ok(ScopePolicy {
            scope: self,
            catalog,
            priorities,
            coverage: ScopeCoverage {
                configured_packages: self.packages.len(),
                indexed_known_packages: indexed.len(),
                unindexed_paths: unindexed,
                unscoped_catalog_paths: unscoped,
                active_pack_entitlements_verified: false,
                unqualified_config_directives: self.issues.len(),
            },
        })
    }
}
impl ScopePolicy<'_> {
    pub fn resolve_key(&self, key: ResourceKey) -> EffectiveResolution {
        let candidates = self
            .catalog
            .providers(key)
            .iter()
            .map(|&provider| {
                let entry = self.catalog.provider(provider);
                let p = self
                    .priorities
                    .get(provider.package_index)
                    .copied()
                    .flatten();
                RankedProvider {
                    provider,
                    priority: p.map(|p| p.priority),
                    package_path: entry.map(|(p, _)| p.path.clone()),
                    evidence: p.map(|p| p.rules.clone()).unwrap_or_default(),
                    deleted: entry.map(|(_, e)| e.deleted()).unwrap_or(false),
                }
            })
            .collect::<Vec<_>>();
        let (state, selected) = if candidates.is_empty() {
            (EffectiveState::Missing, None)
        } else if candidates.iter().any(|c| {
            self.catalog
                .provider(c.provider)
                .map(|(_, e)| e.key != key)
                .unwrap_or(true)
        }) {
            (EffectiveState::InvalidProvider, None)
        } else if candidates.iter().any(|c| c.priority.is_none()) {
            (EffectiveState::UnconfiguredProvider, None)
        } else {
            let highest = candidates
                .iter()
                .map(|c| c.priority.unwrap())
                .max()
                .unwrap();
            let winners = candidates
                .iter()
                .filter(|c| c.priority == Some(highest))
                .collect::<Vec<_>>();
            match winners.as_slice() {
                winners if winners.iter().all(|w|w.deleted) => (EffectiveState::DeletedHighestPriority, None),
                [w] => {
                    if candidates.len() == 1 {
                        (EffectiveState::UniqueProvider, Some(w.provider))
                    } else {
                        (EffectiveState::UniqueHighestPriority, Some(w.provider))
                    }
                }
                _ => (EffectiveState::TiedHighestPriority, None),
            }
        };
        EffectiveResolution {
            key,
            state,
            selected,
            candidates,
            configured_scope_complete: self.coverage.configured_scope_complete(),
        }
    }
    pub fn provider_choices(&self) -> PolicyChoices {
        self.provider_choices_for_keys(
            self.catalog
                .by_key
                .iter()
                .filter(|(_, p)| p.len() > 1)
                .map(|(k, _)| *k),
        )
    }
    /// Prefer targeted keys for a selected Sim instead of materializing decisions
    /// for every duplicate in a multi-million-resource catalog.
    pub fn provider_choices_for_keys(
        &self,
        keys: impl IntoIterator<Item = ResourceKey>,
    ) -> PolicyChoices {
        let mut choices = BTreeMap::new();
        let mut decisions = Vec::new();
        let mut keys = keys.into_iter().collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        for key in keys {
            if self.catalog.providers(key).len() < 2 {
                continue;
            }
            let resolution = self.resolve_key(key);
            if resolution.state == EffectiveState::UniqueHighestPriority {
                choices.insert(key, resolution.selected.unwrap());
            }
            decisions.push(resolution);
        }
        PolicyChoices {
            choices,
            decisions,
            coverage: self.coverage.clone(),
            observed_game_version: self.scope.observed_game_version.clone(),
        }
    }
}
