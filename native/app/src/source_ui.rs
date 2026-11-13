//! Original source movies on a native worker. Only observed draft data and
//! supported actions cross the versioned editor boundary.
use eframe::{egui, egui_wgpu::RenderState};
use external_cas_source_ui::{actor::*, *};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};

#[derive(Clone, PartialEq, Eq)]
struct EditorKey {
    files: Vec<String>,
    revision: Option<u64>,
    slot: Option<usize>,
    busy: bool,
}
pub struct State {
    render: RenderState,
    actor: Option<Actor>,
    editor_key: Option<EditorKey>,
    editor_snapshot: EditorSnapshot,
    actions: Vec<ActionRequest>,
    pending_updates: VecDeque<(String, HostValue)>,
    source_ready: bool,
    pending_input: Vec<UiInput>,
    texture: Option<egui::TextureId>,
    frame: Option<UiFrame>,
    in_flight: bool,
    pub loading: bool,
    pub status: String,
    pub error: Option<String>,
    report: serde_json::Value,
    ticks: u64,
}
impl State {
    pub fn start(
        render: RenderState,
        ctx: egui::Context,
        provider: PathBuf,
        root: String,
        locale: String,
        string_provider: PathBuf,
    ) -> Self {
        let gpu = SharedGpu {
            instance: render.instance.clone(),
            adapter: render.adapter.clone(),
            device: render.device.clone(),
            queue: render.queue.clone(),
            target_format: wgpu::TextureFormat::Rgba8Unorm,
        };
        let config = HostConfig {
            provider,
            root,
            locale,
            string_providers: vec![string_provider],
            expected_ui_pins: None,
            trope_global_provider: None,
            trope_item_providers: vec![],
            menu_provider: None,
            static_images: None,
            static_replies: BTreeMap::from([
                ("GetGameState".into(), HostValue::Number(983016380.)),
                ("CasGetCASEditMode".into(), HostValue::Number(0.)),
            ]),
            required_before_first_frame: ["CASGetSimInfo".into()].into_iter().collect(),
            configure_runtime: None,
        };
        let initial = EditorSnapshot::default();
        let result = Actor::spawn(
            gpu,
            config,
            initial.clone(),
            Arc::new(move || ctx.request_repaint()),
        );
        let (actor, error) = match result {
            Ok(actor) => (Some(actor), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            render,
            actor,
            error,
            editor_key: None,
            editor_snapshot: initial,
            actions: vec![],
            pending_updates: VecDeque::new(),
            source_ready: false,
            pending_input: vec![],
            texture: None,
            frame: None,
            in_flight: false,
            loading: true,
            status: "Preparing the original CAS interface…".into(),
            report: serde_json::json!({}),
            ticks: 0,
        }
    }
    pub fn update_editor(
        &mut self,
        draft: Option<&external_cas_tray::ProjectDraft>,
        slot: Option<usize>,
        busy: bool,
    ) {
        let key = EditorKey {
            files: draft
                .map(|d| d.documents().iter().map(|f| f.sha256.clone()).collect())
                .unwrap_or_default(),
            revision: draft.map(|d| d.revision()),
            slot,
            busy,
        };
        if self.editor_key.as_ref() == Some(&key) {
            return;
        }
        let result = (|| -> Result<EditorSnapshot, String> {
            let mut snapshot = EditorSnapshot {
                revision: self
                    .editor_snapshot
                    .revision
                    .checked_add(1)
                    .ok_or("Editor UI revision exhausted")?,
                ..Default::default()
            };
            if let Some(draft) = draft {
                let household = draft
                    .documents()
                    .iter()
                    .find_map(|d| match &d.summary {
                        external_cas_tray::DocumentSummary::Household(h) => Some(h),
                        _ => None,
                    })
                    .ok_or("Verified draft household is absent")?;
                let slots = draft.slots().map_err(|e| e.to_string())?;
                let index = slot
                    .and_then(|s| slots.get(s))
                    .map(|s| s.sim_index)
                    .unwrap_or(0);
                let metadata = household
                    .sims
                    .get(index)
                    .ok_or("Selected native Sim metadata is absent")?;
                let sim_id = metadata
                    .sim_id
                    .ok_or("Selected native Sim identity is absent")?;
                let native = draft
                    .appearance()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|s| s.sim_id == sim_id)
                    .ok_or("Draft Sim identity and appearance disagree")?;
                let mut observed_fields = BTreeMap::new();
                if let Some(id) = metadata.household_id {
                    observed_fields.insert("householdId".into(), HostValue::String(id.to_string()));
                }
                snapshot.sim = Some(ObservedSimInfo {
                    sim_id,
                    first_name: metadata.first_name.clone(),
                    last_name: metadata.last_name.clone(),
                    age: native.age,
                    gender: native.gender,
                    species_type: native.extended_species,
                    observed_fields,
                });
                snapshot.ready = !busy;
                if !busy && draft.can_undo() {
                    snapshot.action_services.insert("CasNavigationUndo".into());
                }
                if !busy && draft.can_redo() {
                    snapshot.action_services.insert("CasNavigationRedo".into());
                }
            }
            self.actor
                .as_ref()
                .ok_or("Original UI actor is unavailable")?
                .publish(snapshot.clone())?;
            Ok(snapshot)
        })();
        match result {
            Ok(snapshot) => {
                self.pending_input.clear();
                self.frame = None;
                if let Some(id) = self.texture.take() {
                    self.render.renderer.write().free_texture(&id);
                }
                self.pending_updates.clear();
                if let Some(sim) = &snapshot.sim {
                    match sim.packet() {
                        Ok(packet) => self
                            .pending_updates
                            .push_back(("CASSetSimInfo".into(), packet)),
                        Err(error) => {
                            self.error = Some(error);
                            return;
                        }
                    }
                }
                for (message, service) in [
                    ("CasUndoEnabled", "CasNavigationUndo"),
                    ("CasRedoEnabled", "CasNavigationRedo"),
                ] {
                    self.pending_updates.push_back((
                        message.into(),
                        HostValue::Bool(
                            snapshot.ready && snapshot.action_services.contains(service),
                        ),
                    ));
                }
                self.editor_snapshot = snapshot;
                self.editor_key = Some(key);
            }
            Err(error) => {
                self.error = Some(format!(
                    "Original UI editor data failed validation: {error}"
                ))
            }
        }
    }
    pub fn take_actions(&mut self) -> Vec<ActionRequest> {
        std::mem::take(&mut self.actions)
    }
    pub fn accepts(&self, request: &ActionRequest) -> bool {
        self.editor_snapshot.ready
            && request.snapshot_revision == self.editor_snapshot.revision
            && self
                .editor_snapshot
                .action_services
                .contains(&request.original.name)
    }
    fn send_pending_update(&mut self) {
        if !self.source_ready || self.in_flight {
            return;
        }
        let Some((name, payload)) = self.pending_updates.pop_front() else {
            return;
        };
        let command = Command::SendMessage { name, payload };
        match self.actor.as_ref().map(|a| a.try_send(command)) {
            Some(Ok(())) => {}
            Some(Err(mpsc::TrySendError::Full(Command::SendMessage { name, payload }))) => {
                self.pending_updates.push_front((name, payload))
            }
            Some(Err(mpsc::TrySendError::Disconnected(_))) => {
                self.error = Some("Original interface actor stopped".into())
            }
            Some(Err(mpsc::TrySendError::Full(_))) => unreachable!(),
            None => {}
        }
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"status":self.status,"error":self.error,"actor_thread":true,"ticks":self.ticks,"runtime":self.report,"snapshot_revision":self.editor_snapshot.revision,"observed_sim_present":self.editor_snapshot.sim.is_some(),"supported_actions":self.editor_snapshot.action_services,"full_ui_qualified":false})
    }
    pub fn capture_ready(&self) -> bool {
        self.error.is_some() || (!self.loading && self.ticks >= 30)
    }
    pub fn failed(&self) -> bool {
        self.error.is_some()
            || self.report["vm_errors"].as_u64().unwrap_or(0) > 0
            || self.report["unbound_services"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
            || self.report["lifecycle_errors"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
    }
    fn poll(&mut self) {
        while let Some(Ok(message)) = self.actor.as_ref().map(|a| a.try_recv()) {
            match message {
                ActorEvent::Progress(status) => self.status = status,
                ActorEvent::Failed(error) => {
                    self.error = Some(error);
                    self.loading = false;
                    self.in_flight = false;
                }
                ActorEvent::WaitingForEditor { revision, missing } => {
                    self.status = format!(
                        "Original interface waiting for verified editor data: {}",
                        missing.join(", ")
                    );
                    self.report["waiting_snapshot_revision"] = serde_json::json!(revision);
                    self.in_flight = false;
                    self.frame = None;
                    if let Some(id) = self.texture.take() {
                        self.render.renderer.write().free_texture(&id);
                    }
                }
                ActorEvent::Action(request) => {
                    if self.actions.len() >= 256 {
                        self.error =
                            Some("Original interface action backlog exceeded its budget".into());
                    } else {
                        self.actions.push(request);
                    }
                }
                ActorEvent::InboundResult { operation, result } => {
                    if let Err(error) = result {
                        let mut errors = self.report["lifecycle_errors"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                        if errors.len() < 32 {
                            errors.push(serde_json::json!(format!("{operation}: {error}")));
                        }
                        self.report["lifecycle_errors"] = serde_json::json!(errors);
                    }
                }
                ActorEvent::Frame {
                    frame,
                    status,
                    events,
                    ticks,
                    pins,
                    bootstrap,
                    snapshot_revision,
                } => {
                    self.source_ready = bootstrap.source_root_ready;
                    if snapshot_revision != self.editor_snapshot.revision {
                        self.in_flight = false;
                        continue;
                    }
                    let mut renderer = self.render.renderer.write();
                    if let Some(id) = self.texture {
                        renderer.update_egui_texture_from_wgpu_texture(
                            &self.render.device,
                            &frame.view,
                            wgpu::FilterMode::Linear,
                            id,
                        );
                    } else {
                        self.texture = Some(renderer.register_native_texture(
                            &self.render.device,
                            &frame.view,
                            wgpu::FilterMode::Linear,
                        ));
                    }
                    self.frame = Some(frame);
                    self.loading = false;
                    self.in_flight = false;
                    self.ticks = ticks;
                    self.source_ready = bootstrap.source_root_ready;
                    let mut errors = self.report["recent_vm_errors"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    errors.extend(events.iter().filter_map(|e| match e {
                        RuntimeEvent::VmError(s) => Some(serde_json::json!(s)),
                        _ => None,
                    }));
                    if errors.len() > 32 {
                        errors.drain(..errors.len() - 32);
                    }
                    let mut lifecycle = self.report["lifecycle_errors"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    for error in &bootstrap.errors {
                        if lifecycle.len() < 32
                            && !lifecycle.iter().any(|e| e.as_str() == Some(error))
                        {
                            lifecycle.push(serde_json::json!(error));
                        }
                    }
                    self.report = serde_json::json!({"vm_errors":status.vm_errors,"unbound_services":status.unbound_services,"events_dropped":status.events_dropped,"recent_vm_errors":errors,"source_pins":pins,"complete_gfx_support":status.complete_gfx_support,"lifecycle_errors":lifecycle,"widget_initialize_completed":bootstrap.widget_initialized,"source_root_ready":bootstrap.source_root_ready,"full_source_manager_lifecycle_completed":false,"frame_snapshot_revision":snapshot_revision});
                    self.status = format!(
                        "Original movie running · {} VM errors · {} unbound services",
                        status.vm_errors,
                        status.unbound_services.len()
                    );
                }
            }
        }
    }
    pub fn surface(&mut self, ui: &mut egui::Ui, viewport: &mut crate::viewport::Viewport) {
        self.poll();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let scale = ui.ctx().pixels_per_point();
                let size = [
                    (rect.width() * scale).max(1.) as u32,
                    (rect.height() * scale).max(1.) as u32,
                ];
                viewport.framing_offset = 0.;
                if let Err(error) = viewport.render(size) {
                    self.error = Some(format!("Original interface viewport failed: {error}"));
                }
                ui.painter().image(
                    viewport.texture_id,
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
                if let Some(id) = self.texture {
                    ui.painter().image(
                        id,
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        egui::Color32::WHITE,
                    );
                }
                for event in ui.input(|i| {
                    if self.editor_snapshot.ready && self.texture.is_some() {
                        i.events.clone()
                    } else {
                        vec![]
                    }
                }) {
                    let point = |p: egui::Pos2| {
                        (
                            ((p.x - rect.left()) * scale) as f64,
                            ((p.y - rect.top()) * scale) as f64,
                        )
                    };
                    match event {
                        egui::Event::PointerMoved(p) => {
                            let (x, y) = point(p);
                            self.pending_input.push(UiInput::MouseMove { x, y });
                        }
                        egui::Event::PointerGone => self.pending_input.push(UiInput::MouseLeave),
                        egui::Event::MouseWheel { unit, delta, .. } => {
                            self.pending_input.push(UiInput::MouseWheel {
                                delta: match unit {
                                    egui::MouseWheelUnit::Point => {
                                        MouseWheelDelta::Pixels(delta.y as f64 * scale as f64)
                                    }
                                    egui::MouseWheelUnit::Line => {
                                        MouseWheelDelta::Lines(delta.y as f64)
                                    }
                                    egui::MouseWheelUnit::Page => {
                                        MouseWheelDelta::Pixels(delta.y as f64 * size[1] as f64)
                                    }
                                },
                            })
                        }
                        egui::Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            ..
                        } => {
                            let button = match button {
                                egui::PointerButton::Primary => MouseButton::Left,
                                egui::PointerButton::Secondary => MouseButton::Right,
                                egui::PointerButton::Middle => MouseButton::Middle,
                                _ => continue,
                            };
                            let (x, y) = point(pos);
                            self.pending_input.push(if pressed {
                                UiInput::MouseDown {
                                    x,
                                    y,
                                    button,
                                    index: None,
                                }
                            } else {
                                UiInput::MouseUp { x, y, button }
                            });
                        }
                        egui::Event::Text(s) => self
                            .pending_input
                            .extend(s.chars().map(|codepoint| UiInput::TextInput { codepoint })),
                        _ => {}
                    }
                }
                if self.pending_input.len() > 4096 {
                    self.error =
                        Some("Original interface input backlog exceeded its budget".into());
                    self.actor = None;
                }
                self.send_pending_update();
                if !self.in_flight
                    && self.error.is_none()
                    && self.actor.is_some()
                    && (!self.source_ready || self.pending_updates.is_empty())
                {
                    let command = Command::Frame(FrameRequest {
                        size,
                        dpi: scale as f64,
                        input: std::mem::take(&mut self.pending_input),
                    });
                    match self.actor.as_ref().expect("active actor").try_send(command) {
                        Ok(()) => self.in_flight = true,
                        Err(mpsc::TrySendError::Full(Command::Frame(frame))) => {
                            self.pending_input = frame.input;
                        }
                        Err(mpsc::TrySendError::Full(_)) => unreachable!(),
                        Err(mpsc::TrySendError::Disconnected(_)) => {
                            self.error = Some("Original interface actor stopped".into());
                            self.loading = false;
                        }
                    }
                }
            });
        egui::Window::new("Original UI compatibility")
            .default_pos([12., 75.])
            .show(ui.ctx(), |ui| {
                ui.label(&self.status);
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                ui.label(
                    "Source control bindings and complete interface qualification are in progress.",
                );
            });
        if self.error.is_none() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(id) = self.texture {
            self.render.renderer.write().free_texture(&id);
        }
    }
}
