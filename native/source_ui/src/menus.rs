// SPDX-License-Identifier: GPL-3.0-or-later
//! Original CAS static menu projections. The installed SimData and every
//! unknown field remain authority; contextual facts must come from the editor.
use crate::HostValue as V;
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use external_cas_fidelity::installed::SourcePin;
use external_cas_room::{DataDocument, DataValue as D, parse_simdata};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CriterionContext {
    /// Exact native AgeGender bitfield; not a UI gender ordinal.
    pub age_gender: Option<u32>,
    pub species: Option<i32>,
    pub occult_flags: Option<u32>,
    pub active_occult_form: Option<i32>,
    /// Source criterion names (e.g. is_new_sim, is_mannequin,
    /// is_full_edit_mode). An absent observation is an explicit error.
    pub booleans: BTreeMap<String, bool>,
    /// Explicitly resolved source pack capabilities, not account entitlement.
    /// None means unknown; an observed empty set remains distinct.
    pub available_packs: Option<BTreeSet<i64>>,
}
pub struct CasMenuSnapshot {
    pub pin: SourcePin,
    pub original_bytes: Vec<u8>,
    pub document: DataDocument,
}
fn field<'a>(d: &'a D, n: &str) -> Result<&'a D, String> {
    d.field(n)?
        .ok_or_else(|| format!("Source CAS menu field {n} absent"))
}
fn vector(d: &D) -> Result<&[D], String> {
    if let D::Vector(v) = d {
        Ok(v)
    } else {
        Err("Source CAS menu vector type differs".into())
    }
}
fn number(d: &D) -> Result<i64, String> {
    match d {
        D::Signed64(v) => Ok(*v),
        D::Unsigned32(v) => Ok(i64::from(*v)),
        _ => Err("Source CAS menu numeric type differs".into()),
    }
}
fn obj(fields: impl IntoIterator<Item = (&'static str, V)>) -> V {
    V::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn fnv1(name: &str) -> u32 {
    name.bytes()
        .fold(0x811c9dc5, |h, b| h.wrapping_mul(0x01000193) ^ u32::from(b))
}
pub const MENU_SERVICES: [&str; 3] = [
    "CasGetCopyFiltersData",
    "CasGetRandomizationFiltersData",
    "CasGetAddSimMenuData",
];
impl CasMenuSnapshot {
    pub fn reply(&self, name: &str, context: &CriterionContext) -> Option<Result<V, String>> {
        match name {
            "CasGetCopyFiltersData" => Some(self.copy_filters(context)),
            "CasGetRandomizationFiltersData" => Some(self.randomization_filters(context)),
            "CasGetAddSimMenuData" => Some(self.add_sim_menu(context)),
            _ => None,
        }
    }
    pub fn load(provider: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("CAS menu loading cancelled".into());
        }
        let index = PackageIndex::open(provider).map_err(|e| e.to_string())?;
        let key = ResourceKey::new(0x545ac67a, 0x006ca304, 0x5e2c8d43e4f3df6c);
        let rows = index
            .matching(key)
            .filter(|e| !e.deleted())
            .collect::<Vec<_>>();
        if rows.len() != 1 {
            return Err("Source CAS menu resource absent/ambiguous".into());
        }
        let row = rows[0];
        let raw = index
            .read_resource(
                row,
                ReadLimits {
                    max_resource_bytes: 16 * 1024 * 1024,
                },
            )
            .map_err(|e| e.to_string())?;
        let pin = SourcePin {
            provider_path: index.path.clone(),
            key,
            ordinal: row.ordinal,
            index_sha256: index
                .index_sha256
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            payload_sha256: format!("{:x}", Sha256::digest(&raw)),
        };
        let document = parse_simdata(&raw, cancel)?;
        let result = Self {
            pin,
            original_bytes: raw,
            document,
        };
        result.root()?;
        Ok(result)
    }
    fn root(&self) -> Result<&D, String> {
        let mut roots =
            self.document.instances.iter().filter(
                |(_, d)| matches!(d,D::Object{schema,..}if schema=="cas.cas_tuning.CASTuning"),
            );
        let root = roots.next().ok_or("Source CASTuning schema absent")?;
        if roots.next().is_some() {
            return Err("Source CASTuning schema ambiguous".into());
        }
        Ok(&root.1)
    }
    fn take(&self, at: usize, len: usize) -> Result<&[u8], String> {
        self.original_bytes
            .get(at..at.checked_add(len).ok_or("CAS menu cell extent overflow")?)
            .ok_or_else(|| "Truncated source CAS menu cell".into())
    }
    fn opaque(&self, d: &D, typ: u16, len: usize) -> Result<(usize, &[u8]), String> {
        let D::Opaque {
            data_type,
            source_offset,
            bytes,
        } = d
        else {
            return Err("Source CAS menu raw cell type differs".into());
        };
        if *data_type != typ || bytes.len() != len {
            return Err("Source CAS menu raw cell width/type differs".into());
        }
        let raw = self.take(*source_offset, len)?;
        if raw != bytes {
            return Err("Source CAS menu retained cell differs from original bytes".into());
        }
        Ok((*source_offset, raw))
    }
    fn i32(&self, d: &D) -> Result<i32, String> {
        let (_, raw) = self.opaque(d, 6, 4)?;
        Ok(i32::from_le_bytes(raw.try_into().unwrap()))
    }
    fn relative(&self, at: usize) -> Result<Option<usize>, String> {
        let delta = i32::from_le_bytes(self.take(at, 4)?.try_into().unwrap());
        if delta == i32::MIN {
            return Ok(None);
        }
        let p = (at as i64)
            .checked_add(i64::from(delta))
            .filter(|p| *p >= 0 && *p <= self.original_bytes.len() as i64)
            .ok_or("Source CAS menu relative extent")?;
        Ok(Some(p as usize))
    }
    fn text(&self, d: &D) -> Result<String, String> {
        let (at, _) = self.opaque(d, 11, 4)?;
        let Some(start) = self.relative(at)? else {
            return Ok(String::new());
        };
        let rest = self
            .original_bytes
            .get(start..)
            .ok_or("Source menu string extent")?;
        let len = rest
            .iter()
            .take(65537)
            .position(|b| *b == 0)
            .ok_or("Source menu string terminator/budget")?;
        std::str::from_utf8(&rest[..len])
            .map(str::to_owned)
            .map_err(|_| "Source menu string UTF8 differs".into())
    }
    fn icon(&self, d: &D) -> Result<V, String> {
        let (_, raw) = self.opaque(d, 19, 16)?;
        Ok(V::String(
            u64::from_le_bytes(raw[..8].try_into().unwrap()).to_string(),
        ))
    }
    fn variant(&self, d: &D) -> Result<(u32, usize), String> {
        let (at, raw) = self.opaque(d, 21, 8)?;
        let target = self.relative(at)?.ok_or("Source criterion target absent")?;
        Ok((u32::from_le_bytes(raw[4..8].try_into().unwrap()), target))
    }
    fn enum_list(&self, at: usize) -> Result<Vec<i64>, String> {
        let count = u32::from_le_bytes(self.take(at + 4, 4)?.try_into().unwrap()) as usize;
        if count == 0 {
            return Ok(Vec::new());
        }
        let target = self
            .relative(at)?
            .ok_or("Source criterion vector target absent")?;
        let raw = self.take(
            target,
            count
                .checked_mul(8)
                .ok_or("Source criterion vector extent overflow")?,
        )?;
        Ok(raw
            .chunks_exact(8)
            .map(|v| i64::from_le_bytes(v.try_into().unwrap()))
            .collect())
    }
    fn criterion(&self, criterion: &D, context: &CriterionContext) -> Result<bool, String> {
        let (hash, target) = self.variant(field(criterion, "criterion")?)?;
        let exclude = field(criterion, "exclude_if_matched")?.bool_value()?;
        let matches = if hash == fnv1("species") {
            self.enum_list(target)?.contains(&i64::from(
                context.species.ok_or("Source species context unobserved")?,
            ))
        } else if hash == fnv1("age") {
            self.enum_list(target)?.contains(&i64::from(
                context
                    .age_gender
                    .ok_or("Source AgeGender context unobserved")?
                    & 0xff,
            ))
        } else if hash == fnv1("gender") {
            self.enum_list(target)?.contains(&i64::from(
                context
                    .age_gender
                    .ok_or("Source AgeGender context unobserved")?
                    & 0x3000,
            ))
        } else if hash == fnv1("active_occult_form") {
            self.enum_list(target)?.contains(&i64::from(
                context
                    .active_occult_form
                    .ok_or("Source active occult form context unobserved")?,
            ))
        } else if hash == fnv1("occult_type") {
            let flags = context
                .occult_flags
                .ok_or("Source occult type context unobserved")?;
            let selected = [2, 4, 8, 16, 32, 64]
                .into_iter()
                .find(|b| flags & b != 0)
                .unwrap_or(1);
            self.enum_list(target)?.contains(&i64::from(selected))
        } else if hash == fnv1("allowed_packs") {
            let packs = context
                .available_packs
                .as_ref()
                .ok_or("Source pack capability context unobserved")?;
            self.enum_list(target)?.iter().any(|id| packs.contains(id))
        } else {
            let name = ["is_mannequin", "is_new_sim", "is_full_edit_mode"]
                .into_iter()
                .find(|name| fnv1(name) == hash)
                .ok_or_else(|| format!("Source CAS criterion {hash:08X} is unsupported"))?;
            let actual = context
                .booleans
                .get(name)
                .ok_or_else(|| format!("Source {name} context unobserved"))?;
            let expected = match self.take(target, 1)?[0] {
                0 => false,
                1 => true,
                _ => return Err("Source criterion Boolean differs".into()),
            };
            *actual == expected
        };
        Ok(matches != exclude)
    }
    fn eligible(&self, data: &D, context: &CriterionContext) -> Result<bool, String> {
        for c in vector(field(data, "criteria")?)? {
            if !self.criterion(c, context)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn flags(d: &D) -> Result<u32, String> {
        vector(d)?
            .iter()
            .try_fold(0u32, |bits, v| Ok(bits | number(v)? as u32))
    }
    fn filter_packet(&self, context: &CriterionContext, randomization: bool) -> Result<V, String> {
        let mut items = Vec::new();
        let mut all = 0u32;
        let name = if randomization {
            "CAS_RANDOMIZE_FILTERS"
        } else {
            "CAS_COPY_FILTERS"
        };
        for entry in vector(field(self.root()?, name)?)? {
            let data = if randomization {
                field(entry, "value")?
            } else {
                entry
            };
            if !self.eligible(data, context)? {
                continue;
            }
            let flags = Self::flags(field(data, "flags")?)?;
            all |= flags;
            let sort = if randomization {
                self.i32(field(entry, "key")?)?
            } else {
                i32::try_from(items.len() + 1).map_err(|_| "Source filter order overflow")?
            };
            items.push(obj([
                (
                    "name",
                    obj([("hash", V::Number(number(field(data, "name")?)? as f64))]),
                ),
                ("uiSortOrder", V::Number(f64::from(sort))),
                (
                    "required_flags",
                    V::Number(f64::from(Self::flags(field(data, "required_flags")?)?)),
                ),
                ("flags", V::Number(f64::from(flags))),
            ]));
        }
        Ok(obj([
            ("flagItems", V::List(items)),
            ("allFlags", V::Number(f64::from(all))),
        ]))
    }
    pub fn copy_filters(&self, context: &CriterionContext) -> Result<V, String> {
        self.filter_packet(context, false)
    }
    pub fn randomization_filters(&self, context: &CriterionContext) -> Result<V, String> {
        self.filter_packet(context, true)
    }
    pub fn add_sim_menu(&self, context: &CriterionContext) -> Result<V, String> {
        let mut items = Vec::new();
        for entry in vector(field(self.root()?, "CAS_ADD_SIM_MENU_DATA")?)? {
            let data = field(entry, "data")?;
            if !self.eligible(data, context)? {
                continue;
            }
            let list = vector(field(data, "limit_genetics_species")?)?
                .iter()
                .map(|v| Ok(V::String(number(v)?.to_string())))
                .collect::<Result<Vec<_>, String>>()?;
            items.push(obj([
                (
                    "uiSortIndex",
                    V::Number(f64::from(self.i32(field(entry, "index")?)?)),
                ),
                (
                    "parent_index",
                    V::Number(f64::from(self.i32(field(data, "parent_index")?)?)),
                ),
                (
                    "action",
                    V::String(number(field(data, "action")?)?.to_string()),
                ),
                (
                    "audio_name",
                    V::String(self.text(field(data, "audio_name")?)?),
                ),
                (
                    "flair_name",
                    V::String(self.text(field(data, "flair_name")?)?),
                ),
                ("icon", self.icon(field(data, "icon")?)?),
                ("icon_selected", self.icon(field(data, "icon_selected")?)?),
                (
                    "species",
                    V::String(number(field(data, "species")?)?.to_string()),
                ),
                (
                    "tooltip",
                    obj([("hash", V::Number(number(field(data, "tooltip")?)? as f64))]),
                ),
                (
                    "tutorial_control_enum",
                    V::String(number(field(data, "tutorial_control_enum")?)?.to_string()),
                ),
                ("limit_genetics_species", V::List(list)),
                (
                    "occult_type",
                    V::Number(f64::from(Self::flags(field(data, "occult_type")?)?)),
                ),
            ]));
        }
        Ok(obj([("menu_items", V::List(items))]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(
        name: &str,
        values: &[i64],
        boolean: Option<bool>,
        exclude: bool,
    ) -> (CasMenuSnapshot, D) {
        let mut raw = vec![0; 24 + 8 * values.len()];
        raw[0..4].copy_from_slice(&16i32.to_le_bytes());
        raw[4..8].copy_from_slice(&fnv1(name).to_le_bytes());
        if let Some(b) = boolean {
            raw[16] = u8::from(b);
        } else {
            raw[16..20].copy_from_slice(&8i32.to_le_bytes());
            raw[20..24].copy_from_slice(&(values.len() as u32).to_le_bytes());
            for (i, v) in values.iter().enumerate() {
                raw[24 + i * 8..32 + i * 8].copy_from_slice(&v.to_le_bytes());
            }
        }
        let criterion = D::Object {
            schema: "CASContextCriterionListEntry".into(),
            schema_hash: 0x1a28f2fe,
            fields: vec![
                ("exclude_if_matched".into(), D::Boolean(u8::from(exclude))),
                (
                    "criterion".into(),
                    D::Opaque {
                        data_type: 21,
                        source_offset: 0,
                        bytes: raw[..8].to_vec(),
                    },
                ),
            ],
        };
        (
            CasMenuSnapshot {
                pin: SourcePin {
                    provider_path: "own-fixture.package".into(),
                    key: ResourceKey::new(0, 0, 0),
                    ordinal: 0,
                    index_sha256: "own fixture".into(),
                    payload_sha256: format!("{:x}", Sha256::digest(&raw)),
                },
                original_bytes: raw,
                document: DataDocument {
                    version: 257,
                    schemas: vec![],
                    instances: vec![],
                    issues: vec![],
                },
            },
            criterion,
        )
    }
    #[test]
    fn source_criterion_bitfields_and_exclusion_do_not_use_ui_ordinals() {
        let c = CriterionContext {
            age_gender: Some(0x2020),
            species: Some(1),
            occult_flags: Some(2 | 32),
            ..Default::default()
        };
        for (name, values) in [
            ("age", vec![32]),
            ("gender", vec![0x2000]),
            ("species", vec![1]),
            ("occult_type", vec![2]),
        ] {
            let (s, d) = fixture(name, &values, None, false);
            assert!(s.criterion(&d, &c).unwrap());
            let (s, d) = fixture(name, &values, None, true);
            assert!(!s.criterion(&d, &c).unwrap());
        }
        let (s, d) = fixture("occult_type", &[32], None, false);
        assert!(
            !s.criterion(&d, &c).unwrap(),
            "original priority chooses first active occult bit, not union membership"
        );
    }
    #[test]
    fn missing_observations_and_unknown_variants_are_refused() {
        let (s, d) = fixture("gender", &[0x2000], None, false);
        assert!(
            s.criterion(&d, &CriterionContext::default())
                .unwrap_err()
                .contains("unobserved")
        );
        let (s, d) = fixture("is_new_sim", &[], Some(false), false);
        assert!(s.criterion(&d, &CriterionContext::default()).is_err());
        let (s, d) = fixture("unqualified_variant", &[], Some(false), false);
        assert!(
            s.criterion(&d, &CriterionContext::default())
                .unwrap_err()
                .contains("unsupported")
        );
        let (s, d) = fixture("allowed_packs", &[3, 17], None, false);
        assert!(s.criterion(&d, &CriterionContext::default()).is_err());
        let c = CriterionContext {
            available_packs: Some(BTreeSet::new()),
            ..Default::default()
        };
        assert!(!s.criterion(&d, &c).unwrap());
        let c = CriterionContext {
            available_packs: Some(BTreeSet::from([17])),
            ..Default::default()
        };
        assert!(s.criterion(&d, &c).unwrap());
    }
    #[test]
    fn truncated_pointers_vectors_and_mutated_retained_cells_fail() {
        let (s, d) = fixture("age", &[32], None, false);
        for end in 0..s.original_bytes.len() {
            let copy = CasMenuSnapshot {
                pin: s.pin.clone(),
                original_bytes: s.original_bytes[..end].to_vec(),
                document: s.document.clone(),
            };
            assert!(
                copy.criterion(
                    &d,
                    &CriterionContext {
                        age_gender: Some(32),
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
        let (mut s, d) = fixture("age", &[32], None, false);
        s.original_bytes[0] = 0xff;
        assert!(
            s.criterion(
                &d,
                &CriterionContext {
                    age_gender: Some(32),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .contains("differs")
        );
        let (mut s, d) = fixture("age", &[32], None, false);
        s.original_bytes[16..20].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(
            s.criterion(
                &d,
                &CriterionContext {
                    age_gender: Some(32),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
