#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod cas_ui;
mod gallery;
mod library;
mod persistence;
mod profiling;
mod texture;
mod viewport;

use eframe::egui;
use external_cas_assets::ResourceKey;
use external_cas_session::{
    CameraState, ClipSelection, PinnedFile, ResourceSelection, SessionState,
};
use library::Event;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Startup {
    roots: Vec<PathBuf>,
    ui_assets: Option<PathBuf>,
    selection: Option<ResourceKey>,
    hidden: bool,
    report: Option<PathBuf>,
    capture: Option<PathBuf>,
    exit_on_ready: bool,
    rig: Option<PathBuf>,
    pose: Option<ResourceKey>,
    tick: f32,
    project: Option<PathBuf>,
    session_root: Option<PathBuf>,
    selection_provider: Option<PathBuf>,
    pose_provider: Option<PathBuf>,
    tick_provided: bool,
    camera: Option<CameraState>,
    tab: Option<String>,
    apply_selected_slot: Option<usize>,
    selection_requested: bool,
    pose_requested: bool,
    query: Option<String>,
    catalog_columns: Option<usize>,
    profile_frames: usize,
    profile_scroll: bool,
}
impl Startup {
    fn parse() -> Result<Self, String> {
        let mut out = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--assets" => out.roots.push(PathBuf::from(
                    args.next().ok_or("--assets requires a path")?,
                )),
                "--ui-assets" => {
                    out.ui_assets = Some(absolute_path(
                        args.next()
                            .ok_or("--ui-assets requires a package path")?
                            .into(),
                    )?)
                }
                "--select" => {
                    out.selection_requested = true;
                    out.selection = Some(
                        args.next()
                            .ok_or("--select requires a TGI")?
                            .parse()
                            .map_err(|e| format!("{e}"))?,
                    )
                }
                "--report" => {
                    out.report = Some(args.next().ok_or("--report requires a path")?.into())
                }
                "--capture" => {
                    out.capture = Some(args.next().ok_or("--capture requires a path")?.into())
                }
                "--hidden" => out.hidden = true,
                "--exit-on-ready" => out.exit_on_ready = true,
                "--rig" => out.rig = Some(args.next().ok_or("--rig requires a path")?.into()),
                "--pose" => {
                    out.pose_requested = true;
                    out.pose = Some(
                        args.next()
                            .ok_or("--pose requires a TGI")?
                            .parse()
                            .map_err(|e| format!("{e}"))?,
                    )
                }
                "--pose-tick" => {
                    out.tick_provided = true;
                    out.tick = args
                        .next()
                        .ok_or("--pose-tick requires a number")?
                        .parse()
                        .map_err(|e| format!("{e}"))?;
                    if !out.tick.is_finite() || out.tick < 0. {
                        return Err("Invalid source tick".into());
                    }
                }
                "--project" => {
                    out.project = Some(args.next().ok_or("--project requires a path")?.into())
                }
                "--session-root" => {
                    out.session_root = Some(absolute_path(
                        args.next().ok_or("--session-root requires a path")?.into(),
                    )?)
                }
                "--select-provider" => {
                    out.selection_provider = Some(absolute_path(
                        args.next()
                            .ok_or("--select-provider requires a path")?
                            .into(),
                    )?)
                }
                "--pose-provider" => {
                    out.pose_provider = Some(absolute_path(
                        args.next().ok_or("--pose-provider requires a path")?.into(),
                    )?)
                }
                "--tab" => out.tab = Some(args.next().ok_or("--tab requires a name")?),
                "--query" => out.query = Some(args.next().ok_or("--query requires text")?),
                "--catalog-columns" => {
                    let count: usize = args
                        .next()
                        .ok_or("--catalog-columns requires 1–4")?
                        .parse()
                        .map_err(|e| format!("{e}"))?;
                    if !(1..=4).contains(&count) {
                        return Err("Catalog columns must be 1–4".into());
                    }
                    out.catalog_columns = Some(count);
                }
                "--profile-frames" => {
                    out.profile_frames = args
                        .next()
                        .ok_or("--profile-frames requires a count")?
                        .parse()
                        .map_err(|e| format!("{e}"))?;
                    if !(1..=36000).contains(&out.profile_frames) {
                        return Err("Profile frames must be 1–36000".into());
                    }
                }
                "--profile-scroll" => out.profile_scroll = true,
                "--apply-selected-to-slot" => {
                    out.apply_selected_slot = Some(
                        args.next()
                            .ok_or("--apply-selected-to-slot requires an index")?
                            .parse()
                            .map_err(|e| format!("{e}"))?,
                    )
                }
                "--camera" => {
                    let values = args
                        .next()
                        .ok_or("--camera requires yaw,pitch,distance,x,y,z")?
                        .split(',')
                        .map(str::parse::<f32>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?;
                    if values.len() != 6 || values.iter().any(|v| !v.is_finite()) || values[2] <= 0.
                    {
                        return Err("Invalid camera values".into());
                    }
                    out.camera = Some(CameraState {
                        yaw: values[0],
                        pitch: values[1],
                        distance: values[2],
                        target: [values[3], values[4], values[5]],
                        ..Default::default()
                    });
                }
                _ => return Err(format!("Unknown argument: {arg}")),
            }
        }
        out.roots = out
            .roots
            .into_iter()
            .map(absolute_path)
            .collect::<Result<_, _>>()?;
        out.rig = out.rig.map(absolute_path).transpose()?;
        out.project = out.project.map(absolute_path).transpose()?;
        Ok(out)
    }
}

fn main() -> eframe::Result {
    let startup = match Startup::parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440., 900.])
            .with_min_inner_size([1000., 650.])
            .with_active(false)
            .with_visible(!startup.hidden),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let diagnostic_failed = Arc::new(AtomicBool::new(false));
    let result_failed = diagnostic_failed.clone();
    let result = eframe::run_native(
        "External CAS",
        options,
        Box::new(move |cc| Ok(Box::new(Studio::new(cc, startup, diagnostic_failed)))),
    );
    if result_failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
    result
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Library,
    Sims,
    Poses,
    Project,
}
impl Tab {
    fn name(self) -> &'static str {
        match self {
            Self::Library => "wardrobe",
            Self::Sims => "sims",
            Self::Poses => "poses",
            Self::Project => "project",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value {
            "wardrobe" | "library" => Some(Self::Library),
            "sims" => Some(Self::Sims),
            "poses" => Some(Self::Poses),
            "project" => Some(Self::Project),
            _ => None,
        }
    }
}
fn absolute_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|root| root.join(path))
            .map_err(|e| e.to_string())
    }
}
fn hash_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn same_provider_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        // Package discovery returns Windows canonical verbatim paths. Compare
        // that spelling with ordinary drive/UNC paths without merging distinct
        // provider filenames or probing every source file on the UI thread.
        fn spelling(path: &std::path::Path) -> String {
            let value = path.to_string_lossy().replace('/', "\\");
            if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
                format!("\\\\{rest}")
            } else {
                value.strip_prefix("\\\\?\\").unwrap_or(&value).to_owned()
            }
        }
        spelling(a).eq_ignore_ascii_case(&spelling(b))
    }
    #[cfg(not(windows))]
    {
        false
    }
}
#[derive(Default)]
struct SessionUi {
    worker: Option<persistence::Persistence>,
    state: SessionState,
    loading: bool,
    saving: bool,
    dirty_at: Option<Instant>,
    last_saved: Option<SessionState>,
    snapshot: Option<PathBuf>,
    error: Option<String>,
    restored: bool,
    restore_pending: bool,
    restore_selection: Option<ResourceSelection>,
    restore_clip: Option<ClipSelection>,
    restore_pins: Option<Vec<library::SourcePin>>,
    restore_camera: Option<CameraState>,
    restore_tab: Option<Tab>,
    restore_project: Option<PathBuf>,
    restore_rig: Option<PinnedFile>,
    restore_bind_pose: bool,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct InstalledUiContext {
    provider: PathBuf,
    pins: Vec<external_cas_fidelity::installed::SourcePin>,
}
struct Studio {
    viewport: viewport::Viewport,
    ui_assets: cas_ui::InstalledUiState,
    ui_restore_required: bool,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    library: Option<Arc<library::Library>>,
    rows: Vec<usize>,
    gallery_groups: Vec<library::GalleryGroup>,
    gallery: gallery::Gallery,
    query: String,
    body: Option<u32>,
    query_changed: Option<Instant>,
    search_elapsed: Duration,
    search_cache_used: bool,
    search_requested_at: Option<Instant>,
    search_first_frame_ms: Option<f64>,
    search_thumbnails_ready_ms: Option<f64>,
    initial_ready_ms: Option<f64>,
    scan_cancel: Arc<AtomicBool>,
    search_cancel: Arc<AtomicBool>,
    preview_cancel: Arc<AtomicBool>,
    scan_gen: u64,
    search_gen: u64,
    preview_gen: u64,
    tray_gen: u64,
    project_gen: u64,
    scanning: bool,
    searching: bool,
    preparing: bool,
    working_project: bool,
    selected: Option<usize>,
    status: String,
    warnings: Vec<String>,
    errors: Vec<String>,
    tray: Option<external_cas_tray::TrayLibrary>,
    project: Option<external_cas_tray::TrayProject>,
    tab: Tab,
    vertices: usize,
    triangles: usize,
    preview_elapsed: Duration,
    startup: Startup,
    started: Instant,
    ready: bool,
    capture_requested: bool,
    report_written: bool,
    draft: Option<external_cas_tray::ProjectDraft>,
    slot: Option<usize>,
    geometries: Vec<external_cas_assets::Geometry>,
    rig: Option<external_cas_animation::Rig>,
    bind: Option<external_cas_animation::BindPose>,
    clip: Option<external_cas_animation::Clip>,
    rig_loading: bool,
    clip_loading: bool,
    skin_bound: bool,
    tick: f32,
    playing: bool,
    playback_clock: Instant,
    pose_last: Option<f32>,
    pose_status: String,
    rig_gen: u64,
    clip_gen: u64,
    pose_palette_sha256: Option<String>,
    scene_name: Option<String>,
    diagnostic_failed: Arc<AtomicBool>,
    scene_generation: u64,
    recovery_location: Option<PathBuf>,
    session: SessionUi,
    startup_started: bool,
    pending_scan_roots: Vec<PathBuf>,
    initial_scan: bool,
    pending_rig: Option<(u64, PathBuf, Option<String>)>,
    pending_clip: Option<(u64, ClipSelection, Option<String>, bool)>,
    pose_reset: bool,
    closing: bool,
    allow_close: bool,
    profile: profiling::Profile,
}
impl Studio {
    fn new(
        cc: &eframe::CreationContext<'_>,
        startup: Startup,
        diagnostic_failed: Arc<AtomicBool>,
    ) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = egui::Color32::from_rgb(24, 28, 29);
        style.visuals.window_fill = egui::Color32::from_rgb(30, 35, 35);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(65, 95, 82);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(68, 108, 88);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(48, 65, 58);
        style.spacing.item_spacing = egui::vec2(9., 9.);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.));
        cc.egui_ctx.set_global_style(style);
        let (tx, rx) = mpsc::channel();
        let mut out = Self {
            viewport: viewport::Viewport::new(
                cc.wgpu_render_state.clone().expect("wgpu is required"),
            ),
            ui_assets: cas_ui::InstalledUiState::new(),
            ui_restore_required: false,
            tx,
            rx,
            library: None,
            rows: Vec::new(),
            gallery_groups: Vec::new(),
            gallery: gallery::Gallery::new(),
            query: String::new(),
            body: None,
            query_changed: None,
            search_elapsed: Duration::ZERO,
            search_cache_used: false,
            search_requested_at: None,
            search_first_frame_ms: None,
            search_thumbnails_ready_ms: None,
            initial_ready_ms: None,
            scan_cancel: Arc::new(AtomicBool::new(false)),
            search_cancel: Arc::new(AtomicBool::new(false)),
            preview_cancel: Arc::new(AtomicBool::new(false)),
            scan_gen: 0,
            search_gen: 0,
            preview_gen: 0,
            tray_gen: 0,
            project_gen: 0,
            scanning: false,
            searching: false,
            preparing: false,
            working_project: false,
            selected: None,
            status: "Choose your installed content to begin".into(),
            warnings: Vec::new(),
            errors: Vec::new(),
            tray: None,
            project: None,
            tab: Tab::Library,
            vertices: 0,
            triangles: 0,
            preview_elapsed: Duration::ZERO,
            startup,
            started: Instant::now(),
            ready: false,
            capture_requested: false,
            report_written: false,
            draft: None,
            slot: None,
            geometries: Vec::new(),
            rig: None,
            bind: None,
            clip: None,
            rig_loading: false,
            clip_loading: false,
            skin_bound: false,
            tick: 0.,
            playing: false,
            playback_clock: Instant::now(),
            pose_last: None,
            pose_status: "Choose a source rig and an installed pose".into(),
            rig_gen: 0,
            clip_gen: 0,
            pose_palette_sha256: None,
            scene_name: None,
            diagnostic_failed,
            scene_generation: 0,
            recovery_location: None,
            session: SessionUi::default(),
            startup_started: false,
            pending_scan_roots: Vec::new(),
            initial_scan: false,
            pending_rig: None,
            pending_clip: None,
            pose_reset: false,
            closing: false,
            allow_close: false,
            profile: profiling::Profile::new(),
        };
        if let Some(query) = out.startup.query.clone() {
            out.query = query;
        }
        if let Some(columns) = out.startup.catalog_columns {
            out.gallery.columns = columns;
        }
        if let Some(path) = out.startup.ui_assets.clone() {
            out.ui_assets.load(&cc.egui_ctx, path);
        }
        if !out.startup.exit_on_ready || out.startup.session_root.is_some() {
            match persistence::Persistence::start(out.startup.session_root.clone()) {
                Ok(worker) => {
                    out.session.worker = Some(worker);
                    out.session.loading = true;
                    out.status = "Restoring the last verified local editor context…".into();
                }
                Err(error) => {
                    out.errors
                        .push(format!("Session storage unavailable: {error}"));
                    out.session.error = Some(error);
                    out.begin_startup();
                }
            }
        } else {
            out.begin_startup();
        }
        out
    }
    fn begin_startup(&mut self) {
        if self.startup_started {
            return;
        }
        self.startup_started = true;
        self.session.restore_pending = true;
        if let Some(camera) = self.startup.camera.clone() {
            self.session.restore_camera = Some(camera);
        }
        if let Some(tab) = self.startup.tab.as_deref() {
            if let Some(tab) = Tab::parse(tab) {
                self.session.restore_tab = Some(tab);
                self.session.state.ui_tab = tab.name().into();
            } else {
                self.errors.push("Unknown requested UI tab".into());
            }
        }
        self.ready = self.startup.roots.is_empty();
        if !self.startup.roots.is_empty() {
            self.initial_scan = true;
            self.begin_scan(self.startup.roots.clone());
        }
        if let Some(path) = self.startup.rig.clone() {
            let expected = self
                .session
                .restore_rig
                .as_ref()
                .filter(|p| p.path == path)
                .map(|p| p.sha256.clone());
            self.load_rig(path, expected);
        }
        if let Some(path) = self.startup.project.clone() {
            self.open_project(path);
        }
    }
    fn drain_session(&mut self, ctx: &egui::Context) {
        loop {
            let reply = self
                .session
                .worker
                .as_ref()
                .and_then(|worker| worker.poll());
            let Some(reply) = reply else {
                break;
            };
            match reply {
                persistence::Reply::Loaded(result) => {
                    self.session.loading = false;
                    match result {
                        Ok(report) => {
                            self.session.snapshot = report.snapshot;
                            if report.recovered_from_previous {
                                self.warnings.push("The newest session was incomplete or unsupported; a previous verified context was recovered.".into());
                            }
                            for issue in report.issues {
                                self.warnings
                                    .push(format!("Session recovery: {}", issue.message));
                            }
                            if !report.future_schemas.is_empty() {
                                let error = format!(
                                    "A newer session schema {:?} is preserved. This build will not overwrite that context.",
                                    report.future_schemas
                                );
                                self.session.error = Some(error.clone());
                                self.errors.push(error);
                            }
                            if let Some(state) = report.state {
                                self.session.restored = true;
                                self.session.last_saved = Some(state.clone());
                                self.session.state = state.clone();
                                if self.startup.ui_assets.is_none() {
                                    if let Some(value) = state.extra.get("installed_ui") {
                                        match serde_json::from_value::<InstalledUiContext>(value.clone()) {
                                            Ok(ui)=>{
                                                self.ui_restore_required=true;
                                                self.ui_assets.load_pinned(ctx,ui.provider,ui.pins);
                                            }
                                            Err(error)=>self.errors.push(format!("Saved installed UI context is invalid; original snapshot preserved: {error}")),
                                        }
                                    }
                                }
                                let same_scope = self.startup.roots.is_empty()
                                    || self.startup.roots == state.roots;
                                if self.startup.roots.is_empty() {
                                    self.startup.roots = state.roots.clone();
                                }
                                if same_scope && self.startup.selection.is_none() {
                                    self.session.restore_selection = state.selection.clone();
                                    self.startup.selection_requested = state.selection.is_some();
                                }
                                if same_scope && self.startup.pose.is_none() {
                                    self.session.restore_clip = state.clip.clone();
                                    self.startup.pose_requested = state.clip.is_some();
                                }
                                if self.startup.rig.is_none() {
                                    self.session.restore_rig = state.rig.clone();
                                    self.startup.rig =
                                        state.rig.as_ref().map(|rig| rig.path.clone());
                                }
                                if !self.startup.tick_provided && self.startup.pose.is_none() {
                                    self.startup.tick = state.tick;
                                }
                                if self.startup.project.is_none() {
                                    let path = state
                                        .recovery_project_path
                                        .clone()
                                        .or(state.project_path.clone());
                                    self.session.restore_project = path.clone();
                                    self.startup.project = path;
                                }
                                self.session.restore_camera = Some(state.camera.clone());
                                self.session.restore_tab = Tab::parse(&state.ui_tab);
                                if self.session.restore_tab.is_none() {
                                    self.warnings.push("Saved tab is not supported by this build; its identifier is retained.".into());
                                }
                                self.session.restore_bind_pose =
                                    state.extra.get("pose_mode").and_then(|v| v.as_str())
                                        == Some("bind");
                                if let Some(value) = state.extra.get("preview_source_pins") {
                                    match serde_json::from_value::<Vec<library::SourcePin>>(
                                        value.clone(),
                                    ) {
                                        Ok(pins) => self.session.restore_pins = Some(pins),
                                        Err(error) => self.errors.push(format!(
                                            "Saved preview receipt is invalid: {error}"
                                        )),
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            self.errors.push(format!(
                                "Session could not be read; files are preserved: {error}"
                            ));
                            self.session.error = Some(error);
                        }
                    }
                    self.begin_startup();
                }
                persistence::Reply::Saved { state, result } => {
                    self.session.saving = false;
                    match result {
                        Ok(receipt) => {
                            self.session.last_saved = Some(state);
                            self.session.snapshot = Some(receipt.path);
                            self.session.error = None;
                        }
                        Err(error) => {
                            self.errors.push(format!("Editor context was not saved; prior snapshots are retained: {error}"));
                            self.session.error = Some(error);
                        }
                    }
                }
            }
        }
    }
    fn mark_session_dirty(&mut self) {
        if self.session.worker.is_some() {
            self.session.dirty_at = Some(Instant::now());
        }
    }
    fn context_snapshot(&self) -> SessionState {
        let mut state = self.session.state.clone();
        if let Some(provider) = &self.ui_assets.provider {
            if let Ok(value) = serde_json::to_value(InstalledUiContext {
                provider: provider.clone(),
                pins: self.ui_assets.pins.clone(),
            }) {
                state.extra.insert("installed_ui".into(), value);
            }
        }
        state.camera.yaw = self.viewport.camera.yaw;
        state.camera.pitch = self.viewport.camera.pitch;
        state.camera.distance = self.viewport.camera.distance;
        state.camera.target = self.viewport.camera.target;
        if self.clip.is_some() {
            state.tick = self.tick;
            state.extra.insert(
                "pose_mode".into(),
                serde_json::json!(if self.pose_reset { "bind" } else { "sample" }),
            );
        }
        state
    }
    fn save_session(&mut self, force: bool) {
        if self.session.worker.is_none()
            || self.session.loading
            || self.session.saving
            || self.session.error.is_some()
        {
            return;
        }
        if self.scanning
            || self.ui_assets.is_loading()
            || self.preparing
            || self.working_project
            || self.rig_loading
            || self.clip_loading
            || self.session.restore_pending
        {
            return;
        }
        if !force
            && !self
                .session
                .dirty_at
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(450))
        {
            return;
        }
        let state = self.context_snapshot();
        if self.session.last_saved.as_ref() == Some(&state) {
            self.session.dirty_at = None;
            return;
        }
        match self.session.worker.as_ref().unwrap().save(state) {
            Ok(()) => {
                self.session.saving = true;
                self.session.dirty_at = None;
            }
            Err(error) => {
                self.errors
                    .push(format!("Session writer unavailable: {error}"));
                self.session.error = Some(error);
            }
        }
    }
    fn finish_restore(&mut self) {
        if !self.startup_started
            || !self.session.restore_pending
            || self.scanning
            || self.preparing
            || self.working_project
            || self.rig_loading
            || self.clip_loading
        {
            return;
        }
        if let Some(camera) = self.session.restore_camera.take() {
            self.viewport.camera.yaw = camera.yaw;
            self.viewport.camera.pitch = camera.pitch;
            self.viewport.camera.distance = camera.distance;
            self.viewport.camera.target = camera.target;
        }
        if let Some(tab) = self.session.restore_tab.take() {
            self.tab = tab;
        }
        self.session.restore_pending = false;
        self.initial_scan = false;
        self.mark_session_dirty();
    }
    fn startup_edit(&mut self) {
        if !self.startup_started
            || self.session.loading
            || self.session.restore_pending
            || self.scanning
            || self.preparing
            || self.working_project
            || self.rig_loading
            || self.clip_loading
        {
            return;
        }
        let Some(index) = self.startup.apply_selected_slot.take() else {
            return;
        };
        let slot = self
            .draft
            .as_ref()
            .and_then(|draft| draft.slots().ok())
            .and_then(|slots| slots.get(index).cloned());
        if let (Some(slot), Some(selected)) = (slot, self.selected) {
            if self.scene_generation == self.preview_gen && self.scene_name.is_some() {
                self.start_draft_change(library::DraftChange::Apply {
                    index: selected,
                    slot,
                });
                return;
            }
        }
        self.errors.push(
            "Requested diagnostic edit has no verified matching project slot and loaded item"
                .into(),
        );
    }
    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = true;
            self.playing = false;
            self.mark_session_dirty();
        }
        if !self.closing || self.allow_close {
            return;
        }
        if self.scanning
            || self.preparing
            || self.working_project
            || self.rig_loading
            || self.clip_loading
            || self.session.loading
        {
            return;
        }
        self.save_session(true);
        if self.session.saving {
            return;
        }
        if self.session.error.is_some() && !self.startup.exit_on_ready {
            return;
        }
        self.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    fn start_scan(&mut self, roots: Vec<PathBuf>) {
        self.gallery.reset(self.scan_gen.wrapping_add(1));
        self.initial_scan = false;
        self.session.restore_selection = None;
        self.session.restore_clip = None;
        self.session.restore_pins = None;
        self.begin_scan(roots);
    }
    fn begin_scan(&mut self, roots: Vec<PathBuf>) {
        self.pending_scan_roots = roots.clone();
        self.scan_cancel.store(true, Ordering::Relaxed);
        self.scan_cancel = Arc::new(AtomicBool::new(false));
        self.scan_gen += 1;
        self.scanning = true;
        self.preview_cancel.store(true, Ordering::Relaxed);
        self.preview_gen += 1;
        self.preparing = false;
        self.status = "Discovering authorized local packages…".into();
        self.ready = false;
        let (generation, cancel, tx) = (self.scan_gen, self.scan_cancel.clone(), self.tx.clone());
        std::thread::spawn(move || library::scan(roots, generation, cancel, tx));
    }
    fn start_search(&mut self) {
        let Some(library) = &self.library else {
            return;
        };
        self.search_cancel.store(true, Ordering::Relaxed);
        self.search_cancel = Arc::new(AtomicBool::new(false));
        self.search_gen += 1;
        self.search_requested_at = Some(self.query_changed.unwrap_or_else(Instant::now));
        self.search_first_frame_ms = None;
        self.search_thumbnails_ready_ms = None;
        self.searching = true;
        let (library, query, body, generation, cancel, tx) = (
            library.clone(),
            self.query.clone(),
            self.body,
            self.search_gen,
            self.search_cancel.clone(),
            self.tx.clone(),
        );
        std::thread::spawn(move || library::search(library, query, body, generation, cancel, tx));
        self.query_changed = None;
    }
    fn select(&mut self, index: usize) {
        self.select_with_pins(index, None);
    }
    fn select_with_pins(&mut self, index: usize, expected_pins: Option<Vec<library::SourcePin>>) {
        if self.scanning {
            return;
        }
        let Some(library) = &self.library else {
            return;
        };
        self.selected = Some(index);
        self.preview_cancel.store(true, Ordering::Relaxed);
        self.preview_cancel = Arc::new(AtomicBool::new(false));
        self.preview_gen += 1;
        self.preparing = true;
        self.ready = false;
        self.status = "Loading original geometry and material…".into();
        let (library, generation, cancel, tx) = (
            library.clone(),
            self.preview_gen,
            self.preview_cancel.clone(),
            self.tx.clone(),
        );
        std::thread::spawn(move || {
            if let Some(pins) = expected_pins {
                if let Err(error) = library::verify_source_pins(&pins, &cancel) {
                    let _=tx.send(Event::Preview{generation,result:Err(format!("Saved appearance sources changed; reselect explicitly to inspect current content. {error}"))});
                    return;
                }
            }
            library::prepare(library, index, generation, cancel, tx)
        });
    }
    fn item_context(library: &library::Library, index: usize) -> Result<ResourceSelection, String> {
        let item = library.items.get(index).ok_or("Selected item is absent")?;
        let (package, _) = library
            .catalog
            .provider(item.source)
            .ok_or("Selected provider is absent")?;
        let mut context = ResourceSelection::new(item.key.to_string(), package.path.clone());
        context.payload_sha256 = Some(hash_string(&item.payload_sha256));
        context.source_index_sha256 = Some(hash_string(&package.index_sha256));
        Ok(context)
    }
    fn choose_clip(
        &mut self,
        library: Arc<library::Library>,
        provider: external_cas_assets::scan::Provider,
        expected: Option<ClipSelection>,
    ) {
        let Some((package, entry)) = library.catalog.provider(provider) else {
            self.errors.push("Pose provider is absent".into());
            return;
        };
        let mut context =
            ClipSelection::new(entry.key.to_string(), package.path.clone(), String::new());
        context.source_index_sha256 = Some(hash_string(&package.index_sha256));
        if let Some(saved) = &expected {
            if saved.key != context.key
                || !same_provider_path(&saved.provider_path, &context.provider_path)
                || saved
                    .source_index_sha256
                    .as_ref()
                    .is_some_and(|hash| Some(hash) != context.source_index_sha256.as_ref())
            {
                self.errors.push("Saved pose provider or package index changed; choose the current pose explicitly.".into());
                return;
            }
            context.extra = saved.extra.clone();
        }
        self.clip_loading = true;
        self.clip_gen += 1;
        let generation = self.clip_gen;
        self.playing = false;
        self.pose_reset = true;
        self.pose_last = Some(self.tick);
        self.viewport.reset_pose();
        self.pose_palette_sha256 = None;
        let restoring = expected.is_some();
        let expected_payload = expected.map(|saved| saved.payload_sha256);
        self.pending_clip = Some((generation, context, expected_payload, restoring));
        let tx = self.tx.clone();
        std::thread::spawn(move || library::load_pose(library, provider, generation, tx));
    }
    fn drain(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Progress {
                    generation,
                    message,
                } if generation == self.scan_gen => self.status = message,
                Event::Library { generation, result } if generation == self.scan_gen => {
                    self.scanning = false;
                    self.preview_cancel.store(true, Ordering::Relaxed);
                    self.preview_gen += 1;
                    self.preparing = false;
                    match result {
                        Ok(library) => {
                            self.status = format!(
                                "{} appearance items · {} packages",
                                library.items.len(),
                                library.catalog.packages.len()
                            );
                            self.rows = (0..library.items.len()).collect();
                            self.gallery_groups.clear();
                            self.gallery.reset(self.scan_gen);
                            self.selected = None;
                            self.clear_scene();
                            self.clip_gen += 1;
                            self.clip = None;
                            self.pending_clip = None;
                            self.clip_loading = false;
                            let saved_selection = self.session.restore_selection.take();
                            let requested_key = self.startup.selection.take().or_else(|| {
                                saved_selection.as_ref().and_then(|s| s.key.parse().ok())
                            });
                            let requested_provider =
                                self.startup.selection_provider.take().or_else(|| {
                                    saved_selection.as_ref().map(|s| s.provider_path.clone())
                                });
                            let matches = library
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| {
                                    Some(item.key) == requested_key
                                        && requested_provider.as_ref().is_none_or(|path| {
                                            library.catalog.provider(item.source).is_some_and(
                                                |(p, _)| same_provider_path(&p.path, path),
                                            )
                                        })
                                })
                                .map(|(i, _)| i)
                                .collect::<Vec<_>>();
                            let selected = if requested_key.is_none() {
                                None
                            } else if matches.len() == 1 {
                                let index = matches[0];
                                let current = Self::item_context(&library, index);
                                if let Some(saved) = &saved_selection {
                                    match current {
                                        Ok(current)
                                            if saved.payload_sha256.is_some()
                                                && saved.payload_sha256
                                                    == current.payload_sha256
                                                && saved
                                                    .source_index_sha256
                                                    .as_ref()
                                                    .is_none_or(|h| {
                                                        Some(h)
                                                            == current.source_index_sha256.as_ref()
                                                    })
                                                && self
                                                    .session
                                                    .restore_pins
                                                    .as_ref()
                                                    .is_some_and(|pins| !pins.is_empty()) =>
                                        {
                                            Some(index)
                                        }
                                        _ => {
                                            self.errors.push("Saved appearance content or its dependency receipt changed/is unavailable; select current content explicitly.".into());
                                            None
                                        }
                                    }
                                } else {
                                    Some(index)
                                }
                            } else {
                                self.errors.push(format!("Requested appearance has {} matching providers; no source was chosen.",matches.len()));
                                None
                            };
                            self.session.state.roots = self.pending_scan_roots.clone();
                            if saved_selection.is_none() {
                                self.session.state.selection = None;
                                self.session.state.extra.remove("preview_source_pins");
                            }
                            let saved_clip = self.session.restore_clip.take();
                            if saved_clip.is_none() {
                                self.session.state.clip = None;
                            }
                            let pose_key =
                                self.startup.pose.take().or_else(|| {
                                    saved_clip.as_ref().and_then(|s| s.key.parse().ok())
                                });
                            let pose_provider =
                                self.startup.pose_provider.take().or_else(|| {
                                    saved_clip.as_ref().map(|s| s.provider_path.clone())
                                });
                            let pose_matches = library
                                .poses
                                .iter()
                                .filter(|(provider, key, _)| {
                                    Some(*key) == pose_key
                                        && pose_provider.as_ref().is_none_or(|path| {
                                            library.catalog.provider(*provider).is_some_and(
                                                |(p, _)| same_provider_path(&p.path, path),
                                            )
                                        })
                                })
                                .map(|(provider, _, _)| *provider)
                                .collect::<Vec<_>>();
                            self.library = Some(library.clone());
                            self.start_search();
                            self.mark_session_dirty();
                            if let Some(index) = selected {
                                let pins = if saved_selection.is_some() {
                                    self.session.restore_pins.take()
                                } else {
                                    None
                                };
                                self.select_with_pins(index, pins);
                            } else {
                                self.ready = true;
                            }
                            if pose_key.is_some() {
                                if pose_matches.len() == 1 {
                                    self.choose_clip(library, pose_matches[0], saved_clip);
                                } else {
                                    self.errors.push(format!("Requested pose has {} matching providers; no source was chosen.",pose_matches.len()));
                                }
                            }
                        }
                        Err(error) => {
                            self.status = error.clone();
                            self.errors.push(error);
                            self.ready = true;
                        }
                    }
                }
                Event::Search {
                    generation,
                    rows,
                    elapsed,
                    cache_used,
                    groups,
                } if generation == self.search_gen => {
                    self.rows = rows;
                    self.gallery_groups = groups;
                    self.searching = false;
                    self.search_elapsed = elapsed;
                    self.search_cache_used = cache_used;
                }
                Event::Preview { generation, result } if generation == self.preview_gen => {
                    self.preparing = false;
                    match result {
                        Ok(preview) => match self.viewport.set_parts(preview.parts) {
                            Ok(()) => {
                                self.vertices = preview.vertices;
                                self.triangles = preview.triangles;
                                self.preview_elapsed = preview.elapsed;
                                self.warnings.extend(preview.warnings);
                                self.geometries = preview.geometries;
                                self.scene_name = self
                                    .library
                                    .as_ref()
                                    .and_then(|l| self.selected.and_then(|i| l.items.get(i)))
                                    .map(|i| i.name.clone());
                                self.scene_generation = self.preview_gen;
                                if let Some((library, index)) =
                                    self.library.as_ref().zip(self.selected)
                                {
                                    if let Ok(mut context) = Self::item_context(library, index) {
                                        if let Some(previous) =
                                            self.session.state.selection.as_ref().filter(|p| {
                                                p.key == context.key
                                                    && p.provider_path == context.provider_path
                                            })
                                        {
                                            context.extra = previous.extra.clone();
                                        }
                                        self.session.state.selection = Some(context);
                                    }
                                }
                                match serde_json::to_value(&preview.source_pins) {
                                    Ok(value) => {
                                        self.session
                                            .state
                                            .extra
                                            .insert("preview_source_pins".into(), value);
                                    }
                                    Err(error) => {
                                        self.session.error = Some(error.to_string());
                                        self.errors.push(format!(
                                            "Preview receipt could not be persisted: {error}"
                                        ));
                                    }
                                }
                                self.bind_scene();
                                self.status = "Original item loaded · bind geometry preview".into();
                                self.mark_session_dirty();
                            }
                            Err(e) => {
                                self.clear_scene();
                                self.errors.push(e.clone());
                                self.status = e;
                            }
                        },
                        Err(e) => {
                            self.clear_scene();
                            self.status = e.clone();
                            self.errors.push(e);
                        }
                    }
                    self.ready = true;
                }
                Event::Tray { generation, result } if generation == self.tray_gen => match result {
                    Ok(tray) => {
                        self.status = format!(
                            "{} households · {} file issues",
                            tray.groups.len(),
                            tray.issues.len()
                        );
                        self.tray = Some(tray);
                    }
                    Err(e) => self.errors.push(e),
                },
                Event::Project { generation, result } if generation == self.project_gen => {
                    self.working_project = false;
                    self.ready = true;
                    match result {
                        Ok(prepared) => {
                            let path = prepared.project.directory.clone();
                            let restored = self
                                .session
                                .restore_project
                                .as_ref()
                                .is_some_and(|saved| same_provider_path(saved, &path));
                            if restored
                                && self
                                    .session
                                    .state
                                    .recovery_project_path
                                    .as_ref()
                                    .is_some_and(|saved| same_provider_path(saved, &path))
                            {
                                self.recovery_location = Some(path.clone());
                            } else {
                                self.session.state.project_path = Some(path);
                                self.session.state.recovery_project_path = None;
                                self.recovery_location = None;
                            }
                            self.session.restore_project = None;
                            self.status = "Project reopened and byte-verified".into();
                            self.draft = prepared.draft;
                            if let Some(error) = prepared.draft_error {
                                self.errors.push(error);
                            }
                            self.project = Some(prepared.project);
                            self.slot = None;
                            self.tab = Tab::Project;
                            self.mark_session_dirty();
                        }
                        Err(e) => self.errors.push(e),
                    }
                }
                Event::Rig { generation, result } if generation == self.rig_gen => {
                    self.rig_loading = false;
                    let pending = self.pending_rig.take();
                    match result {
                        Ok(rig) => {
                            let observed = hash_string(&rig.sha256);
                            if let Some((_, path, expected)) = pending {
                                if expected.as_ref().is_some_and(|hash| hash != &observed) {
                                    self.errors.push("Saved rig bytes changed; choose the rig explicitly to inspect current content.".into());
                                    continue;
                                }
                                match rig.bind_pose() {
                                    Ok(bind) => {
                                        let mut context = PinnedFile::new(path, observed);
                                        if let Some(previous) = self
                                            .session
                                            .state
                                            .rig
                                            .as_ref()
                                            .filter(|p| p.path == context.path)
                                        {
                                            context.extra = previous.extra.clone();
                                        }
                                        self.session.state.rig = Some(context);
                                        self.bind = Some(bind);
                                        self.rig = Some(rig);
                                        self.bind_scene();
                                        self.mark_session_dirty();
                                    }
                                    Err(e) => self.errors.push(e.to_string()),
                                }
                            } else {
                                self.errors
                                    .push("Rig load has no matching source request".into());
                            }
                        }
                        Err(e) => self.errors.push(e),
                    }
                }
                Event::Clip { generation, result } if generation == self.clip_gen => {
                    self.clip_loading = false;
                    let pending = self.pending_clip.take();
                    match result {
                        Ok(clip) => {
                            if let Some((_, mut context, expected, restoring)) = pending {
                                let observed = hash_string(&clip.sha256);
                                if expected.as_ref().is_some_and(|hash| hash != &observed) {
                                    self.errors.push("Saved pose payload changed; choose the current pose explicitly.".into());
                                    continue;
                                }
                                context.payload_sha256 = observed;
                                self.session.state.clip = Some(context);
                                self.tick = if restoring || self.startup.tick_provided {
                                    self.startup.tick.min(clip.max_tick().unwrap_or(0) as f32)
                                } else {
                                    0.
                                };
                                self.pose_reset = restoring && self.session.restore_bind_pose;
                                self.startup.tick_provided = false;
                                self.clip = Some(clip);
                                self.pose_last = None;
                                self.playing = false;
                                self.pose_status = "Pose decoded · explicit preview policy".into();
                                self.mark_session_dirty();
                            } else {
                                self.errors
                                    .push("Pose load has no matching source request".into());
                            }
                        }
                        Err(e) => self.errors.push(e),
                    }
                }
                Event::Draft {
                    generation,
                    scan_generation,
                    selection_generation,
                    base_revision,
                    result,
                } if generation == self.project_gen => {
                    self.working_project = false;
                    if scan_generation != self.scan_gen
                        || selection_generation != self.preview_gen
                        || self
                            .draft
                            .as_ref()
                            .is_none_or(|d| d.revision() != base_revision)
                    {
                        self.status =
                            "Selection changed while validating the edit; active draft retained"
                                .into();
                        continue;
                    }
                    match result {
                        Ok((draft, path)) => {
                            self.draft = Some(draft);
                            self.session.state.recovery_project_path = Some(path.clone());
                            self.recovery_location = Some(path);
                            self.slot = None;
                            self.status = "Draft updated and recovery revision verified".into();
                            self.mark_session_dirty();
                        }
                        Err(error) => {
                            self.status = "Edit was not applied".into();
                            self.errors.push(error);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fn clear_scene(&mut self) {
        let _ = self.viewport.set_parts(Vec::new());
        self.scene_name = None;
        self.scene_generation = 0;
        self.geometries.clear();
        self.vertices = 0;
        self.triangles = 0;
        self.skin_bound = false;
        self.playing = false;
        self.pose_last = None;
        self.pose_palette_sha256 = None;
    }
    fn start_draft_change(&mut self, change: library::DraftChange) {
        let Some(draft) = self.draft.clone() else {
            return;
        };
        if self.working_project {
            return;
        }
        self.working_project = true;
        self.project_gen += 1;
        let source_pins = self
            .session
            .state
            .extra
            .get("preview_source_pins")
            .and_then(|value| serde_json::from_value::<Vec<library::SourcePin>>(value.clone()).ok())
            .unwrap_or_default();
        let (library, generation, scan_generation, selection_generation, tx) = (
            self.library.clone(),
            self.project_gen,
            self.scan_gen,
            self.preview_gen,
            self.tx.clone(),
        );
        std::thread::spawn(move || {
            library::change_draft(
                draft,
                change,
                library,
                source_pins,
                generation,
                scan_generation,
                selection_generation,
                tx,
            )
        });
    }
    fn load_rig(&mut self, path: PathBuf, expected: Option<String>) {
        self.rig_loading = true;
        self.rig_gen += 1;
        let generation = self.rig_gen;
        let tx = self.tx.clone();
        self.pending_rig = Some((generation, path.clone(), expected));
        std::thread::spawn(move || {
            let result = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    external_cas_animation::Rig::parse(&raw).map_err(|e| e.to_string())
                });
            let _ = tx.send(Event::Rig { generation, result });
        });
    }
    fn bind_scene(&mut self) {
        self.playing = false;
        self.pose_last = None;
        self.skin_bound = false;
        self.pose_palette_sha256 = None;
        self.viewport.reset_pose();
        let Some(rig) = &self.rig else {
            return;
        };
        let result = (|| -> Result<Vec<Vec<viewport::Vertex>>, String> {
            if rig.bones.len() > 256 {
                return Err("This rig exceeds the qualified viewport bone palette".into());
            }
            self.geometries
                .iter()
                .map(|geometry| {
                    if let Some(error) = &geometry.palette_error {
                        return Err(error.clone());
                    }
                    geometry
                        .vertices
                        .iter()
                        .map(|vertex| {
                            if !vertex.has_skin {
                                return Err("Geometry has no complete skin binding".into());
                            }
                            let mut joints = [0u16; 4];
                            for i in 0..4 {
                                if vertex.raw_weights[i] == 0 {
                                    continue;
                                }
                                let hash = *geometry
                                    .bone_palette
                                    .get(vertex.joints[i] as usize)
                                    .ok_or("Vertex joint is outside its source palette")?;
                                joints[i] =
                                    u16::try_from(rig.bone_index(hash).map_err(|e| e.to_string())?)
                                        .map_err(|e| e.to_string())?;
                            }
                            Ok(viewport::Vertex {
                                position: vertex.position,
                                normal: vertex.normal,
                                uv: vertex.uv,
                                joints,
                                weights: vertex.weights,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .collect()
        })();
        match result.and_then(|parts| self.viewport.set_skin_vertices(&parts)) {
            Ok(()) => {
                self.skin_bound = !self.geometries.is_empty();
                self.pose_status = format!("{} bones · exact GEOM hash binding", rig.bones.len());
            }
            Err(error) => {
                self.pose_status = error.clone();
                self.errors.push(error);
            }
        }
    }
    fn update_pose(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.playback_clock).as_secs_f32();
        self.playback_clock = now;
        let (Some(rig), Some(bind), Some(clip)) = (&self.rig, &self.bind, &self.clip) else {
            return;
        };
        if !self.skin_bound || self.pose_reset {
            return;
        }
        if self.playing {
            let tick_length = clip
                .codec
                .as_ref()
                .map(|c| c.tick_length)
                .filter(|t| t.is_finite() && *t > 0.)
                .unwrap_or(1. / 30.);
            self.tick += elapsed / tick_length;
            let end = clip.max_tick().unwrap_or(0) as f32;
            if self.tick > end {
                self.tick = 0.;
            }
        }
        if self.pose_last == Some(self.tick) {
            return;
        }
        let options = external_cas_animation::SampleOptions {
            ancillary_policy: external_cas_animation::AncillaryPolicy::PreserveAndIgnore,
            unsupported_channels:
                external_cas_animation::UnsupportedChannelPolicy::PreserveAndSkipNonBoneTracks,
            ..Default::default()
        };
        match clip.sample_with_bind(rig,bind,self.tick,options).map_err(|e|e.to_string()).and_then(|pose| {
            self.viewport.set_palette(&pose.palette)?;
            use sha2::Digest;
            self.pose_palette_sha256=Some(format!("{:x}",sha2::Sha256::digest(bytemuck::cast_slice(&pose.palette))));
            self.pose_status=format!("{} bone components · {} events / {} IK slots retained · {} other tracks retained",pose.edited_channels,pose.ignored_events,pose.ignored_ik_slots,pose.skipped_channels.len());
            Ok(())
        }) {
            Ok(())=>{self.pose_last=Some(self.tick);}
            Err(error)=>{self.playing=false;self.pose_last=Some(self.tick);self.pose_status=error;self.pose_palette_sha256=None;self.viewport.reset_pose();}
        }
    }
    fn scan_tray(&mut self, path: PathBuf) {
        self.tray_gen += 1;
        let generation = self.tray_gen;
        let tx = self.tx.clone();
        self.status = "Reading native households…".into();
        std::thread::spawn(move || {
            let result = external_cas_tray::scan_library(&path).map_err(|e| e.to_string());
            let _ = tx.send(Event::Tray { generation, result });
        });
    }
    fn open_project(&mut self, path: PathBuf) {
        self.project_gen += 1;
        let generation = self.project_gen;
        let tx = self.tx.clone();
        self.working_project = true;
        self.ready = false;
        std::thread::spawn(move || {
            let result = library::project_ready(external_cas_tray::reopen_project(&path));
            let _ = tx.send(Event::Project { generation, result });
        });
    }
    fn import_group(&mut self, index: usize, destination: PathBuf) {
        let Some(group) = self
            .tray
            .as_ref()
            .and_then(|t| t.groups.get(index))
            .cloned()
        else {
            return;
        };
        self.project_gen += 1;
        let generation = self.project_gen;
        let tx = self.tx.clone();
        self.working_project = true;
        std::thread::spawn(move || {
            let result =
                library::project_ready(external_cas_tray::create_project(&group, &destination));
            let _ = tx.send(Event::Project { generation, result });
        });
    }
    fn library_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your wardrobe");
        ui.label(egui::RichText::new("Installed game and creator content").weak());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add folder").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.start_scan(vec![path]);
                }
            }
            if ui.button("Open packages").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Sims packages", &["package"])
                    .pick_files()
                {
                    self.start_scan(paths);
                }
            }
        });
        if self.scanning {
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button("Cancel discovery").clicked() {
                    self.scan_cancel.store(true, Ordering::Relaxed);
                }
            });
        }
        ui.separator();
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search names, packages or resource IDs")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.query_changed = Some(Instant::now());
            self.search_cancel.store(true, Ordering::Relaxed);
            self.search_gen += 1;
            self.searching = true;
        }
        let previous = self.body;
        egui::ComboBox::from_id_salt("body_filter")
            .selected_text(
                self.body
                    .map(library::body_label)
                    .unwrap_or("All appearance slots".into()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.body, None, "All appearance slots");
                for body in [2, 5, 6, 7, 8] {
                    ui.selectable_value(&mut self.body, Some(body), library::body_label(body));
                }
            });
        if self.body != previous {
            self.query_changed = Some(Instant::now() - Duration::from_millis(200));
            self.search_gen += 1;
            self.searching = true;
        }
        if let Some(library) = self.library.clone() {
            ui.label(
                egui::RichText::new(format!(
                    "{} results · {} unsupported",
                    self.rows.len(),
                    library.unsupported.len()
                ))
                .weak(),
            );
            let selected = self.gallery.show(
                ui,
                library,
                &self.gallery_groups,
                self.selected,
                !self.scanning,
            );
            if let Some(index) = selected {
                self.select(index);
            }
        } else if !self.scanning {
            ui.add_space(24.);
            ui.label("Choose a package or content folder. Resources are decoded locally in Rust.");
        }
    }
    fn sims_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your Sims");
        ui.label("Read native Tray households into a separate project.");
        if ui.button("Browse Tray folder").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.scan_tray(path);
            }
        }
        ui.separator();
        let mut selected = None;
        if let Some(tray) = &self.tray {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, group) in tray.groups.iter().enumerate() {
                    ui.group(|ui| {
                        ui.strong(&group.metadata.name);
                        for sim in &group.metadata.sims {
                            ui.label(format!("{} {}", sim.first_name, sim.last_name));
                        }
                        for missing in &group.missing {
                            ui.colored_label(
                                egui::Color32::LIGHT_YELLOW,
                                format!("Missing {missing}"),
                            );
                        }
                        if ui
                            .add_enabled(
                                !self.working_project && group.missing.is_empty(),
                                egui::Button::new("Import a preserved copy"),
                            )
                            .clicked()
                        {
                            selected = Some(i);
                        }
                    });
                }
                for issue in &tray.issues {
                    ui.label(&issue.message);
                }
            });
        }
        if let Some(index) = selected {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Choose a new project folder")
                .save_file()
            {
                self.import_group(index, path);
            }
        }
    }
    fn project_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your project");
        if ui
            .add_enabled(!self.working_project, egui::Button::new("Reopen project"))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.open_project(path);
            }
        }
        if ui
            .add_enabled(
                !self.working_project,
                egui::Button::new("Open a recovery revision"),
            )
            .clicked()
        {
            if let Some(base) = std::env::var_os("LOCALAPPDATA") {
                let root = PathBuf::from(base).join("ExternalCAS").join("Recovery");
                if let Some(path) = rfd::FileDialog::new().set_directory(root).pick_folder() {
                    self.open_project(path);
                }
            }
        }
        if self.working_project {
            ui.spinner();
        }
        if let Some(path) = &self.recovery_location {
            ui.label("Latest recovery revision verified");
            ui.label(egui::RichText::new(path.display().to_string()).small());
        }
        if let Some(error) = self.session.error.clone() {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Editor context: {error}"));
            if ui.button("Retry editor context save").clicked() {
                self.session.error = None;
                self.mark_session_dirty();
            }
        }
        if let Some(project) = &self.project {
            if let Some(household) = project.household() {
                ui.strong(&household.family_name);
                ui.label(format!(
                    "{} Sims · native version {}",
                    household.sims.len(),
                    household.version
                ));
                for sim in &household.sims {
                    ui.label(format!("{} {}", sim.first_name, sim.last_name));
                }
                for warning in &household.warnings {
                    ui.label(warning);
                }
            }
            ui.label(format!(
                "{} original files preserved and SHA-256 verified",
                project.documents.len()
            ));
            ui.label("Native household editing and game acceptance remain under development.");
        }
        let mut change = None;
        if let Some(draft) = &mut self.draft {
            ui.separator();
            ui.strong(format!("Draft revision {}", draft.revision()));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.working_project && draft.can_undo(),
                        egui::Button::new("Undo"),
                    )
                    .clicked()
                {
                    change = Some(library::DraftChange::Undo);
                }
                if ui
                    .add_enabled(
                        !self.working_project && draft.can_redo(),
                        egui::Button::new("Redo"),
                    )
                    .clicked()
                {
                    change = Some(library::DraftChange::Redo);
                }
            });
            match draft.slots() {
                Ok(slots) => {
                    egui::ScrollArea::vertical()
                        .max_height(260.)
                        .show(ui, |ui| {
                            for (i, slot) in slots.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.slot,
                                    Some(i),
                                    format!(
                                        "Sim {} · outfit {} · {}\n{:016X}",
                                        slot.sim_index + 1,
                                        slot.outfit_index + 1,
                                        library::body_label(slot.body_type),
                                        slot.part_id
                                    ),
                                );
                            }
                        });
                    if let (Some(slot), Some(item)) = (
                        self.slot.and_then(|i| slots.get(i)),
                        self.library
                            .as_ref()
                            .and_then(|l| self.selected.and_then(|i| l.items.get(i))),
                    ) {
                        ui.label(format!("Selected wardrobe item: {}", item.name));
                        if ui
                            .add_enabled(
                                !self.working_project
                                    && !self.scanning
                                    && !self.preparing
                                    && self.scene_generation == self.preview_gen
                                    && self.scene_name.is_some()
                                    && slot.body_type == item.body_type,
                                egui::Button::new("Apply item to draft slot"),
                            )
                            .clicked()
                        {
                            if let Some(index) = self.selected {
                                change = Some(library::DraftChange::Apply {
                                    index,
                                    slot: slot.clone(),
                                });
                            }
                        }
                        ui.label(egui::RichText::new("Body slot checked. Sim eligibility, game load order and native acceptance remain unverified.").small());
                    }
                }
                Err(e) => {
                    ui.label(e.to_string());
                }
            }
            if ui
                .add_enabled(
                    !self.working_project,
                    egui::Button::new("Save as new project revision"),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Choose a new revision folder")
                    .save_file()
                {
                    let draft = draft.clone();
                    let tx = self.tx.clone();
                    self.project_gen += 1;
                    let generation = self.project_gen;
                    self.working_project = true;
                    std::thread::spawn(move || {
                        let result = library::project_ready(draft.save_as(&path));
                        let _ = tx.send(Event::Project { generation, result });
                    });
                }
            }
        }
        if let Some(change) = change {
            self.start_draft_change(change);
        }
    }
    fn poses_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Poses & animation");
        if ui.button("Choose source rig").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Sims rig", &["grannyrig"])
                .pick_file()
            {
                self.load_rig(path, None);
            }
        }
        if self.rig_loading || self.clip_loading {
            ui.spinner();
        }
        if let Some(rig) = &self.rig {
            ui.label(format!("{} bones · caller-selected rig", rig.bones.len()));
        }
        ui.label(egui::RichText::new("Original game timing and rig selection remain unverified. Events, IK and non-bone tracks are retained without execution.").small());
        ui.separator();
        if self.clip.is_some() {
            let name = self.clip.as_ref().unwrap().name().to_owned();
            let max_tick = self.clip.as_ref().unwrap().max_tick().unwrap_or(0) as f32;
            ui.strong(name);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.skin_bound,
                        egui::Button::new(if self.playing { "Pause" } else { "Play" }),
                    )
                    .clicked()
                {
                    self.playing = !self.playing;
                    self.pose_reset = false;
                    self.playback_clock = Instant::now();
                    self.mark_session_dirty();
                }
                if ui.button("Reset to bind pose").clicked() {
                    self.playing = false;
                    self.pose_reset = true;
                    self.tick = 0.;
                    self.pose_last = Some(0.);
                    self.pose_palette_sha256 = None;
                    self.pose_status = "Bind geometry restored".into();
                    self.viewport.reset_pose();
                    self.mark_session_dirty();
                }
            });
            if ui
                .add(egui::Slider::new(&mut self.tick, 0.0..=max_tick).text("Source tick"))
                .changed()
            {
                self.pose_reset = false;
                self.pose_last = None;
                self.mark_session_dirty();
            }
        }
        ui.label(&self.pose_status);
        ui.separator();
        let mut chosen = None;
        if let Some(library) = self.library.clone() {
            ui.label(format!("{} installed clips", library.poses.len()));
            egui::ScrollArea::vertical().show_rows(ui, 46., library.poses.len(), |ui, range| {
                for i in range {
                    let (provider, key, label) = &library.poses[i];
                    if ui
                        .add_sized(
                            [ui.available_width(), 42.],
                            egui::Button::new(format!("{label}\n{key}")),
                        )
                        .clicked()
                    {
                        chosen = Some(*provider);
                    }
                }
            });
            if let Some(provider) = chosen {
                self.choose_clip(library, provider, None);
            }
        }
    }
    fn capture_and_report(&mut self, ctx: &egui::Context) {
        if self.ready
            && !self.ui_assets.is_loading()
            && self.profile.finished(self.startup.profile_frames)
            && !self.searching
            && self.query_changed.is_none()
            && !self.gallery.pending
            && !self.preparing
            && !self.scanning
            && !self.working_project
            && !self.rig_loading
            && !self.clip_loading
            && !self.session.loading
            && !self.session.restore_pending
            && !self.report_written
        {
            if self.startup.exit_on_ready && self.session.worker.is_some() {
                self.save_session(true);
                if self.session.saving {
                    return;
                }
            }
            if self.startup.capture.is_some() && !self.capture_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.capture_requested = true;
                ctx.request_repaint();
                return;
            }
            let mut capture_done = self.startup.capture.is_none();
            let events = ctx.input(|i| i.events.clone());
            for event in events {
                if let egui::Event::Screenshot { image, .. } = event {
                    if let Some(path) = &self.startup.capture {
                        let bytes: Vec<u8> =
                            image.pixels.iter().flat_map(|p| p.to_array()).collect();
                        if let Err(e) = image::save_buffer(
                            path,
                            &bytes,
                            image.size[0] as u32,
                            image.size[1] as u32,
                            image::ColorType::Rgba8,
                        ) {
                            self.errors.push(e.to_string());
                        }
                        capture_done = true;
                    }
                }
            }
            if !capture_done {
                ctx.request_repaint_after(Duration::from_millis(16));
                return;
            }
            let adapter = self.viewport.adapter_info();
            let report = serde_json::json!({"application":"External CAS","native_ui":"eframe + wgpu","gpu_adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"renderer":"Rust GPU diffuse cutout bind-geometry preview","end_to_end_accepted":false,"ready_after_ms":self.started.elapsed().as_secs_f64()*1000.,"packages":self.library.as_ref().map(|l|l.catalog.packages.len()),"items":self.library.as_ref().map(|l|l.items.len()),"unsupported_items":self.library.as_ref().map(|l|l.unsupported.len()),"discovery_ms":self.library.as_ref().map(|l|l.elapsed.as_secs_f64()*1000.),"cache_hits":self.library.as_ref().map(|l|l.cache_hits),"cache_misses":self.library.as_ref().map(|l|l.cache_misses),"cache_issues":self.library.as_ref().map(|l|l.cache_issues.len()),"preview_ms":self.preview_elapsed.as_secs_f64()*1000.,"vertices":self.vertices,"triangles":self.triangles,"rendered_frames":self.viewport.rendered_frames,"status":self.status,"warnings":self.warnings,"errors":self.errors,"pose_status":self.pose_status,"pose_tick":self.tick,"pose_palette_sha256":self.pose_palette_sha256,"rig_bones":self.rig.as_ref().map(|r|r.bones.len()),"project_native_files":self.project.as_ref().map(|p|p.documents.len()),"draft_slots":self.draft.as_ref().and_then(|d|d.slots().ok()).map(|s|s.len()),"draft_revision":self.draft.as_ref().map(|d|d.revision()),"recovery_revision_loaded":self.project.as_ref().zip(self.session.state.recovery_project_path.as_ref()).is_some_and(|(p,path)|same_provider_path(&p.directory,path)),"session_enabled":self.session.worker.is_some(),"session_restored":self.session.restored,"session_saved":self.session.snapshot.is_some(),"session_error":self.session.error,"camera":{"yaw":self.viewport.camera.yaw,"pitch":self.viewport.camera.pitch,"distance":self.viewport.camera.distance,"target":self.viewport.camera.target},"ui_tab":self.tab.name()});
            let mut report = report;
            report["profile"] = self.profile.report();
            report["query"] = serde_json::json!(self.query);
            report["search_ms"] = serde_json::json!(self.search_elapsed.as_secs_f64() * 1000.);
            report["search_indexed"] = serde_json::json!(self.search_cache_used);
            report["initial_ready_ms"] = serde_json::json!(self.initial_ready_ms);
            report["query_to_results_ui_submission_ms"] =
                serde_json::json!(self.search_first_frame_ms);
            report["query_to_visible_thumbnails_ready_ms"] =
                serde_json::json!(self.search_thumbnails_ready_ms);
            report["catalog_designs"] = serde_json::json!(self.gallery_groups.len());
            report["thumbnail_loads"] = serde_json::json!(self.gallery.loaded);
            report["thumbnail_unavailable"] = serde_json::json!(self.gallery.unavailable);
            report["thumbnail_last_batch_ms"] = serde_json::json!(self.gallery.batch_ms);
            report["installed_ui_status"] = serde_json::json!(self.ui_assets.status);
            report["installed_ui_issues"] = serde_json::json!(self.ui_assets.issues);
            report["installed_ui_source_pins"] = serde_json::json!(self.ui_assets.pins);
            if let Some(path) = &self.startup.report {
                let result = (|| -> std::io::Result<()> {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)?;
                    file.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
                    file.sync_all()
                })();
                if let Err(e) = result {
                    self.errors
                        .push(format!("Could not write diagnostic report: {e}"));
                }
            }
            self.report_written = true;
            if self.startup.exit_on_ready {
                self.diagnostic_failed.store(
                    !self.errors.is_empty()
                        || ((self.startup.ui_assets.is_some() || self.ui_restore_required)
                            && self.ui_assets.provider.is_none())
                        || self.session.error.is_some()
                        || (self.startup.selection_requested && self.vertices == 0)
                        || (self.startup.pose_requested
                            && !self.pose_reset
                            && self.pose_palette_sha256.is_none()),
                    Ordering::Relaxed,
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame_started = Instant::now();
        if self.profile.running(self.startup.profile_frames) && self.startup.profile_scroll {
            self.gallery.profile_scroll(self.profile.frames());
        }
        let ui_was_loading = self.ui_assets.is_loading();
        self.ui_assets.poll(ui.ctx());
        if ui_was_loading && !self.ui_assets.is_loading() && self.ui_assets.provider.is_some() {
            self.mark_session_dirty();
        }
        self.drain_session(ui.ctx());
        self.drain();
        self.update_pose();
        self.finish_restore();
        self.startup_edit();
        if self
            .query_changed
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(150))
        {
            self.start_search();
        }
        cas_ui::show(self, ui);
        if !self.searching {
            if let Some(requested) = self.search_requested_at {
                self.search_first_frame_ms
                    .get_or_insert_with(|| requested.elapsed().as_secs_f64() * 1000.);
                if !self.gallery.pending {
                    self.search_thumbnails_ready_ms =
                        Some(requested.elapsed().as_secs_f64() * 1000.);
                    self.search_requested_at = None;
                }
            }
        }
        let can_profile = self.ready
            && !self.scanning
            && !self.preparing
            && !self.searching
            && !self.working_project
            && !self.session.restore_pending
            && !self.ui_assets.loading;
        if can_profile
            && !self.gallery.pending
            && !self.rig_loading
            && !self.clip_loading
            && !self.session.loading
        {
            self.initial_ready_ms
                .get_or_insert_with(|| self.started.elapsed().as_secs_f64() * 1000.);
        }
        self.profile.frame(
            self.startup.profile_frames,
            can_profile && !self.gallery.pending,
            frame_started,
        );
        if self.profile.running(self.startup.profile_frames) {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        if self.closing {
            egui::Window::new("Finishing recovery")
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.label("Finishing verified recovery and local editor context...");
                    if let Some(error) = self.session.error.clone() {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                        ui.horizontal(|ui| {
                            if ui.button("Retry context save").clicked() {
                                self.session.error = None;
                                self.mark_session_dirty();
                            }
                            if ui.button("Close keeping prior session").clicked() {
                                self.allow_close = true;
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    }
                });
        }
        let ctx = ui.ctx();
        self.save_session(false);
        self.capture_and_report(ctx);
        self.handle_close(ctx);
        if self.scanning
            || self.preparing
            || self.searching
            || self.working_project
            || self.rig_loading
            || self.clip_loading
            || self.query_changed.is_some()
            || self.session.loading
            || self.session.saving
            || self.session.dirty_at.is_some()
            || self.closing
        {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        if self.playing {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}
impl Drop for Studio {
    fn drop(&mut self) {
        self.scan_cancel.store(true, Ordering::Relaxed);
        self.search_cancel.store(true, Ordering::Relaxed);
        self.preview_cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod persistence_path_tests {
    use super::same_provider_path;
    use std::path::Path;
    #[test]
    #[cfg(windows)]
    fn canonical_windows_and_user_path_spellings_restore_the_same_provider() {
        assert!(same_provider_path(
            Path::new(r"C:\private\source.package"),
            Path::new(r"\\?\C:\private\source.package")
        ));
        assert!(same_provider_path(
            Path::new(r"\\server\share\source.package"),
            Path::new(r"\\?\UNC\server\share\source.package")
        ));
    }
    #[test]
    fn duplicate_basenames_in_distinct_directories_remain_distinct_providers() {
        let root = std::env::current_dir().unwrap();
        assert!(!same_provider_path(
            &root.join("first/source.package"),
            &root.join("second/source.package")
        ));
    }
}
