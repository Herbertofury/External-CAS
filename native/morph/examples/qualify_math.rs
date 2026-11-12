//! Opt-in read-only real-resource comparison with recovered unchanged C# methods.
//! Arguments: dmap-pins.json bgeo-pins.json dmap-oracle.dll bgeo-oracle.dll report.json
use external_cas_assets::{Geometry, PackageIndex, ReadLimits};
use external_cas_morph::{apply_bgeo, apply_dmap, Bgeo, Dmap};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn positions(g: &Geometry, normal: bool) -> String {
    let b: Vec<_> = g
        .vertices
        .iter()
        .flat_map(|v| if normal { v.normal } else { v.position })
        .flat_map(f32::to_le_bytes)
        .collect();
    hash(&b)
}
fn u32b(b: &mut Vec<u8>, v: usize) {
    b.extend((v as u32).to_le_bytes());
}
fn f32b(b: &mut Vec<u8>, v: f32) {
    b.extend(v.to_le_bytes());
}
fn compare(dll: &str, input: Vec<u8>, g: &Geometry) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("dotnet")
        .arg(dll)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(&input)?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!("Oracle failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    let v: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(v["positionHash"].as_str().unwrap(), positions(g, false));
    assert_eq!(v["normalHash"].as_str().unwrap(), positions(g, true));
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 6 {
        return Err("expected six arguments".into());
    }
    let pins: serde_json::Value = serde_json::from_slice(&fs::read(&args[0])?)?;
    let bp: serde_json::Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let package = PackageIndex::open(pins["source"]["path"].as_str().unwrap())?;
    let mut reader = package.reader()?;
    let mut meshes = vec![];
    for f in pins["fixtures"].as_array().unwrap() {
        let raw = fs::read(f["path"].as_str().unwrap())?;
        assert_eq!(hash(&raw), f["sha256"].as_str().unwrap());
        meshes.push(Geometry::parse(&raw)?);
    }
    let mut dc = 0;
    let mut dv = 0;
    let mut changed = 0;
    let mut versions = std::collections::BTreeSet::new();
    let mut map_pins = vec![];
    for row in pins["rows"].as_array().unwrap() {
        if row["metadata"]["kind"] != 0 || row["status"] != "dmap-structure-candidate" {
            continue;
        }
        let entry = &package.entries[row["ordinal"].as_u64().unwrap() as usize];
        let raw = reader.read_resource(entry, ReadLimits::default())?;
        assert_eq!(hash(&raw), row["payloadSha256"].as_str().unwrap());
        let map = Dmap::parse(&raw)?;
        versions.insert(map.version);
        map_pins.push(hash(&raw));
        for geometry in &meshes {
            let original = geometry
                .vertices
                .iter()
                .map(|v| v.position)
                .collect::<Vec<_>>();
            for weight in [0.75f32, -0.5] {
                let result = apply_dmap(geometry, &original, &map, weight, None)?;
                let mut b = vec![];
                u32b(&mut b, geometry.vertices.len());
                for (i, v) in geometry.vertices.iter().enumerate() {
                    for f in v.position.into_iter().chain(v.normal).chain(v.uvs[1]) {
                        f32b(&mut b, f);
                    }
                    b.extend(v.tag.ok_or("no tag")?.to_le_bytes());
                    let seams = geometry.stitch_uvs(i)?;
                    u32b(&mut b, seams.len());
                    for s in seams {
                        for f in s {
                            f32b(&mut b, *f);
                        }
                    }
                }
                u32b(&mut b, raw.len());
                b.extend(&raw);
                f32b(&mut b, weight);
                u32b(&mut b, 0);
                f32b(&mut b, 0.0);
                compare(&args[2], b, &result.geometry)?;
                assert_eq!(result.geometry.original, geometry.original);
                dc += 1;
                dv += geometry.vertices.len();
                changed += result.stats.changed_positions;
            }
        }
    }
    let row = &bp["bgeo"]["row"];
    let raw = reader.read_resource(
        &package.entries[row["ordinal"].as_u64().unwrap() as usize],
        ReadLimits::default(),
    )?;
    assert_eq!(hash(&raw), bp["bgeo"]["payloadSha256"].as_str().unwrap());
    let bg = Bgeo::parse(&raw)?;
    let body_pins: serde_json::Value = serde_json::from_slice(&fs::read(&args[5])?)?;
    let body_package = PackageIndex::open(body_pins["sources"][0]["path"].as_str().unwrap())?;
    let mut body_reader = body_package.reader()?;
    let mut body_meshes = vec![];
    for part in body_pins["parts"].as_array().unwrap() {
        let bytes = body_reader.read_resource(
            &body_package.entries[part["row"]["ordinal"].as_u64().unwrap() as usize],
            ReadLimits::default(),
        )?;
        assert_eq!(hash(&bytes), part["payloadSha256"].as_str().unwrap());
        let g = Geometry::parse(&bytes)?;
        if g.vertices.iter().all(|v| v.vertex_id.is_some()) {
            body_meshes.push(g);
        }
    }
    if body_meshes.is_empty() {
        return Err("No native IDs in selected actual game GEOMs".into());
    }
    let mut bc = 0;
    let mut bv = 0;
    for geometry in &body_meshes {
        for weight in [0.75f32, -0.5, 0.0] {
            let result = apply_bgeo(geometry, &bg, 0, weight, false)?;
            let mut b = vec![];
            u32b(&mut b, geometry.vertices.len());
            for v in &geometry.vertices {
                for f in v.position.into_iter().chain(v.normal) {
                    f32b(&mut b, f);
                }
                b.extend(v.vertex_id.ok_or("no ID")?.to_le_bytes());
                b.extend(v.tag.unwrap_or(0).to_le_bytes());
            }
            u32b(&mut b, raw.len());
            b.extend(&raw);
            f32b(&mut b, weight);
            u32b(&mut b, 0);
            compare(&args[3], b, &result.geometry)?;
            assert_eq!(result.geometry.original, geometry.original);
            bc += 1;
            bv += geometry.vertices.len();
        }
    }
    let report = serde_json::json!({"schema":1,"oracle":"recovered unchanged SimRipper methods over parsed GEOM shims","dmap":{"cases":dc,"vertex_observations":dv,"changed_positions":changed,"versions":versions,"payload_sha256":map_pins},"bgeo":{"cases":bc,"vertex_observations":bv,"payload_sha256":hash(&raw)},"original_geom_bytes_preserved":true,"game_acceptance":false,"normal_maps":"synthetic tests only; same-crop application guarded"});
    let path = Path::new(&args[4]);
    if path.exists() {
        return Err("report target exists".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("{}", report);
    Ok(())
}
