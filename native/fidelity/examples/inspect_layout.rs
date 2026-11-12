// SPDX-License-Identifier: GPL-3.0-or-later
//! Original source manifest/runtime metadata; redirect private pins outside Git.
use external_cas_assets::PackageIndex;
use external_cas_fidelity::{
    installed::load_movie,
    runtime::requirements,
    ui_layout::{UiPlatform, load_manifest},
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: inspect_layout UI.package")?;
    let index = PackageIndex::open(path)?;
    let loaded = load_manifest(&index)?;
    let cas = loaded.manifest.widgets_for_state("cas", UiPlatform::Pc)?;
    let mut movies = Vec::new();
    for w in &cas {
        if !w.name.starts_with("CAS") && w.name != "GameTray" {
            continue;
        }
        let source = load_movie(&index, &w.name)?;
        movies.push(json!({"name":w.name,"pin":source.pin,"requirements":requirements(&source.source_bytes)?}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"private":true,"manifest_pin":loaded.pin,"widgets":loaded.manifest.widgets.len(),"game_states":loaded.manifest.game_states.len(),"cas":cas,"movies":movies})
        )?
    );
    Ok(())
}
