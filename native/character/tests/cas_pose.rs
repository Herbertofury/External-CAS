use external_cas_character::{inspect_cas_idle_controllers,ResolvedTuningNode};
use std::{collections::BTreeMap,sync::atomic::AtomicBool};
fn node(tag:&str,attrs:&[(&str,&str)],children:Vec<ResolvedTuningNode>)->ResolvedTuningNode{ResolvedTuningNode{tag:tag.into(),attrs:attrs.iter().map(|(k,v)|(k.to_string(),v.to_string())).collect(),text:String::new(),children}}
fn controller(name:&str)->ResolvedTuningNode{node("Controller",&[("controller",name)],vec![])}
fn fixture()->ResolvedTuningNode{
    node("ASM",&[("name","CAS")],vec![
        node("Parameter",&[("name","isMermaid"),("default","False"),("type","bool")],vec![]),
        node("State",&[("name","Idle")],vec![]),
        node("State",&[("name","Idle")],vec![node("MakeController",&[],vec![node("Reference",&[("target","1")],vec![])])]),
        node("ParameterSelector",&[("unique_id","1"),("parameter","isMermaid")],vec![node("Choice",&[("value","false")],vec![node("RandomSelector",&[("unique_id","2")],vec![
            node("Choice",&[("weight","0")],vec![controller("@ClipController(clip=disabled_native_x, loop_count=#1)")]),
            node("Choice",&[("weight","100")],vec![controller("@ClipController(clip=real_native_idle_x, loop_count=#1)")]),
            node("Choice",&[("weight","30")],vec![controller("@ClipController(clip=real_native_variation_x, loop_count=#1)")]),
        ])])]),
    ])
}
#[test]
fn declared_boolean_defaults_references_and_positive_weight_order_are_retained(){
    let root=fixture();let original=root.clone();let result=inspect_cas_idle_controllers(&root,&BTreeMap::new(),&AtomicBool::new(false)).unwrap();
    assert_eq!(result.iter().map(|r|r.clip_name.as_str()).collect::<Vec<_>>(),["real_native_idle_x","real_native_variation_x"]);
    assert!(result[0].trace.iter().any(|s|s.contains("isMermaid=False")));assert_eq!(root.children.len(),original.children.len());
    let context=BTreeMap::from([("isMermaid".into(),"true".into())]);assert!(inspect_cas_idle_controllers(&root,&context,&AtomicBool::new(false)).unwrap().is_empty());
}
#[test]
fn ambiguous_cycles_dynamic_names_weights_unknown_selectors_and_cancellation_fail_explicitly(){
    let root=fixture();let mut duplicate=root.clone();duplicate.children.push(root.children[3].clone());assert!(inspect_cas_idle_controllers(&duplicate,&BTreeMap::new(),&AtomicBool::new(false)).is_err());
    let mut cycle=root.clone();cycle.children[3]=node("Reference",&[("unique_id","1"),("target","1")],vec![]);assert!(inspect_cas_idle_controllers(&cycle,&BTreeMap::new(),&AtomicBool::new(false)).is_err());
    for replacement in [node("UnsupportedRequiredNode",&[("unique_id","1")],vec![]),node("Controller",&[("unique_id","1"),("controller","@ClipController(clip=#dynamic, loop_count=#1)")],vec![])]{let mut invalid=root.clone();invalid.children[3]=replacement;assert!(inspect_cas_idle_controllers(&invalid,&BTreeMap::new(),&AtomicBool::new(false)).is_err());}
    let mut invalid=root.clone();invalid.children[3]=node("RandomSelector",&[("unique_id","1")],vec![node("Choice",&[("weight","NaN")],vec![])]);assert!(inspect_cas_idle_controllers(&invalid,&BTreeMap::new(),&AtomicBool::new(false)).is_err());
    assert!(inspect_cas_idle_controllers(&root,&BTreeMap::new(),&AtomicBool::new(true)).is_err());
}
