// SPDX-License-Identifier: GPL-3.0-or-later
//! Installed resource resolution and the public room integration models.
use crate::*;
use external_cas_assets::{Catalog, PackageIndex, ReadLimits, ResourceKey};
use external_cas_scene::{GameInstallScope, ScopePolicy};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Debug, Clone)]
pub struct RoomProviderChoice {
    pub path: PathBuf,
    pub ordinal: Option<u32>,
}
#[derive(Debug, Clone)]
pub struct RoomRequest {
    pub game_root: PathBuf,
    pub world_path: PathBuf,
    pub world_key: Option<ResourceKey>,
    pub provider_choices: BTreeMap<ResourceKey, RoomProviderChoice>,
    pub definition_choices: BTreeMap<u64, ResourceKey>,
    pub mods: Option<RoomModsRequest>,
    /// Initial static source material-set choice. All alternatives are retained;
    /// this does not infer live game material state or variant switching.
    pub material_state: u32,
    pub material_variant: u32,
    /// Diagnostic tooling only. Normal app requests leave this false. Never a
    /// game-parity or verified palette claim.
    pub diagnostic_source_bind_space: bool,
    pub lod_id: u32,
    pub max_vertices: usize,
    pub max_triangles: usize,
    pub max_resource_bytes: u64,
}
impl RoomRequest {
    pub fn installed_base(game_root: impl Into<PathBuf>) -> Self {
        let game_root = game_root.into();
        Self {
            world_path: game_root.join("Data/Shared/Worlds/Areas/CAS_GCW.world"),
            game_root,
            world_key: None,
            provider_choices: BTreeMap::new(),
            definition_choices: BTreeMap::new(),
            mods: None,
            material_state: 0,
            material_variant: 0,
            diagnostic_source_bind_space: false,
            lod_id: 0,
            max_vertices: 1_000_000,
            max_triangles: 2_000_000,
            max_resource_bytes: 128 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct RoomSourcePin {
    pub provider_path: PathBuf,
    pub key: ResourceKey,
    pub ordinal: u32,
    pub index_sha256: String,
    pub payload_sha256: String,
}
#[derive(Debug, Clone)]
pub struct RoomResource {
    pub pin: RoomSourcePin,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct RoomMesh {
    pub object_index: usize,
    pub model_key: ResourceKey,
    pub model_pin: RoomSourcePin,
    pub record: MeshRecord,
    pub geometry: MeshGeometry,
    pub material: Material,
    pub parameter_material: Option<Material>,
    pub skin: Option<RoomSkin>,
    pub material_set: Option<MaterialSet>,
    pub selected_material_reference: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct UnsupportedRoomMesh {
    pub model_pin: RoomSourcePin,
    pub record: MeshRecord,
    pub reason: String,
}
#[derive(Debug, Clone)]
pub struct RoomTexture {
    pub source: Arc<RoomResource>,
    pub decoded: Option<(u32, u32, Vec<u8>)>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RoomIssue {
    pub stage: String,
    pub detail: String,
    pub required_for_parity: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct RoomProviderCandidate {
    pub pin: Option<RoomSourcePin>,
    pub provider_path: PathBuf,
    pub ordinal: u32,
    pub priority: Option<i32>,
    pub deleted: bool,
    pub read_error: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RoomProviderDecision {
    pub key: ResourceKey,
    pub selected: RoomSourcePin,
    pub rule: String,
    pub candidates: Vec<RoomProviderCandidate>,
}
#[derive(Debug, Clone)]
pub struct RoomModel {
    pub source: Arc<RoomResource>,
    pub container: Rcol,
    pub header: Model,
    pub selected_lod: usize,
    pub external_lods: Vec<Arc<RoomResource>>,
}
#[derive(Debug, Clone)]
pub struct RoomScene {
    pub world: Arc<RoomResource>,
    pub world_objects: Vec<WorldObject>,
    pub definitions: Vec<(Arc<RoomResource>, ObjectDefinition)>,
    pub models: Vec<RoomModel>,
    pub meshes: Vec<RoomMesh>,
    pub textures: Vec<RoomTexture>,
    pub opaque_camera_light: Vec<Arc<RoomResource>>,
    pub issues: Vec<RoomIssue>,
    pub provider_decisions: Vec<RoomProviderDecision>,
    pub game_version: Option<String>,
    pub config_pins: Vec<(PathBuf, String)>,
    pub unsupported_meshes: Vec<UnsupportedRoomMesh>,
}
#[derive(Debug, Clone)]
pub struct RoomProgress {
    pub stage: &'static str,
    pub completed: usize,
    pub total: usize,
    pub detail: String,
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("room load cancelled".into())
    } else {
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(
    package: &PackageIndex,
    entry: &external_cas_assets::ResourceEntry,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<RoomResource> {
    check(cancel)?;
    let bytes = package
        .reader_cancellable(cancel)
        .and_then(|mut reader| {
            reader.read_resource_cancellable(
                entry,
                ReadLimits {
                    max_resource_bytes: limit,
                },
                cancel,
            )
        })
        .map_err(|e| e.to_string())?;
    Ok(RoomResource {
        pin: RoomSourcePin {
            provider_path: package.path.clone(),
            key: entry.key,
            ordinal: entry.ordinal,
            index_sha256: digest_bytes(&package.index_sha256),
            payload_sha256: digest(&bytes),
        },
        bytes,
    })
}
fn digest_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Loads actual CAS camera/lighting tuning from configured base Simulation
/// providers. Resource.cfg establishes provider order; the full combined and
/// SimData payloads remain source authorities. DLC/CC entitlement is separate.
pub fn load_cas_tuning(
    request: &RoomRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(RoomProgress),
) -> Result<CasTuningScene> {
    check(cancel)?;
    let scope = GameInstallScope::discover(&request.game_root).map_err(|e| e.to_string())?;
    let mods = request
        .mods
        .as_ref()
        .map(|m| discover_mods(m, cancel))
        .transpose()?;
    let simulation = std::fs::canonicalize(request.game_root.join("Data/Simulation"))
        .map_err(|e| e.to_string())?;
    let mut paths = scope
        .packages
        .iter()
        .filter(|p| p.path.parent() == Some(simulation.as_path()))
        .map(|p| p.path.clone())
        .collect::<Vec<_>>();
    paths.extend(request.provider_choices.values().map(|c| c.path.clone()));
    paths = paths
        .into_iter()
        .map(|p| std::fs::canonicalize(p).map_err(|e| e.to_string()))
        .collect::<Result<BTreeSet<_>>>()?
        .into_iter()
        .collect();
    let catalog = crate::mods::catalog_with_mods(paths.clone(), mods.as_ref(), cancel, |s| {
        progress(RoomProgress {
            stage: "tuning-indexes",
            completed: s.packages as usize,
            total: paths.len(),
            detail: s.current.display().to_string(),
        })
    })
    .map_err(|e| e.to_string())?;
    if !catalog.issues.is_empty() {
        return Err("configured CAS tuning source index failed".into());
    }
    let policy = scope.prepare(&catalog).map_err(|e| e.to_string())?;
    let mut resolver = Resolver {
        catalog: &catalog,
        policy: &policy,
        request,
        mods: mods.as_ref(),
        cancel,
        cache: BTreeMap::new(),
        decisions: Vec::new(),
        observed: 0,
        used_packages: BTreeSet::new(),
    };
    let mut out = CasTuningScene {
        sources: Vec::new(),
        cameras: Vec::new(),
        lighting: Vec::new(),
        other_records: Vec::new(),
        provider_decisions: Vec::new(),
        config_pins: scope
            .configs
            .iter()
            .map(|c| (c.path.clone(), c.sha256.clone()))
            .collect(),
        game_version: scope.observed_game_version.clone(),
        issues: Vec::new(),
        simdata: Vec::new(),
        logical_decisions: Vec::new(),
    };
    if let Some(mods) = &mods {
        out.config_pins.extend(
            mods.observations
                .iter()
                .map(|o| (o.path.clone(), o.sha256.clone())),
        );
        out.issues.extend(mods.issues.clone());
    }
    let keys: Vec<_> = catalog
        .by_key
        .keys()
        .filter(|k| k.type_id == 0x62e94d38)
        .copied()
        .collect();
    let mut logical: BTreeMap<
        (String, u64),
        Vec<(
            external_cas_character::ResolvedTuningRecord,
            Arc<RoomResource>,
        )>,
    > = BTreeMap::new();
    for (i, key) in keys.iter().enumerate() {
        check(cancel)?;
        progress(RoomProgress {
            stage: "tuning",
            completed: i,
            total: keys.len(),
            detail: key.to_string(),
        });
        let source = resolver.resource(*key)?;
        let inspection = external_cas_character::inspect_combined_classes(
            &source.bytes,
            &[
                "Client_CASCameraTuning",
                "Client_CASLightingTuning",
                "Client_CASTuning",
            ],
            cancel,
        )
        .map_err(|e| e.to_string())?;
        for issue in inspection.issues {
            out.issues.push(RoomIssue {
                stage: "tuning-shared-references".into(),
                detail: issue,
                required_for_parity: true,
            });
        }
        for record in inspection.records {
            logical
                .entry((record.class.clone(), record.instance))
                .or_default()
                .push((record, Arc::clone(&source)));
        }
        out.sources.push(source);
    }
    let mut instances: BTreeSet<_> = logical.keys().map(|(_, instance)| *instance).collect();
    instances.extend(
        catalog
            .by_key
            .keys()
            .filter(|k| {
                k.type_id == 0x545ac67a
                    && matches!(k.group_id, 0x005adec7 | 0x000a3610 | 0x004b5265)
            })
            .map(|k| k.instance_id),
    );
    let standalone: Vec<_> = catalog
        .by_key
        .keys()
        .filter(|k| {
            matches!(k.type_id, 0x255adee2 | 0x800a3690 | 0xe24b5287)
                || (k.type_id == 0x03b33ddf && instances.contains(&k.instance_id))
        })
        .copied()
        .collect();
    for (i, key) in standalone.iter().enumerate() {
        check(cancel)?;
        progress(RoomProgress {
            stage: "tuning-overrides",
            completed: i,
            total: standalone.len(),
            detail: key.to_string(),
        });
        let source = resolver.resource(*key)?;
        let inspection = external_cas_character::inspect_tuning_classes(
            &source.bytes,
            &[
                "Client_CASCameraTuning",
                "Client_CASLightingTuning",
                "Client_CASTuning",
            ],
            cancel,
        )
        .map_err(|e| e.to_string())?;
        for issue in inspection.issues {
            out.issues.push(RoomIssue {
                stage: "tuning-override".into(),
                detail: issue,
                required_for_parity: true,
            });
        }
        for record in inspection.records {
            if record.instance != key.instance_id {
                return Err(
                    "standalone CAS tuning XML instance mismatches original resource key".into(),
                );
            }
            logical
                .entry((record.class.clone(), record.instance))
                .or_default()
                .push((record, Arc::clone(&source)));
        }
        out.sources.push(source);
    }
    for ((class, instance), candidates) in logical {
        let observations = candidates
            .iter()
            .map(|(_, source)| LogicalTuningCandidate {
                source_pin: source.pin.clone(),
                priority: resolver.pin_priority(&source.pin),
            })
            .collect::<Vec<_>>();
        let explicit: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter(|(_, (_, s))| {
                request.provider_choices.get(&s.pin.key).is_some_and(|c| {
                    std::fs::canonicalize(&c.path).ok().as_ref() == Some(&s.pin.provider_path)
                        && c.ordinal.is_none_or(|o| o == s.pin.ordinal)
                })
            })
            .map(|(i, _)| i)
            .collect();
        let (selected, rule) = if explicit.len() == 1 {
            (
                explicit[0],
                "explicit caller source choice across logical tuning representations".to_string(),
            )
        } else if explicit.len() > 1 {
            return Err(
                "multiple explicit choices conflict for one logical CAS tuning record".into(),
            );
        } else {
            if observations.iter().any(|c| c.priority.is_none()) {
                return Err(format!(
                    "unconfigured logical CAS tuning candidate {class}/{instance}"
                ));
            }
            let highest = observations
                .iter()
                .filter_map(|c| c.priority)
                .max()
                .ok_or("missing logical CAS tuning candidate")?;
            let winners = observations
                .iter()
                .enumerate()
                .filter(|(_, c)| c.priority == Some(highest))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if winners.len() != 1 {
                return Err(format!(
                    "logical CAS tuning {class}/{instance} has {} tied source representations; explicit provider required",
                    winners.len()
                ));
            }
            (winners[0],"unique highest Resource.cfg priority across combined and standalone CAS tuning representations".into())
        };
        let (record, source) = candidates.into_iter().nth(selected).unwrap();
        out.logical_decisions.push(LogicalTuningDecision {
            class: class.clone(),
            instance,
            selected: source.pin.clone(),
            candidates: observations,
            rule,
        });
        match class.as_str() {
            "Client_CASCameraTuning" => out
                .cameras
                .push(crate::tuning::camera(record, source.pin.clone())?),
            "Client_CASLightingTuning" => out
                .lighting
                .push(crate::tuning::lighting(record, source.pin.clone())?),
            _ => out.other_records.push((source.pin.clone(), record)),
        }
    }
    if out.cameras.is_empty() || out.lighting.is_empty() {
        return Err(
            "configured combined tuning did not provide actual CAS camera and lighting records"
                .into(),
        );
    }
    let partners: Vec<_> = out
        .cameras
        .iter()
        .map(|c| {
            (
                ResourceKey::new(0x545ac67a, 0x005adec7, c.instance),
                0u8,
                c.instance,
            )
        })
        .chain(out.lighting.iter().map(|c| {
            (
                ResourceKey::new(0x545ac67a, 0x000a3610, c.instance),
                1u8,
                c.instance,
            )
        }))
        .chain(
            out.other_records
                .iter()
                .filter(|(_, record)| record.class == "Client_CASTuning")
                .map(|(_, record)| {
                    (
                        ResourceKey::new(0x545ac67a, 0x004b5265, record.instance),
                        2u8,
                        record.instance,
                    )
                }),
        )
        .collect();
    for (key, kind, instance) in partners {
        check(cancel)?;
        if !catalog.by_key.contains_key(&key) {
            out.issues.push(RoomIssue {
                stage: "tuning-simdata".into(),
                detail: format!(
                    "CAS SimData partner {key} absent; omitted class fields remain unknown"
                ),
                required_for_parity: true,
            });
            continue;
        }
        let source = resolver.resource(key)?;
        let document = parse_simdata(&source.bytes, cancel)?;
        let data = CasDataResource { source, document };
        if kind == 0 {
            let t = out
                .cameras
                .iter_mut()
                .filter(|t| t.instance == instance)
                .collect::<Vec<_>>();
            if t.len() != 1 {
                return Err("duplicate CAS camera logical instance".into());
            }
            crate::tuning::camera_data(t.into_iter().next().unwrap(), &data)?;
        } else if kind == 1 {
            let t = out
                .lighting
                .iter_mut()
                .filter(|t| t.instance == instance)
                .collect::<Vec<_>>();
            if t.len() != 1 {
                return Err("duplicate CAS lighting logical instance".into());
            }
            crate::tuning::lighting_data(t.into_iter().next().unwrap(), &data)?;
        }
        out.simdata.push(data);
    }
    out.issues.push(RoomIssue{stage:"camera-light-conventions".into(),detail:"Installed tuning values and SimData class-default values are source-backed. FOV axis, angle conventions, Sim placement, light direction and shader equations require independent visual/source qualification".into(),required_for_parity:true});
    scope.revalidate().map_err(|e| e.to_string())?;
    if let Some(mods) = &mods {
        mods.revalidate(cancel)?;
        mods.revalidate_catalog(&catalog, cancel)?;
    }
    for index in resolver.used_packages {
        catalog.packages[index]
            .revalidate(cancel)
            .map_err(|e| e.to_string())?;
    }
    out.provider_decisions = resolver.decisions;
    progress(RoomProgress {
        stage: "tuning-ready",
        completed: out.cameras.len() + out.lighting.len(),
        total: out.cameras.len() + out.lighting.len(),
        detail: format!(
            "{} actual camera contexts and {} lighting records",
            out.cameras.len(),
            out.lighting.len()
        ),
    });
    Ok(out)
}
struct Resolver<'a> {
    catalog: &'a Catalog,
    policy: &'a ScopePolicy<'a>,
    request: &'a RoomRequest,
    mods: Option<&'a RoomModsDiscovery>,
    cancel: &'a AtomicBool,
    cache: BTreeMap<ResourceKey, Arc<RoomResource>>,
    decisions: Vec<RoomProviderDecision>,
    observed: u64,
    used_packages: BTreeSet<usize>,
}
impl Resolver<'_> {
    fn pin_priority(&self, pin: &RoomSourcePin) -> Option<i32> {
        if let Some(priority) = self.mods.and_then(|m| m.priority(&pin.provider_path)) {
            return Some(priority);
        }
        self.policy
            .resolve_key(pin.key)
            .candidates
            .into_iter()
            .find(|c| {
                c.package_path.as_ref() == Some(&pin.provider_path)
                    && self
                        .catalog
                        .provider(c.provider)
                        .is_some_and(|(_, e)| e.ordinal == pin.ordinal)
            })
            .and_then(|c| c.priority)
    }
    fn resource(&mut self, key: ResourceKey) -> Result<Arc<RoomResource>> {
        check(self.cancel)?;
        if let Some(found) = self.cache.get(&key) {
            return Ok(Arc::clone(found));
        }
        let mut decision = self.policy.resolve_key(key);
        if let Some(mods) = self.mods {
            for candidate in &mut decision.candidates {
                if let Some(path) = &candidate.package_path {
                    if let Some(priority) = mods.priority(path) {
                        candidate.priority = Some(priority);
                    }
                }
            }
        }
        let (selected, rule) = if let Some(choice) = self.request.provider_choices.get(&key) {
            let path = std::fs::canonicalize(&choice.path).map_err(|e| e.to_string())?;
            let matches: Vec<_> = decision
                .candidates
                .iter()
                .filter(|candidate| {
                    self.catalog
                        .provider(candidate.provider)
                        .is_some_and(|(package, entry)| {
                            package.path == path
                                && choice
                                    .ordinal
                                    .is_none_or(|ordinal| ordinal == entry.ordinal)
                        })
                })
                .collect();
            if matches.len() != 1 {
                return Err(format!(
                    "explicit room provider for {key} matches {} ordinals",
                    matches.len()
                ));
            }
            (
                matches[0].provider,
                "explicit caller full-key/provider choice".to_string(),
            )
        } else {
            if decision.candidates.iter().any(|c| c.priority.is_none()) {
                return Err(format!(
                    "room dependency {key} has an unconfigured provider; select an explicit source"
                ));
            }
            let highest = decision
                .candidates
                .iter()
                .filter_map(|c| c.priority)
                .max()
                .ok_or_else(|| format!("missing room dependency {key}"))?;
            let winners: Vec<_> = decision
                .candidates
                .iter()
                .filter(|c| c.priority == Some(highest))
                .collect();
            if winners.len() != 1 {
                return Err(format!(
                    "room dependency {key} has {} tied highest-priority providers; select an explicit source",
                    winners.len()
                ));
            }
            if winners[0].deleted {
                return Err(format!("highest-priority room dependency {key} is deleted"));
            }
            let selected = winners[0].provider;
            (
                selected,
                "unique highest explicit Resource.cfg priority within declared installation/Mods scope"
                    .to_string(),
            )
        };
        let mut candidates = Vec::new();
        let mut accepted = None;
        for candidate in &decision.candidates {
            let (package, entry) = self
                .catalog
                .provider(candidate.provider)
                .ok_or("invalid room provider locator")?;
            let mut observed = RoomProviderCandidate {
                pin: None,
                provider_path: package.path.clone(),
                ordinal: entry.ordinal,
                priority: candidate.priority,
                deleted: entry.deleted(),
                read_error: None,
            };
            if !entry.deleted() {
                let limit = self
                    .request
                    .max_resource_bytes
                    .checked_sub(self.observed)
                    .ok_or("room resource budget exhausted")?;
                match read(package, entry, limit, self.cancel) {
                    Ok(resource) => {
                        self.observed = self
                            .observed
                            .checked_add(resource.bytes.len() as u64)
                            .ok_or("room resource budget overflow")?;
                        observed.pin = Some(resource.pin.clone());
                        self.used_packages.insert(candidate.provider.package_index);
                        if candidate.provider == selected {
                            accepted = Some(Arc::new(resource));
                        }
                    }
                    Err(error) => {
                        if candidate.provider == selected {
                            return Err(format!("selected room dependency {key}: {error}"));
                        }
                        observed.read_error = Some(error);
                    }
                }
            }
            candidates.push(observed);
        }
        let accepted = accepted.ok_or("selected room resource is deleted or unavailable")?;
        self.decisions.push(RoomProviderDecision {
            key,
            selected: accepted.pin.clone(),
            rule,
            candidates,
        });
        self.cache.insert(key, Arc::clone(&accepted));
        Ok(accepted)
    }
    fn definition_key(&self, instance: u64) -> Result<ResourceKey> {
        if let Some(key) = self.request.definition_choices.get(&instance) {
            if key.type_id != 0xc0db5ae7 || key.instance_id != instance {
                return Err("explicit room definition key mismatches world instance".into());
            }
            return Ok(*key);
        }
        let keys: Vec<_> = self
            .catalog
            .by_key
            .keys()
            .filter(|key| key.type_id == 0xc0db5ae7 && key.instance_id == instance)
            .copied()
            .collect();
        if keys.len() != 1 {
            return Err(format!(
                "world definition instance {instance:016X} maps to {} full keys; explicit group required",
                keys.len()
            ));
        }
        Ok(keys[0])
    }
}

/// Load the explicitly named CAS world and configured base-client providers.
/// This scope is intentional: active DLC/CC override entitlement is not inferred.
pub fn load(
    request: &RoomRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(RoomProgress),
) -> Result<RoomScene> {
    check(cancel)?;
    progress(RoomProgress {
        stage: "scope",
        completed: 0,
        total: 1,
        detail: "Reading installed Resource.cfg evidence".into(),
    });
    let scope = GameInstallScope::discover(&request.game_root).map_err(|e| e.to_string())?;
    let mods = request
        .mods
        .as_ref()
        .map(|m| discover_mods(m, cancel))
        .transpose()?;
    let client =
        std::fs::canonicalize(request.game_root.join("Data/Client")).map_err(|e| e.to_string())?;
    let mut paths = scope
        .packages
        .iter()
        .filter(|package| package.path.parent() == Some(client.as_path()))
        .map(|package| package.path.clone())
        .collect::<Vec<_>>();
    paths.extend(
        request
            .provider_choices
            .values()
            .map(|choice| choice.path.clone()),
    );
    paths = paths
        .into_iter()
        .map(|path| std::fs::canonicalize(path).map_err(|e| e.to_string()))
        .collect::<Result<BTreeSet<_>>>()?
        .into_iter()
        .collect();
    if paths.is_empty() {
        return Err("no configured base-client packages in selected installation".into());
    }
    let catalog = crate::mods::catalog_with_mods(paths.clone(), mods.as_ref(), cancel, |state| {
        progress(RoomProgress {
            stage: "indexes",
            completed: state.packages as usize,
            total: paths.len(),
            detail: state.current.display().to_string(),
        })
    })
    .map_err(|e| e.to_string())?;
    if !catalog.issues.is_empty() {
        return Err(format!(
            "configured room source indexes failed: {}",
            catalog
                .issues
                .iter()
                .map(|issue| format!("{}: {}", issue.path.display(), issue.error))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let policy = scope.prepare(&catalog).map_err(|e| e.to_string())?;
    let world_index =
        PackageIndex::open_cancellable(&request.world_path, cancel).map_err(|e| e.to_string())?;
    let worlds: Vec<_> = world_index
        .entries
        .iter()
        .filter(|entry| {
            entry.key.type_id == 0x810a102d
                && !entry.deleted()
                && request.world_key.is_none_or(|key| key == entry.key)
        })
        .collect();
    if worlds.len() != 1 {
        return Err(format!(
            "CAS world has {} selected DWorld resources; full world key required",
            worlds.len()
        ));
    }
    let world = Arc::new(read(
        &world_index,
        worlds[0],
        request.max_resource_bytes,
        cancel,
    )?);
    let world_data = parse_world_data(&world.bytes)?;
    let mut resolver = Resolver {
        catalog: &catalog,
        policy: &policy,
        request,
        mods: mods.as_ref(),
        cancel,
        cache: BTreeMap::new(),
        decisions: Vec::new(),
        observed: world.bytes.len() as u64,
        used_packages: BTreeSet::new(),
    };
    let mut scene = RoomScene {
        world,
        world_objects: world_data.objects,
        definitions: Vec::new(),
        models: Vec::new(),
        meshes: Vec::new(),
        textures: Vec::new(),
        opaque_camera_light: Vec::new(),
        unsupported_meshes: Vec::new(),
        issues: Vec::new(),
        provider_decisions: Vec::new(),
        game_version: scope.observed_game_version.clone(),
        config_pins: scope
            .configs
            .iter()
            .map(|config| (config.path.clone(), config.sha256.clone()))
            .collect(),
    };
    if let Some(mods) = &mods {
        scene.config_pins.extend(
            mods.observations
                .iter()
                .map(|o| (o.path.clone(), o.sha256.clone())),
        );
        scene.issues.extend(mods.issues.clone());
    }
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    for object_index in 0..scene.world_objects.len() {
        check(cancel)?;
        let object = &scene.world_objects[object_index];
        let mut model_keys = Vec::new();
        let mut explicit_null_object_rig = false;
        if let Some(instance) = object.definition_instance {
            let key = resolver.definition_key(instance)?;
            let resource = resolver.resource(key)?;
            let definition = parse_object_definition(&resource.bytes)?;
            explicit_null_object_rig = definition
                .links
                .get(&0xe206ae4f)
                .is_some_and(|keys| !keys.is_empty() && keys.iter().all(|k| k.is_empty()));
            if let Some(keys) = definition.models() {
                model_keys.extend_from_slice(keys);
            }
            scene.definitions.push((resource, definition));
        }
        if let Some(explicit) = object.model_key {
            if !model_keys.is_empty() && !model_keys.contains(&explicit) {
                return Err("world model and object-definition model links disagree".into());
            }
            model_keys = vec![explicit];
        }
        if model_keys.is_empty() {
            scene.issues.push(RoomIssue {
                stage: "world".into(),
                detail: format!("world object {object_index} has no qualified model link"),
                required_for_parity: true,
            });
            continue;
        }
        for model_key in model_keys {
            progress(RoomProgress {
                stage: "models",
                completed: scene.models.len(),
                total: scene.world_objects.len(),
                detail: model_key.to_string(),
            });
            let source = resolver.resource(model_key)?;
            let container = Rcol::parse(&source.bytes)?;
            let headers: Vec<_> = container
                .chunks
                .iter()
                .filter(|chunk| &chunk.tag == b"MODL")
                .collect();
            if headers.len() != 1 {
                return Err("room model must contain one MODL envelope".into());
            }
            let header = parse_model(&source.bytes, &container, headers[0])?;
            let selected: Vec<_> = header
                .lods
                .iter()
                .enumerate()
                .filter(|(_, lod)| lod.id == request.lod_id)
                .collect();
            if selected.len() != 1 {
                return Err(format!(
                    "room model has {} LODs with requested id {}",
                    selected.len(),
                    request.lod_id
                ));
            }
            let selected_lod = selected[0].0;
            let mut external_lods = Vec::new();
            for lod in &header.lods {
                if let ChunkReference::External { key, .. } = lod.target {
                    match resolver.resource(key) {
                        Ok(resource) => external_lods.push(resource),
                        Err(error) => scene.issues.push(RoomIssue {
                            stage: "alternate-lod".into(),
                            detail: error,
                            required_for_parity: false,
                        }),
                    }
                }
            }
            let lod = &header.lods[selected_lod];
            let (mesh_source, mesh_container, chunk) = match &lod.target {
                ChunkReference::Internal { index, .. } => (
                    Arc::clone(&source),
                    container.clone(),
                    container.chunks[*index].clone(),
                ),
                ChunkReference::External { key, .. } => {
                    let resource = resolver.resource(*key)?;
                    let rcol = Rcol::parse(&resource.bytes)?;
                    let chunks: Vec<_> = rcol
                        .chunks
                        .iter()
                        .filter(|chunk| &chunk.tag == b"MLOD")
                        .collect();
                    if chunks.len() != 1 {
                        return Err("external room LOD has ambiguous MLOD chunks".into());
                    }
                    let chunk = chunks[0].clone();
                    (resource, rcol, chunk)
                }
                other => return Err(format!("selected room LOD is {other:?}")),
            };
            let mlod = parse_mlod(&mesh_source.bytes, &chunk)?;
            let mesh_total = scene.meshes.len() + mlod.meshes.len();
            for record in &mlod.meshes {
                check(cancel)?;
                progress(RoomProgress {
                    stage: "meshes",
                    completed: scene.meshes.len(),
                    total: mesh_total,
                    detail: format!("{:08X}", record.name),
                });
                if record.vertex_format == 0 && record.flags & 8 != 0 {
                    let reason="Source DropShadow mesh uses implicit UShort4N/Short4_DropShadow layout; original shadow shader/projection is not yet qualified".to_string();
                    scene.issues.push(RoomIssue {
                        stage: "shadow-mesh".into(),
                        detail: format!("mesh {:08X}: {reason}", record.name),
                        required_for_parity: true,
                    });
                    scene.unsupported_meshes.push(UnsupportedRoomMesh {
                        model_pin: mesh_source.pin.clone(),
                        record: record.clone(),
                        reason,
                    });
                    continue;
                }
                let format = parse_vertex_format(
                    &mesh_source.bytes,
                    mesh_container.internal(record.vertex_format)?,
                )?;
                let mut geometry = extract_mesh_cancellable(
                    &mesh_source.bytes,
                    record,
                    format,
                    mesh_container.internal(record.vertex_buffer)?,
                    mesh_container.internal(record.index_buffer)?,
                    request.max_vertices.saturating_sub(total_vertices),
                    request.max_triangles.saturating_sub(total_triangles),
                    cancel,
                )?;
                let skin = record
                    .skin_controller
                    .filter(|r| *r != 0)
                    .map(|reference| {
                        mesh_container
                            .internal(reference)
                            .and_then(|chunk| parse_room_skin(&mesh_source.bytes, chunk))
                    })
                    .transpose()?;
                if let Err(reason) = qualify_static_skinning(
                    &mut geometry,
                    record,
                    skin.as_ref(),
                    explicit_null_object_rig,
                ) {
                    if !request.diagnostic_source_bind_space {
                        return Err(reason);
                    }
                    geometry.skinning_status = RoomSkinningStatus::DiagnosticSourceBindSpace {
                        reason: reason.clone(),
                    };
                    scene.issues.push(RoomIssue {
                        stage: "diagnostic-unqualified-bind-space".into(),
                        detail: format!("mesh {:08X}: {reason}", record.name),
                        required_for_parity: true,
                    });
                }
                total_vertices += geometry.source_vertices.len();
                total_triangles += geometry.indices.len() / 3;
                let material_chunk = mesh_container.internal(record.material)?;
                let material_set = if &material_chunk.tag == b"MTST" {
                    Some(parse_material_set(&mesh_source.bytes, material_chunk)?)
                } else {
                    None
                };
                let selected_material_reference = if let Some(set) = &material_set {
                    let matching: Vec<_> = set
                        .entries
                        .iter()
                        .filter(|e| {
                            e.state == request.material_state
                                && e.variant.is_none_or(|v| v == request.material_variant)
                        })
                        .collect();
                    if matching.len() != 1 {
                        return Err(format!(
                            "room material-set state {:08X}/variant {:08X} matches {} source entries",
                            request.material_state,
                            request.material_variant,
                            matching.len()
                        ));
                    }
                    matching[0].reference
                } else {
                    record.material
                };
                let material = parse_material(
                    &mesh_source.bytes,
                    mesh_container.internal(selected_material_reference)?,
                )?;
                let parameter_material = record
                    .parameter_material
                    .map(|reference| {
                        parse_material(&mesh_source.bytes, mesh_container.internal(reference)?)
                    })
                    .transpose()?;
                scene.meshes.push(RoomMesh {
                    object_index,
                    model_key,
                    model_pin: mesh_source.pin.clone(),
                    record: record.clone(),
                    geometry,
                    material,
                    parameter_material,
                    skin,
                    material_set,
                    selected_material_reference,
                });
            }
            scene.models.push(RoomModel {
                source,
                container,
                header,
                selected_lod,
                external_lods,
            });
        }
    }
    let texture_keys: BTreeSet<_> = scene
        .meshes
        .iter()
        .flat_map(|mesh| {
            mesh.material.fields.iter().chain(
                mesh.parameter_material
                    .iter()
                    .flat_map(|material| material.fields.iter()),
            )
        })
        .filter_map(|field| field.texture_key)
        .filter(|key| !key.is_empty())
        .collect();
    for (i, key) in texture_keys.iter().enumerate() {
        check(cancel)?;
        progress(RoomProgress {
            stage: "textures",
            completed: i,
            total: texture_keys.len(),
            detail: key.to_string(),
        });
        let source = resolver.resource(*key)?;
        let (decoded, status) = match decode_room_texture(&source.bytes) {
            Ok((width, height, rgba, status)) => (Some((width, height, rgba)), status),
            Err(error) => {
                scene.issues.push(RoomIssue {
                    stage: "texture".into(),
                    detail: format!("{key}: {error}"),
                    required_for_parity: true,
                });
                (None, error)
            }
        };
        scene.textures.push(RoomTexture {
            source,
            decoded,
            status,
        });
    }
    for entry in world_index
        .entries
        .iter()
        .filter(|entry| matches!(entry.key.type_id, 0x892c4b8a | 0x18f3c673) && !entry.deleted())
    {
        scene
            .opaque_camera_light
            .push(Arc::new(read(&world_index, entry, 1024 * 1024, cancel)?));
    }
    scene.issues.push(RoomIssue{stage:"scope".into(),detail:"Source resolution covers explicit installed base-client Resource.cfg providers; active DLC/CC room override entitlement is not inferred".into(),required_for_parity:true});
    scene.issues.push(RoomIssue{stage:"camera-light".into(),detail:"Small world camera/light payloads are preserved, not interpreted as actual CAS tuning; scene camera/light qualification remains open".into(),required_for_parity:true});
    scene.issues.push(RoomIssue{stage:"material".into(),detail:"Packed source attributes and MATD parameters are retained. Reference-tool projection and candidate native parameter reconstruction are distinct; shader/game parity requires GPU comparison".into(),required_for_parity:true});
    check(cancel)?;
    scope.revalidate().map_err(|e| e.to_string())?;
    if let Some(mods) = &mods {
        mods.revalidate(cancel)?;
        mods.revalidate_catalog(&catalog, cancel)?;
    }
    world_index.revalidate(cancel).map_err(|e| e.to_string())?;
    for index in resolver.used_packages {
        catalog.packages[index]
            .revalidate(cancel)
            .map_err(|e| e.to_string())?;
    }
    scene.provider_decisions = resolver.decisions;
    progress(RoomProgress {
        stage: "ready",
        completed: scene.meshes.len(),
        total: scene.meshes.len(),
        detail: format!(
            "{} source vertices, {} triangles",
            total_vertices, total_triangles
        ),
    });
    Ok(scene)
}
