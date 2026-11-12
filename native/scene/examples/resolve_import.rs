//! Read-only explicit-scope imported-Sim resource qualification; output is private.
use external_cas_assets::{Catalog, PackageIndex, Rig};
use external_cas_scene::*;
use external_cas_tray::{reopen_project, ProjectDraft};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn dep(d: &Dependency, catalog: &Catalog) -> serde_json::Value {
    json!({"role":d.role,"requested_keys":d.requested_keys.iter().map(ToString::to_string).collect::<Vec<_>>(),"state":format!("{:?}",d.state),"candidates":d.candidates.iter().map(|c|json!({"key":c.key.to_string(),"package_index":c.provider.package_index,"entry_index":c.provider.entry_index,"package":catalog.provider(c.provider).map(|(p,_)|p.path.clone()),"deleted":c.deleted})).collect::<Vec<_>>(),"selected_key":d.selected.as_ref().map(|c|c.key.to_string()),"provider_payload_hashes":d.provider_payload_hashes.iter().map(|(p,h)|json!({"package_index":p.package_index,"entry_index":p.entry_index,"sha256":h})).collect::<Vec<_>>(),"payload_error":d.payload_error})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project = PathBuf::from(
        args.next()
            .ok_or("Expected private project followed by explicit package paths")?,
    );
    let roots = args.map(PathBuf::from).collect::<Vec<_>>();
    if roots.is_empty() {
        return Err("An explicit package scope is required".into());
    }
    let p = reopen_project(&project)?;
    let draft = ProjectDraft::from_project(&p)?;
    let sims = draft.appearance()?;
    let sim = sims.first().ok_or("No native Sim")?;
    let catalog = Catalog::scan_roots(&roots, &AtomicBool::new(false), |_| {})?;
    let mut plan = resolve_scene(sim, &catalog, &SceneOptions::default());
    let mut policy_receipt = json!(null);
    if let Some(root) = std::env::var_os("CAS_SCENE_GAME_ROOT") {
        let scope = GameInstallScope::discover(&PathBuf::from(root))?;
        let policy = scope.prepare(&catalog)?;
        let choices = policy.provider_choices_for_keys(plan.required_keys());
        let mut options = SceneOptions::default();
        options.provider_choices = choices.choices.clone();
        plan = external_cas_scene::resolve_scene_scoped(sim, &catalog, &options, Some(&policy));
        scope.revalidate()?;
        policy_receipt = json!({"observed_game_version":scope.observed_game_version,"config_count":scope.configs.len(),"configured_packages":choices.coverage.configured_packages,"indexed_known_packages":choices.coverage.indexed_known_packages,"unindexed_package_count":choices.coverage.unindexed_paths.len(),"unscoped_catalog_package_count":choices.coverage.unscoped_catalog_paths.len(),"unqualified_directives":choices.coverage.unqualified_config_directives,"configured_scope_complete":choices.coverage.configured_scope_complete(),"active_pack_entitlements_verified":false,"priority_choices":choices.choices.len(),"decisions":choices.decisions.iter().map(|d|json!({"key":d.key.to_string(),"state":format!("{:?}",d.state),"selected":d.selected.map(|p|json!({"package_index":p.package_index,"entry_index":p.entry_index})),"candidates":d.candidates.iter().map(|c|json!({"priority":c.priority,"package":c.package_path,"rules":c.evidence.iter().map(|r|json!({"config":r.config_path,"config_sha256":r.config_sha256,"line":r.line,"pattern":r.pattern,"priority":r.priority})).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"configs_revalidated":true});
    }
    let mut rig_decode = json!(null);
    if let Some(raw) = &plan.rig_payload {
        rig_decode = match Rig::parse(raw) {
            Ok(rig) => {
                json!({"parsed":true,"bones":rig.bones.len(),"payload_sha256":format!("{:x}",Sha256::digest(raw)),"bytes":raw.len()})
            }
            Err(e) => json!({"parsed":false,"error":e.to_string()}),
        };
    }
    let index_pins_unchanged = catalog.packages.iter().all(|p| {
        PackageIndex::open(&p.path)
            .map(|n| n.index_sha256 == p.index_sha256 && n.file_size == p.file_size)
            .unwrap_or(false)
    });
    let tone=plan.tone.as_ref().map(|t|json!({"resource":dep(&t.resource,&catalog),"version":t.parsed.as_ref().map(|p|p.version),"skin_set_count":t.parsed.as_ref().map(|p|p.skin_sets.len()),"selected_skin_set":t.selected_skin_set,"selection_rule":t.skin_set_selection_rule,"value_shift":t.value_shift,"parse_error":t.parse_error,"opaque_suffix_bytes":t.parsed.as_ref().map(|p|p.uninterpreted_suffix.len()),"texture_dependencies":t.texture_dependencies.iter().map(|d|dep(d,&catalog)).collect::<Vec<_>>(),"payload_sha256":t.raw.as_ref().map(|b|format!("{:x}",Sha256::digest(b)))}));
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":1,"configured_scope":roots,"installed_policy":policy_receipt,"indexed_packages":catalog.packages.len(),"indexed_resources":catalog.progress.resources,"catalog_issues":catalog.issues.iter().map(|i|json!({"path":i.path,"error":i.error})).collect::<Vec<_>>(),"index_pins_unchanged":index_pins_unchanged,"full_source_hashes_checked":false,"original_tray_sha256_unchanged":p.check_source_conflicts().is_empty(),"native_sim_sha256":plan.native_sim_sha256,"rig":plan.rig.as_ref().map(|d|dep(d,&catalog)),"rig_decode":rig_decode,"tone":tone,"current_outfit_index":plan.outfit_index,"current_outfit_id":plan.outfit_id,"outfit":plan.outfit.iter().map(|p|dep(&p.resource,&catalog)).collect::<Vec<_>>(),"morph_dependency_count":plan.morphs.len(),"morphs":plan.morphs.iter().map(|m|json!({"amount":m.amount,"dependency":dep(&m.dependency,&catalog)})).collect::<Vec<_>>(),"failures":plan.failures.iter().map(|f|json!({"kind":format!("{:?}",f.kind),"role":f.role,"detail":f.detail})).collect::<Vec<_>>(),"complete_sim":plan.complete_sim,"game_contacted":false,"game_or_tray_files_written":false})
        )?
    );
    Ok(())
}
