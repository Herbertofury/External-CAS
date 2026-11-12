// SPDX-License-Identifier: GPL-3.0-or-later
//! Render data preserves its declared interpretation. Native material equations
//! still require comparison; the verified independent-tool projection is named.
use crate::*;
use external_cas_assets::ResourceKey;
use serde::Serialize;
pub const SHADER_FULL_BRIGHT: u32 = 0x14fa335e;
pub const SHADER_PHONG_ALPHA: u32 = 0xfc5fc212;
pub const DIFFUSE_MAP: u32 = 0x6cc0fd85;
pub const ALPHA_MAP: u32 = 0xc3faac4f;
pub const NORMAL_MAP: u32 = 0x6e56548a;
pub const ALPHA_MASK_THRESHOLD: u32 = 0xe77a2b60;
pub const TRANSPARENCY: u32 = 0x05d22fd3;
pub const DIFFUSE_COLOR: u32 = 0x637daa05;
pub const UV_SCALES: u32 = 0x420520e9;
pub const POS_SCALE: u32 = 0x487648e5;
pub const POS_OFFSET: u32 = 0x790ebf2c;
pub const UV_SCALE: u32 = 0x159ba53e;
pub const UV_OFFSET: u32 = 0x57582869;
#[derive(Debug, Clone, Copy, Serialize)]
pub enum GeometryProjection {
    VerifiedReferenceTool,
    SourceMatdParameterCandidate,
}
#[derive(Debug, Clone, Serialize)]
pub struct RenderVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv0: [f32; 2],
    pub source_color: Option<[u8; 4]>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MaterialParameters {
    pub shader: u32,
    pub diffuse_map: Option<ResourceKey>,
    pub alpha_map: Option<ResourceKey>,
    pub normal_map: Option<ResourceKey>,
    pub diffuse: Option<Vec<f32>>,
    pub transparency: Option<Vec<f32>>,
    pub alpha_mask_threshold: Option<Vec<f32>>,
    pub uv_scales: Option<Vec<f32>>,
    pub pos_scale: Option<[f32; 4]>,
    pub pos_offset: Option<[f32; 4]>,
    pub uv_scale: Option<[f32; 4]>,
    pub uv_offset: Option<[f32; 4]>,
}
fn texture(material: &Material, field: u32) -> Result<Option<ResourceKey>> {
    let found: Vec<_> = material
        .fields
        .iter()
        .filter(|f| f.field == field)
        .collect();
    if found.len() > 1 {
        return Err("ambiguous room material texture field".into());
    }
    match found.first() {
        None => Ok(None),
        Some(value) => value
            .texture_key
            .map(Some)
            .ok_or("room material texture uses unsupported encoding".into()),
    }
}
fn float4(material: Option<&Material>, field: u32) -> Result<Option<[f32; 4]>> {
    let Some(material) = material else {
        return Ok(None);
    };
    material
        .float_values(field)?
        .map(|values| {
            values
                .try_into()
                .map_err(|_| "room material transform is not Float4".into())
        })
        .transpose()
}
impl RoomMesh {
    pub fn parameters(&self) -> Result<MaterialParameters> {
        Ok(MaterialParameters {
            shader: self.material.shader,
            diffuse_map: texture(&self.material, DIFFUSE_MAP)?,
            alpha_map: texture(&self.material, ALPHA_MAP)?,
            normal_map: texture(&self.material, NORMAL_MAP)?,
            diffuse: self.material.float_values(DIFFUSE_COLOR)?,
            transparency: self.material.float_values(TRANSPARENCY)?,
            alpha_mask_threshold: self.material.float_values(ALPHA_MASK_THRESHOLD)?,
            uv_scales: self.material.float_values(UV_SCALES)?,
            pos_scale: float4(self.parameter_material.as_ref(), POS_SCALE)?,
            pos_offset: float4(self.parameter_material.as_ref(), POS_OFFSET)?,
            uv_scale: float4(self.parameter_material.as_ref(), UV_SCALE)?,
            uv_offset: float4(self.parameter_material.as_ref(), UV_OFFSET)?,
        })
    }
    pub fn render_vertices(&self, projection: GeometryProjection) -> Result<Vec<RenderVertex>> {
        if matches!(
            self.geometry.skinning_status,
            RoomSkinningStatus::UnqualifiedBindings
        ) {
            return Err("room skin bindings have not been qualified for static projection".into());
        }
        let parameters = self.parameters()?;
        let transforms = if matches!(projection, GeometryProjection::SourceMatdParameterCandidate) {
            Some((
                parameters.pos_scale.ok_or("source PosScale absent")?,
                parameters.pos_offset.ok_or("source PosOffset absent")?,
                parameters.uv_scale.ok_or("source UVScale absent")?,
                parameters.uv_offset.ok_or("source UVOffset absent")?,
                *parameters
                    .uv_scales
                    .as_ref()
                    .and_then(|v| v.first())
                    .ok_or("source primary UVScales absent")?,
            ))
        } else {
            None
        };
        self.geometry
            .source_vertices
            .iter()
            .enumerate()
            .map(|(index, source)| {
                if source.normal_ubyte4n.is_none() {
                    return Err("source room vertex has no supported normal".into());
                }
                let raw_uv = source
                    .uv_short2
                    .iter()
                    .find(|(channel, _)| *channel == 0)
                    .map(|(_, uv)| *uv)
                    .ok_or("source room vertex has no UV0")?;
                let (position, uv0) =
                    if let Some((pos_scale, pos_offset, uv_scale, uv_offset, primary_uv)) =
                        transforms
                    {
                        (
                            std::array::from_fn(|axis| {
                                f32::from(source.position_short4[axis]) * pos_scale[axis]
                                    + pos_offset[axis]
                            }),
                            [
                                f32::from(raw_uv[0]) * primary_uv * uv_scale[0] + uv_offset[0],
                                f32::from(raw_uv[1]) * primary_uv * uv_scale[1] + uv_offset[1],
                            ],
                        )
                    } else {
                        (
                            self.geometry.source_tool_positions[index],
                            self.geometry.source_tool_uv0[index],
                        )
                    };
                Ok(RenderVertex {
                    position,
                    normal: self.geometry.source_tool_normals[index],
                    uv0,
                    source_color: source.color_ubyte4,
                })
            })
            .collect()
    }
}
impl WorldObject {
    /// No assumed Sim anchor. The caller chooses an explicit source-world origin
    /// and gets a column-major Y-up matrix from the retained source transform.
    pub fn matrix_relative_to(&self, origin: [f32; 3]) -> Result<[f32; 16]> {
        let position = self.position().ok_or("room world position absent")?;
        let q = self.rotation().ok_or("room world rotation absent")?;
        let scale = self.scale().ok_or("room world scale absent")?;
        if !position
            .iter()
            .chain(q.iter())
            .chain(origin.iter())
            .all(|v| v.is_finite())
            || !scale.is_finite()
        {
            return Err("nonfinite source room transform".into());
        }
        let norm = q.iter().map(|v| v * v).sum::<f32>();
        if (norm - 1.).abs() > 0.001 {
            return Err("source room quaternion is not unit length".into());
        }
        let [x, y, z, w] = q;
        let xx = x * x;
        let yy = y * y;
        let zz = z * z;
        let xy = x * y;
        let xz = x * z;
        let yz = y * z;
        let wx = w * x;
        let wy = w * y;
        let wz = w * z;
        Ok([
            (1. - 2. * (yy + zz)) * scale,
            2. * (xy + wz) * scale,
            2. * (xz - wy) * scale,
            0.,
            2. * (xy - wz) * scale,
            (1. - 2. * (xx + zz)) * scale,
            2. * (yz + wx) * scale,
            0.,
            2. * (xz + wy) * scale,
            2. * (yz - wx) * scale,
            (1. - 2. * (xx + yy)) * scale,
            0.,
            position[0] - origin[0],
            position[1] - origin[1],
            position[2] - origin[2],
            1.,
        ])
    }
}
