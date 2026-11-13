//! Read-only explicit small-sample comparison to the unchanged pinned GPL CASP
//! reader, supplied as an absolute private harness path. Never runs game code.
use external_cas_assets::{CasPart, PackageIndex, ReadLimits, dbpf::CASP};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::args()
            .nth(1)
            .ok_or("Explicit private config required")?,
    )?)?;
    if config["private"] != true {
        return Err("Private sample manifest required".into());
    }
    let oracle = std::path::Path::new(
        config["oracle"]
            .as_str()
            .ok_or("Explicit GPL source-reader harness required")?,
    );
    if !oracle.is_absolute() {
        return Err("Absolute oracle harness required".into());
    }
    let limit = config["per_package_limit"].as_u64().unwrap_or(3) as usize;
    let cancel = AtomicBool::new(false);
    let mut rows = Vec::new();
    for path in config["packages"]
        .as_array()
        .ok_or("Explicit packages required")?
    {
        let package = PackageIndex::open(path.as_str().ok_or("Package path required")?)?;
        let mut reader = package.reader()?;
        for entry in package
            .entries
            .iter()
            .filter(|e| !e.deleted() && e.key.type_id == CASP)
            .take(limit)
        {
            let raw = reader.read_resource(
                entry,
                ReadLimits {
                    max_resource_bytes: 16 * 1024 * 1024,
                },
            )?;
            let part = CasPart::parse(&raw)?;
            let mut child = Command::new(oracle)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .ok_or("Oracle stdin unavailable")?
                .write_all(&raw)?;
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(format!(
                    "GPL CASP reader refused: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            let text = String::from_utf8(output.stdout)?;
            let values: Vec<u32> = text
                .trim()
                .split('\t')
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            if values.len() != 3
                || values[0] != part.version
                || values[1] != part.body_type
                || if part.version >= 27 {
                    Some(values[2]) != part.shared_uv_space
                } else {
                    part.shared_uv_space.is_some()
                }
            {
                return Err(format!("Source SharedUV mismatch for {}", entry.key).into());
            }
            rows.push(json!({"path":package.path,"key":entry.key.to_string(),"version":part.version,"body_type":part.body_type,
                "shared_uv_raw":part.shared_uv_space,"effective_shared_uv_body_type":part.effective_shared_uv_body_type(),
                "payload_sha256":format!("{:x}",Sha256::digest(&raw)),"source_index_sha256":package.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),
                "original_csharp_reader_match":true}));
        }
        package.revalidate(&cancel)?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"private":true,"game_contact":false,"source_files_unchanged":true,"rows":rows})
        )?
    );
    Ok(())
}
