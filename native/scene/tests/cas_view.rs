// SPDX-License-Identifier: GPL-3.0-or-later
#[path = "../src/cas_view.rs"]
mod cas_view;
use cas_view::*;

fn explicit_tuning() -> SourceCasRotationTuning {
    SourceCasRotationTuning {
        view_angles: [0.0, 45.0, 90.0, 180.0, 225.0, 315.0],
        lerp_speed_mult: 2.0,
        rotate_key_speed: 150.0,
        rotate_key_speed_fast: 250.0,
        rotate_continuous_speed: 15.0,
        rotate_continuous_ease_in: 0.1,
    }
}
#[test]
fn deadline_short_release_hold_reset_and_cancellation_are_explicit() {
    let tuning = explicit_tuning();
    let mut controller = SourceCasViewController::default();
    assert_eq!(controller.tick(100, &tuning).unwrap(), None);
    controller.press(true, 100).unwrap();
    assert_eq!(controller.tick(400, &tuning).unwrap(), None);
    assert!(!controller.hold_started());
    assert!(matches!(
        controller.release(0.0, &tuning).unwrap(),
        Some(SourceCasViewAction::PresetTarget(SourceCasViewTarget {
            preset_index: 1,
            ..
        }))
    ));
    assert_eq!(controller.release(0.0, &tuning).unwrap(), None);
    controller.press(false, 1000).unwrap();
    assert_eq!(
        controller.tick(1301, &tuning).unwrap(),
        Some(SourceCasViewAction::IncrementDegrees(-0.1 * 0.15))
    );
    assert!(controller.hold_started());
    assert_eq!(controller.release(0.0, &tuning).unwrap(), None);
    controller.press(true, 2000).unwrap();
    controller.cancel();
    assert!(!controller.is_pressed());
    assert_eq!(controller.release(0.0, &tuning).unwrap(), None);
    controller.press(true, 3000).unwrap();
    controller.press(false, 3010).unwrap();
    // Release does not manufacture a hold merely from time passing without a tick.
    assert!(matches!(
        controller.release(0.0, &tuning).unwrap(),
        Some(SourceCasViewAction::PresetTarget(SourceCasViewTarget {
            preset_index: 5,
            ..
        }))
    ));
}
#[test]
fn invalid_tuning_and_clock_do_not_discard_pending_input() {
    let mut controller = SourceCasViewController::default();
    let mut tuning = explicit_tuning();
    controller.press(true, 1000).unwrap();
    assert!(controller.tick(999, &tuning).is_err());
    assert!(controller.is_pressed());
    tuning.rotate_continuous_ease_in = f32::NAN;
    assert!(controller.tick(1301, &tuning).is_err());
    assert!(controller.is_pressed());
    assert!(!controller.hold_started());
    assert!(controller.release(0.0, &tuning).is_err());
    assert!(controller.is_pressed());
    assert!(controller.press(false, u64::MAX).is_err());
    assert!(controller.is_pressed());
}

#[test]
#[ignore = "explicit private original SetSimView instruction receipt required"]
fn original_preset_and_press_hold_release_observations_match() {
    let path = std::env::var_os("EXTERNAL_CAS_VIEW_RECEIPT")
        .expect("explicit private instruction receipt");
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(receipt["game_contact"], false);
    assert_eq!(receipt["mxcsr"], "0x1f80");
    let scalar = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let array = |v: &serde_json::Value| -> [f32; 6] {
        v.as_array()
            .unwrap()
            .iter()
            .map(scalar)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    };
    let presets = receipt["preset_cases"].as_array().unwrap();
    assert!(presets.len() >= 2964);
    for row in presets {
        assert_eq!(
            source_cas_next_preset_index(
                scalar(&row["heading"]),
                row["left"].as_bool().unwrap(),
                array(&row["angles"])
            )
            .unwrap(),
            row["expected_index"].as_u64().unwrap() as usize,
            "{row}"
        );
    }
    let sessions = receipt["controller_sessions"].as_array().unwrap();
    assert!(sessions.len() >= 64);
    let mut events = 0;
    for session in sessions {
        let t = &session["tuning"];
        let tuning = SourceCasRotationTuning {
            view_angles: array(&t["view_angles"]),
            lerp_speed_mult: scalar(&t["lerp_speed_mult"]),
            rotate_key_speed: scalar(&t["rotate_key_speed"]),
            rotate_key_speed_fast: scalar(&t["rotate_key_speed_fast"]),
            rotate_continuous_speed: scalar(&t["rotate_continuous_speed"]),
            rotate_continuous_ease_in: scalar(&t["rotate_continuous_ease_in"]),
        };
        let mut controller = SourceCasViewController::default();
        for event in session["events"].as_array().unwrap() {
            events += 1;
            let time = event["time_ms"].as_u64().unwrap();
            let actual = match event["kind"].as_str().unwrap() {
                "press" => {
                    controller
                        .press(event["left"].as_bool().unwrap(), time)
                        .unwrap();
                    None
                }
                "tick" => controller.tick(time, &tuning).unwrap(),
                "release" => controller
                    .release(scalar(&session["heading"]), &tuning)
                    .unwrap(),
                other => panic!("unqualified event {other}"),
            };
            let expected = event["expected_emitted"].as_array().unwrap();
            if expected.is_empty() {
                assert_eq!(actual, None, "{event}");
            } else {
                assert_eq!(expected.len(), 1);
                match expected[0]["kind"].as_str().unwrap() {
                    "increment_degrees" => {
                        let Some(SourceCasViewAction::IncrementDegrees(degrees)) = actual else {
                            panic!("missing held output {event}")
                        };
                        assert_eq!(
                            degrees.to_bits(),
                            scalar(&expected[0]["value"]).to_bits(),
                            "{event}"
                        );
                    }
                    "short_sign" => {
                        let Some(SourceCasViewAction::PresetTarget(target)) = actual else {
                            panic!("missing short output {event}")
                        };
                        let expected_target = source_cas_view_target(
                            scalar(&session["heading"]),
                            scalar(&expected[0]["value"]) > 0.0,
                            &tuning,
                        )
                        .unwrap();
                        assert_eq!(target, expected_target);
                    }
                    other => panic!("unqualified observation {other}"),
                }
            }
            assert_eq!(
                controller.is_pressed(),
                event["expected_pressed"].as_bool().unwrap()
            );
            if controller.is_pressed() {
                assert_eq!(
                    controller.accumulated_speed().unwrap().to_bits(),
                    scalar(&event["expected_speed"]).to_bits()
                );
            }
        }
    }
    let result = serde_json::json!({"preset_cases":presets.len(),"controller_sessions":sessions.len(),"controller_events":events,"preset_indices_match":true,"emitted_degree_and_active_speed_bits_match":true,"native_io":false,"game_contact":false,"source_update_cadence_qualified":false,"actor_world_basis_qualified":false});
    if let Some(path) = std::env::var_os("EXTERNAL_CAS_VIEW_RESULT") {
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    eprintln!("{result}");
}
