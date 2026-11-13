// SPDX-License-Identifier: GPL-3.0-or-later
//! Qualified initial GEOM attribute packing from the installed native producer.
//!
//! These are independent attribute primitives, not a mesh/palette builder. They
//! preserve the source's low-bit wrapping and distinct rounding operations.
//! Original GEOM fields must remain available for morphing and preservation.
//! See `evidence/initial-geom-packing.json` and the original vertex contract.

use crate::{Error, Result};

/// Pack initial position XYZ. The native helper writes six bytes; it does not
/// initialize the fourth word of a Short4 destination.
pub fn pack_initial_position(position: [f32; 3]) -> Result<[i16; 3]> {
    let mut out = [0; 3];
    for (axis, x) in position.into_iter().enumerate() {
        finite(x, "position", axis)?;
        // Separate f32 multiply/add, as in MULSS then ADDSS; no fused operation.
        let biased = (x * 8192.0) + 0.5;
        checked_i32(biased, "position", axis)?;
        // Source explicitly floors, then stores only the low word. This is
        // different from later deformation updates' CVTPS2DQ rounding.
        out[axis] = (biased.floor() as i32) as i16;
    }
    Ok(out)
}

/// Pack initial normal XYZ. The fourth destination byte is untouched by the
/// original helper and is deliberately absent from this result.
pub fn pack_initial_normal(normal: [f32; 3]) -> Result<[u8; 3]> {
    let mut out = [0; 3];
    for (axis, x) in normal.into_iter().enumerate() {
        finite(x, "normal", axis)?;
        out[axis] = checked_i32((x * 127.0) + 128.0, "normal", axis)? as u8;
    }
    Ok(out)
}

/// Pack initial tangent XYZ and the original producer's fourth byte, 0x80.
pub fn pack_initial_tangent(tangent: [f32; 3]) -> Result<[u8; 4]> {
    let mut out = [0, 0, 0, 0x80];
    for (axis, x) in tangent.into_iter().enumerate() {
        finite(x, "tangent", axis)?;
        out[axis] = checked_i32((x + 1.0) * 127.5, "tangent", axis)? as u8;
    }
    Ok(out)
}

/// Pack an original UV occurrence. The producer takes the absolute value of U
/// for occurrence 1 only, before wrapping; it leaves V's sign unchanged.
///
/// Uses the original CVTSS2SI default rounding contract (MXCSR 0x1F80, nearest
/// ties to even). Out-of-range values wrap using truncation, so a negative
/// integer maps to 1 while a positive integer greater than 1 maps to 0.
pub fn pack_initial_uv(mut uv: [f32; 2], uv_set_index: u32) -> Result<[i16; 2]> {
    if uv_set_index == 1 {
        uv[0] = uv[0].abs();
    }
    let mut out = [0; 2];
    for (axis, mut x) in uv.into_iter().enumerate() {
        finite(x, "UV", axis)?;
        if x > 1.0 {
            x -= checked_i32(x, "UV", axis)? as f32;
        } else if x < 0.0 {
            x = (x - checked_i32(x, "UV", axis)? as f32) + 1.0;
        }
        let rounded = (x * 65535.0).round_ties_even();
        let encoded = checked_i32(rounded, "UV", axis)?;
        out[axis] = encoded.wrapping_sub(32768) as i16;
    }
    Ok(out)
}

/// Pack a resolved native post-deformation position, as at the final stores of
/// DMAP workers 01A39090/01A3B3F0. Uses nearest ties to even, saturates X/Y to
/// signed16, and stores only the low word of Z. Fourth word remains untouched.
///
/// This does not decide whether deformation ran or produce the deformed value.
pub fn pack_post_deformation_position(position: [f32; 3]) -> Result<[i16; 3]> {
    let mut out = [0; 3];
    for (axis, x) in position.into_iter().enumerate() {
        finite(x, "post-deformation position", axis)?;
        let encoded = checked_i32(
            (x * 8192.0).round_ties_even(),
            "post-deformation position",
            axis,
        )?;
        out[axis] = if axis < 2 {
            encoded.clamp(i16::MIN as i32, i16::MAX as i32) as i16
        } else {
            encoded as i16
        };
    }
    Ok(out)
}

/// Pack the normal vector immediately before the native post-deformation store.
/// The caller must already have resolved the native optional normalization;
/// this operation performs none. XYZ saturate to unsigned8, W is untouched.
pub fn pack_post_deformation_normal(normal: [f32; 3]) -> Result<[u8; 3]> {
    pack_saturated_direction(normal, "post-deformation normal")
}

/// Pack a tangent AFTER the source seam-group averaging step (01A0D2A0).
/// This uses 127*t+128 with saturation, unlike the initial tangent producer's
/// 127.5*(t+1). The caller supplies the actual averaged vector; W is untouched.
/// It does not establish that a DMAP worker updated tangent data.
pub fn pack_seam_average_tangent(tangent: [f32; 3]) -> Result<[u8; 3]> {
    pack_saturated_direction(tangent, "seam-average tangent")
}

fn pack_saturated_direction(value: [f32; 3], attribute: &str) -> Result<[u8; 3]> {
    let mut out = [0; 3];
    for (axis, x) in value.into_iter().enumerate() {
        finite(x, attribute, axis)?;
        // CVTTPS2DQ -> PACKSSDW -> PACKUSWB is equivalent to this clamp in
        // the explicitly qualified native signed-i32 conversion domain.
        out[axis] = checked_i32((x * 127.0) + 128.0, attribute, axis)?.clamp(0, 255) as u8;
    }
    Ok(out)
}

fn finite(value: f32, attribute: &str, axis: usize) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "initial {attribute} axis {axis} is non-finite; source retained"
        )))
    }
}

/// CVTTSS2SI's defined signed-i32 domain. Rust's saturating float cast would
/// otherwise hide an unsupported native overflow/indefinite result.
fn checked_i32(value: f32, attribute: &str, axis: usize) -> Result<i32> {
    if value.is_finite() && (-2147483648.0..2147483648.0).contains(&value) {
        Ok(value as i32)
    } else {
        Err(Error::unsupported(format!(
            "initial {attribute} axis {axis} exceeds qualified native i32 conversion domain; source retained"
        )))
    }
}
