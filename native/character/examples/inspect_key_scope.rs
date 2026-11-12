//! game-root type:group:instance report packages... (read-only diagnosis)
use external_cas_assets::{Catalog, ResourceKey};
use external_cas_scene::GameInstallScope;
use std::{io::Write, path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() < 4 {
        return Err("game-root T:G:I report packages...".into());
    }
    let text = args[1].to_str().ok_or("Key text")?;
    let fields = text
        .split(':')
        .map(|s| u64::from_str_radix(s, 16))
        .collect::<Result<Vec<_>, _>>()?;
    if fields.len() != 3 || fields[0] > u32::MAX as u64 || fields[1] > u32::MAX as u64 {
        return Err("Key extent".into());
    }
    let key = ResourceKey::new(fields[0] as u32, fields[1] as u32, fields[2]);
    let catalog = Catalog::scan_roots(&args[3..], &AtomicBool::new(false), |_| {})?;
    let scope = GameInstallScope::discover(&args[0])?;
    let policy = scope.prepare(&catalog)?;
    let resolution = policy.resolve_key(key);
    let report = serde_json::json!({"key":key.to_string(),"state":format!("{:?}",resolution.state),"selected":resolution.selected.map(|p|(p.package_index,p.entry_index)),"candidates":resolution.candidates.iter().map(|p|{let row=catalog.provider(p.provider).unwrap().1;serde_json::json!({"path":p.package_path,"entry_index":p.provider.entry_index,"priority":p.priority,"deleted":p.deleted,"compression":row.compression,"committed":row.committed,"stored_size":row.stored_size,"memory_size":row.memory_size,"evidence":p.evidence.iter().map(|r|serde_json::json!({"config":r.config_path,"line":r.line,"pattern":r.pattern,"priority":r.priority})).collect::<Vec<_>>()})}).collect::<Vec<_>>()});
    scope.revalidate()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("{} {:?}", key, resolution.state);
    Ok(())
}
