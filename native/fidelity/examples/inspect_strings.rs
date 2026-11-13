// SPDX-License-Identifier: GPL-3.0-or-later
//! Writes private original strings/pins to stdout; caller chooses destination.
use external_cas_assets::PackageIndex;
use external_cas_fidelity::localization::StringBank;
use std::sync::atomic::AtomicBool;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: inspect_strings explicitly-selected-locale.package")?;
    let index = PackageIndex::open(path)?;
    let bank = StringBank::load(&index, &AtomicBool::new(false))?;
    let mut tables = Vec::new();
    for item in &bank.tables {
        let entries = item
            .table
            .entries
            .iter()
            .map(|e| {
                Ok(serde_json::json!({"key":e.hash,"flags":e.flags,"value":item.table.text(e)?}))
            })
            .collect::<Result<Vec<_>, String>>()?;
        tables.push(serde_json::json!({"pin":item.pin,"entries":entries,"issues":item.table.issues,"reserved":item.table.reserved,"trailing_bytes":item.table.trailing.len()}));
    }
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"private":true,"tables":tables,"distinct_hashes":bank.distinct_hashes()})
        )?
    );
    Ok(())
}
