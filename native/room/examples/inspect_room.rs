// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only real installed qualification. Redirect private output outside Git.
use external_cas_room::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn f32_hash(values: impl IntoIterator<Item = f32>) -> String {
    hash(
        &values
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}
fn main() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or("usage: inspect_room explicit-installed-game-root")?;
    let mut request = RoomRequest::installed_base(root);
    request.diagnostic_source_bind_space =
        std::env::var_os("EXTERNAL_CAS_ROOM_DIAGNOSTIC_BIND_SPACE").is_some();
    if let Some(mods_root) = std::env::args().nth(2) {
        let mut mods = RoomModsRequest::configured(mods_root);
        let selected = std::env::args()
            .skip(3)
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            mods.candidate_paths = Some(selected);
        }
        request.mods = Some(mods);
    }
    let scene = load(&request, &AtomicBool::new(false), |progress| {
        eprintln!(
            "{} {}/{} {}",
            progress.stage, progress.completed, progress.total, progress.detail
        )
    })?;
    let meshes:Vec<_>=scene.meshes.iter().map(|mesh|{
        let native=mesh.render_vertices(GeometryProjection::SourceMatdParameterCandidate);
        json!({"object_index":mesh.object_index,"model_pin":mesh.model_pin,"record":mesh.record,"format":mesh.geometry.format,"vertex_bytes_sha256":hash(&mesh.geometry.source_vertex_bytes),"indices_sha256":hash(&mesh.geometry.indices.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>()),"reference_positions_sha256":f32_hash(mesh.geometry.source_tool_positions.iter().flatten().copied()),"reference_normals_sha256":f32_hash(mesh.geometry.source_tool_normals.iter().flatten().copied()),"reference_uv0_sha256":f32_hash(mesh.geometry.source_tool_uv0.iter().flatten().copied()),"material":mesh.material,"parameter_material":mesh.parameter_material,"parameters":mesh.parameters(),"source_parameter_vertices":native})
    }).collect();
    let textures:Vec<_>=scene.textures.iter().map(|texture|json!({"source":texture.source.pin,"status":texture.status,"decoded":texture.decoded.as_ref().map(|(w,h,pixels)|json!({"dimensions":[w,h],"rgba_sha256":hash(pixels),"alpha_min":pixels.chunks_exact(4).map(|p|p[3]).min(),"alpha_max":pixels.chunks_exact(4).map(|p|p[3]).max()}))})).collect();
    let models:Vec<_>=scene.models.iter().map(|model|json!({"pin":model.source.pin,"container":model.container,"header":model.header,"selected_lod":model.selected_lod,"alternate_sources":model.external_lods.iter().map(|source|&source.pin).collect::<Vec<_>>()})).collect();
    println!("{}",serde_json::to_string_pretty(&json!({"private":true,"world":scene.world.pin,"world_objects":scene.world_objects,"definitions":scene.definitions.iter().map(|(source,definition)|json!({"pin":source.pin,"definition":definition})).collect::<Vec<_>>(),"models":models,"meshes":meshes,"textures":textures,"camera_light_opaque":scene.opaque_camera_light.iter().map(|source|json!({"pin":source.pin,"bytes":source.bytes.len()})).collect::<Vec<_>>(),"provider_decisions":scene.provider_decisions,"config_pins":scene.config_pins,"game_version":scene.game_version,"issues":scene.issues})).map_err(|e|e.to_string())?);
    Ok(())
}
