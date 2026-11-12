//! combined-pins.json oracle.dll report.json
use external_cas_assets::{PackageIndex, ReadLimits};
use external_cas_character::inspect_combined_tuning;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};
fn flag(values: &Option<Vec<String>>, field: &str) -> u32 {
    values
        .as_ref()
        .map(|v| {
            v.iter().fold(0, |a, s| {
                a | match (field, s.as_str()) {
                    ("age", "INFANT") => 128,
                    ("age", "TODDLER") => 2,
                    ("age", "CHILD") => 4,
                    ("age", "TEEN") => 8,
                    ("age", "YOUNGADULT") => 16,
                    ("age", "ADULT") => 32,
                    ("age", "ELDER") => 64,
                    ("gender", "MALE") => 4096,
                    ("gender", "FEMALE") => 8192,
                    ("species", "HUMAN") => 1,
                    ("species", "DOG") => 2,
                    ("species", "CAT") => 3,
                    ("species", "SMALLDOG") => 4,
                    ("species", "FOX") => 5,
                    ("species", "HORSE") => 6,
                    ("occult", "HUMAN") => 1,
                    ("occult", "ALIEN") => 2,
                    ("occult", "VAMPIRE") => 4,
                    ("occult", "MERMAID") => 8,
                    ("occult", "WITCH") => 16,
                    ("occult", "WEREWOLF") => 32,
                    ("occult", "FAIRY") => 64,
                    _ => 0, // The unchanged reference parser ignores unrecognized tokens.
                }
            })
        })
        .unwrap_or(0)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 3 && args.len() != 4 {
        return Err("combined-pins oracle report [S4TK-models-root]".into());
    }
    let pins: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let mut source_count = 0;
    let mut record_count = 0;
    let mut scale_count = 0;
    let mut reports = vec![];
    for source in pins["records"].as_array().ok_or("Records absent")? {
        if source["format"] == "xml-indexable" {
            continue;
        }
        let package = PackageIndex::open(source["source"]["path"].as_str().unwrap())?;
        let row = &package.entries[source["row"]["ordinal"].as_u64().unwrap() as usize];
        assert_eq!(row.key.to_string(), source["row"]["key"].as_str().unwrap());
        let raw = package.read_resource(row, ReadLimits::default())?;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw)),
            source["payloadSha256"].as_str().unwrap()
        );
        let start = std::time::Instant::now();
        let inspection = match inspect_combined_tuning(&raw, &AtomicBool::new(false)) {
            Ok(v) => v,
            Err(e) => {
                reports.push(serde_json::json!({"container_key":row.key.to_string(),"source_path":package.path,"unsupported":e.to_string()}));
                continue;
            }
        };
        let ms = start.elapsed().as_millis();
        assert!(inspection.issues.is_empty(), "{:?}", inspection.issues);
        if let Some(models) = args.get(3) {
            let adapter = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("oracle/combined-data-reference.cjs");
            let mut child = Command::new("node")
                .arg("--max-old-space-size=512")
                .arg(adapter)
                .arg(models)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            child.stdin.take().unwrap().write_all(&raw)?;
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(format!(
                    "Independent S4TK graph failed {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            let expected = expected.as_array().ok_or("S4TK records array expected")?;
            assert_eq!(expected.len(), inspection.records.len());
            for record in &inspection.records {
                let e = expected
                    .iter()
                    .find(|e| {
                        e["instance"].as_str().and_then(|s| s.parse::<u64>().ok())
                            == Some(record.instance)
                    })
                    .ok_or("S4TK record absent")?;
                for (name, values) in [
                    ("Ages", &record.context.ages),
                    ("Genders", &record.context.genders),
                    ("Species", &record.context.species),
                    ("OccultType", &record.context.occult),
                ] {
                    assert_eq!(e["context"][name], serde_json::json!(values));
                }
                let bindings = record
                    .scale_bindings
                    .iter()
                    .map(|(_, name, scale)| {
                        serde_json::json!([name, format!("{:08X}", scale.to_bits())])
                    })
                    .collect::<Vec<_>>();
                assert_eq!(e["bindings"], serde_json::json!(bindings));
            }
        }
        let mut input = vec![];
        input.extend((inspection.records.len() as u32).to_le_bytes());
        for record in &inspection.records {
            let xml = record.resolved_xml.as_ref().ok_or("No resolved XML")?;
            input.extend((xml.len() as u32).to_le_bytes());
            input.extend(xml);
        }
        let mut p = Command::new("dotnet")
            .arg(&args[1])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        p.stdin.take().unwrap().write_all(&input)?;
        let output = p.wait_with_output()?;
        if !output.status.success() {
            return Err(format!("Oracle error {}", String::from_utf8_lossy(&output.stderr)).into());
        }
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let expected = expected.as_array().ok_or("Expected array")?;
        assert_eq!(expected.len(), inspection.records.len());
        for (record, expected) in inspection.records.iter().zip(expected) {
            let c = &record.context;
            assert_eq!(
                expected["age"].as_u64().unwrap() as u32,
                flag(&c.ages, "age")
            );
            for (name, value, default) in [
                ("species", flag(&c.species, "species"), 1),
                ("occult", flag(&c.occult, "occult"), 1),
                ("gender", flag(&c.genders, "gender"), 12288),
            ] {
                assert_eq!(
                    expected[name].as_u64().unwrap() as u32,
                    if value == 0 { default } else { value }
                );
            }
            assert_eq!(
                expected["names"].as_u64().unwrap() as usize,
                record.modifier_names.len()
            );
            let scales = expected["scales"].as_object().ok_or("Expected scale map")?;
            assert_eq!(scales.len(), record.scales.len());
            for (key, scale) in &record.scales {
                assert_eq!(
                    scales[&format!("{key:016X}")].as_str().unwrap(),
                    format!("{:08X}", scale.to_bits())
                );
            }
            scale_count += record.scales.len();
            record_count += 1;
        }
        reports.push(serde_json::json!({"container_key":row.key.to_string(),"payload_sha256":source["payloadSha256"],"records":inspection.records.len(),"retained_nodes":inspection.retained_nodes,"shared_entries":inspection.shared_entries,"parse_ms":ms,"record_contexts":inspection.records.iter().map(|r|serde_json::json!({"instance":format!("{:016X}",r.instance),"namespace":r.resource_namespace,"age":r.context.ages,"gender":r.context.genders,"species":r.context.species,"occult":r.context.occult,"scales":r.scales.len()})).collect::<Vec<_>>() }));
        source_count += 1;
    }
    let report = serde_json::json!({"schema":1,"actual_combined_binary_sources":source_count,"actual_modifier_records":record_count,"all_scale_float_bits_compared":scale_count,"contexts_and_scales_match_unchanged_reference_parser":true,"shared_graph_independent_comparison":args.len()==4,"native_context_and_precedence_qualified":false,"scales_applied_to_native_sim":false,"sources":reports,"game_contacted":false});
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[2])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("sources={source_count} records={record_count} scale_bits={scale_count}");
    Ok(())
}
