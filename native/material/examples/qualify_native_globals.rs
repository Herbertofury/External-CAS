//! PRIVATE_PROJECT EXCLUSIVE_REPORT EXCLUSIVE_AMBIENT_PNG CONTEXT GAME_ROOT PACKAGES...
use external_cas_assets::Catalog;
use external_cas_character::{assemble_character,CharacterOptions};
use external_cas_material::native_globals::{resolve_native_ambient_context,resolve_native_tone_lookup};
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project,ProjectDraft};
use sha2::{Digest,Sha256};
use std::{path::PathBuf,sync::atomic::AtomicBool,io::Write};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args_os().skip(1).collect();if args.len()<6{return Err("project report ambient-png explicit-context game-root packages...".into())}
    let project_path=PathBuf::from(&args[0]);let report_path=PathBuf::from(&args[1]);let png_path=PathBuf::from(&args[2]);if report_path.exists()||png_path.exists(){return Err("Exclusive output paths required".into())}
    let context=args[3].to_str().ok_or("Context encoding")?.parse::<u32>()?;let root=PathBuf::from(&args[4]);let packages=args[5..].iter().map(PathBuf::from).collect::<Vec<_>>();
    let project=reopen_project(&project_path)?;let sims=ProjectDraft::from_project(&project)?.appearance()?;let cancel=AtomicBool::new(false);let catalog=Catalog::scan_roots(&packages,&cancel,|_|{})?;let scope=GameInstallScope::discover(&root)?;let policy=scope.prepare(&catalog)?;let plan=assemble_character(sims.first().ok_or("No native Sim")?,&catalog,Some(&policy),&CharacterOptions::default())?;
    let ambient=resolve_native_ambient_context(context,&catalog,Some(&policy),&cancel)?;let lookup=resolve_native_tone_lookup(&plan,&catalog,Some(&policy),&cancel)?;let pixels=&ambient.image.pixels;
    scope.revalidate()?;for source in &catalog.packages{source.revalidate(&cancel)?;}if !project.check_source_conflicts().is_empty(){return Err("Original source conflict".into())}
    let pin=|s:&external_cas_character::ResourceObservation|serde_json::json!({"key":s.key.to_string(),"selection":s.selection,"payload_sha256":s.payload_sha256.map(|h|h.iter().map(|v|format!("{v:02x}")).collect::<String>()),"candidates":s.candidates.iter().map(|p|serde_json::json!({"path":p.package_path,"index_sha256":p.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),"entry":p.provider.entry_index,"payload_sha256":p.payload_sha256.map(|h|h.iter().map(|v|format!("{v:02x}")).collect::<String>())})).collect::<Vec<_>>()});
    let channels=(0..4).map(|c|{let values=pixels.rgba.chunks_exact(4).map(|p|p[c]).collect::<Vec<_>>();serde_json::json!({"channel":c,"min":values.iter().min(),"max":values.iter().max(),"sha256":format!("{:x}",Sha256::digest(values))})}).collect::<Vec<_>>();
    let report=serde_json::json!({"schema":1,"ambient_explicit_context":context,"ambient_selector_native_sim_writer_qualified":false,"ambient":pin(&ambient.image.source),"ambient_width":pixels.width,"ambient_height":pixels.height,"ambient_rgba_sha256":format!("{:x}",Sha256::digest(&pixels.rgba)),"ambient_channels":channels,"ambient_warnings":ambient.image.warnings,"tone":pin(&lookup.source),"native_lookup_packed_input":format!("{:08X}",lookup.lookup.packed_input),"lookup_width":lookup.lookup.width,"lookup_height":lookup.lookup.height,"lookup_bgra_sha256":format!("{:x}",Sha256::digest(&lookup.lookup.bgra)),"lookup_dxgi_resource_format":lookup.dxgi_resource_format,"issues":lookup.issues,"source_conflicts":0,"originals_written":false,"game_process_contact":false});
    let mut png=vec![];image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png),&pixels.rgba,pixels.width,pixels.height,image::ColorType::Rgba8,image::ImageFormat::Png)?;let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(png_path)?;file.write_all(&png)?;file.sync_all()?;let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(report_path)?;file.write_all(&serde_json::to_vec_pretty(&report)?)?;file.sync_all()?;
    println!("ambient context={} key={} {}x{} nativeTONElookup={:08X} 256x1 originals_unchanged=true",context,ambient.image.source.key,pixels.width,pixels.height,lookup.lookup.packed_input);Ok(())
}
