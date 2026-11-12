use crate::{texture, viewport};
use external_cas_assets::{
    CasPart, Catalog, Geometry, PackageReader, ReadLimits, ResourceKey, scan::Provider,
};
use external_cas_catalog_cache::{CachedPackage, CachedPart, CachedUnsupported, CatalogCache};
#[path = "library_scan_cache.rs"]
mod scan_cache;
pub use scan_cache::CacheScanStats;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Instant,
};

pub struct Item {
    pub name: String,
    pub key: ResourceKey,
    pub source: Provider,
    pub source_label: String,
    pub body_type: u32,
    pub age: u8,
    pub gender: u8,
    pub meshes: Vec<ResourceKey>,
    pub diffuse: Option<ResourceKey>,
    pub colors: Vec<u32>,
    pub search: String,
    pub payload_sha256: [u8; 32],
    pub design: usize,
    pub swatch_order: u16,
    pub color_sliders: Option<external_cas_assets::casp::CaspColorSliders>,
}
#[derive(Clone, Debug)]
pub struct GalleryGroup {
    pub items: Vec<usize>,
}
#[derive(PartialEq, Eq, Hash)]
struct DesignKey {
    package: usize,
    outfit: u32,
    body: u32,
    age: u8,
    gender: u8,
    species: Option<u32>,
    texture_space: u8,
    meshes: Vec<ResourceKey>,
    unique: Option<usize>,
}
pub struct Library {
    pub catalog: Catalog,
    pub items: Arc<Vec<Item>>,
    pub unsupported: Vec<String>,
    pub elapsed: std::time::Duration,
    pub poses: Vec<(Provider, ResourceKey, String)>,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub cache_issues: Vec<String>,
    pub cache_path: Option<PathBuf>,
    pub cache_scope: Option<String>,
    pub cache_stats: CacheScanStats,
    pub thumbnails: external_cas_assets::thumbnail::ThumbnailDesignIndex,
    package_lookup: HashMap<PathBuf, usize>,
    item_lookup: HashMap<(usize, usize), usize>,
}
pub enum Event {
    Progress {
        generation: u64,
        message: String,
    },
    Library {
        generation: u64,
        result: Result<Arc<Library>, String>,
    },
    Search {
        generation: u64,
        rows: Vec<usize>,
        elapsed: std::time::Duration,
        cache_used: bool,
        groups: Vec<GalleryGroup>,
    },
    Preview {
        generation: u64,
        result: Result<PreparedPreview, String>,
    },
    Tray {
        generation: u64,
        result: Result<external_cas_tray::TrayLibrary, String>,
    },
    Project {
        generation: u64,
        result: Result<PreparedProject, String>,
    },
    Rig {
        generation: u64,
        result: Result<external_cas_animation::Rig, String>,
    },
    Clip {
        generation: u64,
        result: Result<external_cas_animation::Clip, String>,
    },
    Draft {
        generation: u64,
        scan_generation: u64,
        selection_generation: u64,
        base_revision: u64,
        result: Result<(external_cas_tray::ProjectDraft, PathBuf, ColorCache), String>,
    },
}
pub struct PreparedProject {
    pub project: external_cas_tray::TrayProject,
    pub draft: Option<external_cas_tray::ProjectDraft>,
    pub draft_error: Option<String>,
    pub colors: ColorCache,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotColorKey {
    pub sim_id: u64,
    pub form: Option<usize>,
    pub outfit_index: usize,
    pub outfit_id: u64,
    pub part_index: usize,
}
#[derive(Default)]
pub struct ColorCache {
    pub values: HashMap<SlotColorKey, Option<u64>>,
    pub issue: Option<String>,
}
pub fn draft_color_cache(draft: &external_cas_tray::ProjectDraft) -> ColorCache {
    let result = (|| {
        let mut values = HashMap::new();
        for sim in draft.appearance().map_err(|e| e.to_string())? {
            for form in std::iter::once(&sim.base).chain(sim.occult_forms.iter()) {
                for outfit in &form.outfits {
                    for part in &outfit.parts {
                        values.insert(
                            SlotColorKey {
                                sim_id: sim.sim_id,
                                form: form.occult_form_index,
                                outfit_index: outfit.outfit_index,
                                outfit_id: outfit.outfit_id,
                                part_index: part.part_index,
                            },
                            part.color_shift,
                        );
                    }
                }
            }
        }
        Ok::<_, String>(values)
    })();
    match result {
        Ok(values) => ColorCache {
            values,
            issue: None,
        },
        Err(issue) => ColorCache {
            values: HashMap::new(),
            issue: Some(issue),
        },
    }
}
pub fn project_ready(
    project: Result<external_cas_tray::TrayProject, external_cas_tray::TrayError>,
) -> Result<PreparedProject, String> {
    let project = project.map_err(|e| e.to_string())?;
    let (draft, draft_error) = match external_cas_tray::ProjectDraft::from_project(&project) {
        Ok(draft) => (Some(draft), None),
        Err(error) => (None, Some(error.to_string())),
    };
    Ok(PreparedProject {
        colors: draft.as_ref().map(draft_color_cache).unwrap_or_default(),
        project,
        draft,
        draft_error,
    })
}
pub struct PreparedPreview {
    pub fully_transparent: bool,
    pub parts: Vec<viewport::MeshPart>,
    pub warnings: Vec<String>,
    pub vertices: usize,
    pub triangles: usize,
    pub elapsed: std::time::Duration,
    pub geometries: Vec<Geometry>,
    pub source_pins: Vec<SourcePin>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourcePin {
    pub key: String,
    pub provider_path: PathBuf,
    pub ordinal: u32,
    pub index_sha256: String,
    pub payload_sha256: String,
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn verify_source_pins(pins: &[SourcePin], cancel: &AtomicBool) -> Result<(), String> {
    let mut packages = HashMap::new();
    for pin in pins {
        if cancel.load(Ordering::Relaxed) {
            return Err("Source validation cancelled".into());
        }
        if !packages.contains_key(&pin.provider_path) {
            packages.insert(
                pin.provider_path.clone(),
                external_cas_assets::PackageIndex::open_cancellable(&pin.provider_path, cancel)
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    for (path, package) in &packages {
        let mut reader = package
            .reader_cancellable(cancel)
            .map_err(|e| e.to_string())?;
        for pin in pins.iter().filter(|p| &p.provider_path == path) {
            if hex(&package.index_sha256) != pin.index_sha256 {
                return Err(format!("Source index changed: {}", path.display()));
            }
            let entry = package
                .entries
                .get(pin.ordinal as usize)
                .filter(|e| e.key.to_string() == pin.key)
                .ok_or("Resource locator changed")?;
            let raw = reader
                .read_resource_cancellable(entry, ReadLimits::default(), cancel)
                .map_err(|e| e.to_string())?;
            if hex(&Sha256::digest(&raw)) != pin.payload_sha256 {
                return Err(format!("Source content changed: {}", pin.key));
            }
        }
    }
    Ok(())
}
pub fn scan(roots: Vec<PathBuf>, generation: u64, cancel: Arc<AtomicBool>, tx: Sender<Event>) {
    let start = Instant::now();
    let mut last = Instant::now();
    let result = (|| {
        let catalog = Catalog::scan_roots(&roots, &cancel, |p| {
            if last.elapsed().as_millis() >= 100 {
                let _ = tx.send(Event::Progress {
                    generation,
                    message: format!(
                        "Discovering · {} packages · {} resources",
                        p.packages, p.resources
                    ),
                });
                last = Instant::now();
            }
        })
        .map_err(|e| e.to_string())?;
        let mut publisher = scan_cache::Publisher::new();
        publisher.stats.index_discovery_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut items = Vec::new();
        let mut thumbnails = external_cas_assets::thumbnail::ThumbnailDesignIndex::new();
        let mut designs = HashMap::<DesignKey, usize>::new();
        let mut unsupported = Vec::new();
        let mut poses = Vec::new();
        let mut cache_issues = Vec::new();
        let cache_path = std::env::var_os("EXTERNAL_CAS_CATALOG_PATH")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA").map(|base| {
                    PathBuf::from(base)
                        .join("ExternalCAS")
                        .join("Catalog")
                        .join("metadata-v1.sqlite")
                })
            });
        let mut cache = cache_path.and_then(|path| match CatalogCache::open(path) {
                Ok(cache) => {
                    if let Ok(value)=std::env::var("EXTERNAL_CAS_CATALOG_CACHE_KIB") {
                        match value.parse::<u32>(){Ok(kib)=>match cache.set_page_cache_budget_kib(kib){Ok(())=>publisher.stats.page_cache_target_kib=Some(kib),Err(error)=>cache_issues.push(format!("Catalog page-cache configuration: {error}"))},Err(error)=>cache_issues.push(format!("Catalog page-cache configuration: {error}"))}
                    }
                    Some(cache)
                },
            Err(e) => {
                cache_issues.push(e.to_string());
                None
            }
        });
        let (mut cache_hits, mut cache_misses) = (0, 0);
        for (package_index, package) in catalog.packages.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Discovery cancelled".into());
            }
            for (entry_index, entry) in package.entries.iter().enumerate() {
                if !entry.deleted() && entry.key.type_id == external_cas_assets::dbpf::CLIP {
                    poses.push((
                        Provider {
                            package_index,
                            entry_index,
                        },
                        entry.key,
                        package
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    ));
                }
            }
            let load_started = Instant::now();
            let cached = match cache.as_ref().map(|c| c.load_package_profiled(package, &cancel)) {
                Some(Ok((cached,timing))) => {publisher.add_load(timing);cached},
                Some(Err(e)) => {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Discovery cancelled".into());
                    }
                    cache_issues.push(e.to_string());
                    None
                }
                None => None,
            };
            publisher.stats.cache_load_ms += load_started.elapsed().as_secs_f64() * 1000.;
            let metadata = if let Some(cached) = cached {
                cache_hits += 1;
                cached
            } else {
                cache_misses += 1;
                let decode_started = Instant::now();
                let mut metadata = CachedPackage::default();
                let mut reader = match package.reader_cancellable(&cancel) {
                    Ok(r) => r,
                    Err(e) => {
                        publisher.stats.source_packages_failed += 1;
                        unsupported.push(format!("{}: {e}", package.path.display()));
                        continue;
                    }
                };
                for (entry_index, entry) in package.entries.iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Discovery cancelled".into());
                    }
                    if entry.deleted() || entry.key.type_id != external_cas_assets::dbpf::CASP {
                        continue;
                    }
                    let result = reader
                        .read_resource_cancellable(entry, ReadLimits::default(), &cancel)
                        .and_then(|b| {
                            Ok((CasPart::parse(&b)?, <[u8; 32]>::from(Sha256::digest(&b))))
                        });
                    match result {
                        Ok((casp, payload_sha256)) => metadata.rows.push(CachedPart {
                            entry_index,
                            payload_sha256,
                            casp,
                        }),
                        Err(e) => metadata.unsupported.push(CachedUnsupported {
                            entry_index,
                            message: e.to_string(),
                        }),
                    }
                    if last.elapsed().as_millis() >= 100 {
                        let _ = tx.send(Event::Progress {
                            generation,
                            message: format!(
                                "Reading appearance · {} items · {} unsupported",
                                items.len() + metadata.rows.len(),
                                unsupported.len() + metadata.unsupported.len()
                            ),
                        });
                        last = Instant::now();
                    }
                }
                publisher.stats.metadata_decode_ms +=
                    decode_started.elapsed().as_secs_f64() * 1000.;
                if let Some(cache) = &mut cache {
                    publisher.enqueue(
                        cache,
                        &catalog,
                        package_index,
                        &metadata,
                        &cancel,
                        generation,
                        &tx,
                        &mut last,
                        &mut cache_issues,
                    )?;
                }
                metadata
            };
            let assemble_started = Instant::now();
            for row in metadata.rows {
                let casp = row.casp;
                let entry = &package.entries[row.entry_index];
                let source_label = package
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let search = format!("{} {} {}", casp.name, source_label, entry.key).to_lowercase();
                let source = Provider {
                    package_index,
                    entry_index: row.entry_index,
                };
                thumbnails
                    .register(entry.key, source, row.payload_sha256, &casp)
                    .map_err(|e| e.to_string())?;
                let design_key = DesignKey {
                    package: package_index,
                    outfit: casp.outfit_id,
                    body: casp.body_type,
                    age: casp.age,
                    gender: casp.gender,
                    species: casp.species,
                    texture_space: casp.texture_space,
                    meshes: casp.lod0_meshes.clone(),
                    unique: (casp.outfit_id == 0 || casp.lod0_meshes.is_empty())
                        .then_some(row.entry_index),
                };
                let next_design = designs.len();
                let design = *designs.entry(design_key).or_insert(next_design);
                items.push(Item {
                    name: casp.name,
                    key: entry.key,
                    source: Provider {
                        package_index,
                        entry_index: row.entry_index,
                    },
                    source_label,
                    body_type: casp.body_type,
                    age: casp.age,
                    gender: casp.gender,
                    meshes: casp.lod0_meshes,
                    diffuse: casp.diffuse,
                    colors: casp.colors,
                    search,
                    payload_sha256: row.payload_sha256,
                    design,
                    swatch_order: casp.swatch_order,
                    color_sliders: casp.color_sliders,
                });
            }
            for row in metadata.unsupported {
                unsupported.push(format!(
                    "{} · {}: {}",
                    package.path.display(),
                    package.entries[row.entry_index].key,
                    row.message
                ));
            }
            publisher.stats.item_assembly_ms += assemble_started.elapsed().as_secs_f64() * 1000.;
        }
        if let Some(cache) = &mut cache {
            publisher.flush(
                cache,
                &catalog,
                &cancel,
                generation,
                &tx,
                &mut last,
                &mut cache_issues,
            )?;
        }
        items.sort_by(|a, b| a.name.cmp(&b.name).then(a.key.cmp(&b.key)));
        let mut cache_scope = None;
        let scope_started = Instant::now();
        if catalog.issues.is_empty()
            && catalog.progress.skipped_links == 0
            && publisher.stats.publication_failures == 0
            && publisher.stats.source_packages_failed == 0
        {
            if let Some(cache) = &mut cache {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos();
                let token = format!("native-{}-{generation}-{stamp}", std::process::id());
                let fingerprints = catalog
                    .packages
                    .iter()
                    .map(|p| p.fingerprint())
                    .collect::<Vec<_>>();
                match cache.bind_scope_with_progress(&token, &fingerprints, &cancel, |done| {
                    if last.elapsed().as_millis() >= 100 {
                        let _ = tx.send(Event::Progress {
                            generation,
                            message: format!("Qualifying indexed search · {done} sources"),
                        });
                        last = Instant::now();
                    }
                }) {
                    Ok(_) => cache_scope = Some(token),
                    Err(error) => {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("Discovery cancelled".into());
                        }
                        cache_issues.push(error.to_string());
                    }
                }
            }
        } else {
            cache_issues.push("Indexed-search scope is unqualified because source discovery or complete cache publication failed".into());
        }
        publisher.stats.scope_bind_ms = scope_started.elapsed().as_secs_f64() * 1000.;
        if let Some(path) = std::env::var_os("EXTERNAL_CAS_SCAN_PROFILE").map(PathBuf::from) {
            let written = (|| -> Result<(), String> {
                use std::io::Write;
                if !path.is_absolute() {
                    return Err("Scan profile path must be absolute".into());
                }
                let mut output = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                let bytes =
                    serde_json::to_vec_pretty(&publisher.stats).map_err(|e| e.to_string())?;
                output.write_all(&bytes).map_err(|e| e.to_string())?;
                Ok(())
            })();
            if let Err(error) = written {
                cache_issues.push(format!("Scan profiling receipt: {error}"));
            }
        }
        let package_lookup = catalog
            .packages
            .iter()
            .enumerate()
            .map(|(i, p)| (p.path.clone(), i))
            .collect();
        let item_lookup = items
            .iter()
            .enumerate()
            .map(|(i, item)| ((item.source.package_index, item.source.entry_index), i))
            .collect();
        let cache_path = cache.as_ref().map(|cache| cache.path.clone());
        Ok(Arc::new(Library {
            catalog,
            items: Arc::new(items),
            unsupported,
            elapsed: start.elapsed(),
            poses,
            cache_hits,
            cache_misses,
            cache_issues,
            cache_path,
            cache_scope,
            cache_stats: publisher.stats,
            thumbnails,
            package_lookup,
            item_lookup,
        }))
    })();
    let _ = tx.send(Event::Library { generation, result });
}
pub fn search(
    library: Arc<Library>,
    query: String,
    body: Option<u32>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    tx: Sender<Event>,
) {
    let start = Instant::now();
    let query = query.to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    if !words.is_empty() {
        if let (Some(path), Some(scope)) = (&library.cache_path, &library.cache_scope) {
            let indexed = (|| -> Result<Vec<usize>, String> {
                let cache = CatalogCache::open(path).map_err(|e| e.to_string())?;
                let hits = cache
                    .search_scoped(scope, &query, body, 0, usize::MAX, &cancel)
                    .map_err(|e| e.to_string())?;
                let mut rows = Vec::with_capacity(hits.len());
                for hit in hits {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Search cancelled".into());
                    }
                    let package = library
                        .package_lookup
                        .get(&hit.package_path)
                        .ok_or("Search provider is outside the active library")?;
                    let index = *library
                        .item_lookup
                        .get(&(*package, hit.entry_index))
                        .ok_or("Search item is outside the active source epoch")?;
                    let item = &library.items[index];
                    if item.key != hit.key
                        || item.payload_sha256 != hit.payload_sha256
                        || item.name != hit.name
                        || item.body_type != hit.body_type
                    {
                        return Err("Cached search metadata changed from the active library".into());
                    }
                    if !words.iter().all(|word| item.search.contains(word)) {
                        return Err("Cached search semantics differ from the active library".into());
                    }
                    rows.push(index);
                }
                rows.sort_unstable();
                Ok(rows)
            })();
            if let Ok(rows) = indexed {
                let groups = group_rows(&library, &rows);
                let _ = tx.send(Event::Search {
                    generation,
                    rows,
                    elapsed: start.elapsed(),
                    cache_used: true,
                    groups,
                });
                return;
            }
        }
    }
    let mut rows = Vec::new();
    for (i, item) in library.items.iter().enumerate() {
        if i % 256 == 0 && cancel.load(Ordering::Relaxed) {
            return;
        }
        if body.is_none_or(|b| b == item.body_type) && words.iter().all(|w| item.search.contains(w))
        {
            rows.push(i);
        }
    }
    let groups = group_rows(&library, &rows);
    let _ = tx.send(Event::Search {
        generation,
        rows,
        elapsed: start.elapsed(),
        cache_used: false,
        groups,
    });
}
fn group_rows(library: &Library, rows: &[usize]) -> Vec<GalleryGroup> {
    let mut positions = HashMap::new();
    let mut groups = Vec::<GalleryGroup>::new();
    for &index in rows {
        let item = &library.items[index];
        let next = groups.len();
        let group = *positions.entry(item.design).or_insert(next);
        if group == groups.len() {
            groups.push(GalleryGroup { items: Vec::new() });
        }
        groups[group].items.push(index);
    }
    for group in &mut groups {
        group
            .items
            .sort_by_key(|&i| (library.items[i].swatch_order, library.items[i].key));
    }
    groups
}
struct Resources<'a> {
    library: &'a Library,
    readers: HashMap<usize, PackageReader<'a>>,
    cancel: &'a AtomicBool,
    pins: Vec<SourcePin>,
}
impl<'a> Resources<'a> {
    fn read_provider(&mut self, provider: Provider) -> Result<Vec<u8>, String> {
        let (package, entry) = self
            .library
            .catalog
            .provider(provider)
            .ok_or("Missing source provider")?;
        if !self.readers.contains_key(&provider.package_index) {
            self.readers.insert(
                provider.package_index,
                package
                    .reader_cancellable(self.cancel)
                    .map_err(|e| e.to_string())?,
            );
        }
        let raw = self
            .readers
            .get_mut(&provider.package_index)
            .unwrap()
            .read_resource_cancellable(entry, ReadLimits::default(), self.cancel)
            .map_err(|e| e.to_string())?;
        self.pins.push(SourcePin {
            key: entry.key.to_string(),
            provider_path: package.path.clone(),
            ordinal: entry.ordinal,
            index_sha256: hex(&package.index_sha256),
            payload_sha256: hex(&Sha256::digest(&raw)),
        });
        Ok(raw)
    }
    fn read(&mut self, key: ResourceKey) -> Result<Vec<u8>, String> {
        match self.library.catalog.providers(key) {
            [provider] => self.read_provider(*provider),
            [] => Err(format!("Missing dependency {key}")),
            providers => Err(format!(
                "Ambiguous dependency {key}: {} providers. Choose a source scope without this ambiguity.",
                providers.len()
            )),
        }
    }
}
pub fn prepare(
    library: Arc<Library>,
    index: usize,
    generation: u64,
    cancel: Arc<AtomicBool>,
    tx: Sender<Event>,
) {
    let start = Instant::now();
    let result = (|| {
        let item = library.items.get(index).ok_or("Selection changed")?;
        let mut resources = Resources {
            library: &library,
            readers: HashMap::new(),
            cancel: &cancel,
            pins: Vec::new(),
        };
        if item.meshes.is_empty() {
            return Err("This item has no supported LOD 0 geometry reference".into());
        }
        // Re-read the selected CASP; a cached label cannot authorize stale content.
        let current_bytes = resources.read_provider(item.source)?;
        if <[u8; 32]>::from(Sha256::digest(&current_bytes)) != item.payload_sha256 {
            return Err("Selected CASP bytes changed; refresh the library".into());
        }
        let current = CasPart::parse(&current_bytes).map_err(|e| e.to_string())?;
        if current.lod0_meshes != item.meshes || current.diffuse != item.diffuse {
            return Err("Selected source changed; refresh the library".into());
        }
        let diffuse = item
            .diffuse
            .ok_or("Diffuse texture is missing; material cannot be rendered accurately")?;
        let (texture, alternative) = if !library.catalog.providers(diffuse).is_empty() {
            (texture::decode(&resources.read(diffuse)?)?, None)
        } else {
            let candidates = library
                .catalog
                .texture_candidates(diffuse, true)
                .into_iter()
                .filter(|c| !c.providers.is_empty())
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [candidate] if candidate.providers.len() == 1 => {
                    let raw = resources.read_provider(candidate.providers[0])?;
                    let image = if candidate.key.type_id == 0x2bc04edf {
                        let decoded = external_cas_assets::lrle::decode_lrle_cancellable(
                            &raw,
                            ReadLimits::default(),
                            &cancel,
                        )
                        .map_err(|e| e.to_string())?;
                        texture::Texture {
                            width: decoded.width,
                            height: decoded.height,
                            rgba: decoded.rgba,
                            warnings: Vec::new(),
                        }
                    } else {
                        texture::decode(&raw)?
                    };
                    (
                        image,
                        Some(format!(
                            "Diffuse resolved through the documented {} texture-family alternative with identical group and instance.",
                            candidate.kind
                        )),
                    )
                }
                [] => {
                    return Err(format!(
                        "Diffuse texture is unavailable in this source scope: {diffuse}"
                    ));
                }
                _ => {
                    return Err(format!(
                        "Diffuse has {} alternative providers; select an unambiguous source scope",
                        candidates.iter().map(|c| c.providers.len()).sum::<usize>()
                    ));
                }
            }
        };
        let mut warnings = texture.warnings;
        if let Some(warning) = alternative {
            warnings.push(warning);
        }
        warnings.push("Item preview uses original GEOM bind geometry and diffuse cutout material. Complete Sim, shader family, morphs and game appearance are not yet qualified.".into());
        let mut parts = Vec::new();
        let mut geometries = Vec::new();
        let shared_rgba: Arc<[u8]> = texture.rgba.into();
        let (mut vertices, mut triangles) = (0, 0);
        for key in &item.meshes {
            if cancel.load(Ordering::Relaxed) {
                return Err("Preview cancelled".into());
            }
            let geometry = Geometry::parse(&resources.read(*key)?).map_err(|e| e.to_string())?;
            vertices += geometry.vertices.len();
            triangles += geometry.indices.len() / 3;
            // A bind-geometry preview does not apply unresolved skin indices or alter
            // original weights. GPU identity projection stays separate from source data.
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
                texture_size: [texture.width, texture.height],
            });
            geometries.push(geometry);
        }
        Ok(PreparedPreview {
            fully_transparent: parts
                .iter()
                .all(|part| part.rgba.chunks_exact(4).all(|pixel| pixel[3] == 0)),
            parts,
            warnings,
            vertices,
            triangles,
            elapsed: start.elapsed(),
            geometries,
            source_pins: resources.pins,
        })
    })();
    let _ = tx.send(Event::Preview { generation, result });
}
pub fn load_pose(library: Arc<Library>, provider: Provider, generation: u64, tx: Sender<Event>) {
    let result = (|| {
        let (package, entry) = library
            .catalog
            .provider(provider)
            .ok_or("Pose source missing")?;
        let bytes = package
            .read_resource(entry, ReadLimits::default())
            .map_err(|e| e.to_string())?;
        external_cas_animation::Clip::parse(
            &bytes,
            external_cas_animation::FrameLayout::Conservative,
        )
        .map_err(|e| e.to_string())
    })();
    let _ = tx.send(Event::Clip { generation, result });
}
pub fn body_label(body: u32) -> String {
    match body {
        2 => "Hair".into(),
        5 => "Full body".into(),
        6 => "Tops".into(),
        7 => "Bottoms".into(),
        8 => "Shoes".into(),
        _ => format!("Body slot {body}"),
    }
}
pub enum DraftChange {
    Apply {
        index: usize,
        slot: external_cas_tray::OutfitSlot,
    },
    ColorShift {
        slot: external_cas_tray::OutfitSlot,
        expected: Option<u64>,
        value: u64,
        materialize: bool,
    },
    Undo,
    Redo,
}
pub fn change_draft(
    mut draft: external_cas_tray::ProjectDraft,
    change: DraftChange,
    library: Option<Arc<Library>>,
    source_pins: Vec<SourcePin>,
    generation: u64,
    scan_generation: u64,
    selection_generation: u64,
    tx: Sender<Event>,
) {
    let base_revision = draft.revision();
    let result = (|| {
        match change {
            DraftChange::Apply { index, slot } => {
                if source_pins.is_empty() {
                    return Err("The visible preview has no verified source receipt".into());
                }
                verify_source_pins(&source_pins, &AtomicBool::new(false))?;
                let library = library.ok_or("No source library is loaded")?;
                let item = library
                    .items
                    .get(index)
                    .ok_or("Selected source is absent")?;
                let (package, entry) = library
                    .catalog
                    .provider(item.source)
                    .ok_or("Selected provider is absent")?;
                let mut reader = package.reader().map_err(|e| e.to_string())?;
                let raw = reader
                    .read_pinned_resource(
                        entry,
                        item.payload_sha256,
                        ReadLimits::default(),
                        &AtomicBool::new(false),
                    )
                    .map_err(|e| e.to_string())?;
                let current = CasPart::parse(&raw).map_err(|e| e.to_string())?;
                draft
                    .apply_slot(
                        base_revision,
                        &slot,
                        entry.key.instance_id,
                        current.body_type,
                    )
                    .map_err(|e| e.to_string())?;
            }
            DraftChange::ColorShift {
                slot,
                expected,
                value,
                materialize,
            } => {
                if source_pins.is_empty() {
                    return Err("The visible color preview has no verified source receipt".into());
                }
                verify_source_pins(&source_pins, &AtomicBool::new(false))?;
                draft
                    .apply_color_shift(
                        base_revision,
                        &slot,
                        expected,
                        value,
                        if materialize {
                            external_cas_tray::MissingColorShiftPolicy::MaterializeNeutralOthers
                        } else {
                            external_cas_tray::MissingColorShiftPolicy::RequireExisting
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
            DraftChange::Undo => draft.undo(base_revision).map_err(|e| e.to_string())?,
            DraftChange::Redo => draft.redo(base_revision).map_err(|e| e.to_string())?,
        }
        let base = std::env::var_os("LOCALAPPDATA")
            .ok_or("Windows application data folder is unavailable; draft was not changed")?;
        let root = PathBuf::from(base).join("ExternalCAS").join("Recovery");
        std::fs::create_dir_all(&root)
            .map_err(|e| format!("Cannot create recovery folder: {e}"))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let destination = root.join(format!(
            "revision-{}-{}-{stamp}",
            draft.revision(),
            std::process::id()
        ));
        draft.save_as(&destination).map_err(|e| {
            format!("Recovery revision could not be verified; edit remains unapplied: {e}")
        })?;
        let colors = draft_color_cache(&draft);
        Ok((draft, destination, colors))
    })();
    let _ = tx.send(Event::Draft {
        generation,
        scan_generation,
        selection_generation,
        base_revision,
        result,
    });
}
