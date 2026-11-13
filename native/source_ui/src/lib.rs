// SPDX-License-Identifier: GPL-3.0-or-later
//! Experimental original-source UI interpreter host. No game or network access.
//! The original Scaleform/native-service capability gates remain explicit.
pub mod actor;
pub mod bootstrap;
mod gpu;
pub mod menus;
mod navigator;
mod source;
mod source_assets;
mod source_fonts;
pub mod static_data;
pub mod static_images;
pub use gpu::{SharedGpu, UiFrame};
pub use ruffle_core::events::{
    KeyCode, KeyDescriptor, MouseButton, MouseWheelDelta, PlayerEvent as UiInput,
};
pub use ruffle_core::external::Value as HostValue;
use ruffle_core::{
    FloatDuration, Player, PlayerBuilder,
    backend::{log::LogBackend, navigator::NullExecutor},
    context::UpdateContext,
    external::{ExternalInterfaceProvider, FsCommandProvider},
    limits::ExecutionLimit,
    tag_utils::SwfMovieData,
};
use ruffle_render::backend::ViewportDimensions;
pub use ruffle_render_wgpu::wgpu;
pub use source::{
    PreparedResource, PreparedUiBundle, locale_aliases, prepare_cas_bundle, prepare_movie_bundle,
    prepare_movie_bundle_with_aliases,
};
pub use static_images::{ResolvedStaticImage, StaticImageRequest, StaticImageResolver};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostBridge {
    CallGameService,
    SendGameMessage,
    ObjectSendMessage,
    Other(String),
}
#[derive(Clone, Debug)]
pub struct HostRequest {
    /// Host audit/correlation identity, not an invented original game ABI field.
    pub request_id: u64,
    pub bridge: HostBridge,
    pub name: String,
    /// Original bridge arguments, including the original asynchronous flag.
    pub args: Vec<HostValue>,
}
impl HostRequest {
    pub fn payload(&self) -> Option<&HostValue> {
        match self.bridge {
            HostBridge::CallGameService | HostBridge::SendGameMessage => self.args.get(1),
            HostBridge::ObjectSendMessage => self.args.get(2),
            HostBridge::Other(_) => None,
        }
    }
    pub fn asynchronous(&self) -> Option<bool> {
        if self.bridge == HostBridge::CallGameService {
            match self.args.get(2) {
                Some(HostValue::Bool(v)) => Some(*v),
                _ => None,
            }
        } else {
            None
        }
    }
}
#[derive(Clone, Debug)]
pub enum HostReply {
    Value(HostValue),
    Unavailable(String),
}
/// Called synchronously by the original VM. The owner supplies a verified
/// editor snapshot/service implementation. Never block waiting on this VM's UI
/// thread. Asynchronous work must publish through original inbound messages.
pub trait UiHost: 'static {
    fn call(&self, request: &HostRequest) -> HostReply;
}
#[derive(Clone, Debug)]
pub struct RuntimeLimits {
    pub script_timeout: Duration,
    pub preload_ops: usize,
    pub preload_slice: Duration,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            script_timeout: Duration::from_secs(2),
            preload_ops: 10_000,
            preload_slice: Duration::from_millis(4),
        }
    }
}
#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    HostCall {
        request: HostRequest,
        reply: HostReply,
    },
    CallbackAvailable(String),
    Trace(String),
    Warning(String),
    Denied(String),
    VmError(String),
    EventsDropped(u64),
    ImageResolved {
        request: StaticImageRequest,
        pins: Vec<external_cas_fidelity::installed::SourcePin>,
        encoded_png_sha256: String,
    },
}
#[derive(Clone)]
pub(crate) struct Audit(Arc<Mutex<AuditState>>);
#[derive(Default)]
struct AuditState {
    events: VecDeque<RuntimeEvent>,
    vm_errors: u64,
    unavailable: BTreeSet<String>,
    dropped: u64,
    initial_pins: Vec<external_cas_fidelity::installed::SourcePin>,
    image_pins: BTreeMap<
        (std::path::PathBuf, external_cas_assets::ResourceKey, u32),
        external_cas_fidelity::installed::SourcePin,
    >,
}
#[derive(Clone, Debug)]
pub struct RuntimeStatus {
    pub vm_errors: u64,
    pub unbound_services: Vec<String>,
    pub events_dropped: u64,
    pub complete_gfx_support: bool,
}
impl Audit {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(AuditState::default())))
    }
    pub(crate) fn accept_image_pins(
        &self,
        pins: &[external_cas_fidelity::installed::SourcePin],
    ) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "Image provenance lock failed")?;
        let provider_key = |path: &std::path::Path| {
            path.to_string_lossy()
                .trim_start_matches("\\\\?\\")
                .replace('/', "\\")
                .to_ascii_lowercase()
        };
        for pin in pins {
            for accepted in state.initial_pins.iter().chain(state.image_pins.values()) {
                if provider_key(&accepted.provider_path) == provider_key(&pin.provider_path)
                    && accepted.index_sha256 != pin.index_sha256
                {
                    return Err(
                        "Original image provider index changed since source preparation".into(),
                    );
                }
            }
            let key = (pin.provider_path.clone(), pin.key, pin.ordinal);
            if let Some(previous) = state.image_pins.get(&key) {
                if previous != pin {
                    return Err("Original static image source changed within this runtime".into());
                }
            }
        }
        for pin in pins {
            state.image_pins.insert(
                (pin.provider_path.clone(), pin.key, pin.ordinal),
                pin.clone(),
            );
        }
        Ok(())
    }
    pub(crate) fn push(&self, s: String) {
        self.event(RuntimeEvent::Warning(s));
    }
    fn event(&self, e: RuntimeEvent) {
        let mut s = self.0.lock().unwrap();
        if matches!(e, RuntimeEvent::VmError(_)) {
            s.vm_errors += 1;
        }
        if let RuntimeEvent::HostCall {
            request,
            reply: HostReply::Unavailable(_),
        } = &e
        {
            s.unavailable.insert(request.name.clone());
        }
        if s.events.len() >= 16384 {
            s.events.pop_front();
            s.dropped += 1;
        }
        s.events.push_back(e);
    }
}
impl LogBackend for Audit {
    fn avm_trace(&self, s: &str) {
        self.event(RuntimeEvent::Trace(s.into()));
    }
    fn avm_warning(&self, s: &str) {
        self.event(RuntimeEvent::Warning(s.into()));
    }
    fn avm_error(&self, s: &str) {
        self.event(RuntimeEvent::VmError(s.into()));
    }
}
struct Bridge {
    host: Arc<dyn UiHost>,
    audit: Audit,
    next: AtomicU64,
}
impl ExternalInterfaceProvider for Bridge {
    fn call_method(
        &self,
        _: &mut UpdateContext<'_>,
        bridge: &str,
        args: &[HostValue],
    ) -> HostValue {
        let kind = match bridge {
            "CallGameService" => HostBridge::CallGameService,
            "SendGameMessage" => HostBridge::SendGameMessage,
            "ObjectSendMessage" => HostBridge::ObjectSendMessage,
            n => HostBridge::Other(n.into()),
        };
        let name = match &kind {
            HostBridge::Other(name) => Some(HostValue::String(name.clone())),
            HostBridge::ObjectSendMessage => args.get(1).cloned(),
            _ => args.first().cloned(),
        };
        let request = HostRequest {
            request_id: self.next.fetch_add(1, Ordering::Relaxed),
            bridge: kind,
            name: match name {
                Some(HostValue::String(s)) => s.clone(),
                _ => String::new(),
            },
            args: args.to_vec(),
        };
        let reply = self.host.call(&request);
        let value = match &reply {
            HostReply::Value(v) => v.clone(),
            HostReply::Unavailable(_) => HostValue::Undefined,
        };
        self.audit.event(RuntimeEvent::HostCall { request, reply });
        value
    }
    fn on_callback_available(&self, n: &str) {
        self.audit.event(RuntimeEvent::CallbackAvailable(n.into()));
    }
    fn get_id(&self) -> Option<String> {
        Some("external-cas-source-ui".into())
    }
}
impl FsCommandProvider for Audit {
    fn on_fs_command(&self, n: &str, a: &str) -> bool {
        self.event(RuntimeEvent::Denied(format!("FSCommand {n}: {a}")));
        false
    }
}

pub struct SourceUiRuntime {
    player: Arc<Mutex<Player>>,
    executor: NullExecutor,
    audit: Audit,
    dimensions: [u32; 2],
    dpi: f64,
    limits: RuntimeLimits,
    image_cancel: Arc<AtomicBool>,
    pub source_pins: Vec<external_cas_fidelity::installed::SourcePin>,
}
impl SourceUiRuntime {
    /// Uses the application's existing Instance/Adapter/Device/Queue. It neither
    /// creates another GPU device nor opens a window or operating-system file.
    pub fn new(
        gpu: SharedGpu,
        bundle: PreparedUiBundle,
        host: Arc<dyn UiHost>,
        dimensions: [u32; 2],
        dpi: f64,
    ) -> Result<Self, String> {
        Self::new_with_limits(gpu, bundle, host, dimensions, dpi, RuntimeLimits::default())
    }
    pub fn new_with_limits(
        gpu: SharedGpu,
        bundle: PreparedUiBundle,
        host: Arc<dyn UiHost>,
        dimensions: [u32; 2],
        dpi: f64,
        limits: RuntimeLimits,
    ) -> Result<Self, String> {
        Self::new_configured(
            gpu,
            bundle,
            host,
            dimensions,
            dpi,
            limits,
            None,
            Arc::new(AtomicBool::new(false)),
        )
    }
    pub fn new_with_static_images(
        gpu: SharedGpu,
        bundle: PreparedUiBundle,
        host: Arc<dyn UiHost>,
        dimensions: [u32; 2],
        dpi: f64,
        resolver: Option<Arc<dyn StaticImageResolver>>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        Self::new_configured(
            gpu,
            bundle,
            host,
            dimensions,
            dpi,
            RuntimeLimits::default(),
            resolver,
            cancel,
        )
    }
    fn new_configured(
        gpu: SharedGpu,
        bundle: PreparedUiBundle,
        host: Arc<dyn UiHost>,
        dimensions: [u32; 2],
        dpi: f64,
        limits: RuntimeLimits,
        resolver: Option<Arc<dyn StaticImageResolver>>,
        image_cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        if limits.script_timeout.is_zero()
            || limits.preload_ops == 0
            || limits.preload_slice.is_zero()
        {
            return Err("source VM limits must be positive".into());
        }
        bundle.validate()?;
        gpu::validate_dimensions(&gpu.device, dimensions, dpi)?;
        let root = bundle
            .resources
            .get(&bundle.root)
            .ok_or("source UI root absent")?;
        let url = format!("https://source-ui.invalid/UI/{}", bundle.root);
        let movie = SwfMovieData::from_data(&root.runtime_bytes, url, None, None)
            .map_err(|e| e.to_string())?;
        let executor = NullExecutor::new();
        let audit = Audit::new();
        audit
            .0
            .lock()
            .map_err(|_| "Source audit lock failed")?
            .initial_pins = bundle.source_pins.clone();
        let renderer = gpu::renderer(gpu, dimensions)?;
        let ui = source_fonts::Ui {
            fonts: bundle.device_fonts,
            audit: audit.clone(),
            source_pin: None,
            config_pin: None,
        };
        let player = PlayerBuilder::new()
            .with_movie(movie)
            .with_renderer(renderer)
            .with_ui(ui)
            .with_navigator(navigator::Navigator::new(
                bundle.resources,
                executor.spawner(),
                audit.clone(),
                resolver,
                image_cancel.clone(),
            ))
            .with_log(audit.clone())
            .with_external_interface(Box::new(Bridge {
                host,
                audit: audit.clone(),
                next: AtomicU64::new(1),
            }))
            .with_fs_commands(Box::new(audit.clone()))
            .with_max_execution_duration(limits.script_timeout)
            .with_viewport_dimensions(dimensions[0], dimensions[1], dpi)
            .with_letterbox(ruffle_core::config::Letterbox::Off)
            .with_autoplay(true)
            .build();
        player.lock().unwrap().set_window_mode("transparent");
        Ok(Self {
            player,
            executor,
            audit,
            dimensions,
            dpi,
            limits,
            image_cancel,
            source_pins: bundle.source_pins,
        })
    }
    /// Process memory-only import jobs and advance actual source timing. Elapsed
    /// time is caller supplied; no per-frame disk IO or source rewrites occur.
    pub fn tick(&mut self, elapsed: Duration) {
        self.executor.run();
        let mut p = self.player.lock().unwrap();
        p.preload(&mut ExecutionLimit::with_max_ops_and_time(
            self.limits.preload_ops,
            self.limits.preload_slice,
        ));
        p.tick(FloatDuration::from_std(elapsed));
    }
    /// Advance source dependency loading without running its first frame.
    /// Native Function slots can then be bound before original constructors use them.
    pub fn preload(&mut self) -> bool {
        self.executor.run();
        self.player
            .lock()
            .unwrap()
            .preload(&mut ExecutionLimit::with_max_ops_and_time(
                self.limits.preload_ops,
                self.limits.preload_slice,
            ))
    }
    pub fn bind_native_function(
        &mut self,
        class: &str,
        member: &str,
        bridge: &str,
    ) -> Result<(), String> {
        self.player
            .lock()
            .unwrap()
            .bind_avm2_static_host_function(class, member, bridge)
    }
    pub fn bind_native_function_projected(
        &mut self,
        class: &str,
        member: &str,
        bridge: &str,
        fields: &[&str],
    ) -> Result<(), String> {
        self.player
            .lock()
            .unwrap()
            .bind_avm2_static_host_function_projected(class, member, bridge, fields)
    }
    pub fn construct_source_class(
        &mut self,
        class: &str,
        args: Vec<HostValue>,
    ) -> Result<HostValue, String> {
        self.player
            .lock()
            .unwrap()
            .construct_avm2_script_class(class, args)
    }
    pub fn retain_source_root(&mut self) -> Result<HostValue, String> {
        self.player.lock().unwrap().retain_avm2_root_object()
    }
    pub fn get_source_property(&mut self, id: u64, name: &str) -> Result<HostValue, String> {
        self.player
            .lock()
            .unwrap()
            .get_retained_avm2_property(id, name)
    }
    pub fn set_source_property(
        &mut self,
        id: u64,
        name: &str,
        value: HostValue,
    ) -> Result<(), String> {
        self.player
            .lock()
            .unwrap()
            .set_retained_avm2_property(id, name, value)
    }
    pub fn call_source_method(
        &mut self,
        id: u64,
        name: &str,
        args: Vec<HostValue>,
    ) -> Result<HostValue, String> {
        self.player
            .lock()
            .unwrap()
            .call_retained_avm2_method(id, name, args)
    }
    /// Trusted native host/qualification seam. Never call from UiHost itself;
    /// the original VM must not be reentered during a synchronous host request.
    pub fn configure_player(
        &mut self,
        configure: impl FnOnce(&mut Player) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut player = self
            .player
            .lock()
            .map_err(|_| "Original player lock failed")?;
        configure(&mut player)
    }
    pub fn input(&mut self, input: UiInput) -> bool {
        self.player.lock().unwrap().handle_event(input)
    }
    pub fn resize(&mut self, size: [u32; 2], dpi: f64) -> Result<(), String> {
        let mut p = self.player.lock().unwrap();
        let r = gpu::get_renderer(&p)?;
        gpu::validate_dimensions(&r.descriptors().device, size, dpi)?;
        p.set_viewport_dimensions(ViewportDimensions {
            width: size[0],
            height: size[1],
            scale_factor: dpi,
        });
        self.dimensions = size;
        self.dpi = dpi;
        Ok(())
    }
    pub fn render(&mut self) -> Result<UiFrame, String> {
        let mut p = self.player.lock().unwrap();
        p.render();
        let r = gpu::get_renderer(&p)?;
        Ok(UiFrame {
            texture: r.target().texture.clone(),
            view: r.target().texture.create_view(&Default::default()),
            dimensions: self.dimensions,
            format: r.target().format,
            premultiplied_alpha: true,
        })
    }
    /// Invoke only the source's actually registered callback. Return its genuine
    /// result; an absent callback is an explicit error instead of silent Null.
    pub fn call_source(&mut self, name: &str, args: Vec<HostValue>) -> Result<HostValue, String> {
        let mut p = self.player.lock().unwrap();
        let present =
            p.mutate_with_update_context(|c| c.external_interface.get_callback(name).is_some());
        if !present {
            return Err(format!("original UI callback {name} is not registered"));
        }
        Ok(p.call_internal_interface(name, args))
    }
    pub fn send_message(&mut self, name: &str, payload: HostValue) -> Result<HostValue, String> {
        self.call_source("SendMessage", vec![HostValue::String(name.into()), payload])
    }
    pub fn call_service(&mut self, name: &str, payload: HostValue) -> Result<HostValue, String> {
        self.call_source("CallService", vec![HostValue::String(name.into()), payload])
    }
    /// Retained native callback values stay in the VM/GC arena; no function is
    /// serialized as Null. Handles are valid only for this runtime's lifetime.
    pub fn call_retained_function(
        &mut self,
        id: u64,
        args: Vec<HostValue>,
    ) -> Result<HostValue, String> {
        self.player
            .lock()
            .unwrap()
            .call_retained_avm2_function(id, args)
    }
    pub fn drain_events(&mut self) -> Vec<RuntimeEvent> {
        {
            let mut s = self.audit.0.lock().unwrap();
            let mut events = s.events.drain(..).collect::<Vec<_>>();
            if s.dropped > 0 {
                events.push(RuntimeEvent::EventsDropped(s.dropped));
            }
            events
        }
    }
    pub fn resolved_image_pins(&self) -> Vec<external_cas_fidelity::installed::SourcePin> {
        self.audit
            .0
            .lock()
            .unwrap()
            .image_pins
            .values()
            .cloned()
            .collect()
    }
    pub fn status(&self) -> RuntimeStatus {
        let s = self.audit.0.lock().unwrap();
        RuntimeStatus {
            vm_errors: s.vm_errors,
            unbound_services: s.unavailable.iter().cloned().collect(),
            events_dropped: s.dropped,
            complete_gfx_support: false,
        }
    }
    pub fn needs_render(&self) -> bool {
        self.player.lock().unwrap().needs_render()
    }
    pub fn movie_frame(&self) -> Option<u16> {
        self.player.lock().unwrap().current_frame()
    }
}

impl Drop for SourceUiRuntime {
    fn drop(&mut self) {
        self.image_cancel.store(true, Ordering::Relaxed);
    }
}
