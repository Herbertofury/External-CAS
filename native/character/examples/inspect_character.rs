//! project report game-root package-path...
use external_cas_assets::{Catalog, PackageIndex};
use external_cas_character::{assemble_character_with_progress, CharacterOptions};
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project, ProjectDraft};
use serde_json::json;
use std::{io::Write, path::PathBuf, sync::atomic::AtomicBool};
fn observation(source: &external_cas_character::ResourceObservation) -> serde_json::Value {
    json!({"key":source.key.to_string(),"selection":source.selection,"payload_bytes":source.bytes.as_ref().map(|b|b.len()),"payload_sha256":source.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>()),"error":source.error,"providers":source.candidates.iter().map(|p|json!({"path":p.package_path,"entry_index":p.provider.entry_index,"selected":source.selected==Some(p.provider),"index_sha256":p.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),"payload_sha256":p.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>()),"deleted":p.deleted,"error":p.error})).collect::<Vec<_>>()})
}
fn material(dependency: &external_cas_character::MaterialDependency) -> serde_json::Value {
    let dimensions=dependency.selected.as_ref().and_then(|r|r.bytes.as_ref().map(|raw|{
        if r.key.type_id==0x2bc04edf { match external_cas_assets::lrle::decode_lrle(raw,external_cas_assets::ReadLimits::default()) { Ok(image)=>json!({"width":image.width,"height":image.height,"mips":image.mips,"decoder":"native-assets LRLE","rgba_bytes":image.rgba.len()}), Err(e)=>json!({"error":e.to_string()}) } }
        else if raw.len()>=12 && &raw[..4]==b"DXT5" {json!({"width":u16::from_le_bytes(raw[8..10].try_into().unwrap()),"height":u16::from_le_bytes(raw[10..12].try_into().unwrap()),"decoder":"header observation only; full atlas decode is qualified by the material example"})}
        else if raw.len()>=20 && &raw[..4]==b"DDS " {json!({"width":u32::from_le_bytes(raw[16..20].try_into().unwrap()),"height":u32::from_le_bytes(raw[12..16].try_into().unwrap()),"decoder":"DDS header observation only"})}
        else {json!({"error":"Non-LRLE image decode belongs to the material crate"})}
    }));
    json!({"role":dependency.role,"declared_key":dependency.declared_key.to_string(),"selected":dependency.selected.as_ref().map(|r|observation(r)),"decoded_dimensions":dimensions,"candidates":dependency.ordered_candidates.iter().map(|r|observation(r)).collect::<Vec<_>>(),"rule":dependency.selection_rule,"decoder_qualified":dependency.decoder_qualified})
}
fn qualify_regions(
    plan: &external_cas_character::CharacterPlan,
    dll: &PathBuf,
) -> Result<usize, Box<dyn std::error::Error>> {
    use std::process::{Command, Stdio};
    let mut compared = 0;
    for part in &plan.parts {
        let Some(map) = &part.region_map else {
            continue;
        };
        let mut child = Command::new("dotnet")
            .arg(dll)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().unwrap().write_all(&map.raw)?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(format!(
                "Region oracle error {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let blocks = expected.as_array().ok_or("RMAP blocks array expected")?;
        assert_eq!(blocks.len(), map.blocks.len());
        for (a, e) in map.blocks.iter().zip(blocks) {
            assert_eq!(a.region, e["region"].as_u64().unwrap() as u32);
            assert_eq!(
                a.layer.to_bits(),
                (e["layer"].as_f64().unwrap() as f32).to_bits()
            );
            assert_eq!(a.is_replacement, e["isReplacement"].as_u64().unwrap() as u8);
            let keys = e["keys"].as_array().unwrap();
            assert_eq!(a.meshes.len(), keys.len());
            for (k, e) in a.meshes.iter().zip(keys) {
                assert_eq!(k.type_id, e["type"].as_u64().unwrap() as u32);
                assert_eq!(k.group_id, e["group"].as_u64().unwrap() as u32);
                assert_eq!(k.instance_id, e["instance"].as_u64().unwrap());
            }
        }
        compared += 1;
    }
    Ok(compared)
}
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
    let reference_deformation = if std::env::var_os("CAS_CHARACTER_REFERENCE_DEFORM").is_some() {
        use external_cas_character::{
            build_reference_deformation, ReferenceDeformOptions, ReferenceOperationState,
        };
        use sha2::{Digest, Sha256};
        let start = std::time::Instant::now();
        let result = build_reference_deformation(
            &plan,
            &ReferenceDeformOptions::default(),
            &AtomicBool::new(false),
            |_| {},
        )?;
        let meshes=result.meshes.iter().map(|m|{
            let native=plan.parts[m.part_index].meshes[m.mesh_index].native_rest.as_ref().unwrap();
            let hash=|normals:bool|format!("{:x}",Sha256::digest(m.geometry.vertices.iter().flat_map(|v|if normals{v.normal}else{v.position}).flat_map(f32::to_le_bytes).collect::<Vec<_>>()));
            json!({"key":m.key.to_string(),"part_index":m.part_index,"mesh_index":m.mesh_index,"vertices":m.geometry.vertices.len(),"changed_positions":m.geometry.vertices.iter().zip(&native.vertices).filter(|(a,b)|a.position!=b.position).count(),"changed_normals":m.geometry.vertices.iter().zip(&native.vertices).filter(|(a,b)|a.normal!=b.normal).count(),"position_sha256":hash(false),"normal_sha256":hash(true),"native_encoded_bytes_retained":m.geometry.original==native.original,"applied":m.operations.iter().filter(|o|matches!(o.state,ReferenceOperationState::Applied)).count(),"ineligible":m.operations.iter().filter(|o|matches!(o.state,ReferenceOperationState::IneligibleBySource)).count(),"unresolved":m.operations.iter().filter_map(|o|match &o.state{ReferenceOperationState::Unresolved(e)=>Some(json!({"operation_index":o.operation_index,"key":o.key.to_string(),"weight":o.weight,"error":e})),_=>None}).collect::<Vec<_>>(),"issues":m.issues})
        }).collect::<Vec<_>>();
        json!({"meshes":meshes,"vertex_operations":result.vertex_operations,"reference_stage_ms":start.elapsed().as_millis(),"rig_bones":result.rig.as_ref().map(|r|r.bones.len()),"issues":result.issues,"native_qualified":result.native_qualified})
    } else {
        serde_json::Value::Null
    };
    let rmap_oracle_comparisons = if let Some(dll) = std::env::var_os("CAS_CHARACTER_RMAP_ORACLE") {
        qualify_regions(&plan, &PathBuf::from(dll))?
    } else {
        0
    };
    scope.revalidate()?;
    let unchanged = catalog.packages.iter().all(|p| {
        PackageIndex::open(&p.path)
            .map(|n| n.index_sha256 == p.index_sha256 && n.file_size == p.file_size)
            .unwrap_or(false)
    });
    let parts=plan.parts.iter().map(|p|json!({"slot":p.native_slot.part_index,"native_body_type":p.native_slot.body_type,"native_casp_instance":format!("{:016X}",p.native_slot.cas_part_id),"selected_casp":p.source.as_ref().map(|r|r.key.to_string()),"casp_version":p.casp.as_ref().map(|c|c.version),"sort_layer":p.casp.as_ref().map(|c|c.sort_layer),"sort_priority":p.casp.as_ref().map(|c|c.sort_priority),"composition_method":p.casp.as_ref().map(|c|c.composition_method),"texture_space":p.casp.as_ref().map(|c|c.texture_space),"native_color_shift":p.native_slot.color_shift.map(|v|format!("{v:016X}")),"rmap_version":p.region_map.as_ref().map(|r|r.version),"rmap_blocks":p.region_map.as_ref().map(|r|r.blocks.len()),"mesh_count":p.meshes.len(),"meshes":p.meshes.iter().map(|m|json!({"key":m.key.to_string(),"payload_sha256":m.source.payload_sha256.map(|b|b.iter().map(|v|format!("{v:02x}")).collect::<String>()),"vertices":m.native_rest.as_ref().map(|g|g.vertices.len()),"shader_hash":m.native_rest.as_ref().map(|g|format!("{:08X}",g.shader)),"vertex_ids":m.native_rest.as_ref().map(|g|g.vertices.iter().filter(|v|v.vertex_id.is_some()).count()),"native_tags":m.native_rest.as_ref().map(|g|g.vertices.iter().filter(|v|v.tag.is_some()).count()),"native_rig_bound":m.native_rig_binding.is_some(),"material_bytes":m.native_rest.as_ref().map(|g|g.material.len()),"regions":m.regions.iter().map(|r|json!({"region":r.region,"layer":r.layer,"is_replacement":r.is_replacement})).collect::<Vec<_>>(),"issues":m.issues})).collect::<Vec<_>>(),"material_dependencies":p.materials.iter().map(material).collect::<Vec<_>>(),"issues":p.issues})).collect::<Vec<_>>();
    let reference_tuning = external_cas_character::select_reference_tuning(&plan);
    let report = json!({"schema":1,"native_age":sim.age,"native_gender":sim.gender,"native_species":sim.extended_species,"base_physique":sim.base.physique,"native_current_occult":sim.current_occult_types,"native_skin_tone_value_shift":sim.base.skin_tone_value_shift,"tone":plan.scene.tone.as_ref().map(|t|json!({"native_key":t.resource.requested_keys.iter().map(ToString::to_string).collect::<Vec<_>>(),"selected_skin_set":t.selected_skin_set,"selection_rule":t.skin_set_selection_rule,"value_shift":t.value_shift,"parsed":t.parsed.as_ref().map(|p|json!({"version":p.version,"saturation":p.saturation,"hue":p.hue,"opacity":p.opacity,"slider":p.slider,"uninterpreted_suffix_bytes":p.uninterpreted_suffix.len(),"sets":p.skin_sets.iter().map(|s|json!({"texture_instance":format!("{:016X}",s.texture_instance),"overlay_instance":format!("{:016X}",s.overlay_instance),"overlay_multiplier":s.overlay_multiplier,"makeup_opacity":s.makeup_opacity,"makeup_opacity_2":s.makeup_opacity_2})).collect::<Vec<_>>()}))})),"game_version":plan.observed_game_version,"scope_complete":plan.scope_coverage.as_ref().map(|c|c.configured_scope_complete()),"packages":catalog.packages.len(),"resources":catalog.progress.resources,"catalog_issues":catalog.issues.len(),"native_source_sha256":plan.scene.native_sim_sha256,"native_head_slot_present":plan.parts.iter().any(|p|p.native_slot.body_type==3),"parts":parts,"genetic_part_observations":plan.genetic_parts.len(),"genetic_parts_injected":false,"skin_material_dependencies":plan.skin_materials.iter().map(material).collect::<Vec<_>>(),"inspection_vertices":plan.inspection_vertices,"resource_read_bytes":plan.resource_bytes,"rmap_oracle_comparisons":rmap_oracle_comparisons,"assembly_ms":elapsed,"progress_callbacks":callbacks,"tuning":plan.tuning.iter().map(|t|json!({"source":observation(&t.source),"binary":t.binary,"record_count":t.inspection.as_ref().map(|i|i.records.len()),"scales":t.inspection.as_ref().map(|i|i.records.iter().map(|r|r.scales.len()).sum::<usize>()),"issues":t.issues})).collect::<Vec<_>>(),"reference_deformation":reference_deformation,"reference_tuning":{"matching_records":reference_tuning.matching_records,"scale_count":reference_tuning.scales.as_ref().map(|s|s.len()),"default_scale":reference_tuning.reference_default_scale,"issues":reference_tuning.issues,"native_qualified":reference_tuning.native_qualified},"morph_operations":plan.morph.as_ref().map(|m|m.operations.iter().map(|o|json!({"source":o.source,"weight":o.weight,"key":o.resource.key.to_string(),"error":o.resource.error,"policy_state":format!("{:?}",policy.resolve_key(o.resource.key).state),"candidates":policy.resolve_key(o.resource.key).candidates.iter().map(|p|json!({"path":p.package_path,"priority":p.priority,"entry":p.provider.entry_index})).collect::<Vec<_>>()})).collect::<Vec<_>>()),"morph_links":plan.morph.as_ref().map(|m|m.link_resources.iter().map(|r|json!({"key":r.key.to_string(),"selected":r.selected.map(|p|(p.package_index,p.entry_index)),"error":r.error,"policy_state":format!("{:?}",policy.resolve_key(r.key).state),"policy_selected":policy.resolve_key(r.key).selected.map(|p|(p.package_index,p.entry_index))})).collect::<Vec<_>>()),"morph_source_links":plan.morph.as_ref().map(|m|m.link_resources.len()),"gates":plan.gates.iter().map(|g|json!({"code":format!("{:?}",g.code),"source":g.source,"detail":g.detail,"required":g.required_for_complete})).collect::<Vec<_>>(),"complete_character":plan.complete,"source_conflicts":project.check_source_conflicts().len(),"package_index_pins_unchanged":unchanged,"game_contacted":false,"game_or_profile_writes":false});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("parts={} meshes={} parsed_meshes={} vertices={} retained_bytes={} gates={} assembly_ms={} source_conflicts={}",plan.parts.len(),plan.parts.iter().map(|p|p.meshes.len()).sum::<usize>(),plan.parts.iter().flat_map(|p|&p.meshes).filter(|m|m.native_rest.is_some()).count(),plan.inspection_vertices,plan.resource_bytes,plan.gates.len(),elapsed,project.check_source_conflicts().len());
    Ok(())
}
