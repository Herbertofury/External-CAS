// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact static GetTropesTuning source projection. Explicit providers only;
//! unknown source fields/bytes remain retained and no game session is contacted.
use crate::HostValue as V;
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use external_cas_fidelity::installed::SourcePin;
use external_cas_room::{DataDocument, DataValue as D, parse_simdata};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
pub struct TropesResource {
    pub pin: SourcePin,
    pub original_bytes: Vec<u8>,
    pub document: DataDocument,
}
pub struct TropesSnapshot {
    pub payload: V,
    pub resources: Vec<TropesResource>,
}
fn required<'a>(d: &'a D, n: &str) -> Result<&'a D, String> {
    d.field(n)?
        .ok_or_else(|| format!("source trope field {n} absent"))
}
fn vec(d: &D) -> Result<&[D], String> {
    if let D::Vector(v) = d {
        Ok(v)
    } else {
        Err("source trope vector type differs".into())
    }
}
fn u32v(d: &D) -> Result<u32, String> {
    if let D::Unsigned32(v) = d {
        Ok(*v)
    } else {
        Err("source trope UInt32 differs".into())
    }
}
fn id(d: &D) -> Result<u64, String> {
    if let D::Unsigned64(v) = d {
        Ok(*v)
    } else {
        Err("source trope tuning ID type differs".into())
    }
}
fn root(resource: &TropesResource) -> Result<&D, String> {
    if resource.document.instances.len() != 1 {
        return Err("ambiguous source trope instance".into());
    }
    Ok(&resource.document.instances[0].1)
}
fn object(fields: impl IntoIterator<Item = (&'static str, V)>) -> V {
    V::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn read(
    index: &PackageIndex,
    key: ResourceKey,
    cancel: &AtomicBool,
) -> Result<TropesResource, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("trope metadata loading cancelled".into());
    }
    let entries = index
        .matching(key)
        .filter(|e| !e.deleted())
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err("source trope key has ambiguous ordinals".into());
    }
    let e = entries[0];
    let raw = index
        .read_resource(
            e,
            ReadLimits {
                max_resource_bytes: 16 * 1024 * 1024,
            },
        )
        .map_err(|e| e.to_string())?;
    let pin = SourcePin {
        provider_path: index.path.clone(),
        key,
        ordinal: e.ordinal,
        index_sha256: index
            .index_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        payload_sha256: format!("{:x}", Sha256::digest(&raw)),
    };
    let document = parse_simdata(&raw, cancel)?;
    Ok(TropesResource {
        pin,
        original_bytes: raw,
        document,
    })
}
impl TropesSnapshot {
    pub fn load(
        global_provider: &Path,
        item_providers: &[PathBuf],
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let index = PackageIndex::open(global_provider).map_err(|e| e.to_string())?;
        let global = read(
            &index,
            ResourceKey::new(0x545ac67a, 0x006ca304, 0x4846c741c3a04482),
            cancel,
        )?;
        let global_data = root(&global)?;
        if !matches!(global_data,D::Object{schema,..}if schema=="relationships.global_relationship_tuning.TropeGlobalTuning")
        {
            return Err("unexpected static trope global schema".into());
        }
        let mut ids = Vec::new();
        for name in ["TROPES", "ROMANCE_TROPES"] {
            for value in vec(required(global_data, name)?)? {
                ids.push(id(value)?);
            }
        }
        let indices = item_providers
            .iter()
            .map(PackageIndex::open)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut items = BTreeMap::new();
        for wanted in ids {
            if items.contains_key(&wanted) {
                continue;
            }
            let mut candidates = Vec::new();
            for index in &indices {
                for entry in index.entries.iter().filter(|e| {
                    e.key.type_id == 0x545ac67a && e.key.instance_id == wanted && !e.deleted()
                }) {
                    candidates.push((index, entry.key));
                }
            }
            if candidates.len() != 1 {
                return Err(format!(
                    "source trope {wanted} has {} explicit-provider candidates",
                    candidates.len()
                ));
            }
            let (index, key) = candidates[0];
            items.insert(wanted, read(index, key, cancel)?);
        }
        let item_list = |name: &str, kind: u32| -> Result<V, String> {
            Ok(V::List(
                vec(required(global_data, name)?)?
                    .iter()
                    .map(|value| {
                        let id = id(value)?;
                        let resource = items.get(&id).ok_or("source trope reference missing")?;
                        let item = root(resource)?;
                        if resource.pin.key.group_id != 0x004dfb84
                            || !matches!(item,D::Object{schema,..}if schema=="RelationshipBit")
                        {
                            return Err("unexpected source trope item group/schema".into());
                        }
                        Ok(object([
                            (
                                "directionality",
                                V::Number(u32v(required(item, "directionality")?)? as f64),
                            ),
                            (
                                "display_name",
                                object([(
                                    "hash",
                                    V::Number(u32v(required(item, "display_name")?)? as f64),
                                )]),
                            ),
                            ("trope_id", V::String(id.to_string())),
                            ("trope_type", V::Number(kind as f64)),
                        ]))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            ))
        };
        let relationships = |name: &str| -> Result<V, String> {
            Ok(V::List(
                vec(required(global_data, name)?)?
                    .iter()
                    .map(|value| {
                        Ok(object([
                            (
                                "key",
                                V::Number(u32v(required(value, "RelationshipType")?)? as f64),
                            ),
                            (
                                "available_tropes",
                                V::List(
                                    vec(required(value, "AvailableTropes")?)?
                                        .iter()
                                        .map(|v| Ok(V::String(id(v)?.to_string())))
                                        .collect::<Result<Vec<_>, String>>()?,
                                ),
                            ),
                        ]))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            ))
        };
        let bits = V::List(
            vec(required(global_data, "RELATIONSHIP_TYPE_TO_BIT")?)?
                .iter()
                .map(|v| {
                    Ok(object([
                        (
                            "key",
                            V::Number(u32v(required(v, "RelationshipType")?)? as f64),
                        ),
                        (
                            "relationship_bit",
                            V::String(id(required(v, "RelationshipBit")?)?.to_string()),
                        ),
                    ]))
                })
                .collect::<Result<Vec<_>, String>>()?,
        );
        let payload = object([
            ("sim_trope_items", item_list("TROPES", 0)?),
            ("sim_romance_trope_items", item_list("ROMANCE_TROPES", 1)?),
            (
                "relationship_type_to_tropes",
                relationships("RELATIONSHIP_TYPE_TO_AVAILABLE_TROPES")?,
            ),
            (
                "romance_relationship_type_to_tropes",
                relationships("RELATIONSHIP_TYPE_TO_AVAILABLE_ROMANCE_TROPES")?,
            ),
            ("relationship_type_to_bit", bits),
        ]);
        let mut resources = vec![global];
        resources.extend(items.into_values());
        Ok(Self { payload, resources })
    }
    pub fn pins(&self) -> Vec<SourcePin> {
        self.resources.iter().map(|r| r.pin.clone()).collect()
    }
}
