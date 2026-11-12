use external_cas_character::{inspect_combined_classes, CharacterError};
use std::sync::atomic::AtomicBool;
#[test]
fn generic_class_fields_remain_uninterpreted_and_shared_aliases_preserve_target_names() {
    let raw=br#"<combined><g><U x="1"><T n="unknown_float">NaN</T><L n="vector"><T>1</T><T>2</T></L></U><E x="2" n="NativeTargetName">unknown-native-token</E></g><R n="camera"><I c="Client_CASCameraTuning" s="42" private_flag="1"><r x="1" n="Camera"/><r x="2" n="Alias"/></I><I c="OtherClass" s="43"><T>skip</T></I></R></combined>"#;
    let out = inspect_combined_classes(raw, &["Client_CASCameraTuning"], &AtomicBool::new(false))
        .unwrap();
    assert!(out.issues.is_empty());
    assert_eq!(out.records.len(), 1);
    let r = &out.records[0];
    assert_eq!(r.instance, 42);
    assert_eq!(r.resource_namespace, "camera");
    assert_eq!(r.root.attrs["private_flag"], "1");
    assert_eq!(r.root.children[0].attrs["n"], "Camera");
    assert_eq!(r.root.children[0].children[0].text, "NaN");
    assert_eq!(r.root.children[1].attrs["n"], "NativeTargetName");
    assert_eq!(r.root.children[1].text, "unknown-native-token");
    assert!(
        inspect_combined_classes(raw, &["OtherClass"], &AtomicBool::new(false))
            .unwrap()
            .records
            .len()
            == 1
    );
}
#[test]
fn generic_selector_uses_proven_packed_schema_and_reports_ambiguous_shared_records() {
    let out = inspect_combined_classes(
        include_bytes!("fixtures/packed-normal.bin"),
        &["Client_CASModifierTuning"],
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(out.records.len(), 2);
    assert_eq!(out.records[0].root.children[0].children[0].text, "TEEN");
    let out = inspect_combined_classes(
        include_bytes!("fixtures/packed-duplicate.bin"),
        &["Client_CASModifierTuning"],
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(out.records.len(), 1);
    assert!(!out.issues.is_empty());
    assert!(inspect_combined_classes(b"<combined/>", &[], &AtomicBool::new(false)).is_err());
    assert!(matches!(
        inspect_combined_classes(b"<combined/>", &["C"], &AtomicBool::new(true)),
        Err(CharacterError::Cancelled)
    ));
}
#[test]
fn standalone_native_tuning_mod_is_retained_without_fake_wrapper_or_defaults() {
    use external_cas_character::inspect_tuning_classes;
    let raw=br#"<?xml version="1.0"?><I c="Client_CASCameraTuning" i="cas_camera" m="opaque_module" n="NativeMod" s="100"><T n="Pitch">-0.25</T><T n="UnknownNative">unchanged</T></I>"#;
    assert!(inspect_combined_classes(raw,&["Client_CASCameraTuning"],&AtomicBool::new(false)).is_err());
    let out=inspect_tuning_classes(raw,&["Client_CASCameraTuning"],&AtomicBool::new(false)).unwrap();
    assert!(out.issues.is_empty());assert_eq!(out.records.len(),1);let r=&out.records[0];
    assert_eq!(r.resource_namespace,"cas_camera");assert_eq!(r.root.tag,"I");assert_eq!(r.root.attrs["m"],"opaque_module");assert_eq!(r.root.children[1].text,"unchanged");
    assert!(inspect_tuning_classes(raw,&["Other"],&AtomicBool::new(false)).unwrap().records.is_empty());
    for n in 0..raw.len(){assert!(inspect_tuning_classes(&raw[..n],&["Client_CASCameraTuning"],&AtomicBool::new(false)).is_err(),"prefix {n}");}
}
