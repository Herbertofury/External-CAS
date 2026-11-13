#[path = "../src/cas_camera.rs"]
mod cas_camera;
use cas_camera::*;

fn profile() -> SourceCasCameraProfile {
    SourceCasCameraProfile {
        target: [0.0; 3],
        fov_degrees: 44.0,
        distance: 3.95,
        orbital_rotation_degrees: 177.0,
        pitch_degrees: 5.0,
    }
}
fn projection() -> SourceCasCameraProjection {
    SourceCasCameraProjection {
        aspect_ratio: 16.0 / 9.0,
        near_clip: 0.1,
        far_clip: 10000.0,
    }
}
fn mul(v: [f32; 4], m: [[f32; 4]; 4]) -> [f32; 4] {
    std::array::from_fn(|j| ((v[0] * m[0][j] + v[1] * m[1][j]) + v[2] * m[2][j]) + v[3] * m[3][j])
}

#[test]
fn source_reference_aspect_and_d3d_depth_are_preserved() {
    let frame = source_cas_camera_frame(profile(), projection()).unwrap();
    assert!((frame.horizontal_span / frame.vertical_span - 16.0 / 9.0).abs() < 1e-6);
    // A conventional 44-degree vertical FOV is materially wider than this source
    // reference profile; verify the source 4:3 conversion against geometry.
    let conventional_span = 2.0 * (44.0f32.to_radians() * 0.5).tan();
    assert!((frame.vertical_span - conventional_span * 0.75).abs() < 3e-6);
    let eye_view = mul(
        [frame.eye[0], frame.eye[1], frame.eye[2], 1.0],
        frame.view_rows,
    );
    assert!(eye_view[..3].iter().all(|v| v.abs() < 3e-6));
    let target_clip = mul([0.0, 0.0, 0.0, 1.0], frame.view_projection_rows);
    assert!(target_clip[0].abs() < 3e-6 && target_clip[1].abs() < 3e-6);
    assert!(target_clip[3] > 0.0);
    for (distance, expected) in [(projection().near_clip, 0.0), (projection().far_clip, 1.0)] {
        let clip = mul([0.0, 0.0, -distance, 1.0], frame.projection_rows);
        assert!((clip[2] / clip[3] - expected).abs() < 2e-7);
    }
    assert!(frame.world_rows[0][3] == 0.0 && frame.world_rows[3][3] == 1.0);
    assert!(frame
        .right
        .iter()
        .chain(frame.up.iter())
        .chain(frame.backward.iter())
        .all(|v| v.is_finite()));
    assert_eq!(frame.target, [0.0; 3]);
}

#[test]
fn missing_or_degenerate_source_inputs_are_refused() {
    for (fov, aspect) in [(0.0, 1.0), (180.0, 1.0), (44.0, 0.0), (f32::NAN, 1.0)] {
        assert!(source_cas_fov_spans(fov, aspect).is_err());
    }
    let mut p = profile();
    p.target[1] = f32::INFINITY;
    assert!(source_cas_camera_frame(p, projection()).is_err());
    let mut p = profile();
    p.distance = 0.0;
    assert!(source_cas_camera_frame(p, projection()).is_err());
    let mut clip = projection();
    clip.far_clip = clip.near_clip;
    assert!(source_cas_camera_frame(profile(), clip).is_err());
}

#[test]
#[ignore = "explicit private original CAS camera instruction receipt required"]
fn explicit_original_instruction_camera_receipt() {
    let path = std::env::var_os("EXTERNAL_CAS_CAMERA_RECEIPT")
        .expect("explicit private original-instruction receipt");
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let number = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let vec3 = |v: &serde_json::Value| std::array::from_fn(|i| number(&v[i]));
    let mut max_eye = 0.0f32;
    let mut max_basis = 0.0f32;
    let mut max_projection = 0.0f32;
    let mut max_view = 0.0f32;
    let mut span_bits_mismatches = 0usize;
    for row in receipt["cases"].as_array().unwrap() {
        let p = &row["profile"];
        let c = &row["projection"];
        let frame = source_cas_camera_frame(
            SourceCasCameraProfile {
                target: vec3(&p["target"]),
                fov_degrees: number(&p["fov_degrees"]),
                distance: number(&p["distance"]),
                orbital_rotation_degrees: number(&p["orbital_rotation_degrees"]),
                pitch_degrees: number(&p["pitch_degrees"]),
            },
            SourceCasCameraProjection {
                aspect_ratio: number(&c["aspect_ratio"]),
                near_clip: number(&c["near_clip"]),
                far_clip: number(&c["far_clip"]),
            },
        )
        .unwrap();
        for i in 0..3 {
            max_eye = max_eye.max((frame.eye[i] - number(&row["expected_eye"][i])).abs());
        }
        for i in 0..4 {
            for j in 0..4 {
                max_basis = max_basis.max(
                    (frame.world_rows[i][j] - number(&row["expected_world_rows"][i][j])).abs(),
                );
                max_projection = max_projection.max(
                    (frame.projection_rows[i][j] - number(&row["expected_projection_rows"][i][j]))
                        .abs(),
                );
                max_view = max_view
                    .max((frame.view_rows[i][j] - number(&row["expected_view_rows"][i][j])).abs());
            }
        }
        for (i, v) in [frame.horizontal_span, frame.vertical_span]
            .into_iter()
            .enumerate()
        {
            span_bits_mismatches +=
                usize::from(v.to_bits() != number(&row["expected_spans"][i]).to_bits());
        }
    }
    let result = serde_json::json!({"cases":receipt["case_count"],"max_eye_abs":max_eye,"max_basis_abs":max_basis,"max_projection_abs":max_projection,"max_view_abs":max_view,"span_bit_mismatches":span_bits_mismatches});
    println!("{result}");
    if let Some(path) = std::env::var_os("EXTERNAL_CAS_CAMERA_RESULT") {
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    assert!(max_eye <= 3e-5, "{result}");
    assert!(max_basis <= 3e-5, "{result}");
    assert!(max_projection <= 2e-6, "{result}");
    assert!(max_view <= 1e-4, "{result}");
    assert_eq!(span_bits_mismatches, 0, "{result}");
}
