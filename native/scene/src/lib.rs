//! Native appearance-to-resource planning. Never substitutes a generic body,
//! authoring rig or invented skin for missing or ambiguous native dependencies.
mod scope;
mod tone;
use external_cas_assets::scan::Provider;
use external_cas_assets::{Catalog, PackageReader, ReadLimits, ResourceKey};
use external_cas_tray::{AppearanceForm, SimAppearance};
pub use scope::{
    ConfigObservation, EffectiveResolution, EffectiveState, GameInstallScope, PackagePriority,
    PolicyChoices, PriorityRule, RankedProvider, ScopeCoverage, ScopeError, ScopeIssue, ScopePolicy,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub use tone::{Tone, ToneError, ToneOverlay, ToneSkinSet};

pub const TONE: u32 = 0x0354796a;
pub const RIG: u32 = 0x8eaf13de;
pub const SCULPT: u32 = 0x9d1ab874;
pub const SIM_MODIFIER: u32 = 0xc5f6763e;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormSelection {
    Current,
    Base,
    OccultIndex(usize),
}
#[derive(Clone, Debug)]
pub struct SceneOptions {
    pub form: FormSelection,
    pub outfit_index: Option<usize>,
    pub skin_set_index: Option<usize>,
    /// Explicit full-TGI/source choices; directory order never supplies priority.
    pub provider_choices: BTreeMap<ResourceKey, Provider>,
    pub read_limits: ReadLimits,
}
impl Default for SceneOptions {
    fn default() -> Self {
        Self {
            form: FormSelection::Current,
            outfit_index: None,
            skin_set_index: None,
            provider_choices: BTreeMap::new(),
            read_limits: ReadLimits::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyState {
    Missing,
    UniqueInScope,
    ExplicitProvider,
    ByteIdenticalProviders,
    Ambiguous,
    Deleted,
    InvalidProvider,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub key: ResourceKey,
    pub provider: Provider,
    pub deleted: bool,
    pub valid: bool,
}
#[derive(Clone, Debug)]
pub struct Dependency {
    pub role: String,
    pub requested_keys: Vec<ResourceKey>,
    pub candidates: Vec<Candidate>,
    pub state: DependencyState,
    pub selected: Option<Candidate>,
    pub provider_payload_hashes: Vec<(Provider, String)>,
    pub payload_error: Option<String>,
}
#[derive(Clone, Debug)]
pub enum FailureKind {
    MissingNativeValue,
    UnsupportedNativeValue,
    AmbiguousCurrentForm,
    MissingResource,
    AmbiguousResource,
    InvalidSource,
    UnsupportedFormat,
    IncompleteMaterialPipeline,
}
#[derive(Clone, Debug)]
pub struct SceneFailure {
    pub kind: FailureKind,
    pub role: String,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub struct OutfitDependency {
    pub part_index: usize,
    pub body_type: u32,
    pub color_shift: Option<u64>,
    pub resource: Dependency,
}
#[derive(Clone, Debug)]
pub struct TonePlan {
    pub resource: Dependency,
    pub raw: Option<Vec<u8>>,
    pub parsed: Option<Tone>,
    pub parse_error: Option<String>,
    pub selected_skin_set: Option<usize>,
    pub skin_set_selection_rule: String,
    pub value_shift: Option<f32>,
    pub texture_dependencies: Vec<Dependency>,
}
#[derive(Clone, Debug)]
pub struct WeightedDependency {
    pub amount: Option<f32>,
    pub dependency: Dependency,
}
#[derive(Clone, Debug)]
pub struct ScenePlan {
    pub native_sim_sha256: String,
    pub resolved_form: Option<FormSelection>,
    pub outfit_index: Option<usize>,
    pub outfit_id: Option<u64>,
    pub rig: Option<Dependency>,
    pub rig_name: Option<String>,
    pub outfit: Vec<OutfitDependency>,
    pub tone: Option<TonePlan>,
    pub rig_payload: Option<Vec<u8>>,
    pub morphs: Vec<WeightedDependency>,
    pub failures: Vec<SceneFailure>,
    /// Resource resolution does not establish a complete morphed/material Sim.
    pub complete_sim: bool,
    pub catalog_issue_count: usize,
}
impl ScenePlan {
    pub fn required_keys(&self) -> Vec<ResourceKey> {
        let mut keys = Vec::new();
        if let Some(d) = &self.rig {
            keys.extend(&d.requested_keys);
        }
        for p in &self.outfit {
            keys.extend(&p.resource.requested_keys);
        }
        if let Some(t) = &self.tone {
            keys.extend(&t.resource.requested_keys);
            for d in &t.texture_dependencies {
                keys.extend(&d.requested_keys);
            }
        }
        for m in &self.morphs {
            keys.extend(&m.dependency.requested_keys);
        }
        keys.sort();
        keys.dedup();
        keys
    }
}

/// Sims resource names use lower-case ASCII FNV-1 (multiply, then xor).
pub fn fnv64(name: &str) -> Result<u64, String> {
    if !name.is_ascii() {
        return Err("Resource names must be ASCII for the reference FNV-1 rule".into());
    }
    Ok(name.bytes().fold(14695981039346656037, |h, b| {
        h.wrapping_mul(1099511628211) ^ (b.to_ascii_lowercase() as u64)
    }))
}
/// Exact game resource key rule from SimRipper GetTS4Rig/GetRigPrefix. No file fallback.
pub fn native_rig_key(
    species: Option<u32>,
    age: Option<u32>,
    occult: u32,
) -> Result<(ResourceKey, String), String> {
    let species = match species {
        None | Some(0) => 1,
        Some(v) => v,
    };
    let age = age.ok_or("Native age is absent")?;
    if ![1, 2, 4, 8, 16, 32, 64, 128].contains(&age) {
        return Err(format!("Unsupported native age flags {age}"));
    }
    if !(1..=6).contains(&species) {
        return Err(format!("Unsupported species {species}"));
    }
    if ![1, 2, 4, 8, 16, 32, 64].contains(&occult) {
        return Err(format!("Combined or unknown occult form {occult}"));
    }
    if occult == 32 {
        return Ok((
            ResourceKey::new(RIG, 0, 0x60faa42f9b0b4e39),
            "werewolf explicit resource".into(),
        ));
    }
    if occult == 64 {
        return Ok((ResourceKey::new(RIG, 0, fnv64("nuRig")?), "nuRig".into()));
    }
    let stage = match age {
        128 => {
            if species == 1 {
                "i"
            } else {
                "c"
            }
        }
        2 => {
            if species == 1 {
                "p"
            } else {
                "c"
            }
        }
        4 => "c",
        _ => "a",
    };
    let suffix = match species {
        1 => "u",
        2 => "d",
        3 => "c",
        4 => {
            if age == 4 {
                "d"
            } else {
                "l"
            }
        }
        5 => "f",
        6 => "h",
        _ => unreachable!(),
    };
    let name = format!("{stage}{suffix}Rig");
    Ok((ResourceKey::new(RIG, 0, fnv64(&name)?), name))
}
fn dependency(
    catalog: &Catalog,
    role: impl Into<String>,
    keys: Vec<ResourceKey>,
    options: &SceneOptions,
) -> Dependency {
    let mut candidates = Vec::new();
    for key in &keys {
        for &provider in catalog.providers(*key) {
            let entry = catalog.provider(provider);
            let deleted = entry.map(|(_, e)| e.deleted()).unwrap_or(false);
            let valid = entry.map(|(_, e)| e.key == *key).unwrap_or(false);
            candidates.push(Candidate {
                key: *key,
                provider,
                deleted,
                valid,
            });
        }
    }
    let choices = candidates
        .iter()
        .filter(|c| options.provider_choices.get(&c.key) == Some(&c.provider))
        .cloned()
        .collect::<Vec<_>>();
    let (state, selected) = if choices.len() == 1 {
        let c = choices[0].clone();
        if !c.valid {
            (DependencyState::InvalidProvider, None)
        } else if c.deleted {
            (DependencyState::Deleted, None)
        } else {
            (DependencyState::ExplicitProvider, Some(c))
        }
    } else if choices.len() > 1 {
        (DependencyState::Ambiguous, None)
    } else if keys
        .iter()
        .any(|k| options.provider_choices.contains_key(k))
    {
        (DependencyState::InvalidProvider, None)
    } else {
        match candidates.as_slice() {
            [] => (DependencyState::Missing, None),
            [c] if !c.valid => (DependencyState::InvalidProvider, None),
            [c] if c.deleted => (DependencyState::Deleted, None),
            [c] => (DependencyState::UniqueInScope, Some(c.clone())),
            _ => (DependencyState::Ambiguous, None),
        }
    };
    Dependency {
        role: role.into(),
        requested_keys: keys,
        candidates,
        state,
        selected,
        provider_payload_hashes: Vec::new(),
        payload_error: None,
    }
}
struct PlanReaders<'a> {
    catalog: &'a Catalog,
    readers: BTreeMap<usize, PackageReader<'a>>,
    failed: BTreeMap<usize, String>,
}
impl<'a> PlanReaders<'a> {
    fn new(catalog: &'a Catalog) -> Self {
        Self {
            catalog,
            readers: BTreeMap::new(),
            failed: BTreeMap::new(),
        }
    }
    fn read(&mut self, candidate: &Candidate, limits: ReadLimits) -> Result<Vec<u8>, String> {
        if !candidate.valid || candidate.deleted {
            return Err("Provider is invalid or deleted".into());
        }
        let (package, entry) = self
            .catalog
            .provider(candidate.provider)
            .ok_or("Provider disappeared")?;
        let index = candidate.provider.package_index;
        if let Some(error) = self.failed.get(&index) {
            return Err(error.clone());
        }
        if !self.readers.contains_key(&index) {
            match package.reader() {
                Ok(reader) => {
                    self.readers.insert(index, reader);
                }
                Err(e) => {
                    let error = e.to_string();
                    self.failed.insert(index, error.clone());
                    return Err(error);
                }
            }
        }
        self.readers
            .get_mut(&index)
            .unwrap()
            .read_resource(entry, limits)
            .map_err(|e| e.to_string())
    }
    fn load(&mut self, d: &mut Dependency, limits: ReadLimits) -> Result<Vec<u8>, String> {
        let result = self.load_inner(d, limits);
        if let Err(error) = &result {
            d.payload_error = Some(error.clone());
        }
        result
    }
    fn load_inner(&mut self, d: &mut Dependency, limits: ReadLimits) -> Result<Vec<u8>, String> {
        if let Some(c) = d.selected.clone() {
            let bytes = self.read(&c, limits)?;
            d.provider_payload_hashes
                .push((c.provider, format!("{:x}", Sha256::digest(&bytes))));
            return Ok(bytes);
        }
        if d.state != DependencyState::Ambiguous
            || d.candidates.is_empty()
            || d.candidates.iter().any(|c| c.key != d.candidates[0].key)
        {
            return Err("No unique full TGI/content provider".into());
        }
        let mut first = None;
        let mut identical = true;
        for c in &d.candidates {
            let bytes = self.read(c, limits)?;
            d.provider_payload_hashes
                .push((c.provider, format!("{:x}", Sha256::digest(&bytes))));
            if let Some(original) = &first {
                if original != &bytes {
                    identical = false;
                }
            } else {
                first = Some(bytes);
            }
        }
        if identical {
            d.state = DependencyState::ByteIdenticalProviders;
            d.selected = Some(d.candidates[0].clone());
            Ok(first.unwrap())
        } else {
            Err(
                "Same full TGI has differing provider payloads; explicit source selection required"
                    .into(),
            )
        }
    }
}
fn check_dependency(plan: &mut ScenePlan, d: &Dependency) {
    let kind = match d.state {
        DependencyState::Missing => Some(FailureKind::MissingResource),
        DependencyState::Ambiguous => Some(FailureKind::AmbiguousResource),
        DependencyState::Deleted | DependencyState::InvalidProvider => {
            Some(FailureKind::InvalidSource)
        }
        _ => None,
    };
    if let Some(kind) = kind {
        plan.failures.push(SceneFailure {
            kind,
            role: d.role.clone(),
            detail: format!(
                "{:?}: {} candidate providers in explicitly configured scope",
                d.state,
                d.candidates.len()
            ),
        });
    }
}
fn choose_form<'a>(
    sim: &'a SimAppearance,
    selection: &FormSelection,
) -> Result<(&'a AppearanceForm, FormSelection, u32), String> {
    match selection {
        FormSelection::Base => Ok((&sim.base, FormSelection::Base, 1)),
        FormSelection::OccultIndex(i) => {
            let f = sim
                .occult_forms
                .get(*i)
                .ok_or("Selected native form is absent")?;
            Ok((
                f,
                selection.clone(),
                f.occult_type.ok_or("Native form type is absent")?,
            ))
        }
        FormSelection::Current => {
            if sim.occult_forms.is_empty() || sim.current_occult_types == Some(1) {
                return Ok((
                    &sim.base,
                    FormSelection::Base,
                    sim.current_occult_types.filter(|t| *t != 0).unwrap_or(1),
                ));
            }
            let current = sim
                .current_occult_types
                .ok_or("Current occult type is absent while alternative forms exist")?;
            let matches = sim
                .occult_forms
                .iter()
                .enumerate()
                .filter(|(_, f)| f.occult_type == Some(current))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [(i, f)] => Ok((f, FormSelection::OccultIndex(*i), current)),
                _ => Err(
                    "Current occult mask has no unique native form; select a form explicitly"
                        .into(),
                ),
            }
        }
    }
}
fn texture_keys(instance: u64) -> Vec<ResourceKey> {
    if instance == 0 {
        Vec::new()
    } else {
        [0x2bc04edf, 0x3453cf95, 0x00b2d882, 0xb6c8b6a0]
            .into_iter()
            .map(|t| ResourceKey::new(t, 0, instance))
            .collect()
    }
}
fn read_varint(raw: &[u8], at: &mut usize) -> Result<u64, String> {
    let mut v = 0u64;
    for i in 0..10 {
        let b = *raw.get(*at).ok_or("Truncated native attribute varint")?;
        *at += 1;
        if i == 9 && b > 1 {
            return Err("Native attribute varint overflow".into());
        }
        v |= ((b & 127) as u64) << (7 * i);
        if b & 128 == 0 {
            return Ok(v);
        }
    }
    Err("Overlong native attribute varint".into())
}
fn skip_proto(
    raw: &[u8],
    at: &mut usize,
    number: u32,
    wire: u8,
    depth: usize,
) -> Result<(usize, usize, Option<u64>), String> {
    if depth > 32 {
        return Err("Native attribute nesting exceeds bound".into());
    }
    let start = *at;
    let mut integer = None;
    let length = match wire {
        0 => {
            integer = Some(read_varint(raw, at)?);
            return Ok((start, *at, integer));
        }
        1 => 8,
        2 => read_varint(raw, at)? as usize,
        5 => 4,
        3 => {
            while *at < raw.len() {
                let tag = read_varint(raw, at)?;
                let n = (tag >> 3) as u32;
                let w = (tag & 7) as u8;
                if w == 4 {
                    return if n == number {
                        Ok((start, *at, None))
                    } else {
                        Err("Mismatched native attribute end-group".into())
                    };
                }
                skip_proto(raw, at, n, w, depth + 1)?;
            }
            return Err("Truncated native attribute group".into());
        }
        _ => return Err("Unsupported native attribute wire type".into()),
    };
    let value_start = *at;
    *at = at
        .checked_add(length)
        .filter(|&n| n <= raw.len())
        .ok_or("Truncated native attribute extent")?;
    Ok((value_start, *at, integer))
}
fn native_field(
    raw: &[u8],
    number: u32,
    expected_wire: u8,
) -> Result<Option<(&[u8], Option<u64>)>, String> {
    let mut at = 0;
    let mut found = None;
    let mut count = 0;
    while at < raw.len() {
        count += 1;
        if count > 1_000_000 {
            return Err("Native attribute fields exceed bound".into());
        }
        let tag = read_varint(raw, &mut at)?;
        let n = (tag >> 3) as u32;
        let wire = (tag & 7) as u8;
        if n == 0 || tag >> 3 >= 1 << 29 {
            return Err("Invalid native attribute field number".into());
        }
        let (start, end, value) = skip_proto(raw, &mut at, n, wire, 0)?;
        if n == number {
            if found.is_some() || wire != expected_wire {
                return Err("Native attribute owner is ambiguous or has changed wire type".into());
            }
            found = Some((&raw[start..end], value));
        }
    }
    Ok(found)
}
fn native_skin_state(sim: &SimAppearance) -> Result<(usize, String), String> {
    let raw = &sim.base.native.bytes;
    let Some((attributes, _)) = native_field(raw, 30, 2)? else {
        return Ok((
            0,
            "reference untanned state: native attributes absent".into(),
        ));
    };
    let Some((tracker, _)) = native_field(attributes, 27, 2)? else {
        return Ok((
            0,
            "reference untanned state: native suntan tracker absent".into(),
        ));
    };
    let level = native_field(tracker, 1, 0)?
        .and_then(|(_, v)| v)
        .unwrap_or(0);
    let level = u32::try_from(level).map_err(|_| "Native suntan level exceeds uint32")?;
    Ok((level as usize,"stored SimData.attributes[30].suntan_tracker[27].tan_level[1] (protobuf default zero if absent)".into()))
}
pub fn resolve_scene(sim: &SimAppearance, catalog: &Catalog, options: &SceneOptions) -> ScenePlan {
    let mut plan = ScenePlan {
        native_sim_sha256: sim.base.native.sha256.clone(),
        resolved_form: None,
        outfit_index: None,
        outfit_id: None,
        rig: None,
        rig_name: None,
        rig_payload: None,
        outfit: Vec::new(),
        tone: None,
        morphs: Vec::new(),
        failures: Vec::new(),
        complete_sim: false,
        catalog_issue_count: catalog.issues.len(),
    };
    let mut readers = PlanReaders::new(catalog);
    let (form, resolved, occult) = match choose_form(sim, &options.form) {
        Ok(v) => v,
        Err(e) => {
            plan.failures.push(SceneFailure {
                kind: FailureKind::AmbiguousCurrentForm,
                role: "form".into(),
                detail: e,
            });
            return plan;
        }
    };
    plan.resolved_form = Some(resolved);
    match native_rig_key(sim.extended_species, sim.age, occult) {
        Ok((key, name)) => {
            let mut d = dependency(catalog, "native rig", vec![key], options);
            match readers.load(
                &mut d,
                ReadLimits {
                    max_resource_bytes: options
                        .read_limits
                        .max_resource_bytes
                        .min(16 * 1024 * 1024),
                },
            ) {
                Ok(raw) => plan.rig_payload = Some(raw),
                Err(e) => {
                    if d.selected.is_some() {
                        plan.failures.push(SceneFailure {
                            kind: FailureKind::InvalidSource,
                            role: "native rig payload".into(),
                            detail: e,
                        });
                    }
                }
            }
            check_dependency(&mut plan, &d);
            plan.rig = Some(d);
            plan.rig_name = Some(name);
        }
        Err(e) => plan.failures.push(SceneFailure {
            kind: FailureKind::UnsupportedNativeValue,
            role: "native rig".into(),
            detail: e,
        }),
    }
    let index = options.outfit_index.or(form.current_outfit_match);
    if let Some((index, outfit)) = index.and_then(|i| form.outfits.get(i).map(|o| (i, o))) {
        plan.outfit_index = Some(index);
        plan.outfit_id = Some(outfit.outfit_id);
        for part in &outfit.parts {
            let keys = catalog
                .casp_candidates(part.cas_part_id)
                .into_iter()
                .map(|c| c.key)
                .collect::<Vec<_>>();
            let mut d = dependency(
                catalog,
                format!(
                    "outfit slot {} CASP instance {:016X} body type {}",
                    part.part_index, part.cas_part_id, part.body_type
                ),
                keys,
                options,
            );
            if d.state == DependencyState::Ambiguous {
                let _ = readers.load(
                    &mut d,
                    ReadLimits {
                        max_resource_bytes: options
                            .read_limits
                            .max_resource_bytes
                            .min(2 * 1024 * 1024),
                    },
                );
            }
            check_dependency(&mut plan, &d);
            plan.outfit.push(OutfitDependency {
                part_index: part.part_index,
                body_type: part.body_type,
                color_shift: part.color_shift,
                resource: d,
            });
        }
    } else {
        plan.failures.push(SceneFailure{kind:FailureKind::MissingNativeValue,role:"current outfit".into(),detail:"Stored category/index does not resolve, or explicit outfit selection is absent/out of range".into()});
    }
    if let Some(id) = form.skin_tone {
        let d = dependency(
            catalog,
            "native TONE",
            vec![ResourceKey::new(TONE, 0, id)],
            options,
        );
        let mut tone_plan = TonePlan {
            resource: d,
            raw: None,
            parsed: None,
            parse_error: None,
            selected_skin_set: None,
            skin_set_selection_rule: String::new(),
            value_shift: form.skin_tone_value_shift,
            texture_dependencies: Vec::new(),
        };
        if !tone_plan.resource.candidates.is_empty() {
            let read = readers.load(
                &mut tone_plan.resource,
                ReadLimits {
                    max_resource_bytes: options.read_limits.max_resource_bytes.min(1024 * 1024),
                },
            );
            match read {
                Ok(raw) => {
                    match Tone::parse(&raw) {
                        Ok(tone) => {
                            let selected_set = if let Some(i) = options.skin_set_index {
                                tone_plan.skin_set_selection_rule =
                                    "explicit skin-set selection".into();
                                Some(i)
                            } else if tone.skin_sets.len() == 1
                                || tone
                                    .skin_sets
                                    .get(1)
                                    .map(|s| s.texture_instance == 0)
                                    .unwrap_or(false)
                            {
                                tone_plan.skin_set_selection_rule="reference base-state rule: no serialized alternate tan texture".into();
                                Some(0)
                            } else {
                                match native_skin_state(sim) {
                                    Ok((i, rule)) => {
                                        tone_plan.skin_set_selection_rule = rule;
                                        Some(i)
                                    }
                                    Err(e) => {
                                        tone_plan.skin_set_selection_rule = e;
                                        None
                                    }
                                }
                            };
                            if let Some(i) = selected_set.filter(|&i| i < tone.skin_sets.len()) {
                                tone_plan.selected_skin_set = Some(i);
                                let set = &tone.skin_sets[i];
                                for (role, id) in [
                                    ("skin-set texture", set.texture_instance),
                                    ("skin-set overlay", set.overlay_instance),
                                ] {
                                    if id != 0 {
                                        let dep =
                                            dependency(catalog, role, texture_keys(id), options);
                                        check_dependency(&mut plan, &dep);
                                        tone_plan.texture_dependencies.push(dep);
                                    }
                                }
                            } else {
                                plan.failures.push(SceneFailure {
                                    kind: FailureKind::MissingNativeValue,
                                    role: "TONE skin set".into(),
                                    detail: tone_plan.skin_set_selection_rule.clone(),
                                });
                            }
                            if !tone.uninterpreted_suffix.is_empty() {
                                plan.failures.push(SceneFailure {
                                    kind: FailureKind::UnsupportedFormat,
                                    role: "TONE suffix".into(),
                                    detail: format!(
                                        "{} unparsed bytes retained",
                                        tone.uninterpreted_suffix.len()
                                    ),
                                });
                            }
                            for overlay in &tone.overlays {
                                if overlay.texture_instance != 0
                                    && Some(overlay.age_gender_flags)
                                        == sim.age.zip(sim.gender).map(|(a, g)| a | g)
                                {
                                    let dep = dependency(
                                        catalog,
                                        "age/gender TONE overlay",
                                        texture_keys(overlay.texture_instance),
                                        options,
                                    );
                                    check_dependency(&mut plan, &dep);
                                    tone_plan.texture_dependencies.push(dep);
                                }
                            }
                            tone_plan.parsed = Some(tone);
                        }
                        Err(e) => {
                            tone_plan.parse_error = Some(e.to_string());
                            plan.failures.push(SceneFailure {
                                kind: FailureKind::UnsupportedFormat,
                                role: "TONE".into(),
                                detail: e.to_string(),
                            });
                        }
                    }
                    tone_plan.raw = Some(raw);
                }
                Err(e) => plan.failures.push(SceneFailure {
                    kind: FailureKind::InvalidSource,
                    role: "TONE payload".into(),
                    detail: e,
                }),
            }
        }
        check_dependency(&mut plan, &tone_plan.resource);
        plan.tone = Some(tone_plan);
    } else {
        plan.failures.push(SceneFailure {
            kind: FailureKind::MissingNativeValue,
            role: "native TONE".into(),
            detail: "Native form contains no skin_tone ID".into(),
        });
    }
    if let Some(facial) = &form.facial {
        for &id in &facial.sculpts {
            let d = dependency(
                catalog,
                "facial sculpt",
                vec![ResourceKey::new(SCULPT, 0, id)],
                options,
            );
            check_dependency(&mut plan, &d);
            plan.morphs.push(WeightedDependency {
                amount: None,
                dependency: d,
            });
        }
        for (role, mods) in [
            ("face modifier", &facial.face_modifiers),
            ("body modifier", &facial.body_modifiers),
            ("aged face modifier", &facial.aged_face_modifiers),
            ("aged body modifier", &facial.aged_body_modifiers),
        ] {
            for m in mods {
                if let Some(id) = m.key {
                    let d = dependency(
                        catalog,
                        role,
                        vec![ResourceKey::new(SIM_MODIFIER, 0, id)],
                        options,
                    );
                    check_dependency(&mut plan, &d);
                    plan.morphs.push(WeightedDependency {
                        amount: m.amount,
                        dependency: d,
                    });
                }
            }
        }
    }
    plan.failures.push(SceneFailure{kind:FailureKind::IncompleteMaterialPipeline,role:"complete Sim".into(),detail:"Native rig/TONE/outfit resource planning does not yet apply physique, sculpt/SMOD links, all skin/detail/makeup layers, region visibility and deformation".into()});
    plan
}
