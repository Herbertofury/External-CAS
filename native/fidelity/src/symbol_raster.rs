// SPDX-License-Identifier: GPL-3.0-or-later
//! Cached bitmap-quad source symbol rasterization. Actual source rectangles,
//! affine placements, bitmap matrices and color transforms drive every pixel.
//! This intentionally refuses unsupported display-list features.
use crate::{
    installed::InstalledAtlas,
    timeline::{BitmapLayer, Matrix, SymbolFrame},
    GfxMovie,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum AtlasPresentation {
    LiteralRgba,
    /// Explicit BC5 UI interpretation, separately validated against source art.
    RedLuminanceGreenAlpha,
}
pub struct AtlasBinding<'a> {
    pub image_id: u16,
    pub atlas: &'a InstalledAtlas,
}
#[derive(Debug)]
pub struct RasterizedSymbol {
    pub bounds_twips: [f64; 4],
    pub dimensions: [u32; 2],
    /// Straight-alpha sRGB-like source UI values; no new transfer curve applied.
    pub rgba: Vec<u8>,
}
struct Resolved<'a> {
    layer: &'a BitmapLayer,
    inverse: Matrix,
    bitmap_inverse: Matrix,
    rect: [u16; 4],
    atlas: &'a InstalledAtlas,
}

pub fn rasterize(
    frame: &SymbolFrame,
    movie: &GfxMovie<'_>,
    bindings: &[AtlasBinding<'_>],
    scale: f64,
    bounds: Option<[f64; 4]>,
    presentation: AtlasPresentation,
) -> Result<RasterizedSymbol, String> {
    if !scale.is_finite() || scale <= 0. || scale > 8. {
        return Err("invalid source symbol render scale".into());
    }
    let bounds = bounds.unwrap_or(frame.bounds()?);
    if !bounds.iter().all(|x| x.is_finite()) || bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
        return Err("invalid source symbol bounds".into());
    }
    let width = ((bounds[2] - bounds[0]) * scale / 20.).ceil() as usize;
    let height = ((bounds[3] - bounds[1]) * scale / 20.).ceil() as usize;
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err("source symbol raster exceeds configured 1024px extent".into());
    }
    let mut resolved = Vec::new();
    for layer in &frame.layers {
        let subs: Vec<_> = movie
            .sub_images
            .iter()
            .filter(|image| image.character_id == layer.bitmap_id)
            .collect();
        if subs.len() != 1 {
            return Err(format!(
                "source bitmap {} has {} matching sub-images",
                layer.bitmap_id,
                subs.len()
            ));
        }
        let sub = subs[0];
        let sources: Vec<_> = bindings
            .iter()
            .filter(|binding| binding.image_id == sub.image_id)
            .collect();
        if sources.len() != 1 {
            return Err(format!(
                "source sub-image {} has {} atlas bindings",
                sub.character_id,
                sources.len()
            ));
        }
        let atlas = sources[0].atlas;
        let rect = sub.pixel_rect;
        if rect[2] <= rect[0]
            || rect[3] <= rect[1]
            || u32::from(rect[2]) > atlas.dimensions[0]
            || u32::from(rect[3]) > atlas.dimensions[1]
        {
            return Err("source sub-image rectangle outside selected atlas".into());
        }
        if atlas.rgba.len() != atlas.dimensions[0] as usize * atlas.dimensions[1] as usize * 4 {
            return Err("source atlas pixel extent".into());
        }
        if atlas.pixel_encoding == "bc5-unorm-rg"
            && !matches!(presentation, AtlasPresentation::RedLuminanceGreenAlpha)
        {
            return Err("two-channel UI atlas requires an explicitly selected presentation".into());
        }
        resolved.push(Resolved {
            layer,
            inverse: layer.matrix.inverse()?,
            bitmap_inverse: layer.bitmap_matrix.inverse()?,
            rect,
            atlas,
        });
    }
    let mut rgba = vec![0; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let mut total = [0.; 4];
            let center = [
                bounds[0] + (x as f64 + 0.5) * 20. / scale,
                bounds[1] + (y as f64 + 0.5) * 20. / scale,
            ];
            for sy in 0..4 {
                for sx in 0..4 {
                    let p = [
                        bounds[0] + (x as f64 + (f64::from(sx) + 0.5) / 4.) * 20. / scale,
                        bounds[1] + (y as f64 + (f64::from(sy) + 0.5) / 4.) * 20. / scale,
                    ];
                    let mut dst = [0.; 4];
                    for item in &resolved {
                        let local = item.inverse.transform(p);
                        let rect = item.layer.rect;
                        if local[0] < rect[0]
                            || local[0] >= rect[2]
                            || local[1] < rect[1]
                            || local[1] >= rect[3]
                        {
                            continue;
                        }
                        // The source bitmap is filtered at the destination pixel
                        // center. Supersampling covers geometry only; integrating the
                        // texture again would blur source pixels at native resolution.
                        let uv = item
                            .bitmap_inverse
                            .transform(item.inverse.transform(center));
                        let src = sample(item, uv, presentation)?;
                        let src = item.layer.color.apply(src);
                        for i in 0..3 {
                            dst[i] = src[i] * src[3] + dst[i] * (1. - src[3]);
                        }
                        dst[3] = src[3] + dst[3] * (1. - src[3]);
                    }
                    for i in 0..4 {
                        total[i] += dst[i] / 16.;
                    }
                }
            }
            let at = (y * width + x) * 4;
            for i in 0..3 {
                rgba[at + i] = if total[3] > 0. {
                    (total[i] / total[3] * 255.).round().clamp(0., 255.) as u8
                } else {
                    0
                };
            }
            rgba[at + 3] = (total[3] * 255.).round().clamp(0., 255.) as u8;
        }
    }
    Ok(RasterizedSymbol {
        bounds_twips: bounds,
        dimensions: [width as u32, height as u32],
        rgba,
    })
}
fn sample(
    item: &Resolved<'_>,
    uv: [f64; 2],
    presentation: AtlasPresentation,
) -> Result<[u8; 4], String> {
    let width = f64::from(item.rect[2] - item.rect[0]);
    let height = f64::from(item.rect[3] - item.rect[1]);
    let coordinate = |p: f64, limit: f64| {
        if item.layer.repeating {
            p.rem_euclid(limit)
        } else {
            p.clamp(0., limit - f64::EPSILON)
        }
    };
    let uv = [coordinate(uv[0], width), coordinate(uv[1], height)];
    let pixel = |x: i32, y: i32| {
        let x = if item.layer.repeating {
            x.rem_euclid(width as i32)
        } else {
            x.clamp(0, width as i32 - 1)
        };
        let y = if item.layer.repeating {
            y.rem_euclid(height as i32)
        } else {
            y.clamp(0, height as i32 - 1)
        };
        let at = ((y as usize + usize::from(item.rect[1])) * item.atlas.dimensions[0] as usize
            + x as usize
            + usize::from(item.rect[0]))
            * 4;
        let raw: &[u8] = &item.atlas.rgba[at..at + 4];
        if item.atlas.pixel_encoding == "bc5-unorm-rg"
            && matches!(presentation, AtlasPresentation::RedLuminanceGreenAlpha)
        {
            [raw[0], raw[0], raw[0], raw[1]]
        } else {
            raw.try_into().unwrap()
        }
    };
    if !item.layer.smoothed {
        return Ok(pixel(uv[0].floor() as i32, uv[1].floor() as i32));
    }
    let x = uv[0] - 0.5;
    let y = uv[1] - 0.5;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let x = x.floor() as i32;
    let y = y.floor() as i32;
    let samples = [
        (pixel(x, y), (1. - fx) * (1. - fy)),
        (pixel(x + 1, y), fx * (1. - fy)),
        (pixel(x, y + 1), (1. - fx) * fy),
        (pixel(x + 1, y + 1), fx * fy),
    ];
    // Filter source straight channels before applying the actual source color
    // transform, matching bitmap filtering rather than inventing a color tint.
    Ok(std::array::from_fn(|i| {
        samples
            .iter()
            .map(|(p, w)| f64::from(p[i]) * w)
            .sum::<f64>()
            .round()
            .clamp(0., 255.) as u8
    }))
}
