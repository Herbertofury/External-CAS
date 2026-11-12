//! project game-root oracle.dll report packages...
//! Compares rig-local updates with unmodified upstream RIG/BOND/Geometry.
use external_cas_assets::{Catalog, Rig};
use external_cas_morph::{apply_bond_rig, build_morph_plan, MorphData, MorphOptions};
use external_cas_scene::{resolve_scene, GameInstallScope, SceneOptions};
use external_cas_tray::{reopen_project, ProjectDraft};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() < 5 {
        return Err("project game-root oracle report packages...".into());
    }
    let project = reopen_project(&args[0])?;
    let draft = ProjectDraft::from_project(&project)?;
    let sims = draft.appearance()?;
    let sim = sims.first().ok_or("No native Sim")?;
    let catalog = Catalog::scan_roots(&args[4..], &AtomicBool::new(false), |_| {})?;
    let scope = GameInstallScope::discover(&args[1])?;
    let policy = scope.prepare(&catalog)?;
    let first = resolve_scene(sim, &catalog, &SceneOptions::default());
    let mut so = SceneOptions::default();
    so.provider_choices = policy
        .provider_choices_for_keys(first.required_keys())
        .choices;
    let scene = resolve_scene(sim, &catalog, &so);
    let plan = build_morph_plan(
        sim,
        &scene,
        &catalog,
        Some(&policy),
        &MorphOptions::default(),
    )?;
    let raw_rig = scene.rig_payload.as_ref().ok_or("Native rig unresolved")?;
    let rig = Rig::parse(raw_rig)?;
    let mut cases = 0;
    let mut adjustments = 0;
    let mut descendants = 0;
    let mut declined = vec![];
    for op in &plan.operations {
        let Some(MorphData::Bond(bond)) = &op.data else {
            continue;
        };
        for weight in [0.75f32, -0.5, 0.0] {
            let result = match apply_bond_rig(&rig, bond, weight) {
                Ok(r) => r,
                Err(e) => {
                    declined.push(serde_json::json!({"key":op.resource.key.to_string(),"weight":weight,"reason":e.to_string()}));
                    continue;
                }
            };
            let mut b = vec![];
            b.extend((raw_rig.len() as u32).to_le_bytes());
            b.extend(raw_rig);
            b.extend((bond.raw.len() as u32).to_le_bytes());
            b.extend(&bond.raw);
            b.extend(weight.to_le_bytes());
            let mut p = Command::new("dotnet")
                .arg(&args[2])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            p.stdin.take().unwrap().write_all(&b)?;
            let output = p.wait_with_output()?;
            if !output.status.success() {
                return Err(format!(
                    "Original BOND oracle failed {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            let bones = expected.as_array().ok_or("Oracle array expected")?;
            assert_eq!(bones.len(), rig.bones.len());
            for (i, bone) in result.rig.bones.iter().enumerate() {
                assert_eq!(bones[i]["hash"].as_u64().unwrap() as u32, bone.hash);
                for (name, value) in [
                    ("position", bone.position.to_vec()),
                    ("orientation", bone.orientation.to_vec()),
                    ("scale", bone.scale.to_vec()),
                ] {
                    for (k, a) in value.iter().enumerate() {
                        let e = bones[i][name][k].as_f64().unwrap() as f32;
                        assert_eq!(
                            a.to_bits(),
                            e.to_bits(),
                            "{} weight{} bone{} field{} axis{}",
                            op.resource.key,
                            weight,
                            i,
                            name,
                            k
                        );
                    }
                }
            }
            cases += 1;
            adjustments += result.adjustments;
            descendants += result.descendant_position_updates;
        }
    }
    scope.revalidate()?;
    assert!(project.check_source_conflicts().is_empty());
    let report = serde_json::json!({"schema":1,"native_rig_bones":rig.bones.len(),"cases":cases,"bone_observations":cases*rig.bones.len(),"applied_adjustments":adjustments,"descendant_position_updates":descendants,"declined":declined,"local_arrays_match_original_csharp":true,"complete_mesh_deformation":false,"original_native_rig_unchanged":true,"source_conflicts":0,"game_contacted":false});
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[3])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("{}", report);
    Ok(())
}
