// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::gpu_vertex::*;

#[test]
fn original_initial_rounding_and_wrapping_are_distinct() {
    assert_eq!(
        pack_initial_position([0.5 / 8192.0, -0.5 / 8192.0, 1.5 / 8192.0]).unwrap(),
        [1, 0, 2]
    );
    assert_eq!(
        pack_initial_position([4.0, -4.0, 12.0]).unwrap(),
        [-32768, -32768, -32768]
    );
    assert_eq!(
        pack_initial_normal([-1.0, 0.0, 1.0]).unwrap(),
        [1, 128, 255]
    );
    assert_eq!(
        pack_initial_tangent([-1.0, 0.0, 1.0]).unwrap(),
        [0, 127, 255, 128]
    );
    assert_eq!(
        pack_initial_uv([0.5 / 65535.0, 1.5 / 65535.0], 0).unwrap(),
        [-32768, -32766]
    );
    assert_eq!(pack_initial_uv([-1.0, 2.0], 0).unwrap(), [32767, -32768]);
    assert_eq!(pack_initial_uv([-0.25, -0.25], 0).unwrap(), [16383, 16383]);
    assert_eq!(pack_initial_uv([-0.25, -0.25], 1).unwrap(), [-16384, 16383]);
    assert_eq!(pack_initial_uv([-0.25, -0.25], 2).unwrap(), [16383, 16383]);
}

#[test]
fn invalid_conversions_refuse_without_clamping_or_repair() {
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        assert!(pack_initial_position([x, 0.0, 0.0]).is_err());
        assert!(pack_initial_normal([0.0, x, 0.0]).is_err());
        assert!(pack_initial_tangent([0.0, 0.0, x]).is_err());
        assert!(pack_initial_uv([x, 0.0], 0).is_err());
    }
    assert!(pack_initial_uv([2147483648.0, 0.0], 0).is_err());
    assert!(pack_initial_uv([-2147483648.0, 0.0], 1).is_err());
    // A native, representable conversion preserves low bits instead of
    // silently clamping coordinates/normals into an invented physical range.
    assert_eq!(
        pack_initial_normal([2.0, -2.0, 0.0]).unwrap(),
        [126, 130, 128]
    );
    assert_eq!(pack_initial_uv([-2147483648.0, 0.0], 0).unwrap()[0], 32767);
}

#[test]
fn explicit_updated_profiles_preserve_native_rounding_saturation_and_w_boundary() {
    assert_eq!(
        pack_post_deformation_position([0.5 / 8192.0, 1.5 / 8192.0, -0.5 / 8192.0]).unwrap(),
        [0, 2, 0]
    );
    assert_eq!(
        pack_post_deformation_position([4.0, -12.0, 12.0]).unwrap(),
        [32767, -32768, -32768]
    );
    assert_eq!(
        pack_post_deformation_normal([2.0, -2.0, 0.0]).unwrap(),
        [255, 0, 128]
    );
    assert_eq!(
        pack_seam_average_tangent([-1.0, 0.0, 1.0]).unwrap(),
        [1, 128, 255]
    );
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        assert!(pack_post_deformation_position([x, 0.0, 0.0]).is_err());
        assert!(pack_post_deformation_normal([0.0, x, 0.0]).is_err());
        assert!(pack_seam_average_tangent([0.0, 0.0, x]).is_err());
    }
}

#[test]
#[ignore = "requires private original-instruction receipt; contains no public game data"]
fn compare_retained_original_instruction_receipt() {
    let path = std::env::var_os("EXTERNAL_CAS_INITIAL_GEOM_RECEIPT")
        .expect("explicit private receipt path");
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(receipt["game_contact"], false);
    assert_eq!(receipt["intercepted_calls"], 0);
    assert_eq!(receipt["mxcsr"], "0x1f80");
    let rows = receipt["cases"].as_array().unwrap();
    assert!(rows.len() >= 2087);
    for row in rows {
        let values: Vec<f32> = row["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let mut actual = vec![0x7b; 8];
        match row["kind"].as_str().unwrap() {
            "position" => {
                let packed = pack_initial_position(values.try_into().unwrap()).unwrap();
                for (target, value) in actual[..6].chunks_exact_mut(2).zip(packed) {
                    target.copy_from_slice(&value.to_le_bytes());
                }
            }
            "normal" => actual[..3]
                .copy_from_slice(&pack_initial_normal(values.try_into().unwrap()).unwrap()),
            "tangent" => actual[..4]
                .copy_from_slice(&pack_initial_tangent(values.try_into().unwrap()).unwrap()),
            "uv" | "uv1" => {
                let set = u32::from(row["kind"] == "uv1");
                let packed = pack_initial_uv(values.try_into().unwrap(), set).unwrap();
                for (target, value) in actual[..4].chunks_exact_mut(2).zip(packed) {
                    target.copy_from_slice(&value.to_le_bytes());
                }
            }
            other => panic!("unqualified receipt kind {other}"),
        }
        let expected_hex = row["bytes_hex"].as_str().unwrap();
        let actual_hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(actual_hex, expected_hex, "{row}");
    }
}

#[test]
#[ignore = "requires private original updated-packing receipt; no public game data"]
fn compare_retained_original_updated_packing_receipt() {
    let path = std::env::var_os("EXTERNAL_CAS_UPDATED_GEOM_RECEIPT")
        .expect("explicit private receipt path");
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(receipt["game_contact"], false);
    assert_eq!(receipt["intercepted_calls"], 0);
    assert_eq!(receipt["mxcsr"], "0x1f80");
    let rows = receipt["cases"].as_array().unwrap();
    assert!(rows.len() >= 1500);
    for row in rows {
        let values: [f32; 3] = row["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let mut actual = vec![0x7b; 8];
        match row["kind"].as_str().unwrap() {
            "post_deformation_position" => {
                let packed = pack_post_deformation_position(values).unwrap();
                for (target, value) in actual[..6].chunks_exact_mut(2).zip(packed) {
                    target.copy_from_slice(&value.to_le_bytes());
                }
            }
            "post_deformation_normal" => {
                actual[..3].copy_from_slice(&pack_post_deformation_normal(values).unwrap())
            }
            "seam_average_tangent" => {
                actual[..3].copy_from_slice(&pack_seam_average_tangent(values).unwrap())
            }
            other => panic!("unqualified receipt kind {other}"),
        }
        let actual_hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(actual_hex, row["bytes_hex"].as_str().unwrap(), "{row}");
    }
}
