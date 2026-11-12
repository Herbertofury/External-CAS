// SPDX-License-Identifier: GPL-3.0-or-later
//! Local read-only qualification. Output contains private source pins.
use external_cas_room::*;
use serde_json::json;
use std::sync::atomic::AtomicBool;
fn main() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or("usage: inspect_tuning explicit-game-root")?;
    let mut request = RoomRequest::installed_base(root);
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
    let scene = load_cas_tuning(&request, &AtomicBool::new(false), |p| {
        eprintln!("{} {}/{} {}", p.stage, p.completed, p.total, p.detail)
    })?;
    println!("{}",serde_json::to_string_pretty(&json!({"private":true,"cameras":scene.cameras,"lighting":scene.lighting,"other_records":scene.other_records,"simdata":scene.simdata.iter().map(|d|json!({"pin":d.source.pin,"document":d.document})).collect::<Vec<_>>(),"sources":scene.sources.iter().map(|s|&s.pin).collect::<Vec<_>>(),"provider_decisions":scene.provider_decisions,"config_pins":scene.config_pins,"game_version":scene.game_version,"issues":scene.issues})).map_err(|e|e.to_string())?);
    Ok(())
}
