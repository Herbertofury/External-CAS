use crate::*;
use external_cas_assets::{scan::Provider, PackageReader};
use external_cas_scene::EffectiveResolution;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
#[derive(Clone, Debug)]
pub struct ProviderObservation {
    pub provider: Provider,
    pub package_path: PathBuf,
    pub index_sha256: [u8; 32],
    pub payload_sha256: Option<[u8; 32]>,
    pub deleted: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ResourceObservation {
    pub key: ResourceKey,
    pub candidates: Vec<ProviderObservation>,
    pub selected: Option<Provider>,
    pub selection: String,
    pub effective_policy: Option<EffectiveResolution>,
    pub payload_sha256: Option<[u8; 32]>,
    pub bytes: Option<Arc<[u8]>>,
    pub error: Option<String>,
}
pub(crate) struct Resolver<'a, 'b> {
    pub catalog: &'a Catalog,
    pub policy: Option<&'b ScopePolicy<'a>>,
    pub choices: &'b BTreeMap<ResourceKey, Provider>,
    pub limits: ReadLimits,
    pub max_bytes: u64,
    pub bytes: u64,
    pub cancel: &'b AtomicBool,
    readers: BTreeMap<usize, std::result::Result<PackageReader<'a>, String>>,
    cache: BTreeMap<ResourceKey, Arc<ResourceObservation>>,
}
impl<'a, 'b> Resolver<'a, 'b> {
    pub fn new(
        catalog: &'a Catalog,
        policy: Option<&'b ScopePolicy<'a>>,
        choices: &'b BTreeMap<ResourceKey, Provider>,
        limits: ReadLimits,
        max_bytes: u64,
        cancel: &'b AtomicBool,
    ) -> Self {
        Self {
            catalog,
            policy,
            choices,
            limits,
            max_bytes,
            bytes: 0,
            cancel,
            readers: BTreeMap::new(),
            cache: BTreeMap::new(),
        }
    }
    fn read(&mut self, key: ResourceKey, p: Provider) -> std::result::Result<Arc<[u8]>, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let (index, entry) = self.catalog.provider(p).ok_or("Provider absent")?;
        if entry.key != key || entry.deleted() {
            return Err("Provider changed or deleted".into());
        }
        if self
            .bytes
            .checked_add(entry.memory_size as u64)
            .is_none_or(|v| v > self.max_bytes)
        {
            return Err("Character resource byte budget exceeded".into());
        }
        let reader = self.readers.entry(p.package_index).or_insert_with(|| {
            index
                .reader_cancellable(self.cancel)
                .map_err(|e| e.to_string())
        });
        let raw = reader
            .as_mut()
            .map_err(|e| e.clone())?
            .read_resource_cancellable(entry, self.limits, self.cancel)
            .map_err(|e| e.to_string())?;
        self.bytes += raw.len() as u64;
        Ok(raw.into())
    }
    pub fn get(&mut self, key: ResourceKey) -> Arc<ResourceObservation> {
        if let Some(old) = self.cache.get(&key) {
            return old.clone();
        }
        let providers = self.catalog.providers(key).to_vec();
        let mut out = ResourceObservation {
            key,
            candidates: vec![],
            selected: None,
            selection: String::new(),
            effective_policy: self.policy.map(|p| p.resolve_key(key)),
            payload_sha256: None,
            bytes: None,
            error: None,
        };
        for p in &providers {
            if let Some((index, entry)) = self.catalog.provider(*p) {
                out.candidates.push(ProviderObservation {
                    provider: *p,
                    package_path: index.path.clone(),
                    index_sha256: index.index_sha256,
                    payload_sha256: None,
                    deleted: entry.deleted(),
                    error: None,
                });
            }
        }
        let explicit = self.choices.get(&key).copied();
        let selected = explicit.or_else(|| out.effective_policy.as_ref().and_then(|p| p.selected));
        if let Some(p) = selected {
            if !providers.contains(&p) {
                out.error =
                    Some("Explicit/priority provider is not a candidate for this full TGI".into());
            } else {
                match self.read(key, p) {
                    Ok(bytes) => {
                        let hash = Sha256::digest(&bytes).into();
                        for c in &mut out.candidates {
                            if c.provider == p {
                                c.payload_sha256 = Some(hash);
                            }
                        }
                        out.selected = Some(p);
                        out.selection = if explicit.is_some() {
                            "explicit-provider"
                        } else {
                            "installed-config-priority"
                        }
                        .into();
                        out.payload_sha256 = Some(hash);
                        out.bytes = Some(bytes);
                    }
                    Err(e) => out.error = Some(e),
                }
            }
        } else if providers.is_empty() {
            out.error = Some("Missing full TGI in configured catalog".into());
        } else {
            let mut first: Option<(Provider, Arc<[u8]>, [u8; 32])> = None;
            let mut identical = true;
            for (i, p) in providers.into_iter().enumerate() {
                match self.read(key, p) {
                    Ok(bytes) => {
                        let hash = Sha256::digest(&bytes).into();
                        if let Some(c) = out.candidates.get_mut(i) {
                            c.payload_sha256 = Some(hash);
                        }
                        if let Some((_, old, _)) = &first {
                            if old.as_ref() != bytes.as_ref() {
                                identical = false;
                            }
                        } else {
                            first = Some((p, bytes, hash));
                        }
                    }
                    Err(e) => {
                        if let Some(c) = out.candidates.get_mut(i) {
                            c.error = Some(e.clone());
                        }
                        out.error = Some(e);
                        break;
                    }
                }
            }
            if out.error.is_none() {
                if identical {
                    let (p, bytes, hash) = first.unwrap();
                    out.selected = Some(p);
                    out.bytes = Some(bytes);
                    out.payload_sha256 = Some(hash);
                    out.selection = "byte-identical-full-TGI-providers".into();
                } else {
                    out.error=Some("Differing providers remain unresolved without qualified priority/explicit choice".into());
                }
            }
        }
        let out = Arc::new(out);
        self.cache.insert(key, out.clone());
        out
    }
}
