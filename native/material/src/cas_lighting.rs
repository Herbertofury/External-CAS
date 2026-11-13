// SPDX-License-Identifier: GPL-3.0-or-later
//! Source CAS light-frame math, with explicit resolved profile/basis inputs.
//! No profile selector, camera identity, missing defaults or pass selection is
//! inferred. The source caller/basis identity remains a separate qualification.

use serde::{Deserialize, Serialize};
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SourceCasKeyLight {
    pub enable: bool,
    pub height: f32,
    pub angle_degrees: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    pub specular_intensity: f32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SourceCasRimLight {
    pub enable: bool,
    pub color: [f32; 3],
    pub intensity: f32,
    pub falloff: f32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SourceCasLightingProfile {
    pub light_base_height: f32,
    pub key_lights: [SourceCasKeyLight; 2],
    pub rim_lights: [SourceCasRimLight; 2],
    pub ambient_min: f32,
    pub ambient_max: f32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SourceCasLightBasis {
    /// Exact source matrix row0 supplied to the native producer, not a camera
    /// axis inferred from its name or an arbitrary host matrix convention.
    pub row0: [f32; 4],
    /// Exact source matrix row2 supplied to the same producer.
    pub row2: [f32; 4],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceCasLightFrame {
    pub light_directions: [[f32; 4]; 4],
    pub light_colors: [[f32; 4]; 4],
    pub specular_terms: [f32; 4],
    pub rim_falloff: [f32; 4],
    pub ambient_intensity: [f32; 4],
}

/// Source degree/cosine-polynomial helper01386B10 before final normalization.
/// Both basis vectors are already resolved by the caller; the native frame
/// producer supplies twice their normalized XZ projections. Height is a
/// vertical offset, not an elevation angle. The source world vertical is Y.
pub fn source_cas_light_vector(
    base_height: f32,
    height: f32,
    angle_degrees: f32,
    basis_row2: [f32; 4],
    basis_row0: [f32; 4],
) -> Result<[f32; 4]> {
    finite(
        [base_height, height, angle_degrees]
            .into_iter()
            .chain(basis_row2)
            .chain(basis_row0),
    )?;
    let radians = angle_degrees * f32::from_bits(0x3c8e_fa35);
    let cosine = source_cosine(radians)?;
    let sine = source_cosine(radians - f32::from_bits(0x3fc9_0fdb))?;
    let delta = height - base_height;
    let result = std::array::from_fn(|i| {
        let vertical = if i == 1 { delta } else { 0.0 };
        (vertical + basis_row0[i] * sine) + basis_row2[i] * cosine
    });
    finite(result)?;
    Ok(result)
}

/// Source two-key/two-rim output profile01387E10. Complete resolved inputs are
/// mandatory. Disabled lights have zero directions/colors, while source scalar
/// specular/rim/ambient fields remain independent of those enable flags.
pub fn source_cas_light_frame(
    profile: SourceCasLightingProfile,
    basis: SourceCasLightBasis,
) -> Result<SourceCasLightFrame> {
    finite([
        profile.light_base_height,
        profile.ambient_min,
        profile.ambient_max,
    ])?;
    let mut row0 = basis.row0;
    let mut row2 = basis.row2;
    // Actual static constructor0004B800 installs mask[1,0,1,1].
    row0[1] *= 0.0;
    row2[1] *= 0.0;
    row0 = normalize_source_xyz(row0)?;
    row2 = normalize_source_xyz(row2)?;
    for i in 0..4 {
        row0[i] *= 2.0;
        row2[i] *= 2.0;
    }
    let mut out = SourceCasLightFrame {
        light_directions: [[0.0; 4]; 4],
        light_colors: [[0.0; 4]; 4],
        specular_terms: [
            profile.key_lights[0].specular_intensity,
            profile.key_lights[1].specular_intensity,
            0.0,
            0.0,
        ],
        rim_falloff: [
            profile.rim_lights[0].falloff,
            profile.rim_lights[1].falloff,
            0.0,
            0.0,
        ],
        ambient_intensity: [profile.ambient_min, profile.ambient_max, 0.0, 0.0],
    };
    for (i, light) in profile.key_lights.into_iter().enumerate() {
        finite(light.color.into_iter().chain([
            light.height,
            light.angle_degrees,
            light.intensity,
            light.specular_intensity,
        ]))?;
        // Native computes both vectors before testing enable flags.
        let direction = normalize_source_xyz(source_cas_light_vector(
            profile.light_base_height,
            light.height,
            light.angle_degrees,
            row2,
            row0,
        )?)?;
        let color = source_color(light.color, light.intensity)?;
        if light.enable {
            out.light_directions[i] = direction;
            out.light_colors[i] = color;
        }
    }
    for (i, light) in profile.rim_lights.into_iter().enumerate() {
        finite(
            light
                .color
                .into_iter()
                .chain([light.intensity, light.falloff]),
        )?;
        let color = source_color(light.color, light.intensity)?;
        if light.enable {
            out.light_colors[i + 2] = color;
        }
    }
    Ok(out)
}

fn source_color(rgb: [f32; 3], intensity: f32) -> Result<[f32; 4]> {
    // Source SimData reader01386F60 expands RGB to[r,g,b,r].
    let result = [
        rgb[0] * intensity,
        rgb[1] * intensity,
        rgb[2] * intensity,
        rgb[0] * intensity,
    ];
    finite(result)?;
    Ok(result)
}
fn finite(values: impl IntoIterator<Item = f32>) -> Result<()> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err("Unqualified non-finite source CAS light input/result; raw source retained".into())
    }
}
fn normalize_source_xyz(value: [f32; 4]) -> Result<[f32; 4]> {
    finite(value)?;
    let squares = value.map(|x| x * x);
    // Native HADDPS order after XYZ mask[FFFFFFFF,FFFFFFFF,FFFFFFFF,0].
    let length2 = (squares[0] + squares[1]) + (squares[2] + 0.0);
    if !length2.is_finite() || length2 <= 0.0 {
        return Err(
            "Degenerate/unqualified source CAS light basis or direction; no inferred axis".into(),
        );
    }
    #[cfg(target_arch = "x86_64")]
    let mut inverse = unsafe {
        // SSE is guaranteed on x86_64; this preserves the native RSQRT seed.
        use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ps, _mm_set1_ps};
        _mm_cvtss_f32(_mm_rsqrt_ps(_mm_set1_ps(length2)))
    };
    #[cfg(not(target_arch = "x86_64"))]
    return Err("Native CAS light RSQRT profile is not qualified on this architecture".into());
    #[cfg(target_arch = "x86_64")]
    {
        for _ in 0..2 {
            inverse = ((1.0 - ((inverse * inverse) * length2)) * (0.5 * inverse)) + inverse;
        }
        let result = value.map(|x| inverse * x);
        finite(result)?;
        Ok(result)
    }
}
fn source_cosine(radians: f32) -> Result<f32> {
    // Exact original float constants and operation order. No libm cosine or
    // generic degree/radian range-reduction substitute is used.
    let absolute = (-radians).max(radians);
    let phase = absolute * f32::from_bits(0xbe22_f983);
    let reduced = (((phase - 12582912.0) + 12582912.0) - phase) - 0.5;
    let x = (-reduced).max(reduced) - 0.25;
    let x2 = x * x;
    let mut power = x * x2;
    let mut sum = x * f32::from_bits(0x40c9_0fdb);
    for bits in [
        0xc225_5de7,
        0x42a3_35e3,
        0xc299_6966,
        0x4228_3c1a,
        0xc171_83a8,
        0x4074_7a1a,
        0xbf37_d6dd,
        0x3dd5_7619,
        0xbc45_2021,
    ] {
        sum += power * f32::from_bits(bits);
        power *= x2;
    }
    finite([radians, phase, reduced, x, sum])?;
    Ok((-1.0f32).max(1.0f32.min(sum)))
}
