use external_cas_character::{
    assemble_character_with_progress, inspect_combined_xml, CharacterError, CharacterOptions,
};
use std::sync::atomic::AtomicBool;
const XML: &str = r#"<combined><g><L x="1"><E>TEEN</E><E>YOUNGADULT</E><E>ADULT</E></L><E x="2">HUMAN</E><U x="3"><T n="ModifierName">ymHead_Test</T><T n="Scale">0.25</T></U></g><R n="cas_modifier"><I c="Client_CASModifierTuning" s="123"><r n="Ages" x="1"/><r n="Species" x="2"/><L n="Modifiers"><r x="3"/></L></I><I c="OtherClass" s="9"><T>ignored</T></I></R></combined>"#;
#[test]
fn selective_shared_context_and_name_to_scale_binding() {
    let r = inspect_combined_xml(XML.as_bytes(), &AtomicBool::new(false)).unwrap();
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert_eq!(r.records.len(), 1);
    let record = &r.records[0];
    assert_eq!(record.instance, 123);
    assert_eq!(record.resource_namespace, "cas_modifier");
    assert_eq!(
        record.context.ages.as_ref().unwrap(),
        &["TEEN", "YOUNGADULT", "ADULT"]
    );
    assert_eq!(record.context.species.as_ref().unwrap(), &["HUMAN"]);
    assert!(record.context.genders.is_none());
    assert_eq!(
        record.scales[&external_cas_scene::fnv64("ymHead_Test").unwrap()],
        0.25
    );
    let xml = std::str::from_utf8(record.resolved_xml.as_ref().unwrap()).unwrap();
    assert!(!xml.contains("<r"));
    assert!(!xml.contains("OtherClass"));
}
#[test]
fn cycles_duplicates_missing_refs_entities_and_malformed_scale_stay_unresolved() {
    for input in [
        XML.replace("x=\"1\"/>", "x=\"99\"/>"),
        XML.replace(
            "<E x=\"2\">HUMAN</E>",
            "<r x=\"2\" x2=\"unused\"/><E x=\"2\">HUMAN</E>",
        ),
        XML.replace("0.25", "NaN"),
    ] {
        let r = inspect_combined_xml(input.as_bytes(), &AtomicBool::new(false)).unwrap();
        assert!(r.records.is_empty());
        assert!(!r.issues.is_empty());
    }
    let cycle = XML.replace(
        "<L x=\"1\"><E>TEEN</E><E>YOUNGADULT</E><E>ADULT</E></L>",
        "<L x=\"1\"><r x=\"1\"/></L>",
    );
    assert!(
        inspect_combined_xml(cycle.as_bytes(), &AtomicBool::new(false))
            .unwrap()
            .records
            .is_empty()
    );
    assert!(inspect_combined_xml(
        b"<!DOCTYPE combined [<!ENTITY a 'x'>]><combined/>",
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(inspect_combined_xml(b"<combined><g></combined>", &AtomicBool::new(false)).is_err());
}
#[test]
fn tuning_truncated_document_is_never_success() {
    for n in 0..XML.len() {
        assert!(
            inspect_combined_xml(&XML.as_bytes()[..n], &AtomicBool::new(false)).is_err(),
            "prefix{n}"
        );
    }
}
