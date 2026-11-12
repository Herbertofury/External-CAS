//! project game-root body-pins oracle.dll report packages...
use external_cas_assets::{Catalog, Geometry, PackageIndex, ReadLimits, Rig};
use external_cas_morph::{apply_bond_mesh, build_morph_plan, Bond, MorphData, MorphOptions};
use external_cas_scene::{resolve_scene, GameInstallScope, SceneOptions};
use external_cas_tray::{reopen_project, ProjectDraft};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn ah(g: &Geometry, n: bool) -> String {
    hash(
        &g.vertices
            .iter()
            .flat_map(|v| if n { v.normal } else { v.position })
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}
fn oracle(
    g: &Geometry,
    rig: &[u8],
    bond: &Bond,
    weight: f32,
    dll: &PathBuf,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut b = vec![];
    b.extend((rig.len() as u32).to_le_bytes());
    b.extend(rig);
    b.extend((bond.raw.len() as u32).to_le_bytes());
    b.extend(&bond.raw);
    b.extend(weight.to_le_bytes());
    b.extend((g.vertices.len() as u32).to_le_bytes());
    for v in &g.vertices {
        for f in v.position.into_iter().chain(v.normal) {
            b.extend(f.to_le_bytes());
        }
        for j in v.joints {
            b.push(u8::try_from(j)?);
        }
        b.extend(v.raw_weights);
    }
    b.extend((g.bone_palette.len() as u32).to_le_bytes());
    for h in &g.bone_palette {
        b.extend(h.to_le_bytes());
    }
    let mut p = Command::new("dotnet")
        .arg(dll)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    p.stdin.take().unwrap().write_all(&b)?;
    let o = p.wait_with_output()?;
    if !o.status.success() {
        return Err(format!("Oracle error {}", String::from_utf8_lossy(&o.stderr)).into());
    }
    Ok(serde_json::from_slice(&o.stdout)?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() < 6 {
        return Err("project game-root body-pins oracle report packages...".into());
    }
    let project = reopen_project(&args[0])?;
    let draft = ProjectDraft::from_project(&project)?;
    let sims = draft.appearance()?;
    let sim = sims.first().ok_or("No native Sim")?;
    let catalog = Catalog::scan_roots(&args[5..], &AtomicBool::new(false), |_| {})?;
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
    let raw_rig = scene.rig_payload.as_ref().ok_or("No native RIG")?;
    let rig = Rig::parse(raw_rig)?;
    let pins: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let package = PackageIndex::open(pins["sources"][0]["path"].as_str().unwrap())?;
    let mut reader = package.reader()?;
    let mut meshes = vec![];
    for part in pins["parts"].as_array().unwrap() {
        let raw = reader.read_resource(
            &package.entries[part["row"]["ordinal"].as_u64().unwrap() as usize],
            ReadLimits::default(),
        )?;
        assert_eq!(hash(&raw), part["payloadSha256"].as_str().unwrap());
        let g = Geometry::parse(&raw)?;
        if g.vertices.len() <= 10000
            && g.vertices.iter().all(|v| v.has_skin)
            && g.palette_error.is_none()
        {
            meshes.push(g);
        }
    }
    let limit = std::env::var("CAS_MORPH_BOND_LIMIT")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(usize::MAX);
    let mut cases = 0;
    let mut vertices = 0;
    let mut positions = 0;
    let mut normals = 0;
    let mut resources = 0;
    for op in &plan.operations {
        let Some(MorphData::Bond(bond)) = &op.data else {
            continue;
        };
        if resources >= limit {
            break;
        }
        resources += 1;
        for g in &meshes {
            for weight in [0.75f32, -0.5] {
                let actual = apply_bond_mesh(g, &rig, bond, weight)?;
                let expected = oracle(g, raw_rig, bond, weight, &args[3])?;
                assert_eq!(
                    expected["positionHash"].as_str().unwrap(),
                    ah(&actual.geometry, false),
                    "{} position weight{}",
                    op.resource.key,
                    weight
                );
                assert_eq!(
                    expected["normalHash"].as_str().unwrap(),
                    ah(&actual.geometry, true),
                    "{} normal weight{}",
                    op.resource.key,
                    weight
                );
                assert_eq!(actual.geometry.original, g.original);
                cases += 1;
                vertices += g.vertices.len();
                positions += actual.stats.changed_positions;
                normals += actual.stats.changed_normals;
            }
        }
    }
    scope.revalidate()?;
    assert!(project.check_source_conflicts().is_empty());
    let report = serde_json::json!({"schema":1,"actual_imported_sim_linked_bonds":resources,"actual_game_geom_fixtures":meshes.len(),"native_rig_bones":rig.bones.len(),"cases":cases,"vertex_observations":vertices,"changed_position_observations":positions,"changed_normal_observations":normals,"arrays_match_original_csharp":true,"untuned_modifier_test_weights_explicit":true,"original_geom_bytes_preserved":true,"source_conflicts":0,"complete_morphology":false,"game_contacted":false});
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[4])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("{}", report);
    Ok(())
}
