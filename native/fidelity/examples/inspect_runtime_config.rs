// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only targeted original UI manifest lookup. Output contains private pins.
use external_cas_assets::{PackageIndex, ReadLimits};
use external_cas_fidelity::fnv1_resource_name;
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: inspect_runtime_config UI.package [exact source names]")?;
    let names = std::env::args().skip(2).collect::<Vec<_>>();
    let names = if names.is_empty() {
        vec![
            "widgetlist".into(),
            "widgetlist.xml".into(),
            "UI/widgetlist.xml".into(),
            "UIContainer".into(),
            "UIRoot".into(),
        ]
    } else {
        names
    };
    let index = PackageIndex::open(path)?;
    let mut reader = index.reader()?;
    let mut observations = Vec::new();
    let mut types = std::collections::BTreeMap::new();
    for e in &index.entries {
        *types
            .entry(format!("{:08X}", e.key.type_id))
            .or_insert(0usize) += 1;
    }
    for name in names {
        let instance = fnv1_resource_name(&name);
        for e in index
            .entries
            .iter()
            .filter(|e| e.key.instance_id == instance && !e.deleted())
        {
            let raw = reader.read_resource(
                e,
                ReadLimits {
                    max_resource_bytes: 64 * 1024 * 1024,
                },
            )?;
            let text = std::str::from_utf8(&raw).ok().filter(|s| {
                s.trim_start_matches('\u{feff}')
                    .trim_start()
                    .starts_with('<')
            });
            observations.push(json!({"name":name,"key":e.key.to_string(),"ordinal":e.ordinal,"index_sha256":index.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),"payload_sha256":format!("{:x}",Sha256::digest(&raw)),"bytes":raw.len(),"xml":text}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"private":true,"source":index.path,"resource_type_counts":types,"observations":observations})
        )?
    );
    Ok(())
}
