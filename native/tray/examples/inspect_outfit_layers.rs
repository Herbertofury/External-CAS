//! PRIVATE_PROJECT EXCLUSIVE_PRIVATE_REPORT. Read-only, no asset/game access.
use external_cas_tray::{reopen_project,ProjectDraft};
use std::{path::PathBuf,io::Write};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args_os().skip(1).map(PathBuf::from).collect();if args.len()!=2{return Err("private-project exclusive-private-report".into())}if args[1].exists(){return Err("Exclusive report required".into())}let project=reopen_project(&args[0])?;let appearance=ProjectDraft::from_project(&project)?.appearance()?;let mut records=vec![];
    for sim in appearance{for form in std::iter::once(&sim.base).chain(sim.occult_forms.iter()){for outfit in &form.outfits{let layers=outfit.native_layer_ids()?;records.push(serde_json::json!({"sim_index":sim.sim_index,"occult_form_index":form.occult_form_index,"outfit_index":outfit.outfit_index,"outfit_id":outfit.outfit_id,"part_count":outfit.parts.len(),"layer_ids":layers.as_ref().map(|l|&l.values),"wrapper_sha256":layers.as_ref().map(|l|&l.native.sha256),"outfit_sha256":outfit.native.sha256}));}}}
    if !project.check_source_conflicts().is_empty(){return Err("Original source conflict".into())}let report=serde_json::json!({"private":true,"records":records,"source_conflicts":0,"originals_written":false,"game_contacted":false});let mut output=std::fs::OpenOptions::new().create_new(true).write(true).open(&args[1])?;output.write_all(&serde_json::to_vec_pretty(&report)?)?;output.sync_all()?;println!("outfit layer projection records={} originals-unchanged=true",records.len());Ok(())
}
