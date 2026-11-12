// SPDX-License-Identifier: GPL-3.0-or-later
//! Native linkage follows pinned SimRipper DisplaySim. Qualification remains
//! separate from decoding: unscanned tuning is not evidence for scale 1.
use super::*;
use external_cas_assets::{scan::Provider, Catalog, PackageReader, ReadLimits, ResourceKey};
use external_cas_scene::{FormSelection, ScenePlan, ScopePolicy};
use external_cas_tray::{AppearanceForm, SimAppearance};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const BGEO: u32 = 0x067caa11;
const DMAP: u32 = 0xdb43e069;
const BOND: u32 = 0x0355e0a6;
const PHYSIQUE: [&str; 9] = [
    "Body_Heavy",
    "Body_Fit",
    "Body_Lean",
    "Body_Bony",
    "Body_Pregnant",
    "Hips_Wide",
    "Hips_Narrow",
    "Waist_Wide",
    "Waist_Narrow",
];
#[derive(Clone, Debug)]
pub enum ModifierScales {
    Unresolved,
    Qualified {
        scales: BTreeMap<u64, f32>,
        evidence: String,
    },
}
#[derive(Clone, Debug)]
pub struct MorphOptions {
    pub provider_choices: BTreeMap<ResourceKey, Provider>,
    pub modifier_scales: ModifierScales,
    pub read_limits: ReadLimits,
}
impl Default for MorphOptions {
    fn default() -> Self {
        Self {
            provider_choices: BTreeMap::new(),
            modifier_scales: ModifierScales::Unresolved,
            read_limits: ReadLimits::default(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct MorphIssue {
    pub source: String,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub struct MorphResource {
    pub key: ResourceKey,
    pub selected: Option<Provider>,
    pub provider_sha256: Vec<(Provider, String)>,
    pub selection: String,
    pub raw: Option<Vec<u8>>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub enum MorphData {
    Bgeo(Bgeo),
    Dmap(Dmap),
    Bond(Bond),
}
#[derive(Clone, Debug)]
pub struct MorphOperation {
    pub source: String,
    pub weight: Option<f32>,
    pub region: u32,
    pub resource: MorphResource,
    pub data: Option<MorphData>,
}
#[derive(Clone, Debug)]
pub struct PhysiqueWeight {
    pub index: usize,
    pub name: String,
    pub value: f32,
    pub applied_by_reference: bool,
}
#[derive(Clone, Debug)]
pub struct MorphPlan {
    pub native_sim_sha256: String,
    pub form: FormSelection,
    pub species: u32,
    pub age: u32,
    pub gender: u32,
    pub frame: u32,
    pub physique: Vec<PhysiqueWeight>,
    pub operations: Vec<MorphOperation>,
    pub link_resources: Vec<MorphResource>,
    pub issues: Vec<MorphIssue>,
    pub complete: bool,
}
struct Resolver<'a, 'b> {
    catalog: &'a Catalog,
    policy: Option<&'b ScopePolicy<'a>>,
    options: &'b MorphOptions,
    readers: BTreeMap<usize, std::result::Result<PackageReader<'a>, String>>,
}
impl<'a, 'b> Resolver<'a, 'b> {
    fn read(&mut self, p: Provider, key: ResourceKey) -> std::result::Result<Vec<u8>, String> {
        let (index, entry) = self.catalog.provider(p).ok_or("Provider missing")?;
        if entry.key != key || entry.deleted() {
            return Err("Provider key changed or deleted".into());
        }
        let r = self
            .readers
            .entry(p.package_index)
            .or_insert_with(|| index.reader().map_err(|e| e.to_string()));
        r.as_mut()
            .map_err(|e| e.clone())?
            .read_resource(entry, self.options.read_limits)
            .map_err(|e| e.to_string())
    }
    fn resolve(&mut self, key: ResourceKey) -> MorphResource {
        let mut result = MorphResource {
            key,
            selected: None,
            provider_sha256: vec![],
            selection: String::new(),
            raw: None,
            error: None,
        };
        let providers = self.catalog.providers(key).to_vec();
        let explicit = self.options.provider_choices.get(&key).copied();
        let selected = explicit.or_else(|| self.policy.and_then(|p| p.resolve_key(key).selected));
        if let Some(p) = selected {
            result.selection = if explicit.is_some() {
                "explicit-provider"
            } else {
                "installed-resource-config-priority"
            }
            .into();
            if !providers.contains(&p) {
                result.error = Some("Selected provider is not a candidate for full TGI".into());
                return result;
            }
            match self.read(p, key) {
                Ok(raw) => {
                    result.provider_sha256.push((p, hash(&raw)));
                    result.selected = Some(p);
                    result.raw = Some(raw);
                }
                Err(e) => result.error = Some(e),
            }
            return result;
        }
        if providers.is_empty() {
            result.error = Some("Missing full TGI in configured catalog".into());
            return result;
        }
        let mut first = None;
        let mut same = true;
        for p in providers {
            match self.read(p, key) {
                Ok(raw) => {
                    result.provider_sha256.push((p, hash(&raw)));
                    if let Some((_, old)) = &first {
                        if old != &raw {
                            same = false;
                        }
                    } else {
                        first = Some((p, raw));
                    }
                }
                Err(e) => {
                    result.error = Some(e);
                    return result;
                }
            }
        }
        if same {
            let (p, raw) = first.unwrap();
            result.selected = Some(p);
            result.raw = Some(raw);
            result.selection = "byte-identical-full-TGI-providers".into();
        } else {
            result.error = Some(
                "Differing full-TGI providers require a qualified priority or explicit choice"
                    .into(),
            );
        }
        result
    }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn form<'a>(
    s: &'a SimAppearance,
    scene: &ScenePlan,
) -> Result<(&'a AppearanceForm, FormSelection)> {
    match scene.resolved_form.as_ref() {
        Some(FormSelection::Base) => Ok((&s.base, FormSelection::Base)),
        Some(FormSelection::OccultIndex(i)) => s
            .occult_forms
            .get(*i)
            .map(|f| (f, FormSelection::OccultIndex(*i)))
            .ok_or_else(|| MorphError::Unresolved("Scene form absent from native Sim".into())),
        _ => Err(MorphError::Unresolved(
            "Scene has no resolved native form".into(),
        )),
    }
}
/// Exact reference prefix; requires native sex instead of the source tool's
/// authoring fallback for invalid gender values.
pub fn physique_prefix(species: u32, age: u32, gender: u32) -> Result<String> {
    let stage = match (species, age) {
        (1, 2) => "p",
        (_, 2) => "c",
        (1, 8 | 16 | 32) => "y",
        (s, 8 | 16 | 32 | 64) if s != 1 => "a",
        (_, 1) => "b",
        (_, 4) => "c",
        (_, 64) => "e",
        (_, 128) => "i",
        _ => return Err(MorphError::Unsupported(format!("Physique age {age}"))),
    };
    let suffix = if species == 1 {
        if age <= 4 {
            "u"
        } else {
            match gender {
                0x1000 => "m",
                0x2000 => "f",
                _ => {
                    return Err(MorphError::Unresolved(
                        "Native physique sex absent/unsupported".into(),
                    ))
                }
            }
        }
    } else {
        match species {
            2 => "d",
            3 => "c",
            4 if age == 4 => "d",
            4 => "l",
            5 => "f",
            6 => "h",
            _ => return Err(MorphError::Unsupported("Physique species".into())),
        }
    };
    Ok(format!("{stage}{suffix}"))
}
fn varint(b: &[u8], at: &mut usize) -> Result<u64> {
    let mut v = 0;
    for i in 0..10 {
        let q = *b.get(*at).ok_or(MorphError::Truncated)?;
        *at += 1;
        if i == 9 && q > 1 {
            return Err(MorphError::Invalid("Protobuf varint overflow".into()));
        }
        v |= ((q & 127) as u64) << (7 * i);
        if q & 128 == 0 {
            return Ok(v);
        }
    }
    Err(MorphError::Invalid("Overlong protobuf varint".into()))
}
fn fields(b: &[u8]) -> Result<Vec<(u32, u8, &[u8])>> {
    let mut out = vec![];
    let mut at = 0;
    while at < b.len() {
        let t = varint(b, &mut at)?;
        let n = (t >> 3) as u32;
        let w = (t & 7) as u8;
        if n == 0 || t >> 3 >= 1 << 29 || out.len() >= 1_000_000 {
            return Err(MorphError::Invalid(
                "Invalid/over-budget native protobuf field".into(),
            ));
        }
        let start = at;
        let len = match w {
            0 => {
                varint(b, &mut at)?;
                out.push((n, w, &b[start..at]));
                continue;
            }
            1 => 8,
            2 => varint(b, &mut at)? as usize,
            5 => 4,
            _ => {
                return Err(MorphError::Unsupported(
                    "Trait protobuf groups retained but unqualified".into(),
                ))
            }
        };
        let start = at;
        at = at
            .checked_add(len)
            .filter(|&e| e <= b.len())
            .ok_or(MorphError::Truncated)?;
        out.push((n, w, &b[start..at]));
    }
    Ok(out)
}
fn one<'a>(b: &'a [u8], n: u32) -> Result<Option<&'a [u8]>> {
    let f = fields(b)?;
    let v = f.iter().filter(|x| x.0 == n).collect::<Vec<_>>();
    match v.as_slice() {
        [] => Ok(None),
        [v] if v.1 == 2 => Ok(Some(v.2)),
        _ => Err(MorphError::Invalid(
            "Repeated/wrong-wire native trait hierarchy".into(),
        )),
    }
}
fn native_frame(s: &SimAppearance, species: u32, age: u32, sex: u32) -> Result<u32> {
    let Some(a) = one(&s.base.native.bytes, 30)? else {
        return Ok(sex);
    };
    let Some(t) = one(a, 10)? else { return Ok(sex) };
    let mut traits = vec![];
    for (n, w, b) in fields(t)? {
        if n != 1 {
            continue;
        }
        if w != 0 && w != 2 {
            return Err(MorphError::Invalid("Trait ID wire changed".into()));
        }
        let mut at = 0;
        while at < b.len() {
            traits.push(varint(b, &mut at)?);
        }
    }
    if species == 1 && age > 4 && age != 128 {
        if sex == 0x1000 && traits.contains(&136878) {
            return Ok(0x2000);
        }
        if sex == 0x2000 && traits.contains(&136877) {
            return Ok(0x1000);
        }
    }
    Ok(sex)
}
fn push_resource(
    r: &mut Resolver<'_, '_>,
    p: &mut MorphPlan,
    source: &str,
    key: ResourceKey,
    weight: Option<f32>,
    region: u32,
) {
    if key.instance_id == 0 {
        return;
    }
    let resource = r.resolve(key);
    let parsed = resource
        .raw
        .as_deref()
        .map(|raw| match key.type_id {
            BGEO => Bgeo::parse(raw).map(MorphData::Bgeo),
            DMAP => Dmap::parse(raw).map(MorphData::Dmap),
            BOND => Bond::parse(raw).map(MorphData::Bond),
            _ => Err(MorphError::Unsupported(format!(
                "Morph link resource type {:08X}",
                key.type_id
            ))),
        })
        .transpose();
    let data = match parsed {
        Ok(d) => d,
        Err(e) => {
            p.issues.push(MorphIssue {
                source: source.into(),
                detail: e.to_string(),
            });
            None
        }
    };
    if let Some(e) = &resource.error {
        p.issues.push(MorphIssue {
            source: source.into(),
            detail: e.clone(),
        });
    }
    p.operations.push(MorphOperation {
        source: source.into(),
        weight,
        region,
        resource,
        data,
    });
}
fn add_links(
    r: &mut Resolver<'_, '_>,
    p: &mut MorphPlan,
    source: &str,
    bgeo: &[ResourceKey],
    shape: ResourceKey,
    normal: ResourceKey,
    bond: Option<ResourceKey>,
    weight: Option<f32>,
    region: u32,
) {
    if let Some(k) = bgeo.first() {
        push_resource(r, p, source, *k, weight, region);
    }
    push_resource(r, p, source, shape, weight, region);
    push_resource(r, p, source, normal, weight, region);
    if let Some(k) = bond {
        push_resource(r, p, source, k, weight, region);
    }
}
pub fn build_morph_plan<'a>(
    s: &SimAppearance,
    scene: &ScenePlan,
    catalog: &'a Catalog,
    policy: Option<&ScopePolicy<'a>>,
    options: &MorphOptions,
) -> Result<MorphPlan> {
    if let ModifierScales::Qualified { scales, .. } = &options.modifier_scales {
        if scales.values().any(|v| !v.is_finite()) {
            return Err(MorphError::Invalid(
                "Nonfinite caller-qualified modifier scale".into(),
            ));
        }
    }
    if hash(&s.base.native.bytes) != scene.native_sim_sha256 {
        return Err(MorphError::Invalid(
            "Scene belongs to a different native Sim revision".into(),
        ));
    }
    let (f, selection) = form(s, scene)?;
    let species = s.extended_species.filter(|&v| v != 0).unwrap_or(1);
    let age = s
        .age
        .ok_or_else(|| MorphError::Unresolved("Native age absent".into()))?;
    let gender = s
        .gender
        .ok_or_else(|| MorphError::Unresolved("Native gender absent".into()))?;
    let frame = native_frame(s, species, age, gender)?;
    let mut p = MorphPlan {
        native_sim_sha256: scene.native_sim_sha256.clone(),
        form: selection,
        species,
        age,
        gender,
        frame,
        physique: vec![],
        operations: vec![],
        link_resources: vec![],
        issues: vec![],
        complete: false,
    };
    let mut r = Resolver {
        catalog,
        policy,
        options,
        readers: BTreeMap::new(),
    };
    let prefix = physique_prefix(species, age, frame)?;
    // DisplaySim adds elder, then werewolf basis maps before physique maps.
    // Use the scene's resolved native rig key so selecting a base form does
    // not accidentally inherit another form's current-occult flag.
    let werewolf = scene
        .rig
        .as_ref()
        .map(|r| {
            r.requested_keys
                .contains(&ResourceKey::new(0x8eaf13de, 0, 0x60faa42f9b0b4e39))
        })
        .unwrap_or(false);
    for (enabled, stem, source) in [
        (
            species == 1 && age == 64,
            format!("e{}Body_Average", if frame == 0x1000 { "m" } else { "f" }),
            "native elder basis",
        ),
        (
            species == 1 && werewolf,
            format!("y{}Body_Werewolf", if frame == 0x1000 { "m" } else { "f" }),
            "native werewolf basis",
        ),
    ] {
        if enabled {
            for suffix in ["_Shape", "_Normals"] {
                push_resource(
                    &mut r,
                    &mut p,
                    source,
                    ResourceKey::new(
                        DMAP,
                        0,
                        external_cas_scene::fnv64(&format!("{stem}{suffix}"))
                            .map_err(MorphError::Invalid)?,
                    ),
                    Some(1.0),
                    99,
                );
            }
        }
    }
    if species != 1 {
        p.issues.push(MorphIssue {
            source: "native physique".into(),
            detail: "Animal physique-to-SMOD mapping not yet qualified".into(),
        });
    } else if let Some(raw) = &f.physique {
        for (i, value) in raw.split(',').filter(|s| !s.is_empty()).enumerate() {
            let weight = value.trim().parse::<f32>().map_err(|_| {
                MorphError::Invalid(
                    "Native physique value is not a finite invariant-culture number".into(),
                )
            })?;
            if !weight.is_finite() {
                return Err(MorphError::Invalid("Nonfinite physique".into()));
            }
            let name = PHYSIQUE.get(i).ok_or_else(|| {
                MorphError::Unsupported("Native physique has additional unknown entries".into())
            })?;
            p.physique.push(PhysiqueWeight {
                index: i,
                name: (*name).into(),
                value: weight,
                applied_by_reference: weight > 0.0,
            });
            if weight > 0.0 {
                for suffix in ["_Shape", "_Normals"] {
                    let key = ResourceKey::new(
                        DMAP,
                        0,
                        external_cas_scene::fnv64(&format!("{prefix}{name}{suffix}"))
                            .map_err(MorphError::Invalid)?,
                    );
                    push_resource(
                        &mut r,
                        &mut p,
                        &format!("physique[{i}] {name}"),
                        key,
                        Some(weight),
                        99,
                    );
                }
            }
        }
    } else {
        p.issues.push(MorphIssue {
            source: "native physique".into(),
            detail: "Physique field absent; no neutral preset substituted".into(),
        });
    }
    if let Some(facial) = &f.facial {
        let mut by_region = BTreeMap::new();
        for id in &facial.sculpts {
            let resource = r.resolve(ResourceKey::new(0x9d1ab874, 0, *id));
            match resource.raw.as_deref().map(Sculpt::parse).transpose() {
                Ok(Some(sculpt)) => {
                    if !sculpt.unparsed_suffix.is_empty() {
                        p.issues.push(MorphIssue {
                            source: format!("sculpt {id:016X}"),
                            detail: "SCULPT suffix retained, application unqualified".into(),
                        });
                    }
                    by_region.insert(sculpt.region, (*id, sculpt));
                }
                Ok(None) => p.issues.push(MorphIssue {
                    source: format!("sculpt {id:016X}"),
                    detail: resource.error.clone().unwrap_or("No SCULPT payload".into()),
                }),
                Err(e) => p.issues.push(MorphIssue {
                    source: format!("sculpt {id:016X}"),
                    detail: e.to_string(),
                }),
            }
            p.link_resources.push(resource);
        }
        for (region, (id, sculpt)) in by_region {
            add_links(
                &mut r,
                &mut p,
                &format!("sculpt {id:016X}"),
                &sculpt.bgeo,
                sculpt.shape,
                sculpt.normal,
                sculpt.bond,
                Some(1.0),
                region,
            );
        }
        for (kind, mods) in [
            ("face", &facial.face_modifiers),
            ("body", &facial.body_modifiers),
        ] {
            for m in mods {
                let (id, amount) = match (m.key, m.amount) {
                    (Some(id), Some(amount)) => (id, amount),
                    _ => {
                        p.issues.push(MorphIssue {
                            source: kind.into(),
                            detail: "Native modifier ID/amount absent".into(),
                        });
                        continue;
                    }
                };
                if !amount.is_finite() {
                    return Err(MorphError::Invalid(
                        "Nonfinite native modifier amount".into(),
                    ));
                }
                let source = format!("{kind} modifier {id:016X}");
                let weight = match &options.modifier_scales {
                    ModifierScales::Unresolved => None,
                    ModifierScales::Qualified { scales, evidence }
                        if !evidence.trim().is_empty() =>
                    {
                        Some(amount * scales.get(&id).copied().unwrap_or(1.0))
                    }
                    _ => None,
                };
                if weight.is_none() {
                    p.issues.push(MorphIssue {
                        source: source.clone(),
                        detail: "CAS modifier tuning scale is unresolved; native amount retained"
                            .into(),
                    });
                }
                let resource = r.resolve(ResourceKey::new(0xc5f6763e, 0, id));
                match resource.raw.as_deref().map(SimModifier::parse).transpose() {
                    Ok(Some(smod)) => {
                        if !smod.unparsed_suffix.is_empty() {
                            p.issues.push(MorphIssue {
                                source: source.clone(),
                                detail: "SMOD suffix retained, application unqualified".into(),
                            });
                        }
                        add_links(
                            &mut r,
                            &mut p,
                            &source,
                            &smod.bgeo,
                            smod.shape,
                            smod.normal,
                            Some(smod.bond),
                            weight,
                            smod.region,
                        );
                    }
                    Ok(None) => p.issues.push(MorphIssue {
                        source: source.clone(),
                        detail: resource.error.clone().unwrap_or("No SMOD payload".into()),
                    }),
                    Err(e) => p.issues.push(MorphIssue {
                        source: source.clone(),
                        detail: e.to_string(),
                    }),
                }
                p.link_resources.push(resource);
            }
        }
        if !facial.aged_face_modifiers.is_empty() || !facial.aged_body_modifiers.is_empty() {
            p.issues.push(MorphIssue {
                source: "aged native modifiers".into(),
                detail: "Preserved; pinned reference does not consume these fields".into(),
            });
        }
    } else {
        p.issues.push(MorphIssue {
            source: "native facial customization".into(),
            detail: "Native facial field absent; no default face substituted".into(),
        });
    }
    p.issues.push(MorphIssue{source:"complete morphology gate".into(),detail:"Native tuning, frame/clothing conversion, pregnancy, mesh eligibility and combined morph ordering require qualification; plan is dependency evidence, not complete native Sim acceptance".into()});
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use external_cas_scene::{resolve_scene, SceneOptions};
    use external_cas_tray::NativeMessage;
    fn message(bytes: Vec<u8>) -> NativeMessage {
        NativeMessage {
            sha256: hash(&bytes),
            bytes,
            fields: vec![],
        }
    }
    fn sim(bytes: Vec<u8>, physique: Option<&str>) -> SimAppearance {
        SimAppearance {
            sim_index: 0,
            sim_id: 1,
            gender: Some(0x1000),
            age: Some(16),
            extended_species: Some(1),
            current_outfit_category: None,
            current_outfit_index: None,
            custom_texture: None,
            pelt_layers: None,
            occult_types: None,
            current_occult_types: None,
            occult_form_available: None,
            base: AppearanceForm {
                occult_form_index: None,
                occult_type: None,
                skin_tone: None,
                skin_tone_value_shift: None,
                physique: physique.map(str::to_string),
                facial: None,
                genetics: None,
                outfits: vec![],
                current_outfit_match: None,
                native: message(bytes),
            },
            occult_forms: vec![],
        }
    }
    fn vi(mut n: u64) -> Vec<u8> {
        let mut b = vec![];
        while n > 127 {
            b.push((n as u8 & 127) | 128);
            n >>= 7;
        }
        b.push(n as u8);
        b
    }
    fn blob(n: u64, b: Vec<u8>) -> Vec<u8> {
        let mut out = vi(n * 8 + 2);
        out.extend(vi(b.len() as u64));
        out.extend(b);
        out
    }
    #[test]
    fn native_frame_traits_exact_path_and_infant_exception() {
        let mut tracker = vec![8];
        tracker.extend(vi(136878));
        let s = sim(blob(30, blob(10, tracker)), None);
        assert_eq!(native_frame(&s, 1, 16, 0x1000).unwrap(), 0x2000);
        assert_eq!(native_frame(&s, 1, 128, 0x1000).unwrap(), 0x1000);
        assert_eq!(native_frame(&s, 2, 16, 0x1000).unwrap(), 0x1000);
        let malformed = sim(vec![0xf2, 1, 8], None);
        assert!(native_frame(&malformed, 1, 16, 0x1000).is_err());
    }
    #[test]
    fn physique_names_and_weights_follow_native_string_without_default() {
        let s = sim(vec![], Some("0.5,,0,-0.25"));
        let c = Catalog::default();
        let scene = resolve_scene(&s, &c, &SceneOptions::default());
        let p = build_morph_plan(&s, &scene, &c, None, &MorphOptions::default()).unwrap();
        assert_eq!(p.physique.len(), 3);
        assert_eq!(p.physique[0].name, "Body_Heavy");
        assert_eq!(p.physique[2].name, "Body_Lean");
        assert_eq!(p.physique[2].value, -0.25);
        assert!(!p.physique[2].applied_by_reference);
        assert_eq!(p.operations.len(), 2);
        assert!(p.operations.iter().all(|o| o.resource.error.is_some()));
        assert!(!p.complete);
        let absent = sim(vec![], None);
        let p = build_morph_plan(
            &absent,
            &resolve_scene(&absent, &c, &SceneOptions::default()),
            &c,
            None,
            &MorphOptions::default(),
        )
        .unwrap();
        assert!(p.physique.is_empty());
        assert!(p
            .issues
            .iter()
            .any(|i| i.detail.contains("Physique field absent")));
    }
    #[test]
    fn malformed_unknown_physique_and_stale_revision_fail() {
        let c = Catalog::default();
        for raw in ["NaN", "1,1,1,1,1,1,1,1,1,1"] {
            let s = sim(vec![], Some(raw));
            let scene = resolve_scene(&s, &c, &SceneOptions::default());
            assert!(build_morph_plan(&s, &scene, &c, None, &MorphOptions::default()).is_err());
        }
        let s = sim(vec![], Some("0"));
        let mut scene = resolve_scene(&s, &c, &SceneOptions::default());
        scene.native_sim_sha256 = "stale".into();
        assert!(build_morph_plan(&s, &scene, &c, None, &MorphOptions::default()).is_err());
    }
    #[test]
    fn prefix_uses_true_age_species_and_frame() {
        for (species, age, gender, name) in [
            (1, 2, 0x1000, "pu"),
            (1, 4, 0x2000, "cu"),
            (1, 16, 0x1000, "ym"),
            (1, 64, 0x2000, "ef"),
            (1, 128, 0x1000, "im"),
            (4, 4, 0x1000, "cd"),
            (4, 16, 0x1000, "al"),
        ] {
            assert_eq!(physique_prefix(species, age, gender).unwrap(), name);
        }
        assert!(physique_prefix(1, 16, 0).is_err());
        assert!(physique_prefix(1, 3, 0x1000).is_err());
    }
}
