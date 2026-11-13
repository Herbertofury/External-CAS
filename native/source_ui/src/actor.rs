// SPDX-License-Identifier: GPL-3.0-or-later
//! Reusable original-movie worker. The app owns its verified editor snapshot and
//! native actions; this module owns VM/GPU lifecycle and never writes game files.
use crate::*;
use external_cas_fidelity::{installed::SourcePin, localization::StringBank};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq)]
pub struct ObservedSimInfo {
    pub sim_id: u64,
    pub first_name: String,
    pub last_name: String,
    pub age: Option<u32>,
    pub gender: Option<u32>,
    pub species_type: Option<u32>,
    /// Only actually observed/source-qualified fields. Identity collisions fail.
    pub observed_fields: BTreeMap<String, HostValue>,
}
impl ObservedSimInfo {
    pub fn packet(&self) -> Result<HostValue, String> {
        if self.sim_id == 0 {
            return Err("Observed Sim identity is absent".into());
        }
        let mut p = self.observed_fields.clone();
        for (name, value) in [
            ("simId", HostValue::String(self.sim_id.to_string())),
            ("firstName", HostValue::String(self.first_name.clone())),
            ("lastName", HostValue::String(self.last_name.clone())),
        ] {
            if p.insert(name.into(), value).is_some() {
                return Err(format!("Observed Sim identity field {name} duplicated"));
            }
        }
        for (name, value) in [
            ("age", self.age),
            ("gender", self.gender),
            ("speciesType", self.species_type),
        ] {
            if let Some(value) = value {
                if p.insert(name.into(), HostValue::Number(f64::from(value)))
                    .is_some()
                {
                    return Err(format!("Observed Sim field {name} duplicated"));
                }
            }
        }
        Ok(HostValue::Object(p))
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorSnapshot {
    pub revision: u64,
    pub ready: bool,
    pub sim: Option<ObservedSimInfo>,
    /// Current native editor facts used by original source menu predicates.
    pub menu_context: Option<menus::CriterionContext>,
    /// Source service packets already qualified by the native app/backend.
    pub service_replies: BTreeMap<String, HostValue>,
    /// Names whose actual native action implementation is currently available.
    pub action_services: BTreeSet<String>,
}
impl EditorSnapshot {
    pub fn reply(&self, name: &str) -> Result<Option<HostValue>, String> {
        if name == "CASGetSimInfo" {
            if let Some(sim) = &self.sim {
                return sim.packet().map(Some);
            }
        }
        Ok(self.service_replies.get(name).cloned())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.sim.is_some() && self.service_replies.contains_key("CASGetSimInfo") {
            return Err("Observed Sim packet and explicit CASGetSimInfo reply conflict".into());
        }
        if let Some(sim) = &self.sim {
            sim.packet()?;
        }
        let mut bytes = 0usize;
        for (name, value) in &self.service_replies {
            if self.action_services.contains(name) {
                return Err(format!(
                    "Service {name} cannot be both a read reply and a native action"
                ));
            }
            add_budget(&mut bytes, name.len())?;
            measure_value(value, 0, &mut bytes)?;
        }
        for name in &self.action_services {
            add_budget(&mut bytes, name.len())?;
        }
        if let Some(sim) = &self.sim {
            measure_value(&sim.packet()?, 0, &mut bytes)?;
        }
        if let Some(context) = &self.menu_context {
            for name in context.booleans.keys() {
                add_budget(&mut bytes, name.len() + 1)?;
            }
            if let Some(packs) = &context.available_packs {
                add_budget(
                    &mut bytes,
                    packs
                        .len()
                        .checked_mul(8)
                        .ok_or("Editor pack context extent overflow")?,
                )?;
            }
        }
        Ok(())
    }
}
const SNAPSHOT_BYTE_LIMIT: usize = 64 * 1024 * 1024;
fn add_budget(bytes: &mut usize, value: usize) -> Result<(), String> {
    *bytes = bytes
        .checked_add(value)
        .ok_or("Editor snapshot size overflow")?;
    if *bytes > SNAPSHOT_BYTE_LIMIT {
        return Err("Editor snapshot exceeds serialized byte budget".into());
    }
    Ok(())
}
fn measure_value(value: &HostValue, depth: usize, bytes: &mut usize) -> Result<(), String> {
    if depth > 64 {
        return Err("Editor snapshot exceeds nested value depth budget".into());
    }
    add_budget(bytes, 16)?;
    match value {
        HostValue::Avm2Handle(_) => {
            return Err(
                "A persistent editor snapshot cannot invent or retain source VM identities".into(),
            );
        }
        HostValue::Number(n) if !n.is_finite() => {
            return Err("Nonfinite editor service number".into());
        }
        HostValue::String(s) => add_budget(bytes, s.len())?,
        HostValue::List(v) => {
            for v in v {
                measure_value(v, depth + 1, bytes)?;
            }
        }
        HostValue::Object(v) => {
            for (k, v) in v {
                add_budget(bytes, k.len())?;
                measure_value(v, depth + 1, bytes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn accept_snapshot(current: &mut EditorSnapshot, next: EditorSnapshot) -> Result<(), String> {
    next.validate()?;
    if next.revision < current.revision || (next.revision == current.revision && next != *current) {
        return Err("Stale or conflicting editor snapshot revision".into());
    }
    *current = next;
    Ok(())
}
fn missing_context(
    snapshot: &EditorSnapshot,
    required: &BTreeSet<String>,
    static_services: &BTreeSet<String>,
    menu: Option<&menus::CasMenuSnapshot>,
) -> Result<Vec<String>, String> {
    let mut missing = Vec::new();
    if !snapshot.ready {
        missing.push("verified editor context".into());
    }
    for name in required {
        if static_services.contains(name) {
            continue;
        }
        if let Some(menu) = menu {
            if menus::MENU_SERVICES.contains(&name.as_str()) {
                let result = snapshot
                    .menu_context
                    .as_ref()
                    .ok_or_else(|| "verified menu context unobserved".to_owned())
                    .and_then(|c| {
                        menu.reply(name, c)
                            .ok_or_else(|| "Source menu service selector differs".to_owned())?
                    });
                if let Err(reason) = result {
                    missing.push(format!("{name}: {reason}"));
                }
                continue;
            }
        }
        if snapshot.reply(name)?.is_none() {
            missing.push(name.clone());
        }
    }
    let mut providers = static_services.clone();
    if menu.is_some() {
        providers.extend(menus::MENU_SERVICES.map(str::to_owned));
    }
    for name in providers {
        if snapshot.reply(&name)?.is_some() || snapshot.action_services.contains(&name) {
            return Err(format!("Static and editor service {name} sources conflict"));
        }
    }
    Ok(missing)
}
#[derive(Clone, Debug, PartialEq)]
pub enum EditorAction {
    SetSimView {
        left: bool,
        button_down: bool,
    },
    Undo,
    Redo,
    /// App explicitly advertises this original service; raw arguments retained.
    Service,
}
#[derive(Clone, Debug)]
pub struct ActionRequest {
    pub snapshot_revision: u64,
    pub request_id: u64,
    pub action: EditorAction,
    pub original: HostRequest,
}
#[derive(Clone, Debug)]
pub struct FrameRequest {
    pub size: [u32; 2],
    pub dpi: f64,
    pub input: Vec<UiInput>,
}
pub enum Command {
    Frame(FrameRequest),
    SendMessage { name: String, payload: HostValue },
    CallSource { name: String, args: Vec<HostValue> },
    CallRetained { id: u64, args: Vec<HostValue> },
}
pub enum ActorEvent {
    Progress(String),
    WaitingForEditor {
        revision: u64,
        missing: Vec<String>,
    },
    Frame {
        frame: UiFrame,
        status: RuntimeStatus,
        events: Vec<RuntimeEvent>,
        ticks: u64,
        pins: Vec<SourcePin>,
        bootstrap: bootstrap::BootstrapStatus,
        snapshot_revision: u64,
    },
    Action(ActionRequest),
    InboundResult {
        operation: String,
        result: Result<HostValue, String>,
    },
    Failed(String),
}
pub struct HostConfig {
    pub provider: PathBuf,
    pub root: String,
    pub locale: String,
    pub string_providers: Vec<PathBuf>,
    pub expected_ui_pins: Option<Vec<SourcePin>>,
    pub trope_global_provider: Option<PathBuf>,
    /// Explicit resolved winner of the exact installed CAS static menu TGI.
    pub menu_provider: Option<PathBuf>,
    pub static_images: Option<Arc<dyn StaticImageResolver>>,
    pub trope_item_providers: Vec<PathBuf>,
    pub static_replies: BTreeMap<String, HostValue>,
    /// No first frame until these native/source packets exist. Never invent data.
    pub required_before_first_frame: BTreeSet<String>,
    /// Qualification-only seam; production core pin/config stays caller-owned.
    pub configure_runtime:
        Option<Box<dyn FnOnce(&mut SourceUiRuntime) -> Result<(), String> + Send>>,
}
pub struct Actor {
    commands: mpsc::SyncSender<Command>,
    events: mpsc::Receiver<ActorEvent>,
    snapshot: Arc<RwLock<EditorSnapshot>>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Actor {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Actor {
    pub fn publish(&self, next: EditorSnapshot) -> Result<(), String> {
        let mut current = self
            .snapshot
            .write()
            .map_err(|_| "Editor snapshot lock failed")?;
        accept_snapshot(&mut current, next)
    }
    pub fn try_send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        self.commands.try_send(command)
    }
    pub fn try_recv(&self) -> Result<ActorEvent, mpsc::TryRecvError> {
        self.events.try_recv()
    }
    pub fn spawn(
        gpu: SharedGpu,
        mut config: HostConfig,
        initial: EditorSnapshot,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        initial.validate()?;
        let snapshot = Arc::new(RwLock::new(initial));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (commands, receive) = mpsc::sync_channel(1);
        let (send, events) = mpsc::channel();
        let shared = snapshot.clone();
        let cancel = cancelled.clone();
        let active = Arc::new(RwLock::new(
            snapshot
                .read()
                .map_err(|_| "Editor snapshot lock failed")?
                .clone(),
        ));
        std::thread::spawn(move || {
            let emit = |event| {
                let result = send.send(event);
                wake();
                result
            };
            let result = (|| -> Result<(), String> {
                emit(ActorEvent::Progress(
                    "Reading original UI dependencies".into(),
                ))
                .map_err(|_| "Actor closed")?;
                let index = external_cas_assets::PackageIndex::open(&config.provider)
                    .map_err(|e| e.to_string())?;
                let aliases = locale_aliases(&config.locale)?;
                let bundle = prepare_movie_bundle_with_aliases(
                    &index,
                    &config.root,
                    config.root == "UIContainer",
                    &aliases,
                    config.expected_ui_pins.as_deref(),
                    &cancel,
                )?;
                let mut tables = Vec::new();
                for path in &config.string_providers {
                    let i =
                        external_cas_assets::PackageIndex::open(path).map_err(|e| e.to_string())?;
                    tables.extend(StringBank::load(&i, &cancel)?.tables);
                }
                let strings = Arc::new(StringBank::from_tables(tables));
                let mut pins = bundle.source_pins.clone();
                pins.extend(strings.source_pins());
                let menu = if let Some(path) = &config.menu_provider {
                    let menu = Arc::new(menus::CasMenuSnapshot::load(path, &cancel)?);
                    pins.push(menu.pin.clone());
                    Some(menu)
                } else {
                    None
                };
                let mut static_replies = config.static_replies;
                let _trope_sources = if let Some(path) = &config.trope_global_provider {
                    let loaded = static_data::TropesSnapshot::load(
                        path,
                        &config.trope_item_providers,
                        &cancel,
                    )?;
                    pins.extend(loaded.pins());
                    if static_replies
                        .insert("GetTropesTuning".into(), loaded.payload.clone())
                        .is_some()
                    {
                        return Err("Static GetTropesTuning sources conflict".into());
                    }
                    Some(loaded)
                } else {
                    None
                };
                let mut static_bytes = 0;
                for (name, value) in &static_replies {
                    add_budget(&mut static_bytes, name.len())?;
                    measure_value(value, 0, &mut static_bytes)?;
                }
                if menu.is_some()
                    && menus::MENU_SERVICES
                        .iter()
                        .any(|n| static_replies.contains_key(*n))
                {
                    return Err("Source menu provider and explicit static replies conflict".into());
                }
                let source_services = static_replies.keys().cloned().collect::<BTreeSet<_>>();
                let host = SnapshotHost {
                    snapshot: active.clone(),
                    strings,
                    static_replies,
                    menu: menu.clone(),
                    actions: send.clone(),
                    wake: wake.clone(),
                };
                let mut runtime = SourceUiRuntime::new_with_static_images(
                    gpu,
                    bundle,
                    Arc::new(host),
                    [1440, 900],
                    1.,
                    config.static_images.take(),
                    cancel.clone(),
                )?;
                if let Some(configure) = config.configure_runtime.take() {
                    configure(&mut runtime)?;
                }
                let mut bootstrap = bootstrap::BootstrapStatus::default();
                emit(ActorEvent::Progress(
                    "Preloading original definitions".into(),
                ))
                .map_err(|_| "Actor closed")?;
                while !cancel.load(Ordering::Relaxed) {
                    if runtime.preload() {
                        bootstrap.definitions_ready = true;
                        break;
                    }
                    std::thread::yield_now();
                }
                if cancel.load(Ordering::Relaxed) {
                    return Ok(());
                }
                bootstrap::bind_localization(&mut runtime)?;
                bootstrap.localization_bound = true;
                if config.root != "UIContainer" {
                    runtime.construct_source_class("olympus.io.CommunicationManager", vec![])?;
                }
                let mut started = false;
                let mut ticks = 0;
                let mut clock = Instant::now();
                let mut size = [1440, 900];
                let mut dpi = 1.;
                while !cancel.load(Ordering::Relaxed) {
                    let command = match receive.recv_timeout(Duration::from_millis(50)) {
                        Ok(v) => v,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    {
                        let s = shared.read().map_err(|_| "Editor snapshot lock failed")?;
                        let missing = missing_context(
                            &s,
                            &config.required_before_first_frame,
                            &source_services,
                            menu.as_deref(),
                        )?;
                        if !missing.is_empty() {
                            match &command {
                                Command::Frame(_) => {
                                    emit(ActorEvent::WaitingForEditor {
                                        revision: s.revision,
                                        missing,
                                    })
                                    .map_err(|_| "Actor closed")?;
                                }
                                _ => {
                                    emit(ActorEvent::InboundResult{operation:"source inbound command".into(),result:Err("Original source lifecycle is waiting for verified editor data".into())}).map_err(|_|"Actor closed")?;
                                }
                            }
                            continue;
                        }
                        if !started {
                            started = true;
                            clock = Instant::now();
                        }
                    }
                    let accepted = shared
                        .read()
                        .map_err(|_| "Editor snapshot lock failed")?
                        .clone();
                    let frame_revision = accepted.revision;
                    *active
                        .write()
                        .map_err(|_| "Active editor snapshot lock failed")? = accepted;
                    match command {
                        Command::Frame(next) => {
                            if next.size != size || next.dpi != dpi {
                                runtime.resize(next.size, next.dpi)?;
                                size = next.size;
                                dpi = next.dpi;
                            }
                            for input in next.input {
                                runtime.input(input);
                            }
                            let now = Instant::now();
                            runtime.tick(now.duration_since(clock).min(Duration::from_millis(250)));
                            clock = now;
                            ticks += 1;
                            if !bootstrap.source_root_ready && runtime.retain_source_root().is_ok()
                            {
                                bootstrap.source_root_ready = true;
                            }
                            if config.root != "UIContainer"
                                && !bootstrap.widget_initialized
                                && bootstrap.errors.is_empty()
                            {
                                match bootstrap::try_initialize_widget(&mut runtime, &config.root) {
                                    Ok(Some(_)) => bootstrap.widget_initialized = true,
                                    Ok(None) => {}
                                    Err(e) => bootstrap.errors.push(e),
                                }
                            }
                            let frame = runtime.render()?;
                            let revision = frame_revision;
                            emit(ActorEvent::Frame {
                                frame,
                                status: runtime.status(),
                                events: runtime.drain_events(),
                                ticks,
                                pins: {
                                    let mut observed = pins.clone();
                                    for pin in runtime.resolved_image_pins() {
                                        if !observed.contains(&pin) {
                                            observed.push(pin);
                                        }
                                    }
                                    observed
                                },
                                bootstrap: bootstrap.clone(),
                                snapshot_revision: revision,
                            })
                            .map_err(|_| "Actor closed")?;
                        }
                        Command::SendMessage { name, payload } => {
                            let result = runtime.send_message(&name, payload);
                            emit(ActorEvent::InboundResult {
                                operation: name,
                                result,
                            })
                            .map_err(|_| "Actor closed")?;
                        }
                        Command::CallSource { name, args } => {
                            let result = runtime.call_source(&name, args);
                            emit(ActorEvent::InboundResult {
                                operation: name,
                                result,
                            })
                            .map_err(|_| "Actor closed")?;
                        }
                        Command::CallRetained { id, args } => {
                            let result = runtime.call_retained_function(id, args);
                            emit(ActorEvent::InboundResult {
                                operation: format!("retained callback {id}"),
                                result,
                            })
                            .map_err(|_| "Actor closed")?;
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = emit(ActorEvent::Failed(error));
            }
        });
        Ok(Self {
            commands,
            events,
            snapshot,
            cancelled,
        })
    }
}
struct SnapshotHost {
    snapshot: Arc<RwLock<EditorSnapshot>>,
    strings: Arc<StringBank>,
    static_replies: BTreeMap<String, HostValue>,
    menu: Option<Arc<menus::CasMenuSnapshot>>,
    actions: mpsc::Sender<ActorEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl UiHost for SnapshotHost {
    fn call(&self, r: &HostRequest) -> HostReply {
        let result = (|| -> Result<HostValue, String> {
            if r.bridge == HostBridge::Other("ExternalCAS.GetLocalizedString".into()) {
                let Some(HostValue::Object(input)) = r.args.first() else {
                    return Err("Original LocKey was not projected".into());
                };
                let integer = |name: &str| -> Result<u32, String> {
                    match input.get(name) {
                        Some(HostValue::Number(v))
                            if v.is_finite()
                                && *v >= 0.
                                && *v <= u32::MAX as f64
                                && v.fract() == 0. =>
                        {
                            Ok(*v as u32)
                        }
                        _ => Err(format!("Original LocKey {name} is not u32")),
                    }
                };
                let tokens = match input.get("tokens") {
                    Some(HostValue::Null) | Some(HostValue::Undefined) | None => 0,
                    Some(HostValue::List(v)) => v.len(),
                    _ => return Err("Original LocKey tokens invalid".into()),
                };
                return Ok(HostValue::String(
                    self.strings
                        .resolve_literal(integer("hash")?, tokens, integer("flags")?)?
                        .to_owned(),
                ));
            }
            if r.bridge != HostBridge::CallGameService {
                return Err("Original bridge has no qualified native binding".into());
            }
            if let Some(value) = self.static_replies.get(&r.name) {
                return Ok(value.clone());
            }
            let s = self
                .snapshot
                .read()
                .map_err(|_| "Editor snapshot lock failed")?;
            if !s.ready {
                return Err("Verified native editor context is not ready".into());
            }
            if let Some(menu) = &self.menu {
                if menus::MENU_SERVICES.contains(&r.name.as_str()) {
                    let context = s
                        .menu_context
                        .as_ref()
                        .ok_or("Verified source menu context absent")?;
                    return menu
                        .reply(&r.name, context)
                        .ok_or("Source menu selector differs")?;
                }
            }
            if r.name == "CASGetSimInfo" {
                match r.payload() {
                    None | Some(HostValue::Null) | Some(HostValue::Undefined) => {}
                    Some(HostValue::String(id))
                        if s.sim
                            .as_ref()
                            .is_some_and(|sim| sim.sim_id.to_string() == *id) => {}
                    _ => {
                        return Err(
                            "CASGetSimInfo requested an identity absent from the verified snapshot"
                                .into(),
                        );
                    }
                }
            }
            if let Some(value) = s.reply(&r.name)? {
                return Ok(value);
            }
            if !s.action_services.contains(&r.name) {
                return Err(format!(
                    "Original service {} has no qualified native binding",
                    r.name
                ));
            }
            let action = match r.name.as_str() {
                "SetSimView" => {
                    let Some(HostValue::Object(p)) = r.payload() else {
                        return Err("Original SetSimView payload is not an object".into());
                    };
                    let read = |n: &str| match p.get(n) {
                        Some(HostValue::Bool(v)) => Ok(*v),
                        _ => Err(format!("Original SetSimView {n} is not Boolean")),
                    };
                    EditorAction::SetSimView {
                        left: read("left")?,
                        button_down: read("button_down")?,
                    }
                }
                "CasNavigationUndo" | "CasNavigationRedo" => {
                    if r.asynchronous() != Some(true)
                        || !matches!(r.payload(), Some(HostValue::Null))
                    {
                        return Err("Original Undo/Redo request requires its Null payload and true asynchronous flag".into());
                    }
                    if r.name == "CasNavigationUndo" {
                        EditorAction::Undo
                    } else {
                        EditorAction::Redo
                    }
                }
                _ => EditorAction::Service,
            };
            self.actions
                .send(ActorEvent::Action(ActionRequest {
                    snapshot_revision: s.revision,
                    request_id: r.request_id,
                    action,
                    original: r.clone(),
                }))
                .map_err(|_| "Native action receiver closed")?;
            (self.wake)();
            // Queue admission only. Mutation completion is never fabricated;
            // app publishes a new snapshot and the source's inbound messages.
            Ok(HostValue::Undefined)
        })();
        match result {
            Ok(value) => HostReply::Value(value),
            Err(reason) => HostReply::Unavailable(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sim() -> ObservedSimInfo {
        ObservedSimInfo {
            sim_id: u64::MAX,
            first_name: "Observed".into(),
            last_name: "Source".into(),
            age: Some(32),
            gender: Some(0x2000),
            species_type: None,
            observed_fields: BTreeMap::new(),
        }
    }
    fn snapshot() -> EditorSnapshot {
        EditorSnapshot {
            revision: 7,
            ready: true,
            sim: Some(sim()),
            ..Default::default()
        }
    }
    fn host(snapshot: EditorSnapshot) -> (SnapshotHost, mpsc::Receiver<ActorEvent>) {
        let (send, receive) = mpsc::channel();
        (
            SnapshotHost {
                snapshot: Arc::new(RwLock::new(snapshot)),
                strings: Arc::new(StringBank::from_tables(Vec::new())),
                static_replies: BTreeMap::new(),
                menu: None,
                actions: send,
                wake: Arc::new(|| {}),
            },
            receive,
        )
    }
    fn request(name: &str, payload: HostValue, asynchronous: bool) -> HostRequest {
        HostRequest {
            request_id: 31,
            bridge: HostBridge::CallGameService,
            name: name.into(),
            args: vec![
                HostValue::String(name.into()),
                payload,
                HostValue::Bool(asynchronous),
            ],
        }
    }
    #[test]
    fn exact_observed_identity_omits_unknown_fields_and_rejects_collisions() {
        let HostValue::Object(p) = sim().packet().unwrap() else {
            panic!()
        };
        assert_eq!(
            p.get("simId"),
            Some(&HostValue::String(u64::MAX.to_string()))
        );
        assert!(!p.contains_key("speciesType"));
        assert!(!p.contains_key("isPregnant"));
        let mut value = sim();
        value
            .observed_fields
            .insert("firstName".into(), HostValue::String("other".into()));
        assert!(value.packet().unwrap_err().contains("duplicated"));
    }
    #[test]
    fn snapshots_reject_handles_nonfinite_deep_values_and_read_action_conflicts() {
        let mut s = snapshot();
        s.service_replies.insert(
            "x".into(),
            HostValue::Object(BTreeMap::from([("deep".into(), HostValue::Avm2Handle(9))])),
        );
        assert!(s.validate().unwrap_err().contains("identities"));
        s.service_replies
            .insert("x".into(), HostValue::Number(f64::NAN));
        assert!(s.validate().unwrap_err().contains("Nonfinite"));
        let mut nested = HostValue::Null;
        for _ in 0..66 {
            nested = HostValue::List(vec![nested]);
        }
        s.service_replies.insert("x".into(), nested);
        assert!(s.validate().unwrap_err().contains("depth"));
        s.service_replies.insert("x".into(), HostValue::Null);
        s.action_services.insert("x".into());
        assert!(s.validate().unwrap_err().contains("both"));
    }
    #[test]
    fn snapshot_revisions_cannot_relabel_changed_content() {
        let mut current = snapshot();
        assert!(accept_snapshot(&mut current, snapshot()).is_ok());
        let mut stale = snapshot();
        stale.revision = 6;
        assert!(accept_snapshot(&mut current, stale).is_err());
        let mut changed = snapshot();
        changed.sim.as_mut().unwrap().first_name = "Changed".into();
        assert!(accept_snapshot(&mut current, changed.clone()).is_err());
        assert_eq!(current, snapshot());
        changed.revision = 8;
        accept_snapshot(&mut current, changed).unwrap();
        assert_eq!(current.revision, 8);
    }
    #[test]
    fn requested_other_sim_is_not_answered_with_current_sim() {
        let (h, _) = host(snapshot());
        assert!(matches!(
            h.call(&request("CASGetSimInfo", HostValue::Null, false)),
            HostReply::Value(HostValue::Object(_))
        ));
        assert!(matches!(
            h.call(&request(
                "CASGetSimInfo",
                HostValue::String(u64::MAX.to_string()),
                false
            )),
            HostReply::Value(HostValue::Object(_))
        ));
        assert!(matches!(
            h.call(&request(
                "CASGetSimInfo",
                HostValue::String("42".into()),
                false
            )),
            HostReply::Unavailable(_)
        ));
        assert!(matches!(
            h.call(&request("GetFamilyInfo", HostValue::Null, false)),
            HostReply::Unavailable(_)
        ));
    }
    #[test]
    fn admitted_original_actions_keep_revision_arguments_and_no_success_fabrication() {
        let mut s = snapshot();
        s.action_services.insert("CasNavigationUndo".into());
        s.action_services.insert("SetSimView".into());
        let (h, rx) = host(s);
        assert!(matches!(
            h.call(&request("CasNavigationUndo", HostValue::Null, true)),
            HostReply::Value(HostValue::Undefined)
        ));
        let ActorEvent::Action(a) = rx.recv().unwrap() else {
            panic!()
        };
        assert_eq!(a.snapshot_revision, 7);
        assert_eq!(a.request_id, 31);
        assert_eq!(a.action, EditorAction::Undo);
        assert_eq!(a.original.asynchronous(), Some(true));
        assert!(matches!(
            h.call(&request("CasNavigationUndo", HostValue::Null, false)),
            HostReply::Unavailable(_)
        ));
        let payload = HostValue::Object(BTreeMap::from([
            ("left".into(), HostValue::Bool(true)),
            ("button_down".into(), HostValue::Bool(false)),
        ]));
        assert!(matches!(
            h.call(&request("SetSimView", payload, false)),
            HostReply::Value(HostValue::Undefined)
        ));
        let ActorEvent::Action(a) = rx.recv().unwrap() else {
            panic!()
        };
        assert_eq!(
            a.action,
            EditorAction::SetSimView {
                left: true,
                button_down: false
            }
        );
        assert!(matches!(
            h.call(&request("CasNavigationRedo", HostValue::Null, true)),
            HostReply::Unavailable(_)
        ));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn unavailable_context_and_conflicting_static_sources_are_visible() {
        let mut s = snapshot();
        s.ready = false;
        assert_eq!(
            missing_context(&s, &BTreeSet::new(), &BTreeSet::new(), None).unwrap(),
            ["verified editor context"]
        );
        let (h, _) = host(s);
        assert!(matches!(
            h.call(&request("CASGetSimInfo", HostValue::Null, false)),
            HostReply::Unavailable(_)
        ));
        assert!(
            missing_context(
                &snapshot(),
                &BTreeSet::new(),
                &BTreeSet::from(["CASGetSimInfo".into()]),
                None
            )
            .unwrap_err()
            .contains("conflict")
        );
        assert_eq!(
            missing_context(
                &snapshot(),
                &BTreeSet::from(["GetFamilyInfo".into()]),
                &BTreeSet::new(),
                None
            )
            .unwrap(),
            ["GetFamilyInfo"]
        );
    }
}
