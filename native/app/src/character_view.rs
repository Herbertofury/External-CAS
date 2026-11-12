//! Whole native Sim preparation. Package IO, deformation and atlas assembly stay
//! off the UI thread; a replaced request can never publish into another Sim.
use crate::{library::SourcePin, viewport};
use eframe::egui;
use external_cas_assets::{Catalog, Geometry};
use external_cas_character::{CharacterOptions, MeshVisibility, PartSelection};
use external_cas_scene::{FormSelection, GameInstallScope};
use external_cas_tray::{ProjectDraft, SimAppearance};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub sim_id: u64,
    pub native_sha256: String,
    pub revision: u64,
    pub form: Option<usize>,
    pub outfit_index: Option<usize>,
    pub game_root: PathBuf,
    pub roots: Vec<PathBuf>,
    pub diagnostic_packages: Vec<PathBuf>,
    #[serde(default)]
    pub preview_color: Option<ColorPreview>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ColorPreview {
    pub slot: external_cas_tray::OutfitSlot,
    pub expected: Option<u64>,
    pub value: u64,
}
pub struct Request {
    pub identity: Identity,
    pub native: SimAppearance,
    pub expected_pins: Option<Vec<SourcePin>>,
}
impl Request {
    pub fn from_draft(
        draft: &ProjectDraft,
        sim_id: Option<u64>,
        form: Option<usize>,
        outfit_index: Option<usize>,
        game_root: PathBuf,
        roots: Vec<PathBuf>,
        diagnostic_packages: Vec<PathBuf>,
        preview_color: Option<ColorPreview>,
    ) -> Result<Self, String> {
        let mut staged;
        let render_draft = if let Some(preview) = &preview_color {
            staged = draft.clone();
            staged
                .apply_color_shift(
                    draft.revision(),
                    &preview.slot,
                    preview.expected,
                    preview.value,
                    if preview.expected.is_some() {
                        external_cas_tray::MissingColorShiftPolicy::RequireExisting
                    } else {
                        external_cas_tray::MissingColorShiftPolicy::MaterializeNeutralOthers
                    },
                )
                .map_err(|e| e.to_string())?;
            &staged
        } else {
            draft
        };
        let native = render_draft
            .appearance()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|sim| sim_id.is_none_or(|id| sim.sim_id == id))
            .ok_or("Selected Sim is absent from this draft")?;
        Ok(Self {
            identity: Identity {
                sim_id: native.sim_id,
                native_sha256: native.base.native.sha256.clone(),
                revision: draft.revision(),
                form,
                outfit_index,
                game_root,
                roots,
                diagnostic_packages,
                preview_color,
            },
            native,
            expected_pins: None,
        })
    }
}
struct Sources {
    game_root: PathBuf,
    roots: Vec<PathBuf>,
    diagnostic_packages: Vec<PathBuf>,
    scope: GameInstallScope,
    catalog: Catalog,
}
#[derive(serde::Serialize)]
pub struct Receipt {
    pub identity: Identity,
    pub packages: usize,
    pub resources: u64,
    pub meshes: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub source_pins: Vec<SourcePin>,
    pub gates: Vec<String>,
    pub atlas_issues: Vec<String>,
    pub deformation_issues: Vec<String>,
    pub native_qualified: bool,
    pub lighting_tuning_instance: Option<u64>,
    pub age: Option<u32>,
    pub species: Option<u32>,
    pub idle_selector_trace: Vec<String>,
    pub idle_issues: Vec<String>,
    pub idle_name: Option<String>,
}
pub struct PreparedCharacter {
    pub parts: Vec<viewport::MeshPart>,
    pub geometries: Vec<Geometry>,
    pub rig: external_cas_animation::Rig,
    pub idle: Option<external_cas_animation::Clip>,
    pub receipt: Receipt,
    pub elapsed: Duration,
    sources: Arc<Sources>,
}
enum Message {
    Progress(String),
    Ready(Result<PreparedCharacter, String>),
}
pub struct CharacterState {
    pub loading: bool,
    pub requested: Option<Identity>,
    pub receipt: Option<Receipt>,
    pub status: String,
    receiver: Option<mpsc::Receiver<Message>>,
    cancel: Arc<AtomicBool>,
    cache: Option<Arc<Sources>>,
}
impl Default for CharacterState {
    fn default() -> Self {
        Self {
            loading: false,
            requested: None,
            receipt: None,
            status: String::new(),
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            cache: None,
        }
    }
}
impl CharacterState {
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.receiver = None;
        self.loading = false;
        self.requested = None;
        self.receipt = None;
    }
    pub fn load(&mut self, request: Request, ctx: &egui::Context) {
        self.stop();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.requested = Some(request.identity.clone());
        self.loading = true;
        self.status = "Reading this Sim’s native appearance…".into();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let cache = self.cache.clone().filter(|old| {
            old.game_root == request.identity.game_root
                && old.roots == request.identity.roots
                && old.diagnostic_packages == request.identity.diagnostic_packages
        });
        let cancel = self.cancel.clone();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = prepare(request, cache, &cancel, |message| {
                let _ = tx.send(Message::Progress(message));
                wake.request_repaint();
            });
            let _ = tx.send(Message::Ready(result));
            wake.request_repaint();
        });
    }
    pub fn poll(&mut self) -> Option<Result<PreparedCharacter, String>> {
        loop {
            match self.receiver.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(Message::Progress(message))) => self.status = message,
                Some(Ok(Message::Ready(result))) => {
                    self.loading = false;
                    self.receiver = None;
                    if let Ok(prepared) = &result {
                        self.cache = Some(prepared.sources.clone());
                    }
                    return Some(result);
                }
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    self.loading = false;
                    self.receiver = None;
                    return Some(Err(
                        "Native Sim worker disconnected before publication".into()
                    ));
                }
                _ => return None,
            }
        }
    }
}
impl Drop for CharacterState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn prepare(
    request: Request,
    cache: Option<Arc<Sources>>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(String),
) -> Result<PreparedCharacter, String> {
    let start = Instant::now();
    let identity = request.identity;
    if let Some(expected) = &request.expected_pins {
        crate::library::verify_source_pins(expected,cancel).map_err(|e|format!("Saved Sim appearance sources changed; draft preserved. Select View full Sim explicitly to inspect current installed sources. {e}"))?;
    }
    let sources = if let Some(cache) = cache {
        cache.scope.revalidate().map_err(|e| e.to_string())?;
        cache
    } else {
        progress("Reading installed resource priorities…".into());
        let scope = GameInstallScope::discover(&identity.game_root).map_err(|e| e.to_string())?;
        let mut roots = if identity.diagnostic_packages.is_empty() {
            scope.package_paths()
        } else {
            identity.diagnostic_packages.clone()
        };
        roots.extend(identity.roots.iter().cloned());
        let mut last = Instant::now();
        let catalog = Catalog::scan_roots(&roots, cancel, |p| {
            if last.elapsed() >= Duration::from_millis(100) {
                progress(format!(
                    "Indexing Sim sources · {} packages · {} resources",
                    p.packages, p.resources
                ));
                last = Instant::now();
            }
        })
        .map_err(|e| e.to_string())?;
        Arc::new(Sources {
            game_root: identity.game_root.clone(),
            roots: identity.roots.clone(),
            diagnostic_packages: identity.diagnostic_packages.clone(),
            scope,
            catalog,
        })
    };
    let policy = sources
        .scope
        .prepare(&sources.catalog)
        .map_err(|e| e.to_string())?;
    let options = CharacterOptions {
        scene: external_cas_scene::SceneOptions {
            form: identity.form.map(FormSelection::OccultIndex).unwrap_or(
                if identity.outfit_index.is_some() {
                    FormSelection::Base
                } else {
                    FormSelection::Current
                },
            ),
            outfit_index: identity.outfit_index,
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = external_cas_character::assemble_character_with_progress(
        &request.native,
        &sources.catalog,
        Some(&policy),
        &options,
        cancel,
        |p| progress(p.detail),
    )
    .map_err(|e| e.to_string())?;
    let deformation = external_cas_character::build_reference_deformation(
        &plan,
        &Default::default(),
        cancel,
        |p| progress(p.detail),
    )
    .map_err(|e| e.to_string())?;
    let atlas = external_cas_material::character_atlas::build_reference_atlas(
        &plan,
        &sources.catalog,
        Some(&policy),
        cancel,
        &mut progress,
    )?;
    progress("Resolving the installed CAS stand/idle state machine…".into());
    let idle = external_cas_character::resolve_cas_stand_idle(
        &plan,
        &sources.catalog,
        Some(&policy),
        cancel,
    );
    let mut rig = external_cas_animation::Rig::parse(
        plan.scene
            .rig_payload
            .as_deref()
            .ok_or("Actual native rig is unresolved")?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(adjusted) = &deformation.rig {
        if rig.bones.len() != adjusted.bones.len() {
            return Err("Deformed rig topology differs from the original rig".into());
        }
        for (bone, native) in rig.bones.iter_mut().zip(&adjusted.bones) {
            if bone.hash != native.hash || bone.parent_index != native.parent_index {
                return Err("Deformed rig identity changed".into());
            }
            bone.position = native.position;
            bone.orientation = native.orientation;
            bone.scale = native.scale;
        }
    }
    let mut parts = Vec::new();
    let mut geometries = Vec::new();
    let mut source_pins = Vec::new();
    let mut gates = plan
        .gates
        .iter()
        .map(|g| format!("{:?}: {}: {}", g.code, g.source, g.detail))
        .collect::<Vec<_>>();
    let shared_rgba: Arc<[u8]> = atlas.image.rgba.into();
    for (pi, part) in plan.parts.iter().enumerate() {
        if let Some(source) = &part.source {
            pin(source, &mut source_pins)?;
        }
        if matches!(
            part.reference_selection,
            PartSelection::ExcludedByReference { .. }
        ) {
            continue;
        }
        for (mi, mesh) in part.meshes.iter().enumerate() {
            pin(&mesh.source, &mut source_pins)?;
            if matches!(mesh.visibility, MeshVisibility::HiddenByRegion { .. }) {
                continue;
            }
            let geometry = mesh
                .morphed_rest
                .as_ref()
                .or_else(|| {
                    deformation
                        .meshes
                        .iter()
                        .find(|m| m.part_index == pi && m.mesh_index == mi)
                        .map(|m| &m.geometry)
                })
                .or(mesh.native_rest.as_ref());
            let Some(geometry) = geometry else {
                gates.push(format!("Mesh {} cannot be rendered", mesh.key));
                continue;
            };
            if cancel.load(Ordering::Relaxed) {
                return Err("Superseded native Sim preparation".into());
            }
            // Identity skinning is replaced on the UI thread by the original
            // GEOM palette hashes through bind_scene, before any rendered frame.
            parts.push(viewport::MeshPart {
                vertices: geometry
                    .vertices
                    .iter()
                    .map(|v| viewport::Vertex {
                        position: v.position,
                        normal: v.normal,
                        uv: v.uv,
                        joints: [0; 4],
                        weights: [1., 0., 0., 0.],
                    })
                    .collect(),
                indices: geometry.indices.clone(),
                rgba: shared_rgba.clone(),
                texture_size: [atlas.image.width, atlas.image.height],
            });
            geometries.push((**geometry).clone());
        }
    }
    for source in &atlas.source_pins {
        pin(source, &mut source_pins)?;
    }
    for source in &plan.tuning {
        pin(&source.source, &mut source_pins)?;
    }
    if let Ok(idle) = &idle {
        pin(&idle.asm_source, &mut source_pins)?;
        pin(&idle.source, &mut source_pins)?;
        if let Some(source) = &idle.declared_rig_source {
            pin(source, &mut source_pins)?;
        }
    }
    if let Some(morph) = &plan.morph {
        for resource in morph
            .link_resources
            .iter()
            .chain(morph.operations.iter().map(|op| &op.resource))
        {
            if let Some((provider, raw)) = resource.selected.zip(resource.raw.as_ref()) {
                pin_provider(
                    &sources.catalog,
                    resource.key,
                    provider,
                    &external_cas_tray::sha256(raw),
                    &mut source_pins,
                )?;
            }
        }
    }
    if let Some((provider, raw)) = plan
        .scene
        .rig
        .as_ref()
        .and_then(|r| r.selected.as_ref())
        .zip(plan.scene.rig_payload.as_ref())
    {
        pin_provider(
            &sources.catalog,
            provider.key,
            provider.provider,
            &external_cas_tray::sha256(raw),
            &mut source_pins,
        )?;
    }
    if parts.is_empty() {
        return Err("This Sim has no resolved renderable native geometry".into());
    }
    sources.scope.revalidate().map_err(|e| e.to_string())?;
    crate::library::verify_source_pins(&source_pins, cancel)?;
    let receipt = Receipt {
        identity,
        packages: sources.catalog.packages.len(),
        resources: sources.catalog.progress.resources,
        meshes: parts.len(),
        vertices: geometries.iter().map(|g| g.vertices.len()).sum(),
        triangles: geometries.iter().map(|g| g.indices.len() / 3).sum(),
        source_pins,
        gates,
        atlas_issues: atlas.issues,
        deformation_issues: deformation.issues,
        native_qualified: false,
        lighting_tuning_instance: plan
            .scene
            .tone
            .as_ref()
            .and_then(|p| p.parsed.as_ref())
            .and_then(|t| t.tuning_instance),
        age: plan.native.age,
        species: plan.native.extended_species,
        idle_selector_trace: idle
            .as_ref()
            .map(|i| i.selector_trace.clone())
            .unwrap_or_default(),
        idle_issues: idle
            .as_ref()
            .map(|i| i.issues.clone())
            .unwrap_or_else(|e| vec![e.clone()]),
        idle_name: idle.as_ref().ok().map(|i| i.clip.name().to_owned()),
    };
    Ok(PreparedCharacter {
        parts,
        geometries,
        rig,
        idle: idle.ok().map(|i| i.clip),
        receipt,
        elapsed: start.elapsed(),
        sources,
    })
}
fn pin_provider(
    catalog: &Catalog,
    key: external_cas_assets::ResourceKey,
    provider: external_cas_assets::scan::Provider,
    payload: &str,
    pins: &mut Vec<SourcePin>,
) -> Result<(), String> {
    let (package, entry) = catalog
        .provider(provider)
        .ok_or("Selected source provider no longer exists")?;
    if entry.key != key {
        return Err("Selected source key changed".into());
    }
    let pin = SourcePin {
        key: key.to_string(),
        provider_path: package.path.clone(),
        ordinal: u32::try_from(provider.entry_index).map_err(|e| e.to_string())?,
        index_sha256: crate::hash_string(&package.index_sha256),
        payload_sha256: payload.into(),
    };
    if !pins.iter().any(|p| {
        p.key == pin.key && p.provider_path == pin.provider_path && p.ordinal == pin.ordinal
    }) {
        pins.push(pin);
    }
    Ok(())
}
fn pin(
    source: &external_cas_character::ResourceObservation,
    pins: &mut Vec<SourcePin>,
) -> Result<(), String> {
    let Some(selected) = source.selected else {
        return Ok(());
    };
    let provider = source
        .candidates
        .iter()
        .find(|p| p.provider == selected)
        .ok_or("Selected source receipt absent")?;
    let Some(payload) = source.payload_sha256 else {
        return Ok(());
    };
    let pin = SourcePin {
        key: source.key.to_string(),
        provider_path: provider.package_path.clone(),
        ordinal: u32::try_from(selected.entry_index).map_err(|e| e.to_string())?,
        index_sha256: crate::hash_string(&provider.index_sha256),
        payload_sha256: crate::hash_string(&payload),
    };
    if !pins.iter().any(|p| {
        p.key == pin.key && p.provider_path == pin.provider_path && p.ordinal == pin.ordinal
    }) {
        pins.push(pin);
    }
    Ok(())
}
