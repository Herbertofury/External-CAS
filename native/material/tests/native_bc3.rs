use external_cas_material::native_bc3::*;
use std::sync::atomic::AtomicBool;

#[test]
fn original_uniform_color_alpha_edges_and_unprepared_bytes_are_preserved() {
    let cancel = AtomicBool::new(false);
    let cases = [
        (
            [0, 0, 0, 0],
            [
                63, 192, 182, 109, 219, 182, 109, 219, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        ),
        (
            [1, 1, 1, 1],
            [1, 1, 73, 146, 36, 73, 146, 36, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
        (
            [128, 128, 128, 128],
            [
                128, 128, 73, 146, 36, 73, 146, 36, 16, 132, 16, 132, 0, 0, 0, 0,
            ],
        ),
        (
            [255, 255, 255, 255],
            [
                63, 192, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0,
            ],
        ),
        (
            [0, 0, 255, 255],
            [
                63, 192, 255, 255, 255, 255, 255, 255, 0, 248, 0, 248, 0, 0, 0, 0,
            ],
        ),
        (
            [255, 0, 0, 0],
            [
                63, 192, 182, 109, 219, 182, 109, 219, 31, 0, 31, 0, 0, 0, 0, 0,
            ],
        ),
        (
            [0, 255, 0, 127],
            [
                127, 127, 73, 146, 36, 73, 146, 36, 224, 7, 224, 7, 0, 0, 0, 0,
            ],
        ),
    ];
    for (pixel, expected) in cases {
        let pixels = [pixel; 16];
        let before = pixels;
        assert_eq!(
            encode_native_cas_bc3_block(&pixels, &cancel).unwrap(),
            expected
        );
        assert_eq!(pixels, before);
    }
    for alpha in [false, true] {
        let color = [1, 7, 99, 255, 11, 19, 128, 65];
        let output = export_native_cas_unprepared_bc3_block(&color, alpha, &cancel).unwrap();
        assert_eq!(&output[8..], &color);
        assert_eq!(&output[..2], &[0, 5]);
        assert!(
            output[2..8]
                .iter()
                .all(|&x| x == if alpha { 255 } else { 0 })
        );
    }
    let cancel = AtomicBool::new(true);
    assert!(encode_native_cas_bc3_block(&[[0; 4]; 16], &cancel).is_err());
    assert!(export_native_cas_unprepared_bc3_block(&[0; 8], false, &cancel).is_err());
}
