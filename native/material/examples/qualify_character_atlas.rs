//! PRIVATE_PROJECT PRIVATE_REPORT PRIVATE_PNG GAME_ROOT EXPLICIT_PACKAGES...
use external_cas_assets::{Catalog,PackageIndex};
use external_cas_character::{assemble_character_with_progress,CharacterOptions};
use external_cas_material::build_reference_atlas;
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project,ProjectDraft};
use sha2::{Digest,Sha256};
use std::{path::PathBuf,sync::atomic::AtomicBool,io::Write};
fn main()->Result<(),Box<dyn std::error::Error>> {
    let args:Vec<_>=std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len()<5{return Err("project report png game-root packages...".into())}
    if args[1].exists()||args[2].exists(){return Err("Exclusive new output paths required".into())}
    let project=reopen_project(&args[0])?;let sims=ProjectDraft::from_project(&project)?.appearance()?;
    let native=sims.first().ok_or("No actual imported Sim")?;let cancel=AtomicBool::new(false);
    let catalog=Catalog::scan_roots(&args[4..],&cancel,|_|{})?;let scope=GameInstallScope::discover(&args[3])?;let policy=scope.prepare(&catalog)?;
    let plan=assemble_character_with_progress(native,&catalog,Some(&policy),&CharacterOptions::default(),&cancel,|_|{})?;
    let start=std::time::Instant::now();let mut callbacks=0;
    let atlas=build_reference_atlas(&plan,&catalog,Some(&policy),&cancel,|_|{callbacks+=1})?;
    let elapsed=start.elapsed().as_millis();
    scope.revalidate()?;for package in &catalog.packages{package.revalidate(&cancel)?;}
    let unchanged=catalog.packages.iter().all(|p|PackageIndex::open(&p.path).is_ok_and(|n|n.index_sha256==p.index_sha256&&n.file_size==p.file_size));
    let conflicts=project.check_source_conflicts();if !unchanged||!conflicts.is_empty(){return Err("Original source changed during atlas qualification".into())}
    let report=serde_json::json!({"schema":1,"tone_tuning_instance":plan.scene.tone.as_ref().and_then(|t|t.parsed.as_ref()).and_then(|t|t.tuning_instance).map(|v|format!("{v:016X}")),"packages":catalog.packages.len(),"resources":catalog.progress.resources,"width":atlas.image.width,"height":atlas.image.height,"rgba_sha256":format!("{:x}",Sha256::digest(&atlas.image.rgba)),"alpha_zero_pixels":atlas.image.rgba.chunks_exact(4).filter(|p|p[3]==0).count(),"nontransparent_pixels":atlas.image.rgba.chunks_exact(4).filter(|p|p[3]>0).count(),"elapsed_ms":elapsed,"progress_callbacks":callbacks,"layers":atlas.layers.iter().map(|l|serde_json::json!({"role":l.role,"key":l.key.to_string(),"part_index":l.part_index,"sort_layer":l.sort_layer,"composition_method":l.composition_method,"packed_color_shift":l.packed_color_shift.map(|v|format!("{v:016X}")),"opacity":l.opacity,"width":l.width,"height":l.height,"applied":l.applied,"issues":l.issues})).collect::<Vec<_>>(),"sources":atlas.source_pins.iter().map(|s|serde_json::json!({"key":s.key.to_string(),"selection":s.selection,"payload_sha256":s.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>()),"error":s.error,"candidates":s.candidates.iter().map(|c|serde_json::json!({"path":c.package_path,"entry":c.provider.entry_index,"index_sha256":c.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),"payload_sha256":c.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>())})).collect::<Vec<_>>() })).collect::<Vec<_>>(),"issues":atlas.issues,"native_qualified":atlas.native_qualified,"source_conflicts":conflicts.len(),"package_pins_unchanged":unchanged,"originals_written":false,"game_contacted":false});
    let mut png=Vec::new();image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png),&atlas.image.rgba,atlas.image.width,atlas.image.height,image::ColorType::Rgba8,image::ImageFormat::Png)?;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&args[2])?;file.write_all(&png)?;file.sync_all()?;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&args[1])?;file.write_all(&serde_json::to_vec_pretty(&report)?)?;file.sync_all()?;
    println!("atlas={}x{} layers={} applied={} source_pins={} ms={} originals_unchanged={}",atlas.image.width,atlas.image.height,atlas.layers.len(),atlas.layers.iter().filter(|l|l.applied).count(),atlas.source_pins.len(),elapsed,unchanged&&conflicts.is_empty());Ok(())
}
