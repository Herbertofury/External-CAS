// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual combined CAS tuning. Missing class defaults and projection/shader
//! conventions are not invented; the complete resolved record remains available.
use crate::*;
use external_cas_assets::ResourceKey;
use external_cas_character::{ResolvedTuningNode, ResolvedTuningRecord};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct TuningFloat {
    pub bits: u32,
    pub lexical: String,
}
impl TuningFloat {
    pub fn value(&self) -> f32 {
        f32::from_bits(self.bits)
    }
    pub fn finite(&self) -> Result<f32> {
        let v = self.value();
        if v.is_finite() {
            Ok(v)
        } else {
            Err("nonfinite applied CAS tuning value".into())
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct CameraParameters {
    pub target: [TuningFloat; 3],
    pub fov: TuningFloat,
    pub distance: TuningFloat,
    pub orbital_rotation: TuningFloat,
    pub pitch: Option<TuningFloat>,
}
impl CameraParameters {
    /// Coordinates only. Angle sign, horizontal/vertical FOV and Sim orientation
    /// must be independently qualified before constructing the native camera.
    pub fn target_relative_to(&self, origin: [f32; 3]) -> Result<[f32; 3]> {
        if origin.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite explicit camera origin".into());
        }
        Ok([
            self.target[0].finite()? - origin[0],
            self.target[1].finite()? - origin[1],
            self.target[2].finite()? - origin[2],
        ])
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct CameraPoseOverride {
    pub pose: String,
    pub source_pose_id: Option<i64>,
    pub camera: CameraParameters,
}
#[derive(Debug, Clone, Serialize)]
pub struct CameraRegion {
    pub region: String,
    pub source_region_id: Option<i64>,
    pub camera: CameraParameters,
    pub pose_overrides: Vec<CameraPoseOverride>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct CameraInteraction {
    pub use_lerp_camera: Option<bool>,
    pub mouse_wheel_zoom_speed_lerp: Option<TuningFloat>,
    pub key_zoom_speed_lerp: Option<TuningFloat>,
    pub mouse_wheel_zoom_speed_velocity: Option<TuningFloat>,
    pub key_zoom_speed_velocity: Option<TuningFloat>,
    pub velocity_loss_rate: Option<TuningFloat>,
    pub velocity_cap: Option<TuningFloat>,
    pub ease_in_pt: Option<TuningFloat>,
    pub ease_out_pt: Option<TuningFloat>,
    pub lateral_lerp_duration: Option<TuningFloat>,
    pub lateral_lerp_ease_in_out: Option<TuningFloat>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CameraTuning {
    pub instance: u64,
    pub name: String,
    pub resource_namespace: String,
    pub source_pin: RoomSourcePin,
    pub ages: Vec<String>,
    pub species: Vec<String>,
    pub occult: Vec<String>,
    pub cas_mode: Vec<String>,
    pub cameras: Vec<CameraRegion>,
    pub interaction: CameraInteraction,
    pub simdata_pin: Option<RoomSourcePin>,
    /// Retains unrecognized fields and all explicit source text; absent class
    /// defaults are not materialized from another age/species record.
    pub source_record: ResolvedTuningNode,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct CasKeyLight {
    pub enable: Option<bool>,
    pub height: Option<TuningFloat>,
    pub angle: Option<TuningFloat>,
    pub color: Option<[TuningFloat; 3]>,
    pub intensity: Option<TuningFloat>,
    pub specular_intensity: Option<TuningFloat>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct CasRimLight {
    pub enable: Option<bool>,
    pub color: Option<[TuningFloat; 3]>,
    pub intensity: Option<TuningFloat>,
    pub falloff: Option<TuningFloat>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LightingTuning {
    pub instance: u64,
    pub name: String,
    pub resource_namespace: String,
    pub source_pin: RoomSourcePin,
    pub light_base_height: Option<TuningFloat>,
    pub key_lights: [CasKeyLight; 2],
    pub rim_lights: [CasRimLight; 2],
    pub ambient_min: Option<TuningFloat>,
    pub ambient_max: Option<TuningFloat>,
    pub source_record: ResolvedTuningNode,
    pub simdata_pin: Option<RoomSourcePin>,
}
#[derive(Debug, Clone)]
pub struct CasDataResource {
    pub source: Arc<RoomResource>,
    pub document: DataDocument,
}
#[derive(Debug, Clone, Serialize)]
pub struct LogicalTuningCandidate {
    pub source_pin: RoomSourcePin,
    pub priority: Option<i32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LogicalTuningDecision {
    pub class: String,
    pub instance: u64,
    pub selected: RoomSourcePin,
    pub candidates: Vec<LogicalTuningCandidate>,
    pub rule: String,
}
#[derive(Debug, Clone)]
pub struct CasTuningScene {
    pub sources: Vec<Arc<RoomResource>>,
    pub cameras: Vec<CameraTuning>,
    pub lighting: Vec<LightingTuning>,
    pub other_records: Vec<(RoomSourcePin, ResolvedTuningRecord)>,
    pub provider_decisions: Vec<RoomProviderDecision>,
    pub config_pins: Vec<(std::path::PathBuf, String)>,
    pub game_version: Option<String>,
    pub issues: Vec<RoomIssue>,
    pub simdata: Vec<CasDataResource>,
    pub logical_decisions: Vec<LogicalTuningDecision>,
}

/// Source Client_CASTuning.PresetRotation, separate from camera orbital tuning.
/// Values are data only; the native selector/easing and angle conventions must
/// be independently qualified before a host applies them to a scene.
#[derive(Debug, Clone, Serialize)]
pub struct PresetRotationTuning {
    pub instance: u64,
    pub name: String,
    pub source_pin: RoomSourcePin,
    pub simdata_pin: Option<RoomSourcePin>,
    pub view_angles: Option<[TuningFloat; 6]>,
    pub lerp_speed_mult: Option<TuningFloat>,
    pub rotate_key_speed: Option<TuningFloat>,
    pub rotate_key_speed_fast: Option<TuningFloat>,
    pub rotate_continuous_speed: Option<TuningFloat>,
    pub rotate_continuous_ease_in: Option<TuningFloat>,
}

fn preset_rotation(
    record: &ResolvedTuningRecord,
    pin: &RoomSourcePin,
    data: Option<&CasDataResource>,
) -> Result<PresetRotationTuning> {
    let node = field(&record.root, "PresetRotation")?;
    let mut result = PresetRotationTuning {
        instance: record.instance,
        name: record.root.attrs.get("n").cloned().unwrap_or_default(),
        source_pin: pin.clone(),
        simdata_pin: None,
        view_angles: None,
        lerp_speed_mult: None,
        rotate_key_speed: None,
        rotate_key_speed_fast: None,
        rotate_continuous_speed: None,
        rotate_continuous_ease_in: None,
    };
    if let Some(node) = node {
        if let Some(angles) = field(node, "ViewAngles")? {
            let values = angles
                .children
                .iter()
                .map(number)
                .collect::<Result<Vec<_>>>()?;
            result.view_angles = Some(
                values
                    .try_into()
                    .map_err(|_| "source PresetRotation ViewAngles does not contain six values")?,
            );
        }
        result.lerp_speed_mult = float(node, "LerpSpeedMult")?;
        result.rotate_key_speed = float(node, "RotateKeySpeed")?;
        result.rotate_key_speed_fast = float(node, "RotateKeySpeedFast")?;
        result.rotate_continuous_speed = float(node, "RotateContinuousSpeed")?;
        result.rotate_continuous_ease_in = float(node, "RotateContinuousEaseIn")?;
    }
    if let Some(data) = data {
        let root = instance(&data.document, &result.name, "Client_CASTuning")?;
        let node = root
            .field("PresetRotation")?
            .ok_or("source SimData PresetRotation absent")?;
        let angles = node
            .field("ViewAngles")?
            .ok_or("source SimData ViewAngles absent")?;
        let DataValue::Object { schema, .. } = angles else {
            return Err("source SimData ViewAngles must be its declared inline object".into());
        };
        let schemas = data
            .document
            .schemas
            .iter()
            .filter(|s| &s.name == schema)
            .collect::<Vec<_>>();
        if schemas.len() != 1 {
            return Err("source SimData angle schema absent or ambiguous".into());
        }
        let mut columns = schemas[0].columns.iter().collect::<Vec<_>>();
        columns.sort_by_key(|c| c.offset);
        if columns.len() != 6
            || columns
                .iter()
                .enumerate()
                .any(|(i, c)| c.offset != i * 4 || c.data_type != 10)
        {
            return Err("unsupported source six-angle layout".into());
        }
        let bits = columns
            .iter()
            .map(|column| {
                angles
                    .field(&column.name)?
                    .ok_or_else(|| "source angle field absent".to_owned())?
                    .float_bits()
            })
            .collect::<Result<Vec<_>>>()?;
        let bits: [u32; 6] = bits
            .try_into()
            .map_err(|_| "source SimData ViewAngles does not contain six values")?;
        if result
            .view_angles
            .as_ref()
            .is_some_and(|v| v.iter().zip(bits).any(|(a, b)| a.bits != b))
        {
            return Err("explicit XML/SimData PresetRotation ViewAngles mismatch".into());
        }
        result.view_angles = Some(std::array::from_fn(|i| TuningFloat {
            bits: bits[i],
            lexical: f32::from_bits(bits[i]).to_string(),
        }));
        bind_float(&mut result.lerp_speed_mult, node, "LerpSpeedMult")?;
        bind_float(&mut result.rotate_key_speed, node, "RotateKeySpeed")?;
        bind_float(
            &mut result.rotate_key_speed_fast,
            node,
            "RotateKeySpeedFast",
        )?;
        bind_float(
            &mut result.rotate_continuous_speed,
            node,
            "RotateContinuousSpeed",
        )?;
        bind_float(
            &mut result.rotate_continuous_ease_in,
            node,
            "RotateContinuousEaseIn",
        )?;
        result.simdata_pin = Some(data.source.pin.clone());
    }
    Ok(result)
}

/// Read each already selected logical Client_CASTuning record. No Human/species
/// fallback is selected here. A missing partner leaves omitted values unknown;
/// duplicate or mismatched source partners refuse rather than invent defaults.
pub fn preset_rotations(scene: &CasTuningScene) -> Result<Vec<PresetRotationTuning>> {
    scene
        .other_records
        .iter()
        .filter(|(_, record)| record.class == "Client_CASTuning")
        .map(|(pin, record)| {
            let partners = scene
                .simdata
                .iter()
                .filter(|d| {
                    d.source.pin.key == ResourceKey::new(0x545ac67a, 0x004b5265, record.instance)
                })
                .collect::<Vec<_>>();
            if partners.len() > 1 {
                return Err("ambiguous Client_CASTuning SimData partner".into());
            }
            preset_rotation(record, pin, partners.first().copied())
        })
        .collect()
}
fn field<'a>(node: &'a ResolvedTuningNode, name: &str) -> Result<Option<&'a ResolvedTuningNode>> {
    let found: Vec<_> = node
        .children
        .iter()
        .filter(|c| c.attrs.get("n").map(String::as_str) == Some(name))
        .collect();
    if found.len() > 1 {
        return Err(format!("ambiguous duplicate CAS tuning field {name}"));
    }
    Ok(found.first().copied())
}
fn raw_text(node: &ResolvedTuningNode) -> Result<String> {
    if !node.children.is_empty() {
        return Err("scalar CAS tuning field has children".into());
    }
    Ok(node.text.trim().into())
}
fn number(node: &ResolvedTuningNode) -> Result<TuningFloat> {
    let lexical = raw_text(node)?;
    let v = lexical
        .parse::<f32>()
        .map_err(|_| format!("invalid CAS numeric text {lexical:?}"))?;
    Ok(TuningFloat {
        bits: v.to_bits(),
        lexical,
    })
}
fn float(node: &ResolvedTuningNode, name: &str) -> Result<Option<TuningFloat>> {
    field(node, name)?.map(number).transpose()
}
fn required_float(node: &ResolvedTuningNode, name: &str) -> Result<TuningFloat> {
    float(node, name)?.ok_or_else(|| format!("required CAS camera field {name} absent"))
}
fn boolean(node: &ResolvedTuningNode, name: &str) -> Result<Option<bool>> {
    field(node, name)?
        .map(|n| match raw_text(n)?.as_str() {
            "True" => Ok(true),
            "False" => Ok(false),
            _ => Err(format!("invalid CAS boolean {name}")),
        })
        .transpose()
}
fn vector(node: &ResolvedTuningNode, name: &str) -> Result<Option<[TuningFloat; 3]>> {
    field(node, name)?
        .map(|v| {
            Ok([
                required_float(v, "x")?,
                required_float(v, "y")?,
                required_float(v, "z")?,
            ])
        })
        .transpose()
}
fn strings(node: &ResolvedTuningNode, name: &str) -> Result<Vec<String>> {
    let Some(v) = field(node, name)? else {
        return Ok(Vec::new());
    };
    if v.children.is_empty() {
        return Ok(vec![raw_text(v)?]);
    }
    v.children.iter().map(raw_text).collect()
}
fn parameters(n: &ResolvedTuningNode) -> Result<CameraParameters> {
    Ok(CameraParameters {
        target: vector(n, "CameraTarget")?.ok_or("required CAS CameraTarget absent")?,
        fov: required_float(n, "FOV")?,
        distance: required_float(n, "Distance")?,
        orbital_rotation: required_float(n, "OrbitalRotation")?,
        pitch: float(n, "Pitch")?,
    })
}
pub(crate) fn camera(record: ResolvedTuningRecord, pin: RoomSourcePin) -> Result<CameraTuning> {
    let n = &record.root;
    let mut cameras = Vec::new();
    if let Some(list) = field(n, "Cameras")? {
        for c in &list.children {
            let region = field(c, "CameraRegion")?
                .map(raw_text)
                .transpose()?
                .ok_or("CAS CameraRegion absent")?;
            let mut pose_overrides = Vec::new();
            if let Some(list) = field(c, "PoseOverrideCameras")? {
                for p in &list.children {
                    pose_overrides.push(CameraPoseOverride {
                        source_pose_id: None,
                        pose: field(p, "Pose")?
                            .map(raw_text)
                            .transpose()?
                            .ok_or("CAS pose camera lacks Pose")?,
                        camera: parameters(p)?,
                    });
                }
            }
            cameras.push(CameraRegion {
                source_region_id: None,
                region,
                camera: parameters(c)?,
                pose_overrides,
            });
        }
    }
    if cameras.is_empty() {
        return Err("CAS camera record has no camera presets".into());
    }
    let interaction = CameraInteraction {
        use_lerp_camera: boolean(n, "UseLerpCamera")?,
        mouse_wheel_zoom_speed_lerp: float(n, "MouseWheelZoomSpeedLerp")?,
        key_zoom_speed_lerp: float(n, "KeyZoomSpeedLerp")?,
        mouse_wheel_zoom_speed_velocity: float(n, "MouseWheelZoomSpeedVelocity")?,
        key_zoom_speed_velocity: float(n, "KeyZoomSpeedVelocity")?,
        velocity_loss_rate: float(n, "VelocityLossRate")?,
        velocity_cap: float(n, "VelocityCap")?,
        ease_in_pt: float(n, "EaseInPt")?,
        ease_out_pt: float(n, "EaseOutPt")?,
        lateral_lerp_duration: float(n, "LateralLerpDuration")?,
        lateral_lerp_ease_in_out: float(n, "LateralLerpEaseInOut")?,
    };
    Ok(CameraTuning {
        instance: record.instance,
        name: n.attrs.get("n").cloned().unwrap_or_default(),
        resource_namespace: record.resource_namespace,
        source_pin: pin,
        ages: strings(n, "Ages")?,
        species: strings(n, "Species")?,
        occult: strings(n, "Occult")?,
        cas_mode: strings(n, "CASMode")?,
        cameras,
        interaction,
        source_record: record.root,
        simdata_pin: None,
    })
}
pub(crate) fn lighting(record: ResolvedTuningRecord, pin: RoomSourcePin) -> Result<LightingTuning> {
    let n = &record.root;
    let key = |name| -> Result<CasKeyLight> {
        let Some(k) = field(n, name)? else {
            return Ok(CasKeyLight::default());
        };
        Ok(CasKeyLight {
            enable: boolean(k, "Enable")?,
            height: float(k, "Height")?,
            angle: float(k, "Angle")?,
            color: vector(k, "Color")?,
            intensity: float(k, "Intensity")?,
            specular_intensity: float(k, "SpecularIntensity")?,
        })
    };
    let rim = |name| -> Result<CasRimLight> {
        let Some(k) = field(n, name)? else {
            return Ok(CasRimLight::default());
        };
        Ok(CasRimLight {
            enable: boolean(k, "Enable")?,
            color: vector(k, "Color")?,
            intensity: float(k, "Intensity")?,
            falloff: float(k, "Falloff")?,
        })
    };
    let ambient = field(n, "AmbientLight")?;
    Ok(LightingTuning {
        instance: record.instance,
        name: n.attrs.get("n").cloned().unwrap_or_default(),
        resource_namespace: record.resource_namespace,
        source_pin: pin,
        light_base_height: float(n, "LightBaseHeight")?,
        key_lights: [key("KeyLight0")?, key("KeyLight1")?],
        rim_lights: [rim("RimLight0")?, rim("RimLight1")?],
        ambient_min: ambient
            .map(|a| float(a, "IntensityMin"))
            .transpose()?
            .flatten(),
        ambient_max: ambient
            .map(|a| float(a, "IntensityMax"))
            .transpose()?
            .flatten(),
        source_record: record.root,
        simdata_pin: None,
    })
}

fn data_float(n: &DataValue, name: &str) -> Result<TuningFloat> {
    let bits = n
        .field(name)?
        .ok_or_else(|| format!("SimData field {name} absent"))?
        .float_bits()?;
    Ok(TuningFloat {
        bits,
        lexical: f32::from_bits(bits).to_string(),
    })
}
fn bind_float(current: &mut Option<TuningFloat>, n: &DataValue, name: &str) -> Result<()> {
    let observed = data_float(n, name)?;
    if current.as_ref().is_some_and(|v| v.bits != observed.bits) {
        return Err(format!("explicit XML/SimData float mismatch {name}"));
    }
    *current = Some(observed);
    Ok(())
}
fn bind_bool(current: &mut Option<bool>, n: &DataValue, name: &str) -> Result<()> {
    let observed = n
        .field(name)?
        .ok_or_else(|| format!("SimData field {name} absent"))?
        .bool_value()?;
    if current.is_some_and(|v| v != observed) {
        return Err(format!("explicit XML/SimData bool mismatch {name}"));
    }
    *current = Some(observed);
    Ok(())
}
fn bind_color(current: &mut Option<[TuningFloat; 3]>, n: &DataValue, name: &str) -> Result<()> {
    let bits = match n.field(name)? {
        Some(DataValue::FloatVectorBits(v)) if v.len() == 3 => v,
        _ => return Err(format!("SimData Float3 {name} absent")),
    };
    if current
        .as_ref()
        .is_some_and(|prior| prior.iter().zip(bits).any(|(a, b)| a.bits != *b))
    {
        return Err(format!("explicit XML/SimData color mismatch {name}"));
    }
    *current = Some(std::array::from_fn(|i| TuningFloat {
        bits: bits[i],
        lexical: f32::from_bits(bits[i]).to_string(),
    }));
    Ok(())
}
fn instance<'a>(document: &'a DataDocument, name: &str, class: &str) -> Result<&'a DataValue> {
    let found: Vec<_> = document
        .instances
        .iter()
        .filter(|(n, v)| n == name && matches!(v,DataValue::Object{schema,..}if schema==class))
        .collect();
    if found.len() != 1 {
        return Err("CAS SimData name/schema partner absent or ambiguous".into());
    }
    Ok(&found[0].1)
}
pub(crate) fn lighting_data(t: &mut LightingTuning, d: &CasDataResource) -> Result<()> {
    let n = instance(&d.document, &t.name, "Client_CASLightingTuning")?;
    bind_float(&mut t.light_base_height, n, "LightBaseHeight")?;
    for i in 0..2 {
        let k = n
            .field(&format!("KeyLight{i}"))?
            .ok_or("SimData key light absent")?;
        let light = &mut t.key_lights[i];
        bind_bool(&mut light.enable, k, "Enable")?;
        bind_float(&mut light.height, k, "Height")?;
        bind_float(&mut light.angle, k, "Angle")?;
        bind_color(&mut light.color, k, "Color")?;
        bind_float(&mut light.intensity, k, "Intensity")?;
        bind_float(&mut light.specular_intensity, k, "SpecularIntensity")?;
        let r = n
            .field(&format!("RimLight{i}"))?
            .ok_or("SimData rim light absent")?;
        let rim = &mut t.rim_lights[i];
        bind_bool(&mut rim.enable, r, "Enable")?;
        bind_color(&mut rim.color, r, "Color")?;
        bind_float(&mut rim.intensity, r, "Intensity")?;
        bind_float(&mut rim.falloff, r, "Falloff")?;
    }
    let a = n
        .field("AmbientLight")?
        .ok_or("SimData ambient light absent")?;
    bind_float(&mut t.ambient_min, a, "IntensityMin")?;
    bind_float(&mut t.ambient_max, a, "IntensityMax")?;
    t.simdata_pin = Some(d.source.pin.clone());
    Ok(())
}
pub(crate) fn camera_data(t: &mut CameraTuning, d: &CasDataResource) -> Result<()> {
    let n = instance(&d.document, &t.name, "Client_CASCameraTuning")?;
    let values = match n.field("Cameras")? {
        Some(DataValue::Vector(v)) => v,
        _ => return Err("SimData camera vector absent".into()),
    };
    if values.len() != t.cameras.len() {
        return Err("XML/SimData camera count mismatch".into());
    }
    for (typed, source) in t.cameras.iter_mut().zip(values) {
        typed.source_region_id = Some(data_signed(source, "CameraRegion")?);
        let p = &mut typed.camera;
        for (name, original) in [
            ("FOV", &p.fov),
            ("Distance", &p.distance),
            ("OrbitalRotation", &p.orbital_rotation),
        ] {
            if data_float(source, name)?.bits != original.bits {
                return Err(format!("XML/SimData camera {name} mismatch"));
            }
        }
        let target = match source.field("CameraTarget")? {
            Some(DataValue::FloatVectorBits(v)) if v.len() == 3 => v,
            _ => return Err("SimData camera target absent".into()),
        };
        if target.iter().zip(&p.target).any(|(a, b)| *a != b.bits) {
            return Err("XML/SimData camera target mismatch".into());
        }
        bind_float(&mut p.pitch, source, "Pitch")?;
        let overrides = match source.field("PoseOverrideCameras")? {
            Some(DataValue::Vector(v)) => v,
            _ => return Err("SimData pose-override vector absent".into()),
        };
        if overrides.len() != typed.pose_overrides.len() {
            return Err("XML/SimData pose-override count mismatch".into());
        }
        for (typed, source) in typed.pose_overrides.iter_mut().zip(overrides) {
            typed.source_pose_id = Some(data_signed(source, "Pose")?);
            let p = &mut typed.camera;
            for (name, original) in [
                ("FOV", &p.fov),
                ("Distance", &p.distance),
                ("OrbitalRotation", &p.orbital_rotation),
            ] {
                if data_float(source, name)?.bits != original.bits {
                    return Err(format!("XML/SimData pose camera {name} mismatch"));
                }
            }
            let target = match source.field("CameraTarget")? {
                Some(DataValue::FloatVectorBits(v)) if v.len() == 3 => v,
                _ => return Err("SimData pose target absent".into()),
            };
            if target.iter().zip(&p.target).any(|(a, b)| *a != b.bits) {
                return Err("XML/SimData pose target mismatch".into());
            }
            bind_float(&mut p.pitch, source, "Pitch")?;
        }
    }
    bind_bool(&mut t.interaction.use_lerp_camera, n, "UseLerpCamera")?;
    bind_float(
        &mut t.interaction.key_zoom_speed_velocity,
        n,
        "KeyZoomSpeedVelocity",
    )?;
    t.simdata_pin = Some(d.source.pin.clone());
    Ok(())
}
fn data_signed(n: &DataValue, name: &str) -> Result<i64> {
    match n.field(name)? {
        Some(DataValue::Signed64(v)) => Ok(*v),
        _ => Err(format!("SimData Int64 {name} absent")),
    }
}

/// Select only a unique exact explicit context. An absent source context is
/// retained as absent; unsupported species/occult selection never borrows Human.
pub fn select_camera<'a>(
    candidates: &'a [CameraTuning],
    age: &str,
    species: &str,
    occult: Option<&str>,
    cas_mode: Option<&str>,
) -> Result<&'a CameraTuning> {
    let matches: Vec<_> = candidates
        .iter()
        .filter(|c| {
            c.ages.iter().any(|s| s == age)
                && c.species.iter().any(|s| s == species)
                && (c.occult.is_empty() || occult.is_some_and(|s| c.occult.iter().any(|v| v == s)))
                && (c.cas_mode.is_empty()
                    || cas_mode.is_some_and(|s| c.cas_mode.iter().any(|v| v == s)))
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "CAS camera context matches {} source records",
            matches.len()
        ));
    }
    Ok(matches[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn n(
        tag: &str,
        name: &str,
        text: &str,
        children: Vec<ResolvedTuningNode>,
    ) -> ResolvedTuningNode {
        ResolvedTuningNode {
            tag: tag.into(),
            attrs: if name.is_empty() {
                BTreeMap::new()
            } else {
                BTreeMap::from([("n".into(), name.into())])
            },
            text: text.into(),
            children,
        }
    }
    #[test]
    fn missing_defaults_not_invented_duplicate_fields_fail_and_nan_retained() {
        let source = n("U", "", "", vec![n("T", "Intensity", "NaN", vec![])]);
        let v = float(&source, "Intensity").unwrap().unwrap();
        assert!(v.value().is_nan());
        assert!(v.finite().is_err());
        assert_eq!(boolean(&source, "Enable").unwrap(), None);
        let mut duplicate = source;
        duplicate.children.push(n("T", "Intensity", "1", vec![]));
        assert!(float(&duplicate, "Intensity").is_err());
    }
    #[test]
    fn camera_origin_is_explicit_and_nonfinite_rejected() {
        let f = |v: f32| TuningFloat {
            bits: v.to_bits(),
            lexical: v.to_string(),
        };
        let camera = CameraParameters {
            target: [f(128.), f(158.5), f(128.)],
            fov: f(44.),
            distance: f(3.95),
            orbital_rotation: f(177.),
            pitch: None,
        };
        assert_eq!(
            camera.target_relative_to([128., 157.5, 128.]).unwrap(),
            [0., 1., 0.]
        );
        assert!(camera.target_relative_to([f32::NAN, 0., 0.]).is_err());
    }
    #[test]
    fn preset_rotation_keeps_source_angles_and_omitted_values_unknown() {
        let root = n(
            "I",
            "OwnSource",
            "",
            vec![n(
                "U",
                "PresetRotation",
                "",
                vec![
                    n(
                        "L",
                        "ViewAngles",
                        "",
                        ["270", "310", "0", "50", "90", "180"]
                            .into_iter()
                            .map(|v| n("T", "", v, vec![]))
                            .collect(),
                    ),
                    n("T", "RotateContinuousEaseIn", "0.1", vec![]),
                ],
            )],
        );
        let record = ResolvedTuningRecord {
            instance: 7,
            class: "Client_CASTuning".into(),
            resource_namespace: "castuning".into(),
            root,
        };
        let pin = RoomSourcePin {
            provider_path: "explicit-own-fixture.package".into(),
            key: ResourceKey::new(0, 0, 7),
            ordinal: 0,
            index_sha256: "00".repeat(32),
            payload_sha256: "11".repeat(32),
        };
        let value = preset_rotation(&record, &pin, None).unwrap();
        assert_eq!(
            value.view_angles.unwrap().map(|v| v.value()),
            [270., 310., 0., 50., 90., 180.]
        );
        assert_eq!(
            value.rotate_continuous_ease_in.unwrap().bits,
            0.1f32.to_bits()
        );
        assert!(value.rotate_key_speed.is_none());
        assert!(value.simdata_pin.is_none());
        let mut invalid = record;
        invalid.root.children[0].children[0].children.pop();
        assert!(preset_rotation(&invalid, &pin, None).is_err());
    }
    #[test]
    fn preset_rotation_defaults_follow_schema_offsets_and_reject_xml_disagreement() {
        let record = ResolvedTuningRecord {
            instance: 7,
            class: "Client_CASTuning".into(),
            resource_namespace: "castuning".into(),
            root: n("I", "OwnSource", "", vec![]),
        };
        let pin = RoomSourcePin {
            provider_path: "explicit-own-fixture.package".into(),
            key: ResourceKey::new(0, 0, 7),
            ordinal: 0,
            index_sha256: "00".repeat(32),
            payload_sha256: "11".repeat(32),
        };
        let angles = DataValue::Object {
            schema: "OwnAngles".into(),
            schema_hash: 1,
            fields: (0..6)
                .rev()
                .map(|i| {
                    (
                        format!("A{i}"),
                        DataValue::FloatBits((10.0 + i as f32).to_bits()),
                    )
                })
                .collect(),
        };
        let mut fields = vec![("ViewAngles".into(), angles)];
        for name in [
            "LerpSpeedMult",
            "RotateKeySpeed",
            "RotateKeySpeedFast",
            "RotateContinuousSpeed",
            "RotateContinuousEaseIn",
        ] {
            fields.push((name.into(), DataValue::FloatBits(0.1f32.to_bits())));
        }
        let preset = DataValue::Object {
            schema: "OwnPreset".into(),
            schema_hash: 2,
            fields,
        };
        let root = DataValue::Object {
            schema: "Client_CASTuning".into(),
            schema_hash: 3,
            fields: vec![("PresetRotation".into(), preset)],
        };
        let data = CasDataResource {
            source: Arc::new(RoomResource {
                pin: pin.clone(),
                bytes: vec![],
            }),
            document: DataDocument {
                version: 0x101,
                issues: vec![],
                schemas: vec![DataSchema {
                    name: "OwnAngles".into(),
                    hash: 1,
                    size: 24,
                    columns: (0..6)
                        .rev()
                        .map(|i| DataColumn {
                            name: format!("A{i}"),
                            data_type: 10,
                            flags: 0,
                            offset: i * 4,
                        })
                        .collect(),
                }],
                instances: vec![("OwnSource".into(), root)],
            },
        };
        let typed = preset_rotation(&record, &pin, Some(&data)).unwrap();
        assert_eq!(
            typed.view_angles.unwrap().map(|v| v.value()),
            [10., 11., 12., 13., 14., 15.]
        );
        assert_eq!(typed.rotate_key_speed.unwrap().bits, 0.1f32.to_bits());
        assert!(typed.simdata_pin.is_some());
        let mut disagree = record;
        disagree.root.children.push(n(
            "U",
            "PresetRotation",
            "",
            vec![n("T", "RotateKeySpeed", "99", vec![])],
        ));
        assert!(preset_rotation(&disagree, &pin, Some(&data)).is_err());
        let mut invalid = data;
        invalid.document.schemas[0].columns.pop();
        assert!(preset_rotation(&disagree, &pin, Some(&invalid)).is_err());
    }
}
