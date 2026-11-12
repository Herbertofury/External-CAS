use super::*;
fn put(raw: &mut [u8], at: usize, value: u32) {
    raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn fixture() -> Vec<u8> {
    let mut raw = vec![0; 512];
    raw[..4].copy_from_slice(b"DATA");
    put(&mut raw, 8, 24);
    put(&mut raw, 52, 12);
    put(&mut raw, 64, 6);
    for (field, target, count) in [
        (68, 200, 1),
        (84, 0, 0),
        (92, 256, 1),
        (100, 0, 0),
        (140, 380, 1),
    ] {
        put(
            &mut raw,
            field,
            if count > 0 {
                (target as i32 - field as i32) as u32
            } else {
                0x80000000
            },
        );
        put(&mut raw, field + 4, count);
    }
    put(&mut raw, 200, 0xdf0863b8);
    put(&mut raw, 220, 104);
    put(&mut raw, 224, 1);
    put(&mut raw, 328, 12);
    put(&mut raw, 332, 1);
    put(&mut raw, 344, 1);
    put(&mut raw, 356, 1);
    put(&mut raw, 360, 0x80000000);
    put(&mut raw, 380, 7);
    put(&mut raw, 384, 1);
    let mut dxbc = vec![0; 52];
    dxbc[..4].copy_from_slice(b"DXBC");
    put(&mut dxbc, 24, 52);
    put(&mut dxbc, 28, 1);
    put(&mut dxbc, 32, 36);
    dxbc[36..40].copy_from_slice(b"SHDR");
    put(&mut dxbc, 40, 8);
    put(&mut dxbc, 44, 0x50);
    put(&mut dxbc, 48, 2);
    raw[400] = 52;
    raw[401] = 204;
    raw[402..454].copy_from_slice(&dxbc);
    put(&mut raw, 256, 144);
    put(&mut raw, 264, 54);
    raw
}
#[test]
fn source_relative_tables_passes_and_original_program_remain_distinct() {
    let source = fixture();
    let lib = ShaderLibrary::from_bytes(
        "fixture".into(),
        source.clone(),
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(lib.stage_count(Stage::Pixel), 1);
    let effect = lib.effect(0).unwrap();
    let techniques = lib.techniques(&effect, &AtomicBool::new(false)).unwrap();
    assert_eq!(techniques[0].passes[0].states, vec![[7, 1]]);
    let shader = lib
        .shader(Stage::Pixel, 1, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(shader.dxbc, &source[402..454]);
    assert_eq!(shader.raw_record[2], 54);
    assert!(
        lib.shader(Stage::Pixel, 0, &AtomicBool::new(false))
            .is_err()
    );
    let mut changed = source.clone();
    changed[16] = 1;
    let other = ShaderLibrary::from_bytes(
        "fixture".into(),
        changed,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(other.techniques(&effect, &AtomicBool::new(false)).is_err());
}
#[test]
fn malformed_counts_pointers_program_stage_and_state_slices_are_refused() {
    for (offset, value) in [(12, u32::MAX), (8, u32::MAX), (64, 7), (96, u32::MAX)] {
        let mut raw = fixture();
        put(&mut raw, offset, value);
        assert!(
            ShaderLibrary::from_bytes(
                "fixture".into(),
                raw,
                Limits::default(),
                &AtomicBool::new(false)
            )
            .is_err(),
            "offset {offset}"
        );
    }
    let mut raw = fixture();
    put(&mut raw, 356, 2);
    let lib = ShaderLibrary::from_bytes(
        "fixture".into(),
        raw,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        lib.techniques(&lib.effect(0).unwrap(), &AtomicBool::new(false))
            .is_err()
    );
    let mut raw = fixture();
    put(&mut raw, 446, 0x10050);
    let lib = ShaderLibrary::from_bytes(
        "fixture".into(),
        raw,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        lib.shader(Stage::Pixel, 1, &AtomicBool::new(false))
            .is_err()
    );
}
#[test]
fn snappy_overlap_copy_budget_truncation_and_cancellation() {
    assert_eq!(
        decode_raw_snappy(&[8, 0, b'a', 13, 1], 8, &AtomicBool::new(false)).unwrap(),
        b"aaaaaaaa"
    );
    for raw in [
        &[1, 1, 0][..],
        &[2, 4, b'a'][..],
        &[1, 0, b'a', 0][..],
        &[255, 255, 255, 255, 255, 0][..],
    ] {
        assert!(decode_raw_snappy(raw, 64, &AtomicBool::new(false)).is_err());
    }
    assert!(decode_raw_snappy(&[8, 0, b'a', 13, 1], 7, &AtomicBool::new(false)).is_err());
    assert!(decode_raw_snappy(&[8, 0, b'a', 13, 1], 8, &AtomicBool::new(true)).is_err());
}
#[test]
fn binding_tables_keep_raw_default_bits_and_refuse_foreign_parameter_extents() {
    let mut raw = fixture();
    raw.resize(1200, 0);
    for (field, target, count) in [
        (124, 600, 1),
        (148, 700, 1),
        (268, 900, 1),
        (292, 920, 1),
        (712, 800, 1),
        (720, 820, 4),
    ] {
        put(&mut raw, field, (target as i32 - field as i32) as u32);
        put(&mut raw, field + 4, count);
    }
    put(&mut raw, 600, 0x6e56548a);
    put(&mut raw, 604, 17);
    put(&mut raw, 808, 0);
    raw[804..806].copy_from_slice(&16u16.to_le_bytes());
    raw[806..808].copy_from_slice(&0x104u16.to_le_bytes());
    let defaults = [0x3f800000, 0x40000000, 0x80000000, 0x7fc12345];
    for (i, v) in defaults.into_iter().enumerate() {
        put(&mut raw, 820 + i * 4, v);
    }
    raw[902..904].copy_from_slice(&4u16.to_le_bytes());
    raw[904..906].copy_from_slice(&1u16.to_le_bytes());
    raw[906..908].copy_from_slice(&22u16.to_le_bytes());
    let library = ShaderLibrary::from_bytes(
        "fixture".into(),
        raw.clone(),
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let bindings = library.pixel_bindings(1, &AtomicBool::new(false)).unwrap();
    assert_eq!(bindings.textures[0].parameter.hash, 0x6e56548a);
    assert_eq!(bindings.textures[0].raw_words, [0, 4, 1, 22]);
    assert_eq!(bindings.constant_buffers[0].default_words, defaults);
    for (offset, value) in [(900, 1u16), (800, 1), (802, 4)] {
        let mut broken = raw.clone();
        broken[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        let library = ShaderLibrary::from_bytes(
            "fixture".into(),
            broken,
            Limits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            library.pixel_bindings(1, &AtomicBool::new(false)).is_err(),
            "offset {offset}"
        );
    }
}
