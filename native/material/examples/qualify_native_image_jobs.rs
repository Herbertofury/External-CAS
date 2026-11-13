//! PRIVATE_PROJECT EXCLUSIVE_PRIVATE_DIRECTORY GAME_ROOT EXPLICIT_PACKAGES...
use external_cas_assets::{Catalog,ReadLimits};
use external_cas_character::{assemble_character,CharacterOptions,ResourceResolver};
use external_cas_material::{Pixels,native_image_source::resolve_native_rle_blend_source,native_rle2::{NativeRle2Image,NativeRle2Operation,apply_native_rle2_mip}};
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project,ProjectDraft};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},path::PathBuf,sync::atomic::AtomicBool,io::Write};
#[cfg(windows)]fn below_normal(){#[link(name="kernel32")]unsafe extern "system"{fn GetCurrentProcess()->isize;fn SetPriorityClass(process:isize,class:u32)->i32;}unsafe{SetPriorityClass(GetCurrentProcess(),0x4000);}}
fn exclusive(path:&std::path::Path,bytes:&[u8])->Result<(),Box<dyn std::error::Error>>{let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;file.write_all(bytes)?;file.sync_all()?;Ok(())}
fn main()->Result<(),Box<dyn std::error::Error>>{
    #[cfg(windows)]below_normal();let args:Vec<_>=std::env::args_os().skip(1).map(PathBuf::from).collect();if args.len()<4{return Err("project exclusive-private-directory game-root packages...".into())}if args[1].exists(){return Err("Exclusive new private directory required".into())}
    let project=reopen_project(&args[0])?;let sims=ProjectDraft::from_project(&project)?.appearance()?;let cancel=AtomicBool::new(false);let catalog=Catalog::scan_roots(&args[3..],&cancel,|_|{})?;let scope=GameInstallScope::discover(&args[2])?;let policy=scope.prepare(&catalog)?;let plan=assemble_character(sims.first().ok_or("No native Sim")?,&catalog,Some(&policy),&CharacterOptions::default())?;
    let mut choices=BTreeMap::new();for source in plan.parts.iter().filter_map(|p|p.source.as_ref()).chain(plan.parts.iter().flat_map(|p|&p.materials).filter_map(|m|m.selected.as_ref())){if let Some(provider)=source.selected{choices.insert(source.key,provider);}}
    let mut resolver=ResourceResolver::new(&catalog,Some(&policy),&choices,ReadLimits{max_resource_bytes:256*1024*1024},512*1024*1024,&cancel);let mut seen=BTreeSet::new();let mut records=Vec::new();std::fs::create_dir(&args[1])?;
    for part in &plan.parts {for material in part.materials.iter().filter(|m|matches!(m.role.as_str(),"diffuse"|"shadow")){
        let(source,selection,observations,issues)=if material.role=="diffuse"{let result=resolve_native_rle_blend_source(&mut resolver,material.declared_key);(result.selected,result.selection_rule,result.observations,result.issues)}else{let source=resolver.get(material.declared_key);(source.bytes.is_some().then(||source.clone()),"Native shadow19E4730 retains its exact declared key".into(),vec![source],vec![])};
        let Some(source)=source else{records.push(serde_json::json!({"role":material.role,"declared":material.declared_key.to_string(),"issues":issues,"selection":selection}));continue};if !seen.insert((material.role.clone(),source.key)){continue}
        let raw=source.bytes.as_ref().ok_or("Source disappeared")?;let mut record=serde_json::json!({"role":material.role,"declared":material.declared_key.to_string(),"selected":source.key.to_string(),"selection":selection,"observation_keys":observations.iter().map(|s|s.key.to_string()).collect::<Vec<_>>(),"payload_sha256":format!("{:x}",Sha256::digest(raw)),"issues":issues});
        if raw.get(..8)==Some(b"DXT5RLE2") {
            let image=NativeRle2Image::parse(raw.clone(),&Default::default(),&cancel)?;let(mip_index,mip)=image.mips().iter().enumerate().find(|(_,m)|m.width<=16&&m.height<=32).ok_or("No bounded native mip")?;
            let rgba=(0..mip.height).flat_map(|y|(0..mip.width).flat_map(move|x|[(x*7+y*11)as u8,(x*13+y*3)as u8,(x*5+y*17)as u8,(x*19+y*23)as u8])).collect::<Vec<_>>();let base=Pixels::new(mip.width,mip.height,rgba,1024*1024)?;
            let operations=if material.role=="shadow"{vec![NativeRle2Operation::Shadow]}else{[0.017,0.144,0.85,1.0].into_iter().map(|opacity|NativeRle2Operation::UnshiftedOrdinaryBlend{opacity}).collect()};
            let mut outputs=Vec::new();for operation in operations{let output=apply_native_rle2_mip(&base,mip,operation,1024*1024,&cancel)?;outputs.push(serde_json::json!({"operation":format!("{operation:?}"),"opacity":match operation{NativeRle2Operation::UnshiftedOrdinaryBlend{opacity}=>Some(opacity),_=>None},"rgba_sha256":format!("{:x}",Sha256::digest(&output.rgba))}));}
            let name=format!("{}.rle2",source.key.to_string().replace(':',"-"));let raw_path=args[1].join(&name);if !raw_path.exists(){exclusive(&raw_path,raw)?;}
            record["native_rle2"]=serde_json::json!({"raw_file":name,"source_original_retained":std::sync::Arc::ptr_eq(raw,&image.original),"width":image.width,"height":image.height,"mips":image.mips().len(),"selected_mip":mip_index,"mip_width":mip.width,"mip_height":mip.height,"prepared_blocks":mip.blocks.len(),"streams":mip.stream_ranges.iter().map(|r|serde_json::json!({"start":r.start,"end":r.end,"sha256":format!("{:x}",Sha256::digest(&raw[r.clone()]))})).collect::<Vec<_>>(),"outputs":outputs});
        }else{record["unsupported_preparation"]=serde_json::json!("Exact source retained; native LRLE image-job execution remains separate");}
        records.push(record);
    }}
    scope.revalidate()?;for package in &catalog.packages{package.revalidate(&cancel)?;}if !project.check_source_conflicts().is_empty(){return Err("Original source conflict".into())}
    let report=serde_json::json!({"private":true,"records":records,"source_conflicts":0,"originals_written":false,"game_contacted":false,"native_canvas_synthesized":false,"complete_native_composition_qualified":false});exclusive(&args[1].join("report.json"),&serde_json::to_vec_pretty(&report)?)?;
    println!("actual native image jobs={} originals_unchanged=true private_dir={}",seen.len(),args[1].display());Ok(())
}
