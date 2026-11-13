// SPDX-License-Identifier: GPL-3.0-or-later
//! Final prepared 4x4 BGRA -> BC3 operation used by the source CAS exporter.
//! Layer selection, alpha-validity caches, mip generation and pass selection
//! are distinct operations. No alternate compressor is silently substituted.
use crate::{Error, Result, check};
use std::sync::atomic::AtomicBool;

fn alpha_block(pixels: &[[u8; 4]; 16]) -> [u8; 8] {
    let special = pixels.iter().any(|p| matches!(p[3], 0 | 255));
    let mut minimum = 255i32;
    let mut maximum = 0i32;
    for p in pixels {
        if !matches!(p[3], 0 | 255) {
            minimum = minimum.min(p[3] as i32);
            maximum = maximum.max(p[3] as i32);
        }
    }
    if minimum > maximum {
        maximum += 192;
        minimum -= 192;
    }
    let range = (maximum - minimum).max(1);
    let factor = ((if special { 40960 } else { 57344 }) + range) / (range * 2);
    let limit = if special { 5 } else { 7 };
    let mut bits = 0u64;
    for (i, p) in pixels.iter().enumerate() {
        let raw = (((p[3] as i32 - minimum) * 32 * factor) >> 16).max(0);
        let raw = ((raw + 1) / 2).min(limit) ^ if special { 0 } else { 7 };
        let index = raw + i32::from(raw > 0) - if raw == limit { limit } else { 0 }
            + if special && matches!(p[3], 0 | 255) {
                6
            } else {
                0
            };
        bits |= (index as u64) << (i * 3);
    }
    let [first, second] = if special {
        [minimum, maximum]
    } else {
        [maximum, minimum]
    };
    let mut out = [0; 8];
    out[0] = first as u8;
    out[1] = second as u8;
    out[2..].copy_from_slice(&bits.to_le_bytes()[..6]);
    out
}

#[cfg(target_arch = "x86_64")]
fn reciprocal(x: f32) -> f32 {
    // The inspected native compressor uses RCPSS without Newton refinement.
    unsafe {
        use std::arch::x86_64::*;
        _mm_cvtss_f32(_mm_rcp_ss(_mm_set_ss(x)))
    }
}
#[cfg(target_arch = "x86_64")]
fn reciprocal_sqrt(x: f32) -> f32 {
    unsafe {
        use std::arch::x86_64::*;
        _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(x)))
    }
}
#[cfg(not(target_arch = "x86_64"))]
fn reciprocal(x: f32) -> f32 {
    1. / x
}
#[cfg(not(target_arch = "x86_64"))]
fn reciprocal_sqrt(x: f32) -> f32 {
    1. / x.sqrt()
}

#[derive(Debug)]
struct ColorState {
    axis: [f32; 4],
    samples: [[f32; 4]; 16],
    endpoints: [[f32; 4]; 2],
}
fn refine_inverse_sqrt(value: f32, seed: f32) -> f32 {
    (1. - ((seed * seed) * value)) * (seed * 0.5) + seed
}
fn constant_luminance_axis(
    samples: &[[f32; 4]; 16],
    sum: [f32; 4],
    variance: [f32; 4],
) -> [f32; 4] {
    let mut products = [0.; 3];
    for p in samples {
        products[0] += p[1] * p[0];
        products[1] += p[2] * p[1];
        products[2] += p[0] * p[2];
    }
    let length = (variance[0] * variance[0] + variance[1] * variance[1])
        + (variance[2] * variance[2] + variance[3] * variance[3]);
    let gain = refine_inverse_sqrt(length, reciprocal_sqrt(length)).max(1.);
    let off = [
        (products[0] * 0.0625 - (sum[1] * sum[0]) * 0.00390625) * gain,
        (products[1] * 0.0625 - (sum[2] * sum[1]) * 0.00390625) * gain,
        (products[2] * 0.0625 - (sum[0] * sum[2]) * 0.00390625) * gain,
    ];
    let diagonal = variance.map(|v| v * gain);
    let columns = [
        [diagonal[0], off[0], off[2]],
        [off[0], diagonal[1], off[1]],
        [off[2], off[1], diagonal[2]],
    ];
    let mut v = if diagonal[0] > diagonal[1] {
        if diagonal[0] > diagonal[2] {
            columns[0]
        } else {
            columns[2]
        }
    } else if diagonal[1] > diagonal[2] {
        columns[1]
    } else {
        columns[2]
    };
    for _ in 0..16 {
        v = std::array::from_fn(|c| {
            (v[1] * columns[1][c] + v[0] * columns[0][c]) + v[2] * columns[2][c]
        });
        if v.iter().all(|v| v.abs() <= f32::EPSILON) {
            return [0.3, 0.59, 0.11, 0.];
        }
        let length = (v[0] * v[0] + v[1] * v[1]) + (v[2] * v[2] + 0.);
        let gain = refine_inverse_sqrt(length, reciprocal_sqrt(length));
        let gain = refine_inverse_sqrt(length, gain);
        v = v.map(|x| x * gain);
    }
    [v[0], v[1], v[2], 0.]
}
fn prepare_color(pixels: &[[u8; 4]; 16]) -> Result<ColorState> {
    let samples = pixels.map(|p| {
        let rgb = [p[2] as f32 / 256., p[1] as f32 / 256., p[0] as f32 / 256.];
        [
            rgb[0],
            rgb[1],
            rgb[2],
            (rgb[2] * 0.11 + rgb[0] * 0.3) + rgb[1] * 0.59,
        ]
    });
    let mut sum = [0.; 4];
    let mut squared = [0.; 4];
    let mut lum_cross = [0.; 4];
    for p in samples {
        for c in 0..4 {
            sum[c] += p[c];
            squared[c] += p[c] * p[c];
            lum_cross[c] += p[3] * p[c];
        }
    }
    let mean = sum.map(|x| x * 0.0625);
    let variance: [f32; 4] = std::array::from_fn(|c| squared[c] * 0.0625 - mean[c] * mean[c]);
    if ((variance[0] + variance[2]) + variance[1]) + variance[3] < 1e-5 {
        return Ok(ColorState {
            axis: [1.; 4],
            samples,
            endpoints: [mean; 2],
        });
    }
    let cross: [f32; 4] = std::array::from_fn(|c| lum_cross[c] * 0.0625 - mean[3] * mean[c]);
    let gain = reciprocal_sqrt((cross[2] * cross[2] + cross[0] * cross[0]) + cross[1] * cross[1]);
    let axis = if variance[3] > 1e-5 {
        [cross[0] * gain, cross[1] * gain, cross[2] * gain, 0.]
    } else {
        constant_luminance_axis(&samples, sum, variance)
    };
    let mut negative_sum = 0.;
    let mut positive_sum = 0.;
    let mut negative = [0.; 4];
    let mut positive = [0.; 4];
    for p in samples {
        let v = (((p[2] - mean[2]) * axis[2]) + ((p[0] - mean[0]) * axis[0]))
            + ((p[1] - mean[1]) * axis[1]);
        let lo = v.min(0.);
        let hi = v.max(0.);
        negative_sum += lo;
        positive_sum += hi;
        for c in 0..4 {
            negative[c] -= lo * p[c];
            positive[c] += hi * p[c];
        }
    }
    let mut low = negative.map(|v| v * reciprocal(positive_sum));
    let mut high = positive.map(|v| v * reciprocal(-negative_sum));
    let delta: [f32; 4] = std::array::from_fn(|c| high[c] - low[c]);
    let length =
        (delta[0] * delta[0] + delta[2] * delta[2]) + (delta[1] * delta[1] + delta[3] * delta[3]);
    if length < f32::from_bits(999400116) {
        let extension = reciprocal_sqrt(length) * f32::from_bits(1023680776);
        for c in 0..4 {
            high[c] += delta[c] * extension;
            low[c] -= delta[c] * extension;
        }
    }
    Ok(ColorState {
        axis,
        samples,
        endpoints: [low, high],
    })
}
fn rgb565(rgb: [f32; 4]) -> u16 {
    let q5 = |v: f32| {
        ((v * f32::from_bits(0x3f78f8f9) + 393216.)
            .to_bits()
            .wrapping_sub(0x48c00000) as i32)
            .clamp(0, 31) as u16
    };
    let q6 = |v: f32| {
        ((v * f32::from_bits(0x3f7cfcfd) + 196608.)
            .to_bits()
            .wrapping_sub(0x48400000) as i32)
            .clamp(0, 63) as u16
    };
    (q5(rgb[0]) << 11) | (q6(rgb[1]) << 5) | q5(rgb[2])
}
fn encode_color(state: &ColorState) -> [u8; 8] {
    let [first, second] = state.endpoints.map(rgb565);
    let mut out = [0; 8];
    if first == second {
        out[..2].copy_from_slice(&first.to_le_bytes());
        out[2..4].copy_from_slice(&first.to_le_bytes());
        return out;
    }
    let projection = |value: u16| {
        let r = ((value >> 11) as f32 * f32::from_bits(0x3d042108)) * state.axis[0];
        let g = (((value >> 5) & 63) as f32 * f32::from_bits(0x3c820821)) * state.axis[1];
        let b = ((value & 31) as f32 * f32::from_bits(0x3d042108)) * state.axis[2];
        (r + g) + b
    };
    let a = projection(first);
    let b = projection(second);
    let low = a.min(b);
    let mut high = a.max(b);
    if (high - low).abs() < 1e-5 {
        high += 1.;
    }
    let span = high - low;
    let scale = (1. / span) * 3.;
    let amplitude = ((0.125 - span) * 16.).clamp(0., 1.);
    let dither = [
        -0.5, 0., -0.375, 0.125, 0.25, -0.25, 0.375, -0.125, -0.3125, 0.1875, -0.4375, 0.0625,
        0.4375, -0.0625, 0.3125, -0.1875,
    ];
    let mut bits = 0u32;
    for (i, p) in state.samples.iter().enumerate() {
        let v = ((p[2] * state.axis[2] + p[0] * state.axis[0]) + p[1] * state.axis[1] - low)
            * scale
            + amplitude * dither[i];
        let q = v.clamp(0., 3.).round_ties_even() as usize;
        let index = [0u32, 2, 3, 1][q] ^ u32::from(second >= first);
        bits |= index << (i * 2);
    }
    out[..2].copy_from_slice(&first.max(second).to_le_bytes());
    out[2..4].copy_from_slice(&first.min(second).to_le_bytes());
    out[4..].copy_from_slice(&bits.to_le_bytes());
    out
}
/// Compresses exactly one prepared native BGRA block. Untouched compressed
/// blocks and their alpha-validity masks belong to the source export wrapper.
pub fn encode_native_cas_bc3_block(
    pixels: &[[u8; 4]; 16],
    cancel: &AtomicBool,
) -> Result<[u8; 16]> {
    check(cancel)?;
    if !cfg!(target_arch = "x86_64") {
        return Err(Error::Unsupported(
            "Native compressor requires qualified x86_64 reciprocal instructions",
        ));
    }
    let color = encode_color(&prepare_color(pixels)?);
    let mut out = [0; 16];
    out[..8].copy_from_slice(&alpha_block(pixels));
    out[8..].copy_from_slice(&color);
    Ok(out)
}
/// Exact source exporter fast path for an unprepared block. Its compressed
/// color bytes must be retained, rather than decoded and recompressed. The
/// source's alpha endpoints0/5 are intentional, even for fully opaque blocks.
pub fn export_native_cas_unprepared_bc3_block(
    color: &[u8; 8],
    alpha_valid: bool,
    cancel: &AtomicBool,
) -> Result<[u8; 16]> {
    check(cancel)?;
    let mut out = [0; 16];
    out[1] = 5;
    if alpha_valid {
        out[2..8].fill(255);
    }
    out[8..].copy_from_slice(color);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires explicit private original-instruction receipt with qualified CPU reciprocal semantics"]
    fn native_prepared_cloth_blocks_match_original_instructions() {
        let path = std::env::var("EXTERNAL_CAS_BC3_VECTORS")
            .expect("explicit private instruction receipt");
        let source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let vectors = source["vectors"].as_array().unwrap();
        assert!(vectors.len()>=599);
        assert_eq!(source["cases"].as_u64().unwrap()as usize,vectors.len());
        let cancel = AtomicBool::new(false);
        let mut alpha = 0;
        let mut color = 0;
        let mut state_difference = 0;
        let mut errors = vec![];
        for (i, vector) in vectors.iter().enumerate() {
            let pixels: Vec<u8> = vector["bgra"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect();
            let pixels: [[u8; 4]; 16] =
                std::array::from_fn(|p| pixels[p * 4..p * 4 + 4].try_into().unwrap());
            let expected: Vec<u8> = vector["bc3"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect();
            let output = encode_native_cas_bc3_block(&pixels, &cancel).unwrap();
            if output[..8] != expected[..8] {
                alpha += 1;
            }
            let state = prepare_color(&pixels).unwrap();
            let actual_state: Vec<u32> = state
                .axis
                .into_iter()
                .chain(state.samples.into_iter().flatten())
                .chain(state.endpoints.into_iter().flatten())
                .map(f32::to_bits)
                .collect();
            let original: Vec<u32> = vector["color_state_u32"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            if actual_state != original {
                state_difference += 1;
            }
            let actual = encode_color(&state);
            if actual != expected[8..] {
                color += 1;
                if errors.len() < 8 {
                    errors.push((
                        i,
                        actual,
                        expected[8..].to_vec(),
                        state.axis,
                        state.endpoints,
                    ));
                }
            }
        }
        assert_eq!(
            (alpha, color, state_difference),
            (0, 0, 0),
            "samples{errors:?}"
        );
    }
}
