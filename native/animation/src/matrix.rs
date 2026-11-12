// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Result, unsupported};

/// Column-major matrix acting on column vectors, matching the viewport/WGSL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4(pub [f64; 16]);
impl Default for Matrix4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Matrix4 {
    pub const IDENTITY: Self = Self([
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ]);
    pub fn from_cols_f32(cols: [f32; 16]) -> Result<Self> {
        let out = Self(cols.map(f64::from));
        out.validate_affine()?;
        Ok(out)
    }
    pub fn from_rows_f64(rows: [f64; 16]) -> Self {
        Self(std::array::from_fn(|i| rows[(i % 4) * 4 + i / 4]))
    }
    pub fn to_rows_f64(self) -> [f64; 16] {
        std::array::from_fn(|i| self.0[(i % 4) * 4 + i / 4])
    }
    pub fn to_cols_f32(self) -> Result<[f32; 16]> {
        let out = self.0.map(|x| x as f32);
        if out.iter().any(|x| !x.is_finite()) {
            return Err(unsupported("matrix exceeds float32 range"));
        }
        Ok(out)
    }
    pub fn multiply(self, rhs: Self) -> Self {
        Self(std::array::from_fn(|i| {
            (0..4)
                .map(|k| self.0[k * 4 + i % 4] * rhs.0[(i / 4) * 4 + k])
                .sum()
        }))
    }
    pub fn validate_affine(self) -> Result<()> {
        if self.0.iter().any(|x| !x.is_finite())
            || self.0[3].abs() > 1e-8
            || self.0[7].abs() > 1e-8
            || self.0[11].abs() > 1e-8
            || (self.0[15] - 1.).abs() > 1e-8
        {
            return Err(unsupported("non-finite or non-affine matrix"));
        }
        Ok(())
    }
    pub fn inverse_affine(self) -> Result<Self> {
        self.validate_affine()?;
        let [a, b, c, tx, d, e, f, ty, g, h, i, tz, ..] = self.to_rows_f64();
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if !det.is_finite() || det.abs() < 1e-12 {
            return Err(unsupported("singular bind or basis matrix"));
        }
        let r = [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ];
        let out = Self::from_rows_f64([
            r[0],
            r[1],
            r[2],
            -r[0] * tx - r[1] * ty - r[2] * tz,
            r[3],
            r[4],
            r[5],
            -r[3] * tx - r[4] * ty - r[5] * tz,
            r[6],
            r[7],
            r[8],
            -r[6] * tx - r[7] * ty - r[8] * tz,
            0.,
            0.,
            0.,
            1.,
        ]);
        out.validate_affine()?;
        Ok(out)
    }
    pub fn transform_point(self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|r| {
            self.0[r] * p[0] + self.0[4 + r] * p[1] + self.0[8 + r] * p[2] + self.0[12 + r]
        })
    }
    pub fn from_trs(position: [f32; 3], orientation: [f32; 4], scale: [f32; 3]) -> Result<Self> {
        if position
            .iter()
            .chain(orientation.iter())
            .chain(scale.iter())
            .any(|x| !x.is_finite())
        {
            return Err(unsupported("non-finite transform"));
        }
        let mut q = orientation.map(f64::from);
        let norm = q.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm == 0. {
            q = [0., 0., 0., 1.];
        } else if (norm - 1.).abs() > 1e-4 {
            return Err(unsupported("non-unit bind quaternion"));
        } else {
            for x in &mut q {
                *x /= norm;
            }
        }
        let mut out = Self::rotation_unchecked(q);
        for col in 0..3 {
            for row in 0..3 {
                out.0[col * 4 + row] *= scale[col] as f64;
            }
        }
        for row in 0..3 {
            out.0[12 + row] = position[row] as f64;
        }
        out.validate_affine()?;
        Ok(out)
    }
    pub(crate) fn rotation_unchecked(q: [f64; 4]) -> Self {
        let [x, y, z, w] = q;
        Self::from_rows_f64([
            1. - 2. * y * y - 2. * z * z,
            2. * x * y - 2. * z * w,
            2. * x * z + 2. * y * w,
            0.,
            2. * x * y + 2. * z * w,
            1. - 2. * x * x - 2. * z * z,
            2. * y * z - 2. * x * w,
            0.,
            2. * x * z - 2. * y * w,
            2. * y * z + 2. * x * w,
            1. - 2. * x * x - 2. * y * y,
            0.,
            0.,
            0.,
            0.,
            1.,
        ])
    }
    pub(crate) fn replace_translation(&mut self, p: &[f32]) {
        for row in 0..3 {
            self.0[12 + row] = p[row] as f64;
        }
    }
    pub(crate) fn replace_rotation(&mut self, rotation: Self) {
        for col in 0..3 {
            for row in 0..3 {
                self.0[col * 4 + row] = rotation.0[col * 4 + row];
            }
        }
    }
    pub(crate) fn column_scales(self) -> Result<[f64; 3]> {
        let scales = std::array::from_fn(|col| {
            (0..3)
                .map(|row| self.0[col * 4 + row].powi(2))
                .sum::<f64>()
                .sqrt()
        });
        if scales.iter().any(|v| !v.is_finite() || *v < 1e-8) {
            return Err(unsupported("singular scale component"));
        }
        Ok(scales)
    }
    pub(crate) fn replace_scale(&mut self, scale: [f64; 3]) -> Result<()> {
        if scale
            .iter()
            .any(|v| !v.is_finite() || *v < 1e-8 || *v > 1e4)
        {
            return Err(unsupported("zero, negative or excessive animated scale"));
        }
        let current = self.column_scales()?;
        for col in 0..3 {
            for row in 0..3 {
                self.0[col * 4 + row] = self.0[col * 4 + row] / current[col] * scale[col];
            }
        }
        Ok(())
    }
    pub(crate) fn is_rigid(self) -> bool {
        for a in 0..3 {
            for b in 0..3 {
                let dot = (0..3)
                    .map(|k| self.0[a * 4 + k] * self.0[b * 4 + k])
                    .sum::<f64>();
                if (dot - if a == b { 1. } else { 0. }).abs() > 1e-5 {
                    return false;
                }
            }
        }
        true
    }
    pub(crate) fn rotation_part(self) -> Self {
        let mut out = self;
        out.0[12] = 0.;
        out.0[13] = 0.;
        out.0[14] = 0.;
        out
    }
    pub(crate) fn studio_rotation(local_bind: Self, q: [f64; 4]) -> Result<Self> {
        // Recovered Blender 4.3.2 matrix-assignment/decomposition observation:
        // avoid silently substituting naive source-quaternion normalization.
        let bind = local_bind.rotation_part();
        let transpose = Self(std::array::from_fn(|i| bind.0[(i % 4) * 4 + i / 4]));
        let raw = Self::rotation_unchecked(q);
        let rounded = Self(raw.0.map(|x| (x as f32) as f64));
        let mut delta = transpose.multiply(rounded);
        for col in 0..3 {
            let length = (0..3)
                .map(|row| delta.0[col * 4 + row].powi(2))
                .sum::<f64>()
                .sqrt();
            if !length.is_finite() || length < 1e-8 {
                return Err(unsupported("degenerate source quaternion decomposition"));
            }
            for row in 0..3 {
                delta.0[col * 4 + row] = ((delta.0[col * 4 + row] / length) as f32) as f64;
            }
        }
        let [a, b, c, _, d, e, f, _, g, h, i, ..] = delta.to_rows_f64();
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if !det.is_finite() || det <= 0. {
            return Err(unsupported("reflected source quaternion decomposition"));
        }
        let (x, y, z, w);
        if i < 0. {
            if a > e {
                let s = 2. * (1. + a - e - i).sqrt();
                x = s / 4.;
                w = (h - f) / s;
                y = (b + d) / s;
                z = (c + g) / s;
            } else {
                let s = 2. * (1. - a + e - i).sqrt();
                y = s / 4.;
                w = (c - g) / s;
                x = (b + d) / s;
                z = (f + h) / s;
            }
        } else if a < -e {
            let s = 2. * (1. - a - e + i).sqrt();
            z = s / 4.;
            w = (d - b) / s;
            x = (c + g) / s;
            y = (f + h) / s;
        } else {
            let s = 2. * (1. + a + e + i).sqrt();
            w = s / 4.;
            x = (h - f) / s;
            y = (c - g) / s;
            z = (d - b) / s;
        }
        let norm = (x * x + y * y + z * z + w * w).sqrt();
        if !norm.is_finite() || norm < 1e-8 {
            return Err(unsupported("invalid decomposed source quaternion"));
        }
        let normalized = Self::rotation_unchecked([x / norm, y / norm, z / norm, w / norm]);
        Ok(bind.multiply(Self(normalized.0.map(|v| (v as f32) as f64))))
    }
}
