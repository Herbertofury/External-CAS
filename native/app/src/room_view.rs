//! Actual installed CAS room, loaded on a worker and uploaded by the viewport.
use crate::viewport::{RoomPart, RoomVertex, Viewport};
use eframe::egui;
use external_cas_room::{GeometryProjection, RoomRequest, RoomScene};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
enum Message {
    Progress(String),
    Ready(Result<PreparedRoom, String>),
}
struct PreparedRoom {
    scene: RoomScene,
    tuning: Option<external_cas_room::CasTuningScene>,
    parts: Vec<RoomPart>,
    notes: Vec<String>,
}
pub struct RoomState {
    pub loading: bool,
    pub source: Option<RoomScene>,
    pub tuning: Option<external_cas_room::CasTuningScene>,
    pub active_lighting: Option<u64>,
    pub status: String,
    pub issues: Vec<String>,
    pub requested_root: Option<PathBuf>,
    receiver: Option<mpsc::Receiver<Message>>,
    cancel: Arc<AtomicBool>,
}
impl Default for RoomState {
    fn default() -> Self {
        Self {
            loading: false,
            source: None,
            tuning: None,
            active_lighting: None,
            status: "Choose an installed game source for the native CAS room".into(),
            issues: Vec::new(),
            requested_root: None,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl RoomState {
    pub fn load(&mut self, root: PathBuf, ctx: &egui::Context) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.requested_root = Some(root.clone());
        self.loading = true;
        self.source = None;
        self.tuning = None;
        self.active_lighting = None;
        self.issues.clear();
        self.status = "Reading the installed CAS world and original room model…".into();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let cancel = self.cancel.clone();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let request = RoomRequest::installed_base(root);
                let scene = external_cas_room::load(&request, &cancel, |progress| {
                    let _ = tx.send(Message::Progress(progress.detail));
                    wake.request_repaint();
                })?;
                let origin = scene
                    .world_objects
                    .first()
                    .and_then(|object| object.position())
                    .ok_or("CAS room world origin unavailable")?;
                let mut parts = Vec::new();
                let mut notes = scene
                    .issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.stage, issue.detail))
                    .collect::<Vec<_>>();
                for mesh in &scene.meshes {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Superseded room source".into());
                    }
                    let parameters = mesh.parameters()?;
                    if ![0x14fa335e, 0xfc5fc212].contains(&parameters.shader) {
                        return Err(format!(
                            "Unsupported room shader {:08X}; original material retained",
                            parameters.shader
                        ));
                    }
                    let key = parameters
                        .diffuse_map
                        .ok_or("Native room diffuse map is absent")?;
                    let texture = scene
                        .textures
                        .iter()
                        .find(|texture| texture.source.pin.key == key)
                        .ok_or("Native room diffuse texture was not resolved")?;
                    let (width, height, rgba) = texture
                        .decoded
                        .as_ref()
                        .ok_or("Native room texture cannot be decoded")?;
                    let world = scene
                        .world_objects
                        .get(mesh.object_index)
                        .ok_or("Room object index is stale")?
                        .matrix_relative_to(origin)?;
                    let vertices = mesh
                        .render_vertices(GeometryProjection::SourceMatdParameterCandidate)?
                        .into_iter()
                        .map(|vertex| {
                            let p = vertex.position;
                            let n = vertex.normal;
                            RoomVertex {
                                position: std::array::from_fn(|row| {
                                    world[row] * p[0]
                                        + world[4 + row] * p[1]
                                        + world[8 + row] * p[2]
                                        + world[12 + row]
                                }),
                                normal: std::array::from_fn(|row| {
                                    world[row] * n[0]
                                        + world[4 + row] * n[1]
                                        + world[8 + row] * n[2]
                                }),
                                uv: vertex.uv0,
                                color: vertex
                                    .source_color
                                    .unwrap_or([255; 4])
                                    .map(|v| v as f32 / 255.),
                            }
                        })
                        .collect();
                    let tint = parameters
                        .diffuse
                        .as_ref()
                        .and_then(|v| <[f32; 4]>::try_from(v.as_slice()).ok())
                        .ok_or("Native room diffuse multiplier unavailable")?;
                    let transparency = parameters
                        .transparency
                        .as_ref()
                        .and_then(|v| v.first())
                        .copied()
                        .ok_or("Native room opacity unavailable")?;
                    parts.push(RoomPart {
                        vertices,
                        indices: mesh.geometry.indices.clone(),
                        rgba: rgba.clone(),
                        texture_size: [*width, *height],
                        tint: [tint[0], tint[1], tint[2], tint[3] * transparency],
                    });
                    if parameters.shader == 0xfc5fc212 {
                        notes.push(format!("Original alpha-plane pixels are rendered with alpha blending; source threshold {:?}, Phong response and contact-shadow semantics require game comparison",parameters.alpha_mask_threshold));
                    }
                }
                let tuning = match external_cas_room::load_cas_tuning(&request, &cancel, |p| {
                    let _ = tx.send(Message::Progress(format!(
                        "Reading installed CAS camera and lighting · {}",
                        p.stage
                    )));
                    wake.request_repaint();
                }) {
                    Ok(tuning) => Some(tuning),
                    Err(error) => {
                        notes.push(format!(
                            "Installed CAS camera/lighting tuning unavailable: {error}"
                        ));
                        None
                    }
                };
                Ok(PreparedRoom {
                    scene,
                    tuning,
                    parts,
                    notes,
                })
            })();
            let _ = tx.send(Message::Ready(result));
            wake.request_repaint();
        });
    }
    pub fn poll(&mut self, viewport: &mut Viewport) {
        loop {
            let message = match self.receiver.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(message)) => message,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    self.loading = false;
                    self.receiver = None;
                    self.status = "Installed room worker stopped".into();
                    self.issues
                        .push("The room worker disconnected before publication".into());
                    break;
                }
                _ => break,
            };
            match message {
                Message::Progress(message) => self.status = message,
                Message::Ready(result) => {
                    self.loading = false;
                    self.receiver = None;
                    match result {
                        Ok(room) => match viewport.set_room(room.parts) {
                            Ok(()) => {
                                self.status = format!(
                                    "Original CAS room · {} meshes · {} textures",
                                    room.scene.meshes.len(),
                                    room.scene.textures.len()
                                );
                                self.source = Some(room.scene);
                                self.tuning = room.tuning;
                                self.issues = room.notes;
                                if let Some(tuning) = &self.tuning {
                                    let lights = tuning
                                        .lighting
                                        .iter()
                                        .filter(|l| l.name == "Default")
                                        .collect::<Vec<_>>();
                                    if let [light] = lights.as_slice() {
                                        match viewport.set_installed_lighting(light) {
                                            Ok(()) => {
                                                self.active_lighting = Some(light.instance);
                                                self.issues.push("Original CAS light values loaded; direction, material response and shadow equations require shader/game comparison".into());
                                            }
                                            Err(error) => self.issues.push(error),
                                        }
                                    }
                                }
                            }
                            Err(error) => self.issues.push(error),
                        },
                        Err(error) => {
                            self.status = "Installed CAS room could not be loaded".into();
                            self.issues.push(error);
                        }
                    }
                }
            }
        }
    }
}
impl Drop for RoomState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub fn installation_from_ui(provider: &std::path::Path) -> Option<PathBuf> {
    let client = provider.parent()?;
    let data = client.parent()?;
    if !client
        .file_name()?
        .to_string_lossy()
        .eq_ignore_ascii_case("Client")
        || !data
            .file_name()?
            .to_string_lossy()
            .eq_ignore_ascii_case("Data")
    {
        return None;
    }
    data.parent().map(std::path::Path::to_path_buf)
}
