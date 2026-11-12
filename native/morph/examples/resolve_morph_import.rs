//! Explicit read-only native Sim morphology dependency qualification.
//! project report game-root package-path...
use external_cas_assets::{Catalog, PackageIndex};
use external_cas_morph::{build_morph_plan, MorphData, MorphOptions};
use external_cas_scene::{resolve_scene, GameInstallScope, SceneOptions};
use external_cas_tray::{reopen_project, ProjectDraft};
use serde_json::json;
use std::{io::Write, path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() < 4 {
        return Err("Expected project report game-root package-path...".into());
    }
    let project = reopen_project(&args[0])?;
    let draft = ProjectDraft::from_project(&project)?;
    let sims = draft.appearance()?;
    let sim = sims.first().ok_or("No native Sim")?;
    let catalog = Catalog::scan_roots(&args[3..], &AtomicBool::new(false), |_| {})?;
    let scope = GameInstallScope::discover(&args[2])?;
    let policy = scope.prepare(&catalog)?;
    let first = resolve_scene(sim, &catalog, &SceneOptions::default());
    let mut options = SceneOptions::default();
    options.provider_choices = policy
        .provider_choices_for_keys(first.required_keys())
        .choices;
    let scene = resolve_scene(sim, &catalog, &options);
    let plan = build_morph_plan(
        sim,
        &scene,
        &catalog,
        Some(&policy),
        &MorphOptions::default(),
    )?;
    scope.revalidate()?;
    let index_unchanged = catalog.packages.iter().all(|p| {
        PackageIndex::open(&p.path)
            .map(|n| n.index_sha256 == p.index_sha256 && n.file_size == p.file_size)
            .unwrap_or(false)
    });
    let operations=plan.operations.iter().map(|o|json!({"source":o.source,"weight":o.weight,"region":o.region,"key":o.resource.key.to_string(),"selection":o.resource.selection,"selected":o.resource.selected.map(|p|json!({"package_index":p.package_index,"entry_index":p.entry_index})),"payload_sha256":o.resource.provider_sha256.iter().map(|(p,h)|json!({"package_index":p.package_index,"entry_index":p.entry_index,"sha256":h})).collect::<Vec<_>>(),"error":o.resource.error,"parsed":match &o.data{Some(MorphData::Bgeo(b))=>json!({"kind":"BGEO","version":b.version,"lods":b.lods.len(),"blends":b.blends.len(),"vectors":b.vectors.len(),"opaque_suffix":b.unparsed_suffix.len()}),Some(MorphData::Dmap(d))=>json!({"kind":"DMAP","version":d.version,"shape_or_normal":d.kind,"has_data":d.has_data,"crop":[d.min_col,d.max_col,d.min_row,d.max_row],"age_gender":d.age_gender,"species":d.species}),Some(MorphData::Bond(b))=>json!({"kind":"BOND","version":b.version,"adjustments":b.adjustments.len(),"opaque_suffix":b.unparsed_suffix.len()}),None=>json!(null)}})).collect::<Vec<_>>();
    let result = json!({"schema":1,"game_version":scope.observed_game_version,"configured_scope_complete":policy.coverage.configured_scope_complete(),"packages":catalog.packages.len(),"resources":catalog.progress.resources,"catalog_issues":catalog.issues.len(),"source_conflicts":project.check_source_conflicts().len(),"package_indices_unchanged":index_unchanged,"native_sim_sha256":plan.native_sim_sha256,"species":plan.species,"age":plan.age,"gender":plan.gender,"frame":plan.frame,"physique":plan.physique.iter().map(|v|json!({"index":v.index,"name":v.name,"value":v.value,"applied_by_reference":v.applied_by_reference})).collect::<Vec<_>>(),"link_resources":plan.link_resources.len(),"resolved_links":plan.link_resources.iter().filter(|r|r.raw.is_some()).count(),"operations":operations,"issues":plan.issues.iter().map(|i|json!({"source":i.source,"detail":i.detail})).collect::<Vec<_>>(),"complete_morphology":plan.complete,"game_contacted":false,"game_or_tray_files_written":false});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    file.write_all(&serde_json::to_vec_pretty(&result)?)?;
    file.sync_all()?;
    println!("links={} resolved={} operations={} parsed={} issues={} source_conflicts={} index_unchanged={}",plan.link_resources.len(),plan.link_resources.iter().filter(|r|r.raw.is_some()).count(),plan.operations.len(),plan.operations.iter().filter(|o|o.data.is_some()).count(),plan.issues.len(),project.check_source_conflicts().len(),index_unchanged);
    Ok(())
}
