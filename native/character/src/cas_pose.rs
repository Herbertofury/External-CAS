// SPDX-License-Identifier: GPL-3.0-or-later
//! Installed CAS ASM selection, not a replacement authoring pose. The retained
//! graph supplies state branches, clip names, random alternatives and defaults.
use crate::*;
use external_cas_animation::{Clip,FrameLayout,Rig};
use external_cas_scene::fnv64;
use sha2::{Digest,Sha256};
use std::collections::{BTreeMap,BTreeSet};
#[derive(Clone,Debug)]
pub struct CasStandIdle {
    pub clip:Clip,pub source:Arc<ResourceObservation>,pub asm_source:Arc<ResourceObservation>,
    /// Explicit current CLIP dependency, retained independently of the native
    /// Sim's bind rig. Matching topology does not authorize retargeting.
    pub declared_rig_source:Option<Arc<ResourceObservation>>,
    pub selector_trace:Vec<String>,pub issues:Vec<String>,
}
#[derive(Clone,Debug)]
pub struct CasIdleController {pub clip_name:String,pub attributes:BTreeMap<String,String>,pub trace:Vec<String>}
fn attr<'a>(n:&'a ResolvedTuningNode,name:&str)->std::result::Result<&'a str,String>{n.attrs.get(name).map(String::as_str).ok_or_else(||format!("ASM {} lacks {name}",n.tag))}
fn clip_name(n:&ResolvedTuningNode)->std::result::Result<String,String>{
    let text=attr(n,"controller")?;let body=text.strip_prefix("@ClipController(").and_then(|s|s.strip_suffix(')')).ok_or("Unsupported ASM controller expression")?;
    let mut name=None;
    for assignment in body.split(',') {let (key,value)=assignment.trim().split_once('=').ok_or("ASM controller assignment")?;if key.trim()=="clip"{if name.is_some(){return Err("Duplicate ASM clip assignment".into())}let value=value.trim();if value.is_empty()||!value.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){return Err("Dynamic or nonliteral ASM clip expression remains unresolved".into())}name=Some(value.to_string());}}
    name.ok_or("ASM controller has no clip".into())
}
/// Exact scalar-selector/reference traversal. Random alternatives stay ordered
/// observations; no PRNG, transition, IK, event or controller code executes.
pub fn inspect_cas_idle_controllers(root:&ResolvedTuningNode,parameters:&BTreeMap<String,String>,cancel:&AtomicBool)->std::result::Result<Vec<CasIdleController>,String>{
    if root.tag!="ASM"||root.attrs.get("name").map(String::as_str)!=Some("CAS"){return Err("Actual human CAS ASM root/name required".into())}
    let mut ids:BTreeMap<String,Vec<&ResolvedTuningNode>>=BTreeMap::new();
    fn index<'a>(n:&'a ResolvedTuningNode,ids:&mut BTreeMap<String,Vec<&'a ResolvedTuningNode>>,depth:usize,count:&mut usize,cancel:&AtomicBool)->std::result::Result<(),String>{
        *count+=1;if depth>64||*count>500000{return Err("ASM index budget".into())}if *count%1024==0&&cancel.load(Ordering::Relaxed){return Err("CAS idle selection cancelled".into())}
        if let Some(id)=n.attrs.get("unique_id"){ids.entry(id.clone()).or_default().push(n)}for c in &n.children{index(c,ids,depth+1,count,cancel)?}Ok(())
    }index(root,&mut ids,0,&mut 0,cancel)?;
    let states=root.children.iter().filter(|n|n.tag=="State"&&n.attrs.get("name").map(String::as_str)==Some("Idle")&&!n.children.is_empty()).collect::<Vec<_>>();
    if states.len()!=1{return Err("CAS Idle must have exactly one concrete decision graph".into())}
    let mut defaults=BTreeMap::new();let mut bools=BTreeSet::new();
    for p in root.children.iter().filter(|n|n.tag=="Parameter"){
        let name=attr(p,"name")?.to_string();let value=attr(p,"default")?.to_string();
        if defaults.insert(name.clone(),value).is_some(){return Err("Duplicate ASM parameter declarations".into())}
        if attr(p,"type")?=="bool"{bools.insert(name);}
    }
    defaults.extend(parameters.iter().map(|(k,v)|(k.clone(),v.clone())));
    struct Walk<'a>{ids:&'a BTreeMap<String,Vec<&'a ResolvedTuningNode>>,parameters:&'a BTreeMap<String,String>,bools:&'a BTreeSet<String>,cancel:&'a AtomicBool,visits:usize,out:Vec<CasIdleController>}
    impl Walk<'_>{
        fn node(&mut self,n:&ResolvedTuningNode,trace:&mut Vec<String>,path:&mut BTreeSet<String>,depth:usize)->std::result::Result<(),String>{
            if self.cancel.load(Ordering::Relaxed){return Err("CAS idle selection cancelled".into())}
            self.visits+=1;if depth>64||self.visits>10000{return Err("ASM selector work budget".into())}
            match n.tag.as_str(){
                "Reference"=>{let target=attr(n,"target")?.to_string();if !path.insert(target.clone()){return Err("ASM reference cycle".into())}let options=self.ids.get(&target).ok_or("ASM reference target missing")?;if options.len()!=1{return Err("ASM reference target ambiguous".into())}trace.push(format!("Reference {target}"));self.node(options[0],trace,path,depth+1)?;trace.pop();path.remove(&target);},
                "ParameterSelector"|"PostureSelector"=>{
                    let parameter=attr(n,"parameter")?;let value=self.parameters.get(parameter).ok_or_else(||format!("ASM parameter {parameter} has no native input or declared default"))?;
                    let choices=n.children.iter().filter(|c|c.tag=="Choice"&&c.attrs.get("value").is_some_and(|v|if self.bools.contains(parameter){v.eq_ignore_ascii_case(value)}else{v==value})).collect::<Vec<_>>();
                    if choices.len()>1{return Err(format!("Ambiguous ASM selector choice {parameter}={value}"))}
                    trace.push(format!("{parameter}={value} [{}]",n.attrs.get("unique_id").map(String::as_str).unwrap_or("unidentified")));
                    if let Some(choice)=choices.first(){self.node(choice,trace,path,depth+1)?;}trace.pop();
                },
                "Controller"=>{let name=clip_name(n)?;let mut record=trace.clone();record.push(format!("Controller {}: {name}",n.attrs.get("unique_id").map(String::as_str).unwrap_or("unidentified")));self.out.push(CasIdleController{clip_name:name,attributes:n.attrs.clone(),trace:record});},
                "RandomSelector"=>{trace.push(format!("RandomSelector {}: ordered alternatives retained",n.attrs.get("unique_id").map(String::as_str).unwrap_or("unidentified")));for c in &n.children{
                    if let Some(raw)=c.attrs.get("weight"){let weight=raw.parse::<f64>().map_err(|_|"ASM random weight is not numeric")?;if !weight.is_finite()||weight<0.{return Err("ASM random weight invalid".into())}if weight==0.{continue}}
                    self.node(c,trace,path,depth+1)?;
                }trace.pop();},
                "State"|"MakeController"|"Choice"=>for c in &n.children{self.node(c,trace,path,depth+1)?;},
                "description"=>{},
                other=>return Err(format!("Unsupported required ASM selector node {other}")),
            }Ok(())
        }
    }
    let mut walk=Walk{ids:&ids,parameters:&defaults,bools:&bools,cancel,visits:0,out:vec![]};walk.node(states[0],&mut vec!["ASM CAS / public Idle / requested standing CAS preview".into()],&mut BTreeSet::new(),0)?;
    Ok(walk.out)
}
/// Resolves the normal human CAS standing-idle graph from installed metadata.
/// The first declared reachable variant supplies a deterministic initial frame;
/// all original animation data and native-runtime limitations remain retained.
pub fn resolve_cas_stand_idle<'a>(plan:&CharacterPlan,catalog:&'a Catalog,policy:Option<&ScopePolicy<'a>>,cancel:&AtomicBool)->std::result::Result<CasStandIdle,String>{
    if cancel.load(Ordering::Relaxed){return Err("CAS idle selection cancelled".into())}
    if format!("{:x}",Sha256::digest(&plan.native.base.native.bytes))!=plan.scene.native_sim_sha256{return Err("CAS pose belongs to a stale native Sim revision".into())}
    if plan.native.extended_species.filter(|s|*s!=0).unwrap_or(1)!=1{return Err("Installed nonhuman CAS ASM selection is not yet qualified".into())}
    let age=match plan.native.age.ok_or("Native age absent")?{1=>"baby",2=>"toddler",4=>"child",8=>"teen",16=>"youngadult",32=>"adult",64=>"elder",128=>"infant",_=>return Err("Ambiguous native age for CAS ASM".into())};
    let sex=match plan.native.gender.ok_or("Native sex absent")?{4096=>"male",8192=>"female",_=>return Err("Ambiguous native sex for CAS ASM".into())};
    let occult=match plan.scene.resolved_form {
        Some(FormSelection::Base)=>1,
        Some(FormSelection::OccultIndex(i))=>plan.native.occult_forms.get(i).and_then(|f|f.occult_type).ok_or("Selected native occult form type absent")?,
        _=>return Err("Selected CAS appearance form is unresolved".into()),
    };
    if ![1,2,4,8,16,32,64].contains(&occult){return Err("Selected native occult state is combined or unknown; no animation context guessed".into())}
    let mut parameters=BTreeMap::from([("x:posture".into(),"stand--FullBody".into()),("x:age".into(),age.into()),("x:sex".into(),sex.into())]);
    for (name,flag) in [("isVampire",4),("isMermaid",8),("isWitch",16),("isWerewolf",32),("isFairy",64)]{parameters.insert(name.into(),if occult&flag!=0{"true"}else{"false"}.into());}
    let choices=BTreeMap::new();let mut resolver=ResourceResolver::new(catalog,policy,&choices,ReadLimits{max_resource_bytes:16*1024*1024},64*1024*1024,cancel);
    let key=ResourceKey::new(0x02d5df13,0,fnv64("CAS")?);let asm_source=resolver.get(key);let raw=asm_source.bytes.as_ref().ok_or_else(||format!("Native CAS ASM unresolved: {:?}",asm_source.error))?;
    let root=inspect_packed_xml_document(raw,cancel).map_err(|e|e.to_string())?;
    let controllers=inspect_cas_idle_controllers(&root,&parameters,cancel)?;
    let chosen=controllers.first().ok_or("Native CAS Idle graph produced no supported literal clip")?;
    for (name,expected) in [("target","x"),("track","normal"),("mask",""),("start_frame_offset","0"),("end_frame_offset","0"),("timescale","1")]{if chosen.attributes.get(name).map(String::as_str)!=Some(expected){return Err(format!("CAS initial controller {name} requires unsupported nondefault handling"))}}
    if chosen.attributes.get("mirror_conditional").is_some_and(|v|!v.eq_ignore_ascii_case("false")){return Err("CAS initial controller requires mirrored sampling".into())}
    let clip_key=ResourceKey::new(0x6b20c4f3,0,fnv64(&chosen.clip_name)?);let source=resolver.get(clip_key);let raw=source.bytes.as_ref().ok_or_else(||format!("Native stand CLIP unresolved: {:?}",source.error))?;
    let clip=Clip::parse(raw,FrameLayout::StudioExplicit).map_err(|e|e.to_string())?;
    if clip.name()!=chosen.clip_name{return Err(format!("Stored CLIP name {:?} / version{} does not match source ASM controller {:?}; prefix{:02x?}",clip.name(),clip.version,chosen.clip_name,&raw[..raw.len().min(16)]))}
    if clip.codec.is_none(){return Err("Native CAS stand CLIP codec unsupported".into())}
    let mut issues=vec![format!("Selected the first of {} declared reachable CAS Idle alternatives for deterministic initial standing preview; native PRNG/transition scheduling is not executed",controllers.len()),"CLIP sampling uses the separately source-qualified StudioExplicit interpretation; native timing, IK, events and control-track effects remain qualification gates".into(),"ASM declared defaults are retained for non-age/sex/occult controls; native ghost/frame/engine parameter delivery remains unqualified".into()];issues.extend(clip.limitations.iter().cloned());
    let declared_rig_source=if let Some(header)=clip.metadata.as_ref().and_then(|m|m.current_header.as_ref()) {
        if header.rig_instance==0{return Err("Current CAS CLIP declares a zero rig dependency".into())}
        let declared_source=resolver.get(ResourceKey::new(0x8eaf13de,0,header.rig_instance));
        let declared=Rig::parse(declared_source.bytes.as_ref().ok_or_else(||format!("Current CAS CLIP declared rig unresolved: {:?}",declared_source.error))?).map_err(|e|e.to_string())?;
        let native=Rig::parse(plan.scene.rig_payload.as_ref().ok_or("Native Sim bind rig absent")?).map_err(|e|e.to_string())?;
        if declared.version!=3||declared.minor_version!=1||declared.bones.len()!=native.bones.len()||declared.bones.iter().zip(&native.bones).any(|(a,b)|a.hash!=b.hash||a.parent_index!=b.parent_index){return Err("Current CAS CLIP declared rig and native Sim bind rig have incompatible bone hash/parent ordering".into())}
        let differences=declared.bones.iter().zip(&native.bones).filter(|(a,b)|a.position!=b.position||a.orientation!=b.orientation||a.scale!=b.scale).count();
        if differences>0{issues.push(format!("CLIP-declared rig matches native Sim bone topology but {differences} local binds differ; both sources retained without automatic rig replacement or retargeting"))}
        Some(declared_source)
    }else{None};
    Ok(CasStandIdle{clip,source,asm_source,declared_rig_source,selector_trace:chosen.trace.clone(),issues})
}
