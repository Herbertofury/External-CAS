//! project report game-root package-path...
use external_cas_assets::{Catalog, PackageIndex};
use external_cas_character::{assemble_character_with_progress, CharacterOptions};
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project, ProjectDraft};
use serde_json::json;
use std::{io::Write, path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() < 4 {
        return Err("project report game-root packages...".into());
    }
    let project = reopen_project(&args[0])?;
    let draft = ProjectDraft::from_project(&project)?;
    let sims = draft.appearance()?;
    let sim = sims.first().ok_or("No native Sim")?;
    let catalog = Catalog::scan_roots(&args[3..], &AtomicBool::new(false), |_| {})?;
    let scope = GameInstallScope::discover(&args[2])?;
    let policy = scope.prepare(&catalog)?;
    let start = std::time::Instant::now();
    let mut callbacks = 0;
    let plan = assemble_character_with_progress(
        sim,
        &catalog,
        Some(&policy),
        &CharacterOptions::default(),
        &AtomicBool::new(false),
        |_| {
            callbacks += 1;
        },
    )?;
    let elapsed = start.elapsed().as_millis();
    scope.revalidate()?;
    let unchanged = catalog.packages.iter().all(|p| {
        PackageIndex::open(&p.path)
            .map(|n| n.index_sha256 == p.index_sha256 && n.file_size == p.file_size)
            .unwrap_or(false)
    });
    let parts=plan.parts.iter().map(|p|json!({"slot":p.native_slot.part_index,"native_body_type":p.native_slot.body_type,"native_casp_instance":format!("{:016X}",p.native_slot.cas_part_id),"selected_casp":p.source.as_ref().map(|r|r.key.to_string()),"casp_version":p.casp.as_ref().map(|c|c.version),"rmap_version":p.region_map.as_ref().map(|r|r.version),"rmap_blocks":p.region_map.as_ref().map(|r|r.blocks.len()),"mesh_count":p.meshes.len(),"meshes":p.meshes.iter().map(|m|json!({"key":m.key.to_string(),"payload_sha256":m.source.payload_sha256.map(|b|b.iter().map(|v|format!("{v:02x}")).collect::<String>()),"vertices":m.native_rest.as_ref().map(|g|g.vertices.len()),"native_rig_bound":m.native_rig_binding.is_some(),"material_bytes":m.native_rest.as_ref().map(|g|g.material.len()),"regions":m.regions.iter().map(|r|json!({"region":r.region,"layer":r.layer,"is_replacement":r.is_replacement})).collect::<Vec<_>>(),"issues":m.issues})).collect::<Vec<_>>(),"material_dependencies":p.materials.iter().map(|m|json!({"role":m.role,"declared_key":m.declared_key.to_string(),"selected":m.selected.as_ref().map(|r|r.key.to_string()),"rule":m.selection_rule,"decoder_qualified":m.decoder_qualified})).collect::<Vec<_>>(),"issues":p.issues})).collect::<Vec<_>>();
    let report = json!({"schema":1,"game_version":plan.observed_game_version,"scope_complete":plan.scope_coverage.as_ref().map(|c|c.configured_scope_complete()),"packages":catalog.packages.len(),"resources":catalog.progress.resources,"catalog_issues":catalog.issues.len(),"native_source_sha256":plan.scene.native_sim_sha256,"native_head_slot_present":plan.parts.iter().any(|p|p.native_slot.body_type==3),"parts":parts,"genetic_part_observations":plan.genetic_parts.len(),"genetic_parts_injected":false,"skin_material_dependencies":plan.skin_materials.iter().map(|m|json!({"role":m.role,"selected":m.selected.as_ref().map(|r|r.key.to_string())})).collect::<Vec<_>>(),"inspection_vertices":plan.inspection_vertices,"unique_retained_resource_bytes":plan.resource_bytes,"assembly_ms":elapsed,"progress_callbacks":callbacks,"morph_source_links":plan.morph.as_ref().map(|m|m.link_resources.len()),"gates":plan.gates.iter().map(|g|json!({"code":format!("{:?}",g.code),"source":g.source,"detail":g.detail,"required":g.required_for_complete})).collect::<Vec<_>>(),"complete_character":plan.complete,"source_conflicts":project.check_source_conflicts().len(),"package_index_pins_unchanged":unchanged,"game_contacted":false,"game_or_profile_writes":false});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("parts={} meshes={} parsed_meshes={} vertices={} retained_bytes={} gates={} assembly_ms={} source_conflicts={}",plan.parts.len(),plan.parts.iter().map(|p|p.meshes.len()).sum::<usize>(),plan.parts.iter().flat_map(|p|&p.meshes).filter(|m|m.native_rest.is_some()).count(),plan.inspection_vertices,plan.resource_bytes,plan.gates.len(),elapsed,project.check_source_conflicts().len());
    Ok(())
}
