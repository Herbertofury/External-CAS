//! Original-movie integration. The interpreter, source intake and expensive
//! first-load work live on an actor; the app presents its shared GPU texture.
use eframe::{egui, egui_wgpu::RenderState};
use external_cas_source_ui::*;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct SnapshotHost {
    editing: Arc<AtomicBool>,
    strings: Arc<external_cas_fidelity::localization::StringBank>,
}
impl UiHost for SnapshotHost {
    fn call(&self, request: &HostRequest) -> HostReply {
        if request.bridge == HostBridge::Other("ExternalCAS.GetLocalizedString".into()) {
            let result = (|| -> Result<String, String> {
                let Some(HostValue::Object(input)) = request.args.first() else {
                    return Err("Original LocKey argument was not projected".into());
                };
                let Some(HostValue::Number(hash)) = input.get("hash") else {
                    return Err("Original LocKey hash absent".into());
                };
                if !hash.is_finite() || *hash < 0. || *hash > u32::MAX as f64 || hash.fract() != 0.
                {
                    return Err("Original LocKey hash is not u32".into());
                }
                let tokens = match input.get("tokens") {
                    Some(HostValue::Null) | Some(HostValue::Undefined) | None => 0,
                    Some(HostValue::List(values)) => values.len(),
                    _ => return Err("Original localization tokens unsupported".into()),
                };
                let flags = match input.get("flags") {
                    Some(HostValue::Number(v))
                        if v.is_finite()
                            && *v >= 0.
                            && *v <= u32::MAX as f64
                            && v.fract() == 0. =>
                    {
                        *v as u32
                    }
                    _ => return Err("Original localization flags invalid".into()),
                };
                Ok(self
                    .strings
                    .resolve_literal(*hash as u32, tokens, flags)?
                    .to_owned())
            })();
            return match result {
                Ok(text) => HostReply::Value(HostValue::String(text)),
                Err(reason) => HostReply::Unavailable(reason),
            };
        }
        // This editor's CAS/full-edit context is independent of household load.
        if request.bridge == HostBridge::CallGameService {
            match request.name.as_str() {
                "GetGameState" => return HostReply::Value(HostValue::Number(983016380.)),
                "CasGetCASEditMode" => return HostReply::Value(HostValue::Number(0.)),
                _ => {}
            }
        }
        HostReply::Unavailable(if self.editing.load(Ordering::Relaxed) {
            "This original service has no qualified native editor binding".into()
        } else {
            "Native household data has not loaded; no service result is fabricated".into()
        })
    }
}
enum Command {
    Frame {
        size: [u32; 2],
        dpi: f64,
        input: Vec<UiInput>,
    },
}
enum Message {
    Progress(&'static str),
    Frame {
        frame: UiFrame,
        status: RuntimeStatus,
        events: Vec<RuntimeEvent>,
        ticks: u64,
        pins: Vec<external_cas_fidelity::installed::SourcePin>,
        lifecycle_errors: Vec<String>,
    },
    Failed(String),
}
pub struct State {
    render: RenderState,
    command: mpsc::SyncSender<Command>,
    messages: mpsc::Receiver<Message>,
    cancel: Arc<AtomicBool>,
    snapshot: Arc<AtomicBool>,
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
        let (send, receive) = mpsc::sync_channel(1);
        let (results, messages) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let snapshot = Arc::new(AtomicBool::new(false));
        let host = snapshot.clone();
        let gpu = SharedGpu {
            instance: render.instance.clone(),
            adapter: render.adapter.clone(),
            device: render.device.clone(),
            queue: render.queue.clone(),
            target_format: wgpu::TextureFormat::Rgba8Unorm,
        };
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let progress = |s| {
                    let _ = results.send(Message::Progress(s));
                    ctx.request_repaint();
                };
                progress("Reading original movie, font and atlas dependencies…");
                let index = external_cas_assets::PackageIndex::open(&provider)
                    .map_err(|e| e.to_string())?;
                let aliases = locale_aliases(&locale)?;
                let bundle = prepare_movie_bundle_with_aliases(
                    &index,
                    &root,
                    root == "UIContainer",
                    &aliases,
                    None,
                    &stopped,
                )?;
                if stopped.load(Ordering::Relaxed) {
                    return Ok(());
                }
                progress("Reading the selected locale string tables…");
                let strings_index = external_cas_assets::PackageIndex::open(&string_provider)
                    .map_err(|e| e.to_string())?;
                let strings = Arc::new(external_cas_fidelity::localization::StringBank::load(
                    &strings_index,
                    &stopped,
                )?);
                let string_pins = strings.source_pins();
                progress("Starting the original movie interpreter…");
                let mut runtime = SourceUiRuntime::new(
                    gpu,
                    bundle,
                    Arc::new(SnapshotHost {
                        editing: host,
                        strings,
                    }),
                    [1440, 900],
                    1.,
                )?;
                progress("Preloading original definitions and native functions…");
                let mut loaded = false;
                for _ in 0..10000 {
                    if stopped.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    if runtime.preload() {
                        loaded = true;
                        break;
                    }
                }
                if !loaded {
                    return Err("Original source preload exceeded its operation budget".into());
                }
                runtime.bind_native_function_projected(
                    "olympus.localization.LocKey",
                    "sGetLocalizedStringFunc",
                    "ExternalCAS.GetLocalizedString",
                    &["hash", "tokens", "flags"],
                )?;
                let mut lifecycle_errors = Vec::new();
                // Standalone-widget inspection uses the original manager's
                // stage settings. Full UIContainer owns its manager lifecycle.
                if root == "CASNavigation" {
                    if let Err(error) =
                        runtime.construct_source_class("olympus.io.CommunicationManager", vec![])
                    {
                        lifecycle_errors.push(format!("Original CommunicationManager: {error}"));
                    }
                    let mut root_object = None;
                    for _ in 0..10 {
                        if let Ok(HostValue::Avm2Handle(id)) = runtime.retain_source_root() {
                            root_object = Some(id);
                            break;
                        }
                        runtime.tick(Duration::from_secs_f64(1. / 24.));
                    }
                    if let Some(root_object) = root_object {
                        let stage = (|| -> Result<(), String> {
                            let HostValue::Avm2Handle(stage) =
                                runtime.get_source_property(root_object, "stage")?
                            else {
                                return Err("Original stage is not ready".into());
                            };
                            runtime.set_source_property(
                                stage,
                                "scaleMode",
                                HostValue::String("noScale".into()),
                            )?;
                            runtime.set_source_property(
                                stage,
                                "align",
                                HostValue::String("TL".into()),
                            )?;
                            Ok(())
                        })();
                        if let Err(error) = stage {
                            lifecycle_errors.push(format!("Original stage setup: {error}"));
                        }
                    } else {
                        lifecycle_errors.push("Original root script was not ready".into());
                    }
                }
                let mut size = [1440, 900];
                let mut dpi = 1.;
                let mut clock = Instant::now();
                let mut ticks = 0;
                while !stopped.load(Ordering::Relaxed) {
                    let command = match receive.recv_timeout(Duration::from_millis(50)) {
                        Ok(c) => c,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let Command::Frame {
                        size: next,
                        dpi: next_dpi,
                        input,
                    } = command;
                    if next != size || next_dpi != dpi {
                        runtime.resize(next, next_dpi)?;
                        size = next;
                        dpi = next_dpi;
                    }
                    for event in input {
                        runtime.input(event);
                    }
                    let now = Instant::now();
                    runtime.tick(now.duration_since(clock).min(Duration::from_millis(250)));
                    clock = now;
                    ticks += 1;
                    let frame = runtime.render()?;
                    let mut pins = runtime.source_pins.clone();
                    pins.extend(string_pins.iter().cloned());
                    let message = Message::Frame {
                        frame,
                        status: runtime.status(),
                        events: runtime.drain_events(),
                        ticks,
                        pins,
                        lifecycle_errors: lifecycle_errors.clone(),
                    };
                    if results.send(message).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = results.send(Message::Failed(error));
                ctx.request_repaint();
            }
        });
        Self {
            render,
            command: send,
            messages,
            cancel,
            snapshot,
            pending_input: vec![],
            texture: None,
            frame: None,
            in_flight: false,
            loading: true,
            status: "Preparing the original CAS interface…".into(),
            error: None,
            report: serde_json::json!({}),
            ticks: 0,
        }
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"status":self.status,"error":self.error,"actor_thread":true,"ticks":self.ticks,
            "runtime":self.report,"full_ui_qualified":false})
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
        while let Ok(message) = self.messages.try_recv() {
            match message {
                Message::Progress(status) => self.status = status.into(),
                Message::Failed(error) => {
                    self.error = Some(error);
                    self.loading = false;
                    self.in_flight = false;
                }
                Message::Frame {
                    frame,
                    status,
                    events,
                    ticks,
                    pins,
                    lifecycle_errors,
                } => {
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
                    let errors = events
                        .iter()
                        .filter_map(|e| match e {
                            RuntimeEvent::VmError(s) => Some(s.clone()),
                            _ => None,
                        })
                        .take(32)
                        .collect::<Vec<_>>();
                    let previous = self.report["recent_vm_errors"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let mut retained = previous;
                    retained.extend(errors.into_iter().map(serde_json::Value::String));
                    if retained.len() > 32 {
                        retained.drain(..retained.len() - 32);
                    }
                    self.report = serde_json::json!({"vm_errors":status.vm_errors,"unbound_services":status.unbound_services,
                    "events_dropped":status.events_dropped,"recent_vm_errors":retained,"source_pins":pins,
                    "complete_gfx_support":status.complete_gfx_support,"lifecycle_errors":lifecycle_errors,
                    "widget_initialize_completed":false,"full_source_manager_lifecycle_completed":false});
                    self.status = format!(
                        "Original movie running · {} VM errors · {} unbound services",
                        status.vm_errors,
                        status.unbound_services.len()
                    );
                }
            }
        }
    }
    pub fn surface(
        &mut self,
        ui: &mut egui::Ui,
        viewport: &mut crate::viewport::Viewport,
        editing: bool,
    ) {
        self.snapshot.store(editing, Ordering::Relaxed);
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
                viewport.render(size);
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
                for event in ui.input(|i| i.events.clone()) {
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
                    self.cancel.store(true, Ordering::Relaxed);
                }
                if !self.in_flight && self.error.is_none() {
                    let command = Command::Frame {
                        size,
                        dpi: scale as f64,
                        input: std::mem::take(&mut self.pending_input),
                    };
                    match self.command.try_send(command) {
                        Ok(()) => self.in_flight = true,
                        Err(mpsc::TrySendError::Full(Command::Frame { input, .. })) => {
                            self.pending_input = input
                        }
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
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(id) = self.texture {
            self.render.renderer.write().free_texture(&id);
        }
    }
}
