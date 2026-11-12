// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only configured package inventory. Output includes private paths.
use external_cas_room::*;
use serde_json::json;
use std::sync::atomic::AtomicBool;
fn main() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or("usage: inspect_mods explicit-mods-root")?;
    let d = discover_mods(&RoomModsRequest::configured(root), &AtomicBool::new(false))?;
    println!("{}",serde_json::to_string_pretty(&json!({"private":true,"metadata_inventory_only":true,"complete_configured_path_inventory":d.complete,"enabled_observed":d.enabled_observed,"packages":d.packages.iter().map(|p|json!({"path":p.path,"priority":p.priority,"rules":p.rules.iter().map(|r|json!({"config":r.config_path,"sha256":r.config_sha256,"line":r.line,"pattern":r.pattern,"priority":r.priority})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"issues":d.issues})).map_err(|e|e.to_string())?);
    Ok(())
}
