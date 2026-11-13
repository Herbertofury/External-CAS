// SPDX-License-Identifier: GPL-3.0-or-later
// The parent owns lib.rs integration; compile the exact module independently
// until its public export is handed off, without editing that shared boundary.
#[path = "../src/cas_lighting.rs"]
mod cas_lighting;
use cas_lighting::*;

fn explicit_profile() -> SourceCasLightingProfile {
    SourceCasLightingProfile {
        light_base_height: 0.0,
        key_lights: [SourceCasKeyLight {
            enable: false,
            height: 0.5,
            angle_degrees: 25.0,
            color: [1.0, 0.5, 0.25],
            intensity: 0.5,
            specular_intensity: 0.75,
        }; 2],
        rim_lights: [SourceCasRimLight {
            enable: false,
            color: [0.2, 0.4, 0.6],
            intensity: 2.0,
            falloff: 3.0,
        }; 2],
        ambient_min: 0.25,
        ambient_max: 0.75,
    }
}
#[test]
fn source_context_is_explicit_and_invalid_inputs_are_not_repaired() {
    let profile = explicit_profile();
    let basis = SourceCasLightBasis {
        row0: [1.0, 0.0, 0.0, 0.0],
        row2: [0.0, 0.0, 1.0, 0.0],
    };
    let output = source_cas_light_frame(profile, basis).unwrap();
    assert_eq!(output.light_colors, [[0.0; 4]; 4]);
    assert_eq!(output.light_directions, [[0.0; 4]; 4]);
    assert_eq!(output.specular_terms, [0.75, 0.75, 0.0, 0.0]);
    assert_eq!(output.rim_falloff, [3.0, 3.0, 0.0, 0.0]);
    assert_eq!(output.ambient_intensity, [0.25, 0.75, 0.0, 0.0]);
    let mut invalid = profile;
    invalid.key_lights[0].angle_degrees = f32::NAN;
    assert!(source_cas_light_frame(invalid, basis).is_err());
    let invalid_basis = SourceCasLightBasis {
        row0: [0.0, 1.0, 0.0, 0.0],
        row2: basis.row2,
    };
    assert!(source_cas_light_frame(profile, invalid_basis).is_err());
    assert!(source_cas_light_vector(0.0, f32::INFINITY, 0.0, basis.row2, basis.row0).is_err());
}

#[test]
#[ignore = "explicit private original CAS light instruction receipt required"]
fn compiled_source_math_matches_original_helper_and_frame_outputs() {
    let path = std::env::var_os("EXTERNAL_CAS_LIGHT_FRAME_RECEIPT")
        .expect("explicit private original-instruction receipt");
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(receipt["game_contact"], false);
    assert_eq!(receipt["mxcsr"], "0x1f80");
    let scalar = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let array = |v: &serde_json::Value| -> [f32; 4] {
        v.as_array()
            .unwrap()
            .iter()
            .map(scalar)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    };
    let helpers = receipt["helper_cases"].as_array().unwrap();
    assert!(helpers.len() >= 268);
    for row in helpers {
        let actual = source_cas_light_vector(
            scalar(&row["base_height"]),
            scalar(&row["height"]),
            scalar(&row["angle_degrees"]),
            array(&row["row2"]),
            array(&row["row0"]),
        )
        .unwrap();
        let expected: Vec<u32> = row["expected_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            actual.map(f32::to_bits).as_slice(),
            expected.as_slice(),
            "helper: {row}"
        );
    }
    let frames = receipt["frame_cases"].as_array().unwrap();
    assert!(frames.len() >= 153);
    let mut maximum_error = 0.0f32;
    for row in frames {
        let profile: SourceCasLightingProfile =
            serde_json::from_value(row["profile"].clone()).unwrap();
        let basis: SourceCasLightBasis = serde_json::from_value(row["basis"].clone()).unwrap();
        let expected: SourceCasLightFrame =
            serde_json::from_value(row["expected"].clone()).unwrap();
        let actual = source_cas_light_frame(profile, basis).unwrap();
        for (a, e) in actual
            .light_directions
            .iter()
            .flatten()
            .zip(expected.light_directions.iter().flatten())
        {
            let error = (a - e).abs();
            maximum_error = maximum_error.max(error);
            assert!(error <= 0.0000005, "direction error {error}: {row}");
        }
        assert_eq!(
            actual.light_colors.map(|v| v.map(f32::to_bits)),
            expected.light_colors.map(|v| v.map(f32::to_bits))
        );
        assert_eq!(
            actual.specular_terms.map(f32::to_bits),
            expected.specular_terms.map(f32::to_bits)
        );
        assert_eq!(
            actual.rim_falloff.map(f32::to_bits),
            expected.rim_falloff.map(f32::to_bits)
        );
        assert_eq!(
            actual.ambient_intensity.map(f32::to_bits),
            expected.ambient_intensity.map(f32::to_bits)
        );
    }
    let result = serde_json::json!({"helper_cases":helpers.len(),"helper_all_output_bits_match":true,"frame_cases":frames.len(),"frame_non_direction_all_output_bits_match":true,"maximum_direction_absolute_error":maximum_error,"game_contact":false,"caller_matrix_identity_qualified":false,"live_selection_qualified":false});
    if let Some(output) = std::env::var_os("EXTERNAL_CAS_LIGHT_FRAME_RESULT") {
        std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    eprintln!("{result}");
}
