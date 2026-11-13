// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::PackageIndex;
use external_cas_fidelity::GfxMovie;
use external_cas_fidelity::{
    installed::{SourcePin, load_movie, verify_pin_set},
    ui_layout::{UiPlatform, load_manifest},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone)]
pub struct PreparedResource {
    pub runtime_bytes: Arc<[u8]>,
    pub runtime_sha256: String,
}
impl PreparedResource {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            runtime_sha256: format!("{:x}", Sha256::digest(&bytes)),
            runtime_bytes: bytes.into(),
        }
    }
}
pub struct PreparedUiBundle {
    pub root: String,
    pub resources: BTreeMap<String, PreparedResource>,
    pub source_pins: Vec<SourcePin>,
    pub device_fonts: Vec<(String, Vec<u8>)>,
    /// Original/derived hashes and conversion details; contains private paths.
    pub preparation: Vec<serde_json::Value>,
}
impl PreparedUiBundle {
    pub fn validate(&self) -> Result<(), String> {
        if !self.resources.contains_key(&self.root) {
            return Err("source UI root is absent".into());
        }
        let mut size = 0usize;
        for (name, r) in &self.resources {
            if name.is_empty()
                || !name.is_ascii()
                || name != &name.to_ascii_lowercase()
                || name.contains(['/', '\\', ':', '?', '#'])
                || name == ".."
            {
                return Err("source virtual resource name is invalid".into());
            }
            size = size
                .checked_add(r.runtime_bytes.len())
                .ok_or("source UI byte overflow")?;
            if size > 256 * 1024 * 1024 {
                return Err("prepared UI memory budget exceeded".into());
            }
            if format!("{:x}", Sha256::digest(&r.runtime_bytes)) != r.runtime_sha256 {
                return Err(format!("prepared source UI resource changed: {name}"));
            }
        }
        Ok(())
    }
}
fn bare(s: &str) -> Result<String, String> {
    let n = s
        .strip_suffix(".swf")
        .or_else(|| s.strip_suffix(".gfx"))
        .unwrap_or(s);
    if n.is_empty() || !n.is_ascii() || n.contains(['/', '\\', ':']) {
        Err("unqualified original import name".into())
    } else {
        Ok(n.into())
    }
}
/// Off-UI worker boundary. Read only the explicit package and the original CAS
/// manifest/import graph. It does not index Mods or infer provider precedence.
pub fn prepare_cas_bundle(
    index: &PackageIndex,
    expected_pins: Option<&[SourcePin]>,
    cancel: &AtomicBool,
) -> Result<PreparedUiBundle, String> {
    prepare_movie_bundle(index, "UIContainer", true, expected_pins, cancel)
}
pub fn prepare_movie_bundle(
    index: &PackageIndex,
    root: &str,
    include_cas_widgets: bool,
    expected_pins: Option<&[SourcePin]>,
    cancel: &AtomicBool,
) -> Result<PreparedUiBundle, String> {
    prepare_movie_bundle_with_aliases(
        index,
        root,
        include_cas_widgets,
        &BTreeMap::new(),
        expected_pins,
        cancel,
    )
}
pub fn locale_aliases(locale: &str) -> Result<BTreeMap<String, String>, String> {
    let locale = locale.to_ascii_lowercase();
    let bytes = locale.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b'_'
        || !bytes[..2]
            .iter()
            .chain(bytes[3..].iter())
            .all(u8::is_ascii_alphabetic)
    {
        return Err("explicit installed UI locale must use ll_cc form".into());
    }
    Ok(["LocalizedLibraryBase", "LocalizedLibraryConsole"]
        .into_iter()
        .map(|n| (n.to_ascii_lowercase(), format!("{n}_{locale}")))
        .collect())
}
/// A standalone widget normally runs below UIContainer's preloaded global
/// source libraries. Carry only the host's actual dependency declarations into
/// the in-memory derivative; no host timeline, ActionScript, or artwork is run.
fn host_imports(index: &PackageIndex) -> Result<(SourcePin, Vec<(u16, Vec<u8>, String)>), String> {
    let host = load_movie(index, "UIContainer")?;
    let movie = host.inspect()?;
    let imports=movie.imports.iter().filter(|i|!i.movie.is_empty()).map(|i|{
        let tag=movie.tags.get(i.tag_index).ok_or("source host import tag absent")?;
        if !matches!(tag.code,57|71) {return Err("source host dependency is not ImportAssets".into());}
        if !i.characters.is_empty() {return Err("source host dependency has display symbol bindings; cannot reuse as global-only import".into());}
        Ok((tag.code,movie.payload(tag).to_vec(),i.movie.clone()))
    }).collect::<Result<Vec<_>,String>>()?;
    Ok((host.pin, imports))
}
fn add_host_imports(raw: &[u8], imports: &[(u16, Vec<u8>, String)]) -> Result<Vec<u8>, String> {
    let movie = GfxMovie::inspect(raw).map_err(|e| e.to_string())?;
    let names = movie
        .imports
        .iter()
        .map(|i| i.movie.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let added = imports
        .iter()
        .filter(|(_, _, name)| !names.contains(&name.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    if added.is_empty() {
        return Ok(raw.to_vec());
    }
    let first = movie.tags.first().ok_or("source widget has no tags")?;
    let header = first.payload.start - if first.payload.len() < 63 { 2 } else { 6 };
    let mut out = raw[..header].to_vec();
    fn tag(out: &mut Vec<u8>, code: u16, payload: &[u8]) {
        out.extend_from_slice(&((code << 6) | payload.len().min(63) as u16).to_le_bytes());
        if payload.len() >= 63 {
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        }
        out.extend_from_slice(payload);
    }
    let mut published = false;
    for source in &movie.tags {
        if !published && source.code != 69 {
            for (code, payload, _) in &added {
                tag(&mut out, *code, payload);
            }
            published = true;
        }
        tag(&mut out, source.code, movie.payload(source));
    }
    if !published {
        return Err("source widget has no executable tag boundary".into());
    }
    out.extend_from_slice(&raw[movie.trailing_bytes.clone()]);
    let len = u32::try_from(out.len()).map_err(|_| "source widget extent")?;
    out[4..8].copy_from_slice(&len.to_le_bytes());
    Ok(out)
}
pub fn prepare_movie_bundle_with_aliases(
    index: &PackageIndex,
    root: &str,
    include_cas_widgets: bool,
    aliases: &BTreeMap<String, String>,
    expected_pins: Option<&[SourcePin]>,
    cancel: &AtomicBool,
) -> Result<PreparedUiBundle, String> {
    let root = bare(root)?;
    let mut pending = vec![root.clone()];
    let mut pins = Vec::new();
    let host_global_imports = if root != "UIContainer" {
        let (pin, imports) = host_imports(index)?;
        pending.extend(
            imports
                .iter()
                .map(|(_, _, name)| bare(name))
                .collect::<Result<Vec<_>, _>>()?,
        );
        pins.push(pin);
        imports
    } else {
        Vec::new()
    };
    let mut resources = BTreeMap::new();
    if root == "UIContainer" || include_cas_widgets {
        let manifest = load_manifest(index)?;
        if include_cas_widgets {
            pending.extend(
                manifest
                    .manifest
                    .widgets_for_state("cas", UiPlatform::Pc)?
                    .into_iter()
                    .map(|w| w.name),
            );
        }
        pins.push(manifest.pin);
        resources.insert(
            "widgetlist.xml".into(),
            PreparedResource::new(manifest.source_bytes),
        );
    }
    let mut seen = BTreeSet::new();
    let mut preparation = Vec::new();
    let mut source_bytes = 0usize;
    while let Some(name) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err("source UI preparation cancelled".into());
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let selected = aliases
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
            .unwrap_or(&name);
        let loaded = load_movie(index, selected)
            .map_err(|e| format!("original source movie {name}: {e}"))?;
        source_bytes += loaded.source_bytes.len();
        if source_bytes > 128 * 1024 * 1024 {
            return Err("source movie graph exceeds memory budget".into());
        }
        let movie = loaded.inspect()?;
        for i in &movie.imports {
            if !i.movie.is_empty() {
                pending.push(bare(&i.movie)?);
            }
        }
        let (mut bytes, mut report, atlas_pins) =
            crate::source_assets::adapt(index, &loaded.source_bytes, cancel)?;
        if name == root && !host_global_imports.is_empty() {
            bytes = add_host_imports(&bytes, &host_global_imports)?;
            report["original_host_dependency_declarations"] = serde_json::json!({"host":"UIContainer","imports":host_global_imports.iter().map(|(_,_,name)|name).collect::<Vec<_>>(),"runtime_sha256":format!("{:x}",Sha256::digest(&bytes)),"host_timeline_executed":false,"source_abc_modified":false});
        }
        if selected != name {
            report["caller_explicit_alias"] =
                serde_json::json!({"requested":name,"selected":selected});
        }
        pins.push(loaded.pin);
        pins.extend(atlas_pins);
        preparation.push(report);
        let resource = PreparedResource::new(bytes);
        resources.insert(
            format!("{}.swf", name.to_ascii_lowercase()),
            resource.clone(),
        );
        resources.insert(format!("{}.gfx", name.to_ascii_lowercase()), resource);
    }
    let source_ui = crate::source_fonts::Ui::load(index, crate::Audit::new())?;
    if let Some(pin) = source_ui.config_pin.clone() {
        pins.push(pin);
    }
    let mut unique = BTreeMap::new();
    for pin in pins {
        let key = (pin.provider_path.clone(), pin.key, pin.ordinal);
        if let Some(old) = unique.insert(key, pin.clone()) {
            if old != pin {
                return Err("source UI dependency changed during preparation".into());
            }
        }
    }
    let pins = unique.into_values().collect::<Vec<_>>();
    if let Some(expected) = expected_pins {
        verify_pin_set(expected, &pins)?;
    }
    if !pins
        .iter()
        .any(|p| Some(p) == source_ui.source_pin.as_ref())
    {
        return Err("source font provider changed during preparation".into());
    }
    let result = PreparedUiBundle {
        root: format!("{}.gfx", root.to_ascii_lowercase()),
        resources,
        source_pins: pins,
        device_fonts: source_ui.fonts,
        preparation,
    };
    result.validate()?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_host_import_declarations_preserve_every_abc_and_existing_import() {
        let raw = include_bytes!("../tests/fixtures/native-function-slot.swf");
        let before = GfxMovie::inspect(raw).unwrap();
        let mut payload = b"SourceGlobalLibrary.swf\0".to_vec();
        payload.extend_from_slice(&[1, 0, 0, 0]);
        let imports = vec![(71, payload.clone(), "SourceGlobalLibrary.swf".into())];
        let adapted = add_host_imports(raw, &imports).unwrap();
        let after = GfxMovie::inspect(&adapted).unwrap();
        assert_eq!(after.imports.len(), before.imports.len() + 1);
        assert_eq!(after.imports[0].movie, "SourceGlobalLibrary.swf");
        assert_eq!(
            after
                .tags
                .iter()
                .filter(|t| matches!(t.code, 72 | 82))
                .map(|t| after.payload(t))
                .collect::<Vec<_>>(),
            before
                .tags
                .iter()
                .filter(|t| matches!(t.code, 72 | 82))
                .map(|t| before.payload(t))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            add_host_imports(&adapted, &imports).unwrap(),
            adapted,
            "already present dependency must not duplicate"
        );
        assert_eq!(
            after.tags.first().unwrap().code,
            69,
            "FileAttributes stays first"
        );
        let _ = payload;
    }
    #[test]
    fn prepared_hash_and_virtual_name_must_match_exact_memory() {
        let mut b = PreparedUiBundle {
            root: "root.swf".into(),
            resources: BTreeMap::from([("root.swf".into(), PreparedResource::new(vec![1, 2, 3]))]),
            source_pins: vec![],
            device_fonts: vec![],
            preparation: vec![],
        };
        assert!(b.validate().is_ok());
        b.resources.get_mut("root.swf").unwrap().runtime_bytes = Arc::from([1, 2, 4]);
        assert!(b.validate().is_err());
        b.resources = BTreeMap::from([("../root.swf".into(), PreparedResource::new(vec![1]))]);
        b.root = "../root.swf".into();
        assert!(b.validate().is_err());
    }
}
