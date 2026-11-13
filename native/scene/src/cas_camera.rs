// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit source CAS camera calculations. No profile, model anchor, clip
//! defaults, transition state, render settings or current actor is inferred.

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug)]
pub struct SourceCasCameraProfile {
    pub target: [f32; 3],
    /// The source angle is horizontal at the native 4:3 reference aspect.
    pub fov_degrees: f32,
    pub distance: f32,
    pub orbital_rotation_degrees: f32,
    pub pitch_degrees: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceCasCameraProjection {
    pub aspect_ratio: f32,
    /// Explicit caller inputs. Native clip-distance policy is a separate seam.
    pub near_clip: f32,
    pub far_clip: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceCasCameraFrame {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub backward: [f32; 3],
    /// Original camera world rows: right, up, backward, eye (row-vector ABI).
    pub world_rows: [[f32; 4]; 4],
    /// Semantic rigid inverse of world_rows; native general inverse rounding
    /// remains independently qualified. For row-vector multiplication.
    pub view_rows: [[f32; 4]; 4],
    /// Native perspective profile: right-handed camera, D3D depth 0..1,
    /// unshifted full viewport; near maps to 0 and far maps to 1.
    pub projection_rows: [[f32; 4]; 4],
    pub view_projection_rows: [[f32; 4]; 4],
    /// Full perspective-plane spans at unit negative camera Z.
    pub horizontal_span: f32,
    pub vertical_span: f32,
}

/// Source scalar trig profile used by camera producer00235370. The native
/// spherical helper00230890 also writes X into W; retain that lane explicitly.
pub fn source_cas_camera_offset_degrees(
    pitch_degrees: f32,
    orbital_rotation_degrees: f32,
) -> Result<[f32; 4]> {
    finite([pitch_degrees, orbital_rotation_degrees])?;
    let pitch = pitch_degrees * f32::from_bits(0x3c8e_fa35);
    let orbital = orbital_rotation_degrees * f32::from_bits(0x3c8e_fa35);
    let pitch_cos = source_cosine(pitch)?;
    let pitch_sin = source_cosine(pitch - f32::from_bits(0x3fc9_0fdb))?;
    let orbital_cos = source_cosine(orbital)?;
    let orbital_sin = source_cosine(orbital - f32::from_bits(0x3fc9_0fdb))?;
    let x = orbital_sin * pitch_cos;
    let out = [x, pitch_sin, -(orbital_cos * pitch_cos), x];
    finite(out)?;
    Ok(out)
}

/// Source00235370→01209730→0120A630, including exact source rational tangent
/// profile01442120. The configured FOV must not be fed directly to a vertical
/// perspective constructor.
pub fn source_cas_fov_spans(fov_degrees: f32, aspect_ratio: f32) -> Result<[f32; 2]> {
    finite([fov_degrees, aspect_ratio])?;
    if !(0.0 < fov_degrees && fov_degrees < 180.0 && aspect_ratio > 0.0) {
        return Err(
            "Source CAS perspective requires 0<FOV<180 and positive aspect; raw inputs retained"
                .into(),
        );
    }
    let radians = fov_degrees * f32::from_bits(0x3c8e_fa35);
    let span = source_tangent(radians * 0.5)? * 2.0;
    let horizontal = (aspect_ratio * 0.75) * span;
    let vertical = horizontal / aspect_ratio;
    finite([horizontal, vertical])?;
    if horizontal <= 0.0 || vertical <= 0.0 {
        return Err("Degenerate source CAS perspective spans".into());
    }
    Ok([horizontal, vertical])
}

pub fn source_cas_camera_frame(
    profile: SourceCasCameraProfile,
    projection: SourceCasCameraProjection,
) -> Result<SourceCasCameraFrame> {
    finite(profile.target.into_iter().chain([
        profile.distance,
        projection.near_clip,
        projection.far_clip,
    ]))?;
    if profile.distance <= 0.0
        || projection.near_clip <= 0.0
        || projection.far_clip <= projection.near_clip
    {
        return Err(
            "Source CAS camera requires positive distance and 0<near<far; no inferred clip policy"
                .into(),
        );
    }
    let offset =
        source_cas_camera_offset_degrees(profile.pitch_degrees, profile.orbital_rotation_degrees)?;
    let eye = std::array::from_fn(|i| profile.distance * offset[i] + profile.target[i]);
    // Source01209FD0 subtracts target-eye, then0120A020 negates and normalizes.
    let backward = normalize(std::array::from_fn(|i| -(profile.target[i] - eye[i])))?;
    // Native static constructor0000C9A0 establishes source world-up[0,1,0,0].
    let cross_right = cross([0.0, 1.0, 0.0], backward);
    let right = source_normalize_or_fallback(cross_right, [1.0, 0.0, 0.0])?;
    let up = source_normalize_or_fallback(cross(backward, right), [0.0, 0.0, 1.0])?;
    let world_rows = [
        [right[0], right[1], right[2], 0.0],
        [up[0], up[1], up[2], 0.0],
        [backward[0], backward[1], backward[2], 0.0],
        [eye[0], eye[1], eye[2], 1.0],
    ];
    let view_rows = [
        [right[0], up[0], backward[0], 0.0],
        [right[1], up[1], backward[1], 0.0],
        [right[2], up[2], backward[2], 0.0],
        [-dot(eye, right), -dot(eye, up), -dot(eye, backward), 1.0],
    ];
    let [horizontal_span, vertical_span] =
        source_cas_fov_spans(profile.fov_degrees, projection.aspect_ratio)?;
    // Source01208EF0 builds full-viewport edges independently. Preserve those
    // operations instead of simplifying them algebraically.
    let left = horizontal_span * -0.5;
    let right_edge = horizontal_span + left;
    let bottom = vertical_span * -0.5;
    let top = vertical_span + bottom;
    let inverse_width = 1.0 / (right_edge - left);
    let inverse_height = 1.0 / (top - bottom);
    let inverse_depth = 1.0 / (projection.near_clip - projection.far_clip);
    let projection_rows = [
        [inverse_width + inverse_width, 0.0, 0.0, 0.0],
        [0.0, inverse_height + inverse_height, 0.0, 0.0],
        [
            (right_edge + left) * inverse_width,
            -((bottom + top) / (bottom - top)),
            inverse_depth * projection.far_clip,
            -1.0,
        ],
        [
            0.0,
            0.0,
            (projection.far_clip * projection.near_clip) * inverse_depth,
            0.0,
        ],
    ];
    let view_projection_rows = multiply_rows(view_rows, projection_rows);
    finite(
        eye.into_iter()
            .chain(world_rows.into_iter().flatten())
            .chain(view_rows.into_iter().flatten())
            .chain(projection_rows.into_iter().flatten())
            .chain(view_projection_rows.into_iter().flatten()),
    )?;
    Ok(SourceCasCameraFrame {
        eye,
        target: profile.target,
        right,
        up,
        backward,
        world_rows,
        view_rows,
        projection_rows,
        view_projection_rows,
        horizontal_span,
        vertical_span,
    })
}

fn finite(values: impl IntoIterator<Item = f32>) -> Result<()> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err("Non-finite source CAS camera input/result; raw inputs retained".into())
    }
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn multiply_rows(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            ((a[i][0] * b[0][j] + a[i][1] * b[1][j]) + a[i][2] * b[2][j]) + a[i][3] * b[3][j]
        })
    })
}
fn source_normalize_or_fallback(value: [f32; 3], fallback: [f32; 3]) -> Result<[f32; 3]> {
    finite(value)?;
    // Exact source0120A020 near-zero cross-product test, then source fixed axis.
    if value
        .into_iter()
        .all(|v| (-v).max(v) <= f32::from_bits(0x3400_0000))
    {
        Ok(fallback)
    } else {
        normalize(value)
    }
}
fn normalize(value: [f32; 3]) -> Result<[f32; 3]> {
    finite(value)?;
    let length2 = (value[0] * value[0] + value[1] * value[1]) + (value[2] * value[2] + 0.0);
    if !length2.is_finite() || length2 <= 0.0 {
        return Err("Degenerate source camera direction".into());
    }
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ps, _mm_set1_ps};
        let mut inverse = unsafe { _mm_cvtss_f32(_mm_rsqrt_ps(_mm_set1_ps(length2))) };
        for _ in 0..2 {
            inverse = ((1.0 - ((inverse * inverse) * length2)) * (0.5 * inverse)) + inverse;
        }
        let out = value.map(|v| v * inverse);
        finite(out)?;
        Ok(out)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err("Source camera RSQRT profile is not qualified on this architecture".into())
    }
}
fn source_cosine(radians: f32) -> Result<f32> {
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
fn source_tangent(radians: f32) -> Result<f32> {
    finite([radians])?;
    if radians.abs() > 10000.0 {
        return Err("Unqualified large-argument source tangent branch".into());
    }
    let biased = radians * f32::from_bits(0x3f22_f983) + 12582912.0;
    let multiple = biased - 12582912.0;
    let mut reduced = radians - f32::from_bits(0x3fc9_0000) * multiple;
    reduced -= f32::from_bits(0x39fd_a000) * multiple;
    reduced -= f32::from_bits(0x33a2_2000) * multiple;
    let precise = reduced - f32::from_bits(0x2c34_611a) * multiple;
    let square = reduced * reduced;
    let numerator = (f32::from_bits(0xbdc4_33b8) * square + 1.0) * precise;
    let denominator =
        (f32::from_bits(0x3c1f_3375) * square + f32::from_bits(0xbedb_b7af)) * square + 1.0;
    let odd = biased.to_bits() & 1 != 0;
    let ratio = if odd {
        denominator / numerator
    } else {
        numerator / denominator
    };
    let out = f32::from_bits(ratio.to_bits() ^ ((biased.to_bits() << 31) & 0x8000_0000));
    finite([out])?;
    Ok(out)
}
