//! Actual native Sim resource, geometry and material-dependency assembly.
//! Qualification is explicit; no authoring body or neutral appearance fallback.
mod region;
mod resource;
mod tuning;
use external_cas_assets::{CasPart, Catalog, Geometry, ReadLimits, ResourceKey, Rig};
use external_cas_morph::{build_morph_plan, MorphOptions, MorphPlan};
use external_cas_scene::{
    resolve_scene, FormSelection, SceneOptions, ScenePlan, ScopeCoverage, ScopePolicy,
};
use external_cas_tray::{AppearanceForm, GeneticPart, OutfitPartAppearance, SimAppearance};
pub use region::{RegionBlock, RegionMap, RegionMembership};
use resource::Resolver;
pub use resource::{ProviderObservation, ResourceObservation};
use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
pub use tuning::{inspect_combined_xml, CombinedXmlInspection, TuningContext, TuningRecord};
#[derive(Clone, Debug)]
pub enum CharacterError {
    Cancelled,
    Invalid(String),
    Unsupported(String),
}
impl fmt::Display for CharacterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CharacterError {}
pub type Result<T> = std::result::Result<T, CharacterError>;
#[derive(Clone, Debug)]
pub struct CharacterOptions {
    pub scene: SceneOptions,
    pub morph: MorphOptions,
    pub lod: u8,
    pub read_limits: ReadLimits,
    pub max_resource_bytes: u64,
    pub max_vertices: usize,
}
impl Default for CharacterOptions {
    fn default() -> Self {
        Self {
            scene: SceneOptions::default(),
            morph: MorphOptions::default(),
            lod: 0,
            read_limits: ReadLimits::default(),
            max_resource_bytes: 512 * 1024 * 1024,
            max_vertices: 2_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Scene,
    Parts,
    Meshes,
    Materials,
    Morphs,
    Finalize,
}
#[derive(Clone, Debug)]
pub struct CharacterProgress {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
    pub detail: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateCode {
    MissingNativeState,
    MissingResource,
    AmbiguousResource,
    InvalidResource,
    UnsupportedFormat,
    ResourceBudget,
    IncompleteScope,
    NativePartSelection,
    Morphology,
    MaterialComposition,
    NativeEligibility,
}
#[derive(Clone, Debug)]
pub struct CharacterGate {
    pub code: GateCode,
    pub source: String,
    pub detail: String,
    pub required_for_complete: bool,
}
#[derive(Clone, Debug)]
pub struct MaterialDependency {
    pub role: String,
    pub declared_key: ResourceKey,
    pub ordered_candidates: Vec<Arc<ResourceObservation>>,
    pub selected: Option<Arc<ResourceObservation>>,
    pub selection_rule: String,
    pub decoder_qualified: bool,
}
#[derive(Clone, Debug)]
pub enum MeshVisibility {
    InspectionOnly,
    VisibleByRegionRule,
    HiddenByRegion {
        occluder: ResourceKey,
        region: u32,
        layer: f32,
    },
}
#[derive(Clone, Debug)]
pub struct CharacterMesh {
    pub key: ResourceKey,
    pub source: Arc<ResourceObservation>,
    pub native_rest: Option<Arc<Geometry>>,
    pub morphed_rest: Option<Arc<Geometry>>,
    pub native_rig_binding: Option<Vec<[u16; 4]>>,
    pub regions: Vec<RegionMembership>,
    pub visibility: MeshVisibility,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct CharacterPart {
    pub native_slot: OutfitPartAppearance,
    pub casp_candidates: Vec<ResourceKey>,
    pub source: Option<Arc<ResourceObservation>>,
    pub casp: Option<Arc<CasPart>>,
    pub meshes: Vec<CharacterMesh>,
    pub materials: Vec<MaterialDependency>,
    pub region_source: Option<Arc<ResourceObservation>>,
    pub region_map: Option<Arc<RegionMap>>,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct GeneticPartObservation {
    pub native: GeneticPart,
    pub casp_candidates: Vec<ResourceKey>,
    pub used_for_current_outfit: bool,
}
#[derive(Clone, Debug)]
pub struct CharacterStep {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub struct CharacterPlan {
    pub native: SimAppearance,
    pub scene: ScenePlan,
    pub morph: Option<MorphPlan>,
    pub rig: Option<Rig>,
    pub parts: Vec<CharacterPart>,
    pub genetic_parts: Vec<GeneticPartObservation>,
    pub skin_materials: Vec<MaterialDependency>,
    pub gates: Vec<CharacterGate>,
    pub steps: Vec<CharacterStep>,
    pub scope_coverage: Option<ScopeCoverage>,
    pub observed_game_version: Option<String>,
    pub resource_bytes: u64,
    pub inspection_vertices: usize,
    pub complete: bool,
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(CharacterError::Cancelled)
    } else {
        Ok(())
    }
}
fn form<'a>(native: &'a SimAppearance, scene: &ScenePlan) -> Option<&'a AppearanceForm> {
    match scene.resolved_form.as_ref()? {
        FormSelection::Base => Some(&native.base),
        FormSelection::OccultIndex(i) => native.occult_forms.get(*i),
        FormSelection::Current => None,
    }
}
fn gate(code: GateCode, source: impl Into<String>, detail: impl Into<String>) -> CharacterGate {
    CharacterGate {
        code,
        source: source.into(),
        detail: detail.into(),
        required_for_complete: true,
    }
}
fn material(
    resolver: &mut Resolver<'_, '_>,
    role: &str,
    key: ResourceKey,
    family: bool,
) -> MaterialDependency {
    let keys = if family {
        [0x2bc04edf, 0x3453cf95, 0x00b2d882]
            .into_iter()
            .map(|t| ResourceKey::new(t, key.group_id, key.instance_id))
            .collect::<Vec<_>>()
    } else {
        vec![key]
    };
    let candidates = keys
        .into_iter()
        .map(|k| resolver.get(k))
        .collect::<Vec<_>>();
    let selected = candidates.iter().find(|r| r.bytes.is_some()).cloned();
    MaterialDependency {
        role: role.into(),
        declared_key: key,
        ordered_candidates: candidates,
        selected,
        selection_rule: if family {
            "reference FetchGameTexture: LRLE, RLE2, DST; decode eligibility remains explicit"
        } else {
            "declared full TGI"
        }
        .into(),
        decoder_qualified: false,
    }
}
pub fn assemble_character<'a>(
    native: &SimAppearance,
    catalog: &'a Catalog,
    policy: Option<&ScopePolicy<'a>>,
    options: &CharacterOptions,
) -> Result<CharacterPlan> {
    assemble_character_with_progress(
        native,
        catalog,
        policy,
        options,
        &AtomicBool::new(false),
        |_| {},
    )
}
/// Missing/invalid dependencies are collected through all stages. Explicit
/// cancellation returns immediately and never publishes an incomplete new plan.
pub fn assemble_character_with_progress<'a>(
    native: &SimAppearance,
    catalog: &'a Catalog,
    policy: Option<&ScopePolicy<'a>>,
    options: &CharacterOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(CharacterProgress),
) -> Result<CharacterPlan> {
    check(cancel)?;
    if options.lod > 3 || options.max_vertices == 0 || options.max_resource_bytes == 0 {
        return Err(CharacterError::Invalid(
            "Invalid character work budget/LOD".into(),
        ));
    }
    progress(CharacterProgress {
        stage: Stage::Scene,
        completed: 0,
        total: 1,
        detail: "Resolving native form, outfit, rig and TONE".into(),
    });
    let mut scene_options = options.scene.clone();
    scene_options.read_limits = options.read_limits;
    let first = resolve_scene(native, catalog, &scene_options);
    check(cancel)?;
    let mut coverage = None;
    let mut version = None;
    if let Some(policy) = policy {
        let choices = policy.provider_choices_for_keys(first.required_keys());
        for (k, p) in choices.choices {
            scene_options.provider_choices.entry(k).or_insert(p);
        }
        coverage = Some(choices.coverage);
        version = choices.observed_game_version;
    }
    let scene = if policy.is_some() {
        resolve_scene(native, catalog, &scene_options)
    } else {
        first
    };
    check(cancel)?;
    let mut plan = CharacterPlan {
        native: native.clone(),
        scene,
        morph: None,
        rig: None,
        parts: vec![],
        genetic_parts: vec![],
        skin_materials: vec![],
        gates: vec![],
        steps: vec![CharacterStep {
            stage: Stage::Scene,
            completed: 1,
            total: 1,
            detail: "Native dependency plan retained".into(),
        }],
        scope_coverage: coverage,
        observed_game_version: version,
        resource_bytes: 0,
        inspection_vertices: 0,
        complete: false,
    };
    for failure in &plan.scene.failures {
        plan.gates.push(gate(
            GateCode::MissingNativeState,
            failure.role.clone(),
            failure.detail.clone(),
        ));
    }
    if plan
        .scope_coverage
        .as_ref()
        .map(|c| !c.configured_scope_complete())
        .unwrap_or(true)
    {
        plan.gates.push(gate(GateCode::IncompleteScope,"installed resource scope","Configured game package scope is incomplete/unproven; missing resources cannot imply absence"));
    }
    plan.rig = plan
        .scene
        .rig_payload
        .as_deref()
        .and_then(|r| match Rig::parse(r) {
            Ok(r) => Some(r),
            Err(e) => {
                plan.gates
                    .push(gate(GateCode::InvalidResource, "native RIG", e.to_string()));
                None
            }
        });
    let mut choices = scene_options.provider_choices.clone();
    for (k, p) in &options.morph.provider_choices {
        choices.insert(*k, *p);
    }
    let mut resolver = Resolver::new(
        catalog,
        policy,
        &choices,
        options.read_limits,
        options.max_resource_bytes,
        cancel,
    );
    let selected_form = form(native, &plan.scene);
    let selected_outfit =
        selected_form.and_then(|f| plan.scene.outfit_index.and_then(|i| f.outfits.get(i)));
    if let Some(f) = selected_form {
        if let Some(g) = &f.genetics {
            for part in g.parts.iter().chain(&g.growth_parts) {
                plan.genetic_parts.push(GeneticPartObservation {
                    native: part.clone(),
                    casp_candidates: catalog
                        .casp_candidates(part.id)
                        .into_iter()
                        .map(|c| c.key)
                        .collect(),
                    used_for_current_outfit: false,
                });
            }
        }
    }
    if let Some(outfit) = selected_outfit {
        let total = outfit.parts.len();
        for (index, slot) in outfit.parts.iter().enumerate() {
            check(cancel)?;
            progress(CharacterProgress {
                stage: Stage::Parts,
                completed: index,
                total,
                detail: format!(
                    "Native outfit slot {} / body type {}",
                    slot.part_index, slot.body_type
                ),
            });
            let dependency = plan
                .scene
                .outfit
                .iter()
                .find(|p| p.part_index == slot.part_index);
            let mut part = CharacterPart {
                native_slot: slot.clone(),
                casp_candidates: dependency
                    .map(|p| p.resource.requested_keys.clone())
                    .unwrap_or_default(),
                source: None,
                casp: None,
                meshes: vec![],
                materials: vec![],
                region_source: None,
                region_map: None,
                issues: vec![],
            };
            if let Some(chosen) = dependency.and_then(|p| p.resource.selected.as_ref()) {
                let source = resolver.get(chosen.key);
                if let Some(raw) = &source.bytes {
                    match CasPart::parse(raw) {
                        Ok(casp) => {
                            if casp.body_type != slot.body_type {
                                part.issues.push(format!(
                                    "Native outfit body type {} differs from CASP {}",
                                    slot.body_type, casp.body_type
                                ));
                            }
                            part.casp = Some(Arc::new(casp));
                        }
                        Err(e) => part.issues.push(e.to_string()),
                    }
                } else {
                    part.issues.push(
                        source
                            .error
                            .clone()
                            .unwrap_or("CASP payload unavailable".into()),
                    );
                }
                part.source = Some(source);
            } else {
                part.issues
                    .push("No unique full-TGI CASP selection for native outfit slot".into());
            }
            if let Some(casp) = &part.casp {
                if let Some(r) = casp
                    .roles
                    .iter()
                    .find(|r| r.role == "region-map" && r.key.instance_id != 0)
                {
                    let source = resolver.get(r.key);
                    if let Some(raw) = &source.bytes {
                        match RegionMap::parse(raw) {
                            Ok(map) => {
                                if !map.suffix.is_empty() {
                                    part.issues.push(
                                        "RMAP opaque suffix retained; visibility unqualified"
                                            .into(),
                                    );
                                }
                                part.region_map = Some(Arc::new(map));
                            }
                            Err(e) => part.issues.push(e.to_string()),
                        }
                    } else {
                        part.issues
                            .push(source.error.clone().unwrap_or("RMAP unavailable".into()));
                    }
                    part.region_source = Some(source);
                }
                let lod_keys = casp
                    .roles
                    .iter()
                    .filter(|r| {
                        r.role == "mesh" && r.lod == Some(options.lod) && r.key.instance_id != 0
                    })
                    .map(|r| r.key)
                    .collect::<Vec<_>>();
                for (key_index, key) in lod_keys.iter().enumerate() {
                    check(cancel)?;
                    progress(CharacterProgress {
                        stage: Stage::Meshes,
                        completed: key_index,
                        total: lod_keys.len(),
                        detail: key.to_string(),
                    });
                    let source = resolver.get(*key);
                    let mut mesh = CharacterMesh {
                        key: *key,
                        source: source.clone(),
                        native_rest: None,
                        morphed_rest: None,
                        native_rig_binding: None,
                        regions: part
                            .region_map
                            .as_ref()
                            .map(|r| r.regions_for(*key))
                            .unwrap_or_default(),
                        visibility: MeshVisibility::InspectionOnly,
                        issues: vec![],
                    };
                    if let Some(raw) = &source.bytes {
                        match Geometry::parse(raw) {
                            Ok(g) => {
                                if plan
                                    .inspection_vertices
                                    .checked_add(g.vertices.len())
                                    .is_none_or(|n| n > options.max_vertices)
                                {
                                    mesh.issues.push("Character vertex budget exceeded".into());
                                } else {
                                    plan.inspection_vertices += g.vertices.len();
                                    if let Some(rig) = &plan.rig {
                                        match rig.bind_geometry(&g) {
                                            Ok(binding) => mesh.native_rig_binding = Some(binding),
                                            Err(e) => mesh.issues.push(e.to_string()),
                                        }
                                    }
                                    mesh.native_rest = Some(Arc::new(g));
                                }
                            }
                            Err(e) => mesh.issues.push(e.to_string()),
                        }
                    } else {
                        mesh.issues.push(
                            source
                                .error
                                .clone()
                                .unwrap_or("Native GEOM unavailable".into()),
                        );
                    }
                    part.meshes.push(mesh);
                }
                for reference in &casp.roles {
                    if [
                        "diffuse",
                        "shadow",
                        "normal",
                        "specular",
                        "emission",
                        "color-shift-mask",
                        "new-image",
                    ]
                    .contains(&reference.role)
                        && reference.key.instance_id != 0
                    {
                        check(cancel)?;
                        let dependency = material(
                            &mut resolver,
                            reference.role,
                            reference.key,
                            reference.role == "diffuse",
                        );
                        if dependency.selected.is_none() {
                            part.issues.push(format!(
                                "Material {} {} unavailable",
                                reference.role, reference.key
                            ));
                        }
                        part.materials.push(dependency);
                    }
                }
            }
            for issue in &part.issues {
                plan.gates.push(gate(
                    GateCode::NativePartSelection,
                    format!("outfit slot {}", slot.part_index),
                    issue.clone(),
                ));
            }
            for mesh in &part.meshes {
                for issue in &mesh.issues {
                    plan.gates.push(gate(
                        GateCode::InvalidResource,
                        mesh.key.to_string(),
                        issue.clone(),
                    ));
                }
            }
            plan.parts.push(part);
        }
        plan.steps.push(CharacterStep {
            stage: Stage::Parts,
            completed: total,
            total,
            detail:
                "All native outfit slots inspected; unknown/missing content retained as diagnostics"
                    .into(),
        });
    } else {
        plan.gates.push(gate(
            GateCode::MissingNativeState,
            "native current outfit",
            "No resolved native outfit; no authoring or genetic substitute selected",
        ));
    }
    check(cancel)?;
    progress(CharacterProgress {
        stage: Stage::Materials,
        completed: 0,
        total: 1,
        detail: "Collecting actual TONE and CASP layer dependencies".into(),
    });
    if let Some(tone) = &plan.scene.tone {
        for dependency in &tone.texture_dependencies {
            if let Some(key) = dependency.requested_keys.first() {
                let m = material(&mut resolver, &dependency.role, *key, true);
                if m.selected.is_none() {
                    plan.gates.push(gate(
                        GateCode::MissingResource,
                        &dependency.role,
                        "Actual native skin texture dependency missing in scope",
                    ));
                }
                plan.skin_materials.push(m);
            }
        }
    }
    plan.resource_bytes = resolver.bytes;
    check(cancel)?;
    progress(CharacterProgress {
        stage: Stage::Morphs,
        completed: 0,
        total: 1,
        detail: "Resolving native physique, sculpt and modifier operations".into(),
    });
    let mut mo = options.morph.clone();
    mo.read_limits = options.read_limits;
    match build_morph_plan(native, &plan.scene, catalog, policy, &mo) {
        Ok(m) => {
            for issue in &m.issues {
                plan.gates.push(gate(
                    GateCode::Morphology,
                    issue.source.clone(),
                    issue.detail.clone(),
                ));
            }
            plan.morph = Some(m);
        }
        Err(e) => plan.gates.push(gate(
            GateCode::Morphology,
            "native morphology",
            e.to_string(),
        )),
    }
    check(cancel)?;
    plan.gates.push(gate(GateCode::NativePartSelection,"native visibility","CASP exclusions, opposite-frame substitutions, shoes and region-layer selection need full qualification; inspection meshes remain explicitly labeled"));
    plan.gates.push(gate(GateCode::MaterialComposition,"native materials","Texture dependency selection does not qualify skin/sculpt/outfit layer composition, color shifts, MTNF shader bindings or game lighting"));
    plan.gates.push(gate(GateCode::NativeEligibility,"complete character gate","Combined deformation ordering, required tuning and every native material/visibility gate must qualify before complete character rendering"));
    plan.steps.push(CharacterStep {
        stage: Stage::Finalize,
        completed: 1,
        total: 1,
        detail: format!(
            "{} actual native outfit slots; {} inspection vertices; {} unresolved gates",
            plan.parts.len(),
            plan.inspection_vertices,
            plan.gates.len()
        ),
    });
    progress(CharacterProgress {
        stage: Stage::Finalize,
        completed: 1,
        total: 1,
        detail: plan.steps.last().unwrap().detail.clone(),
    });
    Ok(plan)
}
