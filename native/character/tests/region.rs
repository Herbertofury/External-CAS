use external_cas_assets::ResourceKey;
use external_cas_character::RegionMap;
fn u(b: &mut Vec<u8>, v: u32) {
    b.extend(v.to_le_bytes());
}
fn fixture() -> Vec<u8> {
    let mut body = vec![];
    u(&mut body, 1);
    u(&mut body, 3);
    for (region, layer) in [(5, 1.0f32), (5, 2.0), (0, 9.0)] {
        u(&mut body, region);
        body.extend(layer.to_le_bytes());
        body.push(1);
        u(&mut body, 1);
        body.extend(7u64.to_le_bytes());
        u(&mut body, 0x015a1849);
        u(&mut body, 0);
    }
    let mut b = vec![];
    for v in [3, 1, 0, 0, 1] {
        u(&mut b, v);
    }
    b.extend([0; 16]);
    u(&mut b, 44);
    u(&mut b, body.len() as u32);
    b.extend(body);
    b
}
#[test]
fn source_base_region_wins_and_raw_is_retained() {
    let bytes = fixture();
    let r = RegionMap::parse(&bytes).unwrap();
    assert_eq!(r.blocks.len(), 3);
    let memberships = r.regions_for(ResourceKey::new(0x015a1849, 0, 7));
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].region, 0);
    assert_eq!(memberships[0].layer, 0.0);
    assert_eq!(r.max_layer(5), 2.0);
    assert_eq!(r.max_layer(99), -1.0);
    assert_eq!(r.raw, bytes);
}
#[test]
fn every_truncated_prefix_fails_and_suffix_is_visible() {
    let bytes = fixture();
    for n in 0..bytes.len() {
        assert!(RegionMap::parse(&bytes[..n]).is_err(), "prefix{n}");
    }
    let mut with_suffix = bytes;
    with_suffix.extend([1, 2]);
    let r = RegionMap::parse(&with_suffix).unwrap();
    assert_eq!(r.suffix, [1, 2]);
    assert_eq!(r.raw, with_suffix);
}
