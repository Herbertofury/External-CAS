//! CAS-oriented native layout, grounded in the preserved EA-control captures.
//! Vector navigation marks are authored here. Game artwork/fonts are not bundled.
//! This layout does not claim room, full-Sim or native-game appearance parity.
use crate::{library, Studio, Tab};
use eframe::egui::{self, pos2, vec2, Color32, FontId, Id, Order, Pos2, Rect, Sense, Stroke};
use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    sync::{
        atomic::AtomicBool,
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

/// Installed source UI resources, prepared off the UI thread. The source pins
/// describe accepted original bytes; failed/superseded loads never relabel them.
pub struct InstalledUiState {
    pub loading: bool,
    pub status: String,
    pub issues: Vec<String>,
    pub pins: Vec<external_cas_fidelity::installed::SourcePin>,
    pub provider: Option<PathBuf>,
    receiver: Option<Receiver<Result<PreparedUi, String>>>,
    cancel: Arc<AtomicBool>,
    textures: Vec<egui::TextureHandle>,
}
struct PreparedUi {
    provider: PathBuf,
    fonts: Vec<(String, Vec<u8>)>,
    atlases: Vec<external_cas_fidelity::installed::InstalledAtlas>,
    pins: Vec<external_cas_fidelity::installed::SourcePin>,
    issues: Vec<String>,
}
impl InstalledUiState {
    pub fn new() -> Self {
        Self {
            loading: false,
            status: "Installed UI resources have not been loaded".into(),
            issues: Vec::new(),
            pins: Vec::new(),
            provider: None,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            textures: Vec::new(),
        }
    }
    pub fn is_loading(&self) -> bool {
        self.loading
    }
    pub fn load(&mut self, ctx: &egui::Context, provider: PathBuf) {
        self.start_load(ctx, provider, None);
    }
    pub fn load_pinned(
        &mut self,
        ctx: &egui::Context,
        provider: PathBuf,
        expected_pins: Vec<external_cas_fidelity::installed::SourcePin>,
    ) {
        self.start_load(ctx, provider, Some(expected_pins));
    }
    fn start_load(
        &mut self,
        ctx: &egui::Context,
        provider: PathBuf,
        expected_pins: Option<Vec<external_cas_fidelity::installed::SourcePin>>,
    ) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let (send, receive) = mpsc::channel();
        self.receiver = Some(receive);
        self.loading = true;
        self.provider = None;
        self.pins.clear();
        self.issues.clear();
        self.textures.clear();
        self.status = "Reading installed CAS UI resources…".into();
        ctx.set_fonts(ui_font_definitions());
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = prepare_installed_ui(provider, &cancel).and_then(|prepared| {
                if let Some(expected) = &expected_pins {
                    external_cas_fidelity::installed::verify_pin_set(expected, &prepared.pins)?;
                }
                Ok(prepared)
            });
            let _ = send.send(result);
            wake.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let result = self.receiver.as_ref().map(|r| r.try_recv());
        match result {
            Some(Ok(Ok(prepared))) => {
                self.loading = false;
                self.receiver = None;
                let mut definitions = ui_font_definitions();
                let mut body = None;
                for (alias, bytes) in prepared.fonts {
                    let name = format!("installed-{alias}");
                    definitions
                        .font_data
                        .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
                    definitions.families.insert(
                        egui::FontFamily::Name(alias.clone().into()),
                        vec![name.clone()],
                    );
                    if alias == "$BodyFont" {
                        body = Some(name);
                    }
                }
                if let Some(body) = body {
                    definitions
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .insert(0, body);
                }
                let font_count = definitions
                    .font_data
                    .len()
                    .saturating_sub(ui_font_definitions().font_data.len());
                ctx.set_fonts(definitions);
                self.textures = prepared
                    .atlases
                    .into_iter()
                    .enumerate()
                    .map(|(i, atlas)| {
                        ctx.load_texture(
                            format!("installed-CAS-atlas-{i}"),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [atlas.dimensions[0] as usize, atlas.dimensions[1] as usize],
                                &atlas.rgba,
                            ),
                            egui::TextureOptions::LINEAR,
                        )
                    })
                    .collect();
                self.status = format!(
                    "Installed UI: {font_count} original fonts, {} decoded atlases",
                    self.textures.len()
                );
                self.provider = Some(prepared.provider);
                self.pins = prepared.pins;
                self.issues = prepared.issues;
            }
            Some(Ok(Err(error))) => {
                self.loading = false;
                self.receiver = None;
                self.status = "Installed UI source could not be loaded".into();
                self.issues.push(error);
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.loading = false;
                self.receiver = None;
                self.status = "Installed UI worker stopped".into();
                self.issues
                    .push("UI resource worker disconnected before a result".into());
            }
            _ => {}
        }
    }
}
impl Drop for InstalledUiState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

// egui applies a new font set at the start of its next pass. Every name must
// therefore already exist in the pending-load fallback set, including reloads.
// This is an explicit loading fallback; it is never reported as installed art.
fn ui_font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for alias in [
        "$HeadlineFont",
        "$BoldHeadlineFont",
        "$TextFont",
        "$SubtitleFont",
        "$BodyFont",
        "Sims4Symbols",
        "ChineseTraditionalFont",
        "KoreanFont",
        "JapaneseFont",
    ] {
        fonts
            .families
            .insert(egui::FontFamily::Name(alias.into()), fallback.clone());
    }
    fonts
}

fn prepare_installed_ui(provider: PathBuf, cancel: &AtomicBool) -> Result<PreparedUi, String> {
    use external_cas_fidelity::{
        font::CompactedFont,
        installed::{load_atlas, load_movie},
        truetype,
    };
    let index = external_cas_assets::PackageIndex::open_cancellable(&provider, cancel)
        .map_err(|e| e.to_string())?;
    let mut prepared = PreparedUi {
        provider,
        fonts: Vec::new(),
        atlases: Vec::new(),
        pins: Vec::new(),
        issues: Vec::new(),
    };
    for name in ["gfxfontlib", "CASLibrary", "ComponentLibrary"] {
        if cancel.load(Ordering::Relaxed) {
            return Err("Superseded installed UI load".into());
        }
        let loaded = load_movie(&index, name)?;
        let movie = loaded.inspect()?;
        for header in &movie.compacted_font_headers {
            if cancel.load(Ordering::Relaxed) {
                return Err("Superseded installed UI load".into());
            }
            let alias = movie
                .symbol_classes
                .iter()
                .find(|s| s.id == header.id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| header.first_font_name.clone());
            let font = CompactedFont::open(&movie, header).map_err(|e| e.to_string())?;
            let converted = truetype::convert(&font)?;
            prepared.fonts.push((alias, converted.bytes));
        }
        for image in &movie.external_images {
            match load_atlas(&index, image) {
                Ok(atlas) => {
                    prepared.pins.push(atlas.pin.clone());
                    prepared.atlases.push(atlas);
                }
                Err(error) => prepared.issues.push(format!("{name}: {error}")),
            }
        }
        prepared.pins.push(loaded.pin);
    }
    index
        .revalidate(cancel)
        .map_err(|error| error.to_string())?;
    prepared.issues.push("Installed font outlines are reused; display-list behavior, atlas symbol mapping and full CAS visual parity remain under validation.".into());
    Ok(prepared)
}

const INK: Color32 = Color32::from_rgb(35, 79, 119);
const WHITE: Color32 = Color32::from_rgb(245, 253, 255);
const PALE: Color32 = Color32::from_rgba_premultiplied(156, 187, 201, 222);
const GREEN: Color32 = Color32::from_rgb(157, 222, 63);

#[derive(Clone, Copy)]
enum Mark {
    Person,
    Hanger,
    Hair,
    Outfit,
    Top,
    Bottom,
    Shoes,
    Play,
    Pause,
    Folder,
    Left,
    Right,
    Undo,
    Redo,
    Front,
    Close,
}

/// One full-bleed viewport render, followed by floating CAS controls. The caller
/// retains async draining, pose updates, persistence, diagnostics and close logic.
pub fn show(studio: &mut Studio, root_ui: &mut egui::Ui) {
    studio.viewport.framing_offset = -0.15;
    let ctx = root_ui.ctx().clone();
    let mut canvas = Rect::NOTHING;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(root_ui, |ui| {
            canvas = ui.max_rect();
            let pixels = ui.ctx().pixels_per_point();
            studio.viewport.render([
                (canvas.width() * pixels) as u32,
                (canvas.height() * pixels) as u32,
            ]);
            let response = ui.allocate_rect(canvas, Sense::drag());
            ui.painter().image(
                studio.viewport.texture_id,
                canvas,
                Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
                Color32::WHITE,
            );
            if response.dragged() {
                let delta = response.drag_delta();
                studio.viewport.camera.yaw += delta.x * 0.007;
                studio.viewport.camera.pitch =
                    (studio.viewport.camera.pitch + delta.y * 0.005).clamp(-1.3, 1.3);
                ctx.request_repaint();
            }
            if response.drag_stopped() {
                studio.mark_session_dirty();
            }
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0. {
                    studio.viewport.camera.distance = (studio.viewport.camera.distance
                        * (-scroll * 0.002).exp())
                    .clamp(0.02, 100.);
                    studio.mark_session_dirty();
                    ctx.request_repaint();
                }
            }
        });
    if !canvas.is_positive() {
        return;
    }
    let scale = (canvas.height() / 850.).clamp(0.78, 1.35);
    let household = household(studio);
    let current_sim = studio.slot.and_then(|index| {
        studio
            .draft
            .as_ref()?
            .slots()
            .ok()?
            .get(index)
            .map(|slot| slot.sim_index)
    });
    let active = current_sim
        .and_then(|index| household.get(index))
        .or_else(|| household.first());
    let name = active
        .map(|sim| sim.name.clone())
        .unwrap_or_else(|| "YOUR SIMS".into());
    let detail = active
        .map(|sim| sim.detail.clone())
        .unwrap_or_else(|| "Open a preserved household".into());

    area(
        &ctx,
        "cas-name",
        canvas.min + vec2(12., 10.),
        canvas,
        |ui| {
            skin(ui);
            ui.set_min_width(220. * scale);
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(244, 251, 255, 244))
                .corner_radius(22)
                .inner_margin(egui::Margin::symmetric(15, 9))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let response = round_button(
                            ui,
                            Mark::Person,
                            false,
                            true,
                            32. * scale,
                            "Browse native Sims",
                        );
                        if response.clicked() {
                            change_tab(studio, Tab::Sims);
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(name.to_uppercase())
                                    .family(if studio.ui_assets.provider.is_some() {
                                        egui::FontFamily::Name("$HeadlineFont".into())
                                    } else {
                                        egui::FontFamily::Proportional
                                    })
                                    .color(INK)
                                    .size(15. * scale),
                            );
                            ui.label(egui::RichText::new(detail).color(INK).size(10.5 * scale));
                        });
                    });
                });
        },
    );

    area(
        &ctx,
        "cas-sections",
        pos2(canvas.right() - 214. * scale, canvas.top() + 12.),
        canvas,
        |ui| {
            skin(ui);
            ui.horizontal(|ui| {
                for (tab, mark, label) in [
                    (Tab::Library, Mark::Hanger, "Wardrobe"),
                    (Tab::Sims, Mark::Person, "Native households"),
                    (Tab::Poses, Mark::Play, "Poses and animation"),
                    (Tab::Project, Mark::Folder, "Project and recovery"),
                ] {
                    if round_button(ui, mark, studio.tab == tab, true, 40. * scale, label).clicked()
                    {
                        change_tab(studio, tab);
                    }
                }
            });
        },
    );

    let panel_width = (canvas.width() * 0.225).clamp(265., 370.);
    let panel_x = (canvas.center().x + 180. * scale).min(canvas.right() - panel_width - 15.);
    let panel_top = canvas.top() + 72. * scale;
    let panel_height = (canvas.height() - panel_top + canvas.top() - 116. * scale).max(240.);
    area(
        &ctx,
        "cas-catalog",
        pos2(panel_x, panel_top),
        canvas,
        |ui| {
            skin(ui);
            egui::Frame::new()
                .fill(PALE)
                .stroke(Stroke::new(
                    1.,
                    Color32::from_rgba_unmultiplied(242, 254, 255, 155),
                ))
                .corner_radius(9)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(panel_width - 20.);
                    ui.set_max_width(panel_width - 20.);
                    ui.set_min_height(panel_height);
                    ui.set_max_height(panel_height);
                    match studio.tab {
                        Tab::Library => studio.library_panel(ui),
                        Tab::Sims => studio.sims_panel(ui),
                        Tab::Poses => studio.poses_panel(ui),
                        Tab::Project => studio.project_panel(ui),
                    }
                });
        },
    );

    if studio.tab == Tab::Library {
        let arc_x = (panel_x - 62. * scale).max(canvas.center().x + 70. * scale);
        let arc_y = canvas.top() + canvas.height() * 0.33;
        // These controls perform real catalog filtering. Unsupported body/face
        // editing and trait controls are deliberately not represented as working.
        for (n, (body, mark, label)) in [
            (2, Mark::Hair, "Hair"),
            (5, Mark::Outfit, "Full-body outfits"),
            (6, Mark::Top, "Tops"),
            (7, Mark::Bottom, "Bottoms"),
            (8, Mark::Shoes, "Shoes"),
        ]
        .into_iter()
        .enumerate()
        {
            let curve = (n as f32 - 2.).powi(2) * -4.5 * scale;
            area(
                &ctx,
                Id::new(("cas-category", body)),
                pos2(arc_x + curve, arc_y + n as f32 * 48. * scale),
                canvas,
                |ui| {
                    skin(ui);
                    if round_button(
                        ui,
                        mark,
                        studio.body == Some(body),
                        !studio.scanning,
                        39. * scale,
                        label,
                    )
                    .clicked()
                    {
                        set_body(studio, Some(body));
                    }
                },
            );
        }
        area(
            &ctx,
            "cas-all",
            pos2(arc_x - 6. * scale, arc_y + 245. * scale),
            canvas,
            |ui| {
                skin(ui);
                if ui
                    .add(egui::Button::new(egui::RichText::new("All").color(INK)).corner_radius(18))
                    .clicked()
                {
                    set_body(studio, None);
                }
            },
        );
    }

    area(
        &ctx,
        "cas-household",
        pos2(canvas.left() + 12., canvas.bottom() - 67. * scale),
        canvas,
        |ui| {
            skin(ui);
            ui.horizontal(|ui| {
                for sim in &household {
                    let selected = active.is_some_and(|active| active.index == sim.index);
                    let response = ui.add_sized(
                        [39. * scale, 44. * scale],
                        egui::Button::new(
                            egui::RichText::new(&sim.initials)
                                .size(14. * scale)
                                .strong()
                                .color(INK),
                        )
                        .fill(if selected { GREEN } else { PALE })
                        .corner_radius(11),
                    );
                    if response
                        .clone()
                        .on_hover_text(format!("{} · choose an actual outfit slot", sim.name))
                        .clicked()
                    {
                        studio.slot = sim.first_slot;
                        change_tab(studio, Tab::Project);
                    }
                }
                if round_button(
                    ui,
                    Mark::Person,
                    false,
                    true,
                    40. * scale,
                    "Browse or import a preserved household",
                )
                .clicked()
                {
                    change_tab(studio, Tab::Sims);
                }
            });
        },
    );

    let controls_x = (canvas.center().x - 147. * scale).min(panel_x - 290. * scale);
    area(
        &ctx,
        "cas-rotate-left",
        pos2(controls_x - 30. * scale, canvas.bottom() - 115. * scale),
        canvas,
        |ui| {
            skin(ui);
            if round_button(ui, Mark::Left, false, true, 49. * scale, "Rotate left").clicked() {
                studio.viewport.camera.yaw -= std::f32::consts::FRAC_PI_8;
                studio.mark_session_dirty();
            }
        },
    );
    area(
        &ctx,
        "cas-rotate-right",
        pos2(controls_x + 252. * scale, canvas.bottom() - 115. * scale),
        canvas,
        |ui| {
            skin(ui);
            if round_button(ui, Mark::Right, false, true, 49. * scale, "Rotate right").clicked() {
                studio.viewport.camera.yaw += std::f32::consts::FRAC_PI_8;
                studio.mark_session_dirty();
            }
        },
    );
    area(
        &ctx,
        "cas-edit-actions",
        pos2(controls_x, canvas.bottom() - 57. * scale),
        canvas,
        |ui| {
            skin(ui);
            egui::Frame::new()
                .fill(PALE)
                .corner_radius(28)
                .inner_margin(egui::Margin::symmetric(9, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let can_undo = !studio.working_project
                            && studio.draft.as_ref().is_some_and(|draft| draft.can_undo());
                        let can_redo = !studio.working_project
                            && studio.draft.as_ref().is_some_and(|draft| draft.can_redo());
                        if round_button(
                            ui,
                            Mark::Undo,
                            false,
                            can_undo,
                            35. * scale,
                            "Undo the last native draft edit",
                        )
                        .clicked()
                        {
                            studio.start_draft_change(library::DraftChange::Undo);
                        }
                        if round_button(
                            ui,
                            Mark::Redo,
                            false,
                            can_redo,
                            35. * scale,
                            "Redo the native draft edit",
                        )
                        .clicked()
                        {
                            studio.start_draft_change(library::DraftChange::Redo);
                        }
                        if round_button(ui, Mark::Front, false, true, 35. * scale, "Front view")
                            .clicked()
                        {
                            studio.viewport.camera.yaw = 0.;
                            studio.viewport.camera.pitch = 0.08;
                            studio.mark_session_dirty();
                        }
                        if round_button(
                            ui,
                            if studio.playing {
                                Mark::Pause
                            } else {
                                Mark::Play
                            },
                            studio.playing,
                            studio.clip.is_some() && studio.skin_bound,
                            35. * scale,
                            "Play or pause the selected source pose",
                        )
                        .clicked()
                        {
                            studio.playing = !studio.playing;
                            studio.pose_reset = false;
                            studio.playback_clock = Instant::now();
                            studio.mark_session_dirty();
                        }
                    });
                });
        },
    );

    area(
        &ctx,
        "cas-project-actions",
        pos2(canvas.right() - 112. * scale, canvas.bottom() - 66. * scale),
        canvas,
        |ui| {
            skin(ui);
            ui.horizontal(|ui| {
                if round_button(
                    ui,
                    Mark::Close,
                    false,
                    true,
                    42. * scale,
                    "Close the editor after recovery and session writes finish",
                )
                .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if round_button(
                    ui,
                    Mark::Folder,
                    studio.tab == Tab::Project,
                    true,
                    44. * scale,
                    "Open project saving and verified recovery",
                )
                .clicked()
                {
                    change_tab(studio, Tab::Project);
                }
            });
        },
    );

    area(
        &ctx,
        "cas-status",
        pos2(canvas.left() + 12., canvas.bottom() - 21.),
        canvas,
        |ui| {
            ui.horizontal(|ui| {
                if studio.scanning
                    || studio.preparing
                    || studio.working_project
                    || studio.session.loading
                    || studio.session.saving
                    || studio.ui_assets.loading
                {
                    ui.spinner();
                }
                ui.label(egui::RichText::new(&studio.status).size(11.).color(WHITE));
            });
        },
    );
    // Keep the current scope honest while the full native Sim/room pipeline is
    // being qualified. It never substitutes an invented avatar or creator item.
    area(
        &ctx,
        "cas-fidelity-state",
        pos2(canvas.left() + 15., canvas.top() + 87. * scale),
        canvas,
        |ui| {
            ui.set_max_width(245. * scale);
            ui.label(
                egui::RichText::new(if studio.vertices > 0 {
                    "Original appearance item preview"
                } else {
                    "No Sim appearance has been rendered"
                })
                .size(11. * scale)
                .color(WHITE),
            );
            ui.label(
                egui::RichText::new("Native room, full Sim and game appearance remain unverified.")
                    .size(9.5 * scale)
                    .color(Color32::from_white_alpha(200)),
            );
            ui.label(
                egui::RichText::new(&studio.ui_assets.status)
                    .size(9.5 * scale)
                    .color(WHITE),
            );
            if ui
                .add_enabled(
                    !studio.ui_assets.loading,
                    egui::Button::new("Installed UI source…").small(),
                )
                .clicked()
            {
                if let Some(provider) = rfd::FileDialog::new()
                    .set_title("Select the installed game's UI.package (read-only)")
                    .add_filter("Sims resource package", &["package"])
                    .pick_file()
                {
                    studio.ui_assets.load(&ctx, provider);
                }
            }
            if !studio.ui_assets.issues.is_empty() {
                egui::CollapsingHeader::new("Installed UI compatibility")
                    .default_open(false)
                    .show(ui, |ui| {
                        for issue in &studio.ui_assets.issues {
                            ui.label(egui::RichText::new(issue).size(9.5 * scale).color(WHITE));
                        }
                    });
            }
        },
    );
    if !studio.errors.is_empty() || !studio.warnings.is_empty() {
        area(
            &ctx,
            "cas-compatibility",
            pos2(canvas.left() + 15., canvas.top() + 210. * scale),
            canvas,
            |ui| {
                skin(ui);
                ui.set_max_width(240. * scale);
                egui::CollapsingHeader::new(format!(
                    "Content status · {} issues",
                    studio.errors.len() + studio.warnings.len()
                ))
                .default_open(!studio.errors.is_empty())
                .show(ui, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(230, 242, 248, 225))
                        .corner_radius(8)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            for error in studio.errors.iter().rev().take(4) {
                                ui.label(
                                    egui::RichText::new(error)
                                        .color(Color32::from_rgb(151, 42, 42))
                                        .small(),
                                );
                            }
                            for warning in studio.warnings.iter().rev().take(3) {
                                ui.label(egui::RichText::new(warning).color(INK).small());
                            }
                        });
                });
            },
        );
    }
}

fn area(
    ctx: &egui::Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    position: Pos2,
    canvas: Rect,
    contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Area::new(Id::new(id))
        .order(Order::Foreground)
        .fixed_pos(position)
        .movable(false)
        .constrain_to(canvas)
        .show(ctx, contents);
}
fn skin(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    *visuals = egui::Visuals::light();
    visuals.override_text_color = Some(INK);
    visuals.panel_fill = PALE;
    visuals.window_fill = PALE;
    visuals.selection.bg_fill = GREEN;
    visuals.selection.stroke = Stroke::new(1., Color32::from_rgb(74, 138, 38));
    visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(236, 247, 252, 190);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(220, 236, 247, 190);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1., INK);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1., Color32::from_white_alpha(170));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(224, 250, 224);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1., INK);
    visuals.widgets.active.bg_fill = GREEN;
    visuals.widgets.active.fg_stroke = Stroke::new(1., INK);
    ui.spacing_mut().item_spacing = vec2(6., 6.);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(18.));
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(12.));
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(12.));
}
fn change_tab(studio: &mut Studio, tab: Tab) {
    studio.tab = tab;
    studio.session.state.ui_tab = tab.name().into();
    studio.mark_session_dirty();
}
fn set_body(studio: &mut Studio, body: Option<u32>) {
    studio.body = body;
    studio.tab = Tab::Library;
    studio.search_cancel.store(true, Ordering::Relaxed);
    studio.search_gen += 1;
    studio.searching = true;
    studio.query_changed = Some(Instant::now() - Duration::from_millis(200));
    studio.session.state.ui_tab = Tab::Library.name().into();
    studio.mark_session_dirty();
}

struct HouseholdMember {
    index: usize,
    name: String,
    initials: String,
    detail: String,
    first_slot: Option<usize>,
}
fn household(studio: &Studio) -> Vec<HouseholdMember> {
    let Some(household) = studio
        .project
        .as_ref()
        .and_then(|project| project.household())
    else {
        return Vec::new();
    };
    let slots = studio
        .draft
        .as_ref()
        .and_then(|draft| draft.slots().ok())
        .unwrap_or_default();
    household
        .sims
        .iter()
        .enumerate()
        .map(|(index, sim)| {
            let name = format!("{} {}", sim.first_name, sim.last_name)
                .trim()
                .to_owned();
            let initials = format!(
                "{}{}",
                sim.first_name.chars().next().unwrap_or(' '),
                sim.last_name.chars().next().unwrap_or(' ')
            );
            let age = match sim.age {
                Some(1) => "Baby",
                Some(2) => "Toddler",
                Some(4) => "Child",
                Some(8) => "Teen",
                Some(16) => "Young adult",
                Some(32) => "Adult",
                Some(64) => "Elder",
                Some(128) => "Infant",
                _ => "Age retained",
            };
            let gender = match sim.gender {
                Some(0x1000) => "Male",
                Some(0x2000) => "Female",
                Some(0x3000) => "Unisex",
                _ => "Gender retained",
            };
            HouseholdMember {
                index,
                name,
                initials,
                detail: format!("{age} · {gender}"),
                first_slot: slots.iter().position(|slot| slot.sim_index == index),
            }
        })
        .collect()
}
fn round_button(
    ui: &mut egui::Ui,
    mark: Mark,
    selected: bool,
    enabled: bool,
    size: f32,
    label: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(size, size),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if selected {
        GREEN
    } else if enabled && response.hovered() {
        Color32::from_rgb(218, 249, 222)
    } else {
        PALE
    };
    ui.painter().circle_filled(
        rect.center(),
        size * 0.47,
        fill.gamma_multiply(if enabled { 1. } else { 0.45 }),
    );
    ui.painter().circle_stroke(
        rect.center(),
        size * 0.47,
        Stroke::new(
            1.3,
            Color32::from_white_alpha(if enabled { 220 } else { 90 }),
        ),
    );
    draw_mark(
        ui.painter(),
        rect.shrink(size * 0.23),
        mark,
        if enabled {
            INK
        } else {
            INK.gamma_multiply(0.4)
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response.on_hover_text(label)
}
fn draw_mark(p: &egui::Painter, rect: Rect, mark: Mark, color: Color32) {
    let c = rect.center();
    let w = rect.width();
    let h = rect.height();
    let stroke = Stroke::new((w * 0.09).max(1.4), color);
    let point = |x: f32, y: f32| pos2(rect.left() + x * w, rect.top() + y * h);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let path = |values: &[(f32, f32)], closed: bool| {
        let points = values.iter().map(|&(x, y)| point(x, y)).collect();
        p.add(if closed {
            egui::Shape::closed_line(points, stroke)
        } else {
            egui::Shape::line(points, stroke)
        });
    };
    match mark {
        Mark::Person => {
            p.circle_stroke(point(0.5, 0.21), w * 0.17, stroke);
            path(
                &[
                    (0.15, 0.95),
                    (0.2, 0.65),
                    (0.38, 0.5),
                    (0.62, 0.5),
                    (0.8, 0.65),
                    (0.85, 0.95),
                ],
                false,
            );
        }
        Mark::Hanger => {
            path(
                &[
                    (0.4, 0.21),
                    (0.46, 0.08),
                    (0.63, 0.1),
                    (0.65, 0.23),
                    (0.51, 0.39),
                    (0.51, 0.48),
                ],
                false,
            );
            path(
                &[(0.5, 0.44), (0.05, 0.82), (0.95, 0.82), (0.5, 0.44)],
                false,
            );
        }
        Mark::Hair => {
            path(
                &[
                    (0.12, 0.87),
                    (0.08, 0.43),
                    (0.2, 0.12),
                    (0.5, 0.04),
                    (0.8, 0.13),
                    (0.94, 0.48),
                    (0.87, 0.9),
                ],
                false,
            );
            path(
                &[
                    (0.12, 0.49),
                    (0.4, 0.32),
                    (0.6, 0.16),
                    (0.7, 0.42),
                    (0.9, 0.53),
                ],
                false,
            );
        }
        Mark::Top => path(
            &[
                (0.33, 0.04),
                (0.07, 0.2),
                (0.02, 0.45),
                (0.25, 0.52),
                (0.27, 0.95),
                (0.73, 0.95),
                (0.75, 0.52),
                (0.98, 0.45),
                (0.93, 0.2),
                (0.67, 0.04),
                (0.6, 0.21),
                (0.4, 0.21),
            ],
            true,
        ),
        Mark::Outfit => {
            path(
                &[
                    (0.35, 0.03),
                    (0.18, 0.15),
                    (0.28, 0.43),
                    (0.1, 0.95),
                    (0.9, 0.95),
                    (0.72, 0.43),
                    (0.82, 0.15),
                    (0.65, 0.03),
                    (0.6, 0.2),
                    (0.4, 0.2),
                ],
                true,
            );
            line((0.29, 0.47), (0.71, 0.47));
        }
        Mark::Bottom => path(
            &[
                (0.19, 0.05),
                (0.81, 0.05),
                (0.89, 0.94),
                (0.6, 0.94),
                (0.5, 0.43),
                (0.4, 0.94),
                (0.11, 0.94),
            ],
            true,
        ),
        Mark::Shoes => {
            path(
                &[
                    (0.1, 0.23),
                    (0.3, 0.24),
                    (0.36, 0.6),
                    (0.5, 0.77),
                    (0.5, 0.92),
                    (0.04, 0.92),
                    (0.02, 0.74),
                    (0.1, 0.23),
                ],
                false,
            );
            path(
                &[
                    (0.61, 0.08),
                    (0.83, 0.1),
                    (0.87, 0.43),
                    (0.99, 0.64),
                    (0.99, 0.8),
                    (0.6, 0.8),
                ],
                false,
            );
        }
        Mark::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![point(0.2, 0.05), point(0.92, 0.5), point(0.2, 0.95)],
                color,
                Stroke::NONE,
            ));
        }
        Mark::Pause => {
            p.rect_filled(
                Rect::from_min_max(point(0.15, 0.1), point(0.37, 0.9)),
                1,
                color,
            );
            p.rect_filled(
                Rect::from_min_max(point(0.63, 0.1), point(0.85, 0.9)),
                1,
                color,
            );
        }
        Mark::Folder => path(
            &[
                (0.04, 0.25),
                (0.04, 0.1),
                (0.4, 0.1),
                (0.51, 0.25),
                (0.95, 0.25),
                (0.9, 0.9),
                (0.1, 0.9),
            ],
            true,
        ),
        Mark::Left | Mark::Undo => {
            path(
                &[(0.86, 0.77), (0.7, 0.42), (0.4, 0.25), (0.08, 0.36)],
                false,
            );
            path(&[(0.28, 0.05), (0.04, 0.38), (0.42, 0.51)], false);
        }
        Mark::Right | Mark::Redo => {
            path(
                &[(0.14, 0.77), (0.3, 0.42), (0.6, 0.25), (0.92, 0.36)],
                false,
            );
            path(&[(0.72, 0.05), (0.96, 0.38), (0.58, 0.51)], false);
        }
        Mark::Front => {
            p.circle_stroke(c, w * 0.37, stroke);
            line((0.5, 0.0), (0.5, 0.25));
            line((0.5, 0.75), (0.5, 1.));
            line((0., 0.5), (0.25, 0.5));
            line((0.75, 0.5), (1., 0.5));
        }
        Mark::Close => {
            line((0.18, 0.18), (0.82, 0.82));
            line((0.18, 0.82), (0.82, 0.18));
        }
    }
}

#[cfg(test)]
mod installed_ui_tests {
    use super::*;
    fn draw_named_fonts(ctx: &egui::Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                for alias in [
                    "$HeadlineFont",
                    "$BoldHeadlineFont",
                    "$BodyFont",
                    "Sims4Symbols",
                ] {
                    ui.label(
                        egui::RichText::new("CAS Aa0").family(egui::FontFamily::Name(alias.into())),
                    );
                }
            });
        });
        output.textures_delta.clear();
    }
    #[test]
    fn installed_font_aliases_survive_first_frame_completion_reload_and_failure() {
        let ctx = egui::Context::default();
        let mut state = InstalledUiState::new();
        assert!(!state.loading && state.provider.is_none());
        let missing = std::env::temp_dir().join(format!(
            "external-cas-ui-does-not-exist-{}.package",
            std::process::id()
        ));
        state.load(&ctx, missing.clone());
        draw_named_fonts(&ctx); // pending-load first pass
        let baseline = ui_font_definitions();
        let fallback = baseline.families[&egui::FontFamily::Proportional][0].clone();
        let bytes = baseline.font_data[&fallback].font.to_vec();
        let (send, receive) = mpsc::channel();
        state.receiver = Some(receive);
        send.send(Ok(PreparedUi {
            provider: missing.clone(),
            fonts: vec![("$HeadlineFont".into(), bytes)],
            atlases: Vec::new(),
            pins: Vec::new(),
            issues: Vec::new(),
        }))
        .unwrap();
        state.poll(&ctx);
        assert!(state.provider.is_some() && !state.loading);
        draw_named_fonts(&ctx);
        draw_named_fonts(&ctx); // font set changes on next pass
        state.load(&ctx, missing);
        assert!(state.provider.is_none() && state.pins.is_empty());
        draw_named_fonts(&ctx); // reload must retain named aliases too
        let (send, receive) = mpsc::channel();
        state.receiver = Some(receive);
        send.send(Err("source missing".into())).unwrap();
        state.poll(&ctx);
        assert!(!state.loading && state.provider.is_none());
        assert_eq!(state.issues, ["source missing"]);
        draw_named_fonts(&ctx);
    }
    #[test]
    fn fallback_aliases_always_point_to_present_font_data() {
        let fonts = ui_font_definitions();
        for (family, names) in &fonts.families {
            assert!(!names.is_empty(), "empty family {family:?}");
            for name in names {
                assert!(
                    fonts.font_data.contains_key(name),
                    "unbound {family:?}: {name}"
                );
            }
        }
    }
}
