// SPDX-License-Identifier: GPL-3.0-or-later
//! JSON stdin/stdout CPU-reference boundary for independent original-DXBC tests.
//! No game files, device, process, profile or source assets are accessed.
use external_cas_material::shader::{
    SimSkinCasLighting, SimSkinSamples, compose_simskin_cas_candidate_albedo,
    simskin_candidate_normal, simskin_cas_candidate_lighting,
};
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Normal {
    rgba: [f32; 4],
    normal: [f32; 3],
    tangent: [f32; 3],
    handedness: f32,
    scale: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lighting {
    view: [f32; 3],
    ambient_texel: [f32; 4],
    input: SimSkinCasLighting,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    samples: SimSkinSamples,
    second_overlay_opacity: f32,
    normal: Normal,
    lighting: Lighting,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Diagnostic JSON input exceeds explicit16MiB budget".into());
    }
    let rows: Vec<Row> = serde_json::from_slice(&bytes)?;
    let output=rows.into_iter().map(|row| {
        let evaluate=||->external_cas_material::Result<serde_json::Value> {
            let albedo=compose_simskin_cas_candidate_albedo(row.samples,row.second_overlay_opacity)?;
            let n=row.normal;
            let normal=simskin_candidate_normal(n.rgba,n.normal,n.tangent,n.handedness,n.scale)?;
            let lighting=simskin_cas_candidate_lighting(normal,row.lighting.view,row.lighting.ambient_texel,row.lighting.input)?;
            let rgb: [f32;3]=std::array::from_fn(|i|albedo[i]*lighting[i]);
            Ok(json!({"albedo":albedo,"normal":normal,"lighting":lighting,"rgb":rgb,"alpha":0.0}))
        };
        evaluate().unwrap_or_else(|error|json!({"error":error.to_string()}))
    }).collect::<Vec<_>>();
    serde_json::to_writer(std::io::stdout(), &output)?;
    Ok(())
}
