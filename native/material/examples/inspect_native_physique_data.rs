//! EXCLUSIVE_PRIVATE_REPORT CASTUNING_INSTANCE_HEX GAME_ROOT PACKAGES...
use external_cas_assets::{Catalog,ReadLimits,ResourceKey};
use external_cas_character::ResourceResolver;
use external_cas_scene::GameInstallScope;
use std::{collections::BTreeMap,path::PathBuf,sync::atomic::AtomicBool,io::Write};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args_os().skip(1).collect();if args.len()<4{return Err("exclusive-private-report castuning-instance-hex game-root packages...".into())}let report=PathBuf::from(&args[0]);if report.exists(){return Err("Exclusive private report required".into())}let instance=u64::from_str_radix(args[1].to_str().ok_or("Instance encoding")?,16)?;let root=PathBuf::from(&args[2]);let packages=args[3..].iter().map(PathBuf::from).collect::<Vec<_>>();let cancel=AtomicBool::new(false);
    let catalog=Catalog::scan_roots(&packages,&cancel,|_|{})?;let scope=GameInstallScope::discover(&root)?;let policy=scope.prepare(&catalog)?;let choices=BTreeMap::new();let mut resolver=ResourceResolver::new(&catalog,Some(&policy),&choices,ReadLimits{max_resource_bytes:16*1024*1024},16*1024*1024,&cancel);let key=ResourceKey::new(0x545ac67a,0x004b5265,instance);let source=resolver.get(key);let raw=source.bytes.as_ref().ok_or_else(||{eprintln!("effective policy: {:?}",source.effective_policy);format!("Native CAS tuning SimData unresolved: {:?}",source.error)})?;let document=external_cas_room::parse_simdata(raw,&cancel)?;
    scope.revalidate()?;for source in &catalog.packages{source.revalidate(&cancel)?;}
    let result=serde_json::json!({"private":true,"key":key.to_string(),"payload_sha256":source.payload_sha256.map(|h|h.iter().map(|v|format!("{v:02x}")).collect::<String>()),"document":document,"originals_written":false,"game_contacted":false});let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(report)?;file.write_all(&serde_json::to_vec_pretty(&result)?)?;file.sync_all()?;println!("native CAS tuning {} schemas={} instances={} issues={}",key,document.schemas.len(),document.instances.len(),document.issues.len());Ok(())
}
