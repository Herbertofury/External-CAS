use external_cas_character::{inspect_combined_data, inspect_combined_tuning, CharacterError};
use std::sync::atomic::AtomicBool;
const NORMAL: &[u8] = include_bytes!("fixtures/packed-normal.bin");
#[test]
fn independent_fabricated_shared_data_retains_duplicate_record_instances() {
    let original = NORMAL.to_vec();
    let out = inspect_combined_data(NORMAL, &AtomicBool::new(false)).unwrap();
    assert!(out.issues.is_empty(), "{:?}", out.issues);
    assert_eq!(out.records.len(), 2);
    assert_eq!(out.records[0].instance, out.records[1].instance);
    assert_eq!(out.records[0].context.ages.as_ref().unwrap(), &["TEEN"]);
    assert!(out.records[1].context.ages.is_none());
    assert_eq!(out.shared_entries, 1);
    assert_eq!(NORMAL, original);
    assert_eq!(
        inspect_combined_tuning(NORMAL, &AtomicBool::new(false))
            .unwrap()
            .records
            .len(),
        2
    );
}
#[test]
fn every_truncated_data_prefix_rejects_without_panicking() {
    for n in 0..NORMAL.len() {
        assert!(
            inspect_combined_data(&NORMAL[..n], &AtomicBool::new(false)).is_err(),
            "prefix{n}"
        );
    }
}
#[test]
fn declared_schema_references_are_required_even_with_matching_row_size() {
    for field in [40usize, 68, 96] {
        let mut raw = NORMAL.to_vec();
        raw[field..field + 4].copy_from_slice(&0i32.to_le_bytes());
        assert!(matches!(
            inspect_combined_data(&raw, &AtomicBool::new(false)),
            Err(CharacterError::Unsupported(_))
        ));
    }
    let mut raw = NORMAL.to_vec();
    raw[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(inspect_combined_data(&raw, &AtomicBool::new(false)).is_err());
}
#[test]
fn ambiguous_or_cyclic_shared_graph_keeps_other_record_evidence() {
    for raw in [
        include_bytes!("fixtures/packed-duplicate.bin").as_slice(),
        include_bytes!("fixtures/packed-cycle.bin").as_slice(),
    ] {
        let out = inspect_combined_data(raw, &AtomicBool::new(false)).unwrap();
        assert_eq!(out.records.len(), 1);
        assert!(!out.issues.is_empty());
        assert!(out.records[0].context.ages.is_none());
    }
}
#[test]
fn malformed_node_and_string_pointers_and_cancellation_are_explicit() {
    let u32at = |p: usize| u32::from_le_bytes(NORMAL[p..p + 4].try_into().unwrap()) as usize;
    let table = 8 + u32at(8);
    for n in [1usize, 3, 5] {
        let p = table + n * 28 + 20;
        let mut raw = NORMAL.to_vec();
        raw[p..p + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(inspect_combined_data(&raw, &AtomicBool::new(false)).is_err());
    }
    assert!(matches!(
        inspect_combined_data(NORMAL, &AtomicBool::new(true)),
        Err(CharacterError::Cancelled)
    ));
}
