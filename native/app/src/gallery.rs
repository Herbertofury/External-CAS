//! Visible-only native thumbnail loading. Package I/O and image decoding never
//! run in the UI. The single worker cancels obsolete requests; images stay local.
use crate::library::{GalleryGroup, Library};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, StrokeKind, TextureHandle, pos2, vec2};
use external_cas_assets::{
    ReadLimits,
    thumbnail::{ThumbnailCandidate, ThumbnailModelHint},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    index: usize,
    model: Option<bool>,
}
struct Request {
    epoch: u64,
    serial: u64,
    library: Arc<Library>,
    keys: Vec<Key>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}
struct Decoded {
    key: Key,
    result: Result<(egui::ColorImage, bool, String), String>,
}
struct Batch {
    epoch: u64,
    serial: u64,
    rows: Vec<Decoded>,
    elapsed_ms: f64,
}
struct CardImage {
    texture: Option<TextureHandle>,
    note: String,
    design_only: bool,
    used: u64,
    bytes: usize,
}
pub struct Gallery {
    scroll_offset: Option<f32>,
    tx: mpsc::Sender<Request>,
    rx: mpsc::Receiver<Batch>,
    cancel: Arc<AtomicBool>,
    epoch: u64,
    serial: u64,
    requested: Vec<Key>,
    cache: HashMap<Key, CardImage>,
    frame: u64,
    pub columns: usize,
    pub model: Option<bool>,
    pub pending: bool,
    pub loaded: usize,
    pub unavailable: usize,
    pub batch_ms: f64,
}
impl Gallery {
    pub fn new() -> Self {
        let (tx, requests) = mpsc::channel::<Request>();
        let (results, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(mut request) = requests.recv() {
                while let Ok(next) = requests.try_recv() {
                    request = next;
                }
                if request.cancel.load(Ordering::Relaxed) {
                    continue;
                }
                let start = Instant::now();
                let rows = decode_batch(&request);
                if !request.cancel.load(Ordering::Relaxed) {
                    let _ = results.send(Batch {
                        epoch: request.epoch,
                        serial: request.serial,
                        rows,
                        elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
                    });
                    request.ctx.request_repaint();
                }
            }
        });
        Self {
            scroll_offset: None,
            tx,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
            epoch: 0,
            serial: 0,
            requested: Vec::new(),
            cache: HashMap::new(),
            frame: 0,
            columns: 2,
            model: None,
            pending: false,
            loaded: 0,
            unavailable: 0,
            batch_ms: 0.,
        }
    }
    pub fn reset(&mut self, epoch: u64) {
        self.scroll_offset = None;
        self.cancel.store(true, Ordering::Relaxed);
        self.epoch = epoch;
        self.serial += 1;
        self.requested.clear();
        self.cache.clear();
        self.pending = false;
        self.loaded = 0;
        self.unavailable = 0;
    }
    pub fn profile_scroll(&mut self, frame: usize) {
        self.scroll_offset = Some(frame as f32 * 18.);
    }
    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(batch) = self.rx.try_recv() {
            if batch.epoch != self.epoch || batch.serial != self.serial {
                continue;
            }
            self.pending = false;
            self.batch_ms = batch.elapsed_ms;
            for row in batch.rows {
                let entry = match row.result {
                    Ok((image, design_only, note)) => {
                        let bytes = image.pixels.len() * 4;
                        let texture = ctx.load_texture(
                            format!(
                                "local-thumbnail-{}-{}-{:?}",
                                self.epoch, row.key.index, row.key.model
                            ),
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.loaded += 1;
                        CardImage {
                            texture: Some(texture),
                            note,
                            design_only,
                            used: self.frame,
                            bytes,
                        }
                    }
                    Err(note) => {
                        self.unavailable += 1;
                        CardImage {
                            texture: None,
                            note,
                            design_only: false,
                            used: self.frame,
                            bytes: 0,
                        }
                    }
                };
                self.cache.insert(row.key, entry);
            }
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        library: Arc<Library>,
        groups: &[GalleryGroup],
        selected: Option<usize>,
        enabled: bool,
    ) -> Option<usize> {
        self.frame += 1;
        self.drain(ui.ctx());
        let mut choice = None;
        let model_before = self.model;
        egui::ComboBox::from_id_salt("thumbnail_model")
            .selected_text(match self.model {
                None => "Source thumbnail model",
                Some(false) => "Female thumbnail model",
                Some(true) => "Male thumbnail model",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.model, None, "Source thumbnail model");
                ui.selectable_value(&mut self.model, Some(false), "Female thumbnail model");
                ui.selectable_value(&mut self.model, Some(true), "Male thumbnail model");
            });
        if self.model != model_before {
            self.requested.clear();
        }
        let columns = self.columns;
        let gap = 6.;
        let width = ((ui.available_width() - gap * (columns - 1) as f32) / columns as f32).max(36.);
        let height = width * 1.18;
        let mut visible = Vec::new();
        let footer_height = if selected.is_some() { 88. } else { 32. };
        let mut scroll = egui::ScrollArea::vertical();
        if let Some(offset) = self.scroll_offset {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        scroll.id_salt("native_wardrobe_grid").max_height((ui.available_height()-footer_height).max(60.)).auto_shrink([false,false]).show_rows(ui,height+gap,groups.len().div_ceil(columns),|ui,range|{
            for row in range {
                ui.horizontal(|ui|{
                    ui.spacing_mut().item_spacing.x=gap;
                    for col in 0..columns {
                        let Some(group)=groups.get(row*columns+col)else{break;};
                        let index=selected.filter(|i|group.items.contains(i)).unwrap_or(group.items[0]);
                        let item=&library.items[index];
                        let key=Key{index,model:self.model}; visible.push(key);
                        let (rect,response)=ui.allocate_exact_size(vec2(width,height),Sense::click());
                        let active=selected.is_some_and(|i|group.items.contains(&i));
                        ui.painter().rect_filled(rect,3,Color32::from_rgb(218,233,239));
                        let image_rect=Rect::from_min_max(rect.min+vec2(3.,3.),rect.max-vec2(3.,25.));
                        let mut note=String::from("Reading installed thumbnail…");
                        let mut design_only=false;
                        if let Some(image)=self.cache.get_mut(&key){
                            image.used=self.frame;note=image.note.clone();design_only=image.design_only;
                            if let Some(texture)=&image.texture {
                                let fit=image_rect.size();
                                let scale=(fit.x/texture.size()[0] as f32).min(fit.y/texture.size()[1] as f32);
                                let size=vec2(texture.size()[0] as f32*scale,texture.size()[1] as f32*scale);
                                let dest=Rect::from_center_size(image_rect.center(),size);
                                ui.painter().image(texture.id(),dest,Rect::from_min_max(pos2(0.,0.),pos2(1.,1.)),Color32::WHITE);
                            }else{
                                ui.painter().text(image_rect.center(),egui::Align2::CENTER_CENTER,"No qualified\nthumbnail",egui::FontId::proportional(11.),Color32::from_rgb(56,85,101));
                            }
                        }
                        if design_only{ui.painter().text(image_rect.left_top()+vec2(3.,3.),egui::Align2::LEFT_TOP,"DESIGN",egui::FontId::proportional(9.),Color32::from_rgb(32,74,105));}
                        let label=if group.items.len()>1 {format!("{} swatches",group.items.len())}else{crate::library::body_label(item.body_type)};
                        ui.painter().text(pos2(rect.center().x,rect.bottom()-13.),egui::Align2::CENTER_CENTER,label,egui::FontId::proportional(10.),Color32::from_rgb(35,79,119));
                        let stroke=if active{Stroke::new(3.,Color32::from_rgb(117,190,37))}else if response.hovered(){Stroke::new(2.,Color32::WHITE)}else{Stroke::NONE};
                        ui.painter().rect_stroke(rect,3,stroke,StrokeKind::Inside);
                        if response.clicked()&&enabled{choice=Some(index);}
                        response.on_hover_ui(|ui|{
                            ui.strong(&item.name);ui.label(&item.source_label);ui.label(item.key.to_string());
                            ui.label(format!("Age mask {:02X} · gender mask {:02X}",item.age,item.gender));
                            ui.label(note);
                            if group.items.len()>1 {
                                ui.label("Choose this design to show its source swatches below the catalog.");
                            }
                        });
                    }
                });
            }
        });
        if let Some(group) = selected.and_then(|i| groups.iter().find(|g| g.items.contains(&i))) {
            ui.label(egui::RichText::new("Source swatches").size(10.));
            egui::ScrollArea::horizontal()
                .id_salt("native_source_swatches")
                .max_height(27.)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for &variant in &group.items {
                            let swatch = &library.items[variant];
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(22., 22.), Sense::click());
                            if swatch.colors.is_empty() {
                                ui.painter().rect_stroke(
                                    rect,
                                    1,
                                    Stroke::new(1., Color32::GRAY),
                                    StrokeKind::Inside,
                                );
                            } else {
                                let n = swatch.colors.len().min(3);
                                for (i, &argb) in swatch.colors.iter().take(n).enumerate() {
                                    let color = Color32::from_rgb(
                                        (argb >> 16) as u8,
                                        (argb >> 8) as u8,
                                        argb as u8,
                                    );
                                    let strip = Rect::from_min_max(
                                        pos2(rect.left() + i as f32 * 22. / n as f32, rect.top()),
                                        pos2(
                                            rect.left() + (i + 1) as f32 * 22. / n as f32,
                                            rect.bottom(),
                                        ),
                                    );
                                    ui.painter().rect_filled(strip, 0, color);
                                }
                            }
                            if selected == Some(variant) {
                                ui.painter().rect_stroke(
                                    rect,
                                    1,
                                    Stroke::new(2., Color32::WHITE),
                                    StrokeKind::Inside,
                                );
                            }
                            if response.clicked() && enabled {
                                choice = Some(variant);
                            }
                            response.on_hover_text(format!("{}\n{}", swatch.name, swatch.key));
                        }
                    });
                });
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Catalog columns").size(10.));
            ui.add(egui::Slider::new(&mut self.columns, 1..=4).show_value(true));
        });
        // Protect visible textures, evict older entries to bound native/GPU memory.
        let protected = visible.iter().copied().collect::<HashSet<_>>();
        let mut bytes = self.cache.values().map(|x| x.bytes).sum::<usize>();
        if bytes > 32 * 1024 * 1024 || self.cache.len() > 2048 {
            let mut old = self
                .cache
                .iter()
                .filter(|(k, _)| !protected.contains(k))
                .map(|(k, v)| (*k, v.used, v.bytes))
                .collect::<Vec<_>>();
            old.sort_by_key(|x| x.1);
            for (key, _, size) in old {
                if bytes <= 32 * 1024 * 1024 && self.cache.len() <= 2048 {
                    break;
                }
                self.cache.remove(&key);
                bytes = bytes.saturating_sub(size);
            }
        }
        let missing = visible
            .into_iter()
            .filter(|k| !self.cache.contains_key(k))
            .collect::<Vec<_>>();
        if missing != self.requested {
            self.cancel.store(true, Ordering::Relaxed);
            self.cancel = Arc::new(AtomicBool::new(false));
            self.serial += 1;
            self.requested = missing.clone();
            self.pending = !missing.is_empty();
            if !missing.is_empty() {
                let _ = self.tx.send(Request {
                    epoch: self.epoch,
                    serial: self.serial,
                    library,
                    keys: missing,
                    cancel: self.cancel.clone(),
                    ctx: ui.ctx().clone(),
                });
            }
        }
        if self.pending {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(30));
        }
        choice
    }
}
impl Drop for Gallery {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn candidates(library: &Library, key: Key) -> Result<(Vec<ThumbnailCandidate>, bool), String> {
    let item = &library.items[key.index];
    let choices = library
        .thumbnails
        .choices(&library.catalog, item.source)
        .map_err(|e| e.to_string())?;
    let design_only = choices.exact.is_empty();
    let mut candidates = if design_only {
        choices
            .design
            .into_iter()
            .flat_map(|d| d.candidates)
            .collect()
    } else {
        choices.exact
    };
    let gender = key.model.or(match item.gender {
        0x10 => Some(true),
        0x20 => Some(false),
        _ => None,
    });
    if let Some(male) = gender {
        let hint = if male {
            ThumbnailModelHint::MaleModel
        } else {
            ThumbnailModelHint::FemaleModel
        };
        if candidates.iter().any(|c| c.model_hint == hint) {
            candidates.retain(|c| c.model_hint == hint);
        } else {
            candidates.retain(|c| c.model_hint == ThumbnailModelHint::Unknown);
        }
    }
    if candidates.is_empty() {
        Err("No source thumbnail for this model. No substitute image was generated.".into())
    } else {
        Ok((candidates, design_only))
    }
}
fn decode_batch(request: &Request) -> Vec<Decoded> {
    let choices = request
        .keys
        .iter()
        .map(|&key| (key, candidates(&request.library, key)))
        .collect::<Vec<_>>();
    let all = choices
        .iter()
        .filter_map(|(_, r)| r.as_ref().ok())
        .flat_map(|(v, _)| v.iter().cloned())
        .collect::<Vec<_>>();
    let inspected = request.library.catalog.inspect_thumbnails(
        &all,
        ReadLimits {
            max_resource_bytes: 64 * 1024 * 1024,
        },
        &request.cancel,
    );
    choices.into_iter().map(|(key,choice)|{
        let result=(||{
            let (candidates,design_only)=choice?;
            let inspected=inspected.as_ref().map_err(|e|e.to_string())?;
            let matches=|resource,provider|candidates.iter().any(|c|c.key==resource&&c.providers.contains(&provider));
            if let Some(failure)=inspected.failures.iter().find(|f|matches(f.key,f.provider)){return Err(failure.message.clone());}
            let payloads=inspected.payloads.iter().filter(|p|matches(p.key,p.provider)).collect::<Vec<_>>();
            let first=payloads.first().ok_or("Thumbnail has no readable payload")?;
            if payloads.iter().any(|p|p.payload_sha256!=first.payload_sha256||p.bytes!=first.bytes){return Err("Conflicting source images; choose a specific model/source rather than guessing precedence".into());}
            let size=first.info.dimensions.ok_or("Thumbnail dimensions are unsupported")?;
            if size[0]>4096||size[1]>4096||size[0]as u64*size[1]as u64>4*1024*1024{return Err("Thumbnail exceeds the bounded preview image budget".into());}
            let decoded=external_cas_assets::thumbnail_decode::decode_pinned_thumbnail(&first.bytes,first.payload_sha256,ReadLimits{max_resource_bytes:32*1024*1024},&request.cancel).map_err(|e|e.to_string())?;
            let alpha=decoded.alpha;
            let mut image=image::RgbaImage::from_raw(decoded.width,decoded.height,decoded.rgba).ok_or("Invalid decoded thumbnail dimensions")?;
            if image.width()>512||image.height()>512{image=image::imageops::thumbnail(&image,512,512);}
            let note=format!("{} · {} · {:?} · source SHA-256 {}",if design_only{"Related design image; exact swatch may differ"}else{"Exact CASP thumbnail"},first.key,alpha,first.payload_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>());
            Ok((egui::ColorImage::from_rgba_unmultiplied([image.width()as usize,image.height()as usize],image.as_raw()),design_only,note))
        })();Decoded{key,result}
    }).collect()
}
