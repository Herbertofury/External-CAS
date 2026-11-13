// SPDX-License-Identifier: GPL-3.0-or-later
//! CPU references recovered from selected installed DX11 SimSkin candidates.
//! These do not select an engine permutation, construct the native texture
//! stacks, establish SRV color space, or qualify the complete renderer.
//! See docs/engine/SIMSKIN_DX11_20261112.md for source/evidence boundaries.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const SIMSKIN: u32 = 0x548394b9;
pub const PHONG: u32 = 0xb9105a6d;
pub const SIMGLASS: u32 = 0x5eda9cde;
pub const SIMSKIN_CAS: u32 = 0xdf0863b8;
pub const NORMAL_DECODE_SCALE_BITS: u32 = 0x40008102;
pub const NORMAL_DECODE_BIAS_BITS: u32 = 0xbf850a14;

/// Functional mapping recovered from the installed client's CAS override
/// producer (observed build1.129.39.1030). The caller must explicitly enter the
/// CAS material context and qualify that build; this does not infer a context
/// from a shader ID or choose the variant/technique. Unknown families stay None.
pub fn cas_family_alias_for_source(shader: u32) -> Option<u32> {
    match shader {
        SIMSKIN => Some(SIMSKIN_CAS),
        SIMGLASS => Some(0xd13e993d),
        0x9516a357 => Some(0x5a16760a), // SimGhost -> SimGhostCAS
        0x43a70b79 => Some(0x69b67078), // SimGhostGlass -> SimGhostGlassCAS
        0x941695ae => Some(0x83aa528d), // SimWings -> SimWingsCAS
        0x966ed9db => Some(0x2043ba56), // SimWingsGhost -> SimWingsGhostCAS
        _ => None,
    }
}

/// Exact installed normal-UV default selector, RVA01A2C150. The two part codes
/// are fields of the source38-byte mesh record (+1C/+20): actual CASP BodyType
/// and effective SharedUVSpace (raw zero replaced by BodyType). The native CASP
/// reader and separately named native BodyType API independently prove domains.
/// `None` means the source routine leaves the caller's existing value intact.
/// This is NOT an identity fallback. The native builder first checks GEOM
/// `NormalMapTransformOverride` (0x5844E532), then publishes its selected vector
/// as the distinct shader binding `NormalMapUVxform`.
pub fn normal_uv_default_for_source_context(
    species_code: u32,
    primary_part_code: i32,
    secondary_part_code: u32,
) -> Option<[f32; 4]> {
    if species_code == 1 {
        match secondary_part_code {
            1 | 0x73 => Some([0.5, 0.125, 2.0, 8.0]),
            5 | 6 | 7 | 9 | 0x3d => Some([0.0, 0.5, 1.0, 2.0]),
            8 => Some([0.0, 0.75, 1.0, 4.0]),
            2 => Some([0.0, 0.0, 1.0, 2.0]),
            0x4b => Some(if matches!(primary_part_code, 1 | 0x73) {
                [0.0, 0.0, 1.0, 2.0]
            } else {
                [0.0, 0.0, 1.0, 1.0]
            }),
            _ => None,
        }
    } else if (2..=6).contains(&species_code) {
        match primary_part_code {
            1 => Some([0.75, 0.01953125, 4.0, 4.0]),
            5 => Some([0.75, 0.0, 4.0, 1.0]),
            0xc => Some([0.75, 0.0, 4.0, 8.0]),
            _ => None,
        }
    } else {
        None
    }
}

/// Values sampled in the shader's input space. SkinDetailHSL is sampled at
/// `(overlay[1], 0)`, not at the Sim's main UV; the caller must preserve this.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SimSkinSamples {
    pub overlay: [f32; 4],
    pub skin_tone_makeup: [f32; 4],
    pub cloth: [f32; 4],
    pub skin_detail_rgb: [f32; 3],
    /// PS17731's additional CASDyedTexture sample. PS10671 has no dyed input.
    pub dyed: Option<[f32; 4]>,
}
fn finite(values: impl IntoIterator<Item = f32>) -> Result<()> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err(Error::Invalid("Nonfinite shader input"))
    }
}
fn overlay(base: f32, blend: f32) -> f32 {
    if base < 0.5 {
        2.0 * base * blend
    } else {
        1.0 - 2.0 * (1.0 - base) * (1.0 - blend)
    }
}
/// Selected PS10671/PS17731 albedo equations, with no invented gamma transform,
/// premultiplication, saturation, texture-role substitution or hidden default.
pub fn compose_simskin_candidate_albedo(
    samples: SimSkinSamples,
    diffuse: [f32; 3],
    second_overlay_opacity: f32,
) -> Result<[f32; 3]> {
    finite(
        samples
            .overlay
            .into_iter()
            .chain(samples.skin_tone_makeup)
            .chain(samples.cloth)
            .chain(samples.skin_detail_rgb)
            .chain(diffuse)
            .chain([second_overlay_opacity])
            .chain(samples.dyed.into_iter().flatten()),
    )?;
    let detail = samples.overlay[1];
    Ok(std::array::from_fn(|i| {
        let skin = samples.skin_tone_makeup[i];
        let adjusted = (skin * samples.skin_detail_rgb[i] - skin) * 0.5 + skin;
        let first = overlay(adjusted, detail);
        let second = overlay(first, detail);
        let skin = (second - first) * second_overlay_opacity + first;
        let clothed = (samples.cloth[i] - skin) * samples.cloth[3] + skin;
        let albedo = clothed * diffuse[i];
        match samples.dyed {
            Some(dyed) => (dyed[i] - albedo) * dyed[3] + albedo,
            None => albedo,
        }
    }))
}
/// Source-named SimSkinCAS PS3038/PS3049 uses the same base composite with NO
/// diffuse multiplier. Its examined family has no direct CASDyedTexture input;
/// color editing still requires the actual native composite-construction path.
pub fn compose_simskin_cas_candidate_albedo(
    samples: SimSkinSamples,
    second_overlay_opacity: f32,
) -> Result<[f32; 3]> {
    if samples.dyed.is_some() {
        return Err(Error::Unsupported(
            "Direct dyed plane is not bound by qualified CAS candidates",
        ));
    }
    compose_simskin_candidate_albedo(samples, [1.0; 3], second_overlay_opacity)
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(value: [f32; 3]) -> Result<[f32; 3]> {
    let squared = dot(value, value);
    if !squared.is_finite() || squared <= 0.0 {
        return Err(Error::Invalid("Degenerate shader basis"));
    }
    let inverse = squared.sqrt().recip();
    Ok(value.map(|v| v * inverse))
}
/// Selected normal-enabled candidates use ALPHA/BLUE, with a neutral encoded
/// byte of132. Z is reconstructed BEFORE the XY scale. Tangent is already the
/// VS output; do not normalize/orthogonalize it as an undocumented repair.
/// The observed VS2929 emits handedness -1, whereas other variants differ.
pub fn simskin_candidate_normal(
    rgba: [f32; 4],
    normal: [f32; 3],
    tangent: [f32; 3],
    handedness: f32,
    scale: f32,
) -> Result<[f32; 3]> {
    finite(
        rgba.into_iter()
            .chain(normal)
            .chain(tangent)
            .chain([handedness, scale]),
    )?;
    let n = normalize(normal)?;
    let gain = f32::from_bits(NORMAL_DECODE_SCALE_BITS);
    let bias = f32::from_bits(NORMAL_DECODE_BIAS_BITS);
    let x = rgba[3] * gain + bias;
    let y = rgba[2] * gain + bias;
    let z = (1.0 - (x * x + y * y)).max(0.0).sqrt();
    let bitangent = [
        n[1] * tangent[2] - n[2] * tangent[1],
        n[2] * tangent[0] - n[0] * tangent[2],
        n[0] * tangent[1] - n[1] * tangent[0],
    ]
    .map(|v| v * handedness);
    normalize(std::array::from_fn(|i| {
        x * scale * tangent[i] - y * scale * bitangent[i] + z * n[i]
    }))
}

#[derive(Clone, Copy, Debug)]
pub struct SimSkinPointLight {
    pub position: [f32; 3],
    pub color: [f32; 3],
    /// Native PointLightTemplates: inner distance, outer distance,
    /// normal-dependent weight, normal-independent weight.
    pub template: [f32; 4],
}
#[derive(Clone, Copy, Debug)]
pub struct SimSkinLighting {
    pub directions: [[f32; 4]; 4],
    pub colors: [[f32; 4]; 4],
    pub points: [SimSkinPointLight; 4],
}
/// PS10671's four point contributions and source ambient polynomial. No
/// calibrated fill light, inverse-square attenuation or invented ambient term.
pub fn simskin_candidate_lighting(
    normal: [f32; 3],
    world_position: [f32; 3],
    model_vertex_y: f32,
    input: SimSkinLighting,
) -> Result<[f32; 3]> {
    finite(
        normal
            .into_iter()
            .chain(world_position)
            .chain([model_vertex_y])
            .chain(input.directions.into_iter().flatten())
            .chain(input.colors.into_iter().flatten())
            .chain(
                input
                    .points
                    .into_iter()
                    .flat_map(|p| p.position.into_iter().chain(p.color).chain(p.template)),
            ),
    )?;
    let normal = normalize(normal)?;
    let q: [f32; 4] = std::array::from_fn(|i| {
        normal[0] * input.directions[0][i]
            + normal[1] * input.directions[1][i]
            + normal[2] * input.directions[2][i]
    });
    let basis = [
        (q[0] * 0.5 + 0.5).powi(2),
        (q[1] * 0.5 + 0.5).powi(2),
        q[2].clamp(0.0, 1.0),
        q[3].clamp(0.0, 1.0),
    ];
    let height = model_vertex_y * input.colors[3][3] + input.directions[3][3];
    let mut output = std::array::from_fn(|i| {
        basis
            .iter()
            .zip(input.colors[i])
            .map(|(a, b)| a * b)
            .sum::<f32>()
            + input.colors[3][i] * height
    });
    for point in input.points {
        let delta = std::array::from_fn(|i| point.position[i] - world_position[i]);
        let length = dot(delta, delta).sqrt();
        let range = point.template[1] - point.template[0];
        if length <= 0.0 || range == 0.0 {
            return Err(Error::Invalid("Degenerate point light"));
        }
        let direction = delta.map(|v| v / length);
        // The actual instruction only clamps the upper bound of the ratio.
        let attenuation = 1.0 - ((length - point.template[0]) / range).min(1.0);
        let weight = point.template[2] * dot(normal, direction).clamp(0.0, 1.0) + point.template[3];
        for i in 0..3 {
            output[i] += point.color[i] * attenuation * weight;
        }
    }
    finite(output)?;
    Ok(output)
}

/// Selected source-named CAS-family constants. This is distinct from the
/// world-family four-point/polynomial lighting above. Arrays retain the exact
/// engine register order: color0/1 key contributions, color2/3 rim endpoints.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SimSkinCasLighting {
    pub directions: [[f32; 4]; 4],
    pub colors: [[f32; 4]; 4],
    pub rim_falloff: [f32; 2],
    pub ambient_intensity: [f32; 2],
}
/// PS3038/PS3049 lighting in sampled/input float space. Ambient texture GREEN
/// modulates rim light; ALPHA interpolates ambient intensity. It is sampled
/// with the second source UV set, independent from the main and normal UVs.
pub fn simskin_cas_candidate_lighting(
    normal: [f32; 3],
    view_vector: [f32; 3],
    ambient_texel: [f32; 4],
    input: SimSkinCasLighting,
) -> Result<[f32; 3]> {
    finite(
        normal
            .into_iter()
            .chain(view_vector)
            .chain(ambient_texel)
            .chain(input.directions.into_iter().flatten())
            .chain(input.colors.into_iter().flatten())
            .chain(input.rim_falloff)
            .chain(input.ambient_intensity),
    )?;
    let n = normalize(normal)?;
    let v = normalize(view_vector)?;
    let h = (n[0] + 0.5).clamp(0.0, 1.0);
    let power = (input.rim_falloff[0] - input.rim_falloff[1]) * h + input.rim_falloff[1];
    let rim = (1.0 - dot(n, v).clamp(0.0, 1.0)).powf(power);
    let ambient = (input.ambient_intensity[1] - input.ambient_intensity[0]) * ambient_texel[3]
        + input.ambient_intensity[0];
    let key0 = dot(n, input.directions[0][..3].try_into().unwrap()).clamp(0.0, 1.0);
    let key1 = dot(n, input.directions[1][..3].try_into().unwrap()).clamp(0.0, 1.0);
    let output = std::array::from_fn(|i| {
        let rim_color = (input.colors[2][i] - input.colors[3][i]) * h + input.colors[3][i];
        rim_color * rim * ambient_texel[1]
            + ambient
            + input.colors[0][i] * key0
            + input.colors[1][i] * key1
    });
    finite(output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_uv_defaults_keep_unwritten_cases_and_two_codes_distinct() {
        assert_eq!(
            normal_uv_default_for_source_context(1, 0, 1),
            Some([0.5, 0.125, 2., 8.])
        );
        assert_eq!(
            normal_uv_default_for_source_context(1, 1, 0x4b),
            Some([0., 0., 1., 2.])
        );
        assert_eq!(
            normal_uv_default_for_source_context(1, 5, 0x4b),
            Some([0., 0., 1., 1.])
        );
        assert_eq!(
            normal_uv_default_for_source_context(2, 1, 8),
            Some([0.75, 0.01953125, 4., 4.])
        );
        assert_eq!(normal_uv_default_for_source_context(2, 8, 1), None);
        assert_eq!(normal_uv_default_for_source_context(0, 1, 1), None);
        assert_eq!(normal_uv_default_for_source_context(1, 0, 999), None);
    }
    #[test]
    #[ignore = "Requires explicit private isolated-original normal-UV routine receipt"]
    fn source_uv_defaults_match_original_machine_routine() {
        #[derive(Deserialize)]
        struct Case {
            species_code: u32,
            primary_part_code: i32,
            secondary_part_code: u32,
            expected: Option<[f32; 4]>,
        }
        #[derive(Deserialize)]
        struct Receipt {
            client_sha256: String,
            cases: Vec<Case>,
            game_contact: bool,
            native_io: bool,
            intercepted_calls: u32,
        }
        let path = std::env::var_os("EXTERNAL_CAS_NORMAL_UV_RECEIPT")
            .expect("Explicit private original-routine receipt required");
        assert!(std::fs::metadata(&path).unwrap().len() <= 8 * 1024 * 1024);
        let r: Receipt = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            r.client_sha256,
            "95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b"
        );
        assert!(!r.game_contact && !r.native_io);
        assert_eq!(r.intercepted_calls, 0);
        for c in &r.cases {
            let actual = normal_uv_default_for_source_context(
                c.species_code,
                c.primary_part_code,
                c.secondary_part_code,
            );
            assert_eq!(
                actual.map(|v| v.map(f32::to_bits)),
                c.expected.map(|v| v.map(f32::to_bits))
            );
        }
        eprintln!(
            "Original normal-UV selector parity: {} bit-exact cases including untouched outputs",
            r.cases.len()
        );
    }
    #[test]
    fn cas_family_override_is_explicit_and_preserves_unknown_families() {
        for (source, cas) in [
            (SIMSKIN, SIMSKIN_CAS),
            (SIMGLASS, 0xd13e993d),
            (0x9516a357, 0x5a16760a),
            (0x43a70b79, 0x69b67078),
            (0x941695ae, 0x83aa528d),
            (0x966ed9db, 0x2043ba56),
        ] {
            assert_eq!(cas_family_alias_for_source(source), Some(cas));
        }
        assert_eq!(cas_family_alias_for_source(PHONG), None);
        assert_eq!(cas_family_alias_for_source(0x12345678), None);
    }
    #[test]
    fn alpha_blue_and_scale_order_are_explicit() {
        let center = 132.0 / 255.0;
        let normal = simskin_candidate_normal(
            [0.2, 1.0, center, center],
            [0., 0., 1.],
            [1., 0., 0.],
            -1.,
            1.,
        )
        .unwrap();
        assert!(normal[0].abs() < 1e-6 && normal[1].abs() < 1e-6 && (normal[2] - 1.).abs() < 1e-6);
        let a = simskin_candidate_normal([0., 0., 0.65, 0.7], [0., 0., 1.], [1., 0., 0.], -1., 2.)
            .unwrap();
        let b = simskin_candidate_normal([1., 1., 0.65, 0.7], [0., 0., 1.], [1., 0., 0.], -1., 2.)
            .unwrap();
        assert_eq!(a, b);
        assert!(a[0] > 0. && a[1] > 0. && a[2] > 0.);
        assert!(simskin_candidate_normal([0.; 4], [0.; 3], [1., 0., 0.], -1., 1.).is_err());
    }
    #[test]
    fn cas_secondary_ambient_channels_and_no_diffuse_or_dyed_fallback() {
        let input = SimSkinCasLighting {
            directions: [[0.; 4]; 4],
            colors: [[0.; 4]; 4],
            rim_falloff: [2., 2.],
            ambient_intensity: [0.4, 0.6],
        };
        let a = simskin_cas_candidate_lighting([0., 0., 1.], [0., 0., 1.], [0., 0., 0., 0.], input)
            .unwrap();
        let b = simskin_cas_candidate_lighting([0., 0., 1.], [0., 0., 1.], [1., 1., 1., 1.], input)
            .unwrap();
        assert_eq!(a, [0.4; 3]);
        assert_eq!(b, [0.6; 3]);
        let mut samples = SimSkinSamples {
            overlay: [0., 0.5, 0., 0.],
            skin_tone_makeup: [0.3, 0.5, 0.7, 1.],
            cloth: [1., 0., 0., 0.],
            skin_detail_rgb: [1.; 3],
            dyed: None,
        };
        assert_eq!(
            compose_simskin_cas_candidate_albedo(samples, 0.).unwrap(),
            [0.3, 0.5, 0.7]
        );
        samples.dyed = Some([1.; 4]);
        assert!(compose_simskin_cas_candidate_albedo(samples, 0.).is_err());
    }
}
