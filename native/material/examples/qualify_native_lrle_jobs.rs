//! PRIVATE_PROJECT EXCLUSIVE_PRIVATE_DIRECTORY GAME_ROOT EXPLICIT_PACKAGES...
//! Emits proprietary input derivatives only to an explicitly private directory.
use external_cas_assets::{Catalog,ReadLimits};
use external_cas_character::{assemble_character,CharacterOptions,ResourceResolver};
use external_cas_material::{Pixels,native_image_source::resolve_native_rle_blend_source,native_blend::{blend_native_image_in_place,NativeBlendAlpha},native_lrle::prepare_native_lrle_mip_zero};
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project,ProjectDraft};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},path::PathBuf,sync::atomic::AtomicBool,io::Write};
#[cfg(windows)]fn below_normal(){#[link(name="kernel32")]unsafe extern "system"{fn GetCurrentProcess()->isize;fn SetPriorityClass(process:isize,class:u32)->i32;}unsafe{SetPriorityClass(GetCurrentProcess(),0x4000);}}
fn exclusive(path:&std::path::Path,bytes:&[u8])->Result<(),Box<dyn std::error::Error>>{let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;file.write_all(bytes)?;file.sync_all()?;Ok(())}
fn u32le(bytes:&[u8],at:usize)->Result<usize,Box<dyn std::error::Error>>{Ok(u32::from_le_bytes(bytes.get(at..at+4).ok_or("LRLE extent")?.try_into()?)as usize)}
fn main()->Result<(),Box<dyn std::error::Error>>{
    #[cfg(windows)]below_normal();let args:Vec<_>=std::env::args_os().skip(1).map(PathBuf::from).collect();if args.len()<4{return Err("project exclusive-private-directory game-root packages...".into())}if args[1].exists(){return Err("Exclusive new private directory required".into())}
    let project=reopen_project(&args[0])?;let sims=ProjectDraft::from_project(&project)?.appearance()?;let cancel=AtomicBool::new(false);let catalog=Catalog::scan_roots(&args[3..],&cancel,|_|{})?;let scope=GameInstallScope::discover(&args[2])?;let policy=scope.prepare(&catalog)?;let plan=assemble_character(sims.first().ok_or("No native Sim")?,&catalog,Some(&policy),&CharacterOptions::default())?;
    let choices=BTreeMap::new();let mut resolver=ResourceResolver::new(&catalog,Some(&policy),&choices,ReadLimits{max_resource_bytes:256*1024*1024},512*1024*1024,&cancel);let mut seen=BTreeSet::new();let mut records=Vec::new();std::fs::create_dir(&args[1])?;
    for material in plan.parts.iter().flat_map(|p|&p.materials).filter(|m|m.role=="diffuse"){
        let resolved=resolve_native_rle_blend_source(&mut resolver,material.declared_key);let Some(source)=resolved.selected else{return Err(format!("Native source unresolved: {:?}",resolved.issues).into())};if source.key.type_id!=0x2bc04edf||!seen.insert(source.key){continue}
        let raw=source.bytes.as_ref().ok_or("Native LRLE unresolved")?;if raw.get(..4)!=Some(b"LRLE"){return Err("Native LRLE magic".into())}let width=u16::from_le_bytes(raw[8..10].try_into()?)as u32;let height=u16::from_le_bytes(raw[10..12].try_into()?)as u32;let mips=u32le(raw,12)?;if mips==0||mips>16{return Err("Native LRLE mip count".into())}let offsets=(0..mips).map(|i|u32le(raw,16+i*4)).collect::<Result<Vec<_>,_>>()?;
        let table_end=16+mips*4;let v2=raw.get(4..8)==Some(b"V002");let(palette_start,palette_count,data_at)=if v2{let count=u32le(raw,table_end)?;(table_end+4,count,table_end+4+count*4)}else if raw.get(4..8)==Some(&[0,0,0,0]){(table_end,0,table_end)}else{return Err("Unsupported native LRLE version".into())};if data_at>raw.len(){return Err("Native LRLE palette extent".into())}
        let mip_index=0;let(w,h)=(width,height);let start=data_at+offsets[mip_index];let end=if mip_index+1<mips{data_at+offsets[mip_index+1]}else{raw.len()};let stream=raw.get(start..end).ok_or("Native LRLE selected extent")?;
        let _selected_stream=stream;let prepared=prepare_native_lrle_mip_zero(raw.clone(),16*1024*1024,&cancel)?;let image=&prepared.pixels;
        let base=Pixels::new(w,h,(0..h).flat_map(|y|(0..w).flat_map(move|x|[(x*7+y*11)as u8,(x*13+y*3)as u8,(x*5+y*17)as u8,(x*19+y*23)as u8])).collect(),32*1024*1024)?;
        let mut outputs=Vec::new();for force_alpha in [false,true]{for opacity in [0.017,0.144,0.85,1.0]{let mut output=base.clone();blend_native_image_in_place(&mut output,&image,opacity,if force_alpha{NativeBlendAlpha::ForceSourceAlphaOpaque}else{NativeBlendAlpha::InterpolateSourceAlpha},&cancel)?;outputs.push(serde_json::json!({"opacity":opacity,"force_alpha":force_alpha,"rgba_sha256":format!("{:x}",Sha256::digest(&output.rgba))}));}}
        let file=format!("{}.lrle",source.key.to_string().replace(':',"-"));exclusive(&args[1].join(&file),raw)?;
        records.push(serde_json::json!({"key":source.key.to_string(),"declared":material.declared_key.to_string(),"selection":resolved.selection_rule,"payload_sha256":format!("{:x}",Sha256::digest(raw)),"raw_file":file,"version":if v2{2}else{0},"mips":mips,"selected_mip":mip_index,"width":w,"height":h,"palette_start":palette_start,"palette_count":palette_count,"stream_start":start,"stream_end":end,"decoded_rgba_sha256":format!("{:x}",Sha256::digest(&image.rgba)),"source_original_retained":std::sync::Arc::ptr_eq(raw,&prepared.source),"native_profile":format!("{:?}",prepared.profile),"nonzero_alpha_pixels":image.rgba.chunks_exact(4).filter(|p|p[3]!=0).count(),"outputs":outputs}));
    }
    scope.revalidate()?;for package in &catalog.packages{package.revalidate(&cancel)?;}if !project.check_source_conflicts().is_empty(){return Err("Original source conflict".into())}
    exclusive(&args[1].join("report.json"),&serde_json::to_vec_pretty(&serde_json::json!({"private":true,"records":records,"source_conflicts":0,"originals_written":false,"game_contacted":false,"native_canvas_synthesized":false,"native_lrle_job_qualified":false}))?)?;println!("actual native LRLE sources={} originals_unchanged=true",seen.len());Ok(())
}
