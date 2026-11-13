// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::PackageIndex;
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
        let (bytes, mut report, atlas_pins) =
            crate::source_assets::adapt(index, &loaded.source_bytes, cancel)?;
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
