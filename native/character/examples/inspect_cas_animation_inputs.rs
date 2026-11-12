//! PRIVATE_REPORT GAME_ROOT EXPLICIT_PACKAGES...; authorized metadata only.
use external_cas_assets::{Catalog,ReadLimits,ResourceKey};
use external_cas_character::ResourceResolver;
use external_cas_scene::GameInstallScope;
use std::{collections::BTreeMap,path::PathBuf,sync::atomic::AtomicBool,io::Write};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args_os().skip(1).map(PathBuf::from).collect();if args.len()<3{return Err("report game-root packages...".into())}
    let cancel=AtomicBool::new(false);let catalog=Catalog::scan_roots(&args[2..],&cancel,|_|{})?;let scope=GameInstallScope::discover(&args[1])?;let policy=scope.prepare(&catalog)?;let choices=BTreeMap::new();let mut resolver=ResourceResolver::new(&catalog,Some(&policy),&choices,ReadLimits::default(),128*1024*1024,&cancel);
    let counts=[0x02d5df13u32,0x6b20c4f3,0xbc4a5044,0x0166038c].into_iter().map(|t|(format!("{t:08X}"),catalog.by_key.keys().filter(|k|k.type_id==t).count())).collect::<BTreeMap<_,_>>();
    let key=ResourceKey::new(0x02d5df13,0,0x92fd3d571bd80a40);let source=resolver.get(key);
    let mut headers=vec![];let mut keys=catalog.by_key.keys().filter(|k|k.type_id==0xbc4a5044).copied().collect::<Vec<_>>();keys.sort();
    if std::env::var_os("CAS_SKIP_HEADERS").is_some(){keys.clear();} for key in keys {let source=resolver.get(key);if let Some(raw)=&source.bytes {let text=String::from_utf8_lossy(raw);if text.to_ascii_lowercase().contains("cas") {headers.push(serde_json::json!({"key":key.to_string(),"size":raw.len(),"prefix_hex":raw.iter().take(64).map(|b|format!("{b:02x}")).collect::<String>(),"ascii_runs":raw.split(|b|!b.is_ascii_graphic()&&*b!=b' ').filter(|s|s.len()>4).map(|s|String::from_utf8_lossy(s).into_owned()).filter(|s|s.to_ascii_lowercase().contains("cas")).collect::<Vec<_>>()}));}}}
    let mut asm_records=vec![];if std::env::var_os("CAS_SCAN_ASMS").is_some(){let mut keys=catalog.by_key.keys().filter(|k|k.type_id==0x02d5df13).copied().collect::<Vec<_>>();keys.sort();for key in keys{let source=resolver.get(key);if let Some(raw)=&source.bytes{if let Ok(root)=external_cas_character::inspect_packed_xml_document(raw,&cancel){if root.attrs.values().any(|v|v.to_ascii_lowercase().contains("cas")){asm_records.push(serde_json::json!({"key":key.to_string(),"root":root}));}}}}}
    let report=serde_json::json!({"counts":counts,"asm_records":asm_records,"asm_key":source.key.to_string(),"asm_error":source.error,"asm_tree":source.bytes.as_ref().map(|b|external_cas_character::inspect_packed_xml_document(b,&cancel).map_err(|e|e.to_string())),"cas_header_observations":headers,"read_bytes":resolver.bytes});
    scope.revalidate()?;let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&args[0])?;file.write_all(&serde_json::to_vec_pretty(&report)?)?;file.sync_all()?;println!("{counts:?} CASheaders={}",headers.len());Ok(())
}
