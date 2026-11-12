//! PRIVATE_PROJECT PRIVATE_REPORT GAME_ROOT EXPLICIT_PACKAGES...
use external_cas_assets::Catalog;
use external_cas_character::{assemble_character,resolve_cas_stand_idle,CharacterOptions};
use external_cas_scene::GameInstallScope;
use external_cas_tray::{reopen_project,ProjectDraft};
use external_cas_animation::{Rig,SampleOptions,AncillaryPolicy,UnsupportedChannelPolicy};
use sha2::{Digest,Sha256};
use std::{path::PathBuf,sync::atomic::AtomicBool,io::Write};
fn compare_primary(clip:&external_cas_animation::Clip,dll:&std::path::Path)->Result<serde_json::Value,Box<dyn std::error::Error>>{
    use std::process::{Command,Stdio};
    let mut child=Command::new("dotnet").arg(dll).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;child.stdin.take().unwrap().write_all(&clip.original)?;let output=child.wait_with_output()?;if !output.status.success(){return Err(format!("Primary reader failed: {}",String::from_utf8_lossy(&output.stderr)).into())}
    let reference:serde_json::Value=serde_json::from_slice(&output.stdout)?;let metadata=clip.metadata.as_ref().ok_or("No metadata")?;let current=metadata.current_header.as_ref().ok_or("No current wrapper metadata")?;let codec=clip.codec.as_ref().ok_or("No codec")?;
    assert_eq!(reference["version"].as_u64(),Some(clip.version as u64));assert_eq!(reference["rig"].as_u64(),Some(current.rig_instance));assert_eq!(reference["actor"].as_str(),Some(metadata.rig_namespace.as_str()));assert_eq!(reference["codecOffset"].as_u64(),Some(current.codec_range.start as u64));assert_eq!(reference["codecLength"].as_u64(),Some(current.codec_range.len() as u64));assert_eq!(reference["name"].as_str(),Some(codec.name.as_str()));assert_eq!(reference["sourceName"].as_str(),Some(codec.source_asset_name.as_str()));assert_eq!(reference["ticks"].as_u64(),Some(codec.ticks as u64));assert_eq!((reference["tickLength"].as_f64().unwrap()as f32).to_bits(),codec.tick_length.to_bits());
    let mut values=0usize;let mut different_bits=0usize;let mut maximum=0f32;
    for track in reference["tracks"].as_array().ok_or("Reference tracks missing")?{let hash=track["hash"].as_u64().unwrap()as u32;for (field,component) in [("position",1),("rotation",2)]{if track[field].is_null(){continue}let channel=codec.channels.iter().find(|c|c.target_hash==hash&&c.sub_target==component).ok_or("Reference channel absent")?;let frames=track[field].as_array().unwrap();assert_eq!(frames.len(),channel.frames.len());for (expected,actual) in frames.iter().zip(&channel.frames){assert_eq!(expected["tick"].as_u64(),Some(actual.tick as u64));let expected=expected["bits"].as_array().unwrap();let actual=actual.values.as_ref().ok_or("Current bone frame not decoded")?;assert_eq!(expected.len(),actual.len());for (bits,value) in expected.iter().zip(actual){let bits=bits.as_i64().unwrap()as i32 as u32;let reference=f32::from_bits(bits);assert!(value.is_finite()&&reference.is_finite());different_bits+=usize::from(value.to_bits()!=bits);maximum=maximum.max((value-reference).abs());values+=1;}}}}
    assert!(maximum<=2e-6,"Source quantization difference{maximum}");Ok(serde_json::json!({"wrapper_fields_exact":true,"skeletal_values_compared":values,"different_float_bits":different_bits,"maximum_float_difference":maximum,"source_reader":"unmodified separately supplied TS4AnimationBrowser.Core1.9.1 StudioClipDecoderV2"}))
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args_os().skip(1).map(PathBuf::from).collect();if args.len()<4{return Err("project report game-root packages...".into())}
    let project=reopen_project(&args[0])?;let sims=ProjectDraft::from_project(&project)?.appearance()?;let cancel=AtomicBool::new(false);
    let catalog=Catalog::scan_roots(&args[3..],&cancel,|_|{})?;let scope=GameInstallScope::discover(&args[2])?;let policy=scope.prepare(&catalog)?;
    let plan=assemble_character(&sims[0],&catalog,Some(&policy),&CharacterOptions::default())?;
    let start=std::time::Instant::now();let selected=resolve_cas_stand_idle(&plan,&catalog,Some(&policy),&cancel)?;let elapsed=start.elapsed().as_millis();
    let primary=if let Some(dll)=std::env::var_os("CAS_CURRENT_CLIP_ORACLE"){compare_primary(&selected.clip,&PathBuf::from(dll))?}else{serde_json::Value::Null};
    let rig=Rig::parse(plan.scene.rig_payload.as_ref().ok_or("No actual native rig")?)?;
    let referenced_rig=if let Some(header)=selected.clip.metadata.as_ref().and_then(|m|m.current_header.as_ref()){
        let choices=std::collections::BTreeMap::new();let mut resolver=external_cas_character::ResourceResolver::new(&catalog,Some(&policy),&choices,external_cas_assets::ReadLimits::default(),16*1024*1024,&cancel);
        let key=external_cas_assets::ResourceKey::new(0x8eaf13de,0,header.rig_instance);let source=resolver.get(key);
        if let Some(raw)=&source.bytes{let declared=Rig::parse(raw)?;
            let same_order=declared.bones.len()==rig.bones.len()&&declared.bones.iter().zip(&rig.bones).all(|(a,b)|a.hash==b.hash&&a.parent_index==b.parent_index);
            let differing=declared.bones.iter().zip(&rig.bones).filter(|(a,b)|a.position!=b.position||a.orientation!=b.orientation||a.scale!=b.scale).count();
            let differences=declared.bones.iter().zip(&rig.bones).enumerate().filter(|(_, (a,b))|a.position!=b.position||a.orientation!=b.orientation||a.scale!=b.scale).map(|(i,(a,b))|serde_json::json!({"index":i,"hash":format!("{:08X}",a.hash),"name":a.name,"declared_position":a.position,"scene_position":b.position,"declared_orientation":a.orientation,"scene_orientation":b.orientation,"declared_scale":a.scale,"scene_scale":b.scale})).collect::<Vec<_>>();
            serde_json::json!({"key":key.to_string(),"bone_count":declared.bones.len(),"same_hash_parent_order":same_order,"different_local_transforms":differing,"transform_differences":differences,"payload_identical":raw.as_ref()==plan.scene.rig_payload.as_ref().unwrap().as_slice(),"sha256":source.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>())})
        }else{serde_json::json!({"key":key.to_string(),"error":source.error})}
    }else{serde_json::Value::Null};
    let mut samples=vec![];let max=selected.clip.max_tick().ok_or("No native ticks")?;
    for tick in [0.,(max as f32*0.5).floor(),max as f32]{
        let pose=selected.clip.sample(&rig,tick,SampleOptions{ancillary_policy:AncillaryPolicy::PreserveAndIgnore,unsupported_channels:UnsupportedChannelPolicy::PreserveAndSkipNonBoneTracks,..Default::default()})?;
        let hash=format!("{:x}",Sha256::digest(pose.palette.iter().flatten().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>()));
        let changed=pose.palette.iter().flat_map(|p|p.iter().enumerate()).filter(|(i,v)|(**v-if matches!(*i,0|5|10|15){1.}else{0.}).abs()>1e-5).count();
        samples.push(serde_json::json!({"tick":tick,"palette_sha256":hash,"nonidentity_components":changed,"edited_channels":pose.edited_channels,"skipped_channels":pose.skipped_channels.len(),"ignored_events":pose.ignored_events,"ignored_ik":pose.ignored_ik_slots}));
    }
    scope.revalidate()?;for p in &catalog.packages{p.revalidate(&cancel)?;}if !project.check_source_conflicts().is_empty(){return Err("Original Tray source changed".into())}
    let source=|s:&external_cas_character::ResourceObservation|serde_json::json!({"key":s.key.to_string(),"payload_sha256":s.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>()),"selection":s.selection,"candidates":s.candidates.iter().map(|c|serde_json::json!({"path":c.package_path,"entry":c.provider.entry_index,"index_sha256":c.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),"payload_sha256":c.payload_sha256.map(|h|h.iter().map(|b|format!("{b:02x}")).collect::<String>())})).collect::<Vec<_>>()});
    let report=serde_json::json!({"schema":1,"primary_reader_comparison":primary,"declared_rig_comparison":referenced_rig,"current_wrapper_rig_instance":selected.clip.metadata.as_ref().and_then(|m|m.current_header.as_ref()).map(|h|format!("{:016X}",h.rig_instance)),"current_wrapper_codec_range":selected.clip.metadata.as_ref().and_then(|m|m.current_header.as_ref()).map(|h|vec![h.codec_range.start,h.codec_range.end]),"asm":source(&selected.asm_source),"clip":source(&selected.source),"clip_name":selected.clip.name(),"clip_original_retained":selected.source.bytes.as_deref()==Some(selected.clip.original.as_slice()),"ticks":max,"bone_count":rig.bones.len(),"resolution_ms":elapsed,"selector_trace":selected.selector_trace,"samples":samples,"issues":selected.issues,"native_engine_default_selection_qualified":false,"game_contacted":false,"originals_written":false,"source_conflicts":0});
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&args[1])?;file.write_all(&serde_json::to_vec_pretty(&report)?)?;file.sync_all()?;println!("CAS idle={} ticks={} bones={} resolution_ms={} sampled={} originals_unchanged=true",selected.clip.name(),max,rig.bones.len(),elapsed,samples.len());Ok(())
}
