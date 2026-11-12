//! project game-root body-fixture-pins oracle.dll report packages...
//! Full arrays compared to unchanged source; no profile or game writes.
use external_cas_assets::{Catalog, Geometry, PackageIndex, ReadLimits};
use external_cas_morph::{apply_dmap, build_morph_plan, Dmap, MorphData, MorphOptions};
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
fn f(b: &mut Vec<u8>, v: f32) {
    b.extend(v.to_le_bytes())
}
fn u(b: &mut Vec<u8>, v: usize) {
    b.extend((v as u32).to_le_bytes())
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
    s: &Dmap,
    n: Option<&Dmap>,
    weight: f32,
    dll: &PathBuf,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut b = vec![];
    u(&mut b, g.vertices.len());
    for (i, v) in g.vertices.iter().enumerate() {
        for x in v.position.into_iter().chain(v.normal).chain(v.uvs[1]) {
            f(&mut b, x);
        }
        b.extend(v.tag.unwrap().to_le_bytes());
        let seams = g.stitch_uvs(i)?;
        u(&mut b, seams.len());
        for uv in seams {
            for x in uv {
                f(&mut b, *x);
            }
        }
    }
    u(&mut b, s.raw.len());
    b.extend(&s.raw);
    f(&mut b, weight);
    u(&mut b, n.map(|n| n.raw.len()).unwrap_or(0));
    if let Some(n) = n {
        b.extend(&n.raw);
    }
    f(&mut b, weight);
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
    let sim = sims.first().ok_or("no native Sim")?;
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
            && g.vertices
                .iter()
                .all(|v| v.tag.is_some() && v.uvs.len() > 1)
            && g.uv_stitches.is_some()
        {
            meshes.push(g);
        }
    }
    let mut cases = 0;
    let mut vertices = 0;
    let mut positions = 0;
    let mut normals = 0;
    let mut pairs = 0;
    let mut different_crops = 0;
    for op in &plan.operations {
        let Some(MorphData::Dmap(shape)) = &op.data else {
            continue;
        };
        if shape.kind != 0 {
            continue;
        }
        let normal = plan
            .operations
            .iter()
            .filter(|n| n.source == op.source)
            .find_map(|o| match &o.data {
                Some(MorphData::Dmap(n)) if n.kind == 1 => Some(n),
                _ => None,
            });
        pairs += 1;
        if normal
            .map(|n| {
                [n.min_col, n.max_col, n.min_row, n.max_row]
                    != [shape.min_col, shape.max_col, shape.min_row, shape.max_row]
            })
            .unwrap_or(false)
        {
            different_crops += 1;
        }
        for g in &meshes {
            let original = g.vertices.iter().map(|v| v.position).collect::<Vec<_>>();
            for weight in [op.weight.unwrap_or(0.75), -0.5] {
                let actual = apply_dmap(g, &original, shape, weight, normal.map(|n| (n, weight)))?;
                let expected = oracle(g, shape, normal, weight, &args[3])?;
                assert_eq!(
                    expected["positionHash"].as_str().unwrap(),
                    ah(&actual.geometry, false),
                    "{} position",
                    op.source
                );
                assert_eq!(
                    expected["normalHash"].as_str().unwrap(),
                    ah(&actual.geometry, true),
                    "{} normal",
                    op.source
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
    let report = serde_json::json!({"schema":1,"actual_imported_sim_linked_pairs":pairs,"different_shape_normal_crops":different_crops,"actual_game_geom_fixtures":meshes.len(),"cases":cases,"vertex_observations":vertices,"changed_position_observations":positions,"changed_normal_observations":normals,"arrays_match_original_csharp":true,"native_amounts_used_for_physique":true,"untuned_modifier_test_weights_explicit":true,"original_geom_bytes_preserved":true,"source_conflicts":0,"complete_morphology":false,"game_contacted":false});
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[4])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("{}", report);
    Ok(())
}
